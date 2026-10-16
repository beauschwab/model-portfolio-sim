//! Structural draw proof: every financial coordinate and basis value survives.
use super::super::*;
use super::*;

fn law(floating: bool) -> Request {
    let graph: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-generated-graph-1.json"
    ))
    .unwrap();
    let mut r: Request = serde_json::from_value(
        graph["positions"][0]["generated_policy"]["pricing_training"].clone(),
    )
    .unwrap();
    for c in [&mut r.source.source.original, &mut r.source.source.current] {
        let mut p = c.periods[0].clone();
        p.end_ordinal = c.asof_ordinal + 30;
        p.payment_ordinal = c.asof_ordinal + 35;
        c.periods = vec![p];
        c.principal_plan = term::indexed::PrincipalPlan::Supplied {
            principal_payments: vec![100.],
        };
    }
    r.coupon_loadings.clear();
    r.hazard_loadings.clear();
    r.source.credit.opening_undrawn_commitment = 20.;
    r.source.credit.max_jumps_per_path = 1;
    r.source.credit.intervals.truncate(1);
    r.source.credit.intervals[0].end_year = 35. / 365.;
    r.source.accounting.recognition.truncate(1);
    r.source.accounting_rates.truncate(1);
    let mut source = r.source.source.clone();
    let at = source.current.asof_ordinal + 1;
    for c in [&mut source.original, &mut source.current] {
        c.contract.id = "generated-draw-contract".into();
        c.asof_ordinal = at;
        c.reporting_end_ordinal = at + 4;
        c.tail_end_ordinal = at + 4;
        c.opening_principal = 20.;
        let mut p = c.periods[0].clone();
        p.period_id = "generated-draw-coupon".into();
        p.start_ordinal = at;
        p.end_ordinal = at + 4;
        p.payment_ordinal = at + 4;
        p.coupon = Coupon::Fixed { annual_rate: 0. };
        c.periods = vec![p];
        c.principal_plan = term::indexed::PrincipalPlan::Supplied {
            principal_payments: vec![20.],
        };
    }
    source.acquisition.identity.id = "generated-draw-acquisition".into();
    source.acquisition.cost = 20.;
    source.acquisition.original_annual_effective_rate = 0.;
    if floating {
        let mut original: Request = serde_json::from_slice(include_bytes!(
            "../../../../../../examples/models/commercial-generated-exercise-1.json"
        ))
        .unwrap();
        let mut coupon = original.source.source.current.periods.remove(1).coupon;
        let clock = utc(at);
        if let Coupon::Indexed {
            observations,
            reset_cutoff_unix_seconds,
            ..
        } = &mut coupon
        {
            *reset_cutoff_unix_seconds = Some(clock);
            let o = &mut observations[0];
            o.interest_start_ordinal = at;
            o.interest_end_ordinal = at + 4;
            o.observation_start_ordinal = at;
            o.observation_end_ordinal = at + 4;
            o.fixing_ordinal = at;
            o.selection_cutoff_unix_seconds = clock;
            o.rate = 0.02;
            if let RateSource::ConditionalProjection {
                path,
                information_unix_seconds,
                measure,
                ..
            } = &mut o.source
            {
                *path = source.current.path.clone();
                *information_unix_seconds = clock;
                *measure = source.current.measure;
            } else {
                panic!("conditional source fixture required")
            }
        } else {
            panic!("indexed fixture required")
        }
        for c in [&mut source.original, &mut source.current] {
            c.periods[0].coupon = coupon.clone();
        }
        source.acquisition.cost = 20. + 20. * 0.02 * 4. / 365.;
        r.time_ordinals.insert(1, at);
        r.coupon_loadings.push(CouponLoading {
            draw_tranche_id: Some("generated-draw".into()),
            period_id: "generated-draw-coupon".into(),
            observation_index: 0,
            driver_index: 0,
            loading: 1.,
        });
        for date in &mut r.policy.exercise_dates {
            date.call_clean_price_factor = Some(1.01);
        }
    }
    r.source.performing_draw_tranches = vec![owner::PerformingDrawTranche {
        review: term::Identity {
            id: "generated-draw".into(),
            revision: "1".into(),
        },
        draw_ordinal: at,
        eligible_origins: [true, false, false],
        source,
    }];
    r
}
fn bits(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}
fn matrix_and_fit(r: &Request, expected: usize, constant_contracts: bool) {
    let facts = inspect(r).unwrap();
    assert_eq!(facts.active_features_upper, expected);
    assert!(facts.measured_work <= facts.scan_work_upper);
    admit(r).unwrap();
    let tape = generate_tape(r, &r.training).unwrap();
    let paths = collect(r, &r.training, &tape).unwrap();
    regression_svd_audit::start();
    let narrow = fit_paths(r, &paths).unwrap();
    let narrow_capture = regression_svd_audit::finish();
    regression_svd_audit::start();
    let generic = fit_paths_with_columns(r, &paths, regression_columns_upper(r).unwrap()).unwrap();
    let generic_capture = regression_svd_audit::finish();
    assert_eq!(narrow_capture.len(), r.policy.exercise_dates.len());
    assert_eq!(generic_capture.len(), narrow_capture.len());
    for (date, (a, b)) in narrow
        .regressions()
        .iter()
        .zip(generic.regressions())
        .enumerate()
    {
        assert_eq!(a.means.len(), paths[0].points[date].features.len());
        assert_eq!(a.active_features, b.active_features);
        assert_eq!(a.rank, b.rank);
        assert!(a.active_features.len() <= expected);
        bits(&a.means, &b.means);
        bits(&a.scales, &b.scales);
        bits(&a.coefficients, &b.coefficients);
        bits(&a.singular_values, &b.singular_values);
        let ma: Vec<_> = paths
            .iter()
            .flat_map(|p| {
                basis::values(
                    &p.points[date].features,
                    &a.means,
                    &a.scales,
                    &a.active_features,
                    a.basis_degree,
                )
            })
            .collect();
        let mb: Vec<_> = paths
            .iter()
            .flat_map(|p| {
                basis::values(
                    &p.points[date].features,
                    &b.means,
                    &b.scales,
                    &b.active_features,
                    b.basis_degree,
                )
            })
            .collect();
        bits(&ma, &mb);
        fn captured_bits(value: &serde_json::Value, bits: &mut Vec<u64>) {
            if let Some(values) = value.as_array() {
                for value in values {
                    captured_bits(value, bits);
                }
            } else {
                bits.push(value.as_f64().unwrap().to_bits());
            }
        }
        for field in ["matrix", "targets", "feature_rows", "means", "scales"] {
            let mut current = Vec::new();
            let mut unrestricted = Vec::new();
            captured_bits(&narrow_capture[date][field], &mut current);
            captured_bits(&generic_capture[date][field], &mut unrestricted);
            assert_eq!(current, unrestricted, "actual SVD input {field}");
            assert!(current.len() <= 128 * 64);
            if field == "matrix" {
                assert_eq!(current, ma.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
            }
        }
        if constant_contracts {
            let first = &paths[0].points[date].features;
            for path in &paths {
                bits(
                    &path.points[date].features[r.drivers.len()..],
                    &first[r.drivers.len()..],
                );
            }
        }
    }
    if let Some(directory) = std::env::var_os("DRAW_NUMERICAL_AUDIT_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let loading = r.coupon_loadings.first().map_or(0, |l| l.loading.to_bits());
        let main = match r.source.source.current.periods[0].coupon {
            Coupon::Fixed { annual_rate } => annual_rate.to_bits(),
            _ => 0,
        };
        let name = format!(
            "full-{}-draw-{loading:x}-main-{main:x}-cov-{:x}-ccf-{:x}-anchor-{}.json",
            facts.full_features_upper,
            r.annual_covariance[0].to_bits(),
            r.source.credit.intervals[0].credit_conversion_factor[0].to_bits(),
            r.hazard_loadings.len()
        );
        let record = serde_json::json!({"raw":r,"narrow_actual_svd":narrow_capture,"generic_actual_svd":generic_capture,"narrow_regressions":narrow.regressions(),"generic_regressions":generic.regressions()});
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))
            .unwrap();
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&record).unwrap())
            .unwrap();
    }
    println!("draw numerical full_features={},active_upper={},columns={},full_columns={},scan_spent={},scan_upper={}",facts.full_features_upper,expected,regression_numerical_columns_upper(r).unwrap(),regression_columns_upper(r).unwrap(),facts.measured_work,facts.scan_work_upper);
}
#[test]
fn fixed_supplied_draws_signed_and_delayed_coupons_keep_complete_native_matrices() {
    for rate in [0., -0., 0.12, -0.12] {
        let mut r = law(false);
        for c in [&mut r.source.source.original, &mut r.source.source.current] {
            c.periods[0].coupon = Coupon::Fixed { annual_rate: rate };
        }
        matrix_and_fit(&r, r.drivers.len(), true);
    }
}
#[test]
fn floating_draw_loading_counts_every_contract_observation_without_pruning() {
    let r = law(true);
    for value in [0., -0., f64::from_bits(1), 1., -1.] {
        let mut r = r.clone();
        r.coupon_loadings[0].loading = value;
        let expected = r.drivers.len() + if value == 0. { 0 } else { 1 + 1 + 2 };
        matrix_and_fit(&r, expected, value == 0.);
    }
    let mut invalid = r;
    invalid.coupon_loadings[0].loading = f64::NAN;
    assert_eq!(inspect(&invalid).unwrap().active_features_upper, 5);
    assert!(admit(&invalid).is_err());
}
#[test]
fn supplied_draws_keep_ccf_and_hazard_anchor_dimensions_without_random_activation() {
    let mut r = law(false);
    r.source.credit.intervals[0].credit_conversion_factor = [1.; 3];
    matrix_and_fit(&r, r.drivers.len(), true);
    r.hazard_loadings.push(HazardLoading {
        interval_index: 0,
        origin_index: 0,
        destination_index: 3,
        betas: vec![0.],
    });
    assert_eq!(
        inspect(&r).unwrap().active_features_upper,
        2 * r.drivers.len()
    );
    matrix_and_fit(&r, 2 * r.drivers.len(), true);
    r.hazard_loadings[0].betas[0] = 0.2;
    assert!(admit(&r)
        .unwrap_err()
        .contains("zero hazard cannot acquire implicit modeled credit"));
}
#[test]
fn zero_covariance_draws_have_intercept_only_and_unsupported_headers_keep_full() {
    let mut r = law(true);
    r.annual_covariance.fill(0.);
    matrix_and_fit(&r, 0, true);
    let mut no_clock = r.clone();
    no_clock.source.performing_draw_tranches[0]
        .source
        .current
        .asof_ordinal -= 1;
    let mut dated = r.clone();
    dated.source.performing_draw_tranches[0]
        .source
        .current
        .principal_plan = term::indexed::PrincipalPlan::DatedSuppliedPeriodOpening {
        principal_events: vec![],
    };
    let mut original_dated = r.clone();
    original_dated.source.performing_draw_tranches[0]
        .source
        .original
        .principal_plan = term::indexed::PrincipalPlan::DatedSuppliedPeriodOpening {
        principal_events: vec![],
    };
    let mut review = r.clone();
    review.source.performing_draw_tranches[0]
        .source
        .current_accounting_rate_reviews
        .push(crate::commercial_source_income::ProspectiveRateReview {
            review: r.identity.clone(),
            period_id: "generated-draw-coupon".into(),
            reset_ordinal: r.time_ordinals[1],
            annual_effective_rate: 0.,
        });
    let mut credit = r.clone();
    credit.source.credit.intervals[0].hazards_per_year[0][3] = 100.;
    let mut reviewed = r.clone();
    reviewed.source.reviewed_credit_actions=Some(serde_json::from_value(serde_json::json!({"policy":{"identity":{"id":"review","revision":"1"},"accrued":"transfer_until_reviewed_reversal","basis":"transfer_until_reviewed_chargeoff","allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"}})).unwrap());
    let mut transfer = r.clone();
    let raw: owner::Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-secondary-entitlement-1.json"
    ))
    .unwrap();
    transfer.source.performing_draw_tranches[0].source.transfer = raw.source.transfer;
    for value in [
        no_clock,
        dated,
        original_dated,
        review,
        credit,
        reviewed,
        transfer,
    ] {
        let facts = inspect(&value).unwrap();
        assert_eq!(facts.active_features_upper, facts.full_features_upper);
        assert!(facts.measured_work <= facts.scan_work_upper);
    }
}
#[test]
fn complete_draw_scan_is_paid_for_all_32_header_and_period_inventories() {
    let mut r = law(true);
    let draw = r.source.performing_draw_tranches[0].clone();
    r.source.performing_draw_tranches = vec![draw; 32];
    let facts = inspect(&r).unwrap();
    assert!(facts.measured_work <= facts.scan_work_upper);
    assert_eq!(facts.active_features_upper, r.drivers.len() + 32 + 1 + 33);
    assert_eq!(
        facts.scan_work_upper,
        32 + 15 * r.source.credit.intervals.len()
            + r.annual_covariance.len()
            + r.coupon_loadings.len()
            + 2 * 33
            + 12 * 32
    );
}
