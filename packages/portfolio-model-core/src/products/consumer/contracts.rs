//! Shared identities and event contracts for consumer products. No fitted policy is inferred.
use crate::cashflow::{validate_ordinal, ActualDayCount, CashflowMeasure};
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};

pub const MAX_CONSUMER_EVENTS: usize = 4096;
pub const MAX_CONSUMER_DAYS: u32 = 36_600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerIdentity {
    pub position_id: String,
    pub original_loan_id: String,
    pub cohort_id: String,
    pub currency: String,
    pub source_revision: String,
    pub model_version: String,
    pub assumption_id: String,
    pub run_id: String,
    pub measure: CashflowMeasure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerState {
    Performing,
    Delinquent,
    Nonaccrual,
    Default,
    PaidOff,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerCashEvent {
    pub event_id: String,
    pub ordinal: i32,
    pub kind: String,
    /// Positive receipts and negative funded draws, from lender's perspective.
    pub principal: f64,
    pub interest: f64,
    pub fees: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerRecognition {
    pub origin_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    /// Supplied-policy economic interest earned over this exact interval, not additional cash.
    pub interest: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerDefault {
    pub ordinal: i32,
    pub principal: f64,
    pub accrued_interest: f64,
    pub recognized_accrued_interest: f64,
    pub unrecognized_contract_interest: f64,
    pub fees: f64,
    pub gross_recovery: f64,
    pub recovery_cost: f64,
    pub net_recovery: f64,
    pub principal_loss: f64,
    pub recovery_ordinal: i32,
}

pub(crate) fn amount(v: f64, name: &str) -> Result<(), String> {
    if !v.is_finite() || !(0.0..=MAX_MONETARY_AMOUNT).contains(&v) {
        return Err(format!("{name} must be finite in monetary bounds"));
    }
    Ok(())
}
pub(crate) fn fraction(v: f64, name: &str) -> Result<(), String> {
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(format!("{name} must be in [0,1]"));
    }
    Ok(())
}
pub(crate) fn rate(v: f64) -> Result<(), String> {
    if !v.is_finite() || !(0.0..=2.0).contains(&v) {
        return Err("consumer rate must be finite in [0,2]".into());
    }
    Ok(())
}
pub(crate) fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err(
            "consumer identity must be nonblank, bounded and free of control characters".into(),
        );
    }
    Ok(())
}
pub(crate) fn identity(v: &ConsumerIdentity, version: &str) -> Result<(), String> {
    for s in [
        &v.position_id,
        &v.original_loan_id,
        &v.cohort_id,
        &v.currency,
        &v.source_revision,
        &v.model_version,
        &v.assumption_id,
        &v.run_id,
    ] {
        label(s)?;
    }
    if v.model_version != version {
        return Err("consumer model version mismatch".into());
    }
    if v.measure == CashflowMeasure::Contractual {
        return Err("consumer path requires explicit physical or pricing measure".into());
    }
    Ok(())
}
pub(crate) fn horizon(as_of: i32, days: u32) -> Result<i32, String> {
    validate_ordinal(as_of)?;
    if days > MAX_CONSUMER_DAYS {
        return Err("consumer horizon exceeds 36600 days".into());
    }
    let end = as_of
        .checked_add(days as i32)
        .ok_or("consumer date overflow")?;
    validate_ordinal(end)?;
    Ok(end)
}
pub(crate) fn day_count(v: ActualDayCount) -> Result<(), String> {
    if !matches!(v, ActualDayCount::Act360 | ActualDayCount::Act365Fixed) {
        return Err("consumer simple interest supports explicit ACT360 or ACT365F only".into());
    }
    Ok(())
}
