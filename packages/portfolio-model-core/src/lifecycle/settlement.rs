//! Declared settlement walk for an admitted plan: obligations in event-coordinate order with
//! cash conservation, facility headroom, pledge release before delivery and dependency gating.
//! It checks the plan's declared signed amounts only. Native valuation, margin calculation and
//! journal posting belong to the later lifecycle stages and to the daily Sim.
use super::admission::Admitted;
use super::types::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Settled,
    /// The obligation itself failed; its gross claim survives.
    Failed,
    /// A dependency did not settle, so this obligation cannot settle either.
    Blocked,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub obligation_id: String,
    pub status: StepStatus,
    pub cash_after: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Walk {
    pub account_id: String,
    pub steps: Vec<Step>,
    pub closing_cash: f64,
    pub minimum_cash: f64,
}

pub fn walk(plan: &Admitted, account_id: &str) -> Result<Walk, Refusal> {
    let r = &plan.request;
    let account = r
        .accounts
        .iter()
        .find(|a| a.account_id == account_id)
        .ok_or_else(|| {
            Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!("unknown account `{account_id}`"),
            )
        })?;
    let lots: HashMap<&str, &PositionLot> = r.lots.iter().map(|l| (l.lot_id.as_str(), l)).collect();
    let mut pledged: HashMap<&str, f64> = HashMap::new();
    let mut pledge_left: HashMap<&str, f64> = HashMap::new();
    for p in &r.pledges {
        *pledged.entry(p.lot_id.as_str()).or_default() += p.quantity;
        pledge_left.insert(p.pledge_id.as_str(), p.quantity);
    }
    let pledge_lot: HashMap<&str, &str> = r
        .pledges
        .iter()
        .map(|p| (p.pledge_id.as_str(), p.lot_id.as_str()))
        .collect();
    let mut delivered: HashMap<&str, f64> = HashMap::new();
    let mut drawn: HashMap<&str, f64> = r
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f.drawn))
        .collect();
    let commitment: HashMap<&str, f64> = r
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f.commitment))
        .collect();

    let mut order: Vec<&SettlementObligation> = r
        .obligations
        .iter()
        .filter(|o| o.account_id == account_id)
        .collect();
    order.sort_by_key(|o| (o.due_ordinal, o.phase, o.sequence));
    let (mut cash, mut minimum) = (account.cash, account.cash);
    let mut settled: HashSet<&str> = HashSet::new();
    let mut steps = Vec::with_capacity(order.len());
    for o in order {
        let id = o.obligation_id.as_str();
        let ready = o.depends_on.iter().all(|d| settled.contains(d.as_str()));
        let status = if o.state == ObligationState::Failed {
            StepStatus::Failed
        } else if !ready {
            StepStatus::Blocked
        } else {
            match o.kind {
                ObligationKind::FinancingDraw => {
                    let f = o.facility_id.as_deref().unwrap_or_default();
                    let headroom = commitment[f] - drawn[f];
                    if o.cash > headroom + 1e-9 * headroom.abs().max(1.) {
                        return Err(Refusal::new(
                            INSUFFICIENT_SETTLEMENT_LIQUIDITY,
                            format!("draw `{id}` exceeds facility `{f}` headroom {headroom}"),
                        ));
                    }
                    *drawn.get_mut(f).unwrap_or(&mut 0.) += o.cash;
                }
                ObligationKind::FinancingRepayment => {
                    if let Some(f) = o.facility_id.as_deref() {
                        let d = drawn.get_mut(f).ok_or_else(|| {
                            Refusal::new(LIFECYCLE_MAPPING_REQUIRED, "unknown facility")
                        })?;
                        *d += o.cash;
                        if *d < -1e-9 {
                            return Err(Refusal::new(
                                OVERALLOCATION,
                                format!("repayment `{id}` exceeds drawn facility `{f}`"),
                            ));
                        }
                    }
                }
                ObligationKind::CollateralRelease => {
                    let p = o.pledge_id.as_deref().unwrap_or_default();
                    let left = pledge_left.get_mut(p).ok_or_else(|| {
                        Refusal::new(LIFECYCLE_MAPPING_REQUIRED, "unknown pledge")
                    })?;
                    if o.quantity > *left + 1e-9 {
                        return Err(Refusal::new(
                            INVENTORY_UNAVAILABLE,
                            format!("release `{id}` exceeds pledge `{p}`"),
                        ));
                    }
                    *left -= o.quantity;
                    *pledged.get_mut(pledge_lot[p]).unwrap_or(&mut 0.) -= o.quantity;
                }
                ObligationKind::SecurityDelivery => {
                    let lot = o.lot_id.as_deref().unwrap_or_default();
                    let l = lots[lot];
                    let gone = delivered.get(lot).copied().unwrap_or(0.);
                    let available = l.quantity
                        - l.outside_encumbrance
                        - pledged.get(lot).copied().unwrap_or(0.)
                        - gone;
                    if o.quantity > available + 1e-9 * l.quantity.max(1.) {
                        return Err(Refusal::new(INVENTORY_UNAVAILABLE, format!(
                            "delivery `{id}` of lot `{lot}` exceeds its unencumbered available quantity {available}")));
                    }
                    delivered.insert(lot, gone + o.quantity);
                }
                _ => {}
            }
            cash += o.cash;
            if cash < -1e-9 * account.cash.abs().max(1.) {
                return Err(Refusal::new(
                    INSUFFICIENT_SETTLEMENT_LIQUIDITY,
                    format!("obligation `{id}` leaves account `{account_id}` cash at {cash}"),
                ));
            }
            minimum = minimum.min(cash);
            settled.insert(id);
            StepStatus::Settled
        };
        steps.push(Step {
            obligation_id: o.obligation_id.clone(),
            status,
            cash_after: cash,
        });
    }
    Ok(Walk {
        account_id: account_id.into(),
        steps,
        closing_cash: cash,
        minimum_cash: minimum,
    })
}
