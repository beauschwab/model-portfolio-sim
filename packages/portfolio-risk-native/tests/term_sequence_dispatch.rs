//! Actual installed dispatcher gates, authored pending copied/default Risk build.
use portfolio_risk_native::term_protocol;
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/term-product-graph-2.json")).unwrap()
}
fn call(schema: &str, r: Value) -> (Value, bool) {
    let (v, ok) = term_protocol::respond(
        &serde_json::to_vec(&json!({"schema":schema,"threads":1,"request":r})).unwrap(),
    );
    (serde_json::from_slice(&v).unwrap(), ok)
}
#[test]
fn selected_actual_dispatch_values_sequence_and_rejects_unselected_version() {
    let (v, ok) = call("term-product-graph-2", raw());
    assert!(ok, "{v}");
    let actual = v["result"]["metrics"][0]["pv"].as_f64().unwrap();
    assert!((actual - -101.).abs() <= 1e-9 + 1e-12 * actual.abs().max(101.));
    let (v, ok) = call("term-product-graph-1", raw());
    assert!(!ok);
    assert!(v["error"].as_str().unwrap().contains("unknown field"));
}
#[test]
fn actual_new_dispatch_rejects_nested_alias_and_duplicate_bytes() {
    let mut r = raw();
    r["original_base"]["scenario"]["collateral_curve"]["right_tail"] = json!({"reject":null});
    let (v, ok) = call("term-product-graph-2", r);
    assert!(!ok);
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("must be an enum string"));
    let bytes = serde_json::to_string(
        &json!({"schema":"term-product-graph-2","threads":1,"request":raw()}),
    )
    .unwrap()
    .replacen(
        "\"principal\":100",
        "\"principal\":100,\"principal\":100",
        1,
    );
    let (v, ok) = term_protocol::respond(bytes.as_bytes());
    let v: Value = serde_json::from_slice(&v).unwrap();
    assert!(!ok);
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("duplicate field `principal`"));
}

#[test]
fn old_selected_dto_keeps_historical_unknown_field_rejections() {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/term-product-graph-1.json"
    ))
    .unwrap();
    for (field, value) in [
        ("original_base", json!(null)),
        ("original_quotes", json!([])),
    ] {
        let mut changed = raw.clone();
        changed[field] = value;
        let (v, ok) = call("term-product-graph-1", changed);
        assert!(!ok);
        assert!(v["error"].as_str().unwrap().contains("unknown field"));
    }
}
