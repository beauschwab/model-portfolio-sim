//! Dated funding, explicit rollover and exclusive collateral state. No legacy
//! constant-balance kernel, legal netting, collateral reuse or PD model implied.
use crate::products::term::contracts::indexed::{build_path, ContractPathRequest, PrincipalPlan};
use crate::products::term::contracts::{
    amount, domain, identity, near, nonnegative, text, Identity,
};
use portfolio_model_core::cashflow::*;
use portfolio_model_core::floating_rate::Direction;
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "funding-contract-1";
pub const MODEL_VERSION: &str = "funding-contracts-1";
pub const MAX_CONTRACTS: usize = 256;
pub const MAX_ASSETS: usize = 4096;
pub const MAX_WORK: usize = 250_000;
mod collateral;
mod timeline;
mod types;
mod validation;
use collateral::*;
pub use types::*;
use validation::*;

fn sign(c: &FundingContract) -> f64 {
    if c.direction == FundingDirection::Lending {
        1.
    } else {
        -1.
    }
}
fn event(
    r: &FundingRequest,
    c: &FundingContract,
    id: &str,
    date: i32,
    [p, i, fee]: [f64; 3],
    source: &Identity,
) -> DatedCashflowEvent {
    DatedCashflowEvent {
        position_id: c.position_id.clone(),
        event_id: id.into(),
        scenario: ScenarioSelector::Named {
            scenario_id: r.scenario_id.clone(),
        },
        payment_ordinal: date,
        day_offset: (date - r.asof_ordinal) as u32,
        accrual_period: None,
        currency: c.currency.clone(),
        components: CashflowComponents {
            principal: sign(c) * p,
            cash_interest: sign(c) * i,
            accrued_interest: 0.,
            book_amortization: 0.,
            fees: sign(c) * fee,
        },
        measure: r.measure,
        provenance: EventProvenance::Modeled {
            model_id: source.id.clone(),
            model_version: MODEL_VERSION.into(),
            input_revision: source.revision.clone(),
        },
        adjustment: CashflowAdjustment::None {},
    }
}
#[derive(Clone, Copy)]
enum ActionKind {
    Open,
    Margin(usize),
    Finish,
    Realize,
}
#[derive(Clone, Copy)]
struct Action {
    date: i32,
    rank: u8,
    contract: usize,
    kind: ActionKind,
}
struct Runtime {
    contract: FundingContract,
    coupon_audit: Vec<crate::products::term::contracts::indexed::PeriodAudit>,
    held: HashMap<usize, f64>,
    realized_principal: f64,
    realized_interest: f64,
}

pub fn evaluate(request: &FundingRequest) -> Result<FundingResult, String> {
    if request.schema != SCHEMA
        || request.model_version != MODEL_VERSION
        || request.monetary_unit != "currency_units"
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || request.contracts.is_empty()
        || request.contracts.len() > MAX_CONTRACTS
        || request.collateral_assets.len() > MAX_ASSETS
    {
        return Err("invalid funding schema/model/currency/unit or book admission".into());
    }
    domain(request.asof_ordinal, request.tail_end_ordinal)?;
    domain(request.asof_ordinal, request.reporting_end_ordinal)?;
    if request.reporting_end_ordinal > request.tail_end_ordinal {
        return Err("reporting horizon exceeds complete funding tail".into());
    }
    let mut bytes = 0;
    identity(&request.assumption, &mut bytes)?;
    text(&request.scenario_id, &mut bytes)?;
    let mut asset_ids = HashMap::new();
    let mut total = 0;
    for (idx, a) in request.collateral_assets.iter().enumerate() {
        for s in [&a.asset_id, &a.owner_entity_id, &a.asset_class] {
            text(s, &mut bytes)?;
        }
        identity(&a.ownership_source, &mut bytes)?;
        currency(&a.currency, &request.currency)?;
        nonnegative(a.quantity)?;
        nonnegative(a.already_encumbered_quantity)?;
        if a.already_encumbered_quantity > a.quantity
            || asset_ids.insert(a.asset_id.as_str(), idx).is_some()
        {
            return Err("invalid or duplicate global collateral inventory".into());
        }
    }
    // Expand at most one generation; neither an outage nor rollover removes the
    // old scheduled maturity payment.
    let mut contracts = request.contracts.clone();
    let mut roll_audits = Vec::new();
    for c in &request.contracts {
        if let Exit::Maturity {
            rollover: Some(roll),
        } = &c.exit
        {
            if request.measure == CashflowMeasure::Contractual {
                return Err("rollover is an explicit modeled election, not a contractual old-book extension".into());
            }
            identity(&roll.election_source, &mut bytes)?;
            nonnegative(roll.requested_principal)?;
            if roll.requested_principal > c.principal || roll.requested_principal == 0. {
                return Err(
                    "partial roll must be positive and no larger than matured principal".into(),
                );
            }
            let executed = if roll.market_access == Access::Available {
                let next = roll
                    .replacement
                    .as_ref()
                    .ok_or("available rollover requires explicit replacement terms")?;
                if next.start_ordinal != c.maturity_ordinal
                    || next.status != FundingStatus::New
                    || next.direction != c.direction
                    || next.entity_id != c.entity_id
                    || next.currency != c.currency
                    || !near(next.principal, roll.requested_principal)
                    || !matches!(next.exit, Exit::Maturity { rollover: None })
                {
                    return Err(
                        "rollover requires separate compatible new contract without recursive roll"
                            .into(),
                    );
                }
                if contracts.len() >= MAX_CONTRACTS {
                    return Err("old plus replacement contracts exceed 256-row admission".into());
                }
                contracts.push((**next).clone());
                roll.requested_principal
            } else {
                if roll.replacement.is_some() {
                    return Err(
                        "unavailable rollover must omit executable replacement contract".into(),
                    );
                }
                0.
            };
            roll_audits.push(RolloverAudit {
                position_id: c.position_id.clone(),
                ordinal: c.maturity_ordinal,
                source: roll.election_source.clone(),
                requested_principal: roll.requested_principal,
                executed_principal: executed,
                market_access_shortfall: roll.requested_principal - executed,
                replacement_position_id: roll.replacement.as_ref().map(|n| n.position_id.clone()),
            });
        }
    }
    let mut runtimes = Vec::with_capacity(contracts.len());
    let mut actions = Vec::new();
    let mut cash = Vec::new();
    let mut position_ids = HashSet::new();
    let mut snapshot_identities: HashMap<(String, String), CollateralSnapshot> = HashMap::new();
    for (idx, c) in contracts.into_iter().enumerate() {
        identity(&c.contract, &mut bytes)?;
        identity(&c.rate_path, &mut bytes)?;
        for s in [&c.position_id, &c.entity_id, &c.counterparty_id] {
            text(s, &mut bytes)?;
        }
        currency(&c.currency, &request.currency)?;
        nonnegative(c.principal)?;
        amount(c.origination_fee)?;
        domain(c.start_ordinal, c.maturity_ordinal)?;
        domain(request.asof_ordinal, c.maturity_ordinal)?;
        if c.principal == 0.
            || c.start_ordinal == c.maturity_ordinal
            || c.maturity_ordinal <= request.asof_ordinal
            || c.maturity_ordinal > request.tail_end_ordinal
            || c.entity_id == c.counterparty_id
            || !position_ids.insert(c.position_id.clone())
            || (c.status == FundingStatus::New && c.start_ordinal < request.asof_ordinal)
            || (c.status == FundingStatus::Opening
                && (c.start_ordinal > request.asof_ordinal || c.origination_fee != 0.))
            || c.interest_periods.is_empty()
            || (c.status == FundingStatus::New
                && c.interest_periods[0].start_ordinal != c.start_ordinal)
            || (c.status == FundingStatus::Opening
                && (c.interest_periods[0].start_ordinal < c.start_ordinal
                    || c.interest_periods[0].start_ordinal > request.asof_ordinal))
            || c.interest_periods.last().is_none_or(|p| {
                p.end_ordinal != c.maturity_ordinal || p.payment_ordinal != c.maturity_ordinal
            })
            || c.interest_periods
                .iter()
                .any(|p| p.payment_ordinal > c.maturity_ordinal)
        {
            return Err(
                "invalid full funding term, opening/new state, bullet maturity or identity".into(),
            );
        }
        work(c.interest_periods.len() + 1, &mut total)?;
        for p in &c.interest_periods {
            coupon_metadata(p, &mut bytes, &mut total)?;
        }
        let mut principal = vec![0.; c.interest_periods.len()];
        *principal.last_mut().ok_or("missing maturity")? = c.principal;
        let projection = build_path(&ContractPathRequest {
            schema: crate::products::term::contracts::SCHEMA.into(),
            model_version: crate::products::term::contracts::MODEL_VERSION.into(),
            contract: c.contract.clone(),
            assumption: request.assumption.clone(),
            path: c.rate_path.clone(),
            position_id: c.position_id.clone(),
            scenario_id: request.scenario_id.clone(),
            currency: c.currency.clone(),
            monetary_unit: request.monetary_unit.clone(),
            direction: if c.direction == FundingDirection::Lending {
                Direction::Receipt
            } else {
                Direction::Payment
            },
            measure: request.measure,
            asof_ordinal: request.asof_ordinal,
            reporting_end_ordinal: request.reporting_end_ordinal,
            tail_end_ordinal: request.tail_end_ordinal,
            observed_publication_cutoff_unix_seconds: request
                .observed_publication_cutoff_unix_seconds,
            opening_principal: c.principal,
            principal_plan: PrincipalPlan::Supplied {
                principal_payments: principal,
            },
            periods: c.interest_periods.clone(),
        })?;
        let default = match &c.exit {
            Exit::CounterpartyDefault { policy } => Some(policy),
            _ => None,
        };
        if c.status == FundingStatus::New {
            cash.push(event(
                request,
                &c,
                "funding-advance",
                c.start_ordinal,
                [-c.principal, 0., c.origination_fee],
                &c.contract,
            ));
        }
        for mut e in projection.schedule.events {
            if default.is_none_or(|d| e.payment_ordinal <= d.ordinal) {
                e.event_id = format!("contractual:{}", e.event_id);
                cash.push(e);
            }
        }
        let close_date = default.map_or(c.maturity_ordinal, |d| d.ordinal);
        let initial_date = if c.status == FundingStatus::Opening {
            request.asof_ordinal
        } else {
            c.start_ordinal
        };
        if c.funding_kind == FundingKind::Repo && c.collateral.is_none() {
            return Err(
                "repo requires explicit collateral and maturity; zero-duration proxy rejected"
                    .into(),
            );
        }
        if c.funding_kind != FundingKind::Repo && c.collateral.is_some() {
            return Err(
                "unsecured/interbank contract cannot silently acquire repo collateral semantics"
                    .into(),
            );
        }
        if let Some(a) = &c.collateral {
            identity(&a.policy, &mut bytes)?;
            identity(&a.initial_accrued_right_source, &mut bytes)?;
            amount(a.initial_accrued_interest_claim)?;
            nonnegative(a.minimum_transfer_value)?;
            if a.eligible_asset_classes.is_empty()
                || a.eligible_asset_classes.len() > 256
                || a.initial_pledges.is_empty()
                || a.margin_steps.len() > 4096
            {
                return Err("repo collateral policy incomplete or unbounded".into());
            }
            let mut classes = HashSet::new();
            for s in &a.eligible_asset_classes {
                text(s, &mut bytes)?;
                if !classes.insert(s) {
                    return Err("duplicate eligibility class".into());
                }
            }
            snapshot(
                &a.initial_snapshot,
                initial_date,
                &asset_ids,
                &mut bytes,
                &mut total,
            )?;
            allocations(
                &a.initial_pledges,
                &asset_ids,
                false,
                &mut bytes,
                &mut total,
            )?;
            actions.push(Action {
                date: initial_date,
                rank: if c.status == FundingStatus::Opening {
                    0
                } else {
                    2
                },
                contract: idx,
                kind: ActionKind::Open,
            });
            let mut prior = initial_date;
            for (step_idx, s) in a.margin_steps.iter().enumerate() {
                domain(initial_date, s.ordinal)?;
                identity(&s.action_source, &mut bytes)?;
                identity(&s.accrued_right_source, &mut bytes)?;
                amount(s.accrued_interest_claim)?;
                if s.ordinal <= prior || s.ordinal >= c.maturity_ordinal || s.ordinal >= close_date
                {
                    return Err(
                        "margin steps must be ordered actual dates before maturity/default close"
                            .into(),
                    );
                }
                prior = s.ordinal;
                snapshot(&s.snapshot, s.ordinal, &asset_ids, &mut bytes, &mut total)?;
                allocations(&s.transfers, &asset_ids, true, &mut bytes, &mut total)?;
                actions.push(Action {
                    date: s.ordinal,
                    rank: 3,
                    contract: idx,
                    kind: ActionKind::Margin(step_idx),
                });
            }
            let snapshots = std::iter::once(&a.initial_snapshot)
                .chain(a.margin_steps.iter().map(|s| &s.snapshot));
            for s in snapshots {
                let key = (s.source.id.clone(), s.source.revision.clone());
                if let Some(prior) = snapshot_identities.get(&key) {
                    if prior != s {
                        return Err(
                            "immutable collateral snapshot identity reused with different content"
                                .into(),
                        );
                    }
                } else {
                    snapshot_identities.insert(key, s.clone());
                }
            }
        }
        if let Some(d) = default {
            if request.measure != CashflowMeasure::Physical
                || c.direction != FundingDirection::Lending
                || d.ordinal < initial_date
                || (c.status == FundingStatus::New && d.ordinal == initial_date)
                || d.ordinal >= c.maturity_ordinal
            {
                return Err(
                    "default policy supports Physical cash-lender claims only before maturity"
                        .into(),
                );
            }
            identity(&d.policy, &mut bytes)?;
            identity(&d.accrued_right_source, &mut bytes)?;
            identity(&d.recovery_source, &mut bytes)?;
            nonnegative(d.accrued_interest_claim)?;
            nonnegative(d.economic_principal_loss)?;
            nonnegative(d.economic_interest_loss)?;
            let first = if let Some(real) = &d.collateral_realization {
                if c.collateral.is_none() {
                    return Err("collateral realization on unsecured contract".into());
                }
                identity(&real.policy, &mut bytes)?;
                domain(d.ordinal, real.ordinal)?;
                if real.ordinal > request.tail_end_ordinal
                    || real.surplus_return_ordinal != real.ordinal
                    || !real.sale_discount.is_finite()
                    || !(0.0..=1.0).contains(&real.sale_discount)
                {
                    return Err("unsupported collateral sale/surplus timing or discount".into());
                }
                nonnegative(real.sale_cost)?;
                snapshot(
                    &real.snapshot,
                    real.ordinal,
                    &asset_ids,
                    &mut bytes,
                    &mut total,
                )?;
                let key = (
                    real.snapshot.source.id.clone(),
                    real.snapshot.source.revision.clone(),
                );
                if let Some(prior) = snapshot_identities.get(&key) {
                    if prior != &real.snapshot {
                        return Err(
                            "immutable liquidation snapshot conflicts with earlier revision".into(),
                        );
                    }
                } else {
                    snapshot_identities.insert(key, real.snapshot.clone());
                }
                actions.push(Action {
                    date: real.ordinal,
                    rank: 1,
                    contract: idx,
                    kind: ActionKind::Realize,
                });
                real.ordinal
            } else {
                if c.collateral.is_some() {
                    return Err(
                        "defaulted repo requires explicit complete collateral disposition".into(),
                    );
                }
                d.ordinal
            };
            recoveries(
                &d.complete_principal_recoveries,
                first,
                request.tail_end_ordinal,
                &mut total,
            )?;
            recoveries(
                &d.complete_interest_recoveries,
                first,
                request.tail_end_ordinal,
                &mut total,
            )?;
            for (j, r) in d.complete_principal_recoveries.iter().enumerate() {
                cash.push(event(
                    request,
                    &c,
                    &format!("principal-recovery:{j}"),
                    r.ordinal,
                    [r.amount, 0., 0.],
                    &d.recovery_source,
                ));
            }
            for (j, r) in d.complete_interest_recoveries.iter().enumerate() {
                cash.push(event(
                    request,
                    &c,
                    &format!("interest-recovery:{j}"),
                    r.ordinal,
                    [0., r.amount, 0.],
                    &d.recovery_source,
                ));
            }
        } else if c.collateral.is_some() {
            actions.push(Action {
                date: c.maturity_ordinal,
                rank: 1,
                contract: idx,
                kind: ActionKind::Finish,
            });
        }
        runtimes.push(Runtime {
            contract: c,
            coupon_audit: projection.audit,
            held: HashMap::new(),
            realized_principal: 0.,
            realized_interest: 0.,
        });
    }
    work(actions.len(), &mut total)?;
    actions.sort_by_key(|a| (a.date, a.rank, a.contract));
    let timeline::Timeline {
        remaining,
        locked,
        disposed,
        reporting_locked,
        movements,
        margins,
        realizations,
    } = timeline::replay(request, &mut runtimes, actions, &asset_ids, &mut cash)?;
    let registry = &request.collateral_assets;
    let mut audits = Vec::new();
    for runtime in &runtimes {
        let c = &runtime.contract;
        let mut principal_loss = 0.;
        let mut interest_loss = 0.;
        let cash_principal;
        let mut live = 0.;
        let mut default_claim = 0.;
        let mut tail_recoveries = 0.;
        match &c.exit {
            Exit::Maturity { .. } => {
                cash_principal = c.principal;
                if c.start_ordinal <= request.reporting_end_ordinal
                    && c.maturity_ordinal > request.reporting_end_ordinal
                {
                    live = c.principal;
                }
            }
            Exit::CounterpartyDefault { policy: d } => {
                let recovery = d
                    .complete_principal_recoveries
                    .iter()
                    .map(|x| x.amount)
                    .sum::<f64>();
                let interest = d
                    .complete_interest_recoveries
                    .iter()
                    .map(|x| x.amount)
                    .sum::<f64>();
                cash_principal = amount(runtime.realized_principal + recovery)?;
                principal_loss = d.economic_principal_loss;
                interest_loss = d.economic_interest_loss;
                if !near(c.principal, cash_principal + principal_loss)
                    || !near(
                        d.accrued_interest_claim,
                        runtime.realized_interest + interest + interest_loss,
                    )
                {
                    return Err("principal/interest loss and complete residual recoveries fail separate claim conservation".into());
                }
                if d.ordinal > request.reporting_end_ordinal {
                    if c.start_ordinal <= request.reporting_end_ordinal {
                        live = c.principal;
                    }
                } else {
                    let realized = if d
                        .collateral_realization
                        .as_ref()
                        .is_some_and(|x| x.ordinal <= request.reporting_end_ordinal)
                    {
                        runtime.realized_principal
                    } else {
                        0.
                    };
                    let paid = d
                        .complete_principal_recoveries
                        .iter()
                        .filter(|x| x.ordinal <= request.reporting_end_ordinal)
                        .map(|x| x.amount)
                        .sum::<f64>();
                    default_claim = (c.principal - realized - paid).max(0.);
                    tail_recoveries = d
                        .complete_principal_recoveries
                        .iter()
                        .filter(|x| x.ordinal > request.reporting_end_ordinal)
                        .map(|x| x.amount)
                        .sum::<f64>();
                    if d.collateral_realization
                        .as_ref()
                        .is_some_and(|x| x.ordinal > request.reporting_end_ordinal)
                    {
                        tail_recoveries += runtime.realized_principal;
                    }
                }
            }
        }
        let calendar_verified = c.interest_periods.iter().all(|p| {
            matches!(
                p.coupon,
                crate::products::term::contracts::indexed::Coupon::Fixed { .. }
            )
        });
        audits.push(PrincipalAudit {
            position_id: c.position_id.clone(),
            entity_id: c.entity_id.clone(),
            counterparty_id: c.counterparty_id.clone(),
            contract: c.contract.clone(),
            rate_path: c.rate_path.clone(),
            coupon_audit: runtime.coupon_audit.clone(),
            direction: c.direction,
            funding_kind: c.funding_kind,
            opening_principal: if c.status == FundingStatus::Opening {
                c.principal
            } else {
                0.
            },
            new_advance_principal: if c.status == FundingStatus::New {
                c.principal
            } else {
                0.
            },
            cash_principal_settled: cash_principal,
            economic_principal_loss: principal_loss,
            economic_interest_loss: interest_loss,
            live_principal_after_reporting: live,
            gross_default_claim_after_reporting: default_claim,
            expected_principal_recoveries_after_reporting: tail_recoveries,
            rate_calendar_mapping_verified: calendar_verified,
        });
    }
    let mut inventory = Vec::with_capacity(registry.len());
    for (idx, a) in registry.iter().enumerate() {
        if !near(remaining[idx] + disposed[idx], a.quantity)
            || !near(locked[idx], 0.)
            || remaining[idx] < a.already_encumbered_quantity
        {
            return Err("collateral inventory/tail conservation failed".into());
        }
        inventory.push(InventoryAudit {
            asset_id: a.asset_id.clone(),
            original_quantity: a.quantity,
            disposed_quantity: disposed[idx],
            remaining_quantity: remaining[idx],
            outside_encumbered_quantity: a.already_encumbered_quantity,
            encumbered_quantity_after_reporting: reporting_locked[idx],
            encumbered_quantity_at_tail_end: locked[idx],
        });
    }
    cash.sort_by_key(|e| e.payment_ordinal);
    let schedule = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: request.asof_ordinal,
        horizon_days: (request.tail_end_ordinal - request.asof_ordinal) as u32,
        scenarios: vec![request.scenario_id.clone()],
        events: cash,
    };
    schedule.validate()?;
    Ok(FundingResult{schema:SCHEMA.into(),model_version:MODEL_VERSION.into(),assumption:request.assumption.clone(),schedule,contracts:audits,collateral_movements:movements,margin_calls:margins,collateral_realizations:realizations,rollover:roll_audits,inventory,disclosure:"Explicit dated funding/roll election and owned-collateral inventory; no constant-balance extension, FX, legal netting or collateral reuse inferred. Market and management outage affect replacement/transfer actions, never erase maturity obligations. Supplied conditional/observed coupons retain exact rate sources; indexed calendar mapping remains unverified. Defaults are Physical cash-lender counterparty claims only, separate reviewed principal/interest losses and complete recovery tails; no allowance or legal extinguishment inferred. Collateral sale/cost/surplus are audit explanations of the one net retained recovery cash event, never additional funding cash. Full maturity/recovery tails retained beyond reporting horizon.".into()})
}
