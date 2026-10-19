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
/// Benchmark-component clean value per unit of hedged notional for a fixed-rate item, as the
/// hedge owner measures it. Shared with the saved derivative risk graph.
pub(crate) fn benchmark_unit_clean(
    item: &HedgedItem,
    rate: f64,
    curve: &crate::products::derivatives::contract::swaps::DiscountCurve,
    t: i32,
    timing: crate::products::derivatives::contract::swaps::PaymentTiming,
    term_end: i32,
) -> Result<f64, String> {
    replay::unit_clean(item, rate, curve, t, timing, term_end)
}
pub fn evaluate(request: &HedgeAccountingRequest) -> Result<HedgeAccountingResult, String> {
    replay::evaluate(request)
}
