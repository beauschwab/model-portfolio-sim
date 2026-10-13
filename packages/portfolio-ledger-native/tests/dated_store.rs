mod common;
use portfolio_ledger_native::dated_store::*;
use serde_json::{json, Value};

fn input(pay: u32, end: u32) -> Value {
    common::request(pay, end, 6., 100., false)["dated"].clone()
}
fn build(value: Value) -> Result<DatedStore, String> {
    DatedStore::build(serde_json::from_value(value).unwrap())
}

#[test]
fn early_settlement_rights_preserve_earned_cash_and_unearned_tail() {
    let store = build(input(0, 60)).unwrap();
    let asof = store.as_of_ordinal();
    let before = store
        .rights_at("base", 0, asof, Boundary::BeforeEvents)
        .unwrap();
    let after = store
        .rights_at("base", 0, asof, Boundary::AfterEvents)
        .unwrap();
    assert_eq!(before.signed_interest_right, 0.);
    assert_eq!(after.signed_interest_right, -6.);
    assert_eq!(
        store
            .rights_at("base", 0, asof + 30, Boundary::AfterEvents)
            .unwrap()
            .signed_interest_right,
        -3.
    );
    assert_eq!(store.pending_accrual_events("base").unwrap().count(), 1);
    assert_eq!(store.settlements_on("base", 0).unwrap().count(), 1);
}
#[test]
fn pre_asof_accrual_and_late_cash_have_separate_opening_anchor() {
    let mut value = input(20, 10);
    let asof = value["as_of_ordinal"].as_i64().unwrap();
    value["sources"][0]["schedule"]["events"][0]["accrual_period"]["start_ordinal"] =
        json!(asof - 10);
    value["accrual_plans"][0]["segments"][0]["start_ordinal"] = json!(asof - 10);
    let store = build(value).unwrap();
    let right = store
        .rights_at("base", 0, asof as i32, Boundary::BeforeEvents)
        .unwrap();
    assert_eq!(right.earned_interest_to_date, 3.);
    assert_eq!(right.cash_interest_settled, 0.);
    assert_eq!(store.opening_accrual_events("base").unwrap().count(), 1);
}
#[test]
fn irregular_supplied_recognition_segments_telescope_independently() {
    let mut value = input(30, 30);
    let asof = value["as_of_ordinal"].as_i64().unwrap();
    value["accrual_plans"][0]["segments"] = json!([
        {"start_ordinal":asof,"end_ordinal":asof+2,"accrued_interest":2.,"book_amortization":0.},
        {"start_ordinal":asof+2,"end_ordinal":asof+30,"accrued_interest":4.,"book_amortization":0.}]);
    let store = build(value).unwrap();
    assert_eq!(
        store
            .accrual_delta("base", 0, asof as i32, asof as i32 + 2)
            .unwrap()
            .accrued_interest,
        2.
    );
    let total: f64 = (0..30)
        .map(|d| {
            store
                .accrual_delta("base", 0, asof as i32 + d, asof as i32 + d + 1)
                .unwrap()
                .accrued_interest
        })
        .sum();
    assert!((total - 6.).abs() < 1e-12);
}
#[test]
fn delayed_payment_full_tail_and_payment_direction_are_preserved() {
    let value = common::request(60, 30, 6., 100., true)["dated"].clone();
    let store = build(value).unwrap();
    assert_eq!(store.tail_events("base").unwrap().count(), 1);
    assert_eq!(
        store
            .rights_at("base", 0, store.as_of_ordinal() + 30, Boundary::AfterEvents)
            .unwrap()
            .signed_interest_right,
        -6.
    );
    assert!(store.settlements_on("base", 31).is_err());
}
#[test]
fn stable_same_day_identities_and_provenance_are_interned() {
    let mut value = input(0, 30);
    let mut second = value["sources"][0]["schedule"]["events"][0].clone();
    second["event_id"] = json!("second");
    value["sources"][0]["schedule"]["events"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let mut plan = value["accrual_plans"][0].clone();
    plan["event_id"] = json!("second");
    value["accrual_plans"].as_array_mut().unwrap().push(plan);
    let store = build(value).unwrap();
    let events: Vec<_> = store
        .settlements_on("base", 0)
        .unwrap()
        .map(|e| e.event_id.as_ref())
        .collect();
    assert_eq!(events, vec!["coupon-1", "second"]);
    assert_eq!(store.provenance_count(), 1);
    assert!(store.retained_capacity_bytes() < MAX_STORE_BYTES);
}
#[test]
fn each_run_requires_explicit_complete_or_no_event_coverage() {
    let mut value = input(30, 30);
    value["sources"][0]["coverage"] = json!([]);
    assert!(build(value).is_err());
    let mut value = input(30, 30);
    value["sources"][0]["coverage"][0]["status"] = json!({"kind":"no_events","rationale":"empty"});
    assert!(build(value).is_err());
    let mut value = input(30, 30);
    value["sources"][0]["schedule"]["events"] = json!([]);
    value["accrual_plans"] = json!([]);
    value["sources"][0]["coverage"][0]["status"] =
        json!({"kind":"no_events","rationale":"explicit open-ended unchanged principal"});
    assert!(build(value).is_ok());
}
#[test]
fn overlap_or_missing_named_run_and_container_identity_fail_closed() {
    for choice in 0..4 {
        let mut value = input(30, 30);
        match choice {
            0 => value["sources"][0]["schedule"]["scenarios"] = json!(["base"]),
            1 => value["owners"][0]["source_revision"] = json!("other"),
            2 => {
                let row = value["sources"][0]["coverage"][0].clone();
                value["sources"][0]["coverage"]
                    .as_array_mut()
                    .unwrap()
                    .push(row);
            }
            _ => {
                let row = value["sources"][0]["schedule"]["events"][0].clone();
                value["sources"][0]["schedule"]["events"]
                    .as_array_mut()
                    .unwrap()
                    .push(row);
            }
        }
        assert!(build(value).is_err(), "case {choice}");
    }
}
#[test]
fn plans_must_explicitly_partition_and_reconcile_whole_period() {
    for choice in 0..5 {
        let mut value = input(30, 30);
        let asof = value["as_of_ordinal"].as_i64().unwrap();
        match choice {
            0 => value["accrual_plans"] = json!([]),
            1 => value["accrual_plans"][0]["segments"] = json!([]),
            2 => value["accrual_plans"][0]["segments"][0]["end_ordinal"] = json!(asof + 29),
            3 => value["accrual_plans"][0]["segments"][0]["start_ordinal"] = json!(asof + 1),
            _ => value["accrual_plans"][0]["segments"][0]["accrued_interest"] = json!(5.),
        }
        assert!(build(value).is_err(), "case {choice}");
    }
}
#[test]
fn measure_and_survival_overlay_are_never_reinterpreted_for_gl() {
    for choice in 0..3 {
        let mut value = input(30, 30);
        match choice {
            0 => value["sources"][0]["schedule"]["events"][0]["measure"] = json!("pricing"),
            1 => value["sources"][0]["schedule"]["events"][0]["measure"] = json!("physical"),
            _ => {
                value["sources"][0]["schedule"]["events"][0]["adjustment"] = json!({"kind":"proportional_surviving_principal","surviving_fraction":0.8,"rationale":"already reduced"})
            }
        }
        assert!(build(value).is_err(), "case {choice}");
    }
    let mut value = input(30, 30);
    value["owners"][0]["ownership"] = json!("fully_scenario_generated");
    value["sources"][0]["schedule"]["events"][0]["measure"] = json!("physical");
    value["sources"][0]["schedule"]["events"][0]["provenance"] = json!({"kind":"modeled","model_id":"scenario-product","model_version":"v1","input_revision":"r1"});
    assert!(build(value).is_ok());
}
#[test]
fn retained_request_domains_and_unknown_schema_fields_fail() {
    for choice in 0..4 {
        let mut value = input(30, 30);
        match choice {
            0 => value["reporting_horizon_days"] = json!(1081),
            1 => value["owners"][0]["position_id"] = json!("x".repeat(257)),
            2 => value["sources"][0]["schedule"]["horizon_days"] = json!(29),
            _ => value["runs"][1]["scenario_id"] = json!("baseline"),
        }
        assert!(build(value).is_err(), "case {choice}");
    }
    let mut value = input(30, 30);
    value["hidden_fallback"] = json!(true);
    assert!(serde_json::from_value::<DatedStoreRequest>(value).is_err());
}
