use portfolio_decision_native::{
    product_mixed_consumer_credit as consumer, product_mixed_product_credit as mixed,
};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-mixed-product-credit-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> mixed::Request {
    serde_json::from_value(v).unwrap()
}

// These frozen fixtures precede explicit liquidity applicability. Their inputs
// supply no reviewed liquidity profile, so the current contract exposes null
// headroom and unconfigured status. Preserve every other original byte/value;
// keep the historical fixture files unchanged and compare complete output bytes.
fn baseline_with_unconfigured_liquidity(baseline: &[u8]) -> Vec<u8> {
    let mut expected: Value = serde_json::from_slice(baseline).unwrap();
    assert_eq!(
        serde_json::to_vec(&expected).unwrap().as_slice(),
        baseline.trim_ascii_end(),
        "frozen baseline must round-trip without changing any original bytes"
    );
    for table in ["daily_balance", "reported_daily_balance"] {
        for row in expected["ok"][table].as_array_mut().unwrap() {
            let row = row.as_object_mut().unwrap();
            assert!(row.contains_key("lcr_headroom") && row.contains_key("nsfr_headroom"));
            assert!(!row.contains_key("lcr_status") && !row.contains_key("nsfr_status"));
            row.insert("lcr_headroom".into(), Value::Null);
            row.insert("nsfr_headroom".into(), Value::Null);
            row.insert("lcr_status".into(), json!("unconfigured"));
            row.insert("nsfr_status".into(), json!("unconfigured"));
        }
    }
    serde_json::to_vec(&expected).unwrap()
}
#[test]
fn dated_candidate_installment_card_share_one_balanced_gl() {
    let raw = raw();
    let price = raw["performing"]["candidate"]["product"]["units"][0]["quote_price_per_face_unit"]
        .as_f64()
        .unwrap();
    let result = mixed::evaluate(&request(raw)).unwrap();
    // Original performing candidate and backbook, then independent principal-only
    // installment/card purchases102/100 with60 gross minus10 recovery cost each.
    let expected = 1000. - 100. * price + 104.45 + 101. - 102. + 50. - 100. + 50.;
    assert!(result.complete && result.mixed_ledger_adoption);
    assert_eq!(result.product_sources.len(), 4);
    for row in &result.closing_statements {
        assert!((row["cash"].as_f64().unwrap() - expected).abs() < 1e-9);
        assert!((row["equity"].as_f64().unwrap() - expected).abs() < 1e-9);
    }
    let mut sums = std::collections::BTreeMap::new();
    for j in &result.journal {
        *sums
            .entry((&j.scenario, j.transaction_number))
            .or_insert(0.) += j.value;
    }
    assert!(sums.values().all(|v| v.abs() < 1e-9));
    assert_eq!(result.product_acquisition_lineage.len(), 4);
    assert!(!result.interim_default_capital_adoption);
    for day in result
        .product_daily
        .iter()
        .filter(|r| r["position_id"] == "credit-card")
    {
        if day["ordinal"] != 738926 {
            assert!(day["product_receipts"]["statement_lineage"]
                .as_array()
                .unwrap()
                .is_empty());
        }
    }
}
#[test]
fn invalid_zero_participation_tail_and_incompatible_anchors_reject() {
    for case in 0..5 {
        let mut v = raw();
        match case {
            0 => v["combined_specification"]["accounts"][0]["cash"] = json!(1001.),
            1 => v["combined_specification"]["positions"][3]["risk_weight"] = json!(1.),
            2 => {
                v["product_owners"][1]["runs"][0]["paths"][0]["request"]["request"]["contract"]
                    ["initial_purchase_grace"] = json!(true)
            }
            3 => {
                v["product_owners"][1]["runs"][0]["paths"][0]["request"]["request"]["contract"]
                    ["identity"]["run_id"] = json!("unknown")
            }
            _ => {
                v["product_owners"][1]["allocated_face"] = json!(0.);
                for run in v["product_owners"][1]["runs"].as_array_mut().unwrap() {
                    run["paths"][0]["request"]["request"]["chargeoffs"] = json!([]);
                }
            }
        }
        assert!(mixed::evaluate(&request(v)).is_err(), "case{case}");
    }
}
#[test]
fn original_consumer_output_matches_frozen_economics_and_current_liquidity_contract() {
    let raw: consumer::Request = serde_json::from_str(include_str!(
        "../../../examples/models/product-mixed-consumer-credit-1.json"
    ))
    .unwrap();
    let result = consumer::evaluate(&raw).unwrap();
    let output = serde_json::to_vec(&json!({"ok":result})).unwrap();
    let baseline = include_bytes!("fixtures/product_mixed_consumer_credit_baseline.json");
    assert_eq!(output, baseline_with_unconfigured_liquidity(baseline));
}
#[test]
fn original_card1_output_matches_frozen_economics_and_current_liquidity_contract() {
    let result = mixed::evaluate(&request(raw())).unwrap();
    let output = serde_json::to_vec(&json!({"ok":result})).unwrap();
    let baseline = include_bytes!("fixtures/product_mixed_card1_baseline.json");
    assert_eq!(output, baseline_with_unconfigured_liquidity(baseline));
}
fn card2() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-mixed-card-credit-2.json"
    ))
    .unwrap()
}
#[test]
fn card2_full_rights_tail_reconciles_one_gl_and_basis_metrics() {
    let r = card2();
    let price = r["performing"]["candidate"]["product"]["units"][0]["quote_price_per_face_unit"]
        .as_f64()
        .unwrap();
    let result = mixed::evaluate(&request(r)).unwrap();
    let expected = 1000. - 100. * price + 104.45 + 101. - 100. + 101.;
    for row in &result.closing_statements {
        assert!((row["cash"].as_f64().unwrap() - expected).abs() < 1e-9);
        assert!((row["equity"].as_f64().unwrap() - expected).abs() < 1e-9);
    }
    assert_eq!(result.capital_ftp_eligible, Some(false));
    assert_eq!(result.closing_capital_eligible, Some(true));
    assert_eq!(result.product_rights_acquisition_lineage.len(), 2);
    assert!(!result.product_rights_lineage.is_empty());
    assert!(result.product_lineage.is_empty());
    let day = result
        .product_rights_daily
        .iter()
        .find(|r| r["ordinal"] == 738926)
        .unwrap();
    let snap = &day["unweighted_rights"];
    assert_eq!(snap["core"]["live"][0]["principal"], 0.);
    assert!(
        (snap["core"]["live"][0]["accrued_interest"]
            .as_f64()
            .unwrap()
            - 1.)
            .abs()
            < 1e-9
    );
    assert!(snap["live_interest_basis"].as_f64().unwrap() < 0.);
    let mut transactions = std::collections::BTreeMap::new();
    for row in &result.journal {
        *transactions
            .entry((&row.scenario, row.transaction_number))
            .or_insert(0.) += row.value;
    }
    assert!(transactions.values().all(|x| x.abs() < 1e-9));
}
#[test]
fn card2_zero_face_unresolved_tail_and_original_clock_reject_before_output() {
    for case in 0..4 {
        let mut raw = card2();
        match case {
            0 => {
                raw["product_owners"][0]["allocated_face"] = json!(0.);
                raw["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]
                    ["steps"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            1 => {
                raw["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["review"]
                    ["original_yield_anchor"]["revision"] = json!("changed")
            }
            2 => {
                raw["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]
                    ["horizon_days"] = json!(65)
            }
            _ => {
                raw["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]
                    ["reporting_ordinal"] = json!(738931)
            }
        }
        let mut output = Vec::new();
        assert!(
            mixed::run(&request(raw), &mut output).is_err(),
            "case{case}"
        );
        assert!(output.is_empty());
    }
}
#[test]
fn canceled_output_remains_empty() {
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let mut output = Vec::new();
    assert!(
        portfolio_compute_control::scope(Some(control), || mixed::run(
            &request(raw()),
            &mut output
        ))
        .is_err()
    );
    assert!(output.is_empty());
}
