use portfolio_risk_native::{
    advanced_graph::{OasTarget, Role},
    card_graph::{self, CardGraph, Request},
};
use serde_json::{json, Value};
const ASOF: i32 = 738916;
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn steps() -> Value {
    json!([
        {"ordinal":ASOF+1,"action":{"kind":"set_rates","rates":{"purchase":0.,"cash_advance":0.,"promotional":0.,"promotional_expiry_ordinal":ASOF+30,"post_promotion":0.}}},
        {"ordinal":ASOF+2,"action":{"kind":"payment","amount":1001.}},
        {"ordinal":ASOF+2,"action":{"kind":"set_limit","amount":0.}}
    ])
}
fn raw() -> Value {
    let mut contract: Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-card-1.json"
    ))
    .unwrap();
    contract["identity"]["measure"] = json!("contractual");
    let p =
        json!([{"path_id":"paired-1","weight":1.,"assumption_id":"observed-p-1","steps":steps()}]);
    let q = json!([{"path_id":"paired-1","weight":1.,"assumption_id":"conditional-q-1","steps":steps()}]);
    json!({"schema":"card-product-graph-1","identity":{"model_revision":"card-graph-1","input_revision":"input-1","paired_history_source":"supplied-panel","paired_history_revision":"1","randomness_domain":"supplied_paired_histories-1"},
        "as_of_ordinal":ASOF,"currency":"USD","reporting_ordinals":[ASOF,ASOF+1,ASOF+2],"base_scenario_id":"base","bump_bp":1.,"terminal_policy":"zero_rights_zero_limit_at_tail","cash_timing":"after_opening_actions",
        "scenarios":[{"scenario_id":"base","run_id":"base","severity":0.,"collateral_curve":{"curve_id":"usd-collateral","source_id":"synthetic-hand","source_revision":"1","currency":"USD","role":{"kind":"collateral","collateral_index":"SOFR"},"times_years":[0.,1.],"discount_factors":[1.,(-0.05f64).exp()],"right_tail":"reject"}}],
        "positions":[{"position_id":"card-1","oas_target":{"kind":"locked","base_oas":-0.05,"source_id":"locked-test","source_revision":"1"},"contract":contract,"histories":[{"scenario_id":"base","physical":p,"pricing":q,"pricing_down":q,"pricing_up":q}]}]})
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn all_paths(v: &mut Value, action: impl Fn(&mut Value)) {
    for set in v["positions"][0]["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in set[role].as_array_mut().unwrap() {
                action(h);
            }
        }
    }
}
#[test]
fn hand_cash_interest_and_native_bump() {
    let result = card_graph::analyze(&request(raw())).unwrap();
    let m = &result.metrics[0];
    near(m.pv, 1001.);
    near(m.physical_periods[0].gross_contract_interest, 1.);
    near(m.physical_periods[1].gross_contract_interest, 0.);
    near(m.physical_periods[1].principal_payment_cash, 1000.);
    near(m.physical_periods[1].interest_cash, 1.);
    let t = 2. / 365.;
    near(
        m.supplied_history_dv01,
        1001. * ((0.0001f64 * t).exp() - (-0.0001f64 * t).exp()) / 2.,
    );
    assert_eq!(result.native_sources.len(), 4);
    for source in &result.native_sources {
        assert_eq!(source.projection.points.len(), 3);
        near(source.projection.closing_principal, 0.);
        near(source.projection.points.last().unwrap().limit, 0.);
        assert_eq!(
            source.assumption_id,
            source.projection.identity.assumption_id
        );
    }
}
#[test]
fn independent_positive_target_and_fixed_anchor_reprice() {
    let mut v = raw();
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":1001.*(-(0.05+0.02)*2./365f64).exp(),"source_id":"cash-target","source_revision":"1"});
    let graph = CardGraph::new(&request(v.clone())).unwrap();
    near(graph.base_anchor().oas[0].base_oas, 0.02);
    let mut stress = v["scenarios"][0].clone();
    stress["scenario_id"] = json!("stress");
    stress["run_id"] = json!("stress-run");
    stress["collateral_curve"]["discount_factors"][1] = json!((-0.15f64).exp());
    v["scenarios"].as_array_mut().unwrap().push(stress);
    let mut history = v["positions"][0]["histories"][0].clone();
    history["scenario_id"] = json!("stress");
    v["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
        .push(history);
    v["identity"]["input_revision"] = json!("stress-input-2");
    let out = graph.reprice(&request(v.clone())).unwrap();
    near(out.base_anchor.oas[0].base_oas, 0.02);
    near(out.metrics[1].pv, 1001. * (-0.17f64 * 2. / 365.).exp());
    assert_eq!(
        serde_json::to_value(graph.base_anchor()).unwrap(),
        serde_json::to_value(&out.base_anchor).unwrap()
    );
    v["positions"][0]["histories"][0]["pricing"][0]["assumption_id"] = json!("new-base-policy");
    assert!(graph
        .reprice(&request(v))
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}
#[test]
fn independent_physical_domain_and_weighted_q_paths() {
    let mut v = raw();
    let set = &mut v["positions"][0]["histories"][0];
    set["physical"][0]["path_id"] = json!("physical-unpaired");
    for role in ["pricing", "pricing_down", "pricing_up"] {
        let mut extra = set[role][0].clone();
        set[role][0]["weight"] = json!(0.25);
        extra["weight"] = json!(0.75);
        extra["path_id"] = json!("paired-2");
        set[role].as_array_mut().unwrap().push(extra);
    }
    let out = card_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].pv, 1001.);
    assert_eq!(out.native_sources.len(), 7);
    assert!(out.native_sources[0].role == Role::Physical);
}
#[test]
fn signed_future_draw_locked_oas_and_asof_cash() {
    let mut v = raw();
    v["positions"][0]["contract"]["opening_principal"] = json!([0., 0., 0.]);
    v["positions"][0]["contract"]["cycles"] = json!([]);
    all_paths(&mut v, |p| {
        p["steps"] = json!([{ "ordinal":ASOF+1,"action":{"kind":"draw","balance_class":"purchase","amount":100.}},{"ordinal":ASOF+2,"action":{"kind":"payment","amount":100.}},{"ordinal":ASOF+2,"action":{"kind":"set_limit","amount":0.}}]);
    });
    let out = card_graph::analyze(&request(v.clone())).unwrap();
    near(out.metrics[0].pv, 0.);
    near(out.metrics[0].physical_periods[0].draw_cash, -100.);
    near(out.metrics[0].physical_periods[1].principal_cash, 100.);
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":100.,"source_id":"target","source_revision":"1"});
    assert!(card_graph::analyze(&request(v.clone()))
        .err()
        .unwrap()
        .contains("signed Q"));
    v["positions"][0]["oas_target"] = serde_json::to_value(OasTarget::Locked {
        base_oas: -0.05,
        source_id: "locked".into(),
        source_revision: "1".into(),
    })
    .unwrap();
    all_paths(&mut v, |p| p["steps"][0]["ordinal"] = json!(ASOF));
    let out = card_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].pv, 100.);
    near(out.metrics[0].opening_action_cash.draw_cash, -100.);
    near(out.metrics[0].physical_periods[0].draw_cash, 0.);
}
fn default_raw(cost: f64) -> Value {
    let mut v = raw();
    v["positions"][0]["contract"]["horizon_days"] = json!(5);
    v["positions"][0]["contract"]["recovery_delay_days"] = json!(3);
    v["positions"][0]["contract"]["collection_cost"] = json!(cost);
    v["reporting_ordinals"] = json!([ASOF, ASOF + 1]);
    all_paths(&mut v, |p| {
        p["steps"] = json!([{ "ordinal":ASOF+1,"action":{"kind":"default"}}])
    });
    v
}
#[test]
fn default_and_recovery_tail_have_no_double_count() {
    let out = card_graph::analyze(&request(default_raw(10.))).unwrap();
    let m = &out.metrics[0];
    let p = &m.physical_periods[0];
    near(p.gross_contract_interest, 1.);
    near(p.default_principal, 1000.);
    near(p.default_accrued_interest, 1.);
    near(p.default_recognized_accrued_interest, 1.);
    near(p.principal_loss, 610.);
    near(m.pv, 390.);
    near(m.physical_tail.principal_cash, 390.);
    near(m.physical_tail.recovery_gross, 400.);
    near(m.physical_tail.recovery_cost, 10.);
    near(m.physical_tail.recovery_net, 390.);
    near(m.physical_tail.fee_cash, 0.);
    assert_eq!(
        out.native_sources[0]
            .projection
            .points
            .last()
            .unwrap()
            .ordinal,
        ASOF + 5
    );
    let out = card_graph::analyze(&request(default_raw(500.))).unwrap();
    let m = &out.metrics[0];
    near(m.pv, -100.);
    near(m.physical_tail.recovery_net, 0.);
    near(m.physical_tail.uncovered_recovery_cost_cash, -100.);
    near(m.physical_periods[0].principal_loss, 1000.);
}
#[test]
fn final_date_recovery_and_default_are_retained() {
    let mut v = default_raw(10.);
    v["positions"][0]["contract"]["horizon_days"] = json!(4);
    v["reporting_ordinals"] = json!([ASOF, ASOF + 4]);
    let out = card_graph::analyze(&request(v.clone())).unwrap();
    near(
        out.metrics[0].physical_periods[0].recovery_principal_cash,
        390.,
    );
    v["positions"][0]["contract"]["horizon_days"] = json!(3);
    v["reporting_ordinals"] = json!([ASOF, ASOF + 3]);
    assert!(card_graph::analyze(&request(v)).is_err());
}
#[test]
fn residual_and_unfunded_commitment_are_rejected_even_zero_weight() {
    let mut v = raw();
    all_paths(&mut v, |p| {
        p["steps"].as_array_mut().unwrap().pop();
    });
    assert!(card_graph::analyze(&request(v))
        .err()
        .unwrap()
        .contains("zero rights/limit"));
    let mut v = raw();
    v["positions"][0]["histories"][0]["physical"][0]["weight"] = json!(0.);
    let mut h = v["positions"][0]["histories"][0]["physical"][0].clone();
    h["path_id"] = json!("p-2");
    h["weight"] = json!(1.);
    v["positions"][0]["histories"][0]["physical"]
        .as_array_mut()
        .unwrap()
        .push(h);
    v["positions"][0]["histories"][0]["physical"][0]["steps"] = json!([]);
    assert!(card_graph::analyze(&request(v)).is_err());
}
#[test]
fn exact_pairing_measure_and_source_admission() {
    for (key, value) in [("path_id", json!("mismatch")), ("weight", json!(0.9))] {
        let mut v = raw();
        v["positions"][0]["histories"][0]["pricing_up"][0][key] = value;
        assert!(card_graph::analyze(&request(v)).is_err());
    }
    let mut v = raw();
    v["positions"][0]["contract"]["identity"]["measure"] = json!("physical");
    assert!(card_graph::analyze(&request(v)).is_err());
    let graph = CardGraph::new(&request(raw())).unwrap();
    let mut v = raw();
    v["identity"]["paired_history_revision"] = json!("changed");
    assert!(graph.reprice(&request(v)).is_err());
}
#[test]
fn bounded_work_before_projection_and_curves_fail_closed() {
    let mut v = raw();
    v["positions"][0]["contract"]["horizon_days"] = json!(36600);
    let e = json!({"ordinal":ASOF,"action":{"kind":"fee","balance_class":"purchase","amount":0.}});
    all_paths(&mut v, |p| p["steps"] = Value::Array(vec![e.clone(); 4095]));
    assert!(card_graph::analyze(&request(v))
        .err()
        .unwrap()
        .contains("work admission"));
    let mut v = raw();
    v["scenarios"][0]["collateral_curve"]["role"] = json!({"kind":"index","index_id":"SOFR"});
    assert!(card_graph::analyze(&request(v)).is_err());
    let mut v = raw();
    v["scenarios"][0]["collateral_curve"]["times_years"][1] = json!(1. / 365.);
    assert!(card_graph::analyze(&request(v)).is_err());
}

#[test]
fn physical_default_history_is_independent_of_q_prices() {
    let mut v = default_raw(10.);
    for role in ["pricing", "pricing_down", "pricing_up"] {
        v["positions"][0]["histories"][0][role][0]["steps"] = steps();
    }
    let out = card_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].pv, 1001.);
    near(out.metrics[0].physical_periods[0].default_principal, 1000.);
    near(out.metrics[0].physical_tail.recovery_principal_cash, 390.);
    for p in out
        .native_sources
        .iter()
        .filter(|p| p.role != Role::Physical)
    {
        assert!(p.projection.default.is_none());
    }
}

#[test]
fn nonaccrual_collection_recognition_is_not_double_income() {
    let mut v = raw();
    let c = &mut v["positions"][0]["contract"];
    c["initial_state"] = json!("nonaccrual");
    c["opening_billed_interest"] = json!([0., 20., 0.]);
    c["opening_unrecognized_billed_interest"] = json!([0., 20., 0.]);
    c["cycles"] = json!([]);
    all_paths(
        &mut v,
        |p| p["steps"] = json!([{ "ordinal":ASOF+1,"action":{"kind":"payment","amount":1020.}},{"ordinal":ASOF+1,"action":{"kind":"set_limit","amount":0.}}]),
    );
    let out = card_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].pv, 1020.);
    near(
        out.metrics[0].physical_periods[0].gross_contract_interest,
        20.,
    );
    near(out.metrics[0].physical_periods[0].interest_cash, 20.);
    near(
        out.metrics[0].physical_periods[1].gross_contract_interest,
        0.,
    );
    let r = &out.native_sources[0].projection.recognition;
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].start_ordinal, ASOF + 1);
    assert_eq!(r[0].end_ordinal, ASOF + 1);
}

#[test]
fn same_asof_default_and_recovery_are_after_actions_opening_cash() {
    let mut v = default_raw(10.);
    v["positions"][0]["contract"]["recovery_delay_days"] = json!(0);
    all_paths(&mut v, |p| p["steps"][0]["ordinal"] = json!(ASOF));
    let out = card_graph::analyze(&request(v)).unwrap();
    near(out.metrics[0].pv, 0.);
    near(out.metrics[0].opening_action_cash.default_principal, 1000.);
    near(
        out.metrics[0].opening_action_cash.recovery_principal_cash,
        390.,
    );
    near(out.metrics[0].physical_periods[0].default_principal, 0.);
}

#[test]
fn published_synthetic_example_and_zero_rights_domain() {
    let r: Request = serde_json::from_str(include_str!(
        "../../../examples/models/card-product-graph-1.json"
    ))
    .unwrap();
    let out = card_graph::analyze(&r).unwrap();
    near(out.metrics[0].pv, 1001.);
    near(
        out.metrics[0].physical_periods[0].gross_contract_interest,
        1.,
    );
    let mut v = raw();
    v["positions"][0]["contract"]["opening_principal"] = json!([0., 0., 0.]);
    v["positions"][0]["contract"]["initial_limit"] = json!(0.);
    all_paths(&mut v, |p| p["steps"] = json!([]));
    near(
        card_graph::analyze(&request(v.clone())).unwrap().metrics[0].pv,
        0.,
    );
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":1.,"source_id":"target","source_revision":"1"});
    assert!(card_graph::analyze(&request(v))
        .err()
        .unwrap()
        .contains("positive future"));
}
#[test]
fn aggregate_work_helper_matches_admission_before_projection() {
    let r = request(raw());
    let work = card_graph::audit_admitted_work(&r).unwrap();
    assert!(work > 0 && work <= card_graph::MAX_WORK);
    let mut v = raw();
    v["schema"] = json!("old-version");
    assert!(card_graph::audit_admitted_work(&request(v)).is_err());
    let mut v = raw();
    let extra = v["positions"][0]["histories"][0]["physical"][0].clone();
    v["positions"][0]["histories"][0]["physical"][0]["weight"] = json!(0.5);
    let mut second = extra;
    second["path_id"] = json!("second-p");
    second["weight"] = json!(0.5);
    v["positions"][0]["histories"][0]["physical"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert!(card_graph::audit_admitted_work(&request(v)).unwrap() > work);
    let mut v = raw();
    v["positions"][0]["contract"]["rates"]["cash_advance"] = json!(-0.1);
    assert!(card_graph::audit_admitted_work(&request(v)).is_err());
}
