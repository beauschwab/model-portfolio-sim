//! Shared financial model contracts. Production calculations stay in Rust;
//! callers own transport, provenance persistence and independent verification.
//! Existing model versions are never implicitly replaced by these models.

pub mod amortization;
pub mod card_credit_accounting;
pub mod card_credit_accounting_v2;
pub mod cashflow;
pub mod consumer_card;
pub mod consumer_contract;
pub mod consumer_credit_accounting;
pub mod consumer_installment;
pub mod credit;
pub mod credit_calibration;
pub mod credit_operator;
pub mod credit_path;
pub mod credit_path_income;
pub mod commercial_coupon_accounting;
pub mod coupon_factor_state;
pub mod credit_path_accounting;
pub mod credit_covariate;
pub mod credit_accounting;
pub mod credit_snapshot_calibration;
pub mod dated_income;
pub mod deposit_state;
pub mod deposit_calibration;
pub mod floating_rate;
pub mod joint_state;
pub mod limits;
pub mod mortgage_state;

pub mod mortgage_receipts;
mod mortgage_coupled_carry;
mod mortgage_default_interest;
mod mortgage_sequential_source;
pub mod mortgage_sequential_carry;

pub mod mortgage_pending_accounting;
pub mod deposit_revised_income;

pub mod deposit_principal;
pub mod deposit_continuation;
pub mod deposit_source_income;
pub mod deposit_source_owner;

pub mod modeled_product_funding;

pub mod modeled_product_ftp_policy;

pub mod deposit_fifo_capacity;
pub mod deposit_source_income_v2;

pub mod strict_json;
pub mod nonintegral_fee;
pub mod observation;
