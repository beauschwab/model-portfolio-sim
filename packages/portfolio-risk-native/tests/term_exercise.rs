use portfolio_model_core::cashflow::{
    ordinal_from_ymd, ActualDayCount, CashflowComponents, CashflowMeasure,
};
use portfolio_model_core::floating_rate::Direction;
use portfolio_risk_native::term_enhancements::{exercise::*, Identity, MODEL_VERSION, SCHEMA};
fn d(n: u32) -> i32 {
    ordinal_from_ymd(2026, 1, n).unwrap()
}
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn cash(p: f64, i: f64) -> CashflowComponents {
    CashflowComponents {
        principal: p,
        cash_interest: i,
        accrued_interest: i,
        book_amortization: 0.,
        fees: 0.,
    }
}
fn node(name: &str, date: i32, p: f64, interest: f64, branches: Vec<Branch>) -> PricingNode {
    PricingNode {
        node_id: name.into(),
        ordinal: date,
        information_set: id(name),
        principal_before_cash: 100.,
        scheduled_cash: cash(p, interest),
        scheduled_accrual_period: None,
        exercise_accrued_interest: 0.,
        accrued_right_source: id("rights"),
        branches,
    }
}
fn branch(child: usize, p: f64, df: f64) -> Branch {
    Branch {
        child,
        conditional_probability: p,
        market_discount_factor: df,
    }
}
fn request() -> ExerciseRequest {
    ExerciseRequest {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        contract: id("corporate"),
        policy: id("bellman-exercise"),
        tree: id("conditional-tree"),
        discount_source: id("market-discount"),
        oas_anchor: id("base-oas"),
        common_random_numbers: id("seed-tape"),
        measure: CashflowMeasure::Pricing,
        discount_basis: "market_without_oas".into(),
        position_id: "corporate:1".into(),
        scenario_id: "base".into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        direction: Direction::Receipt,
        asof_ordinal: d(1),
        reporting_end_ordinal: d(2),
        tail_end_ordinal: d(3),
        fixed_base_oas: 0.,
        oas_day_count: ActualDayCount::Act365Fixed,
        simultaneous_priority: SimultaneousPriority::Issuer,
        exercise_dates: vec![ExerciseDate {
            ordinal: d(2),
            timing: ExerciseTiming::AfterScheduledCash,
            call_clean_price_factor: Some(1.),
            put_clean_price_factor: None,
            call_fee: 0.,
            put_fee: 0.,
        }],
        nodes: vec![
            node(
                "root",
                d(1),
                0.,
                0.,
                vec![branch(1, 0.5, 0.98), branch(2, 0.5, 0.98)],
            ),
            node("low", d(2), 0., 5., vec![branch(3, 1., 1.)]),
            node("high", d(2), 0., 5., vec![branch(4, 1., 0.9)]),
            node("low-maturity", d(3), 100., 5., vec![]),
            node("high-maturity", d(3), 100., 5., vec![]),
        ],
    }
}
#[test]
fn cancellation_and_deadline_at_every_checkpoint_return_no_partial_policy() {
    let r = request();
    let mut checks = 0;
    let checked = value_exercise_with_check(&r, || {
        checks += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(checked).unwrap(),
        serde_json::to_value(value_exercise(&r).unwrap()).unwrap()
    );
    assert!(checks > 30);
    for reason in ["COMPUTE_CANCELLED", "COMPUTE_DEADLINE_EXCEEDED"] {
        for stop_at in 1..=checks {
            let mut count = 0;
            let failed = value_exercise_with_check(&r, || {
                count += 1;
                if count == stop_at {
                    Err(reason.into())
                } else {
                    Ok(())
                }
            });
            assert_eq!(failed.unwrap_err(), reason);
        }
    }
}
#[test]
fn hand_conditional_call_stops_one_branch_and_preserves_coupon_principal_tail() {
    let r = request();
    let x = value_exercise(&r).unwrap();
    // Low continuation 5+105=110, call after paid coupon costs 5+100.
    // High continuation 5+.9*105=99.5, so issuer does not call.
    close(x.value, 0.98 * (105. + 99.5) / 2.);
    assert_eq!(x.nodes[1].decision, Decision::Call);
    assert_eq!(x.nodes[2].decision, Decision::Continue);
    close(x.nodes[3].live_pricing_probability, 0.);
    close(x.nodes[4].live_pricing_probability, 0.5);
    close(x.expected_principal_after_reporting, 50.);
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        100.,
    );
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.cash_interest)
            .sum(),
        7.5,
    );
    assert!(x
        .schedule
        .events
        .iter()
        .all(|e| e.measure == CashflowMeasure::Pricing));
}
#[test]
fn option_free_put_bounds_signed_valuation_and_same_date_cash_order() {
    let mut r = request();
    r.exercise_dates.clear();
    let straight = value_exercise(&r).unwrap();
    close(straight.value, 0.98 * (110. + 99.5) / 2.);
    r = request();
    let called = value_exercise(&r).unwrap();
    assert!(called.value < straight.value);
    r.exercise_dates[0].call_clean_price_factor = None;
    r.exercise_dates[0].put_clean_price_factor = Some(1.);
    let put = value_exercise(&r).unwrap();
    close(put.value, 0.98 * (110. + 105.) / 2.);
    assert!(put.value > straight.value);
    r = request();
    r.exercise_dates[0].timing = ExerciseTiming::BeforeScheduledCash;
    let before = value_exercise(&r).unwrap();
    close(before.value, 0.98 * (100. + 99.5) / 2.);
    r.direction = Direction::Payment;
    close(value_exercise(&r).unwrap().value, -before.value);
    // A before-cash accrued entitlement of 5 pays it once and restores after-cash result.
    r.direction = Direction::Receipt;
    r.nodes[1].exercise_accrued_interest = 5.;
    r.nodes[2].exercise_accrued_interest = 5.;
    close(value_exercise(&r).unwrap().value, called.value);
}
#[test]
fn conditional_expectation_prevents_perfect_foresight_and_ties_continue() {
    let mut r = request();
    r.exercise_dates = vec![ExerciseDate {
        ordinal: d(1),
        timing: ExerciseTiming::BeforeScheduledCash,
        call_clean_price_factor: Some(1.),
        put_clean_price_factor: None,
        call_fee: 0.,
        put_fee: 0.,
    }];
    r.nodes = vec![
        node(
            "root",
            d(1),
            0.,
            0.,
            vec![branch(1, 0.5, 1.), branch(2, 0.5, 1.)],
        ),
        node("up", d(3), 100., 20., vec![]),
        node("down", d(3), 100., -20., vec![]),
    ];
    let x = value_exercise(&r).unwrap();
    close(x.value, 100.);
    assert_eq!(x.nodes[0].decision, Decision::Continue);
    // Wrong foresight would produce .5*min(100,120)+.5*min(100,80)=90.
    assert!(x.value > 90.);
}
#[test]
fn simultaneous_right_priority_is_explicit_and_clean_premium_is_not_face_principal() {
    let mut r = request();
    r.exercise_dates[0].call_clean_price_factor = Some(0.9);
    r.exercise_dates[0].put_clean_price_factor = Some(1.1);
    let issuer = value_exercise(&r).unwrap();
    assert_eq!(issuer.nodes[1].decision, Decision::Call);
    close(issuer.value, 0.98 * 95.);
    close(
        issuer
            .schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        100.,
    );
    close(
        issuer
            .schedule
            .events
            .iter()
            .map(|e| e.components.fees)
            .sum(),
        -10.,
    );
    r.simultaneous_priority = SimultaneousPriority::Holder;
    let holder = value_exercise(&r).unwrap();
    assert_eq!(holder.nodes[1].decision, Decision::Put);
    close(holder.value, 0.98 * 115.);
}
#[test]
fn fixed_base_oas_applied_once_with_identity_and_deterministic_refinement() {
    let mut r = request();
    r.exercise_dates.clear();
    r.fixed_base_oas = 0.02;
    let t = 1. / 365.;
    let expected = 0.98
        * (-0.02_f64 * t).exp()
        * (5. + 105. * (-0.02_f64 * t).exp() + 5. + 94.5 * (-0.02_f64 * t).exp())
        / 2.;
    let x = value_exercise(&r).unwrap();
    close(x.value, expected);
    assert_eq!(x.oas_anchor, r.oas_anchor);
    assert_eq!(x.common_random_numbers, r.common_random_numbers);
    // Subdivision of a deterministic market discount changes no economics.
    r.fixed_base_oas = 0.;
    r.nodes = vec![
        node("root", d(1), 0., 0., vec![branch(1, 1., 0.81)]),
        node("maturity", d(3), 100., 5., vec![]),
    ];
    let whole = value_exercise(&r).unwrap().value;
    r.nodes = vec![
        node("root", d(1), 0., 0., vec![branch(1, 1., 0.9)]),
        node("subdivision", d(2), 0., 0., vec![branch(2, 1., 0.9)]),
        node("maturity", d(3), 100., 5., vec![]),
    ];
    close(value_exercise(&r).unwrap().value, whole);
}
fn lognormal_cap(n: usize) -> f64 {
    let mut r = request();
    r.exercise_dates[0].timing = ExerciseTiming::BeforeScheduledCash;
    let sigma = 0.3;
    let step = sigma / (n as f64).sqrt();
    let normalization = step.cosh().powi(n as i32);
    let mut probabilities = Vec::with_capacity(n + 1);
    let mut prob = 2_f64.powi(-(n as i32));
    for k in 0..=n {
        probabilities.push(prob);
        if k < n {
            prob *= ((n - k) as f64) / ((k + 1) as f64);
        }
    }
    let root = node(
        "root",
        d(1),
        0.,
        0.,
        probabilities
            .iter()
            .enumerate()
            .map(|(k, p)| branch(k + 1, *p, 1.))
            .collect(),
    );
    let mut nodes = vec![root];
    for k in 0..=n {
        nodes.push(node(
            &format!("state:{k}"),
            d(2),
            0.,
            0.,
            vec![branch(n + 2 + k, 1., 1.)],
        ));
    }
    for k in 0..=n {
        let terminal = 100. * (step * (2. * k as f64 - n as f64)).exp() / normalization;
        nodes.push(node(
            &format!("terminal:{k}"),
            d(3),
            100.,
            terminal - 100.,
            vec![],
        ));
    }
    r.nodes = nodes;
    value_exercise(&r).unwrap().value
}
#[test]
fn stochastic_state_refinement_converges_to_independent_lognormal_call_bound() {
    // E[min(100,100 exp(.3 Z-.3^2/2))] = 200 Phi(-.15).
    // Independent standard normal table value Phi(-.15)=.4403823076297575.
    let analytic = 88.0764615259515;
    let coarse = lognormal_cap(16);
    let medium = lognormal_cap(64);
    let fine = lognormal_cap(256);
    assert!((medium - analytic).abs() < (coarse - analytic).abs());
    assert!((fine - analytic).abs() < (medium - analytic).abs());
    assert!((fine - analytic).abs() < 0.02, "{fine} vs {analytic}");
}
#[test]
fn unsupported_tail_branch_measure_and_work_domains_reject() {
    let mut r = request();
    r.nodes[4].scheduled_cash.principal = 99.;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.exercise_dates[0].ordinal = d(2) + 1;
    r.nodes[3].ordinal = d(2);
    assert!(value_exercise(&r).is_err());
    r = request();
    r.nodes[0].branches[0].conditional_probability = 0.6;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.nodes[0].branches[0].child = 0;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.measure = CashflowMeasure::Physical;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.discount_basis = "market_including_oas".into();
    assert!(value_exercise(&r).is_err());
    r = request();
    r.nodes[0].branches[0].market_discount_factor = f64::NAN;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.nodes[2].principal_before_cash = 99.;
    assert!(value_exercise(&r).is_err());
    r = request();
    r.tail_end_ordinal = d(2);
    assert!(value_exercise(&r).is_err());
    r = request();
    r.nodes = vec![r.nodes[0].clone(); MAX_TREE_NODES + 1];
    assert!(value_exercise(&r).is_err());
}
