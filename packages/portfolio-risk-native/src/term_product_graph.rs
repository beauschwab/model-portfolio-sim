//! Explicit indexed term/CD supplied-measure graph, immutable original base OAS.
//! Optimal joint exercise, future behavior fitting and ledger adoption are separate.
mod admission;
pub(crate) mod legacy_request;
mod metrics;
pub(crate) mod pricing;
pub(crate) mod strict_request;
mod types;
use crate::{
    advanced_graph::{BaseAnchor, OasTarget, PositionOas, Role},
    term_enhancements::{cd, cd_sequence, indexed},
};
use std::sync::Arc;
pub use types::*;
pub const SCHEMA: &str = "term-product-graph-1";
pub const SEQUENCE_SCHEMA: &str = "term-product-graph-2";
pub const MAX_RUNS: usize = 4096;
pub const MAX_ROWS: usize = 250_000;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone)]
pub struct TermGraph {
    request: Arc<Request>,
    base_key: Arc<Vec<u8>>,
    anchor: Arc<BaseAnchor>,
    result: Arc<ResultData>,
    original_sources: Arc<Option<Vec<NativeSource>>>,
}
fn project(
    path: &History,
    role: Role,
    control: &portfolio_compute_control::Control,
) -> Result<SourceProjection, String> {
    #[cfg(test)]
    evidence::PROJECTIONS.with(|n| n.set(n.get() + 1));
    let projection = match &path.product {
        ProductHistory::CdSequence { request } => SourceProjection::CdSequence {
            result: cd_sequence::prepare(request, control)?,
        },
        ProductHistory::IndexedTerm { request } => SourceProjection::IndexedTerm {
            result: indexed::build_path(request)?,
        },
        ProductHistory::CdElection { request } => {
            let result = if role == Role::Physical {
                cd::step_cd(request)?
            } else {
                cd::step_cd_pricing(request)?
            };
            if result.interest_cash_paid < 0.
                || result.closing.accrued_interest < 0.
                || (result.closing.principal == 0. && result.closing.accrued_interest != 0.)
                || result.new_contracts.iter().any(|c| c.accrued_interest < 0.)
                || result.remaining_schedule.events.iter().any(|e| {
                    if request.direction == portfolio_model_core::floating_rate::Direction::Receipt
                    {
                        e.components.cash_interest < 0.
                    } else {
                        e.components.cash_interest > 0.
                    }
                })
            {
                return Err("term CD graph does not admit negative/prepaid accrued rights".into());
            }
            SourceProjection::CdElection { result }
        }
    };
    Ok(projection)
}
fn original_sources(r: &Request) -> Result<Option<Vec<NativeSource>>, String> {
    let Some(original) = &r.original_base else {
        return Ok(None);
    };
    let control = portfolio_compute_control::active()
        .unwrap_or(portfolio_compute_control::Control::new(3_600_000)?);
    let mut sources = Vec::new();
    for owner in &original.histories {
        for (role, paths) in [
            (Role::Physical, &owner.histories.physical),
            (Role::Pricing, &owner.histories.pricing),
            (Role::PricingDown, &owner.histories.pricing_down),
            (Role::PricingUp, &owner.histories.pricing_up),
        ] {
            for path in paths {
                portfolio_compute_control::checkpoint()?;
                sources.push(NativeSource {
                    position_id: owner.position_id.clone(),
                    scenario_id: original.scenario.scenario_id.clone(),
                    run_id: original.scenario.run_id.clone(),
                    path_id: path.path_id.clone(),
                    weight: path.weight,
                    role,
                    product: path.product.clone(),
                    projection: project(path, role, &control)?,
                });
            }
        }
    }
    Ok(Some(sources))
}
fn sources(r: &Request) -> Result<Vec<NativeSource>, String> {
    let control = portfolio_compute_control::active()
        .unwrap_or(portfolio_compute_control::Control::new(3_600_000)?);
    let mut out = Vec::new();
    let mut rows = 0usize;
    for p in &r.positions {
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for path in paths {
                    portfolio_compute_control::checkpoint()?;
                    let projection = project(path, role, &control)?;
                    rows = rows
                        .checked_add(admission::source_rows(&projection))
                        .ok_or("term graph generated rows overflow")?;
                    if rows > MAX_ROWS {
                        return Err("term graph generated row admission exceeded".into());
                    }
                    out.push(NativeSource {
                        position_id: p.position_id.clone(),
                        scenario_id: s.scenario_id.clone(),
                        run_id: s.run_id.clone(),
                        path_id: path.path_id.clone(),
                        weight: path.weight,
                        role,
                        product: path.product.clone(),
                        projection,
                    });
                }
            }
        }
    }
    Ok(out)
}
impl TermGraph {
    pub fn new(r: &Request) -> Result<Self, String> {
        let base = admission::validate(r)?;
        let key = admission::base_key(r, base)?;
        let original = original_sources(r)?;
        let source = sources(r)?;
        let mut oas = Vec::new();
        for p in &r.positions {
            let q = if r.schema == SEQUENCE_SCHEMA {
                Vec::new()
            } else {
                pricing::slice(&source, &p.position_id, &r.base_scenario_id, Role::Pricing)
            };
            oas.push(PositionOas {
                position_id: p.position_id.clone(),
                base_oas: if r.schema == SEQUENCE_SCHEMA {
                    match &p.oas_target {
                        OasTarget::Locked { base_oas, .. } => *base_oas,
                        _ => {
                            return Err(
                                "sequence graph requires original locked OAS; no calibration"
                                    .into(),
                            )
                        }
                    }
                } else {
                    pricing::solve(
                        &q,
                        &r.scenarios[base].collateral_curve,
                        r.as_of_ordinal,
                        &p.oas_target,
                        pricing::direction(&p.known),
                    )?
                },
                calibration_kind: if matches!(p.oas_target, OasTarget::Locked { .. }) {
                    "supplied_locked_oas"
                } else {
                    "directional_q_cash_magnitude_target"
                }
                .into(),
            });
        }
        let anchor = BaseAnchor {
            schema: if r.schema == SEQUENCE_SCHEMA {
                "term-base-anchor-2"
            } else {
                "term-base-anchor-1"
            }
            .into(),
            calibration_input: serde_json::from_slice(&key).map_err(|e| e.to_string())?,
            oas,
        };
        if r.schema == SEQUENCE_SCHEMA {
            let original_index =
                pricing::SourceIndex::new(original.as_ref().ok_or("original sources missing")?)?;
            for (p, quote) in r.positions.iter().zip(&r.original_quotes) {
                let original_base = r.original_base.as_ref().ok_or("original base required")?;
                let q = original_index.slice(
                    &p.position_id,
                    &original_base.scenario.scenario_id,
                    Role::Pricing,
                );
                let locked = anchor
                    .oas
                    .iter()
                    .find(|o| o.position_id == p.position_id)
                    .ok_or("locked OAS missing")?;
                let pv = pricing::weighted_price(
                    &q,
                    &original_base.scenario.collateral_curve,
                    r.as_of_ordinal,
                    locked.base_oas,
                    0.,
                )?;
                if (pv - quote.signed_cash_price).abs()
                    > 1e-9_f64.max(1e-12 * pv.abs().max(quote.signed_cash_price.abs()))
                {
                    return Err(
                        "original locked-OAS Q quote conflicts; no refit or ignored target".into(),
                    );
                }
            }
        }
        let mut result = metrics::evaluate(r, source, anchor.clone())?;
        result.original_native_sources = original.clone();
        if let Some(status) = result.sequence_policy.as_mut() {
            status.original_native_sequence_sources = original.as_ref().map_or(0, |v| {
                v.iter()
                    .filter(|s| matches!(s.projection, SourceProjection::CdSequence { .. }))
                    .count()
            });
        }
        admission::retained(r, &result, key.len())?;
        Ok(Self {
            request: Arc::new(r.clone()),
            original_sources: Arc::new(original),
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
            return Err("BASE_ANCHOR_CHANGED: explicit new term calibration required".into());
        }
        let mut result = metrics::evaluate(r, sources(r)?, (*self.anchor).clone())?;
        result.original_native_sources = (*self.original_sources).clone();
        if let Some(status) = result.sequence_policy.as_mut() {
            status.original_native_sequence_sources =
                self.original_sources.as_ref().as_ref().map_or(0, |v| {
                    v.iter()
                        .filter(|s| matches!(s.projection, SourceProjection::CdSequence { .. }))
                        .count()
                });
        }
        admission::retained(r, &result, self.base_key.len())?;
        Ok(result)
    }
    pub fn repriced(&self, r: &Request) -> Result<Self, String> {
        let result = self.reprice(r)?;
        Ok(Self {
            request: Arc::new(r.clone()),
            base_key: Arc::clone(&self.base_key),
            original_sources: Arc::clone(&self.original_sources),
            anchor: Arc::clone(&self.anchor),
            result: Arc::new(result),
        })
    }
}
pub fn analyze(r: &Request) -> Result<ResultData, String> {
    TermGraph::new(r).map(|g| (*g.result).clone())
}

#[cfg(test)]
mod evidence {
    use std::cell::Cell;
    thread_local! {pub static SOLVES:Cell<usize>=const{Cell::new(0)};pub static PROJECTIONS:Cell<usize>=const{Cell::new(0)};}
    pub fn reset() {
        SOLVES.with(|n| n.set(0));
        PROJECTIONS.with(|n| n.set(0));
    }
    pub fn calls() -> (usize, usize) {
        (SOLVES.with(Cell::get), PROJECTIONS.with(Cell::get))
    }
}
#[cfg(test)]
#[path = "term_product_graph/proof_tests.rs"]
mod proof_tests;
