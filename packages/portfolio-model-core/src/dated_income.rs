//! Actual-date effective-yield rollforward with explicit reviewed changes.
//! Reviewed economic policies are supplied, not inferred accounting standards.
use crate::cashflow::{validate_ordinal, CashflowMeasure, MAX_HORIZON_DAYS};
use crate::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "dated-income-1";
pub const MAX_EVENTS: usize = 4096;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FutureCash {
    pub payment_ordinal: i32,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum YieldChange {
    /// Reviewed prospective annual effective rate; no seasoned-loan refit.
    Prospective {
        review: Identity,
        annual_effective_rate: f64,
    },
    /// Complete revised cash rights discounted at the frozen original yield.
    CatchupOriginal {
        review: Identity,
        complete_future_cash: Vec<FutureCash>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeEvent {
    pub event_id: String,
    pub source: Identity,
    pub payment_ordinal: i32,
    /// Earned simple contractual interest in the preceding supplied interval.
    pub contractual_interest_earned: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    /// Integral fee receipts reduce carrying value, separately from cash coupon.
    pub integral_fee_cash: f64,
    /// Reviewed nonintegral fee receipts are recognized separately as fee income.
    pub nonintegral_fee_cash: f64,
    /// Applies after today's recognition and settlement, in this exact order.
    pub yield_change: Option<YieldChange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatedIncomeRequest {
    pub schema: String,
    pub contract: Identity,
    pub original_yield_anchor: Identity,
    pub policy: Identity,
    pub currency: String,
    pub monetary_unit: String,
    pub measure: CashflowMeasure,
    pub as_of_ordinal: i32,
    pub full_contract_end_ordinal: i32,
    pub opening_principal: f64,
    pub opening_accrued_interest: f64,
    pub opening_gross_carrying_value: f64,
    pub original_annual_effective_rate: f64,
    /// A seasoned floating contract supplies its reviewed current rate explicitly.
    pub opening_annual_effective_rate: f64,
    pub events: Vec<IncomeEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct IncomeRow {
    pub event_id: String,
    pub source: Identity,
    pub payment_ordinal: i32,
    pub opening_gross_carrying_value: f64,
    pub annual_effective_rate: f64,
    pub effective_interest: f64,
    pub contractual_interest_earned: f64,
    pub basis_amortization: f64,
    pub modification_gain_loss: f64,
    pub recognized_nonintegral_fee_income: f64,
    pub cash_received: f64,
    pub closing_principal: f64,
    pub closing_accrued_interest: f64,
    pub closing_gross_carrying_value: f64,
    pub closing_book_adjustment: f64,
    pub yield_review: Option<Identity>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DatedIncomeResult {
    pub schema: String,
    pub contract: Identity,
    pub policy: Identity,
    pub original_yield_anchor: Identity,
    pub currency: String,
    pub monetary_unit: String,
    pub measure: CashflowMeasure,
    pub rows: Vec<IncomeRow>,
    pub remaining_principal: f64,
    pub remaining_accrued_interest: f64,
    pub remaining_gross_carrying_value: f64,
}
fn amount(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("dated income amount exceeds finite currency-unit bounds".into())
    } else {
        Ok(())
    }
}
fn rate(x: f64) -> Result<(), String> {
    if !x.is_finite() || !(-0.99..=10.).contains(&x) {
        Err("annual effective yield requires -0.99..10".into())
    } else {
        Ok(())
    }
}
fn identity(x: &Identity, bytes: &mut usize) -> Result<(), String> {
    for s in [&x.id, &x.revision] {
        *bytes = bytes.saturating_add(s.len());
        if s.trim().is_empty()
            || s.len() > 256
            || s.chars().any(char::is_control)
            || *bytes > MAX_DATED_METADATA_BYTES
        {
            return Err("dated income identities exceed bounded metadata domain".into());
        }
    }
    Ok(())
}
pub fn evaluate(r: &DatedIncomeRequest) -> Result<DatedIncomeResult, String> {
    if r.schema != SCHEMA
        || r.monetary_unit != "currency_units"
        || r.measure == CashflowMeasure::Pricing
        || r.currency.len() != 3
        || !r.currency.bytes().all(|x| x.is_ascii_uppercase())
        || r.events.len() > MAX_EVENTS
    {
        return Err(
            "explicit physical/contractual dated-income schema, units and bounded events required"
                .into(),
        );
    }
    validate_ordinal(r.as_of_ordinal)?;
    validate_ordinal(r.full_contract_end_ordinal)?;
    if r.full_contract_end_ordinal < r.as_of_ordinal
        || (r.full_contract_end_ordinal - r.as_of_ordinal) as u32 > MAX_HORIZON_DAYS
    {
        return Err("dated income requires complete bounded contract dates".into());
    }
    rate(r.original_annual_effective_rate)?;
    rate(r.opening_annual_effective_rate)?;
    let mut bytes = 0;
    identity(&r.contract, &mut bytes)?;
    identity(&r.policy, &mut bytes)?;
    identity(&r.original_yield_anchor, &mut bytes)?;
    for x in [
        r.opening_principal,
        r.opening_accrued_interest,
        r.opening_gross_carrying_value,
    ] {
        amount(x)?;
    }
    if r.opening_principal < 0. || r.opening_gross_carrying_value < 0. {
        return Err("negative principal or carrying value".into());
    }
    let mut date = r.as_of_ordinal;
    let mut carrying = r.opening_gross_carrying_value;
    let mut principal = r.opening_principal;
    let mut accrued = r.opening_accrued_interest;
    let mut effective_rate = r.opening_annual_effective_rate;
    let mut rows = Vec::with_capacity(r.events.len());
    let mut ids = HashSet::new();
    for (index, e) in r.events.iter().enumerate() {
        identity(&e.source, &mut bytes)?;
        if e.event_id.trim().is_empty()
            || e.event_id.len() > 256
            || e.event_id.chars().any(char::is_control)
            || !ids.insert(&e.event_id)
        {
            return Err("dated income event identities must be unique and bounded".into());
        }
        bytes = bytes.saturating_add(e.event_id.len());
        if bytes > MAX_DATED_METADATA_BYTES {
            return Err("dated income metadata exceeds admission".into());
        }
        validate_ordinal(e.payment_ordinal)?;
        if e.payment_ordinal < date || e.payment_ordinal > r.full_contract_end_ordinal {
            return Err("dated income events must be ordered within full contract tail".into());
        }
        for x in [
            e.contractual_interest_earned,
            e.principal_cash,
            e.interest_cash,
            e.integral_fee_cash,
            e.nonintegral_fee_cash,
        ] {
            amount(x)?;
        }
        if e.principal_cash < 0. || e.principal_cash > principal {
            return Err(
                "principal repayment exceeds source outstanding; draws need separate ownership"
                    .into(),
            );
        }
        let opening = carrying;
        let used_rate = effective_rate;
        let earned = carrying
            * ((effective_rate.ln_1p() * (e.payment_ordinal - date) as f64 / 365.).exp_m1());
        amount(earned)?;
        let amort = earned - e.contractual_interest_earned - e.integral_fee_cash;
        amount(amort)?;
        carrying += earned - e.principal_cash - e.interest_cash - e.integral_fee_cash;
        principal -= e.principal_cash;
        accrued += e.contractual_interest_earned - e.interest_cash;
        let mut modification = 0.;
        let mut review = None;
        if let Some(change) = &e.yield_change {
            match change {
                YieldChange::Prospective {
                    review: source,
                    annual_effective_rate,
                } => {
                    identity(source, &mut bytes)?;
                    rate(*annual_effective_rate)?;
                    effective_rate = *annual_effective_rate;
                    review = Some(source.clone());
                }
                YieldChange::CatchupOriginal {
                    review: source,
                    complete_future_cash,
                } => {
                    identity(source, &mut bytes)?;
                    review = Some(source.clone());
                    if complete_future_cash.is_empty() || complete_future_cash.len() > MAX_EVENTS {
                        return Err("catchup requires complete bounded future cash rights".into());
                    }
                    // A revised cashflow assumption must describe this same admitted
                    // source schedule, not an unrelated valuation input.
                    let mut expected = std::collections::BTreeMap::<i32, f64>::new();
                    let mut supplied = std::collections::BTreeMap::<i32, f64>::new();
                    for future in &r.events[index + 1..] {
                        let cash =
                            future.principal_cash + future.interest_cash + future.integral_fee_cash;
                        if cash != 0. {
                            *expected.entry(future.payment_ordinal).or_default() += cash;
                        }
                    }
                    for future in complete_future_cash {
                        if future.amount != 0. {
                            *supplied.entry(future.payment_ordinal).or_default() += future.amount;
                        }
                    }
                    if expected.len() != supplied.len()
                        || expected.iter().any(|(date, value)| {
                            supplied.get(date).is_none_or(|x| {
                                !x.is_finite()
                                    || (*x - *value).abs() > 1e-9_f64.max(1e-12 * value.abs())
                            })
                        })
                    {
                        return Err("catchup rights differ from the complete remaining admitted cash schedule".into());
                    }
                    let mut last = e.payment_ordinal;
                    let mut value = 0.;
                    for cash in complete_future_cash {
                        validate_ordinal(cash.payment_ordinal)?;
                        amount(cash.amount)?;
                        if cash.payment_ordinal <= e.payment_ordinal
                            || cash.payment_ordinal < last
                            || cash.payment_ordinal > r.full_contract_end_ordinal
                        {
                            return Err("catchup cash rights must be ordered strictly future within full tail".into());
                        }
                        value += cash.amount
                            * (-r.original_annual_effective_rate.ln_1p()
                                * (cash.payment_ordinal - e.payment_ordinal) as f64
                                / 365.)
                                .exp();
                        amount(value)?;
                        last = cash.payment_ordinal;
                    }
                    modification = value - carrying;
                    carrying = value;
                    effective_rate = r.original_annual_effective_rate;
                }
            }
        }
        let cash =
            e.principal_cash + e.interest_cash + e.integral_fee_cash + e.nonintegral_fee_cash;
        for x in [carrying, principal, accrued, cash, modification] {
            amount(x)?;
        }
        let roundoff = 64. * f64::EPSILON * (opening.abs() + cash.abs() + earned.abs());
        if carrying < 0.
            && principal == 0.
            && accrued.abs() <= roundoff
            && carrying.abs() <= roundoff
        {
            // Retain the independently accumulated accrued right and its offsetting
            // book basis; only the existing terminal carrying roundoff is clamped.
            carrying = 0.;
        }
        if carrying < 0. {
            return Err(
                "negative gross carrying value; explicit recognition/writeoff policy required"
                    .into(),
            );
        }
        let basis = carrying - principal - accrued;
        amount(basis)?;
        rows.push(IncomeRow {
            event_id: e.event_id.clone(),
            source: e.source.clone(),
            payment_ordinal: e.payment_ordinal,
            opening_gross_carrying_value: opening,
            annual_effective_rate: used_rate,
            effective_interest: earned,
            contractual_interest_earned: e.contractual_interest_earned,
            basis_amortization: amort,
            modification_gain_loss: modification,
            recognized_nonintegral_fee_income: e.nonintegral_fee_cash,
            cash_received: cash,
            closing_principal: principal,
            closing_accrued_interest: accrued,
            closing_gross_carrying_value: carrying,
            closing_book_adjustment: basis,
            yield_review: review,
        });
        date = e.payment_ordinal;
    }
    Ok(DatedIncomeResult {
        schema: SCHEMA.into(),
        contract: r.contract.clone(),
        policy: r.policy.clone(),
        original_yield_anchor: r.original_yield_anchor.clone(),
        currency: r.currency.clone(),
        monetary_unit: r.monetary_unit.clone(),
        measure: r.measure,
        rows,
        remaining_principal: principal,
        remaining_accrued_interest: accrued,
        remaining_gross_carrying_value: carrying,
    })
}
