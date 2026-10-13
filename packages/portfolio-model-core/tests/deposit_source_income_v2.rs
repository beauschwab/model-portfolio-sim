use portfolio_model_core::{deposit_source_income as old, deposit_source_income_v2 as v2};
fn fixture(days: u32) -> v2::Request {
    serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/deposit-v2/long-{days}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn actual90day_full_original_current_tails_positive_eir_and_signed_rights() {
    let r = fixture(90);
    assert!(old::audit(&r).is_err());
    let result = v2::evaluate(&r, || Ok(())).unwrap();
    assert_eq!(result.native.current.income.rows.len(), 90);
    assert_eq!(result.native.original.source.closing_balance, 0.);
    assert_eq!(result.native.current.source.closing_balance, 0.);
    assert!(result.retained_used <= result.typed_plan.retained_reserved);
    assert!(result.output_bytes <= result.typed_plan.output_reserved);
    assert!(result.charged_work_used <= result.typed_plan.work_units);
    assert!(result.native.basis_revision < 0.);
    assert_eq!(
        result
            .native
            .raw_request
            .continuation
            .anchor
            .original_annual_effective_rate
            .to_bits(),
        0.02f64.to_bits()
    );
    for r in [
        &result.native.original.rights,
        &result.native.current.rights,
    ] {
        portfolio_model_core::deposit_continuation::complete_rights(r).unwrap();
    }
    assert!(!result.installed_adoption && !result.capital_ftp_eligible);
}
#[test]
fn actual128day_source_is_bounded_not_universal365claim() {
    let r = fixture(128);
    let a = v2::evaluate(&r, || Ok(())).unwrap();
    assert_eq!(a.native.current.income.rows.len(), 128);
}
#[test]
fn complete_future_current_and_unknown_historical_prefix_reject_before_projection() {
    let mut r = fixture(90);
    r.continuation.prefix.drivers.pop();
    assert!(v2::audit(&r, &mut || Ok(()))
        .unwrap_err()
        .contains("prefix"));
    r = fixture(90);
    r.continuation.current.drivers[2].reference_rate = 0.1;
    assert!(v2::audit(&r, &mut || Ok(())).is_err());
}
#[test]
fn cheap_invalid_horizon_cancels_or_refuses_before_large_validation() {
    let mut r = fixture(90);
    r.continuation.original.horizon_days = 366;
    let mut calls = 0;
    assert!(v2::audit(&r, &mut || {
        calls += 1;
        Ok(())
    })
    .unwrap_err()
    .contains("days"));
    assert_eq!(calls, 1);
    assert!(v2::evaluate(&fixture(90), || Err("cancelled".into()))
        .unwrap_err()
        .contains("cancelled"));
}
#[test]
fn old64schema_and_negative_yield_not_silently_extended() {
    let mut r = fixture(90);
    r.schema = old::SCHEMA.into();
    assert!(v2::audit(&r, &mut || Ok(())).is_err());
    r = fixture(90);
    r.continuation.anchor.original_annual_effective_rate = -0.01;
    assert!(v2::audit(&r, &mut || Ok(())).is_err());
}
#[test]
fn overreserved_direct_typed_input_capacity_is_refused() {
    let mut r = fixture(90);
    r.continuation.original.drivers.reserve(1_000_000);
    assert!(v2::audit(&r, &mut || Ok(()))
        .unwrap_err()
        .contains("capacity"));
}
