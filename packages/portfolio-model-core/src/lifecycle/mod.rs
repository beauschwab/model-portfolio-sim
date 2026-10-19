//! Stage A of the Rust liquidity lifecycle plan (issue #43): typed `liquidity-lifecycle-1`
//! relationships, strict admission and a declared settlement walk. Contract and rule matrix:
//! docs/models/liquidity-lifecycle-contract.md. Nothing here posts journal entries.
pub mod admission;
pub mod settlement;
pub mod types;
pub use admission::{admit, validate, Admitted};
pub use settlement::{walk, Step, StepStatus, Walk};
pub use types::*;
