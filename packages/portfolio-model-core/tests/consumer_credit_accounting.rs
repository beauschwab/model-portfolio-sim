//! Public native component fixtures; whole-book adoption has separate gates.
use adapter::{evaluate, Request};
use credit_accounting::{AccountCode, AccruedPolicy, BasisPolicy};
use portfolio_model_core::{
    consumer_credit_accounting as adapter, consumer_installment, credit_accounting,
};
use serde_json::json;

fn raw() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/consumer-credit-accounting-1.json"
    ))
    .unwrap()
}
fn request() -> Request {
    serde_json::from_value(raw()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn total(r: &adapter::ResultData, account: AccountCode) -> f64 {
    r.intents
        .iter()
        .flat_map(|i| &i.postings)
        .filter(|p| p.account == account)
        .map(|p| p.amount)
        .sum()
}
fn balanced(r: &adapter::ResultData) {
    for i in &r.intents {
        close(i.postings.iter().map(|p| p.amount).sum(), 0.);
    }
    assert_eq!(r.lineage.len(), r.intents.len());
    for (index, line) in r.lineage.iter().enumerate() {
        assert_eq!(line.intent_index, index);
        assert_eq!(
            line.original_yield_anchor,
            r.raw_request.acquisition.original_yield_anchor
        );
        assert_eq!(line.economic_source, r.raw_request.economic_source);
        assert_eq!(line.ordinal, r.intents[index].recognition_ordinal);
    }
}

#[test]
fn principal_only_gross_recovery_and_separate_cost_conserve_once() {
    let r = request();
    let out = evaluate(&r).unwrap();
    let d = out.economic.default.as_ref().unwrap();
    // Independent ACT/360: 100 * .36 * 10/360 = 1, default suppresses due cash.
    close(d.recognized_accrued_interest, 1.);
    close(d.gross_recovery, 60.);
    close(d.net_recovery, 50.);
    let rec = out.recovery.as_ref().unwrap();
    close(rec.principal_only_loss, 40.);
    close(rec.total_economic_loss, 50.);
    close(out.reporting.owned.claims[0].outstanding_principal, 100.);
    assert_eq!(
        out.reporting.owned.claims[0].remaining_recoveries[0].payment_ordinal,
        r.contract.as_of_ordinal + 30
    );
    close(total(&out, AccountCode::Cash), 50.);
    close(total(&out, AccountCode::CreditLossExpense), 10.);
    close(total(&out, AccountCode::LivePrincipal), -100.);
    close(total(&out, AccountCode::Allowance), 40.);
    close(out.closing.claims[0].outstanding_principal, 0.);
    close(out.closing.allowance, 0.);
    close(out.closing.claims[0].economic_principal_loss, 40.);
    // Default reverses actual accrued interest and recognizes opposite signed EIR basis.
    close(total(&out, AccountCode::InterestIncome), 1.);
    close(total(&out, AccountCode::BasisRecognitionExpense), -1.);
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), -10.);
    assert!(!out.mixed_ledger_adoption);
    balanced(&out);
    let cost = out
        .intents
        .iter()
        .find(|i| i.event == "product_reviewed_recovery_cost")
        .unwrap();
    assert_eq!(cost.policy, r.policy.cost_review);
    assert_eq!(cost.claim_id.as_deref(), Some(r.claim_id.as_str()));
}

#[test]
fn auto_repossession_uses_same_product_owned_gross_cash_cost_lane() {
    let mut r = raw();
    let start = r["contract"]["as_of_ordinal"].as_i64().unwrap();
    r["contract"]["product"] = json!({"kind":"auto","new_vehicle":false,"vintage_ordinal":start,"original_ltv":1.,"channel":"dealer","collateral_value_as_of":60.,"annual_depreciation":0.,"sale_haircut":0.,"repossession_cost":10.,"recovery_delay_days":20});
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    close(total(&out, AccountCode::Cash), 50.);
    close(total(&out, AccountCode::CreditLossExpense), 10.);
    balanced(&out);
}

#[test]
fn zero_net_recovery_does_not_hide_gross_obligations_or_invent_cash_origin() {
    let mut r = raw();
    r["contract"]["product"]["collection_cost"] = json!(60.);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    assert!(out.economic.cashflows.is_empty());
    close(total(&out, AccountCode::Cash), 0.);
    close(total(&out, AccountCode::CreditLossExpense), 60.);
    close(out.recovery.as_ref().unwrap().total_economic_loss, 100.);
    let cost = out
        .lineage
        .iter()
        .find(|l| l.phase == "product_reviewed_recovery_cost")
        .unwrap();
    assert_eq!(cost.product_event_ids, vec!["default:738926"]);
    balanced(&out);
}

#[test]
fn original_eir_prefix_and_payoff_retire_premium_without_income_plug() {
    let mut r = raw();
    r["contract"]["steps"][0]["action"] = json!({"kind":"payoff"});
    r["acquisition"]["cost"] = json!(103.);
    r["acquisition"]["original_annual_effective_rate"] = json!(0.02);
    r["opening_allowance"] = json!(0.);
    r["chargeoffs"] = json!([]);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    let earned = 103. * (1.02_f64.powf(10. / 365.) - 1.);
    close(
        out.income_prefix
            .rows
            .iter()
            .map(|r| r.effective_interest)
            .sum(),
        earned,
    );
    close(total(&out, AccountCode::Cash), 101.);
    close(total(&out, AccountCode::InterestIncome), -earned);
    close(
        total(&out, AccountCode::BasisRecognitionExpense),
        3. + earned - 1.,
    );
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), -2.);
    close(out.closing.live[0].principal, 0.);
    close(out.closing.live[0].book_adjustment, 0.);
    assert!(out.recovery.is_none());
    assert!(out.closing.claims.is_empty());
    balanced(&out);
}

#[test]
fn positive_eir_default_stops_prefix_and_retains_reviewed_claim_rights() {
    let mut r = request();
    r.acquisition.cost = 103.;
    r.acquisition.original_annual_effective_rate = 0.02;
    r.policy.accounting.accrued = AccruedPolicy::TransferUntilReviewedReversal;
    r.policy.accounting.basis = BasisPolicy::TransferUntilReviewedChargeoff;
    r.chargeoffs.clear();
    let out = evaluate(&r).unwrap();
    let earned = 103. * (1.02_f64.powf(10. / 365.) - 1.);
    close(
        out.reporting.owned.claims[0].outstanding_accrued_interest,
        1.,
    );
    close(
        out.closing.claims[0].outstanding_book_adjustment,
        3. + earned - 1.,
    );
    close(out.closing.claims[0].outstanding_principal, 40.);
    assert!(out.daily[11..]
        .iter()
        .all(|d| d.effective_interest == 0. && d.contractual_interest == 0.));
    close(total(&out, AccountCode::Cash), 50.);
    close(total(&out, AccountCode::InterestIncome), -earned);
    balanced(&out);
}

#[test]
fn discount_and_negative_original_yield_are_not_negative_coupon() {
    let mut r = request();
    r.contract.steps[0].action = consumer_installment::InstallmentAction::Payoff {};
    r.acquisition.cost = 90.;
    r.acquisition.original_annual_effective_rate = -0.01;
    r.opening_allowance = 0.;
    r.chargeoffs.clear();
    let out = evaluate(&r).unwrap();
    let effective = 90. * (0.99_f64.powf(10. / 365.) - 1.);
    close(total(&out, AccountCode::InterestIncome), -effective);
    close(
        total(&out, AccountCode::BasisRecognitionExpense),
        -10. + effective - 1.,
    );
    close(out.daily.iter().map(|d| d.earnings_delta).sum(), 11.);
    close(total(&out, AccountCode::Cash), 101.);
    close(out.closing.live[0].book_adjustment, 0.);
    assert!(out.economic.recognition.iter().all(|s| s.interest >= 0.));
    balanced(&out);
}

#[test]
fn same_day_default_recovery_and_chargeoff_follow_explicit_intent_order() {
    let mut r = raw();
    r["contract"]["product"]["recovery_delay_days"] = json!(0);
    r["chargeoffs"][0]["recognition_ordinal"] = json!(738926);
    let out = evaluate(&serde_json::from_value(r).unwrap()).unwrap();
    let names: Vec<_> = out
        .intents
        .iter()
        .filter(|i| i.recognition_ordinal == 738926)
        .map(|i| i.event.as_str())
        .collect();
    let default = names
        .iter()
        .position(|n| *n == "economic_default_transfer")
        .unwrap();
    let chargeoff = names.iter().position(|n| n.contains("chargeoff")).unwrap();
    let recovery = names
        .iter()
        .position(|n| n.contains("recovery_cash"))
        .unwrap();
    let cost = names
        .iter()
        .position(|n| *n == "product_reviewed_recovery_cost")
        .unwrap();
    assert!(default < chargeoff && chargeoff < recovery && recovery < cost);
    close(total(&out, AccountCode::Cash), 50.);
    close(out.closing.claims[0].outstanding_principal, 0.);
    balanced(&out);
}

#[test]
fn unsupported_ownership_and_bounds_reject_whole_request() {
    for case in 0..14 {
        let mut r = raw();
        match case {
            0 => r["contract"]["identity"]["measure"] = json!("pricing"),
            1 => r["acquisition"]["principal"] = json!(99.),
            2 => r["contract"]["opening_accrued_interest"] = json!(1.),
            3 => r["contract"]["steps"][0]["action"] = json!({"kind":"fee","amount":1.}),
            4 => {
                r["contract"]["steps"][0]["action"] =
                    json!({"kind":"set_state","state":"nonaccrual"})
            }
            5 => r["contract"]["product"]["collection_cost"] = json!(70.),
            6 => r["contract"]["horizon_days"] = json!(20),
            7 => r["economic_source"]["revision"] = json!("wrong"),
            8 => r["acquisition"]["cost"] = json!(1e16),
            9 => r["policy"]["original_income"] = json!("infer_from_q"),
            10 => r["chargeoffs"][0]["principal_amount"] = json!(41.),
            11 => r["path_id"] = json!("x".repeat(257)),
            12 => r["acquisition"]["accounting_basis"] = json!("pcd_gross_up"),
            _ => {
                r["contract"]["steps"] =
                    json!([{"ordinal":738946,"action":{"kind":"payment","amount":1.}}]);
                r["chargeoffs"] = json!([]);
            }
        }
        assert!(
            evaluate(&serde_json::from_value(r).unwrap()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn cancellation_is_cooperative_and_returns_no_partial_result() {
    let r = request();
    let mut checks = 0;
    let err = adapter::evaluate_with_check(&r, || {
        checks += 1;
        if checks == 8 {
            Err("COMPUTE_CANCELLED".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(err, "COMPUTE_CANCELLED");
    assert_eq!(checks, 8);
}

#[test]
fn aggregate_work_is_charged_before_projecting_dense_source() {
    let mut r = request();
    let start = r.contract.as_of_ordinal;
    r.contract.horizon_days = 1080;
    r.contract.payment_dates = (1..=1080).map(|day| start + day).collect();
    r.contract.steps = (1..=1080)
        .map(|day| consumer_installment::InstallmentStep {
            ordinal: start + day,
            action: consumer_installment::InstallmentAction::Payment { amount: 0. },
        })
        .collect();
    r.chargeoffs.clear();
    let mut checks = 0;
    let err = adapter::evaluate_with_check(&r, || {
        checks += 1;
        Ok(())
    })
    .unwrap_err();
    assert!(err.contains("work admission exceeded"), "{err}");
    assert_eq!(checks, 1); // No economic generation after aggregate admission fails.
}
