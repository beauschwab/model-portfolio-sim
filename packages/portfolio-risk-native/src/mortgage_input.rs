//! Owned raw mortgage contract shared by accounting and standalone lifecycles.
use crate::mortgage_risk::{MortgageRiskRequest, PrepayData, RiskConfig};
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Prepay {
    pub month_of_year: Vec<f64>,
    pub seasonality: Vec<f64>,
    pub parameters: Vec<f64>,
    pub ltv_knots: Vec<f64>,
    pub ltv_coefficients: Vec<f64>,
    pub smm_table: Vec<f64>,
    pub smm_scale: f64,
    pub burnout_table: Vec<f64>,
    pub burnout_scale: f64,
    pub cc_vol_points: Vec<f64>,
    pub fico_x: Vec<f64>,
    pub fico_y: Vec<f64>,
    pub size_x: Vec<f64>,
    pub size_y: Vec<f64>,
    pub state_multipliers: Vec<f64>,
    pub channel_multipliers: Vec<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedMortgage {
    pub tenors: Vec<f64>,
    pub swap_rates: Vec<f64>,
    pub vol_quotes: Vec<f64>,
    pub cc_history: Vec<f64>,
    pub ps_history: Vec<f64>,
    pub book: Vec<f64>,
    pub original_hpi: Vec<f64>,
    pub seed: Vec<u32>,
    pub fixed_oas: Vec<f64>,
    pub config: RiskConfig,
    pub prepay: Prepay,
}

impl OwnedMortgage {
    pub fn borrow(&self) -> MortgageRiskRequest<'_> {
        let r = self;
        let p = &r.prepay;
        MortgageRiskRequest {
            tenors: &r.tenors,
            swap_rates: &r.swap_rates,
            vol_quotes: &r.vol_quotes,
            cc_history: &r.cc_history,
            ps_history: &r.ps_history,
            book: &r.book,
            original_hpi: &r.original_hpi,
            seed: &r.seed,
            fixed_oas: &r.fixed_oas,
            config: r.config.clone(),
            prepay: PrepayData {
                month_of_year: &p.month_of_year,
                seasonality: &p.seasonality,
                parameters: &p.parameters,
                ltv_knots: &p.ltv_knots,
                ltv_coefficients: &p.ltv_coefficients,
                smm_table: &p.smm_table,
                smm_scale: p.smm_scale,
                burnout_table: &p.burnout_table,
                burnout_scale: p.burnout_scale,
                cc_vol_points: &p.cc_vol_points,
                fico_x: &p.fico_x,
                fico_y: &p.fico_y,
                size_x: &p.size_x,
                size_y: &p.size_y,
                state_multipliers: &p.state_multipliers,
                channel_multipliers: &p.channel_multipliers,
            },
        }
    }
}
