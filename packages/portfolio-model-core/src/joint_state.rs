//! Bounded supplied-driver coordinator for market, behavioral and credit paths.
//!
//! This layer deliberately composes supplied drivers; it does not fit a model,
//! convert a physical measure to a pricing measure, or infer credit behavior
//! from a market shock. One path key and one caller-owned random stream are
//! retained across every competing transition so downstream product graphs can
//! use the same common-random-number identity.

use crate::cashflow::{actual_year_fraction, validate_ordinal, ActualDayCount, CashflowMeasure};
use crate::credit_path::UniformPair;
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "joint-state-1";
pub const MAX_PERIODS: usize = 4096;
pub const MAX_PATHS: usize = 10_000;
pub const MAX_WORK: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointIdentity {
    pub source_id: String,
    pub source_revision: String,
    pub assumption_id: String,
    pub common_random_numbers_id: String,
    pub measure: CashflowMeasure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointPeriod {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    /// A supplied discount factor or market mark. It is retained as a driver;
    /// this coordinator does not use it to solve OAS or transform cashflows.
    pub market_discount_factor: f64,
    pub behavioral_exit_hazard_per_year: f64,
    pub credit_default_hazard_per_year: f64,
    pub prepayment_hazard_per_year: f64,
    pub recovery_fraction: f64,
    pub recovery_lag_years: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointPathOpening {
    pub position_id: String,
    pub path_key: u64,
    pub principal: f64,
    /// One pair per period. Uniforms are caller-owned and must be shared with
    /// any sibling product graph that claims common-random-number parity.
    pub random_pairs: Vec<UniformPair>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointStateRequest {
    pub schema: String,
    pub identity: JointIdentity,
    pub as_of_ordinal: i32,
    pub day_count: ActualDayCount,
    pub periods: Vec<JointPeriod>,
    pub paths: Vec<JointPathOpening>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointEventKind {
    Continue,
    BehavioralExit,
    CreditDefault,
    Prepayment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointEvent {
    pub position_id: String,
    pub path_key: u64,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub kind: JointEventKind,
    pub principal_before: f64,
    pub principal_after: f64,
    pub recovery_cash: f64,
    pub economic_loss: f64,
    pub market_discount_factor: f64,
    pub recovery_lag_years: f64,
    pub behavioral_exit_probability: f64,
    pub credit_default_probability: f64,
    pub prepayment_probability: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointPathResult {
    pub position_id: String,
    pub path_key: u64,
    pub closing_principal: f64,
    pub total_behavioral_exit: f64,
    pub total_credit_default: f64,
    pub total_prepayment: f64,
    pub total_recovery_cash: f64,
    pub total_economic_loss: f64,
    pub discounted_live_balance: f64,
    pub events: Vec<JointEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JointStateResult {
    pub schema: String,
    pub identity: JointIdentity,
    pub source_status: String,
    pub work_units: usize,
    pub paths: Vec<JointPathResult>,
}

fn label(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must contain 1..256 control-free bytes"));
    }
    Ok(())
}

fn finite(value: f64, low: f64, high: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value < low || value > high {
        return Err(format!("{name} must be finite in [{low}, {high}]"));
    }
    Ok(())
}

fn probabilities(period: &JointPeriod, years: f64) -> Result<[f64; 3], String> {
    let hazards = [
        period.behavioral_exit_hazard_per_year,
        period.credit_default_hazard_per_year,
        period.prepayment_hazard_per_year,
    ];
    for (i, hazard) in hazards.iter().enumerate() {
        finite(
            *hazard,
            0.0,
            100.0,
            [
                "behavioral_exit_hazard_per_year",
                "credit_default_hazard_per_year",
                "prepayment_hazard_per_year",
            ][i],
        )?;
    }
    let total = hazards.iter().sum::<f64>();
    finite(total, 0.0, 100.0, "total_hazard")?;
    if total == 0.0 {
        return Ok([0.0; 3]);
    }
    let exit = -(-total * years).exp_m1();
    Ok(hazards.map(|hazard| exit * hazard / total))
}

fn validate(request: &JointStateRequest) -> Result<(), String> {
    if request.schema != SCHEMA {
        return Err("joint-state schema mismatch".into());
    }
    for (value, name) in [
        (&request.identity.source_id, "source_id"),
        (&request.identity.source_revision, "source_revision"),
        (&request.identity.assumption_id, "assumption_id"),
        (
            &request.identity.common_random_numbers_id,
            "common_random_numbers_id",
        ),
    ] {
        label(value, name)?;
    }
    validate_ordinal(request.as_of_ordinal)?;
    if request.periods.is_empty() || request.periods.len() > MAX_PERIODS {
        return Err(format!("periods must contain 1..{MAX_PERIODS} rows"));
    }
    if request.paths.is_empty() || request.paths.len() > MAX_PATHS {
        return Err(format!("paths must contain 1..{MAX_PATHS} rows"));
    }
    let mut previous_end = request.as_of_ordinal;
    for period in &request.periods {
        validate_ordinal(period.start_ordinal)?;
        validate_ordinal(period.end_ordinal)?;
        if period.start_ordinal != previous_end || period.end_ordinal <= period.start_ordinal {
            return Err("joint periods must be contiguous actual-date intervals".into());
        }
        let years =
            actual_year_fraction(period.start_ordinal, period.end_ordinal, request.day_count)?;
        probabilities(period, years)?;
        finite(
            period.market_discount_factor,
            f64::MIN_POSITIVE,
            1.0e12,
            "market_discount_factor",
        )?;
        finite(period.recovery_fraction, 0.0, 1.0, "recovery_fraction")?;
        finite(period.recovery_lag_years, 0.0, 100.0, "recovery_lag_years")?;
        previous_end = period.end_ordinal;
    }
    let mut positions = HashSet::new();
    let mut keys = HashSet::new();
    for path in &request.paths {
        label(&path.position_id, "position_id")?;
        if !positions.insert(&path.position_id) || !keys.insert(path.path_key) {
            return Err("position_id and path_key must be unique".into());
        }
        finite(path.principal, 0.0, MAX_MONETARY_AMOUNT, "principal")?;
        if path.random_pairs.len() != request.periods.len() {
            return Err("one caller-owned random pair is required per period".into());
        }
        for pair in &path.random_pairs {
            finite(pair.waiting_uniform, 0.0, 1.0, "waiting_uniform")?;
            finite(pair.destination_uniform, 0.0, 1.0, "destination_uniform")?;
            if pair.waiting_uniform >= 1.0 || pair.destination_uniform >= 1.0 {
                return Err("random uniforms must be in [0,1)".into());
            }
        }
    }
    Ok(())
}

/// Evaluate one supplied joint driver frame per path. Competing exits are
/// sampled once per interval; no state is renormalized after a transition.
pub fn evaluate(request: &JointStateRequest) -> Result<JointStateResult, String> {
    evaluate_with_check(request, || Ok(()))
}

/// Evaluate with a caller-owned cooperative cancellation/deadline checkpoint.
/// No partial result is returned when the checkpoint fails.
pub fn evaluate_with_check(
    request: &JointStateRequest,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<JointStateResult, String> {
    check()?;
    validate(request)?;
    let mut work = 0usize;
    let mut results = Vec::with_capacity(request.paths.len());
    for path in &request.paths {
        check()?;
        let mut balance = path.principal;
        let mut total_behavioral_exit = 0.0;
        let mut total_credit_default = 0.0;
        let mut total_prepayment = 0.0;
        let mut total_recovery_cash = 0.0;
        let mut total_economic_loss = 0.0;
        let mut discounted_live_balance = 0.0;
        let mut events = Vec::with_capacity(request.periods.len());
        for (index, period) in request.periods.iter().enumerate() {
            check()?;
            work = work.checked_add(1).ok_or("joint-state work overflow")?;
            if work > MAX_WORK {
                return Err("joint-state work limit exceeded".into());
            }
            let years =
                actual_year_fraction(period.start_ordinal, period.end_ordinal, request.day_count)?;
            let [p_behavior, p_credit, p_prepay] = probabilities(period, years)?;
            let pair = path.random_pairs[index];
            let before = balance;
            let mut kind = JointEventKind::Continue;
            let mut after = before;
            let mut recovery = 0.0;
            let mut loss = 0.0;
            if before > 0.0 {
                let u = pair.waiting_uniform;
                if u < p_behavior {
                    kind = JointEventKind::BehavioralExit;
                    after = 0.0;
                    total_behavioral_exit += before;
                } else if u < p_behavior + p_credit {
                    kind = JointEventKind::CreditDefault;
                    after = 0.0;
                    recovery = before * period.recovery_fraction;
                    loss = before - recovery;
                    total_credit_default += before;
                    total_recovery_cash += recovery;
                    total_economic_loss += loss;
                } else if u < p_behavior + p_credit + p_prepay {
                    kind = JointEventKind::Prepayment;
                    after = 0.0;
                    total_prepayment += before;
                } else {
                    discounted_live_balance += before * period.market_discount_factor;
                }
            }
            events.push(JointEvent {
                position_id: path.position_id.clone(),
                path_key: path.path_key,
                start_ordinal: period.start_ordinal,
                end_ordinal: period.end_ordinal,
                kind,
                principal_before: before,
                principal_after: after,
                recovery_cash: recovery,
                economic_loss: loss,
                market_discount_factor: period.market_discount_factor,
                recovery_lag_years: period.recovery_lag_years,
                behavioral_exit_probability: p_behavior,
                credit_default_probability: p_credit,
                prepayment_probability: p_prepay,
            });
            balance = after;
        }
        results.push(JointPathResult {
            position_id: path.position_id.clone(),
            path_key: path.path_key,
            closing_principal: balance,
            total_behavioral_exit,
            total_credit_default,
            total_prepayment,
            total_recovery_cash,
            total_economic_loss,
            discounted_live_balance,
            events,
        });
    }
    Ok(JointStateResult {
        schema: SCHEMA.into(),
        identity: request.identity.clone(),
        source_status: "supplied_joint_market_behavior_credit_drivers".into(),
        work_units: work,
        paths: results,
    })
}
