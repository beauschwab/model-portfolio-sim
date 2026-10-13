use crate::products::installment::graph::{BaseAnchor, GraphIdentity, OasTarget, Role, Scenario};
use portfolio_model_core::consumer_card::{CardRequest, CardResult, CardStep};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPolicy {
    ZeroRightsZeroLimitAtTail,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CashTiming {
    AfterOpeningActions,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub assumption_id: String,
    pub steps: Vec<CardStep>,
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
    pub oas_target: OasTarget,
    pub contract: CardRequest,
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
    pub terminal_policy: TerminalPolicy,
    pub cash_timing: CashTiming,
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
    pub assumption_id: String,
    pub role: Role,
    pub projection: CardResult,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalAmounts {
    pub gross_contract_interest: f64,
    pub principal_cash: f64,
    pub draw_cash: f64,
    pub principal_payment_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub recovery_principal_cash: f64,
    pub uncovered_recovery_cost_cash: f64,
    pub default_principal: f64,
    pub default_accrued_interest: f64,
    pub default_recognized_accrued_interest: f64,
    pub default_unrecognized_contract_interest: f64,
    pub default_fees: f64,
    pub principal_loss: f64,
    pub recovery_gross: f64,
    pub recovery_cost: f64,
    pub recovery_net: f64,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub position_id: String,
    pub scenario_id: String,
    pub pv: f64,
    pub supplied_history_dv01: f64,
    /// Actual (left,right] reporting intervals; economic recognition is not added cash.
    pub physical_periods: Vec<PhysicalAmounts>,
    /// Full generated tail beyond the last reporting boundary, retained in PV/source.
    pub physical_tail: PhysicalAmounts,
    /// Same-asof action cash is retained explicitly and excluded from after-actions PV.
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
