//! Shared exact-history clocks and provenance. No snapshot timing is inferred.
use crate::cashflow::validate_ordinal;
use serde::{Deserialize, Serialize};

pub(crate) const SECONDS_PER_YEAR: f64 = 365.0 * 86_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationTime {
    pub ordinal: i32,
    pub seconds_of_day: u32,
}

impl ObservationTime {
    pub(crate) fn seconds(self) -> Result<i64, String> {
        validate_ordinal(self.ordinal)?;
        if self.seconds_of_day >= 86_400 {
            return Err("seconds_of_day must be in 0..86399".into());
        }
        Ok(i64::from(self.ordinal) * 86_400 + i64::from(self.seconds_of_day))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationMode {
    ExactTransitionTimes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Observed,
    Synthetic,
}
