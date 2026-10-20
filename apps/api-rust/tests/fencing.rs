//! Lease, attempt, deadline and cancellation fencing across real worker processes. Needs the
//! test-only `fault-injection` feature (stalled heartbeat, delayed final I/O).
mod common;
use common::*;
use serde_json::Value;
use std::time::Duration;

fn job(api: &Api, id: &str) -> Value {
    api.get(&format!("/native/jobs/{id}")).body
}

#[test]
fn a_lost_lease_is_retried_by_the_next_worker_and_the_late_old_attempt_cannot_publish() {
    for backend in backends() {
        let env = Env::new(backend, "lease");
        env.ready();
        let api = env.api();
        let admitted = api
            .submit(0, Some("once"), REQUEST.as_bytes(), Some("600"))
            .body;
        let id = admitted["job_id"].as_str().unwrap().to_string();
        // Worker A stalls its heartbeat (its lease expires) while its slow producer keeps running.
        let a = env.worker(&[
            ("WORKBENCH_FAULT_STALL_HEARTBEAT", "1"),
            ("STUB_SLEEP_MS", "7000"),
        ]);
        wait_for("attempt 1", Duration::from_secs(30), || {
            job(&api, &id)["attempt"] == 1
        });
        // Worker B takes the expired lease, requeues the stale attempt and runs attempt 2.
        let b = env.worker(&[]);
        wait_for("attempt 2 publication", Duration::from_secs(60), || {
            job(&api, &id)["status"] == "native_done"
        });
        let b_log = finish(b, Duration::from_secs(60));
        let a_log = finish(a, Duration::from_secs(60));
        let view = job(&api, &id);
        assert_eq!(view["attempt"], 2, "{a_log}\n{b_log}");
        assert_eq!(
            api.get(&format!("/native/jobs/{id}/manifest")).body["attempt"],
            2
        );
        // The old attempt finished later and was refused at the final fence.
        assert!(
            a_log.contains("discarded") && a_log.contains("LEASE_LOST"),
            "{a_log}"
        );
        assert_eq!(env.publications(), 1);
        // Retries keep the original admission deadline.
        assert_eq!(view["authority"]["deadline_at"], admitted["deadline_at"]);
    }
}

#[test]
fn cancellation_or_expiry_during_final_io_leaves_no_completed_manifest() {
    for backend in backends() {
        let env = Env::new(backend, "final");
        env.ready();
        let api = env.api();
        let slow = [("WORKBENCH_FAULT_FINAL_IO_MS", "3000")];

        let cancelled = api.submit(0, None, REQUEST.as_bytes(), Some("600")).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let w = env.worker(&slow);
        wait_for("final I/O", Duration::from_secs(60), || {
            job(&api, &cancelled)["progress"]["stage"] == "publishing"
        });
        let r = api.call(
            "POST",
            &format!("/native/jobs/{cancelled}/cancel"),
            &[],
            b"",
        );
        assert_eq!(r.body["cancelled"], true);
        let log = finish(w, Duration::from_secs(60));
        let view = job(&api, &cancelled);
        assert_eq!(view["status"], "native_cancelled", "{log}");
        assert_eq!(view["published"], false);
        assert!(log.contains("SUPERSEDED"), "{log}");

        let expiring = api.submit(0, None, REQUEST.as_bytes(), Some("2")).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        let log = finish(env.worker(&slow), Duration::from_secs(60));
        let view = job(&api, &expiring);
        assert_eq!(view["status"], "native_error", "{log}");
        assert!(
            view["detail"]
                .as_str()
                .unwrap()
                .starts_with("DEADLINE_EXPIRED"),
            "{view}"
        );
        assert_eq!(view["published"], false);
        assert_eq!(
            api.get(&format!("/native/jobs/{expiring}/manifest")).body["code"],
            "NOT_PUBLISHED"
        );
        assert_eq!(env.publications(), 0);
    }
}

#[test]
fn a_restarted_worker_retries_without_resetting_the_deadline_and_an_expired_job_never_runs() {
    for backend in backends() {
        let env = Env::new(backend, "restart");
        env.ready();
        let api = env.api();
        let admitted = api.submit(0, None, REQUEST.as_bytes(), Some("600")).body;
        let id = admitted["job_id"].as_str().unwrap().to_string();
        // Crash the first worker mid-run.
        let mut first = env.worker(&[("STUB_SLEEP_MS", "30000")]);
        wait_for("running", Duration::from_secs(30), || {
            job(&api, &id)["status"] == "native_running"
        });
        first.kill().unwrap();
        first.wait().unwrap();
        finish(env.worker(&[]), Duration::from_secs(60));
        let view = job(&api, &id);
        assert_eq!(
            (view["status"].as_str(), view["attempt"].as_i64()),
            (Some("native_done"), Some(2))
        );
        assert_eq!(view["authority"]["deadline_at"], admitted["deadline_at"]);

        // A job whose original deadline passed while no worker ran fails at claim without running.
        let late = api.submit(0, None, REQUEST.as_bytes(), Some("1")).body["job_id"]
            .as_str()
            .unwrap()
            .to_string();
        std::thread::sleep(Duration::from_millis(1500));
        finish(env.worker(&[]), Duration::from_secs(60));
        let view = job(&api, &late);
        assert_eq!(view["attempt"], 0);
        assert!(
            view["detail"]
                .as_str()
                .unwrap()
                .starts_with("DEADLINE_EXPIRED"),
            "{view}"
        );
    }
}
