use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::deposit_state::*;
#[test]
fn public_synthetic_example_has_independent_two_day_interest() {
    let r: DepositStateRequest = serde_json::from_str(include_str!(
        "../../../examples/models/deposit-state-1.json"
    ))
    .unwrap();
    let o = simulate(&r).unwrap();
    near(o.closing_balance, 1000.);
    near(o.total_interest_expense, 1000. * 0.365 * 2. / 365.);
    near(o.total_bank_cash_interest, -2.);
    near(o.closing_accrued_interest, 0.);
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn request(days: u32) -> DepositStateRequest {
    let asof = ordinal_from_ymd(2024, 2, 28).unwrap();
    let mut segments = Vec::new();
    for (i, opening) in [1000., 0.].iter().enumerate() {
        segments.push(DepositSegment {
            segment_id: format!("segment-{i}"),
            opening_balance: *opening,
            opening_accrued_interest: 0.,
            opening_paid_rate: 0.365,
            characteristics: SegmentCharacteristics {
                uninsured_fraction: 0.5,
                concentration: 0.2,
                digital_fraction: 0.8,
                relationship_fraction: 0.5,
                operational_fraction: 0.5,
            },
            repricing: RepricingPolicy {
                target: TargetRule::Linear {
                    intercept: 0.365,
                    beta: 0.,
                    competitor_weight: 0.,
                },
                observation_lag_days: 0,
                rising_half_life_days: 0.,
                falling_half_life_days: 0.,
                floor: 0.,
                ceiling: 1.,
                supported_observation_range: None,
            },
            retention: RetentionPolicy {
                withdrawal_per_year: 0.,
                closure_per_year: 0.,
                opportunity_gap_sensitivity: 0.,
                absolute_daily_velocity_sensitivity: 0.,
                uninsured_stress_sensitivity: 0.,
                concentration_stress_sensitivity: 0.,
                digital_stress_sensitivity: 0.,
                relationship_protection: 0.,
                operational_protection: 0.,
                closure_stress_sensitivity: 0.,
            },
            maturity_ordinal: None,
            interest_payment_ordinals: if days > 0 {
                vec![asof + days as i32]
            } else {
                vec![]
            },
        });
    }
    let drivers = (asof - 1..asof + days as i32)
        .map(|ordinal| DepositDriver {
            ordinal,
            reference_rate: 0.,
            competitor_rates: vec![0.; 2],
            offered_spread_stress: vec![0.; 2],
            confidence_stress: vec![0.; 2],
            external_inflows: vec![0.; 2],
        })
        .collect();
    DepositStateRequest {
        schema: SCHEMA.into(),
        identity: DepositIdentity {
            entity_id: "bank-1".into(),
            cohort_id: "customers-1".into(),
            currency: "USD".into(),
            source_revision: "dated-snapshot-1".into(),
            model_version: SCHEMA.into(),
            assumption_id: "supplied-policy-1".into(),
            assumption_rationale: "Synthetic mechanics fixture; no fitted retention claim".into(),
            run_id: "baseline".into(),
            measure: CashflowMeasure::Physical,
        },
        as_of_ordinal: asof,
        horizon_days: days,
        day_count: ActualDayCount::Act365Fixed,
        segments,
        migrations: vec![],
        drivers,
        terminal: TerminalPolicy::RetainResidual {},
    }
}
fn no_interest(r: &mut DepositStateRequest) {
    for s in &mut r.segments {
        s.opening_paid_rate = 0.;
        s.repricing.target = TargetRule::Linear {
            intercept: 0.,
            beta: 0.,
            competitor_weight: 0.,
        };
    }
}

#[test]
fn leap_day_interest_and_residual_are_actual_dates_without_final_principal_payout() {
    let r = request(2);
    let out = simulate(&r).unwrap();
    near(out.closing_balance, 1000.);
    near(out.total_withdrawals, 0.);
    near(out.total_assumed_terminal_payout, 0.);
    near(out.interest.iter().map(|v| v.amount).sum(), 2.);
    assert_eq!(
        out.interest[1].start_ordinal,
        ordinal_from_ymd(2024, 2, 29).unwrap()
    );
    assert_eq!(
        out.points.last().unwrap().ordinal,
        ordinal_from_ymd(2024, 3, 1).unwrap()
    );
    assert_eq!(out.cashflows.len(), 1);
    near(out.cashflows[0].principal, 0.);
    near(out.cashflows[0].cash_interest, -2.);
    near(out.closing_accrued_interest, 0.);
}
#[test]
fn daily_competing_withdrawal_closure_and_migration_conserve_once() {
    let mut r = request(1);
    no_interest(&mut r);
    r.segments[0].retention.withdrawal_per_year = 10.;
    r.segments[0].retention.closure_per_year = 20.;
    r.migrations.push(MigrationPolicy {
        from_segment: "segment-0".into(),
        to_segment: "segment-1".into(),
        base_per_year: 30.,
        destination_rate_gap_sensitivity: 0.,
    });
    let out = simulate(&r).unwrap();
    let exits = 1000. * (1. - (-60.0_f64 / 365.).exp());
    near(out.total_withdrawals, exits / 6.);
    near(out.total_closures, exits / 3.);
    near(out.migrations[0].principal, exits / 2.);
    near(out.points[1].balances[0], 1000. - exits);
    near(out.points[1].balances[1], exits / 2.);
    near(
        out.closing_balance + out.total_withdrawals + out.total_closures,
        1000.,
    );
}
#[test]
fn simultaneous_two_way_migration_does_not_reuse_incoming_balance_and_transports_interest() {
    let mut r = request(1);
    no_interest(&mut r);
    r.segments[0].opening_accrued_interest = 20.;
    r.segments[1].opening_balance = 500.;
    r.segments[1].opening_accrued_interest = -5.;
    for s in &mut r.segments {
        s.interest_payment_ordinals.clear();
    }
    r.migrations = vec![
        MigrationPolicy {
            from_segment: "segment-0".into(),
            to_segment: "segment-1".into(),
            base_per_year: 20.,
            destination_rate_gap_sensitivity: 0.,
        },
        MigrationPolicy {
            from_segment: "segment-1".into(),
            to_segment: "segment-0".into(),
            base_per_year: 40.,
            destination_rate_gap_sensitivity: 0.,
        },
    ];
    let out = simulate(&r).unwrap();
    let a = (-20.0_f64 / 365.).exp();
    let b = (-40.0_f64 / 365.).exp();
    near(out.points[1].balances[0], 1000. * a + 500. * (1. - b));
    near(out.points[1].balances[1], 500. * b + 1000. * (1. - a));
    near(out.points[1].accrued_interest[0], 20. * a - 5. * (1. - b));
    near(out.closing_balance, 1500.);
    near(out.closing_accrued_interest, 15.);
    assert!(out.cashflows.is_empty());
}
#[test]
fn external_inflows_are_distinct_cash_and_earn_after_start_of_day_transition() {
    let mut r = request(1);
    r.drivers[1].external_inflows = [0., 500.].to_vec();
    let out = simulate(&r).unwrap();
    near(out.closing_balance, 1500.);
    near(out.total_external_inflows, 500.);
    near(out.interest.iter().map(|v| v.amount).sum(), 1.5);
    let inflow = out
        .cashflows
        .iter()
        .find(|e| e.kind == "external_inflow")
        .unwrap();
    near(inflow.principal, 500.);
}
#[test]
fn migrated_balance_accrues_at_destination_offered_rate() {
    let mut r = request(1);
    r.segments[0].opening_paid_rate = 0.;
    r.segments[0].repricing.target = TargetRule::Linear {
        intercept: 0.,
        beta: 0.,
        competitor_weight: 0.,
    };
    r.migrations.push(MigrationPolicy {
        from_segment: "segment-0".into(),
        to_segment: "segment-1".into(),
        base_per_year: 50.,
        destination_rate_gap_sensitivity: 0.,
    });
    let out = simulate(&r).unwrap();
    let moved = 1000. * (1. - (-50.0_f64 / 365.).exp());
    near(out.migrations[0].principal, moved);
    near(out.interest[0].amount, moved * 0.365 / 365.);
}
#[test]
fn offered_rates_really_adjust_with_asymmetric_half_lives() {
    let mut r = request(2);
    for s in &mut r.segments {
        s.opening_paid_rate = 0.02;
        s.repricing.target = TargetRule::Linear {
            intercept: 0.,
            beta: 1.,
            competitor_weight: 0.,
        };
        s.repricing.rising_half_life_days = 1.;
        s.repricing.falling_half_life_days = 2.;
    }
    r.drivers[1].reference_rate = 0.04;
    r.drivers[2].reference_rate = 0.;
    let out = simulate(&r).unwrap();
    near(out.points[1].paid_rates[0], 0.03);
    near(out.points[2].paid_rates[0], 0.03 * 0.5_f64.sqrt());
    near(
        out.interest.iter().map(|v| v.amount).sum(),
        1000. * (0.03 + 0.03 * 0.5_f64.sqrt()) / 365.,
    );
}
#[test]
fn explicit_observation_lag_requires_real_history_and_does_not_look_ahead() {
    let mut r = request(2);
    for s in &mut r.segments {
        s.repricing.observation_lag_days = 1;
        s.repricing.target = TargetRule::Linear {
            intercept: 0.,
            beta: 1.,
            competitor_weight: 0.,
        };
    }
    let mut older = r.drivers[0].clone();
    older.ordinal -= 1;
    r.drivers.insert(0, older);
    r.drivers[1].reference_rate = 0.02;
    r.drivers[2].reference_rate = 0.08;
    let out = simulate(&r).unwrap();
    near(out.points[1].paid_rates[0], 0.02);
    near(out.points[2].paid_rates[0], 0.08);
    r.drivers.remove(0);
    assert!(simulate(&r).is_err());
}
#[test]
fn floors_ceilings_competitor_premium_and_spread_change_actual_paid_rates() {
    let mut r = request(2);
    for s in &mut r.segments {
        s.opening_paid_rate = 0.02;
        s.repricing.floor = 0.005;
        s.repricing.ceiling = 0.05;
        s.repricing.target = TargetRule::Linear {
            intercept: 0.,
            beta: 0.5,
            competitor_weight: 0.5,
        };
    }
    r.drivers[1].reference_rate = 0.04;
    r.drivers[1].competitor_rates = vec![0.06; 2];
    r.drivers[1].offered_spread_stress = vec![0.01; 2];
    r.drivers[2].reference_rate = -0.1;
    let out = simulate(&r).unwrap();
    near(out.points[1].paid_rates[0], 0.04);
    near(out.points[2].paid_rates[0], 0.005);
    r.drivers[2].reference_rate = 0.2;
    r.drivers[2].competitor_rates = vec![0.2; 2];
    near(simulate(&r).unwrap().points[2].paid_rates[0], 0.05);
}
#[test]
fn logistic_beta_midpoint_is_independent_hand_formula_and_regime_is_explicit() {
    let mut r = request(1);
    for s in &mut r.segments {
        s.opening_paid_rate = 0.02;
        s.repricing.target = TargetRule::LogisticBeta {
            intercept: 0.,
            beta_min: 0.2,
            beta_max: 0.8,
            midpoint: 0.04,
            steepness: 1000.,
            competitor_weight: 0.,
        };
        s.repricing.supported_observation_range = Some(ObservedRange {
            history_revision: "declared-range-1".into(),
            minimum_reference_rate: 0.,
            maximum_reference_rate: 0.03,
            logistic_plateau_identified: false,
        });
    }
    r.drivers[1].reference_rate = 0.04;
    let out = simulate(&r).unwrap();
    near(out.points[1].paid_rates[0], 0.02);
    assert_eq!(
        out.points[1].outside_supported_rate_regime,
        vec![true, true]
    );
    assert_eq!(out.points[1].rate_regime_assessed, vec![true, true]);
    assert_eq!(out.parameter_status, "supplied_assumption");
}
#[test]
fn opportunity_gap_responds_to_repricing_and_confidence_features_once() {
    let mut r = request(1);
    for s in &mut r.segments {
        s.opening_paid_rate = 0.02;
        s.repricing.target = TargetRule::Linear {
            intercept: 0.02,
            beta: 0.,
            competitor_weight: 0.,
        };
        s.retention.opportunity_gap_sensitivity = 100.;
        s.retention.uninsured_stress_sensitivity = 10.;
        s.retention.relationship_protection = 2.;
    }
    r.drivers[1].competitor_rates = vec![0.04; 2];
    r.drivers[1].confidence_stress = vec![1.; 2];
    let out = simulate(&r).unwrap();
    near(
        out.total_withdrawals,
        1000. * (1. - (-6.0_f64 / 365.).exp()),
    );
    r.drivers[1].offered_spread_stress = vec![0.02; 2];
    let offered = simulate(&r).unwrap();
    near(
        offered.total_withdrawals,
        1000. * (1. - (-4.0_f64 / 365.).exp()),
    );
    near(offered.points[1].paid_rates[0], 0.04);
}
#[test]
fn true_contractual_maturity_is_distinct_from_terminal_assumption() {
    let mut r = request(2);
    r.segments[0].maturity_ordinal = Some(r.as_of_ordinal + 1);
    let out = simulate(&r).unwrap();
    near(out.total_contractual_maturities, 1000.);
    near(out.total_assumed_terminal_payout, 0.);
    near(out.closing_balance, 0.);
    let cash = out
        .cashflows
        .iter()
        .find(|v| v.kind == "contractual_maturity")
        .unwrap();
    assert_eq!(cash.ordinal, r.as_of_ordinal + 1);
    near(cash.cash_interest, -1.);
    r.drivers[2].external_inflows[0] = 1.;
    assert!(simulate(&r).is_err());
}
#[test]
fn residual_franchise_and_liquidation_are_explicit_and_franchise_is_not_cash() {
    let mut r = request(1);
    let retained = simulate(&r).unwrap();
    r.terminal = TerminalPolicy::FranchiseValueOnly {
        supplied_value: 100.,
        rationale: "Supplied separate discounted continuation valuation; no payout".into(),
    };
    let franchise = simulate(&r).unwrap();
    assert_eq!(franchise.cashflows, retained.cashflows);
    near(franchise.closing_balance, 1000.);
    assert_eq!(franchise.supplied_franchise_value, Some(100.));
    r.terminal = TerminalPolicy::AssumedLiquidation {
        rationale: "Analytical horizon liquidation scenario".into(),
    };
    let closed = simulate(&r).unwrap();
    near(closed.closing_balance, 0.);
    near(closed.total_assumed_terminal_payout, 1000.);
    assert!(closed
        .cashflows
        .iter()
        .any(|v| v.kind == "assumed_terminal_payout"));
}
#[test]
fn horizon_extension_preserves_prefix_and_quantifies_retained_interest() {
    let r = request(1);
    let short = simulate(&r).unwrap();
    let mut longer = request(2);
    for s in &mut longer.segments {
        s.interest_payment_ordinals = vec![longer.as_of_ordinal + 1, longer.as_of_ordinal + 2];
    }
    let long = simulate(&longer).unwrap();
    assert_eq!(short.points[1], long.points[1]);
    near(short.closing_balance, long.closing_balance);
    near(long.interest.iter().map(|v| v.amount).sum(), 2.);
}
#[test]
fn zero_shock_same_policy_replay_is_exact_and_pricing_measure_never_posts() {
    let r = request(2);
    let out = simulate(&r).unwrap();
    assert_eq!(
        serde_json::to_vec(&out).unwrap(),
        serde_json::to_vec(&simulate(&r.clone()).unwrap()).unwrap()
    );
    let mut q = r;
    q.identity.measure = CashflowMeasure::Pricing;
    assert!(simulate(&q).is_err());
}
#[test]
fn malformed_domains_sources_overlap_and_unknown_fields_fail_closed() {
    let r = request(1);
    let mut bad = r.clone();
    bad.segments[0].opening_balance = f64::NAN;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.segments[1].segment_id = bad.segments[0].segment_id.clone();
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.drivers[1].ordinal -= 1;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.segments[0].repricing.ceiling = -1.;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.segments[0].retention.withdrawal_per_year = 101.;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.migrations.push(MigrationPolicy {
        from_segment: "segment-0".into(),
        to_segment: "missing".into(),
        base_per_year: 1.,
        destination_rate_gap_sensitivity: 0.,
    });
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.identity.assumption_rationale.clear();
    assert!(simulate(&bad).is_err());
    let mut raw = serde_json::to_value(r).unwrap();
    raw["calibrated_retention"] = serde_json::json!(true);
    assert!(serde_json::from_value::<DepositStateRequest>(raw).is_err());
    assert!(serde_json::from_value::<TerminalPolicy>(
        serde_json::json!({"kind":"retain_residual","silent_payout":true})
    )
    .is_err());
}
#[test]
fn monetary_work_history_and_migration_duplicates_have_bounded_admission() {
    let mut r = request(1);
    r.segments[0].opening_balance = 1e15;
    r.segments[1].opening_balance = 1e15;
    assert!(simulate(&r).is_err());
    r = request(1);
    r.drivers[0].external_inflows[0] = 1.;
    assert!(simulate(&r).is_err());
    r = request(1);
    r.migrations = vec![
        MigrationPolicy {
            from_segment: "segment-0".into(),
            to_segment: "segment-1".into(),
            base_per_year: 1.,
            destination_rate_gap_sensitivity: 0.
        };
        2
    ];
    assert!(simulate(&r).is_err());
    r = request(36600);
    assert!(simulate(&r).is_err());
}

#[test]
fn signed_negative_rates_cash_and_accrued_rights_reconcile() {
    let mut r = request(2);
    for s in &mut r.segments {
        s.opening_paid_rate = -0.0365;
        s.repricing.floor = -0.1;
        s.repricing.target = TargetRule::Linear {
            intercept: -0.0365,
            beta: 0.,
            competitor_weight: 0.,
        };
        s.opening_accrued_interest = -0.5;
    }
    let out = simulate(&r).unwrap();
    near(out.total_interest_expense, -0.2);
    near(out.total_bank_cash_interest, 1.2);
    near(out.closing_accrued_interest, 0.);
    near(out.closing_balance, 1000.);
}
#[test]
fn partial_withdrawal_settles_same_share_of_opening_accrued_rights() {
    let mut r = request(1);
    no_interest(&mut r);
    r.segments[0].opening_accrued_interest = 20.;
    r.segments[0].interest_payment_ordinals.clear();
    r.segments[0].retention.withdrawal_per_year = 50.;
    let out = simulate(&r).unwrap();
    let stay = (-50.0_f64 / 365.).exp();
    near(out.total_withdrawals, 1000. * (1. - stay));
    near(out.closing_accrued_interest, 20. * stay);
    near(out.total_bank_cash_interest, -20. * (1. - stay));
    near(
        20. + out.total_interest_expense + out.total_bank_cash_interest,
        out.closing_accrued_interest,
    );
}
#[test]
fn daily_reference_velocity_is_explicit_and_logistic_extremes_are_stable() {
    let mut r = request(1);
    no_interest(&mut r);
    r.segments[0].retention.absolute_daily_velocity_sensitivity = 100.;
    r.drivers[0].reference_rate = 0.01;
    r.drivers[1].reference_rate = 0.04;
    let out = simulate(&r).unwrap();
    near(
        out.total_withdrawals,
        1000. * (1. - (-3.0_f64 / 365.).exp()),
    );
    r.segments[0].repricing.target = TargetRule::LogisticBeta {
        intercept: 0.,
        beta_min: 0.,
        beta_max: 1.,
        midpoint: -0.2,
        steepness: 10000.,
        competitor_weight: 0.,
    };
    r.drivers[0].reference_rate = 2.;
    r.drivers[1].reference_rate = 2.;
    near(simulate(&r).unwrap().points[1].paid_rates[0], 1.);
}
#[test]
fn matured_destination_has_no_synthetic_reinvestment_or_migration() {
    let mut r = request(2);
    no_interest(&mut r);
    r.segments[1].maturity_ordinal = Some(r.as_of_ordinal + 1);
    r.migrations.push(MigrationPolicy {
        from_segment: "segment-0".into(),
        to_segment: "segment-1".into(),
        base_per_year: 50.,
        destination_rate_gap_sensitivity: 0.,
    });
    let out = simulate(&r).unwrap();
    assert_eq!(out.migrations.len(), 1);
    near(
        out.total_contractual_maturities,
        out.migrations[0].principal,
    );
    near(out.points[2].balances[1], 0.);
    near(
        out.closing_balance + out.total_contractual_maturities,
        1000.,
    );
}
