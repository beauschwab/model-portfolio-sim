use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_calibration::ObservationTime;
use portfolio_model_core::credit_path::*;
use portfolio_model_core::credit_path_income as income;
pub use portfolio_model_core::{cashflow, credit, credit_calibration, limits};

#[path = "fixtures/credit_path_before_income.rs"]
#[allow(dead_code)]
mod before;

fn request() -> income::Request {
    income::Request {
        schema: income::SCHEMA.into(),
        raw_credit_path: CreditPathRequest {
            schema: SCHEMA.into(),
            measure: CreditMeasure::Physical,
            monetary_unit: "USD currency_units".into(),
            source_id: "reviewed-raw".into(),
            source_revision: "1".into(),
            model_policy_id: "explicit".into(),
            origin_time: ObservationTime {
                ordinal: 739890,
                seconds_of_day: 0,
            },
            max_jumps_per_path: 4,
            intervals: vec![PathInterval {
                end_year: 1.0,
                hazards_per_year: [[0.0; 5]; 3],
                accrual_rates: [0.1; 3],
                loss_given_default: [0.4; 3],
                credit_conversion_factor: [0.0; 3],
                interest_recovery_fraction: [0.5; 3],
                complete_recovery_tails: std::array::from_fn(|_| {
                    vec![RecoveryWeight {
                        lag_years: 0.0,
                        weight: 1.0,
                    }]
                }),
            }],
            boundaries: vec![],
            draw_stream: DrawStream::Supplied {
                stream_id: "fixed".into(),
                stream_revision: "1".into(),
                paths: vec![SuppliedPathDraws {
                    path_key: 17,
                    pairs: vec![UniformPair {
                        waiting_uniform: (-0.5_f64).exp(),
                        destination_uniform: 0.5,
                    }],
                }],
            },
            paths: vec![PathOpening {
                path_id: "commercial".into(),
                path_key: 17,
                initial_live_state: CreditState::Performing,
                principal: 100.0,
                undrawn_commitment: 20.0,
                accrued_interest: 0.0,
                book_adjustment: 2.0,
            }],
        },
        original_anchors: vec![income::OriginalAnchor {
            path_key: 17,
            original_yield_anchor: "original".into(),
            acquisition_cost: 102.0,
            original_annual_effective_rate: 0.05,
        }],
        recognition_rates: vec![income::RecognitionRate {
            interval_index: 0,
            review: "frozen-original".into(),
            annual_effective_rates: [0.05; 3],
        }],
        policy_review: "reviewed-research-only".into(),
        partial_payment_policy: "cash_reduces_gross_no_proportional_basis_disposal-1".into(),
        default_policy: "stop_income_transfer_actual_gross_retain_nominal_claims-1".into(),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn repayment(year: f64, fraction: f64) -> PathBoundary {
    PathBoundary {
        event_id: "payment".into(),
        policy_id: "explicit".into(),
        year,
        action: BoundaryAction::Repayment {
            principal_fraction: [fraction; 3],
            interest_fraction: [1.0; 3],
            cancel_undrawn_fraction: [0.0; 3],
        },
    }
}

#[test]
fn exact_semigroup_and_original_anchor_unchanged() {
    let r = request();
    let a = income::evaluate(&r).unwrap();
    let mut split = r.clone();
    split.raw_credit_path.intervals[0].end_year = 0.5;
    split
        .raw_credit_path
        .intervals
        .push(r.raw_credit_path.intervals[0].clone());
    split.recognition_rates.push(income::RecognitionRate {
        interval_index: 1,
        review: "same-reviewed-rate".into(),
        annual_effective_rates: [0.05; 3],
    });
    let b = income::evaluate(&split).unwrap();
    let full = a.carry_points().last().unwrap();
    close(full.gross_carry, 102.0 * 1.05);
    close(full.book_adjustment, 102.0 * 1.05 - 110.0);
    close(
        full.gross_carry,
        b.carry_points().last().unwrap().gross_carry,
    );
    let encoded = serde_json::to_value(&b).unwrap();
    assert_eq!(
        encoded["raw_request"]["original_anchors"],
        serde_json::to_value(&r.original_anchors).unwrap()
    );
    assert_eq!(encoded["original_anchor_refitted"], false);
}

#[test]
fn partial_cash_reduces_gross_without_proportional_basis_realization() {
    let mut r = request();
    r.raw_credit_path.boundaries.push(repayment(0.5, 0.5));
    let out = income::evaluate(&r).unwrap();
    let first_g = 102.0 * 1.05_f64.sqrt();
    let after_cash = first_g - 55.0;
    let p = &out.credit().result.paths[0];
    close(p.closing_principal, 50.0);
    close(p.closing_accrued_interest, 2.5);
    close(
        out.carry_points().last().unwrap().gross_carry,
        after_cash * 1.05_f64.sqrt(),
    );
    assert_eq!(p.book_adjustment_recognized, 0.0);
    assert_eq!(p.events[0].book_adjustment_recognition, 0.0);
}

#[test]
fn default_transfers_actual_carry_and_recovery_cash_once() {
    let mut r = request();
    r.raw_credit_path.intervals[0].hazards_per_year[0][3] = 1.0;
    r.raw_credit_path.intervals[0].credit_conversion_factor[0] = 0.5;
    let out = income::evaluate(&r).unwrap();
    let p = &out.credit().result.paths[0];
    let claim = p.default.as_ref().unwrap();
    let g = 102.0 * 1.05_f64.sqrt() + 10.0;
    close(claim.principal, 110.0);
    close(claim.accrued_interest, 5.0);
    close(claim.book_adjustment, g - 115.0);
    close(p.cash_received, 66.0 + 2.5);
    close(p.cash_funded, 10.0);
    close(p.closing_principal, 44.0);
    close(p.closing_accrued_interest, 2.5);
    close(out.carry_points().last().unwrap().gross_carry, g - 68.5);
    close(
        out.carry_points()
            .iter()
            .map(|x| x.effective_interest)
            .sum(),
        g - 112.0,
    );
    assert_eq!(p.book_adjustment_recognized, 0.0);
}

#[test]
fn nonaccrual_review_stops_accounting_not_legal_signed_right() {
    let mut r = request();
    r.raw_credit_path.paths[0].initial_live_state = CreditState::Delinquent;
    r.raw_credit_path.intervals[0].accrual_rates[2] = -0.1;
    r.recognition_rates[0].annual_effective_rates[2] = 0.0;
    let out = income::evaluate(&r).unwrap();
    let last = out.carry_points().last().unwrap();
    close(last.gross_carry, 102.0);
    close(last.contractual_accrued_interest, -10.0);
    close(last.book_adjustment, 12.0);
}

#[test]
fn prospective_review_separate_from_frozen_original_yield() {
    let mut r = request();
    r.raw_credit_path.intervals[0].end_year = 0.5;
    let mut next = r.raw_credit_path.intervals[0].clone();
    next.end_year = 1.0;
    r.raw_credit_path.intervals.push(next);
    r.recognition_rates.push(income::RecognitionRate {
        interval_index: 1,
        review: "prospective-reset".into(),
        annual_effective_rates: [0.1; 3],
    });
    let out = income::evaluate(&r).unwrap();
    close(
        out.carry_points().last().unwrap().gross_carry,
        102.0 * (1.05_f64 * 1.1).sqrt(),
    );
    assert_eq!(r.original_anchors[0].original_annual_effective_rate, 0.05);
}

#[test]
fn maturity_separates_cash_from_signed_basis_disposal() {
    let mut r = request();
    r.raw_credit_path.boundaries.push(PathBoundary {
        event_id: "full".into(),
        policy_id: "exit".into(),
        year: 1.0,
        action: BoundaryAction::Maturity {},
    });
    let out = income::evaluate(&r).unwrap();
    let p = &out.credit().result.paths[0];
    close(p.cash_received, 110.0);
    close(p.book_adjustment_recognized, 102.0 * 1.05 - 110.0);
    assert_eq!(out.carry_points().last().unwrap().gross_carry, 0.0);
    assert_eq!(p.closing_book_adjustment, 0.0);
}

#[test]
fn actual_near_unit_recovery_retains_tiny_principal_and_full_lag_tail() {
    let mut r = request();
    r.raw_credit_path.intervals[0].hazards_per_year[0][3] = 1.0;
    r.raw_credit_path.intervals[0].loss_given_default[0] = 0.0;
    r.raw_credit_path.intervals[0].complete_recovery_tails[0] = vec![RecoveryWeight {
        lag_years: 0.0,
        weight: 1.0 - 8.0 * f64::EPSILON,
    }];
    let out = income::evaluate(&r).unwrap();
    let claim = out.credit().result.paths[0].default.as_ref().unwrap();
    let actual = claim.principal - claim.recoveries[0].principal;
    assert!(actual > 0.0);
    assert_eq!(
        out.credit().result.paths[0].closing_principal.to_bits(),
        actual.to_bits()
    );
    r.raw_credit_path.intervals[0].complete_recovery_tails[0] = vec![RecoveryWeight {
        lag_years: 2.0,
        weight: 1.0,
    }];
    let later = income::evaluate(&r).unwrap();
    let value = serde_json::to_value(&later).unwrap();
    assert_eq!(
        value["complete_recovery_receipts"][0]["applied_in_horizon"],
        false
    );
    close(later.credit().result.paths[0].closing_principal, 100.0);
}

#[test]
fn cancellation_during_actual_loop_and_pregeneration_refusal() {
    let r = request();
    let mut complete_probes = 0;
    income::evaluate_with_check(&r, &mut || {
        complete_probes += 1;
        Ok(())
    })
    .unwrap();
    let mut n = 0;
    let err = income::evaluate_with_check(&r, &mut || {
        n += 1;
        if n == complete_probes - 2 {
            Err("cancel during native carry".into())
        } else {
            Ok(())
        }
    });
    assert!(err.unwrap_err().contains("cancel during native carry"));
    let mut too_large = r;
    too_large.raw_credit_path.max_jumps_per_path = 100_000;
    assert!(income::evaluate(&too_large)
        .unwrap_err()
        .contains("before native path generation"));
}

#[test]
fn original_cost_path_and_rate_coverage_are_exact() {
    let mut r = request();
    r.original_anchors[0].acquisition_cost = 103.0;
    assert!(income::evaluate(&r).unwrap_err().contains("exactly match"));
    r = request();
    r.recognition_rates[0].interval_index = 1;
    assert!(income::evaluate(&r)
        .unwrap_err()
        .contains("ordered interval"));
    r = request();
    r.raw_credit_path.measure = CreditMeasure::Pricing;
    assert!(income::evaluate(&r).unwrap_err().contains("Physical"));
}

#[test]
fn actual_migration_clock_changes_reviewed_rate_without_resampling() {
    let mut r = request();
    r.raw_credit_path.intervals[0].hazards_per_year[0][1] = 1.0;
    r.raw_credit_path.intervals[0].hazards_per_year[1][0] = 1.0;
    r.recognition_rates[0].annual_effective_rates = [0.05, 0.02, 0.0];
    if let DrawStream::Supplied { paths, .. } = &mut r.raw_credit_path.draw_stream {
        paths[0].pairs = vec![
            UniformPair {
                waiting_uniform: (-0.2_f64).exp(),
                destination_uniform: 0.5,
            },
            UniformPair {
                waiting_uniform: (-0.3_f64).exp(),
                destination_uniform: 0.5,
            },
            UniformPair {
                waiting_uniform: (-2.0_f64).exp(),
                destination_uniform: 0.5,
            },
        ];
    }
    let out = income::evaluate(&r).unwrap();
    close(
        out.carry_points().last().unwrap().gross_carry,
        102.0 * (1.05_f64.ln() * 0.7 + 1.02_f64.ln() * 0.3).exp(),
    );
    let old = evaluate(&r.raw_credit_path).unwrap();
    let selected = &out.credit().result.paths[0];
    assert_eq!(old.paths[0].random_pairs_used, selected.random_pairs_used);
    assert_eq!(
        old.paths[0]
            .events
            .iter()
            .map(|e| e.year.to_bits())
            .collect::<Vec<_>>(),
        selected
            .events
            .iter()
            .map(|e| e.year.to_bits())
            .collect::<Vec<_>>()
    );
}

#[test]
fn principal_zero_is_not_automatic_basis_or_accrued_right_retirement() {
    let mut r = request();
    let mut pay = repayment(0.5, 1.0);
    if let BoundaryAction::Repayment {
        interest_fraction, ..
    } = &mut pay.action
    {
        *interest_fraction = [0.0; 3];
    }
    r.raw_credit_path.boundaries.push(pay);
    let out = income::evaluate(&r).unwrap();
    let p = &out.credit().result.paths[0];
    assert_eq!(p.closing_principal, 0.0);
    close(p.closing_accrued_interest, 5.0);
    assert_ne!(p.closing_book_adjustment, 0.0);
    assert_ne!(out.carry_points().last().unwrap().gross_carry, 0.0);
    assert_eq!(p.book_adjustment_recognized, 0.0);
}

#[test]
fn large_principal_actual_cash_residue_is_not_replaced_by_weight_formula() {
    let mut r = request();
    r.raw_credit_path.paths[0].principal = 1e10;
    r.raw_credit_path.paths[0].book_adjustment = 0.0;
    r.original_anchors[0].acquisition_cost = 1e10;
    r.raw_credit_path.intervals[0].hazards_per_year[0][3] = 1.0;
    r.raw_credit_path.intervals[0].loss_given_default[0] = 0.0;
    r.raw_credit_path.intervals[0].complete_recovery_tails[0] = vec![RecoveryWeight {
        lag_years: 0.0,
        weight: 1.0 - 8.0 * f64::EPSILON,
    }];
    let out = income::evaluate(&r).unwrap();
    let p = &out.credit().result.paths[0];
    let claim = p.default.as_ref().unwrap();
    let actual = claim.principal - claim.recoveries[0].principal;
    assert!(actual > 0.0);
    assert_eq!(p.closing_principal.to_bits(), actual.to_bits());
    assert_eq!(
        p.closing_accrued_interest.to_bits(),
        (claim.accrued_interest - claim.recoveries[0].interest).to_bits()
    );
}

#[test]
fn gross_anchors_and_income_are_not_probability_weighted_a_second_time() {
    let mut r = request();
    let mut second = r.raw_credit_path.paths[0].clone();
    second.path_key = 18;
    second.path_id = "second-actual".into();
    r.raw_credit_path.paths.push(second);
    let mut anchor = r.original_anchors[0].clone();
    anchor.path_key = 18;
    r.original_anchors.push(anchor);
    if let DrawStream::Supplied { paths, .. } = &mut r.raw_credit_path.draw_stream {
        paths.push(SuppliedPathDraws {
            path_key: 18,
            pairs: vec![],
        });
    }
    let out = income::evaluate(&r).unwrap();
    for key in [17, 18] {
        let last = out
            .carry_points()
            .iter()
            .rfind(|p| p.path_key == key)
            .unwrap();
        close(last.gross_carry, 107.1);
        close(
            out.carry_points()
                .iter()
                .filter(|p| p.path_key == key)
                .map(|p| p.effective_interest)
                .sum(),
            5.1,
        );
    }
}

#[test]
fn eight_historical_none_full_outputs_remain_byte_identical() {
    for case in 0..8 {
        let mut r = request().raw_credit_path;
        match case {
            1 => r.boundaries.push(repayment(0.5, 0.5)),
            2 => r.intervals[0].hazards_per_year[0][3] = 1.0,
            3 => r.intervals[0].hazards_per_year[0][4] = 1.0,
            4 => r.intervals[0].accrual_rates[0] = -0.1,
            5 => r.boundaries.push(PathBoundary {
                event_id: "full".into(),
                policy_id: "exit".into(),
                year: 1.0,
                action: BoundaryAction::Maturity {},
            }),
            6 => r.boundaries.push(PathBoundary {
                event_id: "draw".into(),
                policy_id: "fund".into(),
                year: 0.25,
                action: BoundaryAction::Draw {
                    undrawn_fraction: [0.5; 3],
                },
            }),
            7 => {
                r.intervals[0].hazards_per_year[0][1] = 1.0;
                r.intervals[0].hazards_per_year[1][0] = 1.0;
                if let DrawStream::Supplied { paths, .. } = &mut r.draw_stream {
                    paths[0].pairs = vec![
                        UniformPair {
                            waiting_uniform: (-0.2_f64).exp(),
                            destination_uniform: 0.5,
                        },
                        UniformPair {
                            waiting_uniform: (-0.3_f64).exp(),
                            destination_uniform: 0.5,
                        },
                        UniformPair {
                            waiting_uniform: (-2.0_f64).exp(),
                            destination_uniform: 0.5,
                        },
                    ];
                }
            }
            _ => (),
        }
        let old: before::CreditPathRequest =
            serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_vec(&evaluate(&r).unwrap()).unwrap(),
            serde_json::to_vec(&before::evaluate(&old).unwrap()).unwrap(),
            "case {case}"
        );
        assert_eq!(
            serde_json::to_vec(&evaluate_traced(&r).unwrap()).unwrap(),
            serde_json::to_vec(&before::evaluate_traced(&old).unwrap()).unwrap(),
            "traced case {case}"
        );
    }
}

struct HandNativeFactors {
    calls: usize,
    two_signed_coupons: bool,
    declared_atoms: usize,
    source_bytes: usize,
}
impl income::NativeEarnedFactors for HandNativeFactors {
    fn bound(&self) -> income::NativeFactorBound {
        income::NativeFactorBound {
            maximum_atoms_per_query: self.declared_atoms,
            retained_source_bytes: self.source_bytes,
            source_work: 100,
        }
    }
    fn earned(
        &mut self,
        query: &income::NativeFactorQuery,
        sink: &mut dyn FnMut(income::NativeFactorAtom<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        self.calls += 1;
        let factor = 0.1 * (query.end_year - query.start_year);
        sink(income::NativeFactorAtom {
            coupon_id: "native-period-a",
            source_event_id: "native-cash-a",
            source_id: "actual-native-source",
            source_revision: "1",
            captured_segment_index: 0,
            delta_factor: factor,
        })?;
        if self.two_signed_coupons {
            sink(income::NativeFactorAtom {
                coupon_id: "native-period-b",
                source_event_id: "native-cash-b",
                source_id: "actual-native-source",
                source_revision: "1",
                captured_segment_index: 1,
                delta_factor: -factor,
            })?;
        }
        Ok(())
    }
}
fn hand_factors() -> HandNativeFactors {
    HandNativeFactors {
        calls: 0,
        two_signed_coupons: false,
        declared_atoms: 1,
        source_bytes: 1024,
    }
}

#[test]
fn borrowed_factor_uses_actual_principal_once_and_ignores_caller_accrual_rate() {
    let mut r = request();
    r.raw_credit_path.intervals[0].accrual_rates = [99.0; 3];
    r.raw_credit_path.boundaries.push(repayment(0.5, 0.5));
    let mut source = hand_factors();
    let out = income::evaluate_with_native_factors(&r, &mut source, &mut || Ok(())).unwrap();
    assert_eq!(source.calls, 2);
    close(out.credit().result.paths[0].earned_interest, 7.5);
    let value = serde_json::to_value(&out).unwrap();
    assert_eq!(value["native_factor_provider_selected"], true);
    assert_eq!(value["complete_coupon_settlement_ownership"], false);
    assert_eq!(
        value["earned_origin_contributions"][0]["actual_principal"],
        100.0
    );
    assert_eq!(
        value["earned_origin_contributions"][1]["actual_principal"],
        50.0
    );
}

#[test]
fn opposing_signed_coupon_origins_are_retained_separately_from_net_accrual() {
    let r = request();
    let mut source = hand_factors();
    source.two_signed_coupons = true;
    source.declared_atoms = 2;
    let out = income::evaluate_with_native_factors(&r, &mut source, &mut || Ok(())).unwrap();
    assert_eq!(out.credit().result.paths[0].closing_accrued_interest, 0.0);
    let value = serde_json::to_value(&out).unwrap();
    let rows = value["earned_origin_contributions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["contractual_earned"], 10.0);
    assert_eq!(rows[1]["contractual_earned"], -10.0);
    assert_ne!(rows[0]["source_event_id"], rows[1]["source_event_id"]);
    assert_eq!(value["corporate_source_binding"], false);
}

#[test]
fn source_bound_refuses_before_any_factor_callback_and_catches_underdeclared_atoms() {
    let r = request();
    let mut source = hand_factors();
    source.source_bytes = income::MAX_RETAINED_BYTES;
    let error = income::evaluate_with_native_factors(&r, &mut source, &mut || Ok(())).unwrap_err();
    assert!(error.contains("before native path generation"));
    assert_eq!(source.calls, 0);
    source = hand_factors();
    source.two_signed_coupons = true;
    let error = income::evaluate_with_native_factors(&r, &mut source, &mut || Ok(())).unwrap_err();
    assert!(error.contains("reserved source bound"));
}

#[test]
fn cancelled_factor_owner_publishes_no_partial_result() {
    let r = request();
    let mut source = hand_factors();
    let error = income::evaluate_with_native_factors(&r, &mut source, &mut || {
        Err("parent cancelled".into())
    })
    .unwrap_err();
    assert!(error.contains("parent cancelled"));
    assert_eq!(source.calls, 0);
}

#[test]
fn historical_none_errors_remain_exact() {
    for case in 0..6 {
        let mut r = request().raw_credit_path;
        match case {
            0 => r.source_id.clear(),
            1 => r.intervals[0].hazards_per_year[0][0] = 1.0,
            2 => r.intervals[0].complete_recovery_tails[0][0].weight = 0.9,
            3 => r.paths[0].principal = -1.0,
            4 => r.intervals[0].end_year = f64::NAN,
            _ => r.max_jumps_per_path = 0,
        }
        if case == 4 {
            // JSON cannot encode NaN; preserve the historical typed invalid-input path.
            let mut old: before::CreditPathRequest =
                serde_json::from_value(serde_json::to_value(request().raw_credit_path).unwrap())
                    .unwrap();
            old.intervals[0].end_year = f64::NAN;
            assert_eq!(
                evaluate(&r).unwrap_err(),
                before::evaluate(&old).unwrap_err()
            );
        } else {
            let old: before::CreditPathRequest =
                serde_json::from_value(serde_json::to_value(&r).unwrap()).unwrap();
            assert_eq!(
                evaluate(&r).unwrap_err(),
                before::evaluate(&old).unwrap_err()
            );
            assert_eq!(
                evaluate_traced(&r).unwrap_err(),
                before::evaluate_traced(&old).unwrap_err()
            );
        }
    }
}
