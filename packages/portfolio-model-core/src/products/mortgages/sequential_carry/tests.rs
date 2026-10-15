use crate::products::mortgages::state::{self as mortgage_state, MortgageOwnership};
use crate::{mortgage_sequential_carry as s, mortgage_sequential_source as boundary_receipt};
fn raw() -> s::Request {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/mortgage_receipts/request.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn current_run(r: &mut s::Request) {
    r.contract.identity.run_id = "stress".into();
    r.pairing.current_run_id = "stress".into();
}
fn evaluate(r: &s::Request) -> s::ResultData {
    evaluate_result(r).unwrap()
}
fn evaluate_result(r: &s::Request) -> Result<s::ResultData, String> {
    s::evaluate(r)
}
fn admit(r: &s::Request) -> Result<s::Admission, String> {
    s::admit(r)
}
#[test]
fn unchanged_original_quote_carry_and_actual_generator_identity() {
    let r = raw();
    let o = evaluate(&r);
    assert_eq!(
        serde_json::to_value(&o.source.native_projection).unwrap(),
        serde_json::to_value(mortgage_state::project(&r.contract).unwrap()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&o.source).unwrap(),
        serde_json::to_value(&o.original_source).unwrap()
    );
    assert_eq!(o.original_source_raw_hash, o.current_source_raw_hash);
    assert_eq!(
        o.original_source_receipt_hash,
        o.current_source_receipt_hash
    );
    let expected = 100.3 * (-0.02f64.ln_1p() * 30. / 365.).exp();
    near(o.actual_acquisition_cost, expected);
    near(o.total_earned_eir, 100.3 - expected);
    near(o.total_actual_cash, 100.3);
    near(o.closing_reconciliation_error, 0.);
    near(o.cumulative_earned_eir_conservation_error, 0.);
    assert_eq!(o.closing_live_gross_carrying, [0.; 3]);
    assert!(!o.production_adoption);
}
#[test]
fn changed_default_keeps_original_cost_yield_and_retains_gross_tails() {
    let mut r = raw();
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    let o = evaluate(&r);
    let t = 30. / 365.;
    let k = 0.02f64.ln_1p();
    let h = 0.2f64;
    near(
        o.total_earned_eir,
        r.acquisition.actual_cost * k * ((k - h) * t).exp_m1() / (k - h),
    );
    near(
        o.intervals[0].transport.closing.gross_carrying[3],
        r.acquisition.actual_cost * h * ((k - h) * t).exp_m1() / (k - h),
    );
    assert_eq!(
        o.raw_request.acquisition.actual_cost,
        r.acquisition.actual_cost
    );
    assert_ne!(o.current_source_raw_hash, o.original_source_raw_hash);
    assert_eq!(o.original_quote_pv, evaluate(&raw()).original_quote_pv);
    assert!(o.total_actual_cash < 100.3);
    assert_eq!(o.full_end_ordinal, r.contract.as_of_ordinal + 60);
    assert_eq!(o.default_closeouts.len(), 1);
    let d = &o.default_closeouts[0];
    near(d.gross_principal, 100. * (-h * t).exp_m1().abs());
    assert!(d.gross_contract_interest > 0.);
    assert!(d.principal_economic_disposal > 0.);
    assert_eq!(d.principal_disposal_ordinal, r.contract.as_of_ordinal + 60);
    assert_eq!(d.closing_economic_interest, 0.);
    assert_eq!(d.native_principal_cash_ids.len(), 2);
    near(
        o.actual_acquisition_cost + o.total_accounting_income,
        o.total_actual_cash,
    );
}
#[test]
fn changed_prepayment_recognizes_exact_exit_basis_without_reanchoring() {
    let mut r = raw();
    current_run(&mut r);
    r.contract.path[0].turnover_hazard_per_year = 0.8;
    let o = evaluate(&r);
    assert!(o.actions.iter().any(|a| a.kind == "turnover_exact_exit"));
    assert_eq!(o.actual_acquisition_cost, r.acquisition.actual_cost);
    near(o.original_quote_pv, r.acquisition.actual_cost);
    near(o.closing_reconciliation_error, 0.);
    assert_ne!(o.total_actual_cash, 100.3);
}
#[test]
fn probability_is_not_multiplied_again_into_monetary_carry() {
    let mut r = raw();
    for c in [&mut r.contract, &mut r.original_contract] {
        c.opening_probability = [0.5, 0., 0.];
        c.opening_borrower_principal = [50., 0., 0.];
        c.opening_investor_principal = [50., 0., 0.];
    }
    r.acquisition.actual_cost *= 0.5;
    r.acquisition.opening_gross_carrying[0] *= 0.5;
    let o = evaluate(&r);
    near(o.total_actual_cash, 50.15);
    near(o.total_earned_eir, 50.15 - r.acquisition.actual_cost);
}
#[test]
fn source_boundary_capture_is_exact_and_cash_links_complete() {
    let mut r = raw();
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.3;
    let o = boundary_receipt::project_with_check(&r.contract, &mut || Ok(())).unwrap();
    assert_eq!(
        serde_json::to_value(&o.native_projection).unwrap(),
        serde_json::to_value(mortgage_state::project(&r.contract).unwrap()).unwrap()
    );
    assert_eq!(
        o.capture.cash_links.len(),
        o.native_projection.entitlements.len()
    );
    assert_eq!(o.capture.boundaries[0].default_recoveries.len(), 2);
}
#[test]
fn acquisition_and_original_contract_pairing_fail_closed() {
    let mut r = raw();
    r.acquisition.actual_cost += 1.;
    r.acquisition.opening_gross_carrying[0] += 1.;
    assert!(evaluate_result(&r).unwrap_err().contains("original-yield"));
    let mut r = raw();
    r.original_contract.current_payment += 1.;
    assert!(admit(&r).is_err());
    let mut r = raw();
    r.original_contract.payment_dates[0] += 1;
    assert!(admit(&r).is_err());
    let mut r = raw();
    r.pairing.original_run_id = "wrong".into();
    assert!(admit(&r).is_err());
}
#[test]
fn explicit_float_and_interest_disposal_policy_rejections() {
    let mut r = raw();
    if let MortgageOwnership::WholeLoan {
        remittance_lag_days,
        ..
    } = &mut r.contract.ownership
    {
        *remittance_lag_days = 1;
    }
    assert!(admit(&r).is_err());
    let mut r = raw();
    r.policy.interest.economic_disposal_review = None;
    assert!(admit(&r).is_err());
    let mut r = raw();
    r.policy.economic_principal_disposal_lag_days = 29;
    assert!(admit(&r)
        .unwrap_err()
        .contains("complete native recoveries"));
}
#[test]
fn callbacks_and_failed_writer_do_not_report_success() {
    let r = raw();
    let mut output = Vec::new();
    assert!(s::run_with_check(&r, &mut output, &mut || Err("cancel".into())).is_err());
    assert!(output.is_empty());
    let mut calls = 0;
    assert!(s::evaluate_with_check(&r, &mut || {
        calls += 1;
        if calls > 100 {
            Err("cancel mid-admission".into())
        } else {
            Ok(())
        }
    })
    .is_err());
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("broken"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(s::run(&r, &mut Broken).unwrap_err().contains("broken"));
}
#[test]
fn repeated_same_generator_intervals_preserve_semigroup_and_reward_history() {
    let mut whole = raw();
    current_run(&mut whole);
    whole.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    let a = evaluate(&whole);
    let mut split = whole.clone();
    for day in [10, 20] {
        let mut p = split.contract.path[0].clone();
        p.start_ordinal += day;
        split.contract.path.push(p);
    }
    let b = evaluate(&split);
    assert_eq!(b.intervals.len(), 3);
    assert_eq!(b.admission.original_interval_count, 1);
    assert!(b.intervals[1].opening.earned_eir[0] > 0.);
    assert_eq!(
        b.intervals[1].opening.gross_carrying[0],
        b.intervals[0].closing_live_gross_carrying[0]
    );
    near(b.total_earned_eir, a.total_earned_eir);
    near(
        b.default_closeouts
            .iter()
            .map(|d| d.exact_default_carry)
            .sum(),
        a.default_closeouts[0].exact_default_carry,
    );
    near(
        b.intervals[2].transport.closing.gross_carrying[0],
        a.intervals[0].transport.closing.gross_carrying[0],
    );
    near(b.cumulative_earned_eir_conservation_error, 0.);
    near(b.closing_reconciliation_error, 0.);
}
fn three_payments(y: f64) -> s::Request {
    let mut r = raw();
    let day = r.contract.as_of_ordinal;
    for c in [&mut r.contract, &mut r.original_contract] {
        c.payment_dates = vec![day + 10, day + 20, day + 30];
        c.original_payment = 34.;
        c.current_payment = 34.;
    }
    let k = y.ln_1p();
    let cost = 34. * (-k * 10. / 365.).exp()
        + 34. * (-k * 20. / 365.).exp()
        + 32.1982661 * (-k * 30. / 365.).exp();
    r.acquisition.original_annual_effective_rate = y;
    r.acquisition.actual_cost = cost;
    r.acquisition.opening_gross_carrying = [cost, 0., 0.];
    r
}
#[test]
fn original_quote_three_payment_premium_discount_zero_and_signed_yield() {
    for y in [0.02, 0., -0.02, 0.10] {
        let r = three_payments(y);
        let o = evaluate(&r);
        let mut g = r.acquisition.actual_cost;
        let growth = (y.ln_1p() * 10. / 365.).exp();
        let mut earned = 0.;
        for (i, cash) in [34., 34., 32.1982661].iter().enumerate() {
            earned += g * (growth - 1.);
            g *= growth;
            near(o.intervals[i].transport.closing.gross_carrying[0], g);
            g -= cash;
            near(o.intervals[i].closing_live_gross_carrying[0], g);
        }
        near(o.total_earned_eir, earned);
        near(
            o.total_accounting_income,
            100.1982661 - r.acquisition.actual_cost,
        );
        near(o.original_quote_pv, r.acquisition.actual_cost);
        near(o.closing_reconciliation_error, 0.);
    }
}
#[test]
fn migration_cure_and_multiple_default_origins_preserve_monetary_sources() {
    let mut r = three_payments(0.02);
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year = [
        [0., 1.1, 0.2, 0.3, 0.],
        [0.7, 0., 0.5, 0.4, 0.],
        [0.1, 0.6, 0., 0.8, 0.],
    ];
    let o = evaluate(&r);
    assert_eq!(
        o.source.native_projection.defaults.len(),
        o.default_closeouts.len()
    );
    assert!(o
        .default_closeouts
        .iter()
        .any(|d| d.origin == 2 && d.gross_principal > 0.));
    for d in &o.default_closeouts {
        near(
            d.gross_principal,
            d.principal_cash + d.principal_economic_disposal,
        );
        near(
            d.gross_contract_interest,
            d.interest_cash + d.interest_economic_disposal,
        );
    }
    near(o.cumulative_earned_eir_conservation_error, 0.);
    near(o.closing_reconciliation_error, 0.);
}
#[test]
fn current_credit_recovery_and_collection_changes_do_not_refit_original_quote() {
    let mut r = raw();
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.8;
    r.contract.loss_given_default = [0.8, 0.5, 0.6];
    r.contract.complete_recovery_tails[0] = vec![mortgage_state::MortgageRecoveryWeight {
        lag_days: 30,
        weight: 1.,
    }];
    let o = evaluate(&r);
    near(o.original_quote_pv, r.acquisition.actual_cost);
    near(
        o.default_closeouts[0].principal_cash,
        o.default_closeouts[0].gross_principal * 0.2,
    );
    assert_ne!(
        o.current_source_receipt_hash,
        o.original_source_receipt_hash
    );
}
#[test]
fn native_loop_cancellation_and_flush_failure_are_errors() {
    let r = raw();
    let mut calls = 0;
    assert!(boundary_receipt::project_with_check(&r.contract, &mut || {
        calls += 1;
        if calls == 4 {
            Err("cancel actual loop".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err()
    .contains("cancel actual loop"));
    struct NoFlush(Vec<u8>);
    impl std::io::Write for NoFlush {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("flush failed"))
        }
    }
    let mut output = NoFlush(Vec::new());
    assert!(s::run(&r, &mut output)
        .unwrap_err()
        .contains("flush failed"));
    assert!(!output.0.is_empty());
}
#[test]
fn changed_supplied_arm_fixing_keeps_original_acquisition_and_reset_clock() {
    let mut r = raw();
    let day = r.contract.as_of_ordinal;
    let terms = serde_json::json!({"kind":"arm","terms":{
        "index":{"index_id":"supplied-index","kind":"term","tenor_months":1,"average_calendar_days":null},
        "reference_calendar":{"id":"calendar","revision":"v1"},"margin":0.01,"original_note_rate":0.0365,
        "initial_up_cap":0.02,"initial_down_cap":0.015,"subsequent_up_cap":0.01,"subsequent_down_cap":0.01,
        "lifetime_up_cap":0.03,"lifetime_down_cap":0.025,"note_floor":0.02,"prior_reset_count":0,"payment_recast":"retain_original"},
        "resets":[{"ordinal":day+10,"input":{"kind":"projected","fixing":{"fixing_id":"supplied-fixing","revision":"v1","index_id":"supplied-index","value_ordinal":day+10,
        "publication_unix_seconds":r.contract.publication_cutoff_unix_seconds-1,"rate":0.0265},"projection_source":{"id":"supplied-path","revision":"v1"},
        "available_unix_seconds":r.contract.publication_cutoff_unix_seconds-1,"reset_cutoff_unix_seconds":r.contract.publication_cutoff_unix_seconds+864000}}]});
    r.original_contract.rate = serde_json::from_value(terms.clone()).unwrap();
    let mut stressed = terms;
    stressed["resets"][0]["input"]["fixing"]["rate"] = serde_json::json!(0.06);
    r.contract.rate = serde_json::from_value(stressed).unwrap();
    current_run(&mut r);
    let o = evaluate(&r);
    assert_eq!(o.intervals.len(), 2);
    assert_eq!(o.source.intervals[0].note_rate_before_boundary, 0.0365);
    near(o.source.intervals[1].note_rate_before_boundary, 0.0565);
    near(
        o.original_source.intervals[1].note_rate_before_boundary,
        0.0365,
    );
    assert_eq!(
        o.intervals[1].opening.gross_carrying[0],
        o.intervals[0].closing_live_gross_carrying[0]
    );
    near(o.original_quote_pv, r.acquisition.actual_cost);
    assert!(o.total_actual_cash > 100.3);
    near(o.closing_reconciliation_error, 0.);
}
#[test]
fn refinance_market_change_uses_actual_generator_without_reanchoring() {
    let mut r = three_payments(0.02);
    for c in [&mut r.contract, &mut r.original_contract] {
        c.refinance.incentive_slope = 4.;
        c.path[0].refinancing_market_rate = 0.1;
    }
    r.contract.path[0].refinancing_market_rate = 0.01;
    current_run(&mut r);
    let o = evaluate(&r);
    assert!(
        o.source.intervals[0]
            .loan_operator_request
            .transition_hazards_per_year[0][4]
            > 0.
    );
    assert_eq!(
        o.original_source.intervals[0]
            .loan_operator_request
            .transition_hazards_per_year[0][4],
        0.
    );
    assert!(o.actions.iter().any(|a| a.kind == "refinance_exact_exit"));
    near(o.original_quote_pv, r.acquisition.actual_cost);
    near(o.closing_reconciliation_error, 0.);
}
#[test]
fn escaped_metadata_full_tail_cardinality_and_work_reject_before_generation() {
    let mut r = three_payments(0.02);
    for c in [&mut r.contract, &mut r.original_contract] {
        c.complete_recovery_tails = std::array::from_fn(|_| {
            (1..=32)
                .map(|lag_days| mortgage_state::MortgageRecoveryWeight {
                    lag_days,
                    weight: 1. / 32.,
                })
                .collect()
        });
    }
    r.policy.economic_principal_disposal_lag_days = 32;
    r.policy.interest.disposal_lag_days = 32;
    r.policy.interest.complete_collection_tails = std::array::from_fn(|_| {
        (1..=32)
            .map(
                |lag_days| crate::products::mortgages::default_interest::CollectionWeight {
                    lag_days,
                    weight: 1. / 32.,
                },
            )
            .collect()
    });
    let label = "\\".repeat(250);
    for i in [
        &mut r.acquisition.identity,
        &mut r.acquisition.original_yield_anchor,
        &mut r.acquisition.income_review,
        &mut r.policy.identity,
        &mut r.policy.default_principal_book_review,
        &mut r.policy.economic_principal_disposal_review,
        &mut r.policy.interest.review,
        r.policy.interest.economic_disposal_review.as_mut().unwrap(),
        &mut r.pairing.review,
    ] {
        i.id = label.clone();
        i.revision = label.clone();
    }
    let mut output = Vec::new();
    let e = s::run(&r, &mut output).unwrap_err();
    assert!(e.contains("retained byte admission"), "{e}");
    assert!(output.is_empty());
    let mut r = raw();
    for day in 1..=13 {
        let mut p = r.contract.path[0].clone();
        p.start_ordinal += day;
        r.contract.path.push(p);
    }
    assert!(admit(&r).unwrap_err().contains("interval/work"));
}
#[test]
fn missing_complete_live_tail_never_creates_terminal_cash() {
    let mut r = raw();
    current_run(&mut r);
    r.contract.borrower_collection_fraction[0] = 0.5;
    assert!(s::evaluate(&r)
        .unwrap_err()
        .contains("complete paired native live"));
}
#[test]
fn original_default_economic_receipts_alone_determine_the_quote() {
    let mut r = raw();
    r.original_contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.8;
    current_run(&mut r);
    let h = 0.2f64;
    let t = 30. / 365.;
    let k = 0.02f64.ln_1p();
    let survival = (-h * t).exp();
    let dp = 100. * (1. - survival);
    let da = 100. * 0.0365 * ((1. - survival) / h - t * survival);
    let discount = |day: f64| (-k * day / 365.).exp();
    let cost = 100.3 * survival * discount(30.)
        + 0.6 * dp * (0.25 * discount(45.) + 0.75 * discount(60.))
        + 0.5 * da * (0.25 * discount(35.) + 0.75 * discount(40.));
    r.acquisition.actual_cost = cost;
    r.acquisition.opening_gross_carrying = [cost, 0., 0.];
    let o = evaluate(&r);
    near(o.original_quote_pv, cost);
    assert_eq!(o.original_economic_closeouts.len(), 1);
    let d = &o.original_economic_closeouts[0];
    near(d.gross_principal, dp);
    near(d.gross_contract_interest, da);
    near(d.principal_cash, dp * 0.6);
    near(d.interest_cash, da * 0.5);
    assert_eq!(o.original_reviewed_interest_cash.len(), 2);
    assert_eq!(o.original_full_end_ordinal, r.contract.as_of_ordinal + 60);
    assert!(o.total_actual_cash < cost);
    near(o.closing_reconciliation_error, 0.);
}
#[test]
fn unrecognized_default_interest_is_a_gross_economic_right() {
    let mut r = raw();
    for c in [&mut r.contract, &mut r.original_contract] {
        if let MortgageOwnership::WholeLoan {
            recognized_accrual_fraction,
            ..
        } = &mut c.ownership
        {
            *recognized_accrual_fraction = [0.; 3];
        }
    }
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    current_run(&mut r);
    let o = evaluate(&r);
    let d = &o.default_closeouts[0];
    assert_eq!(d.recognized_interest, 0.);
    assert!(d.gross_contract_interest > 0.);
    assert!(d.interest_cash > 0.);
    assert!(d.interest_economic_disposal > 0.);
    near(
        d.exact_default_carry,
        d.gross_principal + d.signed_default_basis,
    );
    near(o.closing_reconciliation_error, 0.);
}
#[test]
fn economic_waiver_uses_actual_tail_cash_without_normalizing_near_unit_weights() {
    let mut r = raw();
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.policy.interest.complete_collection_tails[0][0].weight = 0.4;
    r.policy.interest.complete_collection_tails[0][1].weight = 0.5999999999998;
    let o = evaluate(&r);
    let actual = o
        .reviewed_interest_cash
        .iter()
        .map(|e| e.amount)
        .sum::<f64>();
    assert_eq!(o.default_closeouts[0].interest_cash, actual);
    assert_eq!(
        o.default_closeouts[0].interest_economic_disposal,
        o.default_closeouts[0].gross_contract_interest - actual
    );
    r.policy.interest.collection_fraction[0] = 1.;
    r.policy.interest.complete_collection_tails[0][0].weight = 0.5;
    r.policy.interest.complete_collection_tails[0][1].weight = 0.5000000000005;
    assert!(evaluate_result(&r)
        .unwrap_err()
        .contains("overcollected gross claims"));
}
fn scale(r: &mut s::Request, factor: f64) {
    for c in [&mut r.contract, &mut r.original_contract] {
        c.original_principal *= factor;
        c.original_payment *= factor;
        c.current_payment *= factor;
        c.opening_contractual_principal *= factor;
        for x in &mut c.opening_borrower_principal {
            *x *= factor;
        }
        for x in &mut c.opening_investor_principal {
            *x *= factor;
        }
    }
    r.acquisition.actual_cost *= factor;
    for x in &mut r.acquisition.opening_gross_carrying {
        *x *= factor;
    }
}
#[test]
fn original_large_near_unit_recovery_disposal_reconciles_actual_rows() {
    let mut r = raw();
    scale(&mut r, 1e8);
    r.original_contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.original_contract.complete_recovery_tails[0][1].weight = 0.749999999999999;
    current_run(&mut r);
    let h = 0.2f64;
    let t = 30. / 365.;
    let k = 0.02f64.ln_1p();
    let survival = (-h * t).exp();
    let dp = 1e10 * (1. - survival);
    let da = 1e10 * 0.0365 * ((1. - survival) / h - t * survival);
    let discount = |d: f64| (-k * d / 365.).exp();
    let cost = 1.003e10 * survival * discount(30.)
        + 0.6 * dp * (0.25 * discount(45.) + 0.749999999999999 * discount(60.))
        + 0.5 * da * (0.25 * discount(35.) + 0.75 * discount(40.));
    r.acquisition.actual_cost = cost;
    r.acquisition.opening_gross_carrying = [cost, 0., 0.];
    let o = evaluate(&r);
    let d = &o.original_economic_closeouts[0];
    let actual = o.original_source.native_projection.defaults[0]
        .complete_recovery_tail
        .iter()
        .map(|x| x.principal)
        .sum::<f64>();
    assert_eq!(d.principal_cash, actual);
    assert_eq!(d.principal_economic_disposal, d.gross_principal - actual);
    assert_eq!(d.closing_economic_principal, 0.);
    near(o.original_quote_pv, cost);
    near(
        o.actual_acquisition_cost + o.total_accounting_income,
        o.total_actual_cash,
    );
}
#[test]
fn large_recovery_shortfall_retains_book_claim_and_full_close_rejects() {
    let mut r = raw();
    scale(&mut r, 1e8);
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.contract.complete_recovery_tails[0][1].weight = 0.749999999999999;
    let native = mortgage_state::project(&r.contract).unwrap();
    let d = &native.defaults[0];
    let actual = d
        .complete_recovery_tail
        .iter()
        .map(|x| x.principal)
        .sum::<f64>();
    let outstanding = d.investor_principal - d.borrower_principal_loss - actual;
    assert!(outstanding > 1e-9);
    assert!(evaluate_result(&r)
        .unwrap_err()
        .contains("full native cash/gross economic/book ownership closure"));
}
#[test]
fn native_principal_tail_domain_and_waiver_only_overcollection_are_explicit() {
    let mut r = raw();
    scale(&mut r, 1e8);
    current_run(&mut r);
    r.contract.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.contract.loss_given_default[0] = 0.;
    r.contract.complete_recovery_tails[0][1].weight = 0.750000000000001;
    assert!(evaluate_result(&r)
        .unwrap_err()
        .contains("overcollected gross claims"));
    r.original_contract = r.contract.clone();
    r.original_contract.identity.run_id = "baseline".into();
    assert!(evaluate_result(&r)
        .unwrap_err()
        .contains("original economic waiver"));
    r.original_contract.complete_recovery_tails[0][1].weight = 0.749999999999;
    assert!(admit(&r).unwrap_err().contains("complete unit-weight tail"));
}
