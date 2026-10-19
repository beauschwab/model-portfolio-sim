//! `nonfinancial-hedge-1`: cash flow hedges of forecasted purchases and sales of nonfinancial
//! assets by commodity swaps, with ASU 2025-09 component eligibility (815-20-25-22C).
//!
//! The hedged risk is the variability of a price component: a contractually specified component
//! (eligible before adoption, former 815-20-25-22A), or after the entity adopts ASU 2025-09 a
//! component clearly and closely related to the asset in a spot or forward market, or a
//! subcomponent of an explicitly referenced component. Eligibility evidence is supplied and
//! reviewed; it is never inferred from the price data. Effectiveness is a cumulative dollar
//! offset against a hypothetical derivative on the hedged component's own index, so basis risk
//! between the swap index and the component is measured, not assumed away. Each designated swap
//! leg's change goes to OCI attributed to its mapped delivery. A sale releases that OCI to revenue
//! at delivery; a purchase releases it to cost of sales as the delivered asset is consumed. A
//! forecast that becomes probable not to occur releases its OCI at once (815-30-40-5).
//! Contract: docs/models/nonfinancial-hedge.md.
use crate::products::derivatives::contract::{amount, identity, near, text, Identity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SCHEMA: &str = "nonfinancial-hedge-1";
pub const MAX_INPUT: usize = 32 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub run: Identity,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub policy: Policy,
    pub dates: Vec<MarketDate>,
    pub exposures: Vec<Exposure>,
    pub instruments: Vec<CommoditySwap>,
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
    /// Entity adoption of ASU 2025-09; required for any component other than a contractually
    /// specified one.
    #[serde(default)]
    pub asu_2025_09_adoption: Option<Adoption>,
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
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ForwardPrice {
    pub delivery_ordinal: i32,
    pub price: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexCurve {
    pub index_id: String,
    pub source: Identity,
    /// Forward prices by delivery date; on a delivery date its own price is the fixing.
    pub forwards: Vec<ForwardPrice>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarketDate {
    pub ordinal: i32,
    pub discount: Vec<Point>,
    pub indices: Vec<IndexCurve>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Purchase,
    Sale,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Market {
    Spot,
    Forward,
}
/// The hedged variable price component and its reviewed eligibility basis.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Component {
    /// Explicitly referenced in an agreement's pricing formula (former 815-20-25-22A; also 22C(b)(1)).
    ContractuallySpecified {
        index_id: String,
        contract: Identity,
    },
    /// No pricing formula: clearly and closely related to the asset (815-20-25-22C(a)).
    ClearlyAndCloselyRelated {
        index_id: String,
        market: Market,
        evidence: Identity,
    },
    /// Clearly and closely related to an explicitly referenced component (815-20-25-22C(b)(2)).
    Subcomponent {
        index_id: String,
        parent_index_id: String,
        contract: Identity,
        evidence: Identity,
    },
}
impl Component {
    fn index(&self) -> &str {
        match self {
            Self::ContractuallySpecified { index_id, .. }
            | Self::ClearlyAndCloselyRelated { index_id, .. }
            | Self::Subcomponent { index_id, .. } => index_id,
        }
    }
    fn needs_adoption(&self) -> bool {
        !matches!(self, Self::ContractuallySpecified { .. })
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub delivery_id: String,
    pub ordinal: i32,
    pub quantity: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Exposure {
    pub exposure_id: String,
    pub source: Identity,
    pub probability_evidence: Identity,
    pub direction: Direction,
    pub asset: String,
    pub quantity_unit: String,
    pub component: Component,
    pub deliveries: Vec<Delivery>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapSide {
    /// Receive the index, pay fixed: gains when prices rise (hedges purchases).
    ReceiveIndex,
    PayIndex,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Leg {
    pub leg_id: String,
    pub settlement_ordinal: i32,
    pub quantity: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommoditySwap {
    pub position_id: String,
    pub source: Identity,
    pub index_id: String,
    pub side: SwapSide,
    pub fixed_price: f64,
    pub trade_ordinal: i32,
    pub legs: Vec<Leg>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegDelivery {
    pub leg_id: String,
    pub delivery_id: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub relationship_id: String,
    pub instrument_position_id: String,
    pub exposure_id: String,
    pub documentation: Identity,
    pub designation_ordinal: i32,
    pub legs: Vec<LegDelivery>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Part of a delivered purchase is consumed (for example sold as inventory or used).
    Consumption,
    /// The forecast for one delivery is no longer probable, or probable not to occur.
    ForecastStatus,
    Dedesignate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastStatus {
    NotProbable,
    ProbableNotToOccur,
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
    #[serde(default)]
    pub exposure_id: Option<String>,
    #[serde(default)]
    pub delivery_id: Option<String>,
    #[serde(default)]
    pub fraction: Option<f64>,
    #[serde(default)]
    pub status: Option<ForecastStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Posting {
    pub transaction_id: String,
    pub ordinal: i32,
    pub event: String,
    pub account: String,
    pub position_id: Option<String>,
    pub relationship_id: Option<String>,
    pub delivery_id: Option<String>,
    pub debit: f64,
    pub credit: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct LegValue {
    pub ordinal: i32,
    pub position_id: String,
    pub leg_id: String,
    pub value: f64,
    pub change: f64,
    pub designated: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Period {
    pub ordinal: i32,
    pub relationship_id: String,
    pub instrument_change: f64,
    pub hypothetical_change: f64,
    pub cumulative_instrument_change: f64,
    pub cumulative_hypothetical_change: f64,
    pub cumulative_ratio: Option<f64>,
    pub assessment_passed: bool,
    pub oci: f64,
    pub earnings: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryClosing {
    pub exposure_id: String,
    pub delivery_id: String,
    pub status: String,
    pub oci_recognized: f64,
    pub oci_reclassified: f64,
    pub oci_balance: f64,
    pub consumed_fraction: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct RelationshipClosing {
    pub relationship_id: String,
    pub status: String,
    pub discontinued_ordinal: Option<i32>,
    pub component_kind: String,
    pub asu_2025_09_component: bool,
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
    pub currency: String,
    pub policy: Identity,
    pub journal: Vec<Posting>,
    pub legs: Vec<LegValue>,
    pub periods: Vec<Period>,
    pub deliveries: Vec<DeliveryClosing>,
    pub relationships: Vec<RelationshipClosing>,
    pub closing: Vec<Balance>,
    pub purchase_and_inventory_cash_included: bool,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub capital_ftp_eligible: bool,
    pub disclosure: String,
}

fn check() -> std::result::Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn finite_positive(x: f64) -> bool {
    x.is_finite() && x > 0.
}
fn price(d: &MarketDate, index: &str, delivery: i32) -> std::result::Result<f64, String> {
    d.indices
        .iter()
        .find(|c| c.index_id == index)
        .ok_or_else(|| format!("missing index {index} on {}", d.ordinal))?
        .forwards
        .iter()
        .find(|f| f.delivery_ordinal == delivery)
        .map(|f| f.price)
        .ok_or_else(|| "missing exact forward price for a delivery date; no interpolation".into())
}
fn discount(d: &MarketDate, t: i32) -> std::result::Result<f64, String> {
    d.discount
        .iter()
        .find(|p| p.ordinal == t)
        .map(|p| p.discount_factor)
        .ok_or_else(|| "missing exact discount point; no interpolation".into())
}
/// Value at market date `d` of `quantity` on `index` against `fixed`, settling on `settle`;
/// on the settlement date it is the settled amount.
fn leg_value(
    d: &MarketDate,
    index: &str,
    sign: f64,
    quantity: f64,
    fixed: f64,
    settle: i32,
) -> std::result::Result<f64, String> {
    if d.ordinal > settle {
        return Ok(0.);
    }
    let p = price(d, index, settle)?;
    let df = if d.ordinal == settle {
        1.
    } else {
        discount(d, settle)?
    };
    amount(sign * quantity * (p - fixed) * df)
}
fn ratio(a: f64, b: f64) -> Option<f64> {
    (b != 0.).then(|| a / b)
}

struct Ledger {
    postings: Vec<Posting>,
    balances: BTreeMap<String, f64>,
    seq: usize,
}
impl Ledger {
    fn post(
        &mut self,
        t: i32,
        event: &str,
        tags: (Option<&str>, Option<&str>, Option<&str>),
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
        let id = format!("nfh:{t}:{:06}:{event}", self.seq);
        for (account, v) in lines {
            *self.balances.entry((*account).into()).or_default() += v;
            self.postings.push(Posting {
                transaction_id: id.clone(),
                ordinal: t,
                event: event.into(),
                account: (*account).into(),
                position_id: tags.0.map(str::to_string),
                relationship_id: tags.1.map(str::to_string),
                delivery_id: tags.2.map(str::to_string),
                debit: v.max(0.),
                credit: (-v).max(0.),
            });
        }
        Ok(())
    }
}

fn admit(r: &Request) -> std::result::Result<(), String> {
    check()?;
    let mut bytes = 0;
    identity(&r.run, &mut bytes)?;
    identity(&r.policy.identity, &mut bytes)?;
    text(&r.entity, &mut bytes)?;
    let p = &r.policy;
    if r.schema != SCHEMA
        || r.model_version != SCHEMA
        || r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
        || r.monetary_unit != "currency_units"
        || !(p.assessment_lower_bound > 0.
            && p.assessment_lower_bound <= 1.
            && p.assessment_upper_bound >= 1.
            && p.assessment_upper_bound.is_finite())
        || r.dates.len() < 2
        || r.dates.len() > 4096
        || r.dates.windows(2).any(|w| w[1].ordinal <= w[0].ordinal)
        || r.exposures.is_empty()
        || r.exposures.len() > 64
        || r.instruments.len() > 64
        || r.relationships.len() > 64
        || r.events.len() > 1024
    {
        return Err("nonfinancial hedge requires its schema, a currency, assessment bounds around one, 2-4096 increasing dates and bounded exposures, swaps, relationships and events".into());
    }
    if let Some(a) = &p.asu_2025_09_adoption {
        identity(&a.review, &mut bytes)?;
    }
    let dates: HashSet<i32> = r.dates.iter().map(|d| d.ordinal).collect();
    let last = r.dates[r.dates.len() - 1].ordinal;
    for d in &r.dates {
        if d.indices.is_empty()
            || d.indices.len() > 16
            || d.discount
                .first()
                .is_none_or(|x| x.ordinal != d.ordinal || x.discount_factor != 1.)
            || d.discount.windows(2).any(|w| w[1].ordinal <= w[0].ordinal)
            || d.discount
                .iter()
                .any(|x| !finite_positive(x.discount_factor))
        {
            return Err(
                "market dates need 1-16 index curves and a discount curve starting at one".into(),
            );
        }
        let mut names = HashSet::new();
        for c in &d.indices {
            text(&c.index_id, &mut bytes)?;
            identity(&c.source, &mut bytes)?;
            if !names.insert(c.index_id.as_str())
                || c.forwards.is_empty()
                || c.forwards.len() > 4096
                || c.forwards.iter().any(|f| !f.price.is_finite())
            {
                return Err("index curves need unique IDs and finite forward prices".into());
            }
        }
    }
    let mut deliveries: HashMap<(&str, &str), &Delivery> = HashMap::new();
    let mut exposures = HashSet::new();
    for e in &r.exposures {
        text(&e.exposure_id, &mut bytes)?;
        text(&e.asset, &mut bytes)?;
        text(&e.quantity_unit, &mut bytes)?;
        identity(&e.source, &mut bytes)?;
        identity(&e.probability_evidence, &mut bytes)?;
        match &e.component {
            Component::ContractuallySpecified { index_id, contract } => {
                text(index_id, &mut bytes)?;
                identity(contract, &mut bytes)?;
            }
            Component::ClearlyAndCloselyRelated {
                index_id, evidence, ..
            } => {
                text(index_id, &mut bytes)?;
                identity(evidence, &mut bytes)?;
            }
            Component::Subcomponent {
                index_id,
                parent_index_id,
                contract,
                evidence,
            } => {
                text(index_id, &mut bytes)?;
                text(parent_index_id, &mut bytes)?;
                identity(contract, &mut bytes)?;
                identity(evidence, &mut bytes)?;
                if index_id == parent_index_id {
                    return Err(
                        "a subcomponent differs from its explicitly referenced parent component"
                            .into(),
                    );
                }
            }
        }
        if !exposures.insert(e.exposure_id.as_str())
            || e.deliveries.is_empty()
            || e.deliveries.len() > 1024
        {
            return Err("exposures need unique IDs and 1-1024 deliveries".into());
        }
        for d in &e.deliveries {
            text(&d.delivery_id, &mut bytes)?;
            if !dates.contains(&d.ordinal)
                || !finite_positive(d.quantity)
                || deliveries
                    .insert((e.exposure_id.as_str(), d.delivery_id.as_str()), d)
                    .is_some()
            {
                return Err(
                    "deliveries need unique IDs, positive quantities and listed dates".into(),
                );
            }
        }
    }
    let mut positions = HashSet::new();
    for s in &r.instruments {
        text(&s.position_id, &mut bytes)?;
        text(&s.index_id, &mut bytes)?;
        identity(&s.source, &mut bytes)?;
        let mut legs = HashSet::new();
        if !positions.insert(s.position_id.as_str())
            || !s.fixed_price.is_finite()
            || !dates.contains(&s.trade_ordinal)
            || s.legs.is_empty()
            || s.legs.len() > 1024
        {
            return Err(
                "swaps need unique IDs, a finite fixed price, a listed trade date and 1-1024 legs"
                    .into(),
            );
        }
        for l in &s.legs {
            text(&l.leg_id, &mut bytes)?;
            if !legs.insert(l.leg_id.as_str())
                || !finite_positive(l.quantity)
                || !dates.contains(&l.settlement_ordinal)
                || l.settlement_ordinal <= s.trade_ordinal
                || l.settlement_ordinal > last
            {
                return Err("swap legs need unique IDs, positive quantities and listed settlement dates after trade".into());
            }
        }
    }
    let mut owned = HashSet::new();
    let mut relationships = HashSet::new();
    for rel in &r.relationships {
        text(&rel.relationship_id, &mut bytes)?;
        identity(&rel.documentation, &mut bytes)?;
        let swap = r
            .instruments
            .iter()
            .find(|s| s.position_id == rel.instrument_position_id)
            .ok_or("relationship swap is missing")?;
        let e = r
            .exposures
            .iter()
            .find(|e| e.exposure_id == rel.exposure_id)
            .ok_or("relationship exposure is missing")?;
        let side_ok = matches!(
            (e.direction, swap.side),
            (Direction::Purchase, SwapSide::ReceiveIndex) | (Direction::Sale, SwapSide::PayIndex)
        );
        if !relationships.insert(rel.relationship_id.as_str())
            || !dates.contains(&rel.designation_ordinal)
            || rel.designation_ordinal < swap.trade_ordinal
            || !side_ok
            || rel.legs.is_empty()
        {
            return Err("relationships need unique IDs, a listed designation on or after trade, a swap that offsets the exposure, and mapped legs".into());
        }
        if e.component.needs_adoption()
            && p.asu_2025_09_adoption
                .as_ref()
                .is_none_or(|a| a.adoption_ordinal > rel.designation_ordinal)
        {
            return Err("this component is eligible only after the entity adopts ASU 2025-09 (815-20-25-22C)".into());
        }
        let mut mapped = HashSet::new();
        for m in &rel.legs {
            let leg = swap
                .legs
                .iter()
                .find(|l| l.leg_id == m.leg_id)
                .ok_or("mapped leg is not a swap leg")?;
            let d = deliveries
                .get(&(rel.exposure_id.as_str(), m.delivery_id.as_str()))
                .ok_or("mapped delivery is not an exposure delivery")?;
            if !owned.insert((swap.position_id.as_str(), leg.leg_id.as_str()))
                || !mapped.insert(m.delivery_id.as_str())
                || leg.settlement_ordinal != d.ordinal
                || leg.quantity > d.quantity
                || leg.settlement_ordinal <= rel.designation_ordinal
            {
                return Err("each designated leg maps once to a later delivery on its settlement date with no more quantity".into());
            }
        }
    }
    let mut events = HashSet::new();
    for ev in &r.events {
        text(&ev.event_id, &mut bytes)?;
        identity(&ev.review, &mut bytes)?;
        if !events.insert(ev.event_id.as_str()) || !dates.contains(&ev.ordinal) {
            return Err("events need unique IDs on listed dates".into());
        }
        let delivery = match (&ev.exposure_id, &ev.delivery_id) {
            (Some(x), Some(y)) => deliveries.get(&(x.as_str(), y.as_str())).copied(),
            _ => None,
        };
        let ok = match ev.kind {
            EventKind::Consumption => {
                delivery.is_some_and(|d| d.ordinal <= ev.ordinal)
                    && ev.fraction.is_some_and(|f| f > 0. && f <= 1.)
                    && ev.status.is_none()
                    && ev.relationship_id.is_none()
                    && r.exposures.iter().any(|e| {
                        Some(&e.exposure_id) == ev.exposure_id.as_ref()
                            && e.direction == Direction::Purchase
                    })
            }
            EventKind::ForecastStatus => {
                delivery.is_some_and(|d| d.ordinal > ev.ordinal)
                    && ev.status.is_some()
                    && ev.fraction.is_none()
                    && ev.relationship_id.is_none()
            }
            EventKind::Dedesignate => {
                ev.relationship_id
                    .as_ref()
                    .is_some_and(|x| relationships.contains(x.as_str()))
                    && ev.exposure_id.is_none()
                    && ev.delivery_id.is_none()
                    && ev.fraction.is_none()
                    && ev.status.is_none()
            }
        };
        if !ok {
            return Err("events: consumption of a delivered purchase by a fraction, a forecast status before delivery, or a dedesignation of one relationship".into());
        }
    }
    if serde_json::to_vec(r).map_err(|e| e.to_string())?.len() > MAX_INPUT {
        return Err("nonfinancial hedge input exceeds 32 MiB".into());
    }
    check()
}

/// Designated instrument change in a period, an unused slot, and the change by delivery.
type PeriodChanges = (f64, f64, Vec<(String, f64)>);

struct LegState {
    value: f64,
    designated: bool,
}
struct DeliveryState {
    recognized: f64,
    reclassified: f64,
    consumed: f64,
    status: &'static str,
    designated: bool,
}
struct RelState {
    active: bool,
    completed: bool,
    discontinued: Option<i32>,
    cum_instrument: f64,
    cum_hypothetical: f64,
    /// Hypothetical fixed price per mapped delivery: the component forward at designation.
    hyp_fixed: HashMap<String, f64>,
    hyp_value: HashMap<String, f64>,
}

pub fn evaluate(r: &Request) -> std::result::Result<Result, String> {
    admit(r)?;
    let mut ledger = Ledger {
        postings: Vec::new(),
        balances: BTreeMap::new(),
        seq: 0,
    };
    let exposure: HashMap<&str, &Exposure> = r
        .exposures
        .iter()
        .map(|e| (e.exposure_id.as_str(), e))
        .collect();
    let rel_of_leg: HashMap<(&str, &str), (&Relationship, &str)> = r
        .relationships
        .iter()
        .flat_map(|rel| {
            rel.legs.iter().map(move |m| {
                (
                    (rel.instrument_position_id.as_str(), m.leg_id.as_str()),
                    (rel, m.delivery_id.as_str()),
                )
            })
        })
        .collect();
    let mut legs: HashMap<(&str, &str), LegState> = HashMap::new();
    let mut deliveries: BTreeMap<(String, String), DeliveryState> = r
        .exposures
        .iter()
        .flat_map(|e| {
            e.deliveries.iter().map(move |d| {
                (
                    (e.exposure_id.clone(), d.delivery_id.clone()),
                    DeliveryState {
                        recognized: 0.,
                        reclassified: 0.,
                        consumed: 0.,
                        status: "probable",
                        designated: false,
                    },
                )
            })
        })
        .collect();
    let mut rels: HashMap<&str, RelState> = r
        .relationships
        .iter()
        .map(|x| {
            (
                x.relationship_id.as_str(),
                RelState {
                    active: false,
                    completed: false,
                    discontinued: None,
                    cum_instrument: 0.,
                    cum_hypothetical: 0.,
                    hyp_fixed: HashMap::new(),
                    hyp_value: HashMap::new(),
                },
            )
        })
        .collect();
    let mut legs_out = Vec::new();
    let mut periods = Vec::new();
    for (k, d) in r.dates.iter().enumerate() {
        check()?;
        let t = d.ordinal;
        // Swap legs: trade, revaluation and settlement.
        let mut by_rel: HashMap<&str, PeriodChanges> = HashMap::new();
        for s in &r.instruments {
            let sign = if s.side == SwapSide::ReceiveIndex {
                1.
            } else {
                -1.
            };
            for leg in &s.legs {
                if t < s.trade_ordinal
                    || (k > 0 && r.dates[k - 1].ordinal >= leg.settlement_ordinal)
                {
                    continue;
                }
                let key = (s.position_id.as_str(), leg.leg_id.as_str());
                let v = leg_value(
                    d,
                    &s.index_id,
                    sign,
                    leg.quantity,
                    s.fixed_price,
                    leg.settlement_ordinal,
                )?;
                let tags = (Some(s.position_id.as_str()), None, None);
                if t == s.trade_ordinal {
                    // An off-market value at trade is a cash premium, never a day-one gain.
                    ledger.post(
                        t,
                        "commodity_swap_trade",
                        tags,
                        &[("commodity_swap", v), ("cash", -v)],
                    )?;
                    let designated = rel_of_leg
                        .get(&key)
                        .is_some_and(|(rel, _)| rels[rel.relationship_id.as_str()].active);
                    legs.insert(
                        key,
                        LegState {
                            value: v,
                            designated,
                        },
                    );
                    legs_out.push(LegValue {
                        ordinal: t,
                        position_id: s.position_id.clone(),
                        leg_id: leg.leg_id.clone(),
                        value: v,
                        change: 0.,
                        designated,
                    });
                    continue;
                }
                let state = legs.get_mut(&key).ok_or("leg state")?;
                let change = amount(v - state.value)?;
                state.value = v;
                let mapped = rel_of_leg.get(&key).copied();
                let designated = state.designated
                    && mapped.is_some_and(|(rel, _)| rels[rel.relationship_id.as_str()].active);
                if designated {
                    let (rel, delivery) = mapped.ok_or("mapping")?;
                    let entry = by_rel.entry(rel.relationship_id.as_str()).or_default();
                    entry.0 = amount(entry.0 + change)?;
                    entry.2.push((delivery.to_string(), change));
                } else {
                    ledger.post(
                        t,
                        "commodity_swap_mark",
                        tags,
                        &[
                            ("commodity_swap", change),
                            ("pnl:commodity_swap_mark", -change),
                        ],
                    )?;
                }
                legs_out.push(LegValue {
                    ordinal: t,
                    position_id: s.position_id.clone(),
                    leg_id: leg.leg_id.clone(),
                    value: v,
                    change,
                    designated,
                });
            }
        }
        // Hypothetical derivative per active relationship and the cumulative dollar offset.
        for rel in &r.relationships {
            let st = rels.get_mut(rel.relationship_id.as_str()).ok_or("state")?;
            if !st.active || rel.designation_ordinal == t {
                continue;
            }
            let e = exposure[rel.exposure_id.as_str()];
            let sign = if e.direction == Direction::Purchase {
                1.
            } else {
                -1.
            };
            let swap = r
                .instruments
                .iter()
                .find(|s| s.position_id == rel.instrument_position_id)
                .ok_or("swap")?;
            let mut hyp_change = 0.;
            for m in &rel.legs {
                let del = e
                    .deliveries
                    .iter()
                    .find(|x| x.delivery_id == m.delivery_id)
                    .ok_or("delivery")?;
                let leg = swap
                    .legs
                    .iter()
                    .find(|l| l.leg_id == m.leg_id)
                    .ok_or("leg")?;
                if r.dates[k - 1].ordinal >= del.ordinal
                    || !deliveries[&(e.exposure_id.clone(), m.delivery_id.clone())].designated
                {
                    continue;
                }
                let v = leg_value(
                    d,
                    e.component.index(),
                    sign,
                    leg.quantity,
                    st.hyp_fixed[&m.delivery_id],
                    del.ordinal,
                )?;
                let prev = st.hyp_value[&m.delivery_id];
                hyp_change = amount(hyp_change + v - prev)?;
                st.hyp_value.insert(m.delivery_id.clone(), v);
            }
            let (inst_change, _, parts) = by_rel
                .remove(rel.relationship_id.as_str())
                .unwrap_or_default();
            st.cum_instrument = amount(st.cum_instrument + inst_change)?;
            st.cum_hypothetical = amount(st.cum_hypothetical + hyp_change)?;
            let ratio = ratio(st.cum_instrument, st.cum_hypothetical);
            let passed = ratio.is_none_or(|x| {
                x >= r.policy.assessment_lower_bound && x <= r.policy.assessment_upper_bound
            });
            let tags = (
                Some(rel.instrument_position_id.as_str()),
                Some(rel.relationship_id.as_str()),
                None,
            );
            if passed {
                for (delivery, change) in &parts {
                    ledger.post(
                        t,
                        "cash_flow_hedge_oci",
                        (tags.0, tags.1, Some(delivery)),
                        &[
                            ("commodity_swap", *change),
                            ("cash_flow_hedge_oci", -change),
                        ],
                    )?;
                    let ds = deliveries
                        .get_mut(&(e.exposure_id.clone(), delivery.clone()))
                        .ok_or("delivery state")?;
                    ds.recognized = amount(ds.recognized + change)?;
                }
            } else {
                ledger.post(
                    t,
                    "commodity_swap_mark",
                    tags,
                    &[
                        ("commodity_swap", inst_change),
                        ("pnl:commodity_swap_mark", -inst_change),
                    ],
                )?;
                st.active = false;
                st.discontinued = Some(t);
            }
            periods.push(Period {
                ordinal: t,
                relationship_id: rel.relationship_id.clone(),
                instrument_change: inst_change,
                hypothetical_change: hyp_change,
                cumulative_instrument_change: st.cum_instrument,
                cumulative_hypothetical_change: st.cum_hypothetical,
                cumulative_ratio: ratio,
                assessment_passed: passed,
                oci: if passed { inst_change } else { 0. },
                earnings: if passed { 0. } else { inst_change },
            });
        }
        // Designation at the end of this date: changes up to now were undesignated, and the
        // hypothetical fixed prices are the component forwards today.
        for rel in r
            .relationships
            .iter()
            .filter(|x| x.designation_ordinal == t)
        {
            let e = exposure[rel.exposure_id.as_str()];
            let st = rels.get_mut(rel.relationship_id.as_str()).ok_or("state")?;
            st.active = true;
            for m in &rel.legs {
                let del = e
                    .deliveries
                    .iter()
                    .find(|x| x.delivery_id == m.delivery_id)
                    .ok_or("delivery")?;
                st.hyp_fixed.insert(
                    m.delivery_id.clone(),
                    price(d, e.component.index(), del.ordinal)?,
                );
                st.hyp_value.insert(m.delivery_id.clone(), 0.);
                if let Some(ds) =
                    deliveries.get_mut(&(e.exposure_id.clone(), m.delivery_id.clone()))
                {
                    ds.designated = true;
                }
            }
            for m in &rel.legs {
                if let Some(l) =
                    legs.get_mut(&(rel.instrument_position_id.as_str(), m.leg_id.as_str()))
                {
                    l.designated = true;
                }
            }
        }
        if !by_rel.is_empty() {
            return Err("designated swap changes without an assessment".into());
        }
        // Settlements on this date close their legs in cash.
        for s in &r.instruments {
            for leg in s.legs.iter().filter(|l| l.settlement_ordinal == t) {
                let key = (s.position_id.as_str(), leg.leg_id.as_str());
                if let Some(state) = legs.get_mut(&key) {
                    let v = state.value;
                    ledger.post(
                        t,
                        "commodity_swap_settlement",
                        (Some(s.position_id.as_str()), None, None),
                        &[("cash", v), ("commodity_swap", -v)],
                    )?;
                    state.value = 0.;
                }
            }
        }
        // Forecast outcomes on this date: sales affect earnings at delivery.
        for e in &r.exposures {
            for del in e.deliveries.iter().filter(|x| x.ordinal == t) {
                let ds = deliveries
                    .get_mut(&(e.exposure_id.clone(), del.delivery_id.clone()))
                    .ok_or("delivery state")?;
                if ds.status == "probable_not_to_occur" {
                    continue;
                }
                ds.status = "occurred";
                if e.direction == Direction::Sale {
                    let release = amount(ds.recognized - ds.reclassified)?;
                    ledger.post(
                        t,
                        "sale_reclassification",
                        (None, None, Some(del.delivery_id.as_str())),
                        &[
                            ("cash_flow_hedge_oci", release),
                            ("pnl:revenue_hedge_reclassification", -release),
                        ],
                    )?;
                    ds.reclassified = ds.recognized;
                }
            }
        }
        for ev in r.events.iter().filter(|x| x.ordinal == t) {
            match ev.kind {
                EventKind::Consumption => {
                    let key = (
                        ev.exposure_id.clone().ok_or("exposure")?,
                        ev.delivery_id.clone().ok_or("delivery")?,
                    );
                    let ds = deliveries.get_mut(&key).ok_or("delivery state")?;
                    let f = ev.fraction.ok_or("fraction")?;
                    if ds.status != "occurred" || ds.consumed + f > 1. + 1e-12 {
                        return Err("consumption exceeds the delivered purchase".into());
                    }
                    let release =
                        amount((ds.recognized - ds.reclassified) * f / (1. - ds.consumed))?;
                    ledger.post(
                        t,
                        "consumption_reclassification",
                        (None, None, Some(key.1.as_str())),
                        &[
                            ("cash_flow_hedge_oci", release),
                            ("pnl:cost_of_sales_hedge_reclassification", -release),
                        ],
                    )?;
                    ds.reclassified = amount(ds.reclassified + release)?;
                    ds.consumed = (ds.consumed + f).min(1.);
                }
                EventKind::ForecastStatus => {
                    let key = (
                        ev.exposure_id.clone().ok_or("exposure")?,
                        ev.delivery_id.clone().ok_or("delivery")?,
                    );
                    let ds = deliveries.get_mut(&key).ok_or("delivery state")?;
                    ds.designated = false;
                    // The mapped leg is no longer designated prospectively.
                    for rel in r.relationships.iter().filter(|x| x.exposure_id == key.0) {
                        for m in rel.legs.iter().filter(|m| m.delivery_id == key.1) {
                            if let Some(l) = legs
                                .get_mut(&(rel.instrument_position_id.as_str(), m.leg_id.as_str()))
                            {
                                l.designated = false;
                            }
                        }
                    }
                    if ev.status == Some(ForecastStatus::ProbableNotToOccur) {
                        let release = amount(ds.recognized - ds.reclassified)?;
                        ledger.post(
                            t,
                            "missed_forecast_reclassification",
                            (None, None, Some(key.1.as_str())),
                            &[
                                ("cash_flow_hedge_oci", release),
                                ("pnl:missed_forecast_reclassification", -release),
                            ],
                        )?;
                        ds.reclassified = ds.recognized;
                        ds.status = "probable_not_to_occur";
                    } else {
                        ds.status = "not_probable";
                    }
                }
                EventKind::Dedesignate => {
                    let id = ev.relationship_id.as_deref().ok_or("relationship")?;
                    let st = rels.get_mut(id).ok_or("state")?;
                    if !st.active {
                        return Err("dedesignation of a relationship that is not active".into());
                    }
                    st.active = false;
                    st.discontinued = Some(t);
                }
            }
        }
        // A relationship whose mapped deliveries are all past or undesignated ends.
        for rel in &r.relationships {
            let st = rels.get_mut(rel.relationship_id.as_str()).ok_or("state")?;
            let e = exposure[rel.exposure_id.as_str()];
            if st.active
                && rel.legs.iter().all(|m| {
                    let ds = &deliveries[&(e.exposure_id.clone(), m.delivery_id.clone())];
                    !ds.designated
                        || e.deliveries
                            .iter()
                            .any(|x| x.delivery_id == m.delivery_id && x.ordinal <= t)
                })
                && rel.designation_ordinal < t
            {
                st.active = false;
                st.completed = true;
            }
        }
    }
    let deliveries_out = deliveries
        .iter()
        .map(|((x, y), d)| DeliveryClosing {
            exposure_id: x.clone(),
            delivery_id: y.clone(),
            status: d.status.into(),
            oci_recognized: d.recognized,
            oci_reclassified: d.reclassified,
            oci_balance: d.recognized - d.reclassified,
            consumed_fraction: d.consumed,
        })
        .collect();
    let relationships = r
        .relationships
        .iter()
        .map(|rel| {
            let st = &rels[rel.relationship_id.as_str()];
            let c = &exposure[rel.exposure_id.as_str()].component;
            RelationshipClosing {
                relationship_id: rel.relationship_id.clone(),
                status: if st.active {
                    "qualified"
                } else if st.completed {
                    "completed"
                } else if st.discontinued.is_some() {
                    "discontinued"
                } else {
                    "pending"
                }
                .into(),
                discontinued_ordinal: st.discontinued,
                component_kind: match c {
                    Component::ContractuallySpecified { .. } => "contractually_specified",
                    Component::ClearlyAndCloselyRelated { .. } => "clearly_and_closely_related",
                    Component::Subcomponent { .. } => "subcomponent",
                }
                .into(),
                asu_2025_09_component: c.needs_adoption(),
            }
        })
        .collect();
    let closing: Vec<_> = ledger
        .balances
        .iter()
        .map(|(a, v)| Balance {
            account: a.clone(),
            debit_balance: *v,
        })
        .collect();
    let total = closing
        .iter()
        .try_fold(0., |a, b| amount(a + b.debit_balance))?;
    if !near(total, 0.) {
        return Err("nonfinancial hedge trial balance does not balance".into());
    }
    let result = Result {
        schema: SCHEMA.into(),
        model_version: SCHEMA.into(),
        run: r.run.clone(),
        entity: r.entity.clone(),
        currency: r.currency.clone(),
        policy: r.policy.identity.clone(),
        journal: ledger.postings,
        legs: legs_out,
        periods,
        deliveries: deliveries_out,
        relationships,
        closing,
        purchase_and_inventory_cash_included: false,
        accounting_standard_approval: false,
        empirical_validation: false,
        capital_ftp_eligible: false,
        disclosure: "Cash flow hedges of forecasted nonfinancial purchases and sales by commodity swaps on supplied forward price curves and discount curves. Eligibility of the hedged component (contractually specified, or after ASU 2025-09 adoption clearly and closely related or a subcomponent, 815-20-25-22C) is a supplied reviewed basis, never inferred. Effectiveness is a cumulative dollar offset against a hypothetical derivative on the component's own index. Designated leg changes go to OCI by delivery; sales reclassify at delivery, purchases as the delivered asset is consumed, and forecasts probable not to occur at once. The purchase or sale cash and inventory themselves belong to the product ledger. Not an accounting-standard approval.".into(),
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_OUTPUT
    {
        return Err("nonfinancial hedge output exceeds 32 MiB".into());
    }
    check()?;
    Ok(result)
}
