pub mod candidates;
pub mod commercial;
pub mod consumer;
pub mod deposits;
pub mod lifecycle;
pub mod mixed;
pub mod mortgages;
pub(crate) mod owned;
pub mod treasury;

use serde_json::Value;
/// Full raw graph/unit/solve/accounting/mapping/daily-ledger stream, no callbacks.
pub fn run_workflow(request: Value, output: Box<dyn std::io::Write + Send>) -> Result<(), String> {
    owned::run_streamed(request, output)
}
