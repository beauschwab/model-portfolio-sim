//! Separate acquired-asset cash ownership for sealed original Physical proofs.
//! Future draws keep their own cost/yield; they never enter base purchase yield.
use crate::commercial_credit_compiler::{EconomicResult, Request};
use crate::term_enhancements as term;
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcquiredCash {
    pub year: f64,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcquiredPathCash {
    pub base: Vec<AcquiredCash>,
    pub tranches: Vec<Vec<AcquiredCash>>,
    pub acquired: Vec<bool>,
}
#[derive(Debug, Serialize)]
pub(crate) struct ConditionalTrancheProof {
    pub tranche_id: String,
    pub acquisition: crate::commercial_source_income::Acquisition,
    pub draw_ordinal: i32,
    pub activation_probability: f64,
    pub conditional_cash_quote: f64,
    pub original_annual_effective_rate: f64,
    pub allocation_policy: &'static str,
}
fn push(rows: &mut Vec<AcquiredCash>, year: f64, amount: f64) -> Result<(), String> {
    term::amount(amount)?;
    if amount != 0. {
        rows.push(AcquiredCash { year, amount });
    }
    Ok(())
}
pub(crate) fn partition(
    raw: &Request,
    result: &EconomicResult,
) -> Result<AcquiredPathCash, String> {
    if result.carry.credit().result.paths.len() != 1 {
        return Err("acquisition partition requires one complete native Physical path".into());
    }
    let path = &result.carry.credit().result.paths[0];
    let n = result.draw_sources.len();
    let mut out = AcquiredPathCash {
        base: Vec::new(),
        tranches: vec![Vec::new(); n],
        acquired: vec![false; n],
    };
    let mut principal = vec![0.; n];
    let mut prior = None::<&portfolio_model_core::credit_path_income::CarryPoint>;
    let mut source_index = 0;
    let mut recovery_index = 0;
    let mut default_principal = vec![0.; n];
    let offsets: Vec<_> = result
        .draw_sources
        .iter()
        .scan(result.current_source.source().audit.len(), |offset, d| {
            let begin = *offset;
            *offset += d.current.source().audit.len();
            Some((begin, *offset))
        })
        .collect();
    let by_id: BTreeMap<_, _> = result
        .draw_sources
        .iter()
        .enumerate()
        .map(|(i, d)| (d.terms.review.id.as_str(), i))
        .collect();
    for point in result.carry.carry_points() {
        match point.kind {
            "funded_draw" => {
                let new = point.draw_tranches.iter().find(|d| {
                    by_id
                        .get(d.tranche_id.as_str())
                        .is_some_and(|i| !out.acquired[*i])
                });
                if let Some(d) = new {
                    let i = by_id[d.tranche_id.as_str()];
                    out.acquired[i] = true;
                    principal[i] = d.nominal_draw;
                } else {
                    push(&mut out.base, point.year, point.cash)?;
                }
            }
            "contractual_cash" => {
                let paid = result
                    .carry
                    .native_source_cash()
                    .get(source_index)
                    .ok_or("partition source cash missing")?;
                if let Some(id) = &paid.cash.tranche_id {
                    let i = *by_id
                        .get(id.as_str())
                        .ok_or("partition unknown tranche source")?;
                    if !out.acquired[i] {
                        return Err("partition source cash precedes acquisition".into());
                    }
                    principal[i] -= paid.cash.principal;
                    push(&mut out.tranches[i], point.year, point.cash)?;
                } else {
                    push(&mut out.base, point.year, point.cash)?;
                }
                source_index += 1;
            }
            "full_live_exit" => {
                let before = prior.ok_or("partition exit lacks native prefix")?;
                let factor = if let Some(stop) = result.native_stops.iter().find(|s| {
                    s.path_key() == path.path_key
                        && f64::from(s.ordinal() - raw.source.current.asof_ordinal) / 365.
                            == point.year
                }) {
                    stop.clean_factor()
                } else if path
                    .events
                    .iter()
                    .any(|e| e.year == point.year && e.kind == "prepayment")
                {
                    1. + raw
                        .prepayment_terms
                        .as_ref()
                        .map_or(0., |p| p.principal_premium_fraction)
                } else {
                    1.
                };
                let mut drawn_cash = 0.;
                for i in 0..n {
                    if out.acquired[i] {
                        let legal: f64 = before
                            .canonical_legal_coupons
                            .iter()
                            .filter(|c| {
                                c.native_audit_index >= offsets[i].0
                                    && c.native_audit_index < offsets[i].1
                            })
                            .map(|c| c.legal_remaining)
                            .sum();
                        let cash = principal[i] * factor + legal;
                        push(&mut out.tranches[i], point.year, cash)?;
                        drawn_cash += cash;
                        principal[i] = 0.;
                    }
                }
                push(&mut out.base, point.year, point.cash - drawn_cash)?;
            }
            "default_gross_transfer" => {
                default_principal.clone_from(&principal);
            }
            "default_actual_cash" => {
                let default = path
                    .default
                    .as_ref()
                    .ok_or("partition recovery missing default")?;
                let recovery = default
                    .recoveries
                    .get(recovery_index)
                    .ok_or("partition complete recovery missing")?;
                let origin = match default.origin {
                    portfolio_model_core::credit::CreditState::Performing => 0,
                    portfolio_model_core::credit::CreditState::Watch => 1,
                    portfolio_model_core::credit::CreditState::Delinquent => 2,
                    _ => return Err("partition default origin not live".into()),
                };
                let term = result
                    .carry
                    .raw_request()
                    .raw_credit_path
                    .intervals
                    .iter()
                    .find(|t| default.year <= t.end_year)
                    .ok_or("partition default interval missing")?;
                let weight = term.complete_recovery_tails[origin]
                    .get(recovery_index)
                    .ok_or("partition recovery weight missing")?
                    .weight;
                let mut drawn_cash = 0.;
                for i in 0..n {
                    if out.acquired[i] {
                        let interest: f64 = result
                            .carry
                            .native_coupon_recoveries()
                            .iter()
                            .filter(|r| {
                                r.recovery_index == recovery_index
                                    && r.native_audit_index >= offsets[i].0
                                    && r.native_audit_index < offsets[i].1
                            })
                            .map(|r| r.interest)
                            .sum();
                        let cash =
                            default_principal[i] * (1. - term.loss_given_default[origin]) * weight
                                + interest;
                        push(&mut out.tranches[i], point.year, cash)?;
                        drawn_cash += cash;
                    }
                }
                push(
                    &mut out.base,
                    point.year,
                    recovery.principal + recovery.interest - drawn_cash,
                )?;
                recovery_index += 1;
            }
            _ => {}
        }
        prior = Some(point);
    }
    Ok(out)
}
