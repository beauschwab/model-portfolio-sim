use portfolio_model_core::credit::CreditState;
use portfolio_model_core::credit_calibration::*;

fn time(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: 1000 + day,
        seconds_of_day: 0,
    }
}

fn spell(
    id: &str,
    account: &str,
    start: i32,
    end: i32,
    origin: CreditState,
    destination: Option<CreditState>,
) -> ObservedSpell {
    ObservedSpell {
        spell_id: id.into(),
        account_id: account.into(),
        start: time(start),
        end: time(end),
        origin,
        ending: match destination {
            Some(destination) => SpellEnding::Transition { destination },
            None => SpellEnding::RightCensored {},
        },
    }
}

fn input() -> CreditCalibrationRequest {
    CreditCalibrationRequest {
        schema: SCHEMA.into(),
        source_id: "fixture-history".into(),
        source_revision: "immutable-revision-1".into(),
        population_id: "mortgage-exact-events".into(),
        model_policy_id: "status-policy-1".into(),
        training_split_id: "train-year-1".into(),
        holdout_split_id: "holdout-year-2".into(),
        observation_mode: ObservationMode::ExactTransitionTimes,
        data_kind: DataKind::Synthetic,
        train_start: time(0),
        train_end: time(365),
        holdout_end: time(730),
        unobserved_row_policy: [
            UnobservedRowPolicy::Reject {},
            UnobservedRowPolicy::Reject {},
            UnobservedRowPolicy::SuppliedRow {
                transition_hazards_per_year: [0.0; 5],
                assumption_id: "no-delinquent-fixture-evidence".into(),
                rationale: "Explicit fixture-only assumption".into(),
            },
        ],
        spells: vec![
            spell(
                "a1",
                "a",
                0,
                100,
                CreditState::Performing,
                Some(CreditState::Watch),
            ),
            spell(
                "a2",
                "a",
                100,
                365,
                CreditState::Watch,
                Some(CreditState::Performing),
            ),
            spell(
                "a3",
                "a",
                365,
                730,
                CreditState::Performing,
                Some(CreditState::Default),
            ),
            spell(
                "b1",
                "b",
                0,
                365,
                CreditState::Performing,
                Some(CreditState::Prepaid),
            ),
            spell("c1", "c", 0, 730, CreditState::Performing, None),
        ],
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-12 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

#[test]
fn exact_exposure_counts_cures_and_competing_exits_match_hand_mle() {
    let result = fit(&input()).unwrap();
    // Performing training exposure: account a 100d, account b 365d, account c 365d.
    close(result.training.exposure_years_by_origin[0], 830.0 / 365.0);
    close(result.training.exposure_years_by_origin[1], 265.0 / 365.0);
    close(result.transition_hazards_per_year[0][1], 365.0 / 830.0);
    close(result.transition_hazards_per_year[0][4], 365.0 / 830.0);
    close(result.transition_hazards_per_year[1][0], 365.0 / 265.0);
    assert_eq!(result.training.event_counts[0][1], 1);
    assert_eq!(result.training.event_counts[0][4], 1);
    assert_eq!(result.training.event_counts[1][0], 1);
    assert_eq!(result.training.event_counts[0][3], 0);
    close(
        result.training.log_likelihood.unwrap(),
        2.0 * (365.0_f64 / 830.0).ln() + (365.0_f64 / 265.0).ln() - 3.0,
    );
    assert_eq!(result.account_count, 3);
    assert_eq!(result.spell_count, 5);
    assert_eq!(result.data_kind, DataKind::Synthetic);
    assert_eq!(result.source_revision, "immutable-revision-1");
}

#[test]
fn boundary_events_count_once_and_holdout_impossible_events_are_explicit() {
    let result = fit(&input()).unwrap();
    assert_eq!(result.training.event_counts[1][0], 1); // Cure exactly at training end.
    assert_eq!(result.holdout.event_counts[1][0], 0);
    assert_eq!(result.training.event_counts[0][4], 1); // Prepayment exactly at training end.
    assert_eq!(result.holdout.event_counts[0][4], 0);
    assert_eq!(result.holdout.event_counts[0][3], 1);
    close(result.holdout.exposure_years_by_origin[0], 2.0);
    close(
        result.holdout.expected_event_counts[0][1],
        2.0 * 365.0 / 830.0,
    );
    assert_eq!(result.holdout.impossible_event_count, 1);
    assert_eq!(result.holdout.log_likelihood, None);
    assert_eq!(result.holdout.right_censored_spell_count, 1);
    assert_eq!(result.training.right_censored_spell_count, 0);
    assert!(serde_json::to_string(&result)
        .unwrap()
        .contains("\"log_likelihood\":null"));
}

#[test]
fn changing_only_holdout_transitions_never_changes_fit_or_training_diagnostics() {
    let original = fit(&input()).unwrap();
    let mut modified = input();
    modified.spells[2].ending = SpellEnding::Transition {
        destination: CreditState::Prepaid,
    };
    let revised = fit(&modified).unwrap();
    assert_eq!(
        original.transition_hazards_per_year,
        revised.transition_hazards_per_year
    );
    assert_eq!(
        serde_json::to_value(&original.training).unwrap(),
        serde_json::to_value(&revised.training).unwrap()
    );
    assert_eq!(revised.holdout.impossible_event_count, 0);
    close(
        revised.holdout.log_likelihood.unwrap(),
        (365.0_f64 / 830.0).ln() - 4.0 * 365.0 / 830.0,
    );
}

#[test]
fn supplied_rows_are_labeled_and_cannot_override_observed_estimates() {
    let mut request = input();
    request.unobserved_row_policy[0] = UnobservedRowPolicy::SuppliedRow {
        transition_hazards_per_year: [0.0, 0.0, 0.0, 3.0, 0.0],
        assumption_id: "unused".into(),
        rationale: "Only applies without exposure".into(),
    };
    request.unobserved_row_policy[2] = UnobservedRowPolicy::SuppliedRow {
        transition_hazards_per_year: [0.1, 0.2, 0.0, 0.3, 0.4],
        assumption_id: "explicit-tail".into(),
        rationale: "Unobserved delinquent row".into(),
    };
    let result = fit(&request).unwrap();
    assert_eq!(result.transition_hazards_per_year[0][3], 0.0);
    assert_eq!(
        result.transition_hazards_per_year[2],
        [0.1, 0.2, 0.0, 0.3, 0.4]
    );
    assert!(matches!(
        result.estimate_origins[0],
        RowEstimateOrigin::ObservedMle {}
    ));
    assert!(
        matches!(&result.estimate_origins[2], RowEstimateOrigin::SuppliedAssumption { assumption_id, .. } if assumption_id == "explicit-tail")
    );
    request.unobserved_row_policy[2] = UnobservedRowPolicy::Reject {};
    assert!(fit(&request).unwrap_err().contains("no training exposure"));
}

#[test]
fn observed_no_events_is_distinct_from_no_exposure() {
    let mut request = input();
    request.spells = vec![spell("only", "one", 0, 730, CreditState::Performing, None)];
    request.unobserved_row_policy[1] = UnobservedRowPolicy::SuppliedRow {
        transition_hazards_per_year: [0.0; 5],
        assumption_id: "watch-unused".into(),
        rationale: "No observed watch exposure".into(),
    };
    let result = fit(&request).unwrap();
    assert!(matches!(
        result.estimate_origins[0],
        RowEstimateOrigin::ObservedNoEvents {}
    ));
    assert_eq!(result.transition_hazards_per_year[0], [0.0; 5]);
    assert_eq!(result.training.log_likelihood, Some(0.0));
}

#[test]
fn exact_seconds_and_unequal_spells_are_not_rounded_to_reporting_days() {
    let mut request = input();
    request.spells[0].end.seconds_of_day = 1;
    request.spells[1].start.seconds_of_day = 1;
    let result = fit(&request).unwrap();
    let year_seconds = 365.0 * 86_400.0;
    close(
        result.transition_hazards_per_year[0][1],
        year_seconds / (830.0 * 86_400.0 + 1.0),
    );
    close(
        result.transition_hazards_per_year[1][0],
        year_seconds / (265.0 * 86_400.0 - 1.0),
    );
}

#[test]
fn input_order_does_not_change_evidence_fit_or_diagnostics() {
    let original = fit(&input()).unwrap();
    let mut reversed = input();
    reversed.spells.reverse();
    assert_eq!(
        serde_json::to_value(original).unwrap(),
        serde_json::to_value(fit(&reversed).unwrap()).unwrap()
    );
}

#[test]
fn invalid_windows_paths_sources_domains_and_unbounded_fits_fail() {
    let mutations: Vec<fn(&mut CreditCalibrationRequest)> = vec![
        |r| r.schema.clear(),
        |r| r.source_revision = "revision\n".into(),
        |r| r.training_split_id = r.holdout_split_id.clone(),
        |r| r.train_start = r.train_end,
        |r| r.holdout_end = r.train_end,
        |r| r.spells[0].start.ordinal = 0,
        |r| r.spells[0].start.seconds_of_day = 86_400,
        |r| r.spells[0].end = r.spells[0].start,
        |r| r.spells[0].spell_id = r.spells[1].spell_id.clone(),
        |r| r.spells[1].start.ordinal += 1,
        |r| r.spells[1].start.ordinal -= 1,
        |r| r.spells[1].origin = CreditState::Delinquent,
        |r| r.spells[0].origin = CreditState::Default,
        |r| r.spells[0].ending = SpellEnding::RightCensored {},
        |r| {
            r.spells[0].ending = SpellEnding::Transition {
                destination: CreditState::Default,
            }
        },
        |r| {
            r.spells[0].ending = SpellEnding::Transition {
                destination: CreditState::Performing,
            }
        },
        |r| r.spells.clear(),
        |r| r.spells = vec![r.spells[0].clone(); MAX_SPELLS + 1],
        |r| {
            r.unobserved_row_policy[2] = UnobservedRowPolicy::SuppliedRow {
                transition_hazards_per_year: [0.0, 0.0, 1.0, 0.0, 0.0],
                assumption_id: "diagonal".into(),
                rationale: "Invalid".into(),
            }
        },
    ];
    for (index, mutation) in mutations.iter().enumerate() {
        let mut request = input();
        mutation(&mut request);
        assert!(fit(&request).is_err(), "accepted invalid mutation {index}");
    }
    let mut request = input();
    request.spells = vec![
        spell(
            "fast",
            "fast",
            0,
            1,
            CreditState::Performing,
            Some(CreditState::Default),
        ),
        spell("future", "future", 365, 730, CreditState::Performing, None),
    ];
    request.unobserved_row_policy[1] = UnobservedRowPolicy::SuppliedRow {
        transition_hazards_per_year: [0.0; 5],
        assumption_id: "unused".into(),
        rationale: "Explicit absent state".into(),
    };
    assert!(fit(&request).unwrap_err().contains("100/year"));
}

#[test]
fn strict_serde_rejects_interval_censoring_weights_and_unknown_fields() {
    let mut json = serde_json::to_value(input()).unwrap();
    json["observation_mode"] = serde_json::json!("interval_censored");
    assert!(serde_json::from_value::<CreditCalibrationRequest>(json).is_err());
    let mut json = serde_json::to_value(input()).unwrap();
    json["spells"][0]["weight"] = serde_json::json!(2.0);
    assert!(serde_json::from_value::<CreditCalibrationRequest>(json).is_err());
    let mut json = serde_json::to_value(input()).unwrap();
    json["holdout_recalibration"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CreditCalibrationRequest>(json).is_err());
}
