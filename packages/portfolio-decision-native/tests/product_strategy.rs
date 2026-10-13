use portfolio_decision_native::product_library::{self, PreparedLibrary, Session};
use portfolio_risk_native::product_strategy::Allocation;
use serde_json::{json, Value};
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn product() -> Value {
    let mut p: Value = serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap();
    let mut g: Value = serde_json::from_str(include_str!(
        "../../../examples/models/advanced-product-graph-1.json"
    ))
    .unwrap();
    let y = 0.02f64;
    let quote = 529. * (-y.ln_1p() * 29. / 365.).exp() + 515.5 * (-y.ln_1p() * 60. / 365.).exp();
    g["positions"][0]["position_id"] = json!("loan-unit");
    g["positions"][0]["product"]["contract"]["identity"]["position_id"] = json!("loan-unit");
    g["positions"][0]["oas_target"]["base_oas"] = json!(y.ln_1p());
    let reports = g["reporting_ordinals"].as_array().unwrap().clone();
    let id = g["base_scenario_id"].clone();
    let path = g["positions"][0]["product"]["histories"][0]["physical"][0]["path_id"].clone();
    p["reporting_ordinals"] = json!(reports);
    p["base_scenario_id"] = id.clone();
    let u = &mut p["units"][0];
    u["unit_id"] = json!("loan-unit");
    u["template_id"] = json!("loan-template");
    u["purchase_ordinal"] = g["as_of_ordinal"].clone();
    u["quote_price_per_face_unit"] = json!(quote / 1000.);
    u["purchase_graph"] = json!({"kind":"consumer_installment","request":g});
    u["age_branches"]=json!(reports[..reports.len()-1].iter().map(|day| json!({"ordinal":day,"scenario_id":id,"physical_path_id":path,"graph":u["purchase_graph"]})).collect::<Vec<_>>());
    p
}
fn raw() -> Value {
    json!({"schema":"strategy-product-1","op":"optimize_product","product":product(),
        "bases":[{"scenario_id":"base","base":{"nii":20.,"dv01":0.,"eve":1e6,"l1":10000.,"l2":0.,"nco":1.,"asf":10000.,"rsf":1.,"cet1":10000.,"rwa":1.,"ni":0.7,"mv_assets":1e6}}],
        "constraints":{"lcr_min":0.,"nsfr_min":0.,"cet1_min":0.,"eve_limit":1.,"cash_budget":10000.,"max_total_assets":100.,"commercial":[],"capital_limits":[]},
        "max_purchase_cost_assets":null,"face_limits":[],"capital_dependency":null})
}
fn normalize(mut v: Value) -> Value {
    v["bases"][0]["scenario_id"] = v["product"]["base_scenario_id"].clone();
    v
}
fn selected(x: f64) -> Vec<Allocation> {
    vec![Allocation {
        unit_id: "loan-unit".into(),
        face: x,
    }]
}
#[test]
fn actual_purchase_positive_nii_is_maximized_and_selected_replayed_independently() {
    let v = normalize(raw());
    let lib = PreparedLibrary::new(&v).unwrap();
    let r = lib.evaluate(&selected(50.)).unwrap();
    let y = 0.02f64;
    let quote = 529. * (-y.ln_1p() * 29. / 365.).exp() + 515.5 * (-y.ln_1p() * 60. / 365.).exp();
    let i1 = quote * (y.ln_1p() * 29. / 365.).exp_m1();
    let i2 = (quote + i1 - 529.) * (y.ln_1p() * 31. / 365.).exp_m1();
    near(
        r["source_replay"]["scenarios"][0]["incremental_external_nii"]
            .as_f64()
            .unwrap(),
        (i1 + i2) * 0.05,
    );
    near(
        r["source_replay"]["scenarios"][0]["cash"][0]["funding_demand"]
            .as_f64()
            .unwrap(),
        quote * 0.05,
    );
    let solved = product_library::optimize(&v).unwrap();
    assert_eq!(solved["feasible"], true);
    near(solved["allocation"][0]["face"].as_f64().unwrap(), 100.);
    near(
        solved["worst_case_nii_$"].as_f64().unwrap(),
        20. + (i1 + i2) * 0.1,
    );
}
#[test]
fn category_face_and_purchase_cost_caps_are_separate() {
    let mut v = normalize(raw());
    v["face_limits"] = json!([{"label":"category","policy_id":"review-v1","selector":{"kind":"accounting_category","accounting_category":"supplied-amortized-cost"},"sense":"<=","amount":80.}]);
    let r = product_library::optimize(&v).unwrap();
    near(r["allocation"][0]["face"].as_f64().unwrap(), 80.);
    v["max_purchase_cost_assets"] = json!(40.);
    let q = v["product"]["units"][0]["quote_price_per_face_unit"]
        .as_f64()
        .unwrap();
    let r = product_library::optimize(&v).unwrap();
    near(r["allocation"][0]["face"].as_f64().unwrap(), 40. / q);
    near(r["total_purchase_cost_assets_$"].as_f64().unwrap(), 40.);
    assert!(PreparedLibrary::new(&v)
        .unwrap()
        .evaluate(&selected(80.))
        .is_err());
}
#[test]
fn infeasible_reviewed_minimum_and_unsupported_selector_fail_closed() {
    let mut v = normalize(raw());
    v["face_limits"] = json!([{"label":"minimum","policy_id":"review-v1","selector":{"kind":"template","template_id":"loan-template"},"sense":">=","amount":101.}]);
    let r = product_library::optimize(&v).unwrap();
    assert_eq!(r["feasible"], false);
    v["face_limits"][0]["selector"]["template_id"] = json!("foreign");
    assert!(product_library::optimize(&v).is_err());
    let mut v = normalize(raw());
    v["bases"][0]["scenario_id"] = json!("foreign");
    assert!(PreparedLibrary::new(&v).is_err());
    let lib = PreparedLibrary::new(&normalize(raw())).unwrap();
    assert!(lib
        .evaluate(&[Allocation {
            unit_id: "foreign".into(),
            face: 0.
        }])
        .is_err());
}
#[test]
fn reviewed_capital_rows_and_exact_raw_dependency_survive_allocation_replay() {
    let mut v = normalize(raw());
    v["capital_dependency"] = v["product"].clone();
    v["constraints"]["capital_limits"] = json!([{"label":"eligible","policy_id":"capital-v1","metric":"cet1","scenario":0,"month":1,"units":[{"template":"loan-unit","h":0,"side":1.}],"numerator":1.,"denominator":1.,"required_ratio":0.1,"numerator_per_unit":[0.],"denominator_per_unit":[1.]}]);
    let r = product_library::optimize(&v).unwrap();
    near(r["allocation"][0]["face"].as_f64().unwrap(), 9.);
    assert!(PreparedLibrary::new(&v)
        .unwrap()
        .evaluate(&selected(20.))
        .is_err());
    v["product"]["identity"]["input_revision"] = json!("new");
    assert!(PreparedLibrary::new(&v)
        .err()
        .unwrap()
        .contains("CAPITAL_REFRESH_REQUIRED"));
}
#[test]
fn immutable_session_stages_constraints_then_publishes_or_aborts() {
    let (mut s, initial) = Session::new(&normalize(raw())).unwrap();
    let r = s
        .call(json!({"op":"eval","version":0,"allocation":[{"unit_id":"loan-unit","face":20.}]}))
        .unwrap();
    assert_eq!(r["work"]["coefficient_only"], true);
    let mut c = normalize(raw())["constraints"].clone();
    c["max_total_assets"] = json!(30.);
    let pending = s
        .call(json!({"op":"update_product","version":0,"constraints":c}))
        .unwrap();
    assert_eq!(pending["work"]["units_rebuilt"], 0);
    assert_eq!(pending["work"]["full_rebuild"], false);
    assert_eq!(pending["product_preparation"], initial["preparation"]);
    near(pending["allocation"][0]["face"].as_f64().unwrap(), 30.);
    let status = s.call(json!({"op":"status","version":0})).unwrap();
    assert_eq!(status["snapshot"], initial);
    assert!(s
        .call(json!({"op":"publish","version":0,"token":999}))
        .is_err());
    s.call(json!({"op":"abort","version":0,"token":pending["token"]}))
        .unwrap();
    let pending = s
        .call(json!({"op":"update_product","version":0,"constraints":c}))
        .unwrap();
    s.call(json!({"op":"publish","version":0,"token":pending["token"]}))
        .unwrap();
    assert!(s.call(json!({"op":"solve","version":0})).is_err());
    let r = s.call(json!({"op":"solve","version":1})).unwrap();
    near(r["allocation"][0]["face"].as_f64().unwrap(), 30.);
}

#[test]
fn supplied_two_unit_example_has_independent_unique_optimal_money() {
    let v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/strategy-product-1.json"
    ))
    .unwrap();
    let r = product_library::optimize(&v).unwrap();
    assert_eq!(r["feasible"], true);
    near(r["allocation"][0]["face"].as_f64().unwrap(), 20.);
    near(r["allocation"][1]["face"].as_f64().unwrap(), 80.);
    let mut total = 20.;
    let mut purchase = 0.;
    for (y, face) in [(0.02f64, 20.), (0.03f64, 80.)] {
        let q = 529. * (-y.ln_1p() * 29. / 365.).exp() + 515.5 * (-y.ln_1p() * 60. / 365.).exp();
        let i1 = q * (y.ln_1p() * 29. / 365.).exp_m1();
        let i2 = (q + i1 - 529.) * (y.ln_1p() * 31. / 365.).exp_m1();
        total += (i1 + i2) * face / 1000.;
        purchase += q * face / 1000.;
    }
    near(r["worst_case_nii_$"].as_f64().unwrap(), total);
    near(
        r["source_replay"]["total_asset_purchase_cost"]
            .as_f64()
            .unwrap(),
        purchase,
    );
    near(
        r["source_replay"]["scenarios"][0]["cash"][2]["cumulative_bank_cash"]
            .as_f64()
            .unwrap(),
        104.45 - purchase,
    );
}

#[test]
fn raw_product_updates_require_full_rebuild_and_bad_quote_override_rejects() {
    let original = normalize(raw());
    let (mut s, _) = Session::new(&original).unwrap();
    let mut changed = original["product"].clone();
    changed["identity"]["input_revision"] = json!("explicit-revision-2");
    let staged = s
        .call(json!({"op":"update_product","version":0,"product":changed}))
        .unwrap();
    assert_eq!(staged["work"]["full_rebuild"], true);
    assert_eq!(staged["work"]["units_rebuilt"], 1);
    assert_eq!(
        staged["product_preparation"]["preparation"]["raw_source"]["identity"]["input_revision"],
        "explicit-revision-2"
    );
    s.call(json!({"op":"abort","version":0,"token":staged["token"]}))
        .unwrap();
    let mut bad = original.clone();
    bad["product"]["units"][0]["quote_price_per_face_unit"] = json!(1.1);
    assert!(PreparedLibrary::new(&bad)
        .err()
        .unwrap()
        .contains("reconcile"));
    let mut bad = original;
    bad["scalar_nii_override"] = json!(999.);
    assert!(PreparedLibrary::new(&bad).is_err());
    let mut malformed = changed;
    malformed["units"][0]["purchase_graph"]["request"]["currency"] = json!("EUR");
    assert!(s
        .call(json!({"op":"update_product","version":0,"product":malformed}))
        .is_err());
    assert_eq!(
        s.call(json!({"op":"status","version":0})).unwrap()["pending"],
        false
    );
}
