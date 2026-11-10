//! Explicit ongoing economic owners. Dates and rate policy are supplied; amounts are native.
use super::acquisition::VariationMarginTerms;
use super::types::Revision;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivativeUpdate {
    pub lot_id: String,
    pub ordinal: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VariationMarginReview {
    pub agreement_id: String,
    pub ordinal: i32,
    pub terms: VariationMarginTerms,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterestBasis {
    Act360,
    Act365Fixed,
}
impl InterestBasis {
    pub fn denominator(self) -> f64 {
        match self {
            Self::Act360 => 360.,
            Self::Act365Fixed => 365.,
        }
    }
}

/// Daily simple interest on the previous day's closing drawn principal. Opening accrued
/// interest is explicitly zero; payments clear accrued rights, never recognize expense twice.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityInterest {
    pub facility_id: String,
    pub annual_rate: f64,
    pub day_count: InterestBasis,
    pub payment_ordinals: Vec<i32>,
    pub review: Revision,
}
