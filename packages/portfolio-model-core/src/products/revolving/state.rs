//! Explicit revolving statement-cycle state. Spend/pay/default inputs are supplied paths.
use crate::cashflows::dated::{actual_year_fraction, validate_ordinal, ActualDayCount};
use crate::products::consumer::contracts::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const SCHEMA: &str = "consumer-card-1";
pub const MAX_CARD_WORK: usize = 250_000;

/// Validate native source domains/admission without generating financial state.
/// Chronological action feasibility is still checked by `simulate` replay.
pub fn validate_request(request: &CardRequest) -> Result<(), String> {
    validate(request)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceClass {
    Purchase,
    CashAdvance,
    Promotional,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardRates {
    pub purchase: f64,
    pub cash_advance: f64,
    pub promotional: f64,
    /// Genuine temporary APR, not deferred interest. Expiry changes future daily accrual only.
    pub promotional_expiry_ordinal: i32,
    pub post_promotion: f64,
}
impl CardRates {
    fn at(&self, c: BalanceClass, d: i32) -> f64 {
        match c {
            BalanceClass::Purchase => self.purchase,
            BalanceClass::CashAdvance => self.cash_advance,
            BalanceClass::Promotional if d < self.promotional_expiry_ordinal => self.promotional,
            BalanceClass::Promotional => self.post_promotion,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimumPayment {
    pub floor: f64,
    pub principal_fraction: f64,
    pub include_billed_interest: bool,
    pub include_fees: bool,
    /// Issuer-supplied minimum portion ordering. Excess is always descending effective APR.
    pub allocation_order: Vec<BalanceClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatementCycle {
    pub statement_ordinal: i32,
    pub due_ordinal: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardAction {
    Draw {
        balance_class: BalanceClass,
        amount: f64,
    },
    Payment {
        amount: f64,
    },
    Fee {
        balance_class: BalanceClass,
        amount: f64,
    },
    SetLimit {
        amount: f64,
    },
    SetRates {
        rates: CardRates,
    },
    SetState {
        state: ConsumerState,
    },
    Default {},
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardStep {
    pub ordinal: i32,
    pub action: CardAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardRequest {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
    pub opening_principal: [f64; 3],
    pub opening_billed_interest: [f64; 3],
    pub opening_unbilled_interest: [f64; 3],
    pub opening_unrecognized_billed_interest: [f64; 3],
    pub opening_fees: [f64; 3],
    pub opening_contingent_purchase_interest: f64,
    pub opening_statement_remaining: [f64; 3],
    pub opening_minimum_remaining: f64,
    pub opening_due_ordinal: Option<i32>,
    pub initial_limit: f64,
    pub initial_state: ConsumerState,
    pub rates: CardRates,
    pub day_count: ActualDayCount,
    pub initial_purchase_grace: bool,
    /// Failure at due charges accrued shadow interest on unpaid eligible purchases only.
    pub regain_grace_after_full_statement_payment: bool,
    /// If true, already posted finance charges enter the next day's interest base.
    pub compound_billed_interest: bool,
    pub minimum: MinimumPayment,
    pub cycles: Vec<StatementCycle>,
    pub steps: Vec<CardStep>,
    pub collection_fraction: f64,
    pub collection_cost: f64,
    pub recovery_delay_days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardStatement {
    pub ordinal: i32,
    pub due_ordinal: i32,
    pub principal: [f64; 3],
    pub billed_interest: [f64; 3],
    pub fees: [f64; 3],
    pub minimum_payment: f64,
    pub statement_total: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardPoint {
    pub ordinal: i32,
    pub state: ConsumerState,
    pub principal: [f64; 3],
    pub billed_interest: [f64; 3],
    pub unbilled_interest: [f64; 3],
    pub fees: [f64; 3],
    pub limit: f64,
    pub utilization: Option<f64>,
    pub undrawn: f64,
    pub purchase_grace: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardResult {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub statements: Vec<CardStatement>,
    pub cashflows: Vec<ConsumerCashEvent>,
    pub recognition: Vec<ConsumerRecognition>,
    pub points: Vec<CardPoint>,
    pub default: Option<ConsumerDefault>,
    pub opening_principal: f64,
    pub drawn_principal: f64,
    pub collected_principal: f64,
    pub closing_principal: f64,
    pub closing_interest: f64,
    pub closing_unbilled_interest: f64,
    pub closing_unrecognized_billed_interest: f64,
    pub closing_fees: f64,
    /// Unbilled contingent interest is retained, never silently treated as earned NII.
    pub closing_grace_contingent_interest: f64,
}

#[derive(Clone)]
struct Lot {
    class: BalanceClass,
    principal: f64,
    interest: f64,
    unbilled: f64,
    unrecognized_billed: f64,
    fees: f64,
    shadow: f64,
    grace: bool,
    statement_remaining: f64,
}
impl Lot {
    fn total(&self) -> f64 {
        self.principal + self.interest + self.fees
    }
}
fn ix(c: BalanceClass) -> usize {
    match c {
        BalanceClass::Purchase => 0,
        BalanceClass::CashAdvance => 1,
        BalanceClass::Promotional => 2,
    }
}
fn totals(lots: &[Lot]) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let (mut p, mut i, mut f) = ([0.; 3], [0.; 3], [0.; 3]);
    for l in lots {
        p[ix(l.class)] += l.principal;
        i[ix(l.class)] += l.interest;
        f[ix(l.class)] += l.fees;
    }
    (p, i, f)
}

pub fn simulate(r: &CardRequest) -> Result<CardResult, String> {
    validate(r)?;
    let end = horizon(r.as_of_ordinal, r.horizon_days)?;
    let mut by_day = BTreeMap::<i32, Vec<&CardAction>>::new();
    for s in &r.steps {
        by_day.entry(s.ordinal).or_default().push(&s.action);
    }
    let mut lots: Vec<Lot> = [
        BalanceClass::Purchase,
        BalanceClass::CashAdvance,
        BalanceClass::Promotional,
    ]
    .iter()
    .enumerate()
    .map(|(j, c)| Lot {
        class: *c,
        principal: r.opening_principal[j],
        interest: r.opening_billed_interest[j],
        unbilled: r.opening_unbilled_interest[j],
        unrecognized_billed: r.opening_unrecognized_billed_interest[j],
        fees: r.opening_fees[j],
        shadow: if j == 0 {
            r.opening_contingent_purchase_interest
        } else {
            0.
        },
        grace: *c == BalanceClass::Purchase && r.initial_purchase_grace,
        statement_remaining: r.opening_statement_remaining[j],
    })
    .collect();
    let (mut limit, mut state, mut rates, mut grace) = (
        r.initial_limit,
        r.initial_state,
        r.rates.clone(),
        r.initial_purchase_grace,
    );
    let (mut minimum_remaining, mut drawn, mut collected) = (r.opening_minimum_remaining, 0.0, 0.0);
    let mut cashflows = Vec::new();
    let mut recognition = Vec::new();
    let mut points = Vec::new();
    let mut statements = Vec::new();
    let mut default = None;
    for day in r.as_of_ordinal..=end {
        // Prior day's interest is accrued before today's transactions; same-day actions retain input order.
        if day > r.as_of_ordinal
            && !matches!(state, ConsumerState::Default | ConsumerState::Nonaccrual)
        {
            let tau = actual_year_fraction(day - 1, day, r.day_count)?;
            for (j, l) in lots.iter_mut().enumerate() {
                let base = l.principal
                    + if r.compound_billed_interest {
                        l.interest
                    } else {
                        0.
                    };
                let earned = base * rates.at(l.class, day - 1) * tau;
                amount(earned, "card daily interest")?;
                if l.grace {
                    l.shadow += earned;
                } else {
                    l.unbilled += earned;
                    if earned != 0. {
                        recognition.push(ConsumerRecognition {
                            origin_id: format!("lot-{j}"),
                            start_ordinal: day - 1,
                            end_ordinal: day,
                            interest: earned,
                        });
                    }
                }
            }
        }
        if let Some(actions) = by_day.get(&day) {
            for action in actions {
                if state == ConsumerState::Default {
                    return Err("card action after default".into());
                }
                match action {
                    CardAction::Draw {
                        balance_class,
                        amount: a,
                    } => {
                        if state != ConsumerState::Performing {
                            return Err("card draws require performing state".into());
                        }
                        let owed: f64 = lots.iter().map(Lot::total).sum();
                        if owed + a > limit {
                            return Err(
                                "card draw exceeds available line including billed charges".into(),
                            );
                        }
                        lots.push(Lot {
                            class: *balance_class,
                            principal: *a,
                            interest: 0.,
                            unbilled: 0.,
                            unrecognized_billed: 0.,
                            fees: 0.,
                            shadow: 0.,
                            grace: *balance_class == BalanceClass::Purchase && grace,
                            statement_remaining: 0.,
                        });
                        drawn += a;
                        cashflows.push(ConsumerCashEvent {
                            event_id: format!("draw-{}", cashflows.len()),
                            ordinal: day,
                            kind: "draw".into(),
                            principal: -a,
                            interest: 0.,
                            fees: 0.,
                        });
                    }
                    CardAction::Fee {
                        balance_class,
                        amount: a,
                    } => lots.push(Lot {
                        class: *balance_class,
                        principal: 0.,
                        interest: 0.,
                        unbilled: 0.,
                        unrecognized_billed: 0.,
                        fees: *a,
                        shadow: 0.,
                        grace: false,
                        statement_remaining: 0.,
                    }),
                    CardAction::Payment { amount: a } => {
                        let owed: f64 = lots.iter().map(Lot::total).sum();
                        if *a > owed + 1e-9 {
                            return Err(
                                "card payment exceeds debt; refund/credit balance unsupported"
                                    .into(),
                            );
                        }
                        let minimum = a.min(minimum_remaining);
                        minimum_remaining -= minimum;
                        let mut paid = [0.; 4];
                        let mut order = Vec::new();
                        for c in &r.minimum.allocation_order {
                            order.extend(
                                lots.iter()
                                    .enumerate()
                                    .filter(|(_, l)| l.class == *c)
                                    .map(|(j, _)| j),
                            );
                        }
                        allocate(&mut lots, &order, minimum, &mut paid);
                        let mut excess_order: Vec<_> = (0..lots.len()).collect();
                        excess_order.sort_by(|a, b| {
                            rates
                                .at(lots[*b].class, day)
                                .total_cmp(&rates.at(lots[*a].class, day))
                                .then(a.cmp(b))
                        });
                        allocate(&mut lots, &excess_order, a - minimum, &mut paid);
                        if paid[3] != 0. {
                            recognition.push(ConsumerRecognition {
                                origin_id: "nonaccrual-cash-basis".into(),
                                start_ordinal: day,
                                end_ordinal: day,
                                interest: paid[3],
                            });
                        }
                        collected += paid[0];
                        cashflows.push(ConsumerCashEvent {
                            event_id: format!("payment-{}", cashflows.len()),
                            ordinal: day,
                            kind: "payment".into(),
                            principal: paid[0],
                            interest: paid[1],
                            fees: paid[2],
                        });
                    }
                    CardAction::SetLimit { amount: a } => {
                        limit = *a;
                    }
                    CardAction::SetRates { rates: v } => {
                        rates = v.clone();
                    }
                    CardAction::SetState { state: v } => {
                        state = *v;
                    }
                    CardAction::Default {} => {
                        let (p, i, f) = totals(&lots);
                        let principal = p.iter().sum::<f64>();
                        let gross = principal * r.collection_fraction;
                        let net = (gross - r.collection_cost).max(0.);
                        let recovery_day = day
                            .checked_add(r.recovery_delay_days as i32)
                            .ok_or("card recovery date overflow")?;
                        if recovery_day > end {
                            return Err("full card horizon must retain recovery tail".into());
                        }
                        default = Some(ConsumerDefault {
                            ordinal: day,
                            principal,
                            accrued_interest: i.iter().sum::<f64>()
                                + lots.iter().map(|l| l.unbilled).sum::<f64>(),
                            recognized_accrued_interest: i.iter().sum::<f64>()
                                + lots
                                    .iter()
                                    .map(|l| l.unbilled - l.unrecognized_billed)
                                    .sum::<f64>(),
                            unrecognized_contract_interest: lots
                                .iter()
                                .map(|l| l.unrecognized_billed)
                                .sum(),
                            fees: f.iter().sum(),
                            gross_recovery: gross,
                            recovery_cost: r.collection_cost,
                            net_recovery: net,
                            principal_loss: principal - net,
                            recovery_ordinal: recovery_day,
                        });
                        let uncovered_cost = (r.collection_cost - gross).max(0.);
                        if net != 0. || uncovered_cost != 0. {
                            cashflows.push(ConsumerCashEvent {
                                event_id: "recovery-0".into(),
                                ordinal: recovery_day,
                                kind: "recovery".into(),
                                principal: net,
                                interest: 0.,
                                fees: -uncovered_cost,
                            });
                        }
                        lots.clear();
                        state = ConsumerState::Default;
                        limit = 0.;
                        grace = false;
                        minimum_remaining = 0.;
                    }
                }
            }
        }
        if state != ConsumerState::Default {
            if r.opening_due_ordinal == Some(day) || r.cycles.iter().any(|c| c.due_ordinal == day) {
                let unpaid: f64 = lots.iter().map(|l| l.statement_remaining).sum();
                if unpaid > 1e-9 {
                    grace = false;
                    for (j, l) in lots.iter_mut().enumerate() {
                        if l.grace {
                            if l.shadow != 0. {
                                l.interest += l.shadow;
                                if state == ConsumerState::Nonaccrual {
                                    l.unrecognized_billed += l.shadow;
                                } else {
                                    recognition.push(ConsumerRecognition {
                                        origin_id: format!("grace-loss-lot-{j}"),
                                        start_ordinal: day,
                                        end_ordinal: day,
                                        interest: l.shadow,
                                    });
                                }
                            }
                            l.shadow = 0.;
                            l.grace = false;
                        }
                    }
                } else if r.regain_grace_after_full_statement_payment {
                    grace = true;
                }
                for l in &mut lots {
                    l.statement_remaining = 0.;
                }
            }
            if let Some(c) = r.cycles.iter().find(|c| c.statement_ordinal == day) {
                for l in &mut lots {
                    l.interest += l.unbilled;
                    l.unbilled = 0.;
                }
                let (p, i, f) = totals(&lots);
                let principal = p.iter().sum::<f64>();
                let billed = i.iter().sum::<f64>();
                let fees = f.iter().sum::<f64>();
                let total = principal + billed + fees;
                minimum_remaining = (r.minimum.floor.max(
                    principal * r.minimum.principal_fraction
                        + if r.minimum.include_billed_interest {
                            billed
                        } else {
                            0.
                        }
                        + if r.minimum.include_fees { fees } else { 0. },
                ))
                .min(total);
                for l in &mut lots {
                    l.statement_remaining = l.total();
                }
                statements.push(CardStatement {
                    ordinal: day,
                    due_ordinal: c.due_ordinal,
                    principal: p,
                    billed_interest: i,
                    fees: f,
                    minimum_payment: minimum_remaining,
                    statement_total: total,
                });
            }
        }
        let (p, i, f) = totals(&lots);
        let principal = p.iter().sum::<f64>();
        let debt = principal + i.iter().sum::<f64>() + f.iter().sum::<f64>();
        let mut unbilled = [0.; 3];
        for l in &lots {
            unbilled[ix(l.class)] += l.unbilled;
        }
        for (v, name) in [
            (debt, "card debt"),
            (drawn, "card cumulative draws"),
            (collected, "card cumulative principal payments"),
        ] {
            amount(v, name)?;
        }
        for l in &lots {
            amount(l.shadow, "contingent grace interest")?;
        }
        amount(
            debt + unbilled.iter().sum::<f64>() + lots.iter().map(|l| l.shadow).sum::<f64>(),
            "total card rights",
        )?;
        points.push(CardPoint {
            ordinal: day,
            state,
            principal: p,
            billed_interest: i,
            unbilled_interest: unbilled,
            fees: f,
            limit,
            utilization: if limit > 0. { Some(debt / limit) } else { None },
            undrawn: (limit - debt).max(0.),
            purchase_grace: grace,
        });
    }
    let (p, i, f) = totals(&lots);
    let closing = p.iter().sum::<f64>();
    let opening = r.opening_principal.iter().sum::<f64>();
    let removed = default.as_ref().map_or(0., |d| d.principal);
    if (opening + drawn - collected - removed - closing).abs()
        > 1e-8_f64.max((opening + drawn) * 1e-12)
    {
        return Err("card principal conservation failed".into());
    }
    cashflows.sort_by_key(|c| c.ordinal);
    Ok(CardResult {
        schema: SCHEMA.into(),
        identity: r.identity.clone(),
        statements,
        cashflows,
        recognition,
        points,
        default,
        opening_principal: opening,
        drawn_principal: drawn,
        collected_principal: collected,
        closing_principal: closing,
        closing_interest: i.iter().sum(),
        closing_unbilled_interest: lots.iter().map(|l| l.unbilled).sum(),
        closing_unrecognized_billed_interest: lots.iter().map(|l| l.unrecognized_billed).sum(),
        closing_fees: f.iter().sum(),
        closing_grace_contingent_interest: lots.iter().map(|l| l.shadow).sum(),
    })
}

/// Per-lot fees, posted interest, then principal. FIFO is stable within equal APR/class.
fn allocate(lots: &mut [Lot], order: &[usize], mut amount: f64, paid: &mut [f64; 4]) {
    for &j in order {
        if amount <= 0. {
            break;
        }
        let l = &mut lots[j];
        let f = amount.min(l.fees);
        l.fees -= f;
        amount -= f;
        paid[2] += f;
        let i = amount.min(l.interest);
        let paid_unrecognized = (i - (l.interest - l.unrecognized_billed)).max(0.);
        l.unrecognized_billed -= paid_unrecognized;
        paid[3] += paid_unrecognized;
        l.interest -= i;
        amount -= i;
        paid[1] += i;
        let p = amount.min(l.principal);
        let before = l.principal;
        l.principal -= p;
        amount -= p;
        paid[0] += p;
        // Grace-protected paid principal incurs no retroactive shadow charge.
        if before > 0. {
            l.shadow *= l.principal / before;
        }
        l.statement_remaining = (l.statement_remaining - f - i - p).max(0.);
    }
}

fn validate(r: &CardRequest) -> Result<(), String> {
    if r.schema != SCHEMA {
        return Err("card schema mismatch".into());
    }
    identity(&r.identity, SCHEMA)?;
    let end = horizon(r.as_of_ordinal, r.horizon_days)?;
    day_count(r.day_count)?;
    rates(&r.rates)?;
    fraction(r.collection_fraction, "card collections")?;
    amount(r.collection_cost, "collection cost")?;
    amount(r.initial_limit, "card limit")?;
    amount(r.minimum.floor, "minimum floor")?;
    fraction(r.minimum.principal_fraction, "minimum principal fraction")?;
    for v in r
        .opening_principal
        .iter()
        .chain(&r.opening_billed_interest)
        .chain(&r.opening_unbilled_interest)
        .chain(&r.opening_fees)
    {
        amount(*v, "opening card amount")?;
    }
    let opening: f64 = r
        .opening_principal
        .iter()
        .chain(&r.opening_billed_interest)
        .chain(&r.opening_unbilled_interest)
        .chain(&r.opening_fees)
        .sum();
    amount(opening, "opening card total")?;
    amount(
        r.opening_contingent_purchase_interest,
        "opening contingent interest",
    )?;
    amount(
        opening + r.opening_contingent_purchase_interest,
        "all opening card rights",
    )?;
    if !r.initial_purchase_grace && r.opening_contingent_purchase_interest != 0. {
        return Err("opening contingent interest requires purchase grace".into());
    }
    amount(r.opening_minimum_remaining, "opening minimum remaining")?;
    for (j, &v) in r.opening_statement_remaining.iter().enumerate() {
        amount(v, "opening statement remaining")?;
        if v > r.opening_principal[j] + r.opening_billed_interest[j] + r.opening_fees[j] {
            return Err("opening statement rights exceed opening billed debt".into());
        }
    }
    for (j, &v) in r.opening_unrecognized_billed_interest.iter().enumerate() {
        amount(v, "opening unrecognized billed interest")?;
        if v > r.opening_billed_interest[j] {
            return Err("unrecognized billed interest exceeds billed interest".into());
        }
    }
    let opening_statement = r.opening_statement_remaining.iter().sum::<f64>();
    if r.opening_minimum_remaining > opening_statement {
        return Err("opening minimum exceeds remaining statement".into());
    }
    let opening_due = if let Some(due) = r.opening_due_ordinal {
        validate_ordinal(due)?;
        if due < r.as_of_ordinal || due > end {
            return Err("opening due date must be within full horizon".into());
        }
        due
    } else {
        if opening_statement != 0. || r.opening_minimum_remaining != 0. {
            return Err("opening statement requires explicit due date".into());
        }
        r.as_of_ordinal - 1
    };
    if matches!(
        r.initial_state,
        ConsumerState::Default | ConsumerState::PaidOff
    ) {
        return Err("card initial state must be live".into());
    }
    if r.minimum.allocation_order.len() != 3
        || r.minimum
            .allocation_order
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != 3
    {
        return Err("minimum allocation order must name all three balance classes once".into());
    }
    if r.steps.len() + r.cycles.len() > MAX_CONSUMER_EVENTS
        || r.recovery_delay_days > MAX_CONSUMER_DAYS
    {
        return Err("card event/tail admission exceeded".into());
    }
    let lot_count = 3 + r
        .steps
        .iter()
        .filter(|s| matches!(s.action, CardAction::Draw { .. } | CardAction::Fee { .. }))
        .count();
    // Daily recognition can emit one row per active lot/day; input scans also bounded.
    if (r.horizon_days as usize + 1)
        .checked_mul(lot_count + r.cycles.len())
        .is_none_or(|work| work > MAX_CARD_WORK)
    {
        return Err("card daily work admission exceeded".into());
    }
    let mut previous_due = opening_due;
    for c in &r.cycles {
        validate_ordinal(c.statement_ordinal)?;
        validate_ordinal(c.due_ordinal)?;
        if c.statement_ordinal <= previous_due
            || c.due_ordinal <= c.statement_ordinal
            || c.due_ordinal > end
            || c.statement_ordinal < r.as_of_ordinal
        {
            return Err("card cycles must be ordered nonoverlapping statement/due intervals within full horizon".into());
        }
        previous_due = c.due_ordinal;
    }
    let mut previous = r.as_of_ordinal;
    for s in &r.steps {
        validate_ordinal(s.ordinal)?;
        if s.ordinal < previous || s.ordinal > end {
            return Err(
                "card actions must be ordered within horizon; equal dates preserve order".into(),
            );
        }
        previous = s.ordinal;
        match &s.action {
            CardAction::Draw{amount:v,..}|CardAction::Payment{amount:v}|CardAction::Fee{amount:v,..}|CardAction::SetLimit{amount:v}=>amount(*v,"card action")?,
            CardAction::SetRates{rates:v}=>rates(v)?,
            CardAction::SetState{state:ConsumerState::Default|ConsumerState::PaidOff}=>return Err("card terminal credit state requires default action; zero balance remains an open line".into()),
            _=>{}
        }
    }
    Ok(())
}
fn rates(v: &CardRates) -> Result<(), String> {
    for r in [v.purchase, v.cash_advance, v.promotional, v.post_promotion] {
        rate(r)?;
    }
    validate_ordinal(v.promotional_expiry_ordinal)
}
