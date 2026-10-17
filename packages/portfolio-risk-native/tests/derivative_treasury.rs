//! Capital from a standalone reviewed derivative journal (derivative-treasury-1): rows reconcile to
//! the derivative owner's trial balance, no hedge accounting is applied, and refusals are explicit.
use portfolio_risk_native::derivative_treasury::{self as treasury, *};
use portfolio_risk_native::hedge_accounting::Account;
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/derivative-treasury-1.json"
    ))
    .unwrap()
}
fn run(v: &Value) -> Result<DerivativeTreasuryResult, String> {
    let request: Request = serde_json::from_value(v.clone()).expect("example parses");
    treasury::evaluate(&request)
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1. + a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
fn row(result: &DerivativeTreasuryResult, gl: &str) -> f64 {
    result
        .rows
        .iter()
        .filter(|r| r.gl_account == gl)
        .map(|r| r.balance)
        .sum()
}
fn balance(result: &DerivativeTreasuryResult, ordinal: i32, account: Account) -> f64 {
    let day = result
        .derivative
        .daily
        .iter()
        .find(|d| d.ordinal == ordinal)
        .unwrap();
    day.financial_trial_balance
        .iter()
        .find(|b| Account::from(b.account) == account)
        .map(|b| b.debit_balance)
        .unwrap_or(0.)
}

#[test]
fn example_maps_the_derivative_journal_without_hedge_accounting() {
    let result = run(&raw()).unwrap();
    assert_eq!(result.closing_date, "2024-04-01");
    assert_eq!(result.scenario, "physical");
    assert!(result.closing_capital_selected && !result.interim_capital);
    assert!(!result.hedge_accounting_applied && !result.accounting_standard_approval);
    let t = result.closing_ordinal;
    close(row(&result, "cash"), balance(&result, t, Account::Cash));
    close(
        row(&result, "pnl:derivative_mark"),
        balance(&result, t, Account::MarkGainLoss),
    );
    close(
        row(&result, "opening_equity"),
        balance(&result, t, Account::OpeningEquity),
    );
    // Every mark stays in earnings: there is no OCI or basis row, and AOCI is zero.
    assert!(result
        .rows
        .iter()
        .all(|r| !r.gl_account.contains("hedge") && r.balance.abs() > 1e-9));
    let audit = &result.capital["capital_bridge"][0];
    close(audit["book_aoci"].as_f64().unwrap(), 0.);
    close(audit["eligible_aoci"].as_f64().unwrap(), 0.);
    assert_eq!(
        result.capital["capital_metrics"].as_array().unwrap().len(),
        13
    );
    assert_eq!(result.capital["ftp_positions"], json!([]));
    assert_eq!(result.rows.len(), 6, "{:?}", result.rows);
    // Explicit interim report: rows through 1 March reconcile to that day's balances.
    let mut interim = raw();
    interim["interim_report"] = json!(true);
    interim["treasury"]["as_of"] = json!("2024-03-01");
    interim["policies"][0]["capital"]["period"] = json!("2024-03-01");
    let result = run(&interim).unwrap();
    assert!(result.interim_capital && result.closing_date == "2024-03-01");
    let t = result.closing_ordinal;
    close(row(&result, "cash"), balance(&result, t, Account::Cash));
    close(
        row(&result, "derivative_value"),
        balance(&result, t, Account::DerivativeAsset)
            + balance(&result, t, Account::DerivativeLiability),
    );
    assert!(result.disclosure.contains("no cash-flow-hedge AOCI"));
}

type Change = Box<dyn Fn(&mut Value)>;
#[test]
fn refusals_without_fallback() {
    let cases: Vec<(&str, Change)> = vec![
        (
            "schema",
            Box::new(|v| v["schema"] = json!("hedge-treasury-1")),
        ),
        (
            "as_of",
            Box::new(|v| v["treasury"]["as_of"] = json!("2024-03-01")),
        ),
        (
            "period",
            Box::new(|v| v["policies"][0]["capital"]["period"] = json!("2024-03-01")),
        ),
        (
            "scenario",
            Box::new(|v| v["policies"][0]["scenario"] = json!("other")),
        ),
        (
            "multiplier",
            Box::new(|v| v["amount_multiplier"] = json!(0.)),
        ),
        (
            "supplied capital",
            Box::new(|v| v["treasury"]["capital"] = json!([{"x": 1}])),
        ),
        ("policy count", Box::new(|v| v["policies"] = json!([]))),
        (
            "interim at tail",
            Box::new(|v| v["interim_report"] = json!(true)),
        ),
        ("unknown field", Box::new(|v| v["hedge"] = json!({}))),
    ];
    for (label, change) in cases {
        let mut v = raw();
        change(&mut v);
        let outcome = serde_json::from_value::<Request>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| treasury::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
    let envelope = json!({"schema": "derivative-treasury-1", "threads": 1, "execution_timeout_ms": 60000, "request": raw()});
    let (out, ok) = portfolio_risk_native::term_protocol::respond(
        serde_json::to_vec(&envelope).unwrap().as_slice(),
    );
    assert!(ok, "{}", String::from_utf8_lossy(&out));
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["result"]["schema"], json!("derivative-treasury-1"));
    assert_eq!(value["result"]["hedge_accounting_applied"], json!(false));
}
