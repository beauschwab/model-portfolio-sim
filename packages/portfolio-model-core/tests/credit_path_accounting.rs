use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_model_core::credit::{CreditMeasure, CreditState};
use portfolio_model_core::credit_accounting::{self as accounting, *};
use portfolio_model_core::credit_calibration::ObservationTime;
use portfolio_model_core::credit_path::*;
use portfolio_model_core::credit_path_accounting::{
    self as joint, DayMapping, PathAccountingRequest, PathBinding,
};
fn id(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "1".into(),
    }
}
fn policy() -> AccountingPolicy {
    AccountingPolicy {
        identity: id("reviewed-accounting"),
        accrued: AccruedPolicy::TransferUntilReviewedReversal,
        basis: BasisPolicy::TransferUntilReviewedChargeoff,
        allowance_shortfall: AllowanceShortfall::Reject,
        excess_cash_recovery: ExcessCashRecovery::RecognizeInEarnings,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn request() -> PathAccountingRequest {
    PathAccountingRequest {
        schema: joint::SCHEMA.into(),
        day_mapping: DayMapping::CalendarMidnightRightClosed,
        reporting_end_ordinal: 739893,
        timestamp_tolerance_seconds: 1e-6,
        path: CreditPathRequest {
            schema: "credit-path-1".into(),
            measure: CreditMeasure::Physical,
            monetary_unit: "currency_units".into(),
            source_id: "synthetic-credit".into(),
            source_revision: "1".into(),
            model_policy_id: "physical-supplied".into(),
            origin_time: ObservationTime {
                ordinal: 739890,
                seconds_of_day: 0,
            },
            max_jumps_per_path: 10,
            intervals: vec![PathInterval {
                end_year: 3.0 / 365.0,
                hazards_per_year: [[0.0, 0.0, 0.0, 73.0, 0.0], [0.0; 5], [0.0; 5]],
                accrual_rates: [0.365, 0.0, 0.0],
                loss_given_default: [0.4; 3],
                credit_conversion_factor: [0.5; 3],
                interest_recovery_fraction: [1.0; 3],
                complete_recovery_tails: std::array::from_fn(|_| {
                    vec![RecoveryWeight {
                        lag_years: 0.5 / 365.0,
                        weight: 1.0,
                    }]
                }),
            }],
            boundaries: vec![],
            draw_stream: DrawStream::Supplied {
                stream_id: "hand-draws".into(),
                stream_revision: "1".into(),
                paths: vec![SuppliedPathDraws {
                    path_key: 17,
                    pairs: vec![UniformPair {
                        waiting_uniform: (-0.1_f64).exp(),
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
                accrued_interest: 0.0,
                book_adjustment: -3.0,
            }],
        },
        bindings: vec![PathBinding {
            path_key: 17,
            position_id: "loan".into(),
            currency: "USD".into(),
            claim_id: "claim".into(),
            economic_source: id("synthetic-credit"),
            recovery_schedule_review: id("complete-supplied-tail"),
            policy: policy(),
            interest_recovery_policy: id("reviewed-interest-settlement"),
            interest_recovery_excess: ExcessCashRecovery::RecognizeInEarnings,
            opening_allowance: 41.0,
            allowance_reviews: vec![],
            chargeoffs: vec![ReviewedChargeoff {
                review: id("reviewed-chargeoff"),
                claim_id: "claim".into(),
                recognition_ordinal: 739892,
                principal_amount: 44.0,
                book_adjustment_amount: -3.0,
                accrued_interest_reversal: 0.0,
            }],
        }],
    }
}

#[test]
fn exact_default_draw_recovery_and_reviewed_chargeoff_reconcile_daily() {
    let r = request();
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    close(p.economic.default.as_ref().unwrap().year, 0.5 / 365.0);
    close(p.daily[0].earned_interest, 0.05);
    close(p.daily[0].cash_delta, 56.05);
    close(p.daily[0].default_principal, 44.0);
    close(p.daily[0].default_accrued_interest, 0.0);
    close(p.daily[0].default_book_adjustment, -3.0);
    close(p.daily[1].default_principal, 0.0);
    close(p.daily[1].allowance, 0.0);
    close(p.daily[2].cumulative_profit, 0.05);
    close(p.daily[2].net_assets_including_cash, 56.05);
    assert_eq!(p.daily[0].probability, [0., 0., 0., 1., 0., 0., 0.]);
    assert_eq!(p.closing.claims[0].settled_interest_recovery_ids.len(), 1);
    close(p.closing.claims[0].interest_recovery_applied, 0.05);
    assert!(p.intents.iter().any(|x| x
        .exact_event
        .as_ref()
        .is_some_and(|t| (t.elapsed_year - 0.5 / 365.0).abs() < 1e-15)));
    for x in &p.intents {
        close(x.intent.postings.iter().map(|p| p.amount).sum(), 0.0);
    }
}

#[test]
fn negative_coupon_and_default_payable_rights_settle_with_signed_cash() {
    let mut r = request();
    r.path.paths[0].accrued_interest = -2.0;
    r.path.intervals[0].accrual_rates[0] = -0.365;
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    close(p.daily[0].cash_delta, 53.95);
    close(p.daily[0].earned_interest, -0.05);
    close(p.daily[2].cumulative_profit, -0.05);
    close(p.daily[2].net_assets_including_cash, 53.95);
    close(p.closing.claims[0].interest_recovery_cash_collected, -2.05);
    close(p.closing.claims[0].interest_recovery_applied, -2.05);
}

#[test]
fn reversed_default_interest_and_later_cash_are_distinct_from_reversal() {
    let mut r = request();
    r.bindings[0].policy.accrued = AccruedPolicy::ReverseIncomeAtDefault;
    let out = joint::evaluate(&r).unwrap();
    let claim = &out.paths[0].closing.claims[0];
    close(claim.accrued_reversed, 0.05);
    close(claim.interest_recovery_applied, 0.0);
    close(claim.interest_recovery_cash_collected, 0.05);
    close(out.paths[0].daily[2].cumulative_profit, 0.05);
    assert!(out.paths[0].intents.iter().any(|x| x.intent.event
        == "reviewed_physical_interest_recovery_cash"
        && x.intent
            .postings
            .iter()
            .any(|p| p.account == AccountCode::RecoveryIncome)));
}

#[test]
fn cures_migration_and_previous_empty_default_origin_have_no_probability_haircut() {
    let mut r = request();
    r.path.intervals[0].hazards_per_year[0] = [0.0, 73.0, 0.0, 0.0, 0.0];
    r.path.intervals[0].hazards_per_year[1] = [0.0, 0.0, 0.0, 73.0, 0.0];
    r.path.intervals[0].accrual_rates[1] = 0.0;
    if let DrawStream::Supplied { paths, .. } = &mut r.path.draw_stream {
        paths[0].pairs.push(UniformPair {
            waiting_uniform: (-0.1_f64).exp(),
            destination_uniform: 0.5,
        });
    }
    r.bindings[0].chargeoffs.clear();
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert_eq!(p.daily[0].probability, [0., 0., 0., 0., 1., 0., 0.]);
    close(p.economic.earned_interest, 0.05);
    close(p.economic.default.as_ref().unwrap().principal, 110.0);
    close(p.closing.claims[0].initial_principal, 110.0);
    assert_eq!(
        p.economic.default.as_ref().unwrap().origin,
        CreditState::Watch
    );
}

#[test]
fn zero_credit_repayment_and_maturity_keep_actual_cash_and_basis_separate() {
    let mut r = request();
    r.path.intervals[0].hazards_per_year = [[0.0; 5]; 3];
    r.bindings[0].chargeoffs.clear();
    r.bindings[0].opening_allowance = 0.0;
    r.path.boundaries = vec![
        PathBoundary {
            event_id: "repay".into(),
            policy_id: "reviewed-payment".into(),
            year: 1.0 / 365.0,
            action: BoundaryAction::Repayment {
                principal_fraction: [0.5; 3],
                interest_fraction: [1.0; 3],
                cancel_undrawn_fraction: [0.0; 3],
            },
        },
        PathBoundary {
            event_id: "maturity".into(),
            policy_id: "terms".into(),
            year: 2.0 / 365.0,
            action: BoundaryAction::Maturity {},
        },
    ];
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    close(p.daily[0].cash_delta, 50.1);
    close(p.daily[1].cash_delta, 50.05);
    close(p.daily[2].cumulative_profit, 3.15);
    close(p.daily[2].cumulative_cash, 100.15);
    assert_eq!(p.daily[1].matured_probability, 1.0);
    assert_eq!(p.daily[1].probability, [0.0; 7]);
    assert!(p.closing.claims.is_empty());
    assert_eq!(p.economic.outcome, PathOutcome::Matured);
}

#[test]
fn full_recovery_tail_survives_principal_horizon_and_long_calendar_lags() {
    let mut r = request();
    for tail in &mut r.path.intervals[0].complete_recovery_tails {
        tail[0].lag_years = 20.0;
    }
    r.bindings[0].chargeoffs.clear();
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert!(p.complete_recoveries[0].after_reporting);
    assert!(p.closing.claims[0].remaining_recoveries[0].payment_ordinal > r.reporting_end_ordinal);
    close(p.daily[2].default_principal, 110.0);
    close(p.daily[2].default_accrued_interest, 0.05);
    close(p.daily[2].cumulative_cash, -10.0);
}

#[test]
fn incompatible_measure_units_clock_review_sources_and_domains_reject() {
    let mut r = request();
    r.path.measure = CreditMeasure::Pricing;
    assert!(joint::evaluate(&r).is_err());
    let mut r = request();
    r.path.monetary_unit = "USD thousands".into();
    assert!(joint::evaluate(&r).is_err());
    let mut r = request();
    r.reporting_end_ordinal += 1;
    assert!(joint::evaluate(&r).unwrap_err().contains("midnight"));
    let mut r = request();
    r.bindings[0].economic_source.revision = "another".into();
    assert!(joint::evaluate(&r).is_err());
    let mut r = request();
    r.bindings[0].chargeoffs[0].principal_amount = 400.0;
    assert!(joint::evaluate(&r).is_err());
    let mut r = request();
    r.path.intervals[0].complete_recovery_tails[0].resize(
        33,
        RecoveryWeight {
            lag_years: 1.0,
            weight: 0.0,
        },
    );
    assert!(joint::evaluate(&r).unwrap_err().contains("tail admission"));
}

fn default_state(accrued: f64, reverse: bool) -> AccountingState {
    let mut p = policy();
    if reverse {
        p.accrued = AccruedPolicy::ReverseIncomeAtDefault;
    }
    let mut opening = AccountingState {
        live: [LiveState::default(); 3],
        allowance: 10.0,
        claims: vec![],
    };
    opening.live[0] = LiveState {
        principal: 100.0,
        undrawn: 0.0,
        accrued_interest: accrued,
        book_adjustment: 0.0,
    };
    let mut by_origin = [OriginDefaultDelta::default(); 3];
    by_origin[0] = OriginDefaultDelta {
        principal: 100.0,
        undrawn: 0.0,
        contingent_draw: 0.0,
        accrued_interest: accrued,
        book_adjustment: 0.0,
    };
    accounting::evaluate(&CreditAccountingRequest {
        schema: "credit-accounting-1".into(),
        measure: CashflowMeasure::Physical,
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        position_id: "loan".into(),
        recognition_ordinal: 739891,
        reporting_end_ordinal: 739900,
        policy: p,
        opening,
        defaults: vec![DefaultInstruction {
            claim_id: "claim".into(),
            economic_source: id("economic"),
            recovery_schedule_review: id("recovery"),
            by_origin,
            economic_principal_loss: 0.0,
            complete_recovery_tail: vec![RecoveryPoint {
                payment_ordinal: 739899,
                amount: 100.0,
            }],
        }],
        chargeoffs: vec![],
        allowance_review: None,
    })
    .unwrap()
    .closing
}
fn interest_request(
    opening: AccountingState,
    amount: f64,
    excess: ExcessCashRecovery,
) -> InterestRecoveryRequest {
    InterestRecoveryRequest {
        schema: "credit-interest-recovery-1".into(),
        measure: CashflowMeasure::Physical,
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        position_id: "loan".into(),
        recognition_ordinal: 739892,
        reporting_end_ordinal: 739900,
        policy: opening.claims[0].policy.clone(),
        opening,
        settlements: vec![ReviewedInterestRecovery {
            recovery: id("once-only-cash"),
            claim_id: "claim".into(),
            recognition_ordinal: 739892,
            amount,
            policy_review: id("cash-policy"),
            source: id("cash-source"),
            excess_cash: excess,
        }],
    }
}

#[test]
fn shared_interest_helper_conserves_positive_negative_and_replayed_signed_rights() {
    for sign in [1.0, -1.0] {
        let r = interest_request(
            default_state(10.0 * sign, false),
            6.0 * sign,
            ExcessCashRecovery::RecognizeInEarnings,
        );
        let out = settle_interest_recoveries(&r).unwrap();
        let c = &out.closing.claims[0];
        close(c.outstanding_accrued_interest, 4.0 * sign);
        close(c.accrued_reversed, 0.0);
        close(c.interest_recovery_applied, 6.0 * sign);
        close(out.intents[0].postings.iter().map(|p| p.amount).sum(), 0.0);
        let mut replay = r.clone();
        replay.opening = out.closing;
        let before = serde_json::to_string(&replay.opening).unwrap();
        assert!(settle_interest_recoveries(&replay)
            .unwrap_err()
            .contains("already settled"));
        assert_eq!(serde_json::to_string(&replay.opening).unwrap(), before);
    }
}

#[test]
fn shared_interest_excess_is_explicit_for_default_reversal_and_reviewed_chargeoff() {
    let r = interest_request(
        default_state(10.0, true),
        6.0,
        ExcessCashRecovery::CreditAllowance,
    );
    let out = settle_interest_recoveries(&r).unwrap();
    close(out.closing.allowance, 16.0);
    close(out.excess_cash, 6.0);
    close(out.applied_to_rights, 0.0);
    let mut opening = default_state(-10.0, false);
    opening.claims[0].outstanding_accrued_interest = -4.0;
    opening.claims[0].accrued_reversed = -6.0;
    let out = settle_interest_recoveries(&interest_request(
        opening,
        -8.0,
        ExcessCashRecovery::RecognizeInEarnings,
    ))
    .unwrap();
    close(out.applied_to_rights, -4.0);
    close(out.excess_cash, -4.0);
    close(out.closing.claims[0].outstanding_accrued_interest, 0.0);
    assert!(out.intents[0]
        .postings
        .iter()
        .any(|p| p.account == AccountCode::RecoveryIncome && p.amount == 4.0));
}

#[test]
fn historical_claim_records_default_additive_interest_fields_to_zero() {
    let mut json = serde_json::to_value(default_state(10.0, false)).unwrap();
    let claim = json["claims"][0].as_object_mut().unwrap();
    claim.remove("interest_recovery_cash_collected");
    claim.remove("interest_recovery_applied");
    claim.remove("settled_interest_recovery_ids");
    let restored: AccountingState = serde_json::from_value(json).unwrap();
    close(restored.claims[0].interest_recovery_cash_collected, 0.0);
    assert!(restored.claims[0].settled_interest_recovery_ids.is_empty());
}

#[test]
fn after_credit_review_is_after_signed_interest_cash_and_preserves_target() {
    let mut r = request();
    r.bindings[0].policy.accrued = AccruedPolicy::ReverseIncomeAtDefault;
    r.bindings[0].interest_recovery_excess = ExcessCashRecovery::CreditAllowance;
    r.bindings[0].allowance_reviews.push(ReviewedAllowance {
        review: id("after-all-cash"),
        recognition_ordinal: 739891,
        phase: ReviewPhase::AfterCredit,
        target_amount: 41.0,
    });
    let out = joint::evaluate(&r).unwrap();
    close(out.paths[0].daily[0].allowance, 41.0);
    close(out.paths[0].daily[1].allowance, 0.0);
    close(out.paths[0].daily[2].cumulative_profit, 0.05);
    let intents = &out.paths[0].intents;
    let cash = intents
        .iter()
        .position(|x| x.intent.event == "reviewed_physical_interest_recovery_cash")
        .unwrap();
    let target = intents
        .iter()
        .position(|x| x.intent.event == "reviewed_allowance_target")
        .unwrap();
    assert!(cash < target);
}

#[test]
fn same_day_recoveries_coalesce_principal_but_retain_exact_cash_lineage() {
    let mut r = request();
    r.bindings[0].chargeoffs.clear();
    for tail in &mut r.path.intervals[0].complete_recovery_tails {
        *tail = vec![
            RecoveryWeight {
                lag_years: 0.1 / 365.0,
                weight: 0.25,
            },
            RecoveryWeight {
                lag_years: 0.2 / 365.0,
                weight: 0.75,
            },
        ];
    }
    let out = joint::evaluate(&r).unwrap();
    let p = &out.paths[0];
    close(p.daily[0].cash_delta, 56.05);
    assert_eq!(p.complete_recoveries.len(), 2);
    assert_eq!(p.closing.claims[0].settled_interest_recovery_ids.len(), 2);
    let principal = p
        .intents
        .iter()
        .filter(|x| x.intent.event == "modeled_physical_recovery_cash")
        .collect::<Vec<_>>();
    assert_eq!(principal.len(), 1);
    assert_eq!(
        principal[0].event_ids,
        vec!["claim:recovery:0", "claim:recovery:1"]
    );
    assert!(
        p.complete_recoveries[0].exact_time.elapsed_seconds
            < p.complete_recoveries[1].exact_time.elapsed_seconds
    );
}

#[test]
fn trace_is_identical_economics_and_accrual_slices_reconcile_from_raw_exposure() {
    let r = request();
    let ordinary = portfolio_model_core::credit_path::evaluate(&r.path).unwrap();
    let traced = evaluate_traced(&r.path).unwrap();
    assert_eq!(
        serde_json::to_string(&ordinary).unwrap(),
        serde_json::to_string(&traced.result).unwrap()
    );
    let out = joint::evaluate(&r).unwrap();
    let accrual = out.paths[0]
        .intents
        .iter()
        .find_map(|x| x.accrual.as_ref())
        .unwrap();
    close(accrual.principal, 100.0);
    close(accrual.end_year - accrual.start_year, 0.5 / 365.0);
    close(
        accrual.earned_interest,
        accrual.principal * accrual.accrual_rate * (accrual.end_year - accrual.start_year),
    );
}

#[test]
fn calendar_ties_and_opening_instant_and_invalid_live_helper_inputs_fail_explicitly() {
    let mut r = request();
    r.path.intervals[0].hazards_per_year = [[0.0; 5]; 3];
    r.bindings[0].chargeoffs.clear();
    r.path.boundaries.push(PathBoundary {
        event_id: "maturity".into(),
        policy_id: "terms-original".into(),
        year: 1.0 / 365.0,
        action: BoundaryAction::Maturity {},
    });
    let out = joint::evaluate(&r).unwrap();
    close(out.paths[0].daily[0].matured_probability, 1.0);
    let cash = out.paths[0]
        .intents
        .iter()
        .find(|x| x.intent.event == "realized_maturity")
        .unwrap();
    assert_eq!(
        cash.contractual_policy_id.as_deref(),
        Some("terms-original")
    );
    assert_eq!(cash.intent.policy.id, "reviewed-accounting");
    r.path.boundaries[0].year = 0.0;
    assert!(joint::evaluate(&r).is_err());
    let mut invalid = interest_request(
        default_state(10.0, false),
        1.0,
        ExcessCashRecovery::RecognizeInEarnings,
    );
    invalid.opening.live[0].principal = -1.0;
    assert!(settle_interest_recoveries(&invalid).is_err());
}

#[test]
fn canonical_synthetic_example_is_strict_and_matches_independent_hand_amounts() {
    let raw = include_str!("../../../examples/models/credit-path-accounting-1.json");
    let r: PathAccountingRequest = serde_json::from_str(raw).unwrap();
    let out = joint::evaluate(&r).unwrap();
    close(out.paths[0].daily[0].cash_delta, 56.05);
    close(out.paths[0].daily[2].cumulative_profit, 0.05);
    close(out.paths[0].closing.allowance, 0.0);
    let mut unknown: serde_json::Value = serde_json::from_str(raw).unwrap();
    unknown["auto_ecl"] = true.into();
    assert!(serde_json::from_value::<PathAccountingRequest>(unknown).is_err());
}
