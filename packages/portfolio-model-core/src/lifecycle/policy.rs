//! Stage G (issue #49): long-term business-plan modes for one execution block, and monthly
//! reporting derived from the dated events.
//!
//! * `runoff` executes no actions: only contractual events of the dated source move.
//! * `schedule` executes exactly the supplied instructions (the stage C behavior).
//! * `policy` adds bounded funding decisions to the supplied instructions. A decision reads only
//!   the state at that moment in the chronological replay (settled cash, restricted cash, live
//!   facility balances and the payment about to settle), never a later outcome. A failed or
//!   blocked leg changes that state, so later decisions see it. Facilities are drawn only inside
//!   their declared access windows and before their current expiry; renewal is an explicit,
//!   bounded series of generations, each permitted by a declared renewal window; an unrenewed
//!   balance is repaid at expiry from usable cash (or refused). No perpetual facility, prefunding
//!   by a future receipt or horizon payoff is synthesized.
//!
//! Monthly reports aggregate the dated intents and liquidity steps of every calendar month from
//! the as-of month to the horizon month. They keep the minimum usable cash of any event in the
//! month, so a positive month-end balance cannot hide an intra-month shortfall, and they
//! reconcile exactly to the dated cash movements.
use super::acquisition::LiquidityStep;
use super::journal::{Intent, Kind};
use super::types::{Refusal, Revision, MALFORMED_INPUT, UNSUPPORTED_POLICY};
use crate::cashflows::dated::ymd_from_ordinal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_POLICY_FACILITIES: usize = 16;
pub const MAX_GENERATIONS: u32 = 24;
pub const MAX_RENEWAL_TENOR_DAYS: i32 = 366;
pub const MAX_WINDOWS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    Runoff,
    Schedule,
    Policy,
}

/// An inclusive interval of day ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub from_ordinal: i32,
    pub to_ordinal: i32,
}
impl Window {
    pub fn contains(&self, ordinal: i32) -> bool {
        self.from_ordinal <= ordinal && ordinal <= self.to_ordinal
    }
}

/// Explicit repeated replacement of one facility at its expiry.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Renewal {
    /// The total number of generations, the original included.
    pub max_generations: u32,
    pub tenor_days: i32,
    /// Declared dates on which the provider will replace the facility (a Physical assumption).
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyFacility {
    pub facility_id: String,
    /// Declared draw access (a Physical business assumption, separate from market states).
    pub access: Vec<Window>,
    #[serde(default)]
    pub renewal: Option<Renewal>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FundingPolicy {
    pub policy: Revision,
    /// Usable cash the policy maintains before any payment settles.
    pub minimum_usable_cash: f64,
    /// Facilities in draw order; surplus repays in reverse order.
    pub facilities: Vec<PolicyFacility>,
    /// Repay drawn policy facilities from usable cash above the floor at the end of each day.
    #[serde(default)]
    pub repay_surplus: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessPlan {
    pub mode: PlanMode,
    #[serde(default)]
    pub funding_policy: Option<FundingPolicy>,
}

/// One funding decision, with the state it was taken from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolicyDecision {
    pub decision_id: String,
    pub ordinal: i32,
    /// `draw`, `surplus_repayment`, `renewal` or `maturity_repayment`.
    pub kind: &'static str,
    pub facility_id: String,
    pub generation: u32,
    pub amount: f64,
    /// The event whose payment needed funding, or `start_of_day` / `end_of_day`.
    pub trigger: String,
    pub usable_cash_before: f64,
    pub usable_cash_after: f64,
    /// Usable cash the triggering payment would have left without this decision.
    pub need: f64,
}

/// One calendar month of dated activity.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthlyReport {
    pub month: String,
    pub first_ordinal: i32,
    pub last_ordinal: i32,
    pub events: usize,
    pub failed_events: usize,
    pub blocked_events: usize,
    pub policy_decisions: usize,
    pub opening_cash: f64,
    pub cash_in: f64,
    pub cash_out: f64,
    pub net_cash: f64,
    pub closing_cash: f64,
    pub minimum_usable_cash: f64,
    pub closing_usable_cash: f64,
    pub closing_facility_drawn: f64,
    pub interest_expense: f64,
    pub fees: f64,
    pub realized: f64,
    pub margin_posted: f64,
    pub margin_returned: f64,
    pub closing_posted_margin: f64,
    pub closing_pledged_quantity: f64,
    pub closing_security_quantity: f64,
}

fn refuse(code: &'static str, detail: impl Into<String>) -> Refusal {
    Refusal::new(code, detail)
}

/// Admission of the business plan against the plan facilities and the execution block.
pub fn validate(
    plan: &BusinessPlan,
    facilities: &[&str],
    scheduled_instructions: usize,
    scheduled_facility_ids: &BTreeSet<&str>,
    asof: i32,
    end: i32,
) -> Result<(), Refusal> {
    match (plan.mode, &plan.funding_policy) {
        (PlanMode::Runoff, None) if scheduled_instructions == 0 => Ok(()),
        (PlanMode::Runoff, None) => Err(refuse(
            MALFORMED_INPUT,
            "runoff executes no actions; remove the instructions or select schedule",
        )),
        (PlanMode::Schedule, None) => Ok(()),
        (PlanMode::Runoff | PlanMode::Schedule, Some(_)) => Err(refuse(
            MALFORMED_INPUT,
            "a funding policy is selected only with the policy mode",
        )),
        (PlanMode::Policy, None) => Err(refuse(
            MALFORMED_INPUT,
            "the policy mode needs a reviewed funding policy",
        )),
        (PlanMode::Policy, Some(p)) => {
            if !p.minimum_usable_cash.is_finite() || p.minimum_usable_cash < 0. {
                return Err(refuse(
                    MALFORMED_INPUT,
                    "the minimum usable cash must be finite and non-negative",
                ));
            }
            if p.facilities.is_empty() || p.facilities.len() > MAX_POLICY_FACILITIES {
                return Err(refuse(
                    MALFORMED_INPUT,
                    format!("a funding policy names 1 to {MAX_POLICY_FACILITIES} facilities"),
                ));
            }
            let mut seen = BTreeSet::new();
            for f in &p.facilities {
                if !facilities.contains(&f.facility_id.as_str()) {
                    return Err(refuse(
                        MALFORMED_INPUT,
                        format!(
                            "the funding policy names unknown facility `{}`",
                            f.facility_id
                        ),
                    ));
                }
                if !seen.insert(f.facility_id.as_str()) {
                    return Err(refuse(
                        MALFORMED_INPUT,
                        format!(
                            "facility `{}` appears twice in the funding policy",
                            f.facility_id
                        ),
                    ));
                }
                if scheduled_facility_ids.contains(f.facility_id.as_str()) {
                    return Err(refuse(
                        UNSUPPORTED_POLICY,
                        format!(
                            "facility `{}` is managed by the funding policy; scheduled draws and repayments of it are not combined",
                            f.facility_id
                        ),
                    ));
                }
                let windows = f
                    .access
                    .iter()
                    .chain(f.renewal.iter().flat_map(|r| &r.windows));
                if f.access.len() + f.renewal.as_ref().map_or(0, |r| r.windows.len()) > MAX_WINDOWS
                {
                    return Err(refuse(
                        MALFORMED_INPUT,
                        "too many access or renewal windows",
                    ));
                }
                for w in windows {
                    if w.from_ordinal > w.to_ordinal || w.to_ordinal < asof || w.from_ordinal > end
                    {
                        return Err(refuse(
                            MALFORMED_INPUT,
                            format!(
                                "facility `{}` has an empty or out-of-horizon window",
                                f.facility_id
                            ),
                        ));
                    }
                }
                if let Some(r) = &f.renewal {
                    if !(1..=MAX_GENERATIONS).contains(&r.max_generations)
                        || !(1..=MAX_RENEWAL_TENOR_DAYS).contains(&r.tenor_days)
                    {
                        return Err(refuse(
                            MALFORMED_INPUT,
                            format!(
                                "renewal of `{}` needs 1 to {MAX_GENERATIONS} generations and a tenor of 1 to {MAX_RENEWAL_TENOR_DAYS} days",
                                f.facility_id
                            ),
                        ));
                    }
                }
            }
            Ok(())
        }
    }
}

/// The instrument of a facility generation: the original keeps the facility id.
pub fn generation_instrument(facility_id: &str, generation: u32) -> String {
    if generation <= 1 {
        facility_id.to_string()
    } else {
        format!("{facility_id}~g{generation}")
    }
}

fn month_key(ordinal: i32) -> Result<(i32, u32), Refusal> {
    ymd_from_ordinal(ordinal)
        .map(|(y, m, _)| (y, m))
        .map_err(|e| refuse(MALFORMED_INPUT, e))
}

/// Opening state the first month starts from.
pub struct Opening {
    pub cash: f64,
    pub usable_cash: f64,
    pub facility_drawn: f64,
    pub posted_margin: f64,
    pub pledged_quantity: f64,
    pub security_quantity: f64,
}

/// Monthly reports for every calendar month from the as-of month to the horizon month.
pub fn monthly(
    intents: &[Intent],
    steps: &[LiquidityStep],
    opening: &Opening,
    asof: i32,
    end: i32,
) -> Result<Vec<MonthlyReport>, Refusal> {
    let mut months: Vec<(i32, u32)> = Vec::new();
    let mut bounds: Vec<(i32, i32)> = Vec::new();
    for ordinal in asof..=end {
        let key = month_key(ordinal)?;
        if months.last() != Some(&key) {
            months.push(key);
            bounds.push((ordinal, ordinal));
        } else if let Some(b) = bounds.last_mut() {
            b.1 = ordinal;
        }
    }
    let mut out = Vec::new();
    let mut cash = opening.cash;
    let mut usable = opening.usable_cash;
    let mut drawn = opening.facility_drawn;
    let mut posted = opening.posted_margin;
    let mut pledged = opening.pledged_quantity;
    let mut securities = opening.security_quantity;
    for (i, (y, m)) in months.iter().enumerate() {
        let (first, last) = bounds[i];
        let in_month = |o: i32| first <= o && o <= last;
        let mut r = MonthlyReport {
            month: format!("{y:04}-{m:02}"),
            first_ordinal: first,
            last_ordinal: last,
            events: 0,
            failed_events: 0,
            blocked_events: 0,
            policy_decisions: 0,
            opening_cash: cash,
            cash_in: 0.,
            cash_out: 0.,
            net_cash: 0.,
            closing_cash: cash,
            minimum_usable_cash: usable,
            closing_usable_cash: usable,
            closing_facility_drawn: drawn,
            interest_expense: 0.,
            fees: 0.,
            realized: 0.,
            margin_posted: 0.,
            margin_returned: 0.,
            closing_posted_margin: posted,
            closing_pledged_quantity: pledged,
            closing_security_quantity: securities,
        };
        for intent in intents.iter().filter(|x| in_month(x.ordinal) && !x.memo) {
            for l in &intent.lines {
                match l.kind {
                    Kind::Cash if l.debit > 0. => r.cash_in += l.debit,
                    Kind::Cash => r.cash_out -= l.debit,
                    Kind::InterestExpense => r.interest_expense += l.debit,
                    Kind::FeeExpense => r.fees += l.debit,
                    Kind::RealizedGainLoss => r.realized -= l.debit,
                    Kind::PostedVm | Kind::PostedIm if l.debit > 0. => r.margin_posted += l.debit,
                    Kind::PostedVm | Kind::PostedIm => r.margin_returned -= l.debit,
                    _ => {}
                }
                if l.kind == Kind::Cash {
                    r.net_cash += l.debit;
                }
            }
        }
        for s in steps.iter().filter(|s| in_month(s.ordinal)) {
            r.events += 1;
            match s.status {
                super::acquisition::EventStatus::Failed => r.failed_events += 1,
                super::acquisition::EventStatus::Blocked => r.blocked_events += 1,
                super::acquisition::EventStatus::Settled => {}
            }
            if s.event_id.starts_with("policy_") {
                r.policy_decisions += 1;
            }
            r.minimum_usable_cash = r.minimum_usable_cash.min(s.usable_cash_after);
            usable = s.usable_cash_after;
            drawn = s.facility_drawn_after;
            posted = s.posted_vm_after + s.posted_im_after;
            pledged = s.pledged_quantity_after;
            securities = s.security_quantity_after;
        }
        cash += r.net_cash;
        r.closing_cash = cash;
        r.closing_usable_cash = usable;
        r.closing_facility_drawn = drawn;
        r.closing_posted_margin = posted;
        r.closing_pledged_quantity = pledged;
        r.closing_security_quantity = securities;
        out.push(r);
    }
    Ok(out)
}
