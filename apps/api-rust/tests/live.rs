//! The real native producer through the whole service: set `WORKBENCH_TEST_LEDGER_EXE` to a
//! built `portfolio-model-ledger` (Decision crate release build). Skipped when unset, because that
//! binary needs the HiGHS toolchain; CI builds it in its own job.
mod common;
use common::*;
use std::time::Duration;

#[test]
fn the_live_native_producer_is_verified_and_published_by_the_rust_service() {
    let Ok(exe) = std::env::var("WORKBENCH_TEST_LEDGER_EXE") else {
        eprintln!("WORKBENCH_TEST_LEDGER_EXE not set; live producer test skipped");
        return;
    };
    for backend in backends() {
        let env = Env::new(backend, "live");
        std::fs::copy(&exe, &env.ledger).unwrap();
        env.ready();
        let api = env.api();
        let id = api
            .submit(0, Some("live"), REQUEST.as_bytes(), Some("600"))
            .body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let log = finish(env.worker(&[]), Duration::from_secs(300));
        let view = api.get(&format!("/native/jobs/{id}")).body;
        assert_eq!(view["status"], "native_done", "{log}\n{view}");
        assert_eq!(view["result"]["receipt"]["verified"], true);
        assert_eq!(view["result"]["receipt"]["transactions_verified"], 16);
        let manifest = api.get(&format!("/native/jobs/{id}/manifest")).body;
        assert_eq!(
            manifest["verification"]["receipt"]["independent_equations"],
            true
        );
        assert!(manifest["stream"]["chunks"]
            .as_array()
            .is_some_and(|c| !c.is_empty()));
        let lineage = api
            .get(&format!(
                "/native/jobs/{id}/tables/lifecycle_journal_lineage?limit=1000"
            ))
            .body;
        assert_eq!(
            lineage["rows"].as_array().unwrap().len(),
            lineage["total"].as_u64().unwrap() as usize
        );
    }
}

const BOOK: &str = include_str!("fixtures/lifecycle-book.json");
const PLAN: &str = include_str!("fixtures/lifecycle-plan.json");

#[test]
fn a_saved_book_runs_verifies_publishes_and_reopens_after_restart_through_rust_only() {
    let Ok(exe) = std::env::var("WORKBENCH_TEST_LEDGER_EXE") else {
        eprintln!("WORKBENCH_TEST_LEDGER_EXE not set; live saved-book test skipped");
        return;
    };
    for backend in backends() {
        let env = Env::new(backend, "livebook");
        std::fs::copy(&exe, &env.ledger).unwrap();
        env.ready();
        let api = env.api();
        assert_eq!(
            api.call(
                "PUT",
                "/native/documents/desk-book",
                &[("If-Match", "0")],
                BOOK.as_bytes()
            )
            .status,
            201
        );
        assert_eq!(
            api.call(
                "PUT",
                "/native/documents/unwind-plan",
                &[("If-Match", "1")],
                PLAN.as_bytes()
            )
            .status,
            201
        );
        let run = |rev: &str, plan: &str, key: &str| {
            let r = api.call(
                "POST",
                &format!("/native/lifecycle/runs?book=desk-book&plan={plan}"),
                &[("X-Expected-Revision", rev), ("Idempotency-Key", key)],
                b"",
            );
            assert_eq!(r.status, 202, "{:?}", r.body);
            r.body["job_id"].as_str().unwrap().to_string()
        };
        let economic = run("2", "unwind-plan", "economic");
        // The paired formal plan has equal economic inputs and an unimplemented formal owner.
        let mut formal: serde_json::Value = serde_json::from_str(PLAN).unwrap();
        formal["actions"][0]["accounting_treatment"] = serde_json::json!("formal");
        let put = api.call(
            "PUT",
            "/native/documents/formal-plan",
            &[("If-Match", "2")],
            &serde_json::to_vec(&formal).unwrap(),
        );
        assert_eq!(put.status, 201);
        let formal_job = run("3", "formal-plan", "formal");
        let log = finish(env.worker(&[]), Duration::from_secs(300));

        let refused = api.get(&format!("/native/jobs/{formal_job}")).body;
        assert_eq!(refused["status"], "native_error", "{log}");
        assert!(
            refused["detail"]
                .as_str()
                .unwrap()
                .starts_with("NATIVE_REFUSED: FORMAL_OWNER_REQUIRED"),
            "{refused}"
        );
        assert_eq!(refused["published"], false);

        // Restart the API, then reopen the published run from storage.
        drop(api);
        let api = env.api();
        let view = api.get(&format!("/native/jobs/{economic}")).body;
        assert_eq!(view["status"], "native_done", "{log}\n{view}");
        let receipt = api.get(&format!("/native/jobs/{economic}/receipt")).body;
        let journal = &receipt["workflow"]["source_receipt"]["journals"][0];
        let steps = journal["receipt"]["steps"].as_array().unwrap();
        let step = |id: &str| steps.iter().find(|s| s["event_id"] == id).unwrap();
        assert_eq!(step("draw")["cash_movement"], 95.0);
        assert_eq!(step("repay-repo")["cash_after"], 0.0);
        assert_eq!(step("vm_return:swap-lot")["posted_vm_after"], 0.0);
        assert_eq!(steps.last().unwrap()["usable_cash_after"], 6.0);
        assert_eq!(
            receipt["workflow"]["source_receipt"]["formal_hedge_accounting"],
            false
        );
        let manifest = api.get(&format!("/native/jobs/{economic}/manifest")).body;
        assert_eq!(manifest["verification"]["receipt"]["verified"], true);
        // Download every row of a table by paging; nothing hydrates the whole stream.
        let first = api
            .get(&format!(
                "/native/jobs/{economic}/tables/lifecycle_journal_lineage?limit=10"
            ))
            .body;
        let total = first["total"].as_u64().unwrap();
        let mut rows = 0;
        let mut offset = 0;
        while offset < total {
            let page = api.get(&format!("/native/jobs/{economic}/tables/lifecycle_journal_lineage?offset={offset}&limit=10")).body;
            rows += page["rows"].as_array().unwrap().len() as u64;
            offset += 10;
        }
        assert_eq!(rows, total);
    }
}

#[test]
fn a_policy_plan_runs_and_publishes_verified_decisions_and_monthly_reports() {
    let Ok(exe) = std::env::var("WORKBENCH_TEST_LEDGER_EXE") else {
        eprintln!("WORKBENCH_TEST_LEDGER_EXE not set; live policy test skipped");
        return;
    };
    let env = Env::new(Backend::Sqlite, "livepolicy");
    std::fs::copy(&exe, &env.ledger).unwrap();
    env.ready();
    let api = env.api();
    let example = api.get("/native/lifecycle/example").body;
    let put = |name: &str, doc: &serde_json::Value, rev: &str| {
        api.call(
            "PUT",
            &format!("/native/documents/{name}"),
            &[("If-Match", rev)],
            &serde_json::to_vec(doc).unwrap(),
        )
    };
    assert_eq!(put("desk-book", &example["book"], "0").status, 201);
    assert_eq!(put("policy-plan", &example["policy_plan"], "1").status, 201);
    let r = api.call(
        "POST",
        "/native/lifecycle/runs?book=desk-book&plan=policy-plan",
        &[("X-Expected-Revision", "2")],
        b"",
    );
    assert_eq!(r.status, 202, "{:?}", r.body);
    let id = r.body["job_id"].as_str().unwrap().to_string();
    let log = finish(env.worker(&[]), Duration::from_secs(300));
    assert_eq!(
        api.get(&format!("/native/jobs/{id}")).body["status"],
        "native_done",
        "{log}"
    );
    let receipt = api.get(&format!("/native/jobs/{id}/receipt")).body;
    let r = &receipt["workflow"]["source_receipt"]["journals"][0]["receipt"];
    assert_eq!(r["business_plan_mode"], "policy");
    assert_eq!(r["policy_decisions"][0]["kind"], "draw");
    assert_eq!(r["policy_decisions"][0]["trigger"], "repay-repo");
    assert_eq!(r["policy_decisions"][1]["kind"], "surplus_repayment");
    assert_eq!(r["monthly"][0]["minimum_usable_cash"], 0.0);
    assert_eq!(r["closing_cash"], 6.0);
}
