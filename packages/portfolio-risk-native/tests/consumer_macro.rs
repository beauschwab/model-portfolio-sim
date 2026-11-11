use portfolio_risk_native::term_protocol::{respond, respond_started};
fn envelope(products: bool) -> serde_json::Value {
    let text = if products {
        include_str!("../../../examples/models/consumer-macro-products-1.json")
    } else {
        include_str!("../../../examples/models/consumer-macro-scenarios-1.json")
    };
    let request: serde_json::Value = serde_json::from_str(text).unwrap();
    serde_json::json!({"schema":request["schema"], "threads":1, "execution_timeout_ms":30000, "request":request})
}
#[test]
fn both_macro_protocols_return_bounded_physical_results() {
    for products in [false, true] {
        let (bytes, ok) = respond(&serde_json::to_vec(&envelope(products)).unwrap());
        assert!(ok, "{}", String::from_utf8_lossy(&bytes));
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["result"]["pricing_measure_permission"], false);
        assert_eq!(v["result"]["empirical_accuracy_validated"], false);
        if products {
            assert_eq!(v["result"]["cases"].as_array().unwrap().len(), 4);
        } else {
            assert_eq!(
                v["result"]["diagnostics"]["training"]["row_ids"]
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
        }
    }
}
#[test]
fn selected_macro_rejects_hidden_duplicate_selector_and_expired_original_clock() {
    for products in [false, true] {
        let value = envelope(products);
        let schema = value["schema"].as_str().unwrap();
        let raw = serde_json::to_string(&value).unwrap();
        let duplicate = raw.replacen(
            &format!("\"schema\":\"{schema}\""),
            &format!("\"schema\":\"{schema}\",\"schema\":\"kpi-1\""),
            1,
        );
        let (bytes, ok) = respond(duplicate.as_bytes());
        assert!(!ok && String::from_utf8_lossy(&bytes).contains("duplicate"));
        let (bytes, ok) = respond_started(
            raw.as_bytes(),
            std::time::Instant::now() - std::time::Duration::from_secs(31),
        );
        assert!(!ok && String::from_utf8_lossy(&bytes).contains("DEADLINE"));
        let mut missing = value;
        missing
            .as_object_mut()
            .unwrap()
            .remove("execution_timeout_ms");
        assert!(!respond(&serde_json::to_vec(&missing).unwrap()).1);
    }
}
