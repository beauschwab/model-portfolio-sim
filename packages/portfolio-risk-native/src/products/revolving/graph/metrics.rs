use super::{admission::money, pricing, *};
use crate::products::installment::graph::Role;

fn in_period(date: i32, left: i32, right: i32, opening: bool) -> bool {
    if opening {
        date == left
    } else {
        date > left && date <= right
    }
}
fn add(to: &mut f64, value: f64, weight: f64) -> Result<(), String> {
    *to = money(*to + weight * value)?;
    Ok(())
}
fn physical(
    paths: &[&NativeSource],
    left: i32,
    right: i32,
    opening: bool,
) -> Result<PhysicalAmounts, String> {
    let mut a = PhysicalAmounts::default();
    for p in paths {
        let w = p.weight;
        for i in &p.projection.recognition {
            let fraction = if i.start_ordinal == i.end_ordinal {
                if in_period(i.end_ordinal, left, right, opening) {
                    1.
                } else {
                    0.
                }
            } else if opening {
                0.
            } else {
                (i.end_ordinal.min(right) - i.start_ordinal.max(left)).max(0) as f64
                    / (i.end_ordinal - i.start_ordinal) as f64
            };
            add(&mut a.gross_contract_interest, i.interest * fraction, w)?;
        }
        for e in &p.projection.cashflows {
            if !in_period(e.ordinal, left, right, opening) {
                continue;
            }
            add(&mut a.principal_cash, e.principal, w)?;
            add(&mut a.interest_cash, e.interest, w)?;
            add(&mut a.fee_cash, e.fees, w)?;
            match e.kind.as_str() {
                "draw" => add(&mut a.draw_cash, e.principal, w)?,
                "payment" => add(&mut a.principal_payment_cash, e.principal, w)?,
                "recovery" => {
                    add(&mut a.recovery_principal_cash, e.principal, w)?;
                    add(&mut a.uncovered_recovery_cost_cash, e.fees, w)?;
                }
                _ => return Err("unsupported generated card cash event kind".into()),
            }
        }
        if let Some(d) = &p.projection.default {
            if in_period(d.ordinal, left, right, opening) {
                add(&mut a.default_principal, d.principal, w)?;
                add(&mut a.default_accrued_interest, d.accrued_interest, w)?;
                add(
                    &mut a.default_recognized_accrued_interest,
                    d.recognized_accrued_interest,
                    w,
                )?;
                add(
                    &mut a.default_unrecognized_contract_interest,
                    d.unrecognized_contract_interest,
                    w,
                )?;
                add(&mut a.default_fees, d.fees, w)?;
                add(&mut a.principal_loss, d.principal_loss, w)?;
            }
            if in_period(d.recovery_ordinal, left, right, opening) {
                add(&mut a.recovery_gross, d.gross_recovery, w)?;
                add(&mut a.recovery_cost, d.recovery_cost, w)?;
                add(&mut a.recovery_net, d.net_recovery, w)?;
            }
        }
    }
    Ok(a)
}
pub(super) fn evaluate(
    r: &Request,
    sources: Vec<NativeSource>,
    anchor: crate::products::installment::graph::BaseAnchor,
) -> Result<ResultData, String> {
    let mut metrics = Vec::new();
    for (p, oas) in r.positions.iter().zip(&anchor.oas) {
        for s in &r.scenarios {
            portfolio_compute_control::checkpoint()?;
            let q = pricing::slice(&sources, &p.position_id, &s.scenario_id, Role::Pricing);
            let down = pricing::slice(&sources, &p.position_id, &s.scenario_id, Role::PricingDown);
            let up = pricing::slice(&sources, &p.position_id, &s.scenario_id, Role::PricingUp);
            let physical_paths =
                pricing::slice(&sources, &p.position_id, &s.scenario_id, Role::Physical);
            let curve = &s.collateral_curve;
            let pv = pricing::weighted_price(&q, curve, r.as_of_ordinal, oas.base_oas, 0.)?;
            let bump = r.bump_bp / 10000.;
            let dv01 = money(
                (pricing::weighted_price(&down, curve, r.as_of_ordinal, oas.base_oas, -bump)?
                    - pricing::weighted_price(&up, curve, r.as_of_ordinal, oas.base_oas, bump)?)
                    / (2. * r.bump_bp),
            )?;
            let mut periods = Vec::new();
            for bounds in r.reporting_ordinals.windows(2) {
                periods.push(physical(&physical_paths, bounds[0], bounds[1], false)?);
            }
            let last = *r
                .reporting_ordinals
                .last()
                .ok_or("missing reporting tail")?;
            let end = r.as_of_ordinal + p.contract.horizon_days as i32;
            metrics.push(Metrics {
                position_id: p.position_id.clone(),
                scenario_id: s.scenario_id.clone(),
                pv,
                supplied_history_dv01: dv01,
                physical_periods: periods,
                physical_tail: physical(&physical_paths, last, end, false)?,
                opening_action_cash: physical(
                    &physical_paths,
                    r.as_of_ordinal,
                    r.as_of_ordinal,
                    true,
                )?,
            });
        }
    }
    Ok(ResultData {schema:SCHEMA.into(),identity:r.identity.clone(),base_anchor:anchor,metrics,native_sources:sources,
        income_policy:"weighted physical supplied-history gross economic recognition, principal/interest/fees/default/recovery cash separately; no effective-yield NII, allowance or journal inference".into(),
        risk_policy:"full future Q rights discounted on supplied collateral curve plus fixed original base OAS; central native curve bump with identically paired supplied Q down/up histories; supplied APR paths are independent and no index mapping or stochastic spending/default law is inferred".into(),
        source_status:"native generated state/cash rights with raw supplied policy/source lineage; complete zero-rights/zero-limit tail required; analysis source, not saved-book, ledger, empirical calibration or treasury adoption".into()})
}
