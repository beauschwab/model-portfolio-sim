//! Public card accounting fixtures with independent hand accounting.
pub use portfolio_model_core::{
    cashflow, consumer_card, consumer_contract, credit_accounting, dated_income, limits,
};
use portfolio_model_core::card_credit_accounting as adapter;
use adapter::{evaluate, Request};
use credit_accounting::{AccountCode, AccruedPolicy, BasisPolicy};
use serde_json::json;
fn raw() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/card-credit-accounting-1.json"
    ))
    .unwrap()
}
fn request() -> Request {
    serde_json::from_value(raw()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn total(r: &adapter::ResultData, code: AccountCode) -> f64 {
    r.intents
        .iter()
        .flat_map(|x| &x.postings)
        .filter(|x| x.account == code)
        .map(|x| x.amount)
        .sum()
}
fn balanced(r: &adapter::ResultData) {
    assert_eq!(r.lineage.len(), r.intents.len());
    for (i, x) in r.intents.iter().enumerate() {
        close(x.postings.iter().map(|p| p.amount).sum(), 0.);
        assert_eq!(r.lineage[i].intent_index, i);
        assert_eq!(
            r.lineage[i].original_yield_anchor,
            r.raw_request.acquisition.original_yield_anchor
        );
    }
}
fn closure() -> serde_json::Value {
    let mut r = raw();
    // Reviewed zero EIR and gross cost101 match the complete supplied cash101.
    // Cost100/y0 would leave principal0/basis-1 during the interest-only tail,
    // outside the unchanged shared credit operator's admitted book-basis domain.
    r["acquisition"]["cost"] = json!(101.);
    r["contract"]["cycles"] = json!([{"statement_ordinal":738926,"due_ordinal":738931}]);
    r["contract"]["steps"] = json!([{"ordinal":738926,"action":{"kind":"payment","amount":100.}},{"ordinal":738927,"action":{"kind":"payment","amount":1.}},{"ordinal":738927,"action":{"kind":"set_limit","amount":0.}}]);
    r["termination"] = json!({"kind":"reviewed_zero_rights_closure","ordinal":738927,"review":{"id":"reviewed-card-closure","revision":"r1"}});
    r["chargeoffs"] = json!([]);
    r["opening_allowance"] = json!(0.);
    r
}

#[test]
fn default_before_statement_consumes_earned_unbilled_once() {
    let r = request();
    let out = evaluate(&r).unwrap();
    let d = out.economic.default.as_ref().unwrap();
    close(d.recognized_accrued_interest, 1.);
    assert!(out.economic.statements.is_empty());
    close(out.daily[9].source_billed_interest.iter().sum(), 0.);
    close(out.daily[9].source_unbilled_interest.iter().sum(), 0.9);
    close(total(&out, AccountCode::Cash), 50.);
    close(total(&out, AccountCode::CreditLossExpense), 10.);
    close(out.reporting.owned.claims[0].outstanding_principal, 100.);
    close(out.closing.claims[0].outstanding_principal, 0.);
    close(out.recovery.as_ref().unwrap().principal_only_loss, 40.);
    close(out.recovery.as_ref().unwrap().total_economic_loss, 50.);
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), -10.);
    balanced(&out);
}
#[test]
fn statement_bills_existing_accrual_without_double_income() {
    let mut r = raw();
    r["contract"]["cycles"] = json!([{"statement_ordinal":738926,"due_ordinal":738931}]);
    r["contract"]["steps"][0]["ordinal"] = json!(738927);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    close(
        out.economic
            .default
            .as_ref()
            .unwrap()
            .recognized_accrued_interest,
        1.1,
    );
    close(out.daily[10].source_billed_interest.iter().sum(), 1.);
    close(out.daily[10].owned.live[0].accrued_interest, 1.);
    assert_eq!(out.statement_lineage.len(), 1);
    assert!(!out.statement_lineage[0].interest_recognized_again);
    close(
        out.income_prefix
            .rows
            .iter()
            .map(|x| x.contractual_interest_earned)
            .sum(),
        1.1,
    );
    balanced(&out);
}
#[test]
fn principal_zero_preserves_unbilled_interest_until_reviewed_closure() {
    let out = evaluate(&serde_json::from_value(closure()).unwrap()).unwrap();
    close(out.daily[10].owned.live[0].principal, 0.);
    close(out.daily[10].owned.live[0].accrued_interest, 1.);
    assert_eq!(out.daily[10].accounting_status, "live");
    assert_eq!(out.daily[11].accounting_status, "reviewed_closed");
    assert_eq!(
        out.daily[11].economic_state,
        consumer_contract::ConsumerState::Performing
    );
    close(total(&out, AccountCode::Cash), 101.);
    close(total(&out, AccountCode::BasisRecognitionExpense), 0.);
    close(out.closing.live[0].principal, 0.);
    close(out.closing.live[0].accrued_interest, 0.);
    close(out.closing.live[0].book_adjustment, 0.);
    balanced(&out);
}

#[test]
fn interest_only_negative_basis_remains_explicitly_unsupported() {
    let mut r = closure();
    r["acquisition"]["cost"] = json!(100.);
    let error = evaluate(&serde_json::from_value(r).unwrap()).unwrap_err();
    // Actual card source is valid P0/A1. Frozen EIR0 yields B-1 and total C0,
    // but assigning that discount to the principal channel is not admitted.
    assert!(
        error.contains("opening live principal plus basis cannot be negative"),
        "{error}"
    );
}
#[test]
fn premium_closure_retains_original_eir_to_interest_tail() {
    let mut r = closure();
    r["acquisition"]["cost"] = json!(103.);
    r["acquisition"]["original_annual_effective_rate"] = json!(0.02);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    let first = 103. * (1.02_f64.powf(10. / 365.) - 1.);
    let last = (103. + first - 100.) * (1.02_f64.powf(1. / 365.) - 1.);
    close(total(&out, AccountCode::InterestIncome), -first - last);
    close(
        total(&out, AccountCode::BasisRecognitionExpense),
        2. + first + last,
    );
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), -2.);
    balanced(&out);
}
#[test]
fn discount_negative_original_eir_is_supported_at_actual_same_day_closure() {
    let mut r = closure();
    r["acquisition"]["cost"] = json!(90.);
    r["acquisition"]["original_annual_effective_rate"] = json!(-0.01);
    // Statement day10 occurs AFTER actions; pay only on day11 after charges billed.
    r["contract"]["steps"] = json!([{"ordinal":738927,"action":{"kind":"payment","amount":101.1}},{"ordinal":738927,"action":{"kind":"set_limit","amount":0.}}]);
    // Day11 non-compounding accrual creates unpaid unbilled .1; include second statement
    // on day11 impossible under nonoverlap, so use zeroAPR on day11 via known promo expiry.
    r["contract"]["rates"]["cash_advance"] = json!(0.);
    r["contract"]["opening_principal"] = json!([0., 0., 100.]);
    r["contract"]["rates"]["promotional"] = json!(0.36);
    r["contract"]["rates"]["promotional_expiry_ordinal"] = json!(738926);
    r["contract"]["rates"]["post_promotion"] = json!(0.);
    r["contract"]["steps"][0]["action"]["amount"] = json!(101.);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    close(total(&out, AccountCode::Cash), 101.);
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), 11.);
    assert!(total(&out, AccountCode::InterestIncome) > 0.);
    balanced(&out);
}
#[test]
fn partial_same_day_payment_precedes_actual_residual_default() {
    let mut r = raw();
    r["contract"]["steps"] = json!([{"ordinal":738926,"action":{"kind":"payment","amount":50.}},{"ordinal":738926,"action":{"kind":"default"}}]);
    r["chargeoffs"][0]["principal_amount"] = json!(20.);
    r["opening_allowance"] = json!(20.);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    close(out.economic.default.as_ref().unwrap().principal, 50.);
    close(
        out.economic
            .default
            .as_ref()
            .unwrap()
            .recognized_accrued_interest,
        1.,
    );
    close(out.recovery.as_ref().unwrap().gross_cash, 30.);
    close(total(&out, AccountCode::Cash), 70.);
    close(out.closing.claims[0].initial_principal, 50.);
    balanced(&out);
}
#[test]
fn same_day_default_gross_recovery_cost_retains_shared_order() {
    let mut r = raw();
    r["contract"]["recovery_delay_days"] = json!(0);
    r["chargeoffs"][0]["recognition_ordinal"] = json!(738926);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    let names: Vec<_> = out
        .intents
        .iter()
        .filter(|x| x.recognition_ordinal == 738926)
        .map(|x| x.event.as_str())
        .collect();
    let a = names
        .iter()
        .position(|x| *x == "economic_default_transfer")
        .unwrap();
    let b = names
        .iter()
        .position(|x| *x == "reviewed_credit_chargeoff")
        .unwrap();
    let c = names
        .iter()
        .position(|x| *x == "modeled_physical_recovery_cash")
        .unwrap();
    let d = names
        .iter()
        .position(|x| *x == "card_reviewed_recovery_cost")
        .unwrap();
    assert!(a < b && b < c && c < d);
    balanced(&out);
}
#[test]
fn retained_claim_rights_and_commitment_memo_are_not_ccf() {
    let mut r = request();
    r.policy.accounting.accrued = AccruedPolicy::TransferUntilReviewedReversal;
    r.policy.accounting.basis = BasisPolicy::TransferUntilReviewedChargeoff;
    r.chargeoffs.clear();
    let out = evaluate(&r).unwrap();
    close(out.closing.claims[0].outstanding_principal, 40.);
    close(out.closing.claims[0].outstanding_accrued_interest, 1.);
    close(out.closing.claims[0].outstanding_book_adjustment, -1.);
    close(out.daily[9].source_undrawn, 100.);
    close(out.daily[9].owned.live[0].undrawn, 0.);
    close(out.commitment_source.funded_contingent_draw, 0.);
    balanced(&out);
}
#[test]
fn zero_net_recovery_keeps_gross_cash_cost_and_real_origin() {
    let mut r = raw();
    r["contract"]["collection_cost"] = json!(60.);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    assert!(out.economic.cashflows.is_empty());
    close(total(&out, AccountCode::Cash), 0.);
    close(total(&out, AccountCode::CreditLossExpense), 60.);
    let line = out
        .lineage
        .iter()
        .find(|x| x.phase == "card_reviewed_recovery_cost")
        .unwrap();
    assert_eq!(line.source_event_ids, vec!["default:738926"]);
    balanced(&out);
}
#[test]
fn class_rates_promo_expiry_and_credit_origins_remain_distinct() {
    let mut r = raw();
    r["contract"]["opening_principal"] = json!([20., 30., 50.]);
    r["contract"]["rates"]["purchase"] = json!(0.18);
    r["contract"]["rates"]["cash_advance"] = json!(0.36);
    r["contract"]["rates"]["promotional"] = json!(0.);
    r["contract"]["rates"]["promotional_expiry_ordinal"] = json!(738921);
    r["contract"]["rates"]["post_promotion"] = json!(0.72);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    let coupon = (20. * 0.18 + 30. * 0.36) * 10. / 360. + 50. * 0.72 * 5. / 360.;
    close(
        out.economic
            .default
            .as_ref()
            .unwrap()
            .recognized_accrued_interest,
        coupon,
    );
    close(out.acquisition_opening.live[0].principal, 100.);
    close(out.acquisition_opening.live[1].principal, 0.);
    close(out.acquisition_opening.live[2].principal, 0.);
    balanced(&out);
}
#[test]
fn ownership_and_complete_tail_mutations_fail_closed() {
    for case in 0..15 {
        let mut r = raw();
        match case {
            0 => r["contract"]["initial_purchase_grace"] = json!(true),
            1 => r["contract"]["regain_grace_after_full_statement_payment"] = json!(true),
            2 => {
                r["contract"]["steps"][0]["action"] =
                    json!({"kind":"draw","balance_class":"purchase","amount":1.})
            }
            3 => {
                r["contract"]["steps"][0]["action"] =
                    json!({"kind":"fee","balance_class":"purchase","amount":1.})
            }
            4 => {
                r["contract"]["steps"][0]["action"] =
                    json!({"kind":"set_state","state":"nonaccrual"})
            }
            5 => r["contract"]["identity"]["measure"] = json!("pricing"),
            6 => r["acquisition"]["principal"] = json!(99.),
            7 => r["economic_source"]["revision"] = json!("wrong"),
            8 => r["contract"]["opening_unbilled_interest"][1] = json!(1.),
            9 => r["contract"]["collection_cost"] = json!(70.),
            10 => r["contract"]["horizon_days"] = json!(29),
            11 => r["contract"]["steps"][0]["ordinal"] = json!(738916),
            12 => r["policy"]["commitment"] = json!("infer_ccf"),
            13 => r["acquisition"]["cost"] = json!(1e16),
            _ => r["contract"]["steps"] = json!([]),
        }
        assert!(
            evaluate(&serde_json::from_value(r).unwrap()).is_err(),
            "case {case}"
        );
    }
    for case in 0..3 {
        let mut r = closure();
        match case {
            0 => r["contract"]["steps"][2]["action"]["amount"] = json!(1.),
            1 => r["termination"]["ordinal"] = json!(738926),
            _ => r["contract"]["steps"][1]["action"]["amount"] = json!(0.),
        }
        assert!(
            evaluate(&serde_json::from_value(r).unwrap()).is_err(),
            "closure case {case}"
        );
    }
}
#[test]
fn cancellation_returns_no_partial_result() {
    let mut n = 0;
    let err = adapter::evaluate_with_check(&request(), || {
        n += 1;
        if n == 8 {
            Err("COMPUTE_CANCELLED".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(err, "COMPUTE_CANCELLED");
    assert_eq!(n, 8);
}

#[test]
fn aggregate_work_rejects_dense_actions_before_source_generation() {
    let mut r = request();
    let start = r.contract.as_of_ordinal;
    r.contract.horizon_days = 1080;
    r.contract.steps = (0..3000)
        .map(|_| consumer_card::CardStep {
            ordinal: start + 1,
            action: consumer_card::CardAction::Payment { amount: 0. },
        })
        .collect();
    r.contract.steps.push(consumer_card::CardStep {
        ordinal: start + 10,
        action: consumer_card::CardAction::Default {},
    });
    let mut checks = 0;
    let error = adapter::evaluate_with_check(&r, || {
        checks += 1;
        Ok(())
    })
    .unwrap_err();
    assert!(
        error.contains("accounting work admission exceeded"),
        "{error}"
    );
    assert_eq!(checks, 1);
}

#[test]
fn equal_day_payments_keep_source_order_and_payment_after_default_rejects() {
    let mut r = raw();
    r["contract"]["steps"] = json!([{"ordinal":738926,"action":{"kind":"payment","amount":20.}},{"ordinal":738926,"action":{"kind":"payment","amount":30.}},{"ordinal":738926,"action":{"kind":"default"}}]);
    r["opening_allowance"] = json!(20.);
    r["chargeoffs"][0]["principal_amount"] = json!(20.);
    let out = evaluate(&serde_json::from_value(r.clone()).unwrap()).unwrap();
    let cash: Vec<_> = out
        .intents
        .iter()
        .filter(|i| i.event == "card_contractual_settlement")
        .map(|i| i.postings[0].amount)
        .collect();
    assert_eq!(cash, vec![20., 30.]);
    r["contract"]["steps"].as_array_mut().unwrap().swap(1, 2);
    let error = evaluate(&serde_json::from_value(r).unwrap()).unwrap_err();
    assert!(error.contains("card action after default"), "{error}");
}
