use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::credit_path::UniformPair;
use portfolio_model_core::joint_state::*;

fn day(y: i32, m: u32, d: u32) -> i32 {
    ordinal_from_ymd(y, m, d).unwrap()
}

fn request() -> JointStateRequest {
    JointStateRequest {
        schema: SCHEMA.into(),
        identity: JointIdentity {
            source_id: "joint-source".into(),
            source_revision: "r1".into(),
            assumption_id: "supplied-driver".into(),
            common_random_numbers_id: "crn-7".into(),
            measure: CashflowMeasure::Physical,
        },
        as_of_ordinal: day(2026, 1, 1),
        day_count: ActualDayCount::Act365Fixed,
        periods: vec![
            JointPeriod {
                start_ordinal: day(2026, 1, 1),
                end_ordinal: day(2026, 7, 1),
                market_discount_factor: 0.98,
                behavioral_exit_hazard_per_year: 0.1,
                credit_default_hazard_per_year: 0.2,
                prepayment_hazard_per_year: 0.3,
                recovery_fraction: 0.4,
                recovery_lag_years: 1.0,
            },
            JointPeriod {
                start_ordinal: day(2026, 7, 1),
                end_ordinal: day(2027, 1, 1),
                market_discount_factor: 0.97,
                behavioral_exit_hazard_per_year: 0.0,
                credit_default_hazard_per_year: 0.0,
                prepayment_hazard_per_year: 0.0,
                recovery_fraction: 0.4,
                recovery_lag_years: 1.0,
            },
        ],
        paths: vec![JointPathOpening {
            position_id: "loan-1".into(),
            path_key: 11,
            principal: 100.0,
            random_pairs: vec![
                UniformPair {
                    waiting_uniform: 0.01,
                    destination_uniform: 0.2,
                },
                UniformPair {
                    waiting_uniform: 0.9,
                    destination_uniform: 0.3,
                },
            ],
        }],
    }
}

#[test]
fn competing_joint_driver_transition_is_once_only_and_retains_crn_identity() {
    let result = evaluate(&request()).unwrap();
    assert_eq!(result.identity.common_random_numbers_id, "crn-7");
    assert_eq!(
        result.source_status,
        "supplied_joint_market_behavior_credit_drivers"
    );
    let path = &result.paths[0];
    assert_eq!(path.events[0].kind, JointEventKind::BehavioralExit);
    assert_eq!(path.closing_principal, 0.0);
    assert_eq!(path.total_credit_default, 0.0);
    assert_eq!(path.total_prepayment, 0.0);
    assert_eq!(path.total_recovery_cash, 0.0);
}

#[test]
fn credit_default_retains_recovery_and_loss_separately() {
    let mut raw = request();
    raw.paths[0].random_pairs[0].waiting_uniform = 0.08;
    let path = &evaluate(&raw).unwrap().paths[0];
    assert_eq!(path.events[0].kind, JointEventKind::CreditDefault);
    assert_eq!(path.total_credit_default, 100.0);
    assert_eq!(path.total_recovery_cash, 40.0);
    assert_eq!(path.total_economic_loss, 60.0);
    assert_eq!(path.events[0].recovery_lag_years, 1.0);
}

#[test]
fn zero_hazards_are_exact_live_identity_and_market_driver_is_retained() {
    let mut raw = request();
    raw.periods[0].behavioral_exit_hazard_per_year = 0.0;
    raw.periods[0].credit_default_hazard_per_year = 0.0;
    raw.periods[0].prepayment_hazard_per_year = 0.0;
    let path = &evaluate(&raw).unwrap().paths[0];
    assert!(path
        .events
        .iter()
        .all(|e| e.kind == JointEventKind::Continue));
    assert_eq!(path.closing_principal, 100.0);
    assert!((path.discounted_live_balance - 195.0).abs() < 1e-12);
}

#[test]
fn invalid_period_continuity_pairs_and_uniforms_fail_before_evaluation() {
    let mut raw = request();
    raw.periods[1].start_ordinal += 1;
    assert!(evaluate(&raw).is_err());
    let mut raw = request();
    raw.paths[0].random_pairs.pop();
    assert!(evaluate(&raw).is_err());
    let mut raw = request();
    raw.paths[0].random_pairs[0].waiting_uniform = 1.0;
    assert!(evaluate(&raw).is_err());
}

#[test]
fn duplicate_positions_and_path_keys_fail_closed() {
    let mut raw = request();
    raw.paths.push(raw.paths[0].clone());
    assert!(evaluate(&raw).is_err());
    let mut raw = request();
    raw.paths.push(JointPathOpening {
        position_id: "loan-2".into(),
        ..raw.paths[0].clone()
    });
    assert!(evaluate(&raw).is_err());
}

#[test]
fn measure_is_retained_and_no_p_to_q_conversion_is_inferred() {
    let mut raw = request();
    raw.identity.measure = CashflowMeasure::Pricing;
    let result = evaluate(&raw).unwrap();
    assert_eq!(result.identity.measure, CashflowMeasure::Pricing);
}

#[test]
fn cooperative_failure_returns_no_partial_result() {
    let mut checks = 0;
    let error = evaluate_with_check(&request(), || {
        checks += 1;
        if checks >= 2 {
            Err("deadline".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error, "deadline");
    assert!(checks >= 2);
}
