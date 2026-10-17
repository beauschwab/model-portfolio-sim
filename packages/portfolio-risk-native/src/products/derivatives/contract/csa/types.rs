use super::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub position_id: String,
    pub contract: Identity,
    pub entity: String,
    pub counterparty: String,
    pub currency: String,
    pub netting_set_id: String,
    pub legal_review: Identity,
    pub enforceable_netting: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trade {
    Swap {
        request: Box<swaps::SwapRequest>,
    },
    ExpiredEuropean {
        terminal: Box<options::SwaptionRequest>,
        election: Identity,
        delivered_underlying: Option<Box<swaps::SwapRequest>>,
        settlement_discount: Option<Box<swaps::DiscountCurve>>,
    },
    EuropeanSwaption {
        request: Box<options::SwaptionRequest>,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub ordinal: i32,
    pub valuation_cutoff_unix_seconds: i64,
    pub payment_timing: swaps::PaymentTiming,
    pub trades: Vec<Trade>,
    pub discount_from_asof: f64,
    pub discount_source: Identity,
    pub collateral_rate: f64,
    pub collateral_rate_source: Identity,
    pub funding_spread: f64,
    pub funding_source: Identity,
    pub settlement_available: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultPolicy {
    pub default_ordinal: i32,
    pub closeout_ordinal: i32,
    pub legal_review: Identity,
    pub replacement_value_policy: Identity,
    pub counterparty_recovery: f64,
    pub posted_vm_return_recovery: f64,
    pub posted_im_return_recovery: f64,
    pub posted_collateral_setoff: bool,
    pub received_im_available_on_default: bool,
    #[serde(default)]
    pub mpor: Option<MporPolicy>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub path_id: String,
    pub source: Identity,
    pub probability: f64,
    pub nodes: Vec<Node>,
    pub default: Option<DefaultPolicy>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub entity: String,
    pub counterparty: String,
    pub currency: String,
    pub monetary_unit: String,
    pub receive_threshold: f64,
    pub post_threshold: f64,
    pub minimum_transfer: f64,
    pub settlement_lag_calendar_days: u32,
    pub opening_vm: f64,
    pub opening_received_im: f64,
    pub opening_posted_im: f64,
    pub opening_source: Identity,
    pub received_vm_reusable_for_funding: bool,
    pub received_im_segregated: bool,
    pub collateral_interest_day_count: ActualDayCount,
    pub tail_return_ordinal: i32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CsaRequest {
    pub asof_ordinal: i32,
    pub measure: CashflowMeasure,
    pub conditional_model: Identity,
    pub crn: Identity,
    pub policy: Policy,
    pub mapping: Vec<Mapping>,
    pub paths: Vec<Path>,
    /// Explicitly selected own-credit DVA memo (Pricing measure only). Absent keeps the
    /// historical request and result bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_credit: Option<OwnCredit>,
}
/// Reviewed own-default survival curve and recovery for a debit valuation adjustment memo.
/// Survival is supplied at every path node date; nothing is interpolated or inferred.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnCredit {
    pub identity: Identity,
    pub recovery: f64,
    pub survival: Vec<SurvivalPoint>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurvivalPoint {
    pub ordinal: i32,
    pub survival_probability: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Movement {
    pub ordinal: i32,
    pub bucket: String,
    pub kind: String,
    pub entity_cash: f64,
    pub vm_delta: f64,
    pub received_im_delta: f64,
    pub posted_im_delta: f64,
    pub custodian_cash: f64,
    pub modeled_measure: CashflowMeasure,
}
#[derive(Debug, Clone, Serialize)]
pub struct Call {
    pub call_ordinal: i32,
    pub due_ordinal: i32,
    pub settled_ordinal: Option<i32>,
    pub cancelled_at_default: bool,
    pub cancelled_at_tail: bool,
    pub bucket: String,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Exposure {
    pub ordinal: i32,
    pub bucket: String,
    pub clean_value: f64,
    pub vm: f64,
    pub received_im: f64,
    pub posted_im: f64,
    pub unsecured_receivable: f64,
    pub posted_collateral_return_claim: f64,
    pub clean_trade_values: Vec<(String, f64)>,
    pub extinguished_after_closeout: bool,
    pub replacement_after_due_obligations: f64,
    pub unpaid_obligation_claim: f64,
    pub total_closeout_claim: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Closeout {
    pub ordinal: i32,
    pub bucket: String,
    pub clean_replacement_value: f64,
    pub unpaid_obligation_claim: f64,
    pub total_closeout_claim: f64,
    pub vm_applied: f64,
    pub received_im_applied: f64,
    pub unsecured_claim: f64,
    pub unsecured_recovery: f64,
    pub counterparty_loss: f64,
    pub posted_return_loss: f64,
    pub entity_cash: f64,
    pub custodian_return: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct PathResult {
    pub mpor_policy: Option<Identity>,
    pub closing_legal_review: Option<Identity>,
    pub replacement_value_policy: Option<Identity>,
    pub path_id: String,
    pub probability: f64,
    pub source: Identity,
    pub exposures: Vec<Exposure>,
    pub calls: Vec<Call>,
    pub movements: Vec<Movement>,
    pub closeouts: Vec<Closeout>,
    pub discounted_credit_loss: f64,
    pub discounted_funding_cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discounted_own_default_benefit: Option<f64>,
    pub residual_vm: f64,
    pub residual_received_im: f64,
    pub residual_posted_im: f64,
    pub pending_calls: f64,
    pub obligations: Vec<MporObligation>,
    pub mpor_exercises: Vec<MporExercise>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CsaResult {
    pub policy: Identity,
    pub legal_mapping: Vec<Mapping>,
    pub conditional_model: Identity,
    pub crn: Identity,
    pub clean_value: f64,
    pub cva: Option<f64>,
    pub fva: Option<f64>,
    pub clean_less_cva_fva: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own_credit_policy: Option<Identity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dva: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean_less_cva_fva_plus_dva: Option<f64>,
    pub paths: Vec<PathResult>,
    pub disclosure: String,
}

/// Reviewed nominal unpaid-obligation policy; no amount/exposure overrides.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MporPolicy {
    pub identity: Identity,
    pub claim_measure: CashflowMeasure,
    pub carry_policy: UnpaidCarry,
    pub exercise_rights_policy: ExerciseRightsPolicy,
    pub settlement_instructions: Vec<ObligationInstruction>,
    pub physical_index_paths: Vec<PhysicalIndexPath>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnpaidCarry {
    NominalNoInterest,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObligationDisposition {
    RetainToCloseout,
    SettleOnNode { ordinal: i32, evidence: Identity },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationInstruction {
    pub position_id: String,
    pub event_id: String,
    pub payment_ordinal: i32,
    #[serde(deserialize_with = "super::strict_disposition::deserialize")]
    pub disposition: ObligationDisposition,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalIndexPath {
    pub position_id: String,
    pub period_id: String,
    pub path: Identity,
    pub observations: Vec<crate::products::term::contracts::indexed::IndexObservation>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MporObligation {
    pub position_id: String,
    pub event_id: String,
    pub bucket: String,
    pub payment_ordinal: i32,
    pub captured_ordinal: i32,
    pub currency: String,
    pub amount: f64,
    pub measure: CashflowMeasure,
    pub contract: Identity,
    pub selected_rate_sources: Vec<crate::products::term::contracts::indexed::RateSource>,
    pub origin_election: Option<Identity>,
    pub delivered_position_id: Option<String>,
    pub settlement_price_sources: Vec<Identity>,
    pub disposition: ObligationDisposition,
    pub settled_ordinal: Option<i32>,
    pub included_in_closeout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseRightsPolicy {
    PreserveContractualEuropeanToCloseout,
}
#[derive(Debug, Clone, Serialize)]
pub struct MporExercise {
    pub position_id: String,
    pub expiry_ordinal: i32,
    pub contract: Identity,
    pub election: Identity,
    pub state_id: String,
    pub exercised: bool,
    pub settlement: options::Settlement,
    pub delivered_position_id: Option<String>,
    pub netting_set_id: String,
    pub underlying_contract: Identity,
    pub pricing_intrinsic_value: f64,
    pub claim_measure: CashflowMeasure,
    pub conditional_model: Identity,
    pub collateral_discount_source: Identity,
}
