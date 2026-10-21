//! Stage H on the service: the independent check of a producer's allocation receipt and the
//! coefficient-only evaluation.
use serde_json::{json, Value};
use workbench_native::allocation::{check, evaluate, merge};

fn request(id: &str, actions: usize) -> Value {
    json!({"dated": {"d": 1}, "hedge_contracts": [],
        "plan": {"entity": "bank", "actions": (0..actions).map(|i| json!({"action_id": format!("{id}-{i}")})).collect::<Vec<_>>()},
        "acquisitions": [{"scenario_id": "s", "horizon_end_ordinal": 2, "sales": [{"instruction_id": id}]}]})
}
fn raw() -> Value {
    json!({"schema": "liquidity-allocation-1", "base": request("base", 0),
    "packages": [
        {"package_id": "a", "group": "g", "review": {}, "request": request("a", 1)},
        {"package_id": "b", "group": "h", "review": {}, "request": request("b", 1)},
    ]})
}
fn receipt(selected: Value, combined: Value) -> Value {
    json!({"schema": "liquidity-allocation-1", "exact_replay_passed": true, "selected": selected, "combined_request": combined,
    "objective": "worst_case_external_nii", "minimum_usable_cash": 0.0, "scenarios": ["s"],
    "base": {"s": {"external_nii": 1.0, "trading_pnl": 0.0, "funding_cost": 0.0}}, "base_daily_minimum": {"s": 5.0},
    "packages": [
        {"package_id": "a", "group": "g", "eligible": true, "coefficients": {"s": {"external_nii": 0.5, "trading_pnl": 2.0, "funding_cost": 1.0, "usable_cash_deterioration": -3.0}}},
        {"package_id": "b", "group": "h", "eligible": false, "refusal": "FORMAL_OWNER_REQUIRED: formal", "coefficients": {"s": {"external_nii": 0.0, "trading_pnl": 0.0, "funding_cost": 0.0, "usable_cash_deterioration": 0.0}}},
    ]})
}

#[test]
fn the_replayed_request_must_equal_the_independent_merge() {
    let r = raw();
    let a = &r["packages"][0]["request"];
    assert_eq!(
        check(&r, &receipt(json!(["a"]), a.clone()), b"x")
            .unwrap()
            .unwrap(),
        serde_json::to_vec(a).unwrap()
    );
    // A producer that replayed something else is refused.
    let mut forged = a.clone();
    forged["acquisitions"][0]["sales"] = json!([]);
    assert!(check(&r, &receipt(json!(["a"]), forged), b"x")
        .unwrap_err()
        .detail
        .contains("independent merge"));
    // An ineligible or unknown package cannot be selected.
    let both = merge(&r["base"], &[a, &r["packages"][1]["request"]]).unwrap();
    assert!(check(&r, &receipt(json!(["a", "b"]), both), b"x")
        .unwrap_err()
        .detail
        .contains("ineligible"));
    assert!(check(&r, &receipt(json!(["z"]), a.clone()), b"x")
        .unwrap_err()
        .detail
        .contains("unknown"));
    // An unvalidated allocation carries no replay stream.
    let mut failed = receipt(Value::Null, Value::Null);
    failed["exact_replay_passed"] = json!(false);
    assert!(check(&r, &failed, b"").unwrap().is_none());
    assert!(check(&r, &failed, b"stream").is_err());
}

#[test]
fn evaluation_adds_prepared_coefficients_and_never_validates() {
    let r = receipt(json!(["a"]), Value::Null);
    let e = evaluate(&r, &["a".into()], None).unwrap();
    assert_eq!(e["validated"], false);
    assert_eq!(e["status"], "estimate");
    assert_eq!(e["scenarios"]["s"]["external_nii"], 1.5);
    assert_eq!(e["scenarios"]["s"]["minimum_usable_cash_bound"], 2.0);
    assert_eq!(e["within_usable_cash_bound"], true);
    // A constraint-only change reuses the coefficients.
    assert_eq!(
        evaluate(&r, &["a".into()], Some(3.0)).unwrap()["within_usable_cash_bound"],
        false
    );
    assert_eq!(
        evaluate(&r, &[], None).unwrap()["worst_case_objective"],
        1.0
    );
    assert_eq!(
        evaluate(&r, &["b".into()], None).unwrap_err().code,
        "INELIGIBLE_PACKAGE"
    );
    assert_eq!(
        evaluate(&r, &["a".into(), "a".into()], None)
            .unwrap_err()
            .code,
        "MALFORMED_INPUT"
    );
}
