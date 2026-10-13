//! Copied-only actual contractual source -> single CTMC/carry -> coupon book.
//! Rust constructors own P/A/G, entitlements, actual cash and prepared authority.
use crate::products::commercial::factor_source::{owner::NativeFactors, segments::FactorPolicy};
use crate::{commercial_source_income as source_income, term_enhancements as term};
use portfolio_model_core::{
    commercial_coupon_accounting as book,
    credit::{CreditMeasure, CreditState},
    credit_calibration::ObservationTime,
    credit_path as path, credit_path_income as carry,
};
use serde::{Deserialize, Serialize};

use crate::products::commercial::factor_source::draws::{
    draw_event_id, project_draws, project_draws_for_acquisition, source_end_year, validate_draws,
};
pub use crate::products::commercial::factor_source::draws::{
    NativeDrawSource, PerformingDrawTranche,
};

pub const SCHEMA: &str = "commercial-source-credit-accounting-proof-1";
const MAX_WORK: usize = 8_000_000;
const MAX_BYTES: usize = 128 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionInterval {
    pub end_year: f64,
    pub hazards_per_year: [[f64; 5]; 3],
    pub loss_given_default: [f64; 3],
    pub interest_recovery_fraction: [f64; 3],
    pub credit_conversion_factor: [f64; 3],
    pub complete_recovery_tails: [Vec<path::RecoveryWeight>; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathIdentity {
    pub path_id: String,
    pub path_key: u64,
    pub initial_live_state: CreditState,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub identity: term::Identity,
    pub max_jumps_per_path: usize,
    pub intervals: Vec<ConditionInterval>,
    pub paths: Vec<PathIdentity>,
    pub draw_stream: path::DrawStream,
    #[serde(default)]
    pub opening_undrawn_commitment: f64,
    #[serde(default)]
    pub instrument_kind: InstrumentKind,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    #[default]
    CommercialLoan,
    IssuerCreditDebt,
    CreditFreeReference,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub source: source_income::Request,
    pub factor_policy: FactorPolicy,
    pub credit: Conditions,
    pub accounting: book::Policy,
    pub accounting_rates: Vec<carry::RecognitionRate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepayment_terms: Option<PrepaymentTerms>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maturity_basis_policy: Option<MaturityBasisPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub performing_draw_tranches: Vec<PerformingDrawTranche>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_credit_actions: Option<book::reviews::ReviewedActions>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaturityBasisPolicy {
    pub review: term::Identity,
    pub disposition: String,
}
fn maturity_event_id(policy: &MaturityBasisPolicy) -> String {
    format!("native-maturity-basis:{}", policy.review.id)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepaymentTerms {
    pub review: term::Identity,
    pub principal_premium_fraction: f64,
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeStopAction {
    Call,
    Put,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeStopTiming {
    BeforeScheduledCash,
    AfterScheduledCash,
}
/// Prepared native Bellman decision. Never accepted as deserializable request
/// cash or a caller-supplied stopping decision.
#[derive(Debug, Clone, Serialize)]
pub struct NativeStop {
    path_key: u64,
    event_id: String,
    ordinal: i32,
    action: NativeStopAction,
    principal_clean_price_factor: f64,
    fee: f64,
    review: term::Identity,
    timing: NativeStopTiming,
    #[serde(skip_serializing_if = "Option::is_none")]
    rights_only_review: Option<term::Identity>,
}
impl NativeStop {
    pub(crate) fn from_compiled_policy(
        path_key: u64,
        event_id: String,
        ordinal: i32,
        action: NativeStopAction,
        principal_clean_price_factor: f64,
        fee: f64,
        review: term::Identity,
    ) -> Result<Self, String> {
        Self::from_compiled_policy_with_timing(
            path_key,
            event_id,
            ordinal,
            action,
            principal_clean_price_factor,
            fee,
            review,
            NativeStopTiming::AfterScheduledCash,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_compiled_policy_with_timing(
        path_key: u64,
        event_id: String,
        ordinal: i32,
        action: NativeStopAction,
        principal_clean_price_factor: f64,
        fee: f64,
        review: term::Identity,
        timing: NativeStopTiming,
        rights_only_review: Option<term::Identity>,
    ) -> Result<Self, String> {
        portfolio_model_core::cashflow::validate_ordinal(ordinal)?;
        let mut meta = 0;
        term::identity(&review, &mut meta)?;
        if let Some(rights) = &rights_only_review {
            term::identity(rights, &mut meta)?;
        }
        if event_id.is_empty()
            || event_id.len() > 256
            || event_id.chars().any(char::is_control)
            || !principal_clean_price_factor.is_finite()
            || !(0. ..=10.).contains(&principal_clean_price_factor)
        {
            return Err("invalid native compiled exercise identity/clean terms".into());
        }
        term::nonnegative(fee)?;
        Ok(Self {
            path_key,
            event_id,
            ordinal,
            action,
            principal_clean_price_factor,
            fee,
            review,
            timing,
            rights_only_review,
        })
    }
    pub(crate) fn clean_factor(&self) -> f64 {
        self.principal_clean_price_factor
    }
    pub fn path_key(&self) -> u64 {
        self.path_key
    }
    pub fn event_id(&self) -> &str {
        &self.event_id
    }
    pub fn ordinal(&self) -> i32 {
        self.ordinal
    }
    pub fn action(&self) -> NativeStopAction {
        self.action
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub raw_bytes: usize,
    pub native_stop_bytes: usize,
    pub maximum_queries: usize,
    pub carry_points_upper: usize,
    pub accounting_groups_upper: usize,
    pub lots_per_path_upper: usize,
    pub original_source_work: usize,
    pub current_source_work: usize,
    pub carry: carry::Admission,
    pub book_work_upper: usize,
    pub book_bytes_upper: usize,
    pub joined_work_upper: usize,
    pub joined_bytes_upper: usize,
    pub full_credit_end_year: f64,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_request: Request,
    admission: Admission,
    #[serde(serialize_with = "serialize_factors")]
    original_source: NativeFactors,
    #[serde(serialize_with = "serialize_factors")]
    current_source: NativeFactors,
    original_cash_quote: f64,
    prepared_credit_request: carry::Request,
    carry: carry::ResultData,
    accounting: book::ResultData,
    original_anchor_refitted: bool,
    prospective_accounting_rate_policy: bool,
    mixed_ledger_adoption: bool,
    empirical_adoption: bool,
    accounting_approval: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    native_stops: Vec<NativeStop>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    draw_sources: Vec<NativeDrawSource>,
}
pub(crate) fn serialize_factors<S: serde::Serializer>(
    source: &NativeFactors,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Receipt<'a> {
        raw: &'a crate::products::term::contracts::indexed::ContractPathRequest,
        source: &'a crate::products::term::contracts::indexed::ContractPathResult,
        policy: &'a FactorPolicy,
        admission: &'a crate::products::commercial::factor_source::owner::Admission,
        segments: Vec<crate::products::commercial::factor_source::segments::SegmentEvidence<'a>>,
    }
    Receipt {
        raw: source.raw(),
        source: source.source(),
        policy: source.policy(),
        admission: source.admission(),
        segments: source.segments().iter().map(|s| s.evidence()).collect(),
    }
    .serialize(serializer)
}
impl ResultData {
    pub fn original_cash_quote(&self) -> f64 {
        self.original_cash_quote
    }
    pub fn raw_request(&self) -> &Request {
        &self.raw_request
    }
    pub fn native_stops(&self) -> &[NativeStop] {
        &self.native_stops
    }

    pub fn carry(&self) -> &carry::ResultData {
        &self.carry
    }
    pub fn accounting(&self) -> &book::ResultData {
        &self.accounting
    }
    pub fn current_source(&self) -> &NativeFactors {
        &self.current_source
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn full_credit_end_year(&self) -> f64 {
        self.admission.full_credit_end_year
    }
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("commercial aggregate overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("commercial aggregate overflow".into())
}

fn contract_end_year(
    source: &crate::products::term::contracts::indexed::ContractPathRequest,
) -> Result<f64, String> {
    portfolio_model_core::cashflow::validate_ordinal(source.asof_ordinal)?;
    portfolio_model_core::cashflow::validate_ordinal(source.tail_end_ordinal)?;
    let days = source
        .tail_end_ordinal
        .checked_sub(source.asof_ordinal)
        .ok_or("commercial complete clock overflow")?;
    if !(1..=36_600).contains(&days) {
        return Err("commercial complete source clock domain exceeded".into());
    }
    Ok(f64::from(days) / 365.)
}
fn full_credit_end_year(r: &Request) -> Result<f64, String> {
    let contract = source_end_year(r)?;
    let mut lag: f64 = 0.;
    for i in &r.credit.intervals {
        for tails in &i.complete_recovery_tails {
            for tail in tails {
                if !tail.lag_years.is_finite() || tail.lag_years < 0. {
                    return Err("invalid complete recovery lag".into());
                }
                lag = lag.max(tail.lag_years);
            }
        }
    }
    let mut full = contract + lag;
    if let Some(actions) = &r.reviewed_credit_actions {
        let dates = actions
            .allowance_reviews
            .iter()
            .map(|a| a.review.recognition_ordinal)
            .chain(
                actions
                    .chargeoffs
                    .iter()
                    .map(|a| a.review.recognition_ordinal),
            )
            .chain(actions.legal_waivers.iter().map(|a| a.recognition_ordinal));
        for date in dates {
            portfolio_model_core::cashflow::validate_ordinal(date)?;
            let elapsed = date
                .checked_sub(r.source.current.asof_ordinal)
                .ok_or("reviewed date overflow")?;
            if elapsed < 0 {
                return Err("reviewed action precedes native acquisition".into());
            }
            full = full.max(f64::from(elapsed) / 365.);
        }
    }
    if !full.is_finite() || !(0. ..=100.).contains(&full) {
        return Err("complete source/recovery clock domain exceeded".into());
    }
    Ok(full)
}
fn prepared(
    r: &Request,
    measure: CreditMeasure,
    stops: &[NativeStop],
) -> Result<carry::Request, String> {
    if measure == CreditMeasure::Pricing && r.reviewed_credit_actions.is_some() {
        return Err("reviewed credit book actions require Physical accounting adoption; Pricing policy is separately configured".into());
    }
    let source = &r.source.current;
    let years = source_end_year(r)?;
    validate_draws(r, measure)?;
    if r.credit
        .intervals
        .last()
        .is_none_or(|i| i.end_year.to_bits() != years.to_bits())
    {
        return Err("credit clock must cover the exact complete native contractual tail".into());
    }
    if source
        .periods
        .iter()
        .any(|p| p.payment_ordinal <= p.start_ordinal)
    {
        return Err("payment-at-opening requires separate actual source/prefix ownership".into());
    }
    if r.source.policy.fees != "integral_cash-1" && source.periods.iter().any(|p| p.fee != 0.) {
        return Err("nonintegral fee accounting requires its own recognition owner".into());
    }
    term::nonnegative(r.credit.opening_undrawn_commitment)?;
    if let Some(terms) = &r.prepayment_terms {
        let mut meta = 0;
        term::identity(&terms.review, &mut meta)?;
        if !terms.principal_premium_fraction.is_finite()
            || !(0. ..=1.).contains(&terms.principal_premium_fraction)
        {
            return Err("reviewed prepayment premium fraction must lie in [0,1]".into());
        }
    }
    if let Some(policy) = &r.maturity_basis_policy {
        let mut meta = 0;
        term::identity(&policy.review, &mut meta)?;
        if policy.disposition != "dispose_financial_basis_after_exact_legal_close-1" {
            return Err("unsupported reviewed maturity basis disposition".into());
        }
    }
    if r.credit.instrument_kind == InstrumentKind::CreditFreeReference
        && (r.credit.opening_undrawn_commitment != 0.
            || r.credit.intervals.iter().any(|i| {
                i.hazards_per_year.iter().flatten().any(|v| *v != 0.)
                    || i.credit_conversion_factor.iter().any(|v| *v != 0.)
            }))
    {
        return Err(
            "credit-free reference cannot carry issuer/borrower hazards or contingent funding"
                .into(),
        );
    }
    if r.accounting_rates.len() != r.credit.intervals.len()
        || r.accounting.recognition.len() != r.credit.intervals.len()
    {
        return Err("raw interval recognition/rate coverage missing".into());
    }
    let mut cuts: Vec<_> = r.credit.intervals.iter().map(|i| i.end_year).collect();
    for draw in &r.performing_draw_tranches {
        cuts.push(f64::from(draw.draw_ordinal - source.asof_ordinal) / 365.);
        for review in &draw.source.current_accounting_rate_reviews {
            cuts.push(f64::from(review.reset_ordinal - source.asof_ordinal) / 365.);
        }
    }
    let full = full_credit_end_year(r)?;
    if (full * 365.).ceil() > 4096. {
        return Err(
            "complete daily recovery clock exceeds 4096 intervals before generation".into(),
        );
    }
    for day in 1..=(full * 365.).ceil() as i32 {
        check()?;
        if f64::from(day) / 365. > full {
            break;
        }
        cuts.push(f64::from(day) / 365.);
    }
    cuts.push(full);
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| a.to_bits() == b.to_bits());
    if cuts.len() > 4096 {
        return Err("complete daily credit clock exceeds existing 4096-interval component bound before projection".into());
    }
    let mut terms = Vec::new();
    let mut reviews = Vec::new();
    for (index, end) in cuts.into_iter().enumerate() {
        check()?;
        let original_index = r
            .credit
            .intervals
            .iter()
            .position(|i| i.end_year >= end)
            .unwrap_or(r.credit.intervals.len() - 1);
        let i = &r.credit.intervals[original_index];
        let exhausted = end > years;
        terms.push(path::PathInterval {
            end_year: end,
            hazards_per_year: if exhausted {
                [[0.; 5]; 3]
            } else {
                i.hazards_per_year
            },
            accrual_rates: [0.; 3],
            loss_given_default: i.loss_given_default,
            credit_conversion_factor: i.credit_conversion_factor,
            interest_recovery_fraction: i.interest_recovery_fraction,
            complete_recovery_tails: i.complete_recovery_tails.clone(),
        });
        let mut rate = r.accounting_rates[original_index].clone();
        rate.interval_index = index;
        if exhausted {
            rate.annual_effective_rates = [0.; 3];
        }
        reviews.push(rate);
    }
    let mut boundaries: Vec<_> = source
        .periods
        .iter()
        .map(|p| path::PathBoundary {
            event_id: p.period_id.clone(),
            policy_id: r.accounting.identity.id.clone(),
            year: f64::from(p.payment_ordinal - source.asof_ordinal) / 365.,
            action: path::BoundaryAction::Repayment {
                principal_fraction: [0.; 3],
                interest_fraction: [0.; 3],
                cancel_undrawn_fraction: [0.; 3],
            },
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for draw in &r.performing_draw_tranches {
        boundaries.push(path::PathBoundary {
            event_id: draw_event_id(draw),
            policy_id: draw.review.id.clone(),
            year: f64::from(draw.draw_ordinal - source.asof_ordinal) / 365.,
            action: path::BoundaryAction::Draw {
                undrawn_fraction: [0.; 3],
            },
        });
        for period in &draw.source.current.periods {
            boundaries.push(path::PathBoundary {
                event_id: period.period_id.clone(),
                policy_id: draw.review.id.clone(),
                year: f64::from(period.payment_ordinal - source.asof_ordinal) / 365.,
                action: path::BoundaryAction::Repayment {
                    principal_fraction: [0.; 3],
                    interest_fraction: [0.; 3],
                    cancel_undrawn_fraction: [0.; 3],
                },
            });
        }
    }
    for stop in stops {
        check()?;
        if stop.ordinal <= source.asof_ordinal
            || stop.ordinal > source.tail_end_ordinal
            || !r.credit.paths.iter().any(|p| p.path_key == stop.path_key)
            || source.periods.iter().any(|p| p.period_id == stop.event_id)
            || !seen.insert(stop.path_key)
        {
            return Err("native stop must bind one in-tail decision per actual path and distinct source event identity".into());
        }
        boundaries.push(path::PathBoundary {
            event_id: stop.event_id.clone(),
            policy_id: stop.review.id.clone(),
            year: f64::from(stop.ordinal - source.asof_ordinal) / 365.,
            action: path::BoundaryAction::Maturity {},
        });
    }
    if let Some(policy) = &r.maturity_basis_policy {
        let event_id = maturity_event_id(policy);
        if boundaries.iter().any(|b| b.event_id == event_id) {
            return Err("reviewed maturity event identity overlaps a native source/stop".into());
        }
        boundaries.push(path::PathBoundary {
            event_id,
            policy_id: policy.review.id.clone(),
            year: years,
            action: path::BoundaryAction::Maturity {},
        });
    }
    // Stable sort leaves all exact source cash before native exercise on the
    // same date. Future source cash is atomically retired by the shared exit.
    let phase = |b: &path::PathBoundary| {
        stops.iter().find(|s| s.event_id == b.event_id).map_or(
            if matches!(b.action, path::BoundaryAction::Maturity {}) {
                2
            } else {
                1
            },
            |s| {
                if s.timing == NativeStopTiming::BeforeScheduledCash {
                    0
                } else {
                    2
                }
            },
        )
    };
    boundaries.sort_by(|a, b| {
        a.year
            .total_cmp(&b.year)
            .then_with(|| phase(a).cmp(&phase(b)))
    });
    let acquisition = &r.source.acquisition;
    let credit = path::CreditPathRequest {
        schema: path::SCHEMA.into(),
        measure,
        monetary_unit: source.monetary_unit.clone(),
        source_id: source.contract.id.clone(),
        source_revision: source.contract.revision.clone(),
        model_policy_id: r.credit.identity.id.clone(),
        origin_time: ObservationTime {
            ordinal: source.asof_ordinal,
            seconds_of_day: 0,
        },
        max_jumps_per_path: r.credit.max_jumps_per_path,
        intervals: terms,
        boundaries,
        draw_stream: r.credit.draw_stream.clone(),
        paths: r
            .credit
            .paths
            .iter()
            .map(|p| path::PathOpening {
                path_id: p.path_id.clone(),
                path_key: p.path_key,
                initial_live_state: p.initial_live_state,
                principal: source.opening_principal,
                undrawn_commitment: r.credit.opening_undrawn_commitment,
                accrued_interest: 0.,
                book_adjustment: acquisition.cost - source.opening_principal,
            })
            .collect(),
    };
    Ok(carry::Request {
        schema: carry::SCHEMA.into(),
        raw_credit_path: credit,
        original_anchors: r
            .credit
            .paths
            .iter()
            .map(|p| carry::OriginalAnchor {
                path_key: p.path_key,
                original_yield_anchor: acquisition.original_yield_anchor.id.clone(),
                acquisition_cost: acquisition.cost,
                original_annual_effective_rate: acquisition.original_annual_effective_rate,
            })
            .collect(),
        recognition_rates: reviews,
        policy_review: r.accounting.identity.id.clone(),
        partial_payment_policy: "cash_reduces_gross_no_proportional_basis_disposal-1".into(),
        default_policy: "stop_income_transfer_actual_gross_retain_nominal_claims-1".into(),
    })
}

fn prepared_accounting(r: &Request, p: &carry::Request) -> Result<book::Policy, String> {
    let mut result = r.accounting.clone();
    result.recognition.clear();
    for (index, interval) in p.raw_credit_path.intervals.iter().enumerate() {
        check()?;
        let original_index = r
            .credit
            .intervals
            .iter()
            .position(|i| i.end_year >= interval.end_year)
            .unwrap_or(r.credit.intervals.len() - 1);
        let mut review = r.accounting.recognition[original_index].clone();
        review.interval_index = index;
        if interval.end_year > source_end_year(r)? {
            review.by_origin = [book::Recognition::LegalOnlyIncomeStopped; 3];
        }
        result.recognition.push(review);
    }
    Ok(result)
}

fn rates(r: &Request, p: &carry::Request) -> Result<(), String> {
    let prospective = r.source.policy.reset_income == "reviewed_prospective_at_period_start-1";
    let mut starts = Vec::new();
    for i in &p.raw_credit_path.intervals {
        starts.push(if starts.is_empty() {
            0.
        } else {
            p.raw_credit_path.intervals[starts.len() - 1].end_year
        });
        if !i.end_year.is_finite() {
            return Err("invalid accounting interval".into());
        }
    }
    for (index, rate) in p.recognition_rates.iter().enumerate() {
        check()?;
        let mut expected = r.source.acquisition.original_annual_effective_rate;
        if prospective {
            for review in &r.source.current_accounting_rate_reviews {
                let at = f64::from(review.reset_ordinal - r.source.current.asof_ordinal) / 365.;
                if starts.get(index).is_some_and(|start| *start >= at) {
                    expected = review.annual_effective_rate;
                }
                if starts
                    .get(index)
                    .zip(p.raw_credit_path.intervals.get(index))
                    .is_some_and(|(start, interval)| at > *start && at < interval.end_year)
                {
                    return Err(
                        "accounting interval must split at every explicit prospective reset".into(),
                    );
                }
            }
        }
        let raw_index = r
            .credit
            .intervals
            .iter()
            .position(|i| i.end_year >= p.raw_credit_path.intervals[index].end_year)
            .unwrap_or(r.credit.intervals.len() - 1);
        let mode = r
            .accounting
            .recognition
            .get(raw_index)
            .ok_or("missing reviewed recognition interval")?;
        for origin in 0..3 {
            let exhausted = starts[index] >= source_end_year(r)?;
            let required = if exhausted
                || mode.by_origin[origin] == book::Recognition::LegalOnlyIncomeStopped
            {
                0.
            } else {
                expected
            };
            if rate.annual_effective_rates[origin].to_bits() != required.to_bits() {
                return Err("accounting rate must bind original/prospective anchor or explicit stopped-income policy".into());
            }
        }
    }
    Ok(())
}

struct SourceAdapter<'a> {
    source: &'a NativeFactors,
    path_keys: Vec<u64>,
    stops: &'a [NativeStop],
    prepayment_terms: Option<&'a PrepaymentTerms>,
    maturity_basis_policy: Option<&'a MaturityBasisPolicy>,
    draw_sources: &'a [NativeDrawSource],
    active_draws: std::collections::BTreeSet<usize>,
    accounting: Option<&'a book::Policy>,
    complete_end_year: f64,
    interval_starts: Vec<f64>,
}
struct SourceView<'a> {
    source: &'a NativeFactors,
    audit_offset: usize,
    cash_offset: usize,
    segment_offset: usize,
    tranche_id: Option<&'a str>,
}
fn relative_source_year(base_asof: i32, source_asof: i32, global: f64) -> Result<f64, String> {
    if !global.is_finite() {
        return Err("nonfinite global source clock".into());
    }
    let day = (global * 365.).round();
    if global.to_bits() == (day / 365.).to_bits() {
        let offset = source_asof
            .checked_sub(base_asof)
            .ok_or("tranche source clock overflow")?;
        Ok((day - f64::from(offset)) / 365.)
    } else {
        Ok(global - f64::from(source_asof - base_asof) / 365.)
    }
}
impl<'a> SourceAdapter<'a> {
    fn with_draws(
        mut self,
        sources: &'a [NativeDrawSource],
        accounting: &'a book::Policy,
        credit: &carry::Request,
    ) -> Self {
        self.draw_sources = sources;
        self.accounting = Some(accounting);
        self.interval_starts = std::iter::once(0.)
            .chain(credit.raw_credit_path.intervals.iter().map(|i| i.end_year))
            .take(credit.raw_credit_path.intervals.len())
            .collect();
        for d in sources {
            self.complete_end_year = self.complete_end_year.max(
                f64::from(d.current.raw().tail_end_ordinal - self.source.raw().asof_ordinal) / 365.,
            );
        }
        self
    }
    fn views(&self) -> Vec<SourceView<'_>> {
        let mut views = vec![SourceView {
            source: self.source,
            audit_offset: 0,
            cash_offset: 0,
            segment_offset: 0,
            tranche_id: None,
        }];
        let mut audit = self.source.source().audit.len();
        let mut cash = self.source.source().schedule.events.len();
        let mut segment = self.source.segments().len();
        for (i, d) in self.draw_sources.iter().enumerate() {
            if self.active_draws.contains(&i) {
                views.push(SourceView {
                    source: &d.current,
                    audit_offset: audit,
                    cash_offset: cash,
                    segment_offset: segment,
                    tranche_id: Some(&d.terms.review.id),
                });
            }
            audit += d.current.source().audit.len();
            cash += d.current.source().schedule.events.len();
            segment += d.current.segments().len();
        }
        views
    }
    fn new(
        source: &'a NativeFactors,
        path_keys: Vec<u64>,
        stops: &'a [NativeStop],
        prepayment_terms: Option<&'a PrepaymentTerms>,
        maturity_basis_policy: Option<&'a MaturityBasisPolicy>,
    ) -> Result<Self, String> {
        Ok(Self {
            source,
            path_keys,
            stops,
            prepayment_terms,
            maturity_basis_policy,
            draw_sources: &[],
            active_draws: std::collections::BTreeSet::new(),
            accounting: None,
            complete_end_year: contract_end_year(source.raw())?,
            interval_starts: Vec::new(),
        })
    }
}
impl carry::NativeEarnedFactors for SourceAdapter<'_> {
    fn begin_path(&mut self, _path_key: u64) -> Result<(), String> {
        self.active_draws.clear();
        Ok(())
    }
    fn draw(
        &mut self,
        q: &carry::NativeDrawQuery<'_>,
    ) -> Result<Option<carry::NativeDrawResolution>, String> {
        let Some((index, d)) = self
            .draw_sources
            .iter()
            .enumerate()
            .find(|(_, d)| draw_event_id(&d.terms) == q.event_id)
        else {
            return Ok(None);
        };
        if q.year.to_bits()
            != (f64::from(d.terms.draw_ordinal - self.source.raw().asof_ordinal) / 365.).to_bits()
        {
            return Err("native draw source global clock mismatch".into());
        }
        if !d.terms.eligible_origins[q.origin_index] {
            return Ok(Some(carry::NativeDrawResolution::Skip));
        }
        if !self.active_draws.insert(index) {
            return Err("native performing tranche repeated draw".into());
        }
        Ok(Some(carry::NativeDrawResolution::Fund {
            tranche_id: d.terms.review.id.clone(),
            principal: d.current.raw().opening_principal,
            cost: d.terms.source.acquisition.cost,
            original_annual_effective_rate: d
                .terms
                .source
                .acquisition
                .original_annual_effective_rate,
        }))
    }
    fn tranche_accounting_rate(
        &self,
        id: &str,
        interval: usize,
        origin: usize,
    ) -> Result<Option<f64>, String> {
        let Some(d) = self.draw_sources.iter().find(|d| d.terms.review.id == id) else {
            return Ok(None);
        };
        let mode = self
            .accounting
            .and_then(|p| p.recognition.get(interval))
            .ok_or("missing explicit drawn tranche recognition review")?
            .by_origin[origin];
        let mut selected = d.terms.source.acquisition.original_annual_effective_rate;
        let start = *self
            .interval_starts
            .get(interval)
            .ok_or("missing drawn tranche interval prefix")?;
        if d.terms.source.policy.reset_income == "reviewed_prospective_at_period_start-1" {
            for review in &d.terms.source.current_accounting_rate_reviews {
                if f64::from(review.reset_ordinal - self.source.raw().asof_ordinal) / 365. <= start
                {
                    selected = review.annual_effective_rate;
                }
            }
        }
        Ok(Some(if mode == book::Recognition::LegalOnlyIncomeStopped {
            0.
        } else {
            selected
        }))
    }
    fn cumulative_coupon_factors(
        &self,
        year: f64,
    ) -> Result<Option<Vec<carry::NativeCumulativeCouponFactor>>, String> {
        let mut result = Vec::new();
        for v in self.views() {
            for (audit, cash, factor) in v.source.cumulative_coupon_factors(
                relative_source_year(
                    self.source.raw().asof_ordinal,
                    v.source.raw().asof_ordinal,
                    year,
                )?
                .max(0.),
            )? {
                let a = &v.source.source().audit[audit];
                result.push(carry::NativeCumulativeCouponFactor {
                    coupon_id: a.period_id.clone(),
                    native_audit_index: audit + v.audit_offset,
                    native_cash_index: cash + v.cash_offset,
                    opening_entitlement_principal: a.opening_principal,
                    cumulative_factor: factor,
                });
            }
        }
        Ok(Some(result))
    }
    fn boundary_applies(&self, path_key: u64, event_id: &str) -> Result<bool, String> {
        if let Some(stop) = self.stops.iter().find(|s| s.event_id == event_id) {
            return Ok(stop.path_key == path_key);
        }
        for (index, d) in self.draw_sources.iter().enumerate() {
            if d.current
                .raw()
                .periods
                .iter()
                .any(|p| p.period_id == event_id)
            {
                return Ok(self.active_draws.contains(&index));
            }
        }
        Ok(true)
    }
    fn exit_cash(
        &self,
        q: &carry::NativeExitQuery<'_>,
    ) -> Result<Option<carry::NativeExitCash>, String> {
        check()?;
        if let Some(policy) = self.maturity_basis_policy {
            if q.event_id == maturity_event_id(policy) {
                if q.is_prepayment
                    || q.year.to_bits() != self.complete_end_year.to_bits()
                    || q.principal != 0.
                    || q.legal_interest != 0.
                {
                    return Err("reviewed maturity basis disposal requires exact complete-tail legal principal and interest closure".into());
                }
                return Ok(Some(carry::NativeExitCash {
                    principal_cash: 0.,
                    fee: 0.,
                }));
            }
        }
        if q.is_prepayment {
            return Ok(self.prepayment_terms.map(|t| carry::NativeExitCash {
                principal_cash: q.principal,
                fee: q.principal * t.principal_premium_fraction,
            }));
        }
        let stop = self
            .stops
            .iter()
            .find(|s| s.path_key == q.path_key && s.event_id == q.event_id)
            .ok_or("missing native exercise policy for actual path/state exit")?;
        let year = f64::from(stop.ordinal - self.source.raw().asof_ordinal) / 365.;
        if year.to_bits() != q.year.to_bits() {
            return Err("native exercise/source clock mismatch".into());
        }
        if q.principal == 0.
            && stop.rights_only_review.is_none()
            && (q.legal_interest != 0.
                || self
                    .source
                    .raw()
                    .periods
                    .iter()
                    .any(|p| p.payment_ordinal > stop.ordinal || p.end_ordinal > stop.ordinal)
                || self.draw_sources.iter().any(|d| {
                    d.terms.draw_ordinal > stop.ordinal
                        || d.current.raw().periods.iter().any(|p| {
                            p.payment_ordinal > stop.ordinal || p.end_ordinal > stop.ordinal
                        })
                }))
        {
            return Err(
                "rights-only native stop requires reviewed legal termination policy".into(),
            );
        }
        Ok(Some(carry::NativeExitCash {
            principal_cash: q.principal * stop.principal_clean_price_factor,
            fee: stop.fee,
        }))
    }
    fn bound(&self) -> carry::NativeFactorBound {
        carry::NativeFactorBound {
            maximum_atoms_per_query: self.source.admission().segment_bound
                + self
                    .draw_sources
                    .iter()
                    .map(|d| d.current.admission().segment_bound)
                    .sum::<usize>(),
            retained_source_bytes: self.source.admission().retained_bytes_upper
                + self
                    .draw_sources
                    .iter()
                    .map(|d| {
                        d.current.admission().retained_bytes_upper
                            + d.original.admission().retained_bytes_upper
                    })
                    .sum::<usize>(),
            source_work: self.source.admission().work_upper
                + self
                    .draw_sources
                    .iter()
                    .map(|d| d.current.admission().work_upper + d.original.admission().work_upper)
                    .sum::<usize>(),
        }
    }
    fn maximum_source_cash_per_path(&self) -> usize {
        self.source.source().schedule.events.len()
            + self
                .draw_sources
                .iter()
                .map(|d| d.current.source().schedule.events.len())
                .sum::<usize>()
    }
    fn maximum_gross_coupon_claims_per_path(&self) -> usize {
        (self.source.source().audit.len()
            + self
                .draw_sources
                .iter()
                .map(|d| d.current.source().audit.len())
                .sum::<usize>())
            * 3
    }
    fn earned(
        &mut self,
        q: &carry::NativeFactorQuery,
        sink: &mut dyn FnMut(carry::NativeFactorAtom<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        check()?;
        if !self.path_keys.contains(&q.path_key) {
            return Err("unbound actual credit/source path key".into());
        }
        for v in self.views() {
            let full = contract_end_year(v.source.raw())?;
            let start = relative_source_year(
                self.source.raw().asof_ordinal,
                v.source.raw().asof_ordinal,
                q.start_year,
            )?
            .max(0.);
            let end = relative_source_year(
                self.source.raw().asof_ordinal,
                v.source.raw().asof_ordinal,
                q.end_year,
            )?
            .min(full);
            if start >= full || end <= start {
                continue;
            }
            for atom in v.source.earned(start, end)? {
                check()?;
                sink(carry::NativeFactorAtom {
                    coupon_id: atom.period_id,
                    source_event_id: atom.native_source_event_id,
                    source_id: &v.source.raw().contract.id,
                    source_revision: &v.source.raw().contract.revision,
                    captured_segment_index: atom.segment_index + v.segment_offset,
                    delta_factor: atom.delta_factor,
                })?;
            }
        }
        Ok(())
    }
    fn entitlement(
        &self,
        atom: &carry::NativeFactorAtom<'_>,
    ) -> Result<Option<carry::NativeCouponEntitlement>, String> {
        let v = self
            .views()
            .into_iter()
            .find(|v| {
                atom.captured_segment_index >= v.segment_offset
                    && atom.captured_segment_index < v.segment_offset + v.source.segments().len()
            })
            .ok_or("unbound actual tranche source segment")?;
        let segment = v
            .source
            .segments()
            .get(atom.captured_segment_index - v.segment_offset)
            .ok_or("unbound native factor segment")?;
        let audit = v
            .source
            .source()
            .audit
            .get(segment.audit_index())
            .ok_or("unbound native entitlement audit")?;
        let cash_index = v
            .source
            .source()
            .schedule
            .events
            .iter()
            .position(|c| c.event_id == segment.period_id())
            .ok_or("unbound indexed native cash origin")?;
        let cash = &v.source.source().schedule.events[cash_index];
        if audit.period_id != atom.coupon_id
            || cash.event_id != atom.source_event_id
            || atom.source_id != v.source.raw().contract.id
            || atom.source_revision != v.source.raw().contract.revision
        {
            return Err("actual factor/entitlement/source identity mismatch".into());
        }
        Ok(Some(carry::NativeCouponEntitlement {
            native_audit_index: segment.audit_index() + v.audit_offset,
            native_cash_index: cash_index + v.cash_offset,
            opening_entitlement_principal: audit.opening_principal,
        }))
    }
    fn contractual_cash(
        &self,
        id: &str,
        year: f64,
    ) -> Result<Option<carry::NativeContractualCash>, String> {
        let (v, index) = self
            .views()
            .into_iter()
            .find_map(|v| {
                v.source
                    .source()
                    .schedule
                    .events
                    .iter()
                    .position(|c| c.event_id == id)
                    .map(|i| (v, i))
            })
            .ok_or("actual source event missing; no scalar fraction fallback")?;
        let cash = &v.source.source().schedule.events[index];
        let native_year = f64::from(cash.payment_ordinal - self.source.raw().asof_ordinal) / 365.;
        if native_year.to_bits() != year.to_bits() {
            return Err("actual source cash clock mismatch".into());
        }
        Ok(Some(carry::NativeContractualCash {
            coupon_id: id.into(),
            source_event_id: id.into(),
            native_cash_index: index + v.cash_offset,
            principal: cash.components.principal,
            interest: cash.components.cash_interest,
            fee: cash.components.fees,
            tranche_id: v.tranche_id.map(str::to_owned),
        }))
    }
}

struct Prepared {
    credit: carry::Request,
    policy: book::Policy,
    admission: Admission,
}
#[derive(Debug, Serialize)]
pub struct EconomicResult {
    #[serde(serialize_with = "serialize_factors")]
    pub original_source: NativeFactors,
    #[serde(serialize_with = "serialize_factors")]
    pub current_source: NativeFactors,
    pub carry: carry::ResultData,
    pub admission: Admission,
    pub physical_journal_eligible: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub native_stops: Vec<NativeStop>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub draw_sources: Vec<NativeDrawSource>,
}
impl EconomicResult {
    pub fn carry(&self) -> &carry::ResultData {
        &self.carry
    }
    pub fn current_source(&self) -> &NativeFactors {
        &self.current_source
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
}
/// Sealed proof generated from complete raw acquired Physical paths and native
/// stopping policies. It cannot be supplied as prepared request money.
#[derive(Debug, Serialize)]
pub(crate) struct NativeOriginalAcquisitionProof {
    original_stopped_cash_quote: f64,
    conditional_tranche_proofs:
        Vec<crate::products::commercial::factor_source::acquisition::ConditionalTrancheProof>,
    acquired_cash_partitions:
        Vec<crate::products::commercial::factor_source::acquisition::AcquiredPathCash>,
    acquisition: source_income::Acquisition,
    weights: Vec<f64>,
    stopped_physical: Vec<EconomicResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    generated_original_physical: Vec<GeneratedOriginalPhysical>,
    #[serde(skip)]
    original_bindings: Vec<(u64, Vec<u8>)>,
    #[serde(skip)]
    draw_bindings: Vec<Vec<u8>>,
}
fn original_draw_bindings(raw: &Request) -> Result<Vec<u8>, String> {
    #[derive(Serialize)]
    struct DrawAnchor<'a> {
        review: &'a term::Identity,
        draw_ordinal: i32,
        eligible_origins: [bool; 3],
        original: &'a term::indexed::ContractPathRequest,
        acquisition: &'a source_income::Acquisition,
    }
    let anchors: Vec<_> = raw
        .performing_draw_tranches
        .iter()
        .map(|d| DrawAnchor {
            review: &d.review,
            draw_ordinal: d.draw_ordinal,
            eligible_origins: d.eligible_origins,
            original: &d.source.original,
            acquisition: &d.source.acquisition,
        })
        .collect();
    portfolio_model_core::strict_json::encode(&anchors, MAX_BYTES)
}
#[derive(Debug, Serialize)]
struct GeneratedOriginalPhysical {
    raw_request: Request,
    native_stops: Vec<NativeStop>,
}
impl NativeOriginalAcquisitionProof {
    pub(crate) fn compile(inputs: &[(&Request, f64, &[NativeStop])]) -> Result<Self, String> {
        Self::compile_internal(inputs, false)
    }
    /// Separate compact generated-sample proof tier; ordinary proof admission
    /// and each underlying native source retain their existing bounds.
    pub(crate) fn compile_generated(
        inputs: &[(&Request, f64, &[NativeStop])],
    ) -> Result<Self, String> {
        Self::compile_internal(inputs, true)
    }
    fn compile_internal(
        inputs: &[(&Request, f64, &[NativeStop])],
        generated: bool,
    ) -> Result<Self, String> {
        check()?;
        let first = inputs
            .first()
            .ok_or("original acquisition requires complete weighted Physical histories")?
            .0;
        let acquisition = first.source.acquisition.clone();
        let mut weight_sum = 0.;
        let mut work = 0usize;
        let mut bytes = 0usize;
        let mut path_keys = std::collections::BTreeSet::new();
        let mut generated_compact = 0usize;
        if generated && !(128..=512).contains(&inputs.len()) {
            return Err("generated original proof requires explicit 128..512 complete generated Physical samples".into());
        }
        for (raw, weight, stops) in inputs {
            check()?;
            if raw.performing_draw_tranches.len() != first.performing_draw_tranches.len()
                || raw
                    .performing_draw_tranches
                    .iter()
                    .zip(&first.performing_draw_tranches)
                    .any(|(d, f)| {
                        d.review != f.review
                            || d.draw_ordinal != f.draw_ordinal
                            || d.eligible_origins != f.eligible_origins
                            || d.source.original.opening_principal.to_bits()
                                != f.source.original.opening_principal.to_bits()
                    })
            {
                return Err("original acquired tranche distribution requires exact source ordering, eligibility and nominal/date anchors".into());
            }
            for (draw, anchor) in raw
                .performing_draw_tranches
                .iter()
                .zip(&first.performing_draw_tranches)
            {
                if portfolio_model_core::strict_json::encode(&draw.source.original, MAX_BYTES)?
                    != portfolio_model_core::strict_json::encode(&draw.source.current, MAX_BYTES)?
                    || portfolio_model_core::strict_json::encode(
                        &draw.source.acquisition,
                        MAX_BYTES,
                    )? != portfolio_model_core::strict_json::encode(
                        &anchor.source.acquisition,
                        MAX_BYTES,
                    )?
                {
                    return Err("original conditional tranche proof requires complete original Physical sources with unchanged own acquisition anchors".into());
                }
            }
            if raw.credit.paths.len() != 1
                || !path_keys.insert(raw.credit.paths[0].path_key)
                || !weight.is_finite()
                || *weight <= 0.
                || *weight > 1.
                || portfolio_model_core::strict_json::encode(&raw.source.current, MAX_BYTES)?
                    != portfolio_model_core::strict_json::encode(&raw.source.original, MAX_BYTES)?
                || portfolio_model_core::strict_json::encode(&raw.source.acquisition, MAX_BYTES)?
                    != portfolio_model_core::strict_json::encode(&acquisition, MAX_BYTES)?
            {
                return Err("original acquisition requires distinct complete original Physical source paths with unchanged purchase anchors".into());
            }
            let a = admit_with_stops(raw, stops)?;
            work = add(work, a.joined_work_upper)?;
            bytes = if generated {
                bytes.max(a.joined_bytes_upper)
            } else {
                add(bytes, a.joined_bytes_upper)?
            };
            if generated {
                generated_compact = add(
                    generated_compact,
                    add(
                        mul(
                            portfolio_model_core::strict_json::serialized_size(raw, 1 << 20)?,
                            20,
                        )?,
                        mul(raw.source.current.periods.len() + 36, 4096)?,
                    )?,
                )?;
            }
            weight_sum += weight;
        }
        if (weight_sum - 1.).abs() > 1e-12
            || work > if generated { 4_000_000_000 } else { MAX_WORK }
            || add(bytes, generated_compact)? > MAX_BYTES
        {
            return Err("original acquisition distribution/admission is incomplete or exceeds native aggregate bounds".into());
        }
        let mut quote = 0.;
        let mut stopped_physical = Vec::new();
        let mut original_bindings = Vec::new();
        let mut draw_bindings = Vec::new();
        let mut acquired_cash_partitions = Vec::new();
        let mut generated_original_physical = Vec::new();
        let mut tranche_quotes = vec![0.; first.performing_draw_tranches.len()];
        let mut activation_probabilities = vec![0.; first.performing_draw_tranches.len()];
        for (raw, weight, stops) in inputs {
            let result = evaluate_economic_internal(raw, CreditMeasure::Physical, stops, false)?;
            let mut cash_quote = 0.;
            for path in &result.carry.credit().result.paths {
                for event in &path.events {
                    check()?;
                    cash_quote += event.cash
                        * (-acquisition.original_annual_effective_rate.ln_1p() * event.year).exp();
                }
                if let Some(default) = &path.default {
                    for recovery in &default.recoveries {
                        check()?;
                        cash_quote += (recovery.principal + recovery.interest)
                            * (-acquisition.original_annual_effective_rate.ln_1p() * recovery.year)
                                .exp();
                    }
                }
            }
            let partition =
                crate::products::commercial::factor_source::acquisition::partition(raw, &result)?;
            if !raw.performing_draw_tranches.is_empty() {
                cash_quote = partition
                    .base
                    .iter()
                    .map(|c| {
                        c.amount
                            * (-acquisition.original_annual_effective_rate.ln_1p() * c.year).exp()
                    })
                    .sum();
                for (i, draw) in raw.performing_draw_tranches.iter().enumerate() {
                    if partition.acquired[i] {
                        activation_probabilities[i] += weight;
                        let origin =
                            f64::from(draw.draw_ordinal - raw.source.current.asof_ordinal) / 365.;
                        let rate = draw.source.acquisition.original_annual_effective_rate;
                        let pv: f64 = partition.tranches[i]
                            .iter()
                            .map(|c| c.amount * (-rate.ln_1p() * (c.year - origin)).exp())
                            .sum();
                        tranche_quotes[i] += weight * pv;
                    }
                }
            }
            acquired_cash_partitions.push(partition);
            term::amount(cash_quote)?;
            quote += weight * cash_quote;
            original_bindings.push((
                raw.credit.paths[0].path_key,
                portfolio_model_core::strict_json::encode(&raw.source.original, MAX_BYTES)?,
            ));
            draw_bindings.push(original_draw_bindings(raw)?);
            if generated {
                generated_original_physical.push(GeneratedOriginalPhysical {
                    raw_request: (*raw).clone(),
                    native_stops: stops.to_vec(),
                });
            } else {
                stopped_physical.push(result);
            }
        }
        term::amount(quote)?;
        if !term::near(quote, acquisition.cost) {
            return Err("original stopped Physical complete cash does not reconcile retained purchase quote/effective yield".into());
        }
        let mut conditional_tranche_proofs = Vec::new();
        for (i, draw) in first.performing_draw_tranches.iter().enumerate() {
            let probability = activation_probabilities[i];
            if probability <= 0. {
                return Err("declared acquired tranche lacks complete positive original activation evidence".into());
            }
            let conditional = tranche_quotes[i] / probability;
            term::amount(conditional)?;
            if !term::near(conditional, draw.source.acquisition.cost) {
                return Err("original conditional acquired-tranche cash does not reconcile its own cost/effective yield".into());
            }
            conditional_tranche_proofs.push(
                crate::products::commercial::factor_source::acquisition::ConditionalTrancheProof {
                    tranche_id: draw.review.id.clone(),
                    acquisition: draw.source.acquisition.clone(),
                    draw_ordinal: draw.draw_ordinal,
                    activation_probability: probability,
                    conditional_cash_quote: conditional,
                    original_annual_effective_rate: draw
                        .source
                        .acquisition
                        .original_annual_effective_rate,
                    allocation_policy: "native_source_principal_coupon_ownership_base_exit_fee-1",
                },
            );
        }
        let proof = Self {
            original_stopped_cash_quote: quote,
            conditional_tranche_proofs,
            acquired_cash_partitions,
            acquisition,
            weights: inputs.iter().map(|(_, w, _)| *w).collect(),
            stopped_physical,
            generated_original_physical,
            original_bindings,
            draw_bindings,
        };
        portfolio_model_core::strict_json::serialized_size(&proof, MAX_BYTES)?;
        check()?;
        Ok(proof)
    }
    fn quote_for(&self, raw: &Request) -> Result<f64, String> {
        check()?;
        let original = portfolio_model_core::strict_json::encode(&raw.source.original, MAX_BYTES)?;
        if raw.credit.paths.len() != 1
            || !self
                .original_bindings
                .contains(&(raw.credit.paths[0].path_key, original))
            || !self.draw_bindings.contains(&original_draw_bindings(raw)?)
            || portfolio_model_core::strict_json::encode(&self.acquisition, MAX_BYTES)?
                != portfolio_model_core::strict_json::encode(&raw.source.acquisition, MAX_BYTES)?
        {
            return Err("current Physical source does not bind immutable original acquired stopping proof/anchors".into());
        }
        Ok(self.original_stopped_cash_quote)
    }
}
pub(crate) fn evaluate_physical_prefix_with_stops_for_acquisition(
    r: &Request,
    stops: &[NativeStop],
) -> Result<EconomicResult, String> {
    evaluate_economic_internal(r, CreditMeasure::Physical, stops, false)
}
/// A pricing graph selects independently supplied Pricing source/conditions.
/// Identical native cash ownership is reused; no accounting book is compiled.
pub fn evaluate_economic_with_measure(
    r: &Request,
    measure: CreditMeasure,
) -> Result<EconomicResult, String> {
    evaluate_economic_with_stops(r, measure, &[])
}
pub fn evaluate_economic_with_stops(
    r: &Request,
    measure: CreditMeasure,
    stops: &[NativeStop],
) -> Result<EconomicResult, String> {
    evaluate_economic_internal(r, measure, stops, true)
}
pub(crate) fn evaluate_physical_prefix_for_acquisition(
    r: &Request,
) -> Result<EconomicResult, String> {
    evaluate_economic_internal(r, CreditMeasure::Physical, &[], false)
}
fn evaluate_economic_internal(
    r: &Request,
    measure: CreditMeasure,
    stops: &[NativeStop],
    check_original_quote: bool,
) -> Result<EconomicResult, String> {
    if measure == CreditMeasure::Pricing && r.reviewed_credit_actions.is_some() {
        return Err("reviewed credit book actions require Physical accounting adoption; Pricing policy is separately configured".into());
    }
    let Prepared {
        credit, admission, ..
    } = prepare(r, 0, 0, measure, stops)?;
    let original_source = NativeFactors::compile_for_measure(
        r.source.original.clone(),
        r.factor_policy.clone(),
        0,
        source_measure(measure),
    )?;
    let current_source = NativeFactors::compile_for_measure(
        r.source.current.clone(),
        r.factor_policy.clone(),
        admission.maximum_queries,
        source_measure(measure),
    )?;
    let draw_sources = if check_original_quote {
        project_draws(r, measure, admission.maximum_queries)?
    } else {
        project_draws_for_acquisition(r, measure, admission.maximum_queries)?
    };
    if measure == CreditMeasure::Physical && check_original_quote {
        source_income::original_cash_quote(&r.source, original_source.source())?;
    }
    let policy = prepared_accounting(r, &credit)?;
    let mut adapter = SourceAdapter::new(
        &current_source,
        r.credit.paths.iter().map(|p| p.path_key).collect(),
        stops,
        r.prepayment_terms.as_ref(),
        r.maturity_basis_policy.as_ref(),
    )?
    .with_draws(&draw_sources, &policy, &credit);
    let carry = carry::evaluate_economic_with_native_factors(&credit, &mut adapter, &mut check)?;
    if carry.carry_points().len() > admission.carry_points_upper
        || carry.admission().work_upper > admission.carry.work_upper
        || carry.admission().retained_bytes_upper > admission.carry.retained_bytes_upper
    {
        return Err("observed economic credit/source output exceeds native admission".into());
    }
    check()?;
    let result = EconomicResult {
        original_source,
        current_source,
        carry,
        admission,
        // This economic helper never compiles the reviewed accounting book.
        // Even a Physical prefix is not a prepared journal owner.
        physical_journal_eligible: false,
        native_stops: stops.to_vec(),
        draw_sources,
    };
    check()?;
    Ok(result)
}
/// Complete policy/source/credit and retained-output admission, no generation.
pub fn admit(r: &Request) -> Result<Admission, String> {
    admit_with_stops(r, &[])
}
pub fn admit_with_stops(r: &Request, stops: &[NativeStop]) -> Result<Admission, String> {
    Ok(prepare(r, 0, 0, CreditMeasure::Physical, stops)?.admission)
}
pub fn admit_economic_with_measure(
    r: &Request,
    measure: CreditMeasure,
) -> Result<Admission, String> {
    admit_economic_with_stops(r, measure, &[])
}
pub fn admit_economic_with_stops(
    r: &Request,
    measure: CreditMeasure,
    stops: &[NativeStop],
) -> Result<Admission, String> {
    Ok(prepare(r, 0, 0, measure, stops)?.admission)
}
fn source_measure(measure: CreditMeasure) -> portfolio_model_core::cashflow::CashflowMeasure {
    match measure {
        CreditMeasure::Physical => portfolio_model_core::cashflow::CashflowMeasure::Physical,
        CreditMeasure::Pricing => portfolio_model_core::cashflow::CashflowMeasure::Pricing,
    }
}
fn prepare(
    r: &Request,
    reserved_work: usize,
    reserved_bytes: usize,
    measure: CreditMeasure,
    stops: &[NativeStop],
) -> Result<Prepared, String> {
    check()?;
    if r.schema != SCHEMA {
        return Err("unsupported commercial native compiler schema".into());
    }
    let raw_bytes = portfolio_model_core::strict_json::serialized_size(r, 1 << 20)?;
    let native_stop_bytes = portfolio_model_core::strict_json::serialized_size(&stops, 1 << 20)?;
    let retained_raw_bytes = add(raw_bytes, native_stop_bytes)?;
    source_income::admit_for_measure(&r.source, source_measure(measure))?;
    let mut meta = 0;
    term::identity(&r.credit.identity, &mut meta)?;
    let p = prepared(r, measure, stops)?;
    path::validate_request_with_check(&p.raw_credit_path, &mut check)?;
    rates(r, &p)?;
    let prepared_policy = prepared_accounting(r, &p)?;
    if measure == CreditMeasure::Physical {
        book::validate_policy(&p, &prepared_policy, &mut check)?;
    }
    let sojourns = add(
        add(
            p.raw_credit_path.intervals.len(),
            p.raw_credit_path.boundaries.len(),
        )?,
        add(r.credit.max_jumps_per_path, 1)?,
    )?;
    let queries = mul(r.credit.paths.len(), mul(sojourns, 2)?)?;
    let original_admission = NativeFactors::admit_for_measure(
        &r.source.original,
        &r.factor_policy,
        0,
        source_measure(measure),
    )?;
    let current_admission = NativeFactors::admit_for_measure(
        &r.source.current,
        &r.factor_policy,
        queries,
        source_measure(measure),
    )?;
    let mut source_work = add(original_admission.work_upper, current_admission.work_upper)?;
    let mut source_bytes = add(
        original_admission.retained_bytes_upper,
        current_admission.retained_bytes_upper,
    )?;
    let mut segments = current_admission.segment_bound;
    let mut periods = r.source.current.periods.len();
    for draw in &r.performing_draw_tranches {
        let o = NativeFactors::admit_for_measure(
            &draw.source.original,
            &r.factor_policy,
            0,
            source_measure(measure),
        )?;
        let c = NativeFactors::admit_for_measure(
            &draw.source.current,
            &r.factor_policy,
            queries,
            source_measure(measure),
        )?;
        source_work = add(source_work, add(o.work_upper, c.work_upper)?)?;
        source_bytes = add(
            source_bytes,
            add(o.retained_bytes_upper, c.retained_bytes_upper)?,
        )?;
        segments = add(segments, c.segment_bound)?;
        periods = add(periods, draw.source.current.periods.len())?;
    }
    let bound = carry::NativeFactorBound {
        maximum_atoms_per_query: segments,
        retained_source_bytes: source_bytes,
        source_work,
    };
    let carry_admission =
        carry::admit_economic_with_native_bound(&p, &bound, periods, mul(periods, 3)?, &mut check)?;
    let per_path_points = add(
        sojourns,
        add(
            add(
                p.raw_credit_path.boundaries.len(),
                r.credit.max_jumps_per_path,
            )?,
            68,
        )?,
    )?;
    let lots = mul(periods, 3)?;
    let reviewed_count = if let Some(actions) = &r.reviewed_credit_actions {
        book::reviews::admit(
            actions,
            &r.credit
                .paths
                .iter()
                .map(|p| p.path_key)
                .collect::<Vec<_>>(),
            r.source.current.asof_ordinal,
            r.source
                .current
                .asof_ordinal
                .checked_add((full_credit_end_year(r)? * 365.).ceil() as i32)
                .ok_or("review horizon overflow")?,
        )?
    } else {
        0
    };
    let accounting_groups_upper = add(mul(r.credit.paths.len(), per_path_points)?, reviewed_count)?;
    let book_admission = book::admit(accounting_groups_upper, lots)?;
    // Each selected legal waiver remains explicit in every later snapshot.
    // Charge the full bounded reviewed-history clone before generation.
    let review_cells = mul(accounting_groups_upper, reviewed_count)?;
    let book_work = add(book_admission.work_upper, mul(review_cells, 128)?)?;
    let book_bytes = add(
        book_admission.retained_bytes_upper,
        mul(review_cells, 1024)?,
    )?;
    let work = add(
        add(carry_admission.work_upper, mul(retained_raw_bytes, 8)?)?,
        book_work,
    )?;
    let bytes = add(
        add(
            carry_admission.retained_bytes_upper,
            mul(retained_raw_bytes, 20)?,
        )?,
        book_bytes,
    )?;
    if add(work, reserved_work)? > MAX_WORK || add(bytes, reserved_bytes)? > MAX_BYTES {
        return Err(
            "commercial joined preflight exceeds 8M/128MiB before any source generation".into(),
        );
    }
    let admission = Admission {
        raw_bytes,
        native_stop_bytes,
        maximum_queries: queries,
        carry_points_upper: mul(r.credit.paths.len(), per_path_points)?,
        accounting_groups_upper,
        lots_per_path_upper: lots,
        original_source_work: original_admission.work_upper,
        current_source_work: current_admission.work_upper,
        carry: carry_admission,
        book_work_upper: book_work,
        book_bytes_upper: book_bytes,
        joined_work_upper: work,
        joined_bytes_upper: bytes,
        full_credit_end_year: full_credit_end_year(r)?,
    };
    Ok(Prepared {
        credit: p,
        policy: prepared_policy,
        admission,
    })
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_reservation(r, 0, 0)
}
pub fn evaluate_with_stops(r: &Request, stops: &[NativeStop]) -> Result<ResultData, String> {
    evaluate_with_stops_and_reservation(r, stops, 0, 0)
}
/// Additional native coordinator reservations are admitted before both sources.
/// They are not caller-supplied JSON checkpoints or financial amounts.
pub fn evaluate_with_reservation(
    r: &Request,
    reserved_work: usize,
    reserved_bytes: usize,
) -> Result<ResultData, String> {
    evaluate_with_stops_and_reservation(r, &[], reserved_work, reserved_bytes)
}
pub fn evaluate_with_stops_and_reservation(
    r: &Request,
    stops: &[NativeStop],
    reserved_work: usize,
    reserved_bytes: usize,
) -> Result<ResultData, String> {
    evaluate_accounting_internal(r, stops, reserved_work, reserved_bytes, None)
}
pub(crate) fn evaluate_with_native_original_acquisition(
    r: &Request,
    stops: &[NativeStop],
    proof: &NativeOriginalAcquisitionProof,
) -> Result<ResultData, String> {
    evaluate_accounting_internal(r, stops, 0, 0, Some(proof))
}
fn evaluate_accounting_internal(
    r: &Request,
    stops: &[NativeStop],
    reserved_work: usize,
    reserved_bytes: usize,
    proof: Option<&NativeOriginalAcquisitionProof>,
) -> Result<ResultData, String> {
    let Prepared {
        credit: p,
        policy: prepared_policy,
        admission,
    } = prepare(
        r,
        reserved_work,
        reserved_bytes,
        CreditMeasure::Physical,
        stops,
    )?;
    let queries = admission.maximum_queries;
    let original = project_source(r.source.original.clone(), r.factor_policy.clone(), 0)?;
    let current = project_source(r.source.current.clone(), r.factor_policy.clone(), queries)?;
    let draw_sources = if proof.is_some() {
        project_draws_for_acquisition(r, CreditMeasure::Physical, queries)?
    } else {
        project_draws(r, CreditMeasure::Physical, queries)?
    };
    let quote = if let Some(proof) = proof {
        proof.quote_for(r)?
    } else {
        source_income::original_cash_quote(&r.source, original.source())?
    };
    // Both native full sources exist once. Same Core CTMC loop owns all earning,
    // state changes, G/EIR, exact settlements and default gross recoveries.
    let mut adapter = SourceAdapter::new(
        &current,
        r.credit.paths.iter().map(|p| p.path_key).collect(),
        stops,
        r.prepayment_terms.as_ref(),
        r.maturity_basis_policy.as_ref(),
    )?
    .with_draws(&draw_sources, &prepared_policy, &p);
    let carry = carry::evaluate_with_native_factors(&p, &mut adapter, &mut check)?;
    if carry.carry_points().len() > admission.carry_points_upper
        || carry.admission().work_upper > admission.carry.work_upper
        || carry.admission().retained_bytes_upper > admission.carry.retained_bytes_upper
    {
        return Err("observed native carry exceeds admitted complete source upper".into());
    }
    let mut accounting = book::compile(&carry, &prepared_policy, &mut check)?;
    if let Some(actions) = &r.reviewed_credit_actions {
        let end = r
            .source
            .current
            .asof_ordinal
            .checked_add((admission.full_credit_end_year * 365.).ceil() as i32)
            .ok_or("review horizon overflow")?;
        book::reviews::apply(
            &mut accounting,
            &carry,
            actions,
            r.source.current.asof_ordinal,
            end,
            &r.source.current.currency,
        )?;
    }
    check()?;
    let result = ResultData {
        schema: SCHEMA,
        raw_request: r.clone(),
        admission,
        original_source: original,
        current_source: current,
        original_cash_quote: quote,
        prepared_credit_request: p,
        carry,
        accounting,
        original_anchor_refitted: false,
        prospective_accounting_rate_policy: r.source.policy.reset_income
            == "reviewed_prospective_at_period_start-1",
        mixed_ledger_adoption: false,
        empirical_adoption: false,
        accounting_approval: false,
        native_stops: stops.to_vec(),
        draw_sources,
    };
    check()?;
    Ok(result)
}

#[cfg(test)]
thread_local! {static SOURCE_CALLS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};}
fn project_source(
    raw: crate::products::term::contracts::indexed::ContractPathRequest,
    policy: FactorPolicy,
    queries: usize,
) -> Result<NativeFactors, String> {
    #[cfg(test)]
    SOURCE_CALLS.with(|n| n.set(n.get() + 1));
    NativeFactors::compile(raw, policy, queries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn reviewed_maturity(raw: &mut Request) {
        raw.maturity_basis_policy = Some(MaturityBasisPolicy {
            review: term::Identity {
                id: "terminal-basis-review".into(),
                revision: "1".into(),
            },
            disposition: "dispose_financial_basis_after_exact_legal_close-1".into(),
        });
    }
    fn projected_coupon(raw: &mut Request, original: bool, rate: f64) {
        let source = if original {
            &mut raw.source.original
        } else {
            &mut raw.source.current
        };
        let period = &mut source.periods[0];
        let cutoff = i64::from(period.start_ordinal - 719163) * 86400;
        period.coupon = serde_json::from_value(json!({
            "kind":"indexed","index":{"index_id":"supplied-term","kind":"term","tenor_months":1,"average_calendar_days":null},
            "calendar":{"id":"supplied-calendar","revision":"1"},"complete_calendar_mapping":true,
            "calculation":"term_simple","margin_treatment":"simple","margin":0.,
            "index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},
            "reset_cutoff_unix_seconds":cutoff,"lookback_business_days":0,"observation_shift":false,
            "observations":[{"interest_start_ordinal":period.start_ordinal,"interest_end_ordinal":period.end_ordinal,
                "observation_start_ordinal":period.start_ordinal,"observation_end_ordinal":period.end_ordinal,
                "fixing_ordinal":period.start_ordinal,"selection_cutoff_unix_seconds":cutoff,"rate":rate,
                "source":{"kind":"conditional_projection","model":{"id":"supplied-projection","revision":"1"},"path":source.path,
                    "information_unix_seconds":cutoff,"measure":"physical"}}]
        })).unwrap();
    }
    #[test]
    fn reviewed_complete_tail_disposes_changed_current_coupon_basis_without_cash() {
        let mut raw = request(35);
        projected_coupon(&mut raw, true, 0.1);
        projected_coupon(&mut raw, false, 0.);
        let retained = evaluate(&raw).unwrap();
        let expected = raw.source.acquisition.cost - 100.;
        assert_eq!(retained.accounting().closing()[0].gross_carry, expected);
        reviewed_maturity(&mut raw);
        let result = evaluate(&raw).unwrap();
        let final_group = result
            .accounting()
            .groups()
            .iter()
            .find(|g| g.phase == "full_live_exit")
            .unwrap();
        assert_eq!(final_group.phase, "full_live_exit");
        assert_eq!(
            final_group.source_event_id.as_deref(),
            Some("native-maturity-basis:terminal-basis-review")
        );
        assert_eq!(final_group.cash, 0.);
        assert_eq!(final_group.basis_realization, expected);
        assert_eq!(final_group.next.principal, 0.);
        assert_eq!(final_group.next.legal_interest, 0.);
        assert_eq!(final_group.next.gross_carry, 0.);
        assert_eq!(final_group.next.book_adjustment, 0.);
        assert_eq!(result.original_cash_quote, raw.source.acquisition.cost);
        assert!(!result.original_anchor_refitted);
    }
    #[test]
    fn reviewed_terminal_basis_closes_nonaccrual_deferred_recognized_right_once() {
        let mut raw = request(1);
        for source in [&mut raw.source.original, &mut raw.source.current] {
            source.periods[0].end_ordinal = source.asof_ordinal + 1;
            source.tail_end_ordinal = source.asof_ordinal + 1;
            source.reporting_end_ordinal = source.asof_ordinal + 1;
        }
        raw.source.acquisition.cost = 100. + 100. * 0.1 / 365.;
        raw.credit.intervals[0].end_year = 1. / 365.;
        raw.accounting.recognition[0].by_origin = [book::Recognition::LegalOnlyIncomeStopped; 3];
        let retained = evaluate(&raw).unwrap();
        assert_eq!(retained.accounting().closing()[0].legal_interest, 0.);
        assert!(retained.accounting().closing()[0].recognized_interest < 0.);
        reviewed_maturity(&mut raw);
        let result = evaluate(&raw).unwrap();
        let closing = &result.accounting().closing()[0];
        assert_eq!(closing.principal, 0.);
        assert_eq!(closing.legal_interest, 0.);
        assert_eq!(closing.recognized_interest, 0.);
        assert_eq!(closing.book_adjustment, 0.);
        assert_eq!(closing.gross_carry, 0.);
        assert_eq!(result.accounting().groups().last().unwrap().cash, 0.);
        assert!(result
            .accounting()
            .groups()
            .iter()
            .all(|g| g.effective_interest == 0.));
    }
    #[test]
    fn reviewed_maturity_never_closes_default_claims_or_invents_terminal_principal() {
        let mut raw = request(35);
        reviewed_maturity(&mut raw);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        raw.credit.intervals[0].complete_recovery_tails = std::array::from_fn(|_| {
            vec![path::RecoveryWeight {
                lag_years: 40. / 365.,
                weight: 1.,
            }]
        });
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        let result = evaluate(&raw).unwrap();
        assert!(result.accounting().closing()[0].in_default);
        assert!(result.accounting().closing()[0].principal > 0.);
        assert!(result
            .accounting()
            .groups()
            .iter()
            .all(|g| g.phase != "full_live_exit"));
        let mut unpaid = request(35);
        for source in [&mut unpaid.source.original, &mut unpaid.source.current] {
            source.principal_plan =
                serde_json::from_value(json!({"kind":"supplied","principal_payments":[60.]}))
                    .unwrap();
        }
        unpaid.source.acquisition.cost = 60. + 100. * 0.1 * 30. / 365.;
        reviewed_maturity(&mut unpaid);
        let error = evaluate(&unpaid).unwrap_err();
        assert!(
            error.contains("exact complete-tail legal")
                || error == "supplied principal does not conserve complete balance",
            "{error}"
        );
    }
    #[test]
    fn reviewed_maturity_rejects_even_tiny_unresolved_actual_legal_state() {
        use carry::NativeEarnedFactors;
        let mut raw = request(35);
        reviewed_maturity(&mut raw);
        let result = evaluate(&request(35)).unwrap();
        let adapter = SourceAdapter::new(
            result.current_source(),
            vec![17],
            &[],
            None,
            raw.maturity_basis_policy.as_ref(),
        )
        .unwrap();
        let event_id = maturity_event_id(raw.maturity_basis_policy.as_ref().unwrap());
        let q = carry::NativeExitQuery {
            path_key: 17,
            event_id: &event_id,
            year: 35. / 365.,
            principal: 0.,
            legal_interest: f64::EPSILON,
            is_prepayment: false,
        };
        assert!(adapter
            .exit_cash(&q)
            .err()
            .unwrap()
            .contains("exact complete-tail legal"));
    }
    #[test]
    fn canonical_native_cumulative_coupons_close_without_daily_money_residuals() {
        use crate::products::term::contracts::indexed::Coupon;
        for rate in [
            -0.217891,
            -0.013333333333,
            0.,
            0.017891,
            0.03123456789,
            0.0577777777,
            0.123456789,
            0.311112,
        ] {
            for indexed in [false, true] {
                let mut raw = request(35);
                if indexed {
                    projected_coupon(&mut raw, true, rate);
                    projected_coupon(&mut raw, false, rate);
                } else {
                    for s in [&mut raw.source.original, &mut raw.source.current] {
                        s.periods[0].coupon = Coupon::Fixed { annual_rate: rate };
                    }
                }
                raw.source.acquisition.cost = 100. + 100. * (rate * (30. / 365.));
                raw.credit.intervals[0].hazards_per_year[0][1] = 1.;
                raw.credit.draw_stream = path::DrawStream::Supplied {
                    stream_id: "observed-cure-partition".into(),
                    stream_revision: "1".into(),
                    paths: vec![path::SuppliedPathDraws {
                        path_key: 17,
                        pairs: vec![path::UniformPair {
                            waiting_uniform: (-5_f64 / 365.).exp(),
                            destination_uniform: 0.5,
                        }],
                    }],
                };
                reviewed_maturity(&mut raw);
                let result = evaluate(&raw).unwrap();
                let cash = result
                    .carry()
                    .carry_points()
                    .iter()
                    .find(|p| p.kind == "contractual_cash")
                    .unwrap();
                assert_eq!(
                    cash.contractual_accrued_interest, 0.,
                    "{rate} indexed={indexed}"
                );
                assert_eq!(cash.canonical_legal_coupons.len(), 1);
                assert_eq!(cash.canonical_legal_coupons[0].legal_remaining, 0.);
                let expected = 100. * cash.canonical_legal_coupons[0].cumulative_factor
                    - result.carry().native_source_cash()[0].cash.interest;
                assert_eq!(expected, 0.);
                assert_eq!(result.accounting().closing()[0].legal_interest, 0.);
                assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
            }
        }
    }
    #[test]
    fn original_stopped_physical_acquisition_proof_is_sealed_and_anchor_bound() {
        let mut raw = request(35);
        let stop = NativeStop::from_compiled_policy(
            17,
            "native-call-original".into(),
            raw.source.original.asof_ordinal + 10,
            NativeStopAction::Call,
            1.05,
            0.,
            term::Identity {
                id: "original-call-policy".into(),
                revision: "1".into(),
            },
        )
        .unwrap();
        raw.source.acquisition.cost = 105. + 100. * (0.1 * (10. / 365.));
        assert!(evaluate(&raw)
            .unwrap_err()
            .contains("original complete actual-cash quote"));
        let prefix = evaluate_physical_prefix_for_acquisition(&raw).unwrap();
        assert!(!prefix.physical_journal_eligible);
        let proof =
            NativeOriginalAcquisitionProof::compile(&[(&raw, 1., std::slice::from_ref(&stop))])
                .unwrap();
        let result =
            evaluate_with_native_original_acquisition(&raw, std::slice::from_ref(&stop), &proof)
                .unwrap();
        assert_eq!(
            result.carry().credit().result.paths[0].outcome,
            path::PathOutcome::Matured
        );
        assert!(!result.original_anchor_refitted);
        let mut changed = raw.clone();
        changed.source.acquisition.cost += 1.;
        assert!(evaluate_with_native_original_acquisition(
            &changed,
            std::slice::from_ref(&stop),
            &proof
        )
        .unwrap_err()
        .contains("immutable original acquired"));
        let mut substituted = raw.clone();
        projected_coupon(&mut substituted, false, 0.);
        assert!(NativeOriginalAcquisitionProof::compile(&[(
            &substituted,
            1.,
            std::slice::from_ref(&stop)
        )])
        .unwrap_err()
        .contains("complete original Physical"));
    }
    fn add_draw(raw: &mut Request, draw_day: i32, end_day: i32, amount: f64, annual: f64) -> f64 {
        use crate::products::term::contracts::indexed::Coupon;
        let mut source = raw.source.clone();
        let origin = raw.source.current.asof_ordinal;
        for s in [&mut source.original, &mut source.current] {
            s.contract.id = "draw-contract".into();
            s.position_id = "draw-contract".into();
            s.asof_ordinal = origin + draw_day;
            s.reporting_end_ordinal = origin + end_day;
            s.tail_end_ordinal = origin + end_day;
            s.opening_principal = amount;
            s.principal_plan =
                serde_json::from_value(json!({"kind":"supplied","principal_payments":[amount]}))
                    .unwrap();
            s.periods[0].period_id = "draw-coupon".into();
            s.periods[0].start_ordinal = origin + draw_day;
            s.periods[0].end_ordinal = origin + end_day;
            s.periods[0].payment_ordinal = origin + end_day;
            s.periods[0].coupon = Coupon::Fixed { annual_rate: 0.15 };
        }
        let years = f64::from(end_day - draw_day) / 365.;
        let cost = (amount + amount * (0.15 * years)) * (-annual.ln_1p() * years).exp();
        source.acquisition.identity.id = "draw-acquisition".into();
        source.acquisition.original_yield_anchor.id = "draw-original-yield".into();
        source.acquisition.cost = cost;
        source.acquisition.original_annual_effective_rate = annual;
        raw.performing_draw_tranches.push(PerformingDrawTranche {
            review: term::Identity {
                id: "draw-review".into(),
                revision: "1".into(),
            },
            draw_ordinal: origin + draw_day,
            eligible_origins: [true, false, false],
            source,
        });
        raw.credit.opening_undrawn_commitment = 40.;
        raw.credit.intervals[0].end_year = f64::from(end_day.max(35)) / 365.;
        cost
    }
    #[test]
    fn discretionary_draw_owns_nominal_cost_original_eir_and_complete_coupon_tail() {
        let mut raw = request(35);
        let cost = add_draw(&mut raw, 10, 50, 20., 0.07);
        reviewed_maturity(&mut raw);
        let result = evaluate(&raw).unwrap();
        let credit = &result.carry().credit().result.paths[0];
        let event = credit.events.iter().find(|e| e.kind == "draw").unwrap();
        assert_eq!(event.principal, 20.);
        assert_eq!(event.cash, -cost);
        assert_eq!(credit.cash_funded, cost);
        let funded = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "funded_draw")
            .unwrap();
        assert_eq!(funded.principal, 120.);
        assert_eq!(funded.undrawn_commitment, Some(20.));
        assert_eq!(funded.draw_tranches[0].gross_carry, cost);
        let next = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "live_sojourn" && p.year > 10. / 365.)
            .unwrap();
        let hand = cost * (0.07_f64.ln_1p() * (next.year - 10. / 365.)).exp_m1();
        assert!((next.effective_interest - hand).abs() < 1e-12);
        assert_eq!(next.draw_tranches[0].original_annual_effective_rate, 0.07);
        assert!(result
            .carry()
            .carry_points()
            .iter()
            .filter(|p| p.year < 10. / 365.)
            .all(|p| p.draw_tranches.is_empty() && p.canonical_legal_coupons.len() <= 1));
        assert_eq!(result.carry().native_source_cash().len(), 2);
        let drawcash = result
            .carry()
            .native_source_cash()
            .iter()
            .find(|r| r.cash.tranche_id.as_deref() == Some("draw-review"))
            .unwrap();
        assert_eq!(drawcash.year, 50. / 365.);
        assert_eq!(drawcash.cash.principal, 20.);
        assert_eq!(drawcash.cash.native_cash_index, 1);
        assert_eq!(result.accounting().closing()[0].principal, 0.);
        assert_eq!(result.accounting().closing()[0].legal_interest, 0.);
        assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
        assert_eq!(result.draw_sources[0].terms.source.acquisition.cost, cost);
    }
    #[test]
    fn inactive_default_prior_draw_and_nominal_overspend_never_fund_or_generate_tranche_rights() {
        let mut raw = request(35);
        add_draw(&mut raw, 10, 50, 20., 0.07);
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        let result = evaluate(&raw).unwrap();
        assert_eq!(result.carry().credit().result.paths[0].cash_funded, 0.);
        assert!(result
            .carry()
            .earned_origin_contributions()
            .iter()
            .all(|c| c.source_id != "draw-contract"));
        assert!(result
            .carry()
            .native_source_cash()
            .iter()
            .all(|c| c.cash.tranche_id.is_none()));
        let mut watch = request(35);
        add_draw(&mut watch, 10, 50, 20., 0.07);
        watch.credit.paths[0].initial_live_state = CreditState::Watch;
        let result = evaluate(&watch).unwrap();
        assert_eq!(result.carry().credit().result.paths[0].cash_funded, 0.);
        assert_eq!(result.carry().native_source_cash().len(), 1);
        let mut excess = request(35);
        add_draw(&mut excess, 10, 50, 41., 0.07);
        SOURCE_CALLS.with(|n| n.set(0));
        assert!(evaluate(&excess).unwrap_err().contains("nominal exceeds"));
        SOURCE_CALLS.with(|n| assert_eq!(n.get(), 0));
    }
    #[test]
    fn post_draw_default_transfers_only_active_actual_ead_and_stop_retires_all_future_coupon_cash()
    {
        let mut raw = request(35);
        let cost = add_draw(&mut raw, 10, 50, 20., 0.07);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        default_at(&mut raw, CreditState::Performing, 0, 15.);
        let result = evaluate(&raw).unwrap();
        let path = &result.carry().credit().result.paths[0];
        let claim = path.default.as_ref().unwrap();
        assert_eq!(path.cash_funded, cost);
        assert_eq!(claim.principal, 120.);
        assert!(result
            .carry()
            .earned_origin_contributions()
            .iter()
            .any(|c| c.source_id == "draw-contract"));
        assert!(result.carry().native_source_cash().is_empty());
        assert!(
            (claim.accrued_interest - (100. * 0.1 * 15. / 365. + 20. * 0.15 * 5. / 365.)).abs()
                < 1e-10
        );
        assert!(result.accounting().closing()[0].in_default);
        let mut performing = request(35);
        add_draw(&mut performing, 10, 50, 20., 0.07);
        let stop = NativeStop::from_compiled_policy(
            17,
            "stop-after-draw".into(),
            performing.source.current.asof_ordinal + 15,
            NativeStopAction::Call,
            1.,
            0.,
            term::Identity {
                id: "draw-exit-review".into(),
                revision: "1".into(),
            },
        )
        .unwrap();
        let result = evaluate_with_stops(&performing, &[stop]).unwrap();
        assert!(result.carry().native_source_cash().is_empty());
        assert_eq!(
            result.carry().credit().result.paths[0]
                .events
                .last()
                .unwrap()
                .principal,
            120.
        );
        assert_eq!(result.accounting().closing()[0].principal, 0.);
        assert_eq!(result.accounting().closing()[0].legal_interest, 0.);
        assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
    }
    #[test]
    fn drawn_tranche_nonaccrual_review_stops_own_eir_without_erasing_legal_coupon() {
        let mut raw = request(35);
        add_draw(&mut raw, 10, 50, 20., 0.07);
        raw.accounting.recognition[0].by_origin = [book::Recognition::LegalOnlyIncomeStopped; 3];
        let result = evaluate(&raw).unwrap();
        assert!(result
            .carry()
            .carry_points()
            .iter()
            .all(|p| p.effective_interest == 0.));
        assert!(result
            .carry()
            .earned_origin_contributions()
            .iter()
            .any(|c| c.source_id == "draw-contract" && c.contractual_earned > 0.));
        let closing = &result.accounting().closing()[0];
        assert_eq!(closing.legal_interest, 0.);
        assert!(closing.recognized_interest < 0.);
        assert!(result
            .carry()
            .carry_points()
            .iter()
            .filter(|p| p.kind == "live_sojourn" && p.year > 10. / 365.)
            .all(|p| p.draw_tranches[0].gross_carry
                == result.draw_sources[0].terms.source.acquisition.cost));
    }
    #[test]
    fn drawn_tranche_prospective_reset_changes_current_income_retaining_original_yield_and_cost() {
        let mut raw = request(35);
        add_draw(&mut raw, 10, 50, 20., 0.07);
        let source = &mut raw.performing_draw_tranches[0].source;
        let origin = raw.source.current.asof_ordinal;
        for s in [&mut source.original, &mut source.current] {
            let mut second = s.periods[0].clone();
            s.periods[0].end_ordinal = origin + 20;
            s.periods[0].payment_ordinal = origin + 20;
            second.period_id = "draw-second-coupon".into();
            second.start_ordinal = origin + 20;
            s.periods.push(second);
            s.principal_plan =
                serde_json::from_value(json!({"kind":"supplied","principal_payments":[0.,20.]}))
                    .unwrap();
        }
        for s in [&mut source.original, &mut source.current] {
            for p in &mut s.periods {
                let cutoff = i64::from(p.start_ordinal - 719163) * 86400;
                p.coupon=serde_json::from_value(json!({"kind":"indexed","index":{"index_id":"draw-term","kind":"term","tenor_months":1,"average_calendar_days":null},
                "calendar":{"id":"draw-calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"term_simple","margin_treatment":"simple","margin":0.,
                "index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":cutoff,"lookback_business_days":0,"observation_shift":false,
                "observations":[{"interest_start_ordinal":p.start_ordinal,"interest_end_ordinal":p.end_ordinal,"observation_start_ordinal":p.start_ordinal,"observation_end_ordinal":p.end_ordinal,
                    "fixing_ordinal":p.start_ordinal,"selection_cutoff_unix_seconds":cutoff,"rate":0.15,
                    "source":{"kind":"conditional_projection","model":{"id":"draw-projection","revision":"1"},"path":s.path,"information_unix_seconds":cutoff,"measure":"physical"}}]})).unwrap();
            }
        }
        let first_coupon = 20. * (0.15 * (10. / 365.));
        let last_cash = 20. + 20. * (0.15 * (30. / 365.));
        let cost = first_coupon * (-0.07_f64.ln_1p() * (10. / 365.)).exp()
            + last_cash * (-0.07_f64.ln_1p() * (40. / 365.)).exp();
        source.acquisition.cost = cost;
        source.policy.reset_income = "reviewed_prospective_at_period_start-1".into();
        source.current_accounting_rate_reviews = vec![
            source_income::ProspectiveRateReview {
                review: term::Identity {
                    id: "draw-first-rate".into(),
                    revision: "1".into(),
                },
                period_id: "draw-coupon".into(),
                reset_ordinal: origin + 10,
                annual_effective_rate: 0.07,
            },
            source_income::ProspectiveRateReview {
                review: term::Identity {
                    id: "draw-second-rate".into(),
                    revision: "1".into(),
                },
                period_id: "draw-second-coupon".into(),
                reset_ordinal: origin + 20,
                annual_effective_rate: 0.11,
            },
        ];
        reviewed_maturity(&mut raw);
        let result = evaluate(&raw).unwrap();
        let after_first_cash = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "contractual_cash" && p.year == 20. / 365.)
            .unwrap();
        let after_reset = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "live_sojourn" && p.year > 20. / 365.)
            .unwrap();
        let hand = after_first_cash.draw_tranches[0].gross_carry
            * (0.11_f64.ln_1p() * (after_reset.year - after_first_cash.year)).exp_m1();
        assert!((after_reset.effective_interest - hand).abs() < 1e-12);
        assert_eq!(
            after_reset.draw_tranches[0].original_annual_effective_rate,
            0.07
        );
        assert_eq!(result.draw_sources[0].terms.source.acquisition.cost, cost);
        assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
    }
    fn request(payment_day: i32) -> Request {
        let source = json!({"schema":"term-enhancements-1","model_version":"term-contracts-1",
            "contract":{"id":"commercial","revision":"1"},"assumption":{"id":"research","revision":"1"},"path":{"id":"base","revision":"1"},
            "position_id":"commercial","scenario_id":"base","currency":"USD","monetary_unit":"currency_units","direction":"receipt","measure":"physical",
            "asof_ordinal":738886,"reporting_end_ordinal":738901,"tail_end_ordinal":738921,"observed_publication_cutoff_unix_seconds":1704067200_i64,
            "opening_principal":100.,"principal_plan":{"kind":"supplied","principal_payments":[100.]},
            "periods":[{"period_id":"coupon","start_ordinal":738886,"end_ordinal":738916,"payment_ordinal":738886+payment_day,"day_count":"act365_fixed","fee":0.,"coupon":{"kind":"fixed","annual_rate":0.1}}]});
        let quote = 100. + 100. * 0.1 * 30. / 365.;
        serde_json::from_value(json!({"schema":SCHEMA,
            "source":{"schema":source_income::SCHEMA,"original":source,"current":source,
                "acquisition":{"identity":{"id":"purchase","revision":"1"},"original_yield_anchor":{"id":"original-only","revision":"1"},"income_review":{"id":"income-review","revision":"1"},"cost":quote,"original_annual_effective_rate":0.},
                "policy":{"identity":{"id":"contract-review","revision":"1"},"recognition":"native_factor_checkpoint_rights-1","reset_income":"reviewed_frozen_original_reference-1","fees":"integral_cash-1"}},
            "factor_policy":{"review":{"id":"partial-clock","revision":"1"},"clock":"credit_act365_elapsed_from_source_asof-1","partial_calendar_day":"simple_partial_native_daycount-1","overnight":"pre_observation_accumulator_partial_weight-1","observation_mapping":"explicit_equal_calendar_span-1","full_checkpoints":"retain_actual_native_endpoints-1"},
            "credit":{"identity":{"id":"physical-review","revision":"1"},"max_jumps_per_path":4,
                "intervals":[{"end_year":35./365.,"hazards_per_year":vec![[0.;5];3],"loss_given_default":vec![0.;3],"credit_conversion_factor":vec![0.;3],"interest_recovery_fraction":vec![0.5;3],
                    "complete_recovery_tails":[[{"lag_years":0.,"weight":1.}],[{"lag_years":0.,"weight":1.}],[{"lag_years":0.,"weight":1.}]]}],
                "paths":[{"path_id":"borrower","path_key":17,"initial_live_state":"performing"}],
                "draw_stream":{"kind":"counter_splitmix64_v1","stream_id":"stable-crn","stream_revision":"1","seed":7}},
            "accounting":{"identity":{"id":"explicit-accounting","revision":"1"},"recognition":[{"interval_index":0,"review":{"id":"full-accrual","revision":"1"},"by_origin":vec!["full_accrual";3]}],
                "cash_application":"exact_native_coupon_cash_origin_fifo-1","unrecognized_cash":"defer_unrecognized_cash_until_reviewed_recognition-1",
                "exit_disposition":"collect_legal_balance_reverse_recognized_realize_remaining_carry-1","default_disposition":"transfer_all_gross_legal_and_recognized_claims_no_implicit_chargeoff-1"},
            "accounting_rates":[{"interval_index":0,"review":"original-yield","annual_effective_rates":vec![0.;3]}]})).unwrap()
    }
    #[test]
    fn actual_source_early_cash_retains_p0_coupon_entitlement_daily() {
        let result = evaluate(&request(10)).unwrap();
        assert!(result
            .carry()
            .credit()
            .sojourns
            .iter()
            .any(|s| s.principal == 0. && s.earned_interest > 0.));
        assert_eq!(result.carry().native_source_cash().len(), 1);
        assert_eq!(
            result.current_source().source().audit[0].opening_principal,
            100.
        );
        assert_eq!(result.carry().native_source_cash()[0].cash.principal, 100.);
        assert!(result
            .accounting()
            .groups()
            .iter()
            .any(|g| g.next.principal == 0. && g.next.legal_interest < 0.));
        assert!(
            result
                .accounting()
                .groups()
                .iter()
                .filter(|g| g.phase == "live_sojourn")
                .count()
                >= 35
        );
    }
    #[test]
    fn actual_source_delayed_coupon_and_original_quote_retained() {
        let result = evaluate(&request(35)).unwrap();
        assert_eq!(
            result.original_cash_quote,
            result.raw_request.source.acquisition.cost
        );
        assert!(result
            .accounting()
            .groups()
            .iter()
            .any(|g| g.year == 30. / 365. && g.next.legal_interest > 0.));
        assert_eq!(result.carry().native_source_cash()[0].year, 35. / 365.);
        assert_eq!(
            result.original_source.source().schedule.events[0]
                .components
                .cash_interest
                .to_bits(),
            result.current_source().source().schedule.events[0]
                .components
                .cash_interest
                .to_bits()
        );
    }
    #[test]
    fn credit_free_contingent_funding_and_incompatible_anchor_refuse_explicitly() {
        let mut raw = request(30);
        raw.credit.instrument_kind = InstrumentKind::CreditFreeReference;
        raw.credit.intervals[0].credit_conversion_factor[0] = 0.1;
        assert!(evaluate(&raw)
            .unwrap_err()
            .contains("credit-free reference"));
        let mut raw = request(30);
        raw.accounting_rates[0].annual_effective_rates[0] = 0.01;
        assert!(evaluate(&raw)
            .unwrap_err()
            .contains("accounting rate must bind"));
    }
    #[test]
    fn policies_budget_and_cancel_refuse_before_either_native_source() {
        SOURCE_CALLS.with(|n| n.set(0));
        let mut raw = request(30);
        raw.accounting.unrecognized_cash = "infer_nonaccrual".into();
        assert!(evaluate(&raw).unwrap_err().contains("unsupported explicit"));
        assert_eq!(SOURCE_CALLS.with(|n| n.get()), 0);
        let mut raw = request(30);
        raw.credit.paths = (0..256)
            .map(|i| PathIdentity {
                path_id: format!("p{i}"),
                path_key: i,
                initial_live_state: CreditState::Performing,
            })
            .collect();
        assert!(evaluate(&raw).is_err());
        assert_eq!(SOURCE_CALLS.with(|n| n.get()), 0);
        let control = portfolio_compute_control::Control::new(30000).unwrap();
        control.cancel();
        assert!(
            portfolio_compute_control::scope(Some(control), || evaluate(&request(30)))
                .unwrap_err()
                .contains("CANCELLED")
        );
        assert_eq!(SOURCE_CALLS.with(|n| n.get()), 0);
    }
    #[test]
    fn actual_default_retains_per_coupon_legal_and_recognized_tail_no_el_chargeoff() {
        SOURCE_CALLS.with(|n| n.set(0));
        let mut raw = request(35);
        raw.credit.intervals[0].hazards_per_year[0][3] = 1.;
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        raw.credit.draw_stream = path::DrawStream::Supplied {
            stream_id: "default-crn".into(),
            stream_revision: "1".into(),
            paths: vec![path::SuppliedPathDraws {
                path_key: 17,
                pairs: vec![path::UniformPair {
                    waiting_uniform: (-5_f64 / 365.).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        };
        let result = evaluate(&raw).unwrap();
        assert_eq!(SOURCE_CALLS.with(|n| n.get()), 2);
        let closing = &result.accounting().closing()[0];
        assert!((closing.principal - 40.).abs() < 1e-10);
        assert!(
            closing.in_default && closing.legal_interest > 0. && closing.recognized_interest > 0.
        );
        assert_eq!(result.carry().native_coupon_recoveries().len(), 1);
        assert!(result.carry().native_source_cash().is_empty());
        assert!(result.accounting().groups().iter().all(|g| g
            .postings
            .iter()
            .all(|p| p.posting.account
                != portfolio_model_core::credit_accounting::AccountCode::CreditLossExpense)));
    }
    #[test]
    fn actual_native_net_zero_cash_retains_signed_gross_components_and_open_principal() {
        use crate::products::term::contracts::indexed::{Coupon, PrincipalPlan};
        let mut raw = request(30);
        let principal = -(100. * (-0.1 * (30. / 365.)));
        for source in [&mut raw.source.original, &mut raw.source.current] {
            source.periods[0].coupon = Coupon::Fixed { annual_rate: -0.1 };
            let mut terminal = source.periods[0].clone();
            terminal.period_id = "terminal-principal".into();
            terminal.start_ordinal = source.periods[0].end_ordinal;
            terminal.end_ordinal = source.tail_end_ordinal;
            terminal.payment_ordinal = source.tail_end_ordinal;
            terminal.coupon = Coupon::Fixed { annual_rate: 0. };
            source.periods.push(terminal);
            source.principal_plan = PrincipalPlan::Supplied {
                principal_payments: vec![principal, 100. - principal],
            };
        }
        raw.source.acquisition.cost = 100. - principal;
        let result = evaluate(&raw).unwrap();
        let cash = &result.carry().native_source_cash()[0].cash;
        assert!(cash.principal > 0. && cash.interest < 0.);
        assert_eq!(cash.principal + cash.interest, 0.);
        assert_eq!(cash.source_event_id, "coupon");
        assert_eq!(result.accounting().closing()[0].principal, 0.);
        assert!(result
            .accounting()
            .groups()
            .iter()
            .any(|g| g.phase == "contractual_cash"
                && g.cash == 0.
                && g.next.principal > 0.
                && !g.postings.is_empty()));
    }

    fn default_at(raw: &mut Request, state: CreditState, origin: usize, day: f64) {
        raw.credit.paths[0].initial_live_state = state;
        raw.credit.intervals[0].hazards_per_year[origin][3] = 1.;
        raw.credit.draw_stream = path::DrawStream::Supplied {
            stream_id: "reviewed-default-crn".into(),
            stream_revision: "1".into(),
            paths: vec![path::SuppliedPathDraws {
                path_key: 17,
                pairs: vec![path::UniformPair {
                    waiting_uniform: (-day / 365.).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        };
    }
    #[test]
    fn contingent_conversion_funds_actual_origin_ead_once_before_default_and_recovery() {
        use portfolio_model_core::credit_accounting::AccountCode;
        for (origin, state) in [
            CreditState::Performing,
            CreditState::Watch,
            CreditState::Delinquent,
        ]
        .into_iter()
        .enumerate()
        {
            let mut raw = request(35);
            raw.credit.opening_undrawn_commitment = 20.;
            raw.credit.intervals[0].credit_conversion_factor = [0.25, 0.5, 0.75];
            raw.credit.intervals[0].loss_given_default = [0.4; 3];
            default_at(&mut raw, state, origin, 5.);
            let result = evaluate(&raw).unwrap();
            let credit = &result.carry().credit().result.paths[0];
            let funded = 20. * raw.credit.intervals[0].credit_conversion_factor[origin];
            let claim = credit.default.as_ref().unwrap();
            assert!((claim.principal - (100. + funded)).abs() < 1e-10);
            assert_eq!(claim.contingent_draw, funded);
            assert_eq!(credit.cash_funded, funded);
            assert!(
                (result.accounting().closing()[0].principal - claim.principal * 0.4).abs() < 1e-10
            );
            let groups = result.accounting().groups();
            let draw_index = groups
                .iter()
                .position(|g| g.phase == "funded_draw")
                .unwrap();
            assert_eq!(groups[draw_index].cash, -funded);
            assert_eq!(groups[draw_index + 1].phase, "default_gross_transfer");
            assert_eq!(
                groups[draw_index]
                    .postings
                    .iter()
                    .filter(|p| p.posting.account == AccountCode::Cash)
                    .count(),
                1
            );
            assert!(groups.iter().all(|g| g
                .postings
                .iter()
                .all(|p| p.posting.account != AccountCode::CreditLossExpense)));
        }
    }
    #[test]
    fn recovery_cash_beyond_actual_contractual_tail_is_applied_once_and_legal_loss_survives() {
        let mut raw = request(35);
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        for tail in &mut raw.credit.intervals[0].complete_recovery_tails {
            tail[0].lag_years = 40. / 365.;
        }
        let result = evaluate(&raw).unwrap();
        assert_eq!(result.full_credit_end_year(), 35. / 365. + 40. / 365.);
        let path = &result.carry().credit().result.paths[0];
        let recovery = &path.default.as_ref().unwrap().recoveries[0];
        assert!(recovery.year > 35. / 365.);
        let cash: Vec<_> = result
            .accounting()
            .groups()
            .iter()
            .filter(|g| g.phase == "default_actual_cash")
            .collect();
        assert_eq!(cash.len(), 1);
        assert_eq!(cash[0].cash, recovery.principal + recovery.interest);
        assert!((result.accounting().closing()[0].principal - 40.).abs() < 1e-10);
        assert!((path.cash_received - recovery.principal - recovery.interest).abs() < 1e-10);
        assert!(result
            .carry()
            .native_coupon_recoveries()
            .iter()
            .all(|r| r.applied_in_horizon));
    }
    #[test]
    fn integral_native_fees_are_distinct_cash_and_original_yield_basis_not_legal_coupon() {
        let mut raw = request(30);
        raw.source.original.periods[0].fee = 5.;
        raw.source.current.periods[0].fee = 5.;
        raw.source.acquisition.cost += 5.;
        let result = evaluate(&raw).unwrap();
        let cash = &result.carry().native_source_cash()[0].cash;
        assert_eq!(cash.fee, 5.);
        assert!((cash.interest - 100. * 0.1 * 30. / 365.).abs() < 1e-12);
        assert_eq!(result.original_cash_quote, raw.source.acquisition.cost);
        let group = result
            .accounting()
            .groups()
            .iter()
            .find(|g| g.phase == "contractual_cash")
            .unwrap();
        assert_eq!(group.cash, cash.principal + cash.interest + cash.fee);
        assert_eq!(group.effective_interest, 0.);
        assert!((result.accounting().closing()[0].gross_carry).abs() < 1e-10);
        assert!(result.accounting().closing()[0].legal_interest.abs() < 1e-10);
        let mut rejected = raw;
        rejected.source.policy.fees = "nonintegral_cash-1".into();
        assert!(evaluate(&rejected).unwrap_err().contains("nonintegral fee"));
    }
    #[test]
    fn issuer_credit_and_credit_free_fixtures_are_explicit_and_pricing_is_not_accounting() {
        let mut raw = request(35);
        raw.credit.instrument_kind = InstrumentKind::IssuerCreditDebt;
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        assert!(evaluate(&raw).unwrap().carry().credit().result.paths[0]
            .default
            .is_some());
        raw.credit.instrument_kind = InstrumentKind::CreditFreeReference;
        assert!(evaluate(&raw).unwrap_err().contains("credit-free"));
        let mut q = request(30);
        q.source.original.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
        q.source.current.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
        let economic = evaluate_economic_with_measure(&q, CreditMeasure::Pricing).unwrap();
        assert!(!economic.physical_journal_eligible);
        assert_eq!(
            economic.carry().credit().result.measure,
            CreditMeasure::Pricing
        );
        assert!(evaluate(&q).is_err());
        assert!(book::compile(economic.carry(), &q.accounting, &mut check)
            .unwrap_err()
            .contains("Physical"));
    }

    #[test]
    fn reviewed_prepayment_premium_is_cash_and_signed_realization_once_on_shared_draws() {
        let mut raw = request(35);
        raw.credit.intervals[0].hazards_per_year[0][4] = 1.;
        raw.credit.draw_stream = path::DrawStream::Supplied {
            stream_id: "prepay-crn".into(),
            stream_revision: "1".into(),
            paths: vec![path::SuppliedPathDraws {
                path_key: 17,
                pairs: vec![path::UniformPair {
                    waiting_uniform: (-5_f64 / 365.).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        };
        let zero = evaluate(&raw).unwrap();
        raw.prepayment_terms = Some(PrepaymentTerms {
            review: term::Identity {
                id: "reviewed-premium".into(),
                revision: "1".into(),
            },
            principal_premium_fraction: 0.02,
        });
        let paid = evaluate(&raw).unwrap();
        let path = &paid.carry().credit().result.paths[0];
        let zero_path = &zero.carry().credit().result.paths[0];
        let event = &path.events[0];
        assert_eq!(event.year, zero_path.events[0].year);
        assert_eq!(path.random_pairs_used, zero_path.random_pairs_used);
        assert!((event.cash - (100. + 100. * 0.1 * event.year + 2.)).abs() < 1e-10);
        assert!((event.cash - zero_path.events[0].cash - 2.).abs() < 1e-10);
        assert!(
            (event.book_adjustment_recognition - (raw.source.acquisition.cost - event.cash)).abs()
                < 1e-10
        );
        assert_eq!(
            path.book_adjustment_recognized,
            event.book_adjustment_recognition
        );
        assert!(paid.carry().native_source_cash().is_empty());
        assert_eq!(paid.accounting().closing()[0].gross_carry, 0.);
        assert!(paid.accounting().closing()[0]
            .lots
            .iter()
            .all(|l| l.legal_remaining == 0. && l.recognized_remaining == 0.));
    }
    fn stop(path_key: u64, day: i32) -> NativeStop {
        NativeStop::from_compiled_policy(
            path_key,
            format!("native-call-{path_key}"),
            738886 + day,
            NativeStopAction::Call,
            1.02,
            1.,
            term::Identity {
                id: "compiled-exercise-review".into(),
                revision: "1".into(),
            },
        )
        .unwrap()
    }
    #[test]
    fn compiled_native_stop_is_exact_path_specific_cash_and_cancels_only_its_future_source() {
        let mut raw = request(35);
        raw.credit.paths.push(PathIdentity {
            path_id: "other-borrower".into(),
            path_key: 18,
            initial_live_state: CreditState::Performing,
        });
        let selected = stop(17, 15);
        let result = evaluate_with_stops(&raw, &[selected]).unwrap();
        let stopped = &result.carry().credit().result.paths[0];
        let other = &result.carry().credit().result.paths[1];
        assert_eq!(stopped.events.len(), 1);
        assert_eq!(stopped.events[0].event_id, "native-call-17");
        assert_eq!(stopped.events[0].year, 15. / 365.);
        assert!((stopped.events[0].cash - (102. + 100. * 0.1 * 15. / 365. + 1.)).abs() < 1e-10);
        assert_eq!(other.events.len(), 1);
        assert_eq!(other.events[0].event_id, "coupon");
        assert_eq!(result.carry().native_source_cash().len(), 1);
        assert_eq!(result.carry().native_source_cash()[0].path_key, 18);
        assert_eq!(
            result
                .accounting()
                .groups()
                .iter()
                .filter(|g| g.phase == "full_live_exit")
                .count(),
            1
        );
        assert_eq!(
            result
                .accounting()
                .groups()
                .iter()
                .find(|g| g.phase == "full_live_exit")
                .unwrap()
                .source_event_id
                .as_deref(),
            Some("native-call-17")
        );
    }
    #[test]
    fn native_stop_after_exact_same_day_cash_uses_remaining_principal_and_signed_legal_right() {
        use crate::products::term::contracts::indexed::PrincipalPlan;
        let mut raw = request(15);
        for source in [&mut raw.source.original, &mut raw.source.current] {
            let mut terminal = source.periods[0].clone();
            terminal.period_id = "remaining-principal".into();
            terminal.start_ordinal = source.periods[0].end_ordinal;
            terminal.end_ordinal = source.tail_end_ordinal;
            terminal.payment_ordinal = source.tail_end_ordinal;
            source.periods.push(terminal);
            source.principal_plan = PrincipalPlan::Supplied {
                principal_payments: vec![40., 60.],
            };
        }
        raw.source.acquisition.cost = 100. + 100. * 0.1 * 30. / 365. + 60. * 0.1 * 5. / 365.;
        let result = evaluate_with_stops(&raw, &[stop(17, 15)]).unwrap();
        let path = &result.carry().credit().result.paths[0];
        assert_eq!(path.events.len(), 2);
        assert_eq!(path.events[0].event_id, "coupon");
        assert_eq!(path.events[1].event_id, "native-call-17");
        let legal = 100. * 0.1 * 15. / 365. - 100. * 0.1 * 30. / 365.;
        assert!(legal < 0.);
        assert!((path.events[1].cash - (60. * 1.02 + legal + 1.)).abs() < 1e-10);
        assert_eq!(path.closing_principal, 0.);
        assert_eq!(path.closing_accrued_interest, 0.);
    }
    #[test]
    fn default_prior_to_native_exercise_keeps_complete_recovery_claim_not_stop_cash() {
        let mut raw = request(35);
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        for tail in &mut raw.credit.intervals[0].complete_recovery_tails {
            tail[0].lag_years = 40. / 365.;
        }
        let result = evaluate_with_stops(&raw, &[stop(17, 15)]).unwrap();
        let path = &result.carry().credit().result.paths[0];
        assert!(path.default.is_some());
        assert!(!path.events.iter().any(|e| e.event_id == "native-call-17"));
        assert!(!result
            .accounting()
            .groups()
            .iter()
            .any(|g| g.phase == "full_live_exit"));
        assert!(result
            .accounting()
            .groups()
            .iter()
            .any(|g| g.phase == "default_actual_cash" && g.year > 35. / 365.));
        assert!((result.accounting().closing()[0].principal - 40.).abs() < 1e-10);
    }
    #[test]
    fn reviewed_nonaccrual_then_cure_retains_legal_cash_and_separate_deferred_recognized_right() {
        let mut raw = request(35);
        raw.credit.paths[0].initial_live_state = CreditState::Delinquent;
        raw.accounting.recognition[0].by_origin[2] = book::Recognition::LegalOnlyIncomeStopped;
        raw.credit.intervals[0].hazards_per_year[2][0] = 1.;
        raw.credit.draw_stream = path::DrawStream::Supplied {
            stream_id: "reviewed-cure-crn".into(),
            stream_revision: "1".into(),
            paths: vec![path::SuppliedPathDraws {
                path_key: 17,
                pairs: vec![path::UniformPair {
                    waiting_uniform: (-5_f64 / 365.).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        };
        let result = evaluate(&raw).unwrap();
        let path = &result.carry().credit().result.paths[0];
        assert_eq!(path.events[0].destination, Some(CreditState::Performing));
        let stopped_years = path.events[0].year;
        let stopped_money = 100. * 0.1 * stopped_years;
        let before_cash = result
            .accounting()
            .groups()
            .iter()
            .rev()
            .find(|g| g.phase == "live_sojourn" && g.year <= 30. / 365.)
            .unwrap();
        assert!((before_cash.next.legal_interest - 100. * 0.1 * 30. / 365.).abs() < 1e-10);
        assert!(
            (before_cash.next.recognized_interest - (100. * 0.1 * 30. / 365. - stopped_money))
                .abs()
                < 1e-10
        );
        let closing = &result.accounting().closing()[0];
        assert!(closing.legal_interest.abs() < 1e-10);
        assert!((closing.recognized_interest + stopped_money).abs() < 1e-10);
        assert!((closing.book_adjustment - stopped_money).abs() < 1e-10);
        assert!(closing.gross_carry.abs() < 1e-10);
        // A legitimate zero original EIR is not a nonaccrual inference: only
        // the explicit origin review stopped recognition before the cure.
        assert!(result
            .accounting()
            .groups()
            .iter()
            .all(|g| g.effective_interest == 0.));
        assert_eq!(path.random_pairs_used, 1);
    }
    #[test]
    fn reviewed_allowance_chargeoff_and_legal_waiver_keep_economic_recovery_cash() {
        use portfolio_model_core::credit_accounting::AccountCode;
        let mut raw = request(35);
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        for tail in &mut raw.credit.intervals[0].complete_recovery_tails {
            tail[0].lag_years = 10. / 365.;
        }
        let plain = evaluate(&raw).unwrap();
        raw.reviewed_credit_actions=Some(serde_json::from_value(json!({
            "policy":{"identity":{"id":"reviewed-credit-book","revision":"1"},"accrued":"transfer_until_reviewed_reversal","basis":"transfer_until_reviewed_chargeoff","allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"},
            "allowance_reviews":[{"path_key":17,"review":{"review":{"id":"provision50","revision":"1"},"recognition_ordinal":738887,"phase":"before_credit","target_amount":50.}}],
            "chargeoffs":[{"path_key":17,"review":{"review":{"id":"charge80","revision":"1"},"claim_id":"commercial-default:17","recognition_ordinal":738893,"principal_amount":80.,"book_adjustment_amount":0.,"accrued_interest_reversal":0.}}],
            "legal_waivers":[{"path_key":17,"review":{"id":"legal10","revision":"1"},"recognition_ordinal":738894,"claim_id":"commercial-default:17","principal_amount":10.,"accrued_interest_amount":0.,"future_cash_disposition":"retain_supplied_voluntary_recovery_cash-1"}]
        })).unwrap());
        let reviewed = evaluate(&raw).unwrap();
        assert_eq!(
            serde_json::to_vec(plain.carry()).unwrap(),
            serde_json::to_vec(reviewed.carry()).unwrap()
        );
        let end = &reviewed.accounting().closing()[0];
        let credit = end.reviewed_credit_state.as_ref().unwrap();
        assert_eq!(end.principal, 0.);
        assert_eq!(credit.legal_principal, 30.);
        assert_eq!(credit.allowance, 0.);
        let expenses: f64 = reviewed
            .accounting()
            .groups()
            .iter()
            .flat_map(|g| &g.postings)
            .filter(|p| p.posting.account == AccountCode::CreditLossExpense)
            .map(|p| p.posting.amount)
            .sum();
        assert_eq!(expenses, 80.);
        let excess: f64 = reviewed
            .accounting()
            .groups()
            .iter()
            .flat_map(|g| &g.postings)
            .filter(|p| p.posting.account == AccountCode::RecoveryIncome)
            .map(|p| p.posting.amount)
            .sum();
        assert_eq!(excess, -40.);
        let cash: f64 = reviewed.accounting().groups().iter().map(|g| g.cash).sum();
        assert_eq!(
            cash,
            plain
                .accounting()
                .groups()
                .iter()
                .map(|g| g.cash)
                .sum::<f64>()
        );
        assert_eq!(credit.claims[0].recovery_cash_collected, 60.);
    }
    #[test]
    fn before_cash_native_stop_cancels_same_clock_scheduled_draw() {
        let mut raw = request(35);
        raw.credit.opening_undrawn_commitment = 20.;
        add_draw(&mut raw, 10, 50, 20., 0.07);
        let stop = NativeStop::from_compiled_policy_with_timing(
            17,
            "before-draw-stop".into(),
            raw.source.current.asof_ordinal + 10,
            NativeStopAction::Call,
            1.,
            0.,
            term::Identity {
                id: "native-call-review".into(),
                revision: "1".into(),
            },
            NativeStopTiming::BeforeScheduledCash,
            None,
        )
        .unwrap();
        let result = evaluate_with_stops(&raw, &[stop]).unwrap();
        assert!(result
            .carry()
            .carry_points()
            .iter()
            .all(|p| p.kind != "funded_draw"));
        assert_eq!(result.carry().credit().result.paths[0].cash_funded, 0.);
        assert!(result.carry().native_source_cash().is_empty());
        assert!(result
            .carry()
            .earned_origin_contributions()
            .iter()
            .all(|c| c.coupon_id != "draw-coupon"));
    }
    #[test]
    fn stopped_original_acquisition_draws_keep_conditional_own_cost_and_yield() {
        let mut raw = request(35);
        raw.credit.opening_undrawn_commitment = 20.;
        add_draw(&mut raw, 10, 50, 20., 0.07);
        let stop = NativeStop::from_compiled_policy(
            17,
            "acquired-call".into(),
            raw.source.current.asof_ordinal + 20,
            NativeStopAction::Call,
            1.,
            0.,
            term::Identity {
                id: "native-call-review".into(),
                revision: "1".into(),
            },
        )
        .unwrap();
        raw.source.acquisition.cost = 100. + 100. * 0.1 * (20. / 365.);
        let draw_cost =
            (20. + 20. * 0.15 * (10. / 365.)) * (-0.07_f64.ln_1p() * (10. / 365.)).exp();
        raw.performing_draw_tranches[0].source.acquisition.cost = draw_cost;
        let mut draw_raw = raw.clone();
        draw_raw.source = raw.performing_draw_tranches[0].source.clone();
        projected_coupon(&mut draw_raw, true, 0.15);
        projected_coupon(&mut draw_raw, false, 0.15);
        raw.performing_draw_tranches[0].source = draw_raw.source;
        assert!(evaluate_with_stops(&raw, std::slice::from_ref(&stop)).is_err());
        let proof =
            NativeOriginalAcquisitionProof::compile(&[(&raw, 1., std::slice::from_ref(&stop))])
                .unwrap();
        assert!(term::near(
            proof.original_stopped_cash_quote,
            raw.source.acquisition.cost
        ));
        assert_eq!(
            proof.conditional_tranche_proofs[0].activation_probability,
            1.
        );
        assert!(term::near(
            proof.conditional_tranche_proofs[0].conditional_cash_quote,
            draw_cost
        ));
        let acquired =
            evaluate_with_native_original_acquisition(&raw, std::slice::from_ref(&stop), &proof)
                .unwrap();
        assert!(acquired
            .carry()
            .carry_points()
            .iter()
            .any(|p| p.kind == "funded_draw" && p.cash.to_bits() == (-draw_cost).to_bits()));
        let mut changed = raw.clone();
        let mut draw_raw = changed.clone();
        draw_raw.source = changed.performing_draw_tranches[0].source.clone();
        projected_coupon(&mut draw_raw, false, 0.2);
        changed.performing_draw_tranches[0].source = draw_raw.source;
        evaluate_with_native_original_acquisition(&changed, std::slice::from_ref(&stop), &proof)
            .unwrap();
        changed.performing_draw_tranches[0].source.acquisition.cost += 0.1;
        assert!(
            evaluate_with_native_original_acquisition(&changed, &[stop], &proof)
                .unwrap_err()
                .contains("immutable original")
        );
    }
    fn empty_reviewed_actions(raw: &mut Request, reverse: bool, expense_basis: bool) {
        raw.reviewed_credit_actions=Some(serde_json::from_value(json!({"policy":{"identity":{"id":"book-review","revision":"1"},"accrued":if reverse{"reverse_income_at_default"}else{"transfer_until_reviewed_reversal"},"basis":if expense_basis{"recognize_in_earnings_at_default"}else{"transfer_until_reviewed_chargeoff"},"allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"}})).unwrap());
    }
    #[test]
    fn reviewed_nonaccrual_early_cash_preserves_signed_book_and_legal_default_claims() {
        for reverse in [false, true] {
            for expense in [false, true] {
                let mut raw = request(10);
                default_at(&mut raw, CreditState::Performing, 0, 20.);
                raw.accounting.recognition[0].by_origin =
                    [book::Recognition::LegalOnlyIncomeStopped; 3];
                empty_reviewed_actions(&mut raw, reverse, expense);
                let result = evaluate(&raw).unwrap();
                let end = &result.accounting().closing()[0];
                assert!(end.legal_interest < 0.);
                if reverse {
                    assert_eq!(end.recognized_interest, 0.);
                } else {
                    assert!(end.recognized_interest < end.legal_interest);
                }
                let state = end.reviewed_credit_state.as_ref().unwrap();
                assert_eq!(state.legal_principal, 0.);
                assert!(state
                    .claims
                    .iter()
                    .any(|c| c.claim_id.starts_with("commercial-legal-cash:")));
                assert_eq!(
                    result.carry().credit().result.paths[0]
                        .default
                        .as_ref()
                        .unwrap()
                        .principal,
                    0.
                );
            }
        }
    }
    #[test]
    fn reviewed_actual_recoveries_sharing_journal_day_keep_source_time_order() {
        let mut raw = request(35);
        default_at(&mut raw, CreditState::Performing, 0, 5.2);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        raw.credit.intervals[0].complete_recovery_tails = std::array::from_fn(|_| {
            vec![
                path::RecoveryWeight {
                    lag_years: 1. / 365.,
                    weight: 0.5,
                },
                path::RecoveryWeight {
                    lag_years: 1.1 / 365.,
                    weight: 0.5,
                },
            ]
        });
        empty_reviewed_actions(&mut raw, false, false);
        let result = evaluate(&raw).unwrap();
        let recoveries: Vec<_> = result
            .accounting()
            .groups()
            .iter()
            .filter(|g| g.phase == "default_actual_cash")
            .collect();
        assert_eq!(recoveries.len(), 2);
        assert!(recoveries[0].year < recoveries[1].year);
        assert_eq!(recoveries[0].next.principal, 70.);
        assert_eq!(recoveries[1].next.principal, 40.);
        assert_eq!(
            result.accounting().closing()[0]
                .reviewed_credit_state
                .as_ref()
                .unwrap()
                .claims[0]
                .recovery_cash_collected,
            60.
        );
    }
    #[test]
    fn reviewed_after_recovery_tail_extends_book_date_without_future_cash_or_earning() {
        let mut raw = request(35);
        default_at(&mut raw, CreditState::Performing, 0, 5.);
        raw.credit.intervals[0].loss_given_default = [0.4; 3];
        empty_reviewed_actions(&mut raw, false, false);
        raw.reviewed_credit_actions
            .as_mut()
            .unwrap()
            .chargeoffs
            .push(book::reviews::PathChargeoffReview {
                path_key: 17,
                review: portfolio_model_core::credit_accounting::ReviewedChargeoff {
                    review: portfolio_model_core::credit_accounting::Identity {
                        id: "late-charge".into(),
                        revision: "1".into(),
                    },
                    claim_id: "commercial-default:17".into(),
                    recognition_ordinal: 738936,
                    principal_amount: 40.,
                    book_adjustment_amount: 0.,
                    accrued_interest_reversal: 0.,
                },
            });
        let result = evaluate(&raw).unwrap();
        assert_eq!(result.full_credit_end_year(), 50. / 365.);
        let end = &result.accounting().closing()[0];
        assert_eq!(end.principal, 0.);
        assert_eq!(
            end.reviewed_credit_state.as_ref().unwrap().legal_principal,
            40.
        );
        assert_eq!(end.year, 50. / 365.);
        assert!(result
            .carry()
            .carry_points()
            .iter()
            .filter(|p| p.year > 35. / 365.)
            .all(|p| p.cash == 0. && p.effective_interest == 0.));
    }
}
