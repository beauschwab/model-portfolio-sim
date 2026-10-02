//! Shared financial model contracts. Production calculations stay in Rust;
//! callers own transport, provenance persistence and independent verification.
//! Existing model versions are never implicitly replaced by these models.

pub mod amortization;
pub mod cashflow;
pub mod credit;
pub mod credit_calibration;
pub mod floating_rate;
pub mod limits;
