use crate::accounting::credit::expected::{AccruedPolicy, Identity};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionWeight {
    pub lag_days: u32,
    pub weight: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemainderOwnership {
    RetainBorrowerClaim,
    ReviewedEconomicDisposal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultInterestOwnership {
    pub review: Identity,
    /// Accounting disposal is distinct from disposal of the contractual borrower right.
    pub accounting_accrued: AccruedPolicy,
    pub collection_fraction: [f64; 3],
    pub complete_collection_tails: [Vec<CollectionWeight>; 3],
    pub economic_remainder: RemainderOwnership,
    pub economic_disposal_review: Option<Identity>,
    pub disposal_lag_days: u32,
}
