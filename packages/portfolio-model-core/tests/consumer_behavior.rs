use portfolio_model_core::consumer_behavior::*;
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::{credit, credit_covariate as c, credit_snapshot_calibration as s};
use std::collections::BTreeMap;

fn identity(id: &str) -> Identity {
    Identity {
        id: id.into(),
        revision: "1".into(),
    }
}
fn live() -> Status {
    Status {
        days_past_due: 0,
        exit: None,
        balance: 100.,
        line_open: true,
        modified_or_reaged: false,
    }
}
fn status(state: credit::CreditState) -> Status {
    let mut x = live();
    match state {
        credit::CreditState::Performing => (),
        credit::CreditState::Watch => x.days_past_due = 30,
        credit::CreditState::Delinquent => x.days_past_due = 60,
        credit::CreditState::Default => x.exit = Some(Exit::ChargeOff),
        credit::CreditState::Prepaid => {
            x.exit = Some(Exit::VoluntaryPayoff);
            x.balance = 0.;
            x.line_open = false;
        }
    };
    x
}
fn exact(product: Product) -> Request {
    let mut q: c::CovariateFitRequest = serde_json::from_str(include_str!(
        "../../../examples/models/credit-covariate-1.json"
    ))
    .unwrap();
    q.columns[0].name = "unemp_level".into();
    let column = Column {
        definition: q.columns[0].clone(),
        availability_lag_days: 0,
        scaling_training_end: q.train_start,
        categories: BTreeMap::new(),
    };
    let records = q
        .segments
        .iter()
        .map(|row| {
            let (observation, end_state) = match row.ending {
                c::Ending::Transition { destination } => {
                    (Observation::ExactTransition, destination)
                }
                c::Ending::CovariateBoundary {} => (Observation::ExactBoundary, row.origin),
                c::Ending::RightCensored {} => (Observation::ExactCensor, row.origin),
            };
            Record {
                record_id: row.segment_id.clone(),
                account_id: row.account_id.clone(),
                source: row.source.clone(),
                start: row.start,
                end: row.end,
                start_status: status(row.origin),
                end_status: status(end_state),
                observation,
                loan_age_months: 0,
                features: BTreeMap::from([(
                    "unemp_level".into(),
                    Feature {
                        value: Value::Numeric(row.covariates[0]),
                        observed_at: row.available_time,
                        published_at: row.available_time,
                        source: identity("synthetic-macro"),
                    },
                )]),
            }
        })
        .collect();
    let specification=Specification{schema:SCHEMA.into(),identity:q.model_policy.clone(),product,watch_dpd:30,
        delinquent_dpd:60,default_dpd:if product==Product::CreditCard{180}else{120},
        permitted_default_causes:vec![Exit::ChargeOff,Exit::Bankruptcy],modified_policy:ModifiedPolicy::Reject,
        columns:vec![column],terms:vec![Term{name:"reference".into(),first_month:0}],
        term_convention:TermConvention::HoldStartLevelWithinInterval,
        minimum_training_support_years:[1.;3],omitted_driver_assumptions:vec!["Synthetic one-driver fixture; all other borrower and macro effects omitted by explicit assumption".into()]};
    q.segments.clear();
    q.columns.clear();
    q.term_levels.clear();
    Request {
        timeout_ms: 60000,
        specification,
        fit_template: FitTemplate::Exact {
            request: Box::new(q),
        },
        records,
    }
}
fn snapshot(product: Product) -> Request {
    let mut q: s::SnapshotCalibrationRequest = serde_json::from_str(include_str!(
        "../../../examples/models/credit-snapshot-calibration-1.json"
    ))
    .unwrap();
    let records = q
        .snapshots
        .iter()
        .map(|row| Record {
            record_id: row.snapshot_id.clone(),
            account_id: row.account_id.clone(),
            source: identity(&row.source_id),
            start: row.start,
            end: row.end,
            start_status: status(row.origin),
            end_status: status(row.destination),
            observation: Observation::Snapshot,
            loan_age_months: 0,
            features: BTreeMap::new(),
        })
        .collect();
    let mut specification = exact(product).specification;
    specification.identity = identity(&q.model_policy_id);
    specification.columns.clear();
    q.snapshots.clear();
    q.term_levels.clear();
    Request {
        timeout_ms: 60000,
        specification,
        fit_template: FitTemplate::Snapshot {
            request: Box::new(q),
        },
        records,
    }
}
#[test]
fn all_products_exact_panels_recover_hand_two_group_hazards() {
    for p in [Product::Auto, Product::Personal, Product::CreditCard] {
        let r = exact(p);
        let result = fit(&r).unwrap();
        let Fitted::Exact { result: fitted } = &result.fitted else {
            panic!()
        };
        let c::ResolvedRow::LogLinear { edges } = &fitted.model.rows[0] else {
            panic!()
        };
        let a = edges[0].intercept.value;
        let b = edges[0].covariate_coefficients[0].value;
        assert!((a.exp() - 0.25).abs() < 1e-9);
        assert!(((a + b).exp() - 0.5).abs() < 1e-9);
        assert!(!result.adoption_eligible && !fitted.model.adoption_eligible);
        assert_eq!(result.prepared.training_origin_support_years, [8., 0., 0.]);
        assert!(matches!(
            fitted.model.rows[1],
            c::ResolvedRow::FixedRow { .. }
        ));
    }
}
#[test]
fn snapshots_use_endpoint_likelihood_in_all_products() {
    for p in [Product::Auto, Product::Personal, Product::CreditCard] {
        let result = fit(&snapshot(p)).unwrap();
        let Fitted::Snapshot { result: fitted } = &result.fitted else {
            panic!()
        };
        let hazards = fitted.project_hazards(&[], "reference").unwrap();
        assert!((hazards[0][3] + 0.75_f64.ln()).abs() < 2e-6);
        assert!((hazards[0][3] - 0.25).abs() > 0.03);
        assert_eq!(fitted.training.intervals, 4);
        assert_eq!(fitted.holdout.intervals, 1);
    }
}
#[test]
fn holdout_values_and_events_do_not_fit_exact_coefficients() {
    let r = exact(Product::Auto);
    let base = fit(&r).unwrap();
    let mut altered = r.clone();
    let cut = match &r.fit_template {
        FitTemplate::Exact { request } => request.train_end,
        _ => panic!(),
    };
    for row in &mut altered.records {
        if row.start >= cut {
            row.end_status = live();
            row.observation = Observation::ExactCensor;
        }
    }
    let changed = fit(&altered).unwrap();
    let (Fitted::Exact { result: a }, Fitted::Exact { result: b }) = (base.fitted, changed.fitted)
    else {
        panic!()
    };
    assert_eq!(
        serde_json::to_value(a.model).unwrap(),
        serde_json::to_value(b.model).unwrap()
    );
}
#[test]
fn snapshot_holdout_does_not_fit_parameters() {
    let r = snapshot(Product::Personal);
    let a = fit(&r).unwrap();
    let mut changed = r.clone();
    changed.records.last_mut().unwrap().end_status = status(credit::CreditState::Default);
    let b = fit(&changed).unwrap();
    let (Fitted::Snapshot { result: a }, Fitted::Snapshot { result: b }) = (a.fitted, b.fitted)
    else {
        panic!()
    };
    assert_eq!(
        serde_json::to_value(a.parameters).unwrap(),
        serde_json::to_value(b.parameters).unwrap()
    );
}
#[test]
fn snapshots_cannot_be_relabelled_as_exact_events() {
    let mut r = exact(Product::Auto);
    r.records[0].observation = Observation::Snapshot;
    assert!(prepare(&r).is_err());
    let mut r = snapshot(Product::Auto);
    r.records[0].observation = Observation::ExactTransition;
    assert!(prepare(&r).is_err());
}
#[test]
fn dpd_and_card_zero_balance_rules_are_explicit() {
    for p in [Product::Auto, Product::Personal, Product::CreditCard] {
        let s = exact(p).specification;
        let mut row = live();
        for (dpd, expected) in [
            (0, credit::CreditState::Performing),
            (29, credit::CreditState::Performing),
            (30, credit::CreditState::Watch),
            (59, credit::CreditState::Watch),
            (60, credit::CreditState::Delinquent),
            (s.default_dpd, credit::CreditState::Default),
        ] {
            row.days_past_due = dpd;
            assert_eq!(state(&s, &row).unwrap(), expected);
        }
    }
    let s = exact(Product::CreditCard).specification;
    let mut row = live();
    row.balance = 0.;
    assert_eq!(state(&s, &row).unwrap(), credit::CreditState::Performing);
    row.exit = Some(Exit::ClosedZeroBalance);
    assert!(state(&s, &row).is_err());
    row.line_open = false;
    assert_eq!(state(&s, &row).unwrap(), credit::CreditState::Prepaid);
    row.balance = 1.;
    assert!(state(&s, &row).is_err());
}
#[test]
fn unknown_drivers_availability_domains_and_modified_rows_reject() {
    let base = exact(Product::Auto);
    let mut r = base.clone();
    r.specification.columns[0].definition.name = "unknown".into();
    assert!(prepare(&r).is_err());
    let mut r = base.clone();
    r.specification.columns[0].availability_lag_days = 1;
    assert!(prepare(&r).is_err());
    let mut r = base.clone();
    r.records[0]
        .features
        .get_mut("unemp_level")
        .unwrap()
        .published_at
        .ordinal += 1;
    assert!(prepare(&r).is_err());
    let mut r = base.clone();
    r.records[0].features.get_mut("unemp_level").unwrap().value = Value::Numeric(1e30);
    assert!(prepare(&r).is_err());
    let mut r = base.clone();
    r.records[0].start_status.modified_or_reaged = true;
    assert!(prepare(&r).is_err());
    let mut r = base.clone();
    r.specification.minimum_training_support_years[0] = 9.;
    assert!(prepare(&r).is_err());
    let mut raw = serde_json::to_value(base).unwrap();
    raw["specification"]["columns"][0]
        .as_object_mut()
        .unwrap()
        .remove("availability_lag_days");
    assert!(serde_json::from_value::<Request>(raw).is_err());
}
#[test]
fn native_record_ordering_preserves_exact_fit_identity() {
    let r = exact(Product::Auto);
    let a = fit(&r).unwrap();
    let mut reverse = r;
    reverse.records.reverse();
    let b = fit(&reverse).unwrap();
    let (Fitted::Exact { result: a }, Fitted::Exact { result: b }) = (a.fitted, b.fitted) else {
        panic!()
    };
    assert_eq!(
        a.model.coefficient_identity_sha256,
        b.model.coefficient_identity_sha256
    );
}

fn projection(fitted: &ResultData) -> Projection {
    let mut source: portfolio_model_core::credit_path::CreditPathRequest =
        serde_json::from_str(include_str!("../../../examples/models/credit-path-1.json")).unwrap();
    let end = match &fitted.prepared.fit_request {
        FitTemplate::Exact { request } => request.holdout_end,
        FitTemplate::Snapshot { request } => request.holdout_end,
    };
    source.origin_time = end;
    source.intervals[0].hazards_per_year = [[0.; 5]; 3];
    let mut through = end;
    through.ordinal += 365;
    Projection {
        model_revision: identity("fitted-panel"),
        review: identity("analytical-only"),
        scenario_id: "base".into(),
        timeout_ms: 10000,
        intervals: vec![c::DriverInterval {
            interval_id: "year".into(),
            start: end,
            end: through,
            available_time: end,
            kind: c::DriverKind::SuppliedProjected,
            source: identity("supplied-macro"),
            covariates: vec![0.; fitted.prepared.specification.columns.len()],
            term_level: "reference".into(),
        }],
        path_source: source,
    }
}
#[test]
fn complete_credit_paths_retain_shared_draws_and_supplied_monetary_laws() {
    for p in [Product::Auto, Product::Personal, Product::CreditCard] {
        let fitted = fit(&exact(p)).unwrap();
        let r = projection(&fitted);
        let projected = fitted.project_paths(&r).unwrap();
        assert_eq!(projected.product, p);
        assert!(!projected.adoption_eligible);
        let event = projected.paths.paths[0].default.as_ref().unwrap();
        assert!((event.year - 0.6).abs() < 1e-8);
        assert!((event.economic_principal_loss - 44.).abs() < 1e-12);
        assert_eq!(event.recoveries.len(), 1);
        assert!((event.recoveries[0].year - 2.6).abs() < 1e-8);
        assert_eq!(
            serde_json::to_value(&projected.projected_path_source.draw_stream).unwrap(),
            serde_json::to_value(&r.path_source.draw_stream).unwrap()
        );
        let mut restored = projected.projected_path_source;
        restored.intervals[0].hazards_per_year = [[0.; 5]; 3];
        assert_eq!(
            serde_json::to_value(restored).unwrap(),
            serde_json::to_value(&r.path_source).unwrap()
        );
    }
}
#[test]
fn snapshot_projection_uses_fitted_endpoint_hazard() {
    let fitted = fit(&snapshot(Product::Auto)).unwrap();
    let r = projection(&fitted);
    let projected = fitted.project_paths(&r).unwrap();
    let event = projected.paths.paths[0].default.as_ref().unwrap();
    assert!((event.year - 0.15 / (-0.75_f64.ln())).abs() < 1e-5);
}
#[test]
fn projection_refuses_pricing_unknown_values_future_sources_and_overwritten_hazards() {
    let fitted = fit(&exact(Product::Auto)).unwrap();
    let base = projection(&fitted);
    let mut r = base.clone();
    r.path_source.measure = credit::CreditMeasure::Pricing;
    assert!(fitted.project_paths(&r).is_err());
    let mut r = base.clone();
    r.intervals[0].available_time.ordinal += 1;
    assert!(fitted.project_paths(&r).is_err());
    let mut r = base.clone();
    r.intervals[0].covariates[0] = 100.;
    assert!(fitted.project_paths(&r).is_err());
    let mut r = base.clone();
    r.path_source.intervals[0].hazards_per_year[0][3] = 0.1;
    assert!(fitted.project_paths(&r).is_err());
    let mut r = base;
    r.intervals[0].end.ordinal -= 1;
    assert!(fitted.project_paths(&r).is_err());
}

#[test]
fn checks_cover_raw_mapping_fit_and_complete_path_replay() {
    let r = exact(Product::Auto);
    assert!(prepare_checked(&r, &mut || Err("stopped".into()))
        .unwrap_err()
        .contains("stopped"));
    let mut calls = 0;
    assert!(fit_checked(&r, &mut || {
        calls += 1;
        if calls == 6 {
            Err("stopped".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err()
    .contains("stopped"));
    let fitted = fit(&r).unwrap();
    let p = projection(&fitted);
    let mut count = 0;
    fitted
        .project_paths_checked(&p, &mut || {
            count += 1;
            Ok(())
        })
        .unwrap();
    let mut next = 0;
    assert!(fitted
        .project_paths_checked(&p, &mut || {
            next += 1;
            if next == count {
                Err("late stop".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err()
        .contains("late stop"));
    let mut invalid = r;
    invalid.timeout_ms = 0;
    assert!(fit_checked(&invalid, &mut || Ok(())).is_err());
}

#[test]
fn categorical_coding_and_term_reference_are_declared() {
    let mut r = exact(Product::Auto);
    r.specification.columns[0].categories =
        BTreeMap::from([("low".into(), 0.), ("high".into(), 1.)]);
    for row in &mut r.records {
        let feature = row.features.get_mut("unemp_level").unwrap();
        let Value::Numeric(x) = feature.value else {
            panic!()
        };
        feature.value = Value::Category(if x == 0. { "low" } else { "high" }.into());
    }
    let result = fit(&r).unwrap();
    let Fitted::Exact { result } = &result.fitted else {
        panic!()
    };
    let c::ResolvedRow::LogLinear { edges } = &result.model.rows[0] else {
        panic!()
    };
    assert!((edges[0].intercept.value.exp() - 0.25).abs() < 1e-9);
    r.records[0].features.get_mut("unemp_level").unwrap().value = Value::Category("missing".into());
    assert!(prepare(&r).is_err());
    let mut r = exact(Product::Auto);
    r.specification.terms[0].first_month = 1;
    assert!(prepare(&r).is_err());
}

#[test]
fn monthly_calendar_panel_retains_holdout_diagnostics_without_coefficient_leakage() {
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-snapshot-1.json"
    ))
    .unwrap();
    let r: Request = serde_json::from_value(raw["fit"].clone()).unwrap();
    assert_eq!(r.records.len(), 528);
    let first = fit(&r).unwrap();
    assert_eq!(
        first.prepared.training_support_definition,
        "observed_start_state_interval_duration"
    );
    let mut changed = r.clone();
    let cut = match &r.fit_template {
        FitTemplate::Snapshot { request } => request.train_end,
        _ => panic!(),
    };
    for row in &mut changed.records {
        if row.start >= cut && row.end_status.exit.is_some() {
            row.end_status = live();
        }
    }
    let second = fit(&changed).unwrap();
    let (Fitted::Snapshot { result: a }, Fitted::Snapshot { result: b }) =
        (first.fitted, second.fitted)
    else {
        panic!()
    };
    assert_eq!(a.training.intervals, 264);
    assert_eq!(a.holdout.intervals, 264);
    assert_eq!(
        serde_json::to_value(&a.parameters).unwrap(),
        serde_json::to_value(&b.parameters).unwrap()
    );
    assert_ne!(
        serde_json::to_value(&a.holdout).unwrap(),
        serde_json::to_value(&b.holdout).unwrap()
    );
    assert!(
        a.project_hazards(&[1.], "reference").unwrap()[0][3]
            > a.project_hazards(&[0.], "reference").unwrap()[0][3]
    );
}

#[test]
fn specification_identity_retains_policy_even_when_observed_mapping_is_equal() {
    let original = exact(Product::Auto);
    let before = fit(&original).unwrap();
    let mut changed = original;
    changed.specification.default_dpd = 121;
    let after = fit(&changed).unwrap();
    assert_ne!(
        before.prepared.specification_sha256,
        after.prepared.specification_sha256
    );
    let (Fitted::Exact { result: a }, Fitted::Exact { result: b }) = (before.fitted, after.fitted)
    else {
        panic!()
    };
    assert_eq!(
        a.model.coefficient_identity_sha256,
        b.model.coefficient_identity_sha256
    );
}
