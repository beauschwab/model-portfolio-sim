use crate::products::installment::graph::{BaseAnchor, GraphIdentity, OasTarget, Role, Scenario};
use portfolio_model_core::deposit_state::{DepositStateRequest, DepositStateResult};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TailPolicy {
    ZeroLiabilityRightsAtTail,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashPolicy {
    SuppliedBankPerspectiveAfterOpeningActions,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub request: DepositStateRequest,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioHistories {
    pub scenario_id: String,
    pub physical: Vec<History>,
    pub pricing: Vec<History>,
    pub pricing_down: Vec<History>,
    pub pricing_up: Vec<History>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub oas_target: OasTarget,
    pub histories: Vec<ScenarioHistories>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub identity: GraphIdentity,
    pub as_of_ordinal: i32,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub bump_bp: f64,
    pub tail_policy: TailPolicy,
    pub cash_policy: CashPolicy,
    pub scenarios: Vec<Scenario>,
    pub positions: Vec<Position>,
}
#[derive(Clone, Serialize)]
pub struct NativeSource {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub weight: f64,
    pub role: Role,
    pub projection: DepositStateResult,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalAmounts {
    pub interest_expense: f64,
    pub bank_principal_cash: f64,
    pub bank_interest_cash: f64,
    pub external_inflow: f64,
    pub withdrawal: f64,
    pub closure: f64,
    pub contractual_maturity: f64,
    pub assumed_terminal_payout: f64,
    pub migrated_principal: f64,
    pub migrated_accrued_interest: f64,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub position_id: String,
    pub scenario_id: String,
    pub liability_pv: f64,
    pub bank_cash_pv: f64,
    pub supplied_history_liability_dv01: f64,
    pub physical_periods: Vec<PhysicalAmounts>,
    pub physical_tail: PhysicalAmounts,
    pub opening_action_cash: PhysicalAmounts,
}
#[derive(Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub identity: GraphIdentity,
    pub base_anchor: BaseAnchor,
    pub metrics: Vec<Metrics>,
    pub native_sources: Vec<NativeSource>,
    pub income_policy: String,
    pub risk_policy: String,
    pub source_status: String,
}
