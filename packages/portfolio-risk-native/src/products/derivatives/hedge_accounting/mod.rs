//! Explicit ASC 815 interest-rate hedge relationships over the reviewed derivative owner.
//! Rule matrix, admission and scope: docs/models/hedge-accounting.md.
mod inputs;
mod replay;
mod types;
pub use types::*;

/// Complete admission without derivative valuation or journal generation.
pub fn validate_request(request: &HedgeAccountingRequest) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    inputs::admit(request)?;
    portfolio_compute_control::checkpoint()
}
pub fn evaluate(request: &HedgeAccountingRequest) -> Result<HedgeAccountingResult, String> {
    replay::evaluate(request)
}
