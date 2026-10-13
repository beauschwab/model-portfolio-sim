//! Native source owner -> explicit reviewed daily FTP -> Risk Treasury. Research proposal only.
mod acquisition_phase;
use crate::modeled_product_ftp_source as source;
use crate::mortgage_closing_capital;
use funding_contract::{Binding, ChargePolicy, Review};
pub use portfolio_model_core::modeled_product_ftp_policy as funding_contract;
use portfolio_model_core::modeled_product_funding as ftp;
pub use portfolio_risk_native::modeled_ftp_policy as charge_math;
use portfolio_risk_native::treasury;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "modeled-product-ftp-preparation-1";
const MAX_CHARGE_WORK: usize = 2_000_000;
const MAX_JOINED_WORK: usize = 62_000_000;
const MAX_RETAINED_CAPACITY: usize = 384 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub source: source::Request,
    pub as_of: String,
    pub policy_review: Review,
    pub curves: Vec<treasury::Curve>,
    pub charges: Vec<ChargePolicy>,
    pub capital: Option<mortgage_closing_capital::Request>,
    pub capital_source_id: Option<String>,
    pub dirty_unit_state: String,
    pub acquisition_phase_policy: String,
}
fn label(x: &str) -> Result<(), String> {
    if x.is_empty() || x.len() > 200 || x.chars().any(char::is_control) {
        Err("bounded explicit identity required".into())
    } else {
        Ok(())
    }
}
fn review(r: &Review) -> Result<(), String> {
    label(&r.id)?;
    label(&r.revision)
}
fn nonnegative(x: f64) -> Result<(), String> {
    if !x.is_finite() || !(0.0..=10000.0).contains(&x) {
        Err("explicit finite nonnegative cost/rate required".into())
    } else {
        Ok(())
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("work overflow".into())
}
fn binding(p: &source::PreparedPath) -> Binding {
    Binding {
        source_id: p.source_id.clone(),
        owner_index: p.owner_index,
        run_index: p.run_index,
        path_index: p.path_index,
    }
}
fn review_binding(r: &source::Review) -> Binding {
    Binding {
        source_id: r.source_id.clone(),
        owner_index: r.owner_index,
        run_index: r.run_index,
        path_index: r.path_index,
    }
}
#[derive(Serialize)]
pub struct DerivedCharge {
    pub binding: Binding,
    pub ordinal: i32,
    pub benchmark: Binding,
    pub contingent_benchmark: Binding,
    pub actual_cash_difference: f64,
    pub incremental_funding: f64,
    pub incremental_undrawn: f64,
    pub option_amount: f64,
    pub contingent_amount: f64,
    pub retained_earnings: Vec<ftp::Amount>,
    pub native_opening_undrawn: f64,
    pub native_closing_undrawn: f64,
    pub benchmark_opening_undrawn: f64,
    pub benchmark_closing_undrawn: f64,
}

mod charge_admission;
mod charge_execution;
mod commitment;
pub use charge_admission::admit;
pub use charge_execution::evaluate;

pub mod public_cli;
