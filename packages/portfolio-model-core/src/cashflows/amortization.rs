//! Shared fully amortizing fixed-rate schedules on actual accrual fractions.
pub const MAX_AMORTIZATION_PERIODS: usize = 4096;

/// Normalized principal for a fully amortizing fixed-rate contract.
/// The remaining annuity value obeys V_i = (1 + V_{i+1}) / (1 + c*tau_i).
/// Log values and backward balance ratios avoid cancellation near zero coupon
/// and the forward recurrence's amplification of rounding error on long terms.
/// Admission is 1..=4096 periods, including for callers outside a term deck.
pub fn level_payment_principal(coupon: f64, taus: &[f64]) -> Result<Vec<f64>, String> {
    if !coupon.is_finite() {
        return Err("level_payment requires a finite coupon".into());
    }
    if taus.is_empty() || taus.len() > MAX_AMORTIZATION_PERIODS {
        return Err("level_payment requires 1..=4096 accrual periods".into());
    }
    let mut log_values = vec![f64::NEG_INFINITY; taus.len() + 1];
    for (i, &tau) in taus.iter().enumerate().rev() {
        let rate = coupon * tau;
        if !tau.is_finite() || tau <= 0. || !rate.is_finite() || rate <= -1. {
            return Err("level_payment requires positive accruals and 1 + coupon*tau > 0".into());
        }
        let next = log_values[i + 1];
        let log_sum = if next > 0. {
            next + (-next).exp().ln_1p()
        } else {
            next.exp().ln_1p()
        };
        log_values[i] = log_sum - rate.ln_1p();
    }
    let payment = (-log_values[0]).exp();
    if !payment.is_finite() || payment <= 0. {
        return Err("level_payment payment is outside the finite numerical domain".into());
    }
    let mut principal = Vec::with_capacity(taus.len());
    let mut remaining = 1.;
    for i in 0..taus.len() {
        let next = (log_values[i + 1] - log_values[0]).exp();
        if !next.is_finite() || next > remaining + 64. * f64::EPSILON {
            return Err("level_payment schedule requires unsupported negative amortization".into());
        }
        let next = next.min(remaining);
        principal.push(remaining - next);
        remaining = next;
    }
    // The terminal annuity is zero, so the final principal pays the remaining
    // balance exactly rather than replacing it with a nominal fixed instalment.
    Ok(principal)
}
