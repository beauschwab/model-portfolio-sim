use portfolio_model_core::nonintegral_fee::{evaluate, evaluate_with_check, Request, SCHEMA};
use serde_json::{json, Value};

fn raw() -> Value {
    let i = json!({"id":"review", "revision":"1"});
    json!({"schema":SCHEMA,"measure":"physical","currency":"USD","monetary_unit":"USD","policy":i,
        "asof_ordinal":730000,"reporting_end_ordinal":730015,"complete_end_ordinal":730040,
        "fees":[{"fee_id":"fee","source":i,"classification_review":i,"service":i,
            "position_id":"loan","tranche_id":"base","entitlement":5.,"prepaid_refunds_allowed":true}],
        "events":[]})
}
fn event(day: i32, sequence: u32, action: &str, amount: f64, reason: &str) -> Value {
    json!({"event_id":format!("event-{sequence}"),"fee_id":"fee","source":{"id":"review","revision":"1"},
        "review":{"id":"action-review","revision":"1"},"ordinal":730000+day,"sequence":sequence,
        "action":action,"amount":amount,"reason":reason})
}
fn result(v: Value) -> Result<Value, String> {
    let r: Request = serde_json::from_value(v).map_err(|e| e.to_string())?;
    serde_json::to_value(evaluate(&r)?).map_err(|e| e.to_string())
}
#[test]
fn earned_receivable_survives_report_and_later_cash_has_no_second_income() {
    let mut r = raw();
    r["events"] = json!([
        event(10, 0, "earn", 5., "service"),
        event(20, 1, "cash", 5., "settlement")
    ]);
    let v = result(r).unwrap();
    assert_eq!(v["reporting"][0]["state"]["fee_receivable"], 5.);
    assert_eq!(v["closing"][0]["state"]["fee_receivable"], 0.);
    assert_eq!(v["groups"][0]["postings"][0]["account"], "fee_income");
    assert_eq!(v["groups"][1]["postings"].as_array().unwrap().len(), 2);
    assert_eq!(v["loan_carrying_value_changed"], false);
}
#[test]
fn prepaid_receipt_is_deferred_until_service_not_cash_income() {
    let mut r = raw();
    r["reporting_end_ordinal"] = json!(730007);
    r["events"] = json!([
        event(5, 0, "cash", 5., "settlement"),
        event(10, 1, "earn", 5., "service")
    ]);
    let v = result(r).unwrap();
    assert_eq!(v["reporting"][0]["state"]["deferred_fee_income"], -5.);
    assert_eq!(
        v["groups"][0]["postings"][1]["account"],
        "deferred_fee_income"
    );
    assert_eq!(v["closing"][0]["state"]["deferred_fee_income"], 0.);
}
#[test]
fn negative_contract_has_gross_payable_and_prepaid_expense() {
    for prepaid in [false, true] {
        let mut r = raw();
        r["fees"][0]["entitlement"] = json!(-5.);
        r["reporting_end_ordinal"] = json!(730007);
        r["events"] = if prepaid {
            json!([
                event(5, 0, "cash", -5., "settlement"),
                event(10, 1, "earn", -5., "service")
            ])
        } else {
            json!([
                event(5, 0, "earn", -5., "service"),
                event(10, 1, "cash", -5., "settlement")
            ])
        };
        let v = result(r).unwrap();
        let s = &v["reporting"][0]["state"];
        assert_eq!(
            s[if prepaid {
                "prepaid_fee_expense"
            } else {
                "fee_payable"
            }],
            if prepaid { 5. } else { -5. }
        );
        assert_eq!(s["fee_receivable"], 0.);
    }
}
#[test]
fn reviewed_default_waiver_and_recovery_are_separate_complete_tail() {
    let mut r = raw();
    r["events"] = json!([
        event(5, 0, "earn", 5., "service"),
        event(10, 1, "waive_unpaid_earned", 2., "default"),
        event(20, 2, "cash", 3., "settlement"),
        event(30, 3, "recover_waived", 2., "default")
    ]);
    let v = result(r).unwrap();
    assert_eq!(v["reporting"][0]["state"]["fee_receivable"], 3.);
    assert_eq!(v["closing"][0]["state"]["recovered_waived"], 2.);
    assert_eq!(
        v["groups"][3]["postings"][1]["account"],
        "fee_recovery_income"
    );
    assert_eq!(v["native_credit_event_binding"], false);
}
#[test]
fn exercise_cancels_future_service_but_keeps_prepaid_refund_obligation() {
    let mut r = raw();
    r["events"] = json!([
        event(5, 0, "cash", 5., "settlement"),
        event(10, 1, "cancel_unearned", 5., "exercise"),
        event(20, 2, "refund_prepaid", 5., "settlement")
    ]);
    let v = result(r).unwrap();
    assert_eq!(v["reporting"][0]["state"]["deferred_fee_income"], -5.);
    assert!(v["groups"][1]["postings"].as_array().unwrap().is_empty());
    assert_eq!(v["closing"][0]["state"]["deferred_fee_income"], 0.);
}
#[test]
fn supplied_tail_does_not_invent_terminal_settlement_or_waiver() {
    let mut r = raw();
    r["events"] = json!([event(5, 0, "earn", 5., "service")]);
    let v = result(r).unwrap();
    assert_eq!(v["closing"][0]["state"]["fee_receivable"], 5.);
    assert_eq!(v["groups"].as_array().unwrap().len(), 1);
}
#[test]
fn strict_rights_no_rounding_tolerance_grants_extra_money() {
    for action in ["earn", "cash", "cancel_unearned"] {
        let mut r = raw();
        r["events"] = json!([event(
            5,
            0,
            action,
            5. + 1e-12,
            if action == "earn" {
                "service"
            } else if action == "cash" {
                "settlement"
            } else {
                "exercise"
            }
        )]);
        assert!(result(r).unwrap_err().contains("exceeds"));
    }
}
#[test]
fn unsupported_refund_and_unearned_waiver_or_recovery_reject() {
    for action in ["refund_prepaid", "waive_unpaid_earned", "recover_waived"] {
        let mut r = raw();
        r["events"] = json!([event(
            5,
            0,
            action,
            1.,
            if action == "refund_prepaid" {
                "settlement"
            } else {
                "default"
            }
        )]);
        assert!(result(r).is_err());
    }
    let mut r = raw();
    r["fees"][0]["prepaid_refunds_allowed"] = json!(false);
    r["events"] = json!([
        event(5, 0, "cash", 5., "settlement"),
        event(10, 1, "refund_prepaid", 5., "settlement")
    ]);
    assert!(result(r).unwrap_err().contains("not allowed"));
}
#[test]
fn source_sign_clock_measure_and_review_do_not_grant_ownership() {
    for kind in 0..8 {
        let mut r = raw();
        r["events"] = json!([event(5, 0, "earn", 5., "service")]);
        match kind {
            0 => r["events"][0]["source"]["id"] = json!("foreign"),
            1 => r["events"][0]["amount"] = json!(-5.),
            2 => r["events"][0]["reason"] = json!("default"),
            3 => r["events"][0]["ordinal"] = json!(730041),
            4 => r["measure"] = json!("pricing"),
            5 => r["events"][0]["review"]["revision"] = json!(""),
            6 => r["opening_checkpoint"] = json!({"fee_receivable":5.}),
            _ => r["fees"][0]["entitlement"] = json!(true),
        }
        assert!(result(r).is_err());
    }
}
#[test]
fn duplicate_clock_and_repeated_event_reject() {
    let mut r = raw();
    let a = event(5, 0, "earn", 1., "service");
    r["events"] = json!([a, a]);
    assert!(result(r).is_err());
    let mut r = raw();
    r["events"] = json!([
        event(5, 0, "earn", 1., "service"),
        event(4, 1, "earn", 1., "service")
    ]);
    assert!(result(r).is_err());
}
#[test]
fn cancellation_fences_fee_validation_and_full_tail_generation() {
    let r: Request = serde_json::from_value(raw()).unwrap();
    let mut checks = 0;
    let err = evaluate_with_check(&r, || {
        checks += 1;
        if checks == 3 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(err, "cancelled");
}

#[test]
fn preflight_rejects_excessive_cardinality_and_calendar_before_generation() {
    let mut r = raw();
    let f = r["fees"][0].clone();
    r["fees"] = json!(vec![f; 2049]);
    assert!(result(r).unwrap_err().contains("cardinality"));
    let mut r = raw();
    r["complete_end_ordinal"] = json!(730000 + 36601);
    assert!(result(r).unwrap_err().contains("calendar"));
}
