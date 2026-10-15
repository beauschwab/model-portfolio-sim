//! Unapplied purchase-time liability expectation revision research policy.
//! Raw Physical deposit state remains the sole economic owner; original yield
//! and cost never change. No accounting-standard approval is inferred.
use crate::accounting::dated_income::{
    self, DatedIncomeRequest, DatedIncomeResult, FutureCash, Identity, IncomeEvent, YieldChange,
};
use crate::cashflows::dated::CashflowMeasure;
use crate::products::deposits::state::{
    self as deposit_state, DepositStateRequest, DepositStateResult, TerminalPolicy,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "deposit-revised-income-source-1";
pub const MAX_DAYS: u32 = 64;
pub const MAX_MONEY: f64 = 10_000.;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RUNS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub purchase_ordinal: i32,
    pub original_principal: f64,
    pub original_price_per_100: f64,
    pub original_gross_carrying_value: f64,
    pub original_annual_effective_rate: f64,
    pub purchase_quote: Identity,
    pub original_yield_anchor: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionPolicy {
    pub review: Identity,
    pub expectation_policy: String,
    pub pnl_policy: String,
    pub phase_policy: String,
    pub tail_policy: String,
    /// Only future observations in these named dimensions may differ.
    pub changed_driver_dimensions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSource {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub revised: DepositStateRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub contract: Identity,
    pub anchor: Anchor,
    pub policy: RevisionPolicy,
    pub original: DepositStateRequest,
    pub runs: Vec<RunSource>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub raw_bytes: usize,
    pub paths: usize,
    pub full_end_ordinal: i32,
}
#[derive(Debug, Clone, Serialize)]
pub struct Path {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub source: DepositStateResult,
    pub income_request: DatedIncomeRequest,
    pub income: DatedIncomeResult,
    pub original_cost: f64,
    pub revised_carrying_value: f64,
    pub basis_revision: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub admission: Admission,
    pub original_source: DepositStateResult,
    pub original_pv: f64,
    pub paths: Vec<Path>,
    pub accounting_standard_approval: bool,
    pub installed_adoption: bool,
    pub capital_ftp_eligible: bool,
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn money(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONEY {
        return Err("revised deposit component money exceeds 10000".into());
    }
    Ok(())
}
fn ident(x: &Identity) -> Result<(), String> {
    if [&x.id, &x.revision]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err("explicit bounded revised deposit identity required".into());
    }
    Ok(())
}
fn exact<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_vec(a).map_err(|e| e.to_string())?
        == serde_json::to_vec(b).map_err(|e| e.to_string())?)
}
pub(crate) fn shape_limit(r: &DepositStateRequest, max_days: u32) -> Result<(), String> {
    if r.schema != deposit_state::SCHEMA
        || r.identity.measure != CashflowMeasure::Physical
        || r.horizon_days == 0
        || r.horizon_days > max_days
        || r.segments.len() != 1
        || !r.migrations.is_empty()
        || !matches!(r.terminal, TerminalPolicy::RetainResidual {})
    {
        return Err("revised deposit requires one physical segment, complete <=64 day actual maturity, no migration or assumed terminal".into());
    }
    let s = &r.segments[0];
    money(s.opening_balance)?;
    money(s.opening_accrued_interest)?;
    if s.opening_balance <= 0.
        || s.opening_accrued_interest != 0.
        || s.opening_paid_rate < 0.
        || s.repricing.floor < 0.
        || s.repricing.ceiling > 1.
        || s.repricing.observation_lag_days > MAX_DAYS
    {
        return Err("revised deposit admits positive opening principal, zero accrued, nonnegative paid rates and bounded lag".into());
    }
    let end = r
        .as_of_ordinal
        .checked_add(r.horizon_days as i32)
        .ok_or("deposit end overflow")?;
    if s.maturity_ordinal
        .is_none_or(|d| d <= r.as_of_ordinal || d > end)
        || s.interest_payment_ordinals
            .iter()
            .any(|d| *d <= r.as_of_ordinal || *d > end)
    {
        return Err(
            "revised deposit maturity must be within retained full end; no day0 cash".into(),
        );
    }
    for d in &r.drivers {
        if d.external_inflows.iter().any(|x| *x != 0.) {
            return Err("revised deposit external inflow unsupported".into());
        }
    }
    deposit_state::validate_declared_request(r)?;
    Ok(())
}
pub fn audit_request(r: &Request) -> Result<Admission, String> {
    if r.schema != SCHEMA || r.runs.is_empty() || r.runs.len() > MAX_RUNS {
        return Err("explicit bounded revised deposit schema and runs required".into());
    }
    validate_contracts(r, MAX_DAYS)?;
    reserve_legacy(r)
}
pub(crate) fn validate_contracts(r: &Request, max_days: u32) -> Result<(), String> {
    ident(&r.contract)?;
    ident(&r.anchor.purchase_quote)?;
    ident(&r.anchor.original_yield_anchor)?;
    ident(&r.policy.review)?;
    let p = &r.policy;
    if p.expectation_policy != "purchase_asof_original_yield_catchup-1"
        || p.pnl_policy != "liability_revision_separate_pnl-1"
        || p.phase_policy != "original_opening_then_revision_then_daily_recognition_settlement-1"
        || p.tail_policy != "complete_original_and_revised_tail-1"
    {
        return Err("explicit reviewed original-yield revision, separate P&L and full-tail policies required".into());
    }
    let dims: BTreeSet<_> = p
        .changed_driver_dimensions
        .iter()
        .map(String::as_str)
        .collect();
    if dims.len() != p.changed_driver_dimensions.len()
        || dims.iter().any(|d| {
            !matches!(
                *d,
                "reference_rate"
                    | "competitor_rates"
                    | "offered_spread_stress"
                    | "confidence_stress"
            )
        })
    {
        return Err("duplicate/unknown revised driver dimensions".into());
    }
    shape_limit(&r.original, max_days)?;
    for x in [
        r.anchor.original_principal,
        r.anchor.original_gross_carrying_value,
    ] {
        money(x)?;
    }
    if r.anchor.purchase_ordinal != r.original.as_of_ordinal
        || r.anchor.original_principal.to_bits() != r.original.segments[0].opening_balance.to_bits()
        || r.anchor.original_gross_carrying_value <= 0.
        || !r.anchor.original_price_per_100.is_finite()
        || r.anchor.original_price_per_100 <= 0.
        || r.anchor.original_price_per_100 > 1e6
        || !r.anchor.original_annual_effective_rate.is_finite()
        || !(0. ..=10.).contains(&r.anchor.original_annual_effective_rate)
        || !near(
            r.anchor.original_gross_carrying_value,
            r.anchor.original_principal * r.anchor.original_price_per_100 / 100.,
        )
    {
        return Err(
            "purchase-time original principal/cost/price/nonnegative yield anchors mismatch".into(),
        );
    }
    let mut ids = BTreeSet::new();
    let mut scenarios = BTreeSet::new();
    for run in &r.runs {
        ident(&Identity {
            id: run.run_id.clone(),
            revision: run.scenario_id.clone(),
        })?;
        if !ids.insert(&run.run_id)
            || !scenarios.insert(&run.scenario_id)
            || run.severity.to_bits() != 1f64.to_bits()
            || run.revised.identity.run_id != run.run_id
        {
            return Err("duplicate revision ownership or run identity/severity mismatch".into());
        }
        shape_limit(&run.revised, max_days)?;
        let a = &r.original;
        let b = &run.revised;
        if a.as_of_ordinal != b.as_of_ordinal
            || a.horizon_days != b.horizon_days
            || !exact(&a.day_count, &b.day_count)?
            || !exact(&a.segments, &b.segments)?
            || !exact(&a.terminal, &b.terminal)?
            || a.identity.entity_id != b.identity.entity_id
            || a.identity.cohort_id != b.identity.cohort_id
            || a.identity.currency != b.identity.currency
            || a.identity.model_version != b.identity.model_version
            || a.drivers.len() != b.drivers.len()
        {
            return Err(
                "revision changes immutable contractual terms/opening/model/identity".into(),
            );
        }
        for (old, new) in a.drivers.iter().zip(&b.drivers) {
            if old.ordinal != new.ordinal
                || !exact(&old.external_inflows, &new.external_inflows)?
                || (old.ordinal < a.as_of_ordinal && !exact(old, new)?)
            {
                return Err("revision changes past observations or driver calendar/inflow".into());
            }
            for (dim, same) in [
                (
                    "reference_rate",
                    old.reference_rate.to_bits() == new.reference_rate.to_bits(),
                ),
                (
                    "competitor_rates",
                    exact(&old.competitor_rates, &new.competitor_rates)?,
                ),
                (
                    "offered_spread_stress",
                    exact(&old.offered_spread_stress, &new.offered_spread_stress)?,
                ),
                (
                    "confidence_stress",
                    exact(&old.confidence_stress, &new.confidence_stress)?,
                ),
            ] {
                if !same && !dims.contains(dim) {
                    return Err(format!("undeclared revised driver dimension {dim}"));
                }
            }
        }
    }
    Ok(())
}
fn reserve_legacy(r: &Request) -> Result<Admission, String> {
    let raw_bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw_bytes > 1024 * 1024 {
        return Err("revised deposit raw bytes exceed 1MiB".into());
    }
    let paths = r.runs.len().checked_add(1).ok_or("path overflow")?;
    let days = r.original.horizon_days as usize;
    let work_units = paths
        .checked_mul(days.checked_mul(4096).ok_or("daily work overflow")?)
        .and_then(|n| n.checked_add(raw_bytes.checked_mul(8)?))
        .ok_or("source work overflow")?;
    let reserved_bytes = paths
        .checked_mul(days.checked_add(2).ok_or("days overflow")?)
        .and_then(|n| n.checked_mul(32768))
        .and_then(|n| n.checked_add(raw_bytes.checked_mul(12)?))
        .ok_or("source bytes overflow")?;
    if work_units > MAX_WORK || reserved_bytes > MAX_BYTES {
        return Err("revised deposit joined source work/bytes exceed admission".into());
    }
    Ok(Admission {
        work_units,
        reserved_bytes,
        raw_bytes,
        paths: r.runs.len(),
        full_end_ordinal: r
            .original
            .as_of_ordinal
            .checked_add(r.original.horizon_days as i32)
            .ok_or("full end overflow")?,
    })
}
fn cash(s: &DepositStateResult) -> Result<BTreeMap<i32, (f64, f64)>, String> {
    let mut out = BTreeMap::new();
    for c in &s.cashflows {
        money(c.principal)?;
        money(c.cash_interest)?;
        if c.principal > 0. || c.cash_interest > 0. {
            return Err("deposit revised source has inflow or negative coupon ownership".into());
        }
        let e = out.entry(c.ordinal).or_insert((0., 0.));
        e.0 -= c.principal;
        e.1 -= c.cash_interest;
        money(e.0)?;
        money(e.1)?;
        money(e.0 + e.1)?;
    }
    Ok(out)
}
fn source_valid(s: &DepositStateResult) -> Result<(), String> {
    if !near(s.closing_balance, 0.)
        || !near(s.closing_accrued_interest, 0.)
        || !s.migrations.is_empty()
        || s.supplied_franchise_value.is_some()
    {
        return Err("revised deposit source incomplete principal/interest tail".into());
    }
    for point in &s.points {
        for x in point.balances.iter().chain(&point.accrued_interest) {
            money(*x)?;
        }
        if point.paid_rates.iter().any(|x| *x < 0.) {
            return Err("negative paid-rate ownership unsupported".into());
        }
    }
    for e in &s.interest {
        money(e.amount)?;
        if e.amount < 0. {
            return Err("negative coupon ownership unsupported".into());
        }
    }
    cash(s)?;
    Ok(())
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let admission = audit_request(r)?;
    check()?;
    let original_source = deposit_state::simulate(&r.original)?;
    check()?;
    source_valid(&original_source)?;
    let original_cash = cash(&original_source)?;
    let asof = r.original.as_of_ordinal;
    if original_cash.contains_key(&asof) {
        return Err("original day0 cash unsupported by purchase-time revision ownership".into());
    }
    let discount = |day: i32| {
        (-r.anchor.original_annual_effective_rate.ln_1p() * f64::from(day - asof) / 365.).exp()
    };
    let original_pv: f64 = original_cash
        .iter()
        .map(|(d, (p, i))| (p + i) * discount(*d))
        .sum();
    if !near(original_pv, r.anchor.original_gross_carrying_value) {
        return Err(
            "complete original physical cash does not reproduce original cost at retained EIR"
                .into(),
        );
    }
    let mut paths = Vec::with_capacity(r.runs.len());
    for run in &r.runs {
        check()?;
        let source = deposit_state::simulate(&run.revised)?;
        check()?;
        source_valid(&source)?;
        let obligations = cash(&source)?;
        if obligations.contains_key(&asof) {
            return Err("day0 cash unsupported by purchase-time revision ownership".into());
        }
        let revised_pv: f64 = obligations
            .iter()
            .map(|(d, (p, i))| (p + i) * discount(*d))
            .sum();
        money(revised_pv)?;
        let future: Vec<_> = obligations
            .iter()
            .map(|(&payment_ordinal, &(p, i))| FutureCash {
                payment_ordinal,
                amount: p + i,
            })
            .collect();
        let mut events = Vec::with_capacity(run.revised.horizon_days as usize + 1);
        events.push(IncomeEvent {
            event_id: "asof_revision".into(),
            source: r.contract.clone(),
            payment_ordinal: asof,
            contractual_interest_earned: 0.,
            principal_cash: 0.,
            interest_cash: 0.,
            integral_fee_cash: 0.,
            nonintegral_fee_cash: 0.,
            yield_change: Some(YieldChange::CatchupOriginal {
                review: r.policy.review.clone(),
                complete_future_cash: future,
            }),
        });
        let earned: BTreeMap<_, _> = source
            .interest
            .iter()
            .map(|x| (x.end_ordinal, x.amount))
            .collect();
        for d in asof + 1..=admission.full_end_ordinal {
            check()?;
            let (p, i) = obligations.get(&d).copied().unwrap_or((0., 0.));
            events.push(IncomeEvent {
                event_id: format!("day_{d}"),
                source: r.contract.clone(),
                payment_ordinal: d,
                contractual_interest_earned: earned.get(&d).copied().unwrap_or(0.),
                principal_cash: p,
                interest_cash: i,
                integral_fee_cash: 0.,
                nonintegral_fee_cash: 0.,
                yield_change: None,
            });
        }
        let income_request = DatedIncomeRequest {
            schema: dated_income::SCHEMA.into(),
            contract: r.contract.clone(),
            original_yield_anchor: r.anchor.original_yield_anchor.clone(),
            policy: r.policy.review.clone(),
            currency: run.revised.identity.currency.clone(),
            monetary_unit: "currency_units".into(),
            measure: CashflowMeasure::Physical,
            as_of_ordinal: asof,
            full_contract_end_ordinal: admission.full_end_ordinal,
            opening_principal: r.anchor.original_principal,
            opening_accrued_interest: 0.,
            opening_gross_carrying_value: r.anchor.original_gross_carrying_value,
            original_annual_effective_rate: r.anchor.original_annual_effective_rate,
            opening_annual_effective_rate: r.anchor.original_annual_effective_rate,
            events,
        };
        check()?;
        let income = dated_income::evaluate(&income_request)?;
        check()?;
        if !near(income.remaining_principal, 0.)
            || !near(income.remaining_accrued_interest, 0.)
            || !near(income.remaining_gross_carrying_value, 0.)
            || !near(income.rows[0].closing_gross_carrying_value, revised_pv)
        {
            return Err("revised original-EIR full-tail/remeasurement conservation failed".into());
        }
        for row in &income.rows {
            for x in [
                row.opening_gross_carrying_value,
                row.closing_principal,
                row.closing_accrued_interest,
                row.closing_gross_carrying_value,
                row.closing_book_adjustment,
                row.basis_amortization,
                row.modification_gain_loss,
                row.effective_interest,
                row.cash_received,
            ] {
                money(x)?;
            }
        }
        let basis_revision = income.rows[0].modification_gain_loss;
        paths.push(Path {
            run_id: run.run_id.clone(),
            scenario_id: run.scenario_id.clone(),
            severity: run.severity,
            source,
            income_request,
            income,
            original_cost: r.anchor.original_gross_carrying_value,
            revised_carrying_value: revised_pv,
            basis_revision,
        });
    }
    let result = ResultData {
        schema: SCHEMA.into(),
        admission,
        original_source,
        original_pv,
        paths,
        accounting_standard_approval: false,
        installed_adoption: false,
        capital_ftp_eligible: false,
    };
    check()?;
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > result.admission.reserved_bytes
    {
        return Err("revised deposit actual source retention exceeds reservation".into());
    }
    check()?;
    Ok(result)
}
