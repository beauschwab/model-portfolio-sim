//! Raw contract admission and Physical fixing arithmetic, no exposure/cash overrides.
use super::types::*;
use crate::derivative_contract::{
    self, amount,
    csa::{self, CsaRequest, Node, Trade},
    domain, identity, nonnegative, options, swaps, text, Identity, Operation,
};
use crate::term_enhancements::indexed::{
    build_path, ContractPathRequest, Coupon, PrincipalPlan, RateSource,
};
use portfolio_model_core::{cashflow::CashflowMeasure, floating_rate::Direction};
use std::collections::{BTreeMap, HashMap, HashSet};
pub(super) fn csa_request(
    raw: &derivative_contract::DerivativeRequest,
) -> Result<&CsaRequest, String> {
    match &raw.operation{Operation::Netting{request}=>Ok(request),_=>Err("daily derivative accounting requires a complete raw netting/CSA lifecycle; standalone snapshots are not daily paths".into())}
}
pub(super) fn position(t: &Trade) -> &str {
    match t {
        Trade::Swap { request } => &request.position_id,
        Trade::EuropeanSwaption { request } => &request.position_id,
        Trade::ExpiredEuropean { terminal, .. } => &terminal.position_id,
    }
}
pub(super) fn owner(t: &Trade) -> &str {
    match t {
        Trade::ExpiredEuropean {
            delivered_underlying: Some(u),
            ..
        } => &u.position_id,
        _ => position(t),
    }
}
pub(super) fn bucket(c: &CsaRequest, id: &str) -> Result<String, String> {
    let m = c
        .mapping
        .iter()
        .find(|m| m.position_id == id)
        .ok_or("missing accounting legal mapping")?;
    Ok(if m.enforceable_netting {
        m.netting_set_id.clone()
    } else {
        format!("trade:{id}")
    })
}
fn swap_work(r: &swaps::SwapRequest) -> usize {
    r.floating_periods
        .iter()
        .fold(r.fixed_periods.len(), |w, p| {
            w.saturating_add(1).saturating_add(match &p.coupon {
                Coupon::Indexed { observations, .. } => observations.len(),
                _ => 0,
            })
        })
}
fn trade_work(t: &Trade) -> usize {
    match t {
        Trade::Swap { request } => swap_work(request),
        Trade::EuropeanSwaption { request } => request
            .states
            .iter()
            .fold(0usize, |w, s| w.saturating_add(swap_work(&s.underlying))),
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => terminal
            .states
            .iter()
            .fold(0usize, |w, s| w.saturating_add(swap_work(&s.underlying)))
            .saturating_add(delivered_underlying.as_ref().map_or(0, |u| swap_work(u))),
    }
}
fn dates(t: &Trade) -> Vec<i32> {
    match t {
        Trade::Swap { request } => request
            .fixed_periods
            .iter()
            .map(|p| p.payment_ordinal)
            .chain(request.floating_periods.iter().map(|p| p.payment_ordinal))
            .collect(),
        Trade::EuropeanSwaption { request } => {
            vec![request.expiry_ordinal, request.settlement_ordinal]
        }
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => {
            let mut dates = vec![terminal.expiry_ordinal, terminal.settlement_ordinal];
            if let Some(u) = delivered_underlying {
                dates.extend(u.fixed_periods.iter().map(|p| p.payment_ordinal));
                dates.extend(u.floating_periods.iter().map(|p| p.payment_ordinal));
            }
            dates
        }
    }
}
pub(super) fn admit(r: &DerivativeAccountingRequest) -> Result<(), String> {
    let mut bytes = 0;
    for i in [&r.run, &r.policy.identity, &r.opening.source] {
        identity(i, &mut bytes)?;
    }
    for s in [&r.scenario_id, &r.entity] {
        text(s, &mut bytes)?;
    }
    if r.schema != super::SCHEMA
        || r.model_version != super::MODEL_VERSION
        || r.measure != CashflowMeasure::Physical
        || r.monetary_unit != "currency_units"
        || r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err("derivative accounting requires explicit compatible Physical schema/model/currency/units".into());
    }
    let c = csa_request(&r.derivative)?;
    if c.measure != CashflowMeasure::Physical
        || c.paths.len() != 1
        || c.paths[0].probability != 1.
        || c.paths[0].path_id != r.scenario_id
        || c.policy.entity != r.entity
        || c.policy.currency != r.currency
        || c.policy.monetary_unit != r.monetary_unit
        || c.asof_ordinal != r.opening.ordinal
        || c.mapping.is_empty()
        || c.mapping.len() > 256
        || c.paths[0].nodes.is_empty()
        || c.paths[0].nodes.len() > 4096
        || r.policy.trades.len() != c.mapping.len()
        || r.opening.trades.len() != c.mapping.len()
        || r.physical_index_paths.len() > 4096
    {
        return Err("daily accounting needs one complete Physical path/probability1 and exact opening/entity/currency/legal trade coverage".into());
    }
    domain(c.asof_ordinal, c.policy.tail_return_ordinal)?;
    if c.paths[0].nodes.len()
        != (i64::from(c.policy.tail_return_ordinal) - i64::from(c.asof_ordinal) + 1) as usize
        || c.paths[0].nodes.first().map(|n| n.ordinal) != Some(c.asof_ordinal)
        || c.paths[0].nodes.last().map(|n| n.ordinal) != Some(c.policy.tail_return_ordinal)
        || c.paths[0]
            .nodes
            .windows(2)
            .any(|n| i64::from(n[1].ordinal) - i64::from(n[0].ordinal) != 1)
    {
        return Err("daily accounting requires every calendar-day raw mark through the complete tail; no missing-mark interpolation".into());
    }
    for value in [r.opening.cash, r.opening.equity_credit_balance] {
        amount(value)?;
    }
    for value in [
        r.opening.posted_vm_asset,
        r.opening.received_vm_liability,
        r.opening.posted_im_asset,
        r.opening.received_im_custody,
    ] {
        nonnegative(value)?;
    }
    let mut reviews = HashSet::new();
    for p in &r.policy.trades {
        text(&p.position_id, &mut bytes)?;
        identity(&p.designation_review, &mut bytes)?;
        if let Some(d) = &p.delivery_review {
            identity(d, &mut bytes)?;
        }
        if !reviews.insert(&p.position_id)
            || !c.mapping.iter().any(|m| m.position_id == p.position_id)
        {
            return Err("reviewed designation coverage/identity mismatch".into());
        }
        if !matches!(
            p.designation,
            Designation::UndesignatedTrading | Designation::UndesignatedEconomicHedge
        ) {
            return Err("formal hedge accounting requires effectiveness, hedged-item and OCI/basis mechanics; unsupported designation rejected".into());
        }
    }
    let mut work = 0usize;
    for raw in std::iter::once(&r.derivative).chain(r.pricing_xva_reference.iter().map(Box::as_ref))
    {
        let source = csa_request(raw)?;
        if source.paths.len() > 256 || source.mapping.len() > 256 {
            return Err("accounting raw XVA/lifecycle path bound exceeded".into());
        }
        for path in &source.paths {
            if path.nodes.len() > 4096 {
                return Err("accounting raw node bound exceeded".into());
            }
            if let Some(m) = path.default.as_ref().and_then(|d| d.mpor.as_ref()) {
                work = m
                    .physical_index_paths
                    .iter()
                    .fold(work, |w, p| w.saturating_add(p.observations.len()));
            }
            for node in &path.nodes {
                if node.trades.len() > 256 {
                    return Err("accounting raw trade/node bound exceeded".into());
                }
                for trade in &node.trades {
                    let w = trade_work(trade);
                    work = work.saturating_add(w);
                    if path.default.as_ref().is_some_and(|d| {
                        d.mpor.is_some()
                            && node.ordinal >= d.default_ordinal
                            && node.ordinal <= d.closeout_ordinal
                    }) {
                        work = work.saturating_add(w);
                    }
                    if work > 250000 {
                        return Err("combined derivative accounting native work exceeds 250000 before schedule allocation".into());
                    }
                    let due_count = dates(trade).iter().filter(|d| **d == node.ordinal).count();
                    if due_count > 0 {
                        // Independent MPOR right repricing is charged for every due event,
                        // including multiple same-day coupons and delivered option legs.
                        work = work.saturating_add(w.saturating_mul(due_count));
                    }
                }
                if work > 250000 {
                    return Err("combined derivative accounting native work exceeds 250000 before schedule allocation".into());
                }
            }
        }
    }
    work = r
        .physical_index_paths
        .iter()
        .fold(work, |w, p| w.saturating_add(p.observations.len()));
    if work > 250000 {
        return Err("combined Physical observation work exceeds 250000 before allocation".into());
    }
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?;
    if raw.len() > 32 * 1024 * 1024 {
        return Err("derivative accounting immutable request exceeds 32 MiB".into());
    }
    let admitted_dates = c.paths[0]
        .nodes
        .iter()
        .map(|n| n.ordinal)
        .collect::<HashSet<_>>();
    for node in &c.paths[0].nodes {
        for t in &node.trades {
            for date in dates(t) {
                if date >= c.asof_ordinal
                    && date <= c.policy.tail_return_ordinal
                    && !admitted_dates.contains(&date)
                {
                    return Err("daily accounting requires every contractual coupon/European event date in the full tail".into());
                }
            }
            if let Trade::ExpiredEuropean {
                terminal,
                delivered_underlying: Some(_),
                ..
            } = t
            {
                let review = r
                    .policy
                    .trades
                    .iter()
                    .find(|p| p.position_id == terminal.position_id)
                    .ok_or("missing delivery review")?;
                if review.delivery_review.is_none() {
                    return Err(
                        "physical delivery requires explicit reviewed successor recognition".into(),
                    );
                }
            }
        }
    }
    let mut anchors = HashSet::new();
    for a in &r.opening.trades {
        text(&a.position_id, &mut bytes)?;
        amount(a.clean_value)?;
        if !anchors.insert(&a.position_id) || !reviews.contains(&a.position_id) {
            return Err(
                "opening clean carrying anchors must exactly cover reviewed mapped positions"
                    .into(),
            );
        }
    }
    let mut physical = HashSet::new();
    for p in &r.physical_index_paths {
        text(&p.position_id, &mut bytes)?;
        text(&p.period_id, &mut bytes)?;
        identity(&p.path, &mut bytes)?;
        if !physical.insert((&p.position_id, &p.period_id)) {
            return Err("duplicate accounting Physical fixing path".into());
        }
    }
    if let Some(reference) = &r.pricing_xva_reference {
        let x = csa_request(reference)?;
        if x.measure != CashflowMeasure::Pricing
            || x.asof_ordinal != c.asof_ordinal
            || x.policy.currency != c.policy.currency
            || x.policy.entity != c.policy.entity
            || x.mapping.len() != c.mapping.len()
            || serde_json::to_string(&x.mapping).map_err(|e| e.to_string())?
                != serde_json::to_string(&c.mapping).map_err(|e| e.to_string())?
        {
            return Err("report-only pricing XVA reference must share exact base/legal identity and Pricing measure".into());
        }
    }
    Ok(())
}
pub(super) struct CashInputs<'a> {
    paths: HashMap<(String, String), &'a csa::PhysicalIndexPath>,
    used: HashSet<(String, String)>,
}
impl<'a> CashInputs<'a> {
    pub fn new(r: &'a DerivativeAccountingRequest) -> Self {
        Self {
            paths: r
                .physical_index_paths
                .iter()
                .map(|p| ((p.position_id.clone(), p.period_id.clone()), p))
                .collect(),
            used: HashSet::new(),
        }
    }
    pub fn finish(&self) -> Result<(), String> {
        if self.used.len() != self.paths.len() {
            return Err("unused Physical accounting fixing path; no cash inferred from unmatched observations".into());
        }
        Ok(())
    }
    pub fn due(&mut self, t: &Trade, node: &Node) -> Result<Vec<CashRight>, String> {
        match t {
            Trade::Swap { request } => self.swap_due(position(t), request, node),
            Trade::EuropeanSwaption { .. } => Ok(Vec::new()),
            Trade::ExpiredEuropean {
                terminal,
                delivered_underlying,
                ..
            } => {
                if terminal.settlement == options::Settlement::CollateralizedCashPrice
                    && terminal.settlement_ordinal == node.ordinal
                {
                    let result = options::value_european(terminal)?;
                    let state = result
                        .states
                        .first()
                        .ok_or("missing selected European expiry state")?;
                    let sign = if terminal.side == options::OptionSide::Long {
                        1.
                    } else {
                        -1.
                    };
                    Ok(vec![CashRight {
                        position_id: position(t).into(),
                        contract_position_id: position(t).into(),
                        event_id: format!("option_cash:{}", state.state_id),
                        payment_ordinal: node.ordinal,
                        physical_amount: amount(sign * state.settlement_cash)?,
                        pricing_due_amount: amount(sign * state.settlement_cash)?,
                        contract: terminal.contract.clone(),
                        recognition_sources: source_ids(t),
                        sources: Vec::new(),
                        physical_path: None,
                        settled_ordinal: Some(node.ordinal),
                        closeout_ordinal: None,
                    }])
                } else if let Some(delivered) = delivered_underlying {
                    let mut rights = self.swap_due(position(t), delivered, node)?;
                    for right in &mut rights {
                        right.recognition_sources.extend(source_ids(t));
                    }
                    Ok(rights)
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
    fn swap_due(
        &mut self,
        mapped: &str,
        r: &swaps::SwapRequest,
        node: &Node,
    ) -> Result<Vec<CashRight>, String> {
        if !r
            .fixed_periods
            .iter()
            .any(|p| p.payment_ordinal == node.ordinal)
            && !r
                .floating_periods
                .iter()
                .any(|p| p.payment_ordinal == node.ordinal)
        {
            return Ok(Vec::new());
        }
        let mut before = r.clone();
        before.payment_timing = swaps::PaymentTiming::BeforeScheduledPayments;
        let priced = swaps::value_swap(&before)?;
        let mut result = Vec::new();
        for e in priced
            .schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal == node.ordinal)
        {
            let mut physical = e.components.cash_total()?;
            let mut sources = Vec::new();
            let mut physical_path = None;
            if let Some(period_id) = e.event_id.strip_prefix("floating:") {
                let p = r
                    .floating_periods
                    .iter()
                    .find(|p| p.period_id == period_id)
                    .ok_or("missing mapped floating cash period")?;
                let mut period = p.clone();
                let mut path = r.rate_path.clone();
                if let Some(actual) = self.paths.get(&(mapped.into(), period_id.into())) {
                    let Coupon::Indexed { observations, .. } = &mut period.coupon else {
                        return Err("Physical fixing path requires indexed cash contract".into());
                    };
                    *observations = actual.observations.clone();
                    path = actual.path.clone();
                    physical_path = Some(path.clone());
                    self.used.insert((mapped.into(), period_id.into()));
                }
                let Coupon::Indexed { observations, .. } = &period.coupon else {
                    return Err("missing actual indexed cash observations".into());
                };
                for o in observations {
                    if let RateSource::ConditionalProjection {
                        measure,
                        information_unix_seconds,
                        ..
                    } = &o.source
                    {
                        if *measure != CashflowMeasure::Physical
                            || *information_unix_seconds > node.valuation_cutoff_unix_seconds
                        {
                            return Err("Physical daily journal cash needs observed fixings or explicit Physical observation path, not Pricing projections".into());
                        }
                    }
                }
                let actual = build_path(&ContractPathRequest {
                    schema: crate::term_enhancements::SCHEMA.into(),
                    model_version: crate::term_enhancements::MODEL_VERSION.into(),
                    contract: r.contract.clone(),
                    assumption: r.index_basis.clone(),
                    path,
                    position_id: r.position_id.clone(),
                    scenario_id: r.scenario_id.clone(),
                    currency: r.currency.clone(),
                    monetary_unit: r.monetary_unit.clone(),
                    direction: Direction::Receipt,
                    measure: CashflowMeasure::Physical,
                    asof_ordinal: node.ordinal,
                    reporting_end_ordinal: node.ordinal,
                    tail_end_ordinal: node.ordinal.max(period.end_ordinal),
                    observed_publication_cutoff_unix_seconds: node.valuation_cutoff_unix_seconds,
                    opening_principal: r.notional,
                    principal_plan: PrincipalPlan::Supplied {
                        principal_payments: vec![r.notional],
                    },
                    periods: vec![period],
                })?;
                let sign = if r.side == swaps::Side::Payer {
                    1.
                } else {
                    -1.
                };
                physical = actual
                    .schedule
                    .events
                    .iter()
                    .try_fold(0., |a, e| amount(a + sign * e.components.cash_interest))?;
                sources = actual
                    .audit
                    .into_iter()
                    .flat_map(|a| a.selected_rate_sources)
                    .collect();
            }
            result.push(CashRight {
                position_id: mapped.into(),
                contract_position_id: r.position_id.clone(),
                event_id: e.event_id.clone(),
                payment_ordinal: node.ordinal,
                physical_amount: physical,
                pricing_due_amount: e.components.cash_total()?,
                contract: r.contract.clone(),
                recognition_sources: vec![
                    r.contract.clone(),
                    r.index_basis.clone(),
                    r.rate_path.clone(),
                    r.discount_curve.source.clone(),
                ],
                sources,
                physical_path,
                settled_ordinal: Some(node.ordinal),
                closeout_ordinal: None,
            });
        }
        Ok(result)
    }
}
pub(super) fn source_ids(t: &Trade) -> Vec<Identity> {
    match t {
        Trade::Swap { request } => vec![
            request.contract.clone(),
            request.index_basis.clone(),
            request.rate_path.clone(),
            request.discount_curve.source.clone(),
        ],
        Trade::EuropeanSwaption { request } => vec![
            request.contract.clone(),
            request.exercise_policy.clone(),
            request.conditional_model.clone(),
            request.crn.clone(),
        ],
        Trade::ExpiredEuropean {
            terminal,
            election,
            delivered_underlying,
            ..
        } => {
            let mut sources = vec![
                terminal.contract.clone(),
                terminal.exercise_policy.clone(),
                terminal.conditional_model.clone(),
                election.clone(),
            ];
            for state in &terminal.states {
                sources.extend([
                    state.discount_source.clone(),
                    state.underlying.discount_curve.source.clone(),
                ]);
            }
            if let Some(u) = delivered_underlying {
                sources.extend([
                    u.contract.clone(),
                    u.index_basis.clone(),
                    u.rate_path.clone(),
                    u.discount_curve.source.clone(),
                ]);
            }
            sources
        }
    }
}
pub(super) fn raw_values(native: &csa::PathResult, date: i32) -> BTreeMap<String, f64> {
    native
        .exposures
        .iter()
        .filter(|e| e.ordinal == date)
        .flat_map(|e| e.clean_trade_values.iter().cloned())
        .collect()
}
