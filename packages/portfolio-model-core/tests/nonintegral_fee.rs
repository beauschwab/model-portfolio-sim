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
fn acquired_fee_asset_and_liability_release_separately_from_service_cash() {
    for sign in [1., -1.] {
        let mut r = raw();
        r["fees"][0]["entitlement"] = json!(sign * 5.);
        r["events"] = json!([
            event(0, 0, "acquire", sign * 4., "other_reviewed"),
            event(10, 1, "earn", sign * 5., "service"),
            event(10, 2, "release_acquired_cost", sign * 1., "other_reviewed"),
            event(20, 3, "cash", sign * 5., "settlement"),
            event(20, 4, "release_acquired_cost", sign * 3., "other_reviewed")
        ]);
        let actual = result(r).unwrap();
        let report = &actual["reporting"][0]["state"];
        assert_eq!(
            report[if sign > 0. {
                "acquired_fee_asset"
            } else {
                "acquired_fee_liability"
            }],
            sign * 3.
        );
        assert_eq!(report["earned"], sign * 5.);
        assert_eq!(
            actual["closing"][0]["state"]["released_acquired_cost"],
            sign * 4.
        );
        assert!(actual["closing"][0]["state"]
            .get("acquired_fee_asset")
            .is_none());
        assert!(actual["closing"][0]["state"]
            .get("acquired_fee_liability")
            .is_none());
        assert_eq!(actual["groups"][0]["postings"][0]["account"], "cash");
        assert_eq!(
            actual["groups"][2]["postings"][0]["account"],
            "fee_acquisition_expense"
        );
    }
}
#[test]
fn acquired_fee_basis_cannot_be_reacquired_or_over_released() {
    for events in [
        vec![
            event(0, 0, "acquire", 4., "other_reviewed"),
            event(0, 1, "acquire", 4., "other_reviewed"),
        ],
        vec![
            event(0, 0, "acquire", 4., "other_reviewed"),
            event(1, 1, "release_acquired_cost", 5., "other_reviewed"),
        ],
        vec![event(1, 0, "acquire", 4., "other_reviewed")],
    ] {
        let mut r = raw();
        r["events"] = json!(events);
        assert!(result(r).is_err());
    }
}
#[test]
fn explicit_pricing_fee_projection_cannot_enter_the_physical_accounting_entrypoint() {
    let mut r = raw();
    r["measure"] = json!("pricing");
    r["events"] = json!([
        event(10, 0, "earn", 5., "service"),
        event(20, 1, "cash", 5., "settlement")
    ]);
    let typed: Request = serde_json::from_value(r).unwrap();
    assert!(evaluate(&typed).is_err());
    let actual =
        portfolio_model_core::nonintegral_fee::evaluate_pricing_with_check(&typed, || Ok(()))
            .unwrap();
    assert_eq!(
        actual.raw_request().measure,
        portfolio_model_core::cashflow::CashflowMeasure::Pricing
    );
    assert_eq!(actual.closing()[0].state.earned, 5.);
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
fn earned_refund_reverses_only_collected_income_and_preserves_gross_service() {
    for sign in [1., -1.] {
        let mut r = raw();
        r["fees"][0]["entitlement"] = json!(5. * sign);
        r["fees"][0]["earned_refunds_allowed"] = json!(true);
        r["events"] = json!([
            event(5, 0, "earn", 5. * sign, "service"),
            event(10, 1, "cash", 5. * sign, "settlement"),
            event(20, 2, "refund_earned", 2. * sign, "other_reviewed")
        ]);
        let v = result(r).unwrap();
        assert!(v["reporting"][0]["state"].get("refunded_earned").is_none());
        let state = &v["closing"][0]["state"];
        assert_eq!(state["earned"], 5. * sign);
        assert_eq!(state["refunded_earned"], 2. * sign);
        assert_eq!(state["net_service_cash"], 3. * sign);
        assert_eq!(state["fee_receivable"], 0.);
        assert_eq!(state["fee_payable"], 0.);
        assert_eq!(state["unearned_entitlement"], 0.);
        assert_eq!(
            v["groups"][2]["postings"],
            json!([
                {"account":"cash", "amount":-2. * sign},
                {"account":"fee_income", "amount":2. * sign}
            ])
        );
    }
}

#[test]
fn earned_and_prepaid_refunds_are_distinct_without_reopening_service_capacity() {
    let mut r = raw();
    r["fees"][0]["earned_refunds_allowed"] = json!(true);
    r["events"] = json!([
        event(2, 0, "cash", 5., "settlement"),
        event(5, 1, "earn", 2., "service"),
        event(10, 2, "refund_earned", 1., "exercise"),
        event(11, 3, "cancel_unearned", 3., "exercise"),
        event(12, 4, "refund_prepaid", 3., "settlement")
    ]);
    let v = result(r.clone()).unwrap();
    assert_eq!(v["groups"][2]["next"]["deferred_fee_income"], -3.);
    assert_eq!(v["closing"][0]["state"]["net_service_cash"], 1.);
    assert_eq!(v["closing"][0]["state"]["unearned_entitlement"], 0.);
    for action in ["earn", "cash", "refund_prepaid"] {
        let mut changed = r.clone();
        changed["events"].as_array_mut().unwrap().push(event(
            13,
            5,
            action,
            1.,
            if action == "earn" {
                "service"
            } else {
                "settlement"
            },
        ));
        assert!(result(changed).unwrap_err().contains("exceeds"));
    }
}

#[test]
fn earned_refund_requires_permission_review_and_remaining_collected_right() {
    for case in 0..6 {
        let mut r = raw();
        r["fees"][0]["earned_refunds_allowed"] = json!(true);
        r["events"] = json!([
            event(2, 0, "earn", 5., "service"),
            event(5, 1, "cash", 2., "settlement"),
            event(10, 2, "refund_earned", 2., "other_reviewed")
        ]);
        match case {
            0 => r["fees"][0]["earned_refunds_allowed"] = json!(false),
            1 => r["events"][2]["amount"] = json!(2. + 1e-12),
            2 => r["events"][2]["reason"] = json!("settlement"),
            3 => r["events"][2]["review"]["id"] = json!(""),
            4 => r["events"].as_array_mut().unwrap().push(event(
                11,
                3,
                "refund_earned",
                1.,
                "default",
            )),
            _ => r["events"][0]["amount"] = json!(1.),
        }
        assert!(result(r).is_err(), "case {case}");
    }
}

#[test]
fn waived_recovery_is_not_collected_service_eligible_for_refund() {
    let mut r = raw();
    r["fees"][0]["earned_refunds_allowed"] = json!(true);
    r["events"] = json!([
        event(2, 0, "earn", 5., "service"),
        event(5, 1, "waive_unpaid_earned", 3., "default"),
        event(10, 2, "cash", 2., "settlement"),
        event(11, 3, "recover_waived", 3., "default"),
        event(12, 4, "refund_earned", 2., "other_reviewed")
    ]);
    let v = result(r.clone()).unwrap();
    assert_eq!(v["closing"][0]["state"]["recovered_waived"], 3.);
    r["events"]
        .as_array_mut()
        .unwrap()
        .push(event(13, 5, "refund_earned", 1., "default"));
    assert!(result(r).unwrap_err().contains("exceeds"));
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

#[test]
fn acquired_fee_face_six_cost_five_has_contra_valuation_without_new_service_income() {
    use portfolio_model_core::nonintegral_fee as f;
    use serde_json::json;
    for sign in [1., -1.] {
        let id = json!({"id":"reviewed-transfer","revision":"1"});
        let source = json!({"schema":f::SCHEMA,"measure":"physical","currency":"USD","monetary_unit":"currency_units","policy":id,"asof_ordinal":738856,"reporting_end_ordinal":738896,"complete_end_ordinal":738896,"fees":[{"fee_id":"bought","source":id,"classification_review":id,"service":id,"position_id":"commercial","tranche_id":"original","entitlement":sign*6.,"prepaid_refunds_allowed":false}],"events":[{"event_id":"seller-service","fee_id":"bought","source":id,"review":id,"ordinal":738866,"sequence":0,"action":"earn","reason":"service","amount":sign*6.},{"event_id":"buyer-cash","fee_id":"bought","source":id,"review":id,"ordinal":738896,"sequence":1,"action":"cash","reason":"settlement","amount":sign*6.}]});
        let full: f::Request = serde_json::from_value(source.clone()).unwrap();
        let transfer = f::transfer_before(
            &full,
            738886,
            serde_json::from_value(id.clone()).unwrap(),
            || Ok(()),
        )
        .unwrap();
        let mut raw = source.clone();
        raw["asof_ordinal"] = json!(738886);
        raw["events"] = json!([
            {"event_id":"buyer-basis","fee_id":"bought","source":id,"review":id,"ordinal":738886,"sequence":0,"action":"acquire","reason":"other_reviewed","amount":-sign},
            {"event_id":"buyer-cash","fee_id":"bought","source":id,"review":id,"ordinal":738896,"sequence":1,"action":"cash","reason":"settlement","amount":sign*6.},
            {"event_id":"buyer-release","fee_id":"bought","source":id,"review":id,"ordinal":738896,"sequence":2,"action":"release_acquired_cost","reason":"other_reviewed","amount":-sign}
        ]);
        let raw: f::Request = serde_json::from_value(raw).unwrap();
        let out = f::evaluate_transferred_with_check(&raw, &transfer, || Ok(())).unwrap();
        let mut collision = raw.clone();
        collision.events[0].event_id = "native-fee-transfer:reviewed-transfer:0".into();
        assert!(
            f::evaluate_transferred_with_check(&collision, &transfer, || Ok(()))
                .unwrap_err()
                .contains("reserved")
        );
        let basis = &out.groups()[1].next;
        if sign > 0. {
            assert_eq!(basis.fee_receivable, 6.);
            assert_eq!(basis.acquired_fee_asset_valuation_adjustment, -1.);
            assert_eq!(basis.acquired_fee_liability, 0.);
        } else {
            assert_eq!(basis.fee_payable, -6.);
            assert_eq!(basis.acquired_fee_liability_valuation_adjustment, 1.);
            assert_eq!(basis.acquired_fee_asset, 0.);
        }
        let postings = out
            .groups()
            .iter()
            .flat_map(|g| &g.postings)
            .collect::<Vec<_>>();
        assert_eq!(
            postings
                .iter()
                .filter(|p| p.account == f::Account::FeeIncome)
                .map(|p| p.amount)
                .sum::<f64>(),
            0.
        );
        assert_eq!(
            postings
                .iter()
                .filter(|p| p.account == f::Account::Cash)
                .map(|p| p.amount)
                .sum::<f64>(),
            sign
        );
        let close = &out.closing()[0].state;
        assert_eq!(close.fee_receivable, 0.);
        assert_eq!(close.fee_payable, 0.);
        assert_eq!(close.acquired_fee_asset_valuation_adjustment, 0.);
        assert_eq!(close.acquired_fee_liability_valuation_adjustment, 0.);
    }
}
