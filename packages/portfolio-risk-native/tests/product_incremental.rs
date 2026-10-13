use portfolio_risk_native::{
    product_strategy::{prepare, prepare_update},
    product_units::ProductUnitsRequest,
};
use serde_json::{json, Value};

fn raw() -> ProductUnitsRequest {
    let v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/strategy-product-1.json"
    ))
    .unwrap();
    serde_json::from_value(v["product"].clone()).unwrap()
}

#[test]
fn dirty_unit_reuses_exact_original_sources_and_matches_complete_rebuild() {
    let initial = raw();
    let previous = prepare(&initial).unwrap();
    let old = serde_json::to_value(previous.source()).unwrap();
    let mut candidate = serde_json::to_value(&initial).unwrap();
    candidate["units"][0]["weights"]["rwa"] = json!(0.25);
    let candidate: ProductUnitsRequest = serde_json::from_value(candidate).unwrap();
    let (updated, rebuilt, full) = prepare_update(&candidate, &previous).unwrap();
    assert_eq!(rebuilt, 1);
    assert!(!full);
    assert_eq!(
        serde_json::to_value(updated.source()).unwrap(),
        serde_json::to_value(prepare(&candidate).unwrap().source()).unwrap()
    );
    assert_eq!(serde_json::to_value(previous.source()).unwrap(), old);
    assert_eq!(
        serde_json::to_value(&updated.source().units[1]).unwrap(),
        old["units"][1]
    );
    assert_eq!(
        serde_json::to_value(&updated.source().funding).unwrap(),
        old["funding"]
    );
}

#[test]
fn shared_identity_change_forces_full_rebuild_and_reordering_preserves_cash_order() {
    let initial = raw();
    let previous = prepare(&initial).unwrap();
    let mut candidate = serde_json::to_value(&initial).unwrap();
    candidate["identity"]["grid_revision"] = json!("new-reviewed-grid");
    let candidate: ProductUnitsRequest = serde_json::from_value(candidate).unwrap();
    let (updated, rebuilt, full) = prepare_update(&candidate, &previous).unwrap();
    assert_eq!(rebuilt, 2);
    assert!(full);
    assert_eq!(
        serde_json::to_value(updated.source()).unwrap(),
        serde_json::to_value(prepare(&candidate).unwrap().source()).unwrap()
    );
    let mut reordered = initial;
    reordered.units.swap(0, 1);
    let (updated, rebuilt, full) = prepare_update(&reordered, &previous).unwrap();
    assert_eq!(rebuilt, 2);
    assert!(!full);
    assert_eq!(
        serde_json::to_value(updated.source()).unwrap(),
        serde_json::to_value(prepare(&reordered).unwrap().source()).unwrap()
    );
}

#[test]
fn invalid_or_cancelled_candidate_cannot_change_the_previous_source() {
    let mut initial = raw();
    let previous = prepare(&initial).unwrap();
    let old = serde_json::to_value(previous.source()).unwrap();
    initial.units[0].quote_price_per_face_unit *= 1.01;
    assert!(prepare_update(&initial, &previous).is_err());
    assert_eq!(serde_json::to_value(previous.source()).unwrap(), old);
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    assert!(
        portfolio_compute_control::scope(Some(control), || prepare_update(&raw(), &previous))
            .is_err()
    );
    assert_eq!(serde_json::to_value(previous.source()).unwrap(), old);
}
