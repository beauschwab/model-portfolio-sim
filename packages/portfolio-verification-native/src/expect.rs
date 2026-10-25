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
    #[serde(default)]
    pub margin_method: Option<MethodIn>,
}
/// The research IM method as the raw plan states it (stage I).
#[derive(Debug, Clone, Deserialize)]
pub struct MethodIn {
    pub method: String,
    pub bucket_upper_days: Vec<i64>,
    pub risk_weights: Vec<f64>,
    pub correlations: Vec<Vec<f64>>,
    pub two_way: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct DynamicIn {
    pub agreement_id: String,
    pub ordinal: i64,
}
/// An independently recomputed research-method IM call.
#[derive(Debug, Clone)]
pub struct Call {
    pub event_id: String,
    pub agreement_id: String,
    pub ordinal: i64,
    pub live_lot_ids: Vec<String>,
    pub sensitivities: Vec<f64>,
    pub requirement: f64,
    pub posted_change: f64,
    pub received_change: f64,
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
    #[serde(default)]
    pub business_plan: Option<PlanIn>,
    #[serde(default)]
    pub dynamic_margin: Vec<DynamicIn>,
}
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct WindowIn {
    pub from_ordinal: i64,
    pub to_ordinal: i64,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RenewalIn {
    pub max_generations: u32,
    pub tenor_days: i64,
    pub windows: Vec<WindowIn>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyFacilityIn {
    pub facility_id: String,
    pub access: Vec<WindowIn>,
    #[serde(default)]
    pub renewal: Option<RenewalIn>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct FundingPolicyIn {
    pub minimum_usable_cash: f64,
    pub facilities: Vec<PolicyFacilityIn>,
    #[serde(default)]
    pub repay_surplus: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct PlanIn {
    pub mode: String,
    #[serde(default)]
    pub funding_policy: Option<FundingPolicyIn>,
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
/// The independently replayed liquidity state after one event (see the native `LiquidityStep`).
#[derive(Debug, Clone)]
pub struct Step {
    pub event_id: String,
    pub ordinal: i64,
    pub phase: u32,
    pub status: &'static str,
    pub depends_on: Vec<String>,
    /// (field, value) in the native field order.
    pub amounts: Vec<(&'static str, f64)>,
}
/// An independently replayed funding decision.
#[derive(Debug, Clone)]
pub struct Decision {
    pub decision_id: String,
    pub ordinal: i64,
    pub kind: &'static str,
    pub facility_id: String,
    pub generation: u32,
    pub trigger: String,
    pub amounts: Vec<(&'static str, f64)>,
}
/// An independently aggregated calendar month.
#[derive(Debug, Clone)]
pub struct Month {
    pub month: String,
    pub first_ordinal: i64,
    pub last_ordinal: i64,
    pub counts: Vec<(&'static str, u64)>,
    pub amounts: Vec<(&'static str, f64)>,
}
#[derive(Debug, Clone)]
pub struct Expected {
    pub opening_cash_bits: u64,
    pub intents: Vec<Intent>,
    pub closing: BTreeMap<(String, String), f64>,
    pub closing_cash: f64,
    pub cash_movements: f64,
    pub events: Vec<(String, &'static str)>,
    pub steps: Vec<Step>,
    pub mode: Option<String>,
    pub decisions: Vec<Decision>,
    pub monthly: Option<Vec<Month>>,
    pub calls: Vec<Call>,
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
    DynamicIm(&'a Agreement, &'a MethodIn),
}
struct Event<'a> {
    id: String,
    ordinal: i64,
    phase: u32,
    depends_on: Vec<String>,
    failed: bool,
    body: Body<'a>,
}

struct Fac {
    id: String,
    commitment: f64,
    expiry: i64,
    generation: u32,
}
struct Policy {
    floor: f64,
    order: Vec<String>,
    access: BTreeMap<String, Vec<WindowIn>>,
    renewal: BTreeMap<String, RenewalIn>,
    repay_surplus: bool,
    count: u32,
}
struct Book {
    financial: BTreeMap<(String, String), f64>,
    cash: f64,
    moved: f64,
    intents: Vec<Intent>,
    drawn: BTreeMap<String, f64>,
    facs: Vec<Fac>,
    segregated: Vec<String>,
    policy: Option<Policy>,
    /// Policy actions taken since the loop last collected them: (status entry, step).
    log: Vec<((String, &'static str), Step)>,
    decisions: Vec<Decision>,
    replacements: Vec<Fact>,
    /// (pledged, owned securities, IM in custody) before the current event.
    pre: (f64, f64, f64),
    phase: u32,
}
fn inside(windows: &[WindowIn], ordinal: i64) -> bool {
    windows
        .iter()
        .any(|w| w.from_ordinal <= ordinal && ordinal <= w.to_ordinal)
}
fn gen_id(id: &str, generation: u32) -> String {
    if generation <= 1 {
        id.to_string()
    } else {
        format!("{id}~g{generation}")
    }
}
impl Book {
    fn restricted(&self) -> f64 {
        self.segregated
            .iter()
            .map(|a| -self.balance("received_vm", a))
            .sum()
    }
    fn total(&self, name: &str) -> f64 {
        self.financial
            .iter()
            .filter(|((g, _), _)| g == gl(name))
            .map(|(_, v)| v)
            .sum()
    }
    fn fac(&self, id: &str) -> &Fac {
        self.facs.iter().find(|f| f.id == id).expect("facility")
    }
    fn undrawn(&self, ordinal: i64) -> f64 {
        self.facs
            .iter()
            .filter(|f| ordinal < f.expiry)
            .map(|f| f.commitment - self.drawn.get(&f.id).copied().unwrap_or(0.))
            .sum()
    }
    /// The liquidity amounts of a step, in the native field order. `inventory` supplies the
    /// pledged quantity, owned securities and custody IM.
    fn amounts(
        &self,
        ordinal: i64,
        cash_before: f64,
        inventory: (f64, f64, f64),
    ) -> Vec<(&'static str, f64)> {
        let restricted = self.restricted();
        let drawn: f64 = self.drawn.values().sum();
        vec![
            ("cash_movement", self.cash - cash_before),
            ("cash_after", self.cash),
            ("restricted_cash_after", restricted),
            ("usable_cash_after", self.cash - restricted),
            ("facility_drawn_after", drawn),
            ("facility_undrawn_after", self.undrawn(ordinal)),
            ("pledged_quantity_after", inventory.0),
            ("security_quantity_after", inventory.1),
            ("posted_vm_after", self.total("posted_vm")),
            ("received_vm_after", -self.total("received_vm")),
            ("posted_im_after", self.total("posted_im")),
            ("received_im_custody_after", inventory.2),
            ("pending_receivable_after", self.total("unpaid_receivable")),
            ("pending_payable_after", -self.total("unpaid_payable")),
            ("derivative_assets_after", self.total("derivative_asset")),
            (
                "derivative_liabilities_after",
                -self.total("derivative_liability"),
            ),
        ]
    }
    /// The independent funding rule: before a payment settles, draw from facilities that are
    /// accessible today and unexpired, in order, what keeps usable cash at the floor.
    fn fund(&mut self, trigger: &str, ordinal: i64, delta: f64) -> Result<(), String> {
        let (floor, order) = match &self.policy {
            Some(p) => (p.floor, p.order.clone()),
            None => return Ok(()),
        };
        let need = floor - (self.cash - self.restricted() + delta);
        if need <= 1e-9 * floor.abs().max(1.) {
            return Ok(());
        }
        let mut left = need;
        for id in &order {
            if left <= 1e-12 {
                break;
            }
            let open = self
                .policy
                .as_ref()
                .and_then(|p| p.access.get(id))
                .is_some_and(|w| inside(w, ordinal));
            let headroom = self.fac(id).commitment - self.drawn.get(id).copied().unwrap_or(0.);
            if !open || ordinal >= self.fac(id).expiry {
                continue;
            }
            let x = left.min(headroom);
            if x <= 1e-12 {
                continue;
            }
            self.act("draw", id, x, trigger, ordinal, need)?;
            left -= x;
        }
        Ok(())
    }
    fn act(
        &mut self,
        kind: &'static str,
        id: &str,
        amount: f64,
        trigger: &str,
        ordinal: i64,
        need: f64,
    ) -> Result<(), String> {
        let n = {
            let p = self.policy.as_mut().expect("policy");
            p.count += 1;
            p.count
        };
        let decision = format!("policy_{kind}:{id}:{ordinal}:{n}");
        let (cash_before, usable_before) = (self.cash, self.cash - self.restricted());
        let generation = self.fac(id).generation;
        let current = gen_id(id, generation);
        let phase = self.phase;
        match kind {
            "draw" => {
                *self.drawn.entry(id.to_string()).or_default() += amount;
                self.raw(
                    &decision,
                    ordinal,
                    &[
                        ("cash", "", amount),
                        ("funding_principal", &current, -amount),
                    ],
                )?;
            }
            "surplus_repayment" => {
                *self.drawn.entry(id.to_string()).or_default() -= amount;
                self.raw(
                    &decision,
                    ordinal,
                    &[
                        ("funding_principal", &current, amount),
                        ("cash", "", -amount),
                    ],
                )?;
            }
            "maturity_repayment" => {
                *self.drawn.entry(id.to_string()).or_default() -= amount;
                self.post(
                    &decision,
                    ordinal,
                    false,
                    &[
                        ("funding_principal", &current, amount),
                        ("cash", "", -amount),
                    ],
                )?;
            }
            "renewal" => {
                let tenor = self
                    .policy
                    .as_ref()
                    .and_then(|p| p.renewal.get(id))
                    .map_or(0, |r| r.tenor_days);
                let next = gen_id(id, generation + 1);
                if let Some(f) = self.facs.iter_mut().find(|f| f.id == id) {
                    f.generation += 1;
                    f.expiry += tenor;
                }
                self.raw(
                    &decision,
                    ordinal,
                    &[
                        ("funding_principal", &current, amount),
                        ("funding_principal", &next, -amount),
                    ],
                )?;
                self.replacements.push(Fact {
                    kind: "funding_replacement",
                    lot_id: next,
                    quantity: amount,
                    retained_quantity: amount,
                    consideration: 0.,
                    removed_carrying: 0.,
                    realized: 0.,
                    economic_allocation_ids: Vec::new(),
                    formal_relationship_ids: Vec::new(),
                });
            }
            _ => return Err("independent replay: unknown policy action".into()),
        }
        let usable_after = self.cash - self.restricted();
        self.decisions.push(Decision {
            decision_id: decision.clone(),
            ordinal,
            kind,
            facility_id: id.to_string(),
            generation: self.fac(id).generation,
            trigger: trigger.to_string(),
            amounts: vec![
                ("amount", amount),
                ("usable_cash_before", usable_before),
                ("usable_cash_after", usable_after),
                ("need", need),
            ],
        });
        let amounts = self.amounts(ordinal, cash_before, self.pre);
        self.log.push((
            (decision.clone(), "settled"),
            Step {
                event_id: decision,
                ordinal,
                phase,
                status: "settled",
                depends_on: Vec::new(),
                amounts,
            },
        ));
        Ok(())
    }
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
        let delta: f64 = lines.iter().filter(|l| l.0 == "cash").map(|l| l.2).sum();
        if !memo && delta < 0. && self.policy.is_some() {
            self.fund(id, ordinal, delta)?;
        }
        self.raw_memo(id, ordinal, memo, lines)
    }
    fn raw(&mut self, id: &str, ordinal: i64, lines: &[(&str, &str, f64)]) -> Result<(), String> {
        self.raw_memo(id, ordinal, false, lines)
    }
    fn raw_memo(
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
    sensitivities: &HashMap<(String, i64), Vec<f64>>,
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
    // Research-method IM recomputations on the live legal portfolio.
    let mut dynamic: BTreeSet<(&str, i64)> = BTreeSet::new();
    for d in &acq.dynamic_margin {
        let a = agreements
            .get(d.agreement_id.as_str())
            .copied()
            .ok_or("independent replay: unknown agreement")?;
        let m = match (a.initial_margin.as_str(), &a.margin_method) {
            ("research_sensitivity", Some(m)) if m.method == "research-sensitivity-im-1" => m,
            _ => {
                return Err(
                    "independent replay: dynamic IM needs the research sensitivity method".into(),
                )
            }
        };
        if !dynamic.insert((&d.agreement_id, d.ordinal)) {
            return Err("independent replay: IM recomputed twice on one date".into());
        }
        push(
            &mut ids,
            &mut events,
            Event {
                id: format!("dynamic_im:{}:{}", d.agreement_id, d.ordinal),
                ordinal: d.ordinal,
                phase: 9,
                depends_on: vec![],
                failed: false,
                body: Body::DynamicIm(a, m),
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
        if a.initial_margin == "research_sensitivity" {
            if !dynamic.contains(&(*agreement_id, *ordinal)) {
                return Err(format!("independent replay: agreement `{agreement_id}` changes population on {ordinal} without an IM recomputation"));
            }
            continue;
        }
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
        drawn: plan
            .facilities
            .iter()
            .map(|f| (f.facility_id.clone(), f.drawn))
            .collect(),
        facs: plan
            .facilities
            .iter()
            .map(|f| Fac {
                id: f.facility_id.clone(),
                commitment: f.commitment,
                expiry: f.expiry_ordinal,
                generation: 1,
            })
            .collect(),
        segregated: plan
            .margin_agreements
            .iter()
            .filter(|a| a.segregation == "segregated")
            .map(|a| a.agreement_id.clone())
            .collect(),
        policy: None,
        log: Vec::new(),
        decisions: Vec::new(),
        replacements: Vec::new(),
        pre: (0., 0., 0.),
        phase: 0,
    };
    let mode = acq.business_plan.as_ref().map(|b| b.mode.clone());
    if let Some(bp) = &acq.business_plan {
        match (bp.mode.as_str(), &bp.funding_policy) {
            ("runoff" | "schedule", None) => {}
            ("policy", Some(p)) => {
                book.policy = Some(Policy {
                    floor: p.minimum_usable_cash,
                    order: p.facilities.iter().map(|f| f.facility_id.clone()).collect(),
                    access: p
                        .facilities
                        .iter()
                        .map(|f| (f.facility_id.clone(), f.access.clone()))
                        .collect(),
                    renewal: p
                        .facilities
                        .iter()
                        .filter_map(|f| f.renewal.clone().map(|r| (f.facility_id.clone(), r)))
                        .collect(),
                    repay_surplus: p.repay_surplus,
                    count: 0,
                });
            }
            _ => return Err("independent replay: unsupported business plan".into()),
        }
    }
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
    let mut settled: BTreeSet<String> = BTreeSet::new();
    let mut statuses = Vec::new();
    let mut steps = Vec::new();
    let mut facts = Vec::new();
    let mut calls: Vec<Call> = Vec::new();
    let (mut fees, mut realized) = (0., 0.);
    if let Some(e) = events.iter().find(|e| e.ordinal < asof || e.ordinal > end) {
        return Err(format!(
            "independent replay: `{}` is outside the horizon",
            e.id
        ));
    }
    let security_ids: Vec<String> = plan
        .lots
        .iter()
        .filter(|l| l.kind == "security" && l.account_id == account_id)
        .map(|l| l.lot_id.clone())
        .collect();
    let inventory = |owned: &BTreeMap<String, f64>,
                     pledged: &BTreeMap<String, f64>,
                     custody: &BTreeMap<String, f64>| {
        (
            pledged.values().sum::<f64>(),
            security_ids
                .iter()
                .map(|l| owned.get(l).copied().unwrap_or(0.))
                .sum::<f64>(),
            custody.values().sum::<f64>(),
        )
    };
    let opening_amounts = book.amounts(asof, book.cash, inventory(&owned, &pledged, &custody));
    for ordinal in asof..=end {
        // Start of day: renew or repay facility generations expiring today.
        book.pre = inventory(&owned, &pledged, &custody);
        book.phase = 0;
        if let Some(order) = book.policy.as_ref().map(|p| p.order.clone()) {
            for id in &order {
                let (expiry, generation) = (book.fac(id).expiry, book.fac(id).generation);
                if expiry != ordinal {
                    continue;
                }
                let drawn = book.drawn.get(id).copied().unwrap_or(0.);
                let renew = book
                    .policy
                    .as_ref()
                    .and_then(|p| p.renewal.get(id))
                    .is_some_and(|r| generation < r.max_generations && inside(&r.windows, ordinal));
                if renew {
                    book.act("renewal", id, drawn, "start_of_day", ordinal, 0.)?;
                } else if drawn > 1e-12 {
                    book.act("maturity_repayment", id, drawn, "start_of_day", ordinal, 0.)?;
                }
            }
        }
        for (status, step) in book.log.drain(..) {
            statuses.push(status);
            steps.push(step);
        }
        for e in events.iter().filter(|e| e.ordinal == ordinal) {
            let cash_before = book.cash;
            book.pre = inventory(&owned, &pledged, &custody);
            book.phase = e.phase;
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
                        let used = book.drawn.get(&d.facility_id).copied().unwrap_or(0.);
                        if d.amount > f.commitment - used + 1e-9 || d.ordinal > f.expiry_ordinal {
                            return Err(format!("independent replay: `{id}` exceeds the facility"));
                        }
                        *book.drawn.entry(d.facility_id.clone()).or_default() += d.amount;
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
                        let used = book.drawn.get(&r.facility_id).copied().unwrap_or(0.);
                        if r.amount > used + 1e-9 {
                            return Err(format!(
                                "independent replay: `{id}` repays more than drawn"
                            ));
                        }
                        *book.drawn.entry(r.facility_id.clone()).or_default() -= r.amount;
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
                    Body::DynamicIm(a, m) => {
                        let n = m.bucket_upper_days.len();
                        if m.risk_weights.len() != n || m.correlations.len() != n {
                            return Err("independent replay: malformed IM method".into());
                        }
                        let live: Vec<String> = a
                            .lot_ids
                            .iter()
                            .filter(|l| carrying.contains_key(l.as_str()))
                            .cloned()
                            .collect();
                        let mut total = vec![0.; n];
                        for l in &live {
                            let s = sensitivities
                                .get(&(l.clone(), e.ordinal))
                                .filter(|s| s.len() == n)
                                .ok_or_else(|| format!("independent replay: live lot `{l}` has no sensitivity on {}", e.ordinal))?;
                            for (t, v) in total.iter_mut().zip(s) {
                                *t += v;
                            }
                        }
                        // Weighted sensitivities, then the correlated quadratic form.
                        let ws: Vec<f64> = total
                            .iter()
                            .zip(&m.risk_weights)
                            .map(|(s, w)| s * w)
                            .collect();
                        let mut q = 0.;
                        for (b, row) in m.correlations.iter().enumerate() {
                            for (c, rho) in row.iter().enumerate() {
                                q += ws[b] * rho * ws[c];
                            }
                        }
                        let requirement = if q > 0. { q.sqrt() } else { 0. };
                        let posted_change =
                            requirement - book.balance("posted_im", &a.agreement_id);
                        if posted_change != 0. {
                            book.post(
                                id,
                                e.ordinal,
                                false,
                                &[
                                    ("posted_im", &a.agreement_id, posted_change),
                                    ("cash", "", -posted_change),
                                ],
                            )?;
                            posted_im += posted_change;
                        }
                        let target = if m.two_way { requirement } else { 0. };
                        if target > 0. && a.segregation != "segregated" {
                            return Err(
                                "independent replay: received IM under a non-segregated agreement"
                                    .into(),
                            );
                        }
                        let held = custody.get(&a.agreement_id).copied().unwrap_or(0.);
                        let received_change = target - held;
                        if received_change != 0. {
                            book.intents.push(Intent {
                                event_id: format!("{id}:custody"),
                                ordinal: e.ordinal,
                                memo: true,
                                postings: vec![
                                    (
                                        gl("custody_asset").into(),
                                        instrument("custody_asset", &a.agreement_id),
                                        received_change,
                                    ),
                                    (
                                        gl("custody_obligation").into(),
                                        instrument("custody_obligation", &a.agreement_id),
                                        -received_change,
                                    ),
                                ],
                            });
                            if target > 0. {
                                custody.insert(a.agreement_id.clone(), target);
                            } else {
                                custody.remove(&a.agreement_id);
                            }
                            received_im += received_change;
                        }
                        calls.push(Call {
                            event_id: id.to_string(),
                            agreement_id: a.agreement_id.clone(),
                            ordinal: e.ordinal,
                            live_lot_ids: live,
                            sensitivities: total,
                            requirement,
                            posted_change,
                            received_change,
                        });
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
            let drawn_for_event: f64 = book
                .log
                .iter()
                .filter(|(_, s)| s.event_id.starts_with("policy_draw:"))
                .map(|(_, s)| s.amounts[0].1)
                .sum();
            for (entry, step) in book.log.drain(..) {
                statuses.push(entry);
                steps.push(step);
            }
            let cash_before = cash_before + drawn_for_event;
            statuses.push((e.id.clone(), status));
            // Restricted cash: received VM held under a segregated agreement (independently from
            // the received VM liability balances, not from a running counter).
            let restricted: f64 = plan
                .margin_agreements
                .iter()
                .filter(|a| a.segregation == "segregated")
                .map(|a| -book.balance("received_vm", &a.agreement_id))
                .sum();
            let total = |name: &str| -> f64 {
                book.financial
                    .iter()
                    .filter(|((g, _), _)| g == gl(name))
                    .map(|(_, v)| v)
                    .sum()
            };
            let drawn_total: f64 = book.drawn.values().sum();
            let securities: f64 = plan
                .lots
                .iter()
                .filter(|l| l.kind == "security" && l.account_id == account_id)
                .map(|l| owned.get(&l.lot_id).copied().unwrap_or(0.))
                .sum();
            steps.push(Step {
                event_id: e.id.clone(),
                ordinal: e.ordinal,
                phase: e.phase,
                status,
                depends_on: e.depends_on.clone(),
                amounts: vec![
                    ("cash_movement", book.cash - cash_before),
                    ("cash_after", book.cash),
                    ("restricted_cash_after", restricted),
                    ("usable_cash_after", book.cash - restricted),
                    ("facility_drawn_after", drawn_total),
                    ("facility_undrawn_after", book.undrawn(e.ordinal)),
                    ("pledged_quantity_after", pledged.values().sum()),
                    ("security_quantity_after", securities),
                    ("posted_vm_after", total("posted_vm")),
                    ("received_vm_after", -total("received_vm")),
                    ("posted_im_after", total("posted_im")),
                    ("received_im_custody_after", custody.values().sum()),
                    ("pending_receivable_after", total("unpaid_receivable")),
                    ("pending_payable_after", -total("unpaid_payable")),
                    ("derivative_assets_after", total("derivative_asset")),
                    (
                        "derivative_liabilities_after",
                        -total("derivative_liability"),
                    ),
                ],
            });
        }
        // End of day: repay drawn policy facilities, last first, from usable cash above the floor.
        book.pre = inventory(&owned, &pledged, &custody);
        book.phase = 10;
        if let Some((floor, order, true)) = book
            .policy
            .as_ref()
            .map(|p| (p.floor, p.order.clone(), p.repay_surplus))
        {
            for id in order.iter().rev() {
                let surplus = book.cash - book.restricted() - floor;
                let x = surplus.min(book.drawn.get(id).copied().unwrap_or(0.));
                if x > 1e-9 * floor.abs().max(1.) {
                    book.act("surplus_repayment", id, x, "end_of_day", ordinal, 0.)?;
                }
            }
        }
        for (status, step) in book.log.drain(..) {
            statuses.push(status);
            steps.push(step);
        }
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
    facts.append(&mut book.replacements);
    let monthly = match &mode {
        None => None,
        Some(_) => Some(months(
            &book.intents,
            &steps,
            account.cash,
            &opening_amounts,
            asof,
            end,
        )?),
    };
    Ok(Expected {
        opening_cash_bits: account.cash.to_bits(),
        intents: book.intents,
        closing,
        closing_cash: book.cash,
        cash_movements: book.moved,
        events: statuses,
        steps,
        mode,
        decisions: std::mem::take(&mut book.decisions),
        monthly,
        calls,
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

/// Calendar year and month of a proleptic Gregorian day ordinal (0001-01-01 is day 1), by the
/// days-from-civil inversion on the Unix epoch (an independent calendar implementation).
pub fn year_month(ordinal: i64) -> (i64, u32) {
    let z = ordinal - 719_163 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month)
}

fn months(
    intents: &[Intent],
    steps: &[Step],
    opening_cash: f64,
    opening: &[(&'static str, f64)],
    asof: i64,
    end: i64,
) -> Result<Vec<Month>, String> {
    let get =
        |a: &[(&'static str, f64)], k: &str| a.iter().find(|x| x.0 == k).map(|x| x.1).unwrap_or(0.);
    let mut usable = get(opening, "usable_cash_after");
    let mut drawn = get(opening, "facility_drawn_after");
    let mut posted = get(opening, "posted_vm_after") + get(opening, "posted_im_after");
    let mut pledged = get(opening, "pledged_quantity_after");
    let mut securities = get(opening, "security_quantity_after");
    let mut cash = opening_cash;
    let mut out = Vec::new();
    let mut first = asof;
    while first <= end {
        let key = year_month(first);
        let mut last = first;
        while last < end && year_month(last + 1) == key {
            last += 1;
        }
        let within = |o: i64| first <= o && o <= last;
        let (mut cash_in, mut cash_out, mut net) = (0., 0., 0.);
        let (mut interest, mut fees, mut realized, mut posted_m, mut returned) =
            (0., 0., 0., 0., 0.);
        for i in intents.iter().filter(|i| within(i.ordinal) && !i.memo) {
            for (g, _, d) in &i.postings {
                match g.as_str() {
                    "cash" => {
                        if *d > 0. {
                            cash_in += d;
                        } else {
                            cash_out -= d;
                        }
                        net += d;
                    }
                    "pnl:lifecycle_interest" => interest += d,
                    "pnl:lifecycle_fees" => fees += d,
                    "pnl:lifecycle_realized" => realized -= d,
                    "derivative_posted_vm_asset" | "derivative_posted_im_asset" => {
                        if *d > 0. {
                            posted_m += d;
                        } else {
                            returned -= d;
                        }
                    }
                    _ => {}
                }
            }
        }
        let opening_month = cash;
        let mut minimum = usable;
        let (mut events, mut failed, mut blocked, mut policy) = (0u64, 0u64, 0u64, 0u64);
        for st in steps.iter().filter(|st| within(st.ordinal)) {
            events += 1;
            failed += u64::from(st.status == "failed");
            blocked += u64::from(st.status == "blocked");
            policy += u64::from(st.event_id.starts_with("policy_"));
            usable = get(&st.amounts, "usable_cash_after");
            minimum = minimum.min(usable);
            drawn = get(&st.amounts, "facility_drawn_after");
            posted = get(&st.amounts, "posted_vm_after") + get(&st.amounts, "posted_im_after");
            pledged = get(&st.amounts, "pledged_quantity_after");
            securities = get(&st.amounts, "security_quantity_after");
        }
        cash += net;
        out.push(Month {
            month: format!("{:04}-{:02}", key.0, key.1),
            first_ordinal: first,
            last_ordinal: last,
            counts: vec![
                ("events", events),
                ("failed_events", failed),
                ("blocked_events", blocked),
                ("policy_decisions", policy),
            ],
            amounts: vec![
                ("opening_cash", opening_month),
                ("cash_in", cash_in),
                ("cash_out", cash_out),
                ("net_cash", net),
                ("closing_cash", cash),
                ("minimum_usable_cash", minimum),
                ("closing_usable_cash", usable),
                ("closing_facility_drawn", drawn),
                ("interest_expense", interest),
                ("fees", fees),
                ("realized", realized),
                ("margin_posted", posted_m),
                ("margin_returned", returned),
                ("closing_posted_margin", posted),
                ("closing_pledged_quantity", pledged),
                ("closing_security_quantity", securities),
            ],
        });
        first = last + 1;
    }
    Ok(out)
}

pub fn plan_from(value: &Value) -> Result<Plan, String> {
    serde_json::from_value(value.clone())
        .map_err(|e| format!("plan does not parse for the independent replay: {e}"))
}
pub fn executions_from(value: &Value) -> Result<Vec<Execution>, String> {
    serde_json::from_value(value.clone())
        .map_err(|e| format!("execution blocks do not parse for the independent replay: {e}"))
}
