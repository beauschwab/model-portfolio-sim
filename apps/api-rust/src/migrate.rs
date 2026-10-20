//! Schema ownership. The shared tables (workspaces, revisions, jobs, sessions, libraries,
//! research snapshots, catalog publications and `schema_version`) keep the exact definitions the
//! SQLAlchemy metadata in `apps/api/app/repository.py` creates, so a fresh environment can be
//! initialized without Python and an existing database is reused unchanged. Rust-owned tables
//! live in an explicit, versioned ledger (`native_migrations`); every applied migration records
//! the SHA-256 of its statements and a binary refuses a database whose ledger differs from what it
//! was compiled with, or that a newer binary has already advanced.
use crate::sql::{Db, Dialect, Sql, Tx};
use sha2::{Digest, Sha256};

/// The shared legacy schema version this binary reads and writes (`repository.py` version 2).
pub const SHARED_SCHEMA_VERSION: i64 = 2;

const SHARED: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL, PRIMARY KEY (version))",
    "CREATE TABLE IF NOT EXISTS workspaces (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     revision INTEGER NOT NULL, owner VARCHAR(64), lease_until FLOAT NOT NULL, PRIMARY KEY (tenant, workspace))",
    "CREATE TABLE IF NOT EXISTS revisions (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     revision INTEGER NOT NULL, manifest JSON NOT NULL, created_at FLOAT NOT NULL, actor VARCHAR(200) NOT NULL, \
     reason VARCHAR(200) NOT NULL, PRIMARY KEY (tenant, workspace, revision), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
    "CREATE TABLE IF NOT EXISTS jobs (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     id VARCHAR(64) NOT NULL, kind VARCHAR(64) NOT NULL, revision INTEGER NOT NULL, status VARCHAR(20) NOT NULL, \
     request JSON NOT NULL, result JSON, progress JSON, detail TEXT, created_at FLOAT NOT NULL, finished_at FLOAT, \
     owner VARCHAR(64), attempt INTEGER NOT NULL, token VARCHAR(64), idempotency_key VARCHAR(128), \
     PRIMARY KEY (tenant, workspace, id), UNIQUE (tenant, workspace, idempotency_key), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
    "CREATE INDEX IF NOT EXISTS jobs_queue ON jobs (tenant, workspace, status, created_at)",
    "CREATE TABLE IF NOT EXISTS sessions (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     id VARCHAR(64) NOT NULL, revision INTEGER NOT NULL, manifest JSON NOT NULL, version INTEGER NOT NULL, \
     closed BOOLEAN NOT NULL, PRIMARY KEY (tenant, workspace, id), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
    "CREATE TABLE IF NOT EXISTS libraries (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     revision INTEGER NOT NULL, manifest JSON NOT NULL, PRIMARY KEY (tenant, workspace, revision), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
    "CREATE TABLE IF NOT EXISTS research_snapshots (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     id VARCHAR(64) NOT NULL, manifest JSON NOT NULL, created_at FLOAT NOT NULL, PRIMARY KEY (tenant, workspace, id), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
    "CREATE TABLE IF NOT EXISTS catalog_publications (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
     job_id VARCHAR(64) NOT NULL, destination VARCHAR(64) NOT NULL, manifest JSON NOT NULL, created_at FLOAT NOT NULL, \
     PRIMARY KEY (tenant, workspace, job_id, destination), \
     FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
];

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub statements: &'static [&'static str],
}

/// Rust-owned additive migrations, applied in order, each in its own transaction.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "worker-routes",
        statements: &[
            // Absent row = legacy routing; the Python worker keeps the workspace until cutover.
            "CREATE TABLE worker_routes (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
             engine VARCHAR(16) NOT NULL, epoch INTEGER NOT NULL, actor VARCHAR(200) NOT NULL, changed_at FLOAT NOT NULL, \
             PRIMARY KEY (tenant, workspace), FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
            "CREATE TABLE native_workspaces (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
             initialized_at FLOAT NOT NULL, actor VARCHAR(200) NOT NULL, PRIMARY KEY (tenant, workspace), \
             FOREIGN KEY(tenant, workspace) REFERENCES workspaces (tenant, workspace))",
        ],
    },
    Migration {
        version: 2,
        name: "native-job-authority",
        statements: &[
            // Written once at admission and never updated: the worker reads raw input, executable and
            // verifier identity and the original deadline from here, never from a mutable job view.
            "CREATE TABLE native_job_authority (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
             job_id VARCHAR(64) NOT NULL, schema_id VARCHAR(64) NOT NULL, raw_key VARCHAR(256) NOT NULL, \
             raw_sha256 VARCHAR(64) NOT NULL, raw_bytes INTEGER NOT NULL, admitted_revision INTEGER NOT NULL, \
             admitted_at FLOAT NOT NULL, deadline_at FLOAT NOT NULL, executable_name VARCHAR(128) NOT NULL, \
             executable_sha256 VARCHAR(64) NOT NULL, verifier VARCHAR(128) NOT NULL, authority_sha256 VARCHAR(64) NOT NULL, \
             PRIMARY KEY (tenant, workspace, job_id), \
             FOREIGN KEY(tenant, workspace, job_id) REFERENCES jobs (tenant, workspace, id))",
        ],
    },
    Migration {
        version: 3,
        name: "native-publications",
        statements: &[
            // A row exists only after the final fenced transaction; it is the completed manifest.
            "CREATE TABLE native_publications (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, \
             job_id VARCHAR(64) NOT NULL, manifest JSON NOT NULL, manifest_sha256 VARCHAR(64) NOT NULL, \
             attempt INTEGER NOT NULL, created_at FLOAT NOT NULL, PRIMARY KEY (tenant, workspace, job_id), \
             FOREIGN KEY(tenant, workspace, job_id) REFERENCES jobs (tenant, workspace, id))",
        ],
    },
];

pub fn checksum(m: &Migration) -> String {
    let mut h = Sha256::new();
    h.update(m.name.as_bytes());
    for s in m.statements {
        h.update([0u8]);
        h.update(s.as_bytes());
    }
    crate::hex(&h.finalize())
}

fn serialize(tx: &mut Tx) -> Result<(), String> {
    // Concurrent first starts must not race `CREATE TABLE IF NOT EXISTS` on PostgreSQL. SQLite
    // already holds its write lock from `BEGIN IMMEDIATE`.
    if tx.dialect() == Dialect::Postgres {
        tx.batch("SELECT pg_advisory_xact_lock(4747004747)")?;
    }
    Ok(())
}

/// Brings the database to the schema this binary was built for, or refuses.
pub fn run(db: &Db) -> Result<i64, String> {
    db.write(|tx| {
        serialize(tx)?;
        for statement in SHARED {
            tx.batch(statement)?;
        }
        let versions: Vec<i64> = tx
            .rows("SELECT version FROM schema_version", &[])?
            .into_iter()
            .filter_map(|r| r[0].int())
            .collect();
        match versions.as_slice() {
            [] => {
                tx.exec(
                    "INSERT INTO schema_version (version) VALUES ($1)",
                    &[Sql::Int(SHARED_SCHEMA_VERSION)],
                )?;
            }
            [1] => {
                // Version 2 only added catalog_publications, created above.
                tx.exec("UPDATE schema_version SET version = $1 WHERE version = 1", &[Sql::Int(2)])?;
            }
            [2] => {}
            other => return Err(format!("unsupported shared schema version {other:?}")),
        }
        tx.batch(
            "CREATE TABLE IF NOT EXISTS native_migrations (version INTEGER NOT NULL, name VARCHAR(128) NOT NULL, \
             sha256 VARCHAR(64) NOT NULL, applied_at FLOAT NOT NULL, PRIMARY KEY (version))",
        )?;
        Ok(())
    })?;
    let mut applied = 0;
    for m in MIGRATIONS {
        db.write(|tx| {
            serialize(tx)?;
            let recorded = tx.one(
                "SELECT name, sha256 FROM native_migrations WHERE version = $1",
                &[Sql::Int(m.version)],
            )?;
            match recorded {
                Some(row) => {
                    if row[0].text() != Some(m.name) || row[1].text() != Some(checksum(m).as_str()) {
                        return Err(format!(
                            "native migration {} differs from the recorded migration; refuse to run",
                            m.version
                        ));
                    }
                }
                None => {
                    for s in m.statements {
                        tx.batch(s)?;
                    }
                    tx.exec(
                        "INSERT INTO native_migrations (version, name, sha256, applied_at) VALUES ($1, $2, $3, $4)",
                        &[Sql::Int(m.version), m.name.into(), checksum(m).into(), Sql::Float(crate::now())],
                    )?;
                    applied += 1;
                }
            }
            Ok(())
        })?;
    }
    let newest = db.read("SELECT MAX(version) FROM native_migrations", &[])?;
    let newest = newest.first().and_then(|r| r[0].int()).unwrap_or(0);
    let compiled = MIGRATIONS.last().map_or(0, |m| m.version);
    if newest > compiled {
        return Err(format!(
            "database native schema {newest} is newer than this binary ({compiled})"
        ));
    }
    Ok(applied)
}
