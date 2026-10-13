//! Synthetic native-provider controls, not completed corporate source adoption.
use portfolio_model_core::{
    credit::{CreditMeasure, CreditState},
    credit_calibration::ObservationTime,
    credit_path::*,
    credit_path_income as income,
};

fn request() -> income::Request {
    income::Request {
        schema: income::SCHEMA.into(),
        raw_credit_path: CreditPathRequest {
            schema: SCHEMA.into(),
            measure: CreditMeasure::Physical,
            monetary_unit: "USD".into(),
            source_id: "actual-contract".into(),
            source_revision: "1".into(),
            model_policy_id: "reviewed".into(),
            origin_time: ObservationTime {
                ordinal: 739890,
                seconds_of_day: 0,
            },
            max_jumps_per_path: 4,
            intervals: vec![PathInterval {
                end_year: 1.,
                hazards_per_year: [[0.; 5]; 3],
                accrual_rates: [0.; 3],
                loss_given_default: [0.; 3],
                credit_conversion_factor: [0.; 3],
                interest_recovery_fraction: [0.; 3],
                complete_recovery_tails: std::array::from_fn(|_| {
                    vec![RecoveryWeight {
                        lag_years: 0.,
                        weight: 1.,
                    }]
                }),
            }],
            boundaries: vec![payment("principal", 0.25), payment("coupon", 1.)],
            draw_stream: DrawStream::CounterSplitmix64V1 {
                stream_id: "crn".into(),
                stream_revision: "1".into(),
                seed: 7,
            },
            paths: vec![PathOpening {
                path_id: "loan".into(),
                path_key: 17,
                initial_live_state: CreditState::Performing,
                principal: 100.,
                undrawn_commitment: 0.,
                accrued_interest: 0.,
                book_adjustment: 0.,
            }],
        },
        original_anchors: vec![income::OriginalAnchor {
            path_key: 17,
            original_yield_anchor: "original".into(),
            acquisition_cost: 100.,
            original_annual_effective_rate: 0.05,
        }],
        recognition_rates: vec![income::RecognitionRate {
            interval_index: 0,
            review: "original-only".into(),
            annual_effective_rates: [0.05; 3],
        }],
        policy_review: "synthetic-full-accrual".into(),
        partial_payment_policy: "cash_reduces_gross_no_proportional_basis_disposal-1".into(),
        default_policy: "stop_income_transfer_actual_gross_retain_nominal_claims-1".into(),
    }
}
fn payment(id: &str, year: f64) -> PathBoundary {
    PathBoundary {
        event_id: id.into(),
        policy_id: "actual-contract".into(),
        year,
        action: BoundaryAction::Repayment {
            principal_fraction: [0.; 3],
            interest_fraction: [0.; 3],
            cancel_undrawn_fraction: [0.; 3],
        },
    }
}
struct Source {
    cash_bound: usize,
    queries: usize,
}
impl income::NativeEarnedFactors for Source {
    fn bound(&self) -> income::NativeFactorBound {
        income::NativeFactorBound {
            maximum_atoms_per_query: 1,
            retained_source_bytes: 1024,
            source_work: 100,
        }
    }
    fn maximum_source_cash_per_path(&self) -> usize {
        self.cash_bound
    }
    fn maximum_gross_coupon_claims_per_path(&self) -> usize {
        1
    }
    fn earned(
        &mut self,
        q: &income::NativeFactorQuery,
        sink: &mut dyn FnMut(income::NativeFactorAtom<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        self.queries += 1;
        sink(income::NativeFactorAtom {
            coupon_id: "coupon",
            source_event_id: "coupon",
            source_id: "actual-contract",
            source_revision: "1",
            captured_segment_index: 0,
            delta_factor: 0.1 * (q.end_year - q.start_year),
        })
    }
    fn entitlement(
        &self,
        _a: &income::NativeFactorAtom<'_>,
    ) -> Result<Option<income::NativeCouponEntitlement>, String> {
        Ok(Some(income::NativeCouponEntitlement {
            native_audit_index: 0,
            native_cash_index: 1,
            opening_entitlement_principal: 100.,
        }))
    }
    fn contractual_cash(
        &self,
        id: &str,
        year: f64,
    ) -> Result<Option<income::NativeContractualCash>, String> {
        let (principal, interest, index) = match (id, year) {
            ("principal", 0.25) => (100., 0., 0),
            ("coupon", 1.) => (0., 10., 1),
            _ => return Err("unmapped actual cash".into()),
        };
        Ok(Some(income::NativeContractualCash {
            coupon_id: "coupon".into(),
            source_event_id: id.into(),
            native_cash_index: index,
            principal,
            interest,
            fee: 0.,
            tranche_id: None,
        }))
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn native_coupon_entitlement_survives_p0_until_actual_payment() {
    let result = income::evaluate_with_native_factors(
        &request(),
        &mut Source {
            cash_bound: 2,
            queries: 0,
        },
        &mut || Ok(()),
    )
    .unwrap();
    let path = &result.credit().result.paths[0];
    close(path.closing_principal, 0.);
    close(path.earned_interest, 10.);
    close(path.cash_received, 110.);
    close(path.closing_accrued_interest, 0.);
    assert!(result
        .credit()
        .sojourns
        .iter()
        .any(|s| s.principal == 0. && s.earned_interest > 0.));
    assert!(result
        .earned_origin_contributions()
        .iter()
        .all(|c| c.actual_principal == 100. && c.native_entitlement.is_some()));
    assert_eq!(result.native_source_cash().len(), 2);
    close(result.native_source_cash()[0].cash.principal, 100.);
    close(result.native_source_cash()[1].cash.interest, 10.);
}

#[test]
fn underdeclared_cash_bound_refuses_before_unreserved_receipt() {
    let error = income::evaluate_with_native_factors(
        &request(),
        &mut Source {
            cash_bound: 1,
            queries: 0,
        },
        &mut || Ok(()),
    )
    .unwrap_err();
    assert!(error.contains("declared per-path bound"), "{error}");
}

#[test]
fn shape_admission_and_cancellation_precede_factor_queries() {
    let mut source = Source {
        cash_bound: 3,
        queries: 0,
    };
    assert!(
        income::evaluate_with_native_factors(&request(), &mut source, &mut || Ok(()))
            .unwrap_err()
            .contains("complete boundary count")
    );
    assert_eq!(source.queries, 0);
    assert!(
        income::evaluate_with_native_factors(&request(), &mut source, &mut || Err("stop".into()))
            .unwrap_err()
            .contains("stop")
    );
    assert_eq!(source.queries, 0);
}

fn policy(
    mode: portfolio_model_core::commercial_coupon_accounting::Recognition,
) -> portfolio_model_core::commercial_coupon_accounting::Policy {
    use portfolio_model_core::{commercial_coupon_accounting as book, credit_accounting::Identity};
    book::Policy {
        identity: Identity {
            id: "synthetic-reviewed".into(),
            revision: "1".into(),
        },
        recognition: vec![book::RecognitionReview {
            interval_index: 0,
            review: Identity {
                id: "explicit-accrual".into(),
                revision: "1".into(),
            },
            by_origin: [mode; 3],
        }],
        cash_application: "exact_native_coupon_cash_origin_fifo-1".into(),
        unrecognized_cash: "defer_unrecognized_cash_until_reviewed_recognition-1".into(),
        exit_disposition: "collect_legal_balance_reverse_recognized_realize_remaining_carry-1"
            .into(),
        default_disposition:
            "transfer_all_gross_legal_and_recognized_claims_no_implicit_chargeoff-1".into(),
    }
}

#[test]
fn actual_carry_compilation_separates_recognized_from_legal_and_acquires_once() {
    use portfolio_model_core::{
        commercial_coupon_accounting as book, credit_accounting::AccountCode,
    };
    let mut raw = request();
    raw.raw_credit_path.paths[0].book_adjustment = 10.;
    raw.original_anchors[0].acquisition_cost = 110.;
    raw.original_anchors[0].original_annual_effective_rate = 0.;
    raw.recognition_rates[0].annual_effective_rates = [0.; 3];
    let result = income::evaluate_with_native_factors(
        &raw,
        &mut Source {
            cash_bound: 2,
            queries: 0,
        },
        &mut || Ok(()),
    )
    .unwrap();
    let full = book::compile(
        &result,
        &policy(book::Recognition::FullAccrual),
        &mut || Ok(()),
    )
    .unwrap();
    let stopped = book::compile(
        &result,
        &policy(book::Recognition::LegalOnlyIncomeStopped),
        &mut || Ok(()),
    )
    .unwrap();
    close(full.closing()[0].legal_interest, 0.);
    close(full.closing()[0].recognized_interest, 0.);
    close(full.closing()[0].book_adjustment, 0.);
    close(stopped.closing()[0].legal_interest, 0.);
    close(stopped.closing()[0].recognized_interest, -10.);
    close(stopped.closing()[0].book_adjustment, 10.);
    assert_eq!(
        full.groups()
            .iter()
            .filter(|g| g.phase == "reviewed_opening_snapshot")
            .count(),
        1
    );
    close(full.groups()[0].cash, -110.);
    assert!(full.groups().iter().all(|g| g
        .postings
        .iter()
        .all(|l| l.posting.account != AccountCode::InterestIncome)));
    assert!(stopped
        .groups()
        .iter()
        .any(|g| g.next.legal_interest > 0. && g.next.recognized_interest == 0.));
}

#[test]
fn zero_accounting_rate_does_not_implicitly_select_nonaccrual() {
    use portfolio_model_core::commercial_coupon_accounting as book;
    let mut raw = request();
    raw.recognition_rates[0].annual_effective_rates = [0.; 3];
    let result = income::evaluate_with_native_factors(
        &raw,
        &mut Source {
            cash_bound: 2,
            queries: 0,
        },
        &mut || Ok(()),
    )
    .unwrap();
    let full = book::compile(
        &result,
        &policy(book::Recognition::FullAccrual),
        &mut || Ok(()),
    )
    .unwrap();
    assert!(full
        .groups()
        .iter()
        .any(|g| g.next.recognized_interest > 0.));
    let mut missing = policy(book::Recognition::LegalOnlyIncomeStopped);
    missing.unrecognized_cash = "infer_from_zero_rate".into();
    assert!(book::compile(&result, &missing, &mut || Ok(()))
        .unwrap_err()
        .contains("unsupported explicit"));
}

#[test]
fn native_gross_default_recovery_is_source_owned_and_unpaid_right_is_retained() {
    use portfolio_model_core::commercial_coupon_accounting as book;
    let mut raw = request();
    raw.raw_credit_path.boundaries.clear();
    raw.raw_credit_path.intervals[0].hazards_per_year[0][3] = 1.;
    raw.raw_credit_path.intervals[0].interest_recovery_fraction = [0.5; 3];
    raw.raw_credit_path.draw_stream = DrawStream::Supplied {
        stream_id: "default-crn".into(),
        stream_revision: "1".into(),
        paths: vec![SuppliedPathDraws {
            path_key: 17,
            pairs: vec![UniformPair {
                waiting_uniform: (-0.5_f64).exp(),
                destination_uniform: 0.5,
            }],
        }],
    };
    let result = income::evaluate_with_native_factors(
        &raw,
        &mut Source {
            cash_bound: 0,
            queries: 0,
        },
        &mut || Ok(()),
    )
    .unwrap();
    close(result.native_coupon_recoveries()[0].interest, 2.5);
    let compiled = book::compile(
        &result,
        &policy(book::Recognition::FullAccrual),
        &mut || Ok(()),
    )
    .unwrap();
    close(compiled.closing()[0].principal, 0.);
    close(compiled.closing()[0].legal_interest, 2.5);
    close(compiled.closing()[0].recognized_interest, 2.5);
    assert!(compiled.closing()[0].lots[0].default_claim);
    assert!(compiled
        .groups()
        .iter()
        .any(|g| g.phase == "default_gross_transfer" && g.next.lots[0].legal_remaining == 5.));
}

struct ZeroNet;
impl income::NativeEarnedFactors for ZeroNet {
    fn bound(&self) -> income::NativeFactorBound {
        income::NativeFactorBound {
            maximum_atoms_per_query: 1,
            retained_source_bytes: 1024,
            source_work: 100,
        }
    }
    fn maximum_source_cash_per_path(&self) -> usize {
        1
    }
    fn maximum_gross_coupon_claims_per_path(&self) -> usize {
        1
    }
    fn earned(
        &mut self,
        q: &income::NativeFactorQuery,
        sink: &mut dyn FnMut(income::NativeFactorAtom<'_>) -> Result<(), String>,
    ) -> Result<(), String> {
        sink(income::NativeFactorAtom {
            coupon_id: "coupon",
            source_event_id: "coupon",
            source_id: "actual-contract",
            source_revision: "1",
            captured_segment_index: 0,
            delta_factor: -(q.end_year - q.start_year),
        })
    }
    fn entitlement(
        &self,
        _: &income::NativeFactorAtom<'_>,
    ) -> Result<Option<income::NativeCouponEntitlement>, String> {
        Ok(Some(income::NativeCouponEntitlement {
            native_audit_index: 0,
            native_cash_index: 0,
            opening_entitlement_principal: 100.,
        }))
    }
    fn contractual_cash(
        &self,
        id: &str,
        year: f64,
    ) -> Result<Option<income::NativeContractualCash>, String> {
        if id != "coupon" || year != 1. {
            return Err("unmapped actual zero-net event".into());
        }
        Ok(Some(income::NativeContractualCash {
            coupon_id: "coupon".into(),
            source_event_id: id.into(),
            native_cash_index: 0,
            principal: 100.,
            interest: -100.,
            fee: 0.,
            tranche_id: None,
        }))
    }
}

#[test]
fn zero_net_cash_does_not_erase_principal_coupon_or_actual_event_identity() {
    use portfolio_model_core::{
        commercial_coupon_accounting as book, credit_accounting::AccountCode,
    };
    let mut raw = request();
    raw.raw_credit_path.boundaries = vec![payment("coupon", 1.)];
    raw.raw_credit_path.paths[0].book_adjustment = -100.;
    raw.original_anchors[0].acquisition_cost = 0.;
    raw.original_anchors[0].original_annual_effective_rate = 0.;
    raw.recognition_rates[0].annual_effective_rates = [0.; 3];
    let result = income::evaluate_with_native_factors(&raw, &mut ZeroNet, &mut || Ok(())).unwrap();
    let event = &result.credit().result.paths[0].events[0];
    assert_eq!(event.cash, 0.);
    assert_eq!(event.principal, 100.);
    assert_eq!(event.interest, -100.);
    assert_eq!(result.native_source_cash().len(), 1);
    let compiled = book::compile(
        &result,
        &policy(book::Recognition::FullAccrual),
        &mut || Ok(()),
    )
    .unwrap();
    let group = compiled
        .groups()
        .iter()
        .find(|g| g.phase == "contractual_cash")
        .unwrap();
    assert_eq!(group.source_event_id.as_deref(), Some("coupon"));
    assert!(group
        .postings
        .iter()
        .any(|l| l.posting.account == AccountCode::LivePrincipal && l.posting.amount == -100.));
    assert!(
        group
            .postings
            .iter()
            .any(|l| l.posting.account == AccountCode::LiveAccruedInterest
                && l.posting.amount == 100.)
    );
    assert_eq!(compiled.closing()[0].principal, 0.);
    assert_eq!(compiled.closing()[0].legal_interest, 0.);
}
