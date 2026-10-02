use portfolio_risk_native::conventions::{Date, DayCount};
use portfolio_risk_native::term_deck::{Amortization, Deck, DeckRequest, Product};
use serde_json::json;

fn date(y: i32, m: i32, d: i32) -> i32 {
    Date::new(y, m, d).unwrap().number()
}

fn request(coupon: f64, maturity: i32, freq: i32, daycount: &str) -> DeckRequest {
    serde_json::from_value(json!({
        "product": "corporate", "asof": date(2026, 1, 1), "months": 72,
        "calendar": {"name": "NONE"}, "bdc": "NONE",
        "contracts": [{"maturity": maturity, "freq_months": freq,
            "daycount": daycount, "coupon": coupon, "notional": 100_000.,
            "price": 100., "amort_type": "level_payment"}]
    }))
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 2e-13,
        "actual={actual:.17}, expected={expected:.17}"
    );
}

fn check_cashflows(deck: &Deck, coupon: f64, payment: f64) {
    let mut balance = 1.;
    for (&principal, &tau) in deck.prin.iter().zip(&deck.tau) {
        close(principal + coupon * tau * balance, payment);
        assert!(principal >= 0. && principal <= balance + 1e-14);
        balance -= principal;
    }
    close(balance, 0.);
    close(deck.prin.iter().sum(), 1.);
}

#[test]
fn regular_payments_match_independent_closed_form() {
    for coupon in [0.06_f64, -0.01, 1e-12, -1e-12] {
        let deck = request(coupon, date(2031, 1, 1), 3, "30/360")
            .build()
            .unwrap();
        assert_eq!(deck.prin.len(), 20);
        assert!(deck.tau.iter().all(|t| *t == 0.25));
        let r = coupon / 4.;
        // Analytic annuity, evaluated stably for tiny rates independently of
        // the production backward annuity recurrence.
        let payment = r / -(-20. * r.ln_1p()).exp_m1();
        check_cashflows(&deck, coupon, payment);
        let mut balance = 1.;
        for &principal in &deck.prin {
            let expected = payment - r * balance;
            close(principal, expected);
            balance -= expected;
        }
    }
}

#[test]
fn zero_coupon_has_equal_principal_and_equal_payment() {
    let deck = request(0., date(2031, 1, 1), 3, "30/360").build().unwrap();
    check_cashflows(&deck, 0., 1. / 20.);
    for &principal in &deck.prin {
        close(principal, 1. / 20.);
    }
}

#[test]
fn irregular_stub_payments_match_independent_forward_recurrence() {
    let req = request(0.08, date(2027, 3, 17), 3, "ACT/365F");
    let deck = req.build().unwrap();
    // Backward schedule creates a short initial stub, followed by calendar
    // quarters of unequal length. Hand dates do not call Calendar::schedule.
    let dates = [
        date(2026, 1, 1),
        date(2026, 3, 17),
        date(2026, 6, 17),
        date(2026, 9, 17),
        date(2026, 12, 17),
        date(2027, 3, 17),
    ];
    let taus: Vec<f64> = dates
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64 / 365.)
        .collect();
    assert_eq!(deck.tau, taus);
    let mut cumulative_factor = 1.;
    let mut annuity = 0.;
    for tau in &taus {
        cumulative_factor *= 1. + 0.08 * tau;
        annuity += 1. / cumulative_factor;
    }
    let payment = 1. / annuity;
    let mut balance = 1.;
    for (&tau, &principal) in taus.iter().zip(&deck.prin) {
        let next = balance * (1. + 0.08 * tau) - payment;
        close(principal, balance - next);
        balance = next;
    }
    close(balance, 0.);
    check_cashflows(&deck, 0.08, payment);
}

#[test]
fn legacy_annuity_retains_exact_equal_principal_and_grid_clamping() {
    let mut req = request(0.06, date(2031, 1, 1), 3, "30/360");
    req.contracts[0].amort_type = Amortization::Annuity;
    req.months = 12;
    let legacy = req.build().unwrap();
    assert!(legacy.prin.iter().all(|p| *p == 1. / 20.));
    assert_eq!(*legacy.t_pay.last().unwrap(), 1. - 1e-9);
    let serialized = serde_json::to_value(&req.contracts[0]).unwrap();
    assert_eq!(serialized["amort_type"], "annuity");
    // New contracts cannot silently compress a five-year repayment schedule
    // into a one-year market grid.
    req.contracts[0].amort_type = Amortization::LevelPayment;
    assert!(req.build().err().unwrap().contains("terminal market grid"));
}

#[test]
fn terminal_residual_payoff_is_conserved_for_long_terms() {
    let mut req = request(0.22, date(2056, 1, 1), 1, "30/360");
    req.months = 365;
    let deck = req.build().unwrap();
    let outstanding = 1. - deck.prin[..deck.prin.len() - 1].iter().sum::<f64>();
    close(*deck.prin.last().unwrap(), outstanding);
    close(deck.prin.iter().sum(), 1.);
    let first_payment = deck.prin[0] + 0.22 * deck.tau[0];
    check_cashflows(&deck, 0.22, first_payment);
}

#[test]
fn irregular_high_coupon_negative_amortization_is_rejected() {
    let mut req = request(0.22, date(2056, 1, 1), 1, "ACT/365F");
    req.months = 365;
    // Independent direct discounting identifies the economic issue: with a
    // long/high-rate loan the first 31-day month's interest exceeds the level
    // amount implied by the shorter future months.
    let mut prior = Date::new(2026, 1, 1).unwrap();
    let mut product = 1.;
    let mut annuity = 0.;
    for months in 1..=360 {
        let end = Date::new(2026, 1, 1).unwrap().add_months(months).unwrap();
        let tau = (end.number() - prior.number()) as f64 / 365.;
        product *= 1. + 0.22 * tau;
        annuity += 1. / product;
        prior = end;
    }
    let payment = 1. / annuity;
    let first_interest = 0.22 * 31. / 365.;
    assert!(payment < first_interest);
    assert!(req.build().err().unwrap().contains("negative amortization"));
}

#[test]
fn unsupported_inputs_fail_explicitly() {
    let mut req = request(0.06, date(2027, 1, 1), 3, "30/360");
    req.contracts[0].is_float = true;
    assert!(req.build().err().unwrap().contains("fixed-rate corporate"));
    req.contracts[0].is_float = false;
    req.product = Product::Cd;
    assert!(req.build().err().unwrap().contains("fixed-rate corporate"));
    req.product = Product::Corporate;
    req.contracts[0].sink_schedule = Some(vec![]);
    assert!(req.build().err().unwrap().contains("sinking schedule"));
    req.contracts[0].sink_schedule = None;
    req.contracts[0].coupon = -4.;
    assert!(req.build().err().unwrap().contains("coupon*tau > 0"));
    req.contracts[0].coupon = -4.001;
    assert!(req.build().is_err());
    req.contracts[0].coupon = f64::NAN;
    assert!(req.build().is_err());
    req.contracts[0].coupon = f64::INFINITY;
    assert!(req.build().is_err());
    // Validate the selected day count is part of the contract identity.
    req.contracts[0].coupon = 0.;
    req.contracts[0].daycount = DayCount::Act360;
    assert!(req.build().is_ok());
}
