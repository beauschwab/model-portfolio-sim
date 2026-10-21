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

pub use verify::{verify, verify_reader, verify_stream, Receipt};

pub const SCHEMA: &str = "liquidity-lifecycle-1";
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;
/// The explicit large tier (#51): selected by `"scale_tier": "acceptance_60000"` in the raw
/// request. Without it every bound is unchanged.
pub const SCALE_TIER: &str = "acceptance_60000";
pub const MAX_TIER_REQUEST_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_TIER_STREAM_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// One stream line (a journal or table batch) is bounded on every tier.
pub const MAX_LINE_BYTES: usize = 256 * 1024 * 1024;

/// Strict raw admission of a lifecycle request and its tier: at most 8 MiB, or 512 MiB with the
/// explicit tier. Any other tier value refuses.
pub fn admit_request(bytes: &[u8]) -> Result<(serde_json::Value, bool), String> {
    let value = raw::parse(bytes, MAX_TIER_REQUEST_BYTES)?;
    let tier = match value.get("scale_tier") {
        None => false,
        Some(t) if t.as_str() == Some(SCALE_TIER) => true,
        Some(_) => return Err(format!("scale_tier must be `{SCALE_TIER}` when present")),
    };
    if !tier && bytes.len() > MAX_REQUEST_BYTES {
        return Err(
            "raw request exceeds 8 MiB; larger requests select the acceptance_60000 tier".into(),
        );
    }
    Ok((value, tier))
}

/// The stream bound of a tier.
pub fn stream_bound(tier: bool) -> u64 {
    if tier {
        MAX_TIER_STREAM_BYTES
    } else {
        MAX_STREAM_BYTES as u64
    }
}
pub const NAMESPACE: &str = "@lifecycle:";

/// The unchanged financial tolerance: 1e-12 relative, 1e-9 absolute.
pub fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
