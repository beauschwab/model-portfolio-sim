use portfolio_risk_native::market_driver_calibration::{evaluate, evaluate_with_check, Request};
use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/market-driver-calibration-1.json"
    ))
    .unwrap()
}

#[test]
fn training_moments_and_chronological_holdout_are_explicit() {
    let result = evaluate(&request()).unwrap();
    assert_eq!(result["schema"], "market-driver-calibration-1");
    assert_eq!(result["adoption_eligible"], false);
    assert_eq!(result["diagnostics"]["training_rows"], 30);
    assert_eq!(result["diagnostics"]["holdout_rows"], 10);
    assert_eq!(result["diagnostics"]["uncertainty"]["lag"], 2);
    assert_eq!(
        result["diagnostics"]["uncertainty"]["serial_dependence_adjusted"],
        true
    );
    assert_eq!(result["training_split_id"], "train-through-30");
    assert_eq!(result["holdout_split_id"], "holdout-31-40");
}

#[test]
fn changing_only_holdout_rows_does_not_change_training_moments() {
    let mut raw = request();
    let before = evaluate(&raw).unwrap();
    raw.values[39][0] = 0.4;
    let after = evaluate(&raw).unwrap();
    assert_eq!(before["diagnostics"]["mean"], after["diagnostics"]["mean"]);
    assert_eq!(
        before["diagnostics"]["covariance"],
        after["diagnostics"]["covariance"]
    );
    assert_eq!(
        before["diagnostics"]["uncertainty"],
        after["diagnostics"]["uncertainty"]
    );
    assert_ne!(
        before["diagnostics"]["holdout_rmse"],
        after["diagnostics"]["holdout_rmse"]
    );
}

#[test]
fn public_dispatch_and_cancellation_are_fail_closed() {
    let envelope = json!({"schema":"market-driver-calibration-1","threads":1,"request":request()});
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["schema"], "market-driver-calibration-1");

    let mut calls = 0;
    let error = evaluate_with_check(&request(), || {
        calls += 1;
        if calls > 1 {
            Err("deadline".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error, "deadline");
}
