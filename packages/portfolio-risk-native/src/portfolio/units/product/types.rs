use crate::{
    advanced_graph,
    aged_units::{AgedIdentity, Weights},
    card_graph, deposit_graph, mortgage_graph,
};
use portfolio_model_core::dated_income::Identity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GraphRequest {
    ConsumerInstallment(Box<advanced_graph::Request>),
    Card(Box<card_graph::Request>),
    Mortgage(Box<mortgage_graph::Request>),
    Deposit(Box<deposit_graph::Request>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomePolicy {
    pub review: Identity,
    pub original_yield_anchor: Identity,
    pub original_annual_effective_rate: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgeBranch {
    pub ordinal: i32,
    pub scenario_id: String,
    pub physical_path_id: String,
    pub graph: GraphRequest,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductUnit {
    pub unit_id: String,
    pub template_id: String,
    pub purchase_ordinal: i32,
    pub unit_face: f64,
    pub quote_price_per_face_unit: f64,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub weights: Weights,
    pub purchase_graph: GraphRequest,
    pub income_policy: IncomePolicy,
    pub age_branches: Vec<AgeBranch>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductUnitsRequest {
    pub schema: String,
    pub identity: AgedIdentity,
    pub currency: String,
    pub monetary_unit: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub units: Vec<ProductUnit>,
}
#[derive(Clone, Serialize)]
pub struct IncomeState {
    pub ordinal: i32,
    pub principal: f64,
    pub accrued_interest: f64,
    pub gross_carrying_value: f64,
    pub book_adjustment: f64,
    pub effective_interest_to_date: f64,
}
#[derive(Clone, Serialize)]
pub struct IncomeReceipt {
    pub scenario_id: String,
    pub path_id: String,
    pub source_weight: f64,
    pub opening_states: Vec<IncomeState>,
    pub native_income: Value,
}
#[derive(Clone, Serialize)]
pub struct AgeReceipt {
    pub ordinal: i32,
    pub scenario_id: String,
    pub physical_path_id: String,
    pub physical_weight: f64,
    pub pv_per_face: f64,
    pub dv01_per_face: f64,
    pub locked_purchase_oas: f64,
    pub complete_native_result: Value,
}
#[derive(Clone, Serialize)]
pub struct UnitReceipt {
    pub unit_id: String,
    pub template_id: String,
    pub purchase_ordinal: i32,
    pub unit_face: f64,
    pub side: f64,
    pub quote_price_per_face_unit: f64,
    pub locked_purchase_oas: f64,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub complete_purchase_result: Value,
    pub income: Vec<IncomeReceipt>,
    pub age_marks: Vec<AgeReceipt>,
}
#[derive(Clone, Serialize)]
pub struct FundingEvent {
    pub scenario_id: String,
    pub unit_index: usize,
    pub path_id: String,
    pub source_event_id: String,
    pub ordinal: i32,
    pub source_weight: f64,
    pub purchase_cash: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub bank_cash: f64,
}
#[derive(Clone, Serialize)]
pub struct ProductFunding {
    pub scenario_ids: Vec<String>,
    pub ordinals: Vec<i32>,
    pub shape: [usize; 3],
    pub demand: Vec<f64>,
    pub cash_events: Vec<FundingEvent>,
    pub policy: String,
}
#[derive(Serialize)]
pub struct ProductUnitsResult {
    pub schema: String,
    pub identity: AgedIdentity,
    pub raw_source: ProductUnitsRequest,
    pub currency: String,
    pub monetary_unit: String,
    pub base_scenario_id: String,
    pub scenario_ids: Vec<String>,
    pub reporting_ordinals: Vec<i32>,
    pub shape: [usize; 4],
    pub metrics: Vec<String>,
    pub values: Vec<f64>,
    pub units: Vec<UnitReceipt>,
    pub funding: ProductFunding,
    pub admitted_work: usize,
    pub income_policy: String,
    pub scope: String,
}
