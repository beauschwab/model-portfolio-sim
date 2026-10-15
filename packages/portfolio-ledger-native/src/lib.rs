//! Native daily specification/ledger runtime and pure ordered GL reduction.
//! Product pricing is external; policy/events execute in daily, transport persists.
//! Reduction calls retain no pointers or global state.

pub mod journal;
pub mod owners;
pub mod replay;
pub mod reports;
pub mod transport;
pub mod validation;

// Retained public Rust entrypoints and historical module paths.
pub use journal::acquisition;
pub use journal::credit as credit_accounting;
pub use owners::commercial::fees as commercial_fee_owner;
pub use owners::commercial::intents as commercial_intent_owner;
pub use owners::consumer as consumer_intent_owner;
pub use owners::credit::state as credit_state;
pub use owners::deposits::fifo_derivation as deposit_fifo_derivation;
pub use owners::deposits::revision as deposit_revision;
pub use owners::deposits::source_portfolio as deposit_source_portfolio;
pub use owners::deposits::source_principal as deposit_source_principal;
pub use owners::derivatives as derivative_intent_owner;
pub use owners::mortgages::pending as mortgage_pending_owner;
pub use owners::product_events as product_event_owner;
pub use replay::credit as credit_replay;
pub use replay::credit_paths as credit_path_replay;
pub use replay::daily;
pub use replay::dated as dated_ledger;
pub use replay::driven_credit;
#[cfg(any(test, feature = "historical-output-control"))]
#[doc(hidden)]
pub use replay::historical as historical_daily;
pub use replay::stores::cashflows as flow_store;
pub use replay::stores::dated as dated_store;
pub use transport::ffi::portfolio_balance_free;
pub use transport::ffi::portfolio_balance_validate;
pub use transport::ffi::portfolio_ledger_abi_version;
pub use transport::ffi::portfolio_ledger_apply;
pub use transport::types;
pub use validation::specification as spec;
