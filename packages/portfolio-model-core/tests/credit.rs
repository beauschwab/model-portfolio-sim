use portfolio_model_core::credit::*;

fn exposure(drawn: f64) -> ExposureAtDefault {
    ExposureAtDefault {
        drawn,
        undrawn: 0.0,
        credit_conversion_factor: 0.0,
    }
}

fn period() -> CreditPeriod {
    CreditPeriod {
        duration_years: 1.0,
        transition_hazards_per_year: [[0.0; 5]; 3],
        exposure_at_default: [exposure(100.0), exposure(100.0), exposure(100.0)],
        loss_given_default: [0.4; 3],
        recovery_distribution: vec![RecoveryPoint {
            lag_years: 0.0,
            weight: 1.0,
        }],
    }
}

fn request(periods: Vec<CreditPeriod>) -> CreditRequest {
    CreditRequest {
        schema: SCHEMA.into(),
        monetary_unit: "USD".into(),
        initial_live_probability: [1.0, 0.0, 0.0],
        periods,
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2.0e-12 * expected.abs().max(1.0),
        "actual {actual:.17e}, expected {expected:.17e}"
    );
}

#[test]
fn zero_hazards_preserve_live_identity_and_have_no_losses() {
    let input = request(vec![period(); 3]);
    let result = evaluate(&input).unwrap();
    for row in result.periods {
        assert_eq!(row.state_probability, [1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(row.expected_loss, 0.0);
    }
    assert!(result.recoveries.is_empty());
    assert_eq!(result.total_expected_loss, 0.0);
}

#[test]
fn competing_default_prepay_match_closed_form_without_double_counting() {
    let mut term = period();
    term.transition_hazards_per_year[0][3] = 0.2;
    term.transition_hazards_per_year[0][4] = 0.3;
    let result = evaluate(&request(vec![term])).unwrap();
    // Direct integrating exp(-(.2+.3)t) * each cause intensity, independent oracle.
    let survival = (-0.5_f64).exp();
    let default = 0.2 / 0.5 * (1.0 - survival);
    let prepaid = 0.3 / 0.5 * (1.0 - survival);
    let row = &result.periods[0];
    close(row.state_probability[0], survival);
    close(row.marginal_default_probability, default);
    close(row.marginal_prepayment_probability, prepaid);
    close(row.state_probability.iter().sum(), 1.0);
    close(row.defaulted_exposure, 100.0 * default);
    close(row.expected_loss, 40.0 * default);
    close(result.total_eventual_nominal_recovery, 60.0 * default);
    assert!(default < 1.0 - (-0.2_f64).exp());
}

#[test]
fn annual_pd_conversion_and_piecewise_marginal_pd_are_distinct() {
    let hazard = annual_pd_to_hazard(0.1).unwrap();
    close(hazard, -(0.9_f64).ln());
    let mut first = period();
    first.transition_hazards_per_year[0][3] = hazard;
    let mut second = period();
    second.transition_hazards_per_year[0][3] = annual_pd_to_hazard(0.2).unwrap();
    let result = evaluate(&request(vec![first, second])).unwrap();
    close(result.periods[0].marginal_default_probability, 0.1);
    close(result.periods[1].marginal_default_probability, 0.9 * 0.2);
    close(
        result.periods[1].conditional_default_probability.unwrap(),
        0.2,
    );
    close(result.periods[1].state_probability[3], 0.28);
}

#[test]
fn cures_and_migrations_allow_multiple_jumps_in_an_interval() {
    let mut term = period();
    term.transition_hazards_per_year[0][1] = 0.4;
    term.transition_hazards_per_year[1][0] = 0.6;
    let result = evaluate(&request(vec![term.clone()])).unwrap();
    // Two-state CTMC: P(watch at t)=a/(a+b)*(1-exp(-(a+b)t)).
    let watch = 0.4 * (1.0 - (-1.0_f64).exp());
    close(result.periods[0].state_probability[1], watch);
    close(result.periods[0].state_probability[0], 1.0 - watch);
    let mut starting_watch = request(vec![term]);
    starting_watch.initial_live_probability = [0.0, 1.0, 0.0];
    let result = evaluate(&starting_watch).unwrap();
    close(
        result.periods[0].state_probability[0],
        0.6 * (1.0 - (-1.0_f64).exp()),
    );
}

#[test]
fn sequential_migration_default_matches_erlang_and_origin_ead() {
    let mut term = period();
    term.transition_hazards_per_year[0][2] = 1.0;
    term.transition_hazards_per_year[2][3] = 1.0;
    term.exposure_at_default[2] = ExposureAtDefault {
        drawn: 100.0,
        undrawn: 80.0,
        credit_conversion_factor: 0.5,
    };
    term.loss_given_default[2] = 0.25;
    let result = evaluate(&request(vec![term])).unwrap();
    let alive = (-1.0_f64).exp();
    let default = 1.0 - 2.0 * alive;
    close(result.periods[0].state_probability[0], alive);
    close(result.periods[0].state_probability[2], alive);
    close(result.periods[0].default_probability_by_origin[2], default);
    close(result.total_defaulted_exposure, 140.0 * default);
    close(result.total_expected_loss, 35.0 * default);
    close(result.total_eventual_nominal_recovery, 105.0 * default);
}

#[test]
fn recovery_distribution_retains_tail_and_nominal_reconciliation() {
    let mut term = period();
    term.transition_hazards_per_year[0][3] = (2.0_f64).ln();
    term.recovery_distribution = vec![
        RecoveryPoint {
            lag_years: 0.0,
            weight: 0.25,
        },
        RecoveryPoint {
            lag_years: 2.0,
            weight: 0.75,
        },
    ];
    let result = evaluate(&request(vec![term])).unwrap();
    close(result.total_defaulted_exposure, 50.0);
    close(result.total_expected_loss, 20.0);
    close(result.recoveries[0].amount, 7.5);
    close(result.recoveries[1].amount, 22.5);
    assert_eq!(result.recoveries[0].payment_year, 1.0);
    assert_eq!(result.recoveries[1].payment_year, 3.0);
    close(result.recovery_after_horizon, 22.5);
    close(
        result.total_defaulted_exposure,
        result.total_expected_loss + result.recoveries.iter().map(|r| r.amount).sum::<f64>(),
    );
}

#[test]
fn stable_tiny_hazards_and_maximum_admitted_hazards() {
    let p = competing_exit_probabilities(1.0e-12, 0.0, 1.0).unwrap();
    assert!((p[1] - 1.0e-12).abs() < 1.0e-24);
    let mut tiny = period();
    tiny.transition_hazards_per_year[0][3] = 1.0e-12;
    let result = evaluate(&request(vec![tiny])).unwrap();
    assert!((result.periods[0].marginal_default_probability - 1.0e-12).abs() < 1.0e-24);
    let mut high = period();
    high.transition_hazards_per_year[0][3] = 100.0;
    let result = evaluate(&request(vec![high])).unwrap();
    close(result.periods[0].state_probability[3], 1.0);
    close(result.periods[0].state_probability[0], (-100.0_f64).exp());
}

#[test]
fn partition_refinement_does_not_change_credit_mass() {
    let mut term = period();
    term.transition_hazards_per_year = [
        [0.0, 0.4, 0.1, 0.2, 0.3],
        [0.8, 0.0, 0.6, 0.7, 0.2],
        [0.1, 0.3, 0.0, 1.2, 0.05],
    ];
    let annual = evaluate(&request(vec![term.clone()])).unwrap();
    term.duration_years = 1.0 / 12.0;
    let monthly = evaluate(&request(vec![term; 12])).unwrap();
    for state in 0..5 {
        close(
            annual.periods[0].state_probability[state],
            monthly.periods[11].state_probability[state],
        );
    }
    close(annual.total_expected_loss, monthly.total_expected_loss);
}

#[test]
fn zero_lgd_and_full_lgd_have_explicit_recovery_semantics() {
    let mut term = period();
    term.transition_hazards_per_year[0][3] = 1.0;
    term.loss_given_default = [0.0; 3];
    let recovered = evaluate(&request(vec![term.clone()])).unwrap();
    assert_eq!(recovered.total_expected_loss, 0.0);
    close(
        recovered.total_defaulted_exposure,
        recovered.total_eventual_nominal_recovery,
    );
    term.loss_given_default = [1.0; 3];
    term.recovery_distribution.clear();
    let lost = evaluate(&request(vec![term])).unwrap();
    close(lost.total_defaulted_exposure, lost.total_expected_loss);
    assert!(lost.recoveries.is_empty());
}

#[test]
fn malformed_domains_and_truncated_tails_fail_before_evaluation() {
    let original = request(vec![period()]);
    let mutations: Vec<fn(&mut CreditRequest)> = vec![
        |r| r.schema = "credit-model-0".into(),
        |r| r.monetary_unit.clear(),
        |r| r.monetary_unit = "USD\n".into(),
        |r| r.initial_live_probability = [0.8, 0.0, 0.0],
        |r| r.initial_live_probability[0] = f64::NAN,
        |r| r.periods.clear(),
        |r| r.periods[0].duration_years = 0.0,
        |r| r.periods[0].duration_years = 1.1,
        |r| r.periods[0].transition_hazards_per_year[0][0] = 0.1,
        |r| r.periods[0].transition_hazards_per_year[0][3] = -0.1,
        |r| r.periods[0].transition_hazards_per_year[0][3] = f64::INFINITY,
        |r| r.periods[0].transition_hazards_per_year[0][3] = 100.1,
        |r| r.periods[0].loss_given_default[0] = 1.01,
        |r| r.periods[0].exposure_at_default[0].drawn = -1.0,
        |r| r.periods[0].exposure_at_default[0].credit_conversion_factor = 1.1,
        |r| r.periods[0].recovery_distribution[0].weight = 0.9,
        |r| r.periods[0].recovery_distribution[0].lag_years = -1.0,
        |r| r.periods[0].recovery_distribution.clear(),
        |r| r.periods = vec![period(); MAX_PERIODS + 1],
        |r| r.periods = vec![period(); 101],
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut input = original.clone();
        mutate(&mut input);
        assert!(
            evaluate(&input).is_err(),
            "mutation {index} unexpectedly accepted"
        );
    }
    assert!(annual_pd_to_hazard(1.0).is_err());
    assert!(annual_pd_to_hazard(-0.1).is_err());
    assert!(competing_exit_probabilities(60.0, 60.0, 1.0).is_err());
}

#[test]
fn serde_rejects_ambiguous_unversioned_pd_fields_and_unknown_fields() {
    let mut input = serde_json::to_value(request(vec![period()])).unwrap();
    input["annual_pd"] = serde_json::json!(0.2);
    assert!(serde_json::from_value::<CreditRequest>(input).is_err());
    let mut input = serde_json::to_value(request(vec![period()])).unwrap();
    input["periods"][0]["default_probability"] = serde_json::json!(0.2);
    assert!(serde_json::from_value::<CreditRequest>(input).is_err());
}

#[test]
fn long_admitted_grid_conserves_probability_without_renormalization() {
    let mut term = period();
    term.duration_years = 100.0 / MAX_PERIODS as f64;
    term.transition_hazards_per_year[0][1] = 0.1;
    term.transition_hazards_per_year[1][0] = 0.2;
    term.transition_hazards_per_year[0][3] = 0.01;
    term.transition_hazards_per_year[1][4] = 0.02;
    let result = evaluate(&request(vec![term; MAX_PERIODS])).unwrap();
    for row in result.periods {
        assert!((row.state_probability.iter().sum::<f64>() - 1.0).abs() < 1.0e-11);
    }
}

#[test]
fn exhausted_population_does_not_default_twice_or_generate_new_recoveries() {
    let mut term = period();
    term.transition_hazards_per_year[0][3] = 100.0;
    let result = evaluate(&request(vec![term; 12])).unwrap();
    // exp(-1000) underflows to zero in f64; the model must preserve absorption.
    assert_eq!(result.periods[11].conditional_default_probability, None);
    assert_eq!(result.periods[11].marginal_default_probability, 0.0);
    assert_eq!(result.periods[11].defaulted_exposure, 0.0);
    assert!(!result
        .recoveries
        .iter()
        .any(|row| row.default_period_index == 11));
    close(result.total_defaulted_exposure, 100.0);
    close(result.total_expected_loss, 40.0);
}

#[test]
fn recovery_order_and_money_bounds_are_validated() {
    let mut input = request(vec![period()]);
    input.periods[0].recovery_distribution = vec![
        RecoveryPoint {
            lag_years: 1.0,
            weight: 0.5,
        },
        RecoveryPoint {
            lag_years: 1.0,
            weight: 0.5,
        },
    ];
    assert!(evaluate(&input).is_err());
    input.periods[0] = period();
    input.periods[0].recovery_distribution = vec![
        RecoveryPoint {
            lag_years: 0.0,
            weight: 1.0 / 33.0
        };
        33
    ];
    assert!(evaluate(&input).is_err());
    input.periods[0] = period();
    input.periods[0].exposure_at_default[0] = ExposureAtDefault {
        drawn: 1.0e15,
        undrawn: 1.0e15,
        credit_conversion_factor: 1.0,
    };
    assert!(evaluate(&input).is_err());
    input.periods[0] = period();
    input.periods[0].recovery_distribution[0].lag_years = 100.1;
    assert!(evaluate(&input).is_err());
}
