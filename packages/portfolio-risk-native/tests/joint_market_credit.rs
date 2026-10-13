use portfolio_risk_native::joint_market_credit::{evaluate, evaluate_with_check, Request};
use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/joint-market-credit-1.json"
    ))
    .unwrap()
}

#[test]
fn generated_market_tape_drives_shared_credit_paths() {
    let result = evaluate(&request()).unwrap();
    assert_eq!(result.schema, "joint-market-credit-1");
    assert!(!result.adoption_eligible);
    assert_eq!(result.market.shape, [4, 2, 3]);
    assert_eq!(result.work_units, 8);
    assert_eq!(result.paths.len(), 4);
    assert_eq!(result.paths[0].events.len(), 2);
    assert!(result
        .paths
        .iter()
        .flat_map(|path| path.events.iter())
        .all(|event| event.principal_after <= event.principal_before));
}

#[test]
fn zero_beta_is_baseline_and_identity_is_bound() {
    let mut raw = request();
    for period in &mut raw.periods {
        period.behavioral_beta.fill(0.0);
        period.credit_beta.fill(0.0);
        period.prepayment_beta.fill(0.0);
    }
    let result = evaluate(&raw).unwrap();
    let expected = -(-0.1_f64).exp_m1() * 0.2;
    assert!((result.paths[0].events[0].credit_default_probability - expected).abs() < 1e-15);
    raw.market.shared_draw_id = "other-crn".into();
    assert!(evaluate(&raw).is_err());
}

#[test]
fn public_dispatch_and_cancellation_fail_closed() {
    let envelope = json!({"schema":"joint-market-credit-1","threads":1,"request":request()});
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["schema"], "joint-market-credit-1");

    let mut calls = 0;
    let error = evaluate_with_check(&request(), || {
        calls += 1;
        if calls > 2 {
            Err("deadline".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error, "deadline");
}

#[test]
fn market_duration_and_beta_domains_are_explicit() {
    let mut raw = request();
    raw.market.durations_years[0] = 0.5;
    assert!(evaluate(&raw).is_err());
    let mut raw = request();
    raw.periods[0].credit_beta[0] = 11.0;
    assert!(evaluate(&raw).is_err());
}
