//! Independent Rust verification of `liquidity-lifecycle-1` sources and persisted journals
//! (stage D of the Rust liquidity lifecycle plan, issue #46).
//!
//! This crate is the second implementation. It depends on no production financial crate: it
//! parses the raw request and the native process stream itself, prices the raw swap contracts
//! with its own equations, replays the acquisitions and unwinds with its own arithmetic, and
//! compares the result with the lineage rows, the ordered journal transactions, the closing
//! stocks and the producer's receipt. Hashes establish identity, not economic correctness; raw
//! byte identity and typed normalization are checked separately from monetary tolerances.
pub mod expect;
pub mod raw;
pub mod stream;
pub mod swap;
pub mod verify;

pub use verify::{verify, Receipt};

pub const SCHEMA: &str = "liquidity-lifecycle-1";
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;
pub const NAMESPACE: &str = "@lifecycle:";

/// The unchanged financial tolerance: 1e-12 relative, 1e-9 absolute.
pub fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
