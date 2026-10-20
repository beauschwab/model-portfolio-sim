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
