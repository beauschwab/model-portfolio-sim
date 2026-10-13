use portfolio_model_core::{cashflow::CashflowMeasure, deposit_state};
use portfolio_risk_native::{
    advanced_graph::Role,
    deposit_graph::{self, DepositGraph, Request},
};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/deposit-product-graph-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn paths(v: &mut Value, f: impl Fn(&mut Value)) {
    for set in v["positions"][0]["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in set[role].as_array_mut().unwrap() {
                f(&mut h["request"]);
            }
        }
    }
}
#[test]
fn native_liability_sign_full_tail_daily_expense_and_curve_bump_match_hand() {
    let r = request(raw());
    let graph = DepositGraph::new(&r).unwrap();
    assert_eq!(
        serde_json::to_value(graph.snapshot()).unwrap(),
        serde_json::to_value(graph.reprice(&r).unwrap()).unwrap()
    );
    let out = deposit_graph::analyze(&r).unwrap();
    let m = &out.metrics[0];
    near(m.liability_pv, 1002.);
    near(m.bank_cash_pv, -1002.);
    near(m.physical_periods[0].interest_expense, 1.);
    near(m.physical_tail.interest_expense, 1.);
    near(m.physical_tail.contractual_maturity, 1000.);
    near(m.physical_tail.bank_principal_cash, -1000.);
    near(m.physical_tail.bank_interest_cash, -2.);
    near(
        m.supplied_history_liability_dv01,
        1002. * ((0.0001f64 * 2. / 365.).exp() - (-0.0001f64 * 2. / 365.).exp()) / 2.,
    );
    assert_eq!(out.native_sources.len(), 4);
    for s in out.native_sources {
        near(s.projection.closing_balance, 0.);
        assert_eq!(
            s.projection.identity.measure,
            if s.role == Role::Physical {
                CashflowMeasure::Physical
            } else {
                CashflowMeasure::Pricing
            }
        );
    }
}
#[test]
fn explicit_pricing_helper_preserves_measure_and_original_physical_only_api() {
    let r = request(raw());
    let physical = &r.positions[0].histories[0].physical[0].request;
    let q = &r.positions[0].histories[0].pricing[0].request;
    let p = deposit_state::simulate(physical).unwrap();
    assert!(deposit_state::simulate(q).is_err());
    let explicit = deposit_state::simulate_declared_measure(q).unwrap();
    assert_eq!(explicit.identity.measure, CashflowMeasure::Pricing);
    assert_eq!(p.cashflows, explicit.cashflows);
    let mut invalid = q.clone();
    invalid.identity.measure = CashflowMeasure::Contractual;
    assert!(deposit_state::simulate_declared_measure(&invalid).is_err());
    assert_eq!(
        p,
        deposit_state::simulate_declared_measure(physical).unwrap()
    );
}
#[test]
fn independent_positive_liability_target_and_immutable_original_base_oas() {
    let mut v = raw();
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":1002.*(-0.07f64*2./365.).exp(),"source_id":"hand-target","source_revision":"1"});
    let graph = DepositGraph::new(&request(v.clone())).unwrap();
    near(graph.base_anchor().oas[0].base_oas, 0.02);
    let mut scenario = v["scenarios"][0].clone();
    scenario["scenario_id"] = json!("stress");
    scenario["run_id"] = json!("stress-run");
    scenario["collateral_curve"]["discount_factors"][1] = json!((-0.15f64).exp());
    v["scenarios"].as_array_mut().unwrap().push(scenario);
    let mut history = v["positions"][0]["histories"][0].clone();
    history["scenario_id"] = json!("stress");
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        history[role][0]["request"]["identity"]["run_id"] = json!("stress-run");
        history[role][0]["request"]["segments"][0]["repricing"]["target"]["intercept"] =
            json!(0.73);
    }
    v["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
        .push(history);
    v["identity"]["input_revision"] = json!("stress-v2");
    let out = graph.reprice(&request(v.clone())).unwrap();
    near(
        out.metrics[1].liability_pv,
        1004. * (-0.17f64 * 2. / 365.).exp(),
    );
    assert_eq!(
        serde_json::to_value(graph.base_anchor()).unwrap(),
        serde_json::to_value(&out.base_anchor).unwrap()
    );
    v["positions"][0]["histories"][0]["pricing"][0]["request"]["identity"]["assumption_id"] =
        json!("recalibrated");
    assert!(graph
        .reprice(&request(v))
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}
#[test]
fn physical_rate_expense_is_separate_from_pricing_policy() {
    let mut v = raw();
    for role in ["pricing", "pricing_down", "pricing_up"] {
        v["positions"][0]["histories"][0][role][0]["request"]["segments"][0]["repricing"]
            ["target"]["intercept"] = json!(0.1825);
    }
    let out = deposit_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].liability_pv, 1001.);
    near(
        out.metrics[0].physical_periods[0].interest_expense
            + out.metrics[0].physical_tail.interest_expense,
        2.,
    );
}

#[test]
fn separate_asymmetric_paid_rate_sources_match_independent_half_life_steps() {
    let mut v = raw();
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        let policy =
            &mut v["positions"][0]["histories"][0][role][0]["request"]["segments"][0]["repricing"];
        if role == "physical" {
            policy["target"]["intercept"] = json!(0.);
            policy["falling_half_life_days"] = json!(2.);
        } else {
            policy["target"]["intercept"] = json!(0.73);
            policy["rising_half_life_days"] = json!(1.);
        }
    }
    let out = deposit_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].liability_pv, 1003.25);
    near(
        out.metrics[0].physical_periods[0].interest_expense,
        2.0f64.powf(-0.5),
    );
    near(out.metrics[0].physical_tail.interest_expense, 0.5);
}
#[test]
fn native_competing_migration_interest_moves_once_and_is_not_cash() {
    let mut v = raw();
    paths(&mut v, |q| {
        let mut second = q["segments"][0].clone();
        second["segment_id"] = json!("other");
        second["opening_balance"] = json!(0.);
        second["opening_paid_rate"] = json!(0.);
        second["repricing"]["target"]["intercept"] = json!(0.);
        q["segments"].as_array_mut().unwrap().push(second);
        q["migrations"] = json!([{"from_segment":"retail","to_segment":"other","base_per_year":100.,"destination_rate_gap_sensitivity":0.}]);
        for d in q["drivers"].as_array_mut().unwrap() {
            for key in [
                "competitor_rates",
                "offered_spread_stress",
                "confidence_stress",
                "external_inflows",
            ] {
                d[key].as_array_mut().unwrap().push(json!(0.));
            }
        }
    });
    let out = deposit_graph::analyze(&request(v)).unwrap();
    let s = (-100.0f64 / 365.).exp();
    let m = &out.metrics[0];
    near(m.liability_pv, 1000. + s + s * s);
    near(m.opening_action_cash.migrated_principal, 1000. * (1. - s));
    near(
        m.physical_periods[0].migrated_principal,
        1000. * s * (1. - s),
    );
    near(
        m.physical_periods[0].migrated_accrued_interest,
        s * (1. - s),
    );
    near(m.physical_periods[0].bank_principal_cash, 0.);
    near(m.physical_tail.contractual_maturity, 1000.);
}
#[test]
fn residual_franchise_and_measure_ambiguities_reject_or_require_explicit_liquidation() {
    for bad in 0..3 {
        let mut v = raw();
        paths(&mut v, |q| {
            if bad == 0 {
                q["segments"][0]["maturity_ordinal"] = Value::Null;
            } else if bad == 1 {
                q["terminal"] = json!({"kind":"franchise_value_only","supplied_value":1000.,"rationale":"not liability cash"});
            } else {
                q["identity"]["measure"] = json!("physical");
            }
        });
        assert!(deposit_graph::analyze(&request(v)).is_err());
    }
    let mut v = raw();
    paths(&mut v, |q| {
        q["segments"][0]["maturity_ordinal"] = Value::Null;
        q["terminal"] = json!({"kind":"assumed_liquidation","rationale":"Explicit reviewed synthetic closing liability assumption"});
    });
    let out = deposit_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].physical_tail.assumed_terminal_payout, 1000.);
    near(out.metrics[0].physical_tail.contractual_maturity, 0.);
}
#[test]
fn signed_negative_coupon_requires_locked_oas_without_sign_clipping() {
    let mut v = raw();
    paths(&mut v, |q| {
        q["segments"][0]["repricing"]["target"]["intercept"] = json!(-0.1825);
        q["segments"][0]["repricing"]["floor"] = json!(-0.2);
    });
    let out = deposit_graph::analyze(&request(v.clone())).unwrap();
    near(out.metrics[0].liability_pv, 999.);
    near(out.metrics[0].physical_tail.bank_interest_cash, 1.);
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":999.,"source_id":"ambiguous-target","source_revision":"1"});
    assert!(deposit_graph::analyze(&request(v))
        .err()
        .unwrap()
        .contains("signed Q"));
}
#[test]
fn original_state_paired_weights_curve_scope_and_admission_are_strict() {
    for failure in 0..6 {
        let mut v = raw();
        match failure {
            0 => v["positions"][0]["histories"][0]["pricing_up"][0]["weight"] = json!(0.9),
            1 => v["positions"][0]["histories"][0]["pricing_up"][0]["path_id"] = json!("unpaired"),
            2 => {
                v["positions"][0]["histories"][0]["physical"][0]["request"]["segments"][0]
                    ["opening_balance"] = json!(1001.)
            }
            3 => v["scenarios"][0]["collateral_curve"]["currency"] = json!("EUR"),
            4 => v["reporting_ordinals"][1] = json!(738919),
            _ => {
                v["positions"][0]["histories"][0]["pricing"][0]["request"]["horizon_days"] =
                    json!(36600)
            }
        }
        assert!(deposit_graph::analyze(&request(v)).is_err());
    }
    let mut v = raw();
    v["infer_terminal"] = json!(true);
    assert!(serde_json::from_value::<Request>(v).is_err());
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    assert!(
        portfolio_compute_control::scope(Some(c), || deposit_graph::analyze(&request(raw())))
            .is_err()
    );
}
