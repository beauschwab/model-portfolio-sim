use portfolio_risk_native::market_bootstrap::{evaluate, evaluate_with_check, Request};
use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/market-quote-bootstrap-1.json"
    ))
    .unwrap()
}

#[test]
fn explicit_schedule_bootstrap_and_validation_residuals_are_reported() {
    let result = evaluate(&request()).unwrap();
    assert_eq!(result.schema, "market-quote-bootstrap-1");
    assert_eq!(result.pillar_times_years, vec![0.0, 1.0, 2.0, 3.0]);
    assert_eq!(result.quote_diagnostics.len(), 4);
    assert_eq!(result.quote_diagnostics[0].kind, "bootstrap");
    assert_eq!(result.quote_diagnostics[3].kind, "validation");
    assert!(result.diagnostics.converged);
    assert!(result.quote_diagnostics[3].residual.abs() > 1e-5);
    assert!(!result.adoption_eligible);
}

#[test]
fn public_dispatch_and_cancellation_are_fail_closed() {
    let envelope = json!({"schema":"market-quote-bootstrap-1","threads":1,"request":request()});
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["schema"], "market-quote-bootstrap-1");

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

#[test]
fn non_pillar_intermediate_payment_is_rejected() {
    let mut request = request();
    request.bootstrap_quotes[1].payments[0].time_years = 1.5;
    let error = evaluate(&request).unwrap_err();
    assert!(error.contains("non-final bootstrap payment"));
}
