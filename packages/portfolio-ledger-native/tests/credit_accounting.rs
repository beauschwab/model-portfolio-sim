use portfolio_ledger_native::credit_accounting::*;
use portfolio_model_core::cashflow::{ordinal_from_ymd, CashflowMeasure};

fn day(n: u32) -> i32 {
    ordinal_from_ymd(2026, 1, n).unwrap()
}
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn request() -> CreditAccountingRequest {
    CreditAccountingRequest {
        schema: SCHEMA.into(),
        measure: CashflowMeasure::Physical,
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        position_id: "loan:1".into(),
        recognition_ordinal: day(1),
        reporting_end_ordinal: day(2),
        policy: AccountingPolicy {
            identity: id("accounting-review"),
            accrued: AccruedPolicy::ReverseIncomeAtDefault,
            basis: BasisPolicy::TransferUntilReviewedChargeoff,
            allowance_shortfall: AllowanceShortfall::Reject,
            excess_cash_recovery: ExcessCashRecovery::CreditAllowance,
        },
        opening: AccountingState {
            live: [
                LiveState {
                    principal: 100.,
                    undrawn: 20.,
                    accrued_interest: 10.,
                    book_adjustment: 0.,
                },
                LiveState::default(),
                LiveState::default(),
            ],
            allowance: 30.,
            claims: vec![],
        },
        defaults: vec![DefaultInstruction {
            claim_id: "default-1".into(),
            economic_source: id("physical-credit"),
            recovery_schedule_review: id("recovery-review"),
            by_origin: [
                OriginDefaultDelta {
                    principal: 40.,
                    undrawn: 8.,
                    contingent_draw: 0.,
                    accrued_interest: 4.,
                    book_adjustment: 0.,
                },
                OriginDefaultDelta::default(),
                OriginDefaultDelta::default(),
            ],
            economic_principal_loss: 24.,
            complete_recovery_tail: vec![RecoveryPoint {
                payment_ordinal: day(3),
                amount: 16.,
            }],
        }],
        chargeoffs: vec![ReviewedChargeoff {
            review: id("chargeoff-review"),
            claim_id: "default-1".into(),
            recognition_ordinal: day(1),
            principal_amount: 24.,
            book_adjustment_amount: 0.,
            accrued_interest_reversal: 0.,
        }],
        allowance_review: None,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn postings(result: &CreditAccountingResult, account: AccountCode) -> f64 {
    result
        .intents
        .iter()
        .flat_map(|j| &j.postings)
        .filter(|p| p.account == account)
        .map(|p| p.amount)
        .sum()
}
fn balanced(result: &CreditAccountingResult) {
    for intent in &result.intents {
        close(intent.postings.iter().map(|p| p.amount).sum(), 0.);
        assert_eq!(intent.policy, result.policy.identity);
    }
}
fn resume(opening: AccountingState, n: u32) -> CreditAccountingRequest {
    let mut req = request();
    req.opening = opening;
    req.recognition_ordinal = day(n);
    req.reporting_end_ordinal = day(n);
    req.defaults.clear();
    req.chargeoffs.clear();
    req
}

#[test]
fn allowance_covered_chargeoff_and_deferred_recovery_reconcile() {
    let req = request();
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.live[0].principal, 60.);
    close(result.closing.live[0].undrawn, 12.);
    close(result.closing.live[0].accrued_interest, 6.);
    close(result.closing.allowance, 6.);
    close(result.economic_principal_loss, 24.);
    close(result.recovery_tail_after_reporting, 16.);
    close(postings(&result, AccountCode::CreditLossExpense), 0.);
    close(postings(&result, AccountCode::InterestIncome), 4.);
    close(postings(&result, AccountCode::DefaultPrincipal), 16.);
    assert_eq!(result.closing.claims[0].remaining_recoveries.len(), 1);
    let finalday = evaluate(&resume(result.closing, 3)).unwrap();
    balanced(&finalday);
    close(finalday.modeled_recovery_cash, 16.);
    close(postings(&finalday, AccountCode::Cash), 16.);
    close(finalday.closing.claims[0].outstanding_principal, 0.);
    assert!(finalday.closing.claims[0].remaining_recoveries.is_empty());
    close(finalday.closing.allowance, 6.);
    assert!(finalday.intents[0].event.contains("modeled_physical"));
}

#[test]
fn uncovered_chargeoff_requires_explicit_shortfall_policy_or_prior_review() {
    let mut req = request();
    req.opening.allowance = 10.;
    let original = serde_json::to_value(&req.opening).unwrap();
    assert!(evaluate(&req)
        .err()
        .unwrap()
        .contains("available allowance"));
    assert_eq!(serde_json::to_value(&req.opening).unwrap(), original);
    req.policy.allowance_shortfall = AllowanceShortfall::ExpenseAtReviewedChargeoff;
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.allowance, 0.);
    close(postings(&result, AccountCode::CreditLossExpense), 14.);
    req.policy.allowance_shortfall = AllowanceShortfall::Reject;
    req.allowance_review = Some(ReviewedAllowance {
        review: id("allowance-target"),
        recognition_ordinal: day(1),
        phase: ReviewPhase::BeforeCredit,
        target_amount: 30.,
    });
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(postings(&result, AccountCode::CreditLossExpense), 20.);
    close(result.closing.allowance, 6.);
    req.allowance_review.as_mut().unwrap().phase = ReviewPhase::AfterCredit;
    assert!(evaluate(&req).is_err()); // closing review cannot fund earlier chargeoff
}

#[test]
fn economic_loss_does_not_choose_allowance_or_chargeoff_timing() {
    let mut req = request();
    req.chargeoffs.clear();
    let default = evaluate(&req).unwrap();
    balanced(&default);
    close(default.closing.allowance, 30.);
    close(default.closing.claims[0].outstanding_principal, 40.);
    close(postings(&default, AccountCode::CreditLossExpense), 0.);
    let mut next = resume(default.closing, 2);
    next.chargeoffs = request().chargeoffs;
    next.chargeoffs[0].recognition_ordinal = day(2);
    let reviewed = evaluate(&next).unwrap();
    balanced(&reviewed);
    close(reviewed.closing.claims[0].outstanding_principal, 16.);
    close(reviewed.closing.allowance, 6.);
    next.chargeoffs[0].recognition_ordinal = day(3);
    assert!(evaluate(&next).is_err()); // future review is not recognized today
}

#[test]
fn full_writeoff_recovers_through_explicit_allowance_or_income_policy() {
    let mut req = request();
    req.opening.allowance = 40.;
    req.chargeoffs[0].principal_amount = 40.;
    let default = evaluate(&req).unwrap();
    balanced(&default);
    close(default.closing.claims[0].outstanding_principal, 0.);
    assert_eq!(
        default.closing.claims[0].remaining_recoveries[0].amount,
        16.
    );
    let recovered = evaluate(&resume(default.closing, 3)).unwrap();
    balanced(&recovered);
    close(recovered.closing.allowance, 16.);
    close(postings(&recovered, AccountCode::Allowance), -16.);
    close(postings(&recovered, AccountCode::DefaultPrincipal), 0.);
    req.policy.excess_cash_recovery = ExcessCashRecovery::RecognizeInEarnings;
    let default = evaluate(&req).unwrap();
    let mut next = resume(default.closing, 3);
    next.policy = req.policy;
    let recovered = evaluate(&next).unwrap();
    balanced(&recovered);
    close(recovered.closing.allowance, 0.);
    close(postings(&recovered, AccountCode::RecoveryIncome), -16.);
}

#[test]
fn default_contingent_draw_is_funded_before_credit_transfer() {
    let mut req = request();
    req.defaults[0].by_origin[0].contingent_draw = 4.;
    req.defaults[0].economic_principal_loss = 26.4;
    req.defaults[0].complete_recovery_tail[0].amount = 17.6;
    req.chargeoffs[0].principal_amount = 26.4;
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.contingent_draw_cash, 4.);
    close(postings(&result, AccountCode::Cash), -4.);
    close(postings(&result, AccountCode::LivePrincipal), -40.);
    close(result.closing.live[0].principal, 60.);
    close(result.closing.live[0].undrawn, 12.);
    close(result.closing.claims[0].initial_principal, 44.);
    assert_eq!(result.intents[0].event, "default_contingent_draw");
    req.defaults[0].by_origin[0].contingent_draw = 8.1;
    assert!(evaluate(&req).is_err());
}

#[test]
fn accrued_and_premium_discount_are_separate_from_principal_lgd() {
    let mut req = request();
    req.opening.live[0].book_adjustment = -5.;
    req.defaults[0].by_origin[0].book_adjustment = -2.;
    req.policy.accrued = AccruedPolicy::TransferUntilReviewedReversal;
    req.chargeoffs[0].book_adjustment_amount = -2.;
    req.chargeoffs[0].accrued_interest_reversal = 4.;
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.live[0].book_adjustment, -3.);
    close(result.closing.allowance, 8.); // reviewed net principal+basis loss is22
    close(result.economic_principal_loss, 24.); // principal-only economic loss unchanged
    close(postings(&result, AccountCode::InterestIncome), 4.);
    close(postings(&result, AccountCode::DefaultAccruedInterest), 0.);
    req.policy.basis = BasisPolicy::RecognizeInEarningsAtDefault;
    req.chargeoffs[0].book_adjustment_amount = 0.;
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.allowance, 6.);
    close(postings(&result, AccountCode::BasisRecognitionExpense), -2.);
    req.opening.live[0].book_adjustment = 5.;
    req.defaults[0].by_origin[0].book_adjustment = 2.;
    let result = evaluate(&req).unwrap();
    close(postings(&result, AccountCode::BasisRecognitionExpense), 2.);
}

#[test]
fn signed_prepaid_interest_rights_are_not_added_to_principal_lgd() {
    let mut req = request();
    req.opening.live[0].accrued_interest = -10.;
    req.defaults[0].by_origin[0].accrued_interest = -4.;
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.live[0].accrued_interest, -6.);
    close(postings(&result, AccountCode::InterestIncome), -4.);
    close(result.economic_principal_loss, 24.);
}

#[test]
fn zero_credit_and_final_day_zero_lag_recovery_have_exact_event_order() {
    let mut req = request();
    req.defaults.clear();
    req.chargeoffs.clear();
    let result = evaluate(&req).unwrap();
    assert!(result.intents.is_empty());
    assert_eq!(
        serde_json::to_value(&result.closing).unwrap(),
        serde_json::to_value(&req.opening).unwrap()
    );
    req = request();
    req.reporting_end_ordinal = day(1);
    req.defaults[0].complete_recovery_tail[0].payment_ordinal = day(1);
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.claims[0].outstanding_principal, 0.);
    close(result.modeled_recovery_cash, 16.);
    assert!(result.closing.claims[0].remaining_recoveries.is_empty());
    assert_eq!(
        result.intents.last().unwrap().event,
        "modeled_physical_recovery_cash"
    );
}

#[test]
fn reviewed_closing_allowance_is_independent_and_resume_policy_must_match() {
    let mut req = request();
    req.allowance_review = Some(ReviewedAllowance {
        review: id("closing-target"),
        recognition_ordinal: day(1),
        phase: ReviewPhase::AfterCredit,
        target_amount: 9.,
    });
    let result = evaluate(&req).unwrap();
    balanced(&result);
    close(result.closing.allowance, 9.);
    close(postings(&result, AccountCode::CreditLossExpense), 3.);
    let mut next = resume(result.closing, 2);
    next.policy.excess_cash_recovery = ExcessCashRecovery::RecognizeInEarnings;
    assert!(evaluate(&next).err().unwrap().contains("accounting policy"));
}

#[test]
fn invalid_measure_unit_tail_rights_dates_and_claims_fail_atomically() {
    let base = request();
    let mut req = base.clone();
    req.measure = CashflowMeasure::Pricing;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.monetary_unit = "millions".into();
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.defaults[0].complete_recovery_tail[0].amount = 15.;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.defaults[0].by_origin[0].accrued_interest = 11.;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.defaults[0].by_origin[0].principal = 101.;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.opening.live[0].principal = f64::NAN;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.defaults[0].economic_principal_loss = -1.;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.defaults.push(req.defaults[0].clone());
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.chargeoffs[0].claim_id = "unknown".into();
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.reporting_end_ordinal = 0;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.policy.identity.id = "x".repeat(1025);
    assert!(evaluate(&req).is_err());
    let initial = evaluate(&base).unwrap();
    let mut next = resume(initial.closing, 2);
    next.opening.claims[0].outstanding_principal = 15.;
    assert!(evaluate(&next).is_err());
    next.opening.claims[0].outstanding_principal = 16.;
    next.opening.claims[0].remaining_recoveries.clear();
    assert!(evaluate(&next).is_err());
}
