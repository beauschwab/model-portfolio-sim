use portfolio_risk_native::market_indexed_adapter::{evaluate, Request};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap()["request"]
        .clone()
}
#[test]
fn historical_available_time_before_epoch_is_retained() {
    let mut r = raw();
    r["bridge"]["original"]["market"]["available_unix_seconds"] = json!(-1);
    r["bridge"]["current_market"]["available_unix_seconds"] = json!(-1);
    let result = evaluate(&serde_json::from_value::<Request>(r).unwrap()).unwrap();
    assert_eq!(
        result
            .cache_parent
            .bridge
            .original
            .market
            .available_unix_seconds,
        -1
    );
    assert_eq!(
        result
            .cache_parent
            .bridge
            .current_market
            .available_unix_seconds,
        -1
    );
}
#[test]
fn availability_before_supported_year_one_is_rejected() {
    for side in ["original", "current_market"] {
        let mut r = raw();
        let m = if side == "original" {
            &mut r["bridge"]["original"]["market"]
        } else {
            &mut r["bridge"][side]
        };
        m["available_unix_seconds"] = json!((1_i64 - 719163) * 86400 - 1);
        assert!(evaluate(&serde_json::from_value::<Request>(r).unwrap())
            .unwrap_err()
            .contains("source unavailable"));
    }
}
