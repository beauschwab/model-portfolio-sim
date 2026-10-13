use portfolio_ledger_native::{
    consumer_intent_owner as consumer, daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
fn fixture() -> (DatedLedgerRequest, consumer::Request) {
    let raw: Value =
        serde_json::from_str(include_str!("fixtures/consumer_intent_owner.json")).unwrap();
    let mut dated = raw["dated"].clone();
    dated["specification"]["positions"].as_array_mut().unwrap().push(json!({"id":"credit-loan","account":"a","kind":"loan","classification":"ac","balance":0.,"rate":0.,"maturity_day":0,"risk_weight":0.,"rsf_weight":0.,"asf_weight":0.,"hqla_weight":0.,"lcr_inflow_weight":0.,"lcr_outflow_weight":0.}));
    let mut owners = raw["consumer_owners"].clone();
    for owner in owners.as_array_mut().unwrap() {
        let native = owner["runs"][0]["paths"][0]["request"].clone();
        owner["accounting_category"] = json!("ac");
        owner["source_identity"] = native["economic_source"].clone();
        owner["original_yield_anchor"] = native["acquisition"]["original_yield_anchor"].clone();
    }
    (serde_json::from_value(dated).unwrap(),serde_json::from_value(json!({"schema":consumer::SCHEMA,"reporting_end_ordinal":raw["reporting_end_ordinal"],"owners":owners})).unwrap())
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
#[test]
fn actual_same_daily_gl_default_cash_and_full_claims() {
    let (raw, owner) = fixture();
    let dated = CompiledDatedLedger::compile(raw, 512, 8_000_000).unwrap();
    let consumer = consumer::Compiled::compile(owner, &dated).unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    daily::run_dated_with_consumers(dated, consumer, Box::new(Capture(bytes.clone()))).unwrap();
    let data = bytes.lock().unwrap();
    let messages: Vec<Value> = String::from_utf8_lossy(&data)
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(messages.last().unwrap()["complete"], true);
    let rows: Vec<_> = messages
        .iter()
        .filter(|m| m["table"] == "closing_statements")
        .flat_map(|m| m["rows"].as_array().unwrap())
        .collect();
    assert!(!rows.is_empty());
    for row in rows {
        assert!((row["cash"].as_f64().unwrap() - 998.05).abs() < 1e-9);
        assert!((row["equity"].as_f64().unwrap() - 998.05).abs() < 1e-9);
    }
    assert!(messages.iter().any(|m| m["table"] == "consumer_daily"));
    assert!(messages.iter().any(|m| m["table"] == "consumer_lineage"));
}
#[test]
fn incomplete_zero_weight_claims_and_duplicate_ownership_reject() {
    let (raw, mut owner) = fixture();
    owner.owners[0].allocated_face = 0.;
    for run in &mut owner.owners[0].runs {
        run.paths[0].request.chargeoffs.clear();
    }
    let dated = CompiledDatedLedger::compile(raw, 512, 8_000_000).unwrap();
    assert!(consumer::Compiled::compile(owner.clone(), &dated).is_err());
    owner.owners[0].position_id = "p".into();
    assert!(consumer::admit(&owner, &dated).is_err());
}
