use crate::{
    advanced_graph::{BaseAnchor, GraphIdentity, OasTarget, Role, Scenario},
    funding_contract::{FundingRequest, FundingResult},
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CashTiming {
    AfterOpeningActions,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TailPolicy {
    CompleteEconomicCashCollateralTail,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub request: FundingRequest,
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
pub struct Request {
    pub schema: String,
    pub identity: GraphIdentity,
    pub book_id: String,
    pub as_of_ordinal: i32,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub bump_bp: f64,
    pub cash_timing: CashTiming,
    pub tail_policy: TailPolicy,
    /// Known contract/opening anchor. Contractual measure, maturity exits without
    /// elections, and empty future margin actions. No financial projection of
    /// this metadata template is implied; conditional rate slots are explicit.
    pub known: FundingRequest,
    pub oas_target: OasTarget,
    pub scenarios: Vec<Scenario>,
    pub histories: Vec<ScenarioHistories>,
}
#[derive(Clone, Serialize)]
pub struct NativeSource {
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub weight: f64,
    pub role: Role,
    pub request: FundingRequest,
    pub projection: FundingResult,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalAmounts {
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub advance_principal_cash: f64,
    pub maturity_principal_cash: f64,
    pub recovery_principal_cash: f64,
    pub recovery_interest_cash: f64,
    pub default_principal: f64,
    pub default_accrued_interest_claim: f64,
    pub economic_principal_loss: f64,
    pub economic_interest_loss: f64,
    pub margin_call_value: f64,
    pub unmet_margin_value: f64,
    pub requested_rollover_principal: f64,
    pub executed_rollover_principal: f64,
    pub rollover_access_shortfall: f64,
    /// Realization explanations are NEVER additional cash/PV amounts.
    pub collateral_gross_sale_proceeds: f64,
    pub collateral_sale_cost: f64,
    pub collateral_net_sale_proceeds: f64,
    pub collateral_surplus_returned: f64,
}
#[derive(Clone, Serialize)]
pub struct SecurityMovement {
    pub asset_id: String,
    pub owner_entity_id: String,
    pub disposition: String,
    pub quantity_delta: f64,
}
#[derive(Clone, Serialize)]
pub struct PhysicalReport {
    pub amounts: PhysicalAmounts,
    pub security_movements: Vec<SecurityMovement>,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub book_id: String,
    pub scenario_id: String,
    pub pv: f64,
    pub supplied_history_dv01: f64,
    pub physical_periods: Vec<PhysicalReport>,
    pub physical_tail: PhysicalReport,
    pub opening_actions: PhysicalReport,
}
#[derive(Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub identity: GraphIdentity,
    pub base_anchor: BaseAnchor,
    pub metrics: Vec<Metrics>,
    pub native_sources: Vec<NativeSource>,
    pub pricing_policy: String,
    pub income_policy: String,
    pub source_status: String,
}
