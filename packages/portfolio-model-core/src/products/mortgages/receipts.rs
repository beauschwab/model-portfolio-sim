//! Borrowed observations from the existing mortgage loop; no second projection.
use crate::{
    credit_operator::{CreditOperatorRequest, CreditOperatorState},
    mortgage_state::{MortgageRequest, MortgageResult},
};

pub const RECEIPT_SCHEMA: &str = "mortgage-loop-receipts-1";
/// Reviewed generator body before the additive receipt instrumentation. This is
/// not the hash of the instrumented file or a supplied market-data attestation.
pub const BASE_FINANCIAL_SOURCE_SHA256: &str =
    "3BBCECC486DE438A3C6C7586337ED292B2ABBED96156102A4CE5BE327E867E71";
pub const SOURCE_HASH_SCOPE: &str = "length-prefixed-core-mortgage-generator-and-dependencies-1";
const SOURCE_FILES: [(&str, &[u8]); 9] = [
    ("mortgage_state.rs", include_bytes!("state.rs")),
    ("mortgage_receipts.rs", include_bytes!("receipts.rs")),
    (
        "consumer_contract.rs",
        include_bytes!("../consumer/contracts.rs"),
    ),
    (
        "credit_operator.rs",
        include_bytes!("../../models/credit/operator.rs"),
    ),
    (
        "credit.rs",
        include_bytes!("../../models/credit/transition.rs"),
    ),
    (
        "amortization.rs",
        include_bytes!("../../cashflows/amortization.rs"),
    ),
    ("cashflow.rs", include_bytes!("../../cashflows/dated.rs")),
    (
        "floating_rate.rs",
        include_bytes!("../../conventions/floating_rate.rs"),
    ),
    ("limits.rs", include_bytes!("../../validation/limits.rs")),
];
pub fn source_code_bytes() -> usize {
    SOURCE_FILES
        .iter()
        .map(|(name, bytes)| 16 + name.len() + bytes.len())
        .sum()
}
/// Compiled source bytes, including the receipt seam and shared financial helpers.
/// None projection never calls this or allocates the digest. Actual market/source
/// inputs remain bound separately by the immutable original/current raw hashes.
pub fn generator_source_sha256() -> &'static str {
    use sha2::{Digest, Sha256};
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for (name, bytes) in SOURCE_FILES {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        format!("{:x}", hash.finalize())
    })
    .as_str()
}
pub struct AdmissionView<'a> {
    pub request: &'a MortgageRequest,
    pub interval_count: usize,
}
pub struct IntervalView<'a> {
    pub request: &'a MortgageRequest,
    pub interval_index: usize,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub path_start_ordinal: i32,
    pub duration_years_act365f: f64,
    pub contractual_day_fraction: f64,
    pub note_rate_before_boundary: f64,
    pub investor_coupon_before_boundary: f64,
    pub payment_before_boundary: f64,
    pub reset_count_before_boundary: u32,
    pub burnout_before_interval: f64,
    pub loan_operator_request: &'a CreditOperatorRequest,
    pub investor_operator_request: &'a CreditOperatorRequest,
    pub loan_transition: &'a [[f64; 7]; 7],
    pub loan_earned_interest: &'a [[f64; 7]; 7],
    pub investor_transition: &'a [[f64; 7]; 7],
    pub investor_earned_interest: &'a [[f64; 7]; 7],
    pub opening_probability: &'a [f64; 7],
    pub opening_borrower_principal: &'a [f64; 3],
    pub opening_borrower_interest: &'a [f64; 3],
    pub opening_investor_principal: &'a [f64; 3],
    pub opening_investor_interest: &'a [f64; 3],
    pub transported_loan: &'a CreditOperatorState,
    pub transported_investor: &'a CreditOperatorState,
    pub default_claim_start_index: usize,
    pub payment_is_boundary: bool,
    pub arm_reset_is_boundary: bool,
}
pub struct RecoveryView<'a> {
    pub claim_index: usize,
    pub origin: usize,
    pub tail_index: usize,
    pub ordinal: i32,
    pub principal: f64,
    pub entitlement_id: Option<&'a str>,
}
pub struct PrepaymentView<'a> {
    pub kind: &'a str,
    pub fraction: f64,
    pub borrower_principal: f64,
    pub borrower_interest: f64,
    pub principal: f64,
    pub interest: f64,
    pub recognized_interest: f64,
    pub entitlement_id: Option<&'a str>,
}
pub struct ScheduledView<'a> {
    pub origin: usize,
    pub borrower_principal: f64,
    pub borrower_interest: f64,
    pub principal: f64,
    pub interest: f64,
    pub recognized_interest: f64,
    pub entitlement_id: Option<&'a str>,
}
pub struct BoundaryView<'a> {
    pub interval_index: usize,
    pub ordinal: i32,
    pub default_claim_end_index: usize,
    pub closing_borrower_principal: &'a [f64; 3],
    pub closing_borrower_interest: &'a [f64; 3],
    pub closing_investor_principal: &'a [f64; 3],
    pub closing_investor_interest: &'a [f64; 3],
    pub closing_probability: &'a [f64; 7],
    pub matured_probability: f64,
    pub closing_contractual_principal: f64,
    pub closing_contractual_interest: f64,
    pub next_note_rate: f64,
    pub next_payment: f64,
    pub next_reset_count: u32,
    pub next_burnout_incentive_years: f64,
}
pub struct CashLinkView<'a> {
    pub entitlement_id: &'a str,
    pub cash_event_id: &'a str,
}

/// Receivers validate aggregate retention before `admit` returns. Views borrow
/// actual native values and cannot outlive a callback; there are no financial
/// override setters. Errors abort the projection before any successful result.
pub trait MortgageReceiptSink {
    fn admit(&mut self, view: AdmissionView<'_>) -> Result<(), String>;
    fn interval(&mut self, view: IntervalView<'_>) -> Result<(), String>;
    fn recovery(&mut self, view: RecoveryView<'_>) -> Result<(), String>;
    fn prepayment(&mut self, view: PrepaymentView<'_>) -> Result<(), String>;
    fn scheduled(&mut self, view: ScheduledView<'_>) -> Result<(), String>;
    fn boundary(&mut self, view: BoundaryView<'_>) -> Result<(), String>;
    fn cash_link(&mut self, view: CashLinkView<'_>) -> Result<(), String>;
    fn complete(&mut self, result: &MortgageResult) -> Result<(), String>;
}
