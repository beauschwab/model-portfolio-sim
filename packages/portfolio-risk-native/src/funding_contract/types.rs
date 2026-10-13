//! Immutable input policies and typed state/audit output.
use crate::term_enhancements::{indexed::ContractPeriod, Identity};
use portfolio_model_core::cashflow::{CashflowMeasure, DatedSchedule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingDirection {
    Lending,
    Borrowing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingKind {
    Interbank,
    UnsecuredFunding,
    Repo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingStatus {
    New,
    Opening,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Available,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Owned,
    ReceivedCollateral,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollateralAsset {
    pub asset_id: String,
    pub owner_entity_id: String,
    pub currency: String,
    pub asset_class: String,
    pub ownership: Ownership,
    pub ownership_source: Identity,
    pub quantity: f64,
    /// Outside this request; never released by one of this request's contracts.
    pub already_encumbered_quantity: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatedAmount {
    pub ordinal: i32,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub asset_id: String,
    pub quantity: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollateralValue {
    pub asset_id: String,
    pub market_price: f64,
    pub eligible: bool,
    /// Discount from collateral market value, not an initial-margin ratio.
    pub haircut: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollateralSnapshot {
    pub source: Identity,
    pub ordinal: i32,
    pub values: Vec<CollateralValue>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarginBreach {
    Reject,
    Report,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarginStep {
    pub ordinal: i32,
    pub management_access: Access,
    pub action_source: Identity,
    pub accrued_interest_claim: f64,
    pub accrued_right_source: Identity,
    pub snapshot: CollateralSnapshot,
    /// Positive delivers additional owned collateral, negative returns pledged
    /// collateral. A substitution is one explicitly paired delivery/return.
    pub transfers: Vec<Allocation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollateralAgreement {
    pub policy: Identity,
    pub eligible_asset_classes: Vec<String>,
    pub include_accrued_interest: bool,
    pub initial_accrued_interest_claim: f64,
    pub initial_accrued_right_source: Identity,
    pub minimum_transfer_value: f64,
    pub margin_breach: MarginBreach,
    pub initial_snapshot: CollateralSnapshot,
    pub initial_pledges: Vec<Allocation>,
    pub margin_steps: Vec<MarginStep>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waterfall {
    PrincipalThenInterest,
    InterestThenPrincipal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollateralRealization {
    pub policy: Identity,
    pub ordinal: i32,
    pub snapshot: CollateralSnapshot,
    pub sale_discount: f64,
    pub sale_cost: f64,
    pub waterfall: Waterfall,
    /// Supported immediate surplus return. A delayed custodial payable needs a
    /// separate policy/state rather than pretending it is funding principal.
    pub surplus_return_ordinal: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultPolicy {
    pub policy: Identity,
    pub ordinal: i32,
    pub accrued_interest_claim: f64,
    pub accrued_right_source: Identity,
    pub economic_principal_loss: f64,
    pub economic_interest_loss: f64,
    pub recovery_source: Identity,
    pub complete_principal_recoveries: Vec<DatedAmount>,
    pub complete_interest_recoveries: Vec<DatedAmount>,
    pub collateral_realization: Option<Box<CollateralRealization>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rollover {
    pub election_source: Identity,
    pub market_access: Access,
    pub requested_principal: f64,
    pub replacement: Option<Box<FundingContract>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Exit {
    Maturity { rollover: Option<Box<Rollover>> },
    CounterpartyDefault { policy: Box<DefaultPolicy> },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingContract {
    pub contract: Identity,
    pub position_id: String,
    pub entity_id: String,
    pub counterparty_id: String,
    pub currency: String,
    pub direction: FundingDirection,
    pub funding_kind: FundingKind,
    pub status: FundingStatus,
    pub principal: f64,
    pub start_ordinal: i32,
    pub maturity_ordinal: i32,
    pub rate_path: Identity,
    /// Fee received by cash lender; sign reverses for a borrowing book.
    pub origination_fee: f64,
    pub interest_periods: Vec<ContractPeriod>,
    pub collateral: Option<Box<CollateralAgreement>>,
    pub exit: Exit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundingRequest {
    pub schema: String,
    pub model_version: String,
    pub assumption: Identity,
    pub scenario_id: String,
    pub measure: CashflowMeasure,
    pub monetary_unit: String,
    pub currency: String,
    pub asof_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub tail_end_ordinal: i32,
    pub observed_publication_cutoff_unix_seconds: i64,
    pub collateral_assets: Vec<CollateralAsset>,
    pub contracts: Vec<FundingContract>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CollateralMovement {
    pub position_id: String,
    pub ordinal: i32,
    pub asset_id: String,
    pub owner_entity_id: String,
    pub quantity_delta: f64,
    pub disposition: String,
    pub source: Identity,
}
#[derive(Debug, Clone, Serialize)]
pub struct MarginAudit {
    pub position_id: String,
    pub ordinal: i32,
    pub snapshot: Identity,
    pub required_exposure: f64,
    pub eligible_value_before: f64,
    pub eligible_value_after: f64,
    pub margin_call_value: f64,
    pub unmet_margin_value: f64,
    pub management_access: Access,
    pub requested_transfers: Vec<Allocation>,
    pub executed_transfers: Vec<Allocation>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RealizationAudit {
    pub position_id: String,
    pub ordinal: i32,
    pub policy: Identity,
    pub collateral_market_value: f64,
    pub gross_sale_proceeds: f64,
    pub sale_cost: f64,
    pub net_sale_proceeds: f64,
    pub principal_recovery: f64,
    pub interest_recovery: f64,
    pub surplus_returned_to_owner: f64,
    /// Gross sale/cost/surplus are explanations, not additional cash rows.
    pub cash_already_in_funding_schedule: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct RolloverAudit {
    pub position_id: String,
    pub ordinal: i32,
    pub source: Identity,
    pub requested_principal: f64,
    pub executed_principal: f64,
    pub market_access_shortfall: f64,
    pub replacement_position_id: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PrincipalAudit {
    pub position_id: String,
    pub entity_id: String,
    pub counterparty_id: String,
    pub contract: Identity,
    pub rate_path: Identity,
    pub coupon_audit: Vec<crate::term_enhancements::indexed::PeriodAudit>,
    pub direction: FundingDirection,
    pub funding_kind: FundingKind,
    pub opening_principal: f64,
    pub new_advance_principal: f64,
    pub cash_principal_settled: f64,
    pub economic_principal_loss: f64,
    pub economic_interest_loss: f64,
    pub live_principal_after_reporting: f64,
    pub gross_default_claim_after_reporting: f64,
    pub expected_principal_recoveries_after_reporting: f64,
    pub rate_calendar_mapping_verified: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct InventoryAudit {
    pub asset_id: String,
    pub original_quantity: f64,
    pub disposed_quantity: f64,
    pub remaining_quantity: f64,
    pub outside_encumbered_quantity: f64,
    pub encumbered_quantity_after_reporting: f64,
    pub encumbered_quantity_at_tail_end: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct FundingResult {
    pub schema: String,
    pub model_version: String,
    pub assumption: Identity,
    pub schedule: DatedSchedule,
    pub contracts: Vec<PrincipalAudit>,
    pub collateral_movements: Vec<CollateralMovement>,
    pub margin_calls: Vec<MarginAudit>,
    pub collateral_realizations: Vec<RealizationAudit>,
    pub rollover: Vec<RolloverAudit>,
    pub inventory: Vec<InventoryAudit>,
    pub disclosure: String,
}
