//! Reviewed service fees with their own gross rights and complete cash tail.
//! No loan principal, yield, coupon recovery fraction or modeled LGD enters here.
use crate::cashflow::{validate_ordinal, CashflowMeasure, MAX_HORIZON_DAYS};
use crate::credit_accounting::Identity;
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "nonintegral-fee-accounting-1";
pub const MAX_INPUT: usize = 2 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;
pub const MAX_FEES: usize = 2048;
pub const MAX_EVENTS: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fee {
    pub fee_id: String,
    pub source: Identity,
    pub classification_review: Identity,
    pub service: Identity,
    pub position_id: String,
    pub tranche_id: String,
    /// Receipts/fee income positive; payments/fee expense negative.
    pub entitlement: f64,
    pub prepaid_refunds_allowed: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Earn,
    Cash,
    RefundPrepaid,
    WaiveUnpaidEarned,
    CancelUnearned,
    RecoverWaived,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Service,
    Settlement,
    Default,
    Exercise,
    Maturity,
    OtherReviewed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: String,
    pub fee_id: String,
    pub source: Identity,
    pub review: Identity,
    pub ordinal: i32,
    /// Explicit within-day order, unique across the entire source.
    pub sequence: u32,
    pub action: Action,
    pub reason: Reason,
    /// Same sign as the contractual entitlement, including refunds/waivers.
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub policy: Identity,
    pub asof_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub complete_end_ordinal: i32,
    pub fees: Vec<Fee>,
    pub events: Vec<Event>,
}
/// Debit-positive accounts independent of the historical credit GL protocol.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    Cash,
    FeeReceivable,
    FeePayable,
    DeferredFeeIncome,
    PrepaidFeeExpense,
    FeeIncome,
    FeeWriteoffExpense,
    FeeRecoveryIncome,
}
#[derive(Debug, Clone, Serialize)]
pub struct Posting {
    pub account: Account,
    pub amount: f64,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct State {
    pub earned: f64,
    pub cancelled_unearned: f64,
    pub waived_earned: f64,
    pub recovered_waived: f64,
    pub net_service_cash: f64,
    pub fee_receivable: f64,
    pub fee_payable: f64,
    pub deferred_fee_income: f64,
    pub prepaid_fee_expense: f64,
    pub unearned_entitlement: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Closing {
    pub fee_id: String,
    pub source: Identity,
    pub position_id: String,
    pub tranche_id: String,
    pub state: State,
}
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub event: Event,
    pub policy: Identity,
    pub postings: Vec<Posting>,
    pub next: State,
}
/// Sealed computed owner: transport cannot deserialize a caller checkpoint.
#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_request: Request,
    groups: Vec<Group>,
    reporting: Vec<Closing>,
    closing: Vec<Closing>,
    full_source_tail_replayed: bool,
    loan_carrying_value_changed: bool,
    native_credit_event_binding: bool,
    mixed_ledger_adoption: bool,
    pricing_unit_ftp_adoption: bool,
    empirical_validation: bool,
    accounting_approval: bool,
}
impl ResultData {
    pub fn raw_request(&self) -> &Request { &self.raw_request }
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }
    pub fn closing(&self) -> &[Closing] {
        &self.closing
    }
}
fn text(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 128 || s.chars().any(char::is_control) {
        Err("fee identity must be nonempty bounded text".into())
    } else {
        Ok(())
    }
}
fn identity(i: &Identity) -> Result<(), String> {
    text(&i.id)?;
    text(&i.revision)
}
fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err("fee monetary domain exceeded".into())
    }
}
fn refresh(f: &Fee, s: &mut State) -> Result<(), String> {
    let sign = f.entitlement.signum();
    let net = money(s.earned - s.waived_earned - s.net_service_cash)? * sign;
    s.fee_receivable = if sign > 0. { net.max(0.) } else { 0. };
    s.fee_payable = if sign < 0. { -net.max(0.) } else { 0. };
    s.deferred_fee_income = if sign > 0. { net.min(0.) } else { 0. };
    s.prepaid_fee_expense = if sign < 0. { -net.min(0.) } else { 0. };
    s.unearned_entitlement = money(f.entitlement - s.earned - s.cancelled_unearned)?;
    Ok(())
}
fn snapshots(r: &Request, states: &[State]) -> Vec<Closing> {
    r.fees
        .iter()
        .zip(states)
        .map(|(f, state)| Closing {
            fee_id: f.fee_id.clone(),
            source: f.source.clone(),
            position_id: f.position_id.clone(),
            tranche_id: f.tranche_id.clone(),
            state: state.clone(),
        })
        .collect()
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check<F: FnMut() -> Result<(), String>>(
    r: &Request,
    mut check: F,
) -> Result<ResultData, String> {
    check()?;
    if r.schema != SCHEMA || r.measure != CashflowMeasure::Physical {
        return Err("explicit physical nonintegral fee selection required".into());
    }
    text(&r.currency)?;
    text(&r.monetary_unit)?;
    identity(&r.policy)?;
    for d in [
        r.asof_ordinal,
        r.reporting_end_ordinal,
        r.complete_end_ordinal,
    ] {
        validate_ordinal(d)?;
    }
    if r.reporting_end_ordinal < r.asof_ordinal
        || r.complete_end_ordinal < r.reporting_end_ordinal
        || r.complete_end_ordinal - r.asof_ordinal > MAX_HORIZON_DAYS as i32
    {
        return Err("fee source requires bounded complete calendar tail".into());
    }
    if r.fees.is_empty()
        || r.fees.len() > MAX_FEES
        || r.events.len() > MAX_EVENTS
        || r.fees.len() * 2048 + r.events.len() * 4096 > MAX_OUTPUT
    {
        return Err("fee source cardinality/capacity exceeded".into());
    }
    if serde_json::to_vec(r).map_err(|e| e.to_string())?.len() > MAX_INPUT {
        return Err("fee raw source exceeds 2MiB".into());
    }
    let mut indices = HashMap::new();
    let mut states = vec![State::default(); r.fees.len()];
    for (i, f) in r.fees.iter().enumerate() {
        check()?;
        text(&f.fee_id)?;
        text(&f.position_id)?;
        text(&f.tranche_id)?;
        identity(&f.source)?;
        identity(&f.classification_review)?;
        identity(&f.service)?;
        if money(f.entitlement)? == 0. || indices.insert(f.fee_id.as_str(), i).is_some() {
            return Err("fee entitlement must be nonzero with unique fee identity".into());
        }
        refresh(f, &mut states[i])?;
    }
    let mut ids = HashSet::new();
    let mut previous = None;
    let mut groups = Vec::with_capacity(r.events.len());
    let mut reporting = states.clone();
    for e in &r.events {
        check()?;
        text(&e.event_id)?;
        identity(&e.source)?;
        identity(&e.review)?;
        let clock = (e.ordinal, e.sequence);
        if e.ordinal < r.asof_ordinal
            || e.ordinal > r.complete_end_ordinal
            || previous.is_some_and(|p| p >= clock)
            || !ids.insert(&e.event_id)
        {
            return Err("fee events require unique identities and strict actual-date order".into());
        }
        previous = Some(clock);
        let i = *indices
            .get(e.fee_id.as_str())
            .ok_or("unknown fee event owner")?;
        let f = &r.fees[i];
        let old = states[i].clone();
        let s = &mut states[i];
        let sign = f.entitlement.signum();
        let a = money(e.amount)? * sign;
        if a <= 0. || e.source != f.source {
            return Err("fee event sign/source mismatch".into());
        }
        let capacity = match e.action {
            Action::Earn | Action::CancelUnearned => s.unearned_entitlement * sign,
            Action::Cash => {
                (f.entitlement - s.cancelled_unearned - s.waived_earned - s.net_service_cash) * sign
            }
            Action::RefundPrepaid => (s.net_service_cash - s.earned + s.waived_earned) * sign,
            Action::WaiveUnpaidEarned => (s.earned - s.waived_earned - s.net_service_cash) * sign,
            Action::RecoverWaived => (s.waived_earned - s.recovered_waived) * sign,
        };
        // No rounding clamp or tolerance can grant an extra legal entitlement.
        if a > capacity {
            return Err("fee event exceeds its separate reviewed right/obligation".into());
        }
        if e.action == Action::Earn && e.reason != Reason::Service
            || matches!(e.action, Action::Cash | Action::RefundPrepaid)
                && e.reason != Reason::Settlement
            || matches!(
                e.action,
                Action::WaiveUnpaidEarned | Action::CancelUnearned | Action::RecoverWaived
            ) && !matches!(
                e.reason,
                Reason::Default | Reason::Exercise | Reason::Maturity | Reason::OtherReviewed
            )
        {
            return Err(
                "fee action requires its explicit service/settlement/disposition review".into(),
            );
        }
        let mut postings = Vec::new();
        let mut post = |account, amount| {
            if amount != 0. {
                postings.push(Posting { account, amount });
            }
        };
        match e.action {
            Action::Earn => {
                s.earned = money(s.earned + e.amount)?;
                post(Account::FeeIncome, -e.amount);
            }
            Action::Cash => {
                s.net_service_cash = money(s.net_service_cash + e.amount)?;
                post(Account::Cash, e.amount);
            }
            Action::RefundPrepaid => {
                if !f.prepaid_refunds_allowed {
                    return Err("fee refund is not allowed by supplied terms".into());
                }
                s.net_service_cash = money(s.net_service_cash - e.amount)?;
                post(Account::Cash, -e.amount);
            }
            Action::WaiveUnpaidEarned => {
                s.waived_earned = money(s.waived_earned + e.amount)?;
                post(Account::FeeWriteoffExpense, e.amount);
            }
            Action::CancelUnearned => {
                s.cancelled_unearned = money(s.cancelled_unearned + e.amount)?;
            }
            Action::RecoverWaived => {
                s.recovered_waived = money(s.recovered_waived + e.amount)?;
                post(Account::Cash, e.amount);
                post(Account::FeeRecoveryIncome, -e.amount);
            }
        }
        refresh(f, s)?;
        for (account, after, before) in [
            (Account::FeeReceivable, s.fee_receivable, old.fee_receivable),
            (Account::FeePayable, s.fee_payable, old.fee_payable),
            (
                Account::DeferredFeeIncome,
                s.deferred_fee_income,
                old.deferred_fee_income,
            ),
            (
                Account::PrepaidFeeExpense,
                s.prepaid_fee_expense,
                old.prepaid_fee_expense,
            ),
        ] {
            post(account, money(after - before)?);
        }
        let residual: f64 = postings.iter().map(|p| p.amount).sum();
        if residual.abs() > 1e-9_f64.max(64. * f64::EPSILON * a) {
            return Err("unbalanced fee intent".into());
        }
        if e.ordinal <= r.reporting_end_ordinal {
            reporting[i] = s.clone();
        }
        groups.push(Group {
            event: e.clone(),
            policy: r.policy.clone(),
            postings,
            next: s.clone(),
        });
    }
    check()?;
    let result = ResultData {
        schema: SCHEMA,
        raw_request: r.clone(),
        groups,
        reporting: snapshots(r, &reporting),
        closing: snapshots(r, &states),
        full_source_tail_replayed: true,
        loan_carrying_value_changed: false,
        native_credit_event_binding: false,
        mixed_ledger_adoption: false,
        pricing_unit_ftp_adoption: false,
        empirical_validation: false,
        accounting_approval: false,
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_OUTPUT
    {
        return Err("fee output exceeds 32MiB".into());
    }
    check()?;
    Ok(result)
}
