//! Joint stochastic market-driver and borrower-state generation.
//!
//! This contract composes the existing PSD Gaussian market tape with the
//! shared competing-hazard state machine.  The market tape is generated once;
//! the same path/period innovations then drive explicitly supplied behavioral,
//! default and prepayment elasticities.  No pricing-measure conversion,
//! OAS solve, recovery fit or accounting policy is inferred here.

use crate::market::analysis::joint::{self, JointGaussianRequest, JointGaussianResult};
use portfolio_model_core::cashflow::{actual_year_fraction, validate_ordinal, ActualDayCount};
use portfolio_model_core::credit_path::UniformPair;
use portfolio_model_core::joint_state::{
    self, JointEventKind, JointIdentity, JointPathOpening, JointPathResult, JointPeriod,
    JointStateRequest,
};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "joint-market-credit-1";
pub const MAX_PERIODS: usize = joint_state::MAX_PERIODS;
pub const MAX_POSITIONS: usize = joint_state::MAX_PATHS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodPolicy {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    /// Supplied pricing-measure discount factor retained by the state output.
    /// The generated rate shock is not silently converted into a discount curve.
    pub market_discount_factor: f64,
    pub behavioral_baseline_hazard_per_year: f64,
    pub behavioral_beta: Vec<f64>,
    pub credit_baseline_hazard_per_year: f64,
    pub credit_beta: Vec<f64>,
    pub prepayment_baseline_hazard_per_year: f64,
    pub prepayment_beta: Vec<f64>,
    pub recovery_fraction: f64,
    pub recovery_lag_years: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub path_key: u64,
    pub principal: f64,
    pub random_pairs: Vec<UniformPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub identity: JointIdentity,
    pub as_of_ordinal: i32,
    pub day_count: ActualDayCount,
    pub market: JointGaussianRequest,
    pub periods: Vec<PeriodPolicy>,
    pub positions: Vec<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointMarketCreditResult {
    pub schema: String,
    pub model_version: String,
    pub identity: JointIdentity,
    pub source_status: String,
    pub adoption_eligible: bool,
    pub market: JointGaussianResult,
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

fn validate_beta(beta: &[f64], drivers: usize, name: &str) -> Result<(), String> {
    if beta.len() != drivers {
        return Err(format!(
            "{name} must match the declared market driver count"
        ));
    }
    for value in beta {
        finite(*value, -10.0, 10.0, name)?;
    }
    Ok(())
}

fn validate(request: &Request) -> Result<(), String> {
    if request.schema != SCHEMA || request.model_version != SCHEMA {
        return Err("joint market-credit schema/version mismatch".into());
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
    if request.positions.is_empty() || request.positions.len() > MAX_POSITIONS {
        return Err(format!("positions must contain 1..{MAX_POSITIONS} rows"));
    }
    if request.market.source_revision != request.identity.source_revision
        || request.market.shared_draw_id != request.identity.common_random_numbers_id
    {
        return Err(
            "market source and shared-draw identities must match the joint identity".into(),
        );
    }
    if request.market.durations_years.len() != request.periods.len()
        || request.market.paths != request.positions.len()
    {
        return Err("market dimensions must match periods and positions".into());
    }
    let mut previous_end = request.as_of_ordinal;
    for (period, duration) in request.periods.iter().zip(&request.market.durations_years) {
        validate_ordinal(period.start_ordinal)?;
        validate_ordinal(period.end_ordinal)?;
        if period.start_ordinal != previous_end || period.end_ordinal <= period.start_ordinal {
            return Err(
                "joint market-credit periods must be contiguous actual-date intervals".into(),
            );
        }
        let years =
            actual_year_fraction(period.start_ordinal, period.end_ordinal, request.day_count)?;
        finite(*duration, 1e-12, 100.0, "market duration")?;
        if (*duration - years).abs() > 1e-12 * years.max(1.0) {
            return Err("market durations must equal the declared actual-date periods".into());
        }
        finite(
            period.market_discount_factor,
            f64::MIN_POSITIVE,
            1.0e12,
            "market_discount_factor",
        )?;
        for (value, name) in [
            (
                period.behavioral_baseline_hazard_per_year,
                "behavioral_baseline_hazard_per_year",
            ),
            (
                period.credit_baseline_hazard_per_year,
                "credit_baseline_hazard_per_year",
            ),
            (
                period.prepayment_baseline_hazard_per_year,
                "prepayment_baseline_hazard_per_year",
            ),
        ] {
            finite(value, 0.0, 100.0, name)?;
        }
        validate_beta(
            &period.behavioral_beta,
            request.market.drivers.len(),
            "behavioral_beta",
        )?;
        validate_beta(
            &period.credit_beta,
            request.market.drivers.len(),
            "credit_beta",
        )?;
        validate_beta(
            &period.prepayment_beta,
            request.market.drivers.len(),
            "prepayment_beta",
        )?;
        finite(period.recovery_fraction, 0.0, 1.0, "recovery_fraction")?;
        finite(period.recovery_lag_years, 0.0, 100.0, "recovery_lag_years")?;
        previous_end = period.end_ordinal;
    }
    let mut positions = HashSet::new();
    let mut keys = HashSet::new();
    for position in &request.positions {
        label(&position.position_id, "position_id")?;
        if !positions.insert(&position.position_id) || !keys.insert(position.path_key) {
            return Err("position_id and path_key must be unique".into());
        }
        finite(position.principal, 0.0, MAX_MONETARY_AMOUNT, "principal")?;
        if position.random_pairs.len() != request.periods.len() {
            return Err("one caller-owned random pair is required per period".into());
        }
        for pair in &position.random_pairs {
            finite(pair.waiting_uniform, 0.0, 1.0, "waiting_uniform")?;
            finite(pair.destination_uniform, 0.0, 1.0, "destination_uniform")?;
            if pair.waiting_uniform >= 1.0 || pair.destination_uniform >= 1.0 {
                return Err("random uniforms must be in [0,1)".into());
            }
        }
    }
    Ok(())
}

fn hazard(baseline: f64, beta: &[f64], annual_shock: &[f64], name: &str) -> Result<f64, String> {
    if baseline == 0.0 {
        if beta.iter().any(|value| *value != 0.0) {
            return Err(format!(
                "{name} beta must be zero when its baseline hazard is zero"
            ));
        }
        return Ok(0.0);
    }
    let linear = beta
        .iter()
        .zip(annual_shock)
        .map(|(coefficient, shock)| coefficient * shock)
        .sum::<f64>();
    let log_hazard = baseline.ln() + linear;
    if !log_hazard.is_finite() || log_hazard > 100.0_f64.ln() {
        return Err(format!(
            "{name} market-linked hazard exceeds the admitted domain"
        ));
    }
    let value = log_hazard.exp();
    finite(value, 0.0, 100.0, name)?;
    Ok(value)
}

/// Generate the market tape and feed its annualized shocks into the shared
/// competing-hazard state machine. All output is withheld if any checkpoint
/// or downstream state validation fails.
pub fn evaluate(request: &Request) -> Result<JointMarketCreditResult, String> {
    evaluate_with_check(request, || Ok(()))
}

pub fn evaluate_with_check(
    request: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<JointMarketCreditResult, String> {
    check()?;
    validate(request)?;
    let market = joint::simulate_with_check(&request.market, &mut check)?;
    let drivers = request.market.drivers.len();
    let steps = request.periods.len();
    let expected_increments = request
        .positions
        .len()
        .checked_mul(steps)
        .and_then(|value| value.checked_mul(drivers))
        .ok_or("joint market-credit increment shape overflow")?;
    if market.increments.len() != expected_increments {
        return Err("joint market tape shape differs from the admitted request".into());
    }
    let mut paths = Vec::with_capacity(request.positions.len());
    for (path_index, position) in request.positions.iter().enumerate() {
        check()?;
        let mut periods = Vec::with_capacity(steps);
        for (step, policy) in request.periods.iter().enumerate() {
            check()?;
            let dt = request.market.durations_years[step];
            let offset = (path_index * steps + step) * drivers;
            let annual_shock: Vec<f64> = market.increments[offset..offset + drivers]
                .iter()
                .map(|increment| *increment / dt.sqrt())
                .collect();
            periods.push(JointPeriod {
                start_ordinal: policy.start_ordinal,
                end_ordinal: policy.end_ordinal,
                market_discount_factor: policy.market_discount_factor,
                behavioral_exit_hazard_per_year: hazard(
                    policy.behavioral_baseline_hazard_per_year,
                    &policy.behavioral_beta,
                    &annual_shock,
                    "behavioral_exit_hazard_per_year",
                )?,
                credit_default_hazard_per_year: hazard(
                    policy.credit_baseline_hazard_per_year,
                    &policy.credit_beta,
                    &annual_shock,
                    "credit_default_hazard_per_year",
                )?,
                prepayment_hazard_per_year: hazard(
                    policy.prepayment_baseline_hazard_per_year,
                    &policy.prepayment_beta,
                    &annual_shock,
                    "prepayment_hazard_per_year",
                )?,
                recovery_fraction: policy.recovery_fraction,
                recovery_lag_years: policy.recovery_lag_years,
            });
        }
        let state = JointStateRequest {
            schema: joint_state::SCHEMA.into(),
            identity: request.identity.clone(),
            as_of_ordinal: request.as_of_ordinal,
            day_count: request.day_count,
            periods,
            paths: vec![JointPathOpening {
                position_id: position.position_id.clone(),
                path_key: position.path_key,
                principal: position.principal,
                random_pairs: position.random_pairs.clone(),
            }],
        };
        let result = joint_state::evaluate_with_check(&state, &mut check)?;
        let path = result
            .paths
            .into_iter()
            .next()
            .ok_or("joint state returned no path")?;
        if path.events.iter().any(|event| {
            matches!(
                event.kind,
                JointEventKind::BehavioralExit
                    | JointEventKind::CreditDefault
                    | JointEventKind::Prepayment
            ) && event.principal_after > event.principal_before
        }) {
            return Err("joint market-credit path increased principal after an exit".into());
        }
        paths.push(path);
    }
    let work_units = request
        .positions
        .len()
        .checked_mul(steps)
        .ok_or("joint market-credit work overflow")?;
    Ok(JointMarketCreditResult {
        schema: SCHEMA.into(),
        model_version: SCHEMA.into(),
        identity: request.identity.clone(),
        source_status: "generated_market_drivers_with_supplied_hazard_elasticities".into(),
        adoption_eligible: false,
        market,
        work_units,
        paths,
    })
}
