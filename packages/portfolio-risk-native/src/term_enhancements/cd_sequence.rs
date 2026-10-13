//! Chronological CD elections with exclusive future-tail replacement.
//!
//! All financial accrual, allocation, penalty and renewal math is owned by the
//! existing native contractual step. This owner retains live states and joins
//! actual election settlements with only the final unsuperseded bullet tails.
use crate::term_enhancements::{cd::*, Identity, MODEL_VERSION, SCHEMA as STEP_SCHEMA};
use portfolio_compute_control::Control;
use portfolio_model_core::cashflow::{
    CashflowMeasure, DatedCashflowEvent, DatedSchedule, DATED_CASHFLOW_SCHEMA,
};
use portfolio_model_core::floating_rate::Direction;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "cd-election-sequence-2";
pub const MAX_ACTIONS: usize = 256;
pub const MAX_LIVE: usize = 256;
pub const MAX_RAW_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyBinding {
    pub position_id: String,
    pub contract: Identity,
    pub policy: CdPolicy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub action_id: String,
    pub position_id: String,
    pub ordinal: i32,
    pub source: Identity,
    pub election: Election,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub input_revision: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub scenario_id: String,
    pub as_of_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub tail_end_ordinal: i32,
    pub opening: CdState,
    pub terminal_projection_source: Identity,
    /// Exact identity-bound terms of every original/renewed/switched contract.
    pub policies: Vec<PolicyBinding>,
    /// Stable supplied order is the declared same-date election precedence.
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActionReceipt {
    pub action: Action,
    pub opening: CdState,
    pub native: CdElectionResult,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResultData {
    pub schema: String,
    /// Declared native DTO. The CLI separately retains the original JSON Value.
    pub admitted_request: Request,
    pub action_receipts: Vec<ActionReceipt>,
    /// Complete external contractual cash: no superseded remaining schedules.
    pub schedule: DatedSchedule,
    /// Residual states before their separately retained terminal cash settlement.
    pub live_before_terminal_settlement: Vec<CdState>,
    pub source_status: String,
    pub accounting_adoption: bool,
    pub empirical_validation: bool,
    pub joint_optimal_exercise: bool,
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn label(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
fn identify_event(event: &mut DatedCashflowEvent, prefix: &str, as_of: i32) -> Result<(), String> {
    event.event_id = format!("{prefix}:{}", event.event_id);
    event.day_offset =
        u32::try_from(event.payment_ordinal - as_of).map_err(|_| "cash before source opening")?;
    Ok(())
}

pub fn prepare(r: &Request, scope: &Control) -> Result<ResultData, String> {
    scope.check()?;
    require(
        r.schema == SCHEMA && label(&r.input_revision),
        "sequence schema/input identity",
    )?;
    require(
        matches!(
            r.measure,
            CashflowMeasure::Physical | CashflowMeasure::Pricing
        ),
        "CD sequence requires explicit Physical or Pricing measure",
    )?;
    require(
        r.actions.len() <= MAX_ACTIONS
            && (r.actions.is_empty() || !r.policies.is_empty())
            && r.policies.len() <= MAX_LIVE,
        "bounded complete CD action/policy sequence required",
    )?;
    let raw_bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    require(raw_bytes <= MAX_RAW_BYTES, "CD sequence raw input capacity")?;
    require(
        r.as_of_ordinal == r.opening.last_accrual_ordinal
            && r.reporting_end_ordinal >= r.as_of_ordinal
            && r.tail_end_ordinal >= r.reporting_end_ordinal,
        "explicit CD opening/report/full-tail dates",
    )?;
    let reserved = raw_bytes
        .checked_mul(12)
        .and_then(|n| {
            (r.actions.len() + r.policies.len() + 1)
                .checked_mul(16_384)
                .and_then(|x| n.checked_add(x))
        })
        .ok_or("CD sequence capacity overflow")?;
    require(
        reserved <= 32 * 1024 * 1024,
        "CD sequence 32 MiB retained/output reservation",
    )?;
    let mut metadata = 0;
    crate::term_enhancements::identity(&r.terminal_projection_source, &mut metadata)?;
    let mut policies = BTreeMap::new();
    for b in &r.policies {
        scope.check()?;
        require(
            label(&b.position_id) && policies.insert(b.position_id.clone(), b).is_none(),
            "unique CD policy position binding",
        )?;
    }
    let mut action_ids = BTreeSet::new();
    let mut prior = r.as_of_ordinal;
    for action in &r.actions {
        require(
            label(&action.action_id)
                && action_ids.insert(&action.action_id)
                && label(&action.position_id)
                && action.ordinal >= prior
                && action.ordinal <= r.tail_end_ordinal,
            "chronological unique CD action identity/date",
        )?;
        prior = action.ordinal;
    }
    if r.actions.is_empty() {
        require(
            r.policies.is_empty(),
            "no-election source has no policy-owned action; unused policies reject",
        )?;
    } else {
        let original = policies
            .get(&r.opening.position_id)
            .ok_or("original CD requires explicit policy")?;
        require(
            original.contract == r.opening.contract,
            "original CD policy contract identity mismatch",
        )?;
    }
    let mut live = BTreeMap::from([(r.opening.position_id.clone(), r.opening.clone())]);
    let mut ever_used = BTreeSet::from([r.opening.position_id.clone()]);
    let mut used_policies = BTreeSet::new();
    let mut events = Vec::new();
    let mut receipts = Vec::with_capacity(r.actions.len());
    for (index, action) in r.actions.iter().enumerate() {
        scope.check()?;
        let opening = live
            .remove(&action.position_id)
            .ok_or("CD election requires an exclusively owned live contract")?;
        let binding = policies
            .get(&opening.position_id)
            .ok_or("CD contract requires explicit policy")?;
        require(
            binding.contract == opening.contract,
            "CD policy contract identity/revision mismatch",
        )?;
        used_policies.insert(opening.position_id.clone());
        let request = CdElectionRequest {
            schema: STEP_SCHEMA.into(),
            model_version: MODEL_VERSION.into(),
            measure: r.measure,
            currency: r.currency.clone(),
            monetary_unit: r.monetary_unit.clone(),
            direction: r.direction,
            scenario_id: r.scenario_id.clone(),
            election_source: action.source.clone(),
            policy: binding.policy.clone(),
            opening: opening.clone(),
            election_ordinal: action.ordinal,
            reporting_end_ordinal: r.reporting_end_ordinal.max(action.ordinal),
            tail_end_ordinal: r.tail_end_ordinal,
            election: action.election.clone(),
        };
        let native = match r.measure {
            CashflowMeasure::Physical => step_cd(&request)?,
            CashflowMeasure::Pricing => step_cd_pricing(&request)?,
            CashflowMeasure::Contractual => {
                return Err("unsupported contractual election measure".into())
            }
        };
        // Intermediate bullet projections are source receipts, not settled cash.
        if native.closing.principal != 0. || native.closing.accrued_interest != 0. {
            live.insert(native.closing.position_id.clone(), native.closing.clone());
        }
        for next in &native.new_contracts {
            require(
                ever_used.insert(next.position_id.clone()),
                "CD renewal/switch cannot reuse a historical position identity",
            )?;
            let policy = policies
                .get(&next.position_id)
                .ok_or("renewed/switched CD requires explicit new policy")?;
            require(
                policy.contract == next.contract,
                "new CD policy contract identity/revision mismatch",
            )?;
            used_policies.insert(next.position_id.clone());
            live.insert(next.position_id.clone(), next.clone());
        }
        require(live.len() <= MAX_LIVE, "CD retained live-state capacity")?;
        for e in &native.settlement_schedule.events {
            let mut e = e.clone();
            identify_event(&mut e, &format!("action-{index}"), r.as_of_ordinal)?;
            events.push(e);
        }
        receipts.push(ActionReceipt {
            action: action.clone(),
            opening,
            native,
        });
    }
    // Every survivor has one independent no-election maturity tail. Intermediate
    // step previews remain evidence only and never enter the complete cash twice.
    for (index, opening) in live.values().enumerate() {
        scope.check()?;
        if !r.actions.is_empty() {
            let binding = policies
                .get(&opening.position_id)
                .ok_or("CD survivor requires explicit policy")?;
            require(
                binding.contract == opening.contract,
                "CD terminal policy identity mismatch",
            )?;
            used_policies.insert(opening.position_id.clone());
        }
        let native = project_cd_tail(&CdTailRequest {
            schema: STEP_SCHEMA.into(),
            model_version: MODEL_VERSION.into(),
            measure: r.measure,
            currency: r.currency.clone(),
            monetary_unit: r.monetary_unit.clone(),
            direction: r.direction,
            scenario_id: r.scenario_id.clone(),
            projection_source: r.terminal_projection_source.clone(),
            opening: opening.clone(),
            as_of_ordinal: opening.last_accrual_ordinal,
            reporting_end_ordinal: r.reporting_end_ordinal.max(opening.last_accrual_ordinal),
            tail_end_ordinal: r.tail_end_ordinal,
        })?;
        for mut e in native.schedule.events {
            identify_event(&mut e, &format!("terminal-{index}"), r.as_of_ordinal)?;
            events.push(e);
        }
    }
    require(
        used_policies.len() == policies.len(),
        "unused CD policy binding is contradictory",
    )?;
    events.sort_by_key(|e| e.payment_ordinal);
    let schedule = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: r.as_of_ordinal,
        horizon_days: u32::try_from(r.tail_end_ordinal - r.as_of_ordinal)
            .map_err(|_| "negative CD tail")?,
        scenarios: vec![r.scenario_id.clone()],
        events,
    };
    schedule.validate()?;
    scope.check()?;
    Ok(ResultData {
        schema: SCHEMA.into(),
        admitted_request: r.clone(),
        action_receipts: receipts,
        schedule,
        live_before_terminal_settlement: live.into_values().collect(),
        source_status: "explicit_supplied_chronological_contract_elections".into(),
        accounting_adoption: false,
        empirical_validation: false,
        joint_optimal_exercise: false,
    })
}
