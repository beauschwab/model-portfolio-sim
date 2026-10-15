//! Shared financial model contracts. Production calculations stay in Rust;
//! callers own transport, provenance persistence and independent verification.
//! Existing model versions are never implicitly replaced by these models.

pub use accounting::credit::expected as credit_accounting;
pub use accounting::credit::income as credit_path_income;
pub use accounting::credit::realized as credit_path_accounting;
pub use accounting::dated_income;
pub use cashflows::amortization;
pub use cashflows::coupon_factor as coupon_factor_state;
pub use cashflows::dated as cashflow;
pub use conventions::floating_rate;
pub use models::behavior::deposits as deposit_calibration;
pub use models::credit::calibration::exact as credit_calibration;
pub use models::credit::calibration::snapshot as credit_snapshot_calibration;
pub use models::credit::covariate as credit_covariate;
pub use models::credit::operator as credit_operator;
pub use models::credit::paths as credit_path;
pub use models::credit::transition as credit;
pub use models::joint_state;
pub use products::commercial::coupon_accounting as commercial_coupon_accounting;
pub use products::consumer::contracts as consumer_contract;
pub use products::deposits::state as deposit_state;
pub use products::installment::accounting as consumer_credit_accounting;
pub use products::installment::state as consumer_installment;
pub use products::mortgages::state as mortgage_state;
pub use products::revolving::accounting as card_credit_accounting;
pub use products::revolving::accounting_v2 as card_credit_accounting_v2;
pub use products::revolving::state as consumer_card;
pub use validation::limits;

use products::mortgages::coupled_carry as mortgage_coupled_carry;
use products::mortgages::default_interest as mortgage_default_interest;
pub use products::mortgages::receipts as mortgage_receipts;
pub use products::mortgages::sequential_carry as mortgage_sequential_carry;
use products::mortgages::sequential_source as mortgage_sequential_source;

pub use products::deposits::revised_income as deposit_revised_income;
pub use products::mortgages::pending_accounting as mortgage_pending_accounting;

pub use products::deposits::continuation as deposit_continuation;
pub use products::deposits::principal as deposit_principal;
pub use products::deposits::source_income as deposit_source_income;
pub use products::deposits::source_owner as deposit_source_owner;

pub use products::funding::modeled as modeled_product_funding;

pub use treasury::ftp_policy as modeled_product_ftp_policy;

pub use products::deposits::fifo_capacity as deposit_fifo_capacity;
pub use products::deposits::source_income_v2 as deposit_source_income_v2;

pub use accounting::fees as nonintegral_fee;
pub use models::observation;
pub use transport::strict_json;

pub mod accounting;
pub mod cashflows;
pub mod conventions;
pub mod models;
pub mod products;
pub mod transport;
pub mod treasury;
pub mod validation;
