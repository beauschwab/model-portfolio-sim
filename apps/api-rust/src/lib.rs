//! Rust workbench service (stage E of the Rust liquidity lifecycle plan, issue #47).
//!
//! One binary provides same-origin HTTP admission, a durable leased worker and admin commands
//! over the existing SQLite/PostgreSQL job, revision and workspace contracts. The worker runs the
//! native `portfolio-model-ledger` process, verifies its stream with the independent
//! `portfolio-verification-native` crate, writes bounded checksummed objects, rereads and
//! re-verifies them, and publishes only in a final transaction fenced by the lease, the attempt
//! token, cancellation, the routing epoch, the immutable authority row and the original deadline.
//! It contains no financial calculation and no Python.
// The HTTP contract tables are large `json!` literals.
#![recursion_limit = "512"]
pub mod compose;
pub mod config;
pub mod contract;
pub mod fault;
pub mod http;
pub mod migrate;
pub mod objects;
pub mod publish;
pub mod repo;
pub mod sql;
pub mod worker;

use sha2::{Digest, Sha256};

pub const LIFECYCLE_SCHEMA: &str = "liquidity-lifecycle-1";
/// The independent verifier compiled into this service.
pub const VERIFIER: &str = concat!(
    "portfolio-verification-native/",
    "0.1.0",
    "+liquidity-lifecycle-1"
);

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// A refusal with a stable code; HTTP maps codes to statuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub detail: String,
    /// Actionable mapping requirements, when the refusal lists them.
    pub requirements: Vec<serde_json::Value>,
}

impl Refusal {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Refusal {
        Refusal {
            code,
            detail: detail.into(),
            requirements: Vec::new(),
        }
    }
}

impl From<String> for Refusal {
    fn from(detail: String) -> Self {
        Refusal::new("STORAGE_ERROR", detail)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
