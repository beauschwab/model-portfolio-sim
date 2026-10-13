use super::{admission::money, pricing, *};
use crate::{
    advanced_graph::{BaseAnchor, Role},
    funding_contract::Exit,
};
use std::collections::BTreeMap;
fn inside(d: i32, left: i32, right: i32, opening: bool) -> bool {
    if opening {
        d == left
    } else {
        d > left && d <= right
    }
}
fn add(to: &mut f64, v: f64, w: f64) -> Result<(), String> {
    *to = money(*to + w * v)?;
    Ok(())
}
fn report(
    paths: &[&NativeSource],
    left: i32,
    right: i32,
    opening: bool,
) -> Result<PhysicalReport, String> {
    let mut a = PhysicalAmounts::default();
    let mut securities = BTreeMap::new();
    for p in paths {
        portfolio_compute_control::checkpoint()?;
        let w = p.weight;
        for e in &p.projection.schedule.events {
            if !inside(e.payment_ordinal, left, right, opening) {
                continue;
            }
            let c = &e.components;
            add(&mut a.principal_cash, c.principal, w)?;
            add(&mut a.interest_cash, c.cash_interest, w)?;
            add(&mut a.fee_cash, c.fees, w)?;
            if e.event_id == "funding-advance" {
                add(&mut a.advance_principal_cash, c.principal, w)?;
            } else if e.event_id == "collateral-retained-recovery"
                || e.event_id.starts_with("principal-recovery:")
                || e.event_id.starts_with("interest-recovery:")
            {
                add(&mut a.recovery_principal_cash, c.principal, w)?;
                add(&mut a.recovery_interest_cash, c.cash_interest, w)?;
            } else if e.event_id.starts_with("contractual:") {
                add(&mut a.maturity_principal_cash, c.principal, w)?;
            } else {
                return Err("unsupported generated funding cash kind".into());
            }
        }
        for c in &p.request.contracts {
            if let Exit::CounterpartyDefault { policy: d } = &c.exit {
                if inside(d.ordinal, left, right, opening) {
                    add(&mut a.default_principal, c.principal, w)?;
                    add(
                        &mut a.default_accrued_interest_claim,
                        d.accrued_interest_claim,
                        w,
                    )?;
                    add(&mut a.economic_principal_loss, d.economic_principal_loss, w)?;
                    add(&mut a.economic_interest_loss, d.economic_interest_loss, w)?;
                }
            }
        }
        for m in &p.projection.margin_calls {
            if inside(m.ordinal, left, right, opening) {
                add(&mut a.margin_call_value, m.margin_call_value, w)?;
                add(&mut a.unmet_margin_value, m.unmet_margin_value, w)?;
            }
        }
        for roll in &p.projection.rollover {
            if inside(roll.ordinal, left, right, opening) {
                add(
                    &mut a.requested_rollover_principal,
                    roll.requested_principal,
                    w,
                )?;
                add(
                    &mut a.executed_rollover_principal,
                    roll.executed_principal,
                    w,
                )?;
                add(
                    &mut a.rollover_access_shortfall,
                    roll.market_access_shortfall,
                    w,
                )?;
            }
        }
        for s in &p.projection.collateral_realizations {
            if inside(s.ordinal, left, right, opening) {
                if !s.cash_already_in_funding_schedule {
                    return Err("funding realization cash lineage missing".into());
                }
                add(
                    &mut a.collateral_gross_sale_proceeds,
                    s.gross_sale_proceeds,
                    w,
                )?;
                add(&mut a.collateral_sale_cost, s.sale_cost, w)?;
                add(&mut a.collateral_net_sale_proceeds, s.net_sale_proceeds, w)?;
                add(
                    &mut a.collateral_surplus_returned,
                    s.surplus_returned_to_owner,
                    w,
                )?;
            }
        }
        for m in &p.projection.collateral_movements {
            if inside(m.ordinal, left, right, opening) {
                let key = (
                    m.asset_id.clone(),
                    m.owner_entity_id.clone(),
                    m.disposition.clone(),
                );
                add(securities.entry(key).or_insert(0.), m.quantity_delta, w)?;
            }
        }
    }
    let security_movements = securities
        .into_iter()
        .map(
            |((asset_id, owner_entity_id, disposition), quantity_delta)| SecurityMovement {
                asset_id,
                owner_entity_id,
                disposition,
                quantity_delta,
            },
        )
        .collect();
    Ok(PhysicalReport {
        amounts: a,
        security_movements,
    })
}
pub(super) fn evaluate(
    r: &Request,
    sources: Vec<NativeSource>,
    anchor: BaseAnchor,
) -> Result<ResultData, String> {
    let mut metrics = Vec::new();
    let oas = anchor.oas[0].base_oas;
    for s in &r.scenarios {
        portfolio_compute_control::checkpoint()?;
        let q = pricing::slice(&sources, &s.scenario_id, Role::Pricing);
        let down = pricing::slice(&sources, &s.scenario_id, Role::PricingDown);
        let up = pricing::slice(&sources, &s.scenario_id, Role::PricingUp);
        let p = pricing::slice(&sources, &s.scenario_id, Role::Physical);
        let bump = r.bump_bp / 10000.;
        let pv = pricing::weighted_price(&q, &s.collateral_curve, r.as_of_ordinal, oas, 0.)?;
        let dv01 = money(
            (pricing::weighted_price(&down, &s.collateral_curve, r.as_of_ordinal, oas, -bump)?
                - pricing::weighted_price(&up, &s.collateral_curve, r.as_of_ordinal, oas, bump)?)
                / (2. * r.bump_bp),
        )?;
        let mut periods = Vec::new();
        for bounds in r.reporting_ordinals.windows(2) {
            periods.push(report(&p, bounds[0], bounds[1], false)?);
        }
        metrics.push(Metrics {
            book_id: r.book_id.clone(),
            scenario_id: s.scenario_id.clone(),
            pv,
            supplied_history_dv01: dv01,
            physical_periods: periods,
            physical_tail: report(
                &p,
                *r.reporting_ordinals
                    .last()
                    .ok_or("funding reporting tail missing")?,
                r.known.tail_end_ordinal,
                false,
            )?,
            opening_actions: report(&p, r.as_of_ordinal, r.as_of_ordinal, true)?,
        });
    }
    Ok(ResultData {schema:SCHEMA.into(),identity:r.identity.clone(),base_anchor:anchor,metrics,native_sources:sources,
        pricing_policy:"full signed native Q funding principal/interest/fee cash only; supplied collateral discount curve and immutable original whole-book OAS; security movements, margin values, gross sales/cost/surplus are separate audits and never extra PV; paired Q down/up supplied histories with native curve bumps".into(),
        income_policy:"weighted Physical actual contractual/default/recovery cash and separate reviewed economic losses; native coupon audits and raw intervals retained, no proportional coupon allocation labeled daily earned interest, effective-yield NII, allowance or journal".into(),
        source_status:"native whole-book replay preserves exclusive owned collateral and complete explicit maturity/replacement/recovery/disposition tail; defaults remain supported only for Physical cash-lender counterparties; no legal netting/reuse/extinguishment, future fixing, replacement funding or terminal cash inferred; analysis source, not saved-book/treasury/ledger adoption".into()})
}
