//! Shared financial-input admission. Currency amounts are actual currency units,
//! never an undisclosed thousands/millions multiplier. Component limits do not
//! imply a total process RSS cap or a regulator-approved exposure limit.

pub const MAX_MONETARY_AMOUNT: f64 = 1.0e15;
pub const MAX_DATED_METADATA_BYTES: usize = 32 * 1024 * 1024;
