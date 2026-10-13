use portfolio_ledger_native::credit_replay::{self, CompiledCreditReplay, CreditReplayRequest};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/credit-ledger-1.json"
    ))
    .unwrap()
}
fn compile(value: Value) -> Result<CompiledCreditReplay, String> {
    CompiledCreditReplay::compile(serde_json::from_value(value).unwrap())
}
fn run(value: Value) -> Vec<Value> {
    let mut data = vec![];
    credit_replay::run(compile(value).unwrap(), &mut data).unwrap();
    String::from_utf8(data)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn rows<'a>(messages: &'a [Value], table: &'a str) -> impl Iterator<Item = &'a Value> {
    messages
        .iter()
        .filter(move |m| m["table"] == table)
        .flat_map(|m| m["rows"].as_array().unwrap())
}
fn balance(messages: &[Value], name: &str) -> f64 {
    rows(messages, "trial_balance")
        .filter(|r| r["gl_account"] == name && r["scenario"] == "baseline")
        .map(|r| r["balance"].as_f64().unwrap())
        .sum()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-9, "{a} != {b}");
}
fn no_hazards(value: &mut Value) {
    value["runs"][0]["positions"][0]["contract"]["transition_hazards_per_year"] =
        serde_json::to_value([[0.0; 5]; 3]).unwrap();
}

#[test]
fn full_daily_default_prepay_accrual_maturity_and_recovery_reconcile_independently() {
    let messages = run(example());
    assert_eq!(
        messages.last().unwrap(),
        &json!({"complete":true,"protocol":4})
    );
    let t = 2. / 365.;
    let s = (-0.5_f64 * t).exp();
    let d = 0.2 / 0.5 * (1. - s);
    let p = 0.3 / 0.5 * (1. - s);
    // Analytic earned default/prepay integrals with stopped accrual at each exit.
    let flux = (1. - s * (1. + 0.5 * t)) / (0.5 * 0.5);
    let default_interest = 100. * 0.05 * 0.2 * flux;
    let prepay_interest = 100. * 0.05 * 0.3 * flux;
    let mature_interest = 100. * 0.05 * t * s;
    let recoveries = 110. * d * 0.6;
    let cash = 10. + 100. * (s + p) + mature_interest + prepay_interest - 10. * d + recoveries;
    close(balance(&messages, "cash"), cash);
    close(balance(&messages, "asset_principal"), 0.);
    close(
        balance(&messages, "credit_default_principal"),
        110. * d * 0.4,
    );
    close(
        balance(&messages, "credit_default_accrued_interest"),
        default_interest,
    );
    close(
        balance(&messages, "credit_default_book_adjustment"),
        -5. * d,
    );
    close(
        balance(&messages, "interest_income"),
        -100. * 0.05 * (1. - s) / 0.5,
    );
    close(
        balance(&messages, "basis_recognition_expense"),
        -5. * (s + p),
    );
    close(balance(&messages, "allowance"), 0.); // Never provision the modeled EL automatically.
    let final_state = rows(&messages, "credit_states").last().unwrap();
    close(final_state["matured_probability"].as_f64().unwrap(), s);
    close(final_state["probability"][3].as_f64().unwrap(), d);
    close(final_state["probability"][6].as_f64().unwrap(), p);
    assert_eq!(final_state["live_principal"], json!([0., 0., 0.]));
    let claims: Vec<_> = rows(&messages, "credit_claims")
        .filter(|r| r["day"] == 30 && r["scenario"] == "baseline")
        .collect();
    assert_eq!(claims.len(), 2);
    close(
        claims
            .iter()
            .map(|r| r["recovery_cash_collected"].as_f64().unwrap())
            .sum(),
        recoveries,
    );
    let mut transactions = BTreeMap::new();
    for r in rows(&messages, "journal") {
        *transactions
            .entry(r["transaction_number"].as_u64().unwrap())
            .or_insert(0.) += r["value"].as_f64().unwrap();
    }
    for total in transactions.values() {
        close(*total, 0.);
    }
    for row in rows(&messages, "daily_balance") {
        close(row["reconciliation_error"].as_f64().unwrap(), 0.);
    }
    assert!(rows(&messages, "credit_lineage").all(|r| r["account"] == "bank-usd"));
}

#[test]
fn repayment_on_surviving_state_does_not_double_apply_survival() {
    let mut value = example();
    let pos = &mut value["runs"][0]["positions"][0];
    let mut maturity = pos["boundaries"][0].clone();
    maturity["event_id"] = json!("actual-maturity");
    pos["contract"]["accrual_rates"] = json!([0., 0., 0.]);
    pos["boundaries"][0]["recognition_ordinal"] = json!(739618);
    pos["boundaries"][0]["action"] = json!({"kind":"repayment","principal_fraction":[0.5,0.5,0.5],"accrued_fraction":[0.,0.,0.],"cancel_undrawn_fraction":[0.,0.,0.]});
    pos["boundaries"].as_array_mut().unwrap().push(maturity);
    let messages = run(value);
    let first = rows(&messages, "credit_states")
        .find(|r| r["day"] == 1)
        .unwrap();
    let survivor = (-0.5_f64 / 365.).exp();
    close(first["live_principal"][0].as_f64().unwrap(), 50. * survivor);
    let repayment_cash = rows(&messages, "journal")
        .filter(|r| {
            r["day"] == 1
                && r["event"] == "loan-1-maturity"
                && r["gl_account"] == "cash"
                && r["scenario"] == "baseline"
        })
        .map(|r| r["value"].as_f64().unwrap())
        .sum();
    close(repayment_cash, 50. * survivor);
}

#[test]
fn ordinary_draw_and_late_interest_right_settlement_survive_principal_end() {
    let mut value = example();
    no_hazards(&mut value);
    value["specification"]["positions"][0]["maturity_day"] = json!(4);
    for run in value["runs"].as_array_mut().unwrap() {
        run["positions"][0]["boundaries"][0]["recognition_ordinal"] = json!(739621);
    }
    let p = &mut value["runs"][0]["positions"][0];
    p["contract"]["accrual_rates"] = json!([-0.1, 0., 0.]);
    let source = json!({"id":"explicit-contract","revision":"r1"});
    let policy = json!({"id":"explicit-settlement","revision":"r1"});
    p["boundaries"] = json!([
        {"event_id":"fund-draw","source":source,"policy":policy,"recognition_ordinal":739618,"action":{"kind":"draw","amount_by_state":[10.,0.,0.]}},
        {"event_id":"principal-end","source":source,"policy":policy,"recognition_ordinal":739619,"action":{"kind":"repayment","principal_fraction":[1.,1.,1.],"accrued_fraction":[0.,0.,0.],"cancel_undrawn_fraction":[1.,1.,1.]}},
        {"event_id":"retained-interest-right","source":source,"policy":policy,"recognition_ordinal":739621,"action":{"kind":"repayment","principal_fraction":[0.,0.,0.],"accrued_fraction":[1.,1.,1.],"cancel_undrawn_fraction":[0.,0.,0.]}},
        {"event_id":"actual-maturity","source":source,"policy":policy,"recognition_ordinal":739621,"action":{"kind":"maturity"}}
    ]);
    let messages = run(value);
    let day3 = rows(&messages, "credit_states")
        .find(|r| r["day"] == 3)
        .unwrap();
    close(day3["live_principal"][0].as_f64().unwrap(), 0.);
    close(
        day3["live_accrued_interest"][0].as_f64().unwrap(),
        -21. / 365.,
    );
    close(balance(&messages, "cash"), 110. - 21. / 365.);
    close(balance(&messages, "interest_income"), 21. / 365.);
    close(balance(&messages, "book_adjustment"), 0.);
    close(balance(&messages, "basis_recognition_expense"), -5.);
}

#[test]
fn scenario_and_reverse_runs_must_be_supplied_exactly_without_inference() {
    let mut value = example();
    value["specification"]["scenarios"] = json!([{"name":"stress"}]);
    value["specification"]["reverse_severities"] = json!([0.5]);
    let baseline = value["runs"][0].clone();
    value["runs"] = json!([baseline]);
    assert!(compile(value.clone()).err().unwrap().contains("coverage"));
    let mut stress = value["runs"][0].clone();
    stress["run_id"] = json!("stress-1");
    stress["scenario_id"] = json!("stress");
    let mut reverse = stress.clone();
    reverse["run_id"] = json!("stress-half");
    reverse["severity"] = json!(0.5);
    value["runs"]
        .as_array_mut()
        .unwrap()
        .extend([stress, reverse]);
    let messages = run(value.clone());
    assert_eq!(rows(&messages, "summary").count(), 3);
    value["runs"][2]["severity"] = json!(0.75);
    assert!(compile(value).is_err());
}

#[test]
fn ownership_opening_policy_and_admission_fail_closed() {
    let mut value = example();
    value["specification"]["positions"][0]["annual_pd"] = json!(0.1);
    assert!(compile(value).is_err());
    value = example();
    value["runs"][0]["positions"][0]["opening"]["accounting"]["live"][0]["principal"] = json!(99.);
    assert!(compile(value).is_err());
    value = example();
    value["runs"][0]["positions"][0]["contract"]["measure"] = json!("pricing");
    assert!(compile(value).is_err());
    let request: CreditReplayRequest = serde_json::from_value(example()).unwrap();
    assert!(CompiledCreditReplay::compile_with_limits(request, 0, 3_000_000).is_err());
    let request = serde_json::from_value(example()).unwrap();
    assert!(CompiledCreditReplay::compile_with_limits(request, 2_000, 1).is_err());
    let request = serde_json::from_value(example()).unwrap();
    let compiled = CompiledCreditReplay::compile_with_limits(request, 2_000, 3_000_000).unwrap();
    assert_eq!(compiled.normalized_spec().positions.len(), 1);
    assert_eq!(compiled.admitted_work(), 122);
}

#[test]
fn failed_dynamic_draw_emits_no_completion_and_legacy_cli_remains_separate() {
    let mut value = example();
    let p = &mut value["runs"][0]["positions"][0];
    let maturity = p["boundaries"][0].clone();
    p["boundaries"][0]["action"] = json!({"kind":"draw","amount_by_state":[100.,0.,0.]});
    p["boundaries"][0]["event_id"] = json!("failed-draw");
    p["boundaries"].as_array_mut().unwrap().push(maturity);
    let mut bytes = vec![];
    assert!(credit_replay::run(compile(value).unwrap(), &mut bytes).is_err());
    assert!(!String::from_utf8(bytes)
        .unwrap()
        .contains("\"complete\":true"));
    let mut invalid = example();
    invalid["runs"][0]["positions"][0]["boundaries"][0]
        .as_object_mut()
        .unwrap()
        .remove("policy");
    assert!(serde_json::from_value::<CreditReplayRequest>(invalid).is_err());
}

#[test]
fn missing_mismatched_and_duplicate_maturity_anchors_fail_before_execution() {
    let mut value = example();
    value["runs"][0]["positions"][0]["boundaries"] = json!([]);
    assert!(compile(value).is_err());
    value = example();
    value["runs"][0]["positions"][0]["boundaries"][0]["recognition_ordinal"] = json!(739620);
    assert!(compile(value).is_err());
    value = example();
    let mut duplicate = value["runs"][0]["positions"][0]["boundaries"][0].clone();
    duplicate["event_id"] = json!("duplicate-maturity");
    value["runs"][0]["positions"][0]["boundaries"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(compile(value).is_err());
    value = example();
    value["specification"]["positions"][0]["maturity_day"] = json!(31);
    for run in value["runs"].as_array_mut().unwrap() {
        run["positions"][0]["boundaries"] = json!([]);
    }
    assert!(compile(value).is_ok());
}

#[test]
fn daily_cure_accrues_only_after_return_to_performing() {
    let mut value = example();
    let p = &mut value["runs"][0]["positions"][0];
    p["contract"]["transition_hazards_per_year"] = json!([
        [0., 0., 0., 0., 0.],
        [1., 0., 0., 0., 0.],
        [0., 0., 0., 0., 0.]
    ]);
    let live = p["opening"]["accounting"]["live"][0].clone();
    p["opening"]["accounting"]["live"][1] = live;
    p["opening"]["accounting"]["live"][0] =
        json!({"principal":0.,"undrawn":0.,"accrued_interest":0.,"book_adjustment":0.});
    p["opening"]["probability"] = json!([0., 1., 0., 0., 0., 0., 0.]);
    let messages = run(value);
    let first = rows(&messages, "credit_states")
        .find(|r| r["day"] == 1)
        .unwrap();
    let t = 1.0_f64 / 365.;
    let performing = 1. - (-t).exp();
    close(
        first["live_principal"][0].as_f64().unwrap(),
        100. * performing,
    );
    close(
        first["live_accrued_interest"][0].as_f64().unwrap(),
        5. * (t - performing),
    );
    close(first["live_accrued_interest"][1].as_f64().unwrap(), 0.);
}

#[test]
fn dated_allowance_and_chargeoff_reviews_reach_actual_gl_once() {
    let mut value = example();
    let p = &mut value["runs"][0]["positions"][0];
    let loss = 110. * 0.4 * 0.2 / 0.5 * (1. - (-0.5_f64 / 365.).exp());
    let basis = -5. * 0.2 / 0.5 * (1. - (-0.5_f64 / 365.).exp());
    p["reviews"] = json!([{"recognition_ordinal":739618,
        "allowance_review":{"review":{"id":"allowance-committee","revision":"r1"},"recognition_ordinal":739618,"phase":"before_credit","target_amount":loss+basis},
        "chargeoffs":[{"review":{"id":"loss-committee","revision":"r1"},"claim_id":"loan1-credit:739618:0","recognition_ordinal":739618,"principal_amount":loss,"book_adjustment_amount":basis,"accrued_interest_reversal":0.}]}]);
    let messages = run(value);
    close(balance(&messages, "allowance"), 0.);
    close(balance(&messages, "credit_loss_expense"), loss + basis);
    let claim = rows(&messages, "credit_claims")
        .find(|r| {
            r["day"] == 30
                && r["scenario"] == "baseline"
                && r["claim_id"] == "loan1-credit:739618:0"
        })
        .unwrap();
    close(claim["principal_charged_off"].as_f64().unwrap(), loss);
    let reviews = rows(&messages, "credit_lineage")
        .filter(|r| r["scenario"] == "baseline" && r["event"] == "reviewed_credit_chargeoff")
        .count();
    assert_eq!(reviews, 1);
}
