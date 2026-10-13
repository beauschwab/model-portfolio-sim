use super::{admission::money, pricing, *};
use crate::advanced_graph::Role;
fn inside(d: i32, left: i32, right: i32, opening: bool) -> bool {
    if opening {
        d == left
    } else {
        d > left && d <= right
    }
}
fn add(out: &mut f64, x: f64, w: f64) -> Result<(), String> {
    *out = money(*out + x * w)?;
    Ok(())
}
fn physical(
    paths: &[&NativeSource],
    left: i32,
    right: i32,
    opening: bool,
) -> Result<PhysicalAmounts, String> {
    let mut out = PhysicalAmounts::default();
    for p in paths {
        let w = p.weight;
        if !opening {
            for i in &p.projection.interest {
                let fraction = (i.end_ordinal.min(right) - i.start_ordinal.max(left)).max(0) as f64
                    / (i.end_ordinal - i.start_ordinal) as f64;
                add(&mut out.interest_expense, i.amount * fraction, w)?;
            }
        }
        for e in &p.projection.cashflows {
            if !inside(e.ordinal, left, right, opening) {
                continue;
            }
            add(&mut out.bank_principal_cash, e.principal, w)?;
            add(&mut out.bank_interest_cash, e.cash_interest, w)?;
            match e.kind.as_str() {
                "external_inflow" => add(&mut out.external_inflow, e.principal, w)?,
                "withdrawal" => add(&mut out.withdrawal, -e.principal, w)?,
                "closure" => add(&mut out.closure, -e.principal, w)?,
                "contractual_maturity" => add(&mut out.contractual_maturity, -e.principal, w)?,
                "assumed_terminal_payout" => {
                    add(&mut out.assumed_terminal_payout, -e.principal, w)?
                }
                "interest_payment" => {}
                _ => return Err("unknown native deposit cash event".into()),
            }
        }
        for m in &p.projection.migrations {
            if inside(m.ordinal, left, right, opening) {
                add(&mut out.migrated_principal, m.principal, w)?;
                add(&mut out.migrated_accrued_interest, m.accrued_interest, w)?;
            }
        }
    }
    Ok(out)
}
pub(super) fn evaluate(
    r: &Request,
    sources: Vec<NativeSource>,
    anchor: crate::advanced_graph::BaseAnchor,
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
            let pv = pricing::weighted(&q, &s.collateral_curve, r.as_of_ordinal, oas.base_oas, 0.)?;
            let bump = r.bump_bp / 10000.;
            let dv01 = money(
                (pricing::weighted(
                    &down,
                    &s.collateral_curve,
                    r.as_of_ordinal,
                    oas.base_oas,
                    -bump,
                )? - pricing::weighted(
                    &up,
                    &s.collateral_curve,
                    r.as_of_ordinal,
                    oas.base_oas,
                    bump,
                )?) / (2. * r.bump_bp),
            )?;
            let mut periods = Vec::new();
            for pair in r.reporting_ordinals.windows(2) {
                periods.push(physical(&physical_paths, pair[0], pair[1], false)?);
            }
            let end = physical_paths[0]
                .projection
                .points
                .last()
                .ok_or("deposit closing source required")?
                .ordinal;
            metrics.push(Metrics {
                position_id: p.position_id.clone(),
                scenario_id: s.scenario_id.clone(),
                liability_pv: pv,
                bank_cash_pv: -pv,
                supplied_history_liability_dv01: dv01,
                physical_periods: periods,
                physical_tail: physical(
                    &physical_paths,
                    *r.reporting_ordinals.last().unwrap(),
                    end,
                    false,
                )?,
                opening_action_cash: physical(
                    &physical_paths,
                    r.as_of_ordinal,
                    r.as_of_ordinal,
                    true,
                )?,
            });
        }
    }
    Ok(ResultData{schema:SCHEMA.into(),identity:r.identity.clone(),base_anchor:anchor,metrics,native_sources:sources,income_policy:"weighted physical signed deposit expense recognition and separate bank cash; migration retained as internal state movement, no effective-yield NII or journal inference".into(),risk_policy:"explicit supplied Q liability outflows minus inflows, original base OAS frozen; paired supplied Q histories plus native curve bumps, no fitted policy/effective DV01 claim".into(),source_status:"supplied assumption histories; no empirical calibration or P-to-Q inference".into()})
}
