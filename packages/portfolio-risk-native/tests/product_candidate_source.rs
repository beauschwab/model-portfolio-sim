use portfolio_risk_native::{
    product_candidate_source::{admitted_work, capture},
    product_strategy::{prepare, Allocation},
    product_units::ProductUnitsRequest,
};
fn raw() -> ProductUnitsRequest {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
#[test]
fn deposit_original_yield_daily_source_preserves_signed_money() {
    let r = raw();
    let p = prepare(&r).unwrap();
    let out = capture(
        &p,
        &[Allocation {
            unit_id: "deposit-1".into(),
            face: 250.,
        }],
        738916,
        738918,
    )
    .unwrap();
    let path = &out.paths[0];
    assert_eq!(path.side, -1.);
    assert_eq!(path.daily_income.rows.len(), 2);
    near(path.cash.iter().map(|c| c.principal).sum(), 1000.);
    near(path.cash.iter().map(|c| c.interest).sum(), 2.);
    let cost = 1002. * (-0.02f64.ln_1p() * 2. / 365.).exp();
    near(path.daily_income_request.opening_gross_carrying_value, cost);
    near(
        path.daily_income
            .rows
            .iter()
            .map(|r| r.effective_interest)
            .sum(),
        1002. - cost,
    );
    near(path.daily_income.rows.last().unwrap().closing_principal, 0.);
    assert_eq!(path.allocation_factor, 0.25);
}
#[test]
fn source_zero_allocation_and_wrong_grid_are_explicit() {
    let p = prepare(&raw()).unwrap();
    let out = capture(
        &p,
        &[Allocation {
            unit_id: "deposit-1".into(),
            face: 0.,
        }],
        738916,
        738918,
    )
    .unwrap();
    assert_eq!(out.paths.len(), 1);
    assert_eq!(out.paths[0].allocation_factor, 0.);
    assert!(admitted_work(&p, 738915, 738918).is_err());
    assert!(admitted_work(&p, 738916, 738917).is_err());
}
