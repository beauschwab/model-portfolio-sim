use portfolio_risk_native::models::lifetime_loss::{evaluate, Model, Request};
use serde_json::to_value;

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/lifetime-credit-loss-1.json"
    ))
    .unwrap()
}

#[test]
fn shared_draw_sensitivity_retains_complete_default_recovery_tails() {
    let out = evaluate(&request()).unwrap();
    assert_eq!(out.cells.len(), 9);
    let cov: Vec<_> = out
        .cells
        .iter()
        .filter(|c| c.model_kind == "supplied_covariate")
        .collect();
    assert!(cov[1].expected_total_loss < cov[0].expected_total_loss);
    assert!(cov[0].expected_total_loss < cov[2].expected_total_loss);
    assert!(out
        .cells
        .iter()
        .flat_map(|c| &c.full_paths)
        .filter_map(|p| p.default.as_ref())
        .flat_map(|d| &d.recoveries)
        .any(|r| r.year > 1.));
    assert!(!out.adoption_eligible && !out.allowance_or_accounting_approval);
}

#[test]
fn zero_coefficients_preserve_exact_constant_row_paths_and_zero_macro_bands() {
    let mut r = request();
    if let Model::SuppliedCovariate { beta, .. } = &mut r.cohorts[0].models[1] {
        for b in beta {
            b.fill(0.);
        }
    }
    let out = evaluate(&r).unwrap();
    assert_eq!(
        to_value(&out.cells[0].full_paths).unwrap(),
        to_value(&out.cells[3].full_paths).unwrap()
    );
    assert!(out
        .sensitivity
        .iter()
        .filter(|b| b.dimension == "macro")
        .all(|b| b.spread == 0.));
}

#[test]
fn zero_macro_dispersion_collapses_scenario_band_without_changing_model_band() {
    let mut r = request();
    for s in &mut r.scenarios {
        s.lag_aligned_values[0][0] = 0.;
    }
    let out = evaluate(&r).unwrap();
    assert!(out
        .sensitivity
        .iter()
        .filter(|b| b.dimension == "macro")
        .all(|b| b.spread == 0.));
    assert!(out
        .sensitivity
        .iter()
        .filter(|b| b.dimension == "model")
        .any(|b| b.spread > 0.));
}

#[test]
fn hand_default_loss_separates_signed_interest_and_basis_and_keeps_zero_scale_owners() {
    let mut r = request();
    let c = &mut r.cohorts[0];
    c.source.paths.truncate(1);
    c.probability_weights = vec![1.];
    c.source.boundaries.retain(|b| b.year == 1.);
    c.source.draw_stream = portfolio_model_core::credit_path::DrawStream::Supplied {
        stream_id: "hand".into(),
        stream_revision: "1".into(),
        paths: vec![portfolio_model_core::credit_path::SuppliedPathDraws {
            path_key: 0,
            pairs: vec![portfolio_model_core::credit_path::UniformPair {
                waiting_uniform: 0.99,
                destination_uniform: 0.1,
            }],
        }],
    };
    let out = evaluate(&r).unwrap();
    assert_eq!(out.cells[0].expected_principal_loss, 40.);
    assert_eq!(out.cells[0].expected_signed_interest_loss, 0.);
    let before = to_value(&out.cells[0].full_paths).unwrap();
    r.cohorts[0].monetary_multiplier = 0.;
    let zero = evaluate(&r).unwrap();
    assert_eq!(zero.cells[0].expected_total_loss, 0.);
    assert_eq!(to_value(&zero.cells[0].full_paths).unwrap(), before);
}

#[test]
fn missing_tail_invalid_hazards_weights_and_cross_measure_requests_refuse() {
    for defect in 0..5 {
        let mut r = request();
        match defect {
            0 => r.cohorts[0].source.boundaries.clear(),
            1 => r.cohorts[0].probability_weights[0] = 0.5,
            2 => r.cohorts[0].source.measure = portfolio_model_core::credit::CreditMeasure::Pricing,
            3 => r.drivers[0].scaling_training_end = r.training_end + 1,
            _ => {
                if let Model::SuppliedCovariate { beta, .. } = &mut r.cohorts[0].models[1] {
                    beta[3][0] = 100.;
                }
            }
        }
        assert!(evaluate(&r).is_err(), "defect {defect}");
    }
}

#[test]
fn outcome_holdout_never_changes_forecasts_and_requires_complete_recovery_followup() {
    let mut r = request();
    let before = evaluate(&r).unwrap();
    r.outcomes[0].reported_principal_loss = 20.;
    let after = evaluate(&r).unwrap();
    assert_eq!(
        to_value(&before.cells).unwrap(),
        to_value(&after.cells).unwrap()
    );
    assert_eq!(
        to_value(&before.sensitivity).unwrap(),
        to_value(&after.sensitivity).unwrap()
    );
    assert_ne!(
        to_value(&before.holdout_outcomes).unwrap(),
        to_value(&after.holdout_outcomes).unwrap()
    );
    r.outcomes[0].observed_through_year = 1.;
    assert!(evaluate(&r).is_err());
}

#[test]
fn product_totals_apply_each_cohort_multiplier_once() {
    let mut r = request();
    // Keep the cross-product inside the unchanged aggregate admission budget.
    r.cohorts[0].source.paths.truncate(16);
    r.cohorts[0].probability_weights = vec![1. / 16.; 16];
    let base = evaluate(&r).unwrap();
    let mut second = r.cohorts[0].clone();
    second.cohort_id = "second-cohort".into();
    second.monetary_multiplier = 2.;
    r.cohorts.push(second);
    let out = evaluate(&r).unwrap();
    assert_eq!(out.product_totals.len(), 9);
    for (total, original) in out.product_totals.iter().zip(&base.product_totals) {
        assert_eq!(total.expected_total_loss, 3. * original.expected_total_loss);
        assert_eq!(total.cohorts.len(), 2);
    }
}

#[test]
fn cancellation_and_expired_original_clock_refuse_before_projection() {
    let r = request();
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    assert!(
        portfolio_compute_control::scope(Some(control), || evaluate(&r))
            .unwrap_err()
            .contains("CANCELLED")
    );
    let start = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let expired = portfolio_compute_control::Control::from_started(1, start).unwrap();
    assert!(
        portfolio_compute_control::scope(Some(expired), || evaluate(&r))
            .unwrap_err()
            .contains("DEADLINE")
    );
}

#[test]
fn public_protocol_rejects_duplicate_nested_fields_and_expired_entry_clock() {
    use portfolio_risk_native::term_protocol::{respond, respond_started};
    let envelope = serde_json::json!({"schema":"lifetime-credit-loss-1","threads":1,"execution_timeout_ms":30000,"request":request()});
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let (result, ok) = respond(&bytes);
    assert!(ok, "{}", String::from_utf8_lossy(&result));
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replace(
        "\"monetary_multiplier\":1.0",
        "\"monetary_multiplier\":1.0,\"monetary_multiplier\":0.0",
    );
    let (result, ok) = respond(duplicate.as_bytes());
    assert!(!ok, "duplicate accepted");
    assert!(String::from_utf8_lossy(&result).contains("duplicate"));
    let past = std::time::Instant::now() - std::time::Duration::from_secs(31);
    let (result, ok) = respond_started(&bytes, past);
    assert!(!ok && String::from_utf8_lossy(&result).contains("DEADLINE"));
}
