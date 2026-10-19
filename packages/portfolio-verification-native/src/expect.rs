//! Independent replay of one scenario's execution block over the raw plan. This is the second
//! implementation of the lifecycle equations: it shares no code with the producer, uses the
//! native swap valuations only after `swap::check` has confirmed them, and derives every expected
//! posting, closing stock, status and accounting fact from the raw instructions.
use crate::{near, NAMESPACE};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Deserialize)]
pub struct Revision {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub account_id: String,
    pub cash: f64,
    pub equity: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Lot {
    pub lot_id: String,
    pub position_id: String,
    pub account_id: String,
    pub kind: String,
    pub quantity: f64,
    pub acquisition_ordinal: i64,
    pub original_cost: f64,
    #[serde(default)]
    pub outside_encumbrance: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Allocation {
    pub allocation_id: String,
    pub exposure_lot_id: String,
    pub hedge_lot_id: String,
    #[serde(default)]
    pub formal_relationship_id: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Pledge {
    pub pledge_id: String,
    pub lot_id: String,
    pub agreement_id: String,
    pub quantity: f64,
}
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OpeningCollateral {
    pub posted_vm: f64,
    pub received_vm: f64,
    pub posted_im: f64,
    pub received_im_custody: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Agreement {
    pub agreement_id: String,
    pub settlement_lag_days: i64,
    pub initial_margin: String,
    pub segregation: String,
    pub vm_at_termination: String,
    pub lot_ids: Vec<String>,
    #[serde(default)]
    pub opening_collateral: Option<OpeningCollateral>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Facility {
    pub facility_id: String,
    pub commitment: f64,
    pub drawn: f64,
    pub expiry_ordinal: i64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ActionLot {
    pub lot_id: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Action {
    pub action_id: String,
    pub lots: Vec<ActionLot>,
    pub accounting_treatment: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub as_of_ordinal: i64,
    pub accounts: Vec<Account>,
    pub lots: Vec<Lot>,
    #[serde(default)]
    pub hedge_allocations: Vec<Allocation>,
    #[serde(default)]
    pub pledges: Vec<Pledge>,
    #[serde(default)]
    pub margin_agreements: Vec<Agreement>,
    #[serde(default)]
    pub facilities: Vec<Facility>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Common {
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Purchase {
    pub lot_id: String,
    pub trade_ordinal: i64,
    pub settlement_ordinal: i64,
    pub clean_price_per_100: f64,
    pub accrued_interest: f64,
    pub fees: f64,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Sale {
    pub lot_id: String,
    pub quantity: f64,
    pub trade_ordinal: i64,
    pub settlement_ordinal: i64,
    pub clean_price_per_100: f64,
    pub accrued_interest: f64,
    pub fees: f64,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Advance {
    pub lot_id: String,
    pub advance_ordinal: i64,
    pub principal: f64,
    pub pledge_id: String,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub pledge_id: String,
    pub quantity: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Repayment {
    pub lot_id: String,
    pub repayment_ordinal: i64,
    pub principal: f64,
    pub accrued_interest: f64,
    pub break_cost: f64,
    #[serde(default)]
    pub pledge_releases: Vec<Release>,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct VmTerms {
    pub receive_threshold: f64,
    pub post_threshold: f64,
    pub minimum_transfer: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub lot_id: String,
    pub agreement_id: String,
    pub entry_ordinal: i64,
    pub upfront_consideration: f64,
    pub variation_margin: VmTerms,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Consideration {
    ModelCleanValue,
    ExecutionQuote { amount: f64 },
}
#[derive(Debug, Clone, Deserialize)]
pub struct Termination {
    pub lot_id: String,
    pub agreement_id: String,
    pub effective_ordinal: i64,
    pub settlement_ordinal: i64,
    pub consideration: Consideration,
    pub fees: f64,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct FacilityMove {
    pub facility_id: String,
    pub ordinal: i64,
    pub amount: f64,
    #[serde(flatten)]
    pub common: Common,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Im {
    pub agreement_id: String,
    pub portfolio_lot_ids: Vec<String>,
    pub scenario_id: String,
    pub ordinal: i64,
    pub posted_requirement: f64,
    pub received_requirement: f64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Execution {
    pub scenario_id: String,
    pub horizon_end_ordinal: i64,
    #[serde(default)]
    pub purchases: Vec<Purchase>,
    #[serde(default)]
    pub sales: Vec<Sale>,
    #[serde(default)]
    pub financings: Vec<Advance>,
    #[serde(default)]
    pub repayments: Vec<Repayment>,
    #[serde(default)]
    pub hedge_entries: Vec<Entry>,
    #[serde(default)]
    pub terminations: Vec<Termination>,
    #[serde(default)]
    pub facility_draws: Vec<FacilityMove>,
    #[serde(default)]
    pub facility_repayments: Vec<FacilityMove>,
    #[serde(default)]
    pub initial_margin: Vec<Im>,
}

/// A confirmed native valuation of a derivative lot at a date.
#[derive(Debug, Clone, Copy)]
pub struct Valuation {
    pub clean_value: f64,
    pub due_cash: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub event_id: String,
    pub ordinal: i64,
    pub memo: bool,
    pub postings: Vec<(String, String, f64)>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub kind: &'static str,
    pub lot_id: String,
    pub quantity: f64,
    pub retained_quantity: f64,
    pub consideration: f64,
    pub removed_carrying: f64,
    pub realized: f64,
    pub economic_allocation_ids: Vec<String>,
    pub formal_relationship_ids: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Expected {
    pub opening_cash_bits: u64,
    pub intents: Vec<Intent>,
    pub closing: BTreeMap<(String, String), f64>,
    pub closing_cash: f64,
    pub cash_movements: f64,
    pub events: Vec<(String, &'static str)>,
    pub facts: Vec<Fact>,
    pub posted_vm: f64,
    pub received_vm: f64,
    pub posted_im: f64,
    pub received_im: f64,
    pub fees: f64,
    pub realized: f64,
    pub open_claims: bool,
}

const GL: [(&str, &str); 17] = [
    ("cash", "cash"),
    ("asset_principal", "asset_principal"),
    ("book_adjustment", "book_adjustment"),
    ("interest_receivable", "dated_interest_receivable"),
    ("funding_principal", "funding_principal"),
    ("derivative_asset", "derivative_asset"),
    ("derivative_liability", "derivative_liability"),
    ("posted_vm", "derivative_posted_vm_asset"),
    ("received_vm", "derivative_received_vm_liability"),
    ("posted_im", "derivative_posted_im_asset"),
    ("unpaid_receivable", "derivative_unpaid_receivable"),
    ("unpaid_payable", "derivative_unpaid_payable"),
    ("fees", "pnl:lifecycle_fees"),
    ("realized", "pnl:lifecycle_realized"),
    ("interest", "pnl:lifecycle_interest"),
    ("custody_asset", "memo:derivative_custodian_asset"),
    ("custody_obligation", "memo:derivative_custodian_obligation"),
];
fn gl(name: &str) -> &'static str {
    GL.iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
        .expect("known account")
}
fn instrument(name: &str, id: &str) -> String {
    if name == "cash" {
        String::new()
    } else {
        format!("{NAMESPACE}{id}")
    }
}

enum Body<'a> {
    Advance(&'a Advance),
    Draw(&'a FacilityMove),
    FacilityRepay(&'a FacilityMove),
    Repayment(&'a Repayment),
    Release(&'a Pledge, f64),
    Purchase(&'a Purchase, &'a Lot),
    Sale(&'a Sale, &'a Lot),
    Pledge(&'a Pledge),
    Entry(&'a Entry, f64),
    TerminationEffective(&'a Termination, f64, f64),
    TerminationSettle(&'a Termination, &'a Agreement),
    VmSettle(&'a Agreement, f64),
    VmReturn(&'a Agreement),
    ImReturn(&'a Agreement),
    Im(&'a Im, &'a Agreement),
}
struct Event<'a> {
    id: String,
    ordinal: i64,
    phase: u32,
    depends_on: Vec<String>,
    failed: bool,
    body: Body<'a>,
}

struct Book {
    financial: BTreeMap<(String, String), f64>,
    cash: f64,
    moved: f64,
    intents: Vec<Intent>,
}
impl Book {
    fn balance(&self, name: &str, id: &str) -> f64 {
        self.financial
            .get(&(gl(name).to_string(), instrument(name, id)))
            .copied()
            .unwrap_or(0.)
    }
    fn post(
        &mut self,
        id: &str,
        ordinal: i64,
        memo: bool,
        lines: &[(&str, &str, f64)],
    ) -> Result<(), String> {
        let total: f64 = lines.iter().map(|l| l.2).sum();
        if !near(total, 0.) {
            return Err(format!("independent intent `{id}` does not balance"));
        }
        let postings: Vec<(String, String, f64)> = lines
            .iter()
            .filter(|l| l.2 != 0.)
            .map(|(name, inst, debit)| (gl(name).to_string(), instrument(name, inst), *debit))
            .collect();
        if postings.is_empty() {
            return Ok(());
        }
        for (g, i, d) in &postings {
            *self.financial.entry((g.clone(), i.clone())).or_default() += d;
            if g == "cash" {
                self.cash += d;
                self.moved += d;
            }
        }
        if self.cash < -1e-9 {
            return Err(format!(
                "independent replay: `{id}` would leave cash at {}",
                self.cash
            ));
        }
        self.intents.push(Intent {
            event_id: id.into(),
            ordinal,
            memo,
            postings,
        });
        Ok(())
    }
}

fn push<'a>(
    ids: &mut BTreeSet<String>,
    events: &mut Vec<Event<'a>>,
    e: Event<'a>,
) -> Result<(), String> {
    if !ids.insert(e.id.clone()) {
        return Err(format!(
            "independent replay: duplicate instruction `{}`",
            e.id
        ));
    }
    events.push(e);
    Ok(())
}

pub fn replay(
    plan: &Plan,
    acq: &Execution,
    valuations: &HashMap<(String, i64), Valuation>,
    account_id: &str,
) -> Result<Expected, String> {
    let account = plan
        .accounts
        .iter()
        .find(|a| a.account_id == account_id)
        .ok_or("unknown plan account")?;
    let asof = plan.as_of_ordinal;
    let end = acq.horizon_end_ordinal;
    let lots: HashMap<&str, &Lot> = plan.lots.iter().map(|l| (l.lot_id.as_str(), l)).collect();
    let agreements: HashMap<&str, &Agreement> = plan
        .margin_agreements
        .iter()
        .map(|a| (a.agreement_id.as_str(), a))
        .collect();
    let pledges: HashMap<&str, &Pledge> = plan
        .pledges
        .iter()
        .map(|p| (p.pledge_id.as_str(), p))
        .collect();
    let facilities: HashMap<&str, &Facility> = plan
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f))
        .collect();
    let lot = |id: &str| -> Result<&Lot, String> {
        lots.get(id)
            .copied()
            .filter(|l| l.account_id == account_id)
            .ok_or_else(|| format!("independent replay: unknown lot `{id}` for `{account_id}`"))
    };
    let touched: BTreeSet<&str> = acq
        .purchases
        .iter()
        .map(|x| x.lot_id.as_str())
        .chain(acq.sales.iter().map(|x| x.lot_id.as_str()))
        .chain(acq.financings.iter().map(|x| x.lot_id.as_str()))
        .chain(acq.repayments.iter().map(|x| x.lot_id.as_str()))
        .chain(acq.hedge_entries.iter().map(|x| x.lot_id.as_str()))
        .chain(acq.terminations.iter().map(|x| x.lot_id.as_str()))
        .collect();
    for a in &plan.actions {
        if a.accounting_treatment == "formal"
            && a.lots.iter().any(|l| touched.contains(l.lot_id.as_str()))
        {
            return Err(format!("independent replay: action `{}` selects formal treatment, which this journal cannot carry", a.action_id));
        }
    }
    let name = |c: &Common, default: String| c.instruction_id.clone().unwrap_or(default);
    let failed = |c: &Common| c.state.as_deref() == Some("failed");
    let mut events: Vec<Event> = Vec::new();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    let opening_lots: Vec<&Lot> = plan
        .lots
        .iter()
        .filter(|l| l.account_id == account_id && l.acquisition_ordinal <= asof)
        .collect();
    for p in &acq.purchases {
        let l = lot(&p.lot_id)?;
        let cost = p.clean_price_per_100 / 100. * l.quantity;
        if !near(l.original_cost, cost + p.fees) {
            return Err(format!(
                "independent replay: purchase `{}` cost differs from the lot's original cost",
                p.lot_id
            ));
        }
        push(
            &mut ids,
            &mut events,
            Event {
                id: name(&p.common, format!("purchase:{}", p.lot_id)),
                ordinal: p.settlement_ordinal,
                phase: 4,
                depends_on: p.common.depends_on.clone(),
                failed: failed(&p.common),
                body: Body::Purchase(p, l),
            },
        )?;
    }
    for s in &acq.sales {
        let l = lot(&s.lot_id)?;
        push(
            &mut ids,
            &mut events,
            Event {
                id: name(
                    &s.common,
                    format!("sale:{}:{}", s.lot_id, s.settlement_ordinal),
                ),
                ordinal: s.settlement_ordinal,
                phase: 4,
                depends_on: s.common.depends_on.clone(),
                failed: failed(&s.common),
                body: Body::Sale(s, l),
            },
        )?;
    }
    for f in &acq.financings {
        let l = lot(&f.lot_id)?;
        if !near(f.principal, l.quantity) {
            return Err("independent replay: advance differs from the funding lot".into());
        }
        let pledge = pledges
            .get(f.pledge_id.as_str())
            .copied()
            .ok_or("independent replay: unknown pledge")?;
        let id = name(&f.common, format!("advance:{}", f.lot_id));
        push(
            &mut ids,
            &mut events,
            Event {
                id: id.clone(),
                ordinal: f.advance_ordinal,
                phase: 1,
                depends_on: f.common.depends_on.clone(),
                failed: failed(&f.common),
                body: Body::Advance(f),
            },
        )?;
        push(
            &mut ids,
            &mut events,
            Event {
                id: format!("pledge:{}", pledge.pledge_id),
                ordinal: f.advance_ordinal,
                phase: 5,
                depends_on: vec![id],
                failed: false,
                body: Body::Pledge(pledge),
            },
        )?;
    }
    for r in &acq.repayments {
        lot(&r.lot_id)?;
        let id = name(
            &r.common,
            format!("repay:{}:{}", r.lot_id, r.repayment_ordinal),
        );
        push(
            &mut ids,
            &mut events,
            Event {
                id: id.clone(),
                ordinal: r.repayment_ordinal,
                phase: 2,
                depends_on: r.common.depends_on.clone(),
                failed: failed(&r.common),
                body: Body::Repayment(r),
            },
        )?;
        for rel in &r.pledge_releases {
            let p = pledges
                .get(rel.pledge_id.as_str())
                .copied()
                .ok_or("independent replay: unknown pledge")?;
            push(
                &mut ids,
                &mut events,
                Event {
                    id: format!("release:{}:{}", p.pledge_id, r.repayment_ordinal),
                    ordinal: r.repayment_ordinal,
                    phase: 3,
                    depends_on: vec![id.clone()],
                    failed: false,
                    body: Body::Release(p, rel.quantity),
                },
            )?;
        }
    }
    for h in &acq.hedge_entries {
        lot(&h.lot_id)?;
        let a = agreements
            .get(h.agreement_id.as_str())
            .copied()
            .ok_or("independent replay: unknown agreement")?;
        let v = valuations
            .get(&(h.lot_id.clone(), h.entry_ordinal))
            .ok_or("independent replay: entry valuation missing")?;
        if !near(h.upfront_consideration, v.clean_value) {
            return Err(format!(
                "independent replay: entry `{}` upfront consideration differs from its clean value",
                h.lot_id
            ));
        }
        let id = name(&h.common, format!("entry:{}", h.lot_id));
        push(
            &mut ids,
            &mut events,
            Event {
                id: id.clone(),
                ordinal: h.entry_ordinal,
                phase: 6,
                depends_on: h.common.depends_on.clone(),
                failed: failed(&h.common),
                body: Body::Entry(h, v.clean_value),
            },
        )?;
        let t = &h.variation_margin;
        let call = if v.clean_value > t.receive_threshold {
            v.clean_value - t.receive_threshold
        } else if v.clean_value < -t.post_threshold {
            v.clean_value + t.post_threshold
        } else {
            0.
        };
        if call != 0.
            && call.abs() >= t.minimum_transfer
            && h.entry_ordinal + a.settlement_lag_days <= end
        {
            push(
                &mut ids,
                &mut events,
                Event {
                    id: format!("vm:{}", h.lot_id),
                    ordinal: h.entry_ordinal + a.settlement_lag_days,
                    phase: 8,
                    depends_on: vec![id],
                    failed: false,
                    body: Body::VmSettle(a, call),
                },
            )?;
        }
    }
    for t in &acq.terminations {
        lot(&t.lot_id)?;
        let a = agreements
            .get(t.agreement_id.as_str())
            .copied()
            .ok_or("independent replay: unknown agreement")?;
        let v = valuations
            .get(&(t.lot_id.clone(), t.effective_ordinal))
            .ok_or("independent replay: termination valuation missing")?;
        let consideration = match &t.consideration {
            Consideration::ModelCleanValue => v.clean_value,
            Consideration::ExecutionQuote { amount } => *amount,
        };
        let id = name(&t.common, format!("terminate:{}", t.lot_id));
        push(
            &mut ids,
            &mut events,
            Event {
                id: id.clone(),
                ordinal: t.effective_ordinal,
                phase: 6,
                depends_on: t.common.depends_on.clone(),
                failed: failed(&t.common),
                body: Body::TerminationEffective(t, consideration, v.due_cash),
            },
        )?;
        let settle = format!("terminate_settle:{}", t.lot_id);
        push(
            &mut ids,
            &mut events,
            Event {
                id: settle.clone(),
                ordinal: t.settlement_ordinal,
                phase: 6,
                depends_on: vec![id],
                failed: false,
                body: Body::TerminationSettle(t, a),
            },
        )?;
        if a.vm_at_termination == "return_separately"
            && t.settlement_ordinal + a.settlement_lag_days <= end
        {
            push(
                &mut ids,
                &mut events,
                Event {
                    id: format!("vm_return:{}", t.lot_id),
                    ordinal: t.settlement_ordinal + a.settlement_lag_days,
                    phase: 8,
                    depends_on: vec![settle],
                    failed: false,
                    body: Body::VmReturn(a),
                },
            )?;
        }
    }
    for d in &acq.facility_draws {
        facilities
            .get(d.facility_id.as_str())
            .ok_or("independent replay: unknown facility")?;
        push(
            &mut ids,
            &mut events,
            Event {
                id: name(&d.common, format!("draw:{}:{}", d.facility_id, d.ordinal)),
                ordinal: d.ordinal,
                phase: 1,
                depends_on: d.common.depends_on.clone(),
                failed: failed(&d.common),
                body: Body::Draw(d),
            },
        )?;
    }
    for r in &acq.facility_repayments {
        facilities
            .get(r.facility_id.as_str())
            .ok_or("independent replay: unknown facility")?;
        push(
            &mut ids,
            &mut events,
            Event {
                id: name(
                    &r.common,
                    format!("facility_repay:{}:{}", r.facility_id, r.ordinal),
                ),
                ordinal: r.ordinal,
                phase: 7,
                depends_on: r.common.depends_on.clone(),
                failed: failed(&r.common),
                body: Body::FacilityRepay(r),
            },
        )?;
    }
    for im in &acq.initial_margin {
        let a = agreements
            .get(im.agreement_id.as_str())
            .copied()
            .ok_or("independent replay: unknown agreement")?;
        if im.scenario_id != acq.scenario_id {
            return Err("independent replay: IM requirement is bound to another scenario".into());
        }
        push(
            &mut ids,
            &mut events,
            Event {
                id: format!("im:{}:{}", im.agreement_id, im.ordinal),
                ordinal: im.ordinal,
                phase: 9,
                depends_on: vec![],
                failed: false,
                body: Body::Im(im, a),
            },
        )?;
    }
    // Supplied IM: scope and requirement bound to each population change; the methodology
    // behind the amounts is not certified here.
    let mut changes: BTreeSet<(&str, i64)> = BTreeSet::new();
    for h in &acq.hedge_entries {
        changes.insert((&h.agreement_id, h.entry_ordinal));
    }
    for t in &acq.terminations {
        changes.insert((&t.agreement_id, t.effective_ordinal));
    }
    for (agreement_id, ordinal) in &changes {
        let a = agreements[agreement_id];
        if a.initial_margin != "supplied_requirements" {
            continue;
        }
        let mut live: BTreeSet<&str> = a
            .lot_ids
            .iter()
            .map(String::as_str)
            .filter(|id| opening_lots.iter().any(|l| l.lot_id == *id))
            .collect();
        for h in acq
            .hedge_entries
            .iter()
            .filter(|h| h.agreement_id == *agreement_id && h.entry_ordinal <= *ordinal)
        {
            live.insert(&h.lot_id);
        }
        for t in acq
            .terminations
            .iter()
            .filter(|t| t.agreement_id == *agreement_id && t.effective_ordinal <= *ordinal)
        {
            live.remove(t.lot_id.as_str());
        }
        let bound = acq.initial_margin.iter().any(|im| {
            let supplied: BTreeSet<&str> =
                im.portfolio_lot_ids.iter().map(String::as_str).collect();
            im.agreement_id == *agreement_id && im.ordinal == *ordinal && supplied == live
        });
        if !bound {
            return Err(format!("independent replay: agreement `{agreement_id}` changes population on {ordinal} without a bound IM requirement"));
        }
        if live.is_empty() {
            let settles: Vec<String> = acq
                .terminations
                .iter()
                .filter(|t| t.agreement_id == *agreement_id)
                .map(|t| format!("terminate_settle:{}", t.lot_id))
                .collect();
            let latest = acq
                .terminations
                .iter()
                .filter(|t| t.agreement_id == *agreement_id)
                .map(|t| t.settlement_ordinal)
                .max()
                .unwrap_or(*ordinal);
            if latest + a.settlement_lag_days <= end {
                push(
                    &mut ids,
                    &mut events,
                    Event {
                        id: format!("im_return:{agreement_id}:{ordinal}"),
                        ordinal: latest + a.settlement_lag_days,
                        phase: 9,
                        depends_on: settles,
                        failed: false,
                        body: Body::ImReturn(a),
                    },
                )?;
            }
        }
    }
    events.sort_by_key(|e| (e.ordinal, e.phase));
    let order: HashMap<&str, usize> = events
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    for (i, e) in events.iter().enumerate() {
        for d in &e.depends_on {
            match order.get(d.as_str()) {
                Some(&j) if j < i => {}
                _ => {
                    return Err(format!(
                    "independent replay: `{}` depends on `{d}`, which is unknown or not earlier",
                    e.id
                ))
                }
            }
        }
    }

    // Opening stocks of this owner.
    let mut book = Book {
        financial: BTreeMap::new(),
        cash: account.cash,
        moved: 0.,
        intents: Vec::new(),
    };
    let mut owned: BTreeMap<String, f64> = BTreeMap::new();
    let mut pledged: BTreeMap<String, f64> = BTreeMap::new();
    let mut pledge_left: BTreeMap<String, f64> = BTreeMap::new();
    let mut carrying: HashMap<String, f64> = HashMap::new();
    let stock = |book: &mut Book, name: &str, id: &str, v: f64| {
        if v != 0. {
            *book
                .financial
                .entry((gl(name).to_string(), instrument(name, id)))
                .or_default() += v;
        }
    };
    for l in &opening_lots {
        match l.kind.as_str() {
            "security" | "loan" => {
                stock(&mut book, "asset_principal", &l.lot_id, l.quantity);
                stock(
                    &mut book,
                    "book_adjustment",
                    &l.lot_id,
                    l.original_cost - l.quantity,
                );
                if l.kind == "security" {
                    owned.insert(l.lot_id.clone(), l.quantity);
                }
            }
            "funding" => stock(&mut book, "funding_principal", &l.lot_id, -l.quantity),
            "derivative" => {
                carrying.insert(l.lot_id.clone(), l.original_cost);
                stock(
                    &mut book,
                    if l.original_cost >= 0. {
                        "derivative_asset"
                    } else {
                        "derivative_liability"
                    },
                    &l.lot_id,
                    l.original_cost,
                );
            }
            other => return Err(format!("independent replay: unknown lot kind `{other}`")),
        }
    }
    for p in &plan.pledges {
        if opening_lots.iter().any(|l| l.lot_id == p.lot_id) {
            *pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
            pledge_left.insert(p.pledge_id.clone(), p.quantity);
        }
    }
    let (mut posted_vm, mut received_vm, mut posted_im, mut received_im) = (0., 0., 0., 0.);
    let mut custody: BTreeMap<String, f64> = BTreeMap::new();
    for a in &plan.margin_agreements {
        if let Some(c) = &a.opening_collateral {
            stock(&mut book, "posted_vm", &a.agreement_id, c.posted_vm);
            stock(&mut book, "received_vm", &a.agreement_id, -c.received_vm);
            stock(&mut book, "posted_im", &a.agreement_id, c.posted_im);
            posted_vm += c.posted_vm;
            received_vm += c.received_vm;
            posted_im += c.posted_im;
            received_im += c.received_im_custody;
            if c.received_im_custody > 0. {
                custody.insert(a.agreement_id.clone(), c.received_im_custody);
            }
        }
    }
    let available = |owned: &BTreeMap<String, f64>, pledged: &BTreeMap<String, f64>, l: &Lot| {
        owned.get(&l.lot_id).copied().unwrap_or(0.)
            - pledged.get(&l.lot_id).copied().unwrap_or(0.)
            - l.outside_encumbrance
    };
    let allocations = |id: &str| {
        let mut e = Vec::new();
        let mut f = Vec::new();
        for h in plan
            .hedge_allocations
            .iter()
            .filter(|h| h.exposure_lot_id == id || h.hedge_lot_id == id)
        {
            e.push(h.allocation_id.clone());
            if let Some(x) = &h.formal_relationship_id {
                f.push(x.clone());
            }
        }
        (e, f)
    };
    let mut drawn: HashMap<&str, f64> = plan
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f.drawn))
        .collect();
    let mut settled: BTreeSet<String> = BTreeSet::new();
    let mut statuses = Vec::new();
    let mut facts = Vec::new();
    let (mut fees, mut realized) = (0., 0.);
    for e in &events {
        if e.ordinal < asof || e.ordinal > end {
            return Err(format!(
                "independent replay: `{}` is outside the horizon",
                e.id
            ));
        }
        let status = if e.failed {
            "failed"
        } else if !e.depends_on.iter().all(|d| settled.contains(d)) {
            "blocked"
        } else {
            let id = e.id.as_str();
            match &e.body {
                Body::Advance(f) => book.post(
                    id,
                    e.ordinal,
                    false,
                    &[
                        ("cash", "", f.principal),
                        ("funding_principal", &f.lot_id, -f.principal),
                    ],
                )?,
                Body::Draw(d) => {
                    let f = facilities[d.facility_id.as_str()];
                    let used = drawn.get_mut(d.facility_id.as_str()).expect("facility");
                    if d.amount > f.commitment - *used + 1e-9 || d.ordinal > f.expiry_ordinal {
                        return Err(format!("independent replay: `{id}` exceeds the facility"));
                    }
                    *used += d.amount;
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("cash", "", d.amount),
                            ("funding_principal", &d.facility_id, -d.amount),
                        ],
                    )?;
                }
                Body::FacilityRepay(r) => {
                    let used = drawn.get_mut(r.facility_id.as_str()).expect("facility");
                    if r.amount > *used + 1e-9 {
                        return Err(format!("independent replay: `{id}` repays more than drawn"));
                    }
                    *used -= r.amount;
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("funding_principal", &r.facility_id, r.amount),
                            ("cash", "", -r.amount),
                        ],
                    )?;
                }
                Body::Repayment(r) => {
                    if r.principal > -book.balance("funding_principal", &r.lot_id) + 1e-9 {
                        return Err(format!(
                            "independent replay: `{id}` repays more than outstanding"
                        ));
                    }
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("funding_principal", &r.lot_id, r.principal),
                            ("interest", &r.lot_id, r.accrued_interest),
                            ("fees", &r.lot_id, r.break_cost),
                            (
                                "cash",
                                "",
                                -(r.principal + r.accrued_interest + r.break_cost),
                            ),
                        ],
                    )?;
                    fees += r.break_cost;
                }
                Body::Release(p, q) => {
                    let left = pledge_left
                        .get_mut(&p.pledge_id)
                        .ok_or("independent replay: release of an undelivered pledge")?;
                    if *q > *left + 1e-9 {
                        return Err(format!(
                            "independent replay: `{id}` releases more than pledged"
                        ));
                    }
                    *left -= q;
                    *pledged.entry(p.lot_id.clone()).or_default() -= q;
                }
                Body::Purchase(p, l) => {
                    let cost = p.clean_price_per_100 / 100. * l.quantity;
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("asset_principal", &l.lot_id, l.quantity),
                            ("book_adjustment", &l.lot_id, cost - l.quantity),
                            ("interest_receivable", &l.lot_id, p.accrued_interest),
                            ("fees", &l.lot_id, p.fees),
                            ("cash", "", -(cost + p.accrued_interest + p.fees)),
                        ],
                    )?;
                    fees += p.fees;
                    *owned.entry(l.lot_id.clone()).or_default() += l.quantity;
                }
                Body::Sale(s, l) => {
                    let have = owned.get(&l.lot_id).copied().unwrap_or(0.);
                    if s.quantity > available(&owned, &pledged, l) + 1e-9 {
                        return Err(format!(
                            "independent replay: `{id}` sells unavailable inventory"
                        ));
                    }
                    let share = s.quantity / have;
                    let principal = book.balance("asset_principal", &l.lot_id) * share;
                    let adjustment = book.balance("book_adjustment", &l.lot_id) * share;
                    let receivable = book.balance("interest_receivable", &l.lot_id) * share;
                    let consideration = s.clean_price_per_100 / 100. * s.quantity;
                    let removed = principal + adjustment;
                    let gain = consideration - removed;
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("cash", "", consideration + s.accrued_interest - s.fees),
                            ("asset_principal", &l.lot_id, -principal),
                            ("book_adjustment", &l.lot_id, -adjustment),
                            ("interest_receivable", &l.lot_id, -receivable),
                            ("interest", &l.lot_id, receivable - s.accrued_interest),
                            ("fees", &l.lot_id, s.fees),
                            ("realized", &l.lot_id, -gain),
                        ],
                    )?;
                    fees += s.fees;
                    realized += gain;
                    *owned.entry(l.lot_id.clone()).or_default() -= s.quantity;
                    let (ea, fr) = allocations(&l.lot_id);
                    facts.push(Fact {
                        kind: if near(s.quantity, have) {
                            "sale"
                        } else {
                            "partial_sale"
                        },
                        lot_id: l.lot_id.clone(),
                        quantity: s.quantity,
                        retained_quantity: have - s.quantity,
                        consideration,
                        removed_carrying: removed,
                        realized: gain,
                        economic_allocation_ids: ea,
                        formal_relationship_ids: fr,
                    });
                }
                Body::Pledge(p) => {
                    let l = lots[p.lot_id.as_str()];
                    if p.quantity > available(&owned, &pledged, l) + 1e-9 {
                        return Err(format!(
                            "independent replay: `{id}` pledges unavailable inventory"
                        ));
                    }
                    *pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
                    pledge_left.insert(p.pledge_id.clone(), p.quantity);
                }
                Body::Entry(h, clean) => {
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            (
                                if *clean >= 0. {
                                    "derivative_asset"
                                } else {
                                    "derivative_liability"
                                },
                                &h.lot_id,
                                *clean,
                            ),
                            ("cash", "", -h.upfront_consideration),
                        ],
                    )?;
                    carrying.insert(h.lot_id.clone(), *clean);
                }
                Body::TerminationEffective(t, consideration, due) => {
                    let c = carrying
                        .remove(&t.lot_id)
                        .ok_or("independent replay: termination of a lot that is not live")?;
                    let claim = consideration + due;
                    let gain = consideration - c;
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            (
                                if c >= 0. {
                                    "derivative_asset"
                                } else {
                                    "derivative_liability"
                                },
                                &t.lot_id,
                                -c,
                            ),
                            (
                                if claim >= 0. {
                                    "unpaid_receivable"
                                } else {
                                    "unpaid_payable"
                                },
                                &t.lot_id,
                                claim,
                            ),
                            ("interest", &t.lot_id, -due),
                            ("realized", &t.lot_id, -gain),
                        ],
                    )?;
                    realized += gain;
                    let (ea, fr) = allocations(&t.lot_id);
                    let l = lots[t.lot_id.as_str()];
                    facts.push(Fact {
                        kind: "termination",
                        lot_id: t.lot_id.clone(),
                        quantity: l.quantity,
                        retained_quantity: 0.,
                        consideration: *consideration,
                        removed_carrying: c,
                        realized: gain,
                        economic_allocation_ids: ea,
                        formal_relationship_ids: fr,
                    });
                }
                Body::TerminationSettle(t, a) => {
                    let receivable = book.balance("unpaid_receivable", &t.lot_id);
                    let payable = book.balance("unpaid_payable", &t.lot_id);
                    let claim = receivable + payable;
                    let mut lines: Vec<(&str, &str, f64)> = vec![
                        (
                            if receivable != 0. {
                                "unpaid_receivable"
                            } else {
                                "unpaid_payable"
                            },
                            &t.lot_id,
                            -claim,
                        ),
                        ("fees", &t.lot_id, t.fees),
                    ];
                    let mut cash = claim - t.fees;
                    if a.vm_at_termination == "apply_against_consideration" {
                        let posted = book.balance("posted_vm", &a.agreement_id);
                        let received = -book.balance("received_vm", &a.agreement_id);
                        lines.push(("posted_vm", &a.agreement_id, -posted));
                        lines.push(("received_vm", &a.agreement_id, received));
                        cash += posted - received;
                        posted_vm -= posted;
                        received_vm -= received;
                    }
                    lines.push(("cash", "", cash));
                    book.post(id, e.ordinal, false, &lines)?;
                    fees += t.fees;
                }
                Body::VmSettle(a, call) => {
                    if *call > 0. {
                        book.post(
                            id,
                            e.ordinal,
                            false,
                            &[
                                ("cash", "", *call),
                                ("received_vm", &a.agreement_id, -*call),
                            ],
                        )?;
                        received_vm += call;
                    } else {
                        book.post(
                            id,
                            e.ordinal,
                            false,
                            &[("posted_vm", &a.agreement_id, -*call), ("cash", "", *call)],
                        )?;
                        posted_vm -= call;
                    }
                }
                Body::VmReturn(a) => {
                    let posted = book.balance("posted_vm", &a.agreement_id);
                    let received = -book.balance("received_vm", &a.agreement_id);
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("cash", "", posted - received),
                            ("posted_vm", &a.agreement_id, -posted),
                            ("received_vm", &a.agreement_id, received),
                        ],
                    )?;
                    posted_vm -= posted;
                    received_vm -= received;
                }
                Body::ImReturn(a) => {
                    let posted = book.balance("posted_im", &a.agreement_id);
                    book.post(
                        id,
                        e.ordinal,
                        false,
                        &[
                            ("cash", "", posted),
                            ("posted_im", &a.agreement_id, -posted),
                        ],
                    )?;
                    posted_im -= posted;
                    if let Some(c) = custody.remove(&a.agreement_id) {
                        book.intents.push(Intent {
                            event_id: format!("{id}:custody"),
                            ordinal: e.ordinal,
                            memo: true,
                            postings: vec![
                                (
                                    gl("custody_asset").into(),
                                    instrument("custody_asset", &a.agreement_id),
                                    -c,
                                ),
                                (
                                    gl("custody_obligation").into(),
                                    instrument("custody_obligation", &a.agreement_id),
                                    c,
                                ),
                            ],
                        });
                        received_im -= c;
                    }
                }
                Body::Im(im, a) => {
                    if im.posted_requirement > 0. {
                        book.post(
                            id,
                            e.ordinal,
                            false,
                            &[
                                ("posted_im", &a.agreement_id, im.posted_requirement),
                                ("cash", "", -im.posted_requirement),
                            ],
                        )?;
                        posted_im += im.posted_requirement;
                    }
                    if im.received_requirement > 0. {
                        if a.segregation != "segregated" {
                            return Err(
                                "independent replay: received IM under a non-segregated agreement"
                                    .into(),
                            );
                        }
                        book.intents.push(Intent {
                            event_id: format!("{id}:custody"),
                            ordinal: e.ordinal,
                            memo: true,
                            postings: vec![
                                (
                                    gl("custody_asset").into(),
                                    instrument("custody_asset", &a.agreement_id),
                                    im.received_requirement,
                                ),
                                (
                                    gl("custody_obligation").into(),
                                    instrument("custody_obligation", &a.agreement_id),
                                    -im.received_requirement,
                                ),
                            ],
                        });
                        *custody.entry(a.agreement_id.clone()).or_default() +=
                            im.received_requirement;
                        received_im += im.received_requirement;
                    }
                }
            }
            settled.insert(e.id.clone());
            "settled"
        };
        statuses.push((e.id.clone(), status));
    }
    let stock_sum = |name: &str| -> f64 {
        book.financial
            .iter()
            .filter(|((g, _), _)| g == gl(name))
            .map(|(_, v)| v)
            .sum()
    };
    let open_claims = !near(
        stock_sum("unpaid_receivable") + stock_sum("unpaid_payable"),
        0.,
    ) || (carrying.is_empty()
        && (!near(posted_vm + received_vm, 0.) || !near(posted_im + received_im, 0.)));
    let closing: BTreeMap<(String, String), f64> = book
        .financial
        .iter()
        .filter(|((g, _), _)| g != "cash")
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    let _ = &account.equity;
    Ok(Expected {
        opening_cash_bits: account.cash.to_bits(),
        intents: book.intents,
        closing,
        closing_cash: book.cash,
        cash_movements: book.moved,
        events: statuses,
        facts,
        posted_vm,
        received_vm,
        posted_im,
        received_im,
        fees,
        realized,
        open_claims,
    })
}

pub fn plan_from(value: &Value) -> Result<Plan, String> {
    serde_json::from_value(value.clone())
        .map_err(|e| format!("plan does not parse for the independent replay: {e}"))
}
pub fn executions_from(value: &Value) -> Result<Vec<Execution>, String> {
    serde_json::from_value(value.clone())
        .map_err(|e| format!("execution blocks do not parse for the independent replay: {e}"))
}
