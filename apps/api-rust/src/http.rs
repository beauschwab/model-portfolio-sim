//! Same-origin HTTP admission. Requests carry no tenant, workspace or storage prefix; the
//! process serves its configured scope. Cross-origin browser requests are refused, a bearer
//! token is required when configured (always in production), bodies are bounded before they
//! are read, and raw bodies are stored as the exact bytes received.
use crate::config::Config;
use crate::objects::{Descriptor, Objects};
use crate::repo::{Repo, Submission};
use crate::{publish, Refusal};
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
        "UNSUPPORTED_SCHEMA" => 422,
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
            ("PUT", ["native", "documents", name]) => {
                let expected = header_int(incoming, "If-Match")?.ok_or_else(|| {
                    Refusal::new("MALFORMED_INPUT", "If-Match names the expected revision")
                })?;
                self.strict(incoming.body)?;
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
        let d = self.objects.put(&raw, "json")?;
        let exe = &self.config.ledger_exe;
        let sha = crate::worker::executable_sha256(exe)?;
        let key = (incoming.header)("Idempotency-Key");
        let admission = self.repo.admit(&Submission {
            schema: crate::LIFECYCLE_SCHEMA,
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
        body: json!({"code": r.code, "detail": r.detail}),
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
