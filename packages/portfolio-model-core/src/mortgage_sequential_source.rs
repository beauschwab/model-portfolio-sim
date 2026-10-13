use crate::{
    consumer_contract::ConsumerIdentity,
    credit_operator::{self, CreditOperatorRequest},
    mortgage_coupled_carry as coupled_carry,
    mortgage_state::{MortgageRequest, MortgageResult},
};
use serde::{Deserialize, Serialize};
#[path = "mortgage_sequential_source/collector.rs"]
mod collector;
pub use collector::project_with_check;
pub const RECEIPT_SCHEMA: &str = "mortgage-loop-interval-1";
pub fn admit_receipts<T: Serialize, U: Serialize>(
    identity: &T,
    ownership: &U,
    count: usize,
) -> Result<(), String> {
    let metadata = serde_json::to_vec(&(identity, ownership))
        .map_err(|e| e.to_string())?
        .len();
    let bytes = count
        .checked_mul(
            metadata
                .checked_add(8192)
                .ok_or("receipt metadata overflow")?,
        )
        .ok_or("receipt bytes overflow")?;
    if count > 4096 || bytes > 64 * 1024 * 1024 {
        return Err("native interval receipt cardinality/retained bytes exceeded".into());
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize)]
pub struct MortgageIntervalReceipt {
    pub schema: &'static str,
    pub financial_source_sha256: &'static str,
    pub identity: ConsumerIdentity,
    pub ownership: serde_json::Value,
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
    pub loan_operator_request: CreditOperatorRequest,
    pub investor_operator_request: CreditOperatorRequest,
    pub loan_transition: [[f64; 7]; 7],
    pub loan_earned_interest: [[f64; 7]; 7],
    pub investor_transition: [[f64; 7]; 7],
    pub investor_earned_interest: [[f64; 7]; 7],
    pub opening_probability: [f64; 7],
    pub opening_borrower_principal: [f64; 3],
    pub opening_borrower_interest: [f64; 3],
    pub opening_investor_principal: [f64; 3],
    pub opening_investor_interest: [f64; 3],
    pub default_claim_start_index: usize,
    pub payment_is_boundary: bool,
    pub arm_reset_is_boundary: bool,
    pub event_order: &'static str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepayment {
    pub kind: String,
    pub fraction: f64,
    pub principal: f64,
    pub interest: f64,
    pub recognized_interest: f64,
    pub entitlement_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scheduled {
    pub origin: usize,
    pub principal: f64,
    pub interest: f64,
    pub recognized_interest: f64,
    pub entitlement_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub interval_index: usize,
    pub ordinal: i32,
    pub transported_loan_principal: [f64; 7],
    pub transported_loan_interest: [f64; 7],
    pub transported_investor_principal: [f64; 7],
    pub transported_investor_interest: [f64; 7],
    pub default_claim_start: usize,
    pub default_claim_end: usize,
    pub default_recoveries: Vec<DefaultRecovery>,
    pub prepayments: Vec<Prepayment>,
    pub scheduled: Vec<Scheduled>,
    pub closing_investor_principal: [f64; 3],
    pub closing_investor_interest: [f64; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultRecovery {
    pub claim_index: usize,
    pub origin: usize,
    pub tail_index: usize,
    pub ordinal: i32,
    pub principal: f64,
    pub entitlement_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashLink {
    pub entitlement_id: String,
    pub cash_event_id: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub boundaries: Vec<Boundary>,
    pub cash_links: Vec<CashLink>,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub native_projection: MortgageResult,
    pub intervals: Vec<MortgageIntervalReceipt>,
    pub capture: Capture,
}
impl MortgageIntervalReceipt {
    /// Converts ONLY the captured native hazard descriptor; no mortgage hazard formula.
    /// Production still needs immutable same-loop receipts exported by the installed engine.
    pub fn carry_request(
        &self,
        y: f64,
        policy: coupled_carry::Policy,
    ) -> Result<coupled_carry::Request, String> {
        if self.schema != RECEIPT_SCHEMA
            || self.financial_source_sha256 != crate::mortgage_receipts::generator_source_sha256()
            || self.loan_operator_request.transition_hazards_per_year
                != self.investor_operator_request.transition_hazards_per_year
            || self.loan_operator_request.duration_years != self.duration_years_act365f
            || self.investor_operator_request.duration_years != self.duration_years_act365f
        {
            return Err(
                "native mortgage interval receipt identity/generator/clock mismatch".into(),
            );
        }
        let loan = credit_operator::compile(&self.loan_operator_request)?;
        let owner = credit_operator::compile(&self.investor_operator_request)?;
        if loan.transition() != &self.loan_transition
            || loan.earned_interest() != &self.loan_earned_interest
            || owner.transition() != &self.investor_transition
            || owner.earned_interest() != &self.investor_earned_interest
        {
            return Err(
                "native mortgage interval captured coefficients differ from shared compiler".into(),
            );
        }
        let mut q = [[0.; 7]; 7];
        for (i, row) in self
            .investor_operator_request
            .transition_hazards_per_year
            .iter()
            .enumerate()
        {
            q[i][i] = -row.iter().sum::<f64>();
            for (j, &rate) in row.iter().take(3).enumerate() {
                if i != j {
                    q[i][j] = rate;
                }
            }
            q[i][3 + i] = row[3];
            q[i][6] = row[4];
        }
        Ok(coupled_carry::Request {
            schema: coupled_carry::SCHEMA.into(),
            generator_source: crate::credit_accounting::Identity {
                id: format!(
                    "{}:{}:interval:{}",
                    self.identity.position_id, self.identity.run_id, self.interval_index
                ),
                revision: self.identity.source_revision.clone(),
            },
            measure: self.investor_operator_request.measure,
            duration_years: self.duration_years_act365f,
            generator: q,
            recognized_contractual_rates: self.investor_operator_request.accrual_rates,
            original_annual_effective_rate: y,
            policy,
        })
    }
}
