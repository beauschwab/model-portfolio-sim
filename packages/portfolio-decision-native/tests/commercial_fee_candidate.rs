use portfolio_decision_native::commercial_candidate_journal::{self as candidate, Request};
use serde_json::{json, Value};
fn request(sign: f64) -> Request {
    let mut raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-candidate-journal-1.json"
    ))
    .unwrap();
    let original: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-fee-journal-1.json"
    ))
    .unwrap();
    let p = &mut raw["commercial"]["units"][0]["graph"]["positions"][0];
    let mut policy = original["fees"][0].clone();
    policy["fees"][0]["entitlement"] = json!(sign * 5.);
    for e in policy["events"].as_array_mut().unwrap() {
        e["amount"] = json!(sign * 5.);
    }
    let review = json!({"id":"separate-acquisition","revision":"1"});
    let make = |h: &Value, physical: bool| -> Value {
        let mut row = json!({"path_id":h["path_id"],"policy":policy});
        if physical {
            row["acquisition_releases"] = json!([
                {"fee_id":"service-fee","ordinal":738896,"sequence":0,"amount":sign*2.,"review":review},
                {"fee_id":"service-fee","ordinal":738926,"sequence":1,"amount":sign*3.,"review":review}
            ]);
        }
        row
    };
    let originals = p["original_pricing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| make(h, false))
        .collect::<Vec<_>>();
    let sets = p["histories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let mut set = json!({"scenario_id":h["scenario_id"]});
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                set[role] = json!(h[role]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| make(h, role == "physical"))
                    .collect::<Vec<_>>());
            }
            set
        })
        .collect::<Vec<_>>();
    p["fees"] = json!({"schema":"commercial-fee-adoption-1","identity":review,"discount_spread":0.,"original_signed_fee_quote":sign*5.,"total_purchase_cost":p["original_signed_cash_quote"].as_f64().unwrap()+sign*5.,"allocations":[{"fee_id":"service-fee","allocated_cost":sign*5.,"review":review}],"original_pricing":originals,"histories":sets});
    serde_json::from_value(raw).unwrap()
}
#[test]
fn finite_candidate_pays_acquired_fees_once_and_posts_separate_cost_release() {
    for sign in [1., -1.] {
        let r = request(sign);
        let out = candidate::evaluate(&r).unwrap();
        assert_eq!(
            out.producer_completion,
            json!({"complete":true,"protocol":7})
        );
        assert_eq!(out.full_end_ordinal, 738931);
        let source = out.preparation.native_preparation.source();
        assert_eq!(source.values[0], 0.);
        assert!((source.fee_income_values[0] - sign * 0.03).abs() < 1e-12);
        let fee = &out.tables["commercial_fee_lineage"];
        assert_eq!(
            fee.iter()
                .filter(|r| r["native_fee_group"]["event"]["action"] == "acquire")
                .count(),
            2
        );
        assert_eq!(
            fee.iter()
                .filter(|r| r["native_fee_group"]["event"]["action"] == "release_acquired_cost")
                .count(),
            4
        );
        for row in &out.tables["closing_statements"] {
            assert!((row["cash"].as_f64().unwrap() - 1000.).abs() < 1e-9);
        }
        let acquisition = out.tables["journal"]
            .iter()
            .filter(|r| r["scenario"] == "baseline" && r["day"] == 0 && r["gl_account"] == "cash")
            .map(|r| r["value"].as_f64().unwrap())
            .sum::<f64>();
        assert!(
            (acquisition
                - (1000.
                    - 0.5
                        * r.commercial.units[0].graph.positions[0]
                            .fees
                            .as_ref()
                            .unwrap()
                            .total_purchase_cost))
                .abs()
                < 1e-9
        );
        assert!(out.tables["commercial_fee_daily"]
            .iter()
            .any(|r| r["day"] == 15
                && r["native_fee_snapshot"][if sign > 0. {
                    "acquired_fee_asset"
                } else {
                    "acquired_fee_liability"
                }] == sign * 3.));
    }
}
#[test]
fn finite_candidate_fee_source_identity_survives_explicit_ledger_alias() {
    let mut r = request(1.);
    r.owners[0].position_id = "mapped-commercial".into();
    r.specification["positions"][0]["id"] = json!("mapped-commercial");
    let out = candidate::evaluate(&r).unwrap();
    assert!(out.tables["commercial_fee_lineage"]
        .iter()
        .all(|r| r["position_id"] == "mapped-commercial"));
}
#[test]
fn zero_allocation_retains_full_fee_source_and_emits_no_acquisition_money() {
    let mut r = request(1.);
    r.allocation[0].face = 0.;
    let out = candidate::evaluate(&r).unwrap();
    assert!(out.tables["commercial_fee_lineage"]
        .iter()
        .all(|r| r["transaction_number"].is_null()));
    assert!(
        !out.preparation.native_preparation.source().native_graphs[0].native_sources[0]
            .fee_projection
            .as_ref()
            .unwrap()
            .bound_path
            .source()
            .groups()
            .is_empty()
    );
    for row in &out.tables["closing_statements"] {
        assert_eq!(row["cash"], 1000.);
    }
}
