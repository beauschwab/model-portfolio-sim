//! Full-tail consumer product events and performing dated owners in one native GL.
mod admission;
use crate::workflows::mixed::performing;
use portfolio_ledger_native::{
    consumer_intent_owner as consumer, dated_ledger::CompiledDatedLedger,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
pub const SCHEMA: &str = "product-mixed-consumer-credit-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: portfolio_model_core::credit_accounting::Identity,
    pub ownership: String,
    pub phase: String,
    pub tail: String,
    pub cash: String,
    pub initial_allowance: String,
    pub generic_cashflows: String,
    pub generic_credit: String,
    pub generic_marks: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub performing: performing::Request,
    pub combined_specification: Value,
    pub consumer_owners: Vec<consumer::Owner>,
    pub policy: Policy,
}
#[derive(Serialize)]
pub struct Prepared {
    pub raw_request: Value,
    pub performing_preparation: performing::Prepared,
    pub consumer_request: consumer::Request,
    pub consumer_reservation: consumer::Admission,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub preparation: Prepared,
    pub protocol: u32,
    pub complete: bool,
    pub journal: Vec<performing::Journal>,
    pub trial_balance: Vec<performing::TrialBalance>,
    pub closing_statements: Vec<Value>,
    pub daily_balance: Vec<Value>,
    pub dated_rights: Vec<Value>,
    pub event_lineage: Vec<Value>,
    pub acquisition_lineage: Vec<Value>,
    pub exposures: Vec<Value>,
    pub ledger: Vec<Value>,
    pub reporting_rights: Vec<Value>,
    pub reported_daily_balance: Vec<Value>,
    pub consumer_sources: Vec<Value>,
    pub consumer_daily: Vec<Value>,
    pub consumer_lineage: Vec<Value>,
    pub consumer_acquisition_lineage: Vec<Value>,
    pub reporting_end_ordinal: i32,
    pub full_end_ordinal: i32,
    pub admitted_work: usize,
    pub mixed_ledger_adoption: bool,
    pub independent_persisted_verification: bool,
    pub dynamic_mixed_acceptance: bool,
    pub interim_default_capital_adoption: bool,
    pub scope: &'static str,
}
pub fn prepare(r: &Request) -> Result<Prepared, String> {
    let (request, reservation, floor) = admission::validate(r)?;
    let mut performing = performing::prepare_with_reservation(
        &r.performing,
        floor,
        reservation.work,
        reservation.retained_bytes,
    )?;
    performing.combined_dated_request.specification = r.combined_specification.clone();
    performing.combined_dated_request.specification["horizon_days"] = serde_json::json!(
        performing.full_end_ordinal - performing.combined_dated_request.dated.as_of_ordinal
    );
    // Existing raw source and individual original ends remain unchanged in receipt.
    let compiled = CompiledDatedLedger::compile_with_acquisitions(
        performing.combined_dated_request.clone(),
        &performing.acquisitions,
        512,
        8_000_000,
    )?;
    let actual = consumer::admit(&request, &compiled)?;
    if actual.work > reservation.work || actual.retained_bytes > reservation.retained_bytes {
        return Err("consumer source exceeded conservative reservation".into());
    }
    Ok(Prepared {
        raw_request: serde_json::to_value(r).map_err(|e| e.to_string())?,
        performing_preparation: performing,
        consumer_request: request,
        consumer_reservation: reservation,
    })
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let p = prepare(r)?;
    let raw_request = p.raw_request;
    let reservation = p.consumer_reservation;
    let consumer_request = p.consumer_request;
    let (old, consumer) =
        performing::evaluate_with_consumers(p.performing_preparation, consumer_request.clone())?;
    let result=ResultData{schema:SCHEMA,protocol:old.protocol,complete:old.complete,reporting_end_ordinal:old.reporting_end_ordinal,full_end_ordinal:old.full_end_ordinal,admitted_work:old.preparation.admitted_work,
        preparation:Prepared{raw_request,performing_preparation:old.preparation,consumer_request,consumer_reservation:reservation},journal:old.journal,trial_balance:old.trial_balance,closing_statements:old.closing_statements,daily_balance:old.daily_balance,dated_rights:old.dated_rights,event_lineage:old.event_lineage,acquisition_lineage:old.acquisition_lineage,exposures:old.exposures,ledger:old.ledger,reporting_rights:old.reporting_rights,reported_daily_balance:old.reported_daily_balance,consumer_sources:consumer.sources,consumer_daily:consumer.daily,consumer_lineage:consumer.lineage,consumer_acquisition_lineage:consumer.acquisitions,
        mixed_ledger_adoption:true,independent_persisted_verification:false,dynamic_mixed_acceptance:false,interim_default_capital_adoption:false,
        scope:"one native full-tail expected dated/backbook/candidate/consumer product-event GL; reviewed original anchors and non-PCD allowance/default/recovery policies; exclusive owners and exactly-once physical weights; no observed-settlement, accounting-standard, empirical, joint-stochastic, pricing/solver credit adoption or independent persisted verification claim; interim capital research weights remain supplied and do not infer default RWA"};
    admission::bounded(&result, 32 * 1024 * 1024)?;
    portfolio_compute_control::checkpoint()?;
    Ok(result)
}
pub fn run(r: &Request, mut output: impl Write) -> Result<(), String> {
    let result = evaluate(r)?;
    portfolio_compute_control::checkpoint()?;
    serde_json::to_writer(&mut output, &result).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()
}
