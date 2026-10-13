//! Proposed Core shared transport contracts only. No Ledger dependency or financial backend.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source_id: String,
    pub owner_index: usize,
    pub run_index: usize,
    pub path_index: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub id: String,
    pub revision: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DifferencePolicy {
    pub review: Review,
    pub benchmark: Binding,
    pub methodology: String,
    pub annual_rate: f64,
    pub rate_vintage: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChargePolicy {
    pub binding: Binding,
    pub review: Review,
    pub curve_id: String,
    pub option: DifferencePolicy,
    pub contingent: DifferencePolicy,
    pub other_earnings: String,
    pub cost_review: Review,
    pub daily_operating_cost: f64,
    pub allocated_capital: f64,
    pub annual_cost_of_capital: f64,
}
