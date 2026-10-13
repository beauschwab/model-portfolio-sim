use portfolio_model_core::deposit_revised_income::{self as m, Request};
fn raw() -> Request {
    serde_json::from_str(include_str!("fixtures/deposit_revised_income.json")).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn rates(r: &mut Request, old: f64, new: f64) {
    r.original.segments[0].opening_paid_rate = old;
    for d in &mut r.original.drivers {
        d.reference_rate = old;
    }
    for path in &mut r.runs {
        path.revised.segments[0].opening_paid_rate = old;
        for d in &mut path.revised.drivers {
            d.reference_rate = if d.ordinal < r.original.as_of_ordinal {
                old
            } else {
                new
            };
        }
    }
    let growth = (r.anchor.original_annual_effective_rate.ln_1p() * 30. / 365.).exp();
    r.anchor.original_gross_carrying_value = (100. + old * 100. * 30. / 360.) / growth;
    r.anchor.original_price_per_100 = r.anchor.original_gross_carrying_value;
}
#[test]
fn premium_original_yield_revision_separate_from_future_expense() {
    let r = raw();
    let v = m::evaluate(&r).unwrap();
    let p = &v.paths[0];
    near(v.original_pv, 102. / 1.01);
    near(p.revised_carrying_value, 103. / 1.01);
    near(p.basis_revision, 1. / 1.01);
    near(
        p.income.rows.iter().map(|x| x.effective_interest).sum(),
        103. - 103. / 1.01,
    );
    near(p.income.remaining_principal, 0.);
    near(p.income.remaining_accrued_interest, 0.);
    near(p.income.remaining_gross_carrying_value, 0.);
    assert_eq!(p.income.original_yield_anchor.id, "original-eir");
    assert!(!v.installed_adoption && !v.accounting_standard_approval);
}
#[test]
fn discount_cost_retained() {
    let mut r = raw();
    rates(&mut r, 0., 0.12);
    let v = m::evaluate(&r).unwrap();
    near(v.original_pv, 100. / 1.01);
    near(v.paths[0].revised_carrying_value, 100.);
    near(v.paths[0].basis_revision, 100. - 100. / 1.01);
    near(
        v.paths[0]
            .income
            .rows
            .iter()
            .map(|x| x.effective_interest)
            .sum(),
        1.,
    );
}
#[test]
fn lower_coupon_separate_gain() {
    let mut r = raw();
    rates(&mut r, 0.24, 0.12);
    let v = m::evaluate(&r).unwrap();
    near(v.paths[0].basis_revision, -1. / 1.01);
}
#[test]
fn zero_yield_has_catchup_without_eir() {
    let mut r = raw();
    r.anchor.original_annual_effective_rate = 0.;
    rates(&mut r, 0.24, 0.36);
    let v = m::evaluate(&r).unwrap();
    near(v.paths[0].basis_revision, 1.);
    assert!(v.paths[0]
        .income
        .rows
        .iter()
        .all(|x| x.effective_interest == 0.));
}
#[test]
fn unchanged_history_exact_zero_revision() {
    let mut r = raw();
    rates(&mut r, 0.24, 0.24);
    let v = m::evaluate(&r).unwrap();
    assert_eq!(v.paths[0].basis_revision, 0.);
}
#[test]
fn original_cost_mismatch_rejects() {
    let mut r = raw();
    r.anchor.original_gross_carrying_value += 1.;
    r.anchor.original_price_per_100 += 1.;
    assert!(m::evaluate(&r)
        .unwrap_err()
        .contains("original physical cash"));
}
#[test]
fn immutable_terms_and_past_observations_reject() {
    let mut r = raw();
    r.runs[0].revised.segments[0].maturity_ordinal = Some(r.original.as_of_ordinal + 29);
    assert!(m::evaluate(&r)
        .unwrap_err()
        .contains("immutable contractual"));
    let mut r = raw();
    r.runs[0].revised.drivers[0].reference_rate = 0.3;
    assert!(m::evaluate(&r).unwrap_err().contains("past observations"));
}
#[test]
fn undeclared_dimension_and_duplicate_owner_reject() {
    let mut r = raw();
    r.policy.changed_driver_dimensions.clear();
    assert!(m::evaluate(&r).unwrap_err().contains("undeclared"));
    let mut r = raw();
    r.runs.push(r.runs[0].clone());
    assert!(m::evaluate(&r).unwrap_err().contains("duplicate revision"));
}
#[test]
fn unsupported_negative_inflow_seasoned_reject() {
    let mut r = raw();
    r.anchor.original_annual_effective_rate = -0.01;
    assert!(m::audit_request(&r).is_err());
    let mut r = raw();
    r.runs[0].revised.drivers[1].external_inflows[0] = 1.;
    assert!(m::audit_request(&r).unwrap_err().contains("inflow"));
    let mut r = raw();
    r.anchor.purchase_ordinal -= 1;
    assert!(m::audit_request(&r).is_err());
}
#[test]
fn incomplete_tail_and_65_days_reject() {
    let mut r = raw();
    r.original.segments[0].maturity_ordinal = None;
    assert!(m::audit_request(&r).is_err());
    let mut r = raw();
    r.original.horizon_days = 65;
    assert!(m::audit_request(&r).is_err());
}
#[test]
fn cancellation_before_source_and_during_day_fails_no_result() {
    let r = raw();
    let mut calls = 0;
    let e = m::evaluate_with_check(&r, || {
        calls += 1;
        if calls == 10 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(e, "cancelled");
    assert_eq!(calls, 10);
    assert!(m::evaluate_with_check(&r, || Err("cancel before admission".into())).is_err());
}
#[test]
fn original_anchors_and_raw_histories_unchanged() {
    let r = raw();
    let before = serde_json::to_vec(&r).unwrap();
    let v = m::evaluate(&r).unwrap();
    assert_eq!(before, serde_json::to_vec(&r).unwrap());
    for p in &v.paths {
        assert_eq!(p.original_cost, r.anchor.original_gross_carrying_value);
        assert_eq!(
            p.income_request.original_annual_effective_rate.to_bits(),
            r.anchor.original_annual_effective_rate.to_bits()
        );
        assert_eq!(
            p.income_request.events[0].payment_ordinal,
            r.anchor.purchase_ordinal
        );
        assert!(p.income.rows[0].effective_interest == 0. && p.income.rows[0].cash_received == 0.);
    }
}
#[test]
fn changed_behavior_moves_expected_cash_only_on_native_dates() {
    let mut r = raw();
    rates(&mut r, 0., 0.);
    r.policy.changed_driver_dimensions = vec!["confidence_stress".into()];
    r.original.segments[0].retention.closure_stress_sensitivity = 1.;
    for p in &mut r.runs {
        p.revised.segments[0].retention.closure_stress_sensitivity = 1.;
        p.revised.drivers[2].confidence_stress[0] = 365. * 1.25_f64.ln();
    }
    let v = m::evaluate(&r).unwrap();
    let p = &v.paths[0];
    let cf: Vec<_> = p
        .source
        .cashflows
        .iter()
        .filter(|x| x.principal != 0.)
        .collect();
    assert_eq!(cf.len(), 2);
    assert_eq!(cf[0].ordinal, r.original.as_of_ordinal + 1);
    near(cf[0].principal, -20.);
    near(cf[1].principal, -80.);
    let expected = 20. / 1.01_f64.powf(1. / 30.) + 80. / 1.01;
    near(p.revised_carrying_value, expected);
    near(p.basis_revision, expected - 100. / 1.01);
}

#[test]
fn original_day0_disposition_is_explicitly_unsupported() {
    let mut r = raw();
    r.original.segments[0].retention.closure_stress_sensitivity = 1.;
    r.original.drivers[1].confidence_stress[0] = 365. * 1.25_f64.ln();
    for run in &mut r.runs {
        run.revised.segments[0].retention.closure_stress_sensitivity = 1.;
    }
    r.policy
        .changed_driver_dimensions
        .push("confidence_stress".into());
    assert!(m::evaluate(&r).unwrap_err().contains("original day0 cash"));
}

#[test]
fn actual_maturity_at_full_horizon_keeps_native_cash_and_closure() {
    let mut r = raw();
    r.original.horizon_days = 30;
    r.original.drivers.pop();
    for run in &mut r.runs {
        run.revised.horizon_days = 30;
        run.revised.drivers.pop();
    }
    let v = m::evaluate(&r).unwrap();
    assert_eq!(v.admission.full_end_ordinal, r.original.as_of_ordinal + 30);
    for path in &v.paths {
        let last = path.source.cashflows.last().unwrap();
        assert_eq!(last.ordinal, r.original.as_of_ordinal + 30);
        near(path.revised_carrying_value, 103. / 1.01);
        near(path.income.remaining_principal, 0.);
        near(path.income.remaining_accrued_interest, 0.);
        near(path.income.remaining_gross_carrying_value, 0.);
    }
}
