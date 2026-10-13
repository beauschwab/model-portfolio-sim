use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_operator::*;
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;

fn request(duration_years: f64) -> CreditOperatorRequest {
    CreditOperatorRequest {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        duration_years,
        transition_hazards_per_year: [[0.0; 5]; 3],
        accrual_rates: [0.0; 3],
    }
}

fn state() -> CreditOperatorState {
    CreditOperatorState {
        schema: STATE_SCHEMA.into(),
        measure: CreditMeasure::Physical,
        monetary_unit: "USD".into(),
        probability: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        principal: [100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        undrawn_commitment: [40.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        accrued_interest: [2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        book_adjustment: [-3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2.0e-12 * expected.abs().max(1.0),
        "{actual:.17e} != {expected:.17e}"
    );
}

fn exits(mut request: CreditOperatorRequest) -> CreditOperatorRequest {
    request.transition_hazards_per_year[0][3] = 0.2;
    request.transition_hazards_per_year[0][4] = 0.3;
    request.accrual_rates[0] = 0.06;
    request
}

#[test]
fn default_prepay_principal_and_interest_flux_match_independent_integrals() {
    let operator = compile(&exits(request(1.0))).unwrap();
    let result = operator.apply(&state()).unwrap();
    let h: f64 = 0.5;
    let survival = (-h).exp();
    let exits_integral = (1.0 - survival) / h;
    let default_time_integral = (1.0 - survival * (1.0 + h)) / (h * h);
    close(result.probability[0], survival);
    close(result.probability[3], 0.2 * exits_integral);
    close(result.probability[6], 0.3 * exits_integral);
    close(result.principal[0], 100.0 * survival);
    close(result.principal[3], 100.0 * 0.2 * exits_integral);
    close(result.principal[6], 100.0 * 0.3 * exits_integral);
    close(result.undrawn_commitment[3], 40.0 * 0.2 * exits_integral);
    close(result.accrued_interest[0], (2.0 + 6.0) * survival);
    close(
        result.accrued_interest[3],
        2.0 * 0.2 * exits_integral + 6.0 * 0.2 * default_time_integral,
    );
    close(
        result.accrued_interest[6],
        2.0 * 0.3 * exits_integral + 6.0 * 0.3 * default_time_integral,
    );
    close(
        result.accrued_interest.iter().sum(),
        2.0 + 6.0 * exits_integral,
    );
    close(result.book_adjustment[3], -3.0 * 0.2 * exits_integral);
}

#[test]
fn cure_nonaccrual_matches_two_state_occupation_and_joint_endpoint_interest() {
    let mut input = request(1.0);
    input.transition_hazards_per_year[0][1] = 0.4;
    input.transition_hazards_per_year[1][0] = 0.6;
    input.accrual_rates = [0.06, 0.0, 0.0];
    let mut initial = state();
    initial.accrued_interest = [0.0; STATES];
    let result = compile(&input).unwrap().apply(&initial).unwrap();
    let exponential = (-1.0_f64).exp();
    let performing = 0.6 + 0.4 * exponential;
    let all_earned = 6.0 * (0.6 + 0.4 * (1.0 - exponential));
    // Integral P_PP(s)*P_PP(1-s)ds, with P_PP(u)=.6+.4exp(-u).
    let earned_ending_performing = 6.0 * (0.36 + 0.48 * (1.0 - exponential) + 0.16 * exponential);
    close(result.principal[0], 100.0 * performing);
    close(result.accrued_interest[0], earned_ending_performing);
    close(
        result.accrued_interest[1],
        all_earned - earned_ending_performing,
    );
    close(result.accrued_interest.iter().sum(), all_earned);
}

#[test]
fn semigroup_refinement_preserves_all_separate_vectors() {
    let mut input = exits(request(2.0));
    input.transition_hazards_per_year[0][1] = 0.4;
    input.transition_hazards_per_year[1][0] = 0.3;
    input.transition_hazards_per_year[1][2] = 0.2;
    input.transition_hazards_per_year[2][3] = 0.8;
    input.accrual_rates = [0.06, 0.0, -0.02];
    let initial = state();
    let full = compile(&input).unwrap().apply(&initial).unwrap();
    input.duration_years = 1.0 / 12.0;
    let monthly = compile(&input).unwrap();
    let mut refined = initial;
    for _ in 0..24 {
        refined = monthly.apply(&refined).unwrap();
    }
    for (a, b) in [
        (&full.probability, &refined.probability),
        (&full.principal, &refined.principal),
        (&full.undrawn_commitment, &refined.undrawn_commitment),
        (&full.accrued_interest, &refined.accrued_interest),
        (&full.book_adjustment, &refined.book_adjustment),
    ] {
        for index in 0..STATES {
            close(a[index], b[index]);
        }
    }
}

#[test]
fn zero_hazard_reward_and_zero_duration_are_exact_without_normalization() {
    let mut input = request(2.0);
    input.accrual_rates = [0.06, -0.02, 0.0];
    let mut initial = state();
    initial.probability = [0.2, 0.1, 0.0, 0.05, 0.0, 0.0, 0.0];
    initial.principal = [100.0, 50.0, 0.0, 10.0, 0.0, 0.0, 0.0];
    initial.accrued_interest = [2.0, -1.0, 0.0, 3.0, 0.0, 0.0, 0.0];
    let operator = compile(&input).unwrap();
    let result = operator.apply(&initial).unwrap();
    assert_eq!(result.probability, initial.probability);
    assert_eq!(result.principal, initial.principal);
    assert_eq!(
        result.accrued_interest,
        [14.0, -3.0, 0.0, 3.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(operator.earned_interest()[0][0], 0.12);
    input.duration_years = 0.0;
    initial.book_adjustment[2] = -0.0;
    let unchanged = compile(&input).unwrap().apply(&initial).unwrap();
    assert_eq!(
        serde_json::to_string(&unchanged).unwrap(),
        serde_json::to_string(&initial).unwrap()
    );
}

#[test]
fn tiny_hazards_and_negative_coupons_keep_small_exit_interest() {
    let mut input = request(1.0);
    input.transition_hazards_per_year[0][3] = 1.0e-12;
    input.accrual_rates[0] = -0.06;
    let operator = compile(&input).unwrap();
    let result = operator.apply(&state()).unwrap();
    assert!((result.probability[3] - 1.0e-12).abs() < 1.0e-24);
    // Newly earned default interest ~ B*r*h*t^2/2; existing A exits ~ A*h*t.
    assert!((operator.earned_interest()[0][3] + 0.03e-12).abs() < 1.0e-24);
    assert!((result.accrued_interest[3] + 1.0e-12).abs() < 1.0e-23);
    close(result.accrued_interest[0], -4.0 * (-1.0e-12_f64).exp());
}

#[test]
fn extreme_admitted_hazards_long_duration_and_reward_domination_are_stable() {
    let mut input = request(100.0);
    input.transition_hazards_per_year[0][3] = 100.0;
    input.accrual_rates[0] = 100.0;
    let result = compile(&input).unwrap().apply(&state()).unwrap();
    close(result.principal[3], 100.0);
    close(result.accrued_interest[3], 102.0); // B*r/h + opening accrued; live vanishes.
    close(result.probability.iter().sum(), 1.0);
    input.duration_years = 1.0;
    input.transition_hazards_per_year[0][3] = 0.5;
    let result = compile(&input).unwrap().apply(&state()).unwrap();
    close(
        result.accrued_interest.iter().sum(),
        2.0 + 100.0 * 100.0 * (1.0 - (-0.5_f64).exp()) / 0.5,
    );
}

#[test]
fn existing_absorbed_money_does_not_accrue_or_exit_twice() {
    let operator = compile(&exits(request(1.0))).unwrap();
    let mut initial = state();
    initial.probability = [0.0, 0.0, 0.0, 0.3, 0.2, 0.0, 0.1];
    initial.principal = [0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 40.0];
    initial.accrued_interest = [0.0, 0.0, 0.0, -1.0, 2.0, 3.0, 4.0];
    initial.undrawn_commitment = [0.0; STATES];
    initial.book_adjustment = [0.0; STATES];
    let result = operator.apply(&initial).unwrap();
    for index in 0..STATES {
        close(result.principal[index], initial.principal[index]);
        close(
            result.accrued_interest[index],
            initial.accrued_interest[index],
        );
    }
    assert_eq!(result.probability[0], 0.0);
}

#[test]
fn deterministic_reuse_and_signed_transport_are_independent_of_probability_mass() {
    let input = exits(request(0.5));
    let operator = compile(&input).unwrap();
    assert_eq!(
        serde_json::to_string(&operator).unwrap(),
        serde_json::to_string(&compile(&input).unwrap()).unwrap()
    );
    let first = operator.apply(&state()).unwrap();
    let mut smaller_mass = state();
    smaller_mass.probability[0] = 0.2;
    let second = operator.apply(&smaller_mass).unwrap();
    assert_eq!(first.principal, second.principal);
    assert_eq!(first.accrued_interest, second.accrued_interest);
    assert_eq!(
        operator.transport_signed(&state().book_adjustment).unwrap(),
        first.book_adjustment
    );
    let from_thread = std::thread::spawn(move || compile(&input).unwrap().apply(&state()).unwrap())
        .join()
        .unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(from_thread).unwrap()
    );
}

#[test]
fn invalid_model_state_measure_and_signed_amount_domains_fail() {
    let request_mutations: Vec<fn(&mut CreditOperatorRequest)> = vec![
        |r| r.schema.clear(),
        |r| r.duration_years = -1.0,
        |r| r.duration_years = 100.1,
        |r| r.duration_years = f64::NAN,
        |r| r.transition_hazards_per_year[0][0] = 0.1,
        |r| r.transition_hazards_per_year[0][3] = -1.0,
        |r| r.transition_hazards_per_year[0][3] = 100.1,
        |r| r.transition_hazards_per_year[0][3] = f64::INFINITY,
        |r| r.accrual_rates[0] = -100.1,
        |r| r.accrual_rates[0] = f64::NAN,
    ];
    for mutate in request_mutations {
        let mut input = request(1.0);
        mutate(&mut input);
        assert!(compile(&input).is_err());
    }
    let operator = compile(&exits(request(1.0))).unwrap();
    let state_mutations: Vec<fn(&mut CreditOperatorState)> = vec![
        |s| s.schema.clear(),
        |s| s.measure = CreditMeasure::Pricing,
        |s| s.monetary_unit = "USD\n".into(),
        |s| s.probability[0] = -0.1,
        |s| s.probability[1] = 0.1,
        |s| s.principal[0] = -1.0,
        |s| s.undrawn_commitment[0] = -1.0,
        |s| s.accrued_interest[0] = f64::INFINITY,
        |s| s.principal = [MAX_MONETARY_AMOUNT; STATES],
    ];
    for mutate in state_mutations {
        let mut initial = state();
        mutate(&mut initial);
        assert!(operator.apply(&initial).is_err());
    }
    let mut input = request(1.0);
    input.accrual_rates[0] = 100.0;
    let mut initial = state();
    initial.principal[0] = MAX_MONETARY_AMOUNT;
    assert!(compile(&input).unwrap().apply(&initial).is_err());
}

#[test]
fn strict_schema_requires_measure_and_rejects_ambiguous_pd_or_extra_components() {
    let mut missing = serde_json::to_value(request(1.0)).unwrap();
    missing.as_object_mut().unwrap().remove("measure");
    assert!(serde_json::from_value::<CreditOperatorRequest>(missing).is_err());
    let mut ambiguous = serde_json::to_value(request(1.0)).unwrap();
    ambiguous["annual_pd"] = serde_json::json!(0.1);
    assert!(serde_json::from_value::<CreditOperatorRequest>(ambiguous).is_err());
    let mut unknown = serde_json::to_value(state()).unwrap();
    unknown["allowance"] = serde_json::json!(1.0);
    assert!(serde_json::from_value::<CreditOperatorState>(unknown).is_err());
}

#[test]
fn allocation_free_in_place_path_matches_copy_path_and_failure_is_atomic() {
    let operator = compile(&exits(request(0.5))).unwrap();
    let expected = operator.apply(&state()).unwrap();
    let mut actual = state();
    operator.apply_in_place(&mut actual).unwrap();
    assert_eq!(
        serde_json::to_value(expected).unwrap(),
        serde_json::to_value(actual).unwrap()
    );
    let mut input = request(1.0);
    input.accrual_rates[0] = 100.0;
    let operator = compile(&input).unwrap();
    let mut inadmissible = state();
    inadmissible.principal[0] = MAX_MONETARY_AMOUNT;
    let original = serde_json::to_value(&inadmissible).unwrap();
    assert!(operator.apply_in_place(&mut inadmissible).is_err());
    assert_eq!(original, serde_json::to_value(inadmissible).unwrap());
}
