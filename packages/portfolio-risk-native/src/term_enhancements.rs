//! Explicit contractual paths, conditional exercise and CD elections.
//! No legacy term kernel or saved contract is reinterpreted by these APIs.
pub mod cd;
pub mod cd_sequence;
pub mod exercise;
pub mod indexed;

use portfolio_model_core::cashflow::{validate_ordinal, MAX_HORIZON_DAYS};
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "term-enhancements-1";
pub const MODEL_VERSION: &str = "term-contracts-1";
pub const MAX_PERIODS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
pub(crate) fn text(value: &str, bytes: &mut usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err("term identities require 1..128 control-free bytes".into());
    }
    *bytes = bytes.checked_add(value.len()).ok_or("metadata overflow")?;
    if *bytes > MAX_DATED_METADATA_BYTES {
        return Err("term metadata exceeds 32 MiB admission".into());
    }
    Ok(())
}
pub(crate) fn identity(id: &Identity, bytes: &mut usize) -> Result<(), String> {
    text(&id.id, bytes)?;
    text(&id.revision, bytes)
}
pub(crate) fn amount(value: f64) -> Result<f64, String> {
    if !value.is_finite() || value.abs() > MAX_MONETARY_AMOUNT {
        return Err("term amount exceeds finite actual-currency domain".into());
    }
    Ok(value)
}
pub(crate) fn nonnegative(value: f64) -> Result<f64, String> {
    amount(value)?;
    if value < 0. {
        return Err("negative principal or policy amount".into());
    }
    Ok(value)
}
pub(crate) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs()))
}
pub(crate) fn domain(asof: i32, end: i32) -> Result<(), String> {
    validate_ordinal(asof)?;
    validate_ordinal(end)?;
    if end < asof || (end - asof) as u32 > MAX_HORIZON_DAYS {
        return Err("term dates exceed explicit 36600-day domain".into());
    }
    Ok(())
}
pub(crate) fn header(
    schema: &str,
    version: &str,
    currency: &str,
    unit: &str,
) -> Result<(), String> {
    if schema != SCHEMA
        || version != MODEL_VERSION
        || unit != "currency_units"
        || currency.len() != 3
        || !currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err(
            "term schema/model/currency/unit mismatch; explicit rebuilt identity required".into(),
        );
    }
    Ok(())
}
