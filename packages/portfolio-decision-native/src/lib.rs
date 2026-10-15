//! Versioned actor sessions: native solver pointers stay on their owner thread.
pub mod coefficients;
pub mod coordinator;
pub mod optimization;
pub mod transport;
pub mod workflows;

// Retained public Rust paths and C entrypoints.
pub use coefficients::commercial as commercial_library;
pub use coefficients::products as product_library;
pub use coordinator::actors::dispatch;
pub use transport::commercial as commercial_intake;
pub use transport::ffi::decision_free;
pub use transport::ffi::decision_request;
pub use workflows::candidates as product_candidate_journal;
pub use workflows::commercial::candidates as commercial_candidate_journal;
pub use workflows::commercial::credit as commercial_ledger;
pub use workflows::commercial::fees as commercial_fee_journal;
pub use workflows::commercial::holdings as commercial_holding_journal;
pub use workflows::commercial::treasury as commercial_treasury;
pub use workflows::consumer::dated as advanced_ledger;
pub use workflows::deposits as deposit_ledger;
pub use workflows::mixed::consumer_credit as product_mixed_consumer_credit;
pub use workflows::mixed::deposit_derivatives as joint_deposit_derivative;
pub use workflows::mixed::derivatives as saved_derivative_mixed;
pub use workflows::mixed::performing as product_mixed_dated;
pub use workflows::mixed::product_credit as product_mixed_product_credit;
pub use workflows::mortgages::closing_capital as mortgage_closing_capital;
pub use workflows::run_workflow;
pub use workflows::treasury::ftp as modeled_product_ftp;
pub use workflows::treasury::sources as modeled_product_ftp_source;
