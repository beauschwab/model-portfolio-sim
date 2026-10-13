use super::{admission::money, pricing, *};
use crate::products::installment::graph::{BaseAnchor, Role};
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
) -> Result<PhysicalAmounts, String> {
    let mut a = PhysicalAmounts::default();
    for p in paths {
        portfolio_compute_control::checkpoint()?;
        let w = p.weight;
        for e in p.projection.events() {
            if inside(e.payment_ordinal, left, right, opening) {
                let c = &e.components;
                add(&mut a.principal_cash, c.principal, w)?;
                add(&mut a.interest_cash, c.cash_interest, w)?;
                add(&mut a.fee_cash, c.fees, w)?;
            }
        }
        if let SourceProjection::CdSequence { result } = &p.projection {
            for receipt in &result.action_receipts {
                portfolio_compute_control::checkpoint()?;
                if inside(receipt.action.ordinal, left, right, opening) {
                    let r = &receipt.native;
                    let sign = if result.admitted_request.direction
                        == portfolio_model_core::floating_rate::Direction::Receipt
                    {
                        1.
                    } else {
                        -1.
                    };
                    add(
                        &mut a.cd_principal_extinguished,
                        r.gross_principal_removed,
                        w,
                    )?;
                    add(&mut a.cd_penalty, r.penalty, w)?;
                    add(
                        &mut a.cd_penalty_from_principal,
                        r.penalty_from_principal,
                        w,
                    )?;
                    add(
                        &mut a.cd_penalty_from_accrued_interest,
                        r.penalty_from_accrued_interest,
                        w,
                    )?;
                    add(
                        &mut a.cd_interest_accrued_through_election,
                        r.earned_interest_delta,
                        w,
                    )?;
                    add(
                        &mut a.cd_new_contract_principal,
                        r.new_contracts.iter().map(|s| s.principal).sum(),
                        w,
                    )?;
                    add(&mut a.cd_net_external_cash, r.net_external_cash, w)?;
                    add(&mut a.cd_interest_cash_paid, sign * r.interest_cash_paid, w)?;
                    add(
                        &mut a.cd_principal_cash_returned,
                        sign * r.principal_cash_returned,
                        w,
                    )?;
                }
            }
        }
        if let (
            ProductHistory::CdElection { request: q },
            SourceProjection::CdElection { result: r },
        ) = (&p.product, &p.projection)
        {
            if inside(q.election_ordinal, left, right, opening) {
                add(
                    &mut a.cd_principal_extinguished,
                    r.gross_principal_removed,
                    w,
                )?;
                add(&mut a.cd_penalty, r.penalty, w)?;
                add(
                    &mut a.cd_penalty_from_principal,
                    r.penalty_from_principal,
                    w,
                )?;
                add(
                    &mut a.cd_penalty_from_accrued_interest,
                    r.penalty_from_accrued_interest,
                    w,
                )?;
                add(
                    &mut a.cd_interest_accrued_through_election,
                    r.earned_interest_delta,
                    w,
                )?;
                add(
                    &mut a.cd_new_contract_principal,
                    r.new_contracts.iter().map(|s| s.principal).sum(),
                    w,
                )?;
                let sign = if q.direction == portfolio_model_core::floating_rate::Direction::Receipt
                {
                    1.
                } else {
                    -1.
                };
                add(&mut a.cd_net_external_cash, r.net_external_cash, w)?;
                add(&mut a.cd_interest_cash_paid, sign * r.interest_cash_paid, w)?;
                add(
                    &mut a.cd_principal_cash_returned,
                    sign * r.principal_cash_returned,
                    w,
                )?;
            }
        }
    }
    Ok(a)
}
pub(super) fn evaluate(
    r: &Request,
    sources: Vec<NativeSource>,
    anchor: BaseAnchor,
) -> Result<ResultData, String> {
    let source_index = if r.schema == SEQUENCE_SCHEMA {
        Some(pricing::SourceIndex::new(&sources)?)
    } else {
        None
    };
    let mut metrics = Vec::new();
    for (p, oas) in r.positions.iter().zip(&anchor.oas) {
        for s in &r.scenarios {
            portfolio_compute_control::checkpoint()?;
            let select = |role| {
                if let Some(index) = &source_index {
                    index.slice(&p.position_id, &s.scenario_id, role)
                } else {
                    pricing::slice(&sources, &p.position_id, &s.scenario_id, role)
                }
            };
            let q = select(Role::Pricing);
            let down = select(Role::PricingDown);
            let up = select(Role::PricingUp);
            let physical = select(Role::Physical);
            let bump = r.bump_bp / 10000.;
            let curve = &s.collateral_curve;
            let pv = pricing::weighted_price(&q, curve, r.as_of_ordinal, oas.base_oas, 0.)?;
            let dv01 = money(
                (pricing::weighted_price(&down, curve, r.as_of_ordinal, oas.base_oas, -bump)?
                    - pricing::weighted_price(&up, curve, r.as_of_ordinal, oas.base_oas, bump)?)
                    / (2. * r.bump_bp),
            )?;
            let tail = match &p.known {
                KnownProduct::CdSequence { contract } => contract.tail_end_ordinal,
                KnownProduct::IndexedTerm { contract } => contract.tail_end_ordinal,
                KnownProduct::CdElection { contract } => contract.tail_end_ordinal,
            };
            let mut periods = Vec::new();
            for bounds in r.reporting_ordinals.windows(2) {
                periods.push(report(&physical, bounds[0], bounds[1], false)?);
            }
            metrics.push(Metrics {
                position_id: p.position_id.clone(),
                scenario_id: s.scenario_id.clone(),
                pv,
                supplied_history_dv01: dv01,
                physical_periods: periods,
                physical_tail: report(
                    &physical,
                    *r.reporting_ordinals
                        .last()
                        .ok_or("term report tail missing")?,
                    tail,
                    false,
                )?,
                opening_actions: report(&physical, r.as_of_ordinal, r.as_of_ordinal, true)?,
            });
        }
    }
    let sequence_policy = if r.schema == SEQUENCE_SCHEMA {
        Some(SequenceGraphStatus {
            original_native_sequence_sources: 0,
            native_sequence_sources: sources
                .iter()
                .filter(|s| matches!(s.projection, SourceProjection::CdSequence { .. }))
                .count(),
            locked_original_oas_sources: r.positions.len(),
            oas_calibration_calls: 0,
            single_election_fallbacks: 0,
            original_quote_reconciled: true,
            joint_optimal_exercise: false,
            accounting_adoption: false,
            empirical_validation: false,
            saved_library_cohort_adoption: false,
            capital_ftp_eligible: false,
        })
    } else {
        None
    };
    let policy = if r.schema == SEQUENCE_SCHEMA {
        "complete future supplied-Q native CD sequence schedule and indexed cash, supplied collateral and immutable original locked OAS; paired down/up histories and actual-date bumps; no P-to-Q conversion"
    } else {
        "complete future declared-Q indexed-term or single-CD-election settlement plus fixed remaining tail, supplied collateral curve and original immutable per-position OAS; supplied paired down/up histories and native curve bumps, no P-to-Q conversion"
    };
    let status = if r.schema == SEQUENCE_SCHEMA {
        "raw supplied full native CD sequence states/action receipts and exclusive unsuperseded maturity cash; signed rights, explicit policies/authorized renewal terms retained; no optimal/joint/empirical/saved/ledger/treasury adoption"
    } else {
        "raw supplied policy/measure/index-source and full native audit/state/cash retained; one CD election per path and explicit fixed remaining maturity/renewal tails; negative/prepaid CD accrued rights, unowned future positions, further elections and joint optimal exercise/credit/market generation unsupported; analysis graph stage, not full library/ledger/treasury adoption"
    };
    Ok(ResultData{sequence_policy,original_native_sources:None,schema:r.schema.clone(),identity:r.identity.clone(),base_anchor:anchor,metrics,native_sources:sources,income_policy:"Physical signed native contractual cash, explicit CD penalty/face-extinction/renewal and earned-through-election diagnostics; no daily coupon proration, effective-yield NII, allowance or journal inferred".into(),pricing_policy:policy.into(),source_status:status.into()})
}
