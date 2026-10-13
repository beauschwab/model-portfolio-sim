#[path = "../src/advanced_ledger.rs"]
mod advanced_ledger;
use advanced_ledger::{prepare, run, Request};
use portfolio_risk_native::advanced_graph::Product;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/graph-dated-ledger-1.json"
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
fn physical_step(value: &mut Value, step: Value) {
    value["graph"]["positions"][0]["product"]["histories"][0]["physical"][0]["steps"] =
        json!([step]);
}

#[test]
fn independent_leap_source_and_native_daily_trial_balance() {
    let request = request();
    let (receipt, messages) = execute(&request);
    // Jan31-Feb29: 1000*.365*29/365=29. First payment529 retires500.
    // Feb29-Mar31: 500*.365*31/365=15.5. Final cash515.5 retires500.
    near(balance(&messages, "baseline", "cash"), 1044.5);
    near(balance(&messages, "baseline", "asset_principal"), 0.);
    near(
        balance(&messages, "baseline", "dated_interest_receivable"),
        0.,
    );
    near(
        balance(&messages, "baseline", "pnl:dated_recognition"),
        -44.5,
    );
    let settlements: Vec<_> = rows(&messages, "event_lineage")
        .filter(|r| r["phase"] == "settlement" && r["scenario"] == "baseline")
        .collect();
    assert_eq!(settlements.len(), 2);
    assert_eq!(settlements[0]["day"], 29);
    assert_eq!(settlements[1]["day"], 60);
    assert_eq!(settlements[0]["run_id"], "base-run");
    near(balance(&messages, "zero-change", "cash"), 1044.5);
    near(
        balance(&messages, "zero-change", "pnl:dated_recognition"),
        -44.5,
    );
    let segments = &receipt["capture"]["accrual_plans"];
    assert!(segments[0]["segments"][0]["origin_id"]
        .as_str()
        .unwrap()
        .starts_with("interest-"));
    near(
        segments[0]["segments"][0]["accrued_interest"]
            .as_f64()
            .unwrap(),
        29.,
    );
    assert_eq!(receipt["raw_request"], raw(&request));
    assert_eq!(
        messages.last().unwrap(),
        &json!({"complete":true,"protocol":3})
    );
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
fn deterministic_preparation_preserves_raw_input_origin_ids_and_native_anchors() {
    let request = request();
    let before = raw(&request);
    let first = serde_json::to_value(prepare(&request).unwrap()).unwrap();
    let second = serde_json::to_value(prepare(&request).unwrap()).unwrap();
    assert_eq!(first, second);
    assert_eq!(raw(&request), before);
    assert!(before["specification"]["positions"][0]
        .get("balance")
        .is_none());
    let opening = &first["dated_request"]["specification"]["positions"][0];
    near(opening["balance"].as_f64().unwrap(), 1000.);
    near(opening["opening_accrued"].as_f64().unwrap(), 0.);
    near(opening["book_adjustment"].as_f64().unwrap(), 0.);
    assert_eq!(opening["source_id"], "consumer-source");
    assert_eq!(
        first["dated_request"]["dated"]["sources"][0]["schedule"]["scenarios"],
        json!(["base-run", "identity-run"])
    );
    assert_eq!(first["receipt"]["raw_request"], before);
}

#[test]
fn reporting_cutoff_preserves_full_contract_tail_and_separates_interest_cash() {
    let mut request = request();
    let asof = request.graph.as_of_ordinal;
    request.graph.reporting_ordinals = vec![asof, asof + 30];
    request.specification["horizon_days"] = json!(30);
    let prepared = prepare(&request).unwrap();
    assert_eq!(
        prepared.dated_request.dated.sources[0]
            .schedule
            .horizon_days,
        120
    );
    assert_eq!(
        prepared.dated_request.dated.sources[0]
            .schedule
            .events
            .len(),
        4
    );
    let (_, messages) = execute(&request);
    near(balance(&messages, "baseline", "cash"), 529.);
    near(balance(&messages, "baseline", "asset_principal"), 500.);
    near(
        balance(&messages, "baseline", "dated_interest_receivable"),
        0.5,
    );
    near(
        balance(&messages, "baseline", "pnl:dated_recognition"),
        -29.5,
    );
    let tail = rows(&messages, "dated_rights")
        .find(|r| !r["cash_settled"].as_bool().unwrap())
        .unwrap();
    near(
        tail["remaining_principal_settlement"].as_f64().unwrap(),
        500.,
    );
    near(tail["remaining_cash_interest"].as_f64().unwrap(), 15.5);
}

#[test]
fn explicit_scenario_and_reverse_run_mapping_uses_native_physical_sources_once() {
    let mut value = raw(&request());
    value["graph"]["scenarios"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    value["graph"]["positions"][0]["product"]["histories"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    value["run_mapping"].as_array_mut().unwrap().truncate(1);
    value["specification"]["scenarios"] = json!([]);
    let mut shock = value["graph"]["scenarios"][0].clone();
    shock["scenario_id"] = json!("explicit-stress");
    shock["run_id"] = json!("stress-run");
    let mut half = shock.clone();
    half["scenario_id"] = json!("explicit-half");
    half["run_id"] = json!("half-run");
    half["severity"] = json!(0.5);
    value["graph"]["scenarios"]
        .as_array_mut()
        .unwrap()
        .extend([shock, half]);
    let mut histories = value["graph"]["positions"][0]["product"]["histories"][0].clone();
    histories["scenario_id"] = json!("explicit-stress");
    histories["physical"][0]["steps"] = json!([{"ordinal":738945,"action":{"kind":"payoff"}}]);
    let mut half_history = histories.clone();
    half_history["scenario_id"] = json!("explicit-half");
    value["graph"]["positions"][0]["product"]["histories"]
        .as_array_mut()
        .unwrap()
        .extend([histories, half_history]);
    value["run_mapping"].as_array_mut().unwrap().extend([
        json!({"run_id":"stress-run","ledger_scenario_id":"stress"}),
        json!({"run_id":"half-run","ledger_scenario_id":"stress"}),
    ]);
    value["specification"]["scenarios"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"stress","rate_shift":0.1,"pd_multiplier":20}));
    value["specification"]["reverse_severities"] = json!([0.5]);
    let (_, messages) = execute(&decode(value));
    near(balance(&messages, "baseline", "cash"), 1044.5);
    near(balance(&messages, "stress", "cash"), 1029.);
    let runs: Vec<_> = rows(&messages, "event_lineage")
        .filter(|v| v["phase"] == "settlement")
        .map(|v| v["run_id"].as_str().unwrap())
        .collect();
    assert!(runs.contains(&"stress-run"));
    let half = rows(&messages, "reverse_grid")
        .find(|r| r["scenario"] == "stress" && r["severity"] == 0.5)
        .unwrap();
    near(half["final_cash"].as_f64().unwrap(), 1029.);
    assert_eq!(runs.iter().filter(|&&run| run == "base-run").count(), 2);
}

#[test]
fn partial_payment_fifo_origin_lineage_and_daily_conservation() {
    let mut value = raw(&request());
    physical_step(
        &mut value,
        json!({"ordinal":738945,"action":{"kind":"payment","amount":10}}),
    );
    let (receipt, messages) = execute(&decode(value));
    let plans = receipt["capture"]["accrual_plans"].as_array().unwrap();
    near(
        plans[0]["segments"][0]["accrued_interest"]
            .as_f64()
            .unwrap(),
        10.,
    );
    near(
        plans[1]["segments"][0]["accrued_interest"]
            .as_f64()
            .unwrap(),
        19.,
    );
    assert_eq!(
        plans[0]["segments"][0]["origin_id"],
        plans[1]["segments"][0]["origin_id"]
    );
    near(balance(&messages, "baseline", "cash"), 1060.);
    near(
        balance(&messages, "baseline", "pnl:dated_recognition"),
        -60.,
    );
    near(balance(&messages, "baseline", "asset_principal"), 0.);
}

#[test]
fn explicit_conflicting_opening_source_and_account_anchors_reject_without_plug() {
    for (key, value) in [
        ("balance", json!(999)),
        ("opening_accrued", json!(1)),
        ("book_adjustment", json!(1)),
        ("source_id", json!("other")),
        ("account", json!("missing")),
        ("kind", json!("security")),
    ] {
        let mut request = request();
        request.specification["positions"][0][key] = value;
        assert!(prepare(&request).is_err(), "{key}");
    }
    let mut request = request();
    request.specification["accounts"][0]["equity"] = json!(0);
    assert!(prepare(&request)
        .err()
        .unwrap()
        .contains("does not reconcile"));
}

#[test]
fn graph_mapping_currency_horizon_and_unrecognized_policies_reject() {
    for case in 0..7 {
        let mut request = request();
        match case {
            0 => request.run_mapping.clear(),
            1 => request.run_mapping[0].run_id = "unknown".into(),
            2 => request.run_mapping[0].ledger_scenario_id = "stress".into(),
            3 => request.specification["accounts"][0]["currency"] = json!("EUR"),
            4 => request.specification["horizon_days"] = json!(30),
            5 => request.recognition_policy.recognition_model = "monthly_guess".into(),
            _ => request.capture_policy.book_basis = "effective_yield_guess".into(),
        }
        assert!(prepare(&request).is_err(), "case{case}");
    }
}

#[test]
fn default_nonaccrual_and_fees_reject_entire_source_before_any_writer_call() {
    for action in [
        json!({"kind":"default"}),
        json!({"kind":"set_state","state":"nonaccrual"}),
        json!({"kind":"fee","amount":2}),
    ] {
        let mut value = raw(&request());
        physical_step(&mut value, json!({"ordinal":738945,"action":action}));
        let data = Arc::new(Mutex::new(Vec::new()));
        assert!(run(&decode(value), Box::new(Capture(data.clone()))).is_err());
        assert!(data.lock().unwrap().is_empty());
    }
}

#[test]
fn legacy_cashflow_credit_and_sale_overlaps_cannot_double_post_dated_owner() {
    for case in 0..4 {
        let mut request = request();
        match case {
            0 => {
                request.specification["cashflows"] = json!([{"position":"personal-1","day":29,"principal":500,"cash_interest":29,"accrual_interest":29}])
            }
            1 => request.specification["positions"][0]["annual_pd"] = json!(0.01),
            2 => request.specification["positions"][0]["floating_beta"] = json!(1),
            _ => {
                request.specification["policies"] = json!([{"id":"sale","account":"bank-loans","kind":"sell","position":"personal-1","trigger_cash":100,"limit":100}])
            }
        }
        assert!(prepare(&request).is_err(), "case{case}");
    }
}

#[test]
fn same_date_distinct_positions_keep_stable_sources_and_once_only_cash() {
    let mut value = raw(&request());
    let mut position = value["graph"]["positions"][0].clone();
    position["position_id"] = json!("personal-2");
    position["product"]["contract"]["identity"]["position_id"] = json!("personal-2");
    value["graph"]["positions"]
        .as_array_mut()
        .unwrap()
        .push(position);
    let mut owner = value["owners"][0].clone();
    owner["position_id"] = json!("personal-2");
    owner["source_id"] = json!("consumer-source-2");
    value["owners"].as_array_mut().unwrap().push(owner);
    value["specification"]["positions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"personal-2","account":"bank-loans","kind":"loan"}));
    value["specification"]["accounts"][0]["equity"] = json!(2000);
    let (_, messages) = execute(&decode(value));
    near(balance(&messages, "baseline", "cash"), 2089.);
    assert_eq!(
        rows(&messages, "event_lineage")
            .filter(|v| v["phase"] == "settlement" && v["scenario"] == "baseline")
            .count(),
        4
    );
}

#[test]
fn mixed_legacy_position_is_preserved_and_executed_independently() {
    let mut request = request();
    request.specification["positions"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"id":"legacy","account":"bank-loans","kind":"loan","balance":100,"rate":0.365}),
        );
    request.specification["accounts"][0]["equity"] = json!(1100);
    let (_, messages) = execute(&request);
    near(balance(&messages, "baseline", "asset_principal"), 100.);
    near(balance(&messages, "baseline", "cash"), 1050.5);
    near(
        balance(&messages, "baseline", "pnl:dated_recognition"),
        -44.5,
    );
}

struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("fixture write failure"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn failed_stream_returns_no_receipt_or_success() {
    assert!(run(&request(), Box::new(FailedWriter)).is_err());
}

#[test]
fn no_event_opening_has_explicit_coverage_and_bounded_identity() {
    let mut request = request();
    let Product::ConsumerInstallment { contract, .. } = &mut request.graph.positions[0].product;
    contract.opening_principal = 0.;
    request.specification["accounts"][0]["equity"] = json!(0);
    let prepared = prepare(&request).unwrap();
    assert!(prepared.dated_request.dated.sources[0]
        .schedule
        .events
        .is_empty());
    assert_eq!(
        serde_json::to_value(&prepared.dated_request.dated.sources[0].coverage[0].status).unwrap()
            ["kind"],
        "no_events"
    );
    request.run_mapping[0].ledger_scenario_id = "x".repeat(121);
    assert!(prepare(&request).is_err());
}
