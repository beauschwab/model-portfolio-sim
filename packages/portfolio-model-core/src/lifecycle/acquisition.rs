//! Stages B and C (issues #44 and #45): native acquisition and unwind replay over an admitted
//! plan. Security purchases and sales, financing advances and voluntary repayments, hedge entries
//! and terminations, bridge facility draws and repayments, cash variation margin and supplied
//! exact initial margin are replayed day by day in explicit phases. Settlement obligations are
//! gated by their dependencies; a failed leg blocks its dependents and nothing settled is undone.
//! The engine produces one ordered journal, daily closings, a conservation receipt and the
//! separate accounting facts the ASC 815 owner consumes. It posts no formal basis or OCI.
//!
//! Derivative values are native-derived: the Decision coordinator prices each raw contract at
//! the entry or termination date and passes the value in `ResolvedHedge`. Callers cannot supply
//! intents or values.
use super::admission::Admitted;
use super::journal::{Closing, Intent, Inventory, Kind, Line};
use super::policy::{
    self, generation_instrument, BusinessPlan, MonthlyReport, PlanMode, PolicyDecision, Renewal,
    Window,
};
use super::types::*;
use crate::validation::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const MAX_EXECUTION_DAYS: i32 = 366;
pub const MAX_INSTRUCTIONS: usize = 4096;
pub const MARGIN_REFRESH_REQUIRED: &str = "MARGIN_REFRESH_REQUIRED";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMechanism {
    /// Delivery and payment are one supported exchange of value.
    DeliveryVersusPayment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionState {
    #[default]
    Pending,
    /// The leg does not settle; its gross claims and dependents remain outstanding.
    Failed,
}
/// A new security lot bought at a reviewed quote. The lot is in the plan with its acquisition
/// date equal to the settlement date; `original_cost` there must equal clean cost plus fees.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityPurchase {
    pub lot_id: String,
    pub quote: Revision,
    pub trade_ordinal: i32,
    pub settlement_ordinal: i32,
    pub clean_price_per_100: f64,
    pub accrued_interest: f64,
    pub fees: f64,
    pub mechanism: SettlementMechanism,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
/// Full or partial sale of an owned lot. The surviving part keeps the lot's original anchors;
/// realized gain or loss is consideration less the removed carrying.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecuritySale {
    pub lot_id: String,
    pub quantity: f64,
    pub quote: Revision,
    pub trade_ordinal: i32,
    pub settlement_ordinal: i32,
    pub clean_price_per_100: f64,
    pub accrued_interest: f64,
    pub fees: f64,
    pub mechanism: SettlementMechanism,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
/// A new funding lot advanced on a date, secured by a plan pledge of a purchased lot.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FinancingAdvance {
    pub lot_id: String,
    pub advance_ordinal: i32,
    pub principal: f64,
    pub pledge_id: String,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PledgeRelease {
    pub pledge_id: String,
    pub quantity: f64,
}
/// Voluntary full or partial repayment of a funding lot with its actual accrued interest, break
/// cost and consent. Releases are separate events that depend on the repayment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FundingRepayment {
    pub lot_id: String,
    pub repayment_ordinal: i32,
    pub principal: f64,
    pub accrued_interest: f64,
    pub break_cost: f64,
    pub consent: Revision,
    #[serde(default)]
    pub pledge_releases: Vec<PledgeRelease>,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VariationMarginTerms {
    pub receive_threshold: f64,
    pub post_threshold: f64,
    pub minimum_transfer: f64,
}
/// A new derivative lot entered under a margin agreement. Any nonzero entry value must be paid
/// or received as explicit upfront consideration: no day-one gain or loss is created. An offset
/// names the lot it offsets; the original lot, its rights and margins survive untouched.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeEntry {
    pub lot_id: String,
    pub agreement_id: String,
    pub entry_ordinal: i32,
    pub upfront_consideration: f64,
    pub variation_margin: VariationMarginTerms,
    #[serde(default)]
    pub offsets_lot_id: Option<String>,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConsiderationSource {
    /// The native clean value of the raw contract at the effective date.
    ModelCleanValue,
    /// A supplied agreed execution quote; its difference from carrying is realized.
    ExecutionQuote { amount: f64, quote: Revision },
}
/// Voluntary termination of a derivative lot. Rights stop at the effective date; consideration
/// and the contractual cash due that day are an unpaid claim until the settlement date.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivativeTermination {
    pub lot_id: String,
    pub agreement_id: String,
    pub effective_ordinal: i32,
    pub settlement_ordinal: i32,
    pub consideration: ConsiderationSource,
    pub fees: f64,
    pub consent: Revision,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityDraw {
    pub facility_id: String,
    pub ordinal: i32,
    pub amount: f64,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityRepayment {
    pub facility_id: String,
    pub ordinal: i32,
    pub amount: f64,
    #[serde(default)]
    pub instruction_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub state: InstructionState,
}
/// Reviewed IM requirement bound to the exact legal portfolio, scenario and date.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedInitialMargin {
    pub agreement_id: String,
    pub portfolio_lot_ids: Vec<String>,
    pub scenario_id: String,
    pub ordinal: i32,
    pub posted_requirement: f64,
    pub received_requirement: f64,
    pub review: Revision,
}
/// One scenario's execution block: acquisitions (stage B) and unwinds, exits and bridge funding
/// (stage C). The name is retained from stage B.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRequest {
    pub scenario_id: String,
    pub horizon_end_ordinal: i32,
    #[serde(default)]
    pub purchases: Vec<SecurityPurchase>,
    #[serde(default)]
    pub sales: Vec<SecuritySale>,
    #[serde(default)]
    pub financings: Vec<FinancingAdvance>,
    #[serde(default)]
    pub repayments: Vec<FundingRepayment>,
    #[serde(default)]
    pub hedge_entries: Vec<HedgeEntry>,
    #[serde(default)]
    pub terminations: Vec<DerivativeTermination>,
    #[serde(default)]
    pub facility_draws: Vec<FacilityDraw>,
    #[serde(default)]
    pub facility_repayments: Vec<FacilityRepayment>,
    #[serde(default)]
    pub initial_margin: Vec<SuppliedInitialMargin>,
    /// Stage G business-plan mode. Absent means the supplied schedule, as in stages B and C.
    #[serde(default)]
    pub business_plan: Option<BusinessPlan>,
    /// Stage I: research-method IM recomputations on the live portfolio of an agreement.
    #[serde(default)]
    pub dynamic_margin: Vec<DynamicMargin>,
}

/// One IM recomputation date of a research-sensitivity agreement.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicMargin {
    pub agreement_id: String,
    pub ordinal: i32,
}

/// Native bucket sensitivities of one derivative lot on one date (never caller supplied): the
/// clean-value change for a one basis point zero-rate bump of each bucket's discount points.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sensitivity {
    pub lot_id: String,
    pub ordinal: i32,
    pub buckets: Vec<f64>,
}

/// One research-method IM recomputation, with the population and inputs it used.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarginCall {
    pub event_id: String,
    pub agreement_id: String,
    pub ordinal: i32,
    pub live_lot_ids: Vec<String>,
    /// Signed bucket sensitivities summed over the live legal portfolio.
    pub sensitivities: Vec<f64>,
    pub requirement: f64,
    pub posted_change: f64,
    pub received_change: f64,
}

/// Native-derived value of a derivative lot at a date (never caller supplied): the clean value
/// and the contractual cash due on that date, signed from the entity's side.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHedge {
    pub lot_id: String,
    pub ordinal: i32,
    pub clean_value: f64,
    pub due_cash: f64,
    pub pricing_source: Revision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Settled,
    Failed,
    Blocked,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventReceipt {
    pub event_id: String,
    pub ordinal: i32,
    pub phase: u32,
    pub status: EventStatus,
}
/// The account's liquidity state after one event, in execution order. Usable cash excludes
/// received VM held under segregation; undrawn facility capacity is capacity, never cash; posted
/// margin and pledged securities are restricted collateral until their settled return or
/// release; unpaid termination consideration is a pending receipt or payment.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiquidityStep {
    pub event_id: String,
    pub ordinal: i32,
    pub phase: u32,
    pub status: EventStatus,
    pub depends_on: Vec<String>,
    pub cash_movement: f64,
    pub cash_after: f64,
    pub restricted_cash_after: f64,
    pub usable_cash_after: f64,
    pub facility_drawn_after: f64,
    pub facility_undrawn_after: f64,
    pub pledged_quantity_after: f64,
    pub security_quantity_after: f64,
    pub posted_vm_after: f64,
    pub received_vm_after: f64,
    pub posted_im_after: f64,
    pub received_im_custody_after: f64,
    pub pending_receivable_after: f64,
    pub pending_payable_after: f64,
    pub derivative_assets_after: f64,
    pub derivative_liabilities_after: f64,
}
/// Sale and termination facts for the ASC 815 owner. Basis, OCI and reclassification remain
/// that owner's decisions under its reviewed rules; nothing here is an accounting entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountingFact {
    pub kind: &'static str,
    pub lot_id: String,
    pub quantity: f64,
    pub retained_quantity: f64,
    pub effective_ordinal: i32,
    pub settlement_ordinal: i32,
    pub consideration: f64,
    pub removed_carrying: f64,
    pub realized: f64,
    pub economic_allocation_ids: Vec<String>,
    pub formal_relationship_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    pub opening_cash: f64,
    pub closing_cash: f64,
    pub settled_cash_movements: f64,
    pub gross_derivative_assets: f64,
    pub gross_derivative_liabilities: f64,
    pub posted_vm: f64,
    pub received_vm: f64,
    pub posted_im: f64,
    pub received_im_custody: f64,
    pub fees_expensed: f64,
    pub realized: f64,
    pub cash_bridge_reconciled: bool,
    pub inventory_conserved: bool,
    pub collateral_conserved: bool,
    /// Collateral or termination claims still outstanding at the horizon: a reporting cutoff
    /// does not extinguish them.
    pub open_claims: bool,
    pub events: Vec<EventReceipt>,
    /// One liquidity step per event, in the same order as `events`.
    pub steps: Vec<LiquidityStep>,
    pub accounting_facts: Vec<AccountingFact>,
    pub formal_hedge_accounting: bool,
    /// The business-plan mode, when one was selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business_plan_mode: Option<PlanMode>,
    /// Funding-policy decisions in execution order (policy mode only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub policy_decisions: Vec<PolicyDecision>,
    /// Calendar-month reports reconciled to the dated events (business-plan modes only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly: Option<Vec<MonthlyReport>>,
    /// Research-method IM recomputations (stage I).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub margin_calls: Vec<MarginCall>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AcquisitionJournal {
    pub account_id: String,
    pub scenario_id: String,
    pub as_of_ordinal: i32,
    pub tail_ordinal: i32,
    pub opening_cash: f64,
    pub opening_equity: f64,
    /// Opening gross stocks posted by this owner: (assets, liabilities).
    pub opening_stocks: (f64, f64),
    pub opening: Vec<Line>,
    pub intents: Vec<Intent>,
    pub daily: Vec<Closing>,
    pub receipt: Receipt,
}

fn money(x: f64, what: &str) -> Result<f64, Refusal> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err(Refusal::new(
            MALFORMED_INPUT,
            format!("{what} must be finite and within the monetary domain"),
        ))
    }
}
/// A reported amount: an exactly zero stock is reported as +0.0, never as a negated -0.0.
fn unsigned(x: f64) -> f64 {
    x + 0.
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn unsupported(detail: String) -> Refusal {
    Refusal::new(UNSUPPORTED_POLICY, detail)
}
fn mapping(detail: String) -> Refusal {
    Refusal::new(LIFECYCLE_MAPPING_REQUIRED, detail)
}

struct Book {
    financial: BTreeMap<(Kind, String), f64>,
    custody: BTreeMap<(Kind, String), f64>,
    cash: f64,
    restricted: f64,
    settled: f64,
    owned: BTreeMap<String, f64>,
    pledged: BTreeMap<String, f64>,
    pledge_left: BTreeMap<String, f64>,
    intents: Vec<Intent>,
    facilities: Vec<FacilityState>,
    security_lots: Vec<String>,
    segregated: Vec<String>,
    policy: Option<PolicyRun>,
    policy_log: Vec<(EventReceipt, LiquidityStep)>,
    decisions: Vec<PolicyDecision>,
    replacements: Vec<AccountingFact>,
    /// Pledged quantity, owned securities and IM in custody before the current event: the
    /// inventory a policy step reports while the event is still settling.
    pre: (f64, f64, f64),
}

struct FacilityState {
    id: String,
    commitment: f64,
    drawn: f64,
    expiry: i32,
    generation: u32,
}

struct PolicyRun {
    floor: f64,
    order: Vec<String>,
    access: BTreeMap<String, Vec<Window>>,
    renewal: BTreeMap<String, Renewal>,
    repay_surplus: bool,
    count: u32,
}

impl Book {
    fn balance(&self, kind: Kind, instrument: &str) -> f64 {
        let book = if kind.memo() {
            &self.custody
        } else {
            &self.financial
        };
        book.get(&(kind, instrument.to_string()))
            .copied()
            .unwrap_or(0.)
    }
    /// Posts one event's lines. Under a funding policy, a payment that would leave usable cash
    /// below the floor is funded first, from the state at this moment only.
    fn post(
        &mut self,
        event_id: &str,
        ordinal: i32,
        phase: u32,
        sequence: u32,
        lines: Vec<(Kind, &str, f64)>,
    ) -> Result<(), Refusal> {
        let delta: f64 = lines
            .iter()
            .filter(|l| l.0 == Kind::Cash)
            .map(|l| l.2)
            .sum();
        if delta < 0. && self.policy.is_some() {
            self.fund(event_id, ordinal, phase, sequence, delta)?;
        }
        self.apply(event_id, ordinal, phase, sequence, lines)
    }
    /// Received VM held under segregation, from the balances (not a running counter).
    fn inventory_now(&self) -> (f64, f64, f64) {
        (
            self.pledged.values().sum(),
            self.security_lots
                .iter()
                .map(|l| self.owned.get(l).copied().unwrap_or(0.))
                .sum(),
            self.stock(Kind::CustodyAsset),
        )
    }
    fn restricted_now(&self) -> f64 {
        self.segregated
            .iter()
            .map(|a| -self.balance(Kind::ReceivedVm, a))
            .sum()
    }
    fn facility(&self, id: &str) -> &FacilityState {
        self.facilities
            .iter()
            .find(|f| f.id == id)
            .expect("facility")
    }
    fn facility_mut(&mut self, id: &str) -> &mut FacilityState {
        self.facilities
            .iter_mut()
            .find(|f| f.id == id)
            .expect("facility")
    }
    fn step(
        &self,
        event_id: &str,
        ordinal: i32,
        phase: u32,
        status: EventStatus,
        depends_on: Vec<String>,
        cash_before: f64,
    ) -> LiquidityStep {
        let drawn_total: f64 = self.facilities.iter().map(|f| f.drawn).sum();
        let undrawn: f64 = self
            .facilities
            .iter()
            .filter(|f| ordinal < f.expiry)
            .map(|f| f.commitment - f.drawn)
            .sum();
        LiquidityStep {
            event_id: event_id.to_string(),
            ordinal,
            phase,
            status,
            depends_on,
            cash_movement: unsigned(self.cash - cash_before),
            cash_after: unsigned(self.cash),
            restricted_cash_after: unsigned(self.restricted_now()),
            usable_cash_after: unsigned(self.cash - self.restricted_now()),
            facility_drawn_after: unsigned(drawn_total),
            facility_undrawn_after: unsigned(undrawn),
            pledged_quantity_after: unsigned(self.pledged.values().sum()),
            security_quantity_after: unsigned(
                self.security_lots
                    .iter()
                    .map(|l| self.owned.get(l).copied().unwrap_or(0.))
                    .sum(),
            ),
            posted_vm_after: unsigned(self.stock(Kind::PostedVm)),
            received_vm_after: unsigned(-self.stock(Kind::ReceivedVm)),
            posted_im_after: unsigned(self.stock(Kind::PostedIm)),
            received_im_custody_after: unsigned(self.stock(Kind::CustodyAsset)),
            pending_receivable_after: unsigned(self.stock(Kind::UnpaidReceivable)),
            pending_payable_after: unsigned(-self.stock(Kind::UnpaidPayable)),
            derivative_assets_after: unsigned(self.stock(Kind::DerivativeAsset)),
            derivative_liabilities_after: unsigned(-self.stock(Kind::DerivativeLiability)),
        }
    }
    /// Draws from accessible policy facilities, in order, what a payment needs.
    fn fund(
        &mut self,
        trigger: &str,
        ordinal: i32,
        phase: u32,
        sequence: u32,
        delta: f64,
    ) -> Result<(), Refusal> {
        let (floor, order) = match &self.policy {
            Some(p) => (p.floor, p.order.clone()),
            None => return Ok(()),
        };
        let usable = self.cash - self.restricted_now();
        let need = floor - (usable + delta);
        if need <= 1e-9 * floor.abs().max(1.) {
            return Ok(());
        }
        let mut remaining = need;
        for id in &order {
            if remaining <= 1e-12 {
                break;
            }
            let accessible = self
                .policy
                .as_ref()
                .and_then(|p| p.access.get(id))
                .is_some_and(|w| w.iter().any(|w| w.contains(ordinal)));
            let f = self.facility(id);
            if !accessible || ordinal >= f.expiry {
                continue;
            }
            let x = remaining.min(f.commitment - f.drawn);
            if x <= 1e-12 {
                continue;
            }
            self.act("draw", id, x, trigger, ordinal, phase, sequence, need)?;
            remaining -= x;
        }
        Ok(())
    }
    /// One policy action: its intent, its decision record and its liquidity step.
    #[allow(clippy::too_many_arguments)]
    fn act(
        &mut self,
        kind: &'static str,
        facility_id: &str,
        amount: f64,
        trigger: &str,
        ordinal: i32,
        phase: u32,
        sequence: u32,
        need: f64,
    ) -> Result<(), Refusal> {
        let n = {
            let p = self.policy.as_mut().expect("policy");
            p.count += 1;
            p.count
        };
        let id = format!("policy_{kind}:{facility_id}:{ordinal}:{n}");
        let (cash_before, usable_before) = (self.cash, self.cash - self.restricted_now());
        let generation = self.facility(facility_id).generation;
        let current = generation_instrument(facility_id, generation);
        match kind {
            "draw" => {
                self.facility_mut(facility_id).drawn += amount;
                self.apply(
                    &id,
                    ordinal,
                    phase,
                    sequence,
                    vec![
                        (Kind::Cash, "", amount),
                        (Kind::FundingPrincipal, &current, -amount),
                    ],
                )?;
            }
            "surplus_repayment" => {
                self.facility_mut(facility_id).drawn -= amount;
                self.apply(
                    &id,
                    ordinal,
                    phase,
                    sequence,
                    vec![
                        (Kind::FundingPrincipal, &current, amount),
                        (Kind::Cash, "", -amount),
                    ],
                )?;
            }
            "maturity_repayment" => {
                self.facility_mut(facility_id).drawn -= amount;
                // Another accessible facility may fund the maturity; nothing else is invented.
                self.post(
                    &id,
                    ordinal,
                    phase,
                    sequence,
                    vec![
                        (Kind::FundingPrincipal, &current, amount),
                        (Kind::Cash, "", -amount),
                    ],
                )?;
            }
            "renewal" => {
                let tenor = self
                    .policy
                    .as_ref()
                    .and_then(|p| p.renewal.get(facility_id))
                    .map_or(0, |r| r.tenor_days);
                let next = generation_instrument(facility_id, generation + 1);
                {
                    let f = self.facility_mut(facility_id);
                    f.generation += 1;
                    f.expiry += tenor;
                }
                self.apply(
                    &id,
                    ordinal,
                    phase,
                    sequence,
                    vec![
                        (Kind::FundingPrincipal, &current, amount),
                        (Kind::FundingPrincipal, &next, -amount),
                    ],
                )?;
                self.replacements.push(AccountingFact {
                    kind: "funding_replacement",
                    lot_id: next,
                    quantity: amount,
                    retained_quantity: amount,
                    effective_ordinal: ordinal,
                    settlement_ordinal: ordinal,
                    consideration: 0.,
                    removed_carrying: 0.,
                    realized: 0.,
                    economic_allocation_ids: Vec::new(),
                    formal_relationship_ids: Vec::new(),
                });
            }
            _ => unreachable!("policy action kind"),
        }
        let usable_after = self.cash - self.restricted_now();
        self.decisions.push(PolicyDecision {
            decision_id: id.clone(),
            ordinal,
            kind,
            facility_id: facility_id.to_string(),
            generation: self.facility(facility_id).generation,
            amount: unsigned(amount),
            trigger: trigger.to_string(),
            usable_cash_before: unsigned(usable_before),
            usable_cash_after: unsigned(usable_after),
            need: unsigned(need),
        });
        let mut step = self.step(
            &id,
            ordinal,
            phase,
            EventStatus::Settled,
            Vec::new(),
            cash_before,
        );
        step.pledged_quantity_after = unsigned(self.pre.0);
        step.security_quantity_after = unsigned(self.pre.1);
        step.received_im_custody_after = unsigned(self.pre.2);
        self.policy_log.push((
            EventReceipt {
                event_id: id,
                ordinal,
                phase,
                status: EventStatus::Settled,
            },
            step,
        ));
        Ok(())
    }
    fn apply(
        &mut self,
        event_id: &str,
        ordinal: i32,
        phase: u32,
        sequence: u32,
        lines: Vec<(Kind, &str, f64)>,
    ) -> Result<(), Refusal> {
        let memo = lines.iter().all(|l| l.0.memo());
        if !memo && lines.iter().any(|l| l.0.memo()) {
            return Err(unsupported(format!(
                "`{event_id}` mixes custody memo and financial lines"
            )));
        }
        let total: f64 = lines.iter().map(|l| l.2).sum();
        if !near(total, 0.) {
            return Err(unsupported(format!("`{event_id}` does not balance")));
        }
        let lines: Vec<Line> = lines
            .into_iter()
            .filter(|l| l.2 != 0.)
            .map(|(kind, instrument, debit)| Line {
                kind,
                instrument: instrument.into(),
                debit,
            })
            .collect();
        if lines.is_empty() {
            return Ok(());
        }
        for l in &lines {
            let book = if memo {
                &mut self.custody
            } else {
                &mut self.financial
            };
            *book.entry((l.kind, l.instrument.clone())).or_default() += l.debit;
            if l.kind == Kind::Cash {
                self.cash += l.debit;
                self.settled += l.debit;
            }
        }
        if self.cash < -1e-9 {
            return Err(Refusal::new(
                INSUFFICIENT_SETTLEMENT_LIQUIDITY,
                format!(
                    "`{event_id}` leaves cash at {}; no cash plug or emergency facility is invented",
                    self.cash
                ),
            ));
        }
        self.intents.push(Intent {
            event_id: event_id.into(),
            ordinal,
            phase,
            sequence,
            memo,
            lines,
        });
        Ok(())
    }
    fn stock(&self, kind: Kind) -> f64 {
        let book = if kind.memo() {
            &self.custody
        } else {
            &self.financial
        };
        book.iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    }
    fn available(&self, lot: &PositionLot) -> f64 {
        self.owned.get(&lot.lot_id).copied().unwrap_or(0.)
            - self.pledged.get(&lot.lot_id).copied().unwrap_or(0.)
            - lot.outside_encumbrance
    }
    fn closing(&self, ordinal: i32, lots: &[&PositionLot]) -> Closing {
        let sum = |book: &BTreeMap<(Kind, String), f64>| {
            let mut out: BTreeMap<Kind, f64> = BTreeMap::new();
            for ((k, _), v) in book {
                *out.entry(*k).or_default() += v;
            }
            out.into_iter().collect::<Vec<_>>()
        };
        Closing {
            ordinal,
            financial: sum(&self.financial),
            custody: sum(&self.custody),
            cash: self.cash,
            usable_cash: self.cash - self.restricted,
            inventory: lots
                .iter()
                .filter(|l| l.kind == LotKind::Security)
                .map(|l| {
                    let owned = self.owned.get(&l.lot_id).copied().unwrap_or(0.);
                    let pledged = self.pledged.get(&l.lot_id).copied().unwrap_or(0.);
                    Inventory {
                        lot_id: l.lot_id.clone(),
                        owned,
                        pledged,
                        outside_encumbrance: l.outside_encumbrance,
                        available: owned - pledged - l.outside_encumbrance,
                    }
                })
                .collect(),
        }
    }
}

enum Body<'a> {
    Advance(&'a FinancingAdvance, &'a PositionLot),
    Purchase(&'a SecurityPurchase, &'a PositionLot, f64, f64),
    Sale(&'a SecuritySale, &'a PositionLot, f64, f64),
    Repayment(&'a FundingRepayment, &'a PositionLot),
    Release(&'a Pledge, f64),
    PledgeDelivery(&'a Pledge),
    Entry(&'a HedgeEntry, &'a PositionLot, f64),
    TerminationEffective(&'a DerivativeTermination, &'a PositionLot, f64, f64),
    TerminationSettle(&'a DerivativeTermination, &'a MarginAgreement),
    VmSettle(&'a MarginAgreement, f64),
    VmReturn(&'a MarginAgreement),
    ImReturn(&'a MarginAgreement),
    Draw(&'a FacilityDraw, &'a Facility),
    FacilityRepay(&'a FacilityRepayment, &'a Facility),
    Im(&'a SuppliedInitialMargin, &'a MarginAgreement),
    DynamicIm(&'a MarginAgreement),
}
struct Event<'a> {
    id: String,
    ordinal: i32,
    phase: u32,
    depends_on: Vec<String>,
    state: InstructionState,
    body: Body<'a>,
}
const PHASE_ADVANCE: u32 = 1;
const PHASE_REPAYMENT: u32 = 2;
const PHASE_RELEASE: u32 = 3;
const PHASE_DVP: u32 = 4;
const PHASE_PLEDGE: u32 = 5;
const PHASE_DERIVATIVE: u32 = 6;
const PHASE_FACILITY_REPAY: u32 = 7;
const PHASE_VM: u32 = 8;
const PHASE_IM: u32 = 9;

fn event<'a>(
    id: String,
    ordinal: i32,
    phase: u32,
    depends_on: Vec<String>,
    state: InstructionState,
    body: Body<'a>,
) -> Event<'a> {
    Event {
        id,
        ordinal,
        phase,
        depends_on,
        state,
        body,
    }
}

fn push<'a>(
    ids: &mut HashSet<String>,
    events: &mut Vec<Event<'a>>,
    e: Event<'a>,
) -> Result<(), Refusal> {
    if !ids.insert(e.id.clone()) {
        return Err(Refusal::new(
            DUPLICATE_IDENTITY,
            format!("instruction `{}` is declared twice", e.id),
        ));
    }
    events.push(e);
    Ok(())
}

pub fn replay(
    plan: &Admitted,
    acq: &AcquisitionRequest,
    hedges: &[ResolvedHedge],
    account_id: &str,
) -> Result<AcquisitionJournal, Refusal> {
    replay_with_margin(plan, acq, hedges, &[], account_id)
}

/// The replay with native bucket sensitivities for research-method IM recomputations.
pub fn replay_with_margin(
    plan: &Admitted,
    acq: &AcquisitionRequest,
    hedges: &[ResolvedHedge],
    sensitivities: &[Sensitivity],
    account_id: &str,
) -> Result<AcquisitionJournal, Refusal> {
    let r = &plan.request;
    let account = r
        .accounts
        .iter()
        .find(|a| a.account_id == account_id)
        .ok_or_else(|| mapping(format!("unknown account `{account_id}`")))?;
    let asof = r.as_of_ordinal;
    let end = acq.horizon_end_ordinal;
    if end < asof || end - asof > MAX_EXECUTION_DAYS {
        return Err(Refusal::new(
            BOUND_EXCEEDED,
            "execution horizon must lie within 366 days of the as-of date",
        ));
    }
    let count = acq.purchases.len()
        + acq.sales.len()
        + acq.financings.len()
        + acq.repayments.len()
        + acq.hedge_entries.len()
        + acq.terminations.len()
        + acq.facility_draws.len()
        + acq.facility_repayments.len()
        + acq.initial_margin.len();
    if count > MAX_INSTRUCTIONS {
        return Err(Refusal::new(
            BOUND_EXCEEDED,
            "execution block exceeds its instruction bound",
        ));
    }
    let lots: HashMap<&str, &PositionLot> = r.lots.iter().map(|l| (l.lot_id.as_str(), l)).collect();
    let agreements: HashMap<&str, &MarginAgreement> = r
        .margin_agreements
        .iter()
        .map(|a| (a.agreement_id.as_str(), a))
        .collect();
    let pledges: HashMap<&str, &Pledge> = r
        .pledges
        .iter()
        .map(|p| (p.pledge_id.as_str(), p))
        .collect();
    let facilities: HashMap<&str, &Facility> = r
        .facilities
        .iter()
        .map(|f| (f.facility_id.as_str(), f))
        .collect();
    let lot = |id: &str, what: &str| -> Result<&PositionLot, Refusal> {
        let l = lots
            .get(id)
            .copied()
            .ok_or_else(|| mapping(format!("{what} names unknown lot `{id}`")))?;
        if l.account_id != account_id {
            return Err(mapping(format!(
                "{what} lot `{id}` belongs to another account"
            )));
        }
        Ok(l)
    };
    let agreement = |id: &str, what: &str| -> Result<&MarginAgreement, Refusal> {
        agreements
            .get(id)
            .copied()
            .ok_or_else(|| mapping(format!("{what} names unknown agreement `{id}`")))
    };
    let within = |ordinal: i32, what: &str| -> Result<(), Refusal> {
        if ordinal <= asof || ordinal > end {
            Err(Refusal::new(
                MALFORMED_INPUT,
                format!("{what} must settle after the as-of date and within the horizon"),
            ))
        } else {
            Ok(())
        }
    };
    // Formal treatment needs the ASC 815 owner's intents bound to this journal. This stage
    // replays economic (undesignated) treatment only; it never falls back.
    let touched: HashSet<&str> = acq
        .purchases
        .iter()
        .map(|p| p.lot_id.as_str())
        .chain(acq.sales.iter().map(|s| s.lot_id.as_str()))
        .chain(acq.financings.iter().map(|f| f.lot_id.as_str()))
        .chain(acq.repayments.iter().map(|f| f.lot_id.as_str()))
        .chain(acq.hedge_entries.iter().map(|h| h.lot_id.as_str()))
        .chain(acq.terminations.iter().map(|t| t.lot_id.as_str()))
        .collect();
    for a in &r.actions {
        if a.accounting_treatment == AccountingTreatment::Formal
            && a.lots.iter().any(|x| touched.contains(x.lot_id.as_str()))
        {
            return Err(Refusal::new(FORMAL_OWNER_REQUIRED, format!(
                "action `{}` selects formal hedge accounting for a lot this execution touches; no hedge-accounting-1 intents are bound and there is no fallback to undesignated treatment",
                a.action_id)));
        }
    }
    let resolved: HashMap<(&str, i32), &ResolvedHedge> = hedges
        .iter()
        .map(|h| ((h.lot_id.as_str(), h.ordinal), h))
        .collect();
    let allocations = |lot_id: &str| -> (Vec<String>, Vec<String>) {
        let mut economic = Vec::new();
        let mut formal = Vec::new();
        for h in r
            .hedge_allocations
            .iter()
            .filter(|h| h.exposure_lot_id == lot_id || h.hedge_lot_id == lot_id)
        {
            economic.push(h.allocation_id.clone());
            if let Some(f) = &h.formal_relationship_id {
                formal.push(f.clone());
            }
        }
        (economic, formal)
    };
    let name = |explicit: &Option<String>, default: String| explicit.clone().unwrap_or(default);

    let opening_lots: Vec<&PositionLot> = r
        .lots
        .iter()
        .filter(|l| l.account_id == account_id && l.acquisition_ordinal <= asof)
        .collect();
    let mut agreement_live: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for a in &r.margin_agreements {
        let live: BTreeSet<&str> = a
            .lot_ids
            .iter()
            .map(String::as_str)
            .filter(|id| opening_lots.iter().any(|l| l.lot_id == *id))
            .collect();
        agreement_live.insert(a.agreement_id.as_str(), live);
    }

    let mut events: Vec<Event> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();

    for p in &acq.purchases {
        let l = lot(&p.lot_id, "purchase")?;
        if l.kind != LotKind::Security
            || l.acquisition_ordinal != p.settlement_ordinal
            || p.settlement_ordinal <= asof
        {
            return Err(unsupported(format!(
                "purchase `{}` must acquire a new security lot on its settlement date after the as-of date",
                p.lot_id
            )));
        }
        within(p.settlement_ordinal, "purchase")?;
        if p.trade_ordinal < asof || p.trade_ordinal > p.settlement_ordinal {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!("purchase `{}` trade date is out of order", p.lot_id),
            ));
        }
        let price = money(p.clean_price_per_100, "clean price")?;
        let accrued = money(p.accrued_interest, "accrued interest")?;
        let fees = money(p.fees, "fees")?;
        if price <= 0. || accrued < 0. || fees < 0. {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "purchase `{}` needs a positive price and nonnegative accrued/fees",
                    p.lot_id
                ),
            ));
        }
        let cost = price / 100. * l.quantity;
        if !near(l.original_cost, cost + fees) {
            return Err(unsupported(format!(
                "purchase `{}` original cost {} must equal clean cost {cost} plus fees {fees}: no purchase-price gain is manufactured",
                p.lot_id, l.original_cost
            )));
        }
        push(
            &mut ids,
            &mut events,
            event(
                name(&p.instruction_id, format!("purchase:{}", p.lot_id)),
                p.settlement_ordinal,
                PHASE_DVP,
                p.depends_on.clone(),
                p.state,
                Body::Purchase(p, l, cost, accrued),
            ),
        )?;
    }
    for s in &acq.sales {
        let l = lot(&s.lot_id, "sale")?;
        if l.kind != LotKind::Security {
            return Err(unsupported(format!(
                "sale `{}` must sell a security lot",
                s.lot_id
            )));
        }
        within(s.settlement_ordinal, "sale")?;
        if s.trade_ordinal < asof
            || s.trade_ordinal > s.settlement_ordinal
            || l.acquisition_ordinal > s.settlement_ordinal
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!("sale `{}` dates are out of order", s.lot_id),
            ));
        }
        let price = money(s.clean_price_per_100, "clean price")?;
        let accrued = money(s.accrued_interest, "accrued interest")?;
        let fees = money(s.fees, "fees")?;
        let quantity = money(s.quantity, "sale quantity")?;
        if price <= 0.
            || accrued < 0.
            || fees < 0.
            || quantity <= 0.
            || quantity > l.quantity + 1e-9
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!("sale `{}` needs a positive price, nonnegative accrued/fees and a quantity within the lot", s.lot_id),
            ));
        }
        push(
            &mut ids,
            &mut events,
            event(
                name(
                    &s.instruction_id,
                    format!("sale:{}:{}", s.lot_id, s.settlement_ordinal),
                ),
                s.settlement_ordinal,
                PHASE_DVP,
                s.depends_on.clone(),
                s.state,
                Body::Sale(s, l, price / 100. * quantity, accrued),
            ),
        )?;
    }
    for f in &acq.financings {
        let l = lot(&f.lot_id, "financing")?;
        if l.kind != LotKind::Funding
            || l.acquisition_ordinal != f.advance_ordinal
            || f.advance_ordinal <= asof
        {
            return Err(unsupported(format!(
                "financing `{}` must advance a new funding lot on its advance date; an opening contract is never advanced again",
                f.lot_id
            )));
        }
        within(f.advance_ordinal, "financing")?;
        if !near(money(f.principal, "principal")?, l.quantity) || f.principal <= 0. {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "financing `{}` principal must equal the funding lot quantity",
                    f.lot_id
                ),
            ));
        }
        let pledge = pledges.get(f.pledge_id.as_str()).copied().ok_or_else(|| {
            mapping(format!(
                "financing `{}` names unknown pledge `{}`",
                f.lot_id, f.pledge_id
            ))
        })?;
        if pledge.agreement_id != f.lot_id {
            return Err(mapping(format!(
                "pledge `{}` does not secure funding lot `{}`",
                f.pledge_id, f.lot_id
            )));
        }
        let collateral = lot(&pledge.lot_id, "pledge")?;
        if collateral.acquisition_ordinal > f.advance_ordinal {
            return Err(Refusal::new(
                INVENTORY_UNAVAILABLE,
                format!(
                    "pledge `{}` delivers lot `{}` before it is owned",
                    f.pledge_id, pledge.lot_id
                ),
            ));
        }
        let id = name(&f.instruction_id, format!("advance:{}", f.lot_id));
        push(
            &mut ids,
            &mut events,
            event(
                id.clone(),
                f.advance_ordinal,
                PHASE_ADVANCE,
                f.depends_on.clone(),
                f.state,
                Body::Advance(f, l),
            ),
        )?;
        push(
            &mut ids,
            &mut events,
            event(
                format!("pledge:{}", pledge.pledge_id),
                f.advance_ordinal,
                PHASE_PLEDGE,
                vec![id],
                InstructionState::Pending,
                Body::PledgeDelivery(pledge),
            ),
        )?;
    }
    for rp in &acq.repayments {
        let l = lot(&rp.lot_id, "repayment")?;
        if l.kind != LotKind::Funding || l.acquisition_ordinal > rp.repayment_ordinal {
            return Err(unsupported(format!(
                "repayment `{}` must repay an advanced funding lot",
                rp.lot_id
            )));
        }
        within(rp.repayment_ordinal, "repayment")?;
        let principal = money(rp.principal, "repayment principal")?;
        if principal <= 0.
            || principal > l.quantity + 1e-9
            || money(rp.accrued_interest, "accrued interest")? < 0.
            || money(rp.break_cost, "break cost")? < 0.
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!("repayment `{}` needs principal within the lot and nonnegative accrued/break cost", rp.lot_id),
            ));
        }
        let id = name(
            &rp.instruction_id,
            format!("repay:{}:{}", rp.lot_id, rp.repayment_ordinal),
        );
        push(
            &mut ids,
            &mut events,
            event(
                id.clone(),
                rp.repayment_ordinal,
                PHASE_REPAYMENT,
                rp.depends_on.clone(),
                rp.state,
                Body::Repayment(rp, l),
            ),
        )?;
        for release in &rp.pledge_releases {
            let p = pledges
                .get(release.pledge_id.as_str())
                .copied()
                .ok_or_else(|| {
                    mapping(format!(
                        "release names unknown pledge `{}`",
                        release.pledge_id
                    ))
                })?;
            if p.agreement_id != rp.lot_id || money(release.quantity, "release quantity")? <= 0. {
                return Err(mapping(format!(
                    "pledge `{}` is not held by funding lot `{}`",
                    release.pledge_id, rp.lot_id
                )));
            }
            push(
                &mut ids,
                &mut events,
                event(
                    format!("release:{}:{}", p.pledge_id, rp.repayment_ordinal),
                    rp.repayment_ordinal,
                    PHASE_RELEASE,
                    vec![id.clone()],
                    InstructionState::Pending,
                    Body::Release(p, release.quantity),
                ),
            )?;
        }
    }
    for h in &acq.hedge_entries {
        let l = lot(&h.lot_id, "hedge entry")?;
        if l.kind != LotKind::Derivative || l.acquisition_ordinal != h.entry_ordinal {
            return Err(unsupported(format!(
                "hedge entry `{}` must enter a new derivative lot on its entry date",
                h.lot_id
            )));
        }
        within(h.entry_ordinal, "hedge entry")?;
        let a = agreement(&h.agreement_id, "hedge entry")?;
        if !a.lot_ids.contains(&h.lot_id) {
            return Err(mapping(format!(
                "agreement `{}` does not scope lot `{}`",
                h.agreement_id, h.lot_id
            )));
        }
        if let Some(offset) = &h.offsets_lot_id {
            let o = lot(offset, "offset")?;
            if o.kind != LotKind::Derivative || offset == &h.lot_id {
                return Err(unsupported(format!(
                    "offset `{}` must name another derivative lot",
                    h.lot_id
                )));
            }
        }
        let value = resolved
            .get(&(h.lot_id.as_str(), h.entry_ordinal))
            .copied()
            .ok_or_else(|| {
                mapping(format!(
                    "hedge entry `{}` has no native entry valuation",
                    h.lot_id
                ))
            })?;
        let clean = money(value.clean_value, "entry clean value")?;
        if !near(
            money(h.upfront_consideration, "upfront consideration")?,
            clean,
        ) {
            return Err(unsupported(format!(
                "hedge entry `{}` clean value {clean} needs equal upfront consideration {}; no day-one gain or loss is created",
                h.lot_id, h.upfront_consideration
            )));
        }
        let vm = &h.variation_margin;
        if money(vm.receive_threshold, "receive threshold")? < 0.
            || money(vm.post_threshold, "post threshold")? < 0.
            || money(vm.minimum_transfer, "minimum transfer")? < 0.
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "variation margin terms must be nonnegative",
            ));
        }
        let id = name(&h.instruction_id, format!("entry:{}", h.lot_id));
        push(
            &mut ids,
            &mut events,
            event(
                id.clone(),
                h.entry_ordinal,
                PHASE_DERIVATIVE,
                h.depends_on.clone(),
                h.state,
                Body::Entry(h, l, clean),
            ),
        )?;
        let call = if clean > vm.receive_threshold {
            clean - vm.receive_threshold
        } else if clean < -vm.post_threshold {
            clean + vm.post_threshold
        } else {
            0.
        };
        if call != 0. && call.abs() >= vm.minimum_transfer {
            let settle = h.entry_ordinal + a.settlement_lag_days as i32;
            if settle > end {
                return Err(unsupported(format!(
                    "hedge entry `{}` variation margin settles beyond the horizon; the complete collateral tail is required",
                    h.lot_id
                )));
            }
            push(
                &mut ids,
                &mut events,
                event(
                    format!("vm:{}", h.lot_id),
                    settle,
                    PHASE_VM,
                    vec![id],
                    InstructionState::Pending,
                    Body::VmSettle(a, call),
                ),
            )?;
        }
    }
    for t in &acq.terminations {
        let l = lot(&t.lot_id, "termination")?;
        if l.kind != LotKind::Derivative || l.acquisition_ordinal > t.effective_ordinal {
            return Err(unsupported(format!(
                "termination `{}` must terminate a held derivative lot",
                t.lot_id
            )));
        }
        within(t.effective_ordinal, "termination")?;
        if t.settlement_ordinal < t.effective_ordinal || t.settlement_ordinal > end {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "termination `{}` settles before its effective date or beyond the horizon",
                    t.lot_id
                ),
            ));
        }
        if money(t.fees, "termination fees")? < 0. {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "termination fees must be nonnegative",
            ));
        }
        let a = agreement(&t.agreement_id, "termination")?;
        if !a.lot_ids.contains(&t.lot_id) {
            return Err(mapping(format!(
                "agreement `{}` does not scope lot `{}`",
                t.agreement_id, t.lot_id
            )));
        }
        let value = resolved
            .get(&(t.lot_id.as_str(), t.effective_ordinal))
            .copied()
            .ok_or_else(|| {
                mapping(format!(
                    "termination `{}` has no native valuation at its effective date",
                    t.lot_id
                ))
            })?;
        let consideration = match &t.consideration {
            ConsiderationSource::ModelCleanValue => {
                money(value.clean_value, "termination clean value")?
            }
            ConsiderationSource::ExecutionQuote { amount, .. } => {
                money(*amount, "execution quote")?
            }
        };
        let due = money(value.due_cash, "contractual cash due")?;
        let id = name(&t.instruction_id, format!("terminate:{}", t.lot_id));
        push(
            &mut ids,
            &mut events,
            event(
                id.clone(),
                t.effective_ordinal,
                PHASE_DERIVATIVE,
                t.depends_on.clone(),
                t.state,
                Body::TerminationEffective(t, l, consideration, due),
            ),
        )?;
        let settle_id = format!("terminate_settle:{}", t.lot_id);
        push(
            &mut ids,
            &mut events,
            event(
                settle_id.clone(),
                t.settlement_ordinal,
                PHASE_DERIVATIVE,
                vec![id],
                InstructionState::Pending,
                Body::TerminationSettle(t, a),
            ),
        )?;
        let lag = a.settlement_lag_days as i32;
        if a.vm_at_termination == VmAtTermination::ReturnSeparately
            && t.settlement_ordinal + lag <= end
        {
            push(
                &mut ids,
                &mut events,
                event(
                    format!("vm_return:{}", t.lot_id),
                    t.settlement_ordinal + lag,
                    PHASE_VM,
                    vec![settle_id],
                    InstructionState::Pending,
                    Body::VmReturn(a),
                ),
            )?;
        }
    }
    for d in &acq.facility_draws {
        let f = facilities
            .get(d.facility_id.as_str())
            .copied()
            .ok_or_else(|| mapping(format!("draw names unknown facility `{}`", d.facility_id)))?;
        within(d.ordinal, "facility draw")?;
        if money(d.amount, "draw amount")? <= 0. || d.ordinal > f.expiry_ordinal {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "draw on `{}` needs a positive amount before expiry",
                    d.facility_id
                ),
            ));
        }
        push(
            &mut ids,
            &mut events,
            event(
                name(
                    &d.instruction_id,
                    format!("draw:{}:{}", d.facility_id, d.ordinal),
                ),
                d.ordinal,
                PHASE_ADVANCE,
                d.depends_on.clone(),
                d.state,
                Body::Draw(d, f),
            ),
        )?;
    }
    for rp in &acq.facility_repayments {
        let f = facilities
            .get(rp.facility_id.as_str())
            .copied()
            .ok_or_else(|| {
                mapping(format!(
                    "repayment names unknown facility `{}`",
                    rp.facility_id
                ))
            })?;
        within(rp.ordinal, "facility repayment")?;
        if money(rp.amount, "repayment amount")? <= 0. {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "facility repayment needs a positive amount",
            ));
        }
        push(
            &mut ids,
            &mut events,
            event(
                name(
                    &rp.instruction_id,
                    format!("facility_repay:{}:{}", rp.facility_id, rp.ordinal),
                ),
                rp.ordinal,
                PHASE_FACILITY_REPAY,
                rp.depends_on.clone(),
                rp.state,
                Body::FacilityRepay(rp, f),
            ),
        )?;
    }
    for im in &acq.initial_margin {
        let a = agreement(&im.agreement_id, "initial margin")?;
        if a.initial_margin != InitialMarginMode::SuppliedRequirements {
            return Err(unsupported(format!(
                "agreement `{}` does not select supplied IM requirements",
                im.agreement_id
            )));
        }
        if money(im.posted_requirement, "posted IM")? < 0.
            || money(im.received_requirement, "received IM")? < 0.
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "initial margin requirements must be nonnegative",
            ));
        }
        if a.segregation != Segregation::Segregated && im.received_requirement > 0. {
            return Err(unsupported(
                "received initial margin is custody memo only under a segregated agreement".into(),
            ));
        }
        if im.scenario_id != acq.scenario_id || im.ordinal <= asof || im.ordinal > end {
            return Err(Refusal::new(
                MARGIN_REFRESH_REQUIRED,
                format!(
                    "initial margin for `{}` is not bound to this scenario and horizon",
                    im.agreement_id
                ),
            ));
        }
        push(
            &mut ids,
            &mut events,
            event(
                format!("im:{}:{}", im.agreement_id, im.ordinal),
                im.ordinal,
                PHASE_IM,
                Vec::new(),
                InstructionState::Pending,
                Body::Im(im, a),
            ),
        )?;
    }
    // Stage I: research-method IM recomputations on the actually live portfolio.
    let mut dynamic_dates: BTreeSet<(&str, i32)> = BTreeSet::new();
    for dm in &acq.dynamic_margin {
        let a = agreement(&dm.agreement_id, "dynamic margin")?;
        if a.initial_margin != InitialMarginMode::ResearchSensitivity {
            return Err(unsupported(format!(
                "agreement `{}` does not select the research sensitivity IM method",
                dm.agreement_id
            )));
        }
        within(dm.ordinal, "dynamic margin")?;
        if !dynamic_dates.insert((dm.agreement_id.as_str(), dm.ordinal)) {
            return Err(Refusal::new(
                DUPLICATE_IDENTITY,
                format!(
                    "agreement `{}` recomputes IM twice on {}",
                    dm.agreement_id, dm.ordinal
                ),
            ));
        }
        push(
            &mut ids,
            &mut events,
            event(
                format!("dynamic_im:{}:{}", dm.agreement_id, dm.ordinal),
                dm.ordinal,
                PHASE_IM,
                Vec::new(),
                InstructionState::Pending,
                Body::DynamicIm(a),
            ),
        )?;
    }
    // Every population change in a legal scope needs a requirement bound to the exact live
    // portfolio, scenario and date. Proportional release is never assumed.
    let mut changes: BTreeSet<(&str, i32)> = BTreeSet::new();
    for h in &acq.hedge_entries {
        changes.insert((h.agreement_id.as_str(), h.entry_ordinal));
    }
    for t in &acq.terminations {
        changes.insert((t.agreement_id.as_str(), t.effective_ordinal));
    }
    for (agreement_id, ordinal) in &changes {
        let a = agreements[agreement_id];
        if a.initial_margin == InitialMarginMode::ResearchSensitivity {
            if !dynamic_dates.contains(&(*agreement_id, *ordinal)) {
                return Err(Refusal::new(MARGIN_REFRESH_REQUIRED, format!(
                    "agreement `{agreement_id}` changes population on {ordinal}: a research-method IM recomputation on that date is required"
                )));
            }
            continue;
        }
        if a.initial_margin != InitialMarginMode::SuppliedRequirements {
            continue;
        }
        let mut live: BTreeSet<&str> = agreement_live[agreement_id].clone();
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
            let mut supplied: Vec<&str> = im.portfolio_lot_ids.iter().map(String::as_str).collect();
            supplied.sort();
            supplied.dedup();
            im.agreement_id == *agreement_id
                && im.ordinal == *ordinal
                && supplied.iter().copied().eq(live.iter().copied())
        });
        if !bound {
            return Err(Refusal::new(MARGIN_REFRESH_REQUIRED, format!(
                "agreement `{agreement_id}` changes population on {ordinal}: a supplied IM requirement bound to the exact live portfolio, scenario and date is required; proportional release is never assumed"
            )));
        }
        if live.is_empty() {
            // An emptied legal scope returns its posted IM after the lag following the last
            // termination settlement; the zero requirement is the supplied, bound fact.
            let settle_ids: Vec<String> = acq
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
            let when = latest + a.settlement_lag_days as i32;
            if when <= end {
                push(
                    &mut ids,
                    &mut events,
                    event(
                        format!("im_return:{agreement_id}:{ordinal}"),
                        when,
                        PHASE_IM,
                        settle_ids,
                        InstructionState::Pending,
                        Body::ImReturn(a),
                    ),
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
        if e.depends_on.len() > MAX_DEPENDENCIES {
            return Err(Refusal::new(
                BOUND_EXCEEDED,
                format!("`{}` has too many dependencies", e.id),
            ));
        }
        for d in &e.depends_on {
            match order.get(d.as_str()) {
                None => {
                    return Err(mapping(format!(
                        "`{}` depends on unknown instruction `{d}`",
                        e.id
                    )))
                }
                Some(&j) if j >= i => {
                    return Err(Refusal::new(
                        SETTLEMENT_ORDER,
                        format!(
                            "`{}` depends on `{d}`, which is not earlier in the settlement order",
                            e.id
                        ),
                    ))
                }
                _ => {}
            }
        }
    }

    // Opening: supplied once. Opening lots carry their original anchors as this owner's stocks.
    let mut book = Book {
        financial: BTreeMap::new(),
        custody: BTreeMap::new(),
        cash: account.cash,
        restricted: 0.,
        settled: 0.,
        owned: opening_lots
            .iter()
            .filter(|l| l.kind == LotKind::Security)
            .map(|l| (l.lot_id.clone(), l.quantity))
            .collect(),
        pledged: BTreeMap::new(),
        pledge_left: BTreeMap::new(),
        intents: Vec::new(),
        facilities: r
            .facilities
            .iter()
            .map(|f| FacilityState {
                id: f.facility_id.clone(),
                commitment: f.commitment,
                drawn: f.drawn,
                expiry: f.expiry_ordinal,
                generation: 1,
            })
            .collect(),
        security_lots: r
            .lots
            .iter()
            .filter(|l| l.account_id == account_id && l.kind == LotKind::Security)
            .map(|l| l.lot_id.clone())
            .collect(),
        segregated: r
            .margin_agreements
            .iter()
            .filter(|a| a.segregation == Segregation::Segregated)
            .map(|a| a.agreement_id.clone())
            .collect(),
        policy: None,
        policy_log: Vec::new(),
        decisions: Vec::new(),
        replacements: Vec::new(),
        pre: (0., 0., 0.),
    };
    // Stage G: admit the business plan and arm the funding policy.
    let plan_mode = acq.business_plan.as_ref().map(|b| b.mode);
    if let Some(bp) = &acq.business_plan {
        let facility_ids: Vec<&str> = r
            .facilities
            .iter()
            .map(|f| f.facility_id.as_str())
            .collect();
        let scheduled: BTreeSet<&str> = acq
            .facility_draws
            .iter()
            .map(|d| d.facility_id.as_str())
            .chain(
                acq.facility_repayments
                    .iter()
                    .map(|d| d.facility_id.as_str()),
            )
            .collect();
        policy::validate(bp, &facility_ids, count, &scheduled, asof, end)?;
        if let Some(p) = &bp.funding_policy {
            book.policy = Some(PolicyRun {
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
    }
    let mut derivative_carrying: HashMap<&str, f64> = HashMap::new();
    let mut opening: Vec<(Kind, &str, f64)> = vec![(Kind::Cash, "", account.cash)];
    let (mut opening_assets, mut opening_liabilities) = (0., 0.);
    for l in &opening_lots {
        match l.kind {
            LotKind::Security | LotKind::Loan => {
                opening.push((Kind::AssetPrincipal, &l.lot_id, l.quantity));
                opening.push((
                    Kind::BookAdjustment,
                    &l.lot_id,
                    l.original_cost - l.quantity,
                ));
                opening_assets += l.original_cost;
            }
            LotKind::Funding => {
                opening.push((Kind::FundingPrincipal, &l.lot_id, -l.quantity));
                opening_liabilities += l.quantity;
            }
            LotKind::Derivative => {
                let carrying = l.original_cost;
                derivative_carrying.insert(&l.lot_id, carrying);
                if carrying >= 0. {
                    opening.push((Kind::DerivativeAsset, &l.lot_id, carrying));
                    opening_assets += carrying;
                } else {
                    opening.push((Kind::DerivativeLiability, &l.lot_id, carrying));
                    opening_liabilities -= carrying;
                }
            }
        }
    }
    for p in &r.pledges {
        if opening_lots.iter().any(|l| l.lot_id == p.lot_id) {
            *book.pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
            book.pledge_left.insert(p.pledge_id.clone(), p.quantity);
        }
    }
    let (
        mut fees_total,
        mut realized_total,
        mut posted_vm,
        mut received_vm,
        mut posted_im,
        mut received_im,
    ) = (0., 0., 0., 0., 0., 0.);
    for a in &r.margin_agreements {
        if let Some(c) = &a.opening_collateral {
            opening.push((Kind::PostedVm, &a.agreement_id, c.posted_vm));
            opening.push((Kind::ReceivedVm, &a.agreement_id, -c.received_vm));
            opening.push((Kind::PostedIm, &a.agreement_id, c.posted_im));
            opening_assets += c.posted_vm + c.posted_im;
            opening_liabilities += c.received_vm;
            posted_vm += c.posted_vm;
            received_vm += c.received_vm;
            posted_im += c.posted_im;
            received_im += c.received_im_custody;
            if a.segregation == Segregation::Segregated {
                book.restricted += c.received_vm;
            }
            if c.received_im_custody > 0. {
                *book
                    .custody
                    .entry((Kind::CustodyAsset, a.agreement_id.clone()))
                    .or_default() += c.received_im_custody;
                *book
                    .custody
                    .entry((Kind::CustodyObligation, a.agreement_id.clone()))
                    .or_default() -= c.received_im_custody;
            }
        }
    }
    let opening_equity_share = account.cash + opening_assets - opening_liabilities;
    opening.push((Kind::OpeningEquity, "", -opening_equity_share));
    let opening_lines: Vec<Line> = opening
        .iter()
        .filter(|l| l.2 != 0.)
        .map(|(k, i, d)| Line {
            kind: *k,
            instrument: (*i).to_string(),
            debit: *d,
        })
        .collect();
    // Every closing reports cash and the opening equity share, even when either is zero.
    book.financial.insert((Kind::Cash, String::new()), 0.);
    book.financial
        .insert((Kind::OpeningEquity, String::new()), 0.);
    for l in &opening_lines {
        *book
            .financial
            .entry((l.kind, l.instrument.clone()))
            .or_default() += l.debit;
    }
    let account_lots: Vec<&PositionLot> = r
        .lots
        .iter()
        .filter(|l| l.account_id == account_id)
        .collect();
    let mut daily = Vec::new();
    let mut receipts = Vec::new();
    let mut facts = Vec::new();
    let mut settled: HashSet<String> = HashSet::new();
    let mut steps = Vec::new();
    let mut margin_calls: Vec<MarginCall> = Vec::new();
    let opening_state = book.step(
        "opening",
        asof,
        0,
        EventStatus::Settled,
        Vec::new(),
        book.cash,
    );
    let mut cursor = 0;
    let mut sequence = 0u32;
    for ordinal in asof..=end {
        // Start of day: facility generations that expire today are renewed when a declared
        // renewal window and the generation bound permit, or their balance is repaid.
        book.pre = book.inventory_now();
        if let Some(order) = book.policy.as_ref().map(|p| p.order.clone()) {
            for id in &order {
                let (expiry, generation, drawn) = {
                    let f = book.facility(id);
                    (f.expiry, f.generation, f.drawn)
                };
                if expiry != ordinal {
                    continue;
                }
                sequence += 1;
                let renewable = book
                    .policy
                    .as_ref()
                    .and_then(|p| p.renewal.get(id))
                    .is_some_and(|r| {
                        generation < r.max_generations
                            && r.windows.iter().any(|w| w.contains(ordinal))
                    });
                if renewable {
                    book.act(
                        "renewal",
                        id,
                        drawn,
                        "start_of_day",
                        ordinal,
                        0,
                        sequence,
                        0.,
                    )?;
                } else if drawn > 1e-12 {
                    book.act(
                        "maturity_repayment",
                        id,
                        drawn,
                        "start_of_day",
                        ordinal,
                        0,
                        sequence,
                        0.,
                    )?;
                }
            }
            for (receipt, step) in book.policy_log.drain(..) {
                receipts.push(receipt);
                steps.push(step);
            }
        }
        while cursor < events.len() && events[cursor].ordinal == ordinal {
            let e = &events[cursor];
            let cash_before = book.cash;
            book.pre = book.inventory_now();
            sequence += 1;
            let status = if e.state == InstructionState::Failed {
                EventStatus::Failed
            } else if !e.depends_on.iter().all(|d| settled.contains(d)) {
                EventStatus::Blocked
            } else {
                let (phase, id) = (e.phase, e.id.as_str());
                match &e.body {
                    Body::Advance(f, l) => {
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", f.principal),
                                (Kind::FundingPrincipal, &l.lot_id, -f.principal),
                            ],
                        )?;
                    }
                    Body::Draw(d, f) => {
                        let used = book.facility(&f.facility_id).drawn;
                        if d.amount > f.commitment - used + 1e-9 * f.commitment.max(1.) {
                            return Err(Refusal::new(
                                INSUFFICIENT_SETTLEMENT_LIQUIDITY,
                                format!(
                                    "`{id}` exceeds facility `{}` headroom {}",
                                    f.facility_id,
                                    f.commitment - used
                                ),
                            ));
                        }
                        book.facility_mut(&f.facility_id).drawn += d.amount;
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", d.amount),
                                (Kind::FundingPrincipal, &f.facility_id, -d.amount),
                            ],
                        )?;
                    }
                    Body::FacilityRepay(rp, f) => {
                        let used = book.facility(&f.facility_id).drawn;
                        if rp.amount > used + 1e-9 {
                            return Err(Refusal::new(
                                OVERALLOCATION,
                                format!(
                                    "`{id}` repays more than facility `{}` has drawn",
                                    f.facility_id
                                ),
                            ));
                        }
                        book.facility_mut(&f.facility_id).drawn -= rp.amount;
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::FundingPrincipal, &f.facility_id, rp.amount),
                                (Kind::Cash, "", -rp.amount),
                            ],
                        )?;
                    }
                    Body::Repayment(rp, l) => {
                        let outstanding = -book.balance(Kind::FundingPrincipal, &l.lot_id);
                        if rp.principal > outstanding + 1e-9 {
                            return Err(Refusal::new(
                                OVERALLOCATION,
                                format!(
                                    "`{id}` repays more than funding lot `{}` has outstanding",
                                    l.lot_id
                                ),
                            ));
                        }
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::FundingPrincipal, &l.lot_id, rp.principal),
                                (Kind::InterestExpense, &l.lot_id, rp.accrued_interest),
                                (Kind::FeeExpense, &l.lot_id, rp.break_cost),
                                (
                                    Kind::Cash,
                                    "",
                                    -(rp.principal + rp.accrued_interest + rp.break_cost),
                                ),
                            ],
                        )?;
                        fees_total += rp.break_cost;
                    }
                    Body::Release(p, quantity) => {
                        let left = book.pledge_left.get_mut(&p.pledge_id).ok_or_else(|| {
                            mapping(format!("pledge `{}` was never delivered", p.pledge_id))
                        })?;
                        if *quantity > *left + 1e-9 {
                            return Err(Refusal::new(
                                INVENTORY_UNAVAILABLE,
                                format!("`{id}` releases more than pledge `{}` holds", p.pledge_id),
                            ));
                        }
                        *left -= quantity;
                        *book.pledged.entry(p.lot_id.clone()).or_default() -= quantity;
                    }
                    Body::Purchase(p, l, cost, accrued) => {
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::AssetPrincipal, &l.lot_id, l.quantity),
                                (Kind::BookAdjustment, &l.lot_id, cost - l.quantity),
                                (Kind::InterestReceivable, &l.lot_id, *accrued),
                                (Kind::FeeExpense, &l.lot_id, p.fees),
                                (Kind::Cash, "", -(cost + accrued + p.fees)),
                            ],
                        )?;
                        fees_total += p.fees;
                        *book.owned.entry(l.lot_id.clone()).or_default() += l.quantity;
                    }
                    Body::Sale(s, l, consideration, accrued) => {
                        let owned = book.owned.get(&l.lot_id).copied().unwrap_or(0.);
                        let available = book.available(l);
                        if s.quantity > available + 1e-9 * l.quantity.max(1.) {
                            return Err(Refusal::new(INVENTORY_UNAVAILABLE, format!(
                                "`{id}` delivers {} of lot `{}` but only {available} is owned, released and unencumbered",
                                s.quantity, l.lot_id
                            )));
                        }
                        let share = s.quantity / owned;
                        let principal = book.balance(Kind::AssetPrincipal, &l.lot_id) * share;
                        let adjustment = book.balance(Kind::BookAdjustment, &l.lot_id) * share;
                        let receivable = book.balance(Kind::InterestReceivable, &l.lot_id) * share;
                        let removed = principal + adjustment;
                        let realized = consideration - removed;
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", consideration + accrued - s.fees),
                                (Kind::AssetPrincipal, &l.lot_id, -principal),
                                (Kind::BookAdjustment, &l.lot_id, -adjustment),
                                (Kind::InterestReceivable, &l.lot_id, -receivable),
                                (Kind::InterestExpense, &l.lot_id, receivable - accrued),
                                (Kind::FeeExpense, &l.lot_id, s.fees),
                                (Kind::RealizedGainLoss, &l.lot_id, -realized),
                            ],
                        )?;
                        fees_total += s.fees;
                        realized_total += realized;
                        *book.owned.entry(l.lot_id.clone()).or_default() -= s.quantity;
                        let (economic, formal) = allocations(&l.lot_id);
                        facts.push(AccountingFact {
                            kind: if near(s.quantity, owned) {
                                "sale"
                            } else {
                                "partial_sale"
                            },
                            lot_id: l.lot_id.clone(),
                            quantity: s.quantity,
                            retained_quantity: owned - s.quantity,
                            effective_ordinal: s.trade_ordinal,
                            settlement_ordinal: s.settlement_ordinal,
                            consideration: *consideration,
                            removed_carrying: removed,
                            realized,
                            economic_allocation_ids: economic,
                            formal_relationship_ids: formal,
                        });
                    }
                    Body::PledgeDelivery(p) => {
                        let l = lots[p.lot_id.as_str()];
                        let available = book.available(l);
                        if p.quantity > available + 1e-9 * l.quantity.max(1.) {
                            return Err(Refusal::new(INVENTORY_UNAVAILABLE, format!("`{id}` needs {} of lot `{}` but only {available} is unencumbered and owned", p.quantity, p.lot_id)));
                        }
                        *book.pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
                        book.pledge_left.insert(p.pledge_id.clone(), p.quantity);
                    }
                    Body::Entry(h, l, clean) => {
                        let (kind, debit) = if *clean >= 0. {
                            (Kind::DerivativeAsset, *clean)
                        } else {
                            (Kind::DerivativeLiability, *clean)
                        };
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (kind, &l.lot_id, debit),
                                (Kind::Cash, "", -h.upfront_consideration),
                            ],
                        )?;
                        derivative_carrying.insert(&l.lot_id, *clean);
                    }
                    Body::TerminationEffective(t, l, consideration, due) => {
                        let carrying =
                            derivative_carrying
                                .remove(l.lot_id.as_str())
                                .ok_or_else(|| {
                                    unsupported(format!("`{id}`: lot `{}` is not live", l.lot_id))
                                })?;
                        let kind = if carrying >= 0. {
                            Kind::DerivativeAsset
                        } else {
                            Kind::DerivativeLiability
                        };
                        let claim = consideration + due;
                        let realized = consideration - carrying;
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (kind, &l.lot_id, -carrying),
                                (
                                    if claim >= 0. {
                                        Kind::UnpaidReceivable
                                    } else {
                                        Kind::UnpaidPayable
                                    },
                                    &l.lot_id,
                                    claim,
                                ),
                                (Kind::InterestExpense, &l.lot_id, -due),
                                (Kind::RealizedGainLoss, &l.lot_id, -realized),
                            ],
                        )?;
                        realized_total += realized;
                        let (economic, formal) = allocations(&l.lot_id);
                        facts.push(AccountingFact {
                            kind: "termination",
                            lot_id: l.lot_id.clone(),
                            quantity: l.quantity,
                            retained_quantity: 0.,
                            effective_ordinal: t.effective_ordinal,
                            settlement_ordinal: t.settlement_ordinal,
                            consideration: *consideration,
                            removed_carrying: carrying,
                            realized,
                            economic_allocation_ids: economic,
                            formal_relationship_ids: formal,
                        });
                    }
                    Body::TerminationSettle(t, a) => {
                        let receivable = book.balance(Kind::UnpaidReceivable, &t.lot_id);
                        let payable = book.balance(Kind::UnpaidPayable, &t.lot_id);
                        let claim = receivable + payable;
                        let mut lines = vec![
                            (
                                if receivable != 0. {
                                    Kind::UnpaidReceivable
                                } else {
                                    Kind::UnpaidPayable
                                },
                                t.lot_id.as_str(),
                                -claim,
                            ),
                            (Kind::FeeExpense, t.lot_id.as_str(), t.fees),
                        ];
                        let mut cash = claim - t.fees;
                        if a.vm_at_termination == VmAtTermination::ApplyAgainstConsideration {
                            // Existing VM is applied against the consideration exactly once.
                            let posted = book.balance(Kind::PostedVm, &a.agreement_id);
                            let received = -book.balance(Kind::ReceivedVm, &a.agreement_id);
                            lines.push((Kind::PostedVm, a.agreement_id.as_str(), -posted));
                            lines.push((Kind::ReceivedVm, a.agreement_id.as_str(), received));
                            cash += posted - received;
                            posted_vm -= posted;
                            received_vm -= received;
                            if a.segregation == Segregation::Segregated {
                                book.restricted -= received;
                            }
                        }
                        lines.push((Kind::Cash, "", cash));
                        book.post(id, ordinal, phase, sequence, lines)?;
                        fees_total += t.fees;
                    }
                    Body::VmSettle(a, call) => {
                        if *call > 0. {
                            book.post(
                                id,
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::Cash, "", *call),
                                    (Kind::ReceivedVm, &a.agreement_id, -*call),
                                ],
                            )?;
                            received_vm += call;
                            if a.segregation == Segregation::Segregated {
                                book.restricted += call;
                            }
                        } else {
                            book.post(
                                id,
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::PostedVm, &a.agreement_id, -*call),
                                    (Kind::Cash, "", *call),
                                ],
                            )?;
                            posted_vm -= call;
                        }
                    }
                    Body::VmReturn(a) => {
                        let posted = book.balance(Kind::PostedVm, &a.agreement_id);
                        let received = -book.balance(Kind::ReceivedVm, &a.agreement_id);
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", posted - received),
                                (Kind::PostedVm, &a.agreement_id, -posted),
                                (Kind::ReceivedVm, &a.agreement_id, received),
                            ],
                        )?;
                        posted_vm -= posted;
                        received_vm -= received;
                        if a.segregation == Segregation::Segregated {
                            book.restricted -= received;
                        }
                    }
                    Body::ImReturn(a) => {
                        let posted = book.balance(Kind::PostedIm, &a.agreement_id);
                        let custody = book.balance(Kind::CustodyAsset, &a.agreement_id);
                        book.post(
                            id,
                            ordinal,
                            phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", posted),
                                (Kind::PostedIm, &a.agreement_id, -posted),
                            ],
                        )?;
                        posted_im -= posted;
                        if custody != 0. {
                            sequence += 1;
                            book.post(
                                &format!("{id}:custody"),
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::CustodyAsset, &a.agreement_id, -custody),
                                    (Kind::CustodyObligation, &a.agreement_id, custody),
                                ],
                            )?;
                            received_im -= custody;
                        }
                    }
                    Body::DynamicIm(a) => {
                        let m = a.margin_method.as_ref().expect("admitted research method");
                        let n = m.bucket_upper_days.len();
                        let live: Vec<&str> = a
                            .lot_ids
                            .iter()
                            .map(String::as_str)
                            .filter(|l| derivative_carrying.contains_key(l))
                            .collect();
                        let mut s = vec![0.; n];
                        for lot in &live {
                            let found = sensitivities
                                .iter()
                                .find(|x| x.lot_id == *lot && x.ordinal == ordinal)
                                .filter(|x| x.buckets.len() == n)
                                .ok_or_else(|| Refusal::new(MARGIN_REFRESH_REQUIRED, format!(
                                    "`{id}`: live lot `{lot}` has no native sensitivity on {ordinal}"
                                )))?;
                            for (t, v) in s.iter_mut().zip(&found.buckets) {
                                *t += v;
                            }
                        }
                        let mut q = 0.;
                        for b in 0..n {
                            for c in 0..n {
                                q += m.risk_weights[b]
                                    * s[b]
                                    * m.correlations[b][c]
                                    * m.risk_weights[c]
                                    * s[c];
                            }
                        }
                        let requirement = unsigned(q.max(0.).sqrt());
                        let received_target = if m.two_way { requirement } else { 0. };
                        if received_target > 0. && a.segregation != Segregation::Segregated {
                            return Err(unsupported(format!(
                                "`{id}`: two-way IM is received only into segregated custody"
                            )));
                        }
                        let posted_change =
                            requirement - book.balance(Kind::PostedIm, &a.agreement_id);
                        if posted_change != 0. {
                            book.post(
                                id,
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::PostedIm, &a.agreement_id, posted_change),
                                    (Kind::Cash, "", -posted_change),
                                ],
                            )?;
                            posted_im += posted_change;
                        }
                        let received_change =
                            received_target - book.balance(Kind::CustodyAsset, &a.agreement_id);
                        if received_change != 0. {
                            sequence += 1;
                            book.post(
                                &format!("{id}:custody"),
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::CustodyAsset, &a.agreement_id, received_change),
                                    (Kind::CustodyObligation, &a.agreement_id, -received_change),
                                ],
                            )?;
                            received_im += received_change;
                        }
                        margin_calls.push(MarginCall {
                            event_id: id.to_string(),
                            agreement_id: a.agreement_id.clone(),
                            ordinal,
                            live_lot_ids: live.iter().map(|l| l.to_string()).collect(),
                            sensitivities: s.iter().map(|v| unsigned(*v)).collect(),
                            requirement,
                            posted_change: unsigned(posted_change),
                            received_change: unsigned(received_change),
                        });
                    }
                    Body::Im(im, a) => {
                        if im.posted_requirement > 0. {
                            book.post(
                                id,
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::PostedIm, &a.agreement_id, im.posted_requirement),
                                    (Kind::Cash, "", -im.posted_requirement),
                                ],
                            )?;
                            posted_im += im.posted_requirement;
                        }
                        if im.received_requirement > 0. {
                            sequence += 1;
                            book.post(
                                &format!("{id}:custody"),
                                ordinal,
                                phase,
                                sequence,
                                vec![
                                    (Kind::CustodyAsset, &a.agreement_id, im.received_requirement),
                                    (
                                        Kind::CustodyObligation,
                                        &a.agreement_id,
                                        -im.received_requirement,
                                    ),
                                ],
                            )?;
                            received_im += im.received_requirement;
                        }
                    }
                }
                settled.insert(e.id.clone());
                EventStatus::Settled
            };
            // Policy funding taken for this event's payment settles before it.
            for (receipt_entry, step) in book.policy_log.drain(..) {
                receipts.push(receipt_entry);
                steps.push(step);
            }
            receipts.push(EventReceipt {
                event_id: e.id.clone(),
                ordinal,
                phase: e.phase,
                status,
            });
            let cash_before_event = cash_before
                + steps
                    .iter()
                    .rev()
                    .take_while(|s: &&LiquidityStep| {
                        s.event_id.starts_with("policy_draw:") && s.ordinal == ordinal
                    })
                    .map(|s| s.cash_movement)
                    .sum::<f64>();
            steps.push(book.step(
                &e.id,
                ordinal,
                e.phase,
                status,
                e.depends_on.clone(),
                cash_before_event,
            ));
            cursor += 1;
        }
        // End of day: repay drawn policy facilities, last drawn first, from usable cash above
        // the floor.
        book.pre = book.inventory_now();
        if let Some((floor, order, true)) = book
            .policy
            .as_ref()
            .map(|p| (p.floor, p.order.clone(), p.repay_surplus))
        {
            for id in order.iter().rev() {
                let surplus = book.cash - book.restricted_now() - floor;
                let drawn = book.facility(id).drawn;
                let x = surplus.min(drawn);
                if x > 1e-9 * floor.abs().max(1.) {
                    sequence += 1;
                    book.act(
                        "surplus_repayment",
                        id,
                        x,
                        "end_of_day",
                        ordinal,
                        10,
                        sequence,
                        0.,
                    )?;
                }
            }
            for (receipt, step) in book.policy_log.drain(..) {
                receipts.push(receipt);
                steps.push(step);
            }
        }
        daily.push(book.closing(ordinal, &account_lots));
    }
    let stock = |kind: Kind| -> f64 {
        book.financial
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    };
    let custody = |kind: Kind| -> f64 {
        book.custody
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    };
    let cash_bridge =
        near(account.cash + book.settled, book.cash) && near(stock(Kind::Cash), book.cash);
    let inventory = daily.last().is_some_and(|d| {
        d.inventory.iter().all(|i| {
            i.available >= -1e-9 && near(i.owned - i.pledged - i.outside_encumbrance, i.available)
        })
    });
    let collateral = near(stock(Kind::PostedVm), posted_vm)
        && near(-stock(Kind::ReceivedVm), received_vm)
        && near(stock(Kind::PostedIm), posted_im)
        && near(custody(Kind::CustodyAsset), received_im)
        && near(-custody(Kind::CustodyObligation), received_im);
    let open_claims = !near(
        stock(Kind::UnpaidReceivable) + stock(Kind::UnpaidPayable),
        0.,
    ) || (derivative_carrying.is_empty()
        && (!near(posted_vm + received_vm, 0.) || !near(posted_im + received_im, 0.)));
    // Stage G: renewals are replacement-debt facts for the ASC 815 owner; monthly reports are
    // derived from the dated intents and steps and must reconcile to them exactly.
    facts.append(&mut book.replacements);
    let monthly = match plan_mode {
        None => None,
        Some(_) => {
            let months = policy::monthly(
                &book.intents,
                &steps,
                &policy::Opening {
                    cash: account.cash,
                    usable_cash: opening_state.usable_cash_after,
                    facility_drawn: opening_state.facility_drawn_after,
                    posted_margin: opening_state.posted_vm_after + opening_state.posted_im_after,
                    pledged_quantity: opening_state.pledged_quantity_after,
                    security_quantity: opening_state.security_quantity_after,
                },
                asof,
                end,
            )?;
            let net: f64 = months.iter().map(|m| m.net_cash).sum();
            let chained = months
                .windows(2)
                .all(|w| w[1].opening_cash == w[0].closing_cash);
            let closes = months
                .last()
                .is_some_and(|m| near(m.closing_cash, book.cash));
            if !(near(net, book.settled) && chained && closes) {
                return Err(unsupported(
                    "monthly reports do not reconcile to the dated cash movements".into(),
                ));
            }
            Some(months)
        }
    };
    let receipt = Receipt {
        opening_cash: account.cash,
        closing_cash: book.cash,
        settled_cash_movements: book.settled,
        gross_derivative_assets: stock(Kind::DerivativeAsset),
        gross_derivative_liabilities: -stock(Kind::DerivativeLiability),
        posted_vm,
        received_vm,
        posted_im,
        received_im_custody: received_im,
        fees_expensed: fees_total,
        realized: realized_total,
        cash_bridge_reconciled: cash_bridge,
        inventory_conserved: inventory,
        collateral_conserved: collateral,
        open_claims,
        events: receipts,
        steps,
        accounting_facts: facts,
        formal_hedge_accounting: false,
        business_plan_mode: plan_mode,
        policy_decisions: std::mem::take(&mut book.decisions),
        monthly,
        margin_calls,
    };
    if !(cash_bridge && inventory && collateral) {
        return Err(unsupported(
            "execution replay failed its own conservation receipt".into(),
        ));
    }
    Ok(AcquisitionJournal {
        account_id: account_id.into(),
        scenario_id: acq.scenario_id.clone(),
        as_of_ordinal: asof,
        tail_ordinal: end,
        opening_cash: account.cash,
        opening_equity: account.equity,
        opening_stocks: (opening_assets, opening_liabilities),
        opening: opening_lines,
        intents: book.intents,
        daily,
        receipt,
    })
}
