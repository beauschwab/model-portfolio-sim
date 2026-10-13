use crate::products::installment::graph::{BaseAnchor, GraphIdentity, OasTarget, Role, Scenario};
use portfolio_model_core::mortgage_state::{
    ArmReset, MortgagePathPoint, MortgageRequest, MortgageResult,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPolicy {
    CompleteGeneratedInvestorCashTail,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub weight: f64,
    pub assumption_id: String,
    pub path: Vec<MortgagePathPoint>,
    pub arm_resets: Vec<ArmReset>,
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
    pub contract: MortgageRequest,
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
    pub projection: MortgageResult,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalAmounts {
    pub gross_recognized_investor_interest: f64,
    pub borrower_principal_cash: f64,
    pub borrower_interest_cash: f64,
    pub investor_principal_cash: f64,
    pub investor_interest_cash: f64,
    pub principal_entitlement: f64,
    pub interest_entitlement: f64,
    pub borrower_default_principal: f64,
    pub investor_default_principal: f64,
    pub borrower_default_contract_interest: f64,
    pub investor_default_accrued_interest: f64,
    pub borrower_principal_loss: f64,
    pub borrower_recovery_principal_cash: f64,
    pub whole_loan_recovery_principal_cash: f64,
    pub agency_buyout_principal: f64,
    pub agency_buyout_interest: f64,
    pub agency_default_guaranteed_coupon_interest: f64,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalTotals {
    pub borrower_scheduled_principal: f64,
    pub borrower_turnover_principal: f64,
    pub borrower_refinance_principal: f64,
    pub investor_scheduled_principal: f64,
    pub investor_turnover_principal: f64,
    pub investor_refinance_principal: f64,
    pub guarantee_principal_advance: f64,
    pub guarantee_interest_advance: f64,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub position_id: String,
    pub scenario_id: String,
    pub pv: f64,
    pub supplied_history_dv01: f64,
    pub physical_periods: Vec<PhysicalAmounts>,
    pub physical_tail: PhysicalAmounts,
    pub physical_totals: PhysicalTotals,
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
