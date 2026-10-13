use portfolio_risk_native::{
    deposit_graph::{self, DepositGraph},
    deposit_graph_capture::{self, Request},
};
use serde_json::Value;
fn raw() -> Value {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/deposit-graph-dated-ledger-1.json"
    ))
    .unwrap();
    // Capture-only hand fixture is independent of the daily ledger's minimum
    // thirty-day reporting admission in the shared publication example.
    let asof = v["graph"]["as_of_ordinal"].as_i64().unwrap();
    v["graph"]["reporting_ordinals"] = serde_json::json!([asof, asof + 1]);
    paths(&mut v, |r| {
        r["horizon_days"] = 2.into();
        r["drivers"]
            .as_array_mut()
            .unwrap()
            .retain(|d| d["ordinal"].as_i64().unwrap() < asof + 2);
        for segment in r["segments"].as_array_mut().unwrap() {
            segment["maturity_ordinal"] = (asof + 2).into();
            segment["interest_payment_ordinals"] = serde_json::json!([asof + 2]);
        }
    });
    v
}
fn request(v: Value) -> Request {
    serde_json::from_value(serde_json::json!({"schema":"deposit-graph-capture-1","graph":v["graph"],"owners":v["owners"],"policy":v["capture_policy"]})).unwrap()
}
fn paths(v: &mut Value, f: impl Fn(&mut Value)) {
    for set in v["graph"]["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
    {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in set[role].as_array_mut().unwrap() {
                f(&mut h["request"]);
            }
        }
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
#[test]
fn full_tail_signed_liability_expense_and_original_origins() {
    let r = request(raw());
    let out = deposit_graph_capture::prepare(&r).unwrap();
    assert_eq!(out.capture.scheduled.sources.len(), 1);
    let capture = &out.capture.scheduled;
    near(capture.openings[0].principal, 1000.);
    assert_eq!(capture.owners[0].direction, "payment");
    assert_eq!(capture.sources[0].schedule.horizon_days, 2);
    for event in &capture.sources[0].schedule.events {
        if event.components.principal != 0. {
            near(event.components.principal, -1000.);
            near(event.components.cash_interest, 0.);
        } else {
            near(event.components.cash_interest, -2.);
            near(event.components.accrued_interest, -2.);
        }
        assert_eq!(event.day_offset, 2);
        assert_eq!(event.components.book_amortization, 0.);
    }
    assert_eq!(capture.accrual_plans.len(), 3);
    for plan in &capture.accrual_plans {
        assert_eq!(plan.segments.len(), 2);
        for s in &plan.segments {
            near(s.accrued_interest, -1.);
            assert_eq!(s.end_ordinal - s.start_ordinal, 1);
            assert!(s.origin_id.starts_with("deposit-interest:"));
        }
    }
    assert_eq!(out.capture.cash_origins.len(), 6);
    assert_eq!(out.graph.native_sources.len(), 12);
    assert_eq!(capture.scenarios[2].severity, 0.5);
}
#[test]
fn capture_does_not_reprice_or_change_frozen_anchor() {
    let r = request(raw());
    let graph = DepositGraph::new(&r.graph).unwrap();
    let anchor = serde_json::to_value(graph.base_anchor()).unwrap();
    let first = deposit_graph_capture::capture(&graph, &r.owners, &r.policy).unwrap();
    let second = deposit_graph_capture::capture(&graph, &r.owners, &r.policy).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(anchor, serde_json::to_value(graph.base_anchor()).unwrap());
}
#[test]
fn actual_day0_competing_runoff_is_once_only_and_signed() {
    let mut v = raw();
    paths(&mut v, |r| {
        r["segments"][0]["retention"]["withdrawal_per_year"] = 2.into();
        r["segments"][0]["retention"]["closure_per_year"] = 1.into();
    });
    let r = request(v);
    let out = deposit_graph_capture::prepare(&r).unwrap();
    let events = &out.capture.scheduled.sources[0].schedule.events;
    let day0: Vec<_> = events.iter().filter(|e| e.day_offset == 0).collect();
    assert_eq!(day0.len(), 6);
    let exit = -(-3_f64 / 365.).exp_m1();
    near(day0[0].components.principal, -1000. * exit * 2. / 3.);
    near(day0[1].components.principal, -1000. * exit / 3.);
    assert_eq!(day0[0].components.cash_interest, 0.);
    for run in &r.graph.scenarios {
        near(events.iter().filter(|e|matches!(&e.scenario,portfolio_model_core::cashflow::ScenarioSelector::Named{scenario_id} if scenario_id==&run.run_id)).map(|e|e.components.principal).sum(),-1000.);
    }
}
#[test]
fn actual_day_leap_expense_remains_daily_not_uniform_coupon() {
    let mut v = raw();
    paths(&mut v, |r| r["day_count"] = "act360".into());
    let out = deposit_graph_capture::prepare(&request(v)).unwrap();
    for plan in &out.capture.scheduled.accrual_plans {
        for segment in &plan.segments {
            near(segment.accrued_interest, -365. / 360.);
        }
    }
}
#[test]
fn unsupported_boundaries_reject_complete_capture() {
    for case in ["inflow", "opening", "negative", "policy", "owner", "weight"] {
        let mut v = raw();
        match case {
            "inflow" => paths(&mut v, |r| {
                r["drivers"][1]["external_inflows"][0] = 1.into()
            }),
            "opening" => paths(&mut v, |r| {
                r["segments"][0]["opening_accrued_interest"] = 1.into()
            }),
            "negative" => paths(&mut v, |r| {
                r["segments"][0]["repricing"]["floor"] = (-0.01).into()
            }),
            "policy" => v["capture_policy"]["book_basis"] = "invented_eir".into(),
            "owner" => v["owners"][0]["direction"] = "receipt".into(),
            _ => {
                let p = &mut v["graph"]["positions"][0]["histories"][0]["physical"];
                let mut other = p[0].clone();
                p[0]["weight"] = (0.5).into();
                other["weight"] = (0.5).into();
                other["path_id"] = "second-physical".into();
                p.as_array_mut().unwrap().push(other);
            }
        }
        assert!(
            deposit_graph_capture::prepare(&request(v)).is_err(),
            "{case}"
        );
    }
}
#[test]
fn declared_terminal_liquidation_is_preserved_as_assumption() {
    let mut v = raw();
    paths(&mut v, |r| {
        r["segments"][0]["maturity_ordinal"] = Value::Null;
        r["terminal"] = serde_json::json!({"kind":"assumed_liquidation","rationale":"explicit reviewed horizon liquidation"});
    });
    let out = deposit_graph_capture::prepare(&request(v)).unwrap();
    assert!(out.capture.scheduled.sources[0]
        .schedule
        .events
        .iter()
        .filter(|e| e.components.principal != 0.)
        .all(|e| matches!(
            e.provenance,
            portfolio_model_core::cashflow::EventProvenance::Assumed { .. }
        )));
    assert!(
        out.capture
            .cash_origins
            .iter()
            .filter(|e| e.kind == "assumed_terminal_payout")
            .count()
            == 3
    );
}
#[test]
fn zero_balance_declares_full_no_event_coverage() {
    let mut v = raw();
    paths(&mut v, |r| r["segments"][0]["opening_balance"] = 0.into());
    for p in v["graph"]["positions"].as_array_mut().unwrap() {
        p["oas_target"]["base_oas"] = 0.into();
    }
    let out = deposit_graph_capture::prepare(&request(v)).unwrap();
    assert!(out.capture.scheduled.sources[0].schedule.events.is_empty());
    assert_eq!(out.capture.scheduled.sources[0].coverage.len(), 3);
    near(out.capture.scheduled.openings[0].principal, 0.);
}
#[test]
fn graph_projection_and_capture_share_same_native_state() {
    let r = request(raw());
    let snapshot = deposit_graph::analyze(&r.graph).unwrap();
    let out = deposit_graph_capture::prepare(&r).unwrap();
    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        serde_json::to_value(out.graph).unwrap()
    );
}
