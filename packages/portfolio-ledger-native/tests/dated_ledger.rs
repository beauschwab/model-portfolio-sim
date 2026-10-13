mod common;
use common::request;
use portfolio_ledger_native::{
    daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn compile(value: Value) -> Result<CompiledDatedLedger, String> {
    let req: DatedLedgerRequest = serde_json::from_value(value).unwrap();
    CompiledDatedLedger::compile(req, 60000, 90_000_000)
}
fn run(value: Value) -> Vec<Value> {
    let capture = Capture(Arc::new(Mutex::new(vec![])));
    let data = capture.0.clone();
    daily::run_dated(compile(value).unwrap(), Box::new(capture)).unwrap();
    let bytes = data.lock().unwrap();
    String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn rows<'a>(messages: &'a [Value], table: &'a str) -> impl Iterator<Item = &'a Value> {
    messages
        .iter()
        .filter(move |v| v["table"] == table)
        .flat_map(|v| v["rows"].as_array().unwrap())
}
fn balance(messages: &[Value], gl: &str) -> f64 {
    rows(messages, "trial_balance")
        .filter(|v| v["scenario"] == "baseline" && v["gl_account"] == gl)
        .map(|v| v["balance"].as_f64().unwrap())
        .sum()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn leap_calendar_coupon_recognized_daily_and_legacy_models_bypassed() {
    let messages = run(request(3, 3, 3., 100., false));
    assert_eq!(
        messages.last().unwrap(),
        &json!({"complete":true,"protocol":3})
    );
    close(balance(&messages, "cash"), 103.);
    close(balance(&messages, "asset_principal"), 0.);
    close(balance(&messages, "pnl:dated_recognition"), -3.);
    let recognition: Vec<_> = rows(&messages, "event_lineage")
        .filter(|r| r["scenario"] == "baseline" && r["phase"] == "recognition")
        .collect();
    assert_eq!(recognition.len(), 3);
    for (i, row) in recognition.iter().enumerate() {
        assert_eq!(row["day"], i + 1);
        close(row["accrued_interest"].as_f64().unwrap(), 1.);
    }
    assert_eq!(
        rows(&messages, "event_lineage")
            .filter(|r| r["scenario"] == "baseline" && r["phase"] == "settlement")
            .count(),
        1
    );
    let base = rows(&messages, "closing_statements")
        .find(|r| r["scenario"] == "baseline")
        .unwrap();
    let shock = rows(&messages, "closing_statements")
        .find(|r| r["scenario"] == "shock")
        .unwrap();
    close(
        base["assets"].as_f64().unwrap(),
        shock["assets"].as_f64().unwrap(),
    );
}
#[test]
fn early_principal_coupon_preserves_deferred_income_after_principal_ends() {
    let messages = run(request(0, 60, 6., 100., false));
    close(balance(&messages, "cash"), 106.);
    close(balance(&messages, "asset_principal"), 0.);
    close(balance(&messages, "dated_deferred_income"), -3.);
    close(balance(&messages, "pnl:dated_recognition"), -3.);
    let statement = rows(&messages, "closing_statements")
        .find(|r| r["scenario"] == "baseline")
        .unwrap();
    close(statement["assets"].as_f64().unwrap(), 106.);
    close(statement["liabilities"].as_f64().unwrap(), 3.);
    let right = rows(&messages, "dated_rights")
        .find(|r| r["scenario"] == "baseline")
        .unwrap();
    assert_eq!(right["cash_settled"], true);
    close(
        right["remaining_interest_recognition"].as_f64().unwrap(),
        3.,
    );
}
#[test]
fn delayed_interest_and_full_contract_tail_are_not_clipped_to_reporting() {
    let messages = run(request(60, 30, 6., 100., false));
    close(balance(&messages, "cash"), 0.);
    close(balance(&messages, "asset_principal"), 100.);
    close(balance(&messages, "dated_interest_receivable"), 6.);
    let right = rows(&messages, "dated_rights")
        .find(|r| r["scenario"] == "baseline")
        .unwrap();
    close(
        right["remaining_principal_settlement"].as_f64().unwrap(),
        100.,
    );
    close(right["remaining_cash_interest"].as_f64().unwrap(), 6.);
}
#[test]
fn early_liability_coupon_creates_prepaid_expense_and_recognizes_after_maturity() {
    let messages = run(request(0, 60, 6., 0., true));
    close(balance(&messages, "cash"), 94.);
    close(balance(&messages, "funding_principal"), -100.);
    close(balance(&messages, "dated_prepaid_expense"), 3.);
    close(balance(&messages, "pnl:dated_recognition"), 3.);
    close(balance(&messages, "dated_interest_payable"), 0.);
}
#[test]
fn same_day_events_keep_distinct_once_only_settlements() {
    let mut value = request(2, 2, 2., 50., false);
    let mut second = value["dated"]["sources"][0]["schedule"]["events"][0].clone();
    second["event_id"] = json!("coupon-2");
    value["dated"]["sources"][0]["schedule"]["events"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let mut plan = value["dated"]["accrual_plans"][0].clone();
    plan["event_id"] = json!("coupon-2");
    value["dated"]["accrual_plans"]
        .as_array_mut()
        .unwrap()
        .push(plan);
    let messages = run(value);
    close(balance(&messages, "cash"), 104.);
    let settlements: Vec<_> = rows(&messages, "event_lineage")
        .filter(|r| r["scenario"] == "baseline" && r["phase"] == "settlement")
        .collect();
    assert_eq!(settlements.len(), 2);
    assert_eq!(settlements[0]["event_id"], "coupon-1");
    assert_eq!(settlements[1]["event_id"], "coupon-2");
}
#[test]
fn mixed_book_preserves_legacy_interest_and_suppresses_only_dated_owner() {
    let mut value = request(3, 3, 3., 100., false);
    value["specification"]["positions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"legacy","account":"a","kind":"loan","balance":100.,"rate":0.365}));
    value["specification"]["accounts"][0]["equity"] = json!(200.);
    let messages = run(value);
    close(balance(&messages, "pnl:interest_income"), -3.);
    close(balance(&messages, "pnl:dated_recognition"), -3.);
}
#[test]
fn ownership_overlap_credit_units_and_missing_reverse_fail_before_execution() {
    let value = request(3, 3, 3., 100., false);
    for mutate in [0, 1, 2, 3, 4, 5] {
        let mut bad = value.clone();
        match mutate {
            0 => {
                bad["specification"]["cashflows"] =
                    json!([{"position":"p","day":3,"principal":100.}])
            }
            1 => bad["specification"]["positions"][0]["annual_pd"] = json!(0.1),
            2 => bad["monetary_unit"] = json!("millions"),
            3 => bad["specification"]["reverse_severities"] = json!([0., 1.]),
            4 => bad["dated"]["owners"][0]["currency"] = json!("EUR"),
            _ => bad["dated"]["sources"][0]["schedule"]["events"][0]["measure"] = json!("pricing"),
        }
        assert!(compile(bad).is_err(), "mutation {mutate} should fail");
    }
}
#[test]
fn recognition_work_is_part_of_combined_admission() {
    let value = request(3, 3, 3., 100., false);
    let source = compile(value.clone()).unwrap();
    let req = serde_json::from_value(value).unwrap();
    assert!(CompiledDatedLedger::compile(req, 60000, source.work_units - 1).is_err());
}

#[test]
fn day_zero_cash_fees_tax_and_activity_close_on_day_zero_only() {
    let mut value = request(0, 3, 0., 100., false);
    value["dated"]["sources"][0]["schedule"]["events"][0]["components"]["fees"] = json!(2.);
    value["specification"]["accounts"][0]["tax_rate"] = json!(0.5);
    let messages = run(value);
    let activity: Vec<_> = rows(&messages, "ledger")
        .filter(|r| r["scenario"] == "baseline")
        .collect();
    assert_eq!(activity.len(), 2);
    assert!(activity.iter().all(|r| r["day"] == 0));
    let settlement = activity
        .iter()
        .find(|r| r["event"] == "dated_settlement")
        .unwrap();
    close(settlement["cash"].as_f64().unwrap(), 102.);
    close(balance(&messages, "cash"), 101.);
    close(balance(&messages, "pnl:tax"), 1.);
}

#[test]
fn callers_cannot_raise_the_engine_admission_tier() {
    for (positions, work) in [(60001, 90_000_000), (60000, 90_000_001)] {
        let req = serde_json::from_value(request(3, 3, 3., 100., false)).unwrap();
        assert!(CompiledDatedLedger::compile(req, positions, work).is_err());
    }
}
