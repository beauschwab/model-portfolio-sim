//! The frozen stage C fixture (the plan's worked unwind in the shared Sim) verifies, and every
//! tamper that keeps the stream well formed is refused: balanced but economically changed
//! postings, duplicates, reordering, forged or misplaced completion, altered stocks, altered
//! native valuations, a changed plan hash, a changed raw instruction and a signed-zero opening.
use portfolio_verification_native::verify;
use serde_json::{json, Value};

const REQUEST: &str = include_str!("fixtures/lifecycle-unwind.request.json");
const STREAM: &str = include_str!("fixtures/lifecycle-unwind.stream.jsonl");

fn lines() -> Vec<Value> {
    STREAM
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn join(lines: &[Value]) -> String {
    lines
        .iter()
        .map(|l| serde_json::to_string(l).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
fn refused(request: &str, stream: &str) -> String {
    verify(request.as_bytes(), stream.as_bytes()).unwrap_err()
}
fn edit_rows(lines: &mut [Value], table: &str, f: impl Fn(&mut Vec<Value>)) {
    for l in lines.iter_mut() {
        if l.get("table").and_then(Value::as_str) == Some(table) {
            let rows = l["rows"].as_array_mut().unwrap();
            f(rows);
        }
    }
}
fn lineage_row<'a>(rows: &'a mut [Value], scenario: &str, event: &str, gl: &str) -> &'a mut Value {
    rows.iter_mut()
        .find(|r| {
            r["scenario"] == scenario && r["source_event_id"] == event && r["gl_account"] == gl
        })
        .unwrap()
}

#[test]
fn the_frozen_worked_unwind_verifies_with_independent_equations() {
    let receipt = verify(REQUEST.as_bytes(), STREAM.as_bytes()).unwrap();
    assert!(
        receipt.verified
            && receipt.independent_equations
            && !receipt.production_financial_crates
            && !receipt.methodology_certified
    );
    assert_eq!(receipt.scenarios, ["baseline", "zero"]);
    assert_eq!(receipt.valuations_verified, 1);
    assert_eq!(receipt.intents_verified, 16); // 8 intents in each of two scenarios
    assert_eq!(receipt.transactions_verified, 16);
    assert!(receipt.closing_stocks_verified >= 12);
}

#[test]
fn balanced_but_economically_changed_postings_are_refused() {
    let mut l = lines();
    // Move 1 of the bond sale proceeds from the bond's principal to its realized line: still balanced.
    edit_rows(&mut l, "lifecycle_journal_lineage", |rows| {
        lineage_row(rows, "baseline", "sell-bond", "asset_principal")["debit_amount"] =
            json!(-99.0);
        let plan_key = lineage_row(rows, "baseline", "sell-bond", "cash")["plan_key"].clone();
        let at = rows
            .iter()
            .rposition(|r| r["scenario"] == "baseline" && r["source_event_id"] == "sell-bond")
            .unwrap()
            + 1;
        rows.insert(at, json!({"account": "a", "day": 1, "debit_amount": -1.0, "gl_account": "pnl:lifecycle_realized", "instrument_id": "@lifecycle:bond-lot",
            "ordinal": 738887, "plan_key": plan_key, "scenario": "baseline",
            "source_event_id": "sell-bond", "source_intent_index": 2, "source_posting_index": 2}));
    });
    assert!(refused(REQUEST, &join(&l)).contains("sell-bond"));
}

#[test]
fn duplicated_reordered_and_altered_rows_are_refused() {
    let mut l = lines();
    edit_rows(&mut l, "lifecycle_journal_lineage", |rows| {
        let dup = rows[0].clone();
        rows.push(dup);
    });
    assert!(!refused(REQUEST, &join(&l)).is_empty());
    let mut l = lines();
    edit_rows(&mut l, "lifecycle_journal_lineage", |rows| rows.swap(0, 2));
    assert!(refused(REQUEST, &join(&l)).contains("order"));
    let mut l = lines();
    edit_rows(&mut l, "lifecycle_journal_lineage", |rows| {
        lineage_row(rows, "zero", "draw", "cash")["debit_amount"] = json!(96.0);
    });
    assert!(refused(REQUEST, &join(&l)).contains("draw"));
}

#[test]
fn the_journal_and_the_closing_stocks_must_agree_with_the_independent_replay() {
    let mut l = lines();
    for line in l.iter_mut() {
        if line.get("table").and_then(Value::as_str) == Some("journal") {
            let values = line["columns"]["value"].as_array_mut().unwrap();
            // Find a lifecycle posting by its magnitude and change it.
            let index = values
                .iter()
                .position(|v| v.as_f64() == Some(95.0))
                .unwrap();
            values[index] = json!(94.0);
        }
    }
    assert!(refused(REQUEST, &join(&l)).contains("journal transaction"));
    let mut l = lines();
    edit_rows(&mut l, "trial_balance", |rows| {
        let row = rows
            .iter_mut()
            .find(|r| {
                r["instrument_id"] == "@lifecycle:swap-csa"
                    && r["gl_account"] == "derivative_posted_vm_asset"
            })
            .unwrap();
        row["balance"] = json!(2.0);
    });
    assert!(refused(REQUEST, &join(&l)).contains("closing derivative_posted_vm_asset"));
}

#[test]
fn forged_missing_or_misplaced_completion_is_refused() {
    let l = lines();
    let without: Vec<Value> = l
        .iter()
        .filter(|x| x.get("complete").is_none())
        .cloned()
        .collect();
    assert!(refused(REQUEST, &join(&without)).contains("completion"));
    let mut twice = l.clone();
    twice.push(json!({"complete": true, "protocol": 3}));
    assert!(refused(REQUEST, &join(&twice)).contains("continues after"));
    let mut early = without.clone();
    early.insert(1, json!({"complete": true, "protocol": 3}));
    assert!(refused(REQUEST, &join(&early)).contains("continues after"));
    let mut wrong = without;
    wrong.push(json!({"complete": true, "protocol": 2}));
    assert!(refused(REQUEST, &join(&wrong)).contains("protocol"));
}

#[test]
fn native_valuations_hash_and_receipt_amounts_are_checked() {
    let mut l = lines();
    l[0]["workflow"]["source_receipt"]["hedge_entries"][0]["clean_value"] = json!(-4.5);
    assert!(refused(REQUEST, &join(&l)).contains("independent equation"));
    let mut l = lines();
    l[0]["workflow"]["source_receipt"]["plan_sha256"] = json!("0".repeat(64));
    assert!(refused(REQUEST, &join(&l)).contains("plan hash"));
    let mut l = lines();
    l[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["closing_cash"] = json!(7.0);
    assert!(refused(REQUEST, &join(&l)).contains("closing cash"));
    let mut l = lines();
    l[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["events"][3]["status"] =
        json!("failed");
    assert!(refused(REQUEST, &join(&l)).contains("status"));
    let mut l = lines();
    l[0]["workflow"]["source_receipt"]["formal_hedge_accounting"] = json!(true);
    assert!(refused(REQUEST, &join(&l)).contains("formal"));
}

#[test]
fn a_changed_raw_instruction_or_signed_zero_opening_is_refused() {
    let mut r: Value = serde_json::from_str(REQUEST).unwrap();
    r["acquisitions"][0]["sales"][0]["clean_price_per_100"] = json!(99.0);
    assert!(
        refused(&serde_json::to_string(&r).unwrap(), STREAM).contains("plan hash")
            || refused(&serde_json::to_string(&r).unwrap(), STREAM).contains("sell-bond")
    );
    // The plan hash covers the plan only; the execution block is verified by replay.
    let text = serde_json::to_string(&r).unwrap();
    assert!(refused(&text, STREAM).contains("sell-bond"));
    // The plan's own opening cash (the dated backbook's cash is the Sim's to check).
    let plan_at = REQUEST.find("\"plan\"").unwrap();
    let at = plan_at + REQUEST[plan_at..].find("\"cash\": 0.0").unwrap();
    let signed = format!(
        "{}\"cash\": -0.0{}",
        &REQUEST[..at],
        &REQUEST[at + "\"cash\": 0.0".len()..]
    );
    assert!(signed != REQUEST);
    assert!(
        refused(&signed, STREAM).contains("typed identity")
            || refused(&signed, STREAM).contains("plan hash")
    );
    let duplicate = REQUEST.replacen(
        "\"schema\": \"liquidity-lifecycle-1\",",
        "\"schema\": \"liquidity-lifecycle-1\", \"schema\": \"x\",",
        1,
    );
    assert!(refused(&duplicate, STREAM).contains("strict JSON"));
}

#[test]
fn supplied_initial_margin_is_checked_for_binding_not_methodology() {
    let mut r: Value = serde_json::from_str(REQUEST).unwrap();
    for a in r["acquisitions"].as_array_mut().unwrap() {
        a["initial_margin"][0]["portfolio_lot_ids"] = json!(["swap-lot"]);
    }
    assert!(refused(&serde_json::to_string(&r).unwrap(), STREAM).contains("bound IM requirement"));
}

#[test]
fn the_verifier_process_round_trips_files_without_python() {
    let dir = std::env::temp_dir().join(format!("lifecycle-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let request = dir.join("request.json");
    let stream = dir.join("stream.jsonl");
    std::fs::write(&request, REQUEST).unwrap();
    std::fs::write(&stream, STREAM).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_portfolio-lifecycle-verify"))
        .arg(&request)
        .arg(&stream)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let receipt: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(receipt["verified"], true);
    assert_eq!(receipt["production_financial_crates"], false);
    std::fs::write(
        &stream,
        STREAM.replacen("\"complete\":true", "\"complete\":false", 1),
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_portfolio-lifecycle-verify"))
        .arg(&request)
        .arg(&stream)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn liquidity_steps_are_recomputed_and_tampered_steps_are_refused() {
    let l = lines();
    let steps = &l[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["steps"];
    let draw = steps
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["event_id"] == "draw")
        .unwrap();
    assert_eq!(draw["cash_movement"], 95.0);
    // An altered amount, a reordered step and undrawn capacity reported as cash are refused.
    let mut a = lines();
    a[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["steps"][1]["usable_cash_after"] =
        json!(95.0);
    assert!(refused(REQUEST, &join(&a)).contains("liquidity step"));
    let mut b = lines();
    b[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["steps"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(refused(REQUEST, &join(&b)).contains("liquidity step"));
    let mut c = lines();
    let step = &mut c[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["steps"][6];
    let undrawn = step["facility_undrawn_after"].as_f64().unwrap();
    step["cash_after"] = json!(step["cash_after"].as_f64().unwrap() + undrawn);
    assert!(refused(REQUEST, &join(&c)).contains("cash_after"));
    let mut d = lines();
    d[0]["workflow"]["source_receipt"]["journals"][0]["receipt"]["steps"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(refused(REQUEST, &join(&d)).contains("step count"));
}
