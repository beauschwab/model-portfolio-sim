//! Standalone acquisition-aware candidate expected journals. This component does
//! not compose a mixed book or substitute for persisted independent verification.
mod admission;
mod replay;
use portfolio_risk_native::{
    product_candidate_source::{self, Capture},
    product_strategy::{self, Allocation, Prepared},
    product_units::ProductUnitsRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
pub const SCHEMA: &str = "product-candidate-journal-1";
pub const MAX_ROWS: usize = 250_000;
pub const MAX_LINES: usize = 2_000_000;
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
/// Source-only selection admission for an acquisition-aware mixed coordinator.
/// Account/position opening ownership must be validated by that coordinator.
pub(crate) fn audit_source_capacity(request:&Request,prepared:&Prepared)->Result<usize,String>{
    admission::raw(request)?;
    let (work,rows,full)=product_candidate_source::admitted_work(prepared,request.as_of_ordinal,request.reporting_end_ordinal)?;
    admission::aggregate(request,prepared,work,rows,full)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub book_basis: String,
    pub acquisition_phase: String,
    pub fees: String,
    pub defaults: String,
    pub marks: String,
    pub tail: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub unit_id: String,
    pub position_id: String,
    pub account_id: String,
    pub source: Identity,
    pub ownership: String,
    pub accounting_category: String,
    pub ledger_classification: String,
    pub pre_purchase_principal: f64,
    pub pre_purchase_accrued: f64,
    pub pre_purchase_basis: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunMapping {
    pub source_scenario_id: String,
    pub ledger_scenario_id: String,
    pub severity: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub product: ProductUnitsRequest,
    pub allocation: Vec<Allocation>,
    pub as_of_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub owners: Vec<Owner>,
    pub policy: Policy,
    pub run_mapping: Vec<RunMapping>,
    pub specification: Value,
    /// Supplied receipt identity only. This component never claims to verify a
    /// previous persisted journal because an identity was supplied.
    pub verified_opening_identity: Identity,
}
#[derive(Serialize)]
pub struct Posting {
    pub gl_account: String,
    pub value: f64,
}
#[derive(Serialize)]
pub struct Journal {
    pub transaction_id: usize,
    pub source_scenario_id: String,
    pub ledger_scenario_id: String,
    pub severity: f64,
    pub ordinal: i32,
    pub phase: String,
    pub unit_id: String,
    pub position_id: String,
    pub account_id: String,
    pub source: Identity,
    pub path_id: String,
    pub source_weight: f64,
    pub allocation_factor: f64,
    pub native_source_index: usize,
    pub native_event_ids: Vec<String>,
    pub policy: Identity,
    pub postings: Vec<Posting>,
}
#[derive(Clone, Serialize)]
pub struct PositionState {
    pub source_scenario_id: String,
    pub ordinal: i32,
    pub unit_id: String,
    pub position_id: String,
    pub account_id: String,
    pub path_id: String,
    pub source_weight: f64,
    pub allocated_principal: f64,
    pub accrued_interest: f64,
    pub book_adjustment: f64,
    pub gross_carrying_value: f64,
    pub effective_interest_to_date: f64,
    pub acquired: bool,
    pub full_end_ordinal: i32,
}
#[derive(Clone, Serialize)]
pub struct AccountState {
    pub source_scenario_id: String,
    pub ordinal: i32,
    pub account_id: String,
    pub cash: f64,
    pub cumulative_net_effective_income: f64,
    pub equity: f64,
}
#[derive(Serialize)]
pub struct TrialBalance {
    pub source_scenario_id: String,
    pub account_id: String,
    pub position_id: String,
    pub path_id: String,
    pub gl_account: String,
    pub balance: f64,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub normalized_specification: Value,
    pub native_preparation: Prepared,
    pub source_capture: Capture,
    pub journals: Vec<Journal>,
    pub daily_positions: Vec<PositionState>,
    pub daily_accounts: Vec<AccountState>,
    pub reported_positions: Vec<PositionState>,
    pub complete_closing_positions: Vec<PositionState>,
    pub reported_accounts: Vec<AccountState>,
    pub complete_closing_accounts: Vec<AccountState>,
    pub trial_balance: Vec<TrialBalance>,
    pub full_end_ordinal: i32,
    pub admitted_work: usize,
    pub independent_persisted_verification: bool,
    pub dynamic_mixed_acceptance: bool,
    pub scope: &'static str,
}
pub fn evaluate(request: &Request) -> Result<ResultData, String> {
    portfolio_compute_control::checkpoint()?;
    admission::raw(request)?;
    let specification =
        portfolio_ledger_native::spec::normalize(request.specification.clone(), 256, 8_000_000)?;
    admission::spec(request, &specification)?;
    let prepared = product_strategy::prepare(&request.product)?;
    let (work, rows, full) = product_candidate_source::admitted_work(
        &prepared,
        request.as_of_ordinal,
        request.reporting_end_ordinal,
    )?;
    let aggregate_work = admission::aggregate(request, &prepared, work, rows, full)?;
    let capture = product_candidate_source::capture(
        &prepared,
        &request.allocation,
        request.as_of_ordinal,
        request.reporting_end_ordinal,
    )?;
    let output = replay::run(
        request,
        &specification,
        prepared,
        capture,
        aggregate_work,
        full,
    )?;
    admission::retained(&output)?;
    portfolio_compute_control::checkpoint()?;
    Ok(output)
}
/// Complete in-memory validation precedes the first output byte. This standalone
/// transport emits no immutable publication or saved mixed-book manifest.
pub fn run(request: &Request, mut output: impl Write) -> Result<(), String> {
    let result = evaluate(request)?;
    portfolio_compute_control::checkpoint()?;
    serde_json::to_writer(&mut output, &result).map_err(|e| e.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
