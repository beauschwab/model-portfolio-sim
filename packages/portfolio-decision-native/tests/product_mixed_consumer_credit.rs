use portfolio_decision_native::product_mixed_consumer_credit as mixed;
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-mixed-consumer-credit-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> mixed::Request {
    serde_json::from_value(v).unwrap()
}
#[test]
fn full_actual_same_gl_default_source_and_once_weighting() {
    let raw = raw();
    let price = raw["performing"]["candidate"]["product"]["units"][0]["quote_price_per_face_unit"]
        .as_f64()
        .unwrap();
    let out = mixed::evaluate(&request(raw)).unwrap();
    // Independent original fixture: performing candidate104.45, backbook101;
    // consumer purchased102, gross60 less collectioncost10 =>50 recovered cash.
    let expected = 1000. - 100. * price + 104.45 + 101. - 102. + 50.;
    assert!(out.complete && out.mixed_ledger_adoption);
    assert!(!out.interim_default_capital_adoption && !out.independent_persisted_verification);
    for row in &out.closing_statements {
        assert!((row["cash"].as_f64().unwrap() - expected).abs() < 1e-9);
        assert!((row["equity"].as_f64().unwrap() - expected).abs() < 1e-9);
    }
    assert_eq!(out.consumer_sources.len(), 2);
    assert_eq!(out.consumer_acquisition_lineage.len(), 2);
    assert!(out
        .consumer_acquisition_lineage
        .iter()
        .all(|l| l["purchase_transaction_number"].is_u64()
            && l["allowance_transaction_number"].is_null()));
    assert_eq!(
        out.preparation.consumer_reservation.full_end_ordinal,
        738916 + 1080
    );
    assert_eq!(out.full_end_ordinal, 738976);
    let opening: Vec<_> = out
        .journal
        .iter()
        .filter(|j| j.scenario == "baseline" && j.event == "opening" && j.gl_account == "cash")
        .collect();
    assert_eq!(opening.len(), 1);
    assert_eq!(opening[0].value, 1000.);
}
#[test]
fn source_owner_initial_allowance_regulatory_and_tail_fail_closed() {
    for case in 0..7 {
        let mut r = raw();
        match case {
            0 => r["combined_specification"]["accounts"][0]["cash"] = json!(1001.),
            1 => r["combined_specification"]["positions"][2]["risk_weight"] = json!(1.),
            2 => {
                r["consumer_owners"][0]["runs"][0]["paths"][0]["request"]["opening_allowance"] =
                    json!(1.)
            }
            3 => r["consumer_owners"][0]["runs"][0]["paths"][0]["source_weight"] = json!(0.5),
            4 => {
                r["consumer_owners"][0]["runs"][0]["paths"][0]["request"]["contract"]["identity"]
                    ["run_id"] = json!("unknown")
            }
            5 => {
                r["consumer_owners"][0]["runs"][0]["paths"][0]["request"]["contract"]
                    ["horizon_days"] = json!(1081)
            }
            _ => {
                r["consumer_owners"][0]["allocated_face"] = json!(0.);
                for run in r["consumer_owners"][0]["runs"].as_array_mut().unwrap() {
                    run["paths"][0]["request"]["chargeoffs"] = json!([]);
                }
            }
        }
        assert!(mixed::evaluate(&request(r)).is_err(), "case{case}");
    }
}
#[test]
fn cancelled_or_broken_output_never_succeeds() {
    let raw = request(raw());
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let mut output = Vec::new();
    assert!(
        portfolio_compute_control::scope(Some(control), || mixed::run(&raw, &mut output)).is_err()
    );
    assert!(output.is_empty());
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("independent sink failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(mixed::run(&raw, Broken).is_err());
}
