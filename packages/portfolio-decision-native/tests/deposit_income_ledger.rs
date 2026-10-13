use portfolio_decision_native::deposit_ledger::income::{prepare, run, Request};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/deposit-graph-income-ledger-1.json"
    ))
    .unwrap()
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
fn execute(r: &Request) -> (Value, Vec<Value>) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let receipt = run(r, Box::new(Capture(bytes.clone()))).unwrap();
    let data = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    (
        serde_json::to_value(receipt).unwrap(),
        data.lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect(),
    )
}
fn balance(messages: &[Value], gl: &str) -> f64 {
    messages
        .iter()
        .filter(|m| m["table"] == "trial_balance")
        .flat_map(|m| m["rows"].as_array().unwrap())
        .filter(|r| r["scenario"] == "baseline" && r["gl_account"] == gl)
        .map(|r| r["balance"].as_f64().unwrap())
        .sum()
}
#[test]
fn day30_reports_original_eir_not_cash_coupon_with_fulltail_retained() {
    let r = request();
    let c = r.income_anchors[0].original_gross_carrying_value;
    let expense = c * (1.1_f64.powf(30. / 365.) - 1.);
    let (receipt, m) = execute(&r);
    near(balance(&m, "cash"), c);
    near(balance(&m, "funding_principal"), -1000.);
    near(balance(&m, "dated_interest_payable"), -30.);
    near(balance(&m, "book_adjustment"), -(c + expense - 1030.));
    near(balance(&m, "pnl:dated_recognition"), expense);
    assert_eq!(receipt["raw_request"], serde_json::to_value(&r).unwrap());
    assert_eq!(
        receipt["income_sources"][0]["anchor"]["original_yield_anchor"]["id"],
        "original-book-yield"
    );
}
#[test]
fn fulltail_closes_cash_principal_accrual_basis_and_effective_expense() {
    let mut r = request();
    r.graph.reporting_ordinals.push(r.graph.as_of_ordinal + 31);
    r.specification["horizon_days"] = json!(31);
    let c = r.income_anchors[0].original_gross_carrying_value;
    let (_, m) = execute(&r);
    near(balance(&m, "cash"), c - 1031.);
    near(balance(&m, "funding_principal"), 0.);
    near(balance(&m, "dated_interest_payable"), 0.);
    near(balance(&m, "book_adjustment"), 0.);
    near(balance(&m, "pnl:dated_recognition"), 1031. - c);
}
#[test]
fn raw_original_book_anchors_are_never_overwritten() {
    let r = request();
    for key in ["balance", "opening_accrued", "book_adjustment"] {
        let mut bad = r.clone();
        bad.specification["positions"][0][key] = json!(999.);
        assert!(prepare(&bad).is_err());
    }
    let mut omitted = r.clone();
    omitted.specification["positions"][0]
        .as_object_mut()
        .unwrap()
        .remove("book_adjustment");
    let prepared = prepare(&omitted).unwrap();
    assert_eq!(
        prepared.dated_request.specification["positions"][0]["book_adjustment"],
        r.specification["positions"][0]["book_adjustment"]
    );
    assert!(omitted.specification["positions"][0]
        .get("book_adjustment")
        .is_none());
}
#[test]
fn schema_policy_and_missing_reverse_income_coverage_reject() {
    let mut r = request();
    r.schema = "deposit-graph-dated-ledger-1".into();
    assert!(prepare(&r).is_err());
    let mut r = request();
    r.capture_policy.recognition_model = "deposit_daily_par-1".into();
    assert!(prepare(&r).is_err());
    let mut r = request();
    r.run_mapping.pop();
    assert!(prepare(&r).is_err());
}
