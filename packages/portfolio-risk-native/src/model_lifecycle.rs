//! Explicit native analysis of new shared model contracts. These entrypoints do
//! not replace saved product models or imply integration with the daily ledger.
use portfolio_model_core::{cashflow::DatedSchedule, credit::CreditRequest};
use serde::{Deserialize, Serialize};

pub fn credit(request: &CreditRequest) -> Result<serde_json::Value, String> {
    let result = portfolio_model_core::credit::evaluate(request)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub fn fit_observed_credit(
    request: &portfolio_model_core::credit_calibration::CreditCalibrationRequest,
) -> Result<serde_json::Value, String> {
    let result = portfolio_model_core::credit_calibration::fit(request)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub fn dated_term(
    request: &crate::dated_term::DatedTermRequest,
) -> Result<serde_json::Value, String> {
    let result = request.build()?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CashflowRequest {
    pub schedule: DatedSchedule,
    pub scenario: String,
}

pub fn floating_rate(
    request: &portfolio_model_core::floating_rate::FloatingRateRequest,
) -> Result<serde_json::Value, String> {
    let result = portfolio_model_core::floating_rate::evaluate(request)?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

pub fn cashflows(request: &CashflowRequest) -> Result<serde_json::Value, String> {
    let events = request.schedule.for_scenario(&request.scenario)?;
    serde_json::to_value(events).map_err(|error| error.to_string())
}
