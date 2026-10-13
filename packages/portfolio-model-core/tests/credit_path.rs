use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_calibration::ObservationTime;
use portfolio_model_core::credit_path::*;

fn request() -> CreditPathRequest {
    CreditPathRequest {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        monetary_unit: "USD currency_units".into(),
        source_id: "synthetic-fixture".into(),
        source_revision: "1".into(),
        model_policy_id: "supplied".into(),
        origin_time: ObservationTime {
            ordinal: 739890,
            seconds_of_day: 0,
        },
        max_jumps_per_path: 10,
        intervals: vec![PathInterval {
            end_year: 1.0,
            hazards_per_year: [[0.0; 5]; 3],
            accrual_rates: [0.1, 0.0, 0.0],
            loss_given_default: [0.4; 3],
            credit_conversion_factor: [0.5; 3],
            interest_recovery_fraction: [0.25; 3],
            complete_recovery_tails: std::array::from_fn(|_| {
                vec![
                    RecoveryWeight {
                        lag_years: 0.1,
                        weight: 0.5,
                    },
                    RecoveryWeight {
                        lag_years: 2.0,
                        weight: 0.5,
                    },
                ]
            }),
        }],
        boundaries: vec![],
        draw_stream: DrawStream::Supplied {
            stream_id: "independent-hand-draws".into(),
            stream_revision: "1".into(),
            paths: vec![SuppliedPathDraws {
                path_key: 17,
                pairs: vec![UniformPair {
                    waiting_uniform: (-0.15_f64).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        },
        paths: vec![PathOpening {
            path_id: "loan".into(),
            path_key: 17,
            initial_live_state: CreditState::Performing,
            principal: 100.0,
            undrawn_commitment: 20.0,
            accrued_interest: -2.0,
            book_adjustment: -3.0,
        }],
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn pairs(r: &mut CreditPathRequest, values: &[(f64, f64)]) {
    if let DrawStream::Supplied { paths, .. } = &mut r.draw_stream {
        paths[0].pairs = values
            .iter()
            .map(|(e, u)| UniformPair {
                waiting_uniform: (-e).exp(),
                destination_uniform: *u,
            })
            .collect();
    }
}

#[test]
fn carries_internal_wait_across_term_boundaries_and_complete_default_tail() {
    let mut r = request();
    r.intervals[0].end_year = 0.1;
    r.intervals[0].hazards_per_year[0][3] = 1.0;
    let mut second = r.intervals[0].clone();
    second.end_year = 1.0;
    second.hazards_per_year[0][3] = 2.0;
    r.intervals.push(second);
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    let d = p.default.as_ref().unwrap();
    close(d.year, 0.125);
    close(p.earned_interest, 1.25);
    close(d.principal, 110.0);
    close(d.accrued_interest, -0.75);
    close(d.economic_principal_loss, 44.0);
    close(d.recoveries[0].principal, 33.0);
    close(d.recoveries[0].interest, -0.09375);
    close(d.recoveries[1].year, 2.125);
    close(p.closing_principal, 77.0);
    close(p.closing_accrued_interest, -0.65625);
    close(p.cash_funded, 10.0);
    close(p.cash_received, 32.90625);
    assert_eq!(p.random_pairs_used, 1);
}

#[test]
fn refinement_preserves_draw_identity_and_event_time() {
    let mut r = request();
    r.intervals[0].hazards_per_year[0][4] = 1.0;
    let a = evaluate(&r).unwrap();
    let mut refined = r.intervals[0].clone();
    r.intervals[0].end_year = 0.05;
    refined.end_year = 1.0;
    r.intervals.push(refined);
    let b = evaluate(&r).unwrap();
    close(a.paths[0].events[0].year, b.paths[0].events[0].year);
    close(a.paths[0].cash_received, b.paths[0].cash_received);
    close(a.paths[0].cash_received, 99.5);
    close(a.paths[0].book_adjustment_recognized, -3.0);
}

#[test]
fn cures_nonaccrual_and_competing_destination_are_realized_once() {
    let mut r = request();
    r.paths[0].initial_live_state = CreditState::Delinquent;
    r.intervals[0].hazards_per_year[2][0] = 2.0;
    r.intervals[0].hazards_per_year[0][3] = 1.0;
    r.intervals[0].hazards_per_year[0][4] = 3.0;
    pairs(&mut r, &[(0.2, 0.4), (0.8, 0.9)]);
    let p = evaluate(&r).unwrap().paths.remove(0);
    assert_eq!(p.outcome, PathOutcome::Prepaid);
    assert!(p.default.is_none());
    assert_eq!(p.events.len(), 2);
    close(p.events[0].year, 0.1);
    close(p.events[1].year, 0.3);
    close(p.earned_interest, 2.0);
    close(p.cash_received, 100.0);
}

#[test]
fn zero_credit_signed_coupon_draw_and_maturity_are_not_prepay() {
    let mut r = request();
    r.intervals[0].accrual_rates[0] = -0.1;
    r.boundaries = vec![
        PathBoundary {
            event_id: "draw".into(),
            policy_id: "draw-policy".into(),
            year: 0.5,
            action: BoundaryAction::Draw {
                undrawn_fraction: [0.5; 3],
            },
        },
        PathBoundary {
            event_id: "maturity".into(),
            policy_id: "terms".into(),
            year: 1.0,
            action: BoundaryAction::Maturity {},
        },
    ];
    let p = evaluate(&r).unwrap().paths.remove(0);
    assert_eq!(p.outcome, PathOutcome::Matured);
    assert_eq!(p.random_pairs_used, 0);
    close(p.earned_interest, -10.5);
    close(p.cash_received, 97.5);
    close(p.cash_funded, 10.0);
    close(p.closing_principal, 0.0);
    close(p.book_adjustment_recognized, -3.0);
}

#[test]
fn counter_stream_is_order_independent_reproducible_and_measure_retaining() {
    let mut r = request();
    r.measure = CreditMeasure::Pricing;
    r.intervals[0].hazards_per_year[0][4] = 1.0;
    r.draw_stream = DrawStream::CounterSplitmix64V1 {
        stream_id: "crn".into(),
        stream_revision: "v1".into(),
        seed: 123,
    };
    let mut other = r.paths[0].clone();
    other.path_id = "other".into();
    other.path_key = 19;
    r.paths.push(other);
    let a = evaluate(&r).unwrap();
    let again = evaluate(&r).unwrap();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
    r.paths.reverse();
    let b = evaluate(&r).unwrap();
    assert_eq!(
        serde_json::to_string(&a.paths[0]).unwrap(),
        serde_json::to_string(&b.paths[1]).unwrap()
    );
    assert_eq!(b.measure, CreditMeasure::Pricing);
}

#[test]
fn no_draw_fallback_and_invalid_domains_are_rejected() {
    let mut r = request();
    r.intervals[0].hazards_per_year[0][1] = 100.0;
    r.intervals[0].hazards_per_year[1][0] = 100.0;
    assert!(evaluate(&r).unwrap_err().contains("exhausted"));
    pairs(&mut r, &[(0.01, 0.5), (0.01, 0.5)]);
    r.max_jumps_per_path = 1;
    assert!(evaluate(&r).unwrap_err().contains("maximum jumps"));
    let mut r = request();
    r.monetary_unit = "USD\n".into();
    assert!(evaluate(&r).is_err());
    let mut r = request();
    r.intervals[0].complete_recovery_tails[0].pop();
    assert!(evaluate(&r).is_err());
    let mut r = request();
    r.paths[0].initial_live_state = CreditState::Default;
    assert!(evaluate(&r).is_err());
    let mut r = request();
    r.intervals[0].hazards_per_year[0][0] = 0.1;
    assert!(evaluate(&r).is_err());
    let mut json = serde_json::to_value(request()).unwrap();
    json.as_object_mut().unwrap().remove("measure");
    assert!(serde_json::from_value::<CreditPathRequest>(json).is_err());
}

#[test]
fn checked_example_has_exact_default_time_and_no_tail_truncation() {
    let r: CreditPathRequest =
        serde_json::from_str(include_str!("../../../examples/models/credit-path-1.json")).unwrap();
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    let claim = p.default.as_ref().unwrap();
    close(claim.year, 0.3);
    close(p.earned_interest, 3.0);
    close(p.earned_interest_by_origin[0], 3.0);
    close(claim.recoveries[0].year, 2.3);
    close(claim.recoveries[0].principal, 66.0);
    close(claim.recoveries[0].interest, 3.0);
    close(p.cash_received, 0.0);
    close(p.closing_principal, 110.0);
    assert_eq!(p.events.len(), 1);
}

#[test]
fn contractual_boundary_changes_actual_amount_without_resampling_wait() {
    let mut r = request();
    r.intervals[0].hazards_per_year[0][4] = 1.0;
    r.boundaries.push(PathBoundary {
        event_id: "scheduled".into(),
        policy_id: "explicit-half-paydown".into(),
        year: 0.1,
        action: BoundaryAction::Repayment {
            principal_fraction: [0.5; 3],
            interest_fraction: [1.0; 3],
            cancel_undrawn_fraction: [0.0; 3],
        },
    });
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    close(p.events[1].year, 0.15);
    close(p.earned_interest, 1.25);
    close(p.cash_received, 99.25);
    close(p.book_adjustment_recognized, -3.0);
    assert_eq!(p.random_pairs_used, 1);
    assert_eq!(p.events[0].policy_id, "explicit-half-paydown");
}

#[test]
fn credit_exit_on_maturity_boundary_is_emitted_once() {
    let mut r = request();
    r.intervals[0].hazards_per_year[0][4] = 1.0;
    pairs(&mut r, &[(1.0, 0.5)]);
    r.boundaries.push(PathBoundary {
        event_id: "maturity".into(),
        policy_id: "terms".into(),
        year: 1.0,
        action: BoundaryAction::Maturity {},
    });
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert_eq!(p.outcome, PathOutcome::Prepaid);
    assert_eq!(p.events.len(), 1);
    close(p.cash_received, 108.0);
    close(p.earned_interest, 10.0);
}
