use portfolio_decision_native::commercial_candidate_journal as candidate;
use serde_json::{json, Value};
fn raw(sign: f64, face: f64) -> Value {
    let mut r: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-secondary-candidate-journal-1.json"
    ))
    .unwrap();
    r["allocation"][0]["face"] = json!(face);
    if sign < 0. {
        let f = &mut r["commercial"]["units"][0]["graph"]["positions"][0]["fees"];
        f["original_signed_fee_quote"] = json!(-5.);
        f["total_purchase_cost"] = json!(94.);
        f["allocations"][0]["allocated_cost"] = json!(-5.);
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
                e["amount"] = json!(1.);
            }
        }
        for h in f["original_pricing"].as_array_mut().unwrap() {
            flip(h);
        }
        for h in f["histories"].as_array_mut().unwrap() {
            for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                for x in h[role].as_array_mut().unwrap() {
                    flip(x);
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
fn secondary_candidate_same_sim_retains_gross_asset_and_liability_valuation_lineage() {
    for sign in [1., -1.] {
        for face in [0., 50., 100.] {
            let r: candidate::Request = serde_json::from_value(raw(sign, face)).unwrap();
            let out = candidate::evaluate(&r).unwrap();
            assert_eq!(
                out.producer_completion,
                json!({"complete":true,"protocol":7})
            );
            let at0 = out.tables["commercial_fee_daily"]
                .iter()
                .find(|r| r["day"] == 0)
                .unwrap();
            let s = &at0["native_fee_snapshot"];
            if sign > 0. {
                near(s["fee_receivable"].as_f64().unwrap(), 6.);
                near(
                    s["acquired_fee_asset_valuation_adjustment"]
                        .as_f64()
                        .unwrap(),
                    -1.,
                );
                assert!(s.get("acquired_fee_liability").is_none());
            } else {
                near(s["fee_payable"].as_f64().unwrap(), -6.);
                near(
                    s["acquired_fee_liability_valuation_adjustment"]
                        .as_f64()
                        .unwrap(),
                    1.,
                );
                assert!(s.get("acquired_fee_asset").is_none());
            }
            // Gross statements retain a purchased payable and its contra on the
            // liability side; no sign-based reclassification into an asset.
            let first = out.tables["path"]
                .iter()
                .find(|r| r["scenario"] == "baseline" && r["day"] == 0)
                .unwrap();
            near(
                first["assets"].as_f64().unwrap(),
                1000. + if sign < 0. { 5. * face / 100. } else { 0. },
            );
            near(
                first["liabilities"].as_f64().unwrap(),
                if sign < 0. { 5. * face / 100. } else { 0. },
            );
            near(first["equity"].as_f64().unwrap(), 1000.);
            let cost = if sign > 0. { 104. } else { 94. };
            let cash: f64 = out.tables["journal"]
                .iter()
                .filter(|r| {
                    r["scenario"] == "baseline" && r["day"] == 0 && r["gl_account"] == "cash"
                })
                .map(|r| r["value"].as_f64().unwrap())
                .sum();
            near(cash, 1000. - cost * face / 100.);
            for close in &out.tables["closing_statements"] {
                near(
                    close["cash"].as_f64().unwrap(),
                    1000. + (4. + sign) * face / 100.,
                );
                near(
                    close["equity"].as_f64().unwrap(),
                    1000. + (4. + sign) * face / 100.,
                );
                near(close["liabilities"].as_f64().unwrap(), 0.);
            }
            assert!(!out.tables["journal"]
                .iter()
                .any(|r| r["gl_account"] == "pnl:commercial_nonintegral_fee_income"));
            let buyer = out.tables["commercial_daily"]
                .iter()
                .find(|r| r["day"] == 0)
                .unwrap();
            assert!(buyer["unweighted_book"]["lots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|l| l["earning_origin_index"] == 3));
        }
    }
}
