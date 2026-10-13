use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_calibration::{DataKind, ObservationTime};
use portfolio_model_core::credit_snapshot_calibration::*;
fn t(ordinal: i32) -> ObservationTime {
    ObservationTime {
        ordinal,
        seconds_of_day: 0,
    }
}
fn fixed() -> RowModel {
    RowModel::FixedRow {
        hazards_per_year: [0.0; 5],
        assumption_id: "unobserved-origin-policy".into(),
        rationale: "Explicit structural supplied row; no synthetic empirical inference".into(),
    }
}
fn p(initial: f64) -> Parameter {
    Parameter {
        initial,
        lower: -10.0,
        upper: 2.0,
    }
}
fn row(id: &str, start: i32, end: i32, destination: CreditState) -> SnapshotInterval {
    SnapshotInterval {
        snapshot_id: id.into(),
        account_id: id.into(),
        source_id: "synthetic-snapshot-tape".into(),
        start: t(start),
        end: t(end),
        origin: CreditState::Performing,
        destination,
        covariates: vec![],
        term_level: "reference".into(),
    }
}
fn request() -> SnapshotCalibrationRequest {
    let mut snapshots = vec![];
    for i in 0..4 {
        snapshots.push(row(
            &format!("train:{i}"),
            739500,
            739865,
            if i == 0 {
                CreditState::Default
            } else {
                CreditState::Performing
            },
        ));
    }
    snapshots.push(row("holdout", 739865, 740230, CreditState::Performing));
    SnapshotCalibrationRequest {
        schema: SCHEMA.into(),
        monetary_unit: "USD currency_units".into(),
        source_id: "synthetic-test-only".into(),
        source_revision: "raw-revision-7".into(),
        population_id: "accounts".into(),
        model_policy_id: "one-edge-panel".into(),
        training_split_id: "past".into(),
        holdout_split_id: "future".into(),
        data_kind: DataKind::Synthetic,
        train_start: t(739500),
        train_end: t(739865),
        holdout_end: t(740230),
        crossing_split_policy: ExclusionPolicy::Reject,
        censored_outcome_policy: ExclusionPolicy::Reject,
        covariate_names: vec![],
        term_levels: vec!["reference".into()],
        rows: [
            RowModel::LogLinear {
                edges: vec![LogLinearEdge {
                    destination: CreditState::Default,
                    intercept: p(-2.0),
                    covariate_coefficients: vec![],
                    term_coefficients: vec![],
                }],
            },
            fixed(),
            fixed(),
        ],
        optimizer: OptimizerPolicy {
            max_search_evaluations: 1800,
            initial_mesh: 0.5,
            final_mesh: 1e-6,
            objective_improvement_tolerance: 1e-12,
        },
        snapshots,
        default_outcomes: vec![],
    }
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() < tol, "{a} != {b}");
}

#[test]
fn panel_mle_matches_independent_binomial_survival_and_clock() {
    let r = request();
    let out = fit(&r).unwrap();
    let q = -0.75_f64.ln();
    close(out.parameters[0].value.exp(), q, 1e-6);
    close(
        out.training.log_likelihood.unwrap(),
        3.0 * 0.75_f64.ln() + 0.25_f64.ln(),
        1e-10,
    );
    close(out.holdout.log_likelihood.unwrap(), 0.75_f64.ln(), 2e-6);
    close(out.training.predicted_endpoint_counts[0][3], 1.0, 1e-5);
    assert_eq!(out.information_rank, 1);
    assert_eq!(out.convergence, "coordinate_mesh_converged");
    assert!(!out.adoption_eligible);
    assert_eq!(out.measure, CreditMeasure::Physical);
    assert_eq!(out.source_revision, "raw-revision-7");
    assert_eq!(
        out.snapshot_provenance[0].source_id,
        "synthetic-snapshot-tape"
    );
    assert_eq!(out.snapshot_provenance[4].partition, "holdout");
    let projected = out.project_hazards(&[], "reference").unwrap();
    close(projected[0][3], q, 1e-6);
    let mut malformed = out.clone();
    malformed.parameters.clear();
    assert!(malformed.project_hazards(&[], "reference").is_err());
}

#[test]
fn holdout_changes_never_change_fit_or_information_and_impossible_events_reported() {
    let mut r = request();
    let a = fit(&r).unwrap();
    r.snapshots[4].destination = CreditState::Prepaid;
    let b = fit(&r).unwrap();
    assert_eq!(
        serde_json::to_string(&a.parameters).unwrap(),
        serde_json::to_string(&b.parameters).unwrap()
    );
    assert_eq!(
        a.observed_information_eigenvalues,
        b.observed_information_eigenvalues
    );
    assert_eq!(b.holdout.impossible_intervals, 1);
    assert!(b.holdout.log_likelihood.is_none());
    assert_eq!(a.training.log_likelihood, b.training.log_likelihood);
}

#[test]
fn supplied_covariate_and_term_effects_fit_separate_training_groups() {
    let mut r = request();
    r.covariate_names = vec!["risk_driver".into()];
    r.term_levels.push("seasoned".into());
    if let RowModel::LogLinear { edges } = &mut r.rows[0] {
        edges[0].covariate_coefficients = vec![Parameter {
            initial: 0.0,
            lower: -3.0,
            upper: 3.0,
        }];
        edges[0].term_coefficients = vec![Parameter {
            initial: 0.0,
            lower: -3.0,
            upper: 3.0,
        }];
    }
    for x in &mut r.snapshots {
        x.covariates = vec![0.0];
    }
    for group in 1..=2 {
        for i in 0..4 {
            let mut x = row(
                &format!("group:{group}:{i}"),
                739500,
                739865,
                if i < 2 {
                    CreditState::Default
                } else {
                    CreditState::Performing
                },
            );
            x.covariates = vec![if group == 1 { 1.0 } else { 0.0 }];
            if group == 2 {
                x.term_level = "seasoned".into();
            }
            r.snapshots.push(x);
        }
    }
    let out = fit(&r).unwrap();
    assert_eq!(out.convergence, "coordinate_mesh_converged");
    let b0 = (-0.75_f64.ln()).ln();
    let effect = (-0.5_f64.ln()).ln() - b0;
    close(out.parameters[0].value, b0, 1e-4);
    close(out.parameters[1].value, effect, 1e-4);
    close(out.parameters[2].value, effect, 1e-4);
    assert_eq!(out.information_rank, 3);
}

#[test]
fn latent_migrations_and_cures_use_endpoint_matrix_not_no_jump_counts() {
    let mut r = request();
    r.snapshots = vec![
        row("train", 739500, 739865, CreditState::Performing),
        row("hold", 739865, 740230, CreditState::Performing),
    ];
    let mut q0 = [0.0; 5];
    q0[1] = 1.0;
    let mut q1 = [0.0; 5];
    q1[0] = 2.0;
    r.rows[0] = RowModel::FixedRow {
        hazards_per_year: q0,
        assumption_id: "P-watch".into(),
        rationale: "hand fixture".into(),
    };
    r.rows[1] = RowModel::FixedRow {
        hazards_per_year: q1,
        assumption_id: "cure".into(),
        rationale: "hand fixture".into(),
    };
    let out = fit(&r).unwrap();
    let pp = 2.0 / 3.0 + (-3.0_f64).exp() / 3.0;
    close(out.training.log_likelihood.unwrap(), pp.ln(), 1e-12);
    assert_ne!(out.training.log_likelihood.unwrap(), -1.0);
}

#[test]
fn crossing_split_never_invents_a_hidden_boundary_state() {
    let mut r = request();
    r.snapshots
        .push(row("cross", 739800, 739900, CreditState::Default));
    assert!(fit(&r).unwrap_err().contains("crosses"));
    r.crossing_split_policy = ExclusionPolicy::ExcludeWithDiagnostics;
    let out = fit(&r).unwrap();
    assert_eq!(out.excluded_cross_split_snapshot_ids, vec!["cross"]);
    assert_eq!(out.training.intervals, 4);
    assert_eq!(out.holdout.intervals, 1);
    assert_eq!(
        out.snapshot_provenance.last().unwrap().partition,
        "excluded_cross_split"
    );
}

#[test]
fn complete_outcomes_and_training_resolution_cutoff_prevent_severity_leakage() {
    let mut r = request();
    let train = DefaultOutcome {
        outcome_id: "resolved-train".into(),
        account_id: "a".into(),
        source_id: "synthetic-outcomes".into(),
        default_origin: CreditState::Performing,
        default_time: t(739600),
        pre_default_drawn: 100.0,
        pre_default_undrawn: 40.0,
        realized_ead: 120.0,
        recoveries: vec![
            ObservedRecovery {
                time: t(739650),
                principal: 30.0,
            },
            ObservedRecovery {
                time: t(739700),
                principal: 30.0,
            },
        ],
        resolution: OutcomeResolution::Complete {
            resolved_time: t(739700),
        },
    };
    let mut pending = train.clone();
    pending.outcome_id = "train-resolves-in-holdout".into();
    pending.account_id = "b".into();
    pending.resolution = OutcomeResolution::Complete {
        resolved_time: t(739900),
    };
    let mut hold = train.clone();
    hold.outcome_id = "hold".into();
    hold.account_id = "c".into();
    hold.default_time = t(739900);
    hold.pre_default_drawn = 80.0;
    hold.pre_default_undrawn = 0.0;
    hold.realized_ead = 80.0;
    hold.recoveries = vec![ObservedRecovery {
        time: t(740000),
        principal: 60.0,
    }];
    hold.resolution = OutcomeResolution::Complete {
        resolved_time: t(740000),
    };
    r.default_outcomes = vec![train, pending, hold];
    let out = fit(&r).unwrap();
    close(
        out.training_severity.ead_weighted_realized_lgd.unwrap(),
        0.5,
        1e-12,
    );
    close(
        out.training_severity.undrawn_weighted_realized_ccf.unwrap(),
        0.5,
        1e-12,
    );
    close(
        out.holdout_severity.ead_weighted_realized_lgd.unwrap(),
        0.25,
        1e-12,
    );
    assert!(out.holdout_severity.undrawn_weighted_realized_ccf.is_none());
    assert_eq!(out.excluded_outcome_ids, vec!["train-resolves-in-holdout"]);
    r.default_outcomes[0].resolution = OutcomeResolution::RightCensored {
        censor_time: t(739700),
    };
    assert!(fit(&r).unwrap_err().contains("censored"));
    r.censored_outcome_policy = ExclusionPolicy::ExcludeWithDiagnostics;
    let out = fit(&r).unwrap();
    assert!(out.training_severity.ead_weighted_realized_lgd.is_none());
    assert_eq!(
        out.outcome_provenance[0].partition,
        "excluded_right_censored"
    );
}

#[test]
fn unobserved_origin_requires_supplied_policy_and_singular_design_is_flagged() {
    let mut r = request();
    r.rows[1] = r.rows[0].clone();
    assert!(fit(&r).unwrap_err().contains("unobserved"));
    let mut r = request();
    r.covariate_names = vec!["all-zero".into()];
    for x in &mut r.snapshots {
        x.covariates = vec![0.0];
    }
    if let RowModel::LogLinear { edges } = &mut r.rows[0] {
        edges[0].covariate_coefficients = vec![p(0.0)];
    }
    let out = fit(&r).unwrap();
    assert_eq!(out.information_rank, 1);
    assert!(!out.adoption_eligible);
}

#[test]
fn nonconvergence_bounds_and_strict_domains_are_not_silently_adopted() {
    let mut r = request();
    r.optimizer.max_search_evaluations = 1;
    let out = fit(&r).unwrap();
    assert_eq!(out.convergence, "search_budget_exhausted");
    assert!(!out.adoption_eligible);
    let mut r = request();
    r.snapshots[0].destination = CreditState::Performing;
    let out = fit(&r).unwrap();
    assert!(!out.active_bound_parameters.is_empty());
    assert!(!out.adoption_eligible);
    let mut r = request();
    r.snapshots[0].start.seconds_of_day = 86400;
    assert!(fit(&r).is_err());
    let mut r = request();
    r.snapshots[0].covariates = vec![f64::NAN];
    assert!(fit(&r).is_err());
    let mut r = request();
    r.snapshots[0].snapshot_id = "bad\n".into();
    assert!(fit(&r).is_err());
    let mut r = request();
    r.rows[0] = RowModel::FixedRow {
        hazards_per_year: [0.0, 0.0, 0.0, 0.0, 0.0],
        assumption_id: "structural-zero".into(),
        rationale: "makes data impossible".into(),
    };
    assert!(fit(&r).unwrap_err().contains("impossible"));
}

#[test]
fn synthetic_example_remains_ineligible_and_missing_units_are_rejected() {
    let r: SnapshotCalibrationRequest = serde_json::from_str(include_str!(
        "../../../examples/models/credit-snapshot-calibration-1.json"
    ))
    .unwrap();
    let out = fit(&r).unwrap();
    assert!(!out.adoption_eligible);
    close(out.parameters[0].value.exp(), -0.75_f64.ln(), 1e-6);
    let mut value = serde_json::to_value(r).unwrap();
    value.as_object_mut().unwrap().remove("monetary_unit");
    assert!(serde_json::from_value::<SnapshotCalibrationRequest>(value).is_err());
}

#[test]
fn train_end_is_counted_once_and_fractional_days_use_elapsed_seconds() {
    let mut r = request();
    r.rows[0] = RowModel::FixedRow {
        hazards_per_year: [0.0, 0.0, 0.0, 1.0, 0.0],
        assumption_id: "hand".into(),
        rationale: "explicit one-day intensity fixture".into(),
    };
    r.train_end = t(739501);
    r.holdout_end = t(739502);
    let mut a = row("fractional", 739500, 739501, CreditState::Default);
    a.start.seconds_of_day = 43200;
    let mut b = row("hold", 739501, 739502, CreditState::Default);
    b.account_id = a.account_id.clone();
    b.origin = CreditState::Default;
    r.snapshots = vec![a, b];
    let out = fit(&r).unwrap();
    assert_eq!(out.training.intervals, 1);
    assert_eq!(out.holdout.intervals, 1);
    close(
        out.training.log_likelihood.unwrap(),
        (-(-0.5_f64 / 365.0).exp_m1()).ln(),
        1e-12,
    );
    close(out.holdout.log_likelihood.unwrap(), 0.0, 1e-12);
}

#[test]
fn parameter_search_is_separate_from_hazard_domain_holdout_diagnostics() {
    let mut r = request();
    r.covariate_names = vec!["driver".into()];
    for s in &mut r.snapshots {
        s.covariates = vec![0.0];
    }
    r.snapshots[4].covariates = vec![10.0];
    if let RowModel::LogLinear { edges } = &mut r.rows[0] {
        edges[0].covariate_coefficients = vec![Parameter {
            initial: 10.0,
            lower: -20.0,
            upper: 20.0,
        }];
    }
    let out = fit(&r).unwrap();
    assert_eq!(out.holdout.domain_rejected_intervals, 1);
    assert_eq!(out.holdout.intervals, 1);
    assert!(out.holdout.log_likelihood.is_none());
    assert_eq!(out.holdout.observed_start_counts[0], 1);
    assert!(!out.adoption_eligible);
    assert!(out.project_hazards(&[10.0], "reference").is_err());
}
