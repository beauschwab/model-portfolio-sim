use portfolio_decision_native::{
    commercial_library::{self, PreparedLibrary, Session},
    dispatch,
};
use portfolio_risk_native::product_strategy::Allocation;
use serde_json::{json, Value};
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn request() -> Value {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/strategy-commercial-1.json"
    ))
    .unwrap();
    let y = 0.03_f64;
    let quote = (100. + 100. * 0.1 * 30. / 365.) * (-y.ln_1p() * 35. / 365.).exp();
    let p = &mut v["commercial"]["units"][0]["graph"]["positions"][0];
    p["original_signed_cash_quote"] = json!(quote);
    p["oas_target"]["base_oas"] = json!(y.ln_1p());
    for source in p["original_pricing"].as_array_mut().unwrap().iter_mut() {
        source["source"]["source"]["acquisition"]["cost"] = json!(quote);
        source["source"]["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
        source["source"]["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    }
    for set in p["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for source in set[role].as_array_mut().unwrap() {
                source["source"]["source"]["acquisition"]["cost"] = json!(quote);
                source["source"]["source"]["acquisition"]["original_annual_effective_rate"] =
                    json!(y);
                source["source"]["accounting_rates"][0]["annual_effective_rates"] =
                    json!([y, y, y]);
            }
        }
    }
    v
}
fn allocation(face: f64) -> Vec<Allocation> {
    vec![Allocation {
        unit_id: "commercial".into(),
        face,
    }]
}
#[test]
fn native_commercial_income_is_maximized_with_purchase_cost_not_face_budget() {
    let v = request();
    let mut p = PreparedLibrary::new(&v).unwrap();
    let selected = p.evaluate(&allocation(50.)).unwrap();
    assert_eq!(selected["source_replay"]["source_cash_replayed"], false);
    let quote = v["commercial"]["units"][0]["graph"]["positions"][0]["original_signed_cash_quote"]
        .as_f64()
        .unwrap();
    let expected = quote * (0.03_f64.ln_1p() * 15. / 365.).exp_m1() * 0.5;
    near(
        selected["source_replay"]["scenarios"][0]["incremental_external_nii"]
            .as_f64()
            .unwrap(),
        expected,
    );
    let solved = p.solve().unwrap();
    assert_eq!(solved["source_replay"]["source_cash_replayed"], true);
    assert_eq!(solved["feasible"], true);
    near(
        solved["native_allocated_face"][0].as_f64().unwrap(),
        10000. / quote,
    );
    near(
        solved["worst_case_nii_$"].as_f64().unwrap(),
        expected * 200. / quote,
    );
    assert!(p.evaluate(&allocation(100.)).is_err());
}
#[test]
fn retained_constraint_updates_do_not_regenerate_sources_and_require_transaction_identity() {
    let (mut s, snapshot) = Session::new(&request()).unwrap();
    let source = snapshot["preparation"].clone();
    let evaluated = s
        .call(json!({"op":"eval","version":0,"allocation":[{"unit_id":"commercial","face":50.}]}))
        .unwrap();
    assert_eq!(evaluated["work"]["pricing_calls"], 0);
    let update = s
        .call(json!({"op":"update_constraints","version":0,"max_purchase_cost_assets":70.}))
        .unwrap();
    assert_eq!(update["work"]["units_rebuilt"], 0);
    assert!(s
        .call(json!({"op":"publish","version":0,"token":99}))
        .is_err());
    let published = s
        .call(json!({"op":"publish","version":0,"token":update["token"]}))
        .unwrap();
    assert_eq!(published["version"], 1);
    let status = s.call(json!({"op":"status","version":1})).unwrap();
    assert_eq!(status["snapshot"]["preparation"], source);
    assert!(s
        .call(json!({"op":"eval","version":0,"allocation":[{"unit_id":"commercial","face":50.}]}))
        .is_err());
    assert!(s
        .call(json!({"op":"update_constraints","version":1,"commercial":{"values":[1.]}}))
        .is_err());
}
#[test]
fn wrong_source_and_missing_capital_identity_reject_before_financial_acceptance() {
    let mut raw = request();
    raw["commercial"]["units"][0]["unit_face"] = json!(101.);
    assert!(PreparedLibrary::new(&raw).is_err());
    let mut raw = request();
    raw["face_limits"][0]["selector"]["coefficient"] = json!(1.);
    assert!(commercial_library::optimize(&raw).is_err());
    let mut raw = request();
    raw["constraints"]["capital_limits"] = json!([{"label":"capital","policy_id":"explicit","metric":"cet1","scenario":0,"month":0,"units":[],"numerator":1.,"denominator":1.,"required_ratio":0.,"numerator_per_unit":[],"denominator_per_unit":[]}]);
    assert!(PreparedLibrary::new(&raw)
        .err()
        .map(|s| s.contains("CAPITAL_REFRESH_REQUIRED"))
        .unwrap());
    let p = PreparedLibrary::new(&request()).unwrap();
    assert!(p
        .evaluate(&[Allocation {
            unit_id: "wrong".into(),
            face: 0.
        }])
        .is_err());
}
#[test]
fn public_actor_retains_native_preparation_and_closes_handle() {
    let mut raw = request();
    raw["op"] = json!("create_commercial");
    let created = dispatch(raw).unwrap();
    let handle = created["handle"].clone();
    let evaluated=dispatch(json!({"op":"eval","handle":handle,"version":0,"allocation":[{"unit_id":"commercial","face":50.}]})).unwrap();
    assert_eq!(evaluated["validated"], true);
    assert_eq!(evaluated["work"]["credit_calls"], 0);
    dispatch(json!({"op":"close","handle":handle})).unwrap();
    assert!(dispatch(json!({"op":"status","handle":handle,"version":0})).is_err());
}

#[test]
fn caller_cancellation_reaches_commercial_creation_and_retained_actor_evaluation() {
    use portfolio_compute_control::{scope, Control};
    let control = Control::new(10_000).unwrap();
    let trigger = control.clone();
    let mut observed = None;
    let mut raw = request();
    raw["op"] = json!("create_commercial");
    let outer = scope(Some(control), || {
        trigger.cancel();
        observed = Some(dispatch(raw));
        Ok(())
    });
    assert_eq!(outer.err().unwrap(), "COMPUTE_CANCELLED");
    assert_eq!(observed.unwrap().err().unwrap(), "COMPUTE_CANCELLED");
    let mut raw = request();
    raw["op"] = json!("create_commercial");
    let created = dispatch(raw).unwrap();
    let handle = created["handle"].clone();
    let control = Control::new(10_000).unwrap();
    let trigger = control.clone();
    let mut observed = None;
    let outer = scope(Some(control), || {
        trigger.cancel();
        observed = Some(dispatch(
            json!({"op":"eval","handle":handle,"version":0,"allocation":[{"unit_id":"commercial","face":50.}]}),
        ));
        Ok(())
    });
    assert_eq!(outer.err().unwrap(), "COMPUTE_CANCELLED");
    assert_eq!(observed.unwrap().err().unwrap(), "COMPUTE_CANCELLED");
    let fresh=dispatch(json!({"op":"eval","handle":handle,"version":0,"allocation":[{"unit_id":"commercial","face":50.}]})).unwrap();
    assert_eq!(fresh["validated"], true);
    dispatch(json!({"op":"close","handle":handle})).unwrap();
}

#[test]
fn completed_creation_deadline_is_not_retained_by_new_actor_calls() {
    use portfolio_compute_control::{scope, Control};
    use std::{thread, time::Duration};
    let mut raw = request();
    raw["op"] = json!("create_commercial");
    let creation_control = Control::new(1000).unwrap();
    let created = scope(Some(creation_control.clone()), || dispatch(raw)).unwrap();
    let handle = created["handle"].clone();
    thread::sleep(Duration::from_millis(1005));
    assert_eq!(
        scope(Some(creation_control), || Ok(())).err().unwrap(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
    let mut observed = None;
    let outcome = scope(Some(Control::new(10_000).unwrap()), || {
        observed = Some(dispatch(
            json!({"op":"eval","handle":handle,"version":0,"allocation":[{"unit_id":"commercial","face":50.}]}),
        ));
        Ok(())
    });
    outcome.unwrap();
    assert_eq!(observed.unwrap().unwrap()["validated"], true);
    dispatch(json!({"op":"close","handle":handle})).unwrap();
}
