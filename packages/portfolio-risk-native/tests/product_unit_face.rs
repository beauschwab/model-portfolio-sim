use portfolio_risk_native::product_units::{self, ProductUnitsRequest};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap()
}
fn reject(raw: Value) -> String {
    let request: ProductUnitsRequest = serde_json::from_value(raw).unwrap();
    product_units::prepare(&request).err().unwrap()
}
#[test]
fn reciprocal_face_and_quote_changes_reject_for_every_product_before_pricing() {
    for kind in ["consumer_installment", "card", "mortgage", "deposit"] {
        let r = raw();
        let g: Value = match kind {
            "consumer_installment" => serde_json::from_str(include_str!(
                "../../../examples/models/advanced-product-graph-1.json"
            )),
            "card" => serde_json::from_str(include_str!(
                "../../../examples/models/card-product-graph-1.json"
            )),
            "mortgage" => serde_json::from_str(include_str!(
                "../../../examples/models/mortgage-product-graph-1.json"
            )),
            _ => Ok(r["units"][0]["purchase_graph"]["request"].clone()),
        }
        .unwrap();
        let original = match kind {
            "consumer_installment" => g["positions"][0]["product"]["contract"]["opening_principal"]
                .as_f64()
                .unwrap(),
            "card" => g["positions"][0]["contract"]["opening_principal"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .sum(),
            "mortgage" => g["positions"][0]["contract"]["opening_investor_principal"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .sum(),
            _ => 1000.,
        };
        for scale in [0.5, 2.] {
            let mut bad = r.clone();
            bad["units"][0]["purchase_graph"] = json!({"kind":kind,"request":g});
            bad["units"][0]["unit_face"] = json!(original * scale);
            bad["units"][0]["quote_price_per_face_unit"] = json!(1. / scale);
            assert!(
                reject(bad).contains("unit_face must equal"),
                "{kind}/{scale}"
            );
        }
    }
}
#[test]
fn every_deposit_role_scenario_path_and_conditioned_branch_reconciles_opening_face() {
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        for branch in [false, true] {
            for later in [false, true] {
                let mut r = raw();
                let g = if branch {
                    &mut r["units"][0]["age_branches"][1]["graph"]["request"]
                } else {
                    &mut r["units"][0]["purchase_graph"]["request"]
                };
                let histories = g["positions"][0]["histories"].as_array_mut().unwrap();
                if later {
                    histories.push(histories[0].clone());
                    let paths = histories[1][role].as_array_mut().unwrap();
                    paths.push(paths[0].clone());
                    paths[1]["request"]["segments"][0]["opening_balance"] = json!(1001.);
                } else {
                    histories[0][role][0]["request"]["segments"][0]["opening_balance"] =
                        json!(1001.);
                }
                assert!(
                    reject(r).contains("unit_face must equal"),
                    "{role}/{branch}/{later}"
                );
            }
        }
    }
}
#[test]
fn strict_existing_amount_tolerance_accepts_roundoff_and_rejects_normalization() {
    let mut r = raw();
    let quote = r["units"][0]["quote_price_per_face_unit"].as_f64().unwrap() * 1000.;
    r["units"][0]["unit_face"] = json!(1000. + 1e-10);
    r["units"][0]["quote_price_per_face_unit"] = json!(quote / (1000. + 1e-10));
    let request = serde_json::from_value(r.clone()).unwrap();
    let prepared = product_units::prepare(&request).unwrap();
    assert!((prepared.values[prepared.shape[3]] - 1.).abs() < 2e-12);
    r["units"][0]["unit_face"] = json!(1000. + 1e-7);
    r["units"][0]["quote_price_per_face_unit"] = json!(quote / (1000. + 1e-7));
    assert!(reject(r).contains("unit_face must equal"));
}

#[test]
fn failed_selective_face_edit_preserves_retained_preparation() {
    let request: ProductUnitsRequest = serde_json::from_value(raw()).unwrap();
    let prepared = portfolio_risk_native::product_strategy::prepare(&request).unwrap();
    let original = serde_json::to_value(prepared.source()).unwrap();
    let mut changed = request.clone();
    changed.units[0].unit_face /= 2.;
    changed.units[0].quote_price_per_face_unit *= 2.;
    let error = portfolio_risk_native::product_strategy::prepare_update(&changed, &prepared)
        .err()
        .unwrap();
    assert!(error.contains("unit_face must equal"));
    assert_eq!(serde_json::to_value(prepared.source()).unwrap(), original);
    let (same, rebuilt, full) =
        portfolio_risk_native::product_strategy::prepare_update(&request, &prepared).unwrap();
    assert_eq!(rebuilt, 0);
    assert!(!full);
    assert_eq!(serde_json::to_value(same.source()).unwrap(), original);
}

#[test]
fn deposit_segment_components_are_nonnegative_and_bounded_before_summing() {
    for amount in [-1., 1e15 + 1.] {
        let mut r = raw();
        r["units"][0]["purchase_graph"]["request"]["positions"][0]["histories"][0]["physical"][0]
            ["request"]["segments"][0]["opening_balance"] = json!(amount);
        let error = reject(r);
        assert!(
            error.contains("nonnegative unit opening") || error.contains("bounded signed money")
        );
    }
}
