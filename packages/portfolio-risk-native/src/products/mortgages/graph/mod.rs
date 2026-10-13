//! Explicit supplied mortgage P/Q histories, full investor tails and frozen OAS.
//! No implicit legacy selection, empirical fit, ledger capture or NII convention.
mod admission;
mod metrics;
mod pricing;
mod types;
use crate::advanced_graph::{BaseAnchor, OasTarget, PositionOas, Role};
use portfolio_model_core::{
    cashflow::CashflowMeasure,
    mortgage_state::{self, MortgageOwnership, MortgageRate, MortgageRequest},
};
use std::sync::Arc;
pub use types::*;
pub const SCHEMA: &str = "mortgage-product-graph-1";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone)]
pub struct MortgageGraph {
    request: Arc<Request>,
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    result: Arc<ResultData>,
}
fn history_request(
    position: &Position,
    scenario: &crate::advanced_graph::Scenario,
    role: Role,
    history: &History,
) -> MortgageRequest {
    let mut raw = position.contract.clone();
    raw.identity.measure = if role == Role::Physical {
        CashflowMeasure::Physical
    } else {
        CashflowMeasure::Pricing
    };
    raw.identity.assumption_id = history.assumption_id.clone();
    raw.identity.run_id = scenario.run_id.clone();
    raw.path = history.path.clone();
    if let MortgageRate::Arm { resets, .. } = &mut raw.rate {
        *resets = history.arm_resets.clone();
    }
    raw
}
fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let mut sources = Vec::new();
    let mut rows = 0usize;
    for position in &r.positions {
        for (scenario, set) in r.scenarios.iter().zip(&position.histories) {
            for (role, histories) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for history in histories {
                    portfolio_compute_control::checkpoint()?;
                    let raw = history_request(position, scenario, role, history);
                    let projection = mortgage_state::project(&raw)?;
                    let count = projection.points.len()
                        + projection.resets.len()
                        + projection.cashflows.len()
                        + projection.borrower_cashflows.len()
                        + projection.entitlements.len()
                        + projection.recognition.len()
                        + projection.defaults.len()
                        + projection
                            .defaults
                            .iter()
                            .map(|d| d.complete_recovery_tail.len())
                            .sum::<usize>();
                    rows = rows
                        .checked_add(count)
                        .ok_or("mortgage generated row overflow")?;
                    if rows > MAX_ROWS {
                        return Err("mortgage generated rows exceed admission".into());
                    }
                    let last = projection
                        .points
                        .last()
                        .ok_or("missing mortgage terminal source")?;
                    if last.contractual_principal != 0.
                        || last
                            .investor_principal
                            .iter()
                            .chain(&last.investor_interest)
                            .any(|v| *v != 0.)
                    {
                        return Err("mortgage graph requires complete contractual/live investor rights; no terminal value inferred".into());
                    }
                    let whole = matches!(raw.ownership, MortgageOwnership::WholeLoan { .. });
                    if whole
                        && (last.borrower_interest.iter().any(|v| *v != 0.)
                            || projection.defaults.iter().any(|d| {
                                d.borrower_contract_interest != 0.
                                    || d.investor_accrued_interest != 0.
                            }))
                    {
                        return Err("whole-loan default/closing interest rights need an explicit collection or disposal policy before valuation".into());
                    }
                    let cash = projection
                        .cashflows
                        .iter()
                        .map(|e| e.principal)
                        .sum::<f64>();
                    let loss = if whole {
                        projection
                            .defaults
                            .iter()
                            .map(|d| d.borrower_principal_loss)
                            .sum::<f64>()
                    } else {
                        0.
                    };
                    let opening = raw.opening_investor_principal.iter().sum::<f64>();
                    if (cash + loss - opening).abs()
                        > 1e-8_f64.max(512. * f64::EPSILON * opening.abs())
                    {
                        return Err("mortgage full investor principal/loss tail does not conserve opening rights".into());
                    }
                    sources.push(NativeSource {
                        position_id: position.position_id.clone(),
                        scenario_id: scenario.scenario_id.clone(),
                        run_id: scenario.run_id.clone(),
                        path_id: history.path_id.clone(),
                        weight: history.weight,
                        assumption_id: history.assumption_id.clone(),
                        role,
                        projection,
                    });
                }
            }
        }
    }
    Ok(sources)
}
impl MortgageGraph {
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
            schema: "mortgage-base-anchor-1".into(),
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
                "BASE_ANCHOR_CHANGED: explicit new mortgage base calibration required".into(),
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
    MortgageGraph::new(r).map(|g| (*g.result).clone())
}
/// Validate graph admission and return its conservative structural work charge,
/// without generating any model path or solving a target OAS.
pub fn audit_admitted_work(r: &Request) -> Result<usize, String> {
    admission::validate_with_work(r).map(|(_, work)| work)
}
