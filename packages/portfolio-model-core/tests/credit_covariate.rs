use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::credit_calibration::{DataKind, ObservationMode, ObservationTime};
use portfolio_model_core::credit_covariate::*;
fn id(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "1".into(),
    }
}
fn t(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: 730120 + day,
        seconds_of_day: 0,
    }
}
fn p(value: f64, lower: f64, upper: f64) -> Parameter {
    Parameter {
        initial: value,
        lower,
        upper,
        ridge_weight: 0.0,
        ridge_center: 0.0,
    }
}
fn fixed() -> OriginModel {
    OriginModel::FixedRow {
        hazards_per_year: [0.0; 5],
        review: id("unobserved-origin-assumption"),
        rationale: "Explicit zero transition assumption for unavailable live origins".into(),
    }
}
fn request() -> CovariateFitRequest {
    let make = |name: &str, end: i32, destination: Option<CreditState>| Segment {
        segment_id: name.into(),
        account_id: name.into(),
        source: id("training-observations"),
        start: t(0),
        end: t(end),
        origin: CreditState::Performing,
        ending: destination.map_or(Ending::RightCensored {}, |destination| Ending::Transition {
            destination,
        }),
        available_time: t(0),
        covariates: vec![],
        term_level: "reference".into(),
    };
    CovariateFitRequest {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        data_kind: DataKind::Synthetic,
        observation_mode: ObservationMode::ExactTransitionTimes,
        population: id("synthetic-population"),
        model_policy: id("reviewed-fit-policy"),
        training_source: id("training-observations"),
        holdout_source: id("holdout-observations"),
        training_split: id("train"),
        holdout_split: id("holdout"),
        train_start: t(0),
        train_end: t(365),
        holdout_end: t(730),
        columns: vec![],
        term_levels: vec!["reference".into()],
        rows: [
            OriginModel::LogLinear {
                edges: vec![Edge {
                    destination: CreditState::Default,
                    intercept: p(-1.0, -5.0, 1.0),
                    covariate_coefficients: vec![],
                    term_coefficients: vec![],
                }],
            },
            fixed(),
            fixed(),
        ],
        optimizer: Optimizer {
            max_iterations: 100,
            max_backtracks: 32,
            timeout_ms: 60_000,
            projected_gradient_tolerance: 1e-9,
            relative_information_tolerance: 1e-8,
        },
        segments: vec![
            make("default1", 365, Some(CreditState::Default)),
            make("default2", 365, Some(CreditState::Default)),
            make("censor", 365, None),
        ],
    }
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() < tolerance, "{a} != {b}");
}
fn intercept(model: &FittedModel) -> f64 {
    match &model.rows[0] {
        ResolvedRow::LogLinear { edges } => edges[0].intercept.value,
        _ => panic!("fitted row required"),
    }
}
fn projection(model: FittedModel) -> CovariateProjectionRequest {
    CovariateProjectionRequest {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        model,
        model_revision: id("frozen-model"),
        review: id("analytical-review"),
        use_policy: ProjectionUse::AnalyticalOnly,
        position_id: "loan".into(),
        scenario_id: "base".into(),
        origin_time: t(730),
        end_time: t(732),
        timeout_ms: 60_000,
        intervals: vec![
            DriverInterval {
                interval_id: "day1".into(),
                start: t(730),
                end: t(731),
                available_time: t(730),
                kind: DriverKind::SuppliedProjected,
                source: id("scenario-driver"),
                covariates: vec![],
                term_level: "reference".into(),
            },
            DriverInterval {
                interval_id: "day2".into(),
                start: t(731),
                end: t(732),
                available_time: t(730),
                kind: DriverKind::SuppliedProjected,
                source: id("scenario-driver"),
                covariates: vec![],
                term_level: "reference".into(),
            },
        ],
    }
}

#[test]
fn exact_intercept_mle_is_events_over_independent_exposure_and_synthetic_ineligible() {
    let result = fit(&request()).unwrap();
    close(intercept(&result.model), (2.0_f64 / 3.0).ln(), 1e-8);
    close(result.training.exposure_years_by_origin[0], 3.0, 1e-12);
    assert_eq!(result.training.event_counts[0][3], 2);
    assert!(result.model.numerical_eligible);
    assert!(!result.model.adoption_eligible);
    assert_eq!(result.information_rank, 1);
    let mut q = projection(result.model);
    let out = project(&q).unwrap();
    assert!(out.analytical_only);
    assert!(!out.adoption_output);
    close(out.intervals[0].hazards_per_year[0][3], 2.0 / 3.0, 1e-8);
    close(out.intervals[1].end_year, 2.0 / 365.0, 1e-14);
    q.use_policy = ProjectionUse::Adoption;
    assert!(project(&q).unwrap_err().contains("ineligible"));
}
#[test]
fn chronological_train_end_event_counts_once_and_holdout_changes_preserve_byte_identity() {
    let mut q = request();
    q.segments.push(Segment {
        segment_id: "cross".into(),
        account_id: "cross".into(),
        source: id("training-observations"),
        start: t(100),
        end: t(500),
        origin: CreditState::Performing,
        ending: Ending::Transition {
            destination: CreditState::Default,
        },
        available_time: t(99),
        covariates: vec![],
        term_level: "reference".into(),
    });
    let a = fit(&q).unwrap();
    assert_eq!(a.training.event_counts[0][3], 2);
    assert_eq!(a.holdout.event_counts[0][3], 1);
    q.segments.last_mut().unwrap().end = t(600);
    q.segments.last_mut().unwrap().ending = Ending::RightCensored {};
    q.holdout_source.revision = "changed-holdout".into();
    q.holdout_split.revision = "2".into();
    q.holdout_end = t(900);
    let b = fit(&q).unwrap();
    assert_eq!(
        serde_json::to_string(&a.model).unwrap(),
        serde_json::to_string(&b.model).unwrap()
    );
    assert_eq!(b.training.event_counts[0][3], 2);
    assert_eq!(b.holdout.event_counts[0][3], 0);
    assert_ne!(a.holdout.log_likelihood, b.holdout.log_likelihood);
}
#[test]
fn two_feature_groups_fit_independent_hand_log_rate_ratio() {
    let mut q = request();
    q.columns = vec![Column {
        name: "unemployment".into(),
        role: FeatureRole::Macro,
        unit: "fraction".into(),
        offset: 0.0,
        scale: 1.0,
        raw_lower: 0.0,
        raw_upper: 1.0,
        scaling_review: id("frozen-supplied-transform"),
    }];
    if let OriginModel::LogLinear { edges } = &mut q.rows[0] {
        edges[0].covariate_coefficients = vec![p(0.0, -2.0, 2.0)];
        edges[0].intercept = p(-2.0, -5.0, 0.0);
    }
    q.segments.clear();
    for group in 0..2 {
        for account in 0..4 {
            q.segments.push(Segment {
                segment_id: format!("g{group}a{account}"),
                account_id: format!("g{group}a{account}"),
                source: id("training-observations"),
                start: t(0),
                end: t(365),
                origin: CreditState::Performing,
                ending: if account < 1 + group {
                    Ending::Transition {
                        destination: CreditState::Default,
                    }
                } else {
                    Ending::RightCensored {}
                },
                available_time: t(0),
                covariates: vec![group as f64],
                term_level: "reference".into(),
            });
        }
    }
    let result = fit(&q).unwrap();
    close(intercept(&result.model), 0.25_f64.ln(), 1e-8);
    let slope = match &result.model.rows[0] {
        ResolvedRow::LogLinear { edges } => edges[0].covariate_coefficients[0].value,
        _ => unreachable!(),
    };
    close(slope, 2.0_f64.ln(), 1e-8);
    assert_eq!(result.information_rank, 2);
    let mut projection = projection(result.model);
    projection.intervals[0].covariates = vec![0.0];
    projection.intervals[1].covariates = vec![1.0];
    let out = project(&projection).unwrap();
    close(out.intervals[0].hazards_per_year[0][3], 0.25, 1e-8);
    close(out.intervals[1].hazards_per_year[0][3], 0.5, 1e-8);
}
#[test]
fn rank_deficiency_is_not_hidden_by_ridge_or_bound_convergence() {
    let mut q = request();
    q.columns = vec![Column {
        name: "constant".into(),
        role: FeatureRole::Borrower,
        unit: "score".into(),
        offset: 0.0,
        scale: 1.0,
        raw_lower: 0.0,
        raw_upper: 1.0,
        scaling_review: id("transform"),
    }];
    for s in &mut q.segments {
        s.covariates = vec![0.0];
    }
    if let OriginModel::LogLinear { edges } = &mut q.rows[0] {
        let mut slope = p(0.0, -1.0, 1.0);
        slope.ridge_weight = 1.0;
        edges[0].covariate_coefficients = vec![slope];
    }
    let out = fit(&q).unwrap();
    assert_eq!(out.information_rank, 1);
    assert_eq!(out.free_parameters, 2);
    assert!(!out.model.numerical_eligible);
    assert!(out.eligibility_reasons.iter().any(|r| r.contains("rank")));
    let mut q = request();
    if let OriginModel::LogLinear { edges } = &mut q.rows[0] {
        edges[0].intercept = p(-2.0, -5.0, -1.0);
    }
    let out = fit(&q).unwrap();
    assert!(!out.active_bound_parameters.is_empty());
    assert!(!out.model.numerical_eligible);
}
#[test]
fn missing_origin_zero_events_unknown_features_unavailable_values_and_excess_hazards_reject() {
    let mut q = request();
    q.rows[1] = q.rows[0].clone();
    assert!(fit(&q).unwrap_err().contains("unobserved"));
    let mut q = request();
    for s in &mut q.segments {
        s.ending = Ending::RightCensored {};
    }
    assert!(fit(&q).unwrap_err().contains("zero-event"));
    let mut q = request();
    q.segments[0].available_time = t(1);
    assert!(fit(&q).is_err());
    let mut q = request();
    q.segments[0].term_level = "unknown".into();
    assert!(fit(&q).is_err());
    let mut q = request();
    if let OriginModel::LogLinear { edges } = &mut q.rows[0] {
        edges[0].intercept.upper = 10.0;
    }
    assert!(fit(&q).unwrap_err().contains("box"));
    let mut q = request();
    q.measure = CreditMeasure::Pricing;
    assert!(fit(&q).is_err());
}
#[test]
fn typed_training_order_and_projection_hash_are_stable_and_tamper_evident() {
    let a = fit(&request()).unwrap();
    let mut q = request();
    q.segments.reverse();
    let b = fit(&q).unwrap();
    assert_eq!(
        a.model.coefficient_identity_sha256,
        b.model.coefficient_identity_sha256
    );
    let mut p = projection(a.model);
    if let ResolvedRow::LogLinear { edges } = &mut p.model.rows[0] {
        edges[0].intercept.value += 0.01;
    }
    assert!(project(&p).unwrap_err().contains("hash"));
    let mut p = projection(b.model);
    p.intervals[1].start = t(730);
    assert!(project(&p).is_err());
    let raw = serde_json::to_value(request()).unwrap();
    let mut unknown = raw;
    unknown["auto_forecast"] = true.into();
    assert!(serde_json::from_value::<CovariateFitRequest>(unknown).is_err());
}
#[test]
fn observed_flag_allows_only_numerically_eligible_projection_and_discloses_assumed_rows() {
    let mut q = request();
    q.data_kind = DataKind::Observed;
    let out = fit(&q).unwrap();
    assert!(out.model.adoption_eligible);
    let mut p = projection(out.model);
    p.use_policy = ProjectionUse::Adoption;
    let out = project(&p).unwrap();
    assert!(out.adoption_output);
    assert!(!out.analytical_only);
}
#[test]
fn exhausted_iteration_policy_remains_diagnostic_not_adoption() {
    let mut q = request();
    q.optimizer.max_iterations = 1;
    q.data_kind = DataKind::Observed;
    let out = fit(&q).unwrap();
    assert!(!out.model.numerical_eligible);
    assert!(!out.model.adoption_eligible);
    let mut p = projection(out.model);
    p.use_policy = ProjectionUse::Adoption;
    assert!(project(&p).is_err());
}

#[test]
fn impossible_holdout_event_is_reported_without_changing_training_coefficients() {
    let mut q = request();
    q.segments.push(Segment {
        segment_id: "holdout".into(),
        account_id: "later".into(),
        source: id("holdout-observations"),
        start: t(400),
        end: t(500),
        origin: CreditState::Performing,
        ending: Ending::Transition {
            destination: CreditState::Default,
        },
        available_time: t(400),
        covariates: vec![],
        term_level: "reference".into(),
    });
    let a = fit(&q).unwrap();
    q.segments.last_mut().unwrap().ending = Ending::Transition {
        destination: CreditState::Prepaid,
    };
    let b = fit(&q).unwrap();
    assert_eq!(
        a.model.coefficient_identity_sha256,
        b.model.coefficient_identity_sha256
    );
    assert_eq!(b.holdout.impossible_events, 1);
    assert!(b.holdout.log_likelihood.is_none());
}

#[test]
fn exact_covariate_boundary_requires_observed_same_state_continuation() {
    let mut q = request();
    q.segments[0].ending = Ending::CovariateBoundary {};
    q.segments[0].end = t(100);
    let mut later = q.segments[0].clone();
    later.segment_id = "continued".into();
    later.start = t(100);
    later.end = t(365);
    later.ending = Ending::Transition {
        destination: CreditState::Default,
    };
    later.available_time = t(100);
    q.segments.push(later);
    let result = fit(&q).unwrap();
    close(result.training.exposure_years_by_origin[0], 3.0, 1e-12);
    assert_eq!(result.training.event_counts[0][3], 2);
    q.segments.last_mut().unwrap().start = t(101);
    assert!(fit(&q).unwrap_err().contains("continuous"));
}

#[test]
fn complete_fit_worst_case_work_and_malformed_projection_models_reject_before_adoption() {
    let mut q = request();
    let original = q.segments.clone();
    q.segments.clear();
    for i in 0..10000 {
        for (s, record) in original.iter().enumerate() {
            let mut record = record.clone();
            record.segment_id = format!("{i}:{s}");
            record.account_id = record.segment_id.clone();
            q.segments.push(record);
        }
    }
    assert!(fit(&q).unwrap_err().contains("worst-case"));
    let model = fit(&request()).unwrap().model;
    let mut p = projection(model);
    p.model.columns.resize(
        9,
        Column {
            name: "too-many".into(),
            role: FeatureRole::Macro,
            unit: "fraction".into(),
            offset: 0.0,
            scale: 1.0,
            raw_lower: 0.0,
            raw_upper: 1.0,
            scaling_review: id("review"),
        },
    );
    assert!(project(&p).is_err());
}

#[test]
fn synthetic_macro_example_projects_into_exact_term_paths_with_unchanged_shared_draws() {
    use portfolio_model_core::credit_path::{self as path, *};
    let q: CovariateFitRequest = serde_json::from_str(include_str!(
        "../../../examples/models/credit-covariate-1.json"
    ))
    .unwrap();
    let fitted = fit(&q).unwrap();
    assert!(!fitted.model.adoption_eligible);
    let mut drivers = projection(fitted.model);
    drivers.intervals[0].covariates = vec![0.0];
    drivers.intervals[1].covariates = vec![1.0];
    let projected = project(&drivers).unwrap();
    assert!(!projected.adoption_output);
    let mut scenario = CreditPathRequest {
        schema: "credit-path-1".into(),
        measure: CreditMeasure::Physical,
        monetary_unit: "currency_units".into(),
        source_id: "synthetic-scenario-projection".into(),
        source_revision: "1".into(),
        model_policy_id: "analytical-hand-CRN".into(),
        origin_time: t(730),
        max_jumps_per_path: 10,
        intervals: projected
            .intervals
            .iter()
            .map(|i| PathInterval {
                end_year: i.end_year,
                hazards_per_year: i.hazards_per_year,
                accrual_rates: [0.0; 3],
                loss_given_default: [0.4; 3],
                credit_conversion_factor: [0.0; 3],
                interest_recovery_fraction: [0.0; 3],
                complete_recovery_tails: std::array::from_fn(|_| {
                    vec![RecoveryWeight {
                        lag_years: 1.0,
                        weight: 1.0,
                    }]
                }),
            })
            .collect(),
        boundaries: vec![],
        draw_stream: DrawStream::Supplied {
            stream_id: "common-hand-stream".into(),
            stream_revision: "1".into(),
            paths: vec![SuppliedPathDraws {
                path_key: 17,
                pairs: vec![UniformPair {
                    waiting_uniform: (-0.001_f64).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        },
        paths: vec![PathOpening {
            path_id: "loan".into(),
            path_key: 17,
            initial_live_state: CreditState::Performing,
            principal: 100.0,
            undrawn_commitment: 0.0,
            accrued_interest: 0.0,
            book_adjustment: 0.0,
        }],
    };
    let stream = serde_json::to_string(&scenario.draw_stream).unwrap();
    let stress = path::evaluate(&scenario).unwrap();
    let default = stress.paths[0].default.as_ref().unwrap();
    close(
        default.year,
        1.0 / 365.0 + (0.001 - 0.25 / 365.0) / 0.5,
        1e-10,
    );
    close(default.principal, 100.0, 1e-12);
    assert_eq!(stress.paths[0].random_pairs_used, 1);
    scenario.intervals[1].hazards_per_year = scenario.intervals[0].hazards_per_year;
    let base = path::evaluate(&scenario).unwrap();
    close(
        base.paths[0].default.as_ref().unwrap().year,
        0.001 / 0.25,
        1e-10,
    );
    assert_eq!(base.paths[0].random_pairs_used, 1);
    assert_eq!(
        serde_json::to_string(&scenario.draw_stream).unwrap(),
        stream
    );
}
