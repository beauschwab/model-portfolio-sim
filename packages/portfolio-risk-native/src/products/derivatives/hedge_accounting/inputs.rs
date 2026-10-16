//! Deterministic relationship/event admission; every unsupported case refuses explicitly.
use super::types::*;
use crate::products::derivatives::accounting::{self as derivative_accounting, CouponRecognition};
use crate::products::derivatives::contract::{
    amount, csa::{CsaRequest, Trade}, domain, identity, near, swaps::{PaymentTiming, Side, SwapRequest},
    text, Operation,
};
use crate::products::term::contracts::indexed::Coupon;
use portfolio_model_core::cashflow::validate_ordinal;
use std::collections::{BTreeMap, HashSet};

pub(super) struct CompiledRelationship {
    pub item: Option<usize>,
    pub exposure: Option<usize>,
    pub direction_sign: f64,
    /// CFH exposure periods starting at or after inception, in order.
    pub hedged_periods: Vec<usize>,
    /// Hypothetical node ordinal -> index in `relationship.hypothetical.nodes`.
    pub hypothetical: BTreeMap<i32, usize>,
    pub predecessor: Option<usize>,
}
pub(super) struct Compiled {
    pub relationships: Vec<CompiledRelationship>,
}

pub(super) fn csa(r: &HedgeAccountingRequest) -> Result<&CsaRequest, String> {
    match &r.derivative.derivative.operation {
        Operation::Netting { request } => Ok(request),
        _ => Err("hedge accounting requires the complete raw netting/CSA derivative lifecycle".into()),
    }
}
pub(super) fn opening_swap<'a>(c: &'a CsaRequest, position: &str) -> Result<&'a SwapRequest, String> {
    let node = c.paths[0].nodes.first().ok_or("derivative path lacks an opening node")?;
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
    s.floating_periods.iter().fold(s.fixed_periods.len(), |w, p| {
        w.saturating_add(1).saturating_add(match &p.coupon {
            Coupon::Indexed { observations, .. } => observations.len(),
            _ => 0,
        })
    })
}
fn periods(periods: &[Period], asof: i32, bytes: &mut usize, label: &str) -> Result<(), String> {
    if periods.is_empty() || periods.len() > MAX_PERIODS {
        return Err(format!("{label} requires 1..={MAX_PERIODS} contiguous periods"));
    }
    let mut ids = HashSet::new();
    let mut prior: Option<i32> = None;
    for p in periods {
        text(&p.period_id, bytes)?;
        domain(asof, p.start_ordinal)?;
        domain(asof, p.end_ordinal)?;
        domain(asof, p.payment_ordinal)?;
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
fn same_periods(contract: &[crate::products::term::contracts::indexed::ContractPeriod], exposure: &[Period]) -> bool {
    contract.len() == exposure.len()
        && contract.iter().zip(exposure).all(|(c, e)| {
            c.start_ordinal == e.start_ordinal
                && c.end_ordinal == e.end_ordinal
                && c.payment_ordinal == e.payment_ordinal
                && c.day_count == e.day_count
                && c.fee == 0.
        })
}
fn index_matches(contract: &[crate::products::term::contracts::indexed::ContractPeriod], index_id: &str) -> bool {
    contract.iter().all(|p| matches!(&p.coupon, Coupon::Indexed { index, .. } if index.index_id == index_id))
}
fn side_for(direction: ExposureDirection) -> Side {
    match direction {
        ExposureDirection::Payment => Side::Payer,
        ExposureDirection::Receipt => Side::Receiver,
    }
}

pub(super) fn admit(r: &HedgeAccountingRequest) -> Result<Compiled, String> {
    let mut bytes = 0usize;
    if r.schema != SCHEMA || r.model_version != MODEL_VERSION {
        return Err("hedge accounting requires explicit hedge-accounting-1 schema/model selection".into());
    }
    if r.policy.rule_version != RULE_VERSION {
        return Err("unsupported hedge accounting rule matrix; only asc815-asu2017-12 is implemented".into());
    }
    for i in [&r.run, &r.policy.identity, &r.policy.assessment.identity] {
        identity(i, &mut bytes)?;
    }
    text(&r.entity, &mut bytes)?;
    let d = &r.derivative;
    if r.entity != d.entity || r.currency != d.currency || r.monetary_unit != d.monetary_unit || r.monetary_unit != "currency_units" {
        return Err("hedge entity/currency/unit must match the derivative source".into());
    }
    derivative_accounting::validate_request(d)?;
    let c = csa(r)?;
    let path = &c.paths[0];
    if path.default.is_some() {
        return Err("counterparty default paths require a reviewed discontinuation lane; rejected".into());
    }
    let asof = c.asof_ordinal;
    let mut node_index = BTreeMap::new();
    for (i, n) in path.nodes.iter().enumerate() {
        if node_index.insert(n.ordinal, i).is_some() {
            return Err("duplicate derivative path node".into());
        }
    }
    let a = &r.policy.adoption;
    let e = &a.asu_2025_09.elections;
    if a.asu_2022_01_portfolio_layer {
        return Err("ASU 2022-01 portfolio-layer accounting is not implemented; election rejected".into());
    }
    if e.similar_risk_grouping || e.selectable_rate_debt || e.swap_written_option_exception || e.dual_hedge_basis {
        return Err("ASU 2025-09 elections are not implemented; explicit refusal".into());
    }
    match (a.asu_2025_09.adopted, a.asu_2025_09.adoption_ordinal) {
        (true, Some(o)) => validate_ordinal(o)?,
        (false, None) => {}
        _ => return Err("ASU 2025-09 adoption requires exactly one adoption date when adopted".into()),
    }
    let s = &r.policy.assessment;
    if !s.lower_bound.is_finite() || !s.upper_bound.is_finite() || s.lower_bound <= 0. || s.lower_bound >= s.upper_bound {
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
        if !exposure_ids.insert(x.exposure_id.as_str()) {
            return Err("duplicate forecast exposure identity".into());
        }
    }
    if r.relationships.is_empty() || r.relationships.len() > MAX_RELATIONSHIPS {
        return Err(format!("1..={MAX_RELATIONSHIPS} hedge relationships required"));
    }
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    let mut proportion_by_instrument: BTreeMap<&str, f64> = BTreeMap::new();
    let mut notional_by_item: BTreeMap<&str, f64> = BTreeMap::new();
    let mut work = 0usize;
    let mut compiled = Vec::new();
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
            return Err("designated proportions on one instrument exceed the whole instrument".into());
        }
        let inception = rel.inception_ordinal;
        let node = *node_index
            .get(&inception)
            .ok_or("inception date is not a derivative path node")?;
        if inception < asof {
            return Err("seasoned relationships before the opening date require retained opening balances; rejected".into());
        }
        let boundary = |periods: &[crate::products::term::contracts::indexed::ContractPeriod], date: i32| periods.iter().any(|q| q.start_ordinal == date);
        if !swap.fixed_periods.iter().any(|q| q.start_ordinal == inception) || !boundary(&swap.floating_periods, inception) {
            return Err("inception must be an accrual-free start of both instrument legs".into());
        }
        let _ = node;
        let predecessor = match &rel.predecessor_relationship_id {
            None => None,
            Some(id) => {
                let prior = *ids.get(id.as_str()).ok_or("predecessor must be an earlier listed relationship")?;
                if prior >= index || r.relationships[prior].hedging_instrument_position_id != rel.hedging_instrument_position_id {
                    return Err("redesignation predecessor must precede and share the instrument".into());
                }
                Some(prior)
            }
        };
        let entry = match rel.designation {
            Designation::FairValueHedge => {
                if rel.forecast_exposure_id.is_some() || rel.hypothetical.is_some() || rel.matched_critical_terms_evidence.is_some() {
                    return Err("fair-value relationships cannot carry forecast exposure or hypothetical inputs".into());
                }
                let item_id = rel.hedged_item_id.as_deref().ok_or("fair-value hedge requires a hedged item")?;
                let item_index = r.hedged_items.iter().position(|i| i.item_id == item_id).ok_or("unknown hedged item")?;
                let item = &r.hedged_items[item_index];
                let hedged = rel.hedged_notional.ok_or("fair-value hedge requires hedged_notional")?;
                let rate = rel.benchmark_rate.ok_or("fair-value hedge requires the designated benchmark rate")?;
                amount(rate)?;
                if !hedged.is_finite() || hedged <= 0. || hedged > item.notional + 1e-12 {
                    return Err("hedged notional must be positive and within the item notional".into());
                }
                let total = notional_by_item.entry(item_id).or_default();
                *total += hedged;
                if *total > item.notional + 1e-9 {
                    return Err("hedged notionals on one item exceed the item".into());
                }
                if !item.periods.iter().any(|q| q.start_ordinal == inception) {
                    return Err("inception must be an accrual-free start of a hedged item period".into());
                }
                CompiledRelationship {
                    item: Some(item_index),
                    exposure: None,
                    direction_sign: if item.direction == ItemDirection::Asset { 1. } else { -1. },
                    hedged_periods: Vec::new(),
                    hypothetical: BTreeMap::new(),
                    predecessor,
                }
            }
            Designation::CashFlowHedge => {
                if rel.hedged_item_id.is_some() || rel.hedged_notional.is_some() || rel.benchmark_rate.is_some() {
                    return Err("cash-flow relationships cannot carry hedged item inputs".into());
                }
                if review.coupon_recognition != CouponRecognition::SettleDerivativeCarrying {
                    return Err("cash-flow hedge instruments require settle_derivative_carrying coupon recognition".into());
                }
                let exposure_id = rel.forecast_exposure_id.as_deref().ok_or("cash-flow hedge requires a forecast exposure")?;
                let exposure_index = r.forecast_exposures.iter().position(|x| x.exposure_id == exposure_id).ok_or("unknown forecast exposure")?;
                let exposure = &r.forecast_exposures[exposure_index];
                if !exposure.periods.iter().any(|q| q.start_ordinal == inception) {
                    return Err("inception must be an accrual-free start of a forecast exposure period".into());
                }
                let hedged_periods = exposure
                    .periods
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| q.start_ordinal >= inception)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                if hedged_periods.is_empty() {
                    return Err("no forecast exposure period starts at or after inception".into());
                }
                let last = path.nodes[path.nodes.len() - 1].ordinal;
                for &k in &hedged_periods {
                    let pay = exposure.periods[k].payment_ordinal;
                    if pay <= last && !node_index.contains_key(&pay) {
                        return Err("hedged forecast period payment date is not a derivative path node".into());
                    }
                }
                let hypothetical = match (&rel.hypothetical, &rel.matched_critical_terms_evidence) {
                    (Some(h), None) => {
                        identity(&h.evidence, &mut bytes)?;
                        let mut map = BTreeMap::new();
                        for (i, hn) in h.nodes.iter().enumerate() {
                            let ni = *node_index.get(&hn.ordinal).ok_or("hypothetical node is not a derivative path node")?;
                            let node = &path.nodes[ni];
                            let q = &hn.request;
                            if map.insert(hn.ordinal, i).is_some() {
                                return Err("duplicate hypothetical node".into());
                            }
                            if hn.ordinal < inception
                                || q.asof_ordinal != hn.ordinal
                                || q.payment_timing != PaymentTiming::AfterScheduledPayments
                                || node.payment_timing != PaymentTiming::AfterScheduledPayments
                                || q.scenario_id != path.path_id
                                || q.currency != r.currency
                                || q.monetary_unit != r.monetary_unit
                                || !near(q.notional, exposure.notional)
                                || q.side != side_for(exposure.direction)
                                || !same_periods(&q.floating_periods, &exposure.periods)
                                || !index_matches(&q.floating_periods, &exposure.index_id)
                            {
                                return Err("hypothetical derivative must mirror the forecast exposure at each after-payment node".into());
                            }
                            work = work.saturating_add(swap_work(q));
                        }
                        for n in &path.nodes[node_index[&inception]..] {
                            if !map.contains_key(&n.ordinal) {
                                return Err("hypothetical derivative must cover every node from inception to the tail".into());
                            }
                        }
                        map
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
                        BTreeMap::new()
                    }
                    _ => return Err("cash-flow hedge requires exactly one of a hypothetical derivative or matched critical terms evidence".into()),
                };
                CompiledRelationship {
                    item: None,
                    exposure: Some(exposure_index),
                    direction_sign: 1.,
                    hedged_periods,
                    hypothetical,
                    predecessor,
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
            return Err("hedge events must be unique and strictly ordered by (ordinal, sequence)".into());
        }
        prior = Some(clock);
        let &index = ids.get(ev.relationship_id.as_str()).ok_or("event names an unknown relationship")?;
        let rel = &r.relationships[index];
        let ni = *node_index.get(&ev.ordinal).ok_or("event date is not a derivative path node")?;
        if ev.ordinal < rel.inception_ordinal {
            return Err("event precedes relationship inception".into());
        }
        let swap = opening_swap(c, &rel.hedging_instrument_position_id)?;
        match ev.kind {
            EventKind::Assess => {
                if ev.proportion.is_some() || ev.reason.is_some() || ev.status.is_some() {
                    return Err("assessment events carry no proportion/reason/status".into());
                }
                let node = &path.nodes[ni];
                let boundary = match (compiled[index].item, compiled[index].exposure) {
                    (Some(i), _) => r.hedged_items[i].periods.iter().any(|q| q.end_ordinal == ev.ordinal),
                    (_, Some(x)) => r.forecast_exposures[x].periods.iter().any(|q| q.end_ordinal == ev.ordinal),
                    _ => false,
                };
                let final_payment = swap
                    .fixed_periods
                    .iter()
                    .map(|q| q.payment_ordinal)
                    .chain(swap.floating_periods.iter().map(|q| q.payment_ordinal))
                    .max()
                    .unwrap_or(ev.ordinal);
                if ev.ordinal <= rel.inception_ordinal
                    || ev.ordinal >= final_payment
                    || node.payment_timing != PaymentTiming::AfterScheduledPayments
                    || !swap.fixed_periods.iter().any(|q| q.end_ordinal == ev.ordinal)
                    || !swap.floating_periods.iter().any(|q| q.end_ordinal == ev.ordinal)
                    || !boundary
                {
                    return Err("assessment requires an accrual-free after-payment node after inception and before the final instrument settlement".into());
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
                if rel.designation != Designation::CashFlowHedge || ev.status.is_none() || ev.proportion.is_some() || ev.reason.is_some() {
                    return Err("forecast status events apply to cash-flow hedges only and carry only a status".into());
                }
            }
        }
    }
    Ok(Compiled { relationships: compiled })
}
