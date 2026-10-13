use crate::model_market::curves::SuppliedCurve;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub source_id: String,
    pub source_revision: String,
    pub input_revision: String,
    pub model_version: String,
    pub currency: String,
    pub as_of_ordinal: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Market {
    pub identity: Identity,
    pub available_unix_seconds: i64,
    pub information_unix_seconds: i64,
    #[serde(deserialize_with = "super::strict_curves::deserialize")]
    pub curves: Vec<SuppliedCurve>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub index_id: String,
    pub index_curve_id: String,
    pub collateral_curve_id: String,
    pub collateral_index: String,
    pub calendar_identity: String,
    pub complete_supplied_date_mapping: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Period {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub reset_ordinal: i32,
    pub reset_cutoff_unix_seconds: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub contract_id: String,
    pub contract_revision: String,
    pub maturity_ordinal: i32,
    pub direction: String,
    pub monetary_unit: String,
    pub notional: f64,
    pub fixed_rate: f64,
    pub margin: f64,
    pub fixed_day_count: String,
    pub index_day_count: String,
    pub periods: Vec<Period>,
}
impl Contract {
    pub(super) fn sign(&self) -> f64 {
        if self.direction == "payer_fixed" {
            1.
        } else {
            -1.
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalAnchor {
    pub review_identity: String,
    pub locked_oas: f64,
    pub market: Market,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub identity: Identity,
    pub measure: String,
    pub projection_policy: String,
    pub curve_time_policy: String,
    pub binding: Binding,
    pub contract: Contract,
    pub original: OriginalAnchor,
    pub current_market: Market,
    pub risk_bump: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub input_bytes: usize,
    pub retained_bytes: usize,
    pub admitted_work: usize,
    pub projection_rows: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Projection {
    pub period_id: String,
    pub scenario_id: String,
    pub reset_ordinal: i32,
    pub reset_cutoff_unix_seconds: i64,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub index_accrual_fraction: f64,
    pub fixed_accrual_fraction: f64,
    pub index_forward: f64,
    pub fixed_cash: f64,
    pub floating_cash: f64,
    pub net_cash: f64,
    pub collateral_discount_factor: f64,
    pub oas_discount_factor: f64,
    pub discounted_cash: f64,
    pub index_tail_used: bool,
    pub collateral_tail_used: bool,
    pub source_kind: String,
    pub information_unix_seconds: i64,
    pub source: Identity,
    pub index_id: String,
    pub calendar_mapping_verified: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Valuation {
    pub scenario_id: String,
    pub pv: f64,
    pub par_fixed_rate: f64,
    pub fixed_annuity: f64,
    pub projections: Vec<Projection>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RiskComparison {
    pub domain: String,
    pub down: Valuation,
    pub up: Valuation,
    pub dollar_pv01: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub model_version: String,
    pub measure: String,
    pub source_status: String,
    pub cache_parent: Request,
    pub admission: Admission,
    pub original: Valuation,
    pub current: Valuation,
    pub risks: Vec<RiskComparison>,
    pub observed_fixings: bool,
    pub stochastic_dynamics: bool,
    pub smile_adoption: bool,
    pub physical_cash: bool,
    pub production_adoption: bool,
    pub calendar_mapping_verified: bool,
    pub paired_history_policy: String,
}
