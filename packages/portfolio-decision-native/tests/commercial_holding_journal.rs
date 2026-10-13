use portfolio_decision_native::commercial_holding_journal::{evaluate, run_cli_started, Request};
use portfolio_compute_control::{scope,Control};
use serde_json::{json,Value};
use std::time::{Instant,Duration};

fn raw() -> Value { serde_json::from_slice(include_bytes!("../../../examples/models/commercial-holding-journal-1.json")).unwrap() }
fn run(raw:Value) -> Result<Value,String> {
    let r:Request=serde_json::from_value(raw).map_err(|e|e.to_string())?;
    scope(Some(Control::new(30000)?),||serde_json::to_value(evaluate(&r)?).map_err(|e|e.to_string()))
}
#[test]
fn journal_cut_retains_native_future_postings_and_separate_prior_pnl() {
    let v=run(raw()).unwrap();let cut=v["cut_day"].as_u64().unwrap();
    let full=v["full_tables"]["journal"].as_array().unwrap();let window=v["window_tables"]["journal"].as_array().unwrap();
    assert!(v["window_tables"]["retained_prior_pnl"].as_array().unwrap().iter().all(|r|r["signed_prior_pnl"].as_f64().unwrap()<0.));
    assert_eq!(window.iter().filter(|r|!r["origin_transaction_number"].is_null()).count(),full.iter().filter(|r|r["day"].as_u64().unwrap()>cut).count());
    assert!(window.iter().all(|r|r["day"].as_u64().unwrap()>=cut && r["event"]!="reviewed_opening_snapshot"));
    assert!(v["window_tables"]["commercial_daily"].as_array().unwrap().iter().any(|r|r["unweighted_book"]["in_default"]==true));
    assert_eq!(v["producer_completion"],json!({"complete":true,"protocol":6}));
    assert_eq!(v["prior_publication_verified"],false);
}
#[test]
fn original_stream_quote_prefix_and_allocation_authority_reject() {
    for target in ["stream","quote","past","allocation"] {
        let mut r=raw();
        match target {
            "stream" => r["revised_sources"][0]["credit"]["draw_stream"]["stream_revision"]="foreign".into(),
            "quote" => r["revised_sources"][0]["source"]["acquisition"]["cost"]=101.0.into(),
            "past" => r["revised_sources"][0]["credit"]["intervals"][0]["hazards_per_year"][0][3]=1.0.into(),
            _ => r["origin_ledger"]["participations"][0]["source_weight"]=0.5.into(),
        }
        assert!(run(r).is_err(),"{target}");
    }
}
#[test]
fn original_deadline_and_cancellation_prohibit_output() {
    let r:Request=serde_json::from_value(raw()).unwrap();let control=Control::new(30000).unwrap();control.cancel();
    assert!(scope(Some(control),||evaluate(&r)).unwrap_err().contains("CANCELLED"));
    let bytes=serde_json::to_vec(&json!({"op":"analyze_commercial_holding_journal","schema":"commercial-holding-journal-1","execution_timeout_ms":1,"request":raw()})).unwrap();
    let mut out=Vec::new();assert!(run_cli_started(&bytes,&mut out,Instant::now()-Duration::from_millis(5)).unwrap_err().contains("DEADLINE"));assert!(out.is_empty());
}
#[test]
fn zero_allocation_keeps_full_sources_without_second_acquisition() {
    let mut r=raw();r["origin_ledger"]["allocations"][0]["allocated_face"]=0.0.into();
    let v=run(r).unwrap();
    assert!(v["window_tables"]["commercial_daily"].as_array().unwrap().iter().all(|r|r["allocation_factor"]==0.0));
    assert!(v["window_tables"]["journal"].as_array().unwrap().iter().all(|r|r["instrument_id"]==""));
}
