//! Explicit supplied multi-curve/smile/joint-shock analysis. Does not switch legacy pricing.
pub mod curves;
pub mod joint;
pub mod smile;
use portfolio_model_core::cashflow::validate_ordinal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "model-market-1";
pub const MAX_QUERIES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketModelIdentity {
    pub model_id: String,
    pub model_version: String,
    pub input_revision: String,
    pub assumption_id: String,
    pub rationale: String,
    pub as_of_ordinal: i32,
    pub currency: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMarketRequest {
    pub schema: String,
    pub identity: MarketModelIdentity,
    pub curves: Vec<curves::SuppliedCurve>,
    pub curve_queries: Vec<curves::ForwardQuery>,
    pub smile: Option<smile::SmileSurface>,
    pub option_queries: Vec<smile::OptionQuery>,
    pub joint: Option<joint::JointGaussianRequest>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMarketResult {
    pub schema: String,
    pub identity: MarketModelIdentity,
    pub source_status: String,
    pub curve_results: Vec<curves::ForwardResult>,
    pub option_results: Vec<smile::OptionResult>,
    pub joint: Option<joint::JointGaussianResult>,
}
pub fn analyze(r: &ModelMarketRequest) -> Result<ModelMarketResult, String> {
    if r.schema != SCHEMA || r.identity.model_version != SCHEMA {
        return Err("market model schema/version mismatch".into());
    }
    for v in [
        &r.identity.model_id,
        &r.identity.model_version,
        &r.identity.input_revision,
        &r.identity.assumption_id,
        &r.identity.currency,
    ] {
        label(v, 256)?;
    }
    label(&r.identity.rationale, 1024)?;
    validate_ordinal(r.identity.as_of_ordinal)?;
    if r.curves.is_empty()
        || r.curves.len() > 64
        || r.curve_queries.len() + r.option_queries.len() > MAX_QUERIES
    {
        return Err("market curve/query admission exceeded".into());
    }
    let mut names = HashSet::new();
    for c in &r.curves {
        c.validate()?;
        if c.currency != r.identity.currency || !names.insert(&c.curve_id) {
            return Err("duplicate curve identity or currency mismatch".into());
        }
    }
    if let Some(surface) = &r.smile {
        surface.validate()?;
    } else if !r.option_queries.is_empty() {
        return Err("option query requires explicit smile surface".into());
    }
    // Validate every request before generating potentially large joint paths.
    let mut curves = Vec::new();
    for q in &r.curve_queries {
        curves.push(curves::forward_query(&r.curves, q)?);
    }
    let mut options = Vec::new();
    if let Some(s) = &r.smile {
        for q in &r.option_queries {
            options.push(s.price(q)?);
        }
    }
    let shocks = r.joint.as_ref().map(joint::simulate).transpose()?;
    Ok(ModelMarketResult {
        schema: SCHEMA.into(),
        identity: r.identity.clone(),
        source_status: "supplied_curves_quotes_covariance".into(),
        curve_results: curves,
        option_results: options,
        joint: shocks,
    })
}
pub(super) fn label(v: &str, max: usize) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err("market identity/source must be bounded nonblank text".into());
    }
    Ok(())
}
pub(super) fn range(v: f64, min: f64, max: f64, name: &str) -> Result<(), String> {
    if !v.is_finite() || v < min || v > max {
        return Err(format!("{name} outside finite [{min},{max}]"));
    }
    Ok(())
}
