//! Deterministic relationship/event admission; every unsupported case refuses explicitly.
use super::types::*;
use crate::products::derivatives::accounting::{self as derivative_accounting, CouponRecognition};
use crate::products::derivatives::contract::{
    amount,
    csa::{CsaRequest, Trade},
    domain, identity, near,
    swaps::{PaymentTiming, Side, SwapRequest},
    text, Operation,
};
use crate::products::term::contracts::indexed::Coupon;
use portfolio_model_core::cashflow::validate_ordinal;
use std::collections::{BTreeMap, HashSet};

/// Where a member's hypothetical derivative for one index lives in the raw relationship.
#[derive(Clone, Copy)]
pub(super) enum HypSource {
    Base,
    Rate(usize),
    Group(usize),
}
pub(super) struct HypRef {
    pub source: HypSource,
    /// Node ordinal -> index in that hypothetical's `nodes`.
    pub nodes: BTreeMap<i32, usize>,
}
/// One hedged forecast exposure of a cash-flow relationship (a group has several).
pub(super) struct CompiledMember {
    pub exposure: usize,
    /// Exposure periods starting at or after inception, in order.
    pub hedged_periods: Vec<usize>,
    /// Hypothetical per documented index; empty when matched critical terms are elected.
    pub hypotheticals: BTreeMap<String, HypRef>,
}
pub(super) struct CompiledRelationship {
    pub item: Option<usize>,
    pub direction_sign: f64,
    pub members: Vec<CompiledMember>,
    pub matched: bool,
    pub predecessor: Option<usize>,
    pub similar_risk: Option<SimilarRiskMethod>,
    pub choose_your_rate: bool,
}
pub(super) struct Compiled {
    pub relationships: Vec<CompiledRelationship>,
}

pub(super) fn csa(r: &HedgeAccountingRequest) -> Result<&CsaRequest, String> {
    match &r.derivative.derivative.operation {
        Operation::Netting { request } => Ok(request),
        _ => Err(
            "hedge accounting requires the complete raw netting/CSA derivative lifecycle".into(),
        ),
    }
}
pub(super) fn opening_swap<'a>(
    c: &'a CsaRequest,
    position: &str,
) -> Result<&'a SwapRequest, String> {
    let node = c.paths[0]
        .nodes
        .first()
        .ok_or("derivative path lacks an opening node")?;
    for trade in &node.trades {
        match trade {
            Trade::Swap { request } if request.position_id == position => return Ok(request),
            Trade::Swap { .. } => {}
            Trade::EuropeanSwaption { request } if request.position_id == position => {
                return Err("option hedging instruments require their own reviewed lane".into())
            }
            Trade::ExpiredEuropean { terminal, .. } if terminal.position_id == position => {
                return Err("option hedging instruments require their own reviewed lane".into())
            }
            _ => {}
        }
    }
    Err("hedging instrument is not a mapped raw swap in the derivative source".into())
}
pub(super) fn swap_work(s: &SwapRequest) -> usize {
    s.floating_periods
        .iter()
        .fold(s.fixed_periods.len(), |w, p| {
            w.saturating_add(1).saturating_add(match &p.coupon {
                Coupon::Indexed { observations, .. } => observations.len(),
                _ => 0,
            })
        })
}
fn periods(periods: &[Period], asof: i32, bytes: &mut usize, label: &str) -> Result<(), String> {
    if periods.is_empty() || periods.len() > MAX_PERIODS {
        return Err(format!(
            "{label} requires 1..={MAX_PERIODS} contiguous periods"
        ));
    }
    let mut ids = HashSet::new();
    let mut prior: Option<i32> = None;
    for p in periods {
        text(&p.period_id, bytes)?;
        // Seasoned relationships carry periods that started before the opening date; the
        // explicit horizon domain still bounds every date against the opening.
        validate_ordinal(p.start_ordinal)?;
        if asof - p.start_ordinal > 36_600 {
            return Err(format!(
                "{label} period starts beyond the explicit domain before the opening date"
            ));
        }
        domain(asof.min(p.start_ordinal), p.end_ordinal)?;
        domain(asof.min(p.start_ordinal), p.payment_ordinal)?;
        if p.start_ordinal >= p.end_ordinal
            || p.payment_ordinal < p.end_ordinal
            || prior.is_some_and(|e| e != p.start_ordinal)
            || !ids.insert(p.period_id.as_str())
        {
            return Err(format!("{label} periods must be chronological, contiguous, unique and pay at or after accrual end"));
        }
        prior = Some(p.end_ordinal);
    }
    Ok(())
}
fn same_periods(
    contract: &[crate::products::term::contracts::indexed::ContractPeriod],
    exposure: &[Period],
) -> bool {
    contract.len() == exposure.len()
        && contract.iter().zip(exposure).all(|(c, e)| {
            c.start_ordinal == e.start_ordinal
                && c.end_ordinal == e.end_ordinal
                && c.payment_ordinal == e.payment_ordinal
                && c.day_count == e.day_count
                && c.fee == 0.
        })
}
fn index_matches(
    contract: &[crate::products::term::contracts::indexed::ContractPeriod],
    index_id: &str,
) -> bool {
    contract
        .iter()
        .all(|p| matches!(&p.coupon, Coupon::Indexed { index, .. } if index.index_id == index_id))
}
fn side_for(direction: ExposureDirection) -> Side {
    match direction {
        ExposureDirection::Payment => Side::Payer,
        ExposureDirection::Receipt => Side::Receiver,
    }
}

struct Ctx<'a> {
    r: &'a HedgeAccountingRequest,
    path: &'a crate::products::derivatives::contract::csa::Path,
    node_index: &'a BTreeMap<i32, usize>,
}
/// Admit one supplied hypothetical mirroring `exposure` on `index_id` from `inception` to the tail.
#[allow(clippy::too_many_arguments)]
fn hypothetical_nodes(
    ctx: &Ctx<'_>,
    h: &Hypothetical,
    exposure: &ForecastExposure,
    index_id: &str,
    inception: i32,
    bytes: &mut usize,
    work: &mut usize,
) -> Result<BTreeMap<i32, usize>, String> {
    identity(&h.evidence, bytes)?;
    let mut map = BTreeMap::new();
    for (i, hn) in h.nodes.iter().enumerate() {
        let ni = *ctx
            .node_index
            .get(&hn.ordinal)
            .ok_or("hypothetical node is not a derivative path node")?;
        let node = &ctx.path.nodes[ni];
        let q = &hn.request;
        if map.insert(hn.ordinal, i).is_some() {
            return Err("duplicate hypothetical node".into());
        }
        if hn.ordinal < inception
            || q.asof_ordinal != hn.ordinal
            || q.payment_timing != PaymentTiming::AfterScheduledPayments
            || node.payment_timing != PaymentTiming::AfterScheduledPayments
            || q.scenario_id != ctx.path.path_id
            || q.currency != ctx.r.currency
            || q.monetary_unit != ctx.r.monetary_unit
            || !near(q.notional, exposure.notional)
            || q.side != side_for(exposure.direction)
            || !same_periods(&q.floating_periods, &exposure.periods)
            || !index_matches(&q.floating_periods, index_id)
        {
            return Err(
                "hypothetical derivative must mirror the forecast exposure at each after-payment node"
                    .into(),
            );
        }
        *work = work.saturating_add(swap_work(q));
    }
    let first = ctx.node_index[&inception.max(ctx.path.nodes[0].ordinal)];
    for n in &ctx.path.nodes[first..] {
        if !map.contains_key(&n.ordinal) {
            return Err(
                "hypothetical derivative must cover every node from inception to the tail".into(),
            );
        }
    }
    Ok(map)
}
fn exposure_index(r: &HedgeAccountingRequest, exposure_id: &str) -> Result<usize, String> {
    r.forecast_exposures
        .iter()
        .position(|x| x.exposure_id == exposure_id)
        .ok_or_else(|| "unknown forecast exposure".into())
}
fn hedged_periods(
    ctx: &Ctx<'_>,
    exposure: &ForecastExposure,
    inception: i32,
) -> Result<Vec<usize>, String> {
    if !exposure
        .periods
        .iter()
        .any(|q| q.start_ordinal == inception)
    {
        return Err("inception must be an accrual-free start of a forecast exposure period".into());
    }
    let hedged = exposure
        .periods
        .iter()
        .enumerate()
        .filter(|(_, q)| q.start_ordinal >= inception)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if hedged.is_empty() {
        return Err("no forecast exposure period starts at or after inception".into());
    }
    let last = ctx.path.nodes[ctx.path.nodes.len() - 1].ordinal;
    let first = ctx.path.nodes[0].ordinal;
    for &k in &hedged {
        let pay = exposure.periods[k].payment_ordinal;
        if pay >= first && pay <= last && !ctx.node_index.contains_key(&pay) {
            return Err("hedged forecast period payment date is not a derivative path node".into());
        }
    }
    Ok(hedged)
}
/// An accrual-free after-payment node after inception and before the instrument's final settlement,
/// at a period end of both instrument legs and of the hedged item/exposure(s).
fn assessment_node(
    ctx: &Ctx<'_>,
    rel: &Relationship,
    cr: &CompiledRelationship,
    swaps: &[&SwapRequest],
    ordinal: i32,
) -> Result<(), String> {
    let node = &ctx.path.nodes[ctx.node_index[&ordinal]];
    let ends = |periods: &[Period]| periods.iter().any(|q| q.end_ordinal == ordinal);
    let boundary = match cr.item {
        Some(i) => ends(&ctx.r.hedged_items[i].periods),
        None => cr
            .members
            .iter()
            .all(|m| ends(&ctx.r.forecast_exposures[m.exposure].periods)),
    };
    let final_payment = swaps
        .iter()
        .flat_map(|swap| {
            swap.fixed_periods
                .iter()
                .map(|q| q.payment_ordinal)
                .chain(swap.floating_periods.iter().map(|q| q.payment_ordinal))
        })
        .max()
        .unwrap_or(ordinal);
    let accrual_free = swaps.iter().all(|swap| {
        swap.fixed_periods.iter().any(|q| q.end_ordinal == ordinal)
            && swap
                .floating_periods
                .iter()
                .any(|q| q.end_ordinal == ordinal)
    });
    if ordinal <= rel.inception_ordinal
        || ordinal >= final_payment
        || node.payment_timing != PaymentTiming::AfterScheduledPayments
        || !accrual_free
        || !boundary
    {
        return Err("assessment requires an accrual-free after-payment node after inception and before the final instrument settlement".into());
    }
    Ok(())
}

pub(super) fn admit(r: &HedgeAccountingRequest) -> Result<Compiled, String> {
    let mut bytes = 0usize;
    if r.schema != SCHEMA || r.model_version != MODEL_VERSION {
        return Err(
            "hedge accounting requires explicit hedge-accounting-1 schema/model selection".into(),
        );
    }
    if r.policy.rule_version != RULE_VERSION {
        return Err(
            "unsupported hedge accounting rule matrix; only asc815-asu2017-12 is implemented"
                .into(),
        );
    }
    for i in [&r.run, &r.policy.identity, &r.policy.assessment.identity] {
        identity(i, &mut bytes)?;
    }
    text(&r.entity, &mut bytes)?;
    let d = &r.derivative;
    if r.entity != d.entity
        || r.currency != d.currency
        || r.monetary_unit != d.monetary_unit
        || r.monetary_unit != "currency_units"
    {
        return Err("hedge entity/currency/unit must match the derivative source".into());
    }
    derivative_accounting::validate_request(d)?;
    let c = csa(r)?;
    let path = &c.paths[0];
    if path.default.is_some() {
        return Err(
            "counterparty default paths require a reviewed discontinuation lane; rejected".into(),
        );
    }
    let asof = c.asof_ordinal;
    let mut node_index = BTreeMap::new();
    for (i, n) in path.nodes.iter().enumerate() {
        if node_index.insert(n.ordinal, i).is_some() {
            return Err("duplicate derivative path node".into());
        }
    }
    let ctx = Ctx {
        r,
        path,
        node_index: &node_index,
    };
    let a = &r.policy.adoption;
    let e = &a.asu_2025_09.elections;
    if a.asu_2022_01_portfolio_layer {
        return Err(
            "ASU 2022-01 portfolio-layer accounting is not implemented; election rejected".into(),
        );
    }
    if e.dual_hedge_basis {
        return Err("ASU 2025-09 dual-hedge election is not implemented; explicit refusal".into());
    }
    let adoption = match (a.asu_2025_09.adopted, a.asu_2025_09.adoption_ordinal) {
        (true, Some(o)) => {
            validate_ordinal(o)?;
            Some(o)
        }
        (false, None) => None,
        _ => {
            return Err(
                "ASU 2025-09 adoption requires exactly one adoption date when adopted".into(),
            )
        }
    };
    // Prospective application (815-20-65-7): an election applies to relationships designated at
    // or after the adoption date; existing hedges are not migrated by this lane.
    let elected = |flag: bool, inception: i32, label: &str| -> Result<(), String> {
        match adoption {
            Some(o) if flag && inception >= o => Ok(()),
            _ => Err(format!(
                "{label} requires the adopted ASU 2025-09 election and a relationship designated at or after the adoption date"
            )),
        }
    };
    let s = &r.policy.assessment;
    if !s.lower_bound.is_finite()
        || !s.upper_bound.is_finite()
        || s.lower_bound <= 0.
        || s.lower_bound >= s.upper_bound
    {
        return Err("assessment band requires finite 0 < lower_bound < upper_bound".into());
    }
    if r.hedged_items.len() > MAX_SOURCES || r.forecast_exposures.len() > MAX_SOURCES {
        return Err("hedged item/exposure count exceeds admission".into());
    }
    let mut item_ids = HashSet::new();
    for item in &r.hedged_items {
        text(&item.item_id, &mut bytes)?;
        identity(&item.source, &mut bytes)?;
        identity(&item.carrying_source, &mut bytes)?;
        amount(item.contractual_rate)?;
        if !item.notional.is_finite() || item.notional <= 0. || amount(item.notional).is_err() {
            return Err("hedged item notional must be finite and positive".into());
        }
        periods(&item.periods, asof, &mut bytes, "hedged item")?;
        domain(asof, item.principal_ordinal)?;
        if item.principal_ordinal < item.periods[item.periods.len() - 1].payment_ordinal {
            return Err("hedged item principal date precedes its final coupon payment".into());
        }
        if !item_ids.insert(item.item_id.as_str()) {
            return Err("duplicate hedged item identity".into());
        }
    }
    let mut exposure_ids = HashSet::new();
    for x in &r.forecast_exposures {
        text(&x.exposure_id, &mut bytes)?;
        text(&x.index_id, &mut bytes)?;
        identity(&x.source, &mut bytes)?;
        identity(&x.probability_evidence, &mut bytes)?;
        if !x.notional.is_finite() || x.notional <= 0. || amount(x.notional).is_err() {
            return Err("forecast exposure notional must be finite and positive".into());
        }
        periods(&x.periods, asof, &mut bytes, "forecast exposure")?;
        if let Some(cyr) = &x.choose_your_rate {
            identity(&cyr.evidence, &mut bytes)?;
            let mut options = HashSet::new();
            for id in &cyr.documented_index_ids {
                text(id, &mut bytes)?;
                if !options.insert(id.as_str()) {
                    return Err("choose-your-rate options must be unique".into());
                }
            }
            // 815-30-35-37B: a liability whose interest varies with a contractually specified rate
            // among documented index options; assets and analogies are excluded.
            if options.len() < 2
                || options.len() > MAX_RATE_OPTIONS
                || !options.contains(x.index_id.as_str())
                || x.direction != ExposureDirection::Payment
            {
                return Err("choose-your-rate exposures require 2..=16 documented index options including the initial index on liability interest payments".into());
            }
        }
        if !exposure_ids.insert(x.exposure_id.as_str()) {
            return Err("duplicate forecast exposure identity".into());
        }
    }
    if r.relationships.is_empty() || r.relationships.len() > MAX_RELATIONSHIPS {
        return Err(format!(
            "1..={MAX_RELATIONSHIPS} hedge relationships required"
        ));
    }
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    let mut proportion_by_instrument: BTreeMap<&str, f64> = BTreeMap::new();
    let mut notional_by_item: BTreeMap<&str, f64> = BTreeMap::new();
    let mut work = 0usize;
    let mut compiled: Vec<CompiledRelationship> = Vec::new();
    for (index, rel) in r.relationships.iter().enumerate() {
        text(&rel.relationship_id, &mut bytes)?;
        identity(&rel.risk_management_objective, &mut bytes)?;
        identity(&rel.documentation, &mut bytes)?;
        if ids.insert(rel.relationship_id.as_str(), index).is_some() {
            return Err("duplicate hedge relationship identity".into());
        }
        let p = rel.designated_proportion;
        if !p.is_finite() || p <= 0. || p > 1. {
            return Err("designated proportion must lie in (0, 1]".into());
        }
        let instrument = rel.hedging_instrument_position_id.as_str();
        let swap = opening_swap(c, instrument)?;
        let review = d
            .policy
            .trades
            .iter()
            .find(|t| t.position_id == instrument)
            .ok_or("hedging instrument lacks a reviewed derivative accounting designation")?;
        if review.designation != derivative_accounting::Designation::UndesignatedEconomicHedge {
            return Err("formal relationships require the raw undesignated_economic_hedge review on the instrument; no fallback".into());
        }
        let total = proportion_by_instrument.entry(instrument).or_default();
        *total += p;
        if *total > 1. + 1e-12 {
            return Err(
                "designated proportions on one instrument exceed the whole instrument".into(),
            );
        }
        let inception = rel.inception_ordinal;
        if let Some(option) = rel.written_option_position_id.as_deref() {
            // 815-20-25-88(a)-(c): a written option combined with the swap, matching notional, in
            // a hedge of interest-rate risk, is not a net written option; the combination is one
            // instrument whose marks and clean value are designated together.
            elected(
                e.swap_written_option_exception,
                inception,
                "swap/written-option combination",
            )?;
            text(option, &mut bytes)?;
            if option == instrument {
                return Err("written option must be a distinct mapped position".into());
            }
            let option_review = d
                .policy
                .trades
                .iter()
                .find(|t| t.position_id == option)
                .ok_or("written option lacks a reviewed derivative accounting designation")?;
            if option_review.designation
                != derivative_accounting::Designation::UndesignatedEconomicHedge
                || (rel.designation == Designation::CashFlowHedge
                    && option_review.coupon_recognition
                        != CouponRecognition::SettleDerivativeCarrying)
            {
                return Err("written option requires the raw undesignated_economic_hedge review and, for cash-flow hedges, settle_derivative_carrying".into());
            }
            let total = proportion_by_instrument.entry(option).or_default();
            *total += p;
            if *total > 1. + 1e-12 {
                return Err(
                    "designated proportions on the written option exceed the whole option".into(),
                );
            }
            let mut seen = false;
            for node in &path.nodes {
                let trade = node.trades.iter().find(|t| match t {
                    Trade::EuropeanSwaption { request } => request.position_id == option,
                    Trade::ExpiredEuropean { terminal, .. } => terminal.position_id == option,
                    Trade::Swap { request } => request.position_id == option,
                });
                let terminal = match trade {
                    Some(Trade::EuropeanSwaption { request }) => request,
                    Some(Trade::ExpiredEuropean {
                        terminal,
                        delivered_underlying,
                        ..
                    }) if node.ordinal >= terminal.expiry_ordinal => {
                        // A physically delivered written option continues as the delivered swap
                        // under the same mapped position; its notional must still match.
                        if let Some(u) = delivered_underlying {
                            if !near(u.notional, swap.notional) {
                                return Err(
                                    "delivered underlying notional must match the swap".into()
                                );
                            }
                            if option_review.delivery_review.is_none() {
                                return Err("physical delivery of the written option requires its reviewed delivery recognition".into());
                            }
                        }
                        terminal
                    }
                    _ => {
                        return Err(
                            "written option must be a mapped European swaption on every path node"
                                .into(),
                        )
                    }
                };
                if terminal.side
                    != crate::products::derivatives::contract::options::OptionSide::Short
                    || terminal
                        .states
                        .iter()
                        .any(|s| !near(s.underlying.notional, swap.notional))
                {
                    return Err("815-20-25-88 requires a written (short) option whose notional matches the swap".into());
                }
                seen = true;
            }
            if !seen {
                return Err("written option absent from the derivative path".into());
            }
        }
        let seasoned = inception < asof;
        if seasoned != rel.opening.is_some() {
            return Err("relationships designated before the opening date require a reviewed opening block, and only those".into());
        }
        if let Some(o) = &rel.opening {
            identity(&o.evidence, &mut bytes)?;
            for v in [
                o.hedged_value,
                o.cumulative_instrument_change,
                o.cumulative_hedged_change,
                o.basis_balance,
                o.oci_balance,
            ] {
                amount(v)?;
            }
            if rel.group.is_some()
                || rel.rate_hypotheticals.is_some()
                || rel.predecessor_relationship_id.is_some()
            {
                return Err("seasoned openings are not combined with groups, rate options or redesignation predecessors".into());
            }
            let scale = o.scale();
            if !scale.is_finite() || scale <= 0. || scale > 1. {
                return Err("seasoned opening scale must lie in (0, 1]".into());
            }
            if o.frozen_basis.len() > MAX_EVENTS || o.frozen_oci.len() > MAX_EVENTS {
                return Err("seasoned opening tranche count exceeds admission".into());
            }
            for t in &o.frozen_basis {
                amount(t.amount)?;
                amount(t.amortized)?;
                validate_ordinal(t.start_ordinal)?;
                if t.start_ordinal >= asof
                    || asof - t.start_ordinal > 36_600
                    || t.amortized * t.amount < 0.
                    || t.amortized.abs() > t.amount.abs() + 1e-12
                {
                    return Err("frozen basis tranche requires a start before the opening and an amortized amount within the tranche".into());
                }
            }
            for t in &o.frozen_oci {
                amount(t.amount)?;
                amount(t.released)?;
                if t.released * t.amount < 0. || t.released.abs() > t.amount.abs() + 1e-12 {
                    return Err(
                        "frozen OCI tranche released amount must lie within the tranche".into(),
                    );
                }
                let mut prior: Option<i32> = None;
                let mut total = 0.;
                for e in &t.entries {
                    validate_ordinal(e.payment_ordinal)?;
                    if e.payment_ordinal < asof
                        || prior.is_some_and(|p| p >= e.payment_ordinal)
                        || !e.weight.is_finite()
                        || e.weight < 0.
                    {
                        return Err("frozen OCI entries must be strictly increasing payment dates at or after the opening with nonnegative weights".into());
                    }
                    prior = Some(e.payment_ordinal);
                    total += e.weight;
                }
                if t.amount != 0. && !near(total, 1. - t.released / t.amount) {
                    return Err(
                        "frozen OCI entry weights must sum to the unreleased fraction".into(),
                    );
                }
            }
            match rel.designation {
                Designation::FairValueHedge if o.oci_balance != 0. || !o.frozen_oci.is_empty() => {
                    return Err("fair-value opening carries no OCI balance or OCI tranches".into())
                }
                Designation::CashFlowHedge
                    if o.basis_balance != 0. || !o.frozen_basis.is_empty() =>
                {
                    return Err(
                        "cash-flow opening carries no basis balance or basis tranches".into(),
                    )
                }
                _ => {}
            }
        } else if !node_index.contains_key(&inception) {
            return Err("inception date is not a derivative path node".into());
        }
        // Every jointly designated swap shares the instrument review, proportion accounting and
        // accrual-free inception boundary of the primary instrument.
        let mut components = vec![swap];
        for id in &rel.additional_instrument_position_ids {
            text(id, &mut bytes)?;
            if id == instrument
                || rel.written_option_position_id.as_deref() == Some(id.as_str())
                || components.iter().any(|s| &s.position_id == id)
            {
                return Err("additional instruments must be distinct from the primary instrument, the written option and each other".into());
            }
            let extra = opening_swap(c, id)?;
            let extra_review = d
                .policy
                .trades
                .iter()
                .find(|t| &t.position_id == id)
                .ok_or(
                    "additional instrument lacks a reviewed derivative accounting designation",
                )?;
            if extra_review.designation
                != derivative_accounting::Designation::UndesignatedEconomicHedge
                || (rel.designation == Designation::CashFlowHedge
                    && extra_review.coupon_recognition
                        != CouponRecognition::SettleDerivativeCarrying)
            {
                return Err("additional instruments require the raw undesignated_economic_hedge review and, for cash-flow hedges, settle_derivative_carrying".into());
            }
            let total = proportion_by_instrument.entry(id.as_str()).or_default();
            *total += p;
            if *total > 1. + 1e-12 {
                return Err("designated proportions on an additional instrument exceed the whole instrument".into());
            }
            components.push(extra);
        }
        for s in &components {
            if !s.fixed_periods.iter().any(|q| q.start_ordinal == inception)
                || !s
                    .floating_periods
                    .iter()
                    .any(|q| q.start_ordinal == inception)
            {
                return Err(
                    "inception must be an accrual-free start of both instrument legs".into(),
                );
            }
        }
        let predecessor = match &rel.predecessor_relationship_id {
            None => None,
            Some(id) => {
                let prior = *ids
                    .get(id.as_str())
                    .ok_or("predecessor must be an earlier listed relationship")?;
                if prior >= index
                    || r.relationships[prior].hedging_instrument_position_id
                        != rel.hedging_instrument_position_id
                {
                    return Err(
                        "redesignation predecessor must precede and share the instrument".into(),
                    );
                }
                Some(prior)
            }
        };
        let entry = match rel.designation {
            Designation::FairValueHedge => {
                if rel.forecast_exposure_id.is_some()
                    || rel.hypothetical.is_some()
                    || rel.matched_critical_terms_evidence.is_some()
                    || rel.group.is_some()
                    || rel.rate_hypotheticals.is_some()
                {
                    return Err("fair-value relationships cannot carry forecast exposure, hypothetical, group or rate-option inputs".into());
                }
                let item_id = rel
                    .hedged_item_id
                    .as_deref()
                    .ok_or("fair-value hedge requires a hedged item")?;
                let item_index = r
                    .hedged_items
                    .iter()
                    .position(|i| i.item_id == item_id)
                    .ok_or("unknown hedged item")?;
                let item = &r.hedged_items[item_index];
                let hedged = rel
                    .hedged_notional
                    .ok_or("fair-value hedge requires hedged_notional")?;
                let rate = rel
                    .benchmark_rate
                    .ok_or("fair-value hedge requires the designated benchmark rate")?;
                amount(rate)?;
                if !hedged.is_finite() || hedged <= 0. || hedged > item.notional + 1e-12 {
                    return Err(
                        "hedged notional must be positive and within the item notional".into(),
                    );
                }
                let total = notional_by_item.entry(item_id).or_default();
                *total += hedged;
                if *total > item.notional + 1e-9 {
                    return Err("hedged notionals on one item exceed the item".into());
                }
                if !item.periods.iter().any(|q| q.start_ordinal == inception) {
                    return Err(
                        "inception must be an accrual-free start of a hedged item period".into(),
                    );
                }
                if let Some(end) = rel.hedged_term_end_ordinal {
                    // The assumed maturity is a coupon period end after inception and before the
                    // item's principal date; its payment date must be exactly discountable.
                    if end <= inception
                        || end >= item.principal_ordinal
                        || !item.periods.iter().any(|q| q.end_ordinal == end)
                    {
                        return Err("partial-term hedge requires an item period end after inception and before the principal date".into());
                    }
                }
                let direction_sign = if item.direction == ItemDirection::Asset {
                    1.
                } else {
                    -1.
                };
                if let Some(o) = &rel.opening {
                    // The live basis plus the original frozen amounts is the signed cumulative
                    // benchmark-component change: basis + sum(frozen amount) = -dir * E.
                    let frozen: f64 = o.frozen_basis.iter().map(|t| t.amount).sum();
                    if !near(
                        o.basis_balance + frozen,
                        -direction_sign * o.cumulative_hedged_change,
                    ) {
                        return Err("seasoned fair-value opening basis plus frozen amounts must equal the signed cumulative hedged change".into());
                    }
                }
                CompiledRelationship {
                    item: Some(item_index),
                    direction_sign,
                    members: Vec::new(),
                    matched: false,
                    predecessor,
                    similar_risk: None,
                    choose_your_rate: false,
                }
            }
            Designation::CashFlowHedge => {
                if rel.hedged_item_id.is_some()
                    || rel.hedged_notional.is_some()
                    || rel.benchmark_rate.is_some()
                    || rel.hedged_term_end_ordinal.is_some()
                {
                    return Err("cash-flow relationships cannot carry hedged item inputs".into());
                }
                if review.coupon_recognition != CouponRecognition::SettleDerivativeCarrying {
                    return Err("cash-flow hedge instruments require settle_derivative_carrying coupon recognition".into());
                }
                if let Some(group) = &rel.group {
                    // ASU 2025-09 Issue 1: a group of individual forecast transactions with a
                    // documented similar-risk method, one hypothetical per member.
                    elected(e.similar_risk_grouping, inception, "similar-risk grouping")?;
                    if rel.forecast_exposure_id.is_some()
                        || rel.hypothetical.is_some()
                        || rel.matched_critical_terms_evidence.is_some()
                        || rel.rate_hypotheticals.is_some()
                    {
                        return Err("group relationships carry member exposures and member hypotheticals only".into());
                    }
                    let n = group.member_exposure_ids.len();
                    if !(2..=MAX_GROUP_MEMBERS).contains(&n) || group.hypotheticals.len() != n {
                        return Err(format!("similar-risk groups require 2..={MAX_GROUP_MEMBERS} members with exactly one hypothetical each"));
                    }
                    let mut seen = HashSet::new();
                    let mut members = Vec::with_capacity(n);
                    for (m, exposure_id) in group.member_exposure_ids.iter().enumerate() {
                        let xi = exposure_index(r, exposure_id)?;
                        let exposure = &r.forecast_exposures[xi];
                        if !seen.insert(exposure_id.as_str())
                            || group.hypotheticals[m].exposure_id != *exposure_id
                            || exposure.choose_your_rate.is_some()
                        {
                            return Err("group members must be unique non-choose-your-rate exposures listed in hypothetical order".into());
                        }
                        let hedged = hedged_periods(&ctx, exposure, inception)?;
                        let nodes = hypothetical_nodes(
                            &ctx,
                            &group.hypotheticals[m].hypothetical,
                            exposure,
                            &exposure.index_id,
                            inception,
                            &mut bytes,
                            &mut work,
                        )?;
                        let mut hyps = BTreeMap::new();
                        hyps.insert(
                            exposure.index_id.clone(),
                            HypRef {
                                source: HypSource::Group(m),
                                nodes,
                            },
                        );
                        members.push(CompiledMember {
                            exposure: xi,
                            hedged_periods: hedged,
                            hypotheticals: hyps,
                        });
                    }
                    compiled.push(CompiledRelationship {
                        item: None,
                        direction_sign: 1.,
                        members,
                        matched: false,
                        predecessor,
                        similar_risk: Some(group.similar_risk_method),
                        choose_your_rate: false,
                    });
                    continue;
                }
                let exposure_id = rel
                    .forecast_exposure_id
                    .as_deref()
                    .ok_or("cash-flow hedge requires a forecast exposure")?;
                let xi = exposure_index(r, exposure_id)?;
                let exposure = &r.forecast_exposures[xi];
                let hedged = hedged_periods(&ctx, exposure, inception)?;
                if let Some(o) = &rel.opening {
                    for e in o.frozen_oci.iter().flat_map(|t| &t.entries) {
                        if !hedged
                            .iter()
                            .any(|&k| exposure.periods[k].payment_ordinal == e.payment_ordinal)
                        {
                            return Err(
                                "frozen OCI entries must name hedged exposure payment dates".into(),
                            );
                        }
                    }
                }
                let mut hyps = BTreeMap::new();
                let matched = match (&rel.hypothetical, &rel.matched_critical_terms_evidence) {
                    (Some(h), None) => {
                        let nodes = hypothetical_nodes(
                            &ctx,
                            h,
                            exposure,
                            &exposure.index_id,
                            inception,
                            &mut bytes,
                            &mut work,
                        )?;
                        hyps.insert(
                            exposure.index_id.clone(),
                            HypRef {
                                source: HypSource::Base,
                                nodes,
                            },
                        );
                        false
                    }
                    (None, Some(evidence)) => {
                        identity(evidence, &mut bytes)?;
                        if !near(p * swap.notional, exposure.notional)
                            || swap.side != side_for(exposure.direction)
                            || !same_periods(&swap.floating_periods, &exposure.periods)
                            || !index_matches(&swap.floating_periods, &exposure.index_id)
                        {
                            return Err("matched critical terms require identical designated notional, side, index and floating periods".into());
                        }
                        true
                    }
                    _ => return Err("cash-flow hedge requires exactly one of a hypothetical derivative or matched critical terms evidence".into()),
                };
                if matched
                    && (rel.written_option_position_id.is_some()
                        || !rel.additional_instrument_position_ids.is_empty())
                {
                    return Err("a combination of instruments cannot claim matched critical terms; supply a hypothetical derivative".into());
                }
                let choose_your_rate = match (&exposure.choose_your_rate, &rel.rate_hypotheticals)
                {
                    (None, None) => false,
                    (Some(cyr), Some(rate_hyps)) => {
                        // ASU 2025-09 Issue 2: one hypothetical per documented alternative rate with
                        // terms as of inception (815-30-35-37M); the hypothetical method is required.
                        elected(e.selectable_rate_debt, inception, "choose-your-rate debt")?;
                        if matched {
                            return Err("choose-your-rate relationships require the hypothetical derivative method".into());
                        }
                        let expected = cyr
                            .documented_index_ids
                            .iter()
                            .filter(|id| **id != exposure.index_id)
                            .collect::<Vec<_>>();
                        if rate_hyps.len() != expected.len() {
                            return Err("choose-your-rate relationships require exactly one hypothetical per documented alternative index".into());
                        }
                        for rh in rate_hyps {
                            text(&rh.index_id, &mut bytes)?;
                            if !expected.contains(&&rh.index_id) || hyps.contains_key(&rh.index_id) {
                                return Err("rate hypothetical index must be a documented alternative listed once".into());
                            }
                            let position = rate_hyps
                                .iter()
                                .position(|x| x.index_id == rh.index_id)
                                .unwrap_or(0);
                            let nodes = hypothetical_nodes(
                                &ctx,
                                &rh.hypothetical,
                                exposure,
                                &rh.index_id,
                                inception,
                                &mut bytes,
                                &mut work,
                            )?;
                            hyps.insert(
                                rh.index_id.clone(),
                                HypRef {
                                    source: HypSource::Rate(position),
                                    nodes,
                                },
                            );
                        }
                        true
                    }
                    _ => {
                        return Err("choose-your-rate exposures and rate hypotheticals must be supplied together".into())
                    }
                };
                CompiledRelationship {
                    item: None,
                    direction_sign: 1.,
                    members: vec![CompiledMember {
                        exposure: xi,
                        hedged_periods: hedged,
                        hypotheticals: hyps,
                    }],
                    matched,
                    predecessor,
                    similar_risk: None,
                    choose_your_rate,
                }
            }
        };
        compiled.push(entry);
    }
    if work > MAX_WORK {
        return Err("hypothetical derivative work exceeds the hedge accounting admission".into());
    }
    if r.events.len() > MAX_EVENTS {
        return Err("hedge event count exceeds admission".into());
    }
    let mut event_ids = HashSet::new();
    let mut prior: Option<(i32, u32)> = None;
    for ev in &r.events {
        text(&ev.event_id, &mut bytes)?;
        identity(&ev.review, &mut bytes)?;
        let clock = (ev.ordinal, ev.sequence);
        if !event_ids.insert(ev.event_id.as_str()) || prior.is_some_and(|c| c >= clock) {
            return Err(
                "hedge events must be unique and strictly ordered by (ordinal, sequence)".into(),
            );
        }
        prior = Some(clock);
        let &index = ids
            .get(ev.relationship_id.as_str())
            .ok_or("event names an unknown relationship")?;
        let rel = &r.relationships[index];
        let cr = &compiled[index];
        if !node_index.contains_key(&ev.ordinal) {
            return Err("event date is not a derivative path node".into());
        }
        if ev.ordinal < rel.inception_ordinal {
            return Err("event precedes relationship inception".into());
        }
        let mut swaps = vec![opening_swap(c, &rel.hedging_instrument_position_id)?];
        for id in &rel.additional_instrument_position_ids {
            swaps.push(opening_swap(c, id)?);
        }
        if ev.kind != EventKind::RateElection && ev.index_id.is_some() {
            return Err("only rate elections carry an index".into());
        }
        if ev.effective_ordinal.is_some()
            && (ev.kind != EventKind::Assess || cr.similar_risk.is_none())
        {
            return Err("only group assessments carry a specific last-similar date".into());
        }
        match ev.kind {
            EventKind::Assess => {
                if ev.proportion.is_some() || ev.reason.is_some() || ev.status.is_some() {
                    return Err("assessment events carry no proportion/reason/status".into());
                }
                assessment_node(&ctx, rel, cr, &swaps, ev.ordinal)?;
                if let Some(eff) = ev.effective_ordinal {
                    if !node_index.contains_key(&eff)
                        || eff < rel.inception_ordinal
                        || eff >= ev.ordinal
                    {
                        return Err("the specific last-similar date must be a path node from inception before the assessment".into());
                    }
                }
            }
            EventKind::Discontinue => {
                if ev.reason.is_none() || ev.status.is_some() {
                    return Err("discontinuation requires a reason and no forecast status".into());
                }
                if let Some(q) = ev.proportion {
                    if !q.is_finite() || q <= 0. || q > 1. {
                        return Err("discontinued proportion must lie in (0, 1]".into());
                    }
                }
            }
            EventKind::ForecastStatus => {
                if rel.designation != Designation::CashFlowHedge
                    || ev.status.is_none()
                    || ev.proportion.is_some()
                    || ev.reason.is_some()
                {
                    return Err("forecast status events apply to cash-flow hedges only and carry only a status".into());
                }
            }
            EventKind::RateElection => {
                let index_id = ev
                    .index_id
                    .as_deref()
                    .ok_or("rate election requires the newly selected index")?;
                if !cr.choose_your_rate
                    || ev.proportion.is_some()
                    || ev.reason.is_some()
                    || ev.status.is_some()
                {
                    return Err("rate elections apply to choose-your-rate relationships and carry only an index".into());
                }
                let exposure = &r.forecast_exposures[cr.members[0].exposure];
                if !exposure
                    .choose_your_rate
                    .as_ref()
                    .is_some_and(|c| c.documented_index_ids.iter().any(|x| x == index_id))
                {
                    return Err("rate election must select a documented index option".into());
                }
                // The newly selected rate begins accruing at an exposure period start that is also
                // an accrual-free instrument boundary, so the final retrospective assessment on the
                // prior rate measures clean values (815-30-35-37L).
                assessment_node(&ctx, rel, cr, &swaps, ev.ordinal)?;
            }
        }
    }
    Ok(Compiled {
        relationships: compiled,
    })
}
