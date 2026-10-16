use portfolio_decision_native::commercial_candidate_journal as candidate;
use serde_json::{json, Value};
fn raw(sign: f64, face: f64) -> Value {
    let mut r: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-secondary-prepaid-candidate-journal-1.json"
    ))
    .unwrap();
    r["allocation"][0]["face"] = json!(face);
    if sign < 0. {
        let f = &mut r["commercial"]["units"][0]["graph"]["positions"][0]["fees"];
        f["original_signed_fee_quote"] = json!(5.);
        f["total_purchase_cost"] = json!(104.);
        f["allocations"][0]["allocated_cost"] = json!(5.);
        fn flip(h: &mut Value) {
            h["policy"]["fees"][0]["entitlement"] = json!(-6.);
            h["policy"]["transfer"]["origin_service"]["fees"][0]["entitlement"] = json!(-6.);
            for e in h["policy"]["events"].as_array_mut().unwrap() {
                e["amount"] = json!(-6.);
            }
            for e in h["policy"]["transfer"]["origin_service"]["events"]
                .as_array_mut()
                .unwrap()
            {
                e["amount"] = json!(-6.);
            }
            for e in h
                .get_mut("acquisition_releases")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                e["amount"] = json!(-1.);
            }
        }
        for h in f["original_pricing"].as_array_mut().unwrap() {
            flip(h);
        }
        for hs in f["histories"].as_array_mut().unwrap() {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for h in hs[role].as_array_mut().unwrap() {
                    flip(h);
                }
            }
        }
    }
    r
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
#[test]
fn prepaid_secondary_actual_cost_gross_stock_contra_and_buyer_only_income() {
    for sign in [1., -1.] {
        for face in [0., 50., 100.] {
            let request: candidate::Request = serde_json::from_value(raw(sign, face)).unwrap();
            let out = candidate::evaluate(&request).unwrap();
            let stock = &out.tables["commercial_fee_daily"]
                .iter()
                .find(|r| r["day"] == 0)
                .unwrap()["native_fee_snapshot"];
            if sign > 0. {
                near(stock["deferred_fee_income"].as_f64().unwrap(), -6.);
                near(
                    stock["acquired_fee_liability_valuation_adjustment"]
                        .as_f64()
                        .unwrap(),
                    1.,
                );
                assert!(stock.get("acquired_fee_asset").is_none());
            } else {
                near(stock["prepaid_fee_expense"].as_f64().unwrap(), 6.);
                near(
                    stock["acquired_fee_asset_valuation_adjustment"]
                        .as_f64()
                        .unwrap(),
                    -1.,
                );
                assert!(stock.get("acquired_fee_liability").is_none());
            }
            let opening = out.tables["path"]
                .iter()
                .find(|r| r["scenario"] == "baseline" && r["day"] == 0)
                .unwrap();
            let liability = if sign > 0. { 5. * face / 100. } else { 0. };
            near(opening["assets"].as_f64().unwrap(), 1000. + liability);
            near(opening["liabilities"].as_f64().unwrap(), liability);
            near(opening["equity"].as_f64().unwrap(), 1000.);
            for close in &out.tables["closing_statements"] {
                near(
                    close["cash"].as_f64().unwrap(),
                    1000. + (4. - sign) * face / 100.,
                );
                near(
                    close["equity"].as_f64().unwrap(),
                    1000. + (4. - sign) * face / 100.,
                );
                near(close["liabilities"].as_f64().unwrap(), 0.);
            }
            assert!(!out.tables["journal"]
                .iter()
                .any(|r| r["gl_account"] == "pnl:commercial_nonintegral_fee_income"));
        }
    }
}
#[test]
fn zero_participation_still_rejects_unreviewed_opposite_cash() {
    let mut r = raw(1., 0.);
    r["commercial"]["units"][0]["graph"]["positions"][0]["fees"]["original_pricing"][0]["policy"]
        ["transfer"]
        .as_object_mut()
        .unwrap()
        .remove("purchase_cash_policy");
    let request: candidate::Request = serde_json::from_value(r).unwrap();
    assert!(candidate::evaluate(&request)
        .err()
        .unwrap()
        .contains("allocation sign"));
}
