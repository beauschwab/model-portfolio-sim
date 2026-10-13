//! Chronological collateral actions, outage calls and realization state.
use super::*;

pub(super) struct Timeline {
    pub remaining: Vec<f64>,
    pub locked: Vec<f64>,
    pub disposed: Vec<f64>,
    pub reporting_locked: Vec<f64>,
    pub movements: Vec<CollateralMovement>,
    pub margins: Vec<MarginAudit>,
    pub realizations: Vec<RealizationAudit>,
}
pub(super) fn replay(
    request: &FundingRequest,
    runtimes: &mut [Runtime],
    actions: Vec<Action>,
    asset_ids: &HashMap<&str, usize>,
    cash: &mut Vec<DatedCashflowEvent>,
) -> Result<Timeline, String> {
    let registry = &request.collateral_assets;
    let mut remaining: Vec<f64> = registry.iter().map(|a| a.quantity).collect();
    let mut locked = vec![0.; registry.len()];
    let mut disposed = vec![0.; registry.len()];
    let mut reporting_locked = None;
    let mut movements = Vec::new();
    let mut margins = Vec::new();
    let mut realizations = Vec::new();
    for action in actions {
        if action.date > request.reporting_end_ordinal && reporting_locked.is_none() {
            reporting_locked = Some(locked.clone());
        }
        let runtime = &mut runtimes[action.contract];
        match action.kind {
            ActionKind::Open => {
                let a = runtime
                    .contract
                    .collateral
                    .as_ref()
                    .ok_or("agreement missing")?
                    .clone();
                for allocation in &a.initial_pledges {
                    transfer(
                        runtime,
                        allocation,
                        action.date,
                        &a.policy,
                        "pledge",
                        &a.initial_snapshot,
                        registry,
                        asset_ids,
                        &remaining,
                        &mut locked,
                        &mut movements,
                    )?;
                }
                let (_, value) = credited(runtime, &a.initial_snapshot, registry, asset_ids)?;
                let required = amount(
                    runtime.contract.principal
                        + if a.include_accrued_interest {
                            a.initial_accrued_interest_claim
                        } else {
                            0.
                        },
                )?;
                if required < 0. || (value < required && !near(value, required)) {
                    return Err(
                        "initial repo collateral does not cover haircut-adjusted exposure".into(),
                    );
                }
            }
            ActionKind::Margin(step_idx) => {
                let (s, include_accrued, margin_breach, minimum_transfer) = {
                    let a = runtime
                        .contract
                        .collateral
                        .as_ref()
                        .ok_or("agreement missing")?;
                    (
                        a.margin_steps[step_idx].clone(),
                        a.include_accrued_interest,
                        a.margin_breach,
                        a.minimum_transfer_value,
                    )
                };
                let (_, before) = credited(runtime, &s.snapshot, registry, asset_ids)?;
                let required = amount(
                    runtime.contract.principal
                        + if include_accrued {
                            s.accrued_interest_claim
                        } else {
                            0.
                        },
                )?;
                if required < 0. {
                    return Err("margin exposure cannot be negative".into());
                }
                let mut executed = Vec::new();
                if s.management_access == Access::Available {
                    // Release first within the atomic, failure-only result. Final
                    // coverage is checked after all explicit substitution legs.
                    let mut transfers = s.transfers.clone();
                    transfers.sort_by(|x, y| x.quantity.total_cmp(&y.quantity));
                    for t in &transfers {
                        transfer(
                            runtime,
                            t,
                            action.date,
                            &s.action_source,
                            "margin-transfer",
                            &s.snapshot,
                            registry,
                            asset_ids,
                            &remaining,
                            &mut locked,
                            &mut movements,
                        )?;
                        executed.push(t.clone());
                    }
                }
                let (_, after) = credited(runtime, &s.snapshot, registry, asset_ids)?;
                let gap = (required - after).max(0.);
                if margin_breach == MarginBreach::Reject
                    && gap > minimum_transfer
                    && !near(gap, minimum_transfer)
                {
                    return Err("unmet explicit repo margin call".into());
                }
                let before_gap = (required - before).max(0.);
                margins.push(MarginAudit {
                    position_id: runtime.contract.position_id.clone(),
                    ordinal: action.date,
                    snapshot: s.snapshot.source.clone(),
                    required_exposure: required,
                    eligible_value_before: before,
                    eligible_value_after: after,
                    margin_call_value: if before_gap > minimum_transfer {
                        before_gap
                    } else {
                        0.
                    },
                    unmet_margin_value: gap,
                    management_access: s.management_access,
                    requested_transfers: s.transfers.clone(),
                    executed_transfers: executed,
                });
            }
            ActionKind::Finish => {
                let source = runtime.contract.contract.clone();
                let mut held: Vec<(usize, f64)> =
                    runtime.held.iter().map(|(&i, &q)| (i, q)).collect();
                held.sort_by_key(|(idx, _)| *idx);
                for (idx, q) in held {
                    locked[idx] -= q;
                    runtime.held.insert(idx, 0.);
                    movements.push(CollateralMovement {
                        position_id: runtime.contract.position_id.clone(),
                        ordinal: action.date,
                        asset_id: registry[idx].asset_id.clone(),
                        owner_entity_id: registry[idx].owner_entity_id.clone(),
                        quantity_delta: -q,
                        disposition: "maturity-release".into(),
                        source: source.clone(),
                    });
                }
            }
            ActionKind::Realize => {
                let d = match &runtime.contract.exit {
                    Exit::CounterpartyDefault { policy } => policy.clone(),
                    _ => return Err("default policy missing".into()),
                };
                let real = d
                    .collateral_realization
                    .as_ref()
                    .ok_or("realization missing")?;
                let (raw, _) = credited(runtime, &real.snapshot, registry, asset_ids)?;
                let gross = amount(raw * (1. - real.sale_discount))?;
                if real.sale_cost > gross {
                    return Err("collateral liquidation cost exceeds proceeds".into());
                }
                let net = gross - real.sale_cost;
                let (p, i) = if real.waterfall == Waterfall::PrincipalThenInterest {
                    let p = net.min(runtime.contract.principal);
                    (p, (net - p).min(d.accrued_interest_claim))
                } else {
                    let i = net.min(d.accrued_interest_claim);
                    ((net - i).min(runtime.contract.principal), i)
                };
                runtime.realized_principal = p;
                runtime.realized_interest = i;
                cash.push(event(
                    request,
                    &runtime.contract,
                    "collateral-retained-recovery",
                    action.date,
                    [p, i, 0.],
                    &real.policy,
                ));
                realizations.push(RealizationAudit {
                    position_id: runtime.contract.position_id.clone(),
                    ordinal: action.date,
                    policy: real.policy.clone(),
                    collateral_market_value: raw,
                    gross_sale_proceeds: gross,
                    sale_cost: real.sale_cost,
                    net_sale_proceeds: net,
                    principal_recovery: p,
                    interest_recovery: i,
                    surplus_returned_to_owner: net - p - i,
                    cash_already_in_funding_schedule: true,
                });
                let mut held: Vec<(usize, f64)> =
                    runtime.held.iter().map(|(&idx, &q)| (idx, q)).collect();
                held.sort_by_key(|(idx, _)| *idx);
                for (idx, q) in held {
                    locked[idx] -= q;
                    remaining[idx] -= q;
                    disposed[idx] += q;
                    runtime.held.insert(idx, 0.);
                    movements.push(CollateralMovement {
                        position_id: runtime.contract.position_id.clone(),
                        ordinal: action.date,
                        asset_id: registry[idx].asset_id.clone(),
                        owner_entity_id: registry[idx].owner_entity_id.clone(),
                        quantity_delta: -q,
                        disposition: "liquidated-not-released-for-reuse".into(),
                        source: real.policy.clone(),
                    });
                }
            }
        }
    }
    let reporting_locked = reporting_locked.unwrap_or_else(|| locked.clone());
    Ok(Timeline {
        remaining,
        locked,
        disposed,
        reporting_locked,
        movements,
        margins,
        realizations,
    })
}
