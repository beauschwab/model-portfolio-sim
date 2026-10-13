//! Whole-book supplied funding histories preserve exclusive collateral replay.
//! Native funding cash is priced once; security movements are separately audited.
mod admission;
mod metrics;
mod pricing;
mod types;
use crate::{
    advanced_graph::{BaseAnchor, OasTarget, PositionOas, Role},
    funding_contract,
};
use std::sync::Arc;
pub use types::*;
pub const SCHEMA: &str = "funding-product-graph-1";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone)]
pub struct FundingGraph {
    request: Arc<Request>,
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    result: Arc<ResultData>,
}
fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let mut out = Vec::new();
    let mut rows = 0usize;
    for (s, set) in r.scenarios.iter().zip(&r.histories) {
        for (role, paths) in [
            (Role::Physical, &set.physical),
            (Role::Pricing, &set.pricing),
            (Role::PricingDown, &set.pricing_down),
            (Role::PricingUp, &set.pricing_up),
        ] {
            for p in paths {
                portfolio_compute_control::checkpoint()?;
                let projection = funding_contract::evaluate(&p.request)?;
                rows = rows
                    .checked_add(admission::source_rows(&projection))
                    .ok_or("funding generated row overflow")?;
                if rows > MAX_ROWS {
                    return Err("funding graph generated rows exceed admission".into());
                }
                out.push(NativeSource {
                    scenario_id: s.scenario_id.clone(),
                    run_id: s.run_id.clone(),
                    path_id: p.path_id.clone(),
                    weight: p.weight,
                    role,
                    request: p.request.clone(),
                    projection,
                });
            }
        }
    }
    Ok(out)
}
impl FundingGraph {
    pub fn new(r: &Request) -> Result<Self, String> {
        let base = admission::validate(r)?;
        let key = admission::base_key(r, base)?;
        let source = sources(r)?;
        let q = pricing::slice(&source, &r.base_scenario_id, Role::Pricing);
        let oas = pricing::solve(
            &q,
            &r.scenarios[base].collateral_curve,
            r.as_of_ordinal,
            &r.oas_target,
            pricing::direction(r),
        )?;
        let anchor = BaseAnchor {
            schema: "funding-base-anchor-1".into(),
            calibration_input: serde_json::from_slice(&key).map_err(|e| e.to_string())?,
            oas: vec![PositionOas {
                position_id: r.book_id.clone(),
                base_oas: oas,
                calibration_kind: if matches!(r.oas_target, OasTarget::Locked { .. }) {
                    "supplied_locked_oas"
                } else {
                    "directional_q_cash_magnitude_target"
                }
                .into(),
            }],
        };
        let result = metrics::evaluate(r, source, anchor.clone())?;
        admission::retained(r, &result, key.len())?;
        Ok(Self {
            request: Arc::new(r.clone()),
            base_key: Arc::new(key),
            anchor: Arc::new(anchor),
            result: Arc::new(result),
        })
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn snapshot(&self) -> &ResultData {
        &self.result
    }
    pub fn base_anchor(&self) -> &BaseAnchor {
        &self.anchor
    }
    pub fn reprice(&self, r: &Request) -> Result<ResultData, String> {
        let base = admission::validate(r)?;
        if admission::base_key(r, base)?.as_slice() != self.base_key.as_slice() {
            return Err("BASE_ANCHOR_CHANGED: explicit new funding calibration required".into());
        }
        let result = metrics::evaluate(r, sources(r)?, (*self.anchor).clone())?;
        admission::retained(r, &result, self.base_key.len())?;
        Ok(result)
    }
    pub fn repriced(&self, r: &Request) -> Result<Self, String> {
        let result = self.reprice(r)?;
        Ok(Self {
            request: Arc::new(r.clone()),
            base_key: Arc::clone(&self.base_key),
            anchor: Arc::clone(&self.anchor),
            result: Arc::new(result),
        })
    }
}
pub fn analyze(r: &Request) -> Result<ResultData, String> {
    FundingGraph::new(r).map(|g| (*g.result).clone())
}
