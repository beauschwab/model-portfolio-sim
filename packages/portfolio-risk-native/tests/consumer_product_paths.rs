use portfolio_risk_native::term_protocol::{respond, respond_started};

fn envelope() -> serde_json::Value {
    let request: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-product-paths-1.json"
    ))
    .unwrap();
    serde_json::json!({"schema":"consumer-behaviour-product-paths-1","threads":1,"execution_timeout_ms":30000,"request":request})
}
#[test]
fn original_raw_protocol_fits_and_projects_one_monetary_product() {
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope()).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let text = serde_json::to_string(&result).unwrap();
    assert!(text.contains("prospective_product_rows") && text.contains("original_year"));
    assert!(text.contains("\"adoption_eligible\":false"));
}
#[test]
fn byte_duplicate_unknown_nested_key_and_original_deadline_reject() {
    let raw = serde_json::to_string(&envelope()).unwrap();
    let duplicate = raw.replace(
        "\"same_day_transitions\":\"reject\"",
        "\"same_day_transitions\":\"reject\",\"same_day_transitions\":\"reject\"",
    );
    let (bytes, ok) = respond(duplicate.as_bytes());
    assert!(!ok && String::from_utf8_lossy(&bytes).contains("duplicate"));
    let mut unknown = envelope();
    unknown["request"]["policy"]["fallback"] = serde_json::json!(0);
    assert!(!respond(&serde_json::to_vec(&unknown).unwrap()).1);
    let (bytes, ok) = respond_started(
        raw.as_bytes(),
        std::time::Instant::now() - std::time::Duration::from_secs(31),
    );
    assert!(!ok && String::from_utf8_lossy(&bytes).contains("DEADLINE"));
}
