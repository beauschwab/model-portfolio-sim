use portfolio_ledger_native::credit_accounting::*;
use portfolio_ledger_native::credit_state::{self, *};
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_model_core::credit::CreditMeasure;

fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn day() -> i32 {
    ordinal_from_ymd(2026, 1, 1).unwrap()
}
fn contract() -> CreditStateContract {
    CreditStateContract {
        schema: credit_state::SCHEMA.into(),
        identity: id("credit-contract"),
        measure: CreditMeasure::Physical,
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        position_id: "loan:1".into(),
        claim_namespace: "loan1-credit".into(),
        economic_source: id("observed-hazard-vintage"),
        recovery_schedule_review: id("recovery-review"),
        accounting_policy: AccountingPolicy {
            identity: id("accounting-review"),
            accrued: AccruedPolicy::TransferUntilReviewedReversal,
            basis: BasisPolicy::TransferUntilReviewedChargeoff,
            allowance_shortfall: AllowanceShortfall::Reject,
            excess_cash_recovery: ExcessCashRecovery::CreditAllowance,
        },
        interval_days: 365,
        transition_hazards_per_year: [[0.; 5]; 3],
        accrual_rates: [0.; 3],
        loss_given_default: [0.4; 3],
        credit_conversion_factor: [0.5; 3],
        complete_recovery_tails: std::array::from_fn(|_| {
            vec![RecoveryFraction {
                lag_days: 2,
                weight: 1.,
            }]
        }),
    }
}
fn opening(c: &CreditStateContract) -> CreditStateOpening {
    CreditStateOpening {
        schema: OPENING_SCHEMA.into(),
        contract_identity: c.identity.clone(),
        last_ordinal: day(),
        probability: [1., 0., 0., 0., 0., 0., 0.],
        accounting: AccountingState {
            live: [
                LiveState {
                    principal: 100.,
                    undrawn: 20.,
                    accrued_interest: 0.,
                    book_adjustment: -5.,
                },
                LiveState::default(),
                LiveState::default(),
            ],
            allowance: 7.,
            claims: vec![],
        },
    }
}
fn step(c: &CreditStateContract) -> CreditStateStep {
    CreditStateStep {
        recognition_ordinal: day() + c.interval_days as i32,
        reporting_end_ordinal: day() + c.interval_days as i32,
        chargeoffs: vec![],
        allowance_review: None,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-10, "{a} != {b}");
}
fn balanced(result: &CreditStateResult) {
    for intent in &result.intents {
        close(intent.postings.iter().map(|p| p.amount).sum(), 0.);
    }
}
fn posting(result: &CreditStateResult, account: AccountCode) -> f64 {
    result
        .intents
        .iter()
        .flat_map(|j| &j.postings)
        .filter(|p| p.account == account)
        .map(|p| p.amount)
        .sum()
}

#[test]
fn competing_exits_negative_coupon_and_ead_match_closed_form() {
    let mut c = contract();
    c.transition_hazards_per_year[0][3] = 0.2;
    c.transition_hazards_per_year[0][4] = 0.3;
    c.accrual_rates[0] = -0.1;
    let result = compile(&c)
        .unwrap()
        .advance(&opening(&c), &step(&c))
        .unwrap();
    let live = (-0.5_f64).exp();
    let exit = 1. - live;
    let default = 0.2 / 0.5 * exit;
    let prepay = 0.3 / 0.5 * exit;
    let accrued_default = -10. * 0.2 / (0.5 * 0.5) * (1. - 1.5 * live);
    let accrued_prepay = -10. * 0.3 / (0.5 * 0.5) * (1. - 1.5 * live);
    close(result.closing.probability[0], live);
    close(result.closing.probability[3], default);
    close(result.closing.probability[6], prepay);
    close(result.closing.accounting.live[0].principal, 100. * live);
    close(
        result.closing.accounting.live[0].accrued_interest,
        -10. * live,
    );
    close(result.default_by_origin[0].principal, 100. * default);
    close(result.default_by_origin[0].contingent_draw, 10. * default);
    close(
        result.default_by_origin[0].accrued_interest,
        accrued_default,
    );
    close(result.earned_interest, -10. * exit / 0.5);
    close(result.economic_principal_loss, 110. * default * 0.4);
    close(result.prepaid.cash, 100. * prepay + accrued_prepay);
    close(result.prepaid.book_adjustment, -5. * prepay);
    close(result.prepaid.undrawn_cancelled, 20. * prepay);
    close(result.recovery_tail_after_reporting, 110. * default * 0.6);
    close(result.closing.accounting.allowance, 7.); // Economic EL is not a provision target.
    close(
        posting(&result, AccountCode::LivePrincipal),
        100. * live - 100.,
    );
    close(
        posting(&result, AccountCode::Cash),
        result.prepaid.cash - result.contingent_draw_cash,
    );
    balanced(&result);
}

#[test]
fn migration_defaults_into_initially_empty_watch_origin() {
    let mut c = contract();
    c.transition_hazards_per_year[0][1] = 1.;
    c.transition_hazards_per_year[1][3] = 2.;
    let result = compile(&c)
        .unwrap()
        .advance(&opening(&c), &step(&c))
        .unwrap();
    let p = (-1.0_f64).exp();
    let w = p - (-2.0_f64).exp();
    let d = 1. - p - w;
    close(result.closing.accounting.live[0].principal, 100. * p);
    close(result.closing.accounting.live[1].principal, 100. * w);
    close(result.default_by_origin[0].principal, 0.);
    close(result.default_by_origin[1].principal, 100. * d);
    close(
        result.closing.accounting.claims[0].initial_principal,
        110. * d,
    );
    balanced(&result);
}

#[test]
fn oppositely_signed_migrated_rights_are_not_netted_before_default_allocation() {
    let mut c = contract();
    c.transition_hazards_per_year[0][1] = 1.;
    c.transition_hazards_per_year[1][3] = 2.;
    let mut o = opening(&c);
    o.probability = [0.5, 0.5, 0., 0., 0., 0., 0.];
    o.accounting.live[0].accrued_interest = 100.;
    o.accounting.live[1] = LiveState {
        principal: 100.,
        undrawn: 0.,
        accrued_interest: -100.,
        book_adjustment: 5.,
    };
    let result = compile(&c).unwrap().advance(&o, &step(&c)).unwrap();
    let e1 = (-1.0_f64).exp();
    let e2 = (-2.0_f64).exp();
    close(
        result.closing.accounting.live[1].accrued_interest,
        100. * (e1 - 2. * e2),
    );
    close(
        result.default_by_origin[1].accrued_interest,
        100. * (-2. * e1 + 2. * e2),
    );
    assert!(result.closing.accounting.live[1].accrued_interest > 0.);
    assert!(result.default_by_origin[1].accrued_interest < 0.);
    balanced(&result);
}

#[test]
fn zero_credit_keeps_probability_and_money_independent_and_accrues_once() {
    let mut c = contract();
    c.accrual_rates[0] = 0.05;
    let mut o = opening(&c);
    o.probability = [0.1, 0., 0., 0.4, 0., 0., 0.5];
    let result = compile(&c).unwrap().advance(&o, &step(&c)).unwrap();
    assert_eq!(result.closing.probability, o.probability);
    close(result.closing.accounting.live[0].principal, 100.);
    close(result.earned_interest, 5.); // No probability-weighting of aggregate monetary state.
    assert!(result.closing.accounting.claims.is_empty());
    assert_eq!(result.intents.len(), 1);
    balanced(&result);
}

#[test]
fn multi_day_recovery_claims_settle_once_and_retained_absorbing_mass_is_exact() {
    let mut c = contract();
    c.interval_days = 1;
    c.transition_hazards_per_year[0][3] = 20.;
    c.complete_recovery_tails = std::array::from_fn(|_| {
        vec![
            RecoveryFraction {
                lag_days: 1,
                weight: 0.25,
            },
            RecoveryFraction {
                lag_days: 3,
                weight: 0.75,
            },
        ]
    });
    let compiled = compile(&c).unwrap();
    let first = compiled.advance(&opening(&c), &step(&c)).unwrap();
    let claim = first.closing.accounting.claims[0].clone();
    let mut resumed = first.closing.clone();
    resumed.accounting.live = [LiveState::default(); 3];
    resumed.probability[0] = 0.;
    let mut next = step(&c);
    next.recognition_ordinal += 1;
    next.reporting_end_ordinal = next.recognition_ordinal;
    let second = compiled.advance(&resumed, &next).unwrap();
    close(
        second.modeled_recovery_cash,
        claim.initial_principal * 0.6 * 0.25,
    );
    assert_eq!(second.closing.probability, resumed.probability);
    next.recognition_ordinal += 1;
    next.reporting_end_ordinal = next.recognition_ordinal;
    let third = compiled.advance(&second.closing, &next).unwrap();
    close(third.modeled_recovery_cash, 0.);
    next.recognition_ordinal += 1;
    next.reporting_end_ordinal = next.recognition_ordinal;
    let fourth = compiled.advance(&third.closing, &next).unwrap();
    close(
        fourth.modeled_recovery_cash,
        claim.initial_principal * 0.6 * 0.75,
    );
    assert!(fourth.closing.accounting.claims[0]
        .remaining_recoveries
        .is_empty());
    close(
        fourth.closing.accounting.claims[0].outstanding_principal,
        claim.initial_principal * 0.4,
    );
    balanced(&second);
    balanced(&third);
    balanced(&fourth);
}

#[test]
fn reviewed_allowance_and_chargeoff_are_explicit_and_failure_is_atomic() {
    let mut c = contract();
    c.transition_hazards_per_year[0][3] = 0.2;
    let compiled = compile(&c).unwrap();
    let o = opening(&c);
    let original = serde_json::to_value(&o).unwrap();
    let mut s = step(&c);
    let first = compiled.advance(&o, &s).unwrap();
    let claim = &first.closing.accounting.claims[0];
    let writeoff = claim.initial_principal * 0.4;
    s.chargeoffs.push(ReviewedChargeoff {
        review: id("reviewed-loss"),
        claim_id: claim.claim_id.clone(),
        recognition_ordinal: s.recognition_ordinal,
        principal_amount: writeoff,
        book_adjustment_amount: 0.,
        accrued_interest_reversal: 0.,
    });
    assert!(compiled.advance(&o, &s).is_err());
    assert_eq!(serde_json::to_value(&o).unwrap(), original);
    s.allowance_review = Some(ReviewedAllowance {
        review: id("allowance-review"),
        recognition_ordinal: s.recognition_ordinal,
        phase: ReviewPhase::BeforeCredit,
        target_amount: writeoff,
    });
    let result = compiled.advance(&o, &s).unwrap();
    close(result.closing.accounting.allowance, 0.);
    close(
        result.closing.accounting.claims[0].principal_charged_off,
        writeoff,
    );
    balanced(&result);
}

#[test]
fn maturity_removes_live_exposure_externally_without_becoming_prepay() {
    let c = contract();
    let mut o = opening(&c);
    o.probability[0] = 0.;
    o.accounting.live = [LiveState::default(); 3];
    let result = compile(&c).unwrap().advance(&o, &step(&c)).unwrap();
    assert_eq!(result.closing.probability, [0.; 7]);
    assert_eq!(result.prepaid.cash, 0.);
    assert!(result.intents.is_empty());
}

#[test]
fn invalid_domains_identity_dates_tails_and_schema_fail_closed() {
    let c = contract();
    let compiled = compile(&c).unwrap();
    let mut bad = c.clone();
    bad.measure = CreditMeasure::Pricing;
    assert!(compile(&bad).is_err());
    bad = c.clone();
    bad.credit_conversion_factor[1] = 1.01;
    assert!(compile(&bad).is_err());
    bad = c.clone();
    bad.complete_recovery_tails[0][0].weight = 0.5;
    assert!(compile(&bad).is_err());
    bad = c.clone();
    bad.complete_recovery_tails[0].clear();
    assert!(compile(&bad).is_err());
    bad.loss_given_default[0] = 1.;
    assert!(compile(&bad).is_ok());
    bad = c.clone();
    bad.interval_days = 0;
    assert!(compile(&bad).is_err());
    bad = c.clone();
    bad.identity.id = "bad\nlabel".into();
    assert!(compile(&bad).is_err());
    let mut o = opening(&c);
    o.contract_identity.revision = "other".into();
    assert!(compiled.advance(&o, &step(&c)).is_err());
    o = opening(&c);
    o.probability[0] = 1.01;
    assert!(compiled.advance(&o, &step(&c)).is_err());
    let mut s = step(&c);
    s.recognition_ordinal += 1;
    assert!(compiled.advance(&opening(&c), &s).is_err());
    let mut raw = serde_json::to_value(&c).unwrap();
    raw.as_object_mut().unwrap().remove("measure");
    assert!(serde_json::from_value::<CreditStateContract>(raw).is_err());
    raw = serde_json::to_value(&c).unwrap();
    raw["survival_adjustment"] = serde_json::json!(0.9);
    assert!(serde_json::from_value::<CreditStateContract>(raw).is_err());
}

#[test]
fn skipped_recovery_dates_require_interval_splitting() {
    let mut c = contract();
    c.interval_days = 1;
    c.transition_hazards_per_year[0][3] = 1.;
    let first = compile(&c)
        .unwrap()
        .advance(&opening(&c), &step(&c))
        .unwrap();
    let mut longer = c.clone();
    longer.interval_days = 3;
    let s = CreditStateStep {
        recognition_ordinal: first.closing.last_ordinal + 3,
        reporting_end_ordinal: first.closing.last_ordinal + 3,
        chargeoffs: vec![],
        allowance_review: None,
    };
    assert!(compile(&longer)
        .unwrap()
        .advance(&first.closing, &s)
        .unwrap_err()
        .contains("split"));
}
