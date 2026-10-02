//! Deterministic expected-credit transitions. No fitted coefficients or implicit annual PDs.
use crate::limits::MAX_MONETARY_AMOUNT as MAX_AMOUNT;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "credit-model-1";
pub const MAX_PERIODS: usize = 4096;
pub const MAX_RECOVERY_POINTS: usize = 32;
const MAX_YEARS: f64 = 100.0;
/// Maximum total outgoing continuous-time intensity admitted per live origin.
pub const MAX_ROW_HAZARD: f64 = 100.0;
const MASS_TOLERANCE: f64 = 1.0e-11;
const POISSON_TOLERANCE: f64 = 2.0e-16;
const MAX_POISSON_TERMS: usize = 128;
const N: usize = 7; // 3 live, 3 default-by-origin, one prepaid

/// States in all input/output arrays: performing, watch, delinquent, default, prepaid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditState {
    Performing,
    Watch,
    Delinquent,
    Default,
    Prepaid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureAtDefault {
    /// Monetary units per original account conditional on being in this live state.
    pub drawn: f64,
    pub undrawn: f64,
    /// Fraction of undrawn commitment used by default, not an unconditional draw rate.
    pub credit_conversion_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPoint {
    /// Time after the period-end recognition of default. Includes an explicit tail.
    pub lag_years: f64,
    /// Share of eventual nominal recovery, not of defaulted exposure.
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditPeriod {
    /// Exact elapsed model years; callers supply their own calendar/day-count conversion.
    pub duration_years: f64,
    /// Continuous-time annual hazards [live origin][five-state destination]. Diagonal = 0.
    pub transition_hazards_per_year: [[f64; 5]; 3],
    pub exposure_at_default: [ExposureAtDefault; 3],
    /// Eventual undiscounted loss / EAD, conditional on default from each live state.
    pub loss_given_default: [f64; 3],
    pub recovery_distribution: Vec<RecoveryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditRequest {
    pub schema: String,
    /// An explicit caller-defined currency/unit label, retained without conversion.
    pub monetary_unit: String,
    /// Performing/watch/delinquent probabilities; nonnegative, sum to one.
    pub initial_live_probability: [f64; 3],
    pub periods: Vec<CreditPeriod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPeriodResult {
    pub period_index: usize,
    pub end_year: f64,
    pub state_probability: [f64; 5],
    /// Unconditional mass of new defaults relative to the original population.
    pub marginal_default_probability: f64,
    pub marginal_prepayment_probability: f64,
    /// Relative to live mass at this interval's start; None if no live mass remains.
    pub conditional_default_probability: Option<f64>,
    pub conditional_prepayment_probability: Option<f64>,
    pub default_probability_by_origin: [f64; 3],
    pub defaulted_exposure: f64,
    pub expected_loss: f64,
    pub eventual_nominal_recovery: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedRecovery {
    pub default_period_index: usize,
    pub default_origin: CreditState,
    pub payment_year: f64,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditResult {
    pub schema: String,
    pub monetary_unit: String,
    pub periods: Vec<CreditPeriodResult>,
    /// Complete projected recoveries, including those falling after the credit horizon.
    pub recoveries: Vec<ExpectedRecovery>,
    pub total_defaulted_exposure: f64,
    pub total_expected_loss: f64,
    pub total_eventual_nominal_recovery: f64,
    pub recovery_after_horizon: f64,
}

fn finite_range(value: f64, min: f64, max: f64, name: &str) -> Result<(), String> {
    if !value.is_finite() || value < min || value > max {
        return Err(format!("{name} must be finite in [{min}, {max}]"));
    }
    Ok(())
}

/// Convert a single-cause conditional annual PD to a constant annual intensity.
/// A PD of one has infinite intensity and is rejected by this bounded model.
pub fn annual_pd_to_hazard(annual_pd: f64) -> Result<f64, String> {
    finite_range(annual_pd, 0.0, 1.0, "annual_pd")?;
    if annual_pd == 1.0 {
        return Err("annual_pd of one has infinite hazard".into());
    }
    Ok(-(-annual_pd).ln_1p())
}

/// Exact one-live-state competing default/prepay probabilities for constant hazards.
pub fn competing_exit_probabilities(
    default_hazard: f64,
    prepayment_hazard: f64,
    duration_years: f64,
) -> Result<[f64; 3], String> {
    finite_range(default_hazard, 0.0, MAX_ROW_HAZARD, "default_hazard")?;
    finite_range(prepayment_hazard, 0.0, MAX_ROW_HAZARD, "prepayment_hazard")?;
    finite_range(duration_years, 0.0, MAX_YEARS, "duration_years")?;
    let hazard = default_hazard + prepayment_hazard;
    finite_range(hazard, 0.0, MAX_ROW_HAZARD, "combined_hazard")?;
    if hazard == 0.0 {
        return Ok([1.0, 0.0, 0.0]);
    }
    let exit = -(-hazard * duration_years).exp_m1();
    Ok([
        (-hazard * duration_years).exp(),
        exit * default_hazard / hazard,
        exit * prepayment_hazard / hazard,
    ])
}

fn validate(request: &CreditRequest) -> Result<(), String> {
    if request.schema != SCHEMA {
        return Err(format!("unsupported credit schema: {}", request.schema));
    }
    if request.monetary_unit.trim().is_empty()
        || request.monetary_unit.len() > 64
        || request.monetary_unit.chars().any(char::is_control)
    {
        return Err("monetary_unit must contain 1..64 bytes without control characters".into());
    }
    for value in request.initial_live_probability {
        finite_range(value, 0.0, 1.0, "initial_live_probability")?;
    }
    if (request.initial_live_probability.iter().sum::<f64>() - 1.0).abs() > 1.0e-12 {
        return Err("initial_live_probability must sum to one".into());
    }
    if request.periods.is_empty() || request.periods.len() > MAX_PERIODS {
        return Err(format!("periods must contain 1..{MAX_PERIODS} terms"));
    }
    let mut total_years = 0.0;
    let mut work_terms = 0usize;
    let mut recovery_rows = 0usize;
    for (index, period) in request.periods.iter().enumerate() {
        finite_range(
            period.duration_years,
            f64::MIN_POSITIVE,
            1.0,
            "duration_years",
        )?;
        total_years += period.duration_years;
        finite_range(total_years, 0.0, MAX_YEARS, "total_years")?;
        let mut maximum_hazard: f64 = 0.0;
        for origin in 0..3 {
            let row = &period.transition_hazards_per_year[origin];
            for rate in row {
                finite_range(*rate, 0.0, MAX_ROW_HAZARD, "transition_hazard")?;
            }
            if row[origin] != 0.0 {
                return Err(format!("period {index}: diagonal hazard must be zero"));
            }
            let sum = row.iter().sum::<f64>();
            finite_range(sum, 0.0, MAX_ROW_HAZARD, "total_origin_hazard")?;
            maximum_hazard = maximum_hazard.max(sum);
            let ead = &period.exposure_at_default[origin];
            finite_range(ead.drawn, 0.0, MAX_AMOUNT, "drawn")?;
            finite_range(ead.undrawn, 0.0, MAX_AMOUNT, "undrawn")?;
            finite_range(
                ead.credit_conversion_factor,
                0.0,
                1.0,
                "credit_conversion_factor",
            )?;
            finite_range(
                ead.drawn + ead.undrawn * ead.credit_conversion_factor,
                0.0,
                MAX_AMOUNT,
                "ead",
            )?;
            finite_range(
                period.loss_given_default[origin],
                0.0,
                1.0,
                "loss_given_default",
            )?;
        }
        work_terms += ((maximum_hazard * period.duration_years / 8.0).ceil() as usize).max(1)
            * MAX_POISSON_TERMS;
        if work_terms > 1_000_000 {
            return Err("credit uniformization work budget exceeded".into());
        }
        if period.recovery_distribution.len() > MAX_RECOVERY_POINTS {
            return Err("recovery_distribution exceeds 32 points".into());
        }
        if period.recovery_distribution.is_empty() {
            if period.loss_given_default.iter().any(|v| *v != 1.0) {
                return Err("nonzero eventual recovery requires an explicit distribution".into());
            }
        } else {
            let mut previous_lag = -1.0;
            let mut weight = 0.0;
            for point in &period.recovery_distribution {
                finite_range(point.lag_years, 0.0, MAX_YEARS, "recovery_lag_years")?;
                finite_range(point.weight, 0.0, 1.0, "recovery_weight")?;
                if point.lag_years <= previous_lag {
                    return Err("recovery lags must be strictly increasing".into());
                }
                previous_lag = point.lag_years;
                weight += point.weight;
            }
            if (weight - 1.0).abs() > 1.0e-12 {
                return Err("recovery weights must sum to one; tails cannot be omitted".into());
            }
        }
        recovery_rows += period.recovery_distribution.len() * 3;
        if recovery_rows > MAX_PERIODS * MAX_RECOVERY_POINTS * 3 {
            return Err("credit recovery output budget exceeded".into());
        }
    }
    Ok(())
}

/// Piecewise continuous-time Markov propagation via nonnegative uniformization.
/// Default absorption tracks the live origin so conditional EAD/LGD stay aligned.
fn propagate(live: [f64; 3], period: &CreditPeriod) -> Result<[f64; N], String> {
    let rate = period
        .transition_hazards_per_year
        .iter()
        .map(|row| row.iter().sum::<f64>())
        .fold(0.0, f64::max);
    let mut state = [0.0; N];
    state[..3].copy_from_slice(&live);
    if rate == 0.0 || live.iter().sum::<f64>() == 0.0 {
        return Ok(state);
    }
    let mut matrix = [[0.0; N]; N];
    for origin in 0..3 {
        let row = &period.transition_hazards_per_year[origin];
        matrix[origin][origin] = 1.0 - row.iter().sum::<f64>() / rate;
        for destination in 0..3 {
            if origin != destination {
                matrix[origin][destination] = row[destination] / rate;
            }
        }
        matrix[origin][3 + origin] = row[3] / rate;
        matrix[origin][6] = row[4] / rate;
    }
    for (index, row) in matrix.iter_mut().enumerate().skip(3) {
        row[index] = 1.0;
    }
    let chunks = (rate * period.duration_years / 8.0).ceil().max(1.0) as usize;
    let poisson_mean = rate * period.duration_years / chunks as f64;
    for _ in 0..chunks {
        let mut power = state;
        let mut weight = (-poisson_mean).exp();
        let mut result = power.map(|v| v * weight);
        let mut converged = false;
        for order in 1..=MAX_POISSON_TERMS {
            let mut next = [0.0; N];
            for origin in 0..N {
                for destination in 0..N {
                    next[destination] += power[origin] * matrix[origin][destination];
                }
            }
            power = next;
            weight *= poisson_mean / order as f64;
            for index in 0..N {
                result[index] += power[index] * weight;
            }
            // Once beyond the mode, all omitted Poisson weights are bounded by a
            // geometric tail. This avoids cancellation in 1 - cumulative_weight.
            let ratio = poisson_mean / (order + 1) as f64;
            if ratio < 1.0
                && weight * ratio / (1.0 - ratio) <= POISSON_TOLERANCE * poisson_mean.min(1.0)
            {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err("credit uniformization failed its explicit tail bound".into());
        }
        if result.iter().any(|v| !v.is_finite() || *v < 0.0)
            || (result.iter().sum::<f64>() - state.iter().sum::<f64>()).abs() > MASS_TOLERANCE
        {
            return Err("credit propagation violated probability mass".into());
        }
        state = result;
    }
    Ok(state)
}

pub fn evaluate(request: &CreditRequest) -> Result<CreditResult, String> {
    validate(request)?; // Validate all terms before financial work or output allocation.
    let mut result = CreditResult {
        schema: SCHEMA.into(),
        monetary_unit: request.monetary_unit.clone(),
        periods: Vec::with_capacity(request.periods.len()),
        recoveries: Vec::new(),
        total_defaulted_exposure: 0.0,
        total_expected_loss: 0.0,
        total_eventual_nominal_recovery: 0.0,
        recovery_after_horizon: 0.0,
    };
    let mut live = request.initial_live_probability;
    let mut default_mass = 0.0;
    let mut prepaid_mass = 0.0;
    let mut end_year = 0.0;
    for (period_index, period) in request.periods.iter().enumerate() {
        end_year += period.duration_years;
        let start_live = live.iter().sum::<f64>();
        let next = propagate(live, period)?;
        live.copy_from_slice(&next[..3]);
        let defaults = [next[3], next[4], next[5]];
        let marginal_default_probability = defaults.iter().sum::<f64>();
        let marginal_prepayment_probability = next[6];
        default_mass += marginal_default_probability;
        prepaid_mass += marginal_prepayment_probability;
        let state_probability = [live[0], live[1], live[2], default_mass, prepaid_mass];
        if (state_probability.iter().sum::<f64>() - 1.0).abs() > MASS_TOLERANCE {
            return Err("credit horizon violated probability mass".into());
        }
        let mut defaulted_exposure = 0.0;
        let mut expected_loss = 0.0;
        let mut eventual_nominal_recovery = 0.0;
        for origin in 0..3 {
            let ead = &period.exposure_at_default[origin];
            let amount =
                defaults[origin] * (ead.drawn + ead.undrawn * ead.credit_conversion_factor);
            let loss = amount * period.loss_given_default[origin];
            let recovery = amount * (1.0 - period.loss_given_default[origin]);
            defaulted_exposure += amount;
            expected_loss += loss;
            eventual_nominal_recovery += recovery;
            for point in &period.recovery_distribution {
                let amount = recovery * point.weight;
                if amount > 0.0 {
                    result.recoveries.push(ExpectedRecovery {
                        default_period_index: period_index,
                        default_origin: [
                            CreditState::Performing,
                            CreditState::Watch,
                            CreditState::Delinquent,
                        ][origin],
                        payment_year: end_year + point.lag_years,
                        amount,
                    });
                }
            }
        }
        result.total_defaulted_exposure += defaulted_exposure;
        result.total_expected_loss += expected_loss;
        result.total_eventual_nominal_recovery += eventual_nominal_recovery;
        result.periods.push(CreditPeriodResult {
            period_index,
            end_year,
            state_probability,
            marginal_default_probability,
            marginal_prepayment_probability,
            conditional_default_probability: (start_live > 0.0)
                .then(|| marginal_default_probability / start_live),
            conditional_prepayment_probability: (start_live > 0.0)
                .then(|| marginal_prepayment_probability / start_live),
            default_probability_by_origin: defaults,
            defaulted_exposure,
            expected_loss,
            eventual_nominal_recovery,
        });
    }
    // Stable deterministic order; period/origin order breaks equal-time ties.
    result
        .recoveries
        .sort_by(|a, b| a.payment_year.total_cmp(&b.payment_year));
    result.recovery_after_horizon = result
        .recoveries
        .iter()
        .filter(|row| row.payment_year > end_year)
        .map(|row| row.amount)
        .sum();
    Ok(result)
}
