//! Saved-book mapping (stage F, issue #48). A `liquidity-book-1` document holds one saved book:
//! the dated source, accounts, lots, hedge and funding allocations, pledges, margin agreements,
//! facilities, raw hedge contracts and scenario horizons. A `liquidity-plan-1` document holds one
//! acquisition or unwind plan: actions, one block of explicit instructions, and the explicit hedge
//! and financing treatment for every affected allocation.
//!
//! Composition is structural only: it binds the saved book revision, checks that every reference
//! resolves and that the plan states everything the native run needs, and assembles the
//! `liquidity-lifecycle-1` request. It computes no amounts. Every missing mapping is reported at
//! once with the record it concerns and the edit that resolves it. Financial admission (liquidity,
//! inventory, margin, formal accounting ownership) remains the native producer's.
use crate::Refusal;
use portfolio_verification_native::{raw, MAX_REQUEST_BYTES};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const BOOK_SCHEMA: &str = "liquidity-book-1";
pub const PLAN_SCHEMA: &str = "liquidity-plan-1";
pub const MAPPING_REQUIRED: &str = "LIFECYCLE_MAPPING_REQUIRED";
/// The plan's worked unwind as a saved book and an unwind plan (served as a reviewed example).
pub const EXAMPLE_BOOK: &str = include_str!("../tests/fixtures/lifecycle-book.json");
pub const EXAMPLE_PLAN: &str = include_str!("../tests/fixtures/lifecycle-plan.json");

const BOOK_FIELDS: [&str; 5] = ["schema", "dated", "plan", "hedge_contracts", "scenarios"];
/// Plan records a saved book supplies; actions and revision bindings come from composition.
/// The first eight are required; the rest default to empty, as in the native request.
const BOOK_PLAN_FIELDS: [&str; 15] = [
    "entity",
    "currency",
    "monetary_unit",
    "as_of_ordinal",
    "settlement_policy",
    "sources",
    "accounts",
    "lots",
    "hedge_allocations",
    "funding_allocations",
    "pledges",
    "margin_agreements",
    "facilities",
    "formal_relationships",
    "obligations",
];
const PLAN_FIELDS: [&str; 5] = [
    "schema",
    "actions",
    "instructions",
    "hedge_treatments",
    "funding_instructions",
];
pub const INSTRUCTION_KINDS: [&str; 9] = [
    "purchases",
    "financings",
    "facility_draws",
    "repayments",
    "sales",
    "hedge_entries",
    "terminations",
    "facility_repayments",
    "initial_margin",
];
const HEDGE_TREATMENTS: [&str; 4] = ["terminate", "retain", "offset", "rebalance"];
const FUNDING_INSTRUCTIONS: [&str; 3] = ["repay", "retain", "replace"];

/// One missing or inconsistent mapping, and the edit that resolves it.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    pub record: String,
    pub detail: String,
    pub fix: String,
}
impl Requirement {
    pub fn to_json(&self) -> Value {
        json!({"record": self.record, "detail": self.detail, "fix": self.fix})
    }
}

/// The composed request and its provenance.
#[derive(Debug, Clone)]
pub struct Composed {
    pub bytes: Vec<u8>,
    pub request: Value,
}

pub struct Binding<'a> {
    pub workspace: &'a str,
    pub revision: i64,
    pub book_name: &'a str,
    pub book_sha256: &'a str,
}

fn malformed(detail: impl Into<String>) -> Refusal {
    Refusal::new("MALFORMED_INPUT", detail)
}

fn exact(v: &Value, allowed: &[&str], required: &[&str], what: &str) -> Result<(), Refusal> {
    let obj = v
        .as_object()
        .ok_or_else(|| malformed(format!("{what} must be an object")))?;
    if let Some(k) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(malformed(format!("{what} has unknown field `{k}`")));
    }
    if let Some(k) = required.iter().find(|k| !obj.contains_key(**k)) {
        return Err(malformed(format!("{what} lacks `{k}`")));
    }
    Ok(())
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn parse_book(bytes: &[u8]) -> Result<Value, Refusal> {
    let book = raw::parse(bytes, MAX_REQUEST_BYTES).map_err(malformed)?;
    if book.get("schema").and_then(Value::as_str) != Some(BOOK_SCHEMA) {
        return Err(Refusal::new(
            "UNSUPPORTED_SCHEMA",
            format!("a saved book document has schema `{BOOK_SCHEMA}`"),
        ));
    }
    exact(&book, &BOOK_FIELDS, &BOOK_FIELDS, "saved book")?;
    exact(
        &book["plan"],
        &BOOK_PLAN_FIELDS,
        &BOOK_PLAN_FIELDS[..8],
        "saved book plan records",
    )?;
    for s in array(&book, "scenarios") {
        exact(
            s,
            &["scenario_id", "horizon_end_ordinal"],
            &["scenario_id", "horizon_end_ordinal"],
            "saved book scenario",
        )?;
    }
    if array(&book, "scenarios").is_empty() {
        return Err(malformed("a saved book declares at least one scenario"));
    }
    Ok(book)
}

pub fn parse_plan(bytes: &[u8]) -> Result<Value, Refusal> {
    let plan = raw::parse(bytes, MAX_REQUEST_BYTES).map_err(malformed)?;
    if plan.get("schema").and_then(Value::as_str) != Some(PLAN_SCHEMA) {
        return Err(Refusal::new(
            "UNSUPPORTED_SCHEMA",
            format!("a lifecycle plan document has schema `{PLAN_SCHEMA}`"),
        ));
    }
    exact(
        &plan,
        &PLAN_FIELDS,
        &["schema", "actions", "instructions"],
        "lifecycle plan",
    )?;
    exact(
        &plan["instructions"],
        &INSTRUCTION_KINDS,
        &[],
        "plan instructions",
    )?;
    for kind in INSTRUCTION_KINDS {
        if let Some(v) = plan["instructions"].get(kind) {
            if !v.is_array() {
                return Err(malformed(format!("instructions `{kind}` must be a list")));
            }
        }
    }
    for (key, allowed) in [
        ("hedge_treatments", &HEDGE_TREATMENTS[..]),
        ("funding_instructions", &FUNDING_INSTRUCTIONS[..]),
    ] {
        if let Some(map) = plan.get(key) {
            let map = map
                .as_object()
                .ok_or_else(|| malformed(format!("`{key}` maps allocation ids to a treatment")))?;
            for (id, t) in map {
                if !t.as_str().is_some_and(|t| allowed.contains(&t)) {
                    return Err(malformed(format!(
                        "`{key}` for `{id}` must be one of {}",
                        allowed.join(", ")
                    )));
                }
            }
        }
    }
    Ok(plan)
}

/// Every reference and explicit treatment the native run needs, checked against the book.
pub fn requirements(book: &Value, plan: &Value) -> Vec<Requirement> {
    let records = &book["plan"];
    let mut out = Vec::new();
    let mut need = |record: String, detail: String, fix: String| {
        out.push(Requirement {
            record,
            detail,
            fix,
        })
    };
    let lots: BTreeMap<&str, &Value> = array(records, "lots")
        .iter()
        .map(|l| (text(l, "lot_id"), l))
        .collect();
    let ids = |key: &str, field: &str| -> BTreeSet<&str> {
        array(records, key).iter().map(|v| text(v, field)).collect()
    };
    let pledges_by_id: BTreeMap<&str, &Value> = array(records, "pledges")
        .iter()
        .map(|p| (text(p, "pledge_id"), p))
        .collect();
    let agreements = ids("margin_agreements", "agreement_id");
    let facilities = ids("facilities", "facility_id");
    let ins = &plan["instructions"];
    let actions = array(plan, "actions");
    if actions.is_empty() {
        need(
            "plan".into(),
            "the plan has no actions".into(),
            "add an acquisition or unwind action naming its lots and quantities".into(),
        );
    }
    // Lots each action touches, with the quantity (full or partial).
    let mut touched: BTreeMap<String, f64> = BTreeMap::new();
    for a in actions {
        let action = text(a, "action_id");
        for l in array(a, "lots") {
            let id = text(l, "lot_id");
            let q = l
                .get("quantity")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            match lots.get(id) {
                None => need(
                    format!("action {action}"),
                    format!("lot `{id}` is not in the saved book"),
                    "choose a lot from the saved book or add it to the book first".into(),
                ),
                Some(lot) => {
                    let held = lot.get("quantity").and_then(Value::as_f64).unwrap_or(0.);
                    let acquisition = text(a, "kind") == "acquisition";
                    // A NaN quantity is refused as well: it is neither positive nor finite.
                    if q.is_nan() || q <= 0. || (!acquisition && q > held) {
                        need(
                            format!("action {action} lot {id}"),
                            format!("quantity {q} is outside (0, {held}] for an exit of this lot"),
                            "use the full lot quantity or a smaller partial quantity".into(),
                        );
                    }
                    *touched.entry(id.to_string()).or_default() += q;
                }
            }
        }
        // Formal treatment is not a mapping gap: the native owner refuses formal treatment it
        // cannot account for, and composition never substitutes the economic treatment.
    }
    for kind in INSTRUCTION_KINDS {
        for i in array(ins, kind) {
            let id = text(i, "instruction_id");
            let label = if id.is_empty() {
                format!("{kind} instruction")
            } else {
                format!("instruction {id}")
            };
            for key in ["lot_id"] {
                if let Some(lot) = i.get(key).and_then(Value::as_str) {
                    if !lots.contains_key(lot) {
                        need(
                            label.clone(),
                            format!("lot `{lot}` is not in the saved book"),
                            "name a saved lot".into(),
                        );
                    }
                }
            }
            if let Some(f) = i.get("facility_id").and_then(Value::as_str) {
                if !facilities.contains(f) {
                    need(
                        label.clone(),
                        format!("facility `{f}` is not in the saved book"),
                        "name a saved facility with an explicit access policy".into(),
                    );
                }
            }
            if let Some(g) = i.get("agreement_id").and_then(Value::as_str) {
                if !agreements.contains(g) {
                    need(
                        label.clone(),
                        format!("margin agreement `{g}` is not in the saved book"),
                        "name the reviewed agreement that covers this derivative".into(),
                    );
                }
            }
            for r in array(i, "pledge_releases") {
                let p = text(r, "pledge_id");
                if !pledges_by_id.contains_key(p) {
                    need(
                        label.clone(),
                        format!("pledge `{p}` is not in the saved book"),
                        "release a saved pledge".into(),
                    );
                }
            }
        }
    }
    let exits: BTreeSet<&str> = array(ins, "sales")
        .iter()
        .chain(array(ins, "terminations"))
        .chain(array(ins, "repayments"))
        .map(|i| text(i, "lot_id"))
        .collect();
    let released: BTreeSet<&str> = array(ins, "repayments")
        .iter()
        .flat_map(|r| array(r, "pledge_releases"))
        .map(|r| text(r, "pledge_id"))
        .collect();
    // A pledged security cannot be delivered until a repayment releases it.
    for s in array(ins, "sales") {
        let lot = text(s, "lot_id");
        for (pid, p) in &pledges_by_id {
            if text(p, "lot_id") == lot && !released.contains(pid) {
                need(
                    format!("sale of {lot}"),
                    format!(
                        "lot `{lot}` is pledged under `{pid}` to `{}`",
                        text(p, "agreement_id")
                    ),
                    format!(
                        "add a repayment of `{}` that releases `{pid}`, and make the sale depend on that release",
                        text(p, "agreement_id")
                    ),
                );
            }
        }
    }
    // Explicit hedge and financing treatment for every allocation an action affects.
    let hedge_treatments = plan.get("hedge_treatments").and_then(Value::as_object);
    for h in array(records, "hedge_allocations") {
        let (alloc, exposure, hedge) = (
            text(h, "allocation_id"),
            text(h, "exposure_lot_id"),
            text(h, "hedge_lot_id"),
        );
        if !touched.contains_key(exposure) && !touched.contains_key(hedge) {
            continue;
        }
        let chosen = hedge_treatments
            .and_then(|m| m.get(alloc))
            .and_then(Value::as_str);
        match chosen {
            None => need(
                format!("hedge allocation {alloc}"),
                format!("the plan affects `{exposure}` or its hedge `{hedge}` but states no hedge treatment"),
                format!("set hedge_treatments.{alloc} to terminate, retain, offset or rebalance"),
            ),
            Some("terminate") if !exits.contains(hedge) => need(
                format!("hedge allocation {alloc}"),
                format!("treatment `terminate` has no termination instruction for `{hedge}`"),
                format!("add a termination of `{hedge}` under its margin agreement, or choose retain"),
            ),
            _ => {}
        }
    }
    let funding_instructions = plan.get("funding_instructions").and_then(Value::as_object);
    for f in array(records, "funding_allocations") {
        let (alloc, funding, asset) = (
            text(f, "allocation_id"),
            text(f, "funding_lot_id"),
            text(f, "attributed_lot_id"),
        );
        if !touched.contains_key(asset) {
            continue;
        }
        let chosen = funding_instructions
            .and_then(|m| m.get(alloc))
            .and_then(Value::as_str);
        match chosen {
            None => need(
                format!("funding allocation {alloc}"),
                format!("the plan exits `{asset}`, which `{funding}` finances, but states no financing treatment"),
                format!("set funding_instructions.{alloc} to repay, retain or replace"),
            ),
            Some("repay") if !exits.contains(funding) => need(
                format!("funding allocation {alloc}"),
                format!("treatment `repay` has no repayment instruction for `{funding}`"),
                format!("add a repayment of `{funding}` (and its pledge releases), funded by cash or a permitted facility draw"),
            ),
            _ => {}
        }
    }
    out
}

/// Composes the request bound to the saved book revision, or refuses with every requirement.
pub fn compose(
    book: &Value,
    plan: &Value,
    binding: &Binding,
) -> Result<Composed, (Refusal, Vec<Requirement>)> {
    let needs = requirements(book, plan);
    if !needs.is_empty() {
        return Err((
            Refusal::new(
                MAPPING_REQUIRED,
                format!(
                    "{} mapping requirement(s) before this plan can run",
                    needs.len()
                ),
            ),
            needs,
        ));
    }
    let mut records: Map<String, Value> = book["plan"].as_object().cloned().unwrap_or_default();
    // The book's allocations carry the plan's explicit treatments.
    for (key, field, chosen) in [
        (
            "hedge_allocations",
            "unwind_treatment",
            plan.get("hedge_treatments"),
        ),
        (
            "funding_allocations",
            "instruction",
            plan.get("funding_instructions"),
        ),
    ] {
        if let (Some(list), Some(Value::Object(map))) = (records.get_mut(key), chosen) {
            for a in list.as_array_mut().into_iter().flatten() {
                if let Some(t) = map.get(text(a, "allocation_id")) {
                    a[field] = t.clone();
                }
            }
        }
    }
    records.insert("schema".into(), json!(crate::LIFECYCLE_SCHEMA));
    records.insert("model_version".into(), json!(crate::LIFECYCLE_SCHEMA));
    records.insert(
        "workspace_revision".into(),
        json!({"id": binding.workspace, "revision": binding.revision.to_string()}),
    );
    records.insert(
        "saved_book_revision".into(),
        json!({"id": binding.book_name, "revision": format!("{}:{}", binding.revision, binding.book_sha256)}),
    );
    records.insert("actions".into(), plan["actions"].clone());
    let acquisitions: Vec<Value> = array(book, "scenarios")
        .iter()
        .map(|s| {
            let scenario = text(s, "scenario_id");
            let mut block = Map::new();
            block.insert("scenario_id".into(), json!(scenario));
            block.insert(
                "horizon_end_ordinal".into(),
                s["horizon_end_ordinal"].clone(),
            );
            for kind in INSTRUCTION_KINDS {
                if let Some(list) = plan["instructions"].get(kind) {
                    let mut list = list.clone();
                    if kind == "initial_margin" {
                        for im in list.as_array_mut().into_iter().flatten() {
                            im["scenario_id"] = json!(scenario);
                        }
                    }
                    block.insert(kind.into(), list);
                }
            }
            Value::Object(block)
        })
        .collect();
    let request = json!({
        "schema": crate::LIFECYCLE_SCHEMA,
        "dated": book["dated"],
        "plan": Value::Object(records),
        "acquisitions": acquisitions,
        "hedge_contracts": book["hedge_contracts"],
    });
    let bytes = serde_json::to_vec(&request).expect("request serializes");
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err((
            Refusal::new("BOUND_EXCEEDED", "the composed request exceeds 8 MiB"),
            Vec::new(),
        ));
    }
    Ok(Composed { bytes, request })
}
