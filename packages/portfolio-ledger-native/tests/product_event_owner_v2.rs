use portfolio_ledger_native::{
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    product_event_owner::{self as product, NativeView, OwnerSources, PostingRef},
};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/product_event_owner_v2.json")).unwrap()
}
fn compiled(v: &Value) -> Result<product::Compiled, String> {
    let r: DatedLedgerRequest =
        serde_json::from_value(v["dated"].clone()).map_err(|e| e.to_string())?;
    let d = CompiledDatedLedger::compile(r, 512, 8_000_000)?;
    let r: product::Request=serde_json::from_value(json!({"schema":product::SCHEMA,"reporting_end_ordinal":v["reporting_end_ordinal"],"owners":v["product_owners"]})).map_err(|e|e.to_string())?;
    product::Compiled::compile(r, &d)
}
fn default_path(v: &mut Value, collect: bool) {
    for run in v["product_owners"][0]["runs"].as_array_mut().unwrap() {
        let r = &mut run["paths"][0]["request"]["request"];
        r["termination"] = json!({"kind":"source_default"});
        r["review"]["policy"]["accrued"] = json!("transfer_until_reviewed_reversal");
        r["review"]["policy"]["basis"] = json!("transfer_until_reviewed_chargeoff");
        r["contract"]["steps"] = json!([
            {"ordinal":738926,"action":{"kind":"payment","amount":100.}},
            {"ordinal":738936,"action":{"kind":"default"}}
        ]);
        if collect {
            r["interest_recoveries"] = json!([{
                "recovery":{"id":"actual-full-interest","revision":"r1"},
                "claim_id":r["claim_id"],"recognition_ordinal":738946,
                "amount":0.9999999999999999,
                "policy_review":{"id":"actual-collection-review","revision":"r1"},
                "source":{"id":"actual-collection-history","revision":"r1"},
                "excess_cash":"credit_allowance"
            }]);
        }
    }
}

#[test]
fn explicit_card2_source_binds_outer_path_without_invented_raw_metadata() {
    let c = compiled(&raw()).unwrap();
    let view = OwnerSources::Product(&c).native(0);
    assert_eq!(view.raw().kind, "card_v2");
    assert_eq!(view.raw().path_id, None);
    assert_eq!(
        OwnerSources::Product(&c).path(0).1.path_id,
        "card-v2-physical"
    );
    let policy = serde_json::to_value(view.raw().policy).unwrap();
    assert!(policy.get("recovery_review").is_none());
    assert!(policy.get("cost_review").is_none());
    assert_eq!(
        policy["exact_raw_policy"]["principal_basis"],
        "actual_terminal_principal_transfer_to_unpaid_interest-1"
    );
    let source = OwnerSources::Product(&c).emit_source(0);
    assert_eq!(source["native"]["kind"], "card_v2");
    assert_eq!(
        source["native"]["result"]["raw_request"],
        raw()["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]
    );
}

#[test]
fn compact_actions_borrow_original_arrays_and_shared_intents_are_not_postings() {
    let mut v = raw();
    default_path(&mut v, true);
    let c = compiled(&v).unwrap();
    let o = OwnerSources::Product(&c);
    let n = o.native(0);
    let NativeView::CardV2(r) = n else {
        panic!("wrong source")
    };
    assert!(!r.shared_intents.is_empty());
    assert!(n.intents().is_empty());
    let mut count = 0;
    for day in 0..=40 {
        assert!(o.intents(0, day).is_empty());
        for p in o.postings(0, day) {
            let PostingRef::CardV2Action { path, index } = p else {
                panic!("duplicate shared-intent posting")
            };
            assert_eq!(path, 0);
            let a = n.rights_action(index).unwrap();
            let events = n.rights_events(a).unwrap();
            assert_eq!(events.len(), a.events.end - a.events.start);
            assert!(std::ptr::eq(
                events.as_ptr(),
                r.events[a.events.start..a.events.end].as_ptr()
            ));
            assert_eq!(
                n.rights_shared_intents(a).unwrap().len(),
                a.shared_intents.end - a.shared_intents.start
            );
            assert_eq!(
                n.rights_income_steps(a).unwrap().len(),
                a.income_steps.end - a.income_steps.start
            );
            assert_eq!(a.ordinal, 738916 + day as i32);
            count += 1;
        }
    }
    assert_eq!(count, r.actions.len());
    let foreign = r.actions[0].clone();
    assert!(n.rights_events(&foreign).is_none());
    let day = n.rights_state(10).unwrap();
    assert_eq!(day.core.live[0].principal, 0.);
    assert!(day.live_interest_basis < 0.);
    assert!(n.rights_closing().unwrap().complete());
    assert_eq!(
        n.rights_state(99).unwrap().original_eir_income,
        n.rights_closing().unwrap().original_eir_income
    );
}

#[test]
fn incomplete_gross_and_basis_claims_reject_even_zero_participation() {
    let mut v = raw();
    default_path(&mut v, false);
    let error = compiled(&v).err().unwrap();
    assert!(
        error.contains("complete individual gross/basis/commitment"),
        "{error}"
    );
    let mut v = raw();
    default_path(&mut v, true);
    v["product_owners"][0]["allocated_face"] = json!(0.);
    for run in v["product_owners"][0]["runs"].as_array_mut().unwrap() {
        let mut other = run["paths"][0].clone();
        other["path_id"] = json!("zero-hidden-tail");
        other["source_weight"] = json!(0.);
        other["request"]["request"]["interest_recoveries"] = json!([]);
        run["paths"].as_array_mut().unwrap().push(other);
    }
    let error = compiled(&v).err().unwrap();
    assert!(
        error.contains("complete individual gross/basis/commitment"),
        "{error}"
    );
}

#[test]
fn actual_statement_dates_are_indexed_without_stale_replay_after_end() {
    let c = compiled(&raw()).unwrap();
    assert_eq!(
        c.daily_receipts(0, 10)["statement_lineage"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(c.daily_receipts(0, 11)["statement_lineage"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(c.daily_receipts(0, 100)["statement_lineage"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        c.daily_receipts(0, 10)["statement_lineage"][0]["interest_recognized_again"],
        false
    );
    assert_eq!(
        c.daily_receipts(0, 10)["commitment_source"]["funded_contingent_draw"],
        0.
    );
}

#[test]
fn raw_anchors_currency_measure_reporting_and_missing_runs_fail() {
    for field in ["currency", "source_revision", "run_id", "position_id"] {
        let mut v = raw();
        v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]
            ["identity"][field] = json!("wrong");
        assert!(compiled(&v).is_err(), "{field}");
    }
    let mut v = raw();
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]["identity"]
        ["measure"] = json!("pricing");
    assert!(compiled(&v).is_err());
    let mut v = raw();
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["review"]
        ["original_annual_effective_rate"] = json!(0.02);
    assert!(compiled(&v).is_err());
    let mut v = raw();
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["reporting_ordinal"] =
        json!(738925);
    assert!(compiled(&v).is_err());
    let mut v = raw();
    v["product_owners"][0]["runs"].as_array_mut().unwrap().pop();
    assert!(compiled(&v).is_err());
}

#[test]
fn v2_allowance_and_unowned_tail_boundaries_reject_before_projection() {
    let mut v = raw();
    v["product_owners"][0]["initial_allowance_review"] =
        json!({"review":{"id":"unowned-initial-allowance","revision":"r1"},"amount":0.});
    assert!(compiled(&v).is_err());
    let mut v = raw();
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]
        ["horizon_days"] = json!(65);
    assert!(compiled(&v).is_err());
    let mut v = raw();
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["contract"]["steps"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(compiled(&v).is_err());
}

#[test]
fn legacy_variants_preserve_intent_iterator_and_have_no_rights_supplements() {
    let old: Value =
        serde_json::from_str(include_str!("fixtures/product_event_owner.json")).unwrap();
    let c = compiled(&old).unwrap();
    let o = OwnerSources::Product(&c);
    for day in 0..=40 {
        let actual: Vec<_> = o.postings(0, day).collect();
        let expected: Vec<_> = o
            .intents(0, day)
            .iter()
            .map(|&(path, index)| PostingRef::Intent { path, index })
            .collect();
        assert_eq!(actual, expected);
    }
    for i in 0..c.sources.len() {
        let n = o.native(i);
        assert!(n.rights_actions().is_none());
        assert!(n.rights_state(10).is_none());
        assert!(n.raw().path_id.is_some());
        assert!(serde_json::to_value(n.raw().policy).unwrap()["cost_review"].is_object());
    }
}

#[test]
fn full_native_capacity_and_outer_views_are_reserved_unweighted_once() {
    let mut v = raw();
    v["product_owners"][0]["allocated_face"] = json!(50.);
    let c = compiled(&v).unwrap();
    let o = OwnerSources::Product(&c);
    let NativeView::CardV2(r) = o.native(0) else {
        panic!("card v2")
    };
    assert_eq!(r.raw_request.review.principal, 100.);
    assert_eq!(r.closing.cash_received, 101.);
    assert_eq!(o.scale(0).factor().unwrap(), 0.5);
    assert!(c.admission.work > r.admitted_work);
    assert!(c.admission.retained_bytes > r.reserved_bytes);
}
