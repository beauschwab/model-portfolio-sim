//! Clean collateral-discounted swaps on actual fixed and indexed schedules.
use super::*;
use crate::products::term::contracts::indexed::{
    build_path, ContractPathRequest, ContractPeriod, Coupon, PeriodAudit, PrincipalPlan, RateSource,
};
use portfolio_model_core::cashflow::*;
use portfolio_model_core::floating_rate::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Payer,
    Receiver,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentTiming {
    BeforeScheduledPayments,
    AfterScheduledPayments,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixedPeriod {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub day_count: ActualDayCount,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscountPoint {
    pub ordinal: i32,
    pub discount_factor: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscountCurve {
    pub source: Identity,
    pub collateral_discount_policy: Identity,
    pub currency: String,
    pub asof_ordinal: i32,
    pub information_unix_seconds: i64,
    pub basis: String,
    pub points: Vec<DiscountPoint>,
}
impl DiscountCurve {
    pub fn validate(&self, asof: i32, currency: &str) -> Result<(), String> {
        let mut bytes = 0;
        identity(&self.source, &mut bytes)?;
        identity(&self.collateral_discount_policy, &mut bytes)?;
        if self.asof_ordinal != asof
            || !(0..=253402300799_i64).contains(&self.information_unix_seconds)
            || 719163 + (self.information_unix_seconds / 86400) as i32 > asof
            || self.currency != currency
            || self.basis != "collateralized_clean"
            || self.points.is_empty()
            || self.points.len() > 8192
        {
            return Err(
                "clean discount curve requires exact asof/currency/policy and bounded points"
                    .into(),
            );
        }
        let mut prior = None;
        for p in &self.points {
            domain(asof, p.ordinal)?;
            if !p.discount_factor.is_finite()
                || p.discount_factor <= 0.
                || prior.is_some_and(|d| d >= p.ordinal)
            {
                return Err("invalid/nonchronological discount points".into());
            }
            prior = Some(p.ordinal);
        }
        if self.points[0].ordinal != asof || self.points[0].discount_factor != 1. {
            return Err("discount anchor must equal exactly one at asof".into());
        }
        Ok(())
    }
    pub fn validate_at(&self, asof: i32, currency: &str, cutoff: i64) -> Result<(), String> {
        self.validate(asof, currency)?;
        if self.information_unix_seconds > cutoff {
            return Err("collateral discount information follows exact valuation cutoff".into());
        }
        Ok(())
    }
    pub fn at(&self, date: i32) -> Result<f64, String> {
        self.points
            .binary_search_by_key(&date, |p| p.ordinal)
            .map(|i| self.points[i].discount_factor)
            .map_err(|_| {
                "missing exact payment discount; no curve interpolation or tail inference".into()
            })
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SwapRequest {
    pub contract: Identity,
    pub index_basis: Identity,
    pub rate_path: Identity,
    pub position_id: String,
    pub scenario_id: String,
    pub currency: String,
    pub monetary_unit: String,
    pub asof_ordinal: i32,
    pub valuation_cutoff_unix_seconds: i64,
    pub tail_end_ordinal: i32,
    pub payment_timing: PaymentTiming,
    pub side: Side,
    pub notional: f64,
    pub fixed_rate: f64,
    pub fixed_periods: Vec<FixedPeriod>,
    pub floating_periods: Vec<ContractPeriod>,
    pub discount_curve: DiscountCurve,
}
#[derive(Debug, Clone, Serialize)]
pub struct SwapResult {
    pub position_id: String,
    pub contract: Identity,
    pub index_basis: Identity,
    pub rate_path: Identity,
    pub collateral_discount_source: Identity,
    pub fixed_leg_pv: f64,
    pub floating_leg_pv: f64,
    pub fixed_annuity: f64,
    pub par_rate: Option<f64>,
    pub clean_value: f64,
    pub schedule: DatedSchedule,
    pub floating_audit: Vec<PeriodAudit>,
    pub disclosure: String,
}
pub fn active(payment: i32, asof: i32, timing: PaymentTiming) -> bool {
    if timing == PaymentTiming::BeforeScheduledPayments {
        payment >= asof
    } else {
        payment > asof
    }
}
fn validate_period_dates(
    start: i32,
    end: i32,
    pay: i32,
    asof: i32,
    tail: i32,
) -> Result<(), String> {
    domain(start, end)?;
    validate_ordinal(pay)?;
    if end == start || end > tail || pay > tail || i64::from(start) < i64::from(asof) - 36600 {
        return Err("swap actual dates exceed complete term domain".into());
    }
    Ok(())
}
pub fn value_swap(r: &SwapRequest) -> Result<SwapResult, String> {
    let mut bytes = 0;
    for id in [&r.contract, &r.index_basis, &r.rate_path] {
        identity(id, &mut bytes)?;
    }
    text(&r.position_id, &mut bytes)?;
    text(&r.scenario_id, &mut bytes)?;
    domain(r.asof_ordinal, r.tail_end_ordinal)?;
    nonnegative(r.notional)?;
    if r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
        || r.monetary_unit != "currency_units"
        || !r.fixed_rate.is_finite()
        || !(0..=253402300799_i64).contains(&r.valuation_cutoff_unix_seconds)
        || 719163 + (r.valuation_cutoff_unix_seconds / 86400) as i32 != r.asof_ordinal
        || r.fixed_periods.is_empty()
        || r.floating_periods.is_empty()
        || r.fixed_periods.len() > 4096
        || r.floating_periods.len() > 4096
    {
        return Err("swap requires exact UTC valuation date, actual currency, bounded explicit legs and finite rate".into());
    }
    if work(r) > 250000 {
        return Err("swap combined raw-leg work exceeds 250000".into());
    }
    if serde_json::to_vec(r).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("swap immutable raw metadata exceeds 32 MiB".into());
    }
    r.discount_curve
        .validate_at(r.asof_ordinal, &r.currency, r.valuation_cutoff_unix_seconds)?;
    let mut fixed_annuity = 0.;
    let mut fixed_pv = 0.;
    let mut events = Vec::new();
    let mut ids = HashSet::new();
    let mut prior = None;
    let fixed_sign = if r.side == Side::Receiver { 1. } else { -1. };
    for p in &r.fixed_periods {
        text(&p.period_id, &mut bytes)?;
        validate_period_dates(
            p.start_ordinal,
            p.end_ordinal,
            p.payment_ordinal,
            r.asof_ordinal,
            r.tail_end_ordinal,
        )?;
        if !ids.insert(&p.period_id) || prior.is_some_and(|e| e != p.start_ordinal) {
            return Err("fixed swap periods must be unique and contiguous".into());
        }
        prior = Some(p.end_ordinal);
        if active(p.payment_ordinal, r.asof_ordinal, r.payment_timing) {
            let tau = actual_year_fraction(p.start_ordinal, p.end_ordinal, p.day_count)?;
            let interest = amount(r.notional * r.fixed_rate * tau)?;
            let df = r.discount_curve.at(p.payment_ordinal)?;
            fixed_annuity = amount(fixed_annuity + r.notional * tau * df)?;
            fixed_pv = amount(fixed_pv + interest * df)?;
            events.push(DatedCashflowEvent {
                position_id: r.position_id.clone(),
                event_id: format!("fixed:{}", p.period_id),
                scenario: ScenarioSelector::Named {
                    scenario_id: r.scenario_id.clone(),
                },
                payment_ordinal: p.payment_ordinal,
                day_offset: (p.payment_ordinal - r.asof_ordinal) as u32,
                accrual_period: Some(AccrualPeriod {
                    start_ordinal: p.start_ordinal,
                    end_ordinal: p.end_ordinal,
                }),
                currency: r.currency.clone(),
                components: CashflowComponents {
                    principal: 0.,
                    cash_interest: fixed_sign * interest,
                    accrued_interest: fixed_sign * interest,
                    book_amortization: 0.,
                    fees: 0.,
                },
                measure: CashflowMeasure::Pricing,
                provenance: EventProvenance::Modeled {
                    model_id: "derivative-clean-swap".into(),
                    model_version: MODEL_VERSION.into(),
                    input_revision: r.contract.revision.clone(),
                },
                adjustment: CashflowAdjustment::None {},
            });
        }
    }
    let mut float_periods = Vec::new();
    let mut prior = None;
    let mut ids = HashSet::new();
    let mut index_work = 0;
    for p in &r.floating_periods {
        validate_period_dates(
            p.start_ordinal,
            p.end_ordinal,
            p.payment_ordinal,
            r.asof_ordinal,
            r.tail_end_ordinal,
        )?;
        text(&p.period_id, &mut bytes)?;
        if !ids.insert(&p.period_id) || prior.is_some_and(|e| e != p.start_ordinal) || p.fee != 0. {
            return Err("floating swap periods must be contiguous, unique and fee-free".into());
        }
        prior = Some(p.end_ordinal);
        let observations =
            match &p.coupon {
                Coupon::Indexed { observations, .. } => observations,
                _ => return Err(
                    "floating swap requires explicit indexed coupon, not a fixed or short proxy"
                        .into(),
                ),
            };
        index_work += observations.len();
        if index_work > 250000 {
            return Err("swap observation work exceeds 250000".into());
        }
        if active(p.payment_ordinal, r.asof_ordinal, r.payment_timing) {
            for o in observations {
                if let RateSource::ConditionalProjection {
                    information_unix_seconds,
                    ..
                } = &o.source
                {
                    if *information_unix_seconds > r.valuation_cutoff_unix_seconds {
                        return Err(
                            "clean swap valuation cannot use future conditional information".into(),
                        );
                    }
                }
            }
            float_periods.push(p.clone());
        }
    }
    if r.fixed_periods.last().map(|p| p.end_ordinal)
        != r.floating_periods.last().map(|p| p.end_ordinal)
        || r.fixed_periods.first().map(|p| p.start_ordinal)
            != r.floating_periods.first().map(|p| p.start_ordinal)
    {
        return Err("fixed/float swap contractual start/maturity mismatch".into());
    }
    let mut float_pv = 0.;
    let mut floating_audit = Vec::new();
    if !float_periods.is_empty() {
        let mut payments = vec![0.; float_periods.len()];
        *payments.last_mut().ok_or("floating leg empty")? = r.notional;
        let path = build_path(&ContractPathRequest {
            schema: crate::products::term::contracts::SCHEMA.into(),
            model_version: crate::products::term::contracts::MODEL_VERSION.into(),
            contract: r.contract.clone(),
            assumption: r.index_basis.clone(),
            path: r.rate_path.clone(),
            position_id: r.position_id.clone(),
            scenario_id: r.scenario_id.clone(),
            currency: r.currency.clone(),
            monetary_unit: r.monetary_unit.clone(),
            direction: Direction::Receipt,
            measure: CashflowMeasure::Pricing,
            asof_ordinal: r.asof_ordinal,
            reporting_end_ordinal: r.asof_ordinal,
            tail_end_ordinal: r.tail_end_ordinal,
            observed_publication_cutoff_unix_seconds: r.valuation_cutoff_unix_seconds,
            opening_principal: r.notional,
            principal_plan: PrincipalPlan::Supplied {
                principal_payments: payments,
            },
            periods: float_periods,
        })?;
        floating_audit = path.audit;
        for mut e in path.schedule.events {
            float_pv = amount(
                float_pv + e.components.cash_interest * r.discount_curve.at(e.payment_ordinal)?,
            )?;
            e.components.principal = 0.;
            e.components.cash_interest *= -fixed_sign;
            e.components.accrued_interest *= -fixed_sign;
            e.event_id = format!("floating:{}", e.event_id);
            events.push(e);
        }
    }
    events.sort_by_key(|e| e.payment_ordinal);
    let schedule = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: r.asof_ordinal,
        horizon_days: (r.tail_end_ordinal - r.asof_ordinal) as u32,
        scenarios: vec![r.scenario_id.clone()],
        events,
    };
    schedule.validate()?;
    let clean = amount(fixed_sign * (fixed_pv - float_pv))?;
    if !near(clean, discounted_schedule(&r.discount_curve, &schedule)?) {
        return Err("swap clean PV does not reconcile dated cashflow schedule".into());
    }
    Ok(SwapResult{position_id:r.position_id.clone(),contract:r.contract.clone(),index_basis:r.index_basis.clone(),rate_path:r.rate_path.clone(),collateral_discount_source:r.discount_curve.source.clone(),fixed_leg_pv:fixed_pv,floating_leg_pv:float_pv,fixed_annuity,par_rate:if fixed_annuity>0. {Some(float_pv/fixed_annuity)} else {None},clean_value:clean,schedule,floating_audit,disclosure:"Clean constant-notional swap: actual dated fixed/indexed legs and separately supplied collateral discount/index basis. Projected coupon information cannot follow valuation cutoff. No notional exchange, OAS recalibration, XVA, accounting designation, curve interpolation or future rate-path generation inferred. Same-day paid coupons follow explicit before/after timing; indexed calendar mapping is supplied/unverified.".into()})
}

pub(crate) fn discounted_schedule(
    curve: &DiscountCurve,
    schedule: &DatedSchedule,
) -> Result<f64, String> {
    let mut value = 0.;
    for e in &schedule.events {
        value = amount(value + curve.at(e.payment_ordinal)? * e.components.cash_total()?)?;
    }
    Ok(value)
}

/// Stable contractual descriptor excludes scenario rates, curves and valuation anchors.
pub(crate) fn contract_key(r: &SwapRequest) -> Result<String, String> {
    let fixed: Vec<_> = r
        .fixed_periods
        .iter()
        .map(|p| {
            (
                &p.period_id,
                p.start_ordinal,
                p.end_ordinal,
                p.payment_ordinal,
                p.day_count,
            )
        })
        .collect();
    let mut floating = Vec::new();
    for p in &r.floating_periods {
        let mut coupon = serde_json::to_value(&p.coupon).map_err(|e| e.to_string())?;
        if let Some(object) = coupon.as_object_mut() {
            object.remove("observations");
        }
        floating.push((
            &p.period_id,
            p.start_ordinal,
            p.end_ordinal,
            p.payment_ordinal,
            p.day_count,
            coupon,
        ));
    }
    serde_json::to_string(&(
        &r.contract,
        &r.index_basis,
        &r.currency,
        &r.monetary_unit,
        r.side,
        r.notional,
        r.fixed_rate,
        fixed,
        floating,
    ))
    .map_err(|e| e.to_string())
}
pub(crate) fn work(r: &SwapRequest) -> usize {
    r.floating_periods
        .iter()
        .fold(r.fixed_periods.len(), |total, p| {
            total.saturating_add(1).saturating_add(match &p.coupon {
                Coupon::Indexed { observations, .. } => observations.len(),
                _ => 0,
            })
        })
}
