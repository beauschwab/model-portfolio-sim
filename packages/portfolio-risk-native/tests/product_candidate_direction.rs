use portfolio_risk_native::{
    product_candidate_source, product_strategy, product_units::ProductUnitsRequest,
};
use serde_json::{json, Value};

fn product(mixed: bool) -> ProductUnitsRequest {
    let mut raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap();
    let y = if mixed { 0. } else { -0.01f64 };
    let quote = if mixed {
        1000.
    } else {
        999. * (-y.ln_1p() * 2. / 365.).exp()
    };
    raw["units"][0]["quote_price_per_face_unit"] = json!(quote / 1000.);
    raw["units"][0]["income_policy"]["original_annual_effective_rate"] = json!(y);
    let u = &mut raw["units"][0];
    change(&mut u["purchase_graph"]["request"], y, mixed);
    for branch in u["age_branches"].as_array_mut().unwrap() {
        change(&mut branch["graph"]["request"], y, mixed);
    }
    serde_json::from_value(raw).unwrap()
}
fn change(g: &mut Value, y: f64, mixed: bool) {
    g["positions"][0]["oas_target"]["base_oas"] = json!(y.ln_1p() - 0.05);
    for history in g["positions"][0]["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for path in history[role].as_array_mut().unwrap() {
                let r = &mut path["request"];
                r["segments"][0]["opening_paid_rate"] = json!(-0.1825);
                r["segments"][0]["repricing"]["target"]["intercept"] = json!(-0.1825);
                r["segments"][0]["repricing"]["floor"] = json!(-0.2);
                if mixed {
                    r["drivers"][2]["offered_spread_stress"][0] = json!(0.365);
                }
            }
        }
    }
}
#[test]
fn complete_uniform_negative_source_retains_direction_with_zero_allocation() {
    let request = product(false);
    let prepared = product_strategy::prepare(&request).unwrap();
    let allocation = vec![product_strategy::Allocation {
        unit_id: request.units[0].unit_id.clone(),
        face: 0.,
    }];
    let capture = product_candidate_source::capture(
        &prepared,
        &allocation,
        request.reporting_ordinals[0],
        *request.reporting_ordinals.last().unwrap(),
    )
    .unwrap();
    for path in capture.paths {
        assert_eq!(path.interest_direction, -1.);
        assert_eq!(
            product_candidate_source::contractual_interest_sign(&path).unwrap(),
            -1.
        );
        assert_eq!(path.allocation_factor, 0.);
        assert_eq!(path.cash.iter().map(|c| c.interest).sum::<f64>(), -1.);
        assert_eq!(
            path.recognition.iter().map(|c| c.interest).sum::<f64>(),
            -1.
        );
        assert_eq!(
            path.daily_income_request.original_annual_effective_rate,
            -0.01
        );
    }
}
#[test]
fn signed_source_net_zero_is_not_uniform_direction_even_when_unallocated() {
    let request = product(true);
    // The analytical product and fixed-yield source remain valid. Gross
    // opposite-sign coupon obligations are rejected only at journal capture.
    let prepared = product_strategy::prepare(&request).unwrap();
    let allocation = vec![product_strategy::Allocation {
        unit_id: request.units[0].unit_id.clone(),
        face: 0.,
    }];
    let error = product_candidate_source::capture(
        &prepared,
        &allocation,
        request.reporting_ordinals[0],
        *request.reporting_ordinals.last().unwrap(),
    )
    .err()
    .unwrap();
    assert!(error.contains("mixed-sign contractual interest"), "{error}");
}
