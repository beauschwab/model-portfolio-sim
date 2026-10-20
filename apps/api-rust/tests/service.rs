//! End-to-end process tests of admission, revisions, idempotency, routing/cutover, authority
//! and verified publication, on SQLite and (with TEST_POSTGRES_URL) PostgreSQL.
mod common;
use common::*;
use serde_json::{json, Value};
use std::time::Duration;
use workbench_native::sql::Sql;

fn job(api: &Api, id: &str) -> Value {
    api.get(&format!("/native/jobs/{id}")).body
}

#[test]
fn a_fresh_native_environment_admits_runs_verifies_and_pages_without_python() {
    for backend in backends() {
        let env = Env::new(backend, "e2e");
        env.ready();
        let api = env.api();
        let health = api.get("/native/health");
        assert_eq!(health.status, 200);
        assert_eq!(health.body["engine"], "rust");
        assert_eq!(api.get("/native/revisions/head").body["revision"], 0);

        // Immutable revision commits compare the expected revision.
        let put = api.call(
            "PUT",
            "/native/documents/unwind",
            &[("If-Match", "0")],
            REQUEST.as_bytes(),
        );
        assert_eq!(put.status, 201, "{:?}", put.body);
        assert_eq!(put.body["revision"], 1);
        let stale = api.call(
            "PUT",
            "/native/documents/again",
            &[("If-Match", "0")],
            REQUEST.as_bytes(),
        );
        assert_eq!(
            (stale.status, stale.body["code"].as_str()),
            (409, Some("STALE_REVISION"))
        );
        let snapshot = api.get("/native/revisions/1/snapshot").body;
        assert_eq!(snapshot["documents"]["unwind"]["format"], "json");

        // Admission: stale revision, idempotent duplicates, conflicting reuse, document runs.
        let stale = api.submit(0, Some("k1"), REQUEST.as_bytes(), None);
        assert_eq!(
            (stale.status, stale.body["code"].as_str()),
            (409, Some("STALE_REVISION"))
        );
        let first = api.submit(1, Some("k1"), REQUEST.as_bytes(), None);
        assert_eq!(first.status, 202, "{:?}", first.body);
        let id = first.body["job_id"].as_str().unwrap().to_string();
        let again = api.submit(1, Some("k1"), REQUEST.as_bytes(), None);
        assert_eq!(
            (
                again.body["job_id"].as_str(),
                again.body["duplicate"].as_bool()
            ),
            (Some(id.as_str()), Some(true))
        );
        assert_eq!(again.body["deadline_at"], first.body["deadline_at"]);
        let other = REQUEST.replacen("\"schema\"", " \"schema\"", 1);
        let conflict = api.submit(1, Some("k1"), other.as_bytes(), None);
        assert_eq!(
            (conflict.status, conflict.body["code"].as_str()),
            (409, Some("IDEMPOTENCY_CONFLICT"))
        );
        let from_doc = api.call(
            "POST",
            "/native/lifecycle/runs?document=unwind",
            &[("X-Expected-Revision", "1"), ("Idempotency-Key", "doc")],
            b"",
        );
        assert_eq!(from_doc.status, 202, "{:?}", from_doc.body);
        let doc_id = from_doc.body["job_id"].as_str().unwrap().to_string();

        // An API restart keeps durable admission: the same key returns the same job.
        drop(api);
        let api = env.api();
        assert_eq!(
            api.submit(1, Some("k1"), REQUEST.as_bytes(), None).body["job_id"],
            id.as_str()
        );

        let log = finish(env.worker(&[]), Duration::from_secs(120));
        for jid in [&id, &doc_id] {
            let view = job(&api, jid);
            assert_eq!(view["status"], "native_done", "{log}\n{view}");
            assert_eq!(view["published"], true);
            assert_eq!(view["attempt"], 1);
            assert_eq!(view["result"]["persisted_verification"], true);
            assert_eq!(view["result"]["receipt"]["verified"], true);
            assert_eq!(
                view["result"]["receipt"]["production_financial_crates"],
                false
            );
        }
        let manifest = api.get(&format!("/native/jobs/{id}/manifest")).body;
        assert_eq!(manifest["schema"], "native-lifecycle-publication-1");
        assert_eq!(
            manifest["source"]["raw"],
            job(&api, &id)["authority"]["raw"]
        );
        assert_eq!(
            manifest["producer"]["executable_name"],
            job(&api, &id)["authority"]["executable_name"]
        );
        let journal = api
            .get(&format!(
                "/native/jobs/{id}/tables/journal?offset=3&limit=5"
            ))
            .body;
        assert_eq!(journal["rows"].as_array().unwrap().len(), 5);
        assert!(journal["rows"][0]["gl_account"].is_string(), "{journal}");
        assert_eq!(journal["total"], 172);
        let tb = api
            .get(&format!(
                "/native/jobs/{id}/tables/trial_balance?offset=20&limit=100"
            ))
            .body;
        assert_eq!(tb["rows"].as_array().unwrap().len(), 4);
        assert_eq!(env.publications(), 2);
        let list = api.get("/native/jobs?limit=10").body;
        assert_eq!(list.as_array().unwrap().len(), 2);
    }
}

#[test]
fn routing_and_cutover_fence_the_legacy_queue_and_lease() {
    for backend in backends() {
        let env = Env::new(backend, "route");
        env.admin(&["migrate"]).unwrap();
        env.admin(&["init"]).unwrap();
        let api = env.api();
        // Legacy routing: admission refuses and a Rust worker never takes the lease.
        let refused = api.submit(0, None, REQUEST.as_bytes(), None);
        assert_eq!(refused.body["code"], "NOT_ROUTED");
        let idle = finish(env.worker(&[]), Duration::from_secs(60));
        let owner = env.rows(
            "SELECT owner FROM workspaces WHERE tenant = $1 AND workspace = $2",
            &env.scope(),
        );
        assert_eq!(owner[0][0], Sql::Null, "{idle}");

        // A queued legacy job and a live legacy lease each block cutover until drained.
        let mut legacy = env.scope();
        legacy.extend([Sql::Float(workbench_native::now())]);
        env.exec(
            "INSERT INTO jobs (tenant, workspace, id, kind, revision, status, request, created_at, attempt) \
             VALUES ($1, $2, 'legacy-1', 'run', 0, 'queued', '{}', $3, 0)",
            &legacy,
        );
        assert!(env
            .admin(&["cutover", "--to", "rust"])
            .unwrap_err()
            .contains("DRAIN_REQUIRED"));
        env.exec("UPDATE jobs SET status = 'done' WHERE tenant = $1 AND workspace = $2 AND id = 'legacy-1'", &env.scope());
        let mut lease = env.scope();
        lease.push(Sql::Float(workbench_native::now() + 600.));
        env.exec("UPDATE workspaces SET owner = 'legacy-worker', lease_until = $3 WHERE tenant = $1 AND workspace = $2", &lease);
        assert!(env
            .admin(&["cutover", "--to", "rust"])
            .unwrap_err()
            .contains("LEASE_HELD"));
        env.exec(
            "UPDATE workspaces SET lease_until = 0 WHERE tenant = $1 AND workspace = $2",
            &env.scope(),
        );
        assert_eq!(env.admin(&["cutover", "--to", "rust"]).unwrap()["epoch"], 1);

        // Native jobs use statuses the legacy oldest-job claim (status = 'queued') never selects.
        let id = api.submit(0, None, REQUEST.as_bytes(), None).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut q = env.scope();
        q.push(id.as_str().into());
        assert_eq!(
            env.rows(
                "SELECT status FROM jobs WHERE tenant = $1 AND workspace = $2 AND id = $3",
                &q
            )[0][0]
                .text(),
            Some("native_queued")
        );
        let legacy_visible = env.rows(
            "SELECT COUNT(*) FROM jobs WHERE tenant = $1 AND workspace = $2 AND status IN ('queued', 'running')",
            &env.scope(),
        );
        assert_eq!(legacy_visible[0][0].int(), Some(0));
        // Returning to legacy needs the native queue drained, then advances the epoch again.
        assert!(env
            .admin(&["cutover", "--to", "legacy"])
            .unwrap_err()
            .contains("DRAIN_REQUIRED"));
        finish(env.worker(&[]), Duration::from_secs(120));
        assert_eq!(job(&api, &id)["status"], "native_done");
        assert_eq!(
            env.admin(&["cutover", "--to", "legacy"]).unwrap()["epoch"],
            2
        );
        assert_eq!(
            api.submit(0, None, REQUEST.as_bytes(), None).body["code"],
            "NOT_ROUTED"
        );
    }
}

#[test]
fn mutable_job_views_and_authority_rows_cannot_rebase_execution() {
    for backend in backends() {
        let env = Env::new(backend, "authority");
        env.ready();
        let api = env.api();
        // A rebased view: point the job's request at different stored raw bytes.
        let a = api.submit(0, None, REQUEST.as_bytes(), None).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let altered = REQUEST.replacen("\"schema\"", "  \"schema\"", 1);
        let put = api.call(
            "PUT",
            "/native/documents/other",
            &[("If-Match", "0")],
            altered.as_bytes(),
        );
        assert_eq!(put.status, 201);
        let other = api.get("/native/revisions/1/snapshot").body["documents"]["other"].clone();
        let mut p = env.scope();
        p.extend([Sql::Json(json!({"schema": "liquidity-lifecycle-1", "raw": other,
            "authority_sha256": job(&api, &a)["authority"]["authority_sha256"], "expected_revision": 0,
            "deadline_at": job(&api, &a)["authority"]["deadline_at"]})), a.as_str().into()]);
        env.exec(
            "UPDATE jobs SET request = $3 WHERE tenant = $1 AND workspace = $2 AND id = $4",
            &p,
        );
        // A rebased authority row: extend the original deadline in place.
        let b = api.submit(1, None, REQUEST.as_bytes(), None).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut p = env.scope();
        p.push(b.as_str().into());
        env.exec(
            "UPDATE native_job_authority SET deadline_at = deadline_at + 3600 WHERE tenant = $1 AND workspace = $2 AND job_id = $3",
            &p,
        );
        let log = finish(env.worker(&[]), Duration::from_secs(120));
        for id in [&a, &b] {
            let view = job(&api, id);
            assert_eq!(view["status"], "native_error", "{log}");
            assert!(
                view["detail"]
                    .as_str()
                    .unwrap()
                    .starts_with("AUTHORITY_MISMATCH"),
                "{view}"
            );
            assert_eq!(view["published"], false);
        }
        assert_eq!(env.publications(), 0);
    }
}

#[test]
fn replaced_executables_and_refused_verification_publish_nothing() {
    for backend in backends() {
        let env = Env::new(backend, "identity");
        env.ready();
        let api = env.api();
        let changed = api.submit(0, None, REQUEST.as_bytes(), None).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let mut bytes = std::fs::read(&env.ledger).unwrap();
        bytes.extend_from_slice(b"replaced");
        std::fs::write(&env.ledger, &bytes).unwrap();
        finish(env.worker(&[]), Duration::from_secs(120));
        let view = job(&api, &changed);
        assert!(
            view["detail"]
                .as_str()
                .unwrap()
                .starts_with("EXECUTABLE_CHANGED"),
            "{view}"
        );

        let tampered = api.submit(0, None, REQUEST.as_bytes(), None).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        finish(
            env.worker(&[("STUB_TAMPER", "1")]),
            Duration::from_secs(120),
        );
        let view = job(&api, &tampered);
        assert_eq!(view["status"], "native_error");
        assert!(
            view["detail"]
                .as_str()
                .unwrap()
                .starts_with("VERIFICATION_REFUSED"),
            "{view}"
        );
        assert_eq!(env.publications(), 0);
    }
}

#[test]
fn admission_refuses_cross_origin_unauthenticated_oversized_and_malformed_requests() {
    let env = Env::new(Backend::Sqlite, "guard");
    env.ready();
    let api = env.api();
    let r = api.call(
        "GET",
        "/native/health",
        &[("Origin", "https://evil.example")],
        b"",
    );
    assert_eq!(
        (r.status, r.body["code"].as_str()),
        (403, Some("CROSS_ORIGIN"))
    );
    let r = api.call(
        "GET",
        "/native/health",
        &[("Sec-Fetch-Site", "cross-site")],
        b"",
    );
    assert_eq!(r.status, 403);
    let r = api.call(
        "GET",
        "/native/health",
        &[("Authorization", "Bearer wrong")],
        b"",
    );
    assert_eq!(
        (r.status, r.body["code"].as_str()),
        (401, Some("UNAUTHORIZED"))
    );
    let big = vec![b' '; 8 * 1024 * 1024 + 1];
    assert_eq!(api.submit(0, None, &big, None).status, 413);
    let duplicate = REQUEST.replacen(
        "\"schema\": \"liquidity-lifecycle-1\",",
        "\"schema\": \"liquidity-lifecycle-1\", \"schema\": \"x\",",
        1,
    );
    assert_eq!(
        api.submit(0, None, duplicate.as_bytes(), None).body["code"],
        "MALFORMED_INPUT"
    );
    let wrong = REQUEST.replacen("liquidity-lifecycle-1", "hedge-accounting-1", 1);
    assert_eq!(api.submit(0, None, wrong.as_bytes(), None).status, 422);
    assert_eq!(
        api.submit(0, None, REQUEST.as_bytes(), Some("0")).body["code"],
        "MALFORMED_INPUT"
    );
    assert_eq!(api.get("/native/jobs/missing").status, 404);
}
