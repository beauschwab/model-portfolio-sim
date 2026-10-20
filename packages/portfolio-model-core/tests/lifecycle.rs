//! Stage A hand fixtures and refusals for liquidity-lifecycle-1 (issue #43).
//!
//! Worked unwind (docs/models/rust-liquidity-lifecycle-plan.md): opening cash 0, bond 100
//! pledged to a 95 repo, swap liability 4, posted VM 2 and IM 3, equity 6. A 95 bridge funds
//! the repo repayment before the bond can be released and sold; the swap is terminated for 4
//! and its VM and IM come back on later days. Final cash equals equity, 6.
use portfolio_model_core::lifecycle::{self, *};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle/unwind-bridge.json")).unwrap()
}
fn admit(v: &Value) -> Result<Admitted, Refusal> {
    lifecycle::admit(&serde_json::to_vec(v).unwrap())
}
fn refusal(v: &Value) -> &'static str {
    admit(v).err().map(|r| r.code).unwrap_or("ADMITTED")
}
fn path(walk: &Walk) -> Vec<f64> {
    walk.steps.iter().map(|s| s.cash_after).collect()
}
fn obligation<'a>(v: &'a mut Value, id: &str) -> &'a mut Value {
    v["obligations"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["obligation_id"] == id)
        .unwrap()
}
fn remove_obligation(v: &mut Value, id: &str) {
    v["obligations"]
        .as_array_mut()
        .unwrap()
        .retain(|o| o["obligation_id"] != id);
    for o in v["obligations"].as_array_mut().unwrap() {
        if let Some(d) = o.get_mut("depends_on").and_then(Value::as_array_mut) {
            d.retain(|x| x != id);
        }
    }
}

#[test]
fn worked_unwind_with_a_bridge_reaches_cash_six() {
    let walk = lifecycle::walk(&admit(&raw()).unwrap(), "bank").unwrap();
    assert_eq!(path(&walk), vec![95., 0., 0., 0., 100., 5., 1., 3., 6.]);
    assert!(walk.steps.iter().all(|s| s.status == StepStatus::Settled));
    assert_eq!((walk.closing_cash, walk.minimum_cash), (6., 0.));
}

#[test]
fn without_the_bridge_the_dependency_chain_is_infeasible() {
    let mut v = raw();
    remove_obligation(&mut v, "bridge-draw");
    remove_obligation(&mut v, "bridge-repayment");
    let refused = lifecycle::walk(&admit(&v).unwrap(), "bank").unwrap_err();
    assert_eq!(refused.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
    assert!(refused.detail.contains("repo-repayment"));
}

#[test]
fn applying_vm_against_consideration_reaches_the_same_cash_once() {
    let mut v = raw();
    v["margin_agreements"][0]["vm_at_termination"] = json!("apply_against_consideration");
    obligation(&mut v, "swap-termination")["cash"] = json!(-2.0);
    // Returning the same VM as well would credit it twice.
    assert_eq!(refusal(&v), DUPLICATE_COLLATERAL_CREDIT);
    remove_obligation(&mut v, "vm-return");
    let walk = lifecycle::walk(&admit(&v).unwrap(), "bank").unwrap();
    assert_eq!(path(&walk), vec![95., 0., 0., 0., 100., 5., 3., 6.]);
}

#[test]
fn pledged_inventory_cannot_be_delivered_before_release() {
    let mut v = raw();
    remove_obligation(&mut v, "bond-release");
    let refused = lifecycle::walk(&admit(&v).unwrap(), "bank").unwrap_err();
    assert_eq!(refused.code, INVENTORY_UNAVAILABLE);
}

#[test]
fn a_failed_leg_blocks_its_dependents_and_keeps_expected_cash_out() {
    let mut v = raw();
    obligation(&mut v, "bond-delivery")["state"] = json!("failed");
    obligation(&mut v, "swap-termination")["depends_on"] = json!(["bond-payment"]);
    // The sale proceeds never arrive: nothing that depends on them settles, and the expected
    // receipt never becomes usable cash. Gross claims survive outside this walk.
    let walk = lifecycle::walk(&admit(&v).unwrap(), "bank").unwrap();
    let status: Vec<_> = walk
        .steps
        .iter()
        .map(|s| (s.obligation_id.as_str(), s.status))
        .collect();
    assert_eq!(status[3], ("bond-delivery", StepStatus::Failed));
    for blocked in &status[4..] {
        assert_eq!(blocked.1, StepStatus::Blocked, "{}", blocked.0);
    }
    assert_eq!((walk.closing_cash, walk.minimum_cash), (0., 0.));
}

#[test]
fn a_failed_leg_with_insufficient_cash_is_refused_rather_than_netted() {
    let mut v = raw();
    obligation(&mut v, "bond-payment")["state"] = json!("failed");
    obligation(&mut v, "swap-termination")["depends_on"] = json!([]);
    // Bridge repayment is blocked, but the 4 termination payment still needs cash: 0 - 4 < 0.
    let refused = lifecycle::walk(&admit(&v).unwrap(), "bank").unwrap_err();
    assert_eq!(refused.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
}

#[test]
fn dependency_cycles_and_order_violations_are_refused() {
    let mut v = raw();
    obligation(&mut v, "bridge-draw")["depends_on"] = json!(["bond-release"]);
    assert_eq!(refusal(&v), SETTLEMENT_ORDER);
    let mut v = raw();
    obligation(&mut v, "bond-release")["due_ordinal"] = json!(739252);
    obligation(&mut v, "bond-release")["depends_on"] = json!([]);
    obligation(&mut v, "repo-repayment")["depends_on"] = json!(["bridge-draw", "bond-release"]);
    // Releasing before repaying is ordered earlier, so this is admitted; the reverse is not.
    assert_eq!(refusal(&v), "ADMITTED");
    let mut v = raw();
    v["actions"] = json!([
        {"action_id": "a", "kind": "rebalance", "trade_ordinal": 739253, "effective_ordinal": 739253, "settlement_ordinal": 739253,
         "lots": [{"lot_id": "swap-lot", "quantity": 1.0}], "policy": {"id": "p", "revision": "r1"}, "accounting_treatment": "economic", "depends_on": ["b"]},
        {"action_id": "b", "kind": "rebalance", "trade_ordinal": 739253, "effective_ordinal": 739253, "settlement_ordinal": 739253,
         "lots": [{"lot_id": "swap-lot", "quantity": 1.0}], "policy": {"id": "p", "revision": "r1"}, "accounting_treatment": "economic", "depends_on": ["a"]}
    ]);
    for o in v["obligations"].as_array_mut().unwrap() {
        o.as_object_mut().unwrap().remove("action_id");
    }
    assert_eq!(refusal(&v), SETTLEMENT_DEPENDENCY_CYCLE);
}

#[test]
fn shared_resources_cannot_be_overallocated() {
    let mut v = raw();
    v["hedge_allocations"].as_array_mut().unwrap().push(json!({"allocation_id": "second", "exposure_lot_id": "bond-lot",
        "hedge_lot_id": "swap-lot", "amount": 1.0, "risk_basis": "interest_rate", "effective_start_ordinal": 739100,
        "review": {"id": "r", "revision": "r1"}, "unwind_treatment": "retain"}));
    assert_eq!(refusal(&v), OVERALLOCATION);
    // A later, non-overlapping interval reuses the same capacity.
    v["hedge_allocations"][0]["effective_end_ordinal"] = json!(739100);
    assert_eq!(refusal(&v), "ADMITTED");
    let mut v = raw();
    v["funding_allocations"][0]["amount"] = json!(95.5);
    assert_eq!(refusal(&v), OVERALLOCATION);
    let mut v = raw();
    v["lots"][0]["outside_encumbrance"] = json!(1.0);
    assert_eq!(refusal(&v), OVERALLOCATION);
    let mut v = raw();
    v["actions"][0]["lots"][0]["quantity"] = json!(100.5);
    assert_eq!(refusal(&v), MALFORMED_INPUT);
    let mut v = raw();
    v["facilities"][0]["drawn"] = json!(96.0);
    assert_eq!(refusal(&v), OVERALLOCATION);
    let mut v = raw();
    obligation(&mut v, "bridge-draw")["cash"] = json!(96.0);
    obligation(&mut v, "bridge-repayment")["cash"] = json!(-96.0);
    assert_eq!(
        lifecycle::walk(&admit(&v).unwrap(), "bank")
            .unwrap_err()
            .code,
        INSUFFICIENT_SETTLEMENT_LIQUIDITY
    );
}

#[test]
fn every_event_has_one_exclusive_owner_and_identities_are_unique() {
    let mut v = raw();
    obligation(&mut v, "vm-return")["source_event"] = json!("swap:termination:1");
    assert_eq!(refusal(&v), DUPLICATE_IDENTITY);
    let mut v = raw();
    v["lots"][2]["lot_id"] = json!("bond-lot");
    assert_eq!(refusal(&v), DUPLICATE_IDENTITY);
    let mut v = raw();
    v["margin_agreements"].as_array_mut().unwrap().push(json!({"agreement_id": "second-csa", "counterparty": "dealer",
        "legal_review": {"id": "r", "revision": "r1"}, "settlement_lag_days": 1, "initial_margin": "none",
        "segregation": "segregated", "vm_at_termination": "return_separately", "calendar": {"id": "c", "revision": "r1"},
        "lot_ids": ["swap-lot"]}));
    assert_eq!(refusal(&v), DUPLICATE_IDENTITY);
}

#[test]
fn economic_links_never_confer_formal_treatment() {
    let mut v = raw();
    v["actions"][0]["accounting_treatment"] = json!("formal");
    assert_eq!(refusal(&v), FORMAL_OWNER_REQUIRED);
    v["formal_relationships"] = json!([{"relationship_id": "fvh-1", "revision": "r1",
        "accounting_source": {"source_id": "hedge-accounting-source", "revision": "r1",
            "sha256": "4444444444444444444444444444444444444444444444444444444444444444", "measure": "physical"},
        "adoption_policy": {"id": "asc815", "revision": "r1"}, "hedging_lot_ids": ["swap-lot"], "hedged_lot_ids": ["bond-lot"]}]);
    v["hedge_allocations"][0]["formal_relationship_id"] = json!("fvh-1");
    assert_eq!(refusal(&v), "ADMITTED");
    // The accounting source must be the listed immutable source, byte-for-byte in identity.
    v["formal_relationships"][0]["accounting_source"]["sha256"] = json!("5".repeat(64));
    assert_eq!(refusal(&v), LIFECYCLE_MAPPING_REQUIRED);
    let mut v = raw();
    v["hedge_allocations"][0]["formal_relationship_id"] = json!("missing");
    assert_eq!(refusal(&v), LIFECYCLE_MAPPING_REQUIRED);
}

#[test]
fn strict_input_refuses_duplicates_unknown_fields_and_legacy_labels() {
    let text = include_str!("fixtures/lifecycle/unwind-bridge.json");
    let duplicated = text.replacen(
        "\"entity\": \"synthetic-bank\",",
        "\"entity\": \"synthetic-bank\", \"entity\": \"other\",",
        1,
    );
    assert_eq!(
        lifecycle::admit(duplicated.as_bytes()).unwrap_err().code,
        MALFORMED_INPUT
    );
    let mut v = raw();
    v["lots"][0]["hedge_label"] = json!("fvh");
    assert_eq!(refusal(&v), MALFORMED_INPUT);
    let mut v = raw();
    v["lots"][0]["accounting_category"] = json!("fvh");
    assert_eq!(refusal(&v), MALFORMED_INPUT);
    let mut v = raw();
    v["schema"] = json!("liquidity-lifecycle-2");
    assert_eq!(refusal(&v), UNSUPPORTED_POLICY);
    let mut v = raw();
    v["lots"][1]["accounting_category"] = json!("trading");
    assert_eq!(refusal(&v), UNSUPPORTED_POLICY);
    let mut v = raw();
    v["sources"][0]["measure"] = json!("pricing");
    assert_eq!(refusal(&v), UNSUPPORTED_POLICY);
    let mut v = raw();
    v["lots"][0]["quantity"] = json!(-1.0);
    assert_eq!(refusal(&v), MALFORMED_INPUT);
    let mut v = raw();
    obligation(&mut v, "bond-payment")["cash"] = json!(-100.0);
    assert_eq!(refusal(&v), MALFORMED_INPUT);
}

#[test]
fn admission_is_bounded() {
    let mut v = raw();
    let lot = v["lots"][0].clone();
    v["lots"] = Value::Array(
        (0..=MAX_RECORDS)
            .map(|i| {
                let mut l = lot.clone();
                l["lot_id"] = json!(format!("lot-{i}"));
                l
            })
            .collect(),
    );
    assert_eq!(refusal(&v), BOUND_EXCEEDED);
    let big = vec![b' '; MAX_RAW_BYTES + 1];
    assert_eq!(lifecycle::admit(&big).unwrap_err().code, BOUND_EXCEEDED);
}
