use portfolio_risk_native::{
    term_enhancements::cd::{self, CdElectionRequest},
    term_product_graph::{self, Request, TermGraph},
};
use serde_json::{json, Value};
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn common(known: Value, history: Value, position: &str, asof: i32) -> Value {
    let mut p = history.clone();
    p["request"]["measure"] = json!("physical");
    p["request"]["scenario_id"] = json!("base-run");
    let mut q = p.clone();
    q["request"]["measure"] = json!("pricing");
    json!({"schema":"term-product-graph-1","identity":{"model_revision":"term-graph-1","input_revision":"input-1","paired_history_source":"supplied-synthetic","paired_history_revision":"r1","randomness_domain":"supplied_paired_histories-1"},"as_of_ordinal":asof,"currency":"USD","reporting_ordinals":[asof,asof+1],"base_scenario_id":"base","bump_bp":1.,"cash_timing":"after_opening_actions","scenarios":[{"scenario_id":"base","run_id":"base-run","severity":0.,"collateral_curve":{"curve_id":"collateral","source_id":"synthetic-hand","source_revision":"r1","currency":"USD","role":{"kind":"collateral","collateral_index":"SOFR"},"times_years":[0.,1.],"discount_factors":[1.,(-0.05f64).exp()],"right_tail":"reject"}}],"positions":[{"position_id":position,"known":known,"oas_target":{"kind":"locked","base_oas":-0.05,"source_id":"reviewed-lock","source_revision":"r1"},"histories":[{"scenario_id":"base","physical":[{"path_id":"physical-independent","weight":1.,"product":p}],"pricing":[{"path_id":"paired-1","weight":1.,"product":q}],"pricing_down":[{"path_id":"paired-1","weight":1.,"product":q}],"pricing_up":[{"path_id":"paired-1","weight":1.,"product":q}]}]}]})
}
fn indexed_raw() -> Value {
    let example: Value =
        serde_json::from_str(include_str!("../../../examples/models/term-indexed.json")).unwrap();
    let mut contract = example["request"].clone();
    contract["measure"] = json!("contractual");
    contract["reporting_end_ordinal"] = json!(738918);
    contract["periods"][0]["coupon"] = json!({"kind":"fixed","annual_rate":0.36});
    common(
        json!({"kind":"indexed_term","contract":contract}),
        json!({"kind":"indexed_term","request":contract}),
        "loan:example",
        738917,
    )
}
fn cd_raw() -> Value {
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/term-cd-election.json"
    ))
    .unwrap();
    let mut request = example["request"].clone();
    request["direction"] = json!("payment");
    request["reporting_end_ordinal"] = request["election_ordinal"].clone();
    let known = json!({"kind":"cd_election","contract":{"model_version":request["model_version"],"currency":request["currency"],"direction":request["direction"],"policy":request["policy"],"opening":request["opening"],"tail_end_ordinal":request["tail_end_ordinal"]}});
    common(
        known,
        json!({"kind":"cd_election","request":request}),
        "cd:example",
        739617,
    )
}
fn req(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn paths(v: &mut Value, f: impl Fn(&mut Value)) {
    for position in v["positions"].as_array_mut().unwrap() {
        for set in position["histories"].as_array_mut().unwrap() {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for p in set[role].as_array_mut().unwrap() {
                    f(&mut p["product"]["request"]);
                }
            }
        }
    }
}
#[test]
fn fixed_term_full_actual_tail_and_signed_liability_cash() {
    let out = term_product_graph::analyze(&req(indexed_raw())).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 102.9);
    close(m.physical_periods[0].interest_cash, 0.);
    close(m.physical_tail.principal_cash, 100.);
    close(m.physical_tail.interest_cash, 2.9);
    let t = 29. / 365.;
    close(
        m.supplied_history_dv01,
        102.9 * ((0.0001f64 * t).exp() - (-0.0001f64 * t).exp()) / 2.,
    );
    let mut v = indexed_raw();
    v["positions"][0]["known"]["contract"]["direction"] = json!("payment");
    paths(&mut v, |r| r["direction"] = json!("payment"));
    let x = term_product_graph::analyze(&req(v)).unwrap();
    close(x.metrics[0].pv, -102.9);
    close(x.metrics[0].physical_tail.interest_cash, -2.9);
}
#[test]
fn original_retained_payment_is_not_rebased_and_principal_conserves() {
    let mut v = indexed_raw();
    let mut period = v["positions"][0]["known"]["contract"]["periods"][0].clone();
    period["period_id"] = json!("first");
    period["end_ordinal"] = json!(738927);
    period["payment_ordinal"] = json!(738927);
    let mut second = period.clone();
    second["period_id"] = json!("last");
    second["start_ordinal"] = json!(738927);
    second["end_ordinal"] = json!(738937);
    second["payment_ordinal"] = json!(738937);
    let apply = |r: &mut Value| {
        r["periods"] = json!([period, second]);
        r["principal_plan"] = json!({"kind":"retained_payment","original_payment":51.,"terminal":{"kind":"residual_payoff"}});
    };
    apply(&mut v["positions"][0]["known"]["contract"]);
    paths(&mut v, apply);
    let out = term_product_graph::analyze(&req(v.clone())).unwrap();
    close(out.metrics[0].pv, 101.5);
    close(out.metrics[0].physical_tail.principal_cash, 100.);
    close(out.metrics[0].physical_tail.interest_cash, 1.5);
    let graph = TermGraph::new(&req(v.clone())).unwrap();
    v["positions"][0]["known"]["contract"]["principal_plan"]["original_payment"] = json!(52.);
    paths(&mut v, |r| {
        r["principal_plan"]["original_payment"] = json!(52.)
    });
    assert!(graph
        .reprice(&req(v))
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}
#[test]
fn declared_cd_pricing_helper_keeps_physical_api_and_exact_arithmetic() {
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/term-cd-election.json"
    ))
    .unwrap();
    let p: CdElectionRequest = serde_json::from_value(example["request"].clone()).unwrap();
    let mut q = p.clone();
    q.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
    assert!(cd::step_cd(&q).is_err());
    assert!(cd::step_cd_pricing(&p).is_err());
    let p = cd::step_cd(&p).unwrap();
    let q = cd::step_cd_pricing(&q).unwrap();
    close(p.penalty, q.penalty);
    close(p.net_external_cash, q.net_external_cash);
    assert_eq!(p.closing.principal, q.closing.principal);
    let mut pv = serde_json::to_value(&p).unwrap();
    let mut qv = serde_json::to_value(&q).unwrap();
    pv.as_object_mut().unwrap().remove("disclosure");
    qv.as_object_mut().unwrap().remove("disclosure");
    for name in ["settlement_schedule", "remaining_schedule"] {
        for e in pv[name]["events"].as_array_mut().unwrap() {
            e["measure"] = json!("pricing");
        }
    }
    assert_eq!(pv, qv);
}
#[test]
fn cd_partial_withdrawal_penalty_and_fixed_tail_are_not_double_cash() {
    let out = term_product_graph::analyze(&req(cd_raw())).unwrap();
    let m = &out.metrics[0];
    let penalty = 40. * 0.05 * 90. / 365.;
    let principal_penalty = penalty - 0.2;
    close(m.pv, -(100.6 - principal_penalty));
    close(m.physical_tail.principal_cash, -100.);
    close(m.physical_tail.interest_cash, -0.8);
    close(m.physical_tail.fee_cash, penalty);
    close(m.physical_tail.cd_interest_cash_paid, 0.);
    close(
        m.physical_tail.cd_principal_cash_returned,
        -(40. - principal_penalty),
    );
    close(
        m.physical_tail.cd_net_external_cash,
        -(40. - principal_penalty),
    );
    close(m.physical_tail.cd_penalty, penalty);
    close(m.physical_tail.cd_principal_extinguished, 40.);
    close(m.physical_tail.cd_interest_accrued_through_election, 0.5);
    close(m.physical_periods[0].principal_cash, 0.);
}
#[test]
fn cd_explicit_renewal_has_one_constructive_settlement_and_complete_new_tail() {
    let mut v = cd_raw();
    paths(&mut v, |r| {
        r["election_ordinal"] = json!(739627);
        r["reporting_end_ordinal"] = json!(739627);
        r["election"] = json!({"kind":"maturity","cash_out_principal":40.,"renew_principal":60.,"switch_principal":0.,"cash_out_interest":1.,"renew_interest":0.,"switch_interest":0.,"renewal":{"contract":{"id":"cd-renewal","revision":"r1"},"position_id":"cd:new","maturity_ordinal":739637,"annual_rate":0.72,"day_count":"act360"},"switching":null});
    });
    let out = term_product_graph::analyze(&req(v.clone())).unwrap();
    let m = &out.metrics[0];
    close(m.pv, -102.2);
    close(m.physical_tail.principal_cash, -100.);
    close(m.physical_tail.interest_cash, -2.2);
    close(m.physical_tail.cd_new_contract_principal, 60.);
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":102.2,"source_id":"target","source_revision":"r1"});
    assert!(term_product_graph::analyze(&req(v.clone()))
        .err()
        .unwrap()
        .contains("signed directional"));
    paths(&mut v, |r| {
        r["election"]["renewal"]["position_id"] = json!("cd:example")
    });
    assert!(term_product_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("collides"));
}
#[test]
fn positive_target_and_scenario_reprice_hold_original_full_base_dependency() {
    let mut v = indexed_raw();
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":102.9*(-0.07f64*29./365.).exp(),"source_id":"cash-target","source_revision":"r1"});
    let graph = TermGraph::new(&req(v.clone())).unwrap();
    close(graph.base_anchor().oas[0].base_oas, 0.02);
    let mut s = v["scenarios"][0].clone();
    s["scenario_id"] = json!("stress");
    s["run_id"] = json!("stress-run");
    s["collateral_curve"]["discount_factors"][1] = json!((-0.15f64).exp());
    v["scenarios"].as_array_mut().unwrap().push(s);
    let mut history = v["positions"][0]["histories"][0].clone();
    history["scenario_id"] = json!("stress");
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        history[role][0]["product"]["request"]["scenario_id"] = json!("stress-run");
    }
    v["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
        .push(history);
    let out = graph.reprice(&req(v.clone())).unwrap();
    close(out.metrics[1].pv, 102.9 * (-0.17f64 * 29. / 365.).exp());
    assert_eq!(
        serde_json::to_value(graph.base_anchor()).unwrap(),
        serde_json::to_value(&out.base_anchor).unwrap()
    );
    v["positions"][0]["histories"][0]["pricing"][0]["product"]["request"]["assumption"]
        ["revision"] = json!("r2");
    assert!(graph
        .reprice(&req(v))
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}
#[test]
fn supplied_index_measures_no_future_publication_or_hidden_index_proxy() {
    let mut v = indexed_raw();
    let example: Value =
        serde_json::from_str(include_str!("../../../examples/models/term-indexed.json")).unwrap();
    v["positions"][0]["known"]["contract"]["periods"] = example["request"]["periods"].clone();
    paths(&mut v, |r| {
        r["periods"] = example["request"]["periods"].clone()
    });
    for (role, rate) in [
        ("physical", 0.04),
        ("pricing", 0.02),
        ("pricing_down", 0.01),
        ("pricing_up", 0.03),
    ] {
        let obs = &mut v["positions"][0]["histories"][0][role][0]["product"]["request"]["periods"]
            [0]["coupon"]["observations"][0];
        obs["rate"] = json!(rate);
        obs["source"]["measure"] = json!(if role == "physical" {
            "physical"
        } else {
            "pricing"
        });
    }
    let out = term_product_graph::analyze(&req(v.clone())).unwrap();
    close(out.metrics[0].pv, 100. + 100. * 0.03 * 29. / 360.);
    close(
        out.metrics[0].physical_tail.interest_cash,
        100. * 0.05 * 29. / 360.,
    );
    v["positions"][0]["histories"][0]["physical"][0]["product"]["request"]["periods"][0]
        ["coupon"]["observations"][0]["source"]["measure"] = json!("pricing");
    assert!(term_product_graph::analyze(&req(v)).is_err());
}
#[test]
fn paired_ids_known_terms_negative_cd_rights_and_large_work_fail_closed() {
    let mut v = indexed_raw();
    v["positions"][0]["histories"][0]["pricing_up"][0]["path_id"] = json!("unpaired");
    assert!(term_product_graph::analyze(&req(v)).is_err());
    let mut v = cd_raw();
    v["positions"][0]["known"]["contract"]["opening"]["accrued_interest"] = json!(-1.);
    paths(&mut v, |r| r["opening"]["accrued_interest"] = json!(-1.));
    assert!(term_product_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("negative accrued"));
    let mut v = indexed_raw();
    let period = v["positions"][0]["known"]["contract"]["periods"][0].clone();
    v["positions"][0]["known"]["contract"]["periods"] = Value::Array(vec![period.clone(); 4096]);
    paths(&mut v, |r| {
        r["periods"] = Value::Array(vec![period.clone(); 4096])
    });
    let paths0 = &mut v["positions"][0]["histories"][0]["pricing"];
    let base = paths0[0].clone();
    paths0.as_array_mut().unwrap().clear();
    for n in 0..10 {
        let mut p = base.clone();
        p["path_id"] = json!(format!("q-{n}"));
        p["weight"] = json!(0.1);
        paths0.as_array_mut().unwrap().push(p);
    }
    assert!(term_product_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("before projection"));
}
#[test]
fn same_asof_paid_rights_are_retained_outside_future_pv() {
    let mut v = cd_raw();
    paths(&mut v, |r| {
        r["election_ordinal"] = json!(739617);
        r["reporting_end_ordinal"] = json!(739618);
    });
    let out = term_product_graph::analyze(&req(v)).unwrap();
    let m = &out.metrics[0];
    let penalty = 40. * 0.05 * 90. / 365.;
    close(m.pv, -60.6);
    close(m.opening_actions.principal_cash, -40.);
    close(m.opening_actions.fee_cash, penalty);
    close(m.opening_actions.cd_net_external_cash, -40. + penalty);
    close(m.physical_tail.principal_cash, -60.);
    let mut v = indexed_raw();
    v["positions"][0]["known"]["contract"]["periods"][0]["payment_ordinal"] = json!(738917);
    paths(&mut v, |r| {
        r["periods"][0]["payment_ordinal"] = json!(738917)
    });
    let out = term_product_graph::analyze(&req(v)).unwrap();
    close(out.metrics[0].pv, 0.);
    close(out.metrics[0].opening_actions.principal_cash, 100.);
    close(out.metrics[0].opening_actions.interest_cash, 2.9);
}
#[test]
fn graph_cd_negative_right_tier_rejects_without_changing_standalone_kernel() {
    let mut v = cd_raw();
    v["positions"][0]["known"]["contract"]["opening"]["annual_rate"] = json!(-0.365);
    paths(&mut v, |r| r["opening"]["annual_rate"] = json!(-0.365));
    assert!(term_product_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("negative/prepaid"));
}
#[test]
fn explicit_maturity_grace_election_keeps_complete_cash_and_no_automatic_renewal() {
    let mut v = cd_raw();
    paths(&mut v, |r| {
        r["election_ordinal"] = json!(739628);
        r["reporting_end_ordinal"] = json!(739628);
        r["election"] = json!({"kind":"maturity","cash_out_principal":100.,"renew_principal":0.,"switch_principal":0.,"cash_out_interest":1.05,"renew_interest":0.,"switch_interest":0.,"renewal":null,"switching":null});
    });
    let out = term_product_graph::analyze(&req(v)).unwrap();
    close(out.metrics[0].pv, -101.05);
    close(out.metrics[0].physical_tail.interest_cash, -1.05);
    close(out.metrics[0].physical_tail.cd_new_contract_principal, 0.);
}
#[test]
fn checked_in_graph_example_has_two_independent_cash_references() {
    let r: Request = serde_json::from_str(include_str!(
        "../../../examples/models/term-product-graph-1.json"
    ))
    .unwrap();
    let out = term_product_graph::analyze(&r).unwrap();
    close(out.metrics[0].pv, 102.9);
    close(out.metrics[1].pv, -100.30684931506849);
    assert_eq!(out.native_sources.len(), 8);
}
