//! Historical DTO-only admission. No pricing or cashflow arithmetic.
use crate::{
    advanced_graph::{GraphIdentity, OasTarget, Scenario},
    term_enhancements::{
        cd::{CdElectionRequest, CdPolicy, CdState},
        indexed::ContractPathRequest,
    },
};
use portfolio_model_core::floating_rate::Direction;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CashTiming {
    AfterOpeningActions,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CdKnown {
    pub model_version: String,
    pub currency: String,
    pub direction: Direction,
    pub policy: CdPolicy,
    pub opening: CdState,
    pub tail_end_ordinal: i32,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnownProduct {
    IndexedTerm { contract: ContractPathRequest },
    CdElection { contract: CdKnown },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProductHistory {
    IndexedTerm { request: ContractPathRequest },
    CdElection { request: CdElectionRequest },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub product: ProductHistory,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioHistories {
    pub scenario_id: String,
    pub physical: Vec<History>,
    pub pricing: Vec<History>,
    pub pricing_down: Vec<History>,
    pub pricing_up: Vec<History>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub known: KnownProduct,
    pub oas_target: OasTarget,
    pub histories: Vec<ScenarioHistories>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub identity: GraphIdentity,
    pub as_of_ordinal: i32,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub bump_bp: f64,
    pub cash_timing: CashTiming,
    pub scenarios: Vec<Scenario>,
    pub positions: Vec<Position>,
}

impl Request {
    pub(crate) fn into_current(self) -> Result<crate::products::term::graph::Request, String> {
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}
