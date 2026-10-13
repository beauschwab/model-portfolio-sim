//! Complete actual-date expected mixed dated backbook/candidate replay.
mod admission;
mod collect;
mod source;
use crate::product_candidate_journal::{Identity, Owner, Policy, RunMapping};
use portfolio_ledger_native::{acquisition, dated_ledger::DatedLedgerRequest};
use portfolio_risk_native::{
    product_candidate_source::Capture,
    product_strategy::{Allocation, Prepared as ProductPrepared},
    product_units::ProductUnitsRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
pub const SCHEMA: &str = "product-mixed-dated-1";
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_POSITIONS: usize = 512;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSelection {
    pub product: ProductUnitsRequest,
    pub allocation: Vec<Allocation>,
    pub owners: Vec<Owner>,
    pub policy: Policy,
    pub run_mapping: Vec<RunMapping>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixedPolicy {
    pub identity: Identity,
    pub expectation: String,
    pub phase: String,
    pub backbook_basis: String,
    pub tail: String,
    pub fees: String,
    pub defaults: String,
    pub marks: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub candidate: CandidateSelection,
    pub backbook: DatedLedgerRequest,
    pub combined_specification: Value,
    pub policy: MixedPolicy,
    pub opening_identity: Identity,
}
#[derive(Serialize)]
pub struct Prepared {
    pub raw_request: Value,
    pub native_preparation: ProductPrepared,
    pub candidate_capture: Capture,
    pub combined_dated_request: DatedLedgerRequest,
    pub acquisitions: acquisition::Request,
    pub backbook_source_ends: Vec<Value>,
    pub reporting_end_ordinal: i32,
    pub full_end_ordinal: i32,
    pub admitted_work: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub transaction_number: usize,
    pub scenario: String,
    pub day: usize,
    pub account: String,
    pub event: String,
    pub gl_account: String,
    pub instrument_id: String,
    pub value: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialBalance {
    pub scenario: String,
    pub account: String,
    pub gl_account: String,
    pub instrument_id: String,
    pub balance: f64,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub preparation: Prepared,
    pub protocol: u32,
    pub complete: bool,
    pub journal: Vec<Journal>,
    pub trial_balance: Vec<TrialBalance>,
    pub closing_statements: Vec<Value>,
    pub daily_balance: Vec<Value>,
    pub dated_rights: Vec<Value>,
    pub event_lineage: Vec<Value>,
    pub acquisition_lineage: Vec<Value>,
    pub exposures: Vec<Value>,
    pub ledger: Vec<Value>,
    pub reporting_rights: Vec<Value>,
    pub reported_daily_balance: Vec<Value>,
    pub independent_persisted_verification: bool,
    pub dynamic_mixed_acceptance: bool,
    pub scope: &'static str,
    pub reporting_end_ordinal: i32,
    pub full_end_ordinal: i32,
}
pub fn prepare(request: &Request) -> Result<Prepared, String> {
    source::prepare(request)
}
pub(crate) fn prepare_with_reservation(
    request: &Request,
    full_floor: i32,
    reserved_work: usize,
    reserved_bytes: usize,
) -> Result<Prepared, String> {
    source::prepare_with_reservation(request, Some(full_floor), reserved_work, reserved_bytes)
}
pub(crate) struct ConsumerTables {
    pub rights_daily: Vec<Value>,
    pub rights_lineage: Vec<Value>,
    pub rights_acquisitions: Vec<Value>,
    pub daily: Vec<Value>,
    pub lineage: Vec<Value>,
    pub acquisitions: Vec<Value>,
    pub sources: Vec<Value>,
}
pub(crate) fn evaluate_with_consumers(
    prepared: Prepared,
    request: portfolio_ledger_native::consumer_intent_owner::Request,
) -> Result<(ResultData, ConsumerTables), String> {
    collect::evaluate_with_consumers(prepared, request)
}
pub(crate) fn evaluate_with_products(
    prepared: Prepared,
    request: portfolio_ledger_native::product_event_owner::Request,
) -> Result<(ResultData, ConsumerTables), String> {
    collect::evaluate_with_products(prepared, request)
}
pub fn evaluate(request: &Request) -> Result<ResultData, String> {
    collect::evaluate(prepare(request)?)
}
pub fn run(request: &Request, mut output: impl Write) -> Result<(), String> {
    let result = evaluate(request)?;
    portfolio_compute_control::checkpoint()?;
    serde_json::to_writer(&mut output, &result).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()
}
