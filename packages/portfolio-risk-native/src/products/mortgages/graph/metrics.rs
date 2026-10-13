use super::{admission::money, pricing, *};
fn in_period(date: i32, left: i32, right: i32) -> bool {
    date > left && date <= right
}
fn add(to: &mut f64, value: f64, weight: f64) -> Result<(), String> {
    *to = money(*to + weight * value)?;
    Ok(())
}
fn physical(paths: &[&NativeSource], left: i32, right: i32) -> Result<PhysicalAmounts, String> {
    let mut amount = PhysicalAmounts::default();
    for p in paths {
        let w = p.weight;
        for recognition in &p.projection.recognition {
            let fraction = if recognition.start_ordinal == recognition.end_ordinal {
                if in_period(recognition.end_ordinal, left, right) {
                    1.
                } else {
                    0.
                }
            } else {
                (recognition.end_ordinal.min(right) - recognition.start_ordinal.max(left)).max(0)
                    as f64
                    / (recognition.end_ordinal - recognition.start_ordinal) as f64
            };
            add(
                &mut amount.gross_recognized_investor_interest,
                recognition.interest * fraction,
                w,
            )?;
        }
        for event in &p.projection.borrower_cashflows {
            if !in_period(event.ordinal, left, right) {
                continue;
            }
            add(&mut amount.borrower_principal_cash, event.principal, w)?;
            add(&mut amount.borrower_interest_cash, event.interest, w)?;
            if event.kind == "borrower_default_recovery" {
                add(
                    &mut amount.borrower_recovery_principal_cash,
                    event.principal,
                    w,
                )?;
            }
        }
        for event in &p.projection.cashflows {
            if !in_period(event.ordinal, left, right) {
                continue;
            }
            add(&mut amount.investor_principal_cash, event.principal, w)?;
            add(&mut amount.investor_interest_cash, event.interest, w)?;
            match event.kind.as_str() {
                "whole_loan_default_recovery" => add(
                    &mut amount.whole_loan_recovery_principal_cash,
                    event.principal,
                    w,
                )?,
                "agency_default_buyout" => {
                    add(&mut amount.agency_buyout_principal, event.principal, w)?;
                    add(&mut amount.agency_buyout_interest, event.interest, w)?;
                }
                "agency_default_guaranteed_coupon" => add(
                    &mut amount.agency_default_guaranteed_coupon_interest,
                    event.interest,
                    w,
                )?,
                "scheduled_investor_entitlement" | "turnover" | "refinance" => {}
                _ => return Err("unsupported generated mortgage investor cash kind".into()),
            }
        }
        for event in &p.projection.entitlements {
            if in_period(event.entitlement_ordinal, left, right) {
                add(&mut amount.principal_entitlement, event.principal, w)?;
                add(&mut amount.interest_entitlement, event.interest, w)?;
            }
        }
        for claim in &p.projection.defaults {
            if in_period(claim.default_ordinal, left, right) {
                add(
                    &mut amount.borrower_default_principal,
                    claim.borrower_principal,
                    w,
                )?;
                add(
                    &mut amount.investor_default_principal,
                    claim.investor_principal,
                    w,
                )?;
                add(
                    &mut amount.borrower_default_contract_interest,
                    claim.borrower_contract_interest,
                    w,
                )?;
                add(
                    &mut amount.investor_default_accrued_interest,
                    claim.investor_accrued_interest,
                    w,
                )?;
                add(
                    &mut amount.borrower_principal_loss,
                    claim.borrower_principal_loss,
                    w,
                )?;
            }
        }
    }
    Ok(amount)
}
fn totals(paths: &[&NativeSource]) -> Result<PhysicalTotals, String> {
    let mut out = PhysicalTotals::default();
    for p in paths {
        let r = &p.projection;
        let w = p.weight;
        for (to, value) in [
            (
                &mut out.borrower_scheduled_principal,
                r.borrower_scheduled_principal,
            ),
            (
                &mut out.borrower_turnover_principal,
                r.borrower_turnover_principal,
            ),
            (
                &mut out.borrower_refinance_principal,
                r.borrower_refinance_principal,
            ),
            (
                &mut out.investor_scheduled_principal,
                r.investor_scheduled_principal,
            ),
            (
                &mut out.investor_turnover_principal,
                r.investor_turnover_principal,
            ),
            (
                &mut out.investor_refinance_principal,
                r.investor_refinance_principal,
            ),
            (
                &mut out.guarantee_principal_advance,
                r.guarantee_principal_advance,
            ),
            (
                &mut out.guarantee_interest_advance,
                r.guarantee_interest_advance,
            ),
        ] {
            add(to, value, w)?;
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
            let pv = pricing::weighted_price(
                &q,
                &s.collateral_curve,
                r.as_of_ordinal,
                oas.base_oas,
                0.,
            )?;
            let bump = r.bump_bp / 10000.;
            let supplied_history_dv01 = money(
                (pricing::weighted_price(
                    &down,
                    &s.collateral_curve,
                    r.as_of_ordinal,
                    oas.base_oas,
                    -bump,
                )? - pricing::weighted_price(
                    &up,
                    &s.collateral_curve,
                    r.as_of_ordinal,
                    oas.base_oas,
                    bump,
                )?) / (2. * r.bump_bp),
            )?;
            let mut periods = Vec::new();
            for bounds in r.reporting_ordinals.windows(2) {
                periods.push(physical(&physical_paths, bounds[0], bounds[1])?);
            }
            let last = *r
                .reporting_ordinals
                .last()
                .ok_or("missing mortgage reporting tail")?;
            let end = physical_paths
                .iter()
                .flat_map(|p| {
                    p.projection
                        .cashflows
                        .iter()
                        .map(|e| e.ordinal)
                        .chain(p.projection.borrower_cashflows.iter().map(|e| e.ordinal))
                        .chain(
                            p.projection
                                .entitlements
                                .iter()
                                .map(|e| e.entitlement_ordinal),
                        )
                        .chain(p.projection.recognition.iter().map(|e| e.end_ordinal))
                        .chain(p.projection.points.iter().map(|e| e.ordinal))
                })
                .max()
                .unwrap_or(last)
                .max(last);
            metrics.push(Metrics {
                position_id: p.position_id.clone(),
                scenario_id: s.scenario_id.clone(),
                pv,
                supplied_history_dv01,
                physical_periods: periods,
                physical_tail: physical(&physical_paths, last, end)?,
                physical_totals: totals(&physical_paths)?,
            });
        }
    }
    Ok(ResultData{schema:SCHEMA.into(),identity:r.identity.clone(),base_anchor:anchor,metrics,native_sources:sources,
        income_policy:"weighted physical native investor recognized note/cash-basis interest, actual borrower versus investor cash, entitlement/default/loss/advance evidence; uniform allocation of explicit recognition intervals; no effective-yield NII, allowance, provisioning or ledger inference".into(),
        risk_policy:"complete generated investor Q remittances on supplied collateral curve plus original frozen base OAS; central curve bump with explicitly paired supplied Q path/reset histories; no stochastic draw consumption or P-to-Q conversion".into(),
        source_status:"native supplied mortgage state graph, retained full borrower/investor/ARM/default/guarantee sources; persist complete raw Request alongside result; supplied interval-end behavior and fixed recovery/agency assumptions, not fitted HPI/geographic credit, automatic legacy, cohort or ledger adoption".into()})
}
