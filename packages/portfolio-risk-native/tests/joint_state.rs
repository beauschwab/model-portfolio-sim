use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};

fn request() -> Value {
    json!({
        "schema": "joint-state-1",
        "threads": 1,
        "request": {
            "schema": "joint-state-1",
            "identity": {
                "source_id": "fixture-joint",
                "source_revision": "v1",
                "assumption_id": "hazard-v1",
                "common_random_numbers_id": "crn-v1",
                "measure": "physical"
            },
            "as_of_ordinal": 1,
            "day_count": "act365_fixed",
            "periods": [{
                "start_ordinal": 1,
                "end_ordinal": 366,
                "market_discount_factor": 0.97,
                "behavioral_exit_hazard_per_year": 0.1,
                "credit_default_hazard_per_year": 0.2,
                "prepayment_hazard_per_year": 0.0,
                "recovery_fraction": 0.4,
                "recovery_lag_years": 1.0
            }],
            "paths": [{
                "position_id": "loan-1",
                "path_key": 11,
                "principal": 100.0,
                "random_pairs": [{"waiting_uniform": 0.2, "destination_uniform": 0.0}]
            }]
        }
    })
}

#[test]
fn public_dispatch_preserves_joint_state_result_and_identity() {
    let (bytes, ok) = respond(&serde_json::to_vec(&request()).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["result"]["schema"], "joint-state-1");
    assert_eq!(
        value["result"]["identity"]["common_random_numbers_id"],
        "crn-v1"
    );
    assert_eq!(value["result"]["paths"][0]["total_credit_default"], 100.0);
    assert_eq!(value["result"]["paths"][0]["total_recovery_cash"], 40.0);
    assert_eq!(
        value["result"]["paths"][0]["events"][0]["recovery_lag_years"],
        1.0
    );
}

#[test]
fn public_dispatch_rejects_wrong_schema_and_unknown_fields() {
    let mut wrong = request();
    wrong["schema"] = json!("credit-path-1");
    let (_, ok) = respond(&serde_json::to_vec(&wrong).unwrap());
    assert!(!ok);

    let mut unknown = request();
    unknown["request"]["unexpected"] = json!(true);
    let (_, ok) = respond(&serde_json::to_vec(&unknown).unwrap());
    assert!(!ok);
}
