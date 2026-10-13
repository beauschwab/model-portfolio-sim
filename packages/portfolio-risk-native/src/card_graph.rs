//! Supplied revolving-card histories, native state generation and immutable base OAS.
//! No fitted future spending/default policy, effective-yield income or ledger adoption.
mod admission;
mod metrics;
mod pricing;
mod types;
use crate::advanced_graph::{BaseAnchor, OasTarget, PositionOas, Role};
use portfolio_model_core::{cashflow::CashflowMeasure, consumer_card};
use std::sync::Arc;
pub use types::*;
pub const SCHEMA: &str = "card-product-graph-1";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct CardGraph {
    request: Arc<Request>,
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    result: Arc<ResultData>,
}
fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let mut result = Vec::new();
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
                    let mut raw = p.contract.clone();
                    raw.identity.measure = if role == Role::Physical {
                        CashflowMeasure::Physical
                    } else {
                        CashflowMeasure::Pricing
                    };
                    raw.identity.assumption_id = h.assumption_id.clone();
                    raw.identity.run_id = s.run_id.clone();
                    raw.steps = h.steps.clone();
                    let projection = consumer_card::simulate(&raw)?;
                    rows = rows
                        .checked_add(
                            projection.points.len()
                                + projection.cashflows.len()
                                + projection.recognition.len()
                                + projection.statements.len()
                                + usize::from(projection.default.is_some()),
                        )
                        .ok_or("card generated row overflow")?;
                    if rows > MAX_ROWS {
                        return Err("card generated rows exceed admission".into());
                    }
                    let last = projection
                        .points
                        .last()
                        .ok_or("card graph missing terminal source state")?;
                    if [
                        projection.closing_principal,
                        projection.closing_interest,
                        projection.closing_unbilled_interest,
                        projection.closing_unrecognized_billed_interest,
                        projection.closing_fees,
                        projection.closing_grace_contingent_interest,
                        last.limit,
                        last.undrawn,
                    ]
                    .iter()
                    .any(|v| *v != 0.)
                    {
                        return Err("card graph requires explicit complete zero rights/limit tail; no residual balloon or commitment value inferred".into());
                    }
                    result.push(NativeSource {
                        position_id: p.position_id.clone(),
                        scenario_id: s.scenario_id.clone(),
                        run_id: s.run_id.clone(),
                        path_id: h.path_id.clone(),
                        weight: h.weight,
                        assumption_id: h.assumption_id.clone(),
                        role,
                        projection,
                    });
                }
            }
        }
    }
    Ok(result)
}
impl CardGraph {
    pub fn new(r: &Request) -> Result<Self, String> {
        let base = admission::validate(r)?;
        let key = admission::base_key(r, base)?;
        let source = sources(r)?;
        let mut oas = Vec::new();
        for p in &r.positions {
            let paths = pricing::slice(&source, &p.position_id, &r.base_scenario_id, Role::Pricing);
            oas.push(PositionOas {
                position_id: p.position_id.clone(),
                base_oas: pricing::solve(
                    &paths,
                    &r.scenarios[base].collateral_curve,
                    r.as_of_ordinal,
                    &p.oas_target,
                )?,
                calibration_kind: if matches!(p.oas_target, OasTarget::Locked { .. }) {
                    "supplied_locked_oas"
                } else {
                    "positive_q_cash_target"
                }
                .into(),
            });
        }
        let anchor = BaseAnchor {
            schema: "card-base-anchor-1".into(),
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
            return Err("BASE_ANCHOR_CHANGED: explicit new card base calibration required".into());
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
    CardGraph::new(r).map(|g| (*g.result).clone())
}
/// Exact conservative graph preflight work, after graph and core input-domain
/// admission and before source projection. Chronological action feasibility
/// remains authoritative in the native core replay.
pub fn audit_admitted_work(r: &Request) -> Result<usize, String> {
    admission::validate_with_work(r).map(|(_, work)| work)
}
