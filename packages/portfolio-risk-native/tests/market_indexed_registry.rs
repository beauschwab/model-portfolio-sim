use portfolio_risk_native::{incremental, market_indexed_graph, term_protocol::respond};
use serde_json::{json, Value};
use std::sync::Mutex;
static SERIAL: Mutex<()> = Mutex::new(());
fn raw() -> Value {
    let value: Value = serde_json::from_str(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap();
    json!({"schema":"market-indexed-graph-1","product":"vanilla_swap","source":value["request"]})
}
fn call(request: Value) -> Result<Value, String> {
    let (bytes, ok) = respond(
        &serde_json::to_vec(&json!({"schema":"graph-1","threads":2,"request":request})).unwrap(),
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    if ok {
        Ok(value["result"].clone())
    } else {
        Err(value["error"].as_str().unwrap().into())
    }
}
fn create(r: Value) -> Result<Value, String> {
    call(json!({"op":"create_market_indexed","max_bytes":8388608,"max_entries":1024,"request":r}))
}
fn drop_handle(handle: u64) {
    assert_eq!(
        call(json!({"op":"drop","handle":handle})).unwrap(),
        json!({})
    );
}
pub fn legacy_price() -> Value {
    let account = json!({"market":{"config":{"paths":1,"months":2,"forwards":4,"factors":1,"dt":1./12.,"tenor":0.25,"shift":0.03,"curve_bump":0.0001,"vol_bump":0.01},"tenors":[1.,2.],"swap_rates":[0.02,0.02],"vol_quotes":[1.,0.2,0.2],"seed":[1]},"asof":738886,"horizon":1,"mbs":null,"terms":[],"deposits":null,"mm":null,"hedges":null,"withdrawal_parameters":[],"anchors":{},"deposit_initial_rate":null,"forecast":null,"draws":null,"capture_anchor":false,"capture_cashflows":false});
    json!({"base":account,"current":account,"spread_shift":0.,"overrides":{},"include_analytics":false,"include_key_rates":false,"kpis":null,"auxiliary":null})
}
#[test]
fn real_registry_prices_then_warms_and_invalidates() {
    let _lock = SERIAL.lock().unwrap();
    let r = raw();
    let created = create(r.clone()).unwrap();
    let handle = created["handle"].as_u64().unwrap();
    assert_eq!(created["owner"], market_indexed_graph::SCHEMA);
    assert!(
        created["initial"]["counts"]["projection"]["computed"]
            .as_u64()
            .unwrap()
            > 0
    );
    let warm = call(json!({"op":"price_market_indexed","handle":handle,"request":r})).unwrap();
    assert_eq!(created["initial"]["cases"], warm["cases"]);
    for counts in warm["counts"].as_object().unwrap().values() {
        assert_eq!(counts["computed"], 0);
    }
    let mut changed = r.clone();
    changed["source"]["bridge"]["current_market"]["curves"][0]["discount_factors"][1] =
        json!(0.979);
    let out = call(json!({"op":"price_market_indexed","handle":handle,"request":changed})).unwrap();
    assert_eq!(out["counts"]["indexed-cash"]["computed"], 0);
    assert!(out["counts"]["swap-clean"]["computed"].as_u64().unwrap() > 0);
    let info = call(json!({"op":"info","handle":handle})).unwrap();
    assert_eq!(info["owner"], market_indexed_graph::SCHEMA);
    assert!(info["cache"]["entries"].as_u64().unwrap() > 0);
    let cleared = call(json!({"op":"clear","handle":handle})).unwrap();
    assert_eq!(cleared["cache"]["entries"], 0);
    assert_eq!(
        call(json!({"op":"price_market_indexed","handle":handle,"request":r})).unwrap()["cases"],
        warm["cases"]
    );
    drop_handle(handle);
    assert!(call(json!({"op":"info","handle":handle}))
        .unwrap_err()
        .contains("unknown graph"));
}
#[test]
fn owner_mismatch_has_no_legacy_coercion_or_default_success() {
    let _lock = SERIAL.lock().unwrap();
    let legacy = call(json!({"op":"create","max_bytes":1000000,"max_entries":100})).unwrap()
        ["handle"]
        .as_u64()
        .unwrap();
    let market = create(raw()).unwrap()["handle"].as_u64().unwrap();
    assert!(
        call(json!({"op":"price_market_indexed","handle":legacy,"request":raw()}))
            .unwrap_err()
            .contains("OWNER_MISMATCH")
    );
    for op in ["price", "compare"] {
        let mut request = json!({"op":op,"handle":market,"request":legacy_price()});
        if op == "compare" {
            request["patches"] = json!({});
            request["calibration_mode"] = json!("fixed");
        }
        assert!(call(request).unwrap_err().contains("OWNER_MISMATCH"));
    }
    let info = call(json!({"op":"clear","handle":legacy})).unwrap();
    assert!(info.get("owner").is_none());
    assert_eq!(info["entries"], 0);
    assert!(call(json!({"op":"price","handle":legacy,"request":legacy_price()})).is_ok());
    drop_handle(legacy);
    drop_handle(market);
}
#[test]
fn original_identity_and_invalid_warm_requests_reject() {
    let _lock = SERIAL.lock().unwrap();
    let r = raw();
    let h = create(r.clone()).unwrap()["handle"].as_u64().unwrap();
    for which in ["oas", "schema", "currency"] {
        let mut bad = r.clone();
        match which {
            "oas" => bad["source"]["bridge"]["original"]["locked_oas"] = json!(0.021),
            "schema" => bad["schema"] = json!("market-indexed-graph-2"),
            _ => bad["source"]["bridge"]["current_market"]["curves"][0]["currency"] = json!("EUR"),
        }
        assert!(call(json!({"op":"price_market_indexed","handle":h,"request":bad})).is_err());
    }
    assert!(call(json!({"op":"price_market_indexed","handle":h,"request":r})).is_ok());
    drop_handle(h);
}
#[test]
fn failed_creation_limit_and_cancel_leave_no_registered_owner() {
    let _lock = SERIAL.lock().unwrap();
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    let signal = control.clone();
    let error = portfolio_compute_control::scope(Some(control), || {
        signal.cancel();
        assert!(create(raw()).unwrap_err().contains("CANCELLED"));
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("CANCELLED"));
    let mut bad = raw();
    bad["source"]["bridge"]["binding"]["index_id"] = json!("unknown");
    assert!(create(bad).is_err());
    let mut handles = Vec::new();
    for _ in 0..128 {
        handles.push(
            call(json!({"op":"create","max_bytes":0,"max_entries":0})).unwrap()["handle"]
                .as_u64()
                .unwrap(),
        );
    }
    assert!(create(raw()).unwrap_err().contains("too many active"));
    for handle in handles {
        drop_handle(handle);
    }
    let h = create(raw()).unwrap()["handle"].as_u64().unwrap();
    drop_handle(h);
}
#[test]
fn retained_initial_request_capacity_is_not_lost_on_shorter_edits() {
    let _lock = SERIAL.lock().unwrap();
    let mut r = raw();
    r["source"]["bridge"]["current_market"]["identity"]["source_revision"] = json!("x".repeat(128));
    let created = create(r.clone()).unwrap();
    let h = created["handle"].as_u64().unwrap();
    let original: market_indexed_graph::Request = serde_json::from_value(r.clone()).unwrap();
    let original_size = serde_json::to_vec(&original).unwrap().len();
    r["source"]["bridge"]["current_market"]["identity"]["source_revision"] = json!("r2");
    let current: market_indexed_graph::Request = serde_json::from_value(r.clone()).unwrap();
    let current_size = serde_json::to_vec(&current).unwrap().len();
    let fresh = market_indexed_graph::Graph::new(&current, 8388608, 1024)
        .unwrap()
        .price(&current)
        .unwrap();
    let shorter = call(json!({"op":"price_market_indexed","handle":h,"request":r})).unwrap();
    assert_eq!(
        shorter["admitted_joined_bytes"].as_u64().unwrap() as usize,
        fresh.admitted_joined_bytes + 16 * (original_size - current_size)
    );
    drop_handle(h);
}
#[test]
fn dispatcher_propagates_delayed_caller_control_into_market_price() {
    let _lock = SERIAL.lock().unwrap();
    let r = raw();
    let h = create(r.clone()).unwrap()["handle"].as_u64().unwrap();
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    let signal = control.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5));
        signal.cancel();
    });
    // Active scope is entered while uncancelled; each actual Rayon dispatcher must see it.
    let error = portfolio_compute_control::scope(Some(control), || {
        for _ in 0..100 {
            call(json!({"op":"price_market_indexed","handle":h,"request":r}))?;
        }
        Ok(())
    })
    .unwrap_err();
    worker.join().unwrap();
    assert!(error.contains("CANCELLED"));
    drop_handle(h);
}
#[test]
fn real_bytes_keep_nested_strict_shape_and_duplicates() {
    let _lock = SERIAL.lock().unwrap();
    let envelope = json!({"schema":"graph-1","threads":1,"request":{"op":"create_market_indexed","max_bytes":1,"max_entries":1,"request":raw()}});
    for bytes in [
        serde_json::to_string(&envelope).unwrap().replacen(
            "\"right_tail\":\"reject\"",
            "\"right_tail\":{\"reject\":null}",
            1,
        ),
        serde_json::to_string(&envelope).unwrap().replacen(
            "\"right_tail\":\"reject\"",
            "\"right_tail\":\"reject\",\"right_tail\":\"reject\"",
            1,
        ),
    ] {
        assert!(!respond(bytes.as_bytes()).1);
    }
    // Public type includes the versioned owner, never deserializes a legacy PriceRequest.
    let request: incremental::Request =
        serde_json::from_value(envelope["request"].clone()).unwrap();
    assert!(request.is_market_indexed());
}
#[test]
fn original_product_version_and_signed_zero_oas_stay_immutable() {
    let _lock = SERIAL.lock().unwrap();
    let mut r = raw();
    r["source"]["bridge"]["original"]["locked_oas"] = json!(0.0);
    let h = create(r.clone()).unwrap()["handle"].as_u64().unwrap();
    let mut zero = r.clone();
    zero["source"]["bridge"]["original"]["locked_oas"] = json!(-0.0);
    assert!(
        call(json!({"op":"price_market_indexed","handle":h,"request":zero}))
            .unwrap_err()
            .contains("BASE_ANCHOR_CHANGED")
    );
    r["product"] = json!("indexed_loan");
    assert!(
        call(json!({"op":"price_market_indexed","handle":h,"request":r}))
            .unwrap_err()
            .contains("BASE_ANCHOR_CHANGED")
    );
    drop_handle(h);
}
#[test]
fn caller_deadline_creation_is_not_registered() {
    let _lock = SERIAL.lock().unwrap();
    let control = portfolio_compute_control::Control::new(1).unwrap();
    let error = portfolio_compute_control::scope(Some(control), || {
        std::thread::sleep(std::time::Duration::from_millis(3));
        assert!(create(raw()).unwrap_err().contains("DEADLINE"));
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("DEADLINE"));
}
#[test]
fn legacy_serialized_golden_outputs_remain_unchanged() {
    let _lock = SERIAL.lock().unwrap();
    let h = call(json!({"op":"create","max_bytes":1000000,"max_entries":100})).unwrap()["handle"]
        .as_u64()
        .unwrap();
    let golden: Value =
        serde_json::from_str(include_str!("fixtures/market_indexed_legacy_outputs.json")).unwrap();
    let mut results = Vec::new();
    for request in [
        json!({"op":"info","handle":h}),
        json!({"op":"price","handle":h,"request":legacy_price()}),
        json!({"op":"compare","handle":h,"request":legacy_price(),"patches":{},"calibration_mode":"hold"}),
        json!({"op":"clear","handle":h}),
        json!({"op":"drop","handle":h}),
    ] {
        results.push(call(request).unwrap());
    }
    assert_eq!(
        serde_json::to_vec(&results).unwrap(),
        serde_json::to_vec(&golden).unwrap()
    );
}
