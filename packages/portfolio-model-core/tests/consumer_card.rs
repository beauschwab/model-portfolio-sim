use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::consumer_card::*;
use portfolio_model_core::consumer_contract::{ConsumerIdentity, ConsumerState};
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn request() -> CardRequest {
    let asof = ordinal_from_ymd(2024, 1, 31).unwrap();
    CardRequest {
        schema: SCHEMA.into(),
        identity: ConsumerIdentity {
            position_id: "card-1".into(),
            original_loan_id: "account-1".into(),
            cohort_id: "cohort-card".into(),
            currency: "USD".into(),
            source_revision: "account-panel-v1".into(),
            model_version: SCHEMA.into(),
            assumption_id: "supplied-panel-v1".into(),
            run_id: "base".into(),
            measure: CashflowMeasure::Physical,
        },
        as_of_ordinal: asof,
        horizon_days: 60,
        opening_principal: [0.; 3],
        opening_billed_interest: [0.; 3],
        opening_unbilled_interest: [0.; 3],
        opening_unrecognized_billed_interest: [0.; 3],
        opening_fees: [0.; 3],
        opening_contingent_purchase_interest: 0.,
        opening_statement_remaining: [0.; 3],
        opening_minimum_remaining: 0.,
        opening_due_ordinal: None,
        initial_limit: 2000.,
        initial_state: ConsumerState::Performing,
        rates: CardRates {
            purchase: 0.365,
            cash_advance: 0.73,
            promotional: 0.,
            promotional_expiry_ordinal: asof + 30,
            post_promotion: 0.365,
        },
        day_count: ActualDayCount::Act365Fixed,
        initial_purchase_grace: true,
        regain_grace_after_full_statement_payment: true,
        compound_billed_interest: false,
        minimum: MinimumPayment {
            floor: 25.,
            principal_fraction: 0.01,
            include_billed_interest: true,
            include_fees: true,
            allocation_order: vec![
                BalanceClass::Purchase,
                BalanceClass::CashAdvance,
                BalanceClass::Promotional,
            ],
        },
        cycles: vec![StatementCycle {
            statement_ordinal: asof + 10,
            due_ordinal: asof + 31,
        }],
        steps: vec![],
        collection_fraction: 0.4,
        collection_cost: 10.,
        recovery_delay_days: 10,
    }
}
#[test]
fn transactor_grace_draw_and_full_payment_waive_interest_and_conserve_line() {
    let mut r = request();
    r.steps = vec![
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 1000.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 31,
            action: CardAction::Payment { amount: 1000. },
        },
    ];
    let out = simulate(&r).unwrap();
    near(out.drawn_principal, 1000.);
    near(out.collected_principal, 1000.);
    near(out.closing_principal, 0.);
    near(out.closing_interest, 0.);
    assert!(out.recognition.is_empty());
    near(out.statements[0].minimum_payment, 25.);
    near(out.points[0].undrawn, 1000.);
    near(out.points.last().unwrap().undrawn, 2000.);
}
#[test]
fn revolver_cash_advance_accrues_actual_leap_days_without_grace() {
    let mut r = request();
    r.horizon_days = 29;
    r.cycles.clear();
    r.opening_principal = [0., 1000., 0.];
    r.rates.cash_advance = 0.365;
    let out = simulate(&r).unwrap();
    near(out.closing_interest, 0.);
    near(out.closing_unbilled_interest, 29.);
    near(out.recognition.iter().map(|a| a.interest).sum(), 29.);
    assert_eq!(
        out.points.last().unwrap().ordinal,
        ordinal_from_ymd(2024, 2, 29).unwrap()
    );
}
#[test]
fn issuer_minimum_and_excess_highest_apr_have_distinct_allocations() {
    let mut r = request();
    r.horizon_days = 2;
    r.cycles = vec![StatementCycle {
        statement_ordinal: r.as_of_ordinal,
        due_ordinal: r.as_of_ordinal + 2,
    }];
    r.opening_principal = [1000., 500., 0.];
    r.initial_purchase_grace = false;
    r.minimum.floor = 100.;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal + 1,
        action: CardAction::Payment { amount: 300. },
    });
    let out = simulate(&r).unwrap();
    let p = &out.points[1];
    // Day-one $1 interest in each class remains unbilled. Minimum applies $100
    // purchase principal; excess applies $200 to higher-APR cash principal.
    near(p.principal[0], 900.);
    near(p.principal[1], 300.);
    near(p.unbilled_interest[0], 1.);
    near(p.unbilled_interest[1], 1.);
    near(out.cashflows[0].interest, 0.);
    near(out.cashflows[0].principal, 300.);
}
#[test]
fn partial_grace_payment_protects_paid_portion_and_charges_only_unpaid() {
    let mut r = request();
    r.horizon_days = 31;
    r.steps = vec![
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 1000.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 31,
            action: CardAction::Payment { amount: 400. },
        },
    ];
    let out = simulate(&r).unwrap();
    near(out.closing_principal, 600.);
    near(out.closing_interest, 18.6);
    near(out.recognition.iter().map(|v| v.interest).sum(), 18.6);
    assert!(!out.points.last().unwrap().purchase_grace);
}
#[test]
fn fifo_purchase_lots_preserve_transaction_date_specific_grace_protection() {
    let mut r = request();
    r.horizon_days = 31;
    r.steps = vec![
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 1000.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 5,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 500.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 31,
            action: CardAction::Payment { amount: 1000. },
        },
    ];
    let out = simulate(&r).unwrap();
    near(out.closing_interest, 500. * 0.365 * 26. / 365.);
    near(out.closing_principal, 500.);
}
#[test]
fn promo_expiry_changes_only_future_accrual_no_deferred_interest_relabel() {
    let mut r = request();
    r.horizon_days = 40;
    r.cycles.clear();
    r.opening_principal = [0., 0., 1000.];
    let out = simulate(&r).unwrap();
    near(out.closing_interest, 0.);
    near(out.closing_unbilled_interest, 10.);
    assert_eq!(
        out.recognition.first().unwrap().start_ordinal,
        r.as_of_ordinal + 30
    );
}
#[test]
fn statement_minimum_includes_explicit_interest_and_fee_components() {
    let mut r = request();
    r.horizon_days = 20;
    r.cycles = vec![StatementCycle {
        statement_ordinal: r.as_of_ordinal + 10,
        due_ordinal: r.as_of_ordinal + 20,
    }];
    r.opening_principal = [0., 1000., 0.];
    r.rates.cash_advance = 0.365;
    r.opening_fees = [0., 20., 0.];
    let out = simulate(&r).unwrap();
    near(out.statements[0].minimum_payment, 40.);
    near(out.statements[0].statement_total, 1030.);
}
#[test]
fn limit_enforcement_includes_posted_charges_and_limit_reduction_does_not_create_draws() {
    let mut r = request();
    r.opening_principal = [1000., 0., 0.];
    r.opening_fees = [10., 0., 0.];
    r.initial_limit = 1010.;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::Draw {
            balance_class: BalanceClass::Purchase,
            amount: 1.,
        },
    });
    assert!(simulate(&r).is_err());
    r.steps = vec![CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::SetLimit { amount: 500. },
    }];
    let out = simulate(&r).unwrap();
    near(out.points[0].undrawn, 0.);
    near(out.points[0].utilization.unwrap(), 2.02);
    near(out.drawn_principal, 0.);
}
#[test]
fn same_date_input_order_is_economic_and_stable() {
    let mut r = request();
    r.horizon_days = 0;
    r.cycles.clear();
    r.initial_limit = 1000.;
    r.steps = vec![
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 500.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Payment { amount: 500. },
        },
    ];
    let out = simulate(&r).unwrap();
    near(out.closing_principal, 0.);
    assert_eq!(out.cashflows[0].kind, "draw");
    assert_eq!(out.cashflows[1].kind, "payment");
    r.steps.reverse();
    assert!(simulate(&r).is_err());
}
#[test]
fn credit_exit_keeps_principal_and_accrued_loss_separate_and_recovery_tail() {
    let mut r = request();
    r.opening_principal = [0., 1000., 0.];
    r.rates.cash_advance = 0.365;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal + 10,
        action: CardAction::Default {},
    });
    let out = simulate(&r).unwrap();
    let def = out.default.unwrap();
    near(def.principal, 1000.);
    near(def.accrued_interest, 10.);
    near(def.net_recovery, 390.);
    near(def.principal_loss, 610.);
    near(out.closing_principal, 0.);
    near(out.cashflows[0].principal, 390.);
    assert_eq!(out.cashflows[0].ordinal, r.as_of_ordinal + 20);
    r.horizon_days = 19;
    r.cycles.clear();
    assert!(simulate(&r).is_err());
}
#[test]
fn delinquent_draws_rejected_and_nonaccrual_interest_suspended() {
    let mut r = request();
    r.initial_state = ConsumerState::Delinquent;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::Draw {
            balance_class: BalanceClass::Purchase,
            amount: 10.,
        },
    });
    assert!(simulate(&r).is_err());
    r.steps.clear();
    r.initial_state = ConsumerState::Nonaccrual;
    r.opening_principal = [0., 1000., 0.];
    let out = simulate(&r).unwrap();
    near(out.closing_interest, 0.);
    assert!(out.recognition.is_empty());
}
#[test]
fn no_new_draws_and_zero_change_measure_identity_preserve_balances() {
    let r = request();
    let out = simulate(&r).unwrap();
    assert_eq!(
        serde_json::to_vec(&out).unwrap(),
        serde_json::to_vec(&simulate(&r.clone()).unwrap()).unwrap()
    );
    near(out.drawn_principal, 0.);
    near(out.closing_principal, 0.);
    let mut pricing = r;
    pricing.identity.measure = CashflowMeasure::Pricing;
    assert_eq!(simulate(&pricing).unwrap().cashflows, out.cashflows);
}
#[test]
fn invalid_domains_unknown_conventions_coverage_and_work_are_rejected() {
    let r = request();
    let mut bad = r.clone();
    bad.rates.purchase = f64::NAN;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.minimum.allocation_order[1] = BalanceClass::Purchase;
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.cycles.push(bad.cycles[0].clone());
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.steps.push(CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::Payment { amount: 1. },
    });
    assert!(simulate(&bad).is_err());
    bad = r.clone();
    bad.horizon_days = 36600;
    bad.cycles.clear();
    for _ in 0..100 {
        bad.steps.push(CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Fee {
                balance_class: BalanceClass::Purchase,
                amount: 0.,
            },
        });
    }
    assert!(simulate(&bad).is_err());
    let mut raw = serde_json::to_value(r).unwrap();
    raw["deferred_interest"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CardRequest>(raw).is_err());
    let raw = serde_json::json!({"kind":"default","implicit_ccf":1.});
    assert!(serde_json::from_value::<CardAction>(raw).is_err());
}

#[test]
fn statement_posts_unbilled_interest_and_only_posted_charges_compound() {
    let mut r = request();
    r.horizon_days = 3;
    r.cycles = vec![StatementCycle {
        statement_ordinal: r.as_of_ordinal + 2,
        due_ordinal: r.as_of_ordinal + 3,
    }];
    r.opening_principal = [0., 1000., 0.];
    r.opening_billed_interest = [0., 10., 0.];
    r.rates.cash_advance = 0.365;
    r.compound_billed_interest = true;
    let out = simulate(&r).unwrap();
    near(out.statements[0].billed_interest[1], 12.02);
    near(out.points[1].unbilled_interest[1], 1.01);
    near(out.closing_unbilled_interest, 1.01202);
    near(out.closing_interest, 12.02);
}
#[test]
fn opening_pending_statement_and_contingent_rights_survive_valuation_boundary() {
    let mut r = request();
    r.horizon_days = 5;
    r.cycles.clear();
    r.opening_principal = [1000., 0., 0.];
    r.opening_statement_remaining = [1000., 0., 0.];
    r.opening_minimum_remaining = 25.;
    r.opening_due_ordinal = Some(r.as_of_ordinal + 5);
    r.opening_contingent_purchase_interest = 10.;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal + 5,
        action: CardAction::Payment { amount: 400. },
    });
    let out = simulate(&r).unwrap();
    near(out.closing_principal, 600.);
    near(out.closing_interest, 9.);
    near(out.recognition.iter().map(|v| v.interest).sum(), 9.);
    r.opening_due_ordinal = None;
    assert!(simulate(&r).is_err());
}
#[test]
fn nonaccrual_grace_loss_stays_unrecognized_until_cash_collection() {
    let mut r = request();
    r.horizon_days = 32;
    r.steps = vec![
        CardStep {
            ordinal: r.as_of_ordinal,
            action: CardAction::Draw {
                balance_class: BalanceClass::Purchase,
                amount: 1000.,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 20,
            action: CardAction::SetState {
                state: ConsumerState::Nonaccrual,
            },
        },
        CardStep {
            ordinal: r.as_of_ordinal + 32,
            action: CardAction::Payment { amount: 20. },
        },
    ];
    let out = simulate(&r).unwrap();
    near(out.points[31].billed_interest[0], 20.);
    near(out.recognition.iter().map(|v| v.interest).sum(), 20.);
    near(out.closing_unrecognized_billed_interest, 0.);
    near(out.cashflows[1].interest, 20.);
    assert_eq!(out.recognition[0].start_ordinal, r.as_of_ordinal + 32);
}
#[test]
fn card_collection_expense_and_same_day_competing_exit_are_explicit() {
    let mut r = request();
    r.horizon_days = 20;
    r.cycles.clear();
    r.opening_principal = [100., 0., 0.];
    r.collection_fraction = 0.;
    r.collection_cost = 10.;
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::Default {},
    });
    let out = simulate(&r).unwrap();
    near(out.cashflows[0].fees, -10.);
    near(out.default.unwrap().principal_loss, 100.);
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal,
        action: CardAction::Payment { amount: 100. },
    });
    assert!(simulate(&r).is_err());
}
#[test]
fn card_aggregate_money_and_contractual_measure_are_not_admitted_as_physical_paths() {
    let mut r = request();
    r.opening_principal = [1e15, 1e15, 0.];
    assert!(simulate(&r).is_err());
    r = request();
    r.identity.measure = CashflowMeasure::Contractual;
    assert!(simulate(&r).is_err());
}

#[test]
fn saved_example_is_strict_valid_and_separates_statement_posting_from_earning() {
    let r: CardRequest = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-card-1.json"
    ))
    .unwrap();
    let out = simulate(&r).unwrap();
    near(out.closing_principal, 1000.);
    near(out.closing_interest, 1.);
    near(out.closing_unbilled_interest, 1.);
    near(out.recognition.iter().map(|v| v.interest).sum(), 2.);
    near(out.statements[0].minimum_payment, 25.);
    assert!(out.cashflows.is_empty());
}
