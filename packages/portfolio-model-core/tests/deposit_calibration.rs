use portfolio_model_core::credit_calibration::DataKind;
use portfolio_model_core::deposit_calibration::*;

fn time(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: 1000 + day,
        seconds_of_day: 0,
    }
}

fn spell(
    id: &str,
    account: &str,
    segment: &str,
    start: i32,
    end: i32,
    ending: SpellEnding,
) -> ObservedSpell {
    ObservedSpell {
        spell_id: id.into(),
        account_id: account.into(),
        segment_id: segment.into(),
        start: time(start),
        end: time(end),
        ending,
    }
}

fn request() -> Request {
    Request {
        schema: SCHEMA.into(),
        source_id: "deposit-history".into(),
        source_revision: "revision-1".into(),
        population_id: "nmd-cohorts".into(),
        model_policy_id: "retention-v1".into(),
        training_split_id: "train-1".into(),
        holdout_split_id: "holdout-2".into(),
        observation_mode: ObservationMode::ExactTransitionTimes,
        sampling_unit: SamplingUnit::AccountFullExit,
        data_kind: DataKind::Synthetic,
        train_start: time(0),
        train_end: time(365),
        holdout_end: time(730),
        segment_ids: vec!["retail".into(), "commercial".into()],
        unobserved_row_policy: vec![
            UnobservedRowPolicy::Reject {},
            UnobservedRowPolicy::Reject {},
        ],
        spells: vec![
            spell(
                "a1",
                "a",
                "retail",
                0,
                100,
                SpellEnding::Migration {
                    destination: "commercial".into(),
                },
            ),
            spell(
                "a2",
                "a",
                "commercial",
                100,
                730,
                SpellEnding::RightCensored {},
            ),
            spell("b1", "b", "retail", 0, 730, SpellEnding::RightCensored {}),
            spell("c1", "c", "retail", 0, 365, SpellEnding::Withdrawal {}),
            spell("d1", "d", "retail", 365, 500, SpellEnding::Closure {}),
        ],
    }
}

#[test]
fn exact_deposit_events_fit_competing_behavior_and_holdout() {
    let result = fit(&request()).unwrap();
    assert_eq!(result.schema, SCHEMA);
    assert_eq!(
        result.segment_hazards[0].estimate_origin,
        EstimateOrigin::ObservedMle {}
    );
    assert!((result.segment_hazards[0].migration_per_year[1] - 365.0 / 830.0).abs() < 1e-12);
    assert!((result.segment_hazards[0].withdrawal_per_year - 365.0 / 830.0).abs() < 1e-12);
    assert_eq!(result.training.migration_counts[0][1], 1);
    assert_eq!(result.training.withdrawal_counts[0], 1);
    assert_eq!(result.holdout.closure_counts[0], 1);
    assert!(result.segment_hazards[0]
        .standard_error_withdrawal
        .is_some());
    assert!(!result.adoption_eligible);
}

#[test]
fn holdout_only_event_changes_never_change_training_fit() {
    let original = fit(&request()).unwrap();
    let mut changed = request();
    changed.spells[4].ending = SpellEnding::Withdrawal {};
    let revised = fit(&changed).unwrap();
    assert_eq!(original.segment_hazards, revised.segment_hazards);
    assert_eq!(
        original.training.exposure_years_by_segment,
        revised.training.exposure_years_by_segment
    );
    assert_ne!(
        original.holdout.closure_counts,
        revised.holdout.closure_counts
    );
}

#[test]
fn invalid_migration_and_unknown_fields_fail_closed() {
    let mut invalid = request();
    invalid.spells[0].ending = SpellEnding::Migration {
        destination: "unknown".into(),
    };
    assert!(fit(&invalid).is_err());
    let raw = serde_json::json!({"schema": SCHEMA, "unexpected": true});
    assert!(serde_json::from_value::<Request>(raw).is_err());
}

#[test]
fn split_boundary_and_seconds_use_exact_exposure_not_reporting_months() {
    let mut raw = request();
    raw.spells[2].start.seconds_of_day = 3600;
    let result = fit(&raw).unwrap();
    let exposure = 830.0 / 365.0 - 3600.0 / (365.0 * 86400.0);
    assert!((result.training.exposure_years_by_segment[0] - exposure).abs() < 1e-14);
    assert!((result.segment_hazards[0].withdrawal_per_year - 1.0 / exposure).abs() < 1e-14);
    assert_eq!(result.training.withdrawal_counts[0], 1);
    assert_eq!(result.holdout.withdrawal_counts[0], 0);
    assert_eq!(
        result.segment_hazards[1].estimate_origin,
        EstimateOrigin::ObservedNoEvents {}
    );
    assert_eq!(result.segment_hazards[1].standard_error_withdrawal, None);
    assert_eq!(result.holdout.log_likelihood, None);
    assert_eq!(result.holdout.impossible_event_count, 1);
}

#[test]
fn unobserved_segment_requires_reviewed_row_and_retains_provenance() {
    let mut raw = request();
    raw.segment_ids.push("unseen".into());
    raw.unobserved_row_policy
        .push(UnobservedRowPolicy::Reject {});
    assert!(fit(&raw).unwrap_err().contains("no training exposure"));
    raw.unobserved_row_policy[2] = UnobservedRowPolicy::SuppliedRow {
        withdrawal_per_year: 0.1,
        closure_per_year: 0.2,
        migration_per_year: vec![0.3, 0.0, 0.0],
        assumption_id: "unseen-assumption".into(),
        rationale: "reviewed prior only".into(),
    };
    let result = fit(&raw).unwrap();
    assert_eq!(
        result.segment_hazards[2].estimate_origin,
        EstimateOrigin::SuppliedAssumption {
            assumption_id: "unseen-assumption".into(),
            rationale: "reviewed prior only".into(),
        }
    );
    assert_eq!(
        result.segment_hazards[2].standard_error_migration,
        vec![None; 3]
    );
    assert!(!result.adoption_eligible);
    if let UnobservedRowPolicy::SuppliedRow {
        migration_per_year, ..
    } = &mut raw.unobserved_row_policy[2]
    {
        migration_per_year[2] = 0.01;
    }
    assert!(fit(&raw).is_err());
}

#[test]
fn input_order_cannot_change_fit_and_cancellation_propagates() {
    let raw = request();
    let baseline = serde_json::to_value(fit(&raw).unwrap()).unwrap();
    let mut reordered = raw.clone();
    reordered.spells.reverse();
    assert_eq!(
        baseline,
        serde_json::to_value(fit(&reordered).unwrap()).unwrap()
    );
    let mut calls = 0;
    let error = fit_with_check(&raw, || {
        calls += 1;
        if calls == 8 {
            Err("cancelled during history replay".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error, "cancelled during history replay");
}

#[test]
fn truncated_migration_terminal_resume_and_invalid_clock_reject() {
    let mut raw = request();
    raw.spells.remove(1);
    assert!(fit(&raw).unwrap_err().contains("destination continuation"));
    let mut raw = request();
    raw.spells[0].ending = SpellEnding::Withdrawal {};
    assert!(fit(&raw).unwrap_err().contains("terminal spell"));
    let mut raw = request();
    raw.spells[0].start.seconds_of_day = 86400;
    assert!(fit(&raw).unwrap_err().contains("seconds_of_day"));
    let mut value = serde_json::to_value(request()).unwrap();
    value["sampling_unit"] = serde_json::json!("partial_balance_withdrawal");
    assert!(serde_json::from_value::<Request>(value.clone()).is_err());
    value.as_object_mut().unwrap().remove("sampling_unit");
    assert!(serde_json::from_value::<Request>(value).is_err());
}
