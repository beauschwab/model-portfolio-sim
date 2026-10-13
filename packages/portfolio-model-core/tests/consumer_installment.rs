use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::consumer_contract::{ConsumerIdentity, ConsumerState};
use portfolio_model_core::consumer_installment::*;

fn d(y: i32, m: u32, day: u32) -> i32 {
    ordinal_from_ymd(y, m, day).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn request() -> InstallmentRequest {
    let asof = d(2024, 1, 31);
    InstallmentRequest {
        schema: SCHEMA.into(),
        identity: ConsumerIdentity {
            position_id: "personal-1".into(),
            original_loan_id: "original-1".into(),
            cohort_id: "cohort-1".into(),
            currency: "USD".into(),
            source_revision: "tape-v1".into(),
            model_version: SCHEMA.into(),
            assumption_id: "supplied-v1".into(),
            run_id: "base".into(),
            measure: CashflowMeasure::Physical,
        },
        product: InstallmentProduct::Personal {
            vintage_ordinal: d(2023, 1, 31),
            dti: 0.4,
            dti_definition: "monthly-debt/gross-monthly-income".into(),
            channel: "branch".into(),
            collection_fraction: 0.5,
            collection_cost: 50.,
            recovery_delay_days: 20,
        },
        as_of_ordinal: asof,
        horizon_days: 120,
        original_principal: 1000.,
        opening_principal: 1000.,
        opening_accrued_interest: 0.,
        opening_unrecognized_interest: 0.,
        opening_fees: 0.,
        initial_state: ConsumerState::Performing,
        note_rate: 0.,
        disclosed_apr: 0.,
        day_count: ActualDayCount::Act365Fixed,
        original_payment: 500.,
        payment_dates: vec![d(2024, 2, 29), d(2024, 3, 31)],
        prepayment_penalty_fraction: 0.,
        accrue_contract_interest_in_nonaccrual: true,
        steps: vec![],
    }
}

#[test]
fn zero_rate_original_payment_and_final_payment_conserve_principal() {
    let r = request();
    let out = project(&r).unwrap();
    assert_eq!(out.cashflows.len(), 2);
    near(out.cashflows[0].principal, 500.);
    near(out.cashflows[1].principal, 500.);
    near(out.principal_collected, 1000.);
    near(out.closing_principal, 0.);
    assert_eq!(out.original_payment, 500.);
    assert_eq!(out.identity.original_loan_id, "original-1");
    assert!(out.recognition.is_empty());
}
#[test]
fn leap_day_simple_interest_uses_note_rate_and_actual_days_not_disclosed_apr() {
    let mut r = request();
    r.note_rate = 0.365;
    r.disclosed_apr = 0.5;
    r.original_payment = 529.;
    let out = project(&r).unwrap();
    near(out.cashflows[0].interest, 29.);
    near(out.cashflows[0].principal, 500.);
    near(out.cashflows[1].interest, 15.5);
    near(out.cashflows[1].principal, 500.);
    near(out.recognition.iter().map(|v| v.interest).sum(), 44.5);
    r.disclosed_apr = 0.8;
    assert_eq!(project(&r).unwrap().cashflows, out.cashflows);
}
#[test]
fn early_payoff_uses_actual_accrual_and_explicit_penalty_no_future_interest() {
    let mut r = request();
    r.note_rate = 0.365;
    r.prepayment_penalty_fraction = 0.02;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal + 10,
        action: InstallmentAction::Payoff {},
    });
    let out = project(&r).unwrap();
    assert_eq!(out.cashflows.len(), 1);
    near(out.cashflows[0].principal, 1000.);
    near(out.cashflows[0].interest, 10.);
    near(out.cashflows[0].fees, 20.);
    near(out.recognition.iter().map(|v| v.interest).sum(), 10.);
    near(out.closing_principal, 0.);
}
#[test]
fn delinquent_skip_and_explicit_cure_keep_principal_until_paid() {
    let mut r = request();
    r.steps = vec![
        InstallmentStep {
            ordinal: r.as_of_ordinal,
            action: InstallmentAction::SetState {
                state: ConsumerState::Delinquent,
            },
        },
        InstallmentStep {
            ordinal: r.as_of_ordinal + 40,
            action: InstallmentAction::SetState {
                state: ConsumerState::Performing,
            },
        },
    ];
    let out = project(&r).unwrap();
    assert_eq!(out.cashflows.len(), 1);
    assert_eq!(out.cashflows[0].ordinal, d(2024, 3, 31));
    near(out.cashflows[0].principal, 1000.);
    let leap = out
        .points
        .iter()
        .find(|p| p.ordinal == d(2024, 2, 29))
        .unwrap();
    near(leap.principal, 1000.);
    assert_eq!(leap.state, ConsumerState::Delinquent);
}
#[test]
fn default_recovery_is_principal_loss_not_default_cash_or_double_payment() {
    let mut r = request();
    r.note_rate = 0.365;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal + 10,
        action: InstallmentAction::Default {},
    });
    let out = project(&r).unwrap();
    let def = out.default.unwrap();
    near(def.principal, 1000.);
    near(def.accrued_interest, 10.);
    near(def.gross_recovery, 500.);
    near(def.net_recovery, 450.);
    near(def.principal_loss, 550.);
    assert_eq!(out.cashflows.len(), 1);
    assert_eq!(out.cashflows[0].ordinal, r.as_of_ordinal + 30);
    near(out.cashflows[0].principal, 450.);
    near(out.principal_collected, 0.);
}
#[test]
fn auto_collateral_surplus_pays_cost_before_principal_cap() {
    let mut r = request();
    r.product = InstallmentProduct::Auto {
        new_vehicle: true,
        vintage_ordinal: d(2023, 1, 31),
        original_ltv: 0.8,
        channel: "dealer".into(),
        collateral_value_as_of: 1400.,
        annual_depreciation: 0.,
        sale_haircut: 0.2,
        repossession_cost: 100.,
        recovery_delay_days: 10,
    };
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::Default {},
    });
    let out = project(&r).unwrap();
    let def = out.default.unwrap();
    near(def.gross_recovery, 1120.);
    near(def.net_recovery, 1000.);
    near(def.principal_loss, 0.);
}
#[test]
fn auto_depreciates_through_recovery_sale_date_and_preserves_tail() {
    let mut r = request();
    r.horizon_days = 400;
    r.product = InstallmentProduct::Auto {
        new_vehicle: false,
        vintage_ordinal: d(2023, 1, 31),
        original_ltv: 1.,
        channel: "dealer".into(),
        collateral_value_as_of: 1400.,
        annual_depreciation: 0.5,
        sale_haircut: 0.2,
        repossession_cost: 50.,
        recovery_delay_days: 365,
    };
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::Default {},
    });
    let out = project(&r).unwrap();
    let def = out.default.unwrap();
    near(def.gross_recovery, 560.);
    near(def.net_recovery, 510.);
    assert_eq!(def.recovery_ordinal, r.as_of_ordinal + 365);
    r.horizon_days = 364;
    assert!(project(&r).is_err());
}
#[test]
fn recovery_cost_beyond_proceeds_is_not_silently_lost() {
    let mut r = request();
    if let InstallmentProduct::Personal {
        collection_fraction,
        collection_cost,
        ..
    } = &mut r.product
    {
        *collection_fraction = 0.;
        *collection_cost = 100.;
    }
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::Default {},
    });
    let out = project(&r).unwrap();
    near(out.cashflows[0].principal, 0.);
    near(out.cashflows[0].fees, -100.);
    near(out.default.unwrap().principal_loss, 1000.);
}
#[test]
fn nonaccrual_contractual_interest_is_cash_basis_recognition_only_on_collection() {
    let mut r = request();
    r.note_rate = 0.365;
    r.initial_state = ConsumerState::Nonaccrual;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal + 10,
        action: InstallmentAction::Payoff {},
    });
    let out = project(&r).unwrap();
    near(out.cashflows[0].interest, 10.);
    assert_eq!(out.recognition.len(), 1);
    assert_eq!(
        out.recognition[0].start_ordinal,
        out.recognition[0].end_ordinal
    );
    near(out.recognition[0].interest, 10.);
    r.accrue_contract_interest_in_nonaccrual = false;
    near(project(&r).unwrap().cashflows[0].interest, 0.);
}
#[test]
fn fee_and_changed_payment_do_not_rewrite_original_contract_lineage() {
    let mut r = request();
    r.opening_fees = 20.;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::ChangePayment { amount: 300. },
    });
    let out = project(&r).unwrap();
    near(out.cashflows[0].fees, 20.);
    near(out.cashflows[0].principal, 280.);
    near(out.cashflows[1].principal, 720.);
    near(out.original_payment, 500.);
}
#[test]
fn named_measure_and_zero_change_are_exact_output_identity() {
    let r = request();
    let out = project(&r).unwrap();
    assert_eq!(
        serde_json::to_vec(&out).unwrap(),
        serde_json::to_vec(&project(&r.clone()).unwrap()).unwrap()
    );
    let mut pricing = r;
    pricing.identity.measure = CashflowMeasure::Pricing;
    let q = project(&pricing).unwrap();
    assert_eq!(q.cashflows, out.cashflows);
    assert_eq!(q.identity.measure, CashflowMeasure::Pricing);
}
#[test]
fn invalid_domains_overlap_dates_and_terminal_actions_fail_explicitly() {
    let r = request();
    let mut bad = r.clone();
    bad.note_rate = f64::NAN;
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.opening_principal = 1001.;
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.payment_dates[1] = bad.payment_dates[0];
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.identity.model_version = "unknown".into();
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.horizon_days = 59;
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.steps = vec![
        InstallmentStep {
            ordinal: r.as_of_ordinal,
            action: InstallmentAction::Default {},
        },
        InstallmentStep {
            ordinal: r.as_of_ordinal + 1,
            action: InstallmentAction::Payoff {},
        },
    ];
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.steps = vec![InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::Payment { amount: 2000. },
    }];
    assert!(project(&bad).is_err());
    bad = r.clone();
    bad.steps = vec![
        InstallmentStep {
            ordinal: r.as_of_ordinal,
            action: InstallmentAction::Default {},
        },
        InstallmentStep {
            ordinal: r.as_of_ordinal,
            action: InstallmentAction::Payoff {},
        },
    ];
    assert!(project(&bad).is_err());
}
#[test]
fn unknown_schema_fields_and_precomputed_interest_are_rejected() {
    let mut value = serde_json::to_value(request()).unwrap();
    value["precomputed_interest"] = serde_json::json!(true);
    assert!(serde_json::from_value::<InstallmentRequest>(value).is_err());
    let bad = serde_json::json!({"kind":"payoff","survival_haircut":0.5});
    assert!(serde_json::from_value::<InstallmentAction>(bad).is_err());
}

#[test]
fn aggregate_amount_and_payoff_penalty_cash_are_bounded() {
    let mut r = request();
    r.original_principal = 1e15;
    r.opening_principal = 1e15;
    r.opening_fees = 1.;
    assert!(project(&r).is_err());
    r.opening_fees = 0.;
    r.prepayment_penalty_fraction = 0.1;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal,
        action: InstallmentAction::Payoff {},
    });
    assert!(project(&r).is_err());
}
#[test]
fn nonaccrual_default_does_not_mislabel_contract_interest_as_recognized() {
    let mut r = request();
    r.note_rate = 0.365;
    r.initial_state = ConsumerState::Nonaccrual;
    r.steps.push(InstallmentStep {
        ordinal: r.as_of_ordinal + 10,
        action: InstallmentAction::Default {},
    });
    let out = project(&r).unwrap();
    let def = out.default.unwrap();
    near(def.accrued_interest, 10.);
    near(def.recognized_accrued_interest, 0.);
    near(def.unrecognized_contract_interest, 10.);
    assert!(out.recognition.is_empty());
}
#[test]
fn analytical_payment_rate_grid_conserves_without_default_or_prepay_haircuts() {
    for rate in [0., 0.0001, 0.03, 0.365, 1.] {
        for payment in [10., 250., 500., 2000.] {
            let mut r = request();
            r.note_rate = rate;
            r.original_payment = payment;
            let out = project(&r).unwrap();
            near(out.cashflows.iter().map(|e| e.principal).sum(), 1000.);
            near(out.closing_principal, 0.);
            assert!(out.default.is_none());
            let interest1 = 1000. * rate * 29. / 365.;
            let principal1 = (payment - interest1).clamp(0., 1000.);
            near(out.cashflows[0].principal, principal1);
        }
    }
}

#[test]
fn saved_example_is_strict_valid_and_has_independently_expected_payments() {
    let r: InstallmentRequest = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-installment-1.json"
    ))
    .unwrap();
    assert_eq!(r.as_of_ordinal, d(2024, 1, 31));
    assert_eq!(r.payment_dates, vec![d(2024, 2, 29), d(2024, 3, 31)]);
    let out = project(&r).unwrap();
    near(out.cashflows[0].principal, 500.);
    near(out.cashflows[0].interest, 29.);
    near(out.cashflows[1].principal, 500.);
    near(out.cashflows[1].interest, 15.5);
}
