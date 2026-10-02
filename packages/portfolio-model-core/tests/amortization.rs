use portfolio_model_core::amortization::level_payment_principal;

#[test]
fn public_helper_validates_its_own_input_domain() {
    for coupon in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(level_payment_principal(coupon, &[0.25]).is_err());
    }
    assert!(level_payment_principal(0., &[]).is_err());
    assert!(level_payment_principal(0., &vec![0.25; 4097]).is_err());
    for tau in [0., -0.25, f64::NAN, f64::INFINITY] {
        assert!(level_payment_principal(0.06, &[tau]).is_err());
    }
    assert!(level_payment_principal(-4., &[0.25]).is_err());
    assert!(level_payment_principal(f64::MAX, &[2.]).is_err());
    // Finite inputs may nevertheless produce an unrepresentable payment.
    assert!(level_payment_principal(-0.999999, &vec![1.; 100]).is_err());
}

#[test]
fn arbitrary_accruals_match_direct_discounted_payment_and_balances() {
    let taus = [0.04, 0.25, 0.2, 0.3, 0.22];
    for coupon in [-1., -0.03, 0., 0.03, 0.25] {
        let principal = level_payment_principal(coupon, &taus).unwrap();
        let mut factor = 1.;
        let mut annuity = 0.;
        for tau in &taus {
            factor *= 1. + coupon * tau;
            annuity += 1. / factor;
        }
        let payment = 1. / annuity;
        let mut balance = 1.;
        for (&tau, &repayment) in taus.iter().zip(&principal) {
            assert!(repayment >= 0.);
            let actual_payment = coupon * tau * balance + repayment;
            assert!((actual_payment - payment).abs() < 1e-14);
            balance -= repayment;
        }
        assert!(balance.abs() < 1e-15);
        assert!((principal.iter().sum::<f64>() - 1.).abs() < 1e-15);
    }
}

#[test]
fn bounded_schedule_limit_is_stable_near_zero_and_for_long_positive_terms() {
    let taus = vec![1. / 12.; 4096];
    for coupon in [0., 1e-12, -1e-12, 0.06] {
        let principal = level_payment_principal(coupon, &taus).unwrap();
        let rate: f64 = coupon / 12.;
        let payment = if coupon == 0. {
            1. / 4096.
        } else {
            rate / -(-4096. * rate.ln_1p()).exp_m1()
        };
        let mut balance = 1.;
        for repayment in principal {
            assert!(repayment >= 0.);
            assert!((repayment + rate * balance - payment).abs() < 2e-13);
            balance -= repayment;
        }
        assert!(balance.abs() < 1e-15);
    }
}
