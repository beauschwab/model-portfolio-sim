//! Process-test harness: real `workbench-native` processes over a temporary SQLite file (and
//! PostgreSQL when `TEST_POSTGRES_URL` is set, with a unique workspace), loopback ports and a
//! cleared environment, so no Python runtime, package or PATH entry is available to them.
#![allow(dead_code)]
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use workbench_native::sql::{Db, Sql};

pub const REQUEST: &str = include_str!(
    "../../../../packages/portfolio-verification-native/tests/fixtures/lifecycle-unwind.request.json"
);
pub const TOKEN: &str = "test-token-0123456789";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backend {
    Sqlite,
    Postgres,
}

pub fn backends() -> Vec<Backend> {
    let mut b = vec![Backend::Sqlite];
    if std::env::var("TEST_POSTGRES_URL").is_ok_and(|v| !v.is_empty()) {
        b.push(Backend::Postgres);
    }
    b
}

pub struct Env {
    pub dir: PathBuf,
    pub database_url: String,
    pub artifacts: PathBuf,
    pub tenant: String,
    pub workspace: String,
    pub ledger: PathBuf,
    pub extra: Vec<(String, String)>,
}

/// The stub producer example. A filtered run (`cargo test --test live`) does not build examples,
/// so callers that replace the producer do not need it.
pub fn stub_producer() -> Option<PathBuf> {
    let exe = std::env::current_exe().expect("test exe");
    let debug = exe
        .parent()
        .and_then(Path::parent)
        .expect("target profile dir");
    let name = if cfg!(windows) {
        "stub_producer.exe"
    } else {
        "stub_producer"
    };
    let path = debug.join("examples").join(name);
    path.exists().then_some(path)
}

impl Env {
    pub fn new(backend: Backend, label: &str) -> Env {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = std::env::temp_dir().join(format!("wbn-{label}-{}", &id[..12]));
        std::fs::create_dir_all(&dir).unwrap();
        let database_url = match backend {
            Backend::Sqlite => format!(
                "sqlite:///{}",
                dir.join("workbench.db")
                    .to_string_lossy()
                    .replace('\\', "/")
            ),
            Backend::Postgres => std::env::var("TEST_POSTGRES_URL").unwrap(),
        };
        // Each test owns a copy of the producer so replacing it cannot affect another test.
        let ledger = dir.join(if cfg!(windows) {
            "portfolio-model-ledger.exe"
        } else {
            "portfolio-model-ledger"
        });
        if let Some(stub) = stub_producer() {
            std::fs::copy(stub, &ledger).unwrap();
        }
        Env {
            artifacts: dir.join("artifacts"),
            dir,
            database_url,
            tenant: format!("t-{}", &id[..8]),
            workspace: format!("w-{}", &id[8..16]),
            ledger,
            extra: Vec::new(),
        }
    }

    pub fn db(&self) -> Db {
        Db::open(&self.database_url).unwrap()
    }

    pub fn command(&self, args: &[&str], extra: &[(&str, &str)]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_workbench-native"));
        c.args(args).env_clear();
        // Only what a Windows socket stack needs; no PATH, so no interpreter can be found.
        for keep in ["SystemRoot", "SYSTEMROOT", "TEMP", "TMP"] {
            if let Ok(v) = std::env::var(keep) {
                c.env(keep, v);
            }
        }
        c.env("PATH", "")
            .env("DATABASE_URL", &self.database_url)
            .env("ARTIFACT_URL", &self.artifacts)
            .env("WORKBENCH_TENANT_ID", &self.tenant)
            .env("WORKBENCH_WORKSPACE_ID", &self.workspace)
            .env("WORKBENCH_API_TOKEN", TOKEN)
            .env("PORTFOLIO_MODEL_LEDGER_EXE", &self.ledger)
            .env("WORKBENCH_LEASE_SECONDS", "3");
        for (k, v) in &self.extra {
            c.env(k, v);
        }
        for (k, v) in extra {
            c.env(k, v);
        }
        c
    }

    pub fn admin(&self, args: &[&str]) -> Result<Value, String> {
        let mut full = vec!["admin"];
        full.extend_from_slice(args);
        let out = self.command(&full, &[]).output().unwrap();
        if out.status.success() {
            Ok(serde_json::from_slice(&out.stdout).unwrap())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    /// Migrates, creates a native workspace and routes it to Rust.
    pub fn ready(&self) {
        self.admin(&["migrate"]).unwrap();
        assert_eq!(self.admin(&["init"]).unwrap()["native_workspace"], true);
        assert_eq!(
            self.admin(&["cutover", "--to", "rust"]).unwrap()["engine"],
            "rust"
        );
    }

    pub fn api(&self) -> Api {
        let mut child = self
            .command(
                &["api", "--threads", "4"],
                &[("WORKBENCH_NATIVE_BIND", "127.0.0.1:0")],
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let addr = line
            .trim()
            .strip_prefix("listening on ")
            .expect("listening line")
            .to_string();
        Api { child, addr }
    }

    pub fn worker(&self, extra: &[(&str, &str)]) -> Child {
        self.command(&["worker", "--exit-when-idle"], extra)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    pub fn exec(&self, sql: &str, params: &[Sql]) -> u64 {
        self.db()
            .write::<_, String>(|tx| tx.exec(sql, params))
            .unwrap()
    }
    pub fn rows(&self, sql: &str, params: &[Sql]) -> Vec<Vec<Sql>> {
        self.db().read(sql, params).unwrap()
    }
    pub fn scope(&self) -> Vec<Sql> {
        vec![self.tenant.as_str().into(), self.workspace.as_str().into()]
    }
    pub fn publications(&self) -> i64 {
        self.rows(
            "SELECT COUNT(*) FROM native_publications WHERE tenant = $1 AND workspace = $2",
            &self.scope(),
        )[0][0]
            .int()
            .unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub struct Api {
    pub child: Child,
    pub addr: String,
}

impl Drop for Api {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Response {
    pub status: u16,
    pub body: Value,
}

impl Api {
    pub fn call(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Response {
        let mut stream = TcpStream::connect(&self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n",
            self.addr,
            body.len()
        );
        if !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
        {
            head.push_str(&format!("Authorization: Bearer {TOKEN}\r\n"));
        }
        for (k, v) in headers {
            if !v.is_empty() {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status: u16 = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        // Large responses use chunked transfer encoding.
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut out = String::new();
            let mut rest = body;
            while let Some((size, tail)) = rest.split_once("\r\n") {
                let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
                if n == 0 || tail.len() < n + 2 {
                    break;
                }
                out.push_str(&tail[..n]);
                rest = &tail[n + 2..];
            }
            out
        } else {
            body.to_string()
        };
        Response {
            status,
            body: serde_json::from_str(&body).unwrap_or(Value::Null),
        }
    }
    pub fn get(&self, path: &str) -> Response {
        self.call("GET", path, &[], b"")
    }
    pub fn submit(
        &self,
        revision: i64,
        key: Option<&str>,
        body: &[u8],
        deadline: Option<&str>,
    ) -> Response {
        let rev = revision.to_string();
        self.call(
            "POST",
            "/native/lifecycle/runs",
            &[
                ("X-Expected-Revision", &rev),
                ("Idempotency-Key", key.unwrap_or("")),
                ("X-Deadline-Seconds", deadline.unwrap_or("")),
                ("Content-Type", "application/json"),
            ],
            body,
        )
    }
}

pub fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn finish(mut child: Child, timeout: Duration) -> String {
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            panic!("worker did not exit");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    err
}
