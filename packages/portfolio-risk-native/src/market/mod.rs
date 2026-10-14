//! Market representations, supplied analysis and native path construction.
//! Inputs remain explicit; this domain performs no provider or storage IO.
pub mod analysis;
pub mod curves;
mod paths;

// Preserve the existing market API while keeping the implementation located by responsibility.
pub use paths::*;
pub mod cache;
pub(crate) mod calibration_cache;
pub mod indexed_adapter;
pub mod lifecycle;
pub mod product_source;
