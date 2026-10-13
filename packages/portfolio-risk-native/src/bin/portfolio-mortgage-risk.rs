//! One bounded JSON request in, one JSON result envelope out. No Python runtime.
use portfolio_risk_native::mortgage_risk::{MortgageRiskRequest, PrepayData, RiskConfig};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepay {
    month_of_year: Vec<f64>,
    seasonality: Vec<f64>,
    parameters: Vec<f64>,
    ltv_knots: Vec<f64>,
    ltv_coefficients: Vec<f64>,
    smm_table: Vec<f64>,
    smm_scale: f64,
    burnout_table: Vec<f64>,
    burnout_scale: f64,
    cc_vol_points: Vec<f64>,
    fico_x: Vec<f64>,
    fico_y: Vec<f64>,
    size_x: Vec<f64>,
    size_y: Vec<f64>,
    state_multipliers: Vec<f64>,
    channel_multipliers: Vec<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    tenors: Vec<f64>,
    swap_rates: Vec<f64>,
    vol_quotes: Vec<f64>,
    cc_history: Vec<f64>,
    ps_history: Vec<f64>,
    book: Vec<f64>,
    original_hpi: Vec<f64>,
    #[serde(default)]
    prepay_multiplier: Vec<f64>,
    seed: Vec<u32>,
    fixed_oas: Vec<f64>,
    config: RiskConfig,
    prepay: Prepay,
    threads: usize,
    #[serde(default)]
    horizons: Vec<usize>,
    #[serde(default)]
    shocks_bp: Vec<f64>,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Risk(portfolio_risk_native::mortgage_risk::RiskResult),
    Stress(portfolio_risk_native::mortgage_risk::StressResult),
}
fn execute() -> Result<Output, String> {
    const MAX_INPUT: u64 = 64 * 1024 * 1024;
    let mut body = Vec::new();
    io::stdin()
        .take(MAX_INPUT + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_INPUT {
        return Err("request exceeds 64 MiB".into());
    }
    let r: Request = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if !["mortgage-risk-1", "mortgage-stress-1"].contains(&r.schema.as_str())
        || r.threads == 0
        || r.threads > 256
    {
        return Err("unsupported schema or thread count".into());
    }
    if r.schema == "mortgage-risk-1" && (!r.horizons.is_empty() || !r.shocks_bp.is_empty()) {
        return Err("spot-risk schema does not accept stress scenarios".into());
    }
    let p = &r.prepay;
    let request = MortgageRiskRequest {
        tenors: &r.tenors,
        swap_rates: &r.swap_rates,
        vol_quotes: &r.vol_quotes,
        cc_history: &r.cc_history,
        ps_history: &r.ps_history,
        book: &r.book,
        original_hpi: &r.original_hpi,
        prepay_multiplier: &r.prepay_multiplier,
        seed: &r.seed,
        fixed_oas: &r.fixed_oas,
        config: r.config,
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
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(r.threads)
        .build()
        .map_err(|e| e.to_string())?
        .install(|| {
            if r.schema == "mortgage-risk-1" {
                request.run().map(Output::Risk)
            } else {
                request
                    .run_stress(&r.horizons, &r.shocks_bp)
                    .map(Output::Stress)
            }
        })
}
fn main() {
    let result = execute();
    let code = if result.is_ok() { 0 } else { 1 };
    #[derive(Serialize)]
    struct Success<'a> {
        ok: bool,
        result: &'a Output,
    }
    let written = match &result {
        Ok(v) => serde_json::to_writer(
            io::stdout().lock(),
            &Success {
                ok: true,
                result: v,
            },
        ),
        Err(e) => serde_json::to_writer(
            io::stdout().lock(),
            &serde_json::json!({"ok":false,"error":e}),
        ),
    };
    if written.is_err() {
        std::process::exit(2);
    }
    std::process::exit(code);
}
