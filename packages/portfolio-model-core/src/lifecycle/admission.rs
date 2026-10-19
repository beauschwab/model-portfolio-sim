//! Strict admission of a `liquidity-lifecycle-1` plan before any financial generation:
//! original-byte duplicate-key refusal, typed strict decoding, bounds, exact mappings,
//! exclusive event owners, shared-resource allocation limits and an acyclic, ordered
//! settlement dependency graph.
use super::types::*;
use crate::transport::strict_json;
use crate::validation::limits::MAX_MONETARY_AMOUNT;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// An admitted plan: typed, mapped and allocation-checked. It is not a feasible plan;
/// feasibility is decided by settlement, not admission.
#[derive(Debug, Clone)]
pub struct Admitted {
    pub request: LifecycleRequest,
}

fn malformed(detail: impl Into<String>) -> Refusal {
    Refusal::new(MALFORMED_INPUT, detail)
}
fn missing(detail: impl Into<String>) -> Refusal {
    Refusal::new(LIFECYCLE_MAPPING_REQUIRED, detail)
}
fn money(x: f64, what: &str) -> Result<f64, Refusal> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err(malformed(format!(
            "{what} must be finite and within the monetary domain"
        )))
    }
}
fn positive(x: f64, what: &str) -> Result<f64, Refusal> {
    if money(x, what)? > 0. {
        Ok(x)
    } else {
        Err(malformed(format!("{what} must be positive")))
    }
}
fn text(x: &str, what: &str) -> Result<(), Refusal> {
    if x.is_empty() || x.len() > 256 || x.chars().any(char::is_control) {
        Err(malformed(format!("{what} must be nonempty bounded text")))
    } else {
        Ok(())
    }
}
fn bounded<T>(items: &[T], what: &str) -> Result<(), Refusal> {
    if items.len() > MAX_RECORDS {
        Err(Refusal::new(
            BOUND_EXCEEDED,
            format!("{what} exceed {MAX_RECORDS} records"),
        ))
    } else {
        Ok(())
    }
}
fn unique<'a>(ids: impl Iterator<Item = &'a str>, what: &str) -> Result<HashSet<&'a str>, Refusal> {
    let mut seen = HashSet::new();
    for id in ids {
        text(id, what)?;
        if !seen.insert(id) {
            return Err(Refusal::new(
                DUPLICATE_IDENTITY,
                format!("duplicate {what} `{id}`"),
            ));
        }
    }
    Ok(seen)
}
fn revision(r: &Revision, what: &str) -> Result<(), Refusal> {
    text(&r.id, what)?;
    text(&r.revision, what)
}

/// Peak concurrent allocation of each key over half-open effective intervals.
fn peak(intervals: &[(&str, i32, Option<i32>, f64)]) -> HashMap<String, f64> {
    let mut events: BTreeMap<&str, Vec<(i32, i8, f64)>> = BTreeMap::new();
    for &(key, start, end, amount) in intervals {
        let e = events.entry(key).or_default();
        e.push((start, 1, amount));
        if let Some(end) = end {
            // Ends sort before starts on the same date: intervals are half-open.
            e.push((end, 0, -amount));
        }
    }
    let mut out = HashMap::new();
    for (key, mut e) in events {
        e.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let (mut level, mut max) = (0f64, 0f64);
        for (_, _, delta) in e {
            level += delta;
            max = max.max(level);
        }
        out.insert(key.to_string(), max);
    }
    out
}
fn within(used: f64, capacity: f64) -> bool {
    used <= capacity + 1e-9 * capacity.abs().max(1.)
}

/// Topological order check; returns the first node found on a cycle.
fn acyclic<'a>(edges: &BTreeMap<&'a str, Vec<&'a str>>) -> Result<(), &'a str> {
    let mut state: HashMap<&str, u8> = HashMap::new();
    for &start in edges.keys() {
        if state.get(start).copied().unwrap_or(0) == 2 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state.insert(start, 1);
        while let Some((node, i)) = stack.pop() {
            let next = edges.get(node).map_or(&[][..], |v| v.as_slice());
            if i < next.len() {
                stack.push((node, i + 1));
                let child = next[i];
                match state.get(child).copied().unwrap_or(0) {
                    1 => return Err(child),
                    0 => {
                        state.insert(child, 1);
                        stack.push((child, 0));
                    }
                    _ => {}
                }
            } else {
                state.insert(node, 2);
            }
        }
    }
    Ok(())
}

pub fn admit(bytes: &[u8]) -> Result<Admitted, Refusal> {
    let value = strict_json::parse(bytes, MAX_RAW_BYTES).map_err(|e| {
        if e.contains("exceeds") {
            Refusal::new(BOUND_EXCEEDED, e)
        } else {
            malformed(e)
        }
    })?;
    let request: LifecycleRequest =
        serde_json::from_value(value).map_err(|e| malformed(e.to_string()))?;
    validate(&request)?;
    Ok(Admitted { request })
}

pub fn validate(r: &LifecycleRequest) -> Result<(), Refusal> {
    if r.schema != SCHEMA || r.model_version != SCHEMA {
        return Err(Refusal::new(
            UNSUPPORTED_POLICY,
            "explicit liquidity-lifecycle-1 selection required",
        ));
    }
    if r.monetary_unit != "currency_units" {
        return Err(Refusal::new(
            UNSUPPORTED_POLICY,
            "monetary unit must be currency_units",
        ));
    }
    text(&r.entity, "entity")?;
    if r.currency.len() != 3 || !r.currency.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(malformed("currency must be three uppercase letters"));
    }
    for (x, what) in [
        (&r.workspace_revision, "workspace revision"),
        (&r.saved_book_revision, "saved book revision"),
        (&r.settlement_policy, "settlement policy"),
    ] {
        revision(x, what)?;
    }
    bounded(&r.sources, "sources")?;
    bounded(&r.accounts, "accounts")?;
    bounded(&r.lots, "lots")?;
    bounded(&r.hedge_allocations, "hedge allocations")?;
    bounded(&r.funding_allocations, "funding allocations")?;
    bounded(&r.pledges, "pledges")?;
    bounded(&r.margin_agreements, "margin agreements")?;
    bounded(&r.facilities, "facilities")?;
    bounded(&r.formal_relationships, "formal relationships")?;
    bounded(&r.actions, "actions")?;
    bounded(&r.obligations, "obligations")?;

    let sources: HashMap<&str, &SourceRef> = r
        .sources
        .iter()
        .map(|s| (s.source_id.as_str(), s))
        .collect();
    unique(r.sources.iter().map(|s| s.source_id.as_str()), "source")?;
    for s in &r.sources {
        text(&s.revision, "source revision")?;
        if s.sha256.len() != 64
            || !s
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(malformed(format!(
                "source `{}` sha256 must be 64 lowercase hex digits",
                s.source_id
            )));
        }
    }
    let accounts = unique(r.accounts.iter().map(|a| a.account_id.as_str()), "account")?;
    for a in &r.accounts {
        money(a.cash, "opening cash")?;
        money(a.equity, "opening equity")?;
    }

    unique(r.lots.iter().map(|l| l.lot_id.as_str()), "lot")?;
    let lots: HashMap<&str, &PositionLot> = r.lots.iter().map(|l| (l.lot_id.as_str(), l)).collect();
    for l in &r.lots {
        text(&l.position_id, "position")?;
        match sources.get(l.source_id.as_str()) {
            None => {
                return Err(missing(format!(
                    "lot `{}` names unknown source `{}`",
                    l.lot_id, l.source_id
                )))
            }
            // A Pricing valuation or projection is not settlement evidence.
            Some(s) if s.measure != Measure::Physical => {
                return Err(Refusal::new(
                    UNSUPPORTED_POLICY,
                    format!("lot `{}` needs a Physical source", l.lot_id),
                ))
            }
            Some(_) => {}
        }
        if !accounts.contains(l.account_id.as_str()) {
            return Err(missing(format!(
                "lot `{}` names unknown account `{}`",
                l.lot_id, l.account_id
            )));
        }
        positive(l.quantity, "lot quantity")?;
        money(l.original_cost, "original cost")?;
        money(l.original_effective_yield, "original effective yield")?;
        money(l.original_oas, "original OAS")?;
        if money(l.outside_encumbrance, "outside encumbrance")? < 0.
            || !within(l.outside_encumbrance, l.quantity)
        {
            return Err(malformed(format!(
                "lot `{}` outside encumbrance must lie in [0, quantity]",
                l.lot_id
            )));
        }
        let consistent = match l.kind {
            LotKind::Derivative => l.accounting_category == AccountingCategory::Derivative,
            LotKind::Funding => l.accounting_category == AccountingCategory::FundingLiability,
            LotKind::Security | LotKind::Loan => !matches!(
                l.accounting_category,
                AccountingCategory::Derivative | AccountingCategory::FundingLiability
            ),
        };
        if !consistent {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!("lot `{}` kind and accounting category differ", l.lot_id),
            ));
        }
    }
    let lot = |id: &str, what: &str| -> Result<&PositionLot, Refusal> {
        lots.get(id)
            .copied()
            .ok_or_else(|| missing(format!("{what} names unknown lot `{id}`")))
    };

    let formal: HashMap<&str, &FormalRelationship> = r
        .formal_relationships
        .iter()
        .map(|f| (f.relationship_id.as_str(), f))
        .collect();
    unique(
        r.formal_relationships
            .iter()
            .map(|f| f.relationship_id.as_str()),
        "formal relationship",
    )?;
    for f in &r.formal_relationships {
        text(&f.revision, "formal relationship revision")?;
        revision(&f.adoption_policy, "adoption policy")?;
        if sources.get(f.accounting_source.source_id.as_str()) != Some(&&f.accounting_source)
            || f.accounting_source.measure != Measure::Physical
        {
            return Err(missing(format!(
                "formal relationship `{}` accounting source must be a listed immutable source",
                f.relationship_id
            )));
        }
        if f.hedging_lot_ids.is_empty()
            || (f.hedged_lot_ids.is_empty() == f.forecast_exposure_id.is_none())
        {
            return Err(malformed(format!(
                "formal relationship `{}` needs hedging lots and exactly one of hedged lots or a forecast exposure",
                f.relationship_id
            )));
        }
        for id in &f.hedging_lot_ids {
            if lot(id, "formal relationship")?.kind != LotKind::Derivative {
                return Err(Refusal::new(
                    UNSUPPORTED_POLICY,
                    "formal hedging instruments must be derivative lots",
                ));
            }
        }
        for id in &f.hedged_lot_ids {
            lot(id, "formal relationship")?;
        }
    }

    unique(
        r.hedge_allocations.iter().map(|a| a.allocation_id.as_str()),
        "hedge allocation",
    )?;
    let mut exposure_use = Vec::new();
    let mut hedge_use = Vec::new();
    for a in &r.hedge_allocations {
        revision(&a.review, "hedge allocation review")?;
        positive(a.amount, "hedge allocation amount")?;
        let exposure = lot(&a.exposure_lot_id, "hedge allocation")?;
        let hedge = lot(&a.hedge_lot_id, "hedge allocation")?;
        if hedge.kind != LotKind::Derivative || exposure.kind == LotKind::Derivative {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!(
                    "hedge allocation `{}` must link a derivative lot to a non-derivative exposure",
                    a.allocation_id
                ),
            ));
        }
        if a.effective_end_ordinal
            .is_some_and(|e| e <= a.effective_start_ordinal)
        {
            return Err(malformed(format!(
                "hedge allocation `{}` interval is empty",
                a.allocation_id
            )));
        }
        if let Some(id) = &a.formal_relationship_id {
            let f = formal.get(id.as_str()).ok_or_else(|| {
                missing(format!(
                    "hedge allocation `{}` names unknown formal relationship `{id}`",
                    a.allocation_id
                ))
            })?;
            if !f.hedging_lot_ids.contains(&a.hedge_lot_id)
                || (f.forecast_exposure_id.is_none()
                    && !f.hedged_lot_ids.contains(&a.exposure_lot_id))
            {
                return Err(missing(format!(
                    "hedge allocation `{}` lots are not part of formal relationship `{id}`",
                    a.allocation_id
                )));
            }
        }
        exposure_use.push((
            a.exposure_lot_id.as_str(),
            a.effective_start_ordinal,
            a.effective_end_ordinal,
            a.amount,
        ));
        hedge_use.push((
            a.hedge_lot_id.as_str(),
            a.effective_start_ordinal,
            a.effective_end_ordinal,
            a.amount,
        ));
    }

    unique(
        r.funding_allocations
            .iter()
            .map(|a| a.allocation_id.as_str()),
        "funding allocation",
    )?;
    let mut funding_use = Vec::new();
    let mut attributed_use = Vec::new();
    for a in &r.funding_allocations {
        positive(a.amount, "funding allocation amount")?;
        let funding = lot(&a.funding_lot_id, "funding allocation")?;
        let attributed = lot(&a.attributed_lot_id, "funding allocation")?;
        if funding.kind != LotKind::Funding
            || matches!(attributed.kind, LotKind::Funding | LotKind::Derivative)
        {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!(
                    "funding allocation `{}` must attribute a funding lot to an asset lot",
                    a.allocation_id
                ),
            ));
        }
        if a.effective_end_ordinal
            .is_some_and(|e| e <= a.effective_start_ordinal)
        {
            return Err(malformed(format!(
                "funding allocation `{}` interval is empty",
                a.allocation_id
            )));
        }
        funding_use.push((
            a.funding_lot_id.as_str(),
            a.effective_start_ordinal,
            a.effective_end_ordinal,
            a.amount,
        ));
        attributed_use.push((
            a.attributed_lot_id.as_str(),
            a.effective_start_ordinal,
            a.effective_end_ordinal,
            a.amount,
        ));
    }
    for (uses, what) in [
        (&exposure_use, "hedge allocations on exposure lot"),
        (&hedge_use, "hedge allocations on hedging lot"),
        (&funding_use, "funding allocations on funding lot"),
        (&attributed_use, "funding allocations on attributed lot"),
    ] {
        for (id, used) in peak(uses) {
            if !within(used, lots[id.as_str()].quantity) {
                return Err(Refusal::new(
                    OVERALLOCATION,
                    format!("{what} `{id}` exceed its quantity"),
                ));
            }
        }
    }

    let agreements: HashMap<&str, &MarginAgreement> = r
        .margin_agreements
        .iter()
        .map(|a| (a.agreement_id.as_str(), a))
        .collect();
    unique(
        r.margin_agreements.iter().map(|a| a.agreement_id.as_str()),
        "margin agreement",
    )?;
    let mut scoped = HashSet::new();
    for a in &r.margin_agreements {
        text(&a.counterparty, "counterparty")?;
        revision(&a.legal_review, "legal review")?;
        revision(&a.calendar, "calendar")?;
        for id in &a.lot_ids {
            if lot(id, "margin agreement")?.kind != LotKind::Derivative {
                return Err(Refusal::new(
                    UNSUPPORTED_POLICY,
                    "margin agreements scope derivative lots",
                ));
            }
            if !scoped.insert(id.as_str()) {
                return Err(Refusal::new(
                    DUPLICATE_IDENTITY,
                    format!("lot `{id}` is in two legal margin scopes"),
                ));
            }
        }
    }

    unique(r.pledges.iter().map(|p| p.pledge_id.as_str()), "pledge")?;
    let mut pledged: HashMap<&str, f64> = HashMap::new();
    for p in &r.pledges {
        text(&p.holder, "pledge holder")?;
        revision(&p.eligibility, "pledge eligibility")?;
        positive(p.quantity, "pledge quantity")?;
        if money(p.haircut, "haircut")? < 0. || p.haircut >= 1. {
            return Err(malformed("haircut must lie in [0, 1)"));
        }
        if lot(&p.lot_id, "pledge")?.kind != LotKind::Security {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                "only security lots are pledged",
            ));
        }
        let agreement_known = agreements.contains_key(p.agreement_id.as_str())
            || lots
                .get(p.agreement_id.as_str())
                .is_some_and(|l| l.kind == LotKind::Funding);
        if !agreement_known {
            return Err(missing(format!(
                "pledge `{}` names unknown agreement `{}`",
                p.pledge_id, p.agreement_id
            )));
        }
        *pledged.entry(p.lot_id.as_str()).or_default() += p.quantity;
    }
    for (id, used) in &pledged {
        let l = lots[id];
        if !within(used + l.outside_encumbrance, l.quantity) {
            return Err(Refusal::new(
                OVERALLOCATION,
                format!("pledges and outside encumbrance on lot `{id}` exceed its quantity"),
            ));
        }
    }

    let facilities: HashMap<&str, &Facility> = r
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f))
        .collect();
    unique(
        r.facilities.iter().map(|f| f.facility_id.as_str()),
        "facility",
    )?;
    for f in &r.facilities {
        text(&f.provider, "facility provider")?;
        revision(&f.access_policy, "facility access policy")?;
        positive(f.commitment, "facility commitment")?;
        if money(f.drawn, "facility drawn")? < 0. || !within(f.drawn, f.commitment) {
            return Err(Refusal::new(
                OVERALLOCATION,
                format!(
                    "facility `{}` is drawn beyond its commitment",
                    f.facility_id
                ),
            ));
        }
    }

    unique(r.actions.iter().map(|a| a.action_id.as_str()), "action")?;
    let actions: HashMap<&str, &LifecycleAction> = r
        .actions
        .iter()
        .map(|a| (a.action_id.as_str(), a))
        .collect();
    let mut sold: HashMap<&str, f64> = HashMap::new();
    let mut action_edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for a in &r.actions {
        revision(&a.policy, "action policy")?;
        if !(r.as_of_ordinal <= a.trade_ordinal
            && a.trade_ordinal <= a.effective_ordinal
            && a.effective_ordinal <= a.settlement_ordinal)
        {
            return Err(malformed(format!(
                "action `{}` needs as-of <= trade <= effective <= settlement",
                a.action_id
            )));
        }
        if a.lots.is_empty() || a.depends_on.len() > MAX_DEPENDENCIES {
            return Err(malformed(format!(
                "action `{}` needs lots and bounded dependencies",
                a.action_id
            )));
        }
        let mut seen = HashSet::new();
        for x in &a.lots {
            let l = lot(&x.lot_id, "action")?;
            positive(x.quantity, "action quantity")?;
            if !seen.insert(x.lot_id.as_str()) || !within(x.quantity, l.quantity) {
                return Err(malformed(format!(
                    "action `{}` lot `{}` repeats or exceeds its quantity",
                    a.action_id, x.lot_id
                )));
            }
            if a.kind == ActionKind::Unwind {
                let total = sold.entry(x.lot_id.as_str()).or_default();
                *total += x.quantity;
                if !within(*total, l.quantity) {
                    return Err(Refusal::new(
                        OVERALLOCATION,
                        format!("unwinds of lot `{}` exceed its quantity", x.lot_id),
                    ));
                }
            }
            if a.accounting_treatment == AccountingTreatment::Formal {
                // Every hedge allocation touching the lot needs its formal owner; no fallback.
                let touching = r
                    .hedge_allocations
                    .iter()
                    .filter(|h| h.exposure_lot_id == x.lot_id || h.hedge_lot_id == x.lot_id);
                for h in touching {
                    if h.formal_relationship_id.is_none() {
                        return Err(Refusal::new(FORMAL_OWNER_REQUIRED, format!(
                            "formal treatment of action `{}` needs a formal relationship for allocation `{}`",
                            a.action_id, h.allocation_id)));
                    }
                }
            }
        }
        for d in &a.depends_on {
            if !actions.contains_key(d.as_str()) {
                return Err(missing(format!(
                    "action `{}` depends on unknown action `{d}`",
                    a.action_id
                )));
            }
        }
        action_edges.insert(
            a.action_id.as_str(),
            a.depends_on.iter().map(String::as_str).collect(),
        );
    }
    acyclic(&action_edges).map_err(|n| {
        Refusal::new(
            SETTLEMENT_DEPENDENCY_CYCLE,
            format!("action dependency cycle through `{n}`"),
        )
    })?;

    unique(
        r.obligations.iter().map(|o| o.obligation_id.as_str()),
        "obligation",
    )?;
    unique(
        r.obligations.iter().map(|o| o.source_event.as_str()),
        "obligation source event",
    )
    .map_err(|e| {
        Refusal::new(
            DUPLICATE_IDENTITY,
            format!("every source event has one exclusive owner: {}", e.detail),
        )
    })?;
    let pledges: HashMap<&str, &Pledge> = r
        .pledges
        .iter()
        .map(|p| (p.pledge_id.as_str(), p))
        .collect();
    let obligations: HashMap<&str, &SettlementObligation> = r
        .obligations
        .iter()
        .map(|o| (o.obligation_id.as_str(), o))
        .collect();
    let mut coordinates = BTreeSet::new();
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for o in &r.obligations {
        let id = o.obligation_id.as_str();
        if !accounts.contains(o.account_id.as_str()) {
            return Err(missing(format!("obligation `{id}` names unknown account")));
        }
        money(o.cash, "obligation cash")?;
        money(o.quantity, "obligation quantity")?;
        if o.due_ordinal < r.as_of_ordinal || o.depends_on.len() > MAX_DEPENDENCIES {
            return Err(malformed(format!(
                "obligation `{id}` is before the as-of date or has too many dependencies"
            )));
        }
        if !coordinates.insert((o.account_id.as_str(), o.due_ordinal, o.phase, o.sequence)) {
            return Err(Refusal::new(
                SETTLEMENT_ORDER,
                format!("obligation `{id}` repeats an event coordinate"),
            ));
        }
        if let Some(a) = &o.action_id {
            if !actions.contains_key(a.as_str()) {
                return Err(missing(format!(
                    "obligation `{id}` names unknown action `{a}`"
                )));
            }
        }
        let cash_sign = |sign: f64| sign * o.cash > 0.;
        use ObligationKind as K;
        let shape = match o.kind {
            K::SecurityDelivery | K::SecurityReceipt => {
                o.cash == 0.
                    && o.quantity > 0.
                    && o.lot_id
                        .as_deref()
                        .is_some_and(|l| lots.get(l).is_some_and(|l| l.kind == LotKind::Security))
            }
            K::CollateralRelease => {
                o.cash == 0.
                    && o.quantity > 0.
                    && o.pledge_id
                        .as_deref()
                        .and_then(|p| pledges.get(p))
                        .is_some_and(|p| within(o.quantity, p.quantity))
            }
            K::FinancingDraw => {
                cash_sign(1.)
                    && o.facility_id
                        .as_deref()
                        .is_some_and(|f| facilities.contains_key(f))
            }
            K::FinancingRepayment => {
                cash_sign(-1.)
                    && o.facility_id
                        .as_deref()
                        .is_none_or(|f| facilities.contains_key(f))
            }
            K::CashReceipt | K::TerminationReceipt | K::CollateralReturnReceived => cash_sign(1.),
            K::CashPayment | K::TerminationPayment | K::CollateralReturnPaid => cash_sign(-1.),
        };
        if !shape {
            return Err(malformed(format!(
                "obligation `{id}` fields do not match its kind"
            )));
        }
        if matches!(
            o.kind,
            K::TerminationPayment
                | K::TerminationReceipt
                | K::CollateralReturnReceived
                | K::CollateralReturnPaid
        ) {
            let agreement = o
                .agreement_id
                .as_deref()
                .and_then(|a| agreements.get(a))
                .ok_or_else(|| missing(format!("obligation `{id}` needs its margin agreement")))?;
            let returns = matches!(
                o.kind,
                K::CollateralReturnReceived | K::CollateralReturnPaid
            );
            if returns != o.collateral.is_some() {
                return Err(malformed(format!(
                    "obligation `{id}` collateral kind applies to collateral returns only"
                )));
            }
            if returns
                && o.collateral == Some(CollateralKind::VariationMargin)
                && agreement.vm_at_termination == VmAtTermination::ApplyAgainstConsideration
                && r.obligations.iter().any(|x| {
                    x.agreement_id == o.agreement_id
                        && matches!(x.kind, K::TerminationPayment | K::TerminationReceipt)
                })
            {
                return Err(Refusal::new(DUPLICATE_COLLATERAL_CREDIT, format!(
                    "VM applied against termination consideration cannot also be returned (`{id}`)")));
            }
        } else if o.agreement_id.is_some() || o.collateral.is_some() {
            return Err(malformed(format!(
                "obligation `{id}` names a margin agreement it does not settle"
            )));
        }
        let mut deps = Vec::new();
        for d in &o.depends_on {
            let p = obligations
                .get(d.as_str())
                .ok_or_else(|| missing(format!("obligation `{id}` depends on unknown `{d}`")))?;
            deps.push(d.as_str());
            if (p.due_ordinal, p.phase, p.sequence) >= (o.due_ordinal, o.phase, o.sequence)
                && p.account_id == o.account_id
            {
                return Err(Refusal::new(
                    SETTLEMENT_ORDER,
                    format!("obligation `{id}` depends on `{d}`, which is not earlier"),
                ));
            }
        }
        edges.insert(id, deps);
    }
    acyclic(&edges).map_err(|n| {
        Refusal::new(
            SETTLEMENT_DEPENDENCY_CYCLE,
            format!("settlement dependency cycle through `{n}`"),
        )
    })?;
    Ok(())
}
