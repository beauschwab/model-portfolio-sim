//! Proposed installed protocol gate; isolated subset intentionally omits term dispatcher.
use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};
fn envelope() -> Value {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap();
    json!({"schema":"market-indexed-adapter-1","threads":1,"request":raw["request"]})
}
#[test]
fn explicit_analytical_dispatch_uses_actual_native_products() {
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope()).unwrap());
    assert!(ok);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["schema"], "market-indexed-adapter-1");
    assert_eq!(value["result"]["cases"].as_array().unwrap().len(), 8);
    assert_eq!(value["result"]["capital_ftp_eligible"], false);
}
#[test]
fn real_term_bytes_reject_nested_duplicates_before_finance() {
    let bytes = serde_json::to_string(&envelope()).unwrap().replacen(
        "\"right_tail\":\"reject\"",
        "\"right_tail\":\"reject\",\"right_tail\":\"flat_forward\"",
        1,
    );
    let (response, ok) = respond(bytes.as_bytes());
    assert!(!ok);
    assert!(serde_json::from_slice::<Value>(&response).unwrap()["error"]
        .as_str()
        .unwrap()
        .contains("duplicate field"));
}
