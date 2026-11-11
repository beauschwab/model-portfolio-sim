//! Hand-computed and synthetic fixtures for `consumer-auto-collateral-1`. None is empirical evidence.
use portfolio_model_core::consumer_auto_collateral::*;
use portfolio_model_core::credit::{evaluate, CreditMeasure, CreditState};
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::credit_calibration::{DataKind, ObservationMode, ObservationTime};
use portfolio_model_core::credit_covariate::{
    fit, Column, CovariateFitRequest, DriverKind, Edge, Ending, FeatureRole, FittedModel,
    Optimizer, OriginModel, Parameter, ProjectionUse, ResolvedRow, Segment,
    SCHEMA as COVARIATE_SCHEMA,
};

const BASE: i32 = 738_000;

fn id(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "1".into(),
    }
}
fn at(day: i32) -> ObservationTime {
    ObservationTime {
        ordinal: BASE + day,
        seconds_of_day: 0,
    }
}
fn near(a: f64, b: f64, tolerance: f64) {
    assert!((a - b).abs() <= tolerance, "{a} != {b} within {tolerance}");
}
fn p(initial: f64, lower: f64, upper: f64) -> Parameter {
    Parameter {
        initial,
        lower,
        upper,
        ridge_weight: 0.,
        ridge_center: 0.,
    }
}

/// A new vehicle worth 30,000 at origination; 2% mileage and 3% condition haircuts.
fn collateral() -> Collateral {
    Collateral {
        origination_value: 30_000.,
        origination: at(0),
        segment: VehicleSegment::New,
        vehicle_class: Some("suv".into()),
        depreciation: DepreciationCurve {
            review: id("new-suv-curve"),
            segment: VehicleSegment::New,
            vehicle_class: Some("suv".into()),
            knots: [(0., 1.), (12., 0.8), (36., 0.55), (60., 0.4), (72., 0.34)]
                .map(|(age_months, retained_fraction)| DepreciationKnot {
                    age_months,
                    retained_fraction,
                })
                .to_vec(),
        },
        index_at_origination: 100.,
        mileage_haircut: 0.02,
        condition_haircut: 0.03,
        haircut_review: id("inspection-policy"),
    }
}
fn policy() -> RecoveryPolicy {
    RecoveryPolicy {
        review: id("recovery-policy"),
        recovery_haircut: 0.9,
        repossession_cost: 600.,
        remarketing_cost: 400.,
        recovery_lag_days: 60,
        deficiency_recovery_fraction: 0.2,
        deficiency_lag_days: 240,
    }
}

#[test]
fn collateral_value_is_curve_times_index_times_haircuts() {
    let c = collateral();
    near(age_months(&c, at(730)).unwrap(), 24., 1e-12);
    near(retained_fraction(&c.depreciation, 0.).unwrap(), 1., 0.);
    near(
        retained_fraction(&c.depreciation, 24.).unwrap(),
        0.675,
        1e-15,
    ); // halfway from 0.8 to 0.55
    near(retained_fraction(&c.depreciation, 72.).unwrap(), 0.34, 0.);
    // 30,000 x 0.675 x 1.0 x 0.98 x 0.97
    near(
        collateral_value(&c, at(730), 100.).unwrap(),
        19_249.65,
        1e-9,
    );
    // The index scales value relative to its level at origination.
    near(collateral_value(&c, at(730), 80.).unwrap(), 15_399.72, 1e-9);
    near(
        collateral_value(&c, at(730), 110.).unwrap(),
        21_174.615,
        1e-9,
    );
    assert!(collateral_value(&c, at(-1), 100.)
        .unwrap_err()
        .contains("before origination"));
    assert!(collateral_value(&c, at(73 * 30 + 1), 100.)
        .unwrap_err()
        .contains("outside the supplied depreciation curve"));
    assert!(collateral_value(&c, at(730), 0.).is_err());
    let mut other = collateral();
    other.depreciation.segment = VehicleSegment::Used;
    assert!(collateral_value(&other, at(730), 100.)
        .unwrap_err()
        .contains("different vehicle segment"));
    let mut unordered = collateral();
    unordered.depreciation.knots.swap(1, 2);
    assert!(collateral_value(&unordered, at(730), 100.).is_err());
    let mut haircut = collateral();
    haircut.mileage_haircut = 1.;
    assert!(collateral_value(&haircut, at(730), 100.).is_err());
}

#[test]
fn hand_computed_loss_given_default() {
    // Deep equity: 0.9 x 20,000 = 18,000 of proceeds covers 10,000 of exposure and 1,000 of costs.
    let deep = loss_given_default(10_000., 20_000., &policy()).unwrap();
    assert_eq!(
        (deep.loss, deep.lgd, deep.deficiency_balance),
        (0., Some(0.), 0.)
    );
    assert_eq!(deep.collateral_recovery, 10_000.);
    assert_eq!(deep.surplus_to_borrower, 7_000.);

    // Underwater: exposure minus haircut value plus costs = 20,000 - 10,800 + 1,000.
    let mut no_tail = policy();
    no_tail.deficiency_recovery_fraction = 0.;
    let under = loss_given_default(20_000., 12_000., &no_tail).unwrap();
    near(under.loss, 10_200., 1e-9);
    near(under.lgd.unwrap(), 0.51, 1e-12);
    near(under.collateral_recovery, 9_800., 1e-9);
    assert_eq!(under.surplus_to_borrower, 0.);

    // A 20% deficiency recovery tail collects 2,040 of that 10,200.
    let tail = loss_given_default(20_000., 12_000., &policy()).unwrap();
    near(tail.deficiency_balance, 10_200., 1e-9);
    near(tail.deficiency_recovery, 2_040., 1e-9);
    near(tail.loss, 8_160., 1e-9);
    near(tail.lgd.unwrap(), 0.408, 1e-12);
    near(
        tail.exposure,
        tail.collateral_recovery + tail.deficiency_recovery + tail.loss,
        1e-9,
    );

    // Lags are cash timing: they do not enter the loss.
    let mut slow = policy();
    (slow.recovery_lag_days, slow.deficiency_lag_days) = (400, 2_000);
    assert_eq!(loss_given_default(20_000., 12_000., &slow).unwrap(), tail);

    // Costs above proceeds are a loss above exposure, reported and never clipped.
    let wreck = loss_given_default(5_000., 500., &no_tail).unwrap();
    near(wreck.loss, 5_550., 1e-9);
    assert!(wreck.collateral_recovery < 0. && wreck.lgd.unwrap() > 1.);
    assert_eq!(loss_given_default(0., 500., &policy()).unwrap().lgd, None);
    assert!(loss_given_default(-1., 500., &policy()).is_err());
    let mut bad = policy();
    bad.recovery_haircut = 0.;
    assert!(loss_given_default(1., 1., &bad).is_err());
    near(
        equity_ratio(19_249.65, 18_000., 2.).unwrap(),
        1.069_425,
        1e-12,
    );
    assert_eq!(equity_ratio(90_000., 100., 2.).unwrap(), 2.);
    assert!(equity_ratio(1., 0., 2.).is_err());
}

/// Two equity groups of ten account-years: at equity 0.8 four default and one prepays; at 1.4 one
/// defaults and four prepay. The exact MLE is 0.4 and 0.1 a year against 0.1 and 0.4.
fn fitted(equity_effect: bool, with_column: bool) -> FittedModel {
    let slope = || {
        if equity_effect {
            p(0., -1.5, 1.5)
        } else {
            p(0., 0., 0.)
        }
    };
    let edge = |destination| Edge {
        destination,
        intercept: p(-1., -6., 0.),
        covariate_coefficients: if with_column { vec![slope()] } else { vec![] },
        term_coefficients: vec![],
    };
    let zero = || OriginModel::FixedRow {
        hazards_per_year: [0.; 5],
        review: id("no-watch-or-delinquent-history"),
        rationale: "Explicit zero rows for origins absent from the fixture".into(),
    };
    let mut segments = Vec::new();
    for (group, equity, defaults, prepays) in [("low", 0.8, 4, 1), ("high", 1.4, 1, 4)] {
        for account in 0..10 {
            let name = format!("{group}-{account}");
            segments.push(Segment {
                segment_id: name.clone(),
                account_id: name,
                source: id("auto-training"),
                start: at(0),
                end: at(365),
                origin: CreditState::Performing,
                ending: if account < defaults {
                    Ending::Transition {
                        destination: CreditState::Default,
                    }
                } else if account < defaults + prepays {
                    Ending::Transition {
                        destination: CreditState::Prepaid,
                    }
                } else {
                    Ending::RightCensored {}
                },
                available_time: at(0),
                covariates: if with_column { vec![equity] } else { vec![] },
                term_level: "reference".into(),
            });
        }
    }
    let request = CovariateFitRequest {
        schema: COVARIATE_SCHEMA.into(),
        measure: CreditMeasure::Physical,
        data_kind: DataKind::Synthetic,
        observation_mode: ObservationMode::ExactTransitionTimes,
        population: id("synthetic-auto"),
        model_policy: id("auto-policy"),
        training_source: id("auto-training"),
        holdout_source: id("auto-holdout"),
        training_split: id("train"),
        holdout_split: id("holdout"),
        train_start: at(0),
        train_end: at(365),
        holdout_end: at(730),
        columns: if with_column {
            vec![Column {
                name: "borrower_equity".into(),
                role: FeatureRole::Borrower,
                unit: "collateral value / balance".into(),
                offset: 0.8,
                scale: 0.6,
                raw_lower: 0.2,
                raw_upper: 2.,
                scaling_review: id("declared-scaling"),
            }]
        } else {
            vec![]
        },
        term_levels: vec!["reference".into()],
        rows: [
            OriginModel::LogLinear {
                edges: vec![edge(CreditState::Default), edge(CreditState::Prepaid)],
            },
            zero(),
            zero(),
        ],
        optimizer: Optimizer {
            max_iterations: 200,
            max_backtracks: 48,
            timeout_ms: 60_000,
            projected_gradient_tolerance: 1e-10,
            relative_information_tolerance: 1e-9,
        },
        segments,
    };
    fit(&request).unwrap().model
}

/// Twelve 30-day intervals from month 24 on a balance amortising from 18,000.
fn request(model: FittedModel, index: f64) -> ProjectionRequest {
    let width = model.columns.len();
    ProjectionRequest {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        monetary_unit: "USD".into(),
        equity_column: (width > 0).then(|| "borrower_equity".into()),
        model,
        model_revision: id("frozen-auto-model"),
        review: id("analytical-review"),
        use_policy: ProjectionUse::AnalyticalOnly,
        position_id: "auto-1".into(),
        scenario_id: format!("index-{index}"),
        timeout_ms: 60_000,
        initial_live_probability: [1., 0., 0.],
        collateral: collateral(),
        recovery: policy(),
        equity_cap: 2.,
        intervals: (0..12)
            .map(|i| AutoInterval {
                interval_id: format!("m{i}"),
                start: at(730 + 30 * i),
                end: at(760 + 30 * i),
                available_time: at(730),
                kind: DriverKind::SuppliedProjected,
                source: id("driver-tape"),
                balance: 18_000. - 400. * f64::from(i),
                used_vehicle_index: index,
                covariates: vec![0.; width],
                term_level: "reference".into(),
            })
            .collect(),
    }
}
fn performing(model: &FittedModel) -> Vec<(CreditState, f64, f64)> {
    match &model.rows[0] {
        ResolvedRow::LogLinear { edges } => edges
            .iter()
            .map(|e| {
                (
                    e.destination,
                    e.intercept.value,
                    e.covariate_coefficients.first().map_or(0., |c| c.value),
                )
            })
            .collect(),
        _ => panic!("fitted performing row"),
    }
}

#[test]
fn competing_default_and_prepayment_conserve_probability() {
    let out = project(&request(fitted(true, true), 100.)).unwrap();
    let (mut defaulted, mut prepaid) = (0., 0.);
    for period in &out.credit.periods {
        near(period.state_probability.iter().sum::<f64>(), 1., 1e-12);
        defaulted += period.marginal_default_probability;
        prepaid += period.marginal_prepayment_probability;
        near(period.state_probability[3], defaulted, 1e-12);
        near(period.state_probability[4], prepaid, 1e-12);
    }
    let last = out.credit.periods.last().unwrap();
    near(last.state_probability[0] + defaulted + prepaid, 1., 1e-12);
    assert!(defaulted > 0.05 && prepaid > 0.05 && last.state_probability[0] > 0.5);
    assert_eq!(out.intervals.len(), 12);
    assert!(out.analytical_only && !out.adoption_output && !out.empirical_claim);
}

#[test]
fn zero_coefficient_drivers_reproduce_the_constant_row_model_bit_for_bit() {
    let model = fitted(false, true);
    let rows = performing(&model);
    assert!(rows.iter().all(|(_, _, slope)| *slope == 0.));
    let mut constant = [[0.; 5]; 3];
    for (destination, intercept, _) in &rows {
        constant[0][if *destination == CreditState::Default {
            3
        } else {
            4
        }] = intercept.exp();
    }
    near(constant[0][3], 0.25, 1e-9);
    // Equity moves a great deal with the index; with zero coefficients the hazards must not move at all.
    let (base, shocked) = (
        project(&request(model.clone(), 100.)).unwrap(),
        project(&request(model, 60.)).unwrap(),
    );
    assert!(base.intervals[0].equity > shocked.intervals[0].equity * 1.5);
    for out in [&base, &shocked] {
        for interval in &out.intervals {
            for (a, b) in interval
                .hazards_per_year
                .iter()
                .flatten()
                .zip(constant.iter().flatten())
            {
                assert_eq!(a.to_bits(), b.to_bits());
            }
        }
        // The constant-row credit model with the same exposure and loss terms is identical in every bit.
        let mut by_hand = out.credit_request.clone();
        for period in &mut by_hand.periods {
            period.transition_hazards_per_year = constant;
        }
        let expected = evaluate(&by_hand).unwrap();
        for (a, b) in out.credit.periods.iter().zip(&expected.periods) {
            for (x, y) in a.state_probability.iter().zip(&b.state_probability) {
                assert_eq!(x.to_bits(), y.to_bits());
            }
            assert_eq!(a.expected_loss.to_bits(), b.expected_loss.to_bits());
            assert_eq!(
                a.marginal_prepayment_probability.to_bits(),
                b.marginal_prepayment_probability.to_bits()
            );
        }
        assert_eq!(
            out.credit.total_expected_loss.to_bits(),
            expected.total_expected_loss.to_bits()
        );
    }
    // The column-free constant fit gives the same rates to estimator tolerance.
    for ((_, a, _), (_, b, _)) in performing(&fitted(false, false)).iter().zip(&rows) {
        near(*a, *b, 1e-9);
    }
}

/// Deterministic 48-bit LCG for shared draws.
struct Lcg(u64);
impl Lcg {
    fn uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1 << 48) - 1);
        (self.0 >> 16) as f64 / (1u64 << 32) as f64
    }
}

#[test]
fn used_vehicle_index_shock_raises_loss_and_default_and_lowers_prepayment_on_shared_draws() {
    let model = fitted(true, true);
    // Estimator tolerance 1e-6: default 0.4 -> 0.1 and prepayment 0.1 -> 0.4 as equity goes 0.8 -> 1.4.
    for (destination, intercept, slope) in performing(&model) {
        let (rate, effect) = if destination == CreditState::Default {
            (0.4_f64, 0.25_f64)
        } else {
            (0.1, 4.)
        };
        near(intercept, rate.ln(), 1e-6);
        near(slope, effect.ln(), 1e-6);
    }
    let (base, shocked) = (
        project(&request(model.clone(), 100.)).unwrap(),
        project(&request(model, 80.)).unwrap(),
    );
    for (b, s) in base.intervals.iter().zip(&shocked.intervals) {
        near(
            s.collateral_value_start,
            0.8 * b.collateral_value_start,
            1e-9,
        );
        near(s.equity, 0.8 * b.equity, 1e-12);
        // Closed form on the derived equity covariate, relative tolerance 1e-5.
        for (out, column, rate, effect) in [
            (b, 3, 0.4_f64, 0.25_f64),
            (b, 4, 0.1, 4.),
            (s, 3, 0.4, 0.25),
            (s, 4, 0.1, 4.),
        ] {
            let expected = rate * effect.powf((out.equity - 0.8) / 0.6);
            assert!((out.hazards_per_year[0][column] / expected - 1.).abs() < 1e-5);
        }
        assert!(
            s.hazards_per_year[0][3] > b.hazards_per_year[0][3],
            "default hazard rises"
        );
        assert!(
            s.hazards_per_year[0][4] < b.hazards_per_year[0][4],
            "prepayment hazard falls"
        );
        assert!(s.loss.loss > b.loss.loss, "loss given default rises");
        // Each index point is worth haircut x value: the loss moves by exactly the lost proceeds.
        near(
            s.loss.deficiency_balance - b.loss.deficiency_balance,
            0.9 * 0.2 * b.collateral_value_at_default,
            1e-8,
        );
    }
    let cumulative = |out: &ProjectionResult| {
        let last = out.credit.periods.last().unwrap();
        (last.state_probability[3], last.state_probability[4])
    };
    let ((base_default, base_prepaid), (shock_default, shock_prepaid)) =
        (cumulative(&base), cumulative(&shocked));
    assert!(shock_default > base_default * 1.2 && shock_prepaid < base_prepaid * 0.8);
    assert!(shocked.credit.total_expected_loss > base.credit.total_expected_loss * 2.);

    // The same 2,000 uniform pairs in both scenarios: only the hazards differ.
    let exits = |out: &ProjectionResult| {
        let (mut rng, mut defaults, mut prepays, mut paths) = (Lcg(94), 0, 0, Vec::new());
        for _ in 0..2_000 {
            let exit = shared_draw_exit(&out.intervals, rng.uniform(), rng.uniform()).unwrap();
            match exit {
                Some((_, ExitCause::Default)) => defaults += 1,
                Some((_, ExitCause::Prepaid)) => prepays += 1,
                None => {}
            }
            paths.push(exit);
        }
        (defaults, prepays, paths)
    };
    let ((base_d, base_p, base_paths), (shock_d, shock_p, _)) = (exits(&base), exits(&shocked));
    assert!(
        shock_d > base_d && shock_p < base_p,
        "{base_d}/{base_p} -> {shock_d}/{shock_p}"
    );
    // Draw frequencies agree with the exact probabilities within three binomial standard errors.
    for (count, probability) in [
        (base_d, base_default),
        (base_p, base_prepaid),
        (shock_d, shock_default),
        (shock_p, shock_prepaid),
    ] {
        let error = 3. * (probability * (1. - probability) / 2_000_f64).sqrt();
        near(f64::from(count) / 2_000., probability, error);
    }
    assert_eq!(exits(&base).2, base_paths, "shared draws are reproducible");
    assert!(shared_draw_exit(&base.intervals, 1., 0.).is_err());
}

#[test]
fn recovery_lag_shifts_cash_not_loss_and_feeds_the_credit_model() {
    let model = fitted(true, true);
    let fast = project(&request(model.clone(), 80.)).unwrap();
    let mut slow_request = request(model, 80.);
    (
        slow_request.recovery.recovery_lag_days,
        slow_request.recovery.deficiency_lag_days,
    ) = (60 + 365, 240 + 365);
    let slow = project(&slow_request).unwrap();
    assert_eq!(
        fast.credit.total_expected_loss.to_bits(),
        slow.credit.total_expected_loss.to_bits()
    );
    assert_eq!(
        fast.credit.total_eventual_nominal_recovery.to_bits(),
        slow.credit.total_eventual_nominal_recovery.to_bits()
    );
    assert_eq!(fast.credit.recoveries.len(), slow.credit.recoveries.len());
    for (a, b) in fast.credit.recoveries.iter().zip(&slow.credit.recoveries) {
        near(b.payment_year - a.payment_year, 1., 1e-12);
        assert_eq!(a.amount.to_bits(), b.amount.to_bits());
    }
    // Term LGD and recovery timing reach credit-model-1 as supplied terms.
    for (interval, period) in fast.intervals.iter().zip(&fast.credit_request.periods) {
        assert_eq!(period.loss_given_default, [interval.loss.lgd.unwrap(); 3]);
        assert_eq!(period.exposure_at_default[0].drawn, interval.balance);
        assert_eq!(period.recovery_distribution.len(), 2);
        near(period.recovery_distribution[0].lag_years, 60. / 365., 1e-15);
        near(
            period.recovery_distribution[1].lag_years,
            240. / 365.,
            1e-15,
        );
        let recovered = interval.loss.collateral_recovery + interval.loss.deficiency_recovery;
        near(
            period.recovery_distribution[0].weight,
            interval.loss.collateral_recovery / recovered,
            1e-15,
        );
        near(
            period
                .recovery_distribution
                .iter()
                .map(|r| r.weight)
                .sum::<f64>(),
            1.,
            1e-15,
        );
        near(period.duration_years, 30. / 365., 1e-15);
    }
    // With deep equity nothing is lost and a single collateral payment remains.
    let rich = project(&request(fitted(true, true), 170.)).unwrap();
    assert_eq!(rich.credit.total_expected_loss, 0.);
    assert!(rich
        .intervals
        .iter()
        .all(|i| i.loss.lgd == Some(0.) && i.recovery_distribution.len() == 1 && i.equity <= 2.));
}

#[test]
fn synthetic_fits_cannot_be_adopted_and_contract_violations_reject() {
    let model = fitted(true, true);
    assert!(!model.adoption_eligible);
    let good = request(model, 100.);
    let rejects = |change: &dyn Fn(&mut ProjectionRequest), text: &str| {
        let mut r = good.clone();
        change(&mut r);
        let error = project(&r).unwrap_err();
        assert!(error.contains(text), "{error}");
    };
    rejects(&|r| r.use_policy = ProjectionUse::Adoption, "");
    rejects(&|r| r.schema = "consumer-installment-1".into(), "schema");
    rejects(
        &|r| r.equity_column = Some("ltv".into()),
        "unknown equity column",
    );
    rejects(&|r| r.intervals[3].covariates = vec![1.2], "derived");
    rejects(&|r| r.intervals[3].balance = 0., "positive");
    rejects(&|r| r.intervals[3].used_vehicle_index = -5., "index");
    rejects(&|r| r.intervals.clear(), "1..240");
    rejects(&|r| r.intervals[5].start = at(999), "");
    // An index collapse leaves costs above proceeds: an expense the credit model cannot carry.
    rejects(
        &|r| {
            r.equity_column = None;
            r.model = fitted(false, false);
            r.intervals
                .iter_mut()
                .for_each(|i| (i.used_vehicle_index, i.covariates) = (2., vec![]));
        },
        "costs exceed sale proceeds",
    );
    // Equity below the fitted column's declared domain is refused, not clipped.
    rejects(
        &|r| {
            r.intervals
                .iter_mut()
                .for_each(|i| i.used_vehicle_index = 15.)
        },
        "",
    );
    let json = serde_json::to_value(&good).unwrap();
    assert!(serde_json::from_value::<ProjectionRequest>(json.clone()).is_ok());
    let mut extra = json;
    extra["recovery"]["inferred_recovery"] = serde_json::json!(0.5);
    assert!(serde_json::from_value::<ProjectionRequest>(extra).is_err());
}
