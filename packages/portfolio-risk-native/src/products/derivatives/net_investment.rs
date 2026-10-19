//! `net-investment-hedge-1`: translation of one foreign operation and ASC 815-35 net-investment
//! hedges, including the ASU 2025-09 dual hedge (815-20-55-38).
//!
//! The reporting entity translates the foreign operation's net assets at the closing spot rate
//! and its supplied flows at their supplied rates; the difference is translation CTA. Hedging
//! instruments are FX forwards (spot method with amortized or marked excluded forward points, or
//! the forward method) and foreign-currency debt (spot remeasurement, 830-20). The portion of an
//! instrument matching the hedged net investment goes to the hedge CTA; any excess goes to
//! earnings. Under a dual hedge the debt's fair-value-hedge basis adjustment comes from an embedded
//! `hedge-accounting-1` run in the foreign currency: its spot remeasurement goes to earnings while
//! that fair value hedge is active and joins the net-investment hedge after it is discontinued.
//! Cumulative CTA is released to earnings only on (partial) disposal of the foreign operation.
//! Contract and rule references: docs/models/net-investment-hedge.md.
use crate::products::derivatives::contract::{amount, identity, near, text, Identity};
use crate::products::derivatives::hedge_accounting::{
    self as hedge, HedgeAccountingRequest, ItemDirection, ItemKind, Status as HedgeStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SCHEMA: &str = "net-investment-hedge-1";
pub const MAX_INPUT: usize = 32 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub entity: String,
    pub reporting_currency: String,
    pub foreign_currency: String,
    pub monetary_unit: String,
    pub policy: Policy,
    pub dates: Vec<RateDate>,
    pub foreign_operation: ForeignOperation,
    pub instruments: Vec<Instrument>,
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub assessment_lower_bound: f64,
    pub assessment_upper_bound: f64,
    /// Entity adoption of the ASU 2025-09 dual-hedge amendment; required for any dual hedge.
    #[serde(default)]
    pub dual_hedge_adoption: Option<Adoption>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Adoption {
    pub review: Identity,
    pub adoption_ordinal: i32,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub ordinal: i32,
    pub discount_factor: f64,
}
/// One reporting date: the closing spot rate (reporting currency per foreign unit) and, while a
/// forward is live, both currencies' discount curves from this date (first point exactly one).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RateDate {
    pub ordinal: i32,
    pub spot: f64,
    pub spot_source: Identity,
    #[serde(default)]
    pub reporting_discount: Vec<Point>,
    #[serde(default)]
    pub foreign_discount: Vec<Point>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ForeignOperation {
    pub operation_id: String,
    pub source: Identity,
    /// Net assets in the foreign (functional) currency at the first date.
    pub opening_net_assets: f64,
    /// Reviewed cumulative translation adjustment at the first date (reporting currency, credit positive).
    pub opening_translation_cta: f64,
    #[serde(default)]
    pub movements: Vec<Movement>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementKind {
    Income,
    Contribution,
    Distribution,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    pub movement_id: String,
    pub ordinal: i32,
    pub kind: MovementKind,
    /// Foreign-currency amount. Income may be negative (a loss); flows are positive.
    pub amount: f64,
    /// Supplied translation rate for this flow (for example a period average for income).
    pub rate: f64,
    pub rate_source: Identity,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardSide {
    /// Deliver foreign currency at the forward rate: hedges a net investment (asset).
    SellForeign,
    BuyForeign,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Instrument {
    FxForward {
        position_id: String,
        source: Identity,
        side: ForwardSide,
        notional: f64,
        forward_rate: f64,
        trade_ordinal: i32,
        maturity_ordinal: i32,
    },
    /// Foreign-currency borrowing outstanding from the first date, repaid at maturity.
    ForeignDebt {
        position_id: String,
        source: Identity,
        notional: f64,
        maturity_ordinal: i32,
    },
}
impl Instrument {
    fn id(&self) -> &str {
        match self {
            Self::FxForward { position_id, .. } | Self::ForeignDebt { position_id, .. } => {
                position_id
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Spot,
    Forward,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Excluded {
    /// Initial excluded value recognized straight-line in earnings; the rest goes to CTA.
    Amortization,
    MarkToMarket,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub relationship_id: String,
    pub instrument_position_id: String,
    pub documentation: Identity,
    pub designation_ordinal: i32,
    /// Foreign-currency amount of the net investment designated as hedged.
    pub hedged_net_investment: f64,
    pub method: Method,
    #[serde(default)]
    pub excluded_component: Option<Excluded>,
    #[serde(default)]
    pub dual_fair_value_hedge: Option<DualHedge>,
}
/// The debt is also the hedged item in this fair value hedge of interest rate risk, run in the
/// foreign currency by the hedge-accounting owner.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DualHedge {
    pub relationship_id: String,
    pub request: Box<HedgeAccountingRequest>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Dedesignate,
    Disposal,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: String,
    pub ordinal: i32,
    pub kind: EventKind,
    pub review: Identity,
    #[serde(default)]
    pub relationship_id: Option<String>,
    /// Disposal: fraction of the remaining investment sold, in (0, 1].
    #[serde(default)]
    pub fraction: Option<f64>,
    /// Disposal: reporting-currency proceeds received.
    #[serde(default)]
    pub proceeds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Posting {
    pub transaction_id: String,
    pub ordinal: i32,
    pub event: String,
    pub account: String,
    pub position_id: Option<String>,
    pub relationship_id: Option<String>,
    pub debit: f64,
    pub credit: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Translation {
    pub ordinal: i32,
    pub spot: f64,
    pub net_assets: f64,
    pub carrying: f64,
    pub translation_adjustment: f64,
    pub translation_cta: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Period {
    pub ordinal: i32,
    pub relationship_id: String,
    pub status: String,
    pub hedged_effective: f64,
    pub designated_fraction: f64,
    pub included_change: f64,
    pub excluded_change: f64,
    pub cta: f64,
    pub excluded_earnings: f64,
    pub excess_earnings: f64,
    pub basis_remeasurement: f64,
    pub cumulative_ratio: Option<f64>,
    pub assessment_passed: Option<bool>,
}
#[derive(Debug, Clone, Serialize)]
pub struct InstrumentValue {
    pub ordinal: i32,
    pub position_id: String,
    pub carrying: f64,
    pub change: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct RelationshipClosing {
    pub relationship_id: String,
    pub status: String,
    pub discontinued_ordinal: Option<i32>,
    pub hedge_cta_recognized: f64,
    pub hedge_cta_released: f64,
    pub dual_hedge_applied: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Balance {
    pub account: String,
    pub debit_balance: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Result {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub entity: String,
    pub reporting_currency: String,
    pub foreign_currency: String,
    pub policy: Identity,
    pub journal: Vec<Posting>,
    pub translation: Vec<Translation>,
    pub periods: Vec<Period>,
    pub instruments: Vec<InstrumentValue>,
    pub relationships: Vec<RelationshipClosing>,
    pub closing: Vec<Balance>,
    pub dual_hedge_applied: bool,
    pub debt_interest_included: bool,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub capital_ftp_eligible: bool,
    pub disclosure: String,
}

fn check() -> std::result::Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn currency(c: &str) -> bool {
    c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())
}
fn finite_positive(x: f64) -> bool {
    x.is_finite() && x > 0.
}
fn point(points: &[Point], t: i32) -> std::result::Result<f64, String> {
    points
        .binary_search_by_key(&t, |p| p.ordinal)
        .map(|i| points[i].discount_factor)
        .map_err(|_| "missing exact discount point; no interpolation".into())
}
fn curve(points: &[Point], at: i32) -> std::result::Result<(), String> {
    if points.is_empty()
        || points.len() > 4096
        || points[0].ordinal != at
        || points[0].discount_factor != 1.
        || points.windows(2).any(|w| w[1].ordinal <= w[0].ordinal)
        || points.iter().any(|p| !finite_positive(p.discount_factor))
    {
        return Err(
            "discount curves start at one on their date with increasing positive points".into(),
        );
    }
    Ok(())
}
fn days(a: i32, b: i32) -> f64 {
    f64::from(b - a)
}

struct Ledger {
    postings: Vec<Posting>,
    balances: BTreeMap<String, f64>,
    seq: usize,
}
impl Ledger {
    /// One balanced transaction; each line is (account, debit-positive amount).
    #[allow(clippy::too_many_arguments)]
    fn post(
        &mut self,
        ordinal: i32,
        event: &str,
        position: Option<&str>,
        relationship: Option<&str>,
        lines: &[(&str, f64)],
    ) -> std::result::Result<(), String> {
        let lines: Vec<_> = lines.iter().filter(|(_, v)| *v != 0.).collect();
        if lines.is_empty() {
            return Ok(());
        }
        let total = lines.iter().try_fold(0., |a, (_, v)| amount(a + v))?;
        if !near(total, 0.) {
            return Err(format!("unbalanced {event} transaction"));
        }
        self.seq += 1;
        let id = format!("nih:{ordinal}:{:06}:{event}", self.seq);
        for (account, v) in lines {
            *self.balances.entry((*account).into()).or_default() += v;
            self.postings.push(Posting {
                transaction_id: id.clone(),
                ordinal,
                event: event.into(),
                account: (*account).into(),
                position_id: position.map(str::to_string),
                relationship_id: relationship.map(str::to_string),
                debit: v.max(0.),
                credit: (-v).max(0.),
            });
        }
        Ok(())
    }
}

/// Forward fair value in the reporting currency at date index `k` (zero before trade).
fn forward_value(r: &Request, inst: &Instrument, k: usize) -> std::result::Result<f64, String> {
    let Instrument::FxForward {
        side,
        notional,
        forward_rate,
        trade_ordinal,
        maturity_ordinal,
        ..
    } = inst
    else {
        return Err("not a forward".into());
    };
    let d = &r.dates[k];
    if d.ordinal < *trade_ordinal || d.ordinal > *maturity_ordinal {
        return Ok(0.);
    }
    let sign = if *side == ForwardSide::SellForeign {
        1.
    } else {
        -1.
    };
    if d.ordinal == *maturity_ordinal {
        return amount(sign * notional * (forward_rate - d.spot));
    }
    let p_rc = point(&d.reporting_discount, *maturity_ordinal)?;
    let p_fc = point(&d.foreign_discount, *maturity_ordinal)?;
    let fwd = d.spot * p_fc / p_rc;
    amount(sign * notional * (forward_rate - fwd) * p_rc)
}

struct Dual {
    /// Foreign-currency basis adjustment (item fair-value change) by date ordinal.
    basis: HashMap<i32, f64>,
    /// First date on which the fair value hedge is no longer qualified.
    discontinued: Option<i32>,
}
fn dual(r: &Request, rel: &Relationship, d: &DualHedge) -> std::result::Result<Dual, String> {
    let h = &d.request;
    if h.currency != r.foreign_currency || h.entity != r.entity {
        return Err(
            "dual fair value hedge must run in the foreign currency for the same entity".into(),
        );
    }
    let fvh = h
        .relationships
        .iter()
        .find(|x| x.relationship_id == d.relationship_id)
        .ok_or("dual fair value hedge relationship is missing")?;
    let item = h
        .hedged_items
        .iter()
        .find(|i| Some(&i.item_id) == fvh.hedged_item_id.as_ref())
        .ok_or("dual fair value hedge needs a hedged debt item")?;
    let debt = r
        .instruments
        .iter()
        .find(|i| i.id() == rel.instrument_position_id)
        .ok_or("missing debt")?;
    let Instrument::ForeignDebt { notional, .. } = debt else {
        return Err(
            "only foreign-currency debt can be both hedging instrument and hedged item".into(),
        );
    };
    if item.kind != ItemKind::FixedRateInstrument
        || item.direction != ItemDirection::Liability
        || !near(item.notional, *notional)
        || fvh.hedged_notional.is_none_or(|n| !near(n, *notional))
    {
        return Err(
            "dual hedge item must be the whole fixed-rate foreign-currency debt liability".into(),
        );
    }
    let out = hedge::evaluate(h)?;
    let mut basis = HashMap::new();
    let mut discontinued = None;
    for day in &out.daily {
        if let Some(x) = day
            .relationships
            .iter()
            .find(|x| x.relationship_id == d.relationship_id)
        {
            basis.insert(
                day.ordinal,
                amount(x.basis_adjustment_balance + x.frozen_basis_balance)?,
            );
            if x.status == HedgeStatus::Discontinued && discontinued.is_none() {
                discontinued = Some(day.ordinal);
            }
        }
    }
    Ok(Dual {
        basis,
        discontinued,
    })
}

fn admit(r: &Request) -> std::result::Result<(), String> {
    check()?;
    let mut bytes = 0;
    for id in [&r.run, &r.policy.identity, &r.foreign_operation.source] {
        identity(id, &mut bytes)?;
    }
    text(&r.entity, &mut bytes)?;
    text(&r.foreign_operation.operation_id, &mut bytes)?;
    let p = &r.policy;
    if r.schema != SCHEMA
        || r.model_version != SCHEMA
        || !currency(&r.reporting_currency)
        || !currency(&r.foreign_currency)
        || r.reporting_currency == r.foreign_currency
        || r.monetary_unit != "currency_units"
        || !(p.assessment_lower_bound > 0.
            && p.assessment_lower_bound <= 1.
            && p.assessment_upper_bound >= 1.
            && p.assessment_upper_bound.is_finite())
        || r.dates.len() < 2
        || r.dates.len() > 4096
        || r.dates.windows(2).any(|w| w[1].ordinal <= w[0].ordinal)
        || r.dates.iter().any(|d| !finite_positive(d.spot))
        || r.instruments.len() > 64
        || r.relationships.len() > 64
        || r.events.len() > 256
        || r.foreign_operation.movements.len() > 4096
        || !r.foreign_operation.opening_net_assets.is_finite()
        || r.foreign_operation.opening_net_assets < 0.
        || !r.foreign_operation.opening_translation_cta.is_finite()
    {
        return Err("net investment hedge requires its schema, two distinct currencies, assessment bounds around one, 2-4096 increasing dates with positive spots and bounded instruments, relationships, events and movements".into());
    }
    let dates: HashSet<i32> = r.dates.iter().map(|d| d.ordinal).collect();
    let first = r.dates[0].ordinal;
    let last = r.dates[r.dates.len() - 1].ordinal;
    for d in &r.dates {
        identity(&d.spot_source, &mut bytes)?;
    }
    let mut ids = HashSet::new();
    for m in &r.foreign_operation.movements {
        text(&m.movement_id, &mut bytes)?;
        identity(&m.rate_source, &mut bytes)?;
        if !ids.insert(m.movement_id.as_str())
            || !dates.contains(&m.ordinal)
            || m.ordinal == first
            || !m.amount.is_finite()
            || (m.kind != MovementKind::Income && m.amount <= 0.)
            || !finite_positive(m.rate)
        {
            return Err("movements need unique IDs on listed later dates, finite amounts (positive flows) and positive rates".into());
        }
    }
    let mut positions = HashSet::new();
    for inst in &r.instruments {
        text(inst.id(), &mut bytes)?;
        if !positions.insert(inst.id()) {
            return Err("duplicate instrument".into());
        }
        match inst {
            Instrument::FxForward {
                source,
                notional,
                forward_rate,
                trade_ordinal,
                maturity_ordinal,
                ..
            } => {
                identity(source, &mut bytes)?;
                if !finite_positive(*notional)
                    || !finite_positive(*forward_rate)
                    || !dates.contains(trade_ordinal)
                    || !dates.contains(maturity_ordinal)
                    || maturity_ordinal <= trade_ordinal
                {
                    return Err("forwards need positive notional and rate with trade and maturity on listed dates".into());
                }
                for d in r
                    .dates
                    .iter()
                    .filter(|d| d.ordinal >= *trade_ordinal && d.ordinal < *maturity_ordinal)
                {
                    curve(&d.reporting_discount, d.ordinal)?;
                    curve(&d.foreign_discount, d.ordinal)?;
                }
            }
            Instrument::ForeignDebt {
                source,
                notional,
                maturity_ordinal,
                ..
            } => {
                identity(source, &mut bytes)?;
                if !finite_positive(*notional)
                    || *maturity_ordinal <= first
                    || (*maturity_ordinal <= last && !dates.contains(maturity_ordinal))
                {
                    return Err("foreign debt needs a positive notional and a maturity after the first date (on a listed date if within the window)".into());
                }
            }
        }
    }
    let mut owned = HashSet::new();
    let mut rels = HashSet::new();
    for rel in &r.relationships {
        text(&rel.relationship_id, &mut bytes)?;
        identity(&rel.documentation, &mut bytes)?;
        let inst = r
            .instruments
            .iter()
            .find(|i| i.id() == rel.instrument_position_id)
            .ok_or("relationship instrument is missing")?;
        if !rels.insert(rel.relationship_id.as_str())
            || !owned.insert(rel.instrument_position_id.as_str())
            || !finite_positive(rel.hedged_net_investment)
            || !dates.contains(&rel.designation_ordinal)
        {
            return Err("relationships need unique IDs, one per instrument, a positive hedged amount and a listed designation date".into());
        }
        match inst {
            Instrument::FxForward {
                side,
                trade_ordinal,
                maturity_ordinal,
                ..
            } => {
                let excluded_ok = match rel.method {
                    Method::Spot => rel.excluded_component.is_some(),
                    Method::Forward => rel.excluded_component.is_none(),
                };
                if *side != ForwardSide::SellForeign
                    || rel.designation_ordinal < *trade_ordinal
                    || rel.designation_ordinal >= *maturity_ordinal
                    || !excluded_ok
                    || rel.dual_fair_value_hedge.is_some()
                {
                    return Err("forward hedges of a net investment sell foreign currency, are designated while live, and select excluded-component recognition only under the spot method".into());
                }
            }
            Instrument::ForeignDebt {
                maturity_ordinal, ..
            } => {
                if rel.method != Method::Spot
                    || rel.excluded_component.is_some()
                    || rel.designation_ordinal >= *maturity_ordinal
                {
                    return Err("debt hedges use spot remeasurement with no excluded component and are designated before maturity".into());
                }
                if let Some(d) = &rel.dual_fair_value_hedge {
                    text(&d.relationship_id, &mut bytes)?;
                    let adopted = p.dual_hedge_adoption.as_ref().ok_or(
                        "a dual hedge requires the entity's reviewed ASU 2025-09 adoption",
                    )?;
                    identity(&adopted.review, &mut bytes)?;
                    if adopted.adoption_ordinal > rel.designation_ordinal {
                        return Err(
                            "the dual-hedge amendment must be adopted by designation".into()
                        );
                    }
                }
            }
        }
    }
    let mut events = HashSet::new();
    for e in &r.events {
        text(&e.event_id, &mut bytes)?;
        identity(&e.review, &mut bytes)?;
        if !events.insert(e.event_id.as_str()) || !dates.contains(&e.ordinal) || e.ordinal == first
        {
            return Err("events need unique IDs on listed dates after the first".into());
        }
        match e.kind {
            EventKind::Dedesignate => {
                if e.fraction.is_some()
                    || e.proceeds.is_some()
                    || !e
                        .relationship_id
                        .as_ref()
                        .is_some_and(|id| rels.contains(id.as_str()))
                {
                    return Err("dedesignation names one listed relationship only".into());
                }
            }
            EventKind::Disposal => {
                if e.relationship_id.is_some()
                    || !e.fraction.is_some_and(|f| f > 0. && f <= 1.)
                    || !e.proceeds.is_some_and(|x| x.is_finite() && x >= 0.)
                {
                    return Err(
                        "disposal needs a fraction in (0, 1] and nonnegative proceeds".into(),
                    );
                }
            }
        }
    }
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw > MAX_INPUT {
        return Err("net investment hedge input exceeds 32 MiB".into());
    }
    check()
}

#[derive(Clone)]
struct RelState {
    active: bool,
    discontinued: Option<i32>,
    recognized: f64,
    released: f64,
    actual_cum: f64,
    hyp_cum: f64,
    amortization_total: f64,
    designation_value: f64,
}

pub fn evaluate(r: &Request) -> std::result::Result<Result, String> {
    admit(r)?;
    let first = r.dates[0].ordinal;
    let mut duals: HashMap<&str, Dual> = HashMap::new();
    for rel in &r.relationships {
        if let Some(d) = &rel.dual_fair_value_hedge {
            duals.insert(rel.relationship_id.as_str(), dual(r, rel, d)?);
        }
    }
    let mut ledger = Ledger {
        postings: Vec::new(),
        balances: BTreeMap::new(),
        seq: 0,
    };
    let fo = &r.foreign_operation;
    let s0 = r.dates[0].spot;
    let mut net_assets = fo.opening_net_assets;
    let mut carrying = amount(net_assets * s0)?;
    let mut cta_translation = fo.opening_translation_cta;
    // Opening: translated net investment, its reviewed CTA, and debt outstanding at the first spot.
    ledger.post(
        first,
        "opening_net_investment",
        None,
        None,
        &[
            ("net_investment_foreign_operation", carrying),
            ("cta_translation", -cta_translation),
            ("opening_equity", amount(cta_translation - carrying)?),
        ],
    )?;
    for inst in &r.instruments {
        if let Instrument::ForeignDebt { notional, .. } = inst {
            let v = amount(notional * s0)?;
            ledger.post(
                first,
                "opening_foreign_debt",
                Some(inst.id()),
                None,
                &[("foreign_debt", -v), ("opening_equity", v)],
            )?;
        }
    }
    let mut translation = vec![Translation {
        ordinal: first,
        spot: s0,
        net_assets,
        carrying,
        translation_adjustment: 0.,
        translation_cta: cta_translation,
    }];
    let mut values: HashMap<&str, f64> = HashMap::new();
    let mut instruments_out = Vec::new();
    let mut periods = Vec::new();
    let mut states: HashMap<&str, RelState> = r
        .relationships
        .iter()
        .map(|rel| {
            (
                rel.relationship_id.as_str(),
                RelState {
                    active: false,
                    discontinued: None,
                    recognized: 0.,
                    released: 0.,
                    actual_cum: 0.,
                    hyp_cum: 0.,
                    amortization_total: 0.,
                    designation_value: 0.,
                },
            )
        })
        .collect();
    let rel_of: HashMap<&str, &Relationship> = r
        .relationships
        .iter()
        .map(|x| (x.instrument_position_id.as_str(), x))
        .collect();
    let mut hedge_cta = 0.;
    for k in 0..r.dates.len() {
        check()?;
        let d = &r.dates[k];
        let t = d.ordinal;
        // Translation of the foreign operation (k > 0): flows at their rates, closing at spot.
        if k > 0 {
            let mut flows_rc = 0.;
            let mut flows_fc = 0.;
            for m in fo.movements.iter().filter(|m| m.ordinal == t) {
                let fc = if m.kind == MovementKind::Distribution {
                    -m.amount
                } else {
                    m.amount
                };
                let rc = amount(fc * m.rate)?;
                let other = match m.kind {
                    MovementKind::Income => "pnl:foreign_operation_income",
                    MovementKind::Contribution | MovementKind::Distribution => "cash",
                };
                ledger.post(
                    t,
                    "foreign_operation_flow",
                    None,
                    None,
                    &[("net_investment_foreign_operation", rc), (other, -rc)],
                )?;
                flows_rc = amount(flows_rc + rc)?;
                flows_fc = amount(flows_fc + fc)?;
            }
            net_assets = amount(net_assets + flows_fc)?;
            if net_assets < 0. {
                return Err("foreign operation net assets cannot fall below zero".into());
            }
            let closing = amount(net_assets * d.spot)?;
            let adjustment = amount(closing - carrying - flows_rc)?;
            ledger.post(
                t,
                "translation",
                None,
                None,
                &[
                    ("net_investment_foreign_operation", adjustment),
                    ("cta_translation", -adjustment),
                ],
            )?;
            carrying = closing;
            cta_translation = amount(cta_translation + adjustment)?;
            translation.push(Translation {
                ordinal: t,
                spot: d.spot,
                net_assets,
                carrying,
                translation_adjustment: adjustment,
                translation_cta: cta_translation,
            });
        }
        // Hedged amount effective this period: designated amounts scaled to the opening net assets.
        let opening_na = if k == 0 {
            net_assets
        } else {
            translation[k - 1].net_assets
        };
        let designated: f64 = r
            .relationships
            .iter()
            .filter(|x| states[x.relationship_id.as_str()].active)
            .map(|x| x.hedged_net_investment)
            .sum();
        let scale = if designated > opening_na && designated > 0. {
            opening_na / designated
        } else {
            1.
        };
        for inst in &r.instruments {
            let id = inst.id();
            let rel = rel_of.get(id).copied();
            match inst {
                Instrument::FxForward {
                    side,
                    notional,
                    forward_rate,
                    trade_ordinal,
                    maturity_ordinal,
                    ..
                } => {
                    if t < *trade_ordinal || t > *maturity_ordinal {
                        continue;
                    }
                    let v = forward_value(r, inst, k)?;
                    if t == *trade_ordinal {
                        // Off-market value at trade is a cash premium, never a day-one gain.
                        ledger.post(
                            t,
                            "fx_forward_trade",
                            Some(id),
                            None,
                            &[("fx_forward", v), ("cash", -v)],
                        )?;
                        values.insert(id, v);
                        instruments_out.push(InstrumentValue {
                            ordinal: t,
                            position_id: id.into(),
                            carrying: v,
                            change: 0.,
                        });
                        if let Some(rel) = rel.filter(|x| x.designation_ordinal == t) {
                            designate(&mut states, rel, r, k, *notional, *forward_rate, *side, v)?;
                        }
                        continue;
                    }
                    let prev_v = values[id];
                    let change = amount(v - prev_v)?;
                    let prev_spot = r.dates[k - 1].spot;
                    let sign = if *side == ForwardSide::SellForeign {
                        1.
                    } else {
                        -1.
                    };
                    let mut lines: Vec<(&str, f64)> = vec![("fx_forward", change)];
                    let mut rel_id = None;
                    if let Some(rel) = rel {
                        let st = states
                            .get_mut(rel.relationship_id.as_str())
                            .ok_or("state")?;
                        if st.active {
                            rel_id = Some(rel.relationship_id.as_str());
                            let hedged = rel.hedged_net_investment * scale;
                            let phi = (hedged / notional).min(1.);
                            let included = match rel.method {
                                Method::Spot => amount(sign * notional * (prev_spot - d.spot))?,
                                Method::Forward => change,
                            };
                            let excluded = amount(change - included)?;
                            let amort = match rel.excluded_component {
                                Some(Excluded::Amortization) => amount(
                                    st.amortization_total * days(r.dates[k - 1].ordinal, t)
                                        / days(rel.designation_ordinal, *maturity_ordinal),
                                )?,
                                _ => 0.,
                            };
                            let (excluded_pnl, excluded_cta) = match rel.excluded_component {
                                Some(Excluded::MarkToMarket) => (phi * excluded, 0.),
                                Some(Excluded::Amortization) => {
                                    (phi * amort, phi * (excluded - amort))
                                }
                                None => (0., 0.),
                            };
                            let cta = amount(phi * included + excluded_cta)?;
                            let excess = amount((1. - phi) * (included + excluded))?;
                            st.actual_cum = amount(st.actual_cum + phi * included)?;
                            st.hyp_cum = amount(st.hyp_cum + (hedged / notional) * included)?;
                            let ratio = ratio(st.actual_cum, st.hyp_cum);
                            let passed = ratio.is_none_or(|x| {
                                x >= r.policy.assessment_lower_bound
                                    && x <= r.policy.assessment_upper_bound
                            });
                            if passed {
                                lines.push(("cta_net_investment_hedge", -cta));
                                lines.push(("pnl:fx_forward_excluded_component", -excluded_pnl));
                                lines.push(("pnl:net_investment_hedge_excess", -excess));
                                st.recognized = amount(st.recognized + cta)?;
                                hedge_cta = amount(hedge_cta + cta)?;
                            } else {
                                lines.push(("pnl:fx_forward_mark", -change));
                                st.active = false;
                                st.discontinued = Some(t);
                            }
                            periods.push(Period {
                                ordinal: t,
                                relationship_id: rel.relationship_id.clone(),
                                status: if passed {
                                    "qualified"
                                } else {
                                    "failed_assessment"
                                }
                                .into(),
                                hedged_effective: hedged,
                                designated_fraction: phi,
                                included_change: included,
                                excluded_change: excluded,
                                cta: if passed { cta } else { 0. },
                                excluded_earnings: if passed { excluded_pnl } else { 0. },
                                excess_earnings: if passed { excess } else { change },
                                basis_remeasurement: 0.,
                                cumulative_ratio: ratio,
                                assessment_passed: Some(passed),
                            });
                        } else {
                            lines.push(("pnl:fx_forward_mark", -change));
                        }
                    } else {
                        lines.push(("pnl:fx_forward_mark", -change));
                    }
                    ledger.post(t, "fx_forward_mark", Some(id), rel_id, &lines)?;
                    values.insert(id, v);
                    instruments_out.push(InstrumentValue {
                        ordinal: t,
                        position_id: id.into(),
                        carrying: v,
                        change,
                    });
                    if t == *maturity_ordinal {
                        ledger.post(
                            t,
                            "fx_forward_settlement",
                            Some(id),
                            None,
                            &[("cash", v), ("fx_forward", -v)],
                        )?;
                        values.insert(id, 0.);
                        if let Some(rel) = rel {
                            let st = states
                                .get_mut(rel.relationship_id.as_str())
                                .ok_or("state")?;
                            if st.active {
                                st.active = false;
                                st.discontinued = Some(t);
                            }
                        }
                    } else if let Some(rel) = rel.filter(|x| x.designation_ordinal == t) {
                        designate(&mut states, rel, r, k, *notional, *forward_rate, *side, v)?;
                    }
                }
                Instrument::ForeignDebt {
                    notional,
                    maturity_ordinal,
                    ..
                } => {
                    if k == 0 {
                        values.insert(id, -notional * d.spot);
                        instruments_out.push(InstrumentValue {
                            ordinal: t,
                            position_id: id.into(),
                            carrying: -notional * d.spot,
                            change: 0.,
                        });
                        if let Some(rel) = rel.filter(|x| x.designation_ordinal == t) {
                            designate(
                                &mut states,
                                rel,
                                r,
                                k,
                                *notional,
                                0.,
                                ForwardSide::SellForeign,
                                0.,
                            )?;
                        }
                        continue;
                    }
                    if r.dates[k - 1].ordinal >= *maturity_ordinal {
                        continue;
                    }
                    let prev_spot = r.dates[k - 1].spot;
                    let prev_t = r.dates[k - 1].ordinal;
                    // Transaction gain on the liability: positive when the foreign currency weakens.
                    let gain = amount(-notional * (d.spot - prev_spot))?;
                    // Dual hedge: the fair-value-hedge basis (a liability increase in foreign
                    // currency) is remeasured at spot too.
                    let dual_state = rel.and_then(|x| duals.get(x.relationship_id.as_str()));
                    let (basis_prev, basis_gain, fvh_active) = match dual_state {
                        None => (0., 0., false),
                        Some(dh) => {
                            let b = *dh.basis.get(&prev_t).ok_or(
                                "dual fair value hedge lacks a basis balance on a reporting date",
                            )?;
                            (
                                b,
                                amount(-b * (d.spot - prev_spot))?,
                                dh.discontinued.is_none_or(|x| x > prev_t),
                            )
                        }
                    };
                    let dual = dual_state.is_some();
                    let mut lines: Vec<(&str, f64)> = vec![("foreign_debt", gain)];
                    if dual {
                        lines.push(("foreign_debt_basis_translation", basis_gain));
                    }
                    let mut rel_id = None;
                    let active = rel.is_some_and(|x| states[x.relationship_id.as_str()].active);
                    if let (true, Some(rel)) = (active, rel) {
                        let st = states
                            .get_mut(rel.relationship_id.as_str())
                            .ok_or("state")?;
                        rel_id = Some(rel.relationship_id.as_str());
                        let hedged = rel.hedged_net_investment * scale;
                        // After the fair value hedge ends its basis joins the hedging instrument
                        // prospectively (815-20-55-38); while it is active the basis is excluded.
                        let joined = dual && !fvh_active;
                        let amount_fc = if joined {
                            notional + basis_prev
                        } else {
                            *notional
                        };
                        if amount_fc <= 0. {
                            return Err("hedging debt carrying amount must stay positive".into());
                        }
                        let phi = (hedged / amount_fc).min(1.);
                        let included = if joined {
                            amount(gain + basis_gain)?
                        } else {
                            gain
                        };
                        let cta = amount(phi * included)?;
                        let excess = amount((1. - phi) * included)?;
                        st.actual_cum = amount(st.actual_cum + phi * included)?;
                        st.hyp_cum = amount(st.hyp_cum + (hedged / amount_fc) * included)?;
                        let ratio = ratio(st.actual_cum, st.hyp_cum);
                        let passed = ratio.is_none_or(|x| {
                            x >= r.policy.assessment_lower_bound
                                && x <= r.policy.assessment_upper_bound
                        });
                        let basis_pnl = if dual && (!joined || !passed) {
                            basis_gain
                        } else {
                            0.
                        };
                        if passed {
                            lines.push(("cta_net_investment_hedge", -cta));
                            lines.push(("pnl:net_investment_hedge_excess", -excess));
                            st.recognized = amount(st.recognized + cta)?;
                            hedge_cta = amount(hedge_cta + cta)?;
                        } else {
                            lines.push(("pnl:fx_transaction", -gain));
                            st.active = false;
                            st.discontinued = Some(t);
                        }
                        if basis_pnl != 0. {
                            lines.push(("pnl:fx_basis_remeasurement", -basis_pnl));
                        }
                        periods.push(Period {
                            ordinal: t,
                            relationship_id: rel.relationship_id.clone(),
                            status: if passed {
                                "qualified"
                            } else {
                                "failed_assessment"
                            }
                            .into(),
                            hedged_effective: hedged,
                            designated_fraction: phi,
                            included_change: included,
                            excluded_change: 0.,
                            cta: if passed { cta } else { 0. },
                            excluded_earnings: 0.,
                            excess_earnings: if passed { excess } else { gain },
                            basis_remeasurement: basis_pnl,
                            cumulative_ratio: ratio,
                            assessment_passed: Some(passed),
                        });
                    } else {
                        lines.push(("pnl:fx_transaction", -gain));
                        if dual {
                            lines.push(("pnl:fx_basis_remeasurement", -basis_gain));
                        }
                    }
                    ledger.post(t, "foreign_debt_remeasurement", Some(id), rel_id, &lines)?;
                    let v = amount(values[id] + gain)?;
                    values.insert(id, v);
                    instruments_out.push(InstrumentValue {
                        ordinal: t,
                        position_id: id.into(),
                        carrying: v,
                        change: gain,
                    });
                    if t == *maturity_ordinal {
                        ledger.post(
                            t,
                            "foreign_debt_repayment",
                            Some(id),
                            None,
                            &[("foreign_debt", -v), ("cash", v)],
                        )?;
                        values.insert(id, 0.);
                        if let Some(rel) = rel {
                            let st = states
                                .get_mut(rel.relationship_id.as_str())
                                .ok_or("state")?;
                            if st.active {
                                st.active = false;
                                st.discontinued = Some(t);
                            }
                        }
                    } else if let Some(rel) = rel.filter(|x| x.designation_ordinal == t) {
                        designate(
                            &mut states,
                            rel,
                            r,
                            k,
                            *notional,
                            0.,
                            ForwardSide::SellForeign,
                            0.,
                        )?;
                    }
                }
            }
        }
        // Events at the end of the date: dedesignation, then disposal.
        for e in r
            .events
            .iter()
            .filter(|e| e.ordinal == t && e.kind == EventKind::Dedesignate)
        {
            let id = e
                .relationship_id
                .as_deref()
                .ok_or("dedesignation relationship")?;
            let st = states.get_mut(id).ok_or("state")?;
            if !st.active {
                return Err("dedesignation of a relationship that is not active".into());
            }
            st.active = false;
            st.discontinued = Some(t);
        }
        for e in r
            .events
            .iter()
            .filter(|e| e.ordinal == t && e.kind == EventKind::Disposal)
        {
            let f = e.fraction.ok_or("fraction")?;
            let proceeds = e.proceeds.ok_or("proceeds")?;
            if net_assets <= 0. {
                return Err("disposal after the foreign operation is fully sold".into());
            }
            let released_translation = amount(f * cta_translation)?;
            let released_hedge = amount(f * hedge_cta)?;
            let sold = amount(f * carrying)?;
            ledger.post(
                t,
                "cta_reclassification",
                None,
                None,
                &[
                    ("cta_translation", released_translation),
                    ("cta_net_investment_hedge", released_hedge),
                    (
                        "pnl:cta_reclassification",
                        amount(-released_translation - released_hedge)?,
                    ),
                ],
            )?;
            ledger.post(
                t,
                "disposal",
                None,
                None,
                &[
                    ("cash", proceeds),
                    ("net_investment_foreign_operation", -sold),
                    ("pnl:disposal_gain_loss", amount(sold - proceeds)?),
                ],
            )?;
            for rel in &r.relationships {
                let st = states
                    .get_mut(rel.relationship_id.as_str())
                    .ok_or("state")?;
                if st.recognized - st.released != 0. {
                    let share = amount(f * (st.recognized - st.released))?;
                    st.released = amount(st.released + share)?;
                }
                if f == 1. && st.active {
                    st.active = false;
                    st.discontinued = Some(t);
                }
            }
            cta_translation = amount(cta_translation - released_translation)?;
            hedge_cta = amount(hedge_cta - released_hedge)?;
            net_assets = amount(net_assets * (1. - f))?;
            carrying = amount(carrying - sold)?;
            if let Some(last) = translation.last_mut() {
                last.net_assets = net_assets;
                last.carrying = carrying;
                last.translation_cta = cta_translation;
            }
        }
    }
    let relationships = r
        .relationships
        .iter()
        .map(|rel| {
            let st = &states[rel.relationship_id.as_str()];
            RelationshipClosing {
                relationship_id: rel.relationship_id.clone(),
                status: if st.active {
                    "qualified"
                } else if st.discontinued.is_some() {
                    "discontinued"
                } else {
                    "pending"
                }
                .into(),
                discontinued_ordinal: st.discontinued,
                hedge_cta_recognized: st.recognized,
                hedge_cta_released: st.released,
                dual_hedge_applied: rel.dual_fair_value_hedge.is_some(),
            }
        })
        .collect();
    let closing = ledger
        .balances
        .iter()
        .map(|(k, v)| Balance {
            account: k.clone(),
            debit_balance: *v,
        })
        .collect::<Vec<_>>();
    let total = closing
        .iter()
        .try_fold(0., |a, b| amount(a + b.debit_balance))?;
    if !near(total, 0.) {
        return Err("net investment hedge trial balance does not balance".into());
    }
    let result = Result {
        schema: SCHEMA.into(),
        model_version: SCHEMA.into(),
        run: r.run.clone(),
        entity: r.entity.clone(),
        reporting_currency: r.reporting_currency.clone(),
        foreign_currency: r.foreign_currency.clone(),
        policy: r.policy.identity.clone(),
        journal: ledger.postings,
        translation,
        periods,
        instruments: instruments_out,
        relationships,
        closing,
        dual_hedge_applied: !duals.is_empty(),
        debt_interest_included: false,
        accounting_standard_approval: false,
        empirical_validation: false,
        capital_ftp_eligible: false,
        disclosure: "Translation of one foreign operation (closing spot for net assets, supplied rates for flows) and ASC 815-35 net-investment hedges by FX forwards (spot method with amortized or marked forward points, or forward method) and foreign-currency debt (spot remeasurement). Only the portion matching the hedged net investment, scaled to the opening net assets, goes to the hedge CTA; the excess goes to earnings. A dual hedge excludes the debt's fair-value-hedge basis adjustment, from an embedded foreign-currency hedge-accounting-1 run, from the net-investment hedge and recognizes its spot remeasurement in earnings until that fair value hedge is discontinued (815-20-55-38). CTA is released only on disposal. Debt interest, the foreign-currency fair value hedge journal itself, deferred tax and consolidation eliminations are outside this owner. Not an accounting-standard approval.".into(),
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_OUTPUT
    {
        return Err("net investment hedge output exceeds 32 MiB".into());
    }
    check()?;
    Ok(result)
}

fn ratio(actual: f64, hypothetical: f64) -> Option<f64> {
    (hypothetical != 0.).then(|| actual / hypothetical)
}

#[allow(clippy::too_many_arguments)]
fn designate(
    states: &mut HashMap<&str, RelState>,
    rel: &Relationship,
    r: &Request,
    k: usize,
    notional: f64,
    forward_rate: f64,
    side: ForwardSide,
    value: f64,
) -> std::result::Result<(), String> {
    let st = states
        .get_mut(rel.relationship_id.as_str())
        .ok_or("state")?;
    let sign = if side == ForwardSide::SellForeign {
        1.
    } else {
        -1.
    };
    st.active = true;
    st.designation_value = value;
    // Total excluded change to maturity, known at designation: the forward points less the
    // forward's value at designation (spot component measured undiscounted from the designation spot).
    st.amortization_total = if forward_rate > 0. {
        amount(sign * notional * (forward_rate - r.dates[k].spot) - value)?
    } else {
        0.
    };
    Ok(())
}
