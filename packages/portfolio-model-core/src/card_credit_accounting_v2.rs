//! Explicit acquired-card interest-basis ownership. No implicit schema1 replacement.
use crate::dated_income::{self, DatedIncomeRequest, DatedIncomeResult, IncomeEvent};
use crate::{
    card_credit_accounting::Termination,
    consumer_card::{self, CardAction, CardRequest, CardResult},
    consumer_contract::ConsumerState,
};
use crate::{
    cashflow::{validate_ordinal, CashflowMeasure},
    credit_accounting::{self as credit, *},
    limits::MAX_MONETARY_AMOUNT,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ops::{Deref, DerefMut};
pub const SCHEMA: &str = "card-credit-accounting-2";
pub const MAX_DAYS: u32 = 64;
pub const MAX_SOURCE_ACTIONS: usize = 64;
pub const MAX_COLLECTIONS: usize = 16;
pub const MAX_EVENTS: usize = 256;
pub const MAX_RAW_BYTES: usize = 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WORK: usize = 2_000_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub identity: Identity,
    pub original_yield_anchor: Identity,
    pub economic_source: Identity,
    pub position_id: String,
    pub currency: String,
    pub principal: f64,
    pub cost: f64,
    pub original_annual_effective_rate: f64,
    pub acquisition_ordinal: i32,
    pub policy: AccountingPolicy,
    pub accounting_basis: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub account: OwnedAccount,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub atomic_action: usize,
    pub ordinal: i32,
    pub event: String,
    pub claim_id: Option<String>,
    pub review: Identity,
    pub source: Identity,
    pub original_yield_anchor: Identity,
    pub lines: Vec<Line>,
    pub original_eir_income: f64,
    pub reviewed_basis_income: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OwnedAccount {
    #[serde(rename = "cash")]
    Cash,
    #[serde(rename = "live_principal")]
    LivePrincipal,
    #[serde(rename = "default_principal")]
    DefaultPrincipal,
    #[serde(rename = "live_accrued_interest")]
    LiveAccruedInterest,
    #[serde(rename = "default_accrued_interest")]
    DefaultAccruedInterest,
    #[serde(rename = "live_book_adjustment")]
    LiveBookAdjustment,
    #[serde(rename = "default_book_adjustment")]
    DefaultBookAdjustment,
    #[serde(rename = "allowance")]
    Allowance,
    #[serde(rename = "interest_income")]
    SharedInterestIncome,
    #[serde(rename = "credit_loss_expense")]
    CreditLossExpense,
    #[serde(rename = "basis_recognition_expense")]
    BasisRecognitionExpense,
    #[serde(rename = "recovery_income")]
    RecoveryIncome,
    #[serde(rename = "live_interest_basis")]
    LiveInterestBasis,
    #[serde(rename = "default_interest_basis")]
    DefaultInterestBasis,
    #[serde(rename = "pnl:original_effective_interest")]
    OriginalEffectiveInterest,
    #[serde(rename = "pnl:reviewed_basis_realization")]
    ReviewedBasisRealization,
    #[serde(rename = "pnl:reviewed_default_basis_realization")]
    ReviewedDefaultBasisRealization,
    #[serde(rename = "pnl:reviewed_recovery_basis_realization")]
    ReviewedRecoveryBasisRealization,
}
impl OwnedAccount {
    fn named(s: &str) -> Result<Self, String> {
        serde_json::from_value(serde_json::Value::String(s.into()))
            .map_err(|_| "unsupported card rights posting account".into())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub core: AccountingState,
    pub live_interest_basis: f64,
    pub claim_interest_basis: BTreeMap<String, f64>,
    pub cash_received: f64,
    pub original_eir_income: f64,
    pub reviewed_basis_income: f64,
    pub shared_default_income: f64,
    pub last_ordinal: i32,
    pub action_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomeStep {
    pub channel: String,
    pub request: DatedIncomeRequest,
    pub result: DatedIncomeResult,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct IndexRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceLineage {
    pub kind: String,
    pub recognition_indices: Vec<usize>,
    pub cash_event_id: Option<String>,
    pub interest_recovery_index: Option<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AtomicAction {
    pub atomic_action: usize,
    pub ordinal: i32,
    pub events: IndexRange,
    pub shared_intents: IndexRange,
    pub income_steps: IndexRange,
    pub source: SourceLineage,
    pub committed: Snapshot,
}
struct Book {
    review: Review,
    snapshot: Snapshot,
    events: Vec<Event>,
    shared_intents: Vec<JournalIntent>,
    income_steps: Vec<IncomeStep>,
    actions: Vec<AtomicAction>,
    reserved_bytes: usize,
    charged_bytes: usize,
}
impl Deref for Book {
    type Target = Snapshot;
    fn deref(&self) -> &Snapshot {
        &self.snapshot
    }
}
impl DerefMut for Book {
    fn deref_mut(&mut self) -> &mut Snapshot {
        &mut self.snapshot
    }
}
fn money(v: f64) -> Result<f64, String> {
    if v.is_finite() && v.abs() <= MAX_MONETARY_AMOUNT {
        Ok(v)
    } else {
        Err("finite bounded rights amount required".into())
    }
}
fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("reviewed bounded identity required".into())
    } else {
        Ok(())
    }
}
fn id(i: &Identity) -> Result<(), String> {
    label(&i.id)?;
    label(&i.revision)
}
impl Book {
    pub fn new(r: Review) -> Result<Self, String> {
        for i in [
            &r.identity,
            &r.original_yield_anchor,
            &r.economic_source,
            &r.policy.identity,
        ] {
            id(i)?;
        }
        label(&r.position_id)?;
        label(&r.currency)?;
        validate_ordinal(r.acquisition_ordinal)?;
        money(r.principal)?;
        money(r.cost)?;
        if r.accounting_basis != "supplied_non_pcd_gross_cost-1"
            || r.principal <= 0.
            || r.cost <= 0.
            || !r.original_annual_effective_rate.is_finite()
            || !(-0.99..=10.).contains(&r.original_annual_effective_rate)
        {
            return Err("yield rights proof requires positive acquired cost/face and supplied bounded original EIR".into());
        }
        let mut live = [LiveState::default(); 3];
        live[0].principal = r.principal;
        live[0].book_adjustment = r.cost - r.principal;
        let b = Self {
            review: r.clone(),
            snapshot: Snapshot {
                last_ordinal: r.acquisition_ordinal,
                action_count: 0,
                core: AccountingState {
                    live,
                    allowance: 0.,
                    claims: vec![],
                },
                live_interest_basis: 0.,
                claim_interest_basis: BTreeMap::new(),
                cash_received: 0.,
                original_eir_income: 0.,
                reviewed_basis_income: 0.,
                shared_default_income: 0.,
            },
            events: vec![],
            shared_intents: vec![],
            income_steps: vec![],
            actions: vec![],
            reserved_bytes: MAX_RESULT_BYTES,
            charged_bytes: 0,
        };
        b.validate()?;
        Ok(b)
    }

    fn staged(&self) -> Self {
        Self {
            review: self.review.clone(),
            snapshot: self.snapshot.clone(),
            events: Vec::with_capacity(4),
            shared_intents: Vec::with_capacity(2),
            income_steps: Vec::with_capacity(1),
            actions: vec![],
            reserved_bytes: self.reserved_bytes,
            charged_bytes: self.charged_bytes,
        }
    }
    fn commit(&mut self, mut candidate: Self) -> Result<(), String> {
        candidate.validate()?;
        if self.events.len() + candidate.events.len() > MAX_EVENTS
            || self.actions.len() >= MAX_EVENTS
        {
            return Err("card v2 atomic arena limit".into());
        }
        let payload = bounded(
            &(
                &candidate.snapshot,
                &candidate.events,
                &candidate.shared_intents,
                &candidate.income_steps,
            ),
            MAX_RESULT_BYTES,
            &mut || Ok(()),
        )?;
        let charge = add(self.charged_bytes, mul(payload, 2)?)?;
        if charge > self.reserved_bytes {
            return Err("card v2 staged retained-byte budget".into());
        }
        let action = AtomicAction {
            atomic_action: candidate.action_count,
            ordinal: candidate.last_ordinal,
            events: IndexRange {
                start: self.events.len(),
                end: self.events.len() + candidate.events.len(),
            },
            shared_intents: IndexRange {
                start: self.shared_intents.len(),
                end: self.shared_intents.len() + candidate.shared_intents.len(),
            },
            income_steps: IndexRange {
                start: self.income_steps.len(),
                end: self.income_steps.len() + candidate.income_steps.len(),
            },
            source: SourceLineage {
                kind: String::new(),
                recognition_indices: vec![],
                cash_event_id: None,
                interest_recovery_index: None,
            },
            committed: candidate.snapshot.clone(),
        };
        self.events.append(&mut candidate.events);
        self.shared_intents.append(&mut candidate.shared_intents);
        self.income_steps.append(&mut candidate.income_steps);
        self.actions.push(action);
        self.snapshot = candidate.snapshot;
        self.charged_bytes = charge;
        Ok(())
    }
    fn lineage(
        &mut self,
        kind: &str,
        recognition_indices: Vec<usize>,
        cash_event_id: Option<String>,
        interest_recovery_index: Option<usize>,
    ) {
        self.actions.last_mut().expect("committed action").source = SourceLineage {
            kind: kind.into(),
            recognition_indices,
            cash_event_id,
            interest_recovery_index,
        };
    }
    fn clock(&mut self, d: i32) -> Result<(), String> {
        validate_ordinal(d)?;
        if d < self.last_ordinal || d - self.review.acquisition_ordinal > 64 {
            return Err("actual chronological rights dates required".into());
        }
        self.action_count = self
            .action_count
            .checked_add(1)
            .ok_or("action index overflow")?;
        self.last_ordinal = d;
        Ok(())
    }
    fn emit(
        &mut self,
        d: i32,
        name: &str,
        claim: Option<String>,
        v: Vec<(&str, f64)>,
        basis: f64,
    ) -> Result<(), String> {
        let mut lines = Vec::new();
        let mut net = 0.;
        for (k, x) in v {
            money(x)?;
            net = money(net + x)?;
            if x != 0. {
                lines.push(Line {
                    account: OwnedAccount::named(k)?,
                    amount: x,
                });
            }
        }
        if !near(net, 0.) {
            return Err("unbalanced rights transaction".into());
        }
        if self.events.len() >= MAX_EVENTS {
            return Err("rights event retention exceeded".into());
        }
        self.events.push(Event {
            atomic_action: self.action_count,
            ordinal: d,
            event: name.into(),
            claim_id: claim,
            review: self.review.identity.clone(),
            source: self.review.economic_source.clone(),
            original_yield_anchor: self.review.original_yield_anchor.clone(),
            lines,
            original_eir_income: 0.,
            reviewed_basis_income: basis,
        });
        self.reviewed_basis_income = money(self.reviewed_basis_income + basis)?;
        Ok(())
    }
    fn credit_request(&self, d: i32) -> CreditAccountingRequest {
        CreditAccountingRequest {
            schema: credit::SCHEMA.into(),
            measure: CashflowMeasure::Physical,
            currency: self.review.currency.clone(),
            monetary_unit: "currency_units".into(),
            position_id: self.review.position_id.clone(),
            recognition_ordinal: d,
            reporting_end_ordinal: d,
            policy: self.review.policy.clone(),
            opening: self.core.clone(),
            defaults: vec![],
            chargeoffs: vec![],
            allowance_review: None,
        }
    }
    fn total_carrying(&self) -> f64 {
        self.snapshot.total_carrying()
    }
    fn complete(&self) -> bool {
        self.snapshot.complete()
    }
    pub fn validate(&self) -> Result<(), String> {
        credit::evaluate(&self.credit_request(self.last_ordinal))?;
        money(self.live_interest_basis)?;
        let a = self.core.live[0].accrued_interest;
        if a < 0. || a + self.live_interest_basis < -1e-9 {
            return Err("gross interest plus associated basis is negative".into());
        }
        for (k, b) in &self.claim_interest_basis {
            label(k)?;
            money(*b)?;
            let c = self
                .core
                .claims
                .iter()
                .find(|c| &c.claim_id == k)
                .ok_or("orphan interest basis claim")?;
            if c.outstanding_accrued_interest + b < -1e-9 {
                return Err("claim interest carrying is negative".into());
            }
        }
        let expected = self.review.cost - self.cash_received
            + self.original_eir_income
            + self.reviewed_basis_income
            + self.shared_default_income;
        if !near(self.total_carrying(), expected) {
            return Err("rights carrying/cash/income conservation".into());
        }
        Ok(())
    }
    /// One actual-date native income step, then reviewed channel allocation.
    /// The raw source wrapper supplies the earned amount from CardResult.
    pub fn earned(&mut self, d: i32, a: f64) -> Result<(), String> {
        let mut n = self.staged();
        let previous = n.last_ordinal;
        n.clock(d)?;
        money(a)?;
        if a < 0. || !n.core.claims.is_empty() {
            return Err("live source recognition required; original EIR stops at default".into());
        }
        let iid = |i: &Identity| dated_income::Identity {
            id: i.id.clone(),
            revision: i.revision.clone(),
        };
        let request = DatedIncomeRequest {
            schema: dated_income::SCHEMA.into(),
            contract: iid(&n.review.economic_source),
            original_yield_anchor: iid(&n.review.original_yield_anchor),
            policy: iid(&n.review.identity),
            currency: n.review.currency.clone(),
            monetary_unit: "currency_units".into(),
            measure: CashflowMeasure::Physical,
            as_of_ordinal: previous,
            full_contract_end_ordinal: d,
            opening_principal: n.core.live[0].principal,
            opening_accrued_interest: n.core.live[0].accrued_interest,
            opening_gross_carrying_value: n.total_carrying(),
            original_annual_effective_rate: n.review.original_annual_effective_rate,
            opening_annual_effective_rate: n.review.original_annual_effective_rate,
            events: vec![IncomeEvent {
                event_id: format!("original-rights-yield:{}:{}", d, n.action_count),
                source: iid(&n.review.economic_source),
                payment_ordinal: d,
                contractual_interest_earned: a,
                principal_cash: 0.,
                interest_cash: 0.,
                integral_fee_cash: 0.,
                nonintegral_fee_cash: 0.,
                yield_change: None,
            }],
        };
        let result = dated_income::evaluate(&request)?;
        let e = result.rows[0].effective_interest;
        let amort = result.rows[0].basis_amortization;
        let principal_channel = n.core.live[0].principal > 0.;
        if !principal_channel
            && (n.core.live[0].book_adjustment != 0.
                || n.core.live[0].accrued_interest <= 0. && a > 0.)
        {
            return Err("principal-free EIR requires an existing gross interest right and reviewed transfer".into());
        }
        n.core.live[0].accrued_interest = money(n.core.live[0].accrued_interest + a)?;
        let channel = if principal_channel {
            n.core.live[0].book_adjustment = money(n.core.live[0].book_adjustment + amort)?;
            "live_book_adjustment"
        } else {
            n.live_interest_basis = money(n.live_interest_basis + amort)?;
            "live_interest_basis"
        };
        n.original_eir_income = money(n.original_eir_income + e)?;
        n.emit(
            d,
            "native_original_eir_and_contractual_accrual",
            None,
            vec![
                ("live_accrued_interest", a),
                (channel, amort),
                ("pnl:original_effective_interest", -e),
            ],
            0.,
        )?;
        n.events.last_mut().unwrap().original_eir_income = e;
        n.income_steps.push(IncomeStep {
            channel: channel.into(),
            request,
            result,
        });
        n.validate()?;
        self.commit(n)?;
        Ok(())
    }
    /// Native terminal principal event; never a guessed payoff date.
    pub fn terminal_principal_cash(&mut self, d: i32, p: f64) -> Result<(), String> {
        let mut n = self.staged();
        n.clock(d)?;
        money(p)?;
        if p <= 0. || p != n.core.live[0].principal || !n.core.claims.is_empty() {
            return Err("exact terminal source principal payment required".into());
        }
        let b = n.core.live[0].book_adjustment;
        let a = n.core.live[0].accrued_interest;
        if a <= 0. && b != 0. {
            return Err("basis cannot attach to nonexistent interest right".into());
        }
        if a + n.live_interest_basis + b < 0. {
            return Err("reviewed transfer would make interest carrying negative".into());
        }
        n.core.live[0].book_adjustment = 0.;
        n.live_interest_basis = money(n.live_interest_basis + b)?;
        n.emit(
            d,
            "reviewed_principal_basis_to_interest_right",
            None,
            vec![("live_book_adjustment", -b), ("live_interest_basis", b)],
            0.,
        )?;
        n.core.live[0].principal = 0.;
        n.cash_received = money(n.cash_received + p)?;
        n.emit(
            d,
            "actual_terminal_principal_cash",
            None,
            vec![("cash", p), ("live_principal", -p)],
            0.,
        )?;
        n.validate()?;
        self.commit(n)?;
        Ok(())
    }
    pub fn live_interest_cash(&mut self, d: i32, q: f64) -> Result<(), String> {
        let mut n = self.staged();
        n.clock(d)?;
        money(q)?;
        let a = n.core.live[0].accrued_interest;
        if q <= 0. || q > a || a <= 0. {
            return Err("actual cash exceeds live gross right".into());
        }
        let b = if q == a {
            n.live_interest_basis
        } else {
            n.live_interest_basis * (q / a)
        };
        n.core.live[0].accrued_interest -= q;
        n.live_interest_basis -= b;
        n.cash_received = money(n.cash_received + q)?;
        n.emit(
            d,
            "reviewed_live_interest_cash_and_basis_realization",
            None,
            vec![
                ("cash", q),
                ("live_accrued_interest", -q),
                ("live_interest_basis", -b),
                ("pnl:reviewed_basis_realization", b),
            ],
            -b,
        )?;
        n.validate()?;
        self.commit(n)?;
        Ok(())
    }
    pub fn interest_only_default(&mut self, d: i32, claim_id: &str) -> Result<(), String> {
        let mut n = self.staged();
        n.clock(d)?;
        label(claim_id)?;
        let s = n.core.live[0];
        if s.principal != 0.
            || s.book_adjustment != 0.
            || s.accrued_interest <= 0.
            || !n.core.claims.is_empty()
        {
            return Err("interest-only actual default boundary required".into());
        }
        let transfer = n.review.policy.accrued == AccruedPolicy::TransferUntilReviewedReversal;
        let paired = if transfer {
            n.review.policy.basis == BasisPolicy::TransferUntilReviewedChargeoff
        } else {
            n.review.policy.basis == BasisPolicy::RecognizeInEarningsAtDefault
        };
        if !paired {
            return Err("paired gross/contra default policy required".into());
        }
        let mut r = n.credit_request(d);
        r.defaults.push(DefaultInstruction {
            claim_id: claim_id.into(),
            economic_source: n.review.economic_source.clone(),
            recovery_schedule_review: n.review.identity.clone(),
            by_origin: [
                OriginDefaultDelta {
                    principal: 0.,
                    undrawn: 0.,
                    contingent_draw: 0.,
                    accrued_interest: s.accrued_interest,
                    book_adjustment: 0.,
                },
                OriginDefaultDelta::default(),
                OriginDefaultDelta::default(),
            ],
            economic_principal_loss: 0.,
            complete_recovery_tail: vec![],
        });
        let shared = credit::evaluate(&r)?;
        n.core = shared.closing;
        for i in shared.intents {
            n.shared_intents.push(i.clone());
            let mut v = Vec::new();
            for p in &i.postings {
                let k = serde_json::to_value(p.account).map_err(|e| e.to_string())?;
                v.push((k.as_str().ok_or("account code wire")?.to_owned(), p.amount));
                if matches!(
                    p.account,
                    AccountCode::InterestIncome
                        | AccountCode::CreditLossExpense
                        | AccountCode::BasisRecognitionExpense
                        | AccountCode::RecoveryIncome
                ) {
                    n.shared_default_income = money(n.shared_default_income - p.amount)?;
                }
            }
            n.emit(
                d,
                &i.event,
                i.claim_id.clone(),
                v.iter().map(|(k, v)| (k.as_str(), *v)).collect(),
                0.,
            )?;
        }
        let b = n.live_interest_basis;
        n.live_interest_basis = 0.;
        if transfer {
            n.claim_interest_basis.insert(claim_id.into(), b);
            n.emit(
                d,
                "reviewed_default_interest_basis_transfer",
                Some(claim_id.into()),
                vec![("live_interest_basis", -b), ("default_interest_basis", b)],
                0.,
            )?;
        } else {
            n.emit(
                d,
                "reviewed_default_interest_basis_realization",
                Some(claim_id.into()),
                vec![
                    ("live_interest_basis", -b),
                    ("pnl:reviewed_default_basis_realization", b),
                ],
                -b,
            )?;
        }
        n.validate()?;
        self.commit(n)?;
        Ok(())
    }
    pub fn claim_interest_cash(&mut self, s: ReviewedInterestRecovery) -> Result<(), String> {
        let mut n = self.staged();
        n.clock(s.recognition_ordinal)?;
        id(&s.recovery)?;
        id(&s.policy_review)?;
        id(&s.source)?;
        let c = n
            .core
            .claims
            .iter()
            .find(|c| c.claim_id == s.claim_id)
            .ok_or("actual interest claim missing")?;
        let a = c.outstanding_accrued_interest;
        let ib = *n
            .claim_interest_basis
            .get(&s.claim_id)
            .ok_or("paired claim basis missing")?;
        if s.amount <= 0. || s.amount > a {
            return Err("reviewed interest collection exceeds gross claim".into());
        }
        let released = if s.amount == a { ib } else { ib * s.amount / a };
        let day = s.recognition_ordinal;
        let claim = s.claim_id.clone();
        let amount = s.amount;
        let shared = credit::settle_interest_recoveries(&InterestRecoveryRequest {
            schema: "credit-interest-recovery-1".into(),
            measure: CashflowMeasure::Physical,
            currency: n.review.currency.clone(),
            monetary_unit: "currency_units".into(),
            position_id: n.review.position_id.clone(),
            recognition_ordinal: day,
            reporting_end_ordinal: day,
            policy: n.review.policy.clone(),
            opening: n.core.clone(),
            settlements: vec![s],
        })?;
        n.core = shared.closing;
        n.cash_received = money(n.cash_received + amount)?;
        for i in shared.intents {
            n.shared_intents.push(i.clone());
            let mut v = Vec::new();
            for p in &i.postings {
                let k = serde_json::to_value(p.account).map_err(|e| e.to_string())?;
                v.push((k.as_str().ok_or("account code wire")?.to_owned(), p.amount));
            }
            n.emit(
                day,
                &i.event,
                i.claim_id.clone(),
                v.iter().map(|(k, v)| (k.as_str(), *v)).collect(),
                0.,
            )?;
        }
        n.claim_interest_basis.insert(claim.clone(), ib - released);
        n.emit(
            day,
            "reviewed_recovery_interest_basis_realization",
            Some(claim),
            vec![
                ("default_interest_basis", -released),
                ("pnl:reviewed_recovery_basis_realization", released),
            ],
            -released,
        )?;
        n.validate()?;
        self.commit(n)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RightsPolicy {
    pub review: Identity,
    pub principal_basis: String,
    pub live_original_yield: String,
    pub interest_cash_basis: String,
    pub default_original_yield: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub contract: CardRequest,
    pub review: Review,
    pub policy: RightsPolicy,
    #[serde(deserialize_with = "crate::card_credit_accounting::strict_termination::deserialize")]
    pub termination: Termination,
    pub claim_id: String,
    pub reporting_ordinal: i32,
    pub interest_recoveries: Vec<ReviewedInterestRecovery>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Daily {
    pub ordinal: i32,
    pub snapshot: Snapshot,
    pub actions: IndexRange,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub economic: CardResult,
    pub acquisition_opening: Snapshot,
    pub actions: Vec<AtomicAction>,
    pub events: Vec<Event>,
    pub shared_intents: Vec<JournalIntent>,
    pub income_steps: Vec<IncomeStep>,
    pub daily: Vec<Daily>,
    pub reporting: Snapshot,
    pub closing: Snapshot,
    pub complete: bool,
    pub admitted_work: usize,
    pub reserved_bytes: usize,
    pub retained_capacity_charge: usize,
    pub mixed_ledger_adoption: bool,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub full_end_ordinal: i32,
    pub raw_bytes: usize,
    pub reserved_bytes: usize,
    pub work: usize,
    pub max_actions: usize,
    pub max_events: usize,
    pub max_income_steps: usize,
}
fn bounded(
    v: &impl Serialize,
    limit: usize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<usize, String> {
    struct Count<'a>(usize, usize, &'a mut dyn FnMut() -> Result<(), String>);
    impl std::io::Write for Count<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            (self.2)().map_err(std::io::Error::other)?;
            self.0 = self
                .0
                .checked_add(b.len())
                .filter(|n| *n <= self.1)
                .ok_or_else(|| std::io::Error::other("card v2 byte limit"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0, limit, check);
    serde_json::to_writer(&mut count, v).map_err(|e| e.to_string())?;
    Ok(count.0)
}

pub fn audit_request(r: &Request) -> Result<Admission, String> {
    audit_request_with_check(r, &mut || Ok(()))
}
pub fn audit_request_with_check(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    check()?;
    let raw_bytes = bounded(r, MAX_RAW_BYTES, check)?;
    consumer_card::validate_request(&r.contract)?;
    let c = &r.contract;
    let end = c
        .as_of_ordinal
        .checked_add(c.horizon_days as i32)
        .ok_or("proof date overflow")?;
    if r.schema != SCHEMA
        || c.horizon_days == 0
        || c.horizon_days > MAX_DAYS
        || add(c.steps.len(), c.cycles.len())? > MAX_SOURCE_ACTIONS
        || r.interest_recoveries.len() > MAX_COLLECTIONS
        || c.identity.measure != CashflowMeasure::Physical
        || c.initial_state != ConsumerState::Performing
        || c.identity.position_id != r.review.position_id
        || c.identity.currency != r.review.currency
        || c.identity.source_revision != r.review.economic_source.revision
        || c.as_of_ordinal != r.review.acquisition_ordinal
        || c.opening_principal.iter().sum::<f64>() != r.review.principal
        || c.initial_purchase_grace
        || c.regain_grace_after_full_statement_payment
        || c.opening_contingent_purchase_interest != 0.
        || c.opening_billed_interest
            .iter()
            .chain(&c.opening_unbilled_interest)
            .chain(&c.opening_unrecognized_billed_interest)
            .chain(&c.opening_fees)
            .any(|x| *x != 0.)
        || r.reporting_ordinal < c.as_of_ordinal
        || r.reporting_ordinal > end
        || r.policy.principal_basis != "actual_terminal_principal_transfer_to_unpaid_interest-1"
        || r.policy.live_original_yield != "frozen_original_yield_on_whole_live_carrying-1"
        || r.policy.interest_cash_basis
            != "proportional_basis_realization_separate_from_original_eir-1"
        || r.policy.default_original_yield != "stop_at_source_default_no_claim_eir-1"
    {
        return Err(
            "explicit bounded fee-free physical raw card basis-yield proof required".into(),
        );
    }
    id(&r.policy.review)?;
    label(&r.claim_id)?;
    let closure = match &r.termination {
        Termination::SourceDefault => None,
        Termination::ReviewedZeroRightsClosure { ordinal, review } => {
            id(review)?;
            if *ordinal <= c.as_of_ordinal || *ordinal > end {
                return Err("reviewed closure outside proof".into());
            }
            Some(*ordinal)
        }
    };
    let mut limits = 0;
    let mut default_seen = false;
    for step in &c.steps {
        check()?;
        if step.ordinal <= c.as_of_ordinal || closure.is_some_and(|d| step.ordinal > d) {
            return Err("source action outside reviewed proof".into());
        }
        if default_seen {
            return Err("v2 raw actions after source default are unsupported".into());
        }
        match step.action {
            CardAction::Payment { .. } => {}
            CardAction::Default {} if closure.is_none() => {
                default_seen = true;
            }
            CardAction::SetLimit { amount: 0. } if closure == Some(step.ordinal) => limits += 1,
            _ => return Err("unsupported fee/grace/draw/rate/limit ownership".into()),
        }
    }
    if closure.is_none() && !default_seen {
        return Err("v2 raw source-default action required".into());
    }
    if closure.is_some() && limits != 1 {
        return Err("reviewed closure requires one actual zero-limit action".into());
    }

    Book::new(r.review.clone())?;
    let paired = (r.review.policy.accrued == AccruedPolicy::TransferUntilReviewedReversal
        && r.review.policy.basis == BasisPolicy::TransferUntilReviewedChargeoff)
        || (r.review.policy.accrued == AccruedPolicy::ReverseIncomeAtDefault
            && r.review.policy.basis == BasisPolicy::RecognizeInEarningsAtDefault);
    if !paired {
        return Err("paired gross/contra default policy required".into());
    }
    if c.collection_fraction != 0. || c.collection_cost != 0. {
        return Err("v2 principal collection/cost ownership unsupported".into());
    }
    let mut seen = HashSet::new();
    for x in &r.interest_recoveries {
        check()?;
        id(&x.recovery)?;
        id(&x.policy_review)?;
        id(&x.source)?;
        label(&x.claim_id)?;
        money(x.amount)?;
        if x.amount <= 0.
            || x.claim_id != r.claim_id
            || x.recognition_ordinal <= c.as_of_ordinal
            || x.recognition_ordinal > end
            || !seen.insert((&x.recovery.id, &x.recovery.revision))
        {
            return Err("actual unique bounded interest collections required".into());
        }
    }
    let days = c.horizon_days as usize;
    let steps = c.steps.len();
    let collections = r.interest_recoveries.len();
    let max_actions = sum(&[days, mul(2, steps)?, 1, collections])?;
    let max_events = sum(&[days, mul(3, steps)?, 2, mul(2, collections)?])?;
    if max_actions > MAX_EVENTS || max_events > MAX_EVENTS {
        return Err("card v2 event admission exceeded".into());
    }
    // Fixed-size income steps; one default claim <=16 settled IDs. Escaped raw identity bound charges copied identities in every retained item.
    let identity_bytes = add(
        bounded(&r.review, MAX_RAW_BYTES, check)?,
        bounded(&r.policy, MAX_RAW_BYTES, check)?,
    )?;
    let reserved_bytes = sum(&[
        mul(raw_bytes, 2)?,
        mul(add(days, 1)?, add(8192, identity_bytes)?)?,
        mul(max_events, add(4096, mul(identity_bytes, 3)?)?)?,
        mul(max_actions, add(8192, mul(identity_bytes, 2)?)?)?,
        mul(days, add(16384, mul(identity_bytes, 4)?)?)?,
        512 * 1024,
    ])?;
    if reserved_bytes > MAX_RESULT_BYTES {
        return Err("card v2 retained byte reservation exceeded".into());
    }
    let work = sum(&[
        mul(add(days, 1)?, sum(&[steps, c.cycles.len(), 128])?)?,
        mul(max_events, 128)?,
        mul(collections, 1024)?,
    ])?;
    if work > MAX_WORK {
        return Err("card v2 work admission exceeded".into());
    }
    check()?;
    Ok(Admission {
        full_end_ordinal: end,
        raw_bytes,
        reserved_bytes,
        work,
        max_actions,
        max_events,
        max_income_steps: days,
    })
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let admission = audit_request_with_check(r, &mut check)?;
    let c = &r.contract;
    let end = admission.full_end_ordinal;
    let closure = match r.termination {
        Termination::ReviewedZeroRightsClosure { ordinal, .. } => Some(ordinal),
        Termination::SourceDefault => None,
    };
    let mut book = Book::new(r.review.clone())?;
    book.events
        .try_reserve_exact(admission.max_events)
        .map_err(|_| "v2 event allocation")?;
    book.actions
        .try_reserve_exact(admission.max_actions)
        .map_err(|_| "v2 action allocation")?;
    book.shared_intents
        .try_reserve_exact(admission.max_events)
        .map_err(|_| "v2 intent allocation")?;
    book.income_steps
        .try_reserve_exact(admission.max_income_steps)
        .map_err(|_| "v2 income allocation")?;
    book.reserved_bytes = admission.reserved_bytes;
    book.charged_bytes = sum(&[
        admission.raw_bytes,
        512 * 1024,
        vec_capacity(&book.events)?,
        vec_capacity(&book.actions)?,
        vec_capacity(&book.shared_intents)?,
        vec_capacity(&book.income_steps)?,
    ])?;
    if book.charged_bytes > book.reserved_bytes {
        return Err("v2 reserved arena capacities exceed admission".into());
    }
    let opening = book.snapshot.clone();
    let mut daily = Vec::new();
    daily
        .try_reserve_exact(c.horizon_days as usize + 1)
        .map_err(|_| "v2 snapshot allocation")?;
    daily.push(Daily {
        ordinal: c.as_of_ordinal,
        snapshot: book.snapshot.clone(),
        actions: IndexRange { start: 0, end: 0 },
    });
    check()?;
    let economic = consumer_card::simulate(c)?;
    check()?;
    if economic.drawn_principal != 0.
        || economic
            .cashflows
            .iter()
            .any(|v| v.fees != 0. || v.principal < 0. || v.interest < 0.)
    {
        return Err("unsupported native source cash ownership".into());
    }
    match (&r.termination, &economic.default) {
        (Termination::SourceDefault, Some(d)) if d.principal == 0. => {}
        (Termination::ReviewedZeroRightsClosure { .. }, None) => {}
        _ => {
            return Err(
                "proof requires interest-only source default or explicit live closure".into(),
            )
        }
    }
    let mut recognition = BTreeMap::<i32, Vec<usize>>::new();
    for (i, v) in economic.recognition.iter().enumerate() {
        if v.end_ordinal - v.start_ordinal != 1 {
            return Err("actual daily native recognition required".into());
        }
        recognition.entry(v.end_ordinal).or_default().push(i);
    }
    let mut cash = BTreeMap::<i32, Vec<usize>>::new();
    for (i, v) in economic.cashflows.iter().enumerate() {
        cash.entry(v.ordinal).or_default().push(i);
    }
    let mut recoveries = BTreeMap::<i32, Vec<usize>>::new();
    for (i, v) in r.interest_recoveries.iter().enumerate() {
        if v.recognition_ordinal < c.as_of_ordinal
            || v.recognition_ordinal > end
            || v.claim_id != r.claim_id
        {
            return Err("actual claim collection outside complete proof".into());
        }
        recoveries.entry(v.recognition_ordinal).or_default().push(i);
    }
    let mut stopped = false;
    for d in c.as_of_ordinal + 1..=end {
        check()?;
        let first_action = book.actions.len();
        if !stopped {
            let indices = recognition.get(&d).cloned().unwrap_or_default();
            let a = indices
                .iter()
                .map(|i| economic.recognition[*i].interest)
                .sum::<f64>();
            check()?;
            book.earned(d, a)?;
            check()?;
            book.lineage("native_recognition_and_original_yield", indices, None, None);
        }
        for &i in cash.get(&d).into_iter().flatten() {
            let v = &economic.cashflows[i];
            if stopped {
                return Err("native post-exit cash requires separate reviewed ownership".into());
            }
            // Source payments allocate interest before principal. Basis is only
            // attached to an existing unpaid right at actual terminal principal.
            if v.interest != 0. {
                check()?;
                book.live_interest_cash(d, v.interest)?;
                check()?;
                book.lineage(
                    "native_interest_cash",
                    vec![],
                    Some(v.event_id.clone()),
                    None,
                );
            }
            if v.principal != 0. {
                check()?;
                book.terminal_principal_cash(d, v.principal)?;
                check()?;
                book.lineage(
                    "native_terminal_principal_cash",
                    vec![],
                    Some(v.event_id.clone()),
                    None,
                );
            }
        }
        if let Some(default) = economic.default.as_ref().filter(|v| v.ordinal == d) {
            if !near(book.core.live[0].accrued_interest, default.accrued_interest) {
                return Err("source default accrued mismatch".into());
            }
            check()?;
            book.interest_only_default(d, &r.claim_id)?;
            check()?;
            stopped = true;
            book.lineage("native_interest_only_default", vec![], None, None);
        }
        for &i in recoveries.get(&d).into_iter().flatten() {
            check()?;
            book.claim_interest_cash(r.interest_recoveries[i].clone())?;
            check()?;
            book.lineage("reviewed_actual_interest_recovery", vec![], None, Some(i));
        }
        if closure == Some(d) {
            let point = &economic.points[(d - c.as_of_ordinal) as usize];
            if !book.complete()
                || point
                    .principal
                    .iter()
                    .chain(&point.billed_interest)
                    .chain(&point.unbilled_interest)
                    .chain(&point.fees)
                    .any(|x| *x != 0.)
                || point.limit != 0.
                || point.undrawn != 0.
            {
                return Err(
                    "actual reviewed closure still has gross source rights or commitment".into(),
                );
            }
            stopped = true;
        }
        if !stopped {
            let point = &economic.points[(d - c.as_of_ordinal) as usize];
            if !near(book.core.live[0].principal, point.principal.iter().sum())
                || !near(
                    book.core.live[0].accrued_interest,
                    point
                        .billed_interest
                        .iter()
                        .chain(&point.unbilled_interest)
                        .sum(),
                )
            {
                return Err("native principal or gross contractual right mismatch".into());
            }
        }
        check()?;
        book.charged_bytes = add(
            book.charged_bytes,
            bounded(&book.snapshot, MAX_RESULT_BYTES, &mut check)?,
        )?;
        if book.charged_bytes > book.reserved_bytes {
            return Err("v2 daily retained-byte budget".into());
        }
        daily.push(Daily {
            ordinal: d,
            snapshot: book.snapshot.clone(),
            actions: IndexRange {
                start: first_action,
                end: book.actions.len(),
            },
        });
    }
    let reporting = daily[(r.reporting_ordinal - c.as_of_ordinal) as usize]
        .snapshot
        .clone();
    let complete = book.complete();
    let mut out = ResultData {
        schema: "card-credit-accounting-result-2",
        raw_request: r.clone(),
        economic,
        acquisition_opening: opening,
        actions: book.actions,
        events: book.events,
        shared_intents: book.shared_intents,
        income_steps: book.income_steps,
        daily,
        reporting,
        closing: book.snapshot,
        complete,
        admitted_work: admission.work,
        reserved_bytes: admission.reserved_bytes,
        // Maximum digit width charges the charge field itself conservatively.
        retained_capacity_charge: MAX_RESULT_BYTES,
        mixed_ledger_adoption: false,
        accounting_standard_approval: false,
        empirical_validation: false,
    };
    let escaped_bytes = bounded(&out, admission.reserved_bytes, &mut check)?;
    out.retained_capacity_charge = add(escaped_bytes, retained_capacities(&out)?)?;
    if out.retained_capacity_charge > admission.reserved_bytes {
        return Err("v2 actual retained capacity/escaped byte charge exceeded reservation".into());
    }
    bounded(&out, admission.reserved_bytes, &mut check)?;
    check()?;
    Ok(out)
}

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "card v2 capacity/work overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "card v2 capacity/work overflow".into())
}
fn sum(v: &[usize]) -> Result<usize, String> {
    v.iter().try_fold(0, |a, b| add(a, *b))
}
fn vec_capacity<T>(v: &Vec<T>) -> Result<usize, String> {
    mul(v.capacity(), std::mem::size_of::<T>())
}
fn snapshot_capacity(s: &Snapshot) -> Result<usize, String> {
    let mut n = add(
        vec_capacity(&s.core.claims)?,
        mul(s.claim_interest_basis.len(), 256)?,
    )?;
    for c in &s.core.claims {
        n = sum(&[
            n,
            vec_capacity(&c.remaining_recoveries)?,
            vec_capacity(&c.settled_interest_recovery_ids)?,
        ])?;
    }
    Ok(n)
}
/// Actual vector capacities + conservative map-node capacity. Escaped full wire
/// additionally charges all cloned string payloads and scalar/identity representation.
fn retained_capacities(r: &ResultData) -> Result<usize, String> {
    let mut n = sum(&[
        vec_capacity(&r.events)?,
        vec_capacity(&r.actions)?,
        vec_capacity(&r.daily)?,
        vec_capacity(&r.shared_intents)?,
        vec_capacity(&r.income_steps)?,
        vec_capacity(&r.economic.points)?,
        vec_capacity(&r.economic.statements)?,
        vec_capacity(&r.economic.recognition)?,
        vec_capacity(&r.economic.cashflows)?,
        vec_capacity(&r.raw_request.contract.steps)?,
        vec_capacity(&r.raw_request.contract.cycles)?,
        vec_capacity(&r.raw_request.interest_recoveries)?,
        snapshot_capacity(&r.acquisition_opening)?,
        snapshot_capacity(&r.reporting)?,
        snapshot_capacity(&r.closing)?,
    ])?;
    for a in &r.actions {
        n = sum(&[
            n,
            snapshot_capacity(&a.committed)?,
            vec_capacity(&a.source.recognition_indices)?,
        ])?;
    }
    for d in &r.daily {
        n = add(n, snapshot_capacity(&d.snapshot)?)?;
    }
    for e in &r.events {
        n = add(n, vec_capacity(&e.lines)?)?;
    }
    for i in &r.shared_intents {
        n = add(n, vec_capacity(&i.postings)?)?;
    }
    for i in &r.income_steps {
        n = sum(&[
            n,
            vec_capacity(&i.request.events)?,
            vec_capacity(&i.result.rows)?,
        ])?;
    }
    Ok(n)
}

impl Snapshot {
    pub fn total_carrying(&self) -> f64 {
        self.core
            .live
            .iter()
            .map(|s| s.principal + s.accrued_interest + s.book_adjustment)
            .sum::<f64>()
            + self.live_interest_basis
            + self
                .core
                .claims
                .iter()
                .map(|c| {
                    c.outstanding_principal
                        + c.outstanding_accrued_interest
                        + c.outstanding_book_adjustment
                })
                .sum::<f64>()
            + self.claim_interest_basis.values().sum::<f64>()
    }
    pub fn complete(&self) -> bool {
        near(self.core.allowance, 0.)
            && self.core.live.iter().all(|s| {
                near(s.undrawn, 0.)
                    && near(s.principal, 0.)
                    && near(s.accrued_interest, 0.)
                    && near(s.book_adjustment, 0.)
            })
            && near(self.live_interest_basis, 0.)
            && self.core.claims.iter().all(|c| {
                c.remaining_recoveries.is_empty()
                    && near(c.outstanding_principal, 0.)
                    && near(c.outstanding_accrued_interest, 0.)
                    && near(c.outstanding_book_adjustment, 0.)
            })
            && self.claim_interest_basis.values().all(|v| near(*v, 0.))
    }
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    fn raw(i: usize) -> Request {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/card_credit_v2_proof.json"))
                .unwrap();
        serde_json::from_value(f["cases"][i]["raw"].clone()).unwrap()
    }
    fn receipt(b: &Book) -> serde_json::Value {
        serde_json::json!({"snapshot":b.snapshot,"events":b.events,"actions":b.actions,"intents":b.shared_intents,"income":b.income_steps,"charged":b.charged_bytes})
    }
    #[test]
    fn rejected_partial_principal_preserves_all_state_and_arena_cursors() {
        let r = raw(0);
        let mut b = Book::new(r.review).unwrap();
        b.earned(b.last_ordinal + 1, 0.1).unwrap();
        let before = receipt(&b);
        assert!(b.terminal_principal_cash(b.last_ordinal, 50.).is_err());
        assert_eq!(receipt(&b), before);
    }
    #[test]
    fn negative_transfer_preserves_both_channels_and_earnings() {
        let r = raw(12);
        let mut b = Book::new(r.review).unwrap();
        for _ in 0..10 {
            b.earned(b.last_ordinal + 1, 0.1).unwrap();
        }
        let before = receipt(&b);
        assert!(b.terminal_principal_cash(b.last_ordinal, 100.).is_err());
        assert_eq!(receipt(&b), before);
    }
    #[test]
    fn rounded_claim_cash_preserves_claim_basis_and_credit_intents() {
        let r = raw(16);
        let out = evaluate(&r).unwrap_err();
        assert!(out.contains("exceeds gross claim"));
        let mut b = Book::new(r.review).unwrap();
        for _ in 0..10 {
            b.earned(b.last_ordinal + 1, 0.1).unwrap();
        }
        b.terminal_principal_cash(b.last_ordinal, 100.).unwrap();
        for _ in 0..10 {
            b.earned(b.last_ordinal + 1, 0.).unwrap();
        }
        b.interest_only_default(b.last_ordinal, "interest-claim")
            .unwrap();
        let before = receipt(&b);
        assert!(b
            .claim_interest_cash(r.interest_recoveries[0].clone())
            .is_err());
        assert_eq!(receipt(&b), before);
    }
    #[test]
    fn event_capacity_failure_is_atomic() {
        let r = raw(0);
        let mut b = Book::new(r.review).unwrap();
        b.earned(b.last_ordinal + 1, 0.1).unwrap();
        let event = b.events[0].clone();
        b.events.resize(MAX_EVENTS, event);
        let before = receipt(&b);
        assert!(b.earned(b.last_ordinal + 1, 0.1).is_err());
        assert_eq!(receipt(&b), before);
    }
    #[test]
    fn arithmetic_overflow_and_retention_failure_are_atomic() {
        assert!(add(usize::MAX, 1).is_err());
        assert!(mul(usize::MAX, 2).is_err());
        let r = raw(0);
        let mut b = Book::new(r.review).unwrap();
        b.reserved_bytes = 1;
        let before = receipt(&b);
        assert!(b.earned(b.last_ordinal + 1, 0.1).is_err());
        assert_eq!(receipt(&b), before);
    }
}
