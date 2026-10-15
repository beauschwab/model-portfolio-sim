//! Exact interval carry/reward exponential; no payment, recovery or float ownership.
//! Private carry transport receives the actual same-loop native generator receipt.
use crate::{
    credit::CreditMeasure,
    credit_accounting::Identity,
    credit_operator::{self, CreditOperator, CreditOperatorRequest, CreditOperatorState},
    limits::MAX_MONETARY_AMOUNT,
};
use serde::{Deserialize, Serialize};
const S: usize = 7;
const N: usize = 14;
const MAX_TERMS: usize = 128;
const MAX_CHUNKS: usize = 64;
type Matrix = [[f64; S]; S];
type Block = [[f64; N]; N];
pub const SCHEMA: &str = "mortgage-coupled-carry-1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub original_income: String,
    /// Only an interval before entitlement is admitted. Float stays with a separate owner.
    pub remittance_float: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub generator_source: Identity,
    pub measure: CreditMeasure,
    pub duration_years: f64,
    /// Row-vector generator, same7states as the shared native credit operator.
    /// Artifact boundary only; production must obtain this from a native mortgage receipt.
    pub generator: Matrix,
    pub recognized_contractual_rates: [f64; 3],
    pub original_annual_effective_rate: f64,
    pub policy: Policy,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub probability: [f64; S],
    pub principal: [f64; S],
    pub accrued_interest: [f64; S],
    pub gross_carrying: [f64; S],
    /// Signed accumulated EIR reward routed through the same ending state/origin.
    pub earned_eir: [f64; S],
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub closing: State,
    pub book_adjustment: [f64; S],
    pub interval_earned_eir: [f64; S],
    pub total_interval_earned_eir: f64,
    pub gross_conservation_error: f64,
    pub native_mortgage_generator_verified: bool,
}
#[derive(Debug, Serialize)]
pub struct Operator {
    request: Request,
    base: CreditOperator,
    growth: Matrix,
    absolute_reward: Matrix,
    reward_sign: f64,
    growth_bound: f64,
    work_multiplications: usize,
}
fn text(x: &Identity) -> Result<(), String> {
    for v in [&x.id, &x.revision] {
        if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
            return Err("coupled carry immutable bounded policy/source identity required".into());
        }
    }
    Ok(())
}
fn identity() -> Block {
    let mut b = [[0.; N]; N];
    for (i, row) in b.iter_mut().enumerate() {
        row[i] = 1.;
    }
    b
}
fn multiply(a: &Block, b: &Block) -> Block {
    let mut c = [[0.; N]; N];
    for (i, row) in c.iter_mut().enumerate() {
        for (k, &left) in a[i].iter().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v += left * b[k][j];
            }
        }
    }
    c
}
fn norm(a: &Block) -> f64 {
    a.iter()
        .map(|r| r.iter().map(|x| x.abs()).sum::<f64>())
        .fold(0., f64::max)
}
fn add_scaled(a: &mut Block, b: &Block, weight: f64) {
    for (a, b) in a.iter_mut().zip(b) {
        for (a, b) in a.iter_mut().zip(b) {
            *a += weight * b;
        }
    }
}
fn action(v: &[f64; S], p: &Matrix) -> [f64; S] {
    let mut out = [0.; S];
    for (&left, row) in v.iter().zip(p) {
        for (v, &right) in out.iter_mut().zip(row) {
            *v += left * right;
        }
    }
    out
}
/// Uniformized augmented exponential exp(t*[Q+kL, |k|L;0,Q]).
/// k<0 uses a signed reward after this nonnegative coefficient calculation.
fn exponential(q: &Matrix, k: f64, t: f64) -> Result<(Matrix, Matrix, usize), String> {
    let exits = (0..3).map(|i| -q[i][i]).fold(0., f64::max);
    let lambda = exits + k.abs();
    if lambda == 0. || t == 0. {
        let mut p = [[0.; S]; S];
        for (i, row) in p.iter_mut().enumerate() {
            row[i] = 1.;
        }
        return Ok((p, [[0.; S]; S], 0));
    }
    let chunks = (lambda * t / 8.).ceil().max(1.) as usize;
    if chunks > MAX_CHUNKS {
        return Err("coupled carry exponential chunk budget exceeded".into());
    }
    let mut step = identity();
    for i in 0..S {
        for j in 0..S {
            step[i][j] += q[i][j] / lambda;
            step[S + i][S + j] += q[i][j] / lambda;
        }
        if i < 3 {
            step[i][i] += k / lambda;
            step[i][S + i] = k.abs() / lambda;
        }
    }
    // One-ulp cancellation in diagonal construction must not create a negative kernel.
    // Recompute only the mathematically nonnegative diagonal from its exact numerator.
    for i in 0..3 {
        step[i][i] = ((exits + q[i][i]) + (k.abs() + k)) / lambda;
        step[S + i][S + i] = ((exits + q[i][i]) + k.abs()) / lambda;
    }
    if step.iter().flatten().any(|v| !v.is_finite() || *v < 0.) {
        return Err("coupled carry shifted augmented kernel is not nonnegative".into());
    }
    let mean = lambda * t / chunks as f64;
    let z = mean * norm(&step);
    // Logarithmic norm of the nonnegative augmented block <=2max(k,0).
    let propagated_bound = (2. * k.max(0.) * t).exp();
    let tolerance = 1e-16 / (chunks as f64 * propagated_bound);
    let mut power = identity();
    let mut weight = (-mean).exp();
    let mut chunk = [[0.; N]; N];
    add_scaled(&mut chunk, &power, weight);
    let mut multiplications = 0usize;
    let mut converged = false;
    for n in 1..=MAX_TERMS {
        power = multiply(&power, &step);
        multiplications += 1;
        weight *= mean / n as f64;
        add_scaled(&mut chunk, &power, weight);
        let ratio = z / (n + 1) as f64;
        if ratio < 1. {
            let remainder = weight * norm(&power) * ratio / (1. - ratio);
            if remainder <= tolerance {
                converged = true;
                break;
            }
        }
    }
    if !converged {
        return Err("coupled carry augmented Poisson tail did not converge".into());
    }
    let mut out = identity();
    let mut count = chunks;
    while count > 0 {
        if count & 1 != 0 {
            out = multiply(&out, &chunk);
            multiplications += 1;
        }
        count >>= 1;
        if count > 0 {
            chunk = multiply(&chunk, &chunk);
            multiplications += 1;
        }
    }
    if out.iter().flatten().any(|v| !v.is_finite() || *v < 0.) {
        return Err("coupled carry exponential invalid".into());
    }
    let mut p = [[0.; S]; S];
    let mut reward = [[0.; S]; S];
    for i in 0..S {
        for j in 0..S {
            p[i][j] = out[i][j];
            reward[i][j] = out[i][S + j];
        }
        if i >= 3 {
            // Exactly absorbing rights have no growth/reward under stop_at_exit.
            // Set the known identity coefficient rather than sum its Poisson1 series.
            p[i] = [0.; S];
            p[i][i] = 1.;
            reward[i] = [0.; S];
        }
    }
    Ok((p, reward, multiplications))
}

pub fn compile(r: &Request) -> Result<Operator, String> {
    text(&r.generator_source)?;
    text(&r.policy.identity)?;
    if r.schema != SCHEMA
        || r.policy.original_income != "stop_at_exit-1"
        || r.policy.remittance_float != "external_explicit_owner_required-1"
        || !r.duration_years.is_finite()
        || !(0. ..=1080. / 365.).contains(&r.duration_years)
        || !r.original_annual_effective_rate.is_finite()
        || !(-0.99..=10.).contains(&r.original_annual_effective_rate)
    {
        return Err("explicit bounded original income and external float policy required".into());
    }
    let q = &r.generator;
    let mut hazards = [[0.; 5]; 3];
    for (i, row) in q.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            if !value.is_finite()
                || (i != j && *value < 0.)
                || (i >= 3 && *value != 0.)
                || (i < 3 && j >= 3 && j != 3 + i && j != 6 && *value != 0.)
            {
                return Err(
                    "supplied generator violates native7state/default-origin structure".into(),
                );
            }
        }
        if i < 3 {
            hazards[i][..3].copy_from_slice(&q[i][..3]);
            hazards[i][i] = 0.;
            hazards[i][3] = q[i][3 + i];
            hazards[i][4] = q[i][6];
            if q[i][i] != -hazards[i].iter().sum::<f64>() {
                return Err(
                    "supplied generator diagonal must equal unchanged native hazard sum".into(),
                );
            }
        }
    }
    // P/A use the existing native compiler/application exactly, not a reconstructed generator.
    let base = credit_operator::compile(&CreditOperatorRequest {
        schema: credit_operator::SCHEMA.into(),
        measure: r.measure,
        duration_years: r.duration_years,
        transition_hazards_per_year: hazards,
        accrual_rates: r.recognized_contractual_rates,
    })?;
    let k = r.original_annual_effective_rate.ln_1p();
    let (growth, absolute_reward, work_multiplications) = if k == 0. {
        (*base.transition(), [[0.; S]; S], 0)
    } else {
        exponential(q, k, r.duration_years)?
    };
    Ok(Operator {
        request: r.clone(),
        base,
        growth,
        absolute_reward,
        reward_sign: k.signum(),
        growth_bound: (k.max(0.) * r.duration_years).exp(),
        work_multiplications,
    })
}
fn vector(v: &[f64; S], signed: bool) -> Result<(), String> {
    if v.iter()
        .any(|v| !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT || (!signed && *v < 0.))
        || v.iter().map(|v| v.abs()).sum::<f64>() > MAX_MONETARY_AMOUNT
    {
        Err("coupled carry state monetary admission exceeded".into())
    } else {
        Ok(())
    }
}
impl Operator {
    pub fn apply(&self, s: &State) -> Result<ResultData, String> {
        if s.measure != self.request.measure || s.monetary_unit != "currency_units" {
            return Err("coupled carry state measure/actual units mismatch".into());
        }
        vector(&s.gross_carrying, false)?;
        vector(&s.earned_eir, true)?;
        if s.gross_carrying.iter().sum::<f64>() * self.growth_bound > MAX_MONETARY_AMOUNT {
            return Err("coupled carry growth monetary preflight exceeded".into());
        }
        let mut basis = [0.; S];
        for (i, b) in basis.iter_mut().enumerate() {
            *b = s.gross_carrying[i] - s.principal[i] - s.accrued_interest[i];
        }
        let source = CreditOperatorState {
            schema: credit_operator::STATE_SCHEMA.into(),
            measure: s.measure,
            monetary_unit: s.monetary_unit.clone(),
            probability: s.probability,
            principal: s.principal,
            undrawn_commitment: [0.; S],
            accrued_interest: s.accrued_interest,
            book_adjustment: basis,
        };
        source.validate()?;
        if self.request.duration_years == 0. {
            return Ok(ResultData {
                closing: s.clone(),
                book_adjustment: basis,
                interval_earned_eir: [0.; S],
                total_interval_earned_eir: 0.,
                gross_conservation_error: 0.,
                native_mortgage_generator_verified: false,
            });
        }
        let transported = self.base.apply(&source)?;
        let gross = action(&s.gross_carrying, &self.growth);
        let mut interval = action(&s.gross_carrying, &self.absolute_reward);
        let mut earned = action(&s.earned_eir, self.base.transition());
        for i in 0..S {
            interval[i] *= self.reward_sign;
            earned[i] += interval[i];
            basis[i] = gross[i] - transported.principal[i] - transported.accrued_interest[i];
        }
        vector(&gross, false)?;
        vector(&interval, true)?;
        vector(&earned, true)?;
        vector(&basis, true)?;
        let total = interval.iter().sum::<f64>();
        let error = gross.iter().sum::<f64>() - s.gross_carrying.iter().sum::<f64>() - total;
        if error.abs() > 1e-9 + 1e-12 * s.gross_carrying.iter().sum::<f64>().abs() {
            return Err("coupled gross carry/EIR reward conservation failed".into());
        }
        Ok(ResultData {
            closing: State {
                measure: s.measure,
                monetary_unit: s.monetary_unit.clone(),
                probability: transported.probability,
                principal: transported.principal,
                accrued_interest: transported.accrued_interest,
                gross_carrying: gross,
                earned_eir: earned,
            },
            book_adjustment: basis,
            interval_earned_eir: interval,
            total_interval_earned_eir: total,
            gross_conservation_error: error,
            native_mortgage_generator_verified: false,
        })
    }
}
