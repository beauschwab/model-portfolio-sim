//! Pure running coupon arithmetic. Source selection and recognition policy stay
//! with the native contractual owner; this type accepts no financial wire input.
#[derive(Debug, Clone, Copy, Default)]
pub struct CouponFactorState {
    simple: f64,
    log_factor: f64,
    margin_tau: f64,
}

impl CouponFactorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read-only native capture evidence; no restoration/input constructor.
    pub fn native_snapshot(&self) -> (f64, f64, f64) {
        (self.simple, self.log_factor, self.margin_tau)
    }

    /// Preserve the historical native expression and reduction order exactly.
    pub fn simple_observation(&mut self, all_in: f64, margin: f64, weight: f64, interest_tau: f64) {
        self.simple += (all_in - margin) * weight + margin * interest_tau;
    }

    /// The source owner validates the selected rate and positive factor first.
    pub fn compounded_observation(&mut self, increment: f64, interest_tau: f64) {
        self.log_factor += increment.ln_1p();
        self.margin_tau += interest_tau;
    }

    pub fn simple_factor(&self) -> f64 {
        self.simple
    }

    pub fn compounded_factor(&self, simple_margin: Option<f64>) -> f64 {
        self.log_factor.exp_m1() + simple_margin.map_or(0., |margin| margin * self.margin_tau)
    }

    pub fn fixed_factor(annual_rate: f64, tau: f64) -> f64 {
        annual_rate * tau
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_observations_keep_actual_shift_and_margin_weights() {
        let mut state = CouponFactorState::new();
        state.simple_observation(0.05, 0.01, 3. / 360., 2. / 360.);
        state.simple_observation(-0.005, 0.01, 1. / 360., 2. / 360.);
        let expected: f64 = ((0.05 - 0.01) * (3. / 360.) + 0.01 * (2. / 360.))
            + ((-0.005 - 0.01) * (1. / 360.) + 0.01 * (2. / 360.));
        assert_eq!(state.simple_factor().to_bits(), expected.to_bits());
    }

    #[test]
    fn compounding_retains_signed_and_zero_increments_and_actual_margin_clock() {
        let increments = [0.0002_f64, -0.0001, 0.];
        let mut state = CouponFactorState::new();
        for x in increments {
            state.compounded_observation(x, 1. / 365.);
        }
        let log = increments.iter().fold(0., |sum, x| sum + x.ln_1p());
        let margin_tau = (0..3).fold(0., |sum, _| sum + 1. / 365.);
        assert_eq!(
            state.compounded_factor(Some(-0.01)).to_bits(),
            (log.exp_m1() + -0.01 * margin_tau).to_bits()
        );
        let hand = (1. + increments[0]) * (1. + increments[1]) * (1. + increments[2]) - 1.;
        assert!((state.compounded_factor(None) - hand).abs() < 3e-16);
    }
}
