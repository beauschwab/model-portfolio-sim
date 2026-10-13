//! Explicit supplied P/Q deposit state policies, liability valuation and frozen base OAS.
mod admission;
mod metrics;
mod pricing;
mod types;
use crate::advanced_graph::{BaseAnchor, OasTarget, PositionOas, Role};
use portfolio_model_core::deposit_state;
use std::sync::Arc;
pub use types::*;
pub const SCHEMA: &str = "deposit-product-graph-1";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let mut out = Vec::new();
    let mut rows = 0usize;
    for p in &r.positions {
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            for (role, histories) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for h in histories {
                    portfolio_compute_control::checkpoint()?;
                    let projection = deposit_state::simulate_declared_measure(&h.request)?;
                    rows = rows
                        .checked_add(
                            projection.points.len()
                                + projection.cashflows.len()
                                + projection.migrations.len()
                                + projection.interest.len(),
                        )
                        .ok_or("deposit graph row overflow")?;
                    if rows > MAX_ROWS {
                        return Err("deposit generated rows exceed admission".into());
                    }
                    let last = projection
                        .points
                        .last()
                        .ok_or("deposit graph missing closing state")?;
                    if projection.supplied_franchise_value.is_some()
                        || last
                            .balances
                            .iter()
                            .chain(&last.accrued_interest)
                            .any(|v| *v != 0.)
                    {
                        return Err("deposit graph requires complete zero liability/interest tail; no franchise or residual principal value inferred".into());
                    }
                    out.push(NativeSource {
                        position_id: p.position_id.clone(),
                        scenario_id: s.scenario_id.clone(),
                        run_id: s.run_id.clone(),
                        path_id: h.path_id.clone(),
                        weight: h.weight,
                        role,
                        projection,
                    });
                }
            }
        }
    }
    Ok(out)
}
#[derive(Clone)]
pub struct DepositGraph {
    request: Arc<Request>,
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    result: Arc<ResultData>,
}
impl DepositGraph {
    pub fn new(r: &Request) -> Result<Self, String> {
        let base = admission::validate(r)?;
        let key = admission::base_key(r, base)?;
        let source = sources(r)?;
        let mut oas = Vec::new();
        for p in &r.positions {
            let q = pricing::slice(&source, &p.position_id, &r.base_scenario_id, Role::Pricing);
            oas.push(PositionOas {
                position_id: p.position_id.clone(),
                base_oas: pricing::solve(
                    &q,
                    &r.scenarios[base].collateral_curve,
                    r.as_of_ordinal,
                    &p.oas_target,
                )?,
                calibration_kind: if matches!(p.oas_target, OasTarget::Locked { .. }) {
                    "supplied_locked_oas"
                } else {
                    "positive_q_liability_target"
                }
                .into(),
            });
        }
        let anchor = BaseAnchor {
            schema: "deposit-base-anchor-1".into(),
            calibration_input: serde_json::from_slice(&key).map_err(|e| e.to_string())?,
            oas,
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
            return Err(
                "BASE_ANCHOR_CHANGED: explicit new deposit base calibration required".into(),
            );
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
    DepositGraph::new(r).map(|g| (*g.result).clone())
}
/// Full structural admission and charged work, without projecting any history.
pub fn audit_admitted_work(r: &Request) -> Result<usize, String> {
    admission::validate_with_work(r).map(|v| v.1)
}
