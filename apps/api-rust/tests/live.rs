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

#[test]
fn an_allocation_is_validated_only_after_exact_replay_and_independent_verification() {
    let Ok(exe) = std::env::var("WORKBENCH_TEST_LEDGER_EXE") else {
        eprintln!("WORKBENCH_TEST_LEDGER_EXE not set; live allocation test skipped");
        return;
    };
    let env = Env::new(Backend::Sqlite, "liveallocation");
    std::fs::copy(&exe, &env.ledger).unwrap();
    env.ready();
    let api = env.api();
    let example = api.get("/native/lifecycle/example").body;
    let mut above = example["plan"].clone();
    above["instructions"]["sales"][0]["clean_price_per_100"] = serde_json::json!(101.0);
    let mut rev = 0;
    for (name, doc) in [
        ("desk-book", &example["book"]),
        ("at-par", &example["plan"]),
        ("above-par", &above),
    ] {
        let r = api.call(
            "PUT",
            &format!("/native/documents/{name}"),
            &[("If-Match", &rev.to_string())],
            &serde_json::to_vec(doc).unwrap(),
        );
        assert_eq!(r.status, 201, "{:?}", r.body);
        rev += 1;
    }
    let submit = |body: serde_json::Value| {
        let r = api.call(
            "POST",
            "/native/lifecycle/allocations?book=desk-book",
            &[("X-Expected-Revision", &rev.to_string())],
            &serde_json::to_vec(&body).unwrap(),
        );
        assert_eq!(r.status, 202, "{:?}", r.body);
        r.body["job_id"].as_str().unwrap().to_string()
    };
    let packages = serde_json::json!([{"plan": "at-par", "group": "bond"}, {"plan": "above-par", "group": "bond"}]);
    let good = submit(
        serde_json::json!({"packages": packages, "objective": "worst_case_total_earnings", "minimum_usable_cash": 0.0}),
    );
    let unreachable = submit(serde_json::json!({"packages": packages, "minimum_usable_cash": 7.0}));
    let log = finish(env.worker(&[]), Duration::from_secs(300));

    let view = api.get(&format!("/native/jobs/{good}")).body;
    assert_eq!(view["status"], "native_done", "{log}\n{view}");
    assert_eq!(view["result"]["validated"], true);
    let a = api.get(&format!("/native/jobs/{good}/allocation")).body;
    assert_eq!(a["validated"], true);
    assert_eq!(
        a["allocation"]["selected"],
        serde_json::json!(["above-par"])
    );
    // The validated plan reopens as a lifecycle publication.
    let receipt = api.get(&format!("/native/jobs/{good}/receipt")).body;
    assert_eq!(
        receipt["workflow"]["source_receipt"]["journals"][0]["receipt"]["realized"],
        1.0
    );
    assert!(api
        .get(&format!("/native/jobs/{good}/tables/journal?limit=5"))
        .body["rows"]
        .as_array()
        .is_some());
    // Coefficient-only evaluation: an estimate, never a validated plan.
    let e = api.call(
        "POST",
        &format!("/native/jobs/{good}/allocation/evaluate"),
        &[],
        br#"{"selection": ["at-par"]}"#,
    );
    assert_eq!(
        (e.status, e.body["validated"].as_bool()),
        (200, Some(false)),
        "{:?}",
        e.body
    );
    assert_eq!(e.body["status"], "estimate");

    let failed = api.get(&format!("/native/jobs/{unreachable}")).body;
    assert_eq!(failed["status"], "native_done", "{log}");
    assert_eq!(failed["result"]["validated"], false);
    assert_eq!(failed["result"]["outcome"], "estimate_infeasible");
    assert_eq!(
        api.get(&format!("/native/jobs/{unreachable}/tables/journal"))
            .body["code"],
        "NOT_PUBLISHED"
    );

    // A changed package invalidates the preparation for evaluation.
    let r = api.call(
        "PUT",
        "/native/documents/above-par",
        &[("If-Match", &rev.to_string())],
        &serde_json::to_vec(&example["plan"]).unwrap(),
    );
    assert_eq!(r.status, 201);
    let stale = api.call(
        "POST",
        &format!("/native/jobs/{good}/allocation/evaluate"),
        &[],
        br#"{"selection": ["at-par"]}"#,
    );
    assert_eq!(
        (stale.status, stale.body["code"].as_str()),
        (409, Some("PREPARATION_STALE")),
        "{:?}",
        stale.body
    );
}

/// The unwind with an offsetting receiver under `research-sensitivity-im-1`, recomputed on the
/// termination date, and the dated backbook loan replicated to `n` positions (each funded by
/// opening equity, in both the backbook and the plan account).
fn research_scaled(n: usize) -> serde_json::Value {
    use serde_json::json;
    let mut v: serde_json::Value = serde_json::from_str(REQUEST).unwrap();
    for l in v["plan"]["lots"].as_array_mut().unwrap() {
        if l["lot_id"] == "offset-lot" {
            l["acquisition_ordinal"] = json!(738800);
        }
    }
    let a = &mut v["plan"]["margin_agreements"][0];
    a["initial_margin"] = json!("research_sensitivity");
    a["margin_method"] = json!({"method": "research-sensitivity-im-1", "revision": "r1",
        "bucket_upper_days": [365, 3650], "risk_weights": [5000.0, 5000.0],
        "correlations": [[1.0, 0.3], [0.3, 1.0]], "two_way": false,
        "review": {"id": "im-method-review", "revision": "r1"}});
    for acq in v["acquisitions"].as_array_mut().unwrap() {
        acq["initial_margin"] = json!([]);
        acq["dynamic_margin"] = json!([{"agreement_id": "swap-csa", "ordinal": 738887}]);
    }
    let mut offset = v["hedge_contracts"][0].clone();
    offset["lot_id"] = json!("offset-lot");
    offset["swap"]["position_id"] = json!("swap-offset");
    offset["swap"]["side"] = json!("receiver");
    v["hedge_contracts"].as_array_mut().unwrap().push(offset);
    // Replicate the backbook loan.
    let d = &mut v["dated"];
    let base = d["specification"]["positions"][0].clone();
    let id = base["id"].as_str().unwrap().to_string();
    let owner = d["dated"]["owners"][0].clone();
    let of = |list: &serde_json::Value| -> Vec<serde_json::Value> {
        list.as_array()
            .unwrap()
            .iter()
            .filter(|x| x["position_id"] == id.as_str())
            .cloned()
            .collect()
    };
    let (events, cover, plans) = (
        of(&d["dated"]["sources"][0]["schedule"]["events"]),
        of(&d["dated"]["sources"][0]["coverage"]),
        of(&d["dated"]["accrual_plans"]),
    );
    let (mut po, mut ow, mut ev, mut co, mut pl) = (vec![], vec![], vec![], vec![], vec![]);
    for i in 0..n {
        let pid = if i == 0 {
            id.clone()
        } else {
            format!("{id}-{i:06}")
        };
        let with = |x: &serde_json::Value, k: &str| {
            let mut x = x.clone();
            x[k] = json!(pid);
            x
        };
        po.push(with(&base, "id"));
        ow.push(with(&owner, "position_id"));
        ev.extend(events.iter().map(|e| with(e, "position_id")));
        co.extend(cover.iter().map(|c| with(c, "position_id")));
        pl.extend(plans.iter().map(|p| with(p, "position_id")));
    }
    ev.sort_by(|a, b| {
        (a["payment_ordinal"].as_i64(), a["position_id"].as_str())
            .cmp(&(b["payment_ordinal"].as_i64(), b["position_id"].as_str()))
    });
    d["specification"]["positions"] = json!(po);
    d["dated"]["owners"] = json!(ow);
    d["dated"]["sources"][0]["schedule"]["events"] = json!(ev);
    d["dated"]["sources"][0]["coverage"] = json!(co);
    d["dated"]["accrual_plans"] = json!(pl);
    let added = (n - 1) as f64 * base["balance"].as_f64().unwrap();
    let equity = d["specification"]["accounts"][0]["equity"]
        .as_f64()
        .unwrap()
        + added;
    d["specification"]["accounts"][0]["equity"] = json!(equity);
    v["plan"]["accounts"][0]["equity"] = json!(equity);
    v
}

/// Stage I scale harness: the research-IM unwind through the whole service at increasing backbook
/// sizes, recording the producer's measured peak RSS, input, stream bytes and wall time. Sizes
/// come from `WORKBENCH_SCALE_POSITIONS` (comma separated; default `1,64`). Every run must verify
/// and publish; a size beyond the 8 MiB lifecycle request bound must be refused at admission.
#[test]
fn research_im_runs_record_measured_resources_at_each_scale() {
    let Ok(exe) = std::env::var("WORKBENCH_TEST_LEDGER_EXE") else {
        eprintln!("WORKBENCH_TEST_LEDGER_EXE not set; scale harness skipped");
        return;
    };
    let sizes: Vec<usize> = std::env::var("WORKBENCH_SCALE_POSITIONS")
        .unwrap_or_else(|_| "1,64".into())
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let backend = backends().remove(0);
    let env = Env::new(backend, "scale");
    std::fs::copy(&exe, &env.ledger).unwrap();
    env.ready();
    let api = env.api();
    for n in &sizes {
        let body = serde_json::to_vec(&research_scaled(*n)).unwrap();
        let head = api.get("/native/revisions/head").body["revision"]
            .as_i64()
            .unwrap();
        let admitted = api.submit(head, Some(&format!("scale-{n}")), &body, Some("1800"));
        if body.len() > portfolio_verification_native::MAX_REQUEST_BYTES {
            assert_eq!(admitted.status, 413, "{n}: {}", admitted.body);
            eprintln!(
                "SCALE positions={n} input_bytes={} refused=request_bound",
                body.len()
            );
            continue;
        }
        let id = admitted.body["job_id"].as_str().unwrap().to_string();
        let log = finish(env.worker(&[]), Duration::from_secs(1800));
        let view = api.get(&format!("/native/jobs/{id}")).body;
        // Beyond the 256 MiB stream bound the watchdog stops the producer and nothing publishes.
        if view["status"] == "native_error"
            && view["detail"]
                .as_str()
                .is_some_and(|d| d.contains("STREAM_BOUND"))
        {
            assert_eq!(view["published"], false, "{view}");
            eprintln!(
                "SCALE positions={n} input_bytes={} refused=stream_bound",
                body.len()
            );
            continue;
        }
        assert_eq!(view["status"], "native_done", "{n}: {log}\n{view}");
        assert_eq!(view["result"]["receipt"]["verified"], true);
        let r = &view["result"]["resources"];
        assert_eq!(r["input_bytes"].as_u64().unwrap() as usize, body.len());
        assert!(r["stream_bytes"].as_u64().unwrap() > 0);
        if cfg!(any(windows, target_os = "linux")) {
            assert!(r["peak_rss_bytes"].as_u64().unwrap() > 0, "{r}");
        }
        // The last journal row decodes through the dictionary accumulated over every segment.
        let total = api
            .get(&format!("/native/jobs/{id}/tables/journal?limit=1"))
            .body["total"]
            .as_u64()
            .unwrap();
        let last = api
            .get(&format!(
                "/native/jobs/{id}/tables/journal?offset={}&limit=1",
                total - 1
            ))
            .body;
        let row = &last["rows"][0];
        assert!(
            row["event"].is_string() && row["gl_account"].is_string(),
            "{last}"
        );
        let lifecycle = api
            .get(&format!(
                "/native/jobs/{id}/tables/lifecycle_journal_lineage?limit=1000"
            ))
            .body;
        assert!(lifecycle["total"].as_u64().unwrap() > 0);
        eprintln!(
            "SCALE positions={n} input_bytes={} stream_bytes={} peak_rss_bytes={} worker_peak_rss_bytes={} rss_method={} wall_ms={} transactions_verified={} journal_rows={total}",
            r["input_bytes"], r["stream_bytes"], r["peak_rss_bytes"], r["worker_lifetime_peak_rss_bytes"], r["rss_method"], r["wall_ms"],
            view["result"]["receipt"]["transactions_verified"]
        );
    }
}
