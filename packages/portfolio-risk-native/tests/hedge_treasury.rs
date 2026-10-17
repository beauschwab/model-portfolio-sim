//! Closing capital from the formal hedge journal: row mapping, AOCI policy and refusals.
use portfolio_risk_native::hedge_treasury::{self as feed, Request};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/hedge-treasury-1.json"
    ))
    .unwrap()
}
fn run(v: &Value) -> Result<feed::HedgeTreasuryResult, String> {
    let request: Request = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    feed::evaluate(&request)
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1. + a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
fn row(r: &feed::HedgeTreasuryResult, gl: &str) -> f64 {
    r.rows
        .iter()
        .filter(|x| x.gl_account == gl)
        .map(|x| x.balance)
        .sum()
}

#[test]
fn example_maps_closing_balances_and_excludes_cash_flow_hedge_aoci_by_policy() {
    let result = run(&raw()).unwrap();
    assert_eq!(result.closing_date, "2024-04-01");
    assert_eq!(result.scenario, "physical");
    let hedge = &result.hedge;
    let closing = hedge.daily.last().unwrap();
    let balance = |account: portfolio_risk_native::hedge_accounting::Account| {
        closing
            .financial_trial_balance
            .iter()
            .find(|b| b.account == account)
            .map(|b| b.debit_balance)
            .unwrap_or(0.)
    };
    use portfolio_risk_native::hedge_accounting::Account;
    close(row(&result, "cash"), balance(Account::Cash));
    close(
        row(&result, "cash_flow_hedge_oci"),
        balance(Account::CashFlowHedgeOci),
    );
    close(
        row(&result, "hedged_item_basis_adjustment"),
        balance(Account::HedgedItemBasisAdjustment),
    );
    close(
        row(&result, "pnl:derivative_mark"),
        balance(Account::MarkGainLoss),
    );
    close(
        row(&result, "pnl:hedged_forecast_reclassification"),
        balance(Account::HedgedForecastReclassification),
    );
    // The fourth forecast period is pending, so cash-flow-hedge AOCI is nonzero at the closing date.
    let oci = row(&result, "cash_flow_hedge_oci");
    assert!(oci.abs() > 1e-6, "{oci}");
    assert!(result.rows.iter().all(|r| r.balance.abs() > feed::ZERO_ROW
        && r.account == "bank"
        && r.scenario == "physical"));
    assert!(!result.rows.iter().any(|r| r.gl_account.starts_with("memo")));
    let audit = &result.capital["capital_bridge"][0];
    close(audit["cash_flow_hedge_aoci"].as_f64().unwrap(), -oci);
    close(audit["book_aoci"].as_f64().unwrap(), -oci);
    close(audit["eligible_aoci"].as_f64().unwrap(), 0.);
    assert!(!result.cash_flow_hedge_aoci_eligible);
    let capital = &result.capital["prepared_specification"]["capital"][0];
    close(capital["eligible_aoci"].as_f64().unwrap(), 0.);
    close(
        capital["retained_earnings"].as_f64().unwrap(),
        audit["retained_earnings"].as_f64().unwrap(),
    );
    assert_eq!(
        result.capital["capital_metrics"].as_array().unwrap().len(),
        13
    );
    assert!(result.capital["ftp_positions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        result.closing_capital_selected
            && !result.interim_capital
            && !result.ftp_selected
            && !result.accounting_standard_approval
    );
    // Eligible when the reviewed policy says so.
    let mut eligible = raw();
    eligible["policies"][0]["cash_flow_hedge_aoci_eligible"] = json!(true);
    let result = run(&eligible).unwrap();
    let audit = &result.capital["capital_bridge"][0];
    // Eligible AOCI = included x (1 - deferred tax) x phase-in; the deferred tax is disclosed, not posted.
    close(audit["included_aoci"].as_f64().unwrap(), -oci);
    close(audit["deferred_tax_on_aoci"].as_f64().unwrap(), oci * 0.21);
    close(audit["eligible_aoci"].as_f64().unwrap(), -oci * 0.79 * 0.4);
    assert_eq!(audit["aoci_phase_in_fraction"], json!(0.4));
    assert!(result.cash_flow_hedge_aoci_eligible);
    let mut opted_out = eligible.clone();
    opted_out["policies"][0]["aoci_eligibility"]["opt_out_elected"] = json!(true);
    let result = run(&opted_out).unwrap();
    close(
        result.capital["capital_bridge"][0]["eligible_aoci"]
            .as_f64()
            .unwrap(),
        0.,
    );
    let mut untreated = eligible.clone();
    untreated["policies"][0]["aoci_eligibility"] = Value::Null;
    assert!(run(&untreated).unwrap_err().contains("reviewed AOCI"));
}

type Change = Box<dyn Fn(&mut Value)>;
#[test]
fn explicit_refusals() {
    let cases: Vec<(&str, Change)> = vec![
        (
            "schema",
            Box::new(|v| v["schema"] = json!("hedge-treasury-2")),
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
            Box::new(|v| v["policies"][0]["scenario"] = json!("stress")),
        ),
        (
            "multiplier",
            Box::new(|v| v["amount_multiplier"] = json!(0.)),
        ),
        (
            "supplied capital",
            Box::new(|v| v["treasury"]["capital"] = json!([{"entity":"bank"}])),
        ),
        (
            "missing risk weight",
            Box::new(|v| v["policies"][0]["gl_risk_weights"] = json!({"cash": 0.})),
        ),
        (
            "negative basis weight",
            Box::new(|v| {
                v["policies"][0]["gl_risk_weights"]["hedged_item_basis_adjustment"] = json!(1.)
            }),
        ),
        (
            "tax rate",
            Box::new(|v| v["policies"][0]["aoci_eligibility"]["deferred_tax_rate"] = json!(1.)),
        ),
        (
            "phase-in",
            Box::new(|v| v["policies"][0]["aoci_eligibility"]["phase_in_fraction"] = json!(1.5)),
        ),
        (
            "unknown field",
            Box::new(|v| v["policies"][0]["ftp"] = json!(true)),
        ),
        (
            "hedge refusal",
            Box::new(|v| v["hedge"]["policy"]["rule_version"] = json!("x")),
        ),
    ];
    for (label, change) in cases {
        let mut v = raw();
        change(&mut v);
        assert!(run(&v).is_err(), "{label} must reject");
    }
}

#[test]
fn protocol_dispatch() {
    let envelope = json!({"schema":"hedge-treasury-1","threads":1,"execution_timeout_ms":60000,"request":raw()});
    let (out, ok) = portfolio_risk_native::term_protocol::respond(
        serde_json::to_vec(&envelope).unwrap().as_slice(),
    );
    assert!(ok, "{}", String::from_utf8_lossy(&out));
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["result"]["schema"], json!("hedge-treasury-1"));
    assert_eq!(
        value["result"]["capital"]["capital_metrics"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
}
