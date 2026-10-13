//! Reviewed Physical derivative journal intents, never direct persistence.
mod inputs;
mod journal;
mod replay;
mod types;
pub use types::*;
pub const SCHEMA: &str = "derivative-accounting-1";
pub const MODEL_VERSION: &str = "derivative-accounting-1";
/// Existing complete accounting admission, without marks, cash projection or journal generation.
pub fn validate_request(request: &DerivativeAccountingRequest) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    inputs::admit(request)?;
    portfolio_compute_control::checkpoint()
}
pub fn evaluate(
    request: &DerivativeAccountingRequest,
) -> Result<DerivativeAccountingResult, String> {
    replay::evaluate(request)
}
