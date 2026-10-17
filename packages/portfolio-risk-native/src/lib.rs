//! Native cashflow/pricing kernels and market-path lifecycle. Rust market contexts
//! own curve bootstrap and path stages. Calibration and seeded shared Gaussian
//! tape generation, bounded dependency graphs and public controllers are native.
#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
pub use income::accounting as accounting_lifecycle;
pub use market::cache;
pub use models::calibration;
pub use portfolio::cohorts;
pub use portfolio::units::aged as aged_units;
pub use products::installment::graph as advanced_graph;
pub use products::revolving::graph as card_graph;
pub use products::term::projection as advanced_product;
pub use risk::balance as balance_risk;
pub use runtime::controllers;
pub use transport::abi_operations;
pub mod conventions;
pub use income::forecast::input as forecast_input;
pub use income::forecast::paths as forecast_lifecycle;
pub use portfolio::decision_inputs;
pub use products::deposits::capture as deposit_graph_capture;
pub use products::deposits::graph as deposit_graph;
pub use products::deposits::lifecycle as deposit_lifecycle;
pub use products::derivatives::accounting as derivative_accounting;
pub use products::derivatives::contract as derivative_contract;
pub use products::derivatives::derivative_treasury;
pub use products::derivatives::hedge_accounting;
pub use products::derivatives::hedge_treasury;
pub use products::term::dated as dated_term;

pub use products::funding::contract as funding_contract;
pub use products::funding::graph as funding_graph;

pub use graph::incremental;
pub use market::lifecycle as lifecycle_market;
pub use models::joint_market_credit;
pub use portfolio::cohort_audit as graph_cohort;
pub use portfolio::ledger_mapping;
pub use products::derivatives::lifecycle as hedge_lifecycle;
pub use risk::kpis as kpi_lifecycle;
pub mod market;
pub mod math;
pub mod products;
pub use market::curves::quote_bootstrap as market_bootstrap;

pub use market::analysis as model_market;
pub use market::indexed_adapter as market_indexed_adapter;
pub use market::product_source as market_product_source;
pub use models::analysis as model_lifecycle;
pub use models::market_drivers as market_driver_calibration;
pub use portfolio::candidate_source as product_candidate_source;
pub use portfolio::programs as program_lifecycle;
pub use portfolio::strategy as product_strategy;
pub use portfolio::units::product as product_units;
pub use products::mortgages::graph as mortgage_graph;
pub use products::mortgages::input as mortgage_input;
pub use products::mortgages::risk as mortgage_risk;
pub use products::term::contracts as term_enhancements;
pub use products::term::graph as term_product_graph;
#[cfg(feature = "compute-profile")]
pub use runtime::profile;
pub mod treasury;
pub use treasury::bridge as treasury_bridge;

#[macro_export]
macro_rules! compute_span {
    ($name:literal) => {
        #[cfg(feature = "compute-profile")]
        let _compute_span = $crate::runtime::profile::Span::new($name);
    };
}
pub use math::random;
pub use portfolio::units::monthly as unit_lifecycle;
pub use portfolio::whatif;
pub use products::term::deck as term_deck;
pub use products::term::risk as term_risk;
pub use transport::protocol as term_protocol;

pub use graph::indexed as market_indexed_graph;

pub use portfolio::indexed_books as market_indexed_books;

pub use runtime::control as graph_runtime_control;

pub use treasury::modeled_ftp_policy;

pub use products::commercial::cash as commercial_cash;
pub use products::commercial::continuation as commercial_continuation;
pub use products::commercial::credit_compiler as commercial_credit_compiler;
pub use products::commercial::exercise as commercial_exercise;
pub use products::commercial::factor_source as commercial_factor_source;
pub use products::commercial::fee_source as commercial_fee_source;
pub use products::commercial::graph as commercial_graph;
pub use products::commercial::source_income as commercial_source_income;

pub use products::commercial::stochastic_exercise as commercial_stochastic_exercise;
pub use products::commercial::units as commercial_units;
pub use products::derivatives::saved_source as saved_derivative_source;

pub use products::commercial::generated_graph as commercial_generated_graph;
pub use products::commercial::generated_policy as commercial_generated_policy;

pub mod graph;
pub mod income;
pub mod models;
pub mod portfolio;
pub mod risk;
pub mod runtime;
pub mod transport;

pub mod pricing;
pub use runtime::compute::with_compute_threads;
pub use transport::discount::portfolio_risk_abi_version;
pub use transport::discount::portfolio_risk_price;
