//! The HTTP contract, defined once. `workbench-native contract openapi` and
//! `contract typescript` generate the checked-in OpenAPI document and TypeScript types from
//! these tables; a test fails when either file is stale. No Python generator is involved.
use serde_json::{json, Map, Value};

pub struct Header {
    pub name: &'static str,
    pub required: bool,
    pub description: &'static str,
}

pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
    pub operation: &'static str,
    pub summary: &'static str,
    pub headers: &'static [Header],
    pub query: &'static [&'static str],
    /// `raw-json`: the body is the raw request bytes, preserved exactly.
    pub body: Option<&'static str>,
    pub status: u16,
    pub response: &'static str,
}

const EXPECTED: Header = Header {
    name: "X-Expected-Revision",
    required: true,
    description:
        "Workspace revision the submission was prepared against; a stale value is refused.",
};

pub const ROUTES: &[Route] = &[
    Route { method: "get", path: "/native/health", operation: "health", summary: "Service, schema and routing state",
        headers: &[], query: &[], body: None, status: 200, response: "Health" },
    Route { method: "get", path: "/native/revisions/head", operation: "revisionHead", summary: "Current workspace revision",
        headers: &[], query: &[], body: None, status: 200, response: "RevisionHead" },
    Route { method: "get", path: "/native/revisions/{revision}", operation: "revision", summary: "One immutable revision row",
        headers: &[], query: &[], body: None, status: 200, response: "Revision" },
    Route { method: "get", path: "/native/revisions/{revision}/snapshot", operation: "snapshot",
        summary: "Verified native snapshot of a revision (native workspaces only)",
        headers: &[], query: &[], body: None, status: 200, response: "Snapshot" },
    Route { method: "put", path: "/native/documents/{name}", operation: "commitDocument",
        summary: "Commit an immutable named lifecycle document as the next revision",
        headers: &[Header { name: "If-Match", required: true, description: "Expected head revision." }],
        query: &[], body: Some("raw-json"), status: 201, response: "RevisionHead" },
    Route { method: "post", path: "/native/lifecycle/runs", operation: "submitLifecycleRun",
        summary: "Admit a durable liquidity-lifecycle-1 run (raw body, or ?document= at the expected revision)",
        headers: &[EXPECTED,
            Header { name: "Idempotency-Key", required: false, description: "Repeated keys return the original job for identical input." },
            Header { name: "X-Deadline-Seconds", required: false, description: "Original deadline from admission, within [1, 3600] (the native execution bound); retries never reset it." }],
        query: &["document", "book", "plan"], body: Some("raw-json"), status: 202, response: "Admission" },
    Route { method: "post", path: "/native/lifecycle/compose", operation: "composeLifecyclePlan",
        summary: "Compose a saved book (?book=) and a plan (?plan= or the body) into a request, or list every mapping requirement",
        headers: &[EXPECTED], query: &["book", "plan"], body: Some("raw-json"), status: 200, response: "Composition" },
    Route { method: "get", path: "/native/lifecycle/example", operation: "lifecycleExample",
        summary: "The worked unwind as a saved book and an unwind plan",
        headers: &[], query: &[], body: None, status: 200, response: "LifecycleExample" },
    Route { method: "get", path: "/native/documents", operation: "listDocuments",
        summary: "Saved documents of a revision (default head) with their schemas",
        headers: &[], query: &["revision"], body: None, status: 200, response: "DocumentList" },
    Route { method: "get", path: "/native/documents/{name}", operation: "document",
        summary: "One saved document, reread with its checksum",
        headers: &[], query: &["revision"], body: None, status: 200, response: "SavedDocument" },
    Route { method: "get", path: "/native/jobs/{id}/receipt", operation: "producerReceipt",
        summary: "The independently verified producer receipt of a publication: events, liquidity steps and accounting facts",
        headers: &[], query: &[], body: None, status: 200, response: "ProducerReceipt" },
    Route { method: "get", path: "/native/jobs", operation: "listJobs", summary: "Native job metadata, newest first",
        headers: &[], query: &["offset", "limit"], body: None, status: 200, response: "JobList" },
    Route { method: "get", path: "/native/jobs/{id}", operation: "job", summary: "A freshly built job view (never authority)",
        headers: &[], query: &[], body: None, status: 200, response: "Job" },
    Route { method: "post", path: "/native/jobs/{id}/cancel", operation: "cancelJob", summary: "Cancel a queued or running native job",
        headers: &[], query: &[], body: None, status: 200, response: "Cancelled" },
    Route { method: "get", path: "/native/jobs/{id}/manifest", operation: "manifest",
        summary: "The completed publication manifest, reread and checksum-verified",
        headers: &[], query: &[], body: None, status: 200, response: "Manifest" },
    Route { method: "get", path: "/native/jobs/{id}/tables/{table}", operation: "tablePage",
        summary: "One bounded page of a published table",
        headers: &[], query: &["offset", "limit"], body: None, status: 200, response: "TablePage" },
];

fn obj(required: &[&str], props: Value) -> Value {
    json!({"type": "object", "required": required, "properties": props})
}

/// Component schemas (a JSON Schema subset the TypeScript generator below understands).
pub fn schemas() -> Value {
    let descriptor = obj(
        &["key", "sha256", "bytes", "format"],
        json!({
        "key": {"type": "string"}, "sha256": {"type": "string"}, "bytes": {"type": "integer"},
        "format": {"type": "string", "enum": ["json", "parquet", "bin"]}}),
    );
    let r = |name: &str| json!({"$ref": format!("#/components/schemas/{name}")});
    let list = |name: &str| json!({"type": "array", "items": {"$ref": format!("#/components/schemas/{name}")}});
    let s = json!({"type": "string"});
    let n = json!({"type": "number"});
    let i = json!({"type": "integer"});
    let b = json!({"type": "boolean"});
    let any_obj = json!({"type": "object", "additionalProperties": true});
    let objects =
        json!({"type": "array", "items": {"type": "object", "additionalProperties": true}});
    let lifecycle = json!({
        "Requirement": obj(&["record", "detail", "fix"], json!({"record": s, "detail": s, "fix": s})),
        "DocumentEntry": obj(&["name", "schema", "bytes", "sha256"], json!({
            "name": s, "schema": {"type": ["string", "null"]}, "bytes": i, "sha256": s})),
        "DocumentList": obj(&["revision", "documents"], json!({"revision": i, "documents": list("DocumentEntry")})),
        "SavedDocument": {"type": "object", "additionalProperties": true},
        "LifecycleExample": obj(&["book", "plan", "policy_plan"], json!({"book": r("SavedBook"), "plan": r("LifecyclePlan"), "policy_plan": r("LifecyclePlan")})),
        "PlanWindow": obj(&["from_ordinal", "to_ordinal"], json!({"from_ordinal": i, "to_ordinal": i})),
        "PlanRenewal": obj(&["max_generations", "tenor_days", "windows"], json!({
            "max_generations": i, "tenor_days": i, "windows": list("PlanWindow")})),
        "PolicyFacility": obj(&["facility_id", "access"], json!({
            "facility_id": s, "access": list("PlanWindow"), "renewal": r("PlanRenewal")})),
        "FundingPolicy": obj(&["policy", "minimum_usable_cash", "facilities"], json!({
            "policy": any_obj, "minimum_usable_cash": n, "facilities": list("PolicyFacility"), "repay_surplus": b})),
        "BusinessPlan": obj(&["mode"], json!({
            "mode": {"type": "string", "enum": ["runoff", "schedule", "policy"]}, "funding_policy": r("FundingPolicy")})),
        "PolicyDecision": obj(&["decision_id", "ordinal", "kind", "facility_id", "generation", "amount", "trigger",
                "usable_cash_before", "usable_cash_after", "need"], json!({
            "decision_id": s, "ordinal": i,
            "kind": {"type": "string", "enum": ["draw", "surplus_repayment", "renewal", "maturity_repayment"]},
            "facility_id": s, "generation": i, "amount": n, "trigger": s,
            "usable_cash_before": n, "usable_cash_after": n, "need": n})),
        "MonthlyReport": obj(&["month", "first_ordinal", "last_ordinal", "events", "failed_events", "blocked_events",
                "policy_decisions", "opening_cash", "cash_in", "cash_out", "net_cash", "closing_cash", "minimum_usable_cash",
                "closing_usable_cash", "closing_facility_drawn", "interest_expense", "fees", "realized", "margin_posted",
                "margin_returned", "closing_posted_margin", "closing_pledged_quantity", "closing_security_quantity"], json!({
            "month": s, "first_ordinal": i, "last_ordinal": i, "events": i, "failed_events": i, "blocked_events": i,
            "policy_decisions": i, "opening_cash": n, "cash_in": n, "cash_out": n, "net_cash": n, "closing_cash": n,
            "minimum_usable_cash": n, "closing_usable_cash": n, "closing_facility_drawn": n, "interest_expense": n,
            "fees": n, "realized": n, "margin_posted": n, "margin_returned": n, "closing_posted_margin": n,
            "closing_pledged_quantity": n, "closing_security_quantity": n})),
        "Composition": obj(&["request", "request_sha256", "bytes", "revision"], json!({
            "request": any_obj, "request_sha256": s, "bytes": i, "revision": i})),
        "BookLot": obj(&["lot_id", "position_id", "account_id", "kind", "accounting_category", "quantity", "original_cost"], json!({
            "lot_id": s, "position_id": s, "source_id": s, "account_id": s,
            "kind": {"type": "string", "enum": ["security", "loan", "derivative", "funding"]},
            "accounting_category": s, "quantity": n, "acquisition_ordinal": i, "original_cost": n,
            "original_effective_yield": n, "original_oas": n, "outside_encumbrance": n})),
        "BookAccount": obj(&["account_id", "cash", "equity"], json!({"account_id": s, "cash": n, "equity": n})),
        "HedgeAllocation": obj(&["allocation_id", "exposure_lot_id", "hedge_lot_id", "amount", "unwind_treatment"], json!({
            "allocation_id": s, "exposure_lot_id": s, "hedge_lot_id": s, "amount": n, "risk_basis": s,
            "unwind_treatment": r("HedgeTreatment"), "formal_relationship_id": {"type": ["string", "null"]}})),
        "FundingAllocation": obj(&["allocation_id", "funding_lot_id", "attributed_lot_id", "amount", "instruction"], json!({
            "allocation_id": s, "funding_lot_id": s, "attributed_lot_id": s, "amount": n,
            "instruction": r("FundingTreatment")})),
        "BookPledge": obj(&["pledge_id", "lot_id", "agreement_id", "holder", "quantity", "haircut"], json!({
            "pledge_id": s, "lot_id": s, "agreement_id": s, "holder": s, "quantity": n, "haircut": n})),
        "BookMarginAgreement": obj(&["agreement_id", "counterparty", "settlement_lag_days", "initial_margin", "segregation", "vm_at_termination", "lot_ids"], json!({
            "agreement_id": s, "counterparty": s, "settlement_lag_days": i, "initial_margin": s,
            "segregation": s, "vm_at_termination": s, "lot_ids": {"type": "array", "items": s},
            "opening_collateral": any_obj})),
        "BookFacility": obj(&["facility_id", "provider", "commitment", "drawn", "expiry_ordinal"], json!({
            "facility_id": s, "provider": s, "commitment": n, "drawn": n, "access_policy": any_obj, "expiry_ordinal": i})),
        "BookRecords": obj(&["entity", "currency", "monetary_unit", "as_of_ordinal", "accounts", "lots"], json!({
            "entity": s, "currency": s, "monetary_unit": s, "as_of_ordinal": i, "settlement_policy": any_obj,
            "sources": objects, "accounts": list("BookAccount"), "lots": list("BookLot"),
            "hedge_allocations": list("HedgeAllocation"), "funding_allocations": list("FundingAllocation"),
            "pledges": list("BookPledge"), "margin_agreements": list("BookMarginAgreement"),
            "facilities": list("BookFacility"), "formal_relationships": objects, "obligations": objects})),
        "ScenarioHorizon": obj(&["scenario_id", "horizon_end_ordinal"], json!({"scenario_id": s, "horizon_end_ordinal": i})),
        "SavedBook": obj(&["schema", "dated", "plan", "hedge_contracts", "scenarios"], json!({
            "schema": {"type": "string", "enum": ["liquidity-book-1"]}, "dated": any_obj, "plan": r("BookRecords"),
            "hedge_contracts": objects, "scenarios": list("ScenarioHorizon")})),
        "HedgeTreatment": {"type": "string", "enum": ["terminate", "retain", "offset", "rebalance"]},
        "FundingTreatment": {"type": "string", "enum": ["repay", "retain", "replace"]},
        "ActionLot": obj(&["lot_id", "quantity"], json!({"lot_id": s, "quantity": n})),
        "LifecycleAction": obj(&["action_id", "kind", "trade_ordinal", "effective_ordinal", "settlement_ordinal", "lots", "policy", "accounting_treatment"], json!({
            "action_id": s,
            "kind": {"type": "string", "enum": ["acquisition", "unwind", "offset", "rebalance", "refinance"]},
            "trade_ordinal": i, "effective_ordinal": i, "settlement_ordinal": i, "lots": list("ActionLot"),
            "policy": any_obj, "accounting_treatment": {"type": "string", "enum": ["economic", "formal"]},
            "depends_on": {"type": "array", "items": s}})),
        "Instructions": obj(&[], json!({
            "purchases": objects, "financings": objects, "facility_draws": objects, "repayments": objects,
            "sales": objects, "hedge_entries": objects, "terminations": objects,
            "facility_repayments": objects, "initial_margin": objects})),
        "LifecyclePlan": obj(&["schema", "actions", "instructions"], json!({
            "schema": {"type": "string", "enum": ["liquidity-plan-1"]}, "actions": list("LifecycleAction"),
            "instructions": r("Instructions"),
            "business_plan": r("BusinessPlan"),
            "hedge_treatments": {"type": "object", "additionalProperties": r("HedgeTreatment")},
            "funding_instructions": {"type": "object", "additionalProperties": r("FundingTreatment")}})),
        "EventStatus": {"type": "string", "enum": ["settled", "failed", "blocked"]},
        "EventReceipt": obj(&["event_id", "ordinal", "phase", "status"], json!({
            "event_id": s, "ordinal": i, "phase": i, "status": r("EventStatus")})),
        "LiquidityStep": obj(&["event_id", "ordinal", "phase", "status", "depends_on", "cash_movement", "cash_after",
                "restricted_cash_after", "usable_cash_after", "facility_drawn_after", "facility_undrawn_after",
                "pledged_quantity_after", "security_quantity_after", "posted_vm_after", "received_vm_after",
                "posted_im_after", "received_im_custody_after", "pending_receivable_after", "pending_payable_after",
                "derivative_assets_after", "derivative_liabilities_after"], json!({
            "event_id": s, "ordinal": i, "phase": i, "status": r("EventStatus"), "depends_on": {"type": "array", "items": s},
            "cash_movement": n, "cash_after": n, "restricted_cash_after": n, "usable_cash_after": n,
            "facility_drawn_after": n, "facility_undrawn_after": n, "pledged_quantity_after": n,
            "security_quantity_after": n, "posted_vm_after": n, "received_vm_after": n, "posted_im_after": n,
            "received_im_custody_after": n, "pending_receivable_after": n, "pending_payable_after": n,
            "derivative_assets_after": n, "derivative_liabilities_after": n})),
        "AccountingFact": obj(&["kind", "lot_id", "quantity", "retained_quantity", "effective_ordinal", "settlement_ordinal",
                "consideration", "removed_carrying", "realized", "economic_allocation_ids", "formal_relationship_ids"], json!({
            "kind": s, "lot_id": s, "quantity": n, "retained_quantity": n, "effective_ordinal": i,
            "settlement_ordinal": i, "consideration": n, "removed_carrying": n, "realized": n,
            "economic_allocation_ids": {"type": "array", "items": s},
            "formal_relationship_ids": {"type": "array", "items": s}})),
        "LifecycleReceipt": obj(&["opening_cash", "closing_cash", "settled_cash_movements", "posted_vm", "received_vm",
                "posted_im", "received_im_custody", "fees_expensed", "realized", "open_claims", "events", "steps",
                "accounting_facts", "formal_hedge_accounting"], json!({
            "opening_cash": n, "closing_cash": n, "settled_cash_movements": n, "gross_derivative_assets": n,
            "gross_derivative_liabilities": n, "posted_vm": n, "received_vm": n, "posted_im": n,
            "received_im_custody": n, "fees_expensed": n, "realized": n, "cash_bridge_reconciled": b,
            "inventory_conserved": b, "collateral_conserved": b, "open_claims": b,
            "events": list("EventReceipt"), "steps": list("LiquidityStep"),
            "accounting_facts": list("AccountingFact"), "formal_hedge_accounting": b,
            "business_plan_mode": {"type": "string", "enum": ["runoff", "schedule", "policy"]},
            "policy_decisions": list("PolicyDecision"), "monthly": list("MonthlyReport")})),
        "ScenarioJournal": obj(&["scenario_id", "account_id", "receipt"], json!({
            "scenario_id": s, "account_id": s, "receipt": r("LifecycleReceipt")})),
        "SourceReceipt": obj(&["schema", "plan_sha256", "formal_hedge_accounting", "journals"], json!({
            "schema": s, "stage": s, "plan_sha256": s, "formal_hedge_accounting": b, "disclosure": s,
            "hedge_entries": objects, "journals": list("ScenarioJournal")})),
        "ProducerReceipt": obj(&["workflow"], json!({"workflow": obj(&["schema", "model_schema", "source_receipt"], json!({
            "schema": s, "model_schema": s, "source_receipt": r("SourceReceipt")}))})),
    });
    let mut base = json!({
        "Refusal": obj(&["code", "detail"], json!({"code": {"type": "string"}, "detail": {"type": "string"},
            "requirements": {"type": "array", "items": {"$ref": "#/components/schemas/Requirement"}}})),
        "Descriptor": descriptor,
        "Health": obj(&["ok", "engine", "epoch", "native_schema", "fault_injection"], json!({
            "ok": {"type": "boolean"}, "engine": {"type": "string", "enum": ["rust", "legacy"]},
            "epoch": {"type": "integer"}, "native_schema": {"type": "integer"},
            "fault_injection": {"type": "boolean"}})),
        "RevisionHead": obj(&["revision"], json!({"revision": {"type": "integer"}})),
        "Revision": obj(&["revision", "manifest", "created_at", "actor", "reason"], json!({
            "revision": {"type": "integer"}, "manifest": {"$ref": "#/components/schemas/Descriptor"},
            "created_at": {"type": "number"}, "actor": {"type": "string"}, "reason": {"type": "string"}})),
        "Snapshot": obj(&["schema", "revision", "documents"], json!({
            "schema": {"type": "string"}, "revision": {"type": "integer"},
            "documents": {"type": "object", "additionalProperties": {"$ref": "#/components/schemas/Descriptor"}}})),
        "Admission": obj(&["job_id", "duplicate", "deadline_at", "revision"], json!({
            "job_id": {"type": "string"}, "duplicate": {"type": "boolean"},
            "deadline_at": {"type": "number"}, "revision": {"type": "integer"}})),
        "Authority": obj(&["schema", "raw", "admitted_revision", "admitted_at", "deadline_at", "executable_name",
                "executable_sha256", "verifier", "authority_sha256"], json!({
            "schema": {"type": "string"}, "raw": {"$ref": "#/components/schemas/Descriptor"},
            "admitted_revision": {"type": "integer"}, "admitted_at": {"type": "number"},
            "deadline_at": {"type": "number"}, "executable_name": {"type": "string"},
            "executable_sha256": {"type": "string"}, "verifier": {"type": "string"},
            "authority_sha256": {"type": "string"}})),
        "JobSummary": obj(&["id", "kind", "status", "revision", "attempt", "created_at", "finished_at"], json!({
            "id": {"type": "string"}, "kind": {"type": "string"},
            "status": {"$ref": "#/components/schemas/JobStatus"}, "revision": {"type": "integer"},
            "attempt": {"type": "integer"}, "created_at": {"type": "number"},
            "finished_at": {"type": ["number", "null"]}})),
        "JobStatus": {"type": "string", "enum": ["native_queued", "native_running", "native_done", "native_error", "native_cancelled"]},
        "JobList": {"type": "array", "items": {"$ref": "#/components/schemas/JobSummary"}},
        "Job": obj(&["id", "kind", "status", "revision", "attempt", "created_at", "finished_at", "detail",
                "progress", "result", "published", "authority"], json!({
            "id": {"type": "string"}, "kind": {"type": "string"},
            "status": {"$ref": "#/components/schemas/JobStatus"}, "revision": {"type": "integer"},
            "attempt": {"type": "integer"}, "created_at": {"type": "number"},
            "finished_at": {"type": ["number", "null"]}, "detail": {"type": ["string", "null"]},
            "progress": {}, "result": {}, "published": {"type": "boolean"},
            "authority": {"$ref": "#/components/schemas/Authority"}})),
        "Cancelled": obj(&["cancelled"], json!({"cancelled": {"type": "boolean"}})),
        "Manifest": {"type": "object", "additionalProperties": true},
        "TablePage": obj(&["table", "offset", "limit", "total", "rows"], json!({
            "table": {"type": "string"}, "offset": {"type": "integer"}, "limit": {"type": "integer"},
            "total": {"type": "integer"}, "rows": {"type": "array", "items": {"type": "object", "additionalProperties": true}}})),
    });
    let schemas = base.as_object_mut().expect("schemas");
    for (k, v) in lifecycle.as_object().expect("lifecycle schemas") {
        schemas.insert(k.clone(), v.clone());
    }
    base
}

pub fn openapi() -> Value {
    let mut paths = Map::new();
    for r in ROUTES {
        let mut parameters: Vec<Value> = Vec::new();
        for seg in r.path.split('/') {
            if let Some(name) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let schema = if name == "revision" {
                    json!({"type": "integer"})
                } else {
                    json!({"type": "string"})
                };
                parameters
                    .push(json!({"name": name, "in": "path", "required": true, "schema": schema}));
            }
        }
        for h in r.headers {
            parameters.push(
                json!({"name": h.name, "in": "header", "required": h.required,
                "description": h.description, "schema": {"type": "string"}}),
            );
        }
        for q in r.query {
            let schema = if ["offset", "limit", "revision"].contains(q) {
                json!({"type": "integer"})
            } else {
                json!({"type": "string"})
            };
            parameters.push(json!({"name": q, "in": "query", "required": false, "schema": schema}));
        }
        let mut op = json!({
            "operationId": r.operation, "summary": r.summary, "parameters": parameters,
            "responses": {
                r.status.to_string(): {"description": "OK", "content": {"application/json": {"schema": {"$ref": format!("#/components/schemas/{}", r.response)}}}},
                "default": {"description": "Refusal", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Refusal"}}}},
            },
            "security": [{"bearer": []}],
        });
        if r.body.is_some() {
            op["requestBody"] = json!({"required": false, "description": "Raw strict JSON; the bytes are stored and hashed exactly as sent.",
                "content": {"application/json": {"schema": {"type": "object"}}}});
        }
        paths
            .entry(r.path.to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("path item")
            .insert(r.method.to_string(), op);
    }
    json!({
        "openapi": "3.1.0",
        "info": {"title": "Rust workbench service", "version": env!("CARGO_PKG_VERSION"),
            "description": "Same-origin native admission, durable leased worker and verified publication (issue #47)."},
        "paths": paths,
        "components": {"schemas": schemas(), "securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}}},
    })
}

fn ts_type(schema: &Value) -> String {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        return r.rsplit('/').next().unwrap_or("unknown").to_string();
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
    }
    let one = |t: &str| -> String {
        match t {
            "string" => "string".into(),
            "number" | "integer" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => format!(
                "{}[]",
                schema.get("items").map_or("unknown".into(), ts_type)
            ),
            "object" => {
                if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                    let required: Vec<&str> = schema["required"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let fields: Vec<String> = props
                        .iter()
                        .map(|(k, v)| {
                            format!(
                                "  {k}{}: {};",
                                if required.contains(&k.as_str()) {
                                    ""
                                } else {
                                    "?"
                                },
                                ts_type(v)
                            )
                        })
                        .collect();
                    format!("{{\n{}\n}}", fields.join("\n"))
                } else {
                    match schema.get("additionalProperties") {
                        Some(Value::Object(inner)) => {
                            format!("Record<string, {}>", ts_type(&Value::Object(inner.clone())))
                        }
                        _ => "Record<string, unknown>".into(),
                    }
                }
            }
            _ => "unknown".into(),
        }
    };
    match schema.get("type") {
        Some(Value::String(t)) => one(t),
        Some(Value::Array(ts)) => ts
            .iter()
            .filter_map(Value::as_str)
            .map(one)
            .collect::<Vec<_>>()
            .join(" | "),
        _ => "unknown".into(),
    }
}

pub fn typescript() -> String {
    let mut out = String::from(
        "// Generated by `workbench-native contract typescript` from apps/api-rust/src/contract.rs.\n\
         // Do not edit by hand; `cargo test` in apps/api-rust fails when this file is stale.\n\n",
    );
    let schemas = schemas();
    for (name, schema) in schemas.as_object().expect("schemas") {
        out.push_str(&format!("export type {name} = {};\n\n", ts_type(schema)));
    }
    out.push_str("export const nativeRoutes = {\n");
    for r in ROUTES {
        out.push_str(&format!(
            "  {}: {{ method: \"{}\", path: \"{}\", status: {} }},\n",
            r.operation,
            r.method.to_uppercase(),
            r.path,
            r.status
        ));
    }
    out.push_str("} as const;\n\nexport interface NativeResponses {\n");
    for r in ROUTES {
        out.push_str(&format!("  {}: {};\n", r.operation, r.response));
    }
    out.push_str("}\n");
    out
}
