//! Stage H (issue #50) on the service side: composition of `liquidity-allocation-1` requests from
//! saved documents, the independent check of a published allocation, and coefficient-only
//! evaluation.
//!
//! The native producer prepares coefficients, solves and replays exactly. This module does not
//! trust its merge or its replay: it recomputes the merge of the selected packages from the raw
//! admitted package requests (a separate structural implementation) and requires it to equal the
//! request the producer replayed, then verifies that replay's stream with the independent verifier.
//! Only then is the allocation marked validated. Evaluation adds prepared coefficients; it calls no
//! pricer, replays nothing and never reports a validated plan.
use crate::compose::{self, Binding, Requirement};
use crate::Refusal;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub const SCHEMA: &str = "liquidity-allocation-1";
pub const MAX_PACKAGES: usize = 32;
const KINDS: [&str; 9] = [
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
const OBJECTIVES: [&str; 2] = ["worst_case_external_nii", "worst_case_total_earnings"];

pub struct PackageDoc {
    pub name: String,
    pub group: String,
    pub plan: Value,
    pub sha256: String,
}

fn malformed(detail: impl Into<String>) -> Refusal {
    Refusal::new("MALFORMED_INPUT", detail)
}

/// Body of `POST /native/lifecycle/allocations`: packages, floor, objective and refinements.
pub fn parse_body(body: &[u8]) -> Result<(Vec<(String, String)>, Value), Refusal> {
    let v = portfolio_verification_native::raw::parse(body, 1024 * 1024).map_err(malformed)?;
    let o = v
        .as_object()
        .ok_or_else(|| malformed("an allocation body is an object"))?;
    for k in o.keys() {
        if ![
            "packages",
            "minimum_usable_cash",
            "objective",
            "max_refinements",
        ]
        .contains(&k.as_str())
        {
            return Err(malformed(format!(
                "allocation field `{k}` is not supported"
            )));
        }
    }
    let packages = o
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("`packages` lists saved plans and their groups"))?;
    if packages.is_empty() || packages.len() > MAX_PACKAGES {
        return Err(Refusal::new(
            "BOUND_EXCEEDED",
            format!("an allocation names 1 to {MAX_PACKAGES} packages"),
        ));
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for p in packages {
        let plan = p
            .get("plan")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed("each package names a saved plan"))?;
        let group = p.get("group").and_then(Value::as_str).unwrap_or(plan);
        if !seen.insert(plan.to_string()) {
            return Err(Refusal::new(
                "DUPLICATE_IDENTITY",
                format!("plan `{plan}` is listed twice"),
            ));
        }
        out.push((plan.to_string(), group.to_string()));
    }
    let floor = o
        .get("minimum_usable_cash")
        .and_then(Value::as_f64)
        .unwrap_or(0.);
    let objective = o
        .get("objective")
        .and_then(Value::as_str)
        .unwrap_or("worst_case_external_nii");
    if !OBJECTIVES.contains(&objective) {
        return Err(malformed(
            "objective is worst_case_external_nii or worst_case_total_earnings",
        ));
    }
    let refinements = o
        .get("max_refinements")
        .and_then(Value::as_u64)
        .unwrap_or(4);
    if refinements > 8 || !floor.is_finite() {
        return Err(malformed("a finite floor and at most 8 refinements"));
    }
    Ok((
        out,
        json!({"minimum_usable_cash": floor, "objective": objective, "max_refinements": refinements}),
    ))
}

/// The hold baseline (the saved book, no actions, runoff) and one composed request per package.
pub fn compose_request(
    book: &Value,
    binding: &Binding,
    packages: &[PackageDoc],
    constraints: Value,
) -> Result<Vec<u8>, Refusal> {
    let hold = json!({"schema": compose::PLAN_SCHEMA, "actions": [], "instructions": {}, "business_plan": {"mode": "runoff"}});
    let base = compose::compose(book, &hold, binding)
        .map_err(|(r, _)| r)?
        .request;
    let mut needs: Vec<Requirement> = Vec::new();
    let mut composed = Vec::new();
    for p in packages {
        match compose::compose(book, &p.plan, binding) {
            Ok(c) => composed.push(json!({
                "package_id": p.name, "group": p.group,
                "review": {"id": p.name, "revision": format!("{}:{}", binding.revision, p.sha256)},
                "request": c.request,
            })),
            Err((_, list)) => needs.extend(list.into_iter().map(|r| Requirement {
                record: format!("package {}: {}", p.name, r.record),
                ..r
            })),
        }
    }
    if !needs.is_empty() {
        let mut r = Refusal::new(
            compose::MAPPING_REQUIRED,
            format!("{} mapping requirement(s) in the packages", needs.len()),
        );
        r.requirements = needs.iter().map(Requirement::to_json).collect();
        return Err(r);
    }
    let request =
        json!({"schema": SCHEMA, "base": base, "packages": composed, "constraints": constraints});
    Ok(serde_json::to_vec(&request).expect("request"))
}

/// The service's own structural merge of selected package requests.
pub fn merge(base: &Value, selected: &[&Value]) -> Result<Value, String> {
    let Some((head, tail)) = selected.split_first() else {
        return Ok(base.clone());
    };
    let records = |v: &Value| -> Map<String, Value> {
        let mut m = v["plan"].as_object().cloned().unwrap_or_default();
        m.remove("actions");
        m
    };
    let mut merged = (*head).clone();
    for next in tail {
        if records(next) != records(head)
            || next["dated"] != head["dated"]
            || next["hedge_contracts"] != head["hedge_contracts"]
        {
            return Err("selected packages do not share one book".into());
        }
        let more = next["plan"]["actions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(a) = merged["plan"]["actions"].as_array_mut() {
            a.extend(more);
        }
        let blocks = next["acquisitions"].as_array().cloned().unwrap_or_default();
        let target = merged["acquisitions"]
            .as_array_mut()
            .ok_or("acquisitions")?;
        if blocks.len() != target.len() {
            return Err("selected packages declare different scenarios".into());
        }
        for (into, from) in target.iter_mut().zip(blocks) {
            for k in KINDS {
                if let Some(extra) = from.get(k).and_then(Value::as_array) {
                    let slot = into
                        .as_object_mut()
                        .ok_or("block")?
                        .entry(k)
                        .or_insert(json!([]));
                    if let Some(list) = slot.as_array_mut() {
                        list.extend(extra.iter().cloned());
                    }
                }
            }
        }
    }
    Ok(merged)
}

/// Checks the producer's allocation receipt against the admitted request. Returns the lifecycle
/// request the stream must verify against, when the producer reports a passed exact replay.
pub fn check(raw: &Value, receipt: &Value, stream: &[u8]) -> Result<Option<Vec<u8>>, Refusal> {
    let bad = |d: &str| Refusal::new("ALLOCATION_RECEIPT_REFUSED", d.to_string());
    if receipt["schema"] != SCHEMA {
        return Err(bad("the allocation receipt has another schema"));
    }
    let admitted: Vec<&str> = raw["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["package_id"].as_str())
        .collect();
    let prepared: Vec<&str> = receipt["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["package_id"].as_str())
        .collect();
    if admitted != prepared {
        return Err(bad("prepared packages differ from the admitted packages"));
    }
    if receipt["exact_replay_passed"] != true {
        if !stream.is_empty() {
            return Err(bad("an unvalidated allocation carries a replay stream"));
        }
        return Ok(None);
    }
    let names: Vec<&str> = receipt["selected"]
        .as_array()
        .ok_or_else(|| bad("selection missing"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let selected: Vec<&Value> = names
        .iter()
        .map(|n| {
            raw["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["package_id"] == *n)
                .map(|p| &p["request"])
        })
        .collect::<Option<_>>()
        .ok_or_else(|| bad("the selection names an unknown package"))?;
    let ineligible = receipt["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| names.contains(&p["package_id"].as_str().unwrap_or("")) && p["eligible"] != true);
    if ineligible {
        return Err(bad("the selection includes an ineligible package"));
    }
    let merged = merge(&raw["base"], &selected).map_err(|e| bad(&e))?;
    if merged != receipt["combined_request"] {
        return Err(bad(
            "the replayed request differs from the independent merge of the selection",
        ));
    }
    Ok(Some(serde_json::to_vec(&merged).expect("request")))
}

/// Coefficient-only estimate for a selection: baseline plus the sum of prepared deltas.
pub fn evaluate(
    receipt: &Value,
    selection: &[String],
    floor: Option<f64>,
) -> Result<Value, Refusal> {
    let packages = receipt["packages"]
        .as_array()
        .ok_or_else(|| Refusal::new("NOT_FOUND", "no prepared packages"))?;
    let mut chosen = Vec::new();
    let mut groups = BTreeSet::new();
    for id in selection {
        let p = packages
            .iter()
            .find(|p| p["package_id"] == id.as_str())
            .ok_or_else(|| malformed(format!("package `{id}` was not prepared")))?;
        if p["eligible"] != true {
            return Err(Refusal::new(
                "INELIGIBLE_PACKAGE",
                format!(
                    "package `{id}` refused at preparation: {}",
                    p["refusal"].as_str().unwrap_or("")
                ),
            ));
        }
        if !groups.insert(p["group"].as_str().unwrap_or("")) {
            return Err(malformed(format!(
                "at most one package per group (`{}`)",
                p["group"].as_str().unwrap_or("")
            )));
        }
        chosen.push(p);
    }
    let floor = floor.unwrap_or_else(|| receipt["minimum_usable_cash"].as_f64().unwrap_or(0.));
    let objective = receipt["objective"]
        .as_str()
        .unwrap_or("worst_case_external_nii");
    let mut scenarios = Map::new();
    let mut worst = f64::INFINITY;
    let mut within = true;
    for s in receipt["scenarios"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let b = &receipt["base"][s];
        let g = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.);
        let (mut nii, mut trading, mut funding) = (
            g(b, "external_nii"),
            g(b, "trading_pnl"),
            g(b, "funding_cost"),
        );
        let mut usable = receipt["base_daily_minimum"][s].as_f64().unwrap_or(0.);
        for p in &chosen {
            let c = &p["coefficients"][s];
            nii += g(c, "external_nii");
            trading += g(c, "trading_pnl");
            funding += g(c, "funding_cost");
            usable += g(c, "usable_cash_deterioration");
        }
        let value = if objective == "worst_case_total_earnings" {
            nii + trading - funding
        } else {
            nii
        };
        worst = worst.min(value);
        within &= usable >= floor;
        scenarios.insert(s.into(), json!({"external_nii": nii, "trading_pnl": trading, "funding_cost": funding, "minimum_usable_cash_bound": usable}));
    }
    Ok(json!({
        "selection": selection, "objective": objective, "worst_case_objective": worst,
        "minimum_usable_cash": floor, "within_usable_cash_bound": within, "scenarios": scenarios,
        "status": "estimate", "validated": false,
        "disclosure": "Coefficient-only estimate from exact replays of each package alone. Netting, margin, funding policy and inventory are not additive; only an exact replay of the merged packages, independently verified, validates a plan.",
    }))
}
