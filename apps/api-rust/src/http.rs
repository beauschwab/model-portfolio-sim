//! Same-origin HTTP admission. Requests carry no tenant, workspace or storage prefix; the
//! process serves its configured scope. Cross-origin browser requests are refused, a bearer
//! token is required when configured (always in production), bodies are bounded before they
//! are read, and raw bodies are stored as the exact bytes received.
use crate::config::Config;
use crate::objects::{Descriptor, Objects};
use crate::repo::{Repo, Submission};
use crate::{compose, publish, Refusal};
use portfolio_verification_native::MAX_REQUEST_BYTES;
use serde_json::{json, Value};
use std::io::Read;
use std::sync::Arc;

pub struct App {
    pub config: Config,
    pub repo: Repo,
    pub objects: Objects,
}

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

fn ok(status: u16, body: Value) -> Result<Reply, Refusal> {
    Ok(Reply { status, body })
}

pub fn status_for(code: &str) -> u16 {
    match code {
        "NOT_FOUND" => 404,
        "MALFORMED_INPUT" => 400,
        "UNSUPPORTED_SCHEMA"
        | compose::MAPPING_REQUIRED
        | "BOUND_EXCEEDED"
        | "INELIGIBLE_PACKAGE" => 422,
        "PREPARATION_STALE" => 409,
        "BODY_TOO_LARGE" => 413,
        "UNAUTHORIZED" => 401,
        "CROSS_ORIGIN" => 403,
        "STALE_REVISION"
        | "IDEMPOTENCY_CONFLICT"
        | "NOT_ROUTED"
        | "LEGACY_WORKSPACE"
        | "NOT_PUBLISHED" => 409,
        "QUEUE_FULL" => 429,
        "EXECUTABLE_UNAVAILABLE" | "NOT_INITIALIZED" => 503,
        _ => 500,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub struct Incoming<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub header: &'a dyn Fn(&str) -> Option<String>,
    pub body: &'a [u8],
}

fn query_param(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

fn query_int(query: &str, name: &str, default: i64) -> Result<i64, Refusal> {
    match query_param(query, name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| {
            Refusal::new(
                "MALFORMED_INPUT",
                format!("query `{name}` is not an integer"),
            )
        }),
    }
}

fn header_int(incoming: &Incoming, name: &str) -> Result<Option<i64>, Refusal> {
    (incoming.header)(name)
        .map(|v| {
            v.trim().trim_matches('"').parse::<i64>().map_err(|_| {
                Refusal::new(
                    "MALFORMED_INPUT",
                    format!("header `{name}` is not an integer"),
                )
            })
        })
        .transpose()
}

impl App {
    pub fn new(config: Config) -> Result<App, String> {
        let db = crate::sql::Db::open(&config.database_url)?;
        crate::migrate::run(&db)?;
        let repo = Repo::new(db, &config.tenant, &config.workspace);
        let objects = Objects::open(&config.artifact_url, &config.tenant, &config.workspace)?;
        Ok(App {
            config,
            repo,
            objects,
        })
    }

    fn guard(&self, incoming: &Incoming) -> Result<(), Refusal> {
        if let Some(origin) = (incoming.header)("Origin") {
            if self.config.allowed_origin.as_deref() != Some(origin.as_str()) {
                return Err(Refusal::new(
                    "CROSS_ORIGIN",
                    "cross-origin requests are refused",
                ));
            }
        }
        if let Some(site) = (incoming.header)("Sec-Fetch-Site") {
            if site != "same-origin" && site != "none" {
                return Err(Refusal::new(
                    "CROSS_ORIGIN",
                    "cross-site requests are refused",
                ));
            }
        }
        if let Some(token) = &self.config.api_token {
            let supplied = (incoming.header)("Authorization").unwrap_or_default();
            let expected = format!("Bearer {token}");
            if !constant_time_eq(supplied.as_bytes(), expected.as_bytes()) {
                return Err(Refusal::new(
                    "UNAUTHORIZED",
                    "a valid bearer token is required",
                ));
            }
        }
        Ok(())
    }

    fn strict(&self, raw: &[u8]) -> Result<Value, Refusal> {
        let value = portfolio_verification_native::raw::parse(raw, MAX_REQUEST_BYTES)
            .map_err(|e| Refusal::new("MALFORMED_INPUT", e))?;
        if value.get("schema").and_then(Value::as_str) != Some(crate::LIFECYCLE_SCHEMA) {
            return Err(Refusal::new(
                "UNSUPPORTED_SCHEMA",
                format!(
                    "the Rust service admits `{}` documents",
                    crate::LIFECYCLE_SCHEMA
                ),
            ));
        }
        Ok(value)
    }

    /// Saved documents: raw lifecycle requests, saved books and lifecycle plans.
    fn document(&self, raw: &[u8]) -> Result<(), Refusal> {
        let value = portfolio_verification_native::raw::parse(raw, MAX_REQUEST_BYTES)
            .map_err(|e| Refusal::new("MALFORMED_INPUT", e))?;
        match value.get("schema").and_then(Value::as_str) {
            Some(crate::LIFECYCLE_SCHEMA) => Ok(()),
            Some(compose::BOOK_SCHEMA) => compose::parse_book(raw).map(|_| ()),
            Some(compose::PLAN_SCHEMA) => compose::parse_plan(raw).map(|_| ()),
            _ => Err(Refusal::new(
                "UNSUPPORTED_SCHEMA",
                format!(
                    "saved documents are `{}`, `{}` or `{}`",
                    crate::LIFECYCLE_SCHEMA,
                    compose::BOOK_SCHEMA,
                    compose::PLAN_SCHEMA
                ),
            )),
        }
    }

    fn saved(&self, revision: i64, name: &str) -> Result<(Descriptor, Vec<u8>), Refusal> {
        let snapshot = self.repo.snapshot(&self.objects, revision)?;
        let d = snapshot["documents"].get(name).ok_or_else(|| {
            Refusal::new(
                "NOT_FOUND",
                format!("document `{name}` is not in revision {revision}"),
            )
        })?;
        let d = Descriptor::from_json(d)?;
        let bytes = self.objects.get_bounded(&d, MAX_REQUEST_BYTES as u64)?;
        Ok((d, bytes))
    }

    fn revision_or_head(&self, incoming: &Incoming) -> Result<i64, Refusal> {
        match query_param(incoming.query, "revision") {
            Some(r) => parse_rev(&r),
            None => self.repo.head()?.ok_or_else(|| {
                Refusal::new("NOT_INITIALIZED", "workspace has not been initialized")
            }),
        }
    }

    /// Composes the request for `?book=` (and `?plan=` or the body) at the expected revision.
    fn composed(&self, incoming: &Incoming, expected: i64) -> Result<compose::Composed, Refusal> {
        let book_name = query_param(incoming.query, "book")
            .ok_or_else(|| Refusal::new("MALFORMED_INPUT", "`book` names a saved book"))?;
        let (book_d, book_bytes) = self.saved(expected, &book_name)?;
        let book = compose::parse_book(&book_bytes)?;
        let plan_bytes = match query_param(incoming.query, "plan") {
            Some(name) => {
                if !incoming.body.is_empty() {
                    return Err(Refusal::new(
                        "MALFORMED_INPUT",
                        "a saved plan run has no body",
                    ));
                }
                self.saved(expected, &name)?.1
            }
            None => incoming.body.to_vec(),
        };
        let plan = compose::parse_plan(&plan_bytes)?;
        compose::compose(
            &book,
            &plan,
            &compose::Binding {
                workspace: &self.config.workspace,
                revision: expected,
                book_name: &book_name,
                book_sha256: &book_d.sha256,
            },
        )
        .map_err(|(mut r, needs)| {
            r.requirements = needs.iter().map(compose::Requirement::to_json).collect();
            r
        })
    }

    pub fn handle(&self, incoming: &Incoming) -> Result<Reply, Refusal> {
        self.guard(incoming)?;
        let parts: Vec<&str> = incoming
            .path
            .trim_end_matches('/')
            .split('/')
            .skip(1)
            .collect();
        match (incoming.method, parts.as_slice()) {
            ("GET", ["native", "health"]) => {
                let (engine, epoch) = self.repo.route()?;
                ok(
                    200,
                    json!({"ok": true, "engine": engine, "epoch": epoch,
                    "native_schema": crate::migrate::MIGRATIONS.last().map_or(0, |m| m.version),
                    "fault_injection": crate::fault::ENABLED}),
                )
            }
            ("GET", ["native", "revisions", "head"]) => {
                let head = self.repo.head()?.ok_or_else(|| {
                    Refusal::new("NOT_INITIALIZED", "workspace has not been initialized")
                })?;
                ok(200, json!({"revision": head}))
            }
            ("GET", ["native", "revisions", n]) => ok(200, self.repo.revision(parse_rev(n)?)?),
            ("GET", ["native", "revisions", n, "snapshot"]) => {
                ok(200, self.repo.snapshot(&self.objects, parse_rev(n)?)?)
            }
            ("GET", ["native", "documents"]) => {
                let revision = self.revision_or_head(incoming)?;
                let snapshot = self.repo.snapshot(&self.objects, revision)?;
                let mut docs = Vec::new();
                for (name, d) in snapshot["documents"].as_object().into_iter().flatten() {
                    let d = Descriptor::from_json(d)?;
                    let bytes = self.objects.get_bounded(&d, MAX_REQUEST_BYTES as u64)?;
                    let schema = serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|v| v.get("schema").and_then(Value::as_str).map(str::to_string));
                    docs.push(json!({"name": name, "schema": schema, "bytes": d.bytes, "sha256": d.sha256}));
                }
                ok(200, json!({"revision": revision, "documents": docs}))
            }
            ("GET", ["native", "documents", name]) => {
                let revision = self.revision_or_head(incoming)?;
                let (_, bytes) = self.saved(revision, name)?;
                ok(
                    200,
                    serde_json::from_slice(&bytes)
                        .map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))?,
                )
            }
            ("POST", ["native", "lifecycle", "compose"]) => {
                let expected = header_int(incoming, "X-Expected-Revision")?.ok_or_else(|| {
                    Refusal::new("MALFORMED_INPUT", "X-Expected-Revision is required")
                })?;
                let c = self.composed(incoming, expected)?;
                ok(
                    200,
                    json!({"request": c.request, "request_sha256": crate::sha256_hex(&c.bytes),
                        "bytes": c.bytes.len(), "revision": expected}),
                )
            }
            ("GET", ["native", "lifecycle", "example"]) => ok(
                200,
                json!({
                    "book": serde_json::from_str::<Value>(compose::EXAMPLE_BOOK).expect("example book"),
                    "plan": serde_json::from_str::<Value>(compose::EXAMPLE_PLAN).expect("example plan"),
                    "policy_plan": serde_json::from_str::<Value>(compose::EXAMPLE_POLICY_PLAN).expect("example policy plan"),
                }),
            ),
            ("GET", ["native", "jobs", id, "receipt"]) => {
                let (d, _) = self.published(id)?;
                let d = publish::lifecycle_of(&self.objects, &d)?;
                ok(200, publish::receipt(&self.objects, &d)?)
            }
            ("POST", ["native", "lifecycle", "allocations"]) => self.allocate(incoming),
            ("GET", ["native", "jobs", id, "allocation"]) => {
                let (d, _) = self.published(id)?;
                ok(200, publish::allocation(&self.objects, &d)?)
            }
            ("POST", ["native", "jobs", id, "allocation", "evaluate"]) => {
                let (d, _) = self.published(id)?;
                let published = publish::allocation(&self.objects, &d)?;
                self.fresh(id)?;
                let body = portfolio_verification_native::raw::parse(incoming.body, 64 * 1024)
                    .map_err(|e| Refusal::new("MALFORMED_INPUT", e))?;
                let selection: Vec<String> = body["selection"]
                    .as_array()
                    .ok_or_else(|| {
                        Refusal::new("MALFORMED_INPUT", "`selection` lists package ids")
                    })?
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                let floor = body.get("minimum_usable_cash").and_then(Value::as_f64);
                ok(
                    200,
                    crate::allocation::evaluate(&published["allocation"], &selection, floor)?,
                )
            }
            ("PUT", ["native", "documents", name]) => {
                let expected = header_int(incoming, "If-Match")?.ok_or_else(|| {
                    Refusal::new("MALFORMED_INPUT", "If-Match names the expected revision")
                })?;
                self.document(incoming.body)?;
                let d = self.objects.put(incoming.body, "json")?;
                let revision =
                    self.repo
                        .commit_document(&self.objects, expected, name, &d, "native-api")?;
                ok(201, json!({"revision": revision}))
            }
            ("POST", ["native", "lifecycle", "runs"]) => self.submit(incoming),
            ("GET", ["native", "jobs"]) => {
                let offset = query_int(incoming.query, "offset", 0)?;
                let limit = query_int(incoming.query, "limit", 100)?;
                ok(200, self.repo.list_jobs(offset, limit)?)
            }
            ("GET", ["native", "jobs", id]) => ok(200, self.repo.job(id)?),
            ("POST", ["native", "jobs", id, "cancel"]) => {
                ok(200, json!({"cancelled": self.repo.cancel(id)?}))
            }
            ("GET", ["native", "jobs", id, "manifest"]) => {
                let (d, _) = self.published(id)?;
                ok(200, publish::load_manifest(&self.objects, &d)?)
            }
            ("GET", ["native", "jobs", id, "tables", table]) => {
                let (d, _) = self.published(id)?;
                let d = publish::lifecycle_of(&self.objects, &d)?;
                let offset = query_int(incoming.query, "offset", 0)?;
                let limit = query_int(incoming.query, "limit", 100)?;
                if offset < 0 || limit < 1 {
                    return Err(Refusal::new("MALFORMED_INPUT", "invalid page window"));
                }
                ok(
                    200,
                    publish::page(&self.objects, &d, table, offset as usize, limit as usize)?,
                )
            }
            _ => Err(Refusal::new("NOT_FOUND", "no such route")),
        }
    }

    fn published(&self, id: &str) -> Result<(Descriptor, i64), Refusal> {
        self.repo.job(id)?;
        self.repo
            .publication(id)?
            .ok_or_else(|| Refusal::new("NOT_PUBLISHED", "the job has no completed publication"))
    }

    fn submit(&self, incoming: &Incoming) -> Result<Reply, Refusal> {
        let expected = header_int(incoming, "X-Expected-Revision")?
            .ok_or_else(|| Refusal::new("MALFORMED_INPUT", "X-Expected-Revision is required"))?;
        let deadline = match (incoming.header)("X-Deadline-Seconds") {
            Some(v) => v
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .ok_or_else(|| {
                    Refusal::new("MALFORMED_INPUT", "X-Deadline-Seconds is not a number")
                })?,
            None => self.config.default_deadline_seconds,
        };
        let raw: Vec<u8> = match query_param(incoming.query, "document") {
            _ if query_param(incoming.query, "book").is_some() => {
                self.composed(incoming, expected)?.bytes
            }
            Some(name) => {
                if !incoming.body.is_empty() {
                    return Err(Refusal::new(
                        "MALFORMED_INPUT",
                        "a document run has no body",
                    ));
                }
                let snapshot = self.repo.snapshot(&self.objects, expected)?;
                let d = snapshot["documents"].get(&name).ok_or_else(|| {
                    Refusal::new(
                        "NOT_FOUND",
                        format!("document `{name}` is not in revision {expected}"),
                    )
                })?;
                self.objects
                    .get_bounded(&Descriptor::from_json(d)?, MAX_REQUEST_BYTES as u64)?
            }
            None => incoming.body.to_vec(),
        };
        self.strict(&raw)?;
        self.admit(incoming, crate::LIFECYCLE_SCHEMA, &raw, expected, deadline)
    }

    fn deadline(&self, incoming: &Incoming) -> Result<f64, Refusal> {
        match (incoming.header)("X-Deadline-Seconds") {
            Some(v) => v
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .ok_or_else(|| {
                    Refusal::new("MALFORMED_INPUT", "X-Deadline-Seconds is not a number")
                }),
            None => Ok(self.config.default_deadline_seconds),
        }
    }

    /// `POST /native/lifecycle/allocations?book=`: reviewed packages (saved plans and groups), the
    /// usable-cash floor, the objective and the refinement bound, composed at the expected revision.
    fn allocate(&self, incoming: &Incoming) -> Result<Reply, Refusal> {
        let expected = header_int(incoming, "X-Expected-Revision")?
            .ok_or_else(|| Refusal::new("MALFORMED_INPUT", "X-Expected-Revision is required"))?;
        let deadline = self.deadline(incoming)?;
        let book_name = query_param(incoming.query, "book")
            .ok_or_else(|| Refusal::new("MALFORMED_INPUT", "`book` names a saved book"))?;
        let (book_d, book_bytes) = self.saved(expected, &book_name)?;
        let book = compose::parse_book(&book_bytes)?;
        let (packages, constraints) = crate::allocation::parse_body(incoming.body)?;
        let docs = packages
            .into_iter()
            .map(|(name, group)| {
                let (d, bytes) = self.saved(expected, &name)?;
                Ok(crate::allocation::PackageDoc {
                    plan: compose::parse_plan(&bytes)?,
                    name,
                    group,
                    sha256: d.sha256,
                })
            })
            .collect::<Result<Vec<_>, Refusal>>()?;
        let binding = compose::Binding {
            workspace: &self.config.workspace,
            revision: expected,
            book_name: &book_name,
            book_sha256: &book_d.sha256,
        };
        let raw = crate::allocation::compose_request(&book, &binding, &docs, constraints)?;
        if raw.len() > MAX_REQUEST_BYTES {
            return Err(Refusal::new(
                "BOUND_EXCEEDED",
                "the allocation request exceeds 8 MiB",
            ));
        }
        self.admit(
            incoming,
            crate::allocation::SCHEMA,
            &raw,
            expected,
            deadline,
        )
    }

    /// Prepared coefficients are valid only for the documents they were prepared from: any change
    /// of the saved book or a package plan at the head revision requires a new allocation.
    fn fresh(&self, id: &str) -> Result<(), Refusal> {
        let view = self.repo.job(id)?;
        let raw_d = Descriptor::from_json(&view["authority"]["raw"])?;
        let raw: Value =
            serde_json::from_slice(&self.objects.get_bounded(&raw_d, MAX_REQUEST_BYTES as u64)?)
                .map_err(|e| Refusal::new("STORAGE_ERROR", e.to_string()))?;
        let head = self
            .repo
            .head()?
            .ok_or_else(|| Refusal::new("NOT_INITIALIZED", "workspace has not been initialized"))?;
        let snapshot = self.repo.snapshot(&self.objects, head)?;
        let current = |name: &str| {
            snapshot["documents"][name]["sha256"]
                .as_str()
                .map(str::to_string)
        };
        let book = &raw["base"]["plan"]["saved_book_revision"];
        let book_name = book["id"].as_str().unwrap_or("");
        let book_sha = book["revision"]
            .as_str()
            .and_then(|r| r.split_once(':'))
            .map(|(_, s)| s.to_string());
        let mut stale = Vec::new();
        if current(book_name) != book_sha {
            stale.push(format!("saved book `{book_name}`"));
        }
        for p in raw["packages"].as_array().into_iter().flatten() {
            let name = p["package_id"].as_str().unwrap_or("");
            let sha = p["review"]["revision"]
                .as_str()
                .and_then(|r| r.split_once(':'))
                .map(|(_, s)| s.to_string());
            if current(name) != sha {
                stale.push(format!("plan `{name}`"));
            }
        }
        if !stale.is_empty() {
            return Err(Refusal::new(
                "PREPARATION_STALE",
                format!(
                    "{} changed since this allocation was prepared; submit a new allocation",
                    stale.join(", ")
                ),
            ));
        }
        Ok(())
    }

    fn admit(
        &self,
        incoming: &Incoming,
        schema: &'static str,
        raw: &[u8],
        expected: i64,
        deadline: f64,
    ) -> Result<Reply, Refusal> {
        let d = self.objects.put(raw, "json")?;
        let exe = &self.config.ledger_exe;
        let sha = crate::worker::executable_sha256(exe)?;
        let key = (incoming.header)("Idempotency-Key");
        let admission = self.repo.admit(&Submission {
            schema,
            raw: &d,
            expected_revision: expected,
            deadline_seconds: deadline,
            executable_name: &crate::worker::executable_name(exe),
            executable_sha256: &sha,
            idempotency_key: key.as_deref(),
        })?;
        ok(
            202,
            json!({"job_id": admission.job_id, "duplicate": admission.duplicate,
            "deadline_at": admission.deadline_at, "revision": admission.revision}),
        )
    }
}

fn parse_rev(n: &str) -> Result<i64, Refusal> {
    n.parse()
        .ok()
        .filter(|v: &i64| *v >= 0)
        .ok_or_else(|| Refusal::new("MALFORMED_INPUT", "revision is a non-negative integer"))
}

/// Serves until the process ends. Prints `listening on <addr>` once bound (port 0 allowed).
pub fn serve(app: App, threads: usize) -> Result<(), String> {
    let server = tiny_http::Server::http(&app.config.bind).map_err(|e| e.to_string())?;
    println!("listening on {}", server.server_addr());
    use std::io::Write;
    std::io::stdout().flush().ok();
    let server = Arc::new(server);
    let app = Arc::new(app);
    let workers: Vec<_> = (0..threads.max(1))
        .map(|_| {
            let (server, app) = (server.clone(), app.clone());
            std::thread::spawn(move || {
                while let Ok(mut request) = server.recv() {
                    let reply = respond(&app, &mut request);
                    let bytes = serde_json::to_vec(&reply.body).expect("reply serializes");
                    let mut response =
                        tiny_http::Response::from_data(bytes).with_status_code(reply.status);
                    for (k, v) in [
                        ("Content-Type", "application/json"),
                        ("Cache-Control", "no-store"),
                        ("X-Content-Type-Options", "nosniff"),
                    ] {
                        response = response
                            .with_header(tiny_http::Header::from_bytes(k, v).expect("header"));
                    }
                    let _ = request.respond(response);
                }
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
    Ok(())
}

fn respond(app: &App, request: &mut tiny_http::Request) -> Reply {
    let refusal = |r: Refusal| Reply {
        status: status_for(r.code),
        body: if r.requirements.is_empty() {
            json!({"code": r.code, "detail": r.detail})
        } else {
            json!({"code": r.code, "detail": r.detail, "requirements": r.requirements})
        },
    };
    if request.body_length().is_some_and(|n| n > MAX_REQUEST_BYTES) {
        return refusal(Refusal::new("BODY_TOO_LARGE", "request body exceeds 8 MiB"));
    }
    let mut body = Vec::new();
    if request
        .as_reader()
        .take(MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .is_err()
    {
        return refusal(Refusal::new(
            "MALFORMED_INPUT",
            "request body could not be read",
        ));
    }
    if body.len() > MAX_REQUEST_BYTES {
        return refusal(Refusal::new("BODY_TOO_LARGE", "request body exceeds 8 MiB"));
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let method = request.method().as_str().to_ascii_uppercase();
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|h| {
            (
                h.field.as_str().as_str().to_ascii_lowercase(),
                h.value.as_str().to_string(),
            )
        })
        .collect();
    let header = |name: &str| -> Option<String> {
        let name = name.to_ascii_lowercase();
        headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.clone())
    };
    let incoming = Incoming {
        method: &method,
        path,
        query,
        header: &header,
        body: &body,
    };
    match app.handle(&incoming) {
        Ok(reply) => reply,
        Err(r) => refusal(r),
    }
}
