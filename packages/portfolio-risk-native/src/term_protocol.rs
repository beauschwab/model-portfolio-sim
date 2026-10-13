//! Shared bounded JSON protocol for the standalone executable and Python adapter.
use crate::mortgage_risk::RiskResult;
use crate::term_deck::{Deck, DeckRequest};
use crate::term_risk::TermRiskRequest;
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CString};
use std::io::{self, Write};
pub const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_OUTPUT: usize = 128 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvancedRepriceRequest {
    base: crate::advanced_graph::Request,
    current: crate::advanced_graph::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardRepriceRequest {
    base: crate::card_graph::Request,
    current: crate::card_graph::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DepositGraphRepriceRequest {
    base: crate::deposit_graph::Request,
    current: crate::deposit_graph::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MortgageGraphRepriceRequest {
    base: crate::mortgage_graph::Request,
    current: crate::mortgage_graph::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FundingGraphRepriceRequest {
    base: crate::funding_graph::Request,
    current: crate::funding_graph::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TermProductGraphRepriceRequest {
    base: crate::term_product_graph::legacy_request::Request,
    current: crate::term_product_graph::legacy_request::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TermSequenceGraphRepriceRequest {
    base: crate::term_product_graph::strict_request::SequenceRequest,
    current: crate::term_product_graph::strict_request::SequenceRequest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvancedCaptureRequest {
    graph: crate::advanced_graph::Request,
    owners: Vec<crate::advanced_graph::CaptureOwner>,
    policy: crate::advanced_graph::CapturePolicy,
}
#[derive(Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum Request {
    #[serde(rename = "commercial-holding-continuation-1")]
    CommercialContinuation {
        threads: usize,
        execution_timeout_ms: u64,
        request: Box<crate::commercial_continuation::Request>,
    },
    #[serde(rename = "commercial-generated-exercise-1")]
    CommercialGeneratedExercise {
        threads: usize,
        execution_timeout_ms: u64,
        request: Box<crate::commercial_stochastic_exercise::Request>,
    },
    #[serde(rename = "commercial-product-graph-1")]
    CommercialGraph {
        threads: usize,
        #[serde(default)]
        execution_timeout_ms: Option<u64>,
        request: Box<crate::commercial_graph::Request>,
    },
    #[serde(rename = "commercial-units-1")]
    CommercialUnits {
        threads: usize,
        #[serde(default)]
        execution_timeout_ms: Option<u64>,
        request: Box<crate::commercial_units::Request>,
    },
    #[serde(rename = "product-units-1")]
    ProductUnits {
        threads: usize,
        request: Box<crate::product_units::ProductUnitsRequest>,
    },
    #[serde(rename = "term-product-graph-2")]
    TermSequenceGraph {
        threads: usize,
        request: Box<crate::term_product_graph::strict_request::SequenceRequest>,
    },
    #[serde(rename = "term-product-reprice-2")]
    TermSequenceGraphReprice {
        threads: usize,
        request: Box<TermSequenceGraphRepriceRequest>,
    },
    #[serde(rename = "term-product-graph-1")]
    TermProductGraph {
        threads: usize,
        request: Box<crate::term_product_graph::legacy_request::Request>,
    },
    #[serde(rename = "term-product-reprice-1")]
    TermProductGraphReprice {
        threads: usize,
        request: Box<TermProductGraphRepriceRequest>,
    },
    #[serde(rename = "graph-cohort-audit-1")]
    GraphCohort {
        threads: usize,
        request: Box<crate::graph_cohort::Request>,
    },
    #[serde(rename = "mortgage-product-graph-1")]
    MortgageGraph {
        threads: usize,
        request: Box<crate::mortgage_graph::Request>,
    },
    #[serde(rename = "mortgage-product-reprice-1")]
    MortgageGraphReprice {
        threads: usize,
        request: Box<MortgageGraphRepriceRequest>,
    },
    #[serde(rename = "funding-product-graph-1")]
    FundingGraph {
        threads: usize,
        request: Box<crate::funding_graph::Request>,
    },
    #[serde(rename = "funding-product-reprice-1")]
    FundingGraphReprice {
        threads: usize,
        request: Box<FundingGraphRepriceRequest>,
    },
    #[serde(rename = "deposit-product-graph-1")]
    DepositGraph {
        threads: usize,
        request: Box<crate::deposit_graph::Request>,
    },
    #[serde(rename = "deposit-product-reprice-1")]
    DepositGraphReprice {
        threads: usize,
        request: Box<DepositGraphRepriceRequest>,
    },
    #[serde(rename = "card-product-graph-1")]
    CardGraph {
        threads: usize,
        request: Box<crate::card_graph::Request>,
    },
    #[serde(rename = "card-product-reprice-1")]
    CardReprice {
        threads: usize,
        request: Box<CardRepriceRequest>,
    },
    #[serde(rename = "advanced-product-graph-1")]
    AdvancedGraph {
        threads: usize,
        request: Box<crate::advanced_graph::Request>,
    },
    #[serde(rename = "advanced-product-reprice-1")]
    AdvancedReprice {
        threads: usize,
        request: Box<AdvancedRepriceRequest>,
    },
    #[serde(rename = "advanced-scheduled-capture-1")]
    AdvancedCapture {
        threads: usize,
        request: Box<AdvancedCaptureRequest>,
    },
    #[serde(rename = "credit-covariate-1")]
    CreditCovariate {
        threads: usize,
        request: Box<portfolio_model_core::credit_covariate::CovariateFitRequest>,
    },
    #[serde(rename = "credit-covariate-projection-1")]
    CreditCovariateProjection {
        threads: usize,
        request: Box<portfolio_model_core::credit_covariate::CovariateProjectionRequest>,
    },
    #[serde(rename = "derivative-accounting-1")]
    DerivativeAccounting {
        threads: usize,
        request: Box<crate::derivative_accounting::DerivativeAccountingRequest>,
    },
    #[serde(rename = "credit-path-accounting-1")]
    CreditPathAccounting {
        threads: usize,
        request: Box<portfolio_model_core::credit_path_accounting::PathAccountingRequest>,
    },
    #[serde(rename = "aged-units-1")]
    AgedUnits {
        threads: usize,
        request: Box<crate::aged_units::AgedUnitsRequest>,
    },
    #[serde(rename = "credit-path-1")]
    CreditPath {
        threads: usize,
        request: Box<portfolio_model_core::credit_path::CreditPathRequest>,
    },
    #[serde(rename = "joint-state-1")]
    JointState {
        threads: usize,
        request: Box<portfolio_model_core::joint_state::JointStateRequest>,
    },
    #[serde(rename = "joint-market-credit-1")]
    JointMarketCredit {
        threads: usize,
        request: Box<crate::joint_market_credit::Request>,
    },
    #[serde(rename = "credit-snapshot-calibration-1")]
    CreditSnapshot {
        threads: usize,
        request: Box<portfolio_model_core::credit_snapshot_calibration::SnapshotCalibrationRequest>,
    },
    #[serde(rename = "derivative-contract-1")]
    DerivativeContract {
        threads: usize,
        request: Box<crate::derivative_contract::DerivativeRequest>,
    },
    #[serde(rename = "market-indexed-adapter-1")]
    MarketIndexedAdapter {
        threads: usize,
        request: Box<crate::market_indexed_adapter::Request>,
    },
    #[serde(rename = "model-market-1")]
    ModelMarket {
        threads: usize,
        request: Box<crate::model_market::ModelMarketRequest>,
    },
    #[serde(rename = "market-driver-calibration-1")]
    MarketDriverCalibration {
        threads: usize,
        request: Box<crate::market_driver_calibration::Request>,
    },
    #[serde(rename = "market-quote-bootstrap-1")]
    MarketQuoteBootstrap {
        threads: usize,
        request: Box<crate::market_bootstrap::Request>,
    },
    #[serde(rename = "nonintegral-fee-accounting-1")]
    NonintegralFees {
        threads: usize,
        execution_timeout_ms: u64,
        request: Box<portfolio_model_core::nonintegral_fee::Request>,
    },
    #[serde(rename = "dated-income-1")]
    DatedIncome {
        threads: usize,
        request: Box<portfolio_model_core::dated_income::DatedIncomeRequest>,
    },
    #[serde(rename = "deposit-state-1")]
    DepositState {
        threads: usize,
        request: Box<portfolio_model_core::deposit_state::DepositStateRequest>,
    },
    #[serde(rename = "mortgage-state-1")]
    MortgageState {
        threads: usize,
        request: Box<portfolio_model_core::mortgage_state::MortgageRequest>,
    },
    #[serde(rename = "mortgage-credit-accounting-1")]
    MortgageCreditAccounting {
        threads: usize,
        request: Box<portfolio_model_core::mortgage_sequential_carry::Request>,
    },
    #[serde(rename = "funding-contract-1")]
    FundingContract {
        threads: usize,
        request: Box<crate::funding_contract::FundingRequest>,
    },
    #[serde(rename = "consumer-installment-1")]
    ConsumerInstallment {
        threads: usize,
        request: Box<portfolio_model_core::consumer_installment::InstallmentRequest>,
    },
    #[serde(rename = "consumer-card-1")]
    ConsumerCard {
        threads: usize,
        request: Box<portfolio_model_core::consumer_card::CardRequest>,
    },
    #[serde(rename = "consumer-credit-accounting-1")]
    ConsumerCreditAccounting {
        threads: usize,
        request: Box<portfolio_model_core::consumer_credit_accounting::Request>,
    },
    #[serde(rename = "card-credit-accounting-1")]
    CardCreditAccounting {
        threads: usize,
        request: Box<portfolio_model_core::card_credit_accounting::Request>,
    },
    #[serde(rename = "card-credit-accounting-2")]
    CardCreditAccountingV2 {
        threads: usize,
        request: Box<portfolio_model_core::card_credit_accounting_v2::Request>,
    },
    #[serde(rename = "indexed-contract-1")]
    IndexedContract {
        threads: usize,
        request: Box<crate::term_enhancements::indexed::ContractPathRequest>,
    },
    #[serde(rename = "conditional-exercise-1")]
    ConditionalExercise {
        threads: usize,
        request: Box<crate::term_enhancements::exercise::ExerciseRequest>,
    },
    #[serde(rename = "cd-election-1")]
    CdElection {
        threads: usize,
        request: Box<crate::term_enhancements::cd::CdElectionRequest>,
    },
    #[serde(rename = "floating-rate-1")]
    FloatingRate {
        threads: usize,
        request: Box<portfolio_model_core::floating_rate::FloatingRateRequest>,
    },
    #[serde(rename = "dated-term-1")]
    DatedTerm {
        threads: usize,
        request: Box<crate::dated_term::DatedTermRequest>,
    },
    #[serde(rename = "observed-credit-calibration-1")]
    ObservedCreditCalibration {
        threads: usize,
        request: Box<portfolio_model_core::credit_calibration::CreditCalibrationRequest>,
    },
    #[serde(rename = "observed-deposit-calibration-1")]
    ObservedDepositCalibration {
        threads: usize,
        request: Box<portfolio_model_core::deposit_calibration::Request>,
    },
    #[serde(rename = "credit-model-1")]
    CreditModel {
        threads: usize,
        request: Box<portfolio_model_core::credit::CreditRequest>,
    },
    #[serde(rename = "dated-cashflow-1")]
    DatedCashflow {
        threads: usize,
        request: Box<crate::model_lifecycle::CashflowRequest>,
    },
    #[serde(rename = "treasury-bridge-1")]
    TreasuryBridge {
        threads: usize,
        request: Box<crate::treasury_bridge::Request>,
    },
    #[serde(rename = "treasury-1")]
    Treasury {
        threads: usize,
        request: Box<crate::treasury::Request>,
    },
    #[serde(rename = "cohort-build-1")]
    Cohorts {
        threads: usize,
        request: Box<crate::cohorts::Request>,
    },
    #[serde(rename = "financial-controller-1")]
    Controller {
        threads: usize,
        request: Box<crate::controllers::Request>,
    },
    #[serde(rename = "whatif-overrides-1")]
    WhatIf {
        threads: usize,
        request: crate::whatif::Request,
    },
    #[serde(rename = "graph-1")]
    Graph {
        threads: usize,
        request: Box<crate::incremental::Request>,
    },
    #[serde(rename = "unit-coefficients-1")]
    Coefficients {
        threads: usize,
        request: Box<crate::unit_lifecycle::CoefficientRequest>,
    },
    #[serde(rename = "ledger-map-1")]
    LedgerMap {
        threads: usize,
        request: Box<crate::ledger_mapping::MappingRequest>,
    },
    #[serde(rename = "programs-1")]
    Programs {
        threads: usize,
        request: Box<crate::program_lifecycle::ProgramRequest>,
    },
    #[serde(rename = "unit-library-1")]
    UnitLibrary {
        threads: usize,
        request: Box<crate::unit_lifecycle::UnitRequest>,
    },
    #[serde(rename = "balance-risk-1")]
    BalanceRisk {
        threads: usize,
        request: Box<crate::balance_risk::BalanceRiskRequest>,
    },
    #[serde(rename = "kpi-1")]
    Kpi {
        threads: usize,
        request: Box<crate::kpi_lifecycle::KpiRequest>,
    },
    #[serde(rename = "accounting-1")]
    Accounting {
        threads: usize,
        request: Box<crate::accounting_lifecycle::AccountingRequest>,
    },
    #[serde(rename = "hedge-risk-1")]
    HedgeRisk {
        threads: usize,
        request: Box<crate::hedge_lifecycle::HedgeRequest>,
    },
    #[serde(rename = "hedge-deck-1")]
    HedgeDeck {
        threads: usize,
        request: crate::hedge_lifecycle::HedgeDeckInput,
    },
    #[serde(rename = "deposit-risk-1")]
    DepositRisk {
        threads: usize,
        request: Box<crate::deposit_lifecycle::DepositRequest>,
    },
    #[serde(rename = "deposit-stress-1")]
    DepositStress {
        threads: usize,
        request: Box<crate::deposit_lifecycle::DepositRequest>,
    },
    #[serde(rename = "deposit-deck-1")]
    DepositDeck {
        threads: usize,
        request: DepositDeckInput,
    },
    #[serde(rename = "deposit-horizon-1")]
    DepositHorizon {
        threads: usize,
        request: Box<crate::deposit_lifecycle::DepositRequest>,
    },
    #[serde(rename = "term-risk-1")]
    Risk {
        threads: usize,
        request: Box<TermRiskRequest>,
    },
    #[serde(rename = "term-deck-1")]
    Deck {
        threads: usize,
        request: DeckRequest,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DepositDeckInput {
    book: Vec<crate::deposit_lifecycle::Cohort>,
    assumptions: crate::deposit_lifecycle::Assumptions,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Coefficients(Vec<crate::unit_lifecycle::Coefficients>),
    Programs(Box<crate::program_lifecycle::ProgramResult>),
    UnitLibrary(Box<crate::unit_lifecycle::UnitLibrary>),
    BalanceRisk(Box<crate::balance_risk::BalanceRiskResult>),
    Kpi(serde_json::Value),
    Accounting(Box<crate::accounting_lifecycle::AccountingResult>),
    HedgeRisk(Box<crate::hedge_lifecycle::HedgeRisk>),
    HedgeDeck(Box<crate::hedge_lifecycle::HedgeDeck>),
    DepositRisk(Box<crate::deposit_lifecycle::DepositRisk>),
    DepositStress(Box<crate::deposit_lifecycle::DepositStress>),
    DepositDeck(Box<crate::deposit_lifecycle::DepositDeck>),
    DepositHorizon(Box<crate::deposit_lifecycle::DepositHorizon>),
    Risk(RiskResult),
    Deck(Box<Deck>),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Success { ok: bool, result: Output },
    Error { ok: bool, error: String },
}
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT - self.0.len() {
            return Err(io::Error::other("term response exceeds 128 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn execute(input: &[u8]) -> Result<Output, String> {
    let started = std::time::Instant::now();
    if input.len() > MAX_INPUT {
        return Err("term request exceeds 64 MiB".into());
    }
    let request: Request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let selected_timeout = match &request {
        Request::CommercialGeneratedExercise {
            execution_timeout_ms,
            ..
        }
        | Request::CommercialContinuation {
            execution_timeout_ms,
            ..
        }
        | Request::NonintegralFees {
            execution_timeout_ms,
            ..
        } => Some(*execution_timeout_ms),
        Request::CommercialGraph {
            execution_timeout_ms,
            ..
        }
        | Request::CommercialUnits {
            execution_timeout_ms,
            ..
        } => *execution_timeout_ms,
        _ => None,
    };
    let generated_control = if let Some(execution_timeout_ms) = selected_timeout {
        let control = if let Some(parent) = portfolio_compute_control::active() {
            parent.child_started(execution_timeout_ms, started)?
        } else {
            portfolio_compute_control::Control::from_started(execution_timeout_ms, started)?
        };
        portfolio_compute_control::scope(Some(control.clone()), || {
            let limit = if matches!(&request, Request::CommercialGeneratedExercise { .. }) {
                1 << 20
            } else if matches!(&request, Request::NonintegralFees { .. }) {
                portfolio_model_core::nonintegral_fee::MAX_INPUT
            } else {
                32 << 20
            };
            portfolio_model_core::strict_json::parse_checked(
                input,
                limit + 4096,
                portfolio_compute_control::checkpoint,
            )?;
            Ok(())
        })?;
        Some(control)
    } else {
        None
    };
    let threads = match &request {
        Request::CommercialGeneratedExercise { threads, .. }
        | Request::NonintegralFees { threads, .. }
        | Request::CommercialContinuation { threads, .. }
        | Request::CommercialGraph { threads, .. }
        | Request::CommercialUnits { threads, .. }
        | Request::ProductUnits { threads, .. }
        | Request::AgedUnits { threads, .. }
        | Request::AdvancedGraph { threads, .. }
        | Request::CardGraph { threads, .. }
        | Request::DepositGraph { threads, .. }
        | Request::DepositGraphReprice { threads, .. }
        | Request::MortgageGraph { threads, .. }
        | Request::MortgageGraphReprice { threads, .. }
        | Request::FundingGraph { threads, .. }
        | Request::FundingGraphReprice { threads, .. }
        | Request::GraphCohort { threads, .. }
        | Request::TermSequenceGraph { threads, .. }
        | Request::TermSequenceGraphReprice { threads, .. }
        | Request::TermProductGraph { threads, .. }
        | Request::TermProductGraphReprice { threads, .. }
        | Request::CardReprice { threads, .. }
        | Request::AdvancedReprice { threads, .. }
        | Request::AdvancedCapture { threads, .. }
        | Request::CreditCovariate { threads, .. }
        | Request::CreditCovariateProjection { threads, .. }
        | Request::DerivativeAccounting { threads, .. }
        | Request::CreditPathAccounting { threads, .. }
        | Request::CreditPath { threads, .. }
        | Request::JointState { threads, .. }
        | Request::JointMarketCredit { threads, .. }
        | Request::CreditSnapshot { threads, .. }
        | Request::DerivativeContract { threads, .. }
        | Request::ModelMarket { threads, .. }
        | Request::MarketDriverCalibration { threads, .. }
        | Request::MarketQuoteBootstrap { threads, .. }
        | Request::MarketIndexedAdapter { threads, .. }
        | Request::DatedIncome { threads, .. }
        | Request::DepositState { threads, .. }
        | Request::MortgageState { threads, .. }
        | Request::MortgageCreditAccounting { threads, .. }
        | Request::FundingContract { threads, .. }
        | Request::ConsumerInstallment { threads, .. }
        | Request::ConsumerCard { threads, .. }
        | Request::ConsumerCreditAccounting { threads, .. }
        | Request::CardCreditAccounting { threads, .. }
        | Request::CardCreditAccountingV2 { threads, .. }
        | Request::IndexedContract { threads, .. }
        | Request::ConditionalExercise { threads, .. }
        | Request::CdElection { threads, .. }
        | Request::FloatingRate { threads, .. }
        | Request::DatedTerm { threads, .. }
        | Request::ObservedCreditCalibration { threads, .. }
        | Request::ObservedDepositCalibration { threads, .. }
        | Request::CreditModel { threads, .. }
        | Request::DatedCashflow { threads, .. }
        | Request::TreasuryBridge { threads, .. }
        | Request::Treasury { threads, .. }
        | Request::Cohorts { threads, .. }
        | Request::Controller { threads, .. }
        | Request::WhatIf { threads, .. }
        | Request::Graph { threads, .. }
        | Request::Coefficients { threads, .. }
        | Request::LedgerMap { threads, .. }
        | Request::Programs { threads, .. }
        | Request::UnitLibrary { threads, .. }
        | Request::BalanceRisk { threads, .. }
        | Request::Kpi { threads, .. }
        | Request::Accounting { threads, .. }
        | Request::HedgeRisk { threads, .. }
        | Request::HedgeDeck { threads, .. }
        | Request::Risk { threads, .. }
        | Request::Deck { threads, .. }
        | Request::DepositRisk { threads, .. }
        | Request::DepositStress { threads, .. }
        | Request::DepositDeck { threads, .. }
        | Request::DepositHorizon { threads, .. } => *threads,
    };
    if !(1..=256).contains(&threads) {
        return Err("invalid native thread count".into());
    }
    // Propagate only this new analytical branch's caller control across Rayon.
    // Legacy branches retain their existing execution and None-path behavior.
    let adapter_control = if generated_control.is_some() {
        generated_control
    } else if matches!(
        &request,
        Request::TermSequenceGraph { .. }
            | Request::TermSequenceGraphReprice { .. }
            | Request::CommercialGraph { .. }
            | Request::CommercialUnits { .. }
            | Request::CommercialGeneratedExercise { .. }
            | Request::CommercialContinuation { .. }
    ) || matches!(&request, Request::MarketIndexedAdapter { .. })
        || matches!(&request, Request::JointState { .. })
        || matches!(&request, Request::JointMarketCredit { .. })
        || matches!(&request, Request::MarketDriverCalibration { .. })
        || matches!(&request, Request::MarketQuoteBootstrap { .. })
        || matches!(&request, Request::ObservedDepositCalibration { .. })
        || matches!(&request, Request::ConditionalExercise { .. })
        || matches!(&request, Request::Graph { request, .. } if request.is_market_indexed())
    {
        portfolio_compute_control::active()
    } else {
        None
    };
    let pool = crate::pool(threads).map_err(|_| "native thread pool unavailable")?;
    pool.install(|| match request {
        Request::NonintegralFees { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let result = portfolio_model_core::nonintegral_fee::evaluate_with_check(
                    &request,
                    portfolio_compute_control::checkpoint,
                )?;
                portfolio_model_core::strict_json::serialized_size(
                    &result,
                    portfolio_model_core::nonintegral_fee::MAX_OUTPUT,
                )?;
                Ok(Output::Kpi(
                    serde_json::to_value(result).map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::CommercialContinuation { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let result = crate::commercial_continuation::evaluate(&request)?;
                portfolio_model_core::strict_json::serialized_size(&result, 32 << 20)?;
                Ok(Output::Kpi(
                    serde_json::to_value(result).map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::CommercialGeneratedExercise { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let result = crate::commercial_stochastic_exercise::analyze(&request)?;
                portfolio_compute_control::checkpoint()?;
                Ok(Output::Kpi(
                    serde_json::to_value(result).map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::CommercialGraph { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                if request.positions.iter().any(|p| {
                    p.generated_policy
                        .as_ref()
                        .is_some_and(|g| g.oas_calibration.is_some())
                }) && portfolio_compute_control::active().is_none()
                {
                    return Err(
                        "generated OAS calibration requires original execution deadline".into(),
                    );
                }
                let result = crate::commercial_graph::analyze(&request)?;
                portfolio_compute_control::checkpoint()?;
                Ok(Output::Kpi(
                    serde_json::to_value(result).map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::CommercialUnits { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                if request
                    .units
                    .iter()
                    .flat_map(|u| &u.graph.positions)
                    .any(|p| {
                        p.generated_policy
                            .as_ref()
                            .is_some_and(|g| g.oas_calibration.is_some())
                    })
                    && portfolio_compute_control::active().is_none()
                {
                    return Err(
                        "generated OAS calibration requires original execution deadline".into(),
                    );
                }
                let result = crate::commercial_units::prepare(&request)?;
                portfolio_compute_control::checkpoint()?;
                Ok(Output::Kpi(
                    serde_json::to_value(result.source()).map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::ProductUnits { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::product_units::prepare(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::TermSequenceGraph { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                if request.0.schema != crate::term_product_graph::SEQUENCE_SCHEMA {
                    return Err("explicit term-product-graph-2 selection required".into());
                }
                Ok(Output::Kpi(
                    serde_json::to_value(crate::term_product_graph::analyze(&request.0)?)
                        .map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::TermSequenceGraphReprice { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                if request.base.0.schema != crate::term_product_graph::SEQUENCE_SCHEMA
                    || request.current.0.schema != crate::term_product_graph::SEQUENCE_SCHEMA
                {
                    return Err(
                        "explicit term-product-reprice-2 base/current selection required".into(),
                    );
                }
                let graph = crate::term_product_graph::TermGraph::new(&request.base.0)?;
                Ok(Output::Kpi(
                    serde_json::to_value(graph.reprice(&request.current.0)?)
                        .map_err(|e| e.to_string())?,
                ))
            })
        }
        Request::TermProductGraph { request, .. } => {
            if request.schema == crate::term_product_graph::SEQUENCE_SCHEMA {
                return Err("explicit term-product-graph-2 selection required".into());
            }
            Ok(Output::Kpi(
                serde_json::to_value(crate::term_product_graph::analyze(
                    &request.into_current()?,
                )?)
                .map_err(|e| e.to_string())?,
            ))
        }
        Request::TermProductGraphReprice { request, .. } => {
            if request.base.schema == crate::term_product_graph::SEQUENCE_SCHEMA
                || request.current.schema == crate::term_product_graph::SEQUENCE_SCHEMA
            {
                return Err("explicit term-product-reprice-2 selection required".into());
            }
            let base = request.base.into_current()?;
            let current = request.current.into_current()?;
            let graph = crate::term_product_graph::TermGraph::new(&base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&current)?).map_err(|e| e.to_string())?,
            ))
        }
        Request::GraphCohort { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::graph_cohort::evaluate(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::MortgageGraph { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::mortgage_graph::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::MortgageGraphReprice { request, .. } => {
            let graph = crate::mortgage_graph::MortgageGraph::new(&request.base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&request.current)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::FundingGraph { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::funding_graph::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::FundingGraphReprice { request, .. } => {
            let graph = crate::funding_graph::FundingGraph::new(&request.base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&request.current)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::DepositGraph { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::deposit_graph::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::DepositGraphReprice { request, .. } => {
            let graph = crate::deposit_graph::DepositGraph::new(&request.base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&request.current)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::CardGraph { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::card_graph::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::CardReprice { request, .. } => {
            let graph = crate::card_graph::CardGraph::new(&request.base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&request.current)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::AdvancedGraph { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::advanced_graph::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::AdvancedReprice { request, .. } => {
            let graph = crate::advanced_graph::AdvancedGraph::new(&request.base)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.reprice(&request.current)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::AdvancedCapture { request, .. } => {
            let graph = crate::advanced_graph::AdvancedGraph::new(&request.graph)?;
            Ok(Output::Kpi(
                serde_json::to_value(graph.capture_scheduled(&request.owners, &request.policy)?)
                    .map_err(|e| e.to_string())?,
            ))
        }
        Request::CreditCovariate { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(portfolio_model_core::credit_covariate::fit(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::CreditCovariateProjection { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(portfolio_model_core::credit_covariate::project(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::DerivativeAccounting { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::derivative_accounting::evaluate(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::CreditPathAccounting { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(portfolio_model_core::credit_path_accounting::evaluate(
                &request,
            )?)
            .map_err(|e| e.to_string())?,
        )),
        Request::AgedUnits { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::aged_units::prepare(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::CreditPath { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(portfolio_model_core::credit_path::evaluate(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::JointState { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let mut stopped = None;
                let result =
                    portfolio_model_core::joint_state::evaluate_with_check(&request, || {
                        match portfolio_compute_control::checkpoint() {
                            Ok(()) => Ok(()),
                            Err(reason) => {
                                stopped = Some(reason);
                                Err("joint-state computation cancelled".into())
                            }
                        }
                    });
                if let Some(reason) = stopped {
                    return Err(reason);
                }
                serde_json::to_value(result?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::JointMarketCredit { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let mut stopped = None;
                let result = crate::joint_market_credit::evaluate_with_check(&request, || {
                    match portfolio_compute_control::checkpoint() {
                        Ok(()) => Ok(()),
                        Err(reason) => {
                            stopped = Some(reason);
                            Err("joint market-credit computation cancelled".into())
                        }
                    }
                });
                if let Some(reason) = stopped {
                    return Err(reason);
                }
                serde_json::to_value(result?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::CreditSnapshot { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(portfolio_model_core::credit_snapshot_calibration::fit(
                &request,
            )?)
            .map_err(|e| e.to_string())?,
        )),
        Request::DerivativeContract { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::derivative_contract::evaluate(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::MarketIndexedAdapter { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let mut stopped = None;
                let result = crate::market_indexed_adapter::evaluate_with_check(&request, || {
                    match portfolio_compute_control::checkpoint() {
                        Ok(()) => false,
                        Err(reason) => {
                            stopped = Some(reason);
                            true
                        }
                    }
                });
                if let Some(reason) = stopped {
                    return Err(reason);
                }
                serde_json::to_value(result?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::ModelMarket { request, .. } => Ok(Output::Kpi(
            serde_json::to_value(crate::model_market::analyze(&request)?)
                .map_err(|e| e.to_string())?,
        )),
        Request::MarketDriverCalibration { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                serde_json::to_value(crate::market_driver_calibration::evaluate(&request)?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::MarketQuoteBootstrap { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                serde_json::to_value(crate::market_bootstrap::evaluate(&request)?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::DatedIncome { request, .. } => {
            let result = portfolio_model_core::dated_income::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::DepositState { request, .. } => {
            let result = portfolio_model_core::deposit_state::simulate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::MortgageState { request, .. } => {
            let result = portfolio_model_core::mortgage_state::project(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::MortgageCreditAccounting { request, .. } => {
            let result = portfolio_model_core::mortgage_sequential_carry::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::FundingContract { request, .. } => {
            let result = crate::funding_contract::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::ConsumerInstallment { request, .. } => {
            let result = portfolio_model_core::consumer_installment::project(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::ConsumerCard { request, .. } => {
            let result = portfolio_model_core::consumer_card::simulate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::ConsumerCreditAccounting { request, .. } => {
            let result = portfolio_model_core::consumer_credit_accounting::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::CardCreditAccounting { request, .. } => {
            let result = portfolio_model_core::card_credit_accounting::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::CardCreditAccountingV2 { request, .. } => {
            let result = portfolio_model_core::card_credit_accounting_v2::evaluate(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::IndexedContract { request, .. } => {
            let result = crate::term_enhancements::indexed::build_path(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::ConditionalExercise { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let result = crate::term_enhancements::exercise::value_exercise(&request)?;
                let output = serde_json::to_value(result)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())?;
                portfolio_compute_control::checkpoint()?;
                Ok(output)
            })
        }
        Request::CdElection { request, .. } => {
            let result = crate::term_enhancements::cd::step_cd(&request)?;
            serde_json::to_value(result)
                .map(Output::Kpi)
                .map_err(|e| e.to_string())
        }
        Request::FloatingRate { request, .. } => {
            crate::model_lifecycle::floating_rate(&request).map(Output::Kpi)
        }
        Request::DatedTerm { request, .. } => {
            crate::model_lifecycle::dated_term(&request).map(Output::Kpi)
        }
        Request::ObservedCreditCalibration { request, .. } => {
            crate::model_lifecycle::fit_observed_credit(&request).map(Output::Kpi)
        }
        Request::ObservedDepositCalibration { request, .. } => {
            portfolio_compute_control::scope(adapter_control, || {
                let mut stopped = None;
                let result =
                    portfolio_model_core::deposit_calibration::fit_with_check(&request, || {
                        match portfolio_compute_control::checkpoint() {
                            Ok(()) => Ok(()),
                            Err(reason) => {
                                stopped = Some(reason);
                                Err("deposit calibration cancelled".into())
                            }
                        }
                    });
                if let Some(reason) = stopped {
                    return Err(reason);
                }
                serde_json::to_value(result?)
                    .map(Output::Kpi)
                    .map_err(|e| e.to_string())
            })
        }
        Request::CreditModel { request, .. } => {
            crate::model_lifecycle::credit(&request).map(Output::Kpi)
        }
        Request::DatedCashflow { request, .. } => {
            crate::model_lifecycle::cashflows(&request).map(Output::Kpi)
        }
        Request::TreasuryBridge { request, .. } => request.run().map(Output::Kpi),
        Request::Treasury { request, .. } => request.run().map(Output::Kpi),
        Request::Cohorts { request, .. } => request.run().map(Output::Kpi),
        Request::Controller { request, .. } => request.run().map(Output::Kpi),
        Request::WhatIf { request, .. } => request.run().map(Output::Kpi),
        Request::Graph { request, .. } if request.is_market_indexed() => {
            request.run_with_control(adapter_control).map(Output::Kpi)
        }
        Request::Graph { request, .. } => request.run().map(Output::Kpi),
        Request::Coefficients { request, .. } => request.run().map(Output::Coefficients),
        Request::LedgerMap { request, .. } => request.run().map(Output::Kpi),
        Request::Programs { request, .. } => request.run().map(|v| Output::Programs(Box::new(v))),
        Request::UnitLibrary { request, .. } => {
            request.run().map(|v| Output::UnitLibrary(Box::new(v)))
        }
        Request::BalanceRisk { request, .. } => {
            request.run().map(|v| Output::BalanceRisk(Box::new(v)))
        }
        Request::Kpi { request, .. } => request.run().map(Output::Kpi),
        Request::Accounting { request, .. } => {
            request.run().map(|v| Output::Accounting(Box::new(v)))
        }
        Request::HedgeRisk { request, .. } => request.run().map(|v| Output::HedgeRisk(Box::new(v))),
        Request::HedgeDeck { request, .. } => {
            crate::hedge_lifecycle::HedgeDeck::new(&request.swaps, request.asof, request.months)
                .map(|v| Output::HedgeDeck(Box::new(v)))
        }
        Request::DepositRisk { request, .. } => {
            request.risk().map(|v| Output::DepositRisk(Box::new(v)))
        }
        Request::DepositStress { request, .. } => {
            request.stress().map(|v| Output::DepositStress(Box::new(v)))
        }
        Request::DepositDeck { request, .. } => {
            crate::deposit_lifecycle::DepositDeck::new(&request.book, &request.assumptions)
                .map(|v| Output::DepositDeck(Box::new(v)))
        }
        Request::DepositHorizon { request, .. } => request
            .horizon_sensitivity()
            .map(|v| Output::DepositHorizon(Box::new(v))),
        Request::Risk { request, .. } => request.run().map(Output::Risk),
        Request::Deck { request, .. } => request.build().map(|deck| Output::Deck(Box::new(deck))),
    })
}
pub(crate) fn respond_controlled_graph(
    input: &[u8],
    control: portfolio_compute_control::Control,
    created: &mut Option<u64>,
) -> Result<Vec<u8>, String> {
    portfolio_compute_control::scope(Some(control), || {
        if input.len() > MAX_INPUT {
            return Err("term request exceeds 64 MiB".into());
        }
        let parsed: Request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        let Request::Graph { threads, request } = parsed else {
            return Err("GRAPH_CONTROL_GRAPH_SCHEMA_REQUIRED".into());
        };
        if !(1..=256).contains(&threads) {
            return Err("invalid native thread count".into());
        }
        request.admit_runtime()?;
        let creation = request.is_creation();
        let active = portfolio_compute_control::active();
        let value = crate::pool(threads)
            .map_err(|_| "native thread pool unavailable")?
            .install(|| {
                portfolio_compute_control::scope(active, || {
                    let value = request.run()?;
                    if creation {
                        *created = value.get("handle").and_then(serde_json::Value::as_u64);
                    }
                    Ok(value)
                })
            })?;
        struct ControlledOutput(BoundedOutput);
        impl std::io::Write for ControlledOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
                self.0.write(bytes)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                portfolio_compute_control::checkpoint().map_err(std::io::Error::other)
            }
        }
        let mut output = ControlledOutput(BoundedOutput(Vec::new()));
        serde_json::to_writer(
            &mut output,
            &Response::Success {
                ok: true,
                result: Output::Kpi(value),
            },
        )
        .map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()?;
        Ok(output.0 .0)
    })
}
pub fn respond(input: &[u8]) -> (Vec<u8>, bool) {
    respond_started(input, std::time::Instant::now())
}
pub fn respond_started(input: &[u8], started: std::time::Instant) -> (Vec<u8>, bool) {
    if let Some(selected) = crate::commercial_stochastic_boundary::selected(input, started, || {
        let result = std::panic::catch_unwind(|| execute(input))
            .unwrap_or_else(|_| Err("native generated request failed validation".into()))?;
        let mut output = crate::commercial_stochastic_boundary::Output(Vec::new());
        serde_json::to_writer(&mut output, &Response::Success { ok: true, result })
            .map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()?;
        Ok(output.0)
    }) {
        return match selected {
            Ok(output) => (output, true),
            Err(error) => (
                serde_json::to_vec(&Response::Error { ok: false, error }).unwrap(),
                false,
            ),
        };
    }
    let result = std::panic::catch_unwind(|| execute(input))
        .unwrap_or_else(|_| Err("native term request failed validation".into()));
    let success = result.is_ok();
    let response = match result {
        Ok(result) => Response::Success { ok: true, result },
        Err(error) => Response::Error { ok: false, error },
    };
    let mut output = BoundedOutput(Vec::new());
    if let Err(error) = serde_json::to_writer(&mut output, &response) {
        return (
            serde_json::to_vec(&Response::Error {
                ok: false,
                error: error.to_string(),
            })
            .unwrap(),
            false,
        );
    }
    (output.0, success)
}
/// One batched raw-contract request. Returned allocation must be freed exactly once.
///
/// # Safety
/// input must point to length live bytes for the duration of this synchronous call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_request(input: *const u8, length: usize) -> *mut c_char {
    let bytes = if input.is_null() || length > MAX_INPUT {
        serde_json::to_vec(&Response::Error {
            ok: false,
            error: "null or oversized term input".into(),
        })
        .unwrap()
    } else {
        // SAFETY: caller owns the admitted live input range for this call.
        respond(unsafe { std::slice::from_raw_parts(input, length) }).0
    };
    CString::new(bytes).unwrap().into_raw()
}
/// # Safety
/// pointer must be an unfreed allocation returned by portfolio_term_request.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        // SAFETY: the caller returns this library's owned CString once.
        drop(unsafe { CString::from_raw(pointer) });
    }
}

/// Raw JSON input with contiguous numeric outputs, avoiding bulk result JSON.
/// Returns a small JSON status allocation, freed with portfolio_term_free.
///
/// # Safety
/// input must own length live bytes. outputs must point to four live descriptors
/// with exclusive, aligned f64 storage of the stated lengths. Output ranges must
/// not overlap each other, descriptors or input. No pointer survives this call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_risk_into(
    input: *const u8,
    length: usize,
    outputs: *const crate::quant::Buffer,
    count: usize,
) -> *mut c_char {
    let result = std::panic::catch_unwind(|| -> Result<(), String> {
        if input.is_null() || length > MAX_INPUT || outputs.is_null() || count != 4 {
            return Err("invalid term input/output descriptors".into());
        }
        // SAFETY: caller provides four live descriptors and the admitted input.
        let descriptors = unsafe { std::slice::from_raw_parts(outputs, count) };
        if descriptors.iter().any(|b| {
            b.data.is_null()
                || !(b.data as usize).is_multiple_of(std::mem::align_of::<f64>())
                || b.len > 128 * 1024 * 1024
                || b.shape != [b.len, 1, 1]
        }) {
            return Err("invalid term numeric output storage".into());
        }
        let body = unsafe { std::slice::from_raw_parts(input, length) };
        let Output::Risk(result) = execute(body)? else {
            return Err("numeric outputs require term-risk-1".into());
        };
        let values = [result.oas, result.price, result.dv01, result.sensitivities];
        if descriptors
            .iter()
            .zip(&values)
            .any(|(b, v)| b.len != v.len())
        {
            return Err("term numeric output shape mismatch".into());
        }
        // Validate the whole result before publishing any caller-owned output.
        for (buffer, values) in descriptors.iter().zip(&values) {
            // SAFETY: validated length; caller guarantees disjoint owned storage.
            unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), buffer.data, values.len()) };
        }
        Ok(())
    })
    .unwrap_or_else(|_| Err("native term request failed validation".into()));
    let bytes = match result {
        Ok(()) => b"{\"ok\":true}".to_vec(),
        Err(error) => serde_json::to_vec(&Response::Error { ok: false, error }).unwrap(),
    };
    CString::new(bytes).unwrap().into_raw()
}
