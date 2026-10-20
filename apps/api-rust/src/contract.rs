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
        query: &["document"], body: Some("raw-json"), status: 202, response: "Admission" },
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
    json!({
        "Refusal": obj(&["code", "detail"], json!({"code": {"type": "string"}, "detail": {"type": "string"}})),
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
    })
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
            let schema = if *q == "document" {
                json!({"type": "string"})
            } else {
                json!({"type": "integer"})
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
