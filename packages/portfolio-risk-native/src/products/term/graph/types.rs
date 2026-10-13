use crate::{
    advanced_graph::{BaseAnchor, GraphIdentity, OasTarget, Role, Scenario},
    term_enhancements::{
        cd::{CdElectionRequest, CdElectionResult, CdPolicy, CdState},
        cd_sequence,
        indexed::{ContractPathRequest, ContractPathResult},
    },
};
use portfolio_model_core::{cashflow::DatedCashflowEvent, floating_rate::Direction};
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
#[serde(deny_unknown_fields)]
pub struct CdSequenceKnown {
    pub model_version: String,
    pub currency: String,
    pub direction: Direction,
    pub opening: CdState,
    pub tail_end_ordinal: i32,
    pub policies: Vec<cd_sequence::PolicyBinding>,
    pub authorized_new_contracts: Vec<crate::products::term_deposits::elections::RenewalContract>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalQuote {
    pub position_id: String,
    /// Signed future PV, excluding declared as-of settlements.
    pub signed_cash_price: f64,
    pub source: crate::products::term::contracts::Identity,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalPositionHistories {
    pub position_id: String,
    pub histories: ScenarioHistories,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalBase {
    pub scenario: Scenario,
    pub histories: Vec<OriginalPositionHistories>,
}
#[derive(Clone, Serialize)]
pub struct SequenceGraphStatus {
    pub native_sequence_sources: usize,
    pub original_native_sequence_sources: usize,
    pub locked_original_oas_sources: usize,
    pub oas_calibration_calls: usize,
    pub single_election_fallbacks: usize,
    pub original_quote_reconciled: bool,
    pub joint_optimal_exercise: bool,
    pub accounting_adoption: bool,
    pub empirical_validation: bool,
    pub saved_library_cohort_adoption: bool,
    pub capital_ftp_eligible: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnownProduct {
    IndexedTerm { contract: ContractPathRequest },
    CdElection { contract: CdKnown },
    CdSequence { contract: CdSequenceKnown },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProductHistory {
    IndexedTerm { request: ContractPathRequest },
    CdElection { request: CdElectionRequest },
    CdSequence { request: cd_sequence::Request },
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_quotes: Vec<OriginalQuote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_base: Option<Box<OriginalBase>>,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceProjection {
    IndexedTerm { result: ContractPathResult },
    CdElection { result: CdElectionResult },
    CdSequence { result: cd_sequence::ResultData },
}
impl SourceProjection {
    pub(super) fn events(&self) -> Box<dyn Iterator<Item = &DatedCashflowEvent> + '_> {
        match self {
            Self::CdSequence { result } => Box::new(result.schedule.events.iter()),
            Self::IndexedTerm { result } => Box::new(result.schedule.events.iter()),
            Self::CdElection { result } => Box::new(
                result
                    .settlement_schedule
                    .events
                    .iter()
                    .chain(result.remaining_schedule.events.iter()),
            ),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct NativeSource {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub weight: f64,
    pub role: Role,
    pub product: ProductHistory,
    pub projection: SourceProjection,
}
#[derive(Clone, Default, Serialize)]
pub struct PhysicalAmounts {
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub cd_principal_extinguished: f64,
    pub cd_penalty: f64,
    pub cd_penalty_from_principal: f64,
    pub cd_penalty_from_accrued_interest: f64,
    pub cd_interest_accrued_through_election: f64,
    pub cd_new_contract_principal: f64,
    /// Settlement diagnostics after penalty, not additional cash events.
    pub cd_net_external_cash: f64,
    pub cd_interest_cash_paid: f64,
    pub cd_principal_cash_returned: f64,
}
#[derive(Clone, Serialize)]
pub struct Metrics {
    pub position_id: String,
    pub scenario_id: String,
    pub pv: f64,
    pub supplied_history_dv01: f64,
    pub physical_periods: Vec<PhysicalAmounts>,
    pub physical_tail: PhysicalAmounts,
    pub opening_actions: PhysicalAmounts,
}
#[derive(Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub identity: GraphIdentity,
    pub base_anchor: BaseAnchor,
    pub metrics: Vec<Metrics>,
    pub native_sources: Vec<NativeSource>,
    pub income_policy: String,
    pub pricing_policy: String,
    pub source_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_policy: Option<SequenceGraphStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_native_sources: Option<Vec<NativeSource>>,
}
