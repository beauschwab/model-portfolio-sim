use portfolio_model_core::consumer_macro::*;
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::observation::{DataKind, ObservationTime};
use portfolio_model_core::{
    consumer_behavior as behavior, credit, credit_covariate as c, credit_path,
};
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "1".into(),
    }
}
fn t(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: day,
        seconds_of_day: 0,
    }
}
fn fixture() -> Request {
    let origin = 730850;
    let first = origin - 6 * 365;
    let mut levels = vec![5., 10.];
    let mut observations = Vec::new();
    for i in 0..6 {
        let end = if i % 2 == 0 {
            vec![4., 8.]
        } else {
            vec![5., 10.]
        };
        observations.push(Observation {
            row_id: format!("observed-{i}"),
            start: t(first + i * 365),
            end: t(first + (i + 1) * 365),
            start_values: levels,
            end_values: end.clone(),
            published_at: vec![t(first + (i + 1) * 365); 2],
        });
        levels = end;
    }
    let drivers = [
        ("rate", Metric::ShortRate, [-20., 30.]),
        ("unemployment", Metric::UnemploymentLevel, [0., 100.]),
    ]
    .into_iter()
    .map(|(name, metric, raw_domain)| Driver {
        driver_id: name.into(),
        metric,
        region_id: "US".into(),
        unit: Unit::Percent,
        raw_domain,
        availability_lag_days: 0,
        innovation_scale: 1.,
        scaling_source: id("frozen-scale"),
        scaling_training_end: t(first),
        source: id(name),
    })
    .collect();
    let scenarios = [
        ("baseline", ScenarioKind::Baseline, 0., 10.),
        ("up", ScenarioKind::ParallelUp200, 2., 11.),
        ("down", ScenarioKind::ParallelDown200, -2., 9.),
        ("adverse", ScenarioKind::SeverelyAdverse, -3., 14.),
    ]
    .into_iter()
    .map(|(name, kind, bump, u)| Scenario {
        scenario_id: name.into(),
        kind,
        source: id(name),
        conditional_rate_source: id(&format!("{name}-rate-parent")),
        issued_at: t(origin),
        paths: [17, 19]
            .into_iter()
            .enumerate()
            .map(|(i, path_key)| {
                let rate = 5. + i as f64 + bump;
                ConditionalPath {
                    path_key,
                    short_rates_percent: vec![rate; 2],
                    values: vec![vec![rate, u]; 2],
                }
            })
            .collect(),
    })
    .collect();
    Request {
        schema: SCHEMA.into(),
        measure: credit::CreditMeasure::Physical,
        as_of: t(origin),
        common_random_numbers: id("shared"),
        credit_draw_stream: credit_path::DrawStream::Supplied {
            stream_id: "shared".into(),
            stream_revision: "1".into(),
            paths: [17, 19]
                .into_iter()
                .map(|path_key| credit_path::SuppliedPathDraws {
                    path_key,
                    pairs: vec![credit_path::UniformPair {
                        waiting_uniform: (-0.2_f64 * 0.03).exp(),
                        destination_uniform: 0.4,
                    }],
                })
                .collect(),
        },
        policy: id("supplied-conditional-policy"),
        drivers,
        history: History {
            source: id("synthetic-history"),
            data_kind: DataKind::Synthetic,
            training_split: id("train"),
            holdout_split: id("holdout"),
            training_end: t(first + 4 * 365),
            holdout_end: t(origin),
            observations,
        },
        interval_ends: vec![t(origin + 30), t(origin + 60)],
        scenarios,
    }
}
fn run(r: &Request) -> ResultData {
    evaluate(r, &mut || Ok(())).unwrap()
}
fn mapped() -> (
    Request,
    behavior::Request,
    behavior::Projection,
    Vec<Mapping>,
) {
    let r = fixture();
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-spec-1.json"
    ))
    .unwrap();
    let mut fit: behavior::Request = serde_json::from_value(raw["fit"].clone()).unwrap();
    let mut p: behavior::Projection = serde_json::from_value(raw["projection"].clone()).unwrap();
    fit.specification.columns[0].definition.unit = "percent".into();
    fit.specification.columns[0].definition.raw_upper = 100.;
    let behavior::FitTemplate::Exact { request: q } = &mut fit.fit_template else {
        panic!()
    };
    q.rows = std::array::from_fn(|_| c::OriginModel::FixedRow {
        hazards_per_year: [0., 0., 0., 0.2, 0.],
        review: id("fixed-explicit"),
        rationale: "Synthetic zero-macro-coefficient control".into(),
    });
    let first = p.intervals[0].clone();
    p.intervals = vec![first.clone(), first];
    let src = p.path_source.intervals[0].clone();
    p.path_source.intervals = vec![src.clone(), src];
    p.path_source.boundaries.clear();
    for i in 0..2 {
        p.intervals[i].interval_id = format!("period-{i}");
        p.intervals[i].start = if i == 0 {
            r.as_of
        } else {
            r.interval_ends[i - 1]
        };
        p.intervals[i].end = r.interval_ends[i];
        p.intervals[i].available_time = r.as_of;
        p.path_source.intervals[i].end_year =
            (r.interval_ends[i].ordinal - r.as_of.ordinal) as f64 / 365.;
    }
    let credit_path::DrawStream::Supplied {
        stream_id,
        stream_revision,
        paths,
    } = &r.credit_draw_stream
    else {
        panic!()
    };
    p.path_source.draw_stream = credit_path::DrawStream::Supplied {
        stream_id: stream_id.clone(),
        stream_revision: stream_revision.clone(),
        paths: vec![paths[0].clone()],
    };
    (
        r,
        fit,
        p,
        vec![Mapping {
            column_name: "unemp_level".into(),
            driver_id: "unemployment".into(),
        }],
    )
}
#[test]
fn chronological_innovation_covariance_has_independent_hand_values() {
    let out = run(&fixture());
    let d = &out.diagnostics;
    assert_eq!(d.training.row_ids.len(), 4);
    assert_eq!(d.holdout.row_ids.len(), 2);
    assert_eq!(
        d.training.mean_annualized_standardized_innovation,
        vec![0., 0.]
    );
    for (i, j, want) in [
        (0, 0, 4. / 3.),
        (0, 1, 8. / 3.),
        (1, 0, 8. / 3.),
        (1, 1, 16. / 3.),
    ] {
        assert!((d.training.sample_annual_covariance[i][j] - want).abs() < 1e-12);
    }
    assert!(
        !out.adoption_eligible
            && !out.empirical_accuracy_validated
            && !out.pricing_measure_permission
    );
}
#[test]
fn holdout_changes_neither_training_moments_nor_fitted_identity() {
    let r = fixture();
    let a = run(&r);
    let mut changed = r.clone();
    changed.history.observations[4].end_values[1] += 0.5;
    changed.history.observations[5].start_values =
        changed.history.observations[4].end_values.clone();
    let b = run(&changed);
    assert_eq!(
        serde_json::to_value(a.diagnostics.training).unwrap(),
        serde_json::to_value(b.diagnostics.training).unwrap()
    );
    assert_eq!(a.training_identity_sha256, b.training_identity_sha256);
    assert_ne!(a.source_sha256, b.source_sha256);
    assert_ne!(
        a.diagnostics.holdout.sample_annual_covariance,
        b.diagnostics.holdout.sample_annual_covariance
    );
}
#[test]
fn unavailable_training_releases_are_excluded_and_never_backdated() {
    let mut r = fixture();
    r.history.observations[3].published_at[1].ordinal += 1;
    let out = run(&r);
    assert_eq!(out.diagnostics.training.row_ids.len(), 3);
    assert_eq!(
        out.diagnostics.excluded_unavailable_at_split,
        vec!["observed-3"]
    );
    r.history.observations[0].published_at[0].ordinal -= 1;
    assert!(evaluate(&r, &mut || Ok(())).is_err());
}
#[test]
fn unknown_missing_unit_lag_and_future_forecast_inputs_reject() {
    let r = fixture();
    let mut v = serde_json::to_value(&r).unwrap();
    v["drivers"][0]
        .as_object_mut()
        .unwrap()
        .remove("availability_lag_days");
    assert!(serde_json::from_value::<Request>(v).is_err());
    let mut v = serde_json::to_value(&r).unwrap();
    v["drivers"][0]["metric"] = serde_json::json!("invented");
    assert!(serde_json::from_value::<Request>(v).is_err());
    let mut x = r.clone();
    x.drivers[1].unit = Unit::PercentagePoints;
    assert!(evaluate(&x, &mut || Ok(())).is_err());
    let mut x = r.clone();
    x.scenarios[0].issued_at.ordinal += 1;
    assert!(evaluate(&x, &mut || Ok(())).is_err());
    let mut x = r.clone();
    x.measure = credit::CreditMeasure::Pricing;
    assert!(evaluate(&x, &mut || Ok(())).is_err());
    let mut x = r;
    x.scenarios[1].paths[0].short_rates_percent[0] += 0.01;
    assert!(evaluate(&x, &mut || Ok(())).is_err());
}
#[test]
fn all_products_share_supplied_scenarios_and_zero_effects_preserve_complete_financial_results() {
    let (r, fit, p, mappings) = mapped();
    let macro_result = run(&r);
    for product in [
        behavior::Product::Auto,
        behavior::Product::Personal,
        behavior::Product::CreditCard,
    ] {
        let mut fit = fit.clone();
        fit.specification.product = product;
        if product == behavior::Product::CreditCard {
            for rec in &mut fit.records {
                for status in [&mut rec.start_status, &mut rec.end_status] {
                    if status.exit == Some(behavior::Exit::VoluntaryPayoff) {
                        status.exit = Some(behavior::Exit::ClosedZeroBalance);
                        status.line_open = false;
                        status.balance = 0.;
                    }
                }
            }
        }
        let fitted = behavior::fit(&fit).unwrap();
        let mut financial = None;
        for scenario in &r.scenarios {
            let mut projected = p.clone();
            projected.scenario_id = scenario.scenario_id.clone();
            projected.intervals = macro_result
                .project_intervals(
                    &fit.specification,
                    &p,
                    &scenario.scenario_id,
                    17,
                    &mappings,
                    &mut || Ok(()),
                )
                .unwrap();
            assert_eq!(
                projected.intervals[0].covariates,
                vec![scenario.paths[0].values[0][1]]
            );
            let result = fitted.project_paths(&projected).unwrap();
            let values = serde_json::to_value(result.paths).unwrap();
            if let Some(previous) = &financial {
                assert_eq!(&values, previous);
            } else {
                financial = Some(values);
            }
        }
    }
}
#[test]
fn mapping_binds_units_availability_grid_and_actual_draw_bytes() {
    let (r, fit, p, mappings) = mapped();
    let out = run(&r);
    let mut bad = p.clone();
    let credit_path::DrawStream::Supplied { paths, .. } = &mut bad.path_source.draw_stream else {
        panic!()
    };
    paths[0].pairs[0].waiting_uniform = 0.2;
    assert!(out
        .project_intervals(
            &fit.specification,
            &bad,
            "baseline",
            17,
            &mappings,
            &mut || Ok(())
        )
        .unwrap_err()
        .contains("actual shared draws"));
    let mut bad = fit.specification.clone();
    bad.columns[0].definition.unit = "fraction".into();
    assert!(out
        .project_intervals(&bad, &p, "baseline", 17, &mappings, &mut || Ok(()))
        .is_err());
    let mut bad = p.clone();
    bad.intervals[0].available_time.ordinal += 1;
    assert!(out
        .project_intervals(
            &fit.specification,
            &bad,
            "baseline",
            17,
            &mappings,
            &mut || Ok(())
        )
        .is_err());
    let mut bad = p.clone();
    bad.path_source.intervals[0].end_year += 0.01;
    assert!(out
        .project_intervals(
            &fit.specification,
            &bad,
            "baseline",
            17,
            &mappings,
            &mut || Ok(())
        )
        .is_err());
    let mut bad = out.clone();
    bad.raw_source.scenarios[0].paths[0].values[0][1] += 0.1;
    assert!(bad
        .project_intervals(
            &fit.specification,
            &p,
            "baseline",
            17,
            &mappings,
            &mut || Ok(())
        )
        .unwrap_err()
        .contains("raw source changed"));
}
#[test]
fn cooperative_cancel_withholds_complete_result() {
    let r = fixture();
    let mut count = 0;
    let err = evaluate(&r, &mut || {
        count += 1;
        if count == 12 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(err, "cancelled");
    assert_eq!(count, 12);
}

fn daily_request() -> portfolio_model_core::consumer_macro_products::Request {
    use portfolio_model_core::{consumer_macro_products as joint, consumer_product_paths as daily};
    let (macro_source, fit, mut projection, mappings) = mapped();
    let source = &mut projection.path_source;
    source.monetary_unit = "USD".into();
    for p in &mut source.paths {
        p.principal = 0.;
        p.undrawn_commitment = 0.;
        p.accrued_interest = 0.;
        p.book_adjustment = 0.;
    }
    for i in &mut source.intervals {
        i.accrual_rates = [0.; 3];
        i.credit_conversion_factor = [0.; 3];
    }
    let mut template: daily::Request = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-product-paths-1.json"
    ))
    .unwrap();
    template.fit = fit;
    template.fit.specification.product = behavior::Product::Personal;
    template.projection = projection;
    let daily::ProductSource::Installment { contract, .. } = &mut template.products[0] else {
        panic!()
    };
    contract.horizon_days = 60;
    contract.steps.clear();
    contract.payment_dates = vec![contract.as_of_ordinal + 60];
    joint::Request {
        schema: joint::SCHEMA.into(),
        timeout_ms: 120000,
        macro_source,
        products: vec![joint::Product {
            product_id: "personal-17".into(),
            mappings,
            template,
        }],
    }
}

#[test]
fn daily_macro_cases_preserve_exact_zero_effect_cashflows_and_complete_default_tails() {
    use portfolio_model_core::{consumer_macro_products as joint, consumer_product_paths as daily};
    let r = daily_request();
    let base = daily::evaluate(&r.products[0].template).unwrap();
    let out = joint::evaluate(&r, &mut || Ok(())).unwrap();
    assert_eq!(out.cases.len(), 4);
    for case in &out.cases {
        assert_eq!(
            serde_json::to_value(&base.products).unwrap(),
            serde_json::to_value(&case.result.products).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&base.credit_states.paths).unwrap(),
            serde_json::to_value(&case.result.credit_states.paths).unwrap()
        );
        assert_eq!(
            case.result.raw_source.projection.scenario_id,
            case.scenario_id
        );
        let daily::ProductResult::Installment { result, .. } = &case.result.products[0] else {
            panic!()
        };
        assert!(result.default.is_some());
        assert!(!result.cashflows.is_empty());
    }
    assert!(!out.pricing_measure_permission && !out.adoption_eligible);
}

#[test]
fn daily_macro_rejects_bad_late_owner_before_generating_and_retains_original_cancel() {
    use portfolio_model_core::consumer_macro_products as joint;
    let mut r = daily_request();
    let mut bad = r.products[0].clone();
    bad.product_id = "bad".into();
    bad.mappings[0].driver_id = "unknown".into();
    r.products.push(bad);
    assert!(joint::evaluate(&r, &mut || Ok(()))
        .unwrap_err()
        .contains("unknown driver mapping"));
    let r = daily_request();
    let mut count = 0;
    let e = joint::evaluate(&r, &mut || {
        count += 1;
        if count == 300 {
            Err("original job cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(e, "original job cancelled");
    assert_eq!(count, 300);
    let mut r = r;
    r.products.resize(4, r.products[0].clone());
    assert!(joint::evaluate(&r, &mut || Ok(()))
        .unwrap_err()
        .contains("work admission"));
}

#[test]
fn fitted_macro_response_changes_daily_default_timing_on_identical_draws() {
    use portfolio_model_core::consumer_macro_products as joint;
    let mut r = daily_request();
    r.products[0].template.fit.specification.columns[0]
        .definition
        .raw_upper = 1.;
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-spec-1.json"
    ))
    .unwrap();
    r.products[0].template.fit.fit_template =
        serde_json::from_value(raw["fit"]["fit_template"].clone()).unwrap();
    for (i, s) in r.macro_source.scenarios.iter_mut().enumerate() {
        for p in &mut s.paths {
            for v in &mut p.values {
                v[1] = [0., 0.5, 0.25, 1.][i];
            }
        }
    }
    let out = joint::evaluate(&r, &mut || Ok(())).unwrap();
    let dates: Vec<_> = out
        .cases
        .iter()
        .map(|c| {
            let portfolio_model_core::consumer_product_paths::ProductResult::Installment {
                transitions,
                ..
            } = &c.result.products[0]
            else {
                panic!()
            };
            transitions[0].ordinal
        })
        .collect();
    assert_ne!(dates[0], dates[3]);
    for c in &out.cases {
        assert_eq!(
            serde_json::to_value(&c.result.raw_source.projection.path_source.draw_stream).unwrap(),
            serde_json::to_value(&r.products[0].template.projection.path_source.draw_stream)
                .unwrap()
        );
        assert_eq!(
            serde_json::to_value(&c.result.fitted).unwrap(),
            serde_json::to_value(&out.cases[0].result.fitted).unwrap()
        );
    }
}

#[test]
fn same_macro_tape_replays_auto_personal_card_through_their_daily_owners() {
    use portfolio_model_core::{
        consumer_card as card, consumer_installment as loan, consumer_macro_products as joint,
        consumer_product_paths as daily,
    };
    let mut r = daily_request();
    let mut auto = r.products[0].clone();
    auto.product_id = "auto-17".into();
    auto.template.fit.specification.product = behavior::Product::Auto;
    let daily::ProductSource::Installment { contract, .. } = &mut auto.template.products[0] else {
        panic!()
    };
    contract.product = loan::InstallmentProduct::Auto {
        new_vehicle: false,
        vintage_ordinal: r.macro_source.as_of.ordinal - 365,
        original_ltv: 1.,
        channel: "synthetic".into(),
        collateral_value_as_of: 600.,
        annual_depreciation: 0.,
        sale_haircut: 0.,
        repossession_cost: 0.,
        recovery_delay_days: 10,
    };
    let mut revolving = r.products[0].clone();
    revolving.product_id = "card-17".into();
    revolving.template.fit.specification.product = behavior::Product::CreditCard;
    for row in &mut revolving.template.fit.records {
        for status in [&mut row.start_status, &mut row.end_status] {
            if status.exit == Some(behavior::Exit::VoluntaryPayoff) {
                status.exit = Some(behavior::Exit::ClosedZeroBalance);
                status.line_open = false;
                status.balance = 0.;
            }
        }
    }
    let mut q: card::CardRequest = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-card-1.json"
    ))
    .unwrap();
    q.as_of_ordinal = r.macro_source.as_of.ordinal;
    q.horizon_days = 60;
    q.cycles.clear();
    q.rates.promotional_expiry_ordinal = q.as_of_ordinal + 30;
    revolving.template.products = vec![daily::ProductSource::Card {
        path_id: "example-loan".into(),
        path_key: 17,
        contract: Box::new(q),
    }];
    r.products.extend([auto, revolving]);
    let out = joint::evaluate(&r, &mut || Ok(())).unwrap();
    assert_eq!(out.cases.len(), 12);
    for product in &r.products {
        let base = daily::evaluate(&product.template).unwrap();
        for case in out
            .cases
            .iter()
            .filter(|c| c.product_id == product.product_id)
        {
            assert_eq!(
                serde_json::to_value(&case.result.products).unwrap(),
                serde_json::to_value(&base.products).unwrap()
            );
        }
    }
}

#[test]
fn documented_macro_examples_replay_without_transport_or_financial_fallback() {
    let r: Request = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-macro-scenarios-1.json"
    ))
    .unwrap();
    assert_eq!(run(&r).diagnostics.training.row_ids.len(), 4);
    let r: portfolio_model_core::consumer_macro_products::Request = serde_json::from_str(
        include_str!("../../../examples/models/consumer-macro-products-1.json"),
    )
    .unwrap();
    assert_eq!(
        portfolio_model_core::consumer_macro_products::evaluate(&r, &mut || Ok(()))
            .unwrap()
            .cases
            .len(),
        4
    );
}
