use crate::products::derivatives::contract::{
    csa::{CsaResult, PhysicalIndexPath},
    DerivativeRequest, Identity,
};
use portfolio_model_core::cashflow::CashflowMeasure;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBasis {
    ModeledPhysicalScenario,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Designation {
    UndesignatedTrading,
    UndesignatedEconomicHedge,
    FairValueHedge,
    CashFlowHedge,
    NetInvestmentHedge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkRecognition {
    CleanValueChangesInEarnings,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CouponRecognition {
    SettleDerivativeCarrying,
    ContractualIncomeAtDueWithMarkOffset,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticRecognition {
    ReportOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CustodyRecognition {
    SegregatedCustodianMemoOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultRecognition {
    ReviewedModeledCloseoutShortfallInEarnings,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeReview {
    pub position_id: String,
    pub designation: Designation,
    pub designation_review: Identity,
    pub mark_recognition: MarkRecognition,
    pub coupon_recognition: CouponRecognition,
    pub delivery_review: Option<Identity>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingPolicy {
    pub identity: Identity,
    pub trades: Vec<TradeReview>,
    pub custody: CustodyRecognition,
    pub default: DefaultRecognition,
    pub xva: DiagnosticRecognition,
    pub modeled_funding_cost: DiagnosticRecognition,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TradeAnchor {
    pub position_id: String,
    pub clean_value: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningState {
    pub source: Identity,
    pub ordinal: i32,
    pub cash: f64,
    pub trades: Vec<TradeAnchor>,
    pub posted_vm_asset: f64,
    pub received_vm_liability: f64,
    pub posted_im_asset: f64,
    pub received_im_custody: f64,
    pub equity_credit_balance: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivativeAccountingRequest {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub scenario_id: String,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub measure: CashflowMeasure,
    pub basis: ExecutionBasis,
    pub policy: AccountingPolicy,
    pub opening: OpeningState,
    pub derivative: DerivativeRequest,
    pub physical_index_paths: Vec<PhysicalIndexPath>,
    pub pricing_xva_reference: Option<Box<DerivativeRequest>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountCode {
    Cash,
    DerivativeAsset,
    DerivativeLiability,
    UnpaidReceivable,
    UnpaidPayable,
    PostedVmAsset,
    ReceivedVmLiability,
    PostedImAsset,
    OpeningEquity,
    MarkGainLoss,
    ContractualIncome,
    CollateralInterestIncome,
    CounterpartyLossExpense,
    PostedCollateralReturnLossExpense,
    MemoCustodianAsset,
    MemoCustodianObligation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalLedger {
    Financial,
    CustodyMemo,
}
#[derive(Debug, Clone, Serialize)]
pub struct Posting {
    pub account: AccountCode,
    pub position_id: Option<String>,
    pub bucket: Option<String>,
    pub debit_amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct JournalIntent {
    pub event_id: String,
    pub ordinal: i32,
    pub ledger: JournalLedger,
    pub run: Identity,
    pub scenario_id: String,
    pub entity: String,
    pub currency: String,
    pub policy: Identity,
    pub sources: Vec<Identity>,
    pub mapped_position_id: Option<String>,
    pub contract_position_id: Option<String>,
    pub bucket: Option<String>,
    pub measure: CashflowMeasure,
    pub basis: ExecutionBasis,
    pub postings: Vec<Posting>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AccountBalance {
    pub account: AccountCode,
    pub debit_balance: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ClosingTrade {
    pub mapped_position_id: String,
    pub contract_position_id: String,
    pub clean_carrying: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DailyClosing {
    pub ordinal: i32,
    pub cash: f64,
    pub trades: Vec<ClosingTrade>,
    pub unpaid_receivable: f64,
    pub unpaid_payable: f64,
    pub posted_vm_asset: f64,
    pub received_vm_liability: f64,
    pub posted_im_asset: f64,
    pub received_im_custody: f64,
    pub financial_trial_balance: Vec<AccountBalance>,
    pub custody_trial_balance: Vec<AccountBalance>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CashRight {
    pub position_id: String,
    pub contract_position_id: String,
    pub event_id: String,
    pub payment_ordinal: i32,
    pub physical_amount: f64,
    pub pricing_due_amount: f64,
    pub contract: Identity,
    pub recognition_sources: Vec<Identity>,
    pub sources: Vec<crate::products::term::contracts::indexed::RateSource>,
    pub physical_path: Option<Identity>,
    pub settled_ordinal: Option<i32>,
    pub closeout_ordinal: Option<i32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DerivativeAccountingResult {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub measure: CashflowMeasure,
    pub basis: ExecutionBasis,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub policy: AccountingPolicy,
    pub opening: OpeningState,
    pub intents: Vec<JournalIntent>,
    pub daily: Vec<DailyClosing>,
    pub cash_rights: Vec<CashRight>,
    pub native: CsaResult,
    pub pricing_xva_diagnostic: Option<CsaResult>,
    pub disclosure: String,
}
