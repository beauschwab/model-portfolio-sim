//! Immutable CTMC transition/earned-interest coefficients for independent state vectors.
use crate::credit::{CreditMeasure, MAX_ROW_HAZARD};
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "credit-operator-1";
pub const STATE_SCHEMA: &str = "credit-operator-state-1";
pub const STATES: usize = 7;
pub const MAX_DURATION_YEARS: f64 = 100.0;
pub const MAX_ABS_ACCRUAL_RATE: f64 = 100.0;
const MAX_POISSON_TERMS: usize = 128;
const TRUNCATION_TOLERANCE: f64 = 1.0e-16;
const MASS_TOLERANCE: f64 = 1.0e-11;
type Matrix = [[f64; STATES]; STATES];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditOperatorRequest {
    pub schema: String,
    pub measure: CreditMeasure,
    pub duration_years: f64,
    /// Performing/watch/delinquent rows; columns performing/watch/delinquent/default/prepaid.
    pub transition_hazards_per_year: [[f64; 5]; 3],
    /// Signed annual simple accrual per monetary unit of live principal in each state.
    pub accrual_rates: [f64; 3],
}

/// State order is live 3, default-from-live-origin 3, prepaid 1.
/// Monetary vectors are aggregate amounts, not conditional per-account EAD.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditOperatorState {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    /// Nonnegative absolute population mass <=1; live mass is never renormalized.
    pub probability: [f64; STATES],
    pub principal: [f64; STATES],
    pub undrawn_commitment: [f64; STATES],
    /// Signed receivable/payable amounts; negative coupons are not clipped.
    pub accrued_interest: [f64; STATES],
    /// Signed carrying-value premium/discount/fees; transported without new amortization.
    pub book_adjustment: [f64; STATES],
}

/// Compiled coefficients cannot be deserialized or mutated into unverified matrices.
#[derive(Debug, Clone, Serialize)]
pub struct CreditOperator {
    schema: String,
    measure: CreditMeasure,
    duration_years: f64,
    transition: Matrix,
    earned_interest: Matrix,
}

fn identity() -> Matrix {
    let mut matrix = [[0.0; STATES]; STATES];
    for (index, row) in matrix.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    matrix
}

fn product(a: &Matrix, b: &Matrix) -> Matrix {
    let mut result = [[0.0; STATES]; STATES];
    for (i, row) in result.iter_mut().enumerate() {
        for (k, &left) in a[i].iter().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value += left * b[k][j];
            }
        }
    }
    result
}

fn add(a: &Matrix, b: &Matrix) -> Matrix {
    let mut result = *a;
    for (i, row) in result.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value += b[i][j];
        }
    }
    result
}

fn scaled_add(result: &mut Matrix, term: &Matrix, weight: f64) {
    for (i, row) in result.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            *value += term[i][j] * weight;
        }
    }
}

fn compose(first: &(Matrix, Matrix), second: &(Matrix, Matrix)) -> (Matrix, Matrix) {
    // Block multiplication: [P1 F1;0 P1] [P2 F2;0 P2].
    (
        product(&first.0, &second.0),
        add(&product(&first.0, &second.1), &product(&first.1, &second.0)),
    )
}

fn check_coefficients(transition: &Matrix, reward: &Matrix) -> Result<(), String> {
    for row in transition {
        if row.iter().any(|v| !v.is_finite() || *v < 0.0)
            || (row.iter().sum::<f64>() - 1.0).abs() > MASS_TOLERANCE
        {
            return Err("credit operator violated nonnegative probability mass".into());
        }
    }
    if reward.iter().flatten().any(|v| !v.is_finite()) {
        return Err("credit earned-interest operator is nonfinite".into());
    }
    Ok(())
}

pub fn compile(request: &CreditOperatorRequest) -> Result<CreditOperator, String> {
    if request.schema != SCHEMA {
        return Err("unsupported credit operator schema".into());
    }
    if !request.duration_years.is_finite()
        || !(0.0..=MAX_DURATION_YEARS).contains(&request.duration_years)
    {
        return Err("operator duration must be finite in 0..100 model years".into());
    }
    let mut maximum_exit: f64 = 0.0;
    for (origin, row) in request.transition_hazards_per_year.iter().enumerate() {
        if row.iter().any(|v| !v.is_finite() || *v < 0.0)
            || row[origin] != 0.0
            || row.iter().sum::<f64>() > MAX_ROW_HAZARD
        {
            return Err(format!(
                "operator hazards require zero diagonal and outgoing sum <={MAX_ROW_HAZARD}/year"
            ));
        }
        maximum_exit = maximum_exit.max(row.iter().sum());
    }
    if request
        .accrual_rates
        .iter()
        .any(|v| !v.is_finite() || v.abs() > MAX_ABS_ACCRUAL_RATE)
    {
        return Err("signed accrual rates must be finite with magnitude <=100/year".into());
    }
    let maximum_reward = request
        .accrual_rates
        .iter()
        .map(|v| v.abs())
        .fold(0.0, f64::max);
    let lambda = maximum_exit.max(maximum_reward);
    let (transition, earned_interest) = if maximum_exit == 0.0 || request.duration_years == 0.0 {
        let mut reward = [[0.0; STATES]; STATES];
        for (i, rate) in request.accrual_rates.iter().enumerate() {
            reward[i][i] = rate * request.duration_years;
        }
        (identity(), reward)
    } else {
        let mut step = identity();
        let mut coupling = [[0.0; STATES]; STATES];
        for (origin, row) in request.transition_hazards_per_year.iter().enumerate() {
            step[origin][origin] = 1.0 - row.iter().sum::<f64>() / lambda;
            for (destination, rate) in row.iter().enumerate().take(3) {
                if destination != origin {
                    step[origin][destination] = rate / lambda;
                }
            }
            step[origin][3 + origin] = row[3] / lambda;
            step[origin][6] = row[4] / lambda;
            coupling[origin][origin] = request.accrual_rates[origin] / lambda;
        }
        let chunks = (lambda * request.duration_years / 8.0).ceil().max(1.0) as usize;
        // <=1250 chunks, <=128 series terms, <=11 binary composition levels.
        if chunks > 1250 {
            return Err("credit operator composition budget exceeded".into());
        }
        let mean = lambda * request.duration_years / chunks as f64;
        let coupling_norm = maximum_reward / lambda;
        // Bound accumulated truncation in P and F over chunk composition, separately
        // from f64 roundoff. Relative mean scaling protects tiny exit/reward amounts.
        let tolerance = TRUNCATION_TOLERANCE * mean.min(1.0)
            / (chunks as f64 * (1.0 + maximum_reward * request.duration_years));
        let mut power_p = identity();
        let mut power_f = [[0.0; STATES]; STATES];
        let mut weight = (-mean).exp();
        let mut chunk_p = [[0.0; STATES]; STATES];
        scaled_add(&mut chunk_p, &power_p, weight);
        let mut chunk_f = [[0.0; STATES]; STATES];
        let mut converged = false;
        for order in 1..=MAX_POISSON_TERMS {
            power_f = add(&product(&power_p, &coupling), &product(&power_f, &step));
            power_p = product(&power_p, &step);
            weight *= mean / order as f64;
            scaled_add(&mut chunk_p, &power_p, weight);
            scaled_add(&mut chunk_f, &power_f, weight);
            let first_omitted = weight * mean / (order + 1) as f64;
            let ratio = mean / (order + 2) as f64;
            if ratio < 1.0 {
                let denominator = 1.0 - ratio;
                let probability_tail = first_omitted / denominator;
                // ||F_n||_infinity <= n*||R/lambda||_infinity, including signed rates.
                let reward_tail = coupling_norm
                    * first_omitted
                    * ((order + 1) as f64 / denominator + ratio / (denominator * denominator));
                if probability_tail + reward_tail <= tolerance {
                    converged = true;
                    break;
                }
            }
        }
        if !converged {
            return Err("credit operator exceeded its Poisson reward-tail bound".into());
        }
        check_coefficients(&chunk_p, &chunk_f)?;
        let mut accumulated = (identity(), [[0.0; STATES]; STATES]);
        let mut power = (chunk_p, chunk_f);
        let mut remaining = chunks;
        while remaining > 0 {
            if remaining & 1 == 1 {
                accumulated = compose(&accumulated, &power);
            }
            remaining >>= 1;
            if remaining > 0 {
                power = compose(&power, &power);
            }
        }
        accumulated
    };
    check_coefficients(&transition, &earned_interest)?;
    Ok(CreditOperator {
        schema: SCHEMA.into(),
        measure: request.measure,
        duration_years: request.duration_years,
        transition,
        earned_interest,
    })
}

fn vector_product(vector: &[f64; STATES], matrix: &Matrix) -> [f64; STATES] {
    let mut result = [0.0; STATES];
    for (i, &amount) in vector.iter().enumerate() {
        for (j, value) in result.iter_mut().enumerate() {
            *value += amount * matrix[i][j];
        }
    }
    result
}

fn validate_amounts(vector: &[f64; STATES], signed: bool) -> Result<(), String> {
    if vector
        .iter()
        .any(|v| !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT || (!signed && *v < 0.0))
        || vector.iter().map(|v| v.abs()).sum::<f64>() > MAX_MONETARY_AMOUNT
    {
        return Err(
            "operator monetary vectors exceed finite signed/nonnegative currency admission".into(),
        );
    }
    Ok(())
}

impl CreditOperatorState {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != STATE_SCHEMA {
            return Err("unsupported credit operator state schema".into());
        }
        if self.monetary_unit.trim().is_empty()
            || self.monetary_unit.len() > 64
            || self.monetary_unit.chars().any(char::is_control)
        {
            return Err(
                "operator monetary_unit requires bounded nonempty text without controls".into(),
            );
        }
        if self.probability.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.probability.iter().sum::<f64>() > 1.0 + MASS_TOLERANCE
        {
            return Err("operator probabilities require nonnegative absolute mass <=1".into());
        }
        validate_amounts(&self.principal, false)?;
        validate_amounts(&self.undrawn_commitment, false)?;
        validate_amounts(&self.accrued_interest, true)?;
        validate_amounts(&self.book_adjustment, true)
    }
}

impl CreditOperator {
    pub fn measure(&self) -> CreditMeasure {
        self.measure
    }
    pub fn duration_years(&self) -> f64 {
        self.duration_years
    }
    pub fn transition(&self) -> &Matrix {
        &self.transition
    }
    pub fn earned_interest(&self) -> &Matrix {
        &self.earned_interest
    }

    pub fn apply(&self, state: &CreditOperatorState) -> Result<CreditOperatorState, String> {
        state.validate()?;
        let mut result = state.clone();
        self.apply_in_place(&mut result)?;
        Ok(result)
    }

    /// Allocation-free reusable daily path. Failure commits none of the new vectors.
    pub fn apply_in_place(&self, state: &mut CreditOperatorState) -> Result<(), String> {
        state.validate()?;
        if state.measure != self.measure {
            return Err("credit operator and state probability measures differ".into());
        }
        if self.duration_years == 0.0 {
            return Ok(()); // Preserve zero-duration signed zeros and metadata exactly.
        }
        let probability = vector_product(&state.probability, &self.transition);
        let original_mass = state.probability.iter().sum::<f64>();
        if (probability.iter().sum::<f64>() - original_mass).abs() > MASS_TOLERANCE {
            return Err("credit operator application changed absolute population mass".into());
        }
        let mut accrued_interest = vector_product(&state.accrued_interest, &self.transition);
        let earned = vector_product(&state.principal, &self.earned_interest);
        for (index, amount) in accrued_interest.iter_mut().enumerate() {
            *amount += earned[index];
        }
        let principal = vector_product(&state.principal, &self.transition);
        let undrawn_commitment = vector_product(&state.undrawn_commitment, &self.transition);
        let book_adjustment = vector_product(&state.book_adjustment, &self.transition);
        if probability.iter().any(|v| !v.is_finite() || *v < 0.0)
            || probability.iter().sum::<f64>() > 1.0 + MASS_TOLERANCE
        {
            return Err("operator output probability admission failed".into());
        }
        validate_amounts(&principal, false)?;
        validate_amounts(&undrawn_commitment, false)?;
        validate_amounts(&accrued_interest, true)?;
        validate_amounts(&book_adjustment, true)?;
        state.probability = probability;
        state.principal = principal;
        state.undrawn_commitment = undrawn_commitment;
        state.accrued_interest = accrued_interest;
        state.book_adjustment = book_adjustment;
        Ok(())
    }

    /// Apply P to a separate signed monetary component without probability weighting.
    pub fn transport_signed(&self, values: &[f64; STATES]) -> Result<[f64; STATES], String> {
        validate_amounts(values, true)?;
        let result = vector_product(values, &self.transition);
        validate_amounts(&result, true)?;
        Ok(result)
    }
}
