use portfolio_ledger_native::credit_path_replay::{
    self as replay, CompiledPathLedger, PathLedgerRequest,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn raw() -> Value {
    let mut source: Value = serde_json::from_str(include_str!(
        "../../../examples/models/credit-path-accounting-1.json"
    ))
    .unwrap();
    source["reporting_end_ordinal"] = json!(739920);
    source["path"]["intervals"][0]["end_year"] = json!(30.0 / 365.0);
    let mut v = json!({"schema":"credit-path-ledger-1","monetary_unit":"currency_units","as_of_ordinal":739890,
        "specification":{"version":"balance-stress-2","horizon_days":30,"accounts":[{"id":"bank","entity":"bank","currency":"USD","cash":10.0,"equity":66.0}],"positions":[{"id":"loan","account":"bank","kind":"loan","balance":100.0,"commitment":20.0,"book_adjustment":-3.0,"allowance":41.0}],"scenarios":[{"name":"flat"}],"cashflows":[],"reverse_severities":[]},
        "runs":[{"run_id":"baseline-physical","scenario_id":"baseline","severity":1.0,"batches":[{"batch_id":"supplied-default","account_by_position":[{"position_id":"loan","account_id":"bank"}],"request":source}]}]});
    let mut stress = v["runs"][0].clone();
    stress["run_id"] = json!("flat-physical");
    stress["scenario_id"] = json!("flat");
    v["runs"].as_array_mut().unwrap().push(stress);
    v
}
fn compile(value: Value) -> Result<CompiledPathLedger, String> {
    CompiledPathLedger::compile(serde_json::from_value::<PathLedgerRequest>(value).unwrap())
}
fn rows(bytes: &[u8]) -> (BTreeMap<String, Vec<Value>>, bool) {
    let mut rows: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut complete = false;
    for line in std::str::from_utf8(bytes).unwrap().lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        if v["complete"] == true {
            assert_eq!(v["protocol"], 5);
            complete = true;
        } else {
            rows.entry(v["table"].as_str().unwrap().into())
                .or_default()
                .extend(
                    v["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| r["scenario"] == "baseline")
                        .cloned(),
                );
        }
    }
    (rows, complete)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn supplied_default_daily_gl_is_independently_replayed_and_full_lineage_is_retained() {
    let mut bytes = Vec::new();
    replay::run(compile(raw()).unwrap(), &mut bytes).unwrap();
    let (tables, complete) = rows(&bytes);
    assert!(complete);
    let mut transactions: BTreeMap<u64, f64> = BTreeMap::new();
    let mut accounts: BTreeMap<(String, String), f64> = BTreeMap::new();
    for j in &tables["journal"] {
        *transactions
            .entry(j["transaction_number"].as_u64().unwrap())
            .or_default() += j["value"].as_f64().unwrap();
        *accounts
            .entry((
                j["gl_account"].as_str().unwrap().into(),
                j["instrument_id"].as_str().unwrap().into(),
            ))
            .or_default() += j["value"].as_f64().unwrap();
    }
    for value in transactions.values() {
        close(*value, 0.0);
    }
    let keys = transactions.keys().copied().collect::<Vec<_>>();
    assert_eq!(keys, (1..=keys.len() as u64).collect::<Vec<_>>());
    close(accounts[&("cash".into(), "@cash".into())], 66.05);
    close(
        accounts[&("credit_default_principal".into(), "claim".into())],
        0.0,
    );
    close(
        tables["summary"][0]["final_equity"].as_f64().unwrap(),
        66.05,
    );
    assert_eq!(tables["daily_balance"].len(), 30);
    assert_eq!(tables["credit_path_daily"].len(), 30);
    let recovery = &tables["credit_path_recoveries"][0]["recovery"];
    close(recovery["principal"].as_f64().unwrap(), 66.0);
    close(recovery["interest"].as_f64().unwrap(), 0.05);
    assert!(tables["credit_path_lineage"]
        .iter()
        .any(|v| v["accrual"].is_object()));
    assert_eq!(
        tables["credit_path_claims"][0]["claim"]["settled_interest_recovery_ids"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn missing_reverse_account_opening_owner_measure_and_maturity_coverage_reject_before_rows() {
    let mut v = raw();
    v["specification"]["scenarios"] = json!([{"name":"stress"}]);
    assert!(compile(v).unwrap_err().contains("coverage"));
    let mut v = raw();
    v["runs"][0]["batches"][0]["account_by_position"][0]["account_id"] = json!("other");
    assert!(compile(v).is_err());
    let mut v = raw();
    v["runs"][0]["batches"][0]["request"]["path"]["paths"][0]["principal"] = json!(90);
    assert!(compile(v).is_err());
    let mut v = raw();
    v["runs"][0]["batches"][0]["request"]["path"]["measure"] = json!("pricing");
    assert!(compile(v).is_err());
    let mut v = raw();
    v["specification"]["positions"][0]["maturity_day"] = json!(2);
    assert!(compile(v).unwrap_err().contains("maturity"));
    let mut v = raw();
    v["runs"][0]["batches"][0]["request"]["path"]["boundaries"] = json!([{"event_id":"terms","policy_id":"terms","year":2.0/365.0,"action":{"kind":"maturity"}}]);
    v["specification"]["positions"][0]["maturity_day"] = json!(3);
    assert!(compile(v).is_err());
}
#[test]
fn full_recovery_tail_and_signed_rights_survive_report_end() {
    let mut v = raw();
    let q = &mut v["runs"][0]["batches"][0]["request"];
    q["bindings"][0]["chargeoffs"] = json!([]);
    q["path"]["paths"][0]["accrued_interest"] = json!(-2.0);
    q["path"]["intervals"][0]["accrual_rates"][0] = json!(-0.365);
    for i in 0..3 {
        q["path"]["intervals"][0]["complete_recovery_tails"][i][0]["lag_years"] = json!(20.0);
    }
    v["specification"]["positions"][0]["opening_accrued"] = json!(-2.0);
    v["specification"]["accounts"][0]["equity"] = json!(64.0);
    v["runs"][1]["batches"] = v["runs"][0]["batches"].clone();
    let mut bytes = Vec::new();
    replay::run(compile(v).unwrap(), &mut bytes).unwrap();
    let (tables, complete) = rows(&bytes);
    assert!(complete);
    assert_eq!(
        tables["credit_path_recoveries"][0]["recovery"]["after_reporting"],
        true
    );
    close(tables["summary"][0]["final_cash"].as_f64().unwrap(), 0.0);
    close(
        tables["summary"][0]["final_equity"].as_f64().unwrap(),
        63.95,
    );
    assert_eq!(
        tables["credit_path_claims"][0]["claim"]["remaining_recoveries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn later_financial_failure_keeps_staged_stream_without_completion() {
    let mut v = raw();
    v["runs"][0]["batches"][0]["request"]["bindings"][0]["chargeoffs"][0]["principal_amount"] =
        json!(500);
    let mut bytes = Vec::new();
    assert!(replay::run(compile(v).unwrap(), &mut bytes).is_err());
    assert!(!rows(&bytes).1);
}
#[test]
fn pure_validation_transport_and_limits_preserve_normalized_spec() {
    let v = raw();
    let result = replay::validate_json(&v.to_string(), 2000, 3_000_000).unwrap();
    let normalized: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(normalized["positions"][0]["balance"], 100.0);
    assert!(replay::validate_json(&v.to_string(), 0, 3_000_000).is_err());
    assert!(replay::validate_json(&v.to_string(), 2000, 1).is_err());
}

#[test]
fn signed_opening_is_route_specific_and_legacy_admission_is_unchanged() {
    let mut v = raw();
    v["specification"]["positions"][0]["opening_accrued"] = json!(-2.0);
    v["specification"]["accounts"][0]["equity"] = json!(64.0);
    for run in v["runs"].as_array_mut().unwrap() {
        run["batches"][0]["request"]["path"]["paths"][0]["accrued_interest"] = json!(-2.0);
    }
    assert!(
        portfolio_ledger_native::spec::normalize(v["specification"].clone(), 2000, 3_000_000)
            .is_err()
    );
    assert!(portfolio_ledger_native::spec::normalize_signed_credit_path(
        v["specification"].clone(),
        2000,
        3_000_000
    )
    .is_ok());
    let mut output = Vec::new();
    replay::run(compile(v).unwrap(), &mut output).unwrap();
    let (tables, complete) = rows(&output);
    assert!(complete);
    let first = &tables["daily_balance"][0];
    close(first["cash"].as_f64().unwrap(), 64.05);
    close(first["liabilities"].as_f64().unwrap(), 0.0);
    close(first["net_assets"].as_f64().unwrap(), 64.05);
}

#[test]
fn multiple_batches_replay_out_of_global_date_order_and_never_net_opposite_rights() {
    let mut v = raw();
    let mut second = v["specification"]["positions"][0].clone();
    second["id"] = json!("other-loan");
    second["balance"] = json!(50.0);
    second["commitment"] = json!(0.0);
    second["book_adjustment"] = json!(0.0);
    second["allowance"] = json!(0.0);
    second["opening_accrued"] = json!(-3.0);
    second["maturity_day"] = json!(2);
    v["specification"]["positions"]
        .as_array_mut()
        .unwrap()
        .push(second);
    v["specification"]["accounts"][0]["equity"] = json!(113.0);
    for run in v["runs"].as_array_mut().unwrap() {
        run["batches"][0]["request"]["bindings"][0]["chargeoffs"][0]["recognition_ordinal"] =
            json!(739893);
        let mut b = run["batches"][0].clone();
        b["batch_id"] = json!("second");
        b["account_by_position"][0]["position_id"] = json!("other-loan");
        let q = &mut b["request"];
        q["bindings"][0]["position_id"] = json!("other-loan");
        q["bindings"][0]["claim_id"] = json!("other-claim");
        q["bindings"][0]["opening_allowance"] = json!(0);
        q["bindings"][0]["chargeoffs"] = json!([]);
        let p = &mut q["path"]["paths"][0];
        p["path_id"] = json!("other-loan");
        p["principal"] = json!(50);
        p["undrawn_commitment"] = json!(0);
        p["accrued_interest"] = json!(-3);
        p["book_adjustment"] = json!(0);
        q["path"]["intervals"][0]["hazards_per_year"] = json!(([[0; 5]; 3]));
        q["path"]["intervals"][0]["accrual_rates"] = json!(([0; 3]));
        q["path"]["boundaries"] = json!([{"event_id":"maturity","policy_id":"original-terms","year":2.0/365.0,"action":{"kind":"maturity"}}]);
        run["batches"].as_array_mut().unwrap().push(b);
    }
    let mut output = Vec::new();
    replay::run(compile(v).unwrap(), &mut output).unwrap();
    let (tables, complete) = rows(&output);
    assert!(complete);
    let first = &tables["daily_balance"][0];
    close(first["assets"].as_f64().unwrap(), 116.05);
    close(first["liabilities"].as_f64().unwrap(), 3.0);
    close(first["equity"].as_f64().unwrap(), 113.05);
    close(tables["summary"][0]["final_cash"].as_f64().unwrap(), 113.05);
    let journal = &tables["journal"];
    assert!(journal
        .windows(2)
        .any(|w| w[0]["day"].as_u64().unwrap() > w[1]["day"].as_u64().unwrap()));
}

#[test]
fn duplicate_owners_binding_keys_and_unknown_fields_reject() {
    let mut v = raw();
    let duplicated = v["runs"][0]["batches"][0].clone();
    v["runs"][0]["batches"]
        .as_array_mut()
        .unwrap()
        .push(duplicated);
    assert!(compile(v).is_err());
    let mut v = raw();
    v["model_auto_policy"] = json!(true);
    assert!(serde_json::from_value::<PathLedgerRequest>(v).is_err());
    let mut v = raw();
    v["specification"]["reverse_severities"] = json!([0.5]);
    assert!(compile(v).is_err());
}

#[test]
fn cancelled_execution_and_broken_output_never_complete() {
    let compiled = compile(raw()).unwrap();
    let control = portfolio_compute_control::Control::new(60_000).unwrap();
    control.cancel();
    let mut output = Vec::new();
    assert!(
        portfolio_compute_control::scope(Some(control), || replay::run(compiled, &mut output))
            .is_err()
    );
    assert!(output.is_empty());
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic broken sink"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(replay::run(compile(raw()).unwrap(), Broken).is_err());
}

#[test]
fn canonical_example_is_complete_strict_and_has_independent_final_account_amounts() {
    let raw = include_str!("../../../examples/models/credit-path-ledger-1.json");
    let request: PathLedgerRequest = serde_json::from_str(raw).unwrap();
    let mut output = Vec::new();
    replay::run(CompiledPathLedger::compile(request).unwrap(), &mut output).unwrap();
    let (tables, complete) = rows(&output);
    assert!(complete);
    close(tables["summary"][0]["final_cash"].as_f64().unwrap(), 66.05);
    close(
        tables["summary"][0]["final_net_assets"].as_f64().unwrap(),
        66.05,
    );
}
