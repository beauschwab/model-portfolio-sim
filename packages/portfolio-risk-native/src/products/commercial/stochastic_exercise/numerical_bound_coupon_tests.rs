//! Raw deterministic-coupon proof; actual financial features are never removed.
use super::super::*;
use super::*;

fn raw() -> Request {
    let mut r: Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-generated-exercise-1.json"
    ))
    .unwrap();
    for interval in &mut r.source.credit.intervals {
        interval.hazards_per_year = [[0.; 5]; 3];
    }
    r
}

fn fixed(rate: f64) -> Request {
    let mut r = raw();
    r.coupon_loadings.clear();
    for contract in [&mut r.source.source.original, &mut r.source.source.current] {
        for period in &mut contract.periods {
            period.coupon = Coupon::Fixed { annual_rate: rate };
        }
    }
    r
}

fn assert_bits(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

fn actual_paths(r: &Request) -> Vec<Path> {
    admit(r).unwrap();
    let tape = generate_tape(r, &r.training).unwrap();
    collect(r, &r.training, &tape).unwrap()
}

fn assert_actual_constant_coupons_keep_features_and_matrix(r: &Request, anchors: bool) {
    let paths = actual_paths(r);
    let d = r.drivers.len();
    let expected = d * if anchors { 2 } else { 1 };
    let facts = inspect(r).unwrap();
    assert_eq!(facts.active_features_upper, expected);
    assert!(facts.measured_work <= facts.scan_work_upper);
    for date in 0..r.policy.exercise_dates.len() {
        let first = &paths[0].points[date].features;
        assert!(paths.iter().any(|p| p.points[date].features[0] != first[0]));
        for p in &paths {
            for (index, value) in p.points[date].features.iter().enumerate() {
                if index >= d && !(anchors && (d + 4..2 * d + 4).contains(&index)) {
                    assert_eq!(
                        *value, first[index],
                        "path-dependent contractual feature {index}"
                    );
                }
            }
        }
    }
    let narrow = fit_paths(r, &paths).unwrap();
    let generic = fit_paths_with_columns(r, &paths, regression_columns_upper(r).unwrap()).unwrap();
    for (date, (a, b)) in narrow
        .regressions()
        .iter()
        .zip(generic.regressions())
        .enumerate()
    {
        assert_eq!(a.means.len(), paths[0].points[date].features.len());
        assert_eq!(a.active_features, b.active_features);
        assert!(a.active_features.len() <= expected);
        assert_bits(&a.means, &b.means);
        assert_bits(&a.scales, &b.scales);
        assert_bits(&a.coefficients, &b.coefficients);
        assert_bits(&a.singular_values, &b.singular_values);
        for p in &paths {
            let row = &p.points[date].features;
            assert_bits(
                &basis::values(row, &a.means, &a.scales, &a.active_features, a.basis_degree),
                &basis::values(row, &b.means, &b.scales, &b.active_features, b.basis_degree),
            );
        }
    }
}

#[test]
fn signed_fixed_coupon_paths_keep_all_features_and_exact_native_matrices() {
    for rate in [0., -0., 0.12, -0.12] {
        assert_actual_constant_coupons_keep_features_and_matrix(&fixed(rate), false);
    }
}

#[test]
fn supplied_indexed_fixings_with_zero_loadings_are_constant_at_actual_clocks() {
    for loading in [0., -0.] {
        let mut r = raw();
        r.coupon_loadings[0].loading = loading;
        for contract in [&mut r.source.source.original, &mut r.source.source.current] {
            for period in &mut contract.periods {
                if let Coupon::Indexed { observations, .. } = &mut period.coupon {
                    for observation in observations {
                        observation.rate = -0.04;
                    }
                }
            }
        }
        assert_actual_constant_coupons_keep_features_and_matrix(&r, false);
    }
}

#[test]
fn nonzero_coupon_loading_keeps_the_existing_varying_legal_bound() {
    let r = raw();
    let facts = inspect(&r).unwrap();
    let observations: usize = r
        .source
        .source
        .current
        .periods
        .iter()
        .map(|p| match &p.coupon {
            Coupon::Indexed { observations, .. } => observations.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(
        facts.active_features_upper,
        r.drivers.len() + observations + 1 + r.source.source.current.periods.len()
    );
    for loading in [f64::from_bits(1), -1., f64::NAN] {
        let mut other = r.clone();
        other.coupon_loadings[0].loading = loading;
        assert_eq!(
            inspect(&other).unwrap().active_features_upper,
            facts.active_features_upper
        );
        if loading.is_nan() {
            assert!(admit(&other).is_err());
        }
    }
    let paths = actual_paths(&r);
    let first = &paths[0].points[0].features;
    assert!(paths
        .iter()
        .any(|p| p.points[0].features[r.drivers.len() + 4..]
            .iter()
            .zip(&first[r.drivers.len() + 4..])
            .any(|(a, b)| a != b)));
    let fit = fit_paths(&r, &paths).unwrap();
    assert!(fit.regressions()[0]
        .active_features
        .iter()
        .any(|i| *i >= r.drivers.len() + 4));
}

#[test]
fn deterministic_coupons_retain_anchor_inventory_and_zero_hazard_binding_guard() {
    let mut r = fixed(0.03);
    r.hazard_loadings.push(HazardLoading {
        interval_index: 1,
        origin_index: 0,
        destination_index: 3,
        betas: vec![0.],
    });
    assert_actual_constant_coupons_keep_features_and_matrix(&r, true);
    let fit = fit_training(&r).unwrap();
    assert_eq!(fit.regressions()[0].active_features, [0]);
    assert_eq!(fit.regressions()[0].means[r.drivers.len() + 4], 0.);
    r.hazard_loadings[0].betas[0] = 0.2;
    assert_eq!(
        inspect(&r).unwrap().active_features_upper,
        2 * r.drivers.len()
    );
    assert!(admit(&r)
        .unwrap_err()
        .contains("zero hazard cannot acquire implicit modeled credit"));
}

#[test]
fn every_declared_credit_draw_dated_principal_or_review_keeps_full_fallback() {
    let r = fixed(0.03);
    let mut credit = r.clone();
    credit.source.credit.intervals[0].hazards_per_year[2][3] = 0.01;
    assert_eq!(
        inspect(&credit).unwrap().active_features_upper,
        sources::feature_bound(&credit).unwrap()
    );
    let mut draws = r.clone();
    draws
        .source
        .performing_draw_tranches
        .push(owner::PerformingDrawTranche {
            review: r.identity.clone(),
            draw_ordinal: r.source.source.current.asof_ordinal + 1,
            eligible_origins: [true, false, false],
            source: r.source.source.clone(),
        });
    assert_eq!(
        inspect(&draws).unwrap().active_features_upper,
        sources::feature_bound(&draws).unwrap()
    );
    let mut dated = r.clone();
    dated.source.source.current.principal_plan =
        term::indexed::PrincipalPlan::DatedSuppliedPeriodOpening {
            principal_events: Vec::new(),
        };
    assert_eq!(
        inspect(&dated).unwrap().active_features_upper,
        sources::feature_bound(&dated).unwrap()
    );
    let mut reviewed = r.clone();
    reviewed.source.reviewed_credit_actions=Some(serde_json::from_value(serde_json::json!({
        "policy":{"identity":{"id":"reviewed","revision":"1"},"accrued":"transfer_until_reviewed_reversal","basis":"transfer_until_reviewed_chargeoff","allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"}
    })).unwrap());
    assert_eq!(
        inspect(&reviewed).unwrap().active_features_upper,
        sources::feature_bound(&reviewed).unwrap()
    );
}

#[test]
fn dated_principal_late_signed_rights_keep_full_bound_and_transfer_guard() {
    let mut r = fixed(-0.04);
    let asof = r.source.source.current.asof_ordinal;
    for contract in [&mut r.source.source.original, &mut r.source.source.current] {
        contract.periods[0].payment_ordinal = asof + 35;
        contract.periods[1].coupon = Coupon::Fixed { annual_rate: 0. };
        contract.principal_plan = term::indexed::PrincipalPlan::DatedSuppliedPeriodOpening {
            principal_events: vec![term::indexed::principal::PrincipalCash {
                event_id: "dated-principal".into(),
                payment_ordinal: asof + 20,
                amount: 100.,
            }],
        };
    }
    r.policy.exercise_dates[0].ordinal = asof + 25;
    r.time_ordinals = vec![asof, asof + 15, asof + 20, asof + 25, asof + 35];
    assert_eq!(
        inspect(&r).unwrap().active_features_upper,
        sources::feature_bound(&r).unwrap()
    );
    let contract = term::indexed::build_path(&r.source.source.current).unwrap();
    assert_eq!(
        contract
            .schedule
            .events
            .iter()
            .filter(|e| e.components.principal != 0.)
            .count(),
        1
    );
    assert!(contract
        .schedule
        .events
        .iter()
        .any(|e| e.payment_ordinal == asof + 20 && e.components.principal == 100.));
    assert!(contract
        .schedule
        .events
        .iter()
        .any(|e| e.payment_ordinal == asof + 35 && e.components.cash_interest < 0.));
    // Generated dated-principal execution requires the separate complete-origin
    // transfer owner, which this law deliberately does not supply. The new
    // constant-coupon proof must neither admit it nor reinterpret live principal.
    assert!(admit(&r).unwrap_err().contains(
        "dated principal commercial ownership requires explicit complete-origin transfer"
    ));
}

#[test]
fn raw_coupon_scalar_scan_is_prepaid_and_no_fit_still_has_no_new_ticket() {
    let mut r = raw();
    let before = inspect(&r).unwrap();
    let first = r.coupon_loadings[0].clone();
    r.coupon_loadings.extend(std::iter::repeat_n(first, 63));
    let after = inspect(&r).unwrap();
    assert_eq!(after.scan_work_upper - before.scan_work_upper, 63);
    assert_eq!(after.measured_work - before.measured_work, 63);
    assert!(after.measured_work <= after.scan_work_upper);
    r.policy.exercise_dates.clear();
    let calls = calls();
    let capacity = regression_numerical_capacity(&r).unwrap();
    assert_eq!((capacity.work, capacity.bytes), (0, 0));
    assert_eq!(super::calls(), calls);
}
