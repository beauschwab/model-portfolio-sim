//! Actual contractual source to original-yield income prerequisite.
//! This is not the commercial credit/exercise owner or a mixed journal adapter.
use crate::term_enhancements::{self as term, indexed};
use portfolio_model_core::{cashflow::CashflowMeasure, dated_income as income};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Write};

pub const SCHEMA: &str = "commercial-source-income-proof-1";
const MAX_WORK: usize = 2_000_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_RAW_BYTES: usize = 1024 * 1024;
const ROW_BYTES: usize = 8192;

#[cfg(test)]
thread_local! {static PROJECTED:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub identity: term::Identity,
    pub original_yield_anchor: term::Identity,
    pub income_review: term::Identity,
    pub cost: f64,
    pub original_annual_effective_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: term::Identity,
    pub recognition: String,
    pub reset_income: String,
    pub fees: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProspectiveRateReview {
    pub review: term::Identity,
    pub period_id: String,
    pub reset_ordinal: i32,
    pub annual_effective_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub original: indexed::ContractPathRequest,
    pub current: indexed::ContractPathRequest,
    pub acquisition: Acquisition,
    pub policy: Policy,
    #[serde(default)]
    pub current_accounting_rate_reviews: Vec<ProspectiveRateReview>,
}

#[derive(Debug, Serialize)]
pub struct Admission {
    work_upper: usize,
    retained_bytes_upper: usize,
    typed_raw_bytes: usize,
    original_points: usize,
    current_points: usize,
}

/// Full signed coupon-origin inventory. Recognized net accrued in the income
/// recurrence is separate: a positive and a negative coupon must not erase each
/// other's unpaid legal right. IEEE residuals are retained without a clamp.
#[derive(Debug, Serialize)]
pub struct ContractualRight {
    period_id: String,
    source_event_id: String,
    cash_ordinal: i32,
    opening_principal: f64,
    earned_interest: f64,
    actual_interest_cash: f64,
    remaining_signed_right: f64,
}

#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_request: Request,
    admission: Admission,
    original_source: indexed::ContractPathResult,
    current_source: indexed::ContractPathResult,
    original_factor_points: Vec<indexed::EarnedFactorPoint>,
    current_factor_points: Vec<indexed::EarnedFactorPoint>,
    original_contractual_rights: Vec<ContractualRight>,
    current_contractual_rights: Vec<ContractualRight>,
    original_income_request: income::DatedIncomeRequest,
    current_income_request: income::DatedIncomeRequest,
    original_income: income::DatedIncomeResult,
    current_income: income::DatedIncomeResult,
    original_quote_pv: f64,
    credit_adoption: bool,
    mixed_ledger_adoption: bool,
    empirical_adoption: bool,
    accounting_approval: bool,
}

impl ResultData {
    pub fn current_income(&self) -> &income::DatedIncomeResult {
        &self.current_income
    }
    pub fn original_income(&self) -> &income::DatedIncomeResult {
        &self.original_income
    }
}

fn checkpoint() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}

fn point_count(source: &indexed::ContractPathRequest, daily: bool) -> Result<usize, String> {
    if source.periods.is_empty() || source.periods.len() > income::MAX_EVENTS {
        return Err("commercial source requires complete bounded native periods".into());
    }
    let mut n = 0usize;
    for p in &source.periods {
        checkpoint()?;
        let count = if daily {
            usize::try_from(
                p.end_ordinal
                    .checked_sub(p.start_ordinal)
                    .ok_or("commercial daily source interval overflow")?,
            )
            .map_err(|_| "commercial daily source interval reversed")?
        } else {
            match &p.coupon {
                indexed::Coupon::Fixed { .. } => 1,
                indexed::Coupon::Indexed { observations, .. } => observations.len(),
            }
        };
        n = n
            .checked_add(count)
            .ok_or("commercial source work overflow")?;
        // Cash dates plus recognition dates plus explicit report/full checkpoints.
        if n.checked_add(source.periods.len())
            .and_then(|v| v.checked_add(2))
            .is_none_or(|v| v > income::MAX_EVENTS)
        {
            return Err(
                "commercial source income event upper exceeds 4096 before generation".into(),
            );
        }
    }
    Ok(n)
}

fn contractual_rights(
    source: &indexed::ContractPathResult,
    points: &[indexed::EarnedFactorPoint],
) -> Result<Vec<ContractualRight>, String> {
    // Stable source event ordering is retained in source.schedule; this index
    // only associates each independently owned coupon with its actual payment.
    let cash: BTreeMap<_, _> = source
        .schedule
        .events
        .iter()
        .map(|e| (&e.event_id, e))
        .collect();
    let mut cursor = 0;
    let mut rights = Vec::with_capacity(source.audit.len());
    for audit in &source.audit {
        checkpoint()?;
        let mut prior = 0.;
        let mut earned = 0.;
        while let Some(p) = points
            .get(cursor)
            .filter(|p| p.period_id == audit.period_id)
        {
            checkpoint()?;
            earned = term::amount(
                earned + term::amount(audit.opening_principal * (p.cumulative_factor - prior))?,
            )?;
            prior = p.cumulative_factor;
            cursor += 1;
        }
        let event = cash
            .get(&audit.period_id)
            .ok_or("native coupon/cash identity missing")?;
        rights.push(ContractualRight {
            period_id: audit.period_id.clone(),
            source_event_id: event.event_id.clone(),
            cash_ordinal: event.payment_ordinal,
            opening_principal: audit.opening_principal,
            earned_interest: earned,
            actual_interest_cash: event.components.cash_interest,
            remaining_signed_right: term::amount(earned - event.components.cash_interest)?,
        });
    }
    if cursor != points.len() {
        return Err("native coupon/capture identity mismatch".into());
    }
    Ok(rights)
}

fn bounded_raw(r: &Request) -> Result<usize, String> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            checkpoint().map_err(io::Error::other)?;
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("raw overflow"))?;
            if self.0 > MAX_RAW_BYTES {
                return Err(io::Error::other("commercial typed raw exceeds 1 MiB"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, r).map_err(|e| e.to_string())?;
    Ok(count.0)
}

pub fn admit(r: &Request) -> Result<Admission, String> {
    admit_for_measure(r, CashflowMeasure::Physical)
}
/// Shared contract admission only. Pricing graphs never invoke accounting
/// construction or infer a measure conversion from this bounded source check.
pub fn admit_for_measure(r: &Request, measure: CashflowMeasure) -> Result<Admission, String> {
    checkpoint()?;
    let (o, c) = (&r.original, &r.current);
    if r.schema != SCHEMA
        || !matches!(
            r.policy.recognition.as_str(),
            "native_factor_checkpoint_rights-1" | "actual_daily_matched_native_fragments-1"
        )
        || !matches!(
            r.policy.reset_income.as_str(),
            "reviewed_frozen_original_reference-1" | "reviewed_prospective_at_period_start-1"
        )
        || !matches!(
            r.policy.fees.as_str(),
            "integral_cash-1" | "nonintegral_cash-1"
        )
        || o.measure != measure
        || c.measure != measure
        || o.direction != portfolio_model_core::floating_rate::Direction::Receipt
        || c.direction != o.direction
        || o.currency != c.currency
        || o.monetary_unit != c.monetary_unit
        || o.contract != c.contract
        || o.asof_ordinal != c.asof_ordinal
        || o.opening_principal.to_bits() != c.opening_principal.to_bits()
        || o.periods.len() != c.periods.len()
        || o.tail_end_ordinal != c.tail_end_ordinal
    {
        return Err("explicit paired acquired Physical source and reviewed checkpoint/income/fee policies required".into());
    }
    let mut metadata = 0;
    for id in [
        &r.acquisition.identity,
        &r.acquisition.original_yield_anchor,
        &r.acquisition.income_review,
        &r.policy.identity,
    ] {
        term::identity(id, &mut metadata)?;
    }
    term::nonnegative(r.acquisition.cost)?;
    if !r.acquisition.original_annual_effective_rate.is_finite()
        || !(-0.99..=10.).contains(&r.acquisition.original_annual_effective_rate)
    {
        return Err("commercial original effective yield requires -0.99..10".into());
    }
    let daily = r.policy.recognition == "actual_daily_matched_native_fragments-1";
    let original_points = point_count(o, daily)?;
    let current_points = point_count(c, daily)?;
    let prospective = r.policy.reset_income == "reviewed_prospective_at_period_start-1";
    if (!prospective && !r.current_accounting_rate_reviews.is_empty())
        || (prospective && r.current_accounting_rate_reviews.is_empty())
    {
        return Err(
            "reset recognition selection and explicit prospective rate reviews disagree".into(),
        );
    }
    if current_points
        .checked_add(c.periods.len())
        .and_then(|n| n.checked_add(r.current_accounting_rate_reviews.len() + 2))
        .is_none_or(|n| n > income::MAX_EVENTS)
    {
        return Err(
            "commercial prospective rate/event upper exceeds 4096 before generation".into(),
        );
    }
    let mut next_review = 0;
    if prospective {
        for period in &c.periods {
            checkpoint()?;
            if matches!(period.coupon, indexed::Coupon::Indexed { .. }) {
                let change=r.current_accounting_rate_reviews.get(next_review).ok_or("every indexed period start requires its explicit prospective recognition-rate review")?;
                term::identity(&change.review, &mut metadata)?;
                if change.period_id != period.period_id
                    || change.reset_ordinal != period.start_ordinal
                    || !change.annual_effective_rate.is_finite()
                    || !(-0.99..=10.).contains(&change.annual_effective_rate)
                {
                    return Err("prospective review must bind the exact indexed period start and bounded annual effective recognition rate".into());
                }
                next_review += 1;
            }
        }
        if next_review != r.current_accounting_rate_reviews.len() || next_review == 0 {
            return Err(
                "unmatched prospective accounting-rate review or no actual indexed reset".into(),
            );
        }
    }
    let raw = bounded_raw(r)?;
    if serde_json::to_vec(&o.principal_plan).map_err(|e| e.to_string())?
        != serde_json::to_vec(&c.principal_plan).map_err(|e| e.to_string())?
    {
        return Err("retained original principal/payment plan identity mismatch".into());
    }
    for (a, b) in o.periods.iter().zip(&c.periods) {
        checkpoint()?;
        if a.start_ordinal < o.asof_ordinal {
            return Err(
                "seasoned/pre-acquisition accrued rights require a separate explicit prefix owner"
                    .into(),
            );
        }
        if a.period_id != b.period_id
            || a.start_ordinal != b.start_ordinal
            || a.end_ordinal != b.end_ordinal
            || a.payment_ordinal != b.payment_ordinal
            || a.day_count != b.day_count
            || a.fee.to_bits() != b.fee.to_bits()
        {
            return Err("complete original/current contractual period identity mismatch".into());
        }
        let shape = |coupon: &indexed::Coupon| -> Result<serde_json::Value, String> {
            let mut value = serde_json::to_value(coupon).map_err(|e| e.to_string())?;
            if let Some(observations) = value.get_mut("observations").and_then(|v| v.as_array_mut())
            {
                for observation in observations {
                    checkpoint()?;
                    let object = observation
                        .as_object_mut()
                        .ok_or("invalid native observation shape")?;
                    object.remove("rate");
                    object.remove("source");
                }
            }
            Ok(value)
        };
        if serde_json::to_vec(&shape(&a.coupon)?).map_err(|e| e.to_string())?
            != serde_json::to_vec(&shape(&b.coupon)?).map_err(|e| e.to_string())?
        {
            return Err("contractual reset/index/calendar/payment identity changed without modification ownership".into());
        }
    }
    let points = original_points + current_points + r.current_accounting_rate_reviews.len();
    let periods = o.periods.len() + c.periods.len();
    // Raw/DTO/source metadata clones and escaped output staging are separate
    // from point + income-event + income-row ownership. Every term identity is
    // at most 128 UTF-8 bytes/control-free; only quotes/backslashes expand in
    // JSON. 8192 per reserved row covers all three retained row forms, numeric
    // bodies, allocator-rounded identity copies and their escaped output bytes.
    // Period overhead reserves audit/cash/gross-right rows too. Not process RSS.
    let bytes = raw
        .checked_mul(20)
        .and_then(|n| n.checked_add((points + periods * 2 + 4).checked_mul(ROW_BYTES)?))
        .ok_or("commercial source capacity overflow")?;
    let work = raw
        .checked_mul(4)
        .and_then(|n| n.checked_add((points + periods * 2 + 4).checked_mul(128)?))
        .ok_or("commercial source work overflow")?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err(
            "commercial source joined 2M work/32MiB component admission refused before generation"
                .into(),
        );
    }
    Ok(Admission {
        work_upper: work,
        retained_bytes_upper: bytes,
        typed_raw_bytes: raw,
        original_points,
        current_points,
    })
}

fn income_request(
    r: &Request,
    raw: &indexed::ContractPathRequest,
    source: &indexed::ContractPathResult,
    points: &[indexed::EarnedFactorPoint],
    changes: &[ProspectiveRateReview],
) -> Result<income::DatedIncomeRequest, String> {
    let mut days = BTreeMap::<i32, [f64; 4]>::new();
    let mut cursor = 0;
    for audit in &source.audit {
        checkpoint()?;
        let mut prior = 0.;
        while let Some(point) = points
            .get(cursor)
            .filter(|p| p.period_id == audit.period_id)
        {
            if point.ordinal <= raw.asof_ordinal {
                return Err("seasoned/pre-acquisition accrued rights require a separate explicit prefix owner".into());
            }
            let earned = term::amount(audit.opening_principal * (point.cumulative_factor - prior))?;
            let cell = days.entry(point.ordinal).or_default();
            cell[0] = term::amount(cell[0] + earned)?;
            prior = point.cumulative_factor;
            cursor += 1;
        }
    }
    if cursor != points.len() {
        return Err("native period/capture identity mismatch".into());
    }
    for cash in &source.schedule.events {
        checkpoint()?;
        let cell = days.entry(cash.payment_ordinal).or_default();
        cell[1] = term::amount(cell[1] + cash.components.principal)?;
        cell[2] = term::amount(cell[2] + cash.components.cash_interest)?;
        cell[3] = term::amount(cell[3] + cash.components.fees)?;
    }
    days.entry(raw.reporting_end_ordinal).or_default();
    days.entry(raw.tail_end_ordinal).or_default();
    for change in changes {
        checkpoint()?;
        days.entry(change.reset_ordinal).or_default();
    }
    let rate_changes: BTreeMap<_, _> = changes.iter().map(|c| (c.reset_ordinal, c)).collect();
    let integral = r.policy.fees == "integral_cash-1";
    let source_identity = income::Identity {
        id: raw.path.id.clone(),
        revision: raw.path.revision.clone(),
    };
    let events = days
        .into_iter()
        .map(|(date, v)| income::IncomeEvent {
            event_id: format!("source-income:{date}"),
            source: source_identity.clone(),
            payment_ordinal: date,
            contractual_interest_earned: v[0],
            principal_cash: v[1],
            interest_cash: v[2],
            integral_fee_cash: if integral { v[3] } else { 0. },
            nonintegral_fee_cash: if integral { 0. } else { v[3] },
            yield_change: rate_changes
                .get(&date)
                .map(|change| income::YieldChange::Prospective {
                    review: income::Identity {
                        id: change.review.id.clone(),
                        revision: change.review.revision.clone(),
                    },
                    annual_effective_rate: change.annual_effective_rate,
                }),
        })
        .collect();
    Ok(income::DatedIncomeRequest {
        schema: income::SCHEMA.into(),
        contract: income::Identity {
            id: raw.contract.id.clone(),
            revision: raw.contract.revision.clone(),
        },
        original_yield_anchor: income::Identity {
            id: r.acquisition.original_yield_anchor.id.clone(),
            revision: r.acquisition.original_yield_anchor.revision.clone(),
        },
        policy: income::Identity {
            id: r.acquisition.income_review.id.clone(),
            revision: r.acquisition.income_review.revision.clone(),
        },
        currency: raw.currency.clone(),
        monetary_unit: raw.monetary_unit.clone(),
        measure: raw.measure,
        as_of_ordinal: raw.asof_ordinal,
        full_contract_end_ordinal: raw.tail_end_ordinal,
        opening_principal: raw.opening_principal,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: r.acquisition.cost,
        original_annual_effective_rate: r.acquisition.original_annual_effective_rate,
        opening_annual_effective_rate: r.acquisition.original_annual_effective_rate,
        events,
    })
}

pub(crate) fn original_cash_quote(
    r: &Request,
    original_source: &indexed::ContractPathResult,
) -> Result<f64, String> {
    let mut original_quote_pv = 0.;
    for cash in &original_source.schedule.events {
        checkpoint()?;
        let amount = cash.components.principal
            + cash.components.cash_interest
            + if r.policy.fees == "integral_cash-1" {
                cash.components.fees
            } else {
                0.
            };
        original_quote_pv = term::amount(
            original_quote_pv
                + amount
                    * (-r.acquisition.original_annual_effective_rate.ln_1p()
                        * (cash.payment_ordinal - r.original.asof_ordinal) as f64
                        / 365.)
                        .exp(),
        )?;
    }
    if !term::near(original_quote_pv, r.acquisition.cost) {
        return Err("original complete actual-cash quote versus immutable cost/yield failed; no current refit".into());
    }
    Ok(original_quote_pv)
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let admission = admit(r)?;
    let mut original_factor_points = Vec::with_capacity(admission.original_points);
    let build = |source: &indexed::ContractPathRequest,
                 sink: &mut indexed::EarnedFactorSink<'_>| {
        #[cfg(test)]
        PROJECTED.with(|n| n.set(n.get() + 1));
        if r.policy.recognition == "actual_daily_matched_native_fragments-1" {
            indexed::build_path_with_daily_capture(source, &r.policy.identity, sink)
        } else {
            indexed::build_path_with_capture(source, sink)
        }
    };
    let original_source = build(&r.original, &mut |p| {
        original_factor_points.push(p);
        Ok(())
    })?;
    let mut current_factor_points = Vec::with_capacity(admission.current_points);
    let current_source = build(&r.current, &mut |p| {
        current_factor_points.push(p);
        Ok(())
    })?;
    let original_contractual_rights =
        contractual_rights(&original_source, &original_factor_points)?;
    let current_contractual_rights = contractual_rights(&current_source, &current_factor_points)?;
    let original_income_request = income_request(
        r,
        &r.original,
        &original_source,
        &original_factor_points,
        &[],
    )?;
    let current_income_request = income_request(
        r,
        &r.current,
        &current_source,
        &current_factor_points,
        &r.current_accounting_rate_reviews,
    )?;
    let original_quote_pv = original_cash_quote(r, &original_source)?;
    checkpoint()?;
    let original_income = income::evaluate(&original_income_request)?;
    checkpoint()?;
    let current_income = income::evaluate(&current_income_request)?;
    checkpoint()?;
    Ok(ResultData {
        schema: SCHEMA,
        raw_request: r.clone(),
        admission,
        original_source,
        current_source,
        original_factor_points,
        current_factor_points,
        original_contractual_rights,
        current_contractual_rights,
        original_income_request,
        current_income_request,
        original_income,
        current_income,
        original_quote_pv,
        credit_adoption: false,
        mixed_ledger_adoption: false,
        empirical_adoption: false,
        accounting_approval: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(rate: f64, fee: f64, eir: f64, integral: bool) -> Request {
        let source = json!({"schema":"term-enhancements-1","model_version":"term-contracts-1",
            "contract":{"id":"commercial-original","revision":"1"},"assumption":{"id":"assumption","revision":"1"},
            "path":{"id":"path","revision":"1"},"position_id":"commercial","scenario_id":"base",
            "currency":"USD","monetary_unit":"currency_units","direction":"receipt","measure":"physical",
            "asof_ordinal":738886,"reporting_end_ordinal":738901,"tail_end_ordinal":738916,
            "observed_publication_cutoff_unix_seconds":1704067200_i64,"opening_principal":100.,
            "principal_plan":{"kind":"supplied","principal_payments":[100.]},
            "periods":[{"period_id":"coupon","start_ordinal":738886,"end_ordinal":738916,
                "payment_ordinal":738916,"day_count":"act365_fixed","fee":fee,
                "coupon":{"kind":"fixed","annual_rate":rate}}]});
        let original = serde_json::from_value(source.clone()).unwrap();
        let current = serde_json::from_value(source).unwrap();
        let cost = (100. + 100. * rate * 30. / 365. + if integral { fee } else { 0. })
            * (-eir.ln_1p() * 30. / 365.).exp();
        let identity = term::Identity {
            id: "explicit-review".into(),
            revision: "1".into(),
        };
        Request {
            schema: SCHEMA.into(),
            original,
            current,
            acquisition: Acquisition {
                identity: identity.clone(),
                original_yield_anchor: identity.clone(),
                income_review: identity.clone(),
                cost,
                original_annual_effective_rate: eir,
            },
            policy: Policy {
                identity,
                recognition: "native_factor_checkpoint_rights-1".into(),
                reset_income: "reviewed_frozen_original_reference-1".into(),
                fees: if integral {
                    "integral_cash-1"
                } else {
                    "nonintegral_cash-1"
                }
                .into(),
            },
            current_accounting_rate_reviews: vec![],
        }
    }

    #[test]
    fn commercial_source_income_signed_coupon_premium_discount_and_original_eir() {
        for coupon in [-0.01, 0., 0.05] {
            for eir in [-0.10, 0., 0.07] {
                let raw = request(coupon, 0., eir, true);
                let result = evaluate(&raw).unwrap();
                assert_eq!(
                    result.original_quote_pv.to_bits(),
                    raw.acquisition.cost.to_bits()
                );
                assert_eq!(
                    serde_json::to_vec(&result.original_source).unwrap(),
                    serde_json::to_vec(&result.current_source).unwrap()
                );
                assert!(result.original_income.remaining_gross_carrying_value.abs() < 1e-10);
                assert_eq!(result.current_income.remaining_principal, 0.);
                let report = &result.current_income.rows[0];
                let hand = raw.acquisition.cost * (eir.ln_1p() * 15. / 365.).exp();
                assert!((report.closing_gross_carrying_value - hand).abs() < 1e-12);
                assert_eq!(report.contractual_interest_earned, 0.);
                assert_eq!(report.closing_accrued_interest, 0.);
            }
        }
    }

    #[test]
    fn commercial_source_income_fee_cash_and_effective_yield_are_separate() {
        for integral in [false, true] {
            for fee in [-0.05, 0.05] {
                let result = evaluate(&request(0.05, fee, 0., integral)).unwrap();
                let last = result.current_income.rows.last().unwrap();
                assert_eq!(
                    last.recognized_nonintegral_fee_income,
                    if integral { 0. } else { fee }
                );
                assert_eq!(last.effective_interest, 0.);
                assert_eq!(last.cash_received, 100. + 100. * 0.05 * 30. / 365. + fee);
                assert!(result.current_income.remaining_gross_carrying_value.abs() < 1e-10);
            }
        }
    }

    #[test]
    fn commercial_source_income_full_original_quote_is_not_refitted_on_current_rates() {
        let mut raw = request(0., 0., 0., true);
        let coupon = json!({"kind":"indexed","index":{"index_id":"term","kind":"term","tenor_months":1,"average_calendar_days":null},
            "calendar":{"id":"calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"term_simple","margin_treatment":"simple",
            "margin":0.,"index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":1704067200_i64,
            "lookback_business_days":0,"observation_shift":false,
            "observations":[{"interest_start_ordinal":738886,"interest_end_ordinal":738916,"observation_start_ordinal":738886,"observation_end_ordinal":738916,
                "fixing_ordinal":738886,"selection_cutoff_unix_seconds":1704067200_i64,"rate":0.05,
                "source":{"kind":"conditional_projection","model":{"id":"supplied","revision":"1"},"path":{"id":"path","revision":"1"},"information_unix_seconds":1704067200_i64,"measure":"physical"}}]});
        raw.original.periods[0].coupon = serde_json::from_value(coupon.clone()).unwrap();
        let mut current = coupon;
        current["observations"][0]["rate"] = json!(0.02);
        raw.current.periods[0].coupon = serde_json::from_value(current).unwrap();
        raw.acquisition.cost = 100. + 100. * 0.05 * 30. / 365.;
        let result = evaluate(&raw).unwrap();
        assert_eq!(result.original_quote_pv, raw.acquisition.cost);
        assert!(result.current_income.remaining_gross_carrying_value > 0.);
        assert_eq!(result.current_income.remaining_principal, 0.);
        assert!(result
            .current_income
            .rows
            .iter()
            .all(|row| row.annual_effective_rate == 0.));
        assert!(!result.credit_adoption && !result.mixed_ledger_adoption);
        raw.acquisition.cost += 1.;
        assert!(evaluate(&raw).unwrap_err().contains("immutable cost/yield"));
    }

    #[test]
    fn commercial_source_income_missing_prefix_and_unreviewed_contract_changes_refuse() {
        let mut raw = request(0.05, 0., 0., true);
        raw.current.periods[0].start_ordinal -= 1;
        assert!(admit(&raw).is_err());
        raw = request(0.05, 0., 0., true);
        raw.original.periods[0].start_ordinal -= 1;
        raw.current.periods[0].start_ordinal -= 1;
        assert!(admit(&raw).unwrap_err().contains("prefix"));
        raw = request(0.05, 0., 0., true);
        raw.current.periods[0].coupon = indexed::Coupon::Fixed { annual_rate: 0.08 };
        assert!(admit(&raw).unwrap_err().contains("modification ownership"));
    }

    #[test]
    fn commercial_source_income_preflight_and_inherited_cancel_refuse() {
        let mut raw = request(0., 0., 0., true);
        raw.original
            .periods
            .resize(4097, raw.original.periods[0].clone());
        raw.current.periods = raw.original.periods.clone();
        assert!(admit(&raw).unwrap_err().contains("bounded native periods"));
        let raw = request(0., 0., 0., true);
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let control =
            portfolio_compute_control::Control::with_cancel_flag(30_000, cancelled).unwrap();
        assert!(portfolio_compute_control::scope(Some(control), || evaluate(&raw)).is_err());
    }

    #[test]
    fn commercial_source_income_daily_original_eir_accrued_and_basis_couple() {
        let mut raw = request(0.05, 0., 0.07, true);
        raw.policy.recognition = "actual_daily_matched_native_fragments-1".into();
        let result = evaluate(&raw).unwrap();
        assert_eq!(result.current_factor_points.len(), 30);
        assert_eq!(result.current_income.rows.len(), 30);
        let report = &result.current_income.rows[14];
        let contractual = 100. * 0.05 * 15. / 365.;
        let gross = raw.acquisition.cost * (0.07_f64.ln_1p() * 15. / 365.).exp();
        assert!((report.closing_accrued_interest - contractual).abs() < 1e-12);
        assert!((report.closing_gross_carrying_value - gross).abs() < 1e-12);
        assert!((report.closing_book_adjustment - (gross - 100. - contractual)).abs() < 1e-12);
        assert_eq!(report.closing_principal, 100.);
        assert!(result.current_income.remaining_gross_carrying_value.abs() < 1e-10);
    }

    #[test]
    fn commercial_source_income_early_cash_retains_post_principal_zero_rights() {
        let mut raw = request(0.05, 0., 0., true);
        raw.policy.recognition = "actual_daily_matched_native_fragments-1".into();
        raw.original.periods[0].payment_ordinal = raw.original.asof_ordinal + 10;
        raw.current.periods = raw.original.periods.clone();
        let result = evaluate(&raw).unwrap();
        let payment = &result.current_income.rows[9];
        assert_eq!(payment.closing_principal, 0.);
        assert!(payment.closing_accrued_interest < 0.);
        assert!(payment.closing_book_adjustment > 0.);
        assert!(result.current_income.rows[10].contractual_interest_earned > 0.);
        assert_eq!(result.current_income.rows[10].cash_received, 0.);
        let right = &result.current_contractual_rights[0];
        assert_eq!(right.cash_ordinal, raw.original.asof_ordinal + 10);
        assert_eq!(
            right.actual_interest_cash,
            result.current_source.audit[0].interest
        );
        assert_eq!(
            right.remaining_signed_right.to_bits(),
            (right.earned_interest - right.actual_interest_cash).to_bits()
        );
        // No full-tail clamp of A or its offsetting B: either IEEE remainder is
        // retained exactly even though the original complete cash quote closes G.
        let actual = result.current_income.rows.last().unwrap();
        assert_eq!(actual.closing_principal, 0.);
        assert_eq!(
            actual.closing_book_adjustment.to_bits(),
            (actual.closing_gross_carrying_value - actual.closing_accrued_interest).to_bits()
        );
    }

    #[test]
    fn commercial_source_income_invalid_extreme_daily_clock_is_error_not_panic() {
        let mut raw = request(0.05, 0., 0., true);
        raw.policy.recognition = "actual_daily_matched_native_fragments-1".into();
        raw.original.periods[0].start_ordinal = i32::MIN;
        raw.original.periods[0].end_ordinal = i32::MAX;
        raw.current.periods = raw.original.periods.clone();
        let error = admit(&raw).unwrap_err();
        assert!(error.contains("interval overflow"));
    }

    #[test]
    fn commercial_source_income_joined_capacity_refuses_before_both_generators() {
        let mut raw = request(0.05, 0., 0., true);
        raw.policy.recognition = "actual_daily_matched_native_fragments-1".into();
        for source in [&mut raw.original, &mut raw.current] {
            source.periods[0].end_ordinal = source.asof_ordinal + 2100;
            source.periods[0].payment_ordinal = source.asof_ordinal + 2100;
            source.tail_end_ordinal = source.asof_ordinal + 2100;
        }
        PROJECTED.with(|n| n.set(0));
        assert!(evaluate(&raw).unwrap_err().contains("before generation"));
        PROJECTED.with(|n| assert_eq!(n.get(), 0));
    }

    #[test]
    fn commercial_source_income_prospective_rate_is_separate_from_original_anchor() {
        let mut raw = request(0., 0., 0.03, true);
        let coupon = json!({"kind":"indexed","index":{"index_id":"term","kind":"term","tenor_months":1,"average_calendar_days":null},
            "calendar":{"id":"calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"term_simple","margin_treatment":"simple",
            "margin":0.,"index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":1704067200_i64,
            "lookback_business_days":0,"observation_shift":false,
            "observations":[{"interest_start_ordinal":738886,"interest_end_ordinal":738916,"observation_start_ordinal":738886,"observation_end_ordinal":738916,
                "fixing_ordinal":738886,"selection_cutoff_unix_seconds":1704067200_i64,"rate":0.05,
                "source":{"kind":"conditional_projection","model":{"id":"supplied","revision":"1"},"path":{"id":"path","revision":"1"},"information_unix_seconds":1704067200_i64,"measure":"physical"}}]});
        raw.original.periods[0].coupon = serde_json::from_value(coupon.clone()).unwrap();
        raw.current.periods[0].coupon = serde_json::from_value(coupon).unwrap();
        raw.acquisition.cost =
            (100. + 100. * 0.05 * 30. / 365.) * (-0.03_f64.ln_1p() * 30. / 365.).exp();
        raw.policy.recognition = "actual_daily_matched_native_fragments-1".into();
        raw.policy.reset_income = "reviewed_prospective_at_period_start-1".into();
        raw.current_accounting_rate_reviews
            .push(ProspectiveRateReview {
                review: raw.policy.identity.clone(),
                period_id: "coupon".into(),
                reset_ordinal: 738886,
                annual_effective_rate: 0.04,
            });
        let result = evaluate(&raw).unwrap();
        let first = &result.current_income.rows[0];
        assert_eq!(first.payment_ordinal, 738886);
        assert_eq!(first.annual_effective_rate, 0.03);
        assert_eq!(first.effective_interest, 0.);
        assert_eq!(
            first.closing_gross_carrying_value.to_bits(),
            raw.acquisition.cost.to_bits()
        );
        assert!(result.current_income.rows[1..]
            .iter()
            .all(|r| r.annual_effective_rate == 0.04));
        assert!(result
            .original_income
            .rows
            .iter()
            .all(|r| r.annual_effective_rate == 0.03));
        assert_eq!(
            result.original_income.original_yield_anchor.id,
            raw.acquisition.original_yield_anchor.id
        );
        assert_eq!(
            result.original_quote_pv.to_bits(),
            raw.acquisition.cost.to_bits()
        );
        assert!(result.current_income.remaining_gross_carrying_value > 0.);
        raw.current_accounting_rate_reviews[0].reset_ordinal += 1;
        assert!(admit(&raw)
            .unwrap_err()
            .contains("exact indexed period start"));
        raw.current_accounting_rate_reviews.clear();
        assert!(admit(&raw).unwrap_err().contains("selection"));
    }
}
