//! European conditional valuation; supplied terminal information, never foresight.
use super::swaps::{value_swap, SwapRequest, SwapResult};
use super::*;
use portfolio_model_core::cashflow::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionSide {
    Long,
    Short,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Settlement {
    CollateralizedCashPrice,
    PhysicalDelivery,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalState {
    pub state_id: String,
    pub probability: f64,
    pub discount_to_expiry: f64,
    pub discount_source: Identity,
    pub underlying: SwapRequest,
    pub delivered_position_id: String,
    pub netting_set_id: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SwaptionRequest {
    pub contract: Identity,
    pub exercise_policy: Identity,
    pub conditional_model: Identity,
    pub crn: Identity,
    pub position_id: String,
    pub scenario_id: String,
    pub currency: String,
    pub monetary_unit: String,
    pub measure: CashflowMeasure,
    pub asof_ordinal: i32,
    pub expiry_ordinal: i32,
    pub settlement_ordinal: i32,
    pub tail_end_ordinal: i32,
    pub side: OptionSide,
    pub settlement: Settlement,
    pub states: Vec<TerminalState>,
}
#[derive(Debug, Clone, Serialize)]
pub struct StateResult {
    pub state_id: String,
    pub probability: f64,
    pub underlying: SwapResult,
    pub intrinsic_value: f64,
    pub exercised: bool,
    pub settlement_cash: f64,
    pub expiry_value: f64,
    pub delivered_position_id: String,
    pub netting_set_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct SwaptionResult {
    pub position_id: String,
    pub contract: Identity,
    pub exercise_policy: Identity,
    pub conditional_model: Identity,
    pub crn: Identity,
    pub clean_value: f64,
    pub schedule: DatedSchedule,
    pub states: Vec<StateResult>,
    pub disclosure: String,
}
pub fn value_european(r: &SwaptionRequest) -> Result<SwaptionResult, String> {
    let mut bytes = 0;
    for id in [
        &r.contract,
        &r.exercise_policy,
        &r.conditional_model,
        &r.crn,
    ] {
        identity(id, &mut bytes)?;
    }
    for s in [&r.position_id, &r.scenario_id] {
        text(s, &mut bytes)?;
    }
    domain(r.asof_ordinal, r.expiry_ordinal)?;
    domain(r.expiry_ordinal, r.settlement_ordinal)?;
    domain(r.asof_ordinal, r.tail_end_ordinal)?;
    if r.measure != CashflowMeasure::Pricing
        || r.monetary_unit != "currency_units"
        || r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
        || r.states.is_empty()
        || r.states.len() > 256
        || r.settlement_ordinal > r.tail_end_ordinal
        || (r.settlement == Settlement::PhysicalDelivery
            && r.settlement_ordinal != r.expiry_ordinal)
    {
        return Err("European conditional pricing requires explicit currency/risk-neutral states; physical delivery occurs at expiry".into());
    }
    let mut probability = 0.;
    let mut value = 0.;
    let mut events = Vec::new();
    let mut states = Vec::new();
    let mut ids = HashSet::new();
    let sign = if r.side == OptionSide::Long { 1. } else { -1. };
    let mut work = 0usize;
    let mut descriptor = None;
    for s in &r.states {
        for t in [&s.state_id, &s.delivered_position_id, &s.netting_set_id] {
            text(t, &mut bytes)?;
        }
        identity(&s.discount_source, &mut bytes)?;
        if !ids.insert(&s.state_id)
            || !s.probability.is_finite()
            || s.probability < 0.
            || !s.discount_to_expiry.is_finite()
            || s.discount_to_expiry <= 0.
            || (r.asof_ordinal == r.expiry_ordinal && s.discount_to_expiry != 1.)
        {
            return Err("invalid terminal probability/discount/identity".into());
        }
        let u = &s.underlying;
        if u.asof_ordinal != r.expiry_ordinal
            || u.scenario_id != r.scenario_id
            || u.currency != r.currency
            || u.monetary_unit != r.monetary_unit
            || u.tail_end_ordinal > r.tail_end_ordinal
            || u.fixed_periods
                .first()
                .is_none_or(|p| p.start_ordinal < r.expiry_ordinal)
        {
            return Err("European underlying must start at/after expiry with complete future tail and matching currency".into());
        }
        work = work
            .checked_add(super::swaps::work(u))
            .ok_or("option work overflow")?;
        if work > 250000 {
            return Err("European raw-leg work exceeds 250000".into());
        }
        let serialized = serde_json::to_vec(u).map_err(|e| e.to_string())?;
        bytes = bytes
            .checked_add(serialized.len())
            .ok_or("option metadata overflow")?;
        if bytes > 32 * 1024 * 1024 {
            return Err("option combined raw metadata exceeds 32 MiB".into());
        }
        let key = super::swaps::contract_key(u)?;
        if descriptor.as_ref().is_some_and(|k| k != &key) {
            return Err("terminal states cannot change underlying contractual economics".into());
        }
        descriptor = Some(key);
        let underlying = value_swap(u)?;
        let intrinsic = underlying.clean_value.max(0.);
        let exercised = intrinsic > 0.;
        let mut cash = 0.;
        // Price at expiry carried at the same supplied collateral discount curve to the delayed settlement date.
        if exercised && r.settlement == Settlement::CollateralizedCashPrice {
            cash = amount(intrinsic / u.discount_curve.at(r.settlement_ordinal)?)?;
            events.push(DatedCashflowEvent {
                position_id: r.position_id.clone(),
                event_id: format!("cash:{}", s.state_id),
                scenario: ScenarioSelector::Named {
                    scenario_id: r.scenario_id.clone(),
                },
                payment_ordinal: r.settlement_ordinal,
                day_offset: (r.settlement_ordinal - r.asof_ordinal) as u32,
                accrual_period: None,
                currency: r.currency.clone(),
                components: CashflowComponents {
                    principal: 0.,
                    cash_interest: amount(sign * s.probability * cash)?,
                    accrued_interest: 0.,
                    book_amortization: 0.,
                    fees: 0.,
                },
                measure: CashflowMeasure::Pricing,
                provenance: EventProvenance::Modeled {
                    model_id: r.conditional_model.id.clone(),
                    model_version: r.conditional_model.revision.clone(),
                    input_revision: r.contract.revision.clone(),
                },
                adjustment: CashflowAdjustment::None {},
            });
        } else if exercised {
            for mut event in underlying.schedule.events.clone() {
                event.position_id = s.delivered_position_id.clone();
                event.event_id = format!("physical:{}:{}", s.state_id, event.event_id);
                event.scenario = ScenarioSelector::Named {
                    scenario_id: r.scenario_id.clone(),
                };
                event.day_offset = (event.payment_ordinal - r.asof_ordinal) as u32;
                event.components.cash_interest =
                    amount(event.components.cash_interest * sign * s.probability)?;
                event.components.accrued_interest =
                    amount(event.components.accrued_interest * sign * s.probability)?;
                events.push(event);
            }
        }
        probability += s.probability;
        value = amount(value + sign * s.probability * s.discount_to_expiry * intrinsic)?;
        states.push(StateResult {
            state_id: s.state_id.clone(),
            probability: s.probability,
            underlying,
            intrinsic_value: intrinsic,
            exercised,
            settlement_cash: cash,
            expiry_value: intrinsic,
            delivered_position_id: s.delivered_position_id.clone(),
            netting_set_id: s.netting_set_id.clone(),
        });
    }
    if !near(probability, 1.) {
        return Err("complete risk-neutral terminal probabilities must sum to one".into());
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
    Ok(SwaptionResult{position_id:r.position_id.clone(),contract:r.contract.clone(),exercise_policy:r.exercise_policy.clone(),conditional_model:r.conditional_model.clone(),crn:r.crn.clone(),clean_value:value,schedule,states,disclosure:"European exercise on conditional swap value at expiry; zero intrinsic expires. Collateralized cash price is carried to settlement by the supplied expiry collateral curve, not a realized-annuity convention. Physical delivery creates mapped future underlying Pricing legs only, with no duplicate intrinsic cash. These are risk-neutral conditional projections, not observed physical elections/settlements. No Bermudan exercise, volatility calibration, market-standard legal settlement algorithm or future information inference.".into()})
}
