use portfolio_decision_native::deposit_ledger::{prepare, run, Request};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/deposit-graph-dated-ledger-1.json"
    ))
    .unwrap()
}
fn raw(request: &Request) -> Value {
    serde_json::to_value(request).unwrap()
}
fn decode(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(value);
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn execute(request: &Request) -> (Value, Vec<Value>) {
    let data = Arc::new(Mutex::new(Vec::new()));
    let receipt = run(request, Box::new(Capture(data.clone()))).unwrap();
    let messages = String::from_utf8(data.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (serde_json::to_value(receipt).unwrap(), messages)
}
fn rows<'a>(messages: &'a [Value], table: &'a str) -> impl Iterator<Item = &'a Value> {
    messages
        .iter()
        .filter(move |v| v["table"] == table)
        .flat_map(|v| v["rows"].as_array().unwrap())
}
fn balance(messages: &[Value], scenario: &str, gl: &str) -> f64 {
    rows(messages, "trial_balance")
        .filter(|v| v["scenario"] == scenario && v["gl_account"] == gl)
        .map(|v| v["balance"].as_f64().unwrap())
        .sum()
}
#[test]
fn independently_replay_emitted_postings_to_every_trial_balance_key() {
    let (_, messages) = execute(&request());
    let mut dictionary = Vec::<String>::new();
    let mut balances = BTreeMap::<(String, String, String, String), f64>::new();
    let mut transactions = BTreeMap::<(String, u64), f64>::new();
    for message in &messages {
        if message["table"] != "journal" {
            continue;
        }
        dictionary.extend(
            message["dictionary"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned()),
        );
        let columns = &message["columns"];
        for (i, value) in columns["value"].as_array().unwrap().iter().enumerate() {
            let label = |key: &str| dictionary[columns[key][i].as_u64().unwrap() as usize].clone();
            let amount = value.as_f64().unwrap();
            *balances
                .entry((
                    label("scenario"),
                    label("account"),
                    label("gl_account"),
                    label("instrument_id"),
                ))
                .or_default() += amount;
            *transactions
                .entry((
                    label("scenario"),
                    columns["transaction_number"][i].as_u64().unwrap(),
                ))
                .or_default() += amount;
        }
    }
    for amount in transactions.values() {
        near(*amount, 0.);
    }
    for row in rows(&messages, "trial_balance") {
        let key = (
            row["scenario"].as_str().unwrap().to_owned(),
            row["account"].as_str().unwrap().to_owned(),
            row["gl_account"].as_str().unwrap().to_owned(),
            row["instrument_id"].as_str().unwrap().to_owned(),
        );
        near(
            *balances.get(&key).unwrap_or(&0.),
            row["balance"].as_f64().unwrap(),
        );
    }
    // Replay proves posting/report consistency, not independent raw source economics.
}

#[test]
fn gross_daily_expense_is_not_cash_and_tail_rights_survive_reporting() {
    let r = request();
    let (receipt, messages) = execute(&r);
    near(balance(&messages, "baseline", "cash"), 1000.);
    near(balance(&messages, "baseline", "funding_principal"), -1000.);
    near(
        balance(&messages, "baseline", "dated_interest_payable"),
        -30.,
    );
    near(balance(&messages, "baseline", "pnl:dated_recognition"), 30.);
    assert_eq!(
        receipt["capture"]["sources"][0]["schedule"]["horizon_days"],
        31
    );
    assert_eq!(receipt["cash_origins"].as_array().unwrap().len(), 6);
    assert_eq!(receipt["raw_request"], raw(&r));
    assert_eq!(receipt["capture"]["owners"][0]["direction"], "payment");
    near(balance(&messages, "zero", "dated_interest_payable"), -30.);
    assert!(rows(&messages, "event_lineage").all(|r| r["phase"] == "recognition"));
    assert_eq!(rows(&messages, "reverse_grid").count(), 1);
}
#[test]
fn full_tail_settlement_retires_liability_once() {
    let mut r = request();
    let asof = r.graph.as_of_ordinal;
    r.graph.reporting_ordinals = vec![asof, asof + 30, asof + 31];
    r.specification["horizon_days"] = json!(31);
    let (_, messages) = execute(&r);
    near(balance(&messages, "baseline", "cash"), -31.);
    near(balance(&messages, "baseline", "funding_principal"), 0.);
    near(balance(&messages, "baseline", "dated_interest_payable"), 0.);
    near(balance(&messages, "baseline", "pnl:dated_recognition"), 31.);
    assert_eq!(
        rows(&messages, "event_lineage")
            .filter(|r| r["phase"] == "settlement" && r["scenario"] == "baseline")
            .count(),
        2
    );
}
#[test]
fn conflicting_openings_are_never_overwritten() {
    let r = request();
    for (key, value) in [
        ("balance", json!(999)),
        ("opening_accrued", json!(1)),
        ("book_adjustment", json!(1)),
        ("source_id", json!("wrong")),
        ("kind", json!("loan")),
        ("account", json!("wrong")),
    ] {
        let mut v = raw(&r);
        v["specification"]["positions"][0][key] = value;
        assert!(prepare(&decode(v)).is_err(), "{key}");
    }
    let before = raw(&r);
    let prepared = prepare(&r).unwrap();
    assert_eq!(raw(&r), before);
    assert_eq!(
        prepared.dated_request.specification["positions"][0]["balance"],
        1000.
    );
}
#[test]
fn all_declared_scenario_and_reverse_runs_require_explicit_histories() {
    let r = request();
    let mut missing = r.clone();
    missing.run_mapping.pop();
    assert!(prepare(&missing).is_err());
    let mut duplicate = r.clone();
    duplicate.run_mapping[1].run_id = duplicate.run_mapping[0].run_id.clone();
    assert!(prepare(&duplicate).is_err());
    let mut extra = r.clone();
    extra.specification["reverse_severities"] = json!([0.25]);
    assert!(prepare(&extra).is_err());
    let prepared = prepare(&r).unwrap();
    assert_eq!(prepared.dated_request.dated.runs.len(), 3);
    assert_eq!(prepared.dated_request.dated.runs[2].severity, 0.5);
}
#[test]
fn saved_legacy_deposit_stress_does_not_apply_twice_to_owned_sources() {
    let base = request();
    let (_, expected) = execute(&base);
    let mut r = request();
    r.specification["scenarios"][0]["deposit_flight"] = json!(0.1);
    let (_, actual) = execute(&r);
    for gl in [
        "cash",
        "funding_principal",
        "dated_interest_payable",
        "pnl:dated_recognition",
    ] {
        near(balance(&actual, "zero", gl), balance(&expected, "zero", gl));
    }
    assert_eq!(
        rows(&actual, "event_lineage").collect::<Vec<_>>(),
        rows(&expected, "event_lineage").collect::<Vec<_>>()
    );
}
