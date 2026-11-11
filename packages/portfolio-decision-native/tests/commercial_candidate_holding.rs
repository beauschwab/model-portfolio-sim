use portfolio_compute_control::{scope, Control};
use portfolio_decision_native::commercial_candidate_holding::{evaluate, Request};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_slice(include_bytes!(
        "../../../examples/models/commercial-candidate-holding-journal-1.json"
    ))
    .unwrap()
}
fn run(raw: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    scope(Some(Control::new(30000)?), || {
        serde_json::to_value(evaluate(&request)?).map_err(|e| e.to_string())
    })
}
#[test]
fn complete_origin_cut_keeps_gross_fee_basis_and_only_actual_future_postings() {
    let result = run(raw()).unwrap();
    assert_eq!(result["component_output_limit_bytes"], 8 << 20);
    assert_eq!(
        result["revised_candidate"]["producer_completion"],
        json!({"complete":true,"protocol":7})
    );
    assert_eq!(result["full_end_ordinal"], 738931);
    let window = &result["window_tables"];
    let opening = window["opening"].as_array().unwrap();
    assert_eq!(
        opening
            .iter()
            .find(|r| r["scenario"] == "baseline"
                && r["gl_account"] == "commercial_acquired_fee_asset")
            .unwrap()["balance"],
        1.5
    );
    assert_eq!(
        opening
            .iter()
            .find(|r| r["scenario"] == "baseline" && r["gl_account"] == "commercial_fee_receivable")
            .unwrap()["balance"],
        2.5
    );
    let full = result["revised_candidate"]["tables"]["journal"]
        .as_array()
        .unwrap();
    let future = window["journal"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| !r["origin_transaction_number"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(
        future.len(),
        full.iter()
            .filter(|r| r["day"].as_u64().unwrap() > 15)
            .count()
    );
    assert!(future.iter().all(|r| r["day"].as_u64().unwrap() > 15));
    assert_eq!(result["prior_publication_verified"], false);
    assert_eq!(result["cut_started_mixed_sim_adoption"], false);
}
#[test]
fn zero_allocation_keeps_unweighted_fee_owner_and_prefix() {
    let mut r = raw();
    for name in ["completed_candidate", "revised_candidate"] {
        r[name]["allocation"][0]["face"] = json!(0.);
    }
    let result = run(r).unwrap();
    let rows = result["window_tables"]["commercial_fee_daily"]
        .as_array()
        .unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r["allocation_factor"] == 0.));
    assert!(rows
        .iter()
        .any(|r| r["native_fee_snapshot"]["acquired_cost"] == 5.));
}
#[test]
fn immutable_fee_review_loan_anchor_and_allocation_changes_reject() {
    for target in ["fee_review", "fee_spread", "loan_cost", "allocation"] {
        let mut r = raw();
        let revised = &mut r["revised_candidate"];
        match target {
            "fee_review" => {
                revised["commercial"]["units"][0]["graph"]["positions"][0]["fees"]["histories"][0]
                    ["physical"][0]["policy"]["events"][0]["review"]["revision"] =
                    json!("changed-past")
            }
            "fee_spread" => {
                revised["commercial"]["units"][0]["graph"]["positions"][0]["fees"]
                    ["discount_spread"] = json!(0.01)
            }
            "loan_cost" => {
                revised["commercial"]["units"][0]["graph"]["positions"][0]["histories"][0]
                    ["physical"][0]["source"]["source"]["acquisition"]["cost"] = json!(90.)
            }
            _ => revised["allocation"][0]["face"] = json!(25.),
        }
        assert!(run(r).is_err(), "{target}");
    }
}
