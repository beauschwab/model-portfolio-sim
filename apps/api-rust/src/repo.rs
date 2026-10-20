//! Scoped durable operations over the shared tables. Every statement names the configured
//! tenant/workspace; no request parameter selects either. Mutating operations take the
//! workspace row lock first, matching `Repository._lock` in the Python service.
//!
//! Native jobs share the `jobs` table (and its idempotency constraint, listing and object
//! collection roots) but use the statuses `native_queued`, `native_running`, `native_done`,
//! `native_error` and `native_cancelled`. The legacy oldest-job claim selects only `queued` and
//! requeues only `running`, so a legacy worker can never consume or reset a native job.
use crate::objects::{Descriptor, Objects};
use crate::sql::{Db, Row, Sql, Tx};
use crate::{now, Refusal};
use serde_json::{json, Value};

pub const QUEUED: &str = "native_queued";
pub const RUNNING: &str = "native_running";
pub const DONE: &str = "native_done";
pub const ERROR: &str = "native_error";
pub const CANCELLED: &str = "native_cancelled";
pub const MAX_QUEUE: i64 = 32;
pub const MAX_ATTEMPTS: i64 = 3;
pub const ENGINE_RUST: &str = "rust";
pub const ENGINE_LEGACY: &str = "legacy";
pub const SNAPSHOT_SCHEMA: &str = "native-workspace-1";
pub const MAX_SNAPSHOT_BYTES: u64 = 8 * 1024 * 1024;

pub struct Repo {
    pub db: Db,
    pub tenant: String,
    pub workspace: String,
}

/// The immutable admission record. Its digest is checked again under the final lock.
#[derive(Debug, Clone, PartialEq)]
pub struct Authority {
    pub job_id: String,
    pub schema: String,
    pub raw: Descriptor,
    pub admitted_revision: i64,
    pub admitted_at: f64,
    pub deadline_at: f64,
    pub executable_name: String,
    pub executable_sha256: String,
    pub verifier: String,
}

impl Authority {
    pub fn canonical(&self) -> Value {
        json!({
            "job_id": self.job_id, "schema": self.schema, "raw": self.raw.to_json(),
            "admitted_revision": self.admitted_revision, "admitted_at": self.admitted_at,
            "deadline_at": self.deadline_at, "executable_name": self.executable_name,
            "executable_sha256": self.executable_sha256, "verifier": self.verifier,
        })
    }
    pub fn digest(&self) -> String {
        crate::sha256_hex(&serde_json::to_vec(&self.canonical()).expect("authority serializes"))
    }
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub job_id: String,
    pub owner: String,
    pub token: String,
    pub attempt: i64,
    pub epoch: i64,
    pub view: Value,
    pub authority: Authority,
    pub authority_sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    pub job_id: String,
    pub duplicate: bool,
    pub deadline_at: f64,
    pub revision: i64,
}

pub struct Submission<'a> {
    pub schema: &'a str,
    pub raw: &'a Descriptor,
    pub expected_revision: i64,
    pub deadline_seconds: f64,
    pub executable_name: &'a str,
    pub executable_sha256: &'a str,
    pub idempotency_key: Option<&'a str>,
}

fn s(row: &Row, i: usize) -> Result<String, Refusal> {
    row[i]
        .text()
        .map(str::to_string)
        .ok_or_else(|| Refusal::new("STORAGE_ERROR", format!("column {i} is not text")))
}
fn n(row: &Row, i: usize) -> Result<i64, Refusal> {
    row[i]
        .int()
        .ok_or_else(|| Refusal::new("STORAGE_ERROR", format!("column {i} is not an integer")))
}
fn f(row: &Row, i: usize) -> Result<f64, Refusal> {
    row[i]
        .float()
        .ok_or_else(|| Refusal::new("STORAGE_ERROR", format!("column {i} is not a number")))
}

const AUTHORITY_COLUMNS: &str =
    "job_id, schema_id, raw_key, raw_sha256, raw_bytes, admitted_revision, admitted_at, \
     deadline_at, executable_name, executable_sha256, verifier, authority_sha256";

fn authority_from(row: &Row) -> Result<(Authority, String), Refusal> {
    Ok((
        Authority {
            job_id: s(row, 0)?,
            schema: s(row, 1)?,
            raw: Descriptor {
                key: s(row, 2)?,
                sha256: s(row, 3)?,
                bytes: n(row, 4)? as u64,
                format: "json".into(),
            },
            admitted_revision: n(row, 5)?,
            admitted_at: f(row, 6)?,
            deadline_at: f(row, 7)?,
            executable_name: s(row, 8)?,
            executable_sha256: s(row, 9)?,
            verifier: s(row, 10)?,
        },
        s(row, 11)?,
    ))
}

pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

impl Repo {
    pub fn new(db: Db, tenant: &str, workspace: &str) -> Repo {
        Repo {
            db,
            tenant: tenant.into(),
            workspace: workspace.into(),
        }
    }

    fn scope(&self) -> [Sql; 2] {
        [self.tenant.as_str().into(), self.workspace.as_str().into()]
    }
    fn p(&self, rest: Vec<Sql>) -> Vec<Sql> {
        let mut v = self.scope().to_vec();
        v.extend(rest);
        v
    }

    fn lock(&self, tx: &mut Tx) -> Result<(), Refusal> {
        let n = tx.exec(
            "UPDATE workspaces SET revision = revision WHERE tenant = $1 AND workspace = $2",
            &self.scope(),
        )?;
        if n == 0 {
            return Err(Refusal::new(
                "NOT_INITIALIZED",
                "workspace has not been initialized",
            ));
        }
        Ok(())
    }

    fn route_in(&self, tx: &mut Tx) -> Result<(String, i64), Refusal> {
        let row = tx.one(
            "SELECT engine, epoch FROM worker_routes WHERE tenant = $1 AND workspace = $2",
            &self.scope(),
        )?;
        Ok(match row {
            Some(r) => (s(&r, 0)?, n(&r, 1)?),
            None => (ENGINE_LEGACY.into(), 0),
        })
    }

    fn is_native_workspace(&self, tx: &mut Tx) -> Result<bool, Refusal> {
        Ok(tx
            .one(
                "SELECT 1 FROM native_workspaces WHERE tenant = $1 AND workspace = $2",
                &self.scope(),
            )?
            .is_some())
    }

    pub fn route(&self) -> Result<(String, i64), Refusal> {
        self.db.write(|tx| self.route_in(tx))
    }

    pub fn head(&self) -> Result<Option<i64>, Refusal> {
        let rows = self.db.write(|tx| {
            tx.rows(
                "SELECT revision FROM workspaces WHERE tenant = $1 AND workspace = $2",
                &self.scope(),
            )
            .map_err(Refusal::from)
        })?;
        Ok(rows.first().and_then(|r| r[0].int()))
    }

    /// Creates a native-only workspace (revision 0 holds an empty native snapshot), or leaves an
    /// existing workspace untouched. Returns whether the workspace is native-initialized.
    pub fn initialize(&self, objects: &Objects, actor: &str) -> Result<bool, Refusal> {
        if self.head()?.is_some() {
            return self.db.write(|tx| self.is_native_workspace(tx));
        }
        let snapshot = json!({"schema": SNAPSHOT_SCHEMA, "revision": 0, "documents": {}});
        let d = objects.put(&serde_json::to_vec(&snapshot).expect("snapshot"), "json")?;
        let created = self.db.write(|tx| {
            let exists = tx.one(
                "SELECT 1 FROM workspaces WHERE tenant = $1 AND workspace = $2",
                &self.scope(),
            )?;
            if exists.is_some() {
                return Ok(false);
            }
            let t = now();
            tx.exec(
                "INSERT INTO workspaces (tenant, workspace, revision, owner, lease_until) VALUES ($1, $2, 0, NULL, 0)",
                &self.scope(),
            )?;
            tx.exec(
                "INSERT INTO revisions (tenant, workspace, revision, manifest, created_at, actor, reason) \
                 VALUES ($1, $2, 0, $3, $4, $5, 'initial native snapshot')",
                &self.p(vec![d.to_json().into(), t.into(), actor.into()]),
            )?;
            tx.exec(
                "INSERT INTO native_workspaces (tenant, workspace, initialized_at, actor) VALUES ($1, $2, $3, $4)",
                &self.p(vec![t.into(), actor.into()]),
            )?;
            Ok::<_, Refusal>(true)
        })?;
        if created {
            Ok(true)
        } else {
            self.db.write(|tx| self.is_native_workspace(tx))
        }
    }

    pub fn revision(&self, number: i64) -> Result<Value, Refusal> {
        let row = self.db.write(|tx| {
            tx.one(
                "SELECT revision, manifest, created_at, actor, reason FROM revisions \
                 WHERE tenant = $1 AND workspace = $2 AND revision = $3",
                &self.p(vec![number.into()]),
            )
            .map_err(Refusal::from)
        })?;
        let row = row.ok_or_else(|| Refusal::new("NOT_FOUND", "unknown revision"))?;
        Ok(json!({
            "revision": n(&row, 0)?, "manifest": row[1].json()?.unwrap_or(Value::Null),
            "created_at": f(&row, 2)?, "actor": s(&row, 3)?, "reason": s(&row, 4)?,
        }))
    }

    /// The verified native snapshot of a revision. Legacy (Python codec) workspaces refuse:
    /// their snapshot format stays owned by the Python service.
    pub fn snapshot(&self, objects: &Objects, number: i64) -> Result<Value, Refusal> {
        if !self.db.write(|tx| self.is_native_workspace(tx))? {
            return Err(Refusal::new(
                "LEGACY_WORKSPACE",
                "this workspace's snapshots are owned by the Python service",
            ));
        }
        let view = self.revision(number)?;
        let d = Descriptor::from_json(&view["manifest"])?;
        let bytes = objects.get_bounded(&d, MAX_SNAPSHOT_BYTES)?;
        let snapshot: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if snapshot["schema"] != SNAPSHOT_SCHEMA || snapshot["revision"] != number {
            return Err(Refusal::new(
                "STORAGE_ERROR",
                "native snapshot identity differs from its revision",
            ));
        }
        Ok(snapshot)
    }

    /// Commits one immutable named document as the next revision, if `expected` is still head.
    pub fn commit_document(
        &self,
        objects: &Objects,
        expected: i64,
        name: &str,
        document: &Descriptor,
        actor: &str,
    ) -> Result<i64, Refusal> {
        if !valid_name(name) {
            return Err(Refusal::new(
                "MALFORMED_INPUT",
                "document names are 1-64 of [A-Za-z0-9._-]",
            ));
        }
        let mut snapshot = self.snapshot(objects, expected)?;
        snapshot["documents"][name] = document.to_json();
        snapshot["revision"] = json!(expected + 1);
        let d = objects.put(&serde_json::to_vec(&snapshot).expect("snapshot"), "json")?;
        self.db.write(|tx| {
            self.lock(tx)?;
            let n = tx.exec(
                "UPDATE workspaces SET revision = $3 WHERE tenant = $1 AND workspace = $2 AND revision = $4",
                &self.p(vec![(expected + 1).into(), expected.into()]),
            )?;
            if n != 1 {
                return Err(Refusal::new(
                    "STALE_REVISION",
                    "saved inputs changed; reload before editing",
                ));
            }
            tx.exec(
                "INSERT INTO revisions (tenant, workspace, revision, manifest, created_at, actor, reason) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &self.p(vec![
                    (expected + 1).into(),
                    d.to_json().into(),
                    now().into(),
                    actor.into(),
                    format!("native document {name}").into(),
                ]),
            )?;
            Ok(expected + 1)
        })
    }

    /// Workspace worker cutover. Moving to Rust requires the legacy queue to be drained and no live
    /// legacy lease; moving back requires the native queue to be drained and no live native lease.
    /// Each change advances the routing epoch, which fences every lease and publication taken
    /// under an earlier epoch.
    pub fn cutover(&self, engine: &str, actor: &str) -> Result<(String, i64), Refusal> {
        if engine != ENGINE_RUST && engine != ENGINE_LEGACY {
            return Err(Refusal::new("MALFORMED_INPUT", "engine is rust or legacy"));
        }
        self.db.write(|tx| {
            self.lock(tx)?;
            let (current, epoch) = self.route_in(tx)?;
            if current == engine && epoch > 0 {
                return Ok((current, epoch));
            }
            let t = now();
            let (statuses, foreign_lease) = if engine == ENGINE_RUST {
                ("'queued', 'running'", false)
            } else {
                ("'native_queued', 'native_running'", true)
            };
            let active = tx.one(
                &format!("SELECT COUNT(*) FROM jobs WHERE tenant = $1 AND workspace = $2 AND status IN ({statuses})"),
                &self.scope(),
            )?;
            let active = active.and_then(|r| r[0].int()).unwrap_or(0);
            if active > 0 {
                return Err(Refusal::new(
                    "DRAIN_REQUIRED",
                    format!("{active} active jobs must finish or be cancelled before cutover to {engine}"),
                ));
            }
            let lease = tx
                .one(
                    "SELECT owner, lease_until FROM workspaces WHERE tenant = $1 AND workspace = $2",
                    &self.scope(),
                )?
                .expect("locked workspace");
            let owner = lease[0].text().unwrap_or("").to_string();
            let live = f(&lease, 1)? > t;
            if live && owner.starts_with("rust:") == foreign_lease {
                return Err(Refusal::new(
                    "LEASE_HELD",
                    format!("the current workspace worker `{owner}` still holds the lease; stop it first"),
                ));
            }
            let next = epoch + 1;
            if epoch == 0 {
                tx.exec(
                    "INSERT INTO worker_routes (tenant, workspace, engine, epoch, actor, changed_at) VALUES ($1, $2, $3, $4, $5, $6)",
                    &self.p(vec![engine.into(), next.into(), actor.into(), t.into()]),
                )?;
            } else {
                tx.exec(
                    "UPDATE worker_routes SET engine = $3, epoch = $4, actor = $5, changed_at = $6 WHERE tenant = $1 AND workspace = $2",
                    &self.p(vec![engine.into(), next.into(), actor.into(), t.into()]),
                )?;
            }
            Ok((engine.to_string(), next))
        })
    }

    /// Durable admission. The raw object is already stored. A repeated idempotency key returns
    /// the original job (and its original deadline) only for byte-identical input.
    pub fn admit(&self, sub: &Submission) -> Result<Admission, Refusal> {
        if !(1.0..=3_600.0).contains(&sub.deadline_seconds) {
            return Err(Refusal::new(
                "MALFORMED_INPUT",
                "deadline seconds must be within [1, 3600], the native execution bound",
            ));
        }
        let key = match sub.idempotency_key {
            Some(k)
                if k.is_empty() || k.len() > 120 || !k.bytes().all(|c| c.is_ascii_graphic()) =>
            {
                return Err(Refusal::new(
                    "MALFORMED_INPUT",
                    "idempotency keys are 1-120 printable ASCII characters",
                ))
            }
            Some(k) => Some(format!("native:{k}")),
            None => None,
        };
        self.db.write(|tx| {
            self.lock(tx)?;
            if let Some(key) = &key {
                let old = tx.one(
                    "SELECT id, kind, request, revision FROM jobs WHERE tenant = $1 AND workspace = $2 AND idempotency_key = $3",
                    &self.p(vec![key.as_str().into()]),
                )?;
                if let Some(old) = old {
                    let request = old[2].json()?.unwrap_or(Value::Null);
                    let same = old[1].text() == Some(sub.schema)
                        && request["raw"]["sha256"] == sub.raw.sha256
                        && n(&old, 3)? == sub.expected_revision;
                    if !same {
                        return Err(Refusal::new(
                            "IDEMPOTENCY_CONFLICT",
                            "idempotency key already used for a different request",
                        ));
                    }
                    return Ok(Admission {
                        job_id: s(&old, 0)?,
                        duplicate: true,
                        deadline_at: request["deadline_at"].as_f64().unwrap_or(0.),
                        revision: sub.expected_revision,
                    });
                }
            }
            let (engine, _) = self.route_in(tx)?;
            if engine != ENGINE_RUST {
                return Err(Refusal::new(
                    "NOT_ROUTED",
                    "this workspace is not routed to the Rust worker; run the cutover first",
                ));
            }
            let head = tx
                .one(
                    "SELECT revision FROM workspaces WHERE tenant = $1 AND workspace = $2",
                    &self.scope(),
                )?
                .and_then(|r| r[0].int());
            if head != Some(sub.expected_revision) {
                return Err(Refusal::new(
                    "STALE_REVISION",
                    format!("expected revision {} but head is {head:?}", sub.expected_revision),
                ));
            }
            let active = tx
                .one(
                    "SELECT COUNT(*) FROM jobs WHERE tenant = $1 AND workspace = $2 AND status IN ('native_queued', 'native_running')",
                    &self.scope(),
                )?
                .and_then(|r| r[0].int())
                .unwrap_or(0);
            if active >= MAX_QUEUE {
                return Err(Refusal::new(
                    "QUEUE_FULL",
                    "native compute queue is full; wait for a job to finish",
                ));
            }
            let t = now();
            let authority = Authority {
                job_id: uuid::Uuid::new_v4().simple().to_string(),
                schema: sub.schema.into(),
                raw: sub.raw.clone(),
                admitted_revision: sub.expected_revision,
                admitted_at: t,
                deadline_at: t + sub.deadline_seconds,
                executable_name: sub.executable_name.into(),
                executable_sha256: sub.executable_sha256.into(),
                verifier: crate::VERIFIER.into(),
            };
            let digest = authority.digest();
            let view = json!({
                "schema": sub.schema, "raw": sub.raw.to_json(), "authority_sha256": digest,
                "expected_revision": sub.expected_revision, "deadline_at": authority.deadline_at,
            });
            tx.exec(
                "INSERT INTO jobs (tenant, workspace, id, kind, revision, status, request, result, progress, detail, \
                 created_at, finished_at, owner, attempt, token, idempotency_key) \
                 VALUES ($1, $2, $3, $4, $5, 'native_queued', $6, NULL, $7, NULL, $8, NULL, NULL, 0, NULL, $9)",
                &self.p(vec![
                    authority.job_id.as_str().into(),
                    sub.schema.into(),
                    sub.expected_revision.into(),
                    view.into(),
                    json!({"stage": "queued", "pct": 0, "priority": "interactive"}).into(),
                    t.into(),
                    key.clone().into(),
                ]),
            )?;
            tx.exec(
                &format!("INSERT INTO native_job_authority (tenant, workspace, {AUTHORITY_COLUMNS}) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"),
                &self.p(vec![
                    authority.job_id.as_str().into(),
                    authority.schema.as_str().into(),
                    authority.raw.key.as_str().into(),
                    authority.raw.sha256.as_str().into(),
                    (authority.raw.bytes as i64).into(),
                    authority.admitted_revision.into(),
                    authority.admitted_at.into(),
                    authority.deadline_at.into(),
                    authority.executable_name.as_str().into(),
                    authority.executable_sha256.as_str().into(),
                    authority.verifier.as_str().into(),
                    digest.as_str().into(),
                ]),
            )?;
            Ok(Admission {
                job_id: authority.job_id,
                duplicate: false,
                deadline_at: authority.deadline_at,
                revision: sub.expected_revision,
            })
        })
    }

    fn authority_in(&self, tx: &mut Tx, id: &str) -> Result<Option<(Authority, String)>, Refusal> {
        let row = tx.one(
            &format!("SELECT {AUTHORITY_COLUMNS} FROM native_job_authority WHERE tenant = $1 AND workspace = $2 AND job_id = $3"),
            &self.p(vec![id.into()]),
        )?;
        row.map(|r| authority_from(&r)).transpose()
    }

    /// A freshly built copy of a native job's state. Nothing returned here is read back as
    /// authority: the worker re-reads the immutable authority row.
    pub fn job(&self, id: &str) -> Result<Value, Refusal> {
        self.db.write(|tx| {
            let (authority, digest) = self
                .authority_in(tx, id)?
                .ok_or_else(|| Refusal::new("NOT_FOUND", "unknown native job"))?;
            let row = tx
                .one(
                    "SELECT id, kind, status, revision, attempt, created_at, finished_at, detail, progress, result \
                     FROM jobs WHERE tenant = $1 AND workspace = $2 AND id = $3",
                    &self.p(vec![id.into()]),
                )?
                .ok_or_else(|| Refusal::new("NOT_FOUND", "unknown native job"))?;
            let published = tx
                .one(
                    "SELECT attempt FROM native_publications WHERE tenant = $1 AND workspace = $2 AND job_id = $3",
                    &self.p(vec![id.into()]),
                )?
                .is_some();
            Ok(json!({
                "id": s(&row, 0)?, "kind": s(&row, 1)?, "status": s(&row, 2)?, "revision": n(&row, 3)?,
                "attempt": n(&row, 4)?, "created_at": f(&row, 5)?, "finished_at": row[6].float(),
                "detail": row[7].text(), "progress": row[8].json()?, "result": row[9].json()?,
                "published": published,
                "authority": {
                    "schema": authority.schema, "raw": authority.raw.to_json(),
                    "admitted_revision": authority.admitted_revision, "admitted_at": authority.admitted_at,
                    "deadline_at": authority.deadline_at, "executable_name": authority.executable_name,
                    "executable_sha256": authority.executable_sha256, "verifier": authority.verifier,
                    "authority_sha256": digest,
                },
            }))
        })
    }

    pub fn list_jobs(&self, offset: i64, limit: i64) -> Result<Value, Refusal> {
        if offset < 0 || !(1..=1000).contains(&limit) {
            return Err(Refusal::new("MALFORMED_INPUT", "invalid job page"));
        }
        let rows = self.db.write(|tx| {
            tx.rows(
                "SELECT j.id, j.kind, j.status, j.revision, j.attempt, j.created_at, j.finished_at FROM jobs j \
                 JOIN native_job_authority a ON a.tenant = j.tenant AND a.workspace = j.workspace AND a.job_id = j.id \
                 WHERE j.tenant = $1 AND j.workspace = $2 ORDER BY j.created_at DESC, j.id DESC LIMIT $3 OFFSET $4",
                &self.p(vec![limit.into(), offset.into()]),
            )
            .map_err(Refusal::from)
        })?;
        rows.iter()
            .map(|r| {
                Ok(json!({"id": s(r, 0)?, "kind": s(r, 1)?, "status": s(r, 2)?, "revision": n(r, 3)?,
                    "attempt": n(r, 4)?, "created_at": f(r, 5)?, "finished_at": r[6].float()}))
            })
            .collect::<Result<Vec<_>, Refusal>>()
            .map(Value::Array)
    }

    pub fn cancel(&self, id: &str) -> Result<bool, Refusal> {
        self.db.write(|tx| {
            self.lock(tx)?;
            if self.authority_in(tx, id)?.is_none() {
                return Err(Refusal::new("NOT_FOUND", "unknown native job"));
            }
            let n = tx.exec(
                "UPDATE jobs SET status = 'native_cancelled', token = NULL, finished_at = $4, \
                 detail = 'cancelled by user; in-flight native output will be discarded' \
                 WHERE tenant = $1 AND workspace = $2 AND id = $3 AND status IN ('native_queued', 'native_running')",
                &self.p(vec![id.into(), now().into()]),
            )?;
            Ok(n == 1)
        })
    }

    /// Takes or renews the workspace lease, only while the workspace is routed to Rust.
    pub fn acquire(&self, owner: &str, seconds: f64) -> Result<Option<i64>, Refusal> {
        self.db.write(|tx| {
            let (engine, epoch) = self.route_in(tx)?;
            if engine != ENGINE_RUST {
                return Ok(None);
            }
            let t = now();
            let n = tx.exec(
                "UPDATE workspaces SET owner = $3, lease_until = $4 WHERE tenant = $1 AND workspace = $2 \
                 AND (lease_until < $5 OR owner = $6)",
                &self.p(vec![owner.into(), (t + seconds).into(), t.into(), owner.into()]),
            )?;
            Ok((n == 1).then_some(epoch))
        })
    }

    pub fn renew(&self, owner: &str, epoch: i64, seconds: f64) -> Result<bool, Refusal> {
        self.db.write(|tx| {
            if self.route_in(tx)? != (ENGINE_RUST.to_string(), epoch) {
                return Ok(false);
            }
            let t = now();
            Ok(tx.exec(
                "UPDATE workspaces SET lease_until = $3 WHERE tenant = $1 AND workspace = $2 AND owner = $4 AND lease_until > $5",
                &self.p(vec![(t + seconds).into(), owner.into(), t.into()]),
            )? == 1)
        })
    }

    pub fn release(&self, owner: &str) -> Result<(), Refusal> {
        self.db.write(|tx| {
            tx.exec(
                "UPDATE workspaces SET lease_until = 0 WHERE tenant = $1 AND workspace = $2 AND owner = $3",
                &self.p(vec![owner.into()]),
            )?;
            Ok(())
        })
    }

    fn lease_valid(&self, tx: &mut Tx, owner: &str, epoch: i64) -> Result<(), Refusal> {
        let valid = tx
            .one(
                "SELECT 1 FROM workspaces WHERE tenant = $1 AND workspace = $2 AND owner = $3 AND lease_until > $4",
                &self.p(vec![owner.into(), now().into()]),
            )?
            .is_some();
        if !valid {
            return Err(Refusal::new(
                "LEASE_LOST",
                "worker lease lost; result discarded",
            ));
        }
        if self.route_in(tx)? != (ENGINE_RUST.to_string(), epoch) {
            return Err(Refusal::new(
                "ROUTE_CHANGED",
                "workspace routing changed; result discarded",
            ));
        }
        Ok(())
    }

    fn finish(&self, tx: &mut Tx, id: &str, status: &str, detail: &str) -> Result<(), Refusal> {
        tx.exec(
            "UPDATE jobs SET status = $4, detail = $5, token = NULL, finished_at = $6 WHERE tenant = $1 AND workspace = $2 AND id = $3",
            &self.p(vec![id.into(), status.into(), detail.into(), now().into()]),
        )?;
        Ok(())
    }

    /// Oldest native job first. A new lease holder requeues work a previous holder left running;
    /// that attempt's token can no longer publish. Jobs past their original deadline, or past the
    /// attempt bound, fail here without running again.
    pub fn claim(&self, owner: &str, epoch: i64) -> Result<Option<Claim>, Refusal> {
        self.db.write(|tx| {
            self.lock(tx)?;
            self.lease_valid(tx, owner, epoch)?;
            tx.exec(
                "UPDATE jobs SET status = 'native_queued', owner = NULL, token = NULL WHERE tenant = $1 AND workspace = $2 \
                 AND status = 'native_running' AND (owner IS NULL OR owner <> $3)",
                &self.p(vec![owner.into()]),
            )?;
            let queued = tx.rows(
                "SELECT id, attempt, request FROM jobs WHERE tenant = $1 AND workspace = $2 AND status = 'native_queued' \
                 ORDER BY created_at, id",
                &self.scope(),
            )?;
            for row in queued {
                let id = s(&row, 0)?;
                let attempt = n(&row, 1)?;
                let Some((authority, digest)) = self.authority_in(tx, &id)? else {
                    self.finish(tx, &id, ERROR, "AUTHORITY_MISSING: native job has no admission record")?;
                    continue;
                };
                if now() >= authority.deadline_at {
                    self.finish(tx, &id, ERROR, "DEADLINE_EXPIRED: the original deadline passed before execution")?;
                    continue;
                }
                if attempt >= MAX_ATTEMPTS {
                    self.finish(tx, &id, ERROR, "ATTEMPTS_EXHAUSTED: worker interrupted three attempts")?;
                    continue;
                }
                let token = uuid::Uuid::new_v4().simple().to_string();
                tx.exec(
                    "UPDATE jobs SET owner = $4, token = $5, attempt = $6, status = 'native_running' \
                     WHERE tenant = $1 AND workspace = $2 AND id = $3",
                    &self.p(vec![id.as_str().into(), owner.into(), token.as_str().into(), (attempt + 1).into()]),
                )?;
                return Ok(Some(Claim {
                    job_id: id,
                    owner: owner.into(),
                    token,
                    attempt: attempt + 1,
                    epoch,
                    view: row[2].json()?.unwrap_or(Value::Null),
                    authority,
                    authority_sha256: digest,
                }));
            }
            Ok(None)
        })
    }

    pub fn progress(&self, claim: &Claim, progress: Value) -> Result<(), Refusal> {
        self.db.write(|tx| {
            tx.exec(
                "UPDATE jobs SET progress = $5 WHERE tenant = $1 AND workspace = $2 AND id = $3 AND token = $4 \
                 AND status = 'native_running'",
                &self.p(vec![claim.job_id.as_str().into(), claim.token.as_str().into(), progress.into()]),
            )?;
            Ok(())
        })
    }

    /// Whether this attempt may still run: its token is current and the job is not cancelled.
    pub fn current(&self, claim: &Claim) -> Result<bool, Refusal> {
        self.db.write(|tx| {
            Ok(tx
                .one(
                    "SELECT 1 FROM jobs WHERE tenant = $1 AND workspace = $2 AND id = $3 AND token = $4 AND status = 'native_running'",
                    &self.p(vec![claim.job_id.as_str().into(), claim.token.as_str().into()]),
                )?
                .is_some())
        })
    }

    /// Records a refusal for this attempt, under the same lease/token fences as publication.
    pub fn fail(&self, claim: &Claim, detail: &str) -> Result<(), Refusal> {
        self.db.write(|tx| {
            self.lock(tx)?;
            self.lease_valid(tx, &claim.owner, claim.epoch)?;
            let n = tx.exec(
                "UPDATE jobs SET status = 'native_error', detail = $5, token = NULL, finished_at = $6 \
                 WHERE tenant = $1 AND workspace = $2 AND id = $3 AND token = $4 AND status = 'native_running'",
                &self.p(vec![
                    claim.job_id.as_str().into(),
                    claim.token.as_str().into(),
                    detail.into(),
                    now().into(),
                ]),
            )?;
            if n != 1 {
                return Err(Refusal::new("SUPERSEDED", "job cancelled or superseded"));
            }
            Ok(())
        })
    }

    /// The final publication transaction. The manifest and every object it references are
    /// already written and reread. Nothing is published unless, under the workspace lock, the
    /// lease is live and owned, the routing epoch is unchanged, the immutable authority row is
    /// unchanged, the attempt token is current (not cancelled or superseded) and the original
    /// deadline has not passed.
    pub fn publish(
        &self,
        claim: &Claim,
        manifest: &Descriptor,
        result: Value,
    ) -> Result<(), Refusal> {
        self.db.write(|tx| {
            self.lock(tx)?;
            self.lease_valid(tx, &claim.owner, claim.epoch)?;
            let (authority, digest) = self
                .authority_in(tx, &claim.job_id)?
                .ok_or_else(|| Refusal::new("AUTHORITY_CHANGED", "admission record missing"))?;
            if authority != claim.authority
                || digest != claim.authority_sha256
                || authority.digest() != digest
            {
                return Err(Refusal::new(
                    "AUTHORITY_CHANGED",
                    "admission record changed after the claim",
                ));
            }
            let current = tx
                .one(
                    "SELECT 1 FROM jobs WHERE tenant = $1 AND workspace = $2 AND id = $3 AND token = $4 AND status = 'native_running'",
                    &self.p(vec![claim.job_id.as_str().into(), claim.token.as_str().into()]),
                )?
                .is_some();
            if !current {
                return Err(Refusal::new(
                    "SUPERSEDED",
                    "job cancelled or superseded; result discarded",
                ));
            }
            let t = now();
            if t >= authority.deadline_at {
                return Err(Refusal::new(
                    "DEADLINE_EXPIRED",
                    "the original deadline passed before publication",
                ));
            }
            tx.exec(
                "INSERT INTO native_publications (tenant, workspace, job_id, manifest, manifest_sha256, attempt, created_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &self.p(vec![
                    claim.job_id.as_str().into(),
                    manifest.to_json().into(),
                    manifest.sha256.as_str().into(),
                    claim.attempt.into(),
                    t.into(),
                ]),
            )?;
            let n = tx.exec(
                "UPDATE jobs SET status = 'native_done', result = $5, progress = $6, finished_at = $7 \
                 WHERE tenant = $1 AND workspace = $2 AND id = $3 AND token = $4 AND status = 'native_running'",
                &self.p(vec![
                    claim.job_id.as_str().into(),
                    claim.token.as_str().into(),
                    result.into(),
                    json!({"stage": "done", "pct": 100}).into(),
                    t.into(),
                ]),
            )?;
            if n != 1 {
                return Err(Refusal::new("SUPERSEDED", "job changed during publication"));
            }
            Ok(())
        })
    }

    /// The completed manifest descriptor, if (and only if) the final transaction committed.
    pub fn publication(&self, id: &str) -> Result<Option<(Descriptor, i64)>, Refusal> {
        let row = self.db.write(|tx| {
            tx.one(
                "SELECT manifest, manifest_sha256, attempt FROM native_publications WHERE tenant = $1 AND workspace = $2 AND job_id = $3",
                &self.p(vec![id.into()]),
            )
            .map_err(Refusal::from)
        })?;
        row.map(|r| {
            let d = Descriptor::from_json(&r[0].json()?.unwrap_or(Value::Null))?;
            if r[1].text() != Some(d.sha256.as_str()) {
                return Err(Refusal::new("STORAGE_ERROR", "publication receipt differs"));
            }
            Ok((d, n(&r, 2)?))
        })
        .transpose()
    }

    pub fn count_status(&self, status: &str) -> Result<i64, Refusal> {
        let rows = self.db.write(|tx| {
            tx.rows(
                "SELECT COUNT(*) FROM jobs WHERE tenant = $1 AND workspace = $2 AND status = $3",
                &self.p(vec![status.into()]),
            )
            .map_err(Refusal::from)
        })?;
        Ok(rows.first().and_then(|r| r[0].int()).unwrap_or(0))
    }
}
