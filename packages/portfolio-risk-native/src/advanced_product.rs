//! Reusable exact dated fixed-contract projection and remaining-right valuation.
//! State-dependent product conditioning is deliberately not fabricated here.
use crate::conventions::{Bdc, Calendar, CalendarSpec, Date, DayCount};
use crate::dated_term::{
    AccrualDates, CashflowDirection, DatedTermPosition, DatedTermRequest, TermPeriodAudit,
    DATED_TERM_SCHEMA,
};
use crate::model_market::curves::{CurveRole, SuppliedCurve};
use crate::term_deck::{Amortization, Contract, Product};
use portfolio_model_core::cashflow::{DatedCashflowEvent, MAX_HORIZON_DAYS};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FixedTermDefinition {
    pub product: Product,
    pub term_months: i32,
    pub freq_months: i32,
    pub daycount: DayCount,
    pub coupon: f64,
    pub amortization: Amortization,
    pub calendar: CalendarSpec,
    pub bdc: Bdc,
    pub accrual_dates: AccrualDates,
}
/// Produced only by native contract construction; no deserialize/prepared-flow input.
pub struct ContractualProjection {
    pub purchase_ordinal: i32,
    pub currency: String,
    pub full_end_ordinal: i32,
    pub events: Vec<DatedCashflowEvent>,
    pub periods: Vec<TermPeriodAudit>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub id: String,
    pub revision: String,
}
pub fn project_fixed(
    definition: &FixedTermDefinition,
    purchase: i32,
    id: &str,
    currency: &str,
    source: &SourceIdentity,
) -> Result<ContractualProjection, String> {
    for s in [id, currency, &source.id, &source.revision] {
        label(s)?;
    }
    if !(1..=1200).contains(&definition.term_months)
        || !(0..=120).contains(&definition.freq_months)
        || !definition.coupon.is_finite()
        || !(-1. ..=2.).contains(&definition.coupon)
        || matches!(definition.amortization, Amortization::Sink)
        || (definition.product == Product::Corporate && definition.freq_months == 0)
        || (definition.product == Product::Cd && definition.amortization != Amortization::Bullet)
    {
        return Err(
            "fixed projection requires bounded nonoptional terms; sinking policy unavailable"
                .into(),
        );
    }
    let date = Date::ordinal(purchase)?;
    let maturity = date.add_months(definition.term_months)?.number();
    let adjusted = Calendar::new(&definition.calendar)?
        .adjust(Date::ordinal(maturity)?, definition.bdc)?
        .number();
    let end = maturity.max(adjusted);
    let horizon = u32::try_from(end - purchase).map_err(|_| "invalid full contract horizon")?;
    if horizon > MAX_HORIZON_DAYS {
        return Err("full fixed term horizon exceeds admitted dates".into());
    }
    let r = DatedTermRequest {
        schema: DATED_TERM_SCHEMA.into(),
        product: definition.product,
        asof: purchase,
        horizon_days: horizon,
        calendar: definition.calendar.clone(),
        bdc: definition.bdc,
        accrual_dates: definition.accrual_dates,
        scenarios: vec!["contractual".into()],
        positions: vec![DatedTermPosition {
            position_id: id.into(),
            currency: currency.into(),
            source_id: source.id.clone(),
            source_revision: source.revision.clone(),
            accrual_anchor_ordinal: purchase,
            direction: CashflowDirection::Receipt,
            contract: Contract {
                maturity,
                freq_months: Some(definition.freq_months),
                daycount: definition.daycount,
                coupon: definition.coupon,
                notional: 1.,
                price: 100.,
                is_float: false,
                cap: 10.,
                floor: -10.,
                call_threshold: 0.005,
                amort_type: definition.amortization,
                sink_schedule: None,
                call_schedule: None,
                put_schedule: None,
                channel: "explicit-fixed-contract".into(),
                penalty_months: 0.,
                ew_mult: 0.,
            },
        }],
    }
    .build()?;
    Ok(ContractualProjection {
        purchase_ordinal: purchase,
        currency: currency.into(),
        full_end_ordinal: end,
        events: r.schedule.events,
        periods: r.periods,
    })
}
pub fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("advanced product identity must be bounded nonblank text".into())
    } else {
        Ok(())
    }
}
pub fn amount(v: f64) -> Result<f64, String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        Err("advanced product monetary admission exceeded".into())
    } else {
        Ok(v)
    }
}
fn validate_curve(
    p: &ContractualProjection,
    asof: i32,
    at: i32,
    c: &SuppliedCurve,
    oas: f64,
    bump: f64,
) -> Result<(), String> {
    Date::ordinal(asof)?;
    Date::ordinal(at)?;
    if at < p.purchase_ordinal
        || p.purchase_ordinal < asof
        || c.currency != p.currency
        || !matches!(c.role, CurveRole::Collateral { .. })
        || !oas.is_finite()
        || !(-5. ..=5.).contains(&oas)
        || !bump.is_finite()
        || bump.abs() > 1.
    {
        return Err("remaining-right valuation identity/date/curve/rate mismatch".into());
    }
    c.validate()?;
    Ok(())
}
fn log_price(
    p: &ContractualProjection,
    asof: i32,
    at: i32,
    c: &SuppliedCurve,
    oas: f64,
    bump: f64,
) -> Result<Option<f64>, String> {
    if !p.events.iter().any(|e| e.payment_ordinal > at) {
        return Ok(None);
    }
    let start_log = c
        .value_validated((at - asof) as f64 / 365.)?
        .discount_factor
        .ln();
    let mut log_sum: Option<f64> = None;
    for e in p.events.iter().filter(|e| e.payment_ordinal > at) {
        let cash = e.components.cash_total()?;
        if cash < 0. {
            return Err("positive-right OAS calibration does not admit signed coupon cash; use a separate validated model".into());
        }
        if cash == 0. {
            continue;
        }
        let log = cash.ln()
            + c.value_validated((e.payment_ordinal - asof) as f64 / 365.)?
                .discount_factor
                .ln()
            - start_log
            - (oas + bump) * (e.payment_ordinal - at) as f64 / 365.;
        log_sum = Some(match log_sum {
            None => log,
            Some(old) => {
                let m = old.max(log);
                m + ((old - m).exp() + (log - m).exp()).ln()
            }
        });
    }
    Ok(log_sum)
}
/// Closing-after-settlement mark: cash paid exactly at `at` has retired.
pub fn price_remaining(
    p: &ContractualProjection,
    curve_asof: i32,
    at: i32,
    curve: &SuppliedCurve,
    fixed_oas: f64,
    zero_rate_bump: f64,
) -> Result<f64, String> {
    portfolio_compute_control::checkpoint()?;
    validate_curve(p, curve_asof, at, curve, fixed_oas, zero_rate_bump)?;
    amount(log_price(p, curve_asof, at, curve, fixed_oas, zero_rate_bump)?.map_or(0., f64::exp))
}
/// Solve once at purchase; all ages/scenarios retain this base spread.
pub fn solve_purchase_oas(
    p: &ContractualProjection,
    curve_asof: i32,
    base: &SuppliedCurve,
    quote: f64,
) -> Result<f64, String> {
    if amount(quote)? <= 0. {
        return Err("positive purchase quote required".into());
    }
    validate_curve(p, curve_asof, p.purchase_ordinal, base, 0., 0.)?;
    let target = quote.ln();
    let f = |s| {
        log_price(p, curve_asof, p.purchase_ordinal, base, s, 0.)?
            .ok_or_else(|| "purchase has no positive remaining cash rights".to_string())
    };
    let (mut lo, mut hi) = (-5., 5.);
    if f(lo)? < target || f(hi)? > target {
        return Err("purchase quote outside unique admitted OAS bracket".into());
    }
    for _ in 0..100 {
        let mid = (lo + hi) / 2.;
        if f(mid)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
#[derive(Clone, Serialize)]
pub struct ReplayedBookState {
    pub ordinal: i32,
    pub principal: f64,
    pub gross_carrying_value: f64,
    pub cash_paid_to_date: f64,
}
/// Native purchase-to-age cash replay at immutable effective yield, never refitted.
pub fn replay_book(
    p: &ContractualProjection,
    at: i32,
    quote: f64,
    original_yield: f64,
) -> Result<ReplayedBookState, String> {
    Date::ordinal(at)?;
    if at < p.purchase_ordinal
        || !original_yield.is_finite()
        || !(-0.99..=10.).contains(&original_yield)
        || amount(quote)? <= 0.
    {
        return Err("invalid frozen-yield book replay".into());
    }
    let mut carrying = quote;
    let mut principal = 1.;
    let mut paid = 0.;
    let mut day = p.purchase_ordinal;
    for e in p.events.iter().filter(|e| e.payment_ordinal <= at) {
        let cash = e.components.cash_total()?;
        carrying = amount(
            carrying * (original_yield.ln_1p() * (e.payment_ordinal - day) as f64 / 365.).exp()
                - cash,
        )?;
        principal -= e.components.principal;
        paid = amount(paid + cash)?;
        day = e.payment_ordinal;
    }
    carrying = amount(carrying * (original_yield.ln_1p() * (at - day) as f64 / 365.).exp())?;
    // Source cash has retired; only a numerical terminal residual may be cleared.
    let cash_scale = p
        .events
        .iter()
        .map(|e| e.components.principal.abs() + e.components.cash_interest.abs())
        .sum::<f64>()
        .max(1.);
    if at >= p.full_end_ordinal && carrying.abs() <= 1e-10 * cash_scale {
        carrying = 0.;
    }
    if principal.abs() < 1e-12 {
        principal = 0.;
    }
    if carrying < -1e-10 || principal < -1e-12 {
        return Err("replayed principal/book basis does not conserve purchase rights".into());
    }
    Ok(ReplayedBookState {
        ordinal: at,
        principal,
        gross_carrying_value: carrying.max(0.),
        cash_paid_to_date: paid,
    })
}
pub fn validate_original_yield(
    p: &ContractualProjection,
    quote: f64,
    y: f64,
) -> Result<(), String> {
    // Validate in inception units as well as terminal units: a very large
    // premium must not let a quote-scaled terminal tolerance hide missing cash.
    replay_book(p, p.purchase_ordinal, quote, y)?;
    let mut expected = 0.;
    for event in &p.events {
        let cash = event.components.cash_total()?;
        if cash < 0. {
            return Err(
                "positive-right original-yield validation requires nonnegative contractual cash"
                    .into(),
            );
        }
        expected = amount(
            expected
                + cash
                    * (-y.ln_1p() * (event.payment_ordinal - p.purchase_ordinal) as f64 / 365.)
                        .exp(),
        )?;
    }
    if (expected - quote).abs() > 1e-10 * quote.max(1.) {
        return Err(
            "original yield does not reconcile inception quote to complete contractual cash".into(),
        );
    }
    let end = replay_book(p, p.full_end_ordinal, quote, y)?;
    if end.gross_carrying_value != 0. {
        return Err("supplied original yield does not reconcile complete contractual cash to purchase quote".into());
    }
    Ok(())
}
