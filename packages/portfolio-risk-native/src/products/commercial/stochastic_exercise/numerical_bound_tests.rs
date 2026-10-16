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
fn observations(r: &Request) -> usize {
    r.source
        .source
        .current
        .periods
        .iter()
        .map(|p| match &p.coupon {
            Coupon::Indexed { observations, .. } => observations.len(),
            _ => 0,
        })
        .sum()
}
#[test]
fn structural_constant_and_supplied_principal_bounds_keep_every_actual_feature() {
    let mut r = raw();
    r.annual_covariance.fill(0.);
    assert_eq!(sources::numerical_active_features_upper(&r).unwrap(), 0);
    let constant = fit_training(&r).unwrap();
    assert!(constant
        .regressions()
        .iter()
        .all(|f| f.active_features.is_empty() && f.coefficients.len() == 1));
    r.annual_covariance[0] = 0.0025;
    let expected = r.drivers.len() + observations(&r) + 1 + r.source.source.current.periods.len();
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        expected
    );
    let varying = fit_training(&r).unwrap();
    for fit in varying.regressions() {
        assert!(fit.active_features.len() <= expected);
        assert_eq!(
            fit.means.len(),
            r.drivers.len() + 4 + observations(&r) + 2 * r.source.source.current.periods.len()
        );
        assert!(fit.coefficients.len() <= regression_numerical_columns_upper(&r).unwrap());
    }
}
#[test]
fn any_declared_credit_or_payment_dependence_keeps_the_full_bound() {
    let mut r = raw();
    r.annual_covariance[0] = 0.0025;
    r.source.credit.intervals[0].hazards_per_year[1][3] = 0.01;
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        feature_bound(&r).unwrap()
    );
    r.source.credit.intervals[0].hazards_per_year[1][3] = 0.;
    r.source.source.current.principal_plan = term::indexed::PrincipalPlan::RetainedPayment {
        original_payment: 10.,
        terminal: term::indexed::TerminalPrincipal::ResidualPayoff {},
    };
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        feature_bound(&r).unwrap()
    );
}
#[test]
fn draw_catalog_and_hazard_anchors_and_future_observations_are_not_pruned() {
    let mut r = raw();
    r.annual_covariance[0] = 0.0025;
    let before = sources::numerical_active_features_upper(&r).unwrap();
    r.hazard_loadings.push(HazardLoading {
        interval_index: 0,
        origin_index: 0,
        destination_index: 3,
        betas: vec![0.; r.drivers.len()],
    });
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        before + r.drivers.len()
    );
    let indexed = r
        .source
        .source
        .current
        .periods
        .iter_mut()
        .find_map(|p| match &mut p.coupon {
            Coupon::Indexed { observations, .. } => Some(observations),
            _ => None,
        })
        .unwrap();
    indexed.push(indexed[0].clone());
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        before + r.drivers.len() + 1
    );
    r.source
        .performing_draw_tranches
        .push(owner::PerformingDrawTranche {
            review: r.identity.clone(),
            draw_ordinal: r.source.source.current.asof_ordinal + 1,
            eligible_origins: [true, false, false],
            source: r.source.source.clone(),
        });
    assert_eq!(
        sources::numerical_active_features_upper(&r).unwrap(),
        feature_bound(&r).unwrap()
    );
}
#[test]
fn numerical_reservation_changes_only_its_ticket_and_no_fit_adds_zero() {
    let mut r = raw();
    r.annual_covariance[0] = 0.0025;
    let generic = regression_columns_upper(&r).unwrap();
    let narrow = regression_numerical_columns_upper(&r).unwrap();
    assert!(narrow <= generic);
    let full = regression_numerical_reserve_with_columns(&r, generic).unwrap();
    let selected = regression_numerical_reserve_with_columns(&r, narrow).unwrap();
    assert!(selected.0 <= full.0 && selected.1 <= full.1);
    r.policy.exercise_dates.clear();
    assert_eq!(
        regression_numerical_reserve_with_columns(&r, generic).unwrap(),
        (0, 0)
    );
}

#[test]
fn structural_scan_and_fixed_header_tickets_cover_actual_calls_and_literals() {
    let mut r = raw();
    r.annual_covariance.fill(0.);
    let summary = numerical_bound::inspect(&r).unwrap();
    assert!(summary.measured_work <= summary.scan_work_upper);
    assert!(summary.all_credit_hazards_zero);
    let headers = std::mem::size_of::<numerical_bound::StructuralFeatures>()
        + std::mem::size_of::<NumericalCapacity>()
        + 2 * std::mem::size_of::<usize>();
    assert!(headers <= numerical_bound::HEADER_BYTES);
    let before = numerical_bound::calls();
    let target = analyze(&r).unwrap().evaluation_value;
    let measured = numerical_bound::calls() - before;
    assert_eq!(measured, 3);
    assert!(measured <= numerical_bound::SCAN_PASSES);
    let policy = calibration::Policy {
        policy: "native_retained_q_refit_bracket-1".into(),
        lower_oas: -0.1,
        upper_oas: 0.1,
        max_trials: 3,
    };
    let before = numerical_bound::calls();
    let solved = calibration::solve(&r, &policy, target).unwrap();
    assert_eq!(
        numerical_bound::calls() - before,
        2 + solved.receipt.trials.len()
    );
    assert!(numerical_bound::calls() - before <= numerical_bound::SCAN_PASSES * policy.max_trials);
    r.source.credit.intervals[0].hazards_per_year[0][3] = -0.1;
    let negative = numerical_bound::inspect(&r).unwrap();
    assert!(!negative.all_credit_hazards_zero);
    assert_eq!(negative.active_features_upper, negative.full_features_upper);
}
