use portfolio_ledger_native::{
    daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    product_event_owner as owner,
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
fn source(case: usize) -> Value {
    let all: Value = serde_json::from_str(include_str!(
        "../../portfolio-model-core/tests/fixtures/card_credit_v2_proof.json"
    ))
    .unwrap();
    all["cases"][case]["raw"].clone()
}
fn fixture(case: usize, allocation: f64, weight: f64) -> Value {
    let mut v: Value =
        serde_json::from_str(include_str!("fixtures/product_event_owner.json")).unwrap();
    v["product_owners"].as_array_mut().unwrap().remove(0);
    v["dated"]["specification"]["positions"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    let reporting = v["reporting_end_ordinal"].clone();
    let o = &mut v["product_owners"][0];
    o["allocated_face"] = json!(allocation);
    o["initial_allowance_review"] = Value::Null;
    for run in o["runs"].as_array_mut().unwrap() {
        let mut raw = source(case);
        raw["contract"]["identity"]["position_id"] = json!("credit-card");
        raw["contract"]["identity"]["run_id"] = run["ledger_run_id"].clone();
        raw["review"]["position_id"] = json!("credit-card");
        raw["reporting_ordinal"] = reporting.clone();
        run["paths"][0]["request"] = json!({"kind":"card_v2","request":raw});
        run["paths"][0]["source_weight"] = json!(weight);
        if weight != 1. {
            let mut second = run["paths"][0].clone();
            second["path_id"] = json!("paired-second-path");
            second["source_weight"] = json!(1. - weight);
            run["paths"].as_array_mut().unwrap().push(second);
        }
    }
    v
}
#[derive(Clone)]
struct Sink(Arc<Mutex<Vec<u8>>>);
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn execute(v: Value) -> Result<Vec<Value>, String> {
    let r: DatedLedgerRequest =
        serde_json::from_value(v["dated"].clone()).map_err(|e| e.to_string())?;
    let c = CompiledDatedLedger::compile(r, 512, 8_000_000)?;
    let raw = serde_json::from_value(json!({"schema":owner::SCHEMA,"reporting_end_ordinal":v["reporting_end_ordinal"],"owners":v["product_owners"]})).map_err(|e|e.to_string())?;
    let owned = owner::Compiled::compile(raw, &c)?;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    daily::run_dated_with_products(c, owned, Box::new(Sink(bytes.clone())))?;
    let locked = bytes.lock().unwrap();
    locked
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).map_err(|e| e.to_string()))
        .collect()
}
fn rows(result: &[Value], table: &str) -> Vec<Value> {
    result
        .iter()
        .filter(|r| r["table"] == table)
        .flat_map(|r| r["rows"].as_array().unwrap().clone())
        .collect()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a}!={b}");
}
#[test]
fn original_yield_and_gross_interest_basis_share_actual_gl() {
    let r = execute(fixture(0, 100., 1.)).unwrap();
    let final_rows = rows(&r, "closing_statements");
    for row in final_rows {
        close(row["cash"].as_f64().unwrap(), 1051.05);
        close(row["equity"].as_f64().unwrap(), 1051.05);
    }
    let day = rows(&r, "product_rights_daily")
        .into_iter()
        .find(|r| r["ordinal"] == 738926)
        .unwrap();
    let s = &day["unweighted_rights"];
    close(s["core"]["live"][0]["principal"].as_f64().unwrap(), 0.);
    close(
        s["core"]["live"][0]["accrued_interest"].as_f64().unwrap(),
        1.,
    );
    close(
        s["live_interest_basis"].as_f64().unwrap(),
        100. * (1.02f64).powf(10. / 365.) - 101.,
    );
    assert!(!rows(&r, "product_rights_lineage").is_empty());
    assert!(rows(&r, "product_lineage").is_empty());
    assert_eq!(rows(&r, "product_rights_acquisition_lineage").len(), 2);
}
#[test]
fn delayed_default_interest_basis_and_shared_intents_are_not_reposted() {
    for case in [7, 8, 10, 11] {
        let r = execute(fixture(case, 100., 1.)).unwrap();
        let groups = rows(&r, "product_rights_lineage");
        assert!(groups.iter().any(|g| !g["shared_intents_evidence_only"]
            .as_array()
            .unwrap()
            .is_empty()));
        let expected: portfolio_model_core::card_credit_accounting_v2::Request =
            serde_json::from_value(source(case)).unwrap();
        let expected =
            portfolio_model_core::card_credit_accounting_v2::evaluate(&expected).unwrap();
        let cash =
            1000. - expected.raw_request.review.cost + expected.closing.cash_received + 50.05;
        for row in rows(&r, "closing_statements") {
            close(row["cash"].as_f64().unwrap(), cash);
            close(row["equity"].as_f64().unwrap(), cash);
        }
    }
}
#[test]
fn signed_yield_partial_interest_and_probability_once() {
    for case in [0, 1, 2, 9] {
        let r = execute(fixture(case, 50., 0.25)).unwrap();
        let expected: portfolio_model_core::card_credit_accounting_v2::Request =
            serde_json::from_value(source(case)).unwrap();
        let expected =
            portfolio_model_core::card_credit_accounting_v2::evaluate(&expected).unwrap();
        let cash =
            1050.05 + 0.5 * (expected.closing.cash_received - expected.raw_request.review.cost);
        for row in rows(&r, "closing_statements") {
            close(row["cash"].as_f64().unwrap(), cash);
        }
        assert_eq!(rows(&r, "product_rights_acquisition_lineage").len(), 4);
    }
}
#[test]
fn zero_allocation_retains_unweighted_complete_rights() {
    let r = execute(fixture(8, 0., 0.)).unwrap();
    for row in rows(&r, "closing_statements") {
        close(row["cash"].as_f64().unwrap(), 1050.05);
    }
    assert_eq!(rows(&r, "product_rights_acquisition_lineage").len(), 4);
    let mut v = fixture(8, 0., 0.);
    v["product_owners"][0]["runs"][0]["paths"][0]["request"]["request"]["interest_recoveries"] =
        json!([]);
    assert!(execute(v).is_err());
}
#[test]
fn complete_atomic_group_failure_does_not_commit_partial_money() {
    use portfolio_ledger_native::consumer_intent_owner::{PathBalances, Scale};
    let raw = serde_json::from_value(source(0)).unwrap();
    let result = portfolio_model_core::card_credit_accounting_v2::evaluate(&raw).unwrap();
    let scale = Scale {
        allocation_face: 100.,
        unit_face: 100.,
        source_weight: 1.,
    };
    let mut state = PathBalances::default()
        .acquire(100., 100., 0., None, &scale)
        .unwrap()
        .purchase
        .next;
    state.start_rights(&result.acquisition_opening).unwrap();
    let action = &result.actions[0];
    let mut events = result.events[action.events.start..action.events.end].to_vec();
    events[0].lines[0].amount += 1.;
    assert!(state.stage_rights(action, &events, &scale).is_err());
    state.reconcile_rights(&result.acquisition_opening).unwrap();
    let staged = state
        .stage_rights(
            action,
            &result.events[action.events.start..action.events.end],
            &scale,
        )
        .unwrap();
    staged.next.reconcile_rights(&action.committed).unwrap();
    assert!(staged
        .next
        .stage_rights(
            action,
            &result.events[action.events.start..action.events.end],
            &scale
        )
        .is_err());
}
