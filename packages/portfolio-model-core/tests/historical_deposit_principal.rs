pub use portfolio_model_core::{cashflow, limits};
#[path = "support/historical_deposit_state.rs"]
#[allow(dead_code)]
mod historical;
use portfolio_model_core::{deposit_source_owner as source_owner, deposit_state as source};
#[test]
fn every_original_current_source_none_capture_preserves_historical_bytes() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deposit-principal");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|x| x.unwrap().path())
        .collect();
    files.sort();
    for file in files {
        let bytes = std::fs::read(&file).unwrap();
        let old: historical::DepositStateRequest = serde_json::from_slice(&bytes).unwrap();
        let raw: source::DepositStateRequest = serde_json::from_slice(&bytes).unwrap();
        let expected = serde_json::to_vec(&historical::simulate(&old).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_vec(&source::simulate(&raw).unwrap()).unwrap(),
            expected,
            "None {:?}",
            file
        );
        let (captured, atoms) = source::capture_principal_with_check(&raw, &mut || Ok(())).unwrap();
        assert_eq!(
            serde_json::to_vec(&captured).unwrap(),
            expected,
            "capture {:?}",
            file
        );
        assert!(atoms.len() <= raw.horizon_days as usize + 2);
    }
}
#[test]
fn capture_cancellation_cannot_return_partial_source() {
    let b = include_bytes!("fixtures/deposit-principal/canonical-original.json");
    let r = serde_json::from_slice(b).unwrap();
    assert!(
        source::capture_principal_with_check(&r, &mut || Err("cancelled".into()))
            .unwrap_err()
            .contains("cancelled")
    );
}
#[test]
fn capture_refuses_ambiguous_incoming_principal() {
    let b = include_bytes!("fixtures/deposit-principal/canonical-original.json");
    let mut r: source::DepositStateRequest = serde_json::from_slice(b).unwrap();
    r.drivers
        .iter_mut()
        .find(|d| d.ordinal == r.as_of_ordinal)
        .unwrap()
        .external_inflows[0] = 1.;
    assert!(source::capture_principal_with_check(&r, &mut || Ok(()))
        .unwrap_err()
        .contains("no-inflow"));
}
#[test]
fn shared_source_types_are_not_a_proof_crate_dependency() {
    assert_eq!(source_owner::SCHEMA, "deposit-source-income-1");
    assert!(!include_str!("../../portfolio-ledger-native/Cargo.toml")
        .contains("deposit-source-principal-proof"));
}
#[test]
fn unsupported_shape_refuses_before_driver_validation() {
    let mut r: source::DepositStateRequest = serde_json::from_slice(include_bytes!(
        "fixtures/deposit-principal/canonical-original.json"
    ))
    .unwrap();
    r.horizon_days = 65;
    r.drivers[0].reference_rate = f64::NAN;
    let mut checks = 0;
    let error = source::capture_principal_with_check(&r, &mut || {
        checks += 1;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(checks, 1);
    assert!(error.contains("64"));
}
#[test]
fn capture_checks_during_validation_before_generation() {
    let r: source::DepositStateRequest = serde_json::from_slice(include_bytes!(
        "fixtures/deposit-principal/canonical-original.json"
    ))
    .unwrap();
    let mut checks = 0;
    let limit = r.drivers.len() + 4;
    let error = source::capture_principal_with_check(&r, &mut || {
        checks += 1;
        if checks == limit {
            Err("cancel driver validation".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(checks, limit);
    assert!(error.contains("cancel driver validation"));
}
