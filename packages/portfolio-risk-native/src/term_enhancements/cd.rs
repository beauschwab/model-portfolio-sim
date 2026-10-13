//! Actual-date, supplied Physical CD elections. No fitted withdrawal hazard.
use super::*;
use portfolio_model_core::cashflow::*;
use portfolio_model_core::floating_rate::Direction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Retail,
    Brokered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PenaltyLimit {
    AccruedInterestOnly,
    PrincipalAndAccruedInterest,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PenaltyRule {
    None {},
    InterestDays {
        annual_penalty_rate: f64,
        days: u32,
        denominator: u32,
    },
    PrincipalFraction {
        fraction: f64,
    },
    FixedAmount {
        amount: f64,
    },
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WaiverWindow {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CdPolicy {
    pub identity: Identity,
    pub channel: Channel,
    pub early_withdrawal_permitted: bool,
    pub partial_withdrawal_permitted: bool,
    /// Explicit contract override; brokered channel does not imply liquidity.
    pub brokered_contract_allows_withdrawal: bool,
    pub earliest_withdrawal_ordinal: i32,
    pub minimum_remaining_principal: f64,
    pub penalty: PenaltyRule,
    pub penalty_limit: PenaltyLimit,
    pub penalty_cap: Option<f64>,
    /// Inclusive, contract-supplied exceptional penalty-free windows.
    pub penalty_waivers: Vec<WaiverWindow>,
    pub maturity_grace_days: u32,
    /// Explicit rate on old principal while the maturity election is deferred.
    pub grace_annual_rate: f64,
    pub renewal_permitted: bool,
    pub switching_permitted: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CdState {
    pub contract: Identity,
    pub position_id: String,
    pub principal: f64,
    pub accrued_interest: f64,
    pub last_accrual_ordinal: i32,
    pub maturity_ordinal: i32,
    pub annual_rate: f64,
    pub day_count: ActualDayCount,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenewalContract {
    pub contract: Identity,
    pub position_id: String,
    pub maturity_ordinal: i32,
    pub annual_rate: f64,
    pub day_count: ActualDayCount,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Election {
    Withdraw {
        principal: f64,
    },
    Maturity {
        cash_out_principal: f64,
        renew_principal: f64,
        switch_principal: f64,
        cash_out_interest: f64,
        renew_interest: f64,
        switch_interest: f64,
        renewal: Option<Box<RenewalContract>>,
        switching: Option<Box<RenewalContract>>,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CdElectionRequest {
    pub schema: String,
    pub model_version: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub scenario_id: String,
    pub election_source: Identity,
    pub policy: CdPolicy,
    pub opening: CdState,
    pub election_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub tail_end_ordinal: i32,
    pub election: Election,
}
#[derive(Debug, Clone, Serialize)]
pub struct CdElectionResult {
    pub schema: String,
    pub model_version: String,
    pub policy: Identity,
    pub election_source: Identity,
    pub closing: CdState,
    pub new_contracts: Vec<CdState>,
    /// Immediate constructive settlement and reinvestment, net cash funding.
    pub settlement_schedule: DatedSchedule,
    /// New fixed bullet schedules plus old remaining balance, full maturity.
    /// Separate from settlements so a caller cannot double-count renewal cash.
    pub remaining_schedule: DatedSchedule,
    pub earned_interest_delta: f64,
    pub gross_principal_removed: f64,
    pub principal_cash_returned: f64,
    pub interest_cash_paid: f64,
    pub penalty: f64,
    pub penalty_from_accrued_interest: f64,
    pub penalty_from_principal: f64,
    pub net_external_cash: f64,
    pub disclosure: String,
}
fn state(s: &CdState, bytes: &mut usize) -> Result<(), String> {
    identity(&s.contract, bytes)?;
    text(&s.position_id, bytes)?;
    nonnegative(s.principal)?;
    amount(s.accrued_interest)?;
    domain(s.last_accrual_ordinal, s.maturity_ordinal)?;
    if !s.annual_rate.is_finite() || s.principal + s.accrued_interest < 0. {
        return Err("invalid CD live principal/accrued/rate".into());
    }
    Ok(())
}
/// Pure maturity projection; no withdrawal, renewal, grace election or policy is invented.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CdTailRequest {
    pub schema: String,
    pub model_version: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub scenario_id: String,
    pub projection_source: Identity,
    pub opening: CdState,
    pub as_of_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub tail_end_ordinal: i32,
}
#[derive(Debug, Clone, Serialize)]
pub struct CdTailResult {
    pub schema: String,
    pub model_version: String,
    pub projection_source: Identity,
    pub opening: CdState,
    pub schedule: DatedSchedule,
    pub disclosure: String,
}
struct ScheduleContext<'a> {
    measure: CashflowMeasure,
    currency: &'a str,
    direction: Direction,
    scenario_id: &'a str,
    source: &'a Identity,
    as_of_ordinal: i32,
    tail_end_ordinal: i32,
}
fn context(r: &CdElectionRequest) -> ScheduleContext<'_> {
    ScheduleContext {
        measure: r.measure,
        currency: &r.currency,
        direction: r.direction,
        scenario_id: &r.scenario_id,
        source: &r.election_source,
        as_of_ordinal: r.election_ordinal,
        tail_end_ordinal: r.tail_end_ordinal,
    }
}
fn event(
    request: &CdElectionRequest,
    position: &str,
    id: &str,
    date: i32,
    components: CashflowComponents,
) -> DatedCashflowEvent {
    event_with_context(&context(request), position, id, date, components)
}
fn event_with_context(
    request: &ScheduleContext<'_>,
    position: &str,
    id: &str,
    date: i32,
    components: CashflowComponents,
) -> DatedCashflowEvent {
    let sign = if request.direction == Direction::Receipt {
        1.
    } else {
        -1.
    };
    DatedCashflowEvent {
        position_id: position.into(),
        event_id: id.into(),
        scenario: ScenarioSelector::Named {
            scenario_id: request.scenario_id.into(),
        },
        payment_ordinal: date,
        day_offset: (date - request.as_of_ordinal) as u32,
        accrual_period: None,
        currency: request.currency.into(),
        components: CashflowComponents {
            principal: sign * components.principal,
            cash_interest: sign * components.cash_interest,
            accrued_interest: sign * components.accrued_interest,
            book_amortization: sign * components.book_amortization,
            fees: sign * components.fees,
        },
        measure: request.measure,
        provenance: EventProvenance::Modeled {
            model_id: request.source.id.clone(),
            model_version: MODEL_VERSION.into(),
            input_revision: request.source.revision.clone(),
        },
        adjustment: CashflowAdjustment::None {},
    }
}
fn cash(principal: f64, interest: f64, fees: f64) -> CashflowComponents {
    CashflowComponents {
        principal,
        cash_interest: interest,
        accrued_interest: 0.,
        book_amortization: 0.,
        fees,
    }
}
fn schedule(
    request: &CdElectionRequest,
    events: Vec<DatedCashflowEvent>,
) -> Result<DatedSchedule, String> {
    schedule_with_context(&context(request), events)
}
fn schedule_with_context(
    request: &ScheduleContext<'_>,
    mut events: Vec<DatedCashflowEvent>,
) -> Result<DatedSchedule, String> {
    events.sort_by_key(|e| e.payment_ordinal);
    let s = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: request.as_of_ordinal,
        horizon_days: (request.tail_end_ordinal - request.as_of_ordinal) as u32,
        scenarios: vec![request.scenario_id.into()],
        events,
    };
    s.validate()?;
    Ok(s)
}
fn tail(request: &CdElectionRequest, s: &CdState, id: &str) -> Result<DatedCashflowEvent, String> {
    tail_with_context(&context(request), s, id)
}
/// Validates and projects the fixed bullet right to its actual maturity exactly once.
/// Physical/Pricing are supplied source labels, not a conversion between measures.
pub fn project_cd_tail(r: &CdTailRequest) -> Result<CdTailResult, String> {
    header(&r.schema, &r.model_version, &r.currency, &r.monetary_unit)?;
    domain(r.as_of_ordinal, r.tail_end_ordinal)?;
    domain(r.as_of_ordinal, r.reporting_end_ordinal)?;
    let mut bytes = 0;
    identity(&r.projection_source, &mut bytes)?;
    text(&r.scenario_id, &mut bytes)?;
    state(&r.opening, &mut bytes)?;
    if !matches!(
        r.measure,
        CashflowMeasure::Physical | CashflowMeasure::Pricing
    ) || r.opening.last_accrual_ordinal != r.as_of_ordinal
        || r.as_of_ordinal > r.opening.maturity_ordinal
        || r.opening.maturity_ordinal > r.tail_end_ordinal
        || r.reporting_end_ordinal > r.tail_end_ordinal
    {
        return Err("CD no-election tail requires explicit source measure, current accrual anchor and complete maturity; past-maturity grace needs an election policy".into());
    }
    let ctx = ScheduleContext {
        measure: r.measure,
        currency: &r.currency,
        direction: r.direction,
        scenario_id: &r.scenario_id,
        source: &r.projection_source,
        as_of_ordinal: r.as_of_ordinal,
        tail_end_ordinal: r.tail_end_ordinal,
    };
    let event = tail_with_context(&ctx, &r.opening, "cd-no-election-maturity")?;
    Ok(CdTailResult {schema:SCHEMA.into(),model_version:MODEL_VERSION.into(),
        projection_source:r.projection_source.clone(),opening:r.opening.clone(),
        schedule:schedule_with_context(&ctx,vec![event])?,
        disclosure:"Explicit fixed bullet maturity projection with signed accrued rights. No election, grace extension, compounding, automatic renewal, penalty, behavior fit or accounting recognition inferred.".into()})
}
fn tail_with_context(
    request: &ScheduleContext<'_>,
    s: &CdState,
    id: &str,
) -> Result<DatedCashflowEvent, String> {
    let interest = amount(
        s.accrued_interest
            + s.principal
                * s.annual_rate
                * actual_year_fraction(s.last_accrual_ordinal, s.maturity_ordinal, s.day_count)?,
    )?;
    if s.principal + interest < 0. {
        return Err("CD maturity principal plus interest would be negative".into());
    }
    let mut e = event_with_context(
        request,
        &s.position_id,
        id,
        s.maturity_ordinal,
        cash(s.principal, interest, 0.),
    );
    if s.last_accrual_ordinal < s.maturity_ordinal {
        e.accrual_period = Some(AccrualPeriod {
            start_ordinal: s.last_accrual_ordinal,
            end_ordinal: s.maturity_ordinal,
        });
    }
    Ok(e)
}
fn signed_allocation(total: f64, values: [f64; 3]) -> Result<(), String> {
    for v in values {
        amount(v)?;
        if (total >= 0. && v < 0.) || (total < 0. && v > 0.) {
            return Err("interest elections must preserve signed accrued allocations".into());
        }
    }
    if !near(total, values.iter().sum()) {
        return Err("maturity interest allocations do not conserve accrued rights".into());
    }
    Ok(())
}
pub fn step_cd(request: &CdElectionRequest) -> Result<CdElectionResult, String> {
    step_with_measure(request, CashflowMeasure::Physical)
}

/// Explicit supplied Pricing election. This does not fit or convert a Physical
/// policy/probability into a pricing measure; the caller must supply that source.
pub fn step_cd_pricing(request: &CdElectionRequest) -> Result<CdElectionResult, String> {
    step_with_measure(request, CashflowMeasure::Pricing)
}

fn step_with_measure(
    request: &CdElectionRequest,
    expected_measure: CashflowMeasure,
) -> Result<CdElectionResult, String> {
    header(
        &request.schema,
        &request.model_version,
        &request.currency,
        &request.monetary_unit,
    )?;
    domain(request.election_ordinal, request.tail_end_ordinal)?;
    domain(request.election_ordinal, request.reporting_end_ordinal)?;
    let mut bytes = 0;
    identity(&request.policy.identity, &mut bytes)?;
    identity(&request.election_source, &mut bytes)?;
    text(&request.scenario_id, &mut bytes)?;
    state(&request.opening, &mut bytes)?;
    let s = &request.opening;
    let p = &request.policy;
    validate_ordinal(p.earliest_withdrawal_ordinal)?;
    nonnegative(p.minimum_remaining_principal)?;
    if let Some(v) = p.penalty_cap {
        nonnegative(v)?;
    }
    if request.measure != expected_measure
        || request.election_ordinal < s.last_accrual_ordinal
        || request.reporting_end_ordinal > request.tail_end_ordinal
        || p.maturity_grace_days > 366
        || p.penalty_waivers.len() > 256
        || !p.grace_annual_rate.is_finite()
        || i64::from(request.election_ordinal)
            > i64::from(s.maturity_ordinal) + i64::from(p.maturity_grace_days)
        || s.maturity_ordinal > request.tail_end_ordinal
        || s.principal == 0.
    {
        return Err(if expected_measure==CashflowMeasure::Physical {"CD election requires live Physical state, explicit grace and complete bounded maturity tail"} else {"CD pricing election requires explicitly declared Pricing state, grace and complete bounded maturity tail"}.into());
    }
    let mut prior = None;
    for w in &p.penalty_waivers {
        domain(w.start_ordinal, w.end_ordinal)?;
        if prior.is_some_and(|e| e >= w.start_ordinal) {
            return Err("penalty waiver windows overlap or are unordered".into());
        }
        prior = Some(w.end_ordinal);
    }
    let penalty_rate = match p.penalty {
        PenaltyRule::None {} => 0.,
        PenaltyRule::InterestDays {
            annual_penalty_rate,
            days,
            denominator,
        } => {
            nonnegative(annual_penalty_rate)?;
            if days > 36_600 || !matches!(denominator, 360 | 365) {
                return Err("penalty days/denominator outside explicit supported domain".into());
            }
            annual_penalty_rate * f64::from(days) / f64::from(denominator)
        }
        PenaltyRule::PrincipalFraction { fraction } => nonnegative(fraction)?,
        PenaltyRule::FixedAmount { amount: v } => {
            nonnegative(v)?;
            0.
        }
    };
    let accrual_end = request.election_ordinal.min(s.maturity_ordinal);
    let regular = if s.last_accrual_ordinal < accrual_end {
        s.principal
            * s.annual_rate
            * actual_year_fraction(s.last_accrual_ordinal, accrual_end, s.day_count)?
    } else {
        0.
    };
    let grace = if request.election_ordinal > s.maturity_ordinal {
        s.principal
            * p.grace_annual_rate
            * actual_year_fraction(
                s.last_accrual_ordinal.max(s.maturity_ordinal),
                request.election_ordinal,
                s.day_count,
            )?
    } else {
        0.
    };
    let delta = amount(regular + grace)?;
    let accrued = amount(s.accrued_interest + delta)?;
    if s.principal + accrued < 0. {
        return Err("CD earned accrual exceeds nonnegative total claim domain".into());
    }
    let mut closing = s.clone();
    closing.accrued_interest = accrued;
    closing.last_accrual_ordinal = request.election_ordinal;
    let mut settlements = Vec::new();
    let mut tails = Vec::new();
    let mut new_contracts = Vec::new();
    let removed;
    let principal_cash;
    let interest_cash;
    let mut penalty = 0.;
    let mut interest_penalty = 0.;
    let mut principal_penalty = 0.;
    match &request.election {
        Election::Withdraw { principal } => {
            nonnegative(*principal)?;
            if *principal == 0.
                || *principal > s.principal
                || request.election_ordinal >= s.maturity_ordinal
                || !p.early_withdrawal_permitted
                || (p.channel == Channel::Brokered && !p.brokered_contract_allows_withdrawal)
                || request.election_ordinal < p.earliest_withdrawal_ordinal
                || (*principal < s.principal && !p.partial_withdrawal_permitted)
                || (*principal < s.principal
                    && s.principal - principal < p.minimum_remaining_principal)
            {
                return Err(
                    "withdrawal violates exact-date/channel/partial/minimum-balance contract"
                        .into(),
                );
            }
            let allocated = amount(accrued * principal / s.principal)?;
            let waived = p.penalty_waivers.iter().any(|w| {
                w.start_ordinal <= request.election_ordinal
                    && request.election_ordinal <= w.end_ordinal
            });
            if !waived {
                penalty = match p.penalty {
                    PenaltyRule::FixedAmount { amount: v } => v,
                    _ => amount(principal * penalty_rate)?,
                };
                if let Some(cap) = p.penalty_cap {
                    penalty = penalty.min(cap);
                }
                let limit = match p.penalty_limit {
                    PenaltyLimit::AccruedInterestOnly => allocated.max(0.),
                    PenaltyLimit::PrincipalAndAccruedInterest => (principal + allocated).max(0.),
                };
                penalty = penalty.min(limit);
            }
            interest_penalty = penalty.min(allocated.max(0.));
            principal_penalty = penalty - interest_penalty;
            removed = *principal;
            principal_cash = principal - principal_penalty;
            interest_cash = allocated - interest_penalty;
            closing.principal -= principal;
            closing.accrued_interest -= allocated;
            // Gross extinguished face and interest, plus a separately stated fee,
            // reconcile to the net external payment without principal ambiguity.
            settlements.push(event(
                request,
                &s.position_id,
                "cd-withdrawal",
                request.election_ordinal,
                cash(*principal, allocated, -penalty),
            ));
            if closing.principal > 0. {
                tails.push(tail(request, &closing, "cd-remaining-maturity")?);
            }
        }
        Election::Maturity {
            cash_out_principal,
            renew_principal,
            switch_principal,
            cash_out_interest,
            renew_interest,
            switch_interest,
            renewal,
            switching,
        } => {
            if request.election_ordinal < s.maturity_ordinal {
                return Err("maturity election before actual maturity".into());
            }
            for v in [cash_out_principal, renew_principal, switch_principal] {
                nonnegative(*v)?;
            }
            if !near(
                cash_out_principal + renew_principal + switch_principal,
                s.principal,
            ) {
                return Err(
                    "cash-out plus renewal plus switch must conserve matured principal".into(),
                );
            }
            signed_allocation(
                accrued,
                [*cash_out_interest, *renew_interest, *switch_interest],
            )?;
            settlements.push(event(
                request,
                &s.position_id,
                "cd-maturity-settlement",
                request.election_ordinal,
                cash(s.principal, accrued, 0.),
            ));
            let mut target_ids = std::collections::HashSet::new();
            target_ids.insert(s.position_id.as_str());
            for (principal, interest, contract, permitted, label) in [
                (
                    *renew_principal,
                    *renew_interest,
                    renewal,
                    p.renewal_permitted,
                    "cd-renewal",
                ),
                (
                    *switch_principal,
                    *switch_interest,
                    switching,
                    p.switching_permitted,
                    "cd-switch",
                ),
            ] {
                let new_principal = amount(principal + interest)?;
                if new_principal < 0. {
                    return Err(
                        "reinvested principal including signed interest cannot be negative".into(),
                    );
                }
                if principal != 0. || interest != 0. {
                    let c = contract.as_ref().ok_or(
                        "positive renewal/switch allocation requires explicit new contract",
                    )?;
                    if !permitted
                        || c.maturity_ordinal <= request.election_ordinal
                        || c.maturity_ordinal > request.tail_end_ordinal
                        || !target_ids.insert(c.position_id.as_str())
                    {
                        return Err("renewal/switch permission, target identity or full tail admission failed".into());
                    }
                    let next = CdState {
                        contract: c.contract.clone(),
                        position_id: c.position_id.clone(),
                        principal: new_principal,
                        accrued_interest: 0.,
                        last_accrual_ordinal: request.election_ordinal,
                        maturity_ordinal: c.maturity_ordinal,
                        annual_rate: c.annual_rate,
                        day_count: c.day_count,
                    };
                    state(&next, &mut bytes)?;
                    settlements.push(event(
                        request,
                        &next.position_id,
                        label,
                        request.election_ordinal,
                        cash(-new_principal, 0., 0.),
                    ));
                    tails.push(tail(request, &next, &format!("{label}-maturity"))?);
                    new_contracts.push(next);
                } else if contract.is_some() {
                    return Err("unused renewal/switch contract is contradictory".into());
                }
            }
            removed = s.principal;
            principal_cash = *cash_out_principal;
            interest_cash = *cash_out_interest;
            closing.principal = 0.;
            closing.accrued_interest = 0.;
        }
    }
    let settlement_schedule = schedule(request, settlements)?;
    let remaining_schedule = schedule(request, tails)?;
    let sign = if request.direction == Direction::Receipt {
        1.
    } else {
        -1.
    };
    let net = amount(
        settlement_schedule
            .events
            .iter()
            .map(|e| e.components.cash_total())
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum(),
    )?;
    if !near(sign * net, principal_cash + interest_cash) {
        return Err("CD settlement/reinvestment net cash failed independent reconciliation".into());
    }
    let physical_disclosure="Supplied Physical election, not observed transaction or calibrated behavior. Penalty is separated into forfeited accrued interest and principal-funded fee; gross face extinction is distinct from net principal returned. Grace interest and renewal/switch contracts are explicit. Renewal funding uses same-date gross old settlement and separate reinvestment with one net external cash amount. Remaining fixed bullet tails are separate, complete and untruncated; no future elections, automatic renewal or accounting recognition is inferred.";
    let disclosure = if expected_measure == CashflowMeasure::Physical {
        physical_disclosure.into()
    } else {
        format!("Explicit supplied Pricing election/probabilities; no Physical-to-Pricing conversion or calibrated behavior. {}",physical_disclosure.split_once(". ").ok_or("CD disclosure boundary missing")?.1)
    };
    Ok(CdElectionResult {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        policy: p.identity.clone(),
        election_source: request.election_source.clone(),
        closing,
        new_contracts,
        settlement_schedule,
        remaining_schedule,
        earned_interest_delta: delta,
        gross_principal_removed: removed,
        principal_cash_returned: principal_cash,
        interest_cash_paid: interest_cash,
        penalty,
        penalty_from_accrued_interest: interest_penalty,
        penalty_from_principal: principal_penalty,
        net_external_cash: net,
        disclosure,
    })
}
