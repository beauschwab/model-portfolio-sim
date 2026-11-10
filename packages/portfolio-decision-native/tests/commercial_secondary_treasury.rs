use portfolio_decision_native::commercial_fee_candidate_treasury::{evaluate, Request};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-conveyed-fee-treasury-1.json"
    ))
    .unwrap()
}
fn execute(raw: Value) -> Result<Value, String> {
    evaluate(serde_json::from_value::<Request>(raw).unwrap())
}

#[test]
fn conveyed_claim_and_discount_keep_gross_source_separate_from_buyer_income() {
    let r = execute(raw()).unwrap();
    assert_eq!(r["conveyed_fee_source_adoption"], true);
    assert_eq!(r["capital"]["conveyed_fee_asset_adjustment_grouped"], true);
    assert_eq!(
        r["source_candidate"]["producer_completion"],
        json!({"complete":true,"protocol":7})
    );
    for path in r["funding_paths"].as_array().unwrap() {
        assert_eq!(
            path["request"]["schema"],
            "modeled-product-ftp-source-channels-4"
        );
        let channels = path["request"]["states"][0]["channels"].as_array().unwrap();
        assert_eq!(channels.len(), 26);
        assert_eq!(channels[18]["amount"], 6.);
        assert_eq!(channels[24]["amount"], 1.);
        assert_eq!(path["opening_cash"], -104.);
        let days = path["preparation"]["days"].as_array().unwrap();
        let cost: f64 = days
            .iter()
            .flat_map(|d| d["earnings_by_channel"].as_array().unwrap())
            .filter(|a| a["channel"] == "fee_cost_release")
            .map(|a| a["amount"].as_f64().unwrap())
            .sum();
        assert!((cost - 1.).abs() < 1e-9);
        assert!(days
            .iter()
            .flat_map(|d| d["earnings_by_channel"].as_array().unwrap())
            .filter(|a| a["channel"] == "service_fee")
            .all(|a| a["amount"] == 0.));
    }
    assert_eq!(r["economic_loss_deducted_again"], false);
    assert_eq!(r["regulatory_credit_rwa_approval"], false);
}

#[test]
fn zero_participation_retains_full_unweighted_conveyed_funding_source() {
    let mut r = raw();
    r["raw_candidate"]["allocation"][0]["face"] = json!(0.);
    let r = execute(r).unwrap();
    for path in r["funding_paths"].as_array().unwrap() {
        assert_eq!(path["request"]["states"][0]["channels"][18]["amount"], 6.);
        assert_eq!(path["preparation"]["scale"], 0.);
        assert!(!path["request"]["cash"].as_array().unwrap().is_empty());
    }
}

#[test]
fn reviewed_conveyed_inventory_and_uniform_asset_weight_are_required() {
    let mut r = raw();
    r.as_object_mut().unwrap().remove("secondary_fee_review");
    assert!(execute(r).unwrap_err().contains("secondary_fee_review"));
    let mut r = raw();
    r["ftp"]["reviews"][0]["policy"]["channels"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(execute(r).unwrap_err().contains("channel"));
    let mut r = raw();
    r["capital"]["policies"][0]["gl_risk_weights"]["commercial_fee_receivable"] = json!(0.5);
    assert!(execute(r).unwrap_err().contains("weight"));
}
