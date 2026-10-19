//! Stage B (issue #44) through the native coordinator and the daily Sim: the model-core
//! acquisition fixture joined to the reviewed 30-day dated backbook of `derivative-mixed.json`.
//! The swap is priced natively (payer, 3600 notional, 5% fixed against 1% projected for ten
//! days: clean value -4), then replayed with its upfront consideration, posted IM and VM.
use portfolio_decision_native::workflows::lifecycle::acquisition::{prepare, run, Request};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

const FIXTURE: &str = include_str!("fixtures/lifecycle-acquisition.json");

fn raw() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn execute(v: &Value) -> Result<(Value, Vec<Value>), String> {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let receipt = run(
        &serde_json::to_vec(v).unwrap(),
        Box::new(Capture(bytes.clone())),
    )?;
    let data = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    Ok((
        serde_json::to_value(receipt).unwrap(),
        data.lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect(),
    ))
}
fn balance(messages: &[Value], scenario: &str, gl: &str, instrument: &str) -> f64 {
    messages
        .iter()
        .filter(|m| m["table"] == "trial_balance")
        .flat_map(|m| m["rows"].as_array().unwrap())
        .filter(|r| {
            r["scenario"] == scenario && r["gl_account"] == gl && r["instrument_id"] == instrument
        })
        .map(|r| r["balance"].as_f64().unwrap())
        .sum()
}

#[test]
fn acquisition_posts_once_in_the_shared_sim_and_reconciles() {
    let (receipt, m) = execute(&raw()).unwrap();
    assert_eq!(receipt["stage"], "B");
    assert_eq!(receipt["formal_hedge_accounting"], false);
    near(
        receipt["hedge_entries"][0]["clean_value"].as_f64().unwrap(),
        -4.,
    );
    assert_eq!(
        receipt["hedge_entries"][0]["pricing_source"]["id"],
        "discount"
    );
    for scenario in ["baseline", "zero"] {
        near(
            balance(&m, scenario, "asset_principal", "@lifecycle:bond-lot"),
            100.,
        );
        near(
            balance(&m, scenario, "book_adjustment", "@lifecycle:bond-lot"),
            -0.5,
        );
        near(
            balance(
                &m,
                scenario,
                "dated_interest_receivable",
                "@lifecycle:bond-lot",
            ),
            0.5,
        );
        near(
            balance(&m, scenario, "funding_principal", "@lifecycle:repo-lot"),
            -95.,
        );
        near(
            balance(&m, scenario, "derivative_liability", "@lifecycle:swap-lot"),
            -4.,
        );
        near(
            balance(
                &m,
                scenario,
                "derivative_posted_vm_asset",
                "@lifecycle:swap-csa",
            ),
            4.,
        );
        near(
            balance(
                &m,
                scenario,
                "derivative_posted_im_asset",
                "@lifecycle:swap-csa",
            ),
            3.,
        );
        near(
            balance(&m, scenario, "pnl:lifecycle_fees", "@lifecycle:bond-lot"),
            0.25,
        );
        // Opening cash 100 and the backbook loan's day-30 principal and coupon 100.3, plus the
        // acquisition: +95 -100.25 +4 -3 -4.
        near(
            balance(&m, scenario, "cash", ""),
            100. + 100.3 + 95. - 100.25 + 4. - 3. - 4.,
        );
        // Received IM never reaches the financial GL.
        assert!(m
            .iter()
            .filter(|x| x["table"] == "trial_balance")
            .flat_map(|x| x["rows"].as_array().unwrap())
            .all(|r| !r["gl_account"].as_str().unwrap().starts_with("memo:")));
    }
    let custody: Vec<&Value> = m
        .iter()
        .filter(|x| x["table"] == "lifecycle_custody_journal")
        .flat_map(|x| x["rows"].as_array().unwrap())
        .collect();
    assert!(custody
        .iter()
        .any(|r| r["gl_account"] == "memo:derivative_custodian_asset" && r["debit_amount"] == 2.0));
    let journals = receipt["journals"].as_array().unwrap();
    assert_eq!(journals.len(), 2);
    for j in journals {
        assert_eq!(j["receipt"]["cash_bridge_reconciled"], true);
        assert_eq!(j["receipt"]["collateral_conserved"], true);
        near(j["receipt"]["received_im_custody"].as_f64().unwrap(), 2.);
    }
    assert!(m.last().unwrap()["complete"] == true);
}

#[test]
fn refusals_name_their_cause_and_never_fall_back() {
    let mut v = raw();
    v["plan"]["actions"][0]["accounting_treatment"] = json!("formal");
    assert!(execute(&v).unwrap_err().contains("FORMAL_OWNER_REQUIRED"));
    let mut v = raw();
    v["hedge_contracts"] = json!([]);
    assert!(execute(&v)
        .unwrap_err()
        .contains("LIFECYCLE_MAPPING_REQUIRED"));
    let mut v = raw();
    v["hedge_contracts"][0]["swap"]["asof_ordinal"] = json!(738886);
    assert!(execute(&v).unwrap_err().contains("entry date"));
    let mut v = raw();
    v["plan"]["accounts"][0]["cash"] = json!(99.0);
    assert!(execute(&v)
        .unwrap_err()
        .contains("differ from the dated backbook"));
    let mut v = raw();
    v["acquisitions"].as_array_mut().unwrap().pop();
    assert!(execute(&v).unwrap_err().contains("every dated run"));
    let mut v = raw();
    v["acquisitions"][0]["hedge_entries"][0]["upfront_consideration"] = json!(0.0);
    assert!(execute(&v).unwrap_err().contains("UNSUPPORTED_POLICY"));
    let mut v = raw();
    v["acquisitions"][0]["initial_margin"][0]["ordinal"] = json!(738890);
    assert!(execute(&v).unwrap_err().contains("MARGIN_REFRESH_REQUIRED"));
    let duplicate = FIXTURE.replacen(
        "\"schema\": \"liquidity-lifecycle-1\",",
        "\"schema\": \"liquidity-lifecycle-1\", \"schema\": \"x\",",
        1,
    );
    assert!(prepare(duplicate.as_bytes()).is_err());
    let mut v = raw();
    v["acquisitions"][0]["hedge_entries"][0]["clean_value"] = json!(-4.0);
    assert!(serde_json::from_value::<Request>(v).is_err());
}

#[test]
fn the_native_process_runs_the_schema_without_python() {
    let envelope = json!({"execution_timeout_ms": 60000, "request": raw()});
    let mut child = Command::new(env!("CARGO_BIN_EXE_portfolio-model-ledger"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&envelope).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(
        lines[0]["workflow"]["model_schema"],
        "liquidity-lifecycle-1"
    );
    near(
        lines[0]["workflow"]["source_receipt"]["hedge_entries"][0]["clean_value"]
            .as_f64()
            .unwrap(),
        -4.,
    );
    near(
        balance(
            &lines,
            "baseline",
            "derivative_posted_vm_asset",
            "@lifecycle:swap-csa",
        ),
        4.,
    );
    assert!(lines.last().unwrap()["complete"] == true);
}
