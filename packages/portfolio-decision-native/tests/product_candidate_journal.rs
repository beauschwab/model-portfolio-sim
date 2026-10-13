use portfolio_decision_native::product_candidate_journal::{evaluate, run, Request};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-candidate-journal-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
#[test]
fn acquisition_and_two_loan_complete_cash_are_independently_conserved() {
    let r = request(raw());
    let out = evaluate(&r).unwrap();
    let cost = 20. * r.product.units[0].quote_price_per_face_unit
        + 80. * r.product.units[1].quote_price_per_face_unit;
    let acquisition: Vec<_> = out
        .journals
        .iter()
        .filter(|j| j.phase == "acquisition" && j.source_scenario_id == "base")
        .collect();
    assert_eq!(acquisition.len(), 2);
    near(
        acquisition
            .iter()
            .flat_map(|j| &j.postings)
            .filter(|p| p.gl_account == "cash")
            .map(|p| p.value)
            .sum(),
        -cost,
    );
    near(
        acquisition
            .iter()
            .flat_map(|j| &j.postings)
            .filter(|p| p.gl_account == "asset_principal")
            .map(|p| p.value)
            .sum(),
        100.,
    );
    near(out.complete_closing_accounts[0].cash, 1000. - cost + 104.45);
    near(
        out.complete_closing_accounts[0].cumulative_net_effective_income,
        104.45 - cost,
    );
    for j in &out.journals {
        near(j.postings.iter().map(|p| p.value).sum(), 0.);
    }
    assert!(out
        .complete_closing_positions
        .iter()
        .all(|p| p.allocated_principal == 0.
            && p.accrued_interest.abs() < 1e-9
            && p.gross_carrying_value.abs() < 1e-9));
    assert_eq!(out.daily_accounts.len(), 122);
    assert!(!out.independent_persisted_verification && !out.dynamic_mixed_acceptance);
}
#[test]
fn zero_allocations_retain_source_paths_and_balanced_zero_events() {
    let mut v = raw();
    for a in v["allocation"].as_array_mut().unwrap() {
        a["face"] = json!(0.);
    }
    let out = evaluate(&request(v)).unwrap();
    assert_eq!(out.source_capture.paths.len(), 4);
    assert_eq!(out.complete_closing_positions.len(), 4);
    near(out.complete_closing_accounts[0].cash, 1000.);
    assert!(!out.journals.is_empty());
    assert!(out
        .journals
        .iter()
        .flat_map(|j| &j.postings)
        .all(|p| p.value == 0.));
}
#[test]
fn unsupported_overlap_and_opening_fail_without_output() {
    for change in 0..7 {
        let mut v = raw();
        match change {
            0 => v["specification"]["accounts"][0]["equity"] = json!(900.),
            1 => v["specification"]["positions"][0]["annual_pd"] = json!(0.01),
            2 => v["specification"]["accounts"][0]["annual_fees"] = json!(1.),
            3 => v["owners"][0]["pre_purchase_principal"] = json!(1.),
            4 => v["policy"]["marks"] = json!("infer"),
            5 => v["owners"][1]["position_id"] = v["owners"][0]["position_id"].clone(),
            _ => v["specification"]["accounts"][0]["cash_floor"] = json!(999.),
        }
        let mut bytes = vec![];
        assert!(run(&request(v), &mut bytes).is_err(), "change {change}");
        assert!(bytes.is_empty());
    }
}
#[test]
fn cancellation_never_emits_candidate_output() {
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    let mut bytes = vec![];
    let e = portfolio_compute_control::scope(Some(c), || run(&request(raw()), &mut bytes))
        .err()
        .unwrap();
    assert_eq!(e, "COMPUTE_CANCELLED");
    assert!(bytes.is_empty());
}
#[test]
fn reporting_end_retains_later_actual_cash_tail() {
    let mut v = raw();
    let end = 738946;
    v["reporting_end_ordinal"] = json!(end);
    v["specification"]["horizon_days"] = json!(30);
    v["product"]["reporting_ordinals"] = json!([738916, end]);
    for u in v["product"]["units"].as_array_mut().unwrap() {
        u["purchase_graph"]["request"]["reporting_ordinals"] = json!([738916, end]);
        u["age_branches"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["ordinal"] == 738916);
        for b in u["age_branches"].as_array_mut().unwrap() {
            b["graph"]["request"]["reporting_ordinals"] = json!([738916, end]);
        }
    }
    let out = evaluate(&request(v)).unwrap();
    assert_eq!(out.full_end_ordinal, 738976);
    assert!(out
        .reported_positions
        .iter()
        .any(|p| p.allocated_principal > 0.));
    assert!(out
        .complete_closing_positions
        .iter()
        .all(|p| p.allocated_principal == 0.));
    assert!(out
        .journals
        .iter()
        .any(|j| j.ordinal > end && j.phase == "settlement"));
}
#[test]
fn aggregate_account_state_admission_happens_before_output() {
    let mut v = raw();
    let accounts = v["specification"]["accounts"].as_array_mut().unwrap();
    for i in 0..4100 {
        accounts.push(json!({"id":format!("unused-{i}"),"entity":format!("entity-{i}"),"currency":"USD","cash":0.,"equity":0.,"cet1_floor":0.,"leverage_floor":0.}));
    }
    let mut bytes = vec![];
    let e = run(&request(v), &mut bytes).err().unwrap();
    assert!(e.contains("accounts must be a list of at most 50"), "{e}");
    assert!(bytes.is_empty());
}
#[test]
fn future_acquisition_retains_zero_prepurchase_state() {
    let mut v = raw();
    v["as_of_ordinal"] = json!(738915);
    v["product"]["reporting_ordinals"] = json!([738915, 738916, 738945, 738976]);
    v["specification"]["horizon_days"] = json!(61);
    let r = request(v);
    let out = evaluate(&r).unwrap();
    assert!(out
        .daily_positions
        .iter()
        .filter(|p| p.ordinal == 738915)
        .all(|p| !p.acquired && p.allocated_principal == 0. && p.gross_carrying_value == 0.));
    assert!(out
        .journals
        .iter()
        .filter(|j| j.phase == "acquisition")
        .all(|j| j.ordinal == 738916));
    let cost = 20. * r.product.units[0].quote_price_per_face_unit
        + 80. * r.product.units[1].quote_price_per_face_unit;
    near(out.complete_closing_accounts[0].cash, 1000. - cost + 104.45);
}
#[test]
fn retained_metadata_capacity_rejects_before_zero_allocation_generation() {
    fn rename_position(value: &mut Value, original: &str, replacement: &str) {
        match value {
            Value::String(s) if s == original => *s = replacement.into(),
            Value::Array(values) => {
                for value in values {
                    rename_position(value, original, replacement);
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    rename_position(value, original, replacement);
                }
            }
            _ => {}
        }
    }
    let mut v = raw();
    let unit = v["product"]["units"][0].clone();
    let owner = v["owners"][0].clone();
    let position = v["specification"]["positions"][0].clone();
    v["product"]["units"] = json!([]);
    v["owners"] = json!([]);
    v["allocation"] = json!([]);
    v["specification"]["positions"] = json!([]);
    for i in 0..32 {
        let id = format!("unit-{i}");
        let pid = format!("position-{i}");
        let mut u = unit.clone();
        u["template_id"] = json!(format!("template-{i}"));
        rename_position(&mut u, unit["unit_id"].as_str().unwrap(), &id);
        v["product"]["units"].as_array_mut().unwrap().push(u);
        let mut o = owner.clone();
        o["unit_id"] = json!(id);
        o["position_id"] = json!(pid);
        o["source"]["id"] = json!(format!("source-{i}"));
        v["owners"].as_array_mut().unwrap().push(o);
        v["allocation"]
            .as_array_mut()
            .unwrap()
            .push(json!({"unit_id":id,"face":0.}));
        let mut p = position.clone();
        p["id"] = json!(pid);
        v["specification"]["positions"]
            .as_array_mut()
            .unwrap()
            .push(p);
    }
    let mut bytes = vec![];
    let error = run(&request(v), &mut bytes).err().unwrap();
    assert_eq!(
        error,
        "candidate aggregate journal/state/output work admission exceeded before generation"
    );
    assert!(bytes.is_empty());
}
