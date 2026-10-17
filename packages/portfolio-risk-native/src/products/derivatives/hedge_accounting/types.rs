//! Immutable formal hedge relationship contracts and sealed accounting output.
use crate::products::derivatives::accounting::{
    AccountCode as DerivativeAccount, DerivativeAccountingRequest, DerivativeAccountingResult,
    ExecutionBasis, JournalLedger,
};
use crate::products::derivatives::contract::{swaps::SwapRequest, Identity};
use portfolio_model_core::cashflow::{ActualDayCount, CashflowMeasure};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "hedge-accounting-1";
pub const MODEL_VERSION: &str = "hedge-accounting-1";
pub const RULE_VERSION: &str = "asc815-asu2017-12";
pub const MAX_RELATIONSHIPS: usize = 64;
pub const MAX_EVENTS: usize = 1024;
pub const MAX_SOURCES: usize = 64;
pub const MAX_PERIODS: usize = 4096;
pub const MAX_INPUT: usize = 32 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;
pub const MAX_WORK: usize = 1_000_000;
pub const MAX_GROUP_MEMBERS: usize = 16;
pub const MAX_RATE_OPTIONS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Designation {
    FairValueHedge,
    CashFlowHedge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HedgedRisk {
    BenchmarkInterestRate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentMethod {
    CumulativeDollarOffset,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisAmortization {
    StraightLineActualDaysToPrincipal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reclassification {
    HedgedPeriodHypotheticalSettlement,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    FixedRateInstrument,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemDirection {
    Asset,
    Liability,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureDirection {
    Receipt,
    Payment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Assess,
    Discontinue,
    ForecastStatus,
    /// ASU 2025-09 choose-your-rate election (815-30-35-37I through 35-37M).
    RateElection,
}
/// ASU 2025-09 similar-risk assessment method (815-20-55-23A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SimilarRiskMethod {
    /// 55-23A(a): the instrument, pro rata by member notional, is highly effective against each member.
    InstrumentHighlyEffectiveEachRisk,
    /// 55-23A(b): each member's per-unit hedged change is within the band of every other member's.
    HedgedRisksSimilarToEachOther,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DebtClassification {
    Liability,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscontinueReason {
    Dedesignation,
    HedgedItemSold,
    InstrumentTerminated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastStatus {
    NoLongerProbable,
    ProbableNotToOccur,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Qualified,
    Discontinued,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Elections {
    pub similar_risk_grouping: bool,
    pub selectable_rate_debt: bool,
    pub swap_written_option_exception: bool,
    pub dual_hedge_basis: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asu202509 {
    pub adopted: bool,
    pub adoption_ordinal: Option<i32>,
    pub elections: Elections,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Adoption {
    pub asu_2025_09: Asu202509,
    pub asu_2022_01_portfolio_layer: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentPolicy {
    pub identity: Identity,
    pub method: AssessmentMethod,
    pub lower_bound: f64,
    pub upper_bound: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub rule_version: String,
    pub adoption: Adoption,
    pub assessment: AssessmentPolicy,
    pub fvh_basis_amortization: BasisAmortization,
    pub cfh_reclassification: Reclassification,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Period {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub day_count: ActualDayCount,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgedItem {
    pub item_id: String,
    pub source: Identity,
    pub carrying_source: Identity,
    pub kind: ItemKind,
    pub direction: ItemDirection,
    pub notional: f64,
    pub contractual_rate: f64,
    pub periods: Vec<Period>,
    pub principal_ordinal: i32,
}
/// Documented choose-your-rate options (815-30-35-37D/37J); `index_id` is the initial selection.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseYourRate {
    pub evidence: Identity,
    pub documented_index_ids: Vec<String>,
    pub debt_classification: DebtClassification,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastExposure {
    pub exposure_id: String,
    pub source: Identity,
    pub probability_evidence: Identity,
    pub direction: ExposureDirection,
    pub notional: f64,
    pub index_id: String,
    pub periods: Vec<Period>,
    #[serde(default)]
    pub choose_your_rate: Option<ChooseYourRate>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HypotheticalNode {
    pub ordinal: i32,
    pub request: Box<SwapRequest>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hypothetical {
    pub evidence: Identity,
    pub nodes: Vec<HypotheticalNode>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemberHypothetical {
    pub exposure_id: String,
    pub hypothetical: Hypothetical,
}
/// ASU 2025-09 group of individual forecast exposures with similar risk (815-20-55-23A).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDesignation {
    pub member_exposure_ids: Vec<String>,
    pub similar_risk_method: SimilarRiskMethod,
    pub hypotheticals: Vec<MemberHypothetical>,
}
/// Hypothetical derivative for one documented alternative rate, with terms as of inception (35-37M).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RateHypothetical {
    pub index_id: String,
    pub hypothetical: Hypothetical,
}
/// Reviewed retained balances of a relationship designated before the derivative opening date.
/// `hedged_value` is the after-payment hedged clean value at the opening (per unit of hedged
/// notional for a fair-value hedge, the hypothetical or matched instrument value for a
/// cash-flow hedge); cumulative changes and balances are those through the opening.
/// A frozen basis tranche retained at the opening: amortizes straight-line from `start_ordinal`
/// to the item's principal date; `amortized` is the amount already amortized through the opening.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningFrozenBasis {
    pub amount: f64,
    pub start_ordinal: i32,
    pub amortized: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningFrozenOciEntry {
    pub payment_ordinal: i32,
    pub weight: f64,
}
/// A frozen OCI tranche retained at the opening: `entries` are the remaining hedged payment dates
/// (at or after the opening) with their original release weights; `released` is the amount
/// already reclassified through the opening.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningFrozenOci {
    pub amount: f64,
    pub released: f64,
    pub entries: Vec<OpeningFrozenOciEntry>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipOpening {
    pub evidence: Identity,
    pub hedged_value: f64,
    pub cumulative_instrument_change: f64,
    pub cumulative_hedged_change: f64,
    pub basis_balance: f64,
    pub oci_balance: f64,
    /// Remaining fraction of the designated proportion after earlier partial discontinuations.
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub frozen_basis: Vec<OpeningFrozenBasis>,
    #[serde(default)]
    pub frozen_oci: Vec<OpeningFrozenOci>,
}
impl RelationshipOpening {
    pub fn scale(&self) -> f64 {
        self.scale.unwrap_or(1.)
    }
    pub fn frozen_basis_remaining(&self) -> f64 {
        self.frozen_basis
            .iter()
            .map(|t| t.amount - t.amortized)
            .sum()
    }
    pub fn frozen_oci_remaining(&self) -> f64 {
        self.frozen_oci.iter().map(|t| t.amount - t.released).sum()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub relationship_id: String,
    pub predecessor_relationship_id: Option<String>,
    pub designation: Designation,
    pub hedging_instrument_position_id: String,
    pub designated_proportion: f64,
    pub hedged_item_id: Option<String>,
    pub hedged_notional: Option<f64>,
    pub forecast_exposure_id: Option<String>,
    pub hedged_risk: HedgedRisk,
    pub benchmark_rate: Option<f64>,
    pub inception_ordinal: i32,
    pub risk_management_objective: Identity,
    pub documentation: Identity,
    pub hypothetical: Option<Hypothetical>,
    pub matched_critical_terms_evidence: Option<Identity>,
    #[serde(default)]
    pub group: Option<GroupDesignation>,
    #[serde(default)]
    pub rate_hypotheticals: Option<Vec<RateHypothetical>>,
    /// ASU 2025-09 (815-20-25-88): a written European swaption combined with the swap as one
    /// hedging instrument of interest-rate risk, exempt from the net written option test.
    #[serde(default)]
    pub written_option_position_id: Option<String>,
    /// Required exactly when `inception_ordinal` precedes the derivative opening date.
    #[serde(default)]
    pub opening: Option<RelationshipOpening>,
    /// Partial-term fair-value hedge (815-25-35-13B): the hedged item's benchmark cash flows are
    /// measured as if the item matured at this period end, which also bounds basis amortization.
    #[serde(default)]
    pub hedged_term_end_ordinal: Option<i32>,
    /// Further mapped swaps jointly designated with the primary instrument (815-20-25-45): the
    /// combination is one hedging instrument for clean value, cumulative change and mark ownership.
    #[serde(default)]
    pub additional_instrument_position_ids: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: String,
    pub relationship_id: String,
    pub ordinal: i32,
    pub sequence: u32,
    pub kind: EventKind,
    pub review: Identity,
    #[serde(default)]
    pub proportion: Option<f64>,
    #[serde(default)]
    pub reason: Option<DiscontinueReason>,
    #[serde(default)]
    pub status: Option<ForecastStatus>,
    /// `rate_election`: the newly selected documented index.
    #[serde(default)]
    pub index_id: Option<String>,
    /// Group `assess`: the specific last-similar date when determined (815-20-55-23D).
    #[serde(default)]
    pub effective_ordinal: Option<i32>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeAccountingRequest {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub policy: Policy,
    pub derivative: Box<DerivativeAccountingRequest>,
    pub hedged_items: Vec<HedgedItem>,
    pub forecast_exposures: Vec<ForecastExposure>,
    pub relationships: Vec<Relationship>,
    pub events: Vec<Event>,
}

/// Derivative owner accounts plus the formal hedge accounts, one journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
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
    CashFlowHedgeOci,
    HedgedItemBasisAdjustment,
    HedgedItemBasisGainLoss,
    HedgedItemBasisAmortization,
    HedgedItemBasisDerecognition,
    HedgedForecastReclassification,
}
impl From<DerivativeAccount> for Account {
    fn from(a: DerivativeAccount) -> Self {
        match a {
            DerivativeAccount::Cash => Self::Cash,
            DerivativeAccount::DerivativeAsset => Self::DerivativeAsset,
            DerivativeAccount::DerivativeLiability => Self::DerivativeLiability,
            DerivativeAccount::UnpaidReceivable => Self::UnpaidReceivable,
            DerivativeAccount::UnpaidPayable => Self::UnpaidPayable,
            DerivativeAccount::PostedVmAsset => Self::PostedVmAsset,
            DerivativeAccount::ReceivedVmLiability => Self::ReceivedVmLiability,
            DerivativeAccount::PostedImAsset => Self::PostedImAsset,
            DerivativeAccount::OpeningEquity => Self::OpeningEquity,
            DerivativeAccount::MarkGainLoss => Self::MarkGainLoss,
            DerivativeAccount::ContractualIncome => Self::ContractualIncome,
            DerivativeAccount::CollateralInterestIncome => Self::CollateralInterestIncome,
            DerivativeAccount::CounterpartyLossExpense => Self::CounterpartyLossExpense,
            DerivativeAccount::PostedCollateralReturnLossExpense => {
                Self::PostedCollateralReturnLossExpense
            }
            DerivativeAccount::MemoCustodianAsset => Self::MemoCustodianAsset,
            DerivativeAccount::MemoCustodianObligation => Self::MemoCustodianObligation,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Posting {
    pub account: Account,
    pub position_id: Option<String>,
    pub bucket: Option<String>,
    pub relationship_id: Option<String>,
    pub debit_amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Intent {
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
    pub relationship_id: Option<String>,
    pub measure: CashflowMeasure,
    pub basis: ExecutionBasis,
    pub postings: Vec<Posting>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MemberAssessment {
    pub exposure_id: String,
    pub notional: f64,
    pub cumulative_hedged_change: f64,
    /// 55-23A(a): pro-rata instrument change over the member change; 55-23A(b): the member's
    /// per-unit change over the first member's per-unit change.
    pub ratio: Option<f64>,
    pub similar: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct SimilarRiskAssessment {
    pub method: SimilarRiskMethod,
    pub similar: bool,
    pub members: Vec<MemberAssessment>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Assessment {
    pub event_id: String,
    pub relationship_id: String,
    pub ordinal: i32,
    pub method: AssessmentMethod,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub cumulative_instrument_change: f64,
    pub cumulative_hedged_change: f64,
    pub ratio: Option<f64>,
    pub qualified: bool,
    /// Choose-your-rate: the contractually specified rate this assessment measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similar_risk: Option<SimilarRiskAssessment>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LifecycleRecord {
    pub ordinal: i32,
    pub sequence: Option<u32>,
    pub relationship_id: String,
    pub kind: String,
    pub event_id: Option<String>,
    pub review: Option<Identity>,
    pub reason: Option<String>,
    pub status_before: Status,
    pub status_after: Status,
    pub proportion_before: f64,
    pub proportion_after: f64,
    pub frozen_basis: f64,
    pub frozen_oci: f64,
    pub released_oci: f64,
    pub derecognized_basis: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_ordinal: Option<i32>,
    /// Instrument changes after the effective date moved from OCI back to earnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earnings_catch_up: Option<f64>,
    /// Hedged-period reclassifications after the effective date reversed before freezing OCI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversed_reclassification: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct DailyMember {
    pub exposure_id: String,
    pub index_id: String,
    pub hypothetical_value: f64,
    pub cumulative_hedged_change: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DailyRelationship {
    pub relationship_id: String,
    pub status: Status,
    pub designated_proportion: f64,
    pub instrument_designated_value: f64,
    pub hedged_item_clean_value: Option<f64>,
    pub hypothetical_value: Option<f64>,
    pub cumulative_instrument_change: f64,
    pub cumulative_hedged_change: f64,
    pub basis_adjustment_balance: f64,
    pub frozen_basis_balance: f64,
    pub oci_balance: f64,
    pub frozen_oci_balance: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<DailyMember>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Balance {
    pub account: Account,
    pub debit_balance: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Daily {
    pub ordinal: i32,
    pub relationships: Vec<DailyRelationship>,
    pub financial_trial_balance: Vec<Balance>,
    pub custody_trial_balance: Vec<Balance>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RollForward {
    pub relationship_id: String,
    pub basis_adjustments: f64,
    pub basis_amortized: f64,
    pub basis_derecognized: f64,
    pub basis_closing: f64,
    pub oci_recognized: f64,
    pub oci_reclassified: f64,
    pub oci_closing: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct RelationshipClosing {
    pub relationship_id: String,
    pub predecessor_relationship_id: Option<String>,
    pub designation: Designation,
    pub hedging_instrument_position_id: String,
    pub hedged_item_id: Option<String>,
    pub forecast_exposure_id: Option<String>,
    pub inception_ordinal: i32,
    pub status: Status,
    pub designated_proportion: f64,
    pub basis_adjustment_balance: f64,
    pub frozen_basis_balance: f64,
    pub oci_balance: f64,
    pub frozen_oci_balance: f64,
    pub pending_reclassification_periods: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_exposure_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_index_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written_option_position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_evidence: Option<Identity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hedged_term_end_ordinal: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_instrument_position_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct HedgeAccountingResult {
    pub schema: String,
    pub model_version: String,
    pub rule_version: String,
    pub run: Identity,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub policy: Policy,
    pub derivative: DerivativeAccountingResult,
    pub relationships: Vec<RelationshipClosing>,
    pub lifecycle: Vec<LifecycleRecord>,
    pub assessments: Vec<Assessment>,
    pub daily: Vec<Daily>,
    pub intents: Vec<Intent>,
    pub closing: Vec<RollForward>,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub persisted_journal_verified: bool,
    pub saved_book_adoption: bool,
    pub portfolio_layer_supported: bool,
    pub net_investment_supported: bool,
    pub asu_2025_09_similar_risk_supported: bool,
    pub asu_2025_09_selectable_rate_supported: bool,
    pub asu_2025_09_swap_written_option_supported: bool,
    pub seasoned_openings_supported: bool,
    pub disclosure: String,
}
