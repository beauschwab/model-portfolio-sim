//! Intraday settlement replay (`intraday-settlement-1`).
//!
//! Timestamped payment requests are queued and paid in discrete settlement cycles from available
//! cash. Facility draws and collateral moves are declared in advance, decided only on the state
//! at their own minute, and settle after their own lags. Nothing settles at or after the stop
//! minute. The full rule text is docs/models/intraday-settlement.md.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "intraday-settlement-1";
const DAY: u32 = 1440;
const MAX_MINUTE: u32 = 14 * DAY;
const MAX_ROWS: usize = 100_000;
const MAX_CYCLES: usize = 200_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub opening_cash: f64,
    pub horizon_minute: u32,
    /// Receivership or other hard stop: nothing at or after this minute is processed.
    pub stop_minute: Option<u32>,
    pub systems: Vec<System>,
    #[serde(default)]
    pub payments: Vec<Payment>,
    #[serde(default)]
    pub receipts: Vec<Receipt>,
    /// Netting sets with legally enforceable netting.
    #[serde(default)]
    pub netting_sets: Vec<String>,
    #[serde(default)]
    pub lots: Vec<Lot>,
    #[serde(default)]
    pub moves: Vec<Move>,
    #[serde(default)]
    pub facilities: Vec<Facility>,
    #[serde(default)]
    pub draws: Vec<Draw>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct System {
    pub id: String,
    pub open: u32,
    pub close: u32,
    pub cycle_minutes: u32,
    pub capacity_per_cycle: Option<f64>,
    #[serde(default)]
    pub closed_days: Vec<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payment {
    pub id: String,
    pub minute: u32,
    pub amount: f64,
    pub system: String,
    pub netting_set: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: String,
    pub minute: u32,
    pub amount: f64,
    pub system: Option<String>,
    pub netting_set: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lot {
    pub id: String,
    pub value: f64,
    pub location: String,
    /// Haircut by facility; a facility without an entry cannot lend against the lot.
    #[serde(default)]
    pub haircuts: BTreeMap<String, f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Move {
    pub id: String,
    pub lot: String,
    pub to: String,
    pub minute: u32,
    pub transfer_minutes: u32,
    pub window_open: u32,
    pub window_close: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facility {
    pub id: String,
    pub location: String,
    pub open: u32,
    pub close: u32,
    pub settlement_lag_minutes: u32,
    pub capacity: f64,
    #[serde(default)]
    pub outages: Vec<[u32; 2]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draw {
    pub id: String,
    pub facility: String,
    pub minute: u32,
    pub lots: Vec<String>,
    pub amount: Option<f64>,
    pub only_if_cash_below: Option<f64>,
}

fn money(v: f64, positive: bool) -> bool {
    v.is_finite() && v.abs() <= 1e15 && (!positive || v > 0.)
}
fn unique<'a>(ids: impl Iterator<Item = &'a String>, what: &str) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() || id.len() > 64 || !seen.insert(id) {
            return Err(format!(
                "{what} ids must be nonempty, at most 64 bytes and unique"
            ));
        }
    }
    Ok(())
}
fn window(open: u32, close: u32) -> bool {
    open <= close && close < DAY
}

/// Priority of same-minute work; lower runs first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    MoveCompleted,
    DrawSettled,
    Receipt,
    Payment,
    MoveStart,
    Draw,
    Cycle,
}

struct Sim<'a> {
    r: &'a Request,
    cash: f64,
    events: Vec<Value>,
    /// Unpaid remainder by payment index; negative before arrival.
    remaining: Vec<f64>,
    arrived: Vec<bool>,
    first_paid: Vec<Option<u32>>,
    settled: Vec<Option<u32>>,
    after_cutoff: Vec<bool>,
    receipt_settled: Vec<bool>,
    location: Vec<String>,
    pledged: Vec<bool>,
    used: Vec<f64>,
    paid: f64,
    received: f64,
    drawn: f64,
    unsettled_draws: f64,
    blocked: usize,
    first_shortfall: Option<u32>,
    shortfall_cycles: usize,
    peak_queue: f64,
    peak_minute: Option<u32>,
}

impl Request {
    fn check(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!("intraday settlement must select {SCHEMA}"));
        }
        if !money(self.opening_cash, false) || self.opening_cash < 0. {
            return Err("opening cash must be a finite nonnegative amount".into());
        }
        if self.horizon_minute == 0
            || self.horizon_minute > MAX_MINUTE
            || self.stop_minute.is_some_and(|s| s > self.horizon_minute)
        {
            return Err("horizon must be within 14 days and the stop minute within it".into());
        }
        if self.systems.is_empty()
            || self.systems.len() > 16
            || self.payments.len() > MAX_ROWS
            || self.receipts.len() > MAX_ROWS
            || self.lots.len() > 10_000
            || self.moves.len() > 10_000
            || self.facilities.len() > 64
            || self.draws.len() > 10_000
            || self.netting_sets.len() > 10_000
        {
            return Err("intraday settlement row limits exceeded".into());
        }
        unique(self.systems.iter().map(|x| &x.id), "system")?;
        unique(self.payments.iter().map(|x| &x.id), "payment")?;
        unique(self.receipts.iter().map(|x| &x.id), "receipt")?;
        unique(self.lots.iter().map(|x| &x.id), "lot")?;
        unique(self.moves.iter().map(|x| &x.id), "move")?;
        unique(self.facilities.iter().map(|x| &x.id), "facility")?;
        unique(self.draws.iter().map(|x| &x.id), "draw")?;
        unique(self.netting_sets.iter(), "netting set")?;
        let systems: BTreeSet<_> = self.systems.iter().map(|x| x.id.as_str()).collect();
        let sets: BTreeSet<_> = self.netting_sets.iter().map(String::as_str).collect();
        let lots: BTreeSet<_> = self.lots.iter().map(|x| x.id.as_str()).collect();
        let facilities: BTreeSet<_> = self.facilities.iter().map(|x| x.id.as_str()).collect();
        let mut cycles = 0usize;
        for s in &self.systems {
            if !window(s.open, s.close)
                || s.cycle_minutes == 0
                || s.capacity_per_cycle.is_some_and(|c| !money(c, true))
                || s.closed_days.iter().any(|d| *d > 14)
            {
                return Err(format!(
                    "system {} has an invalid window, cycle or capacity",
                    s.id
                ));
            }
            cycles += ((s.close - s.open) / s.cycle_minutes + 1) as usize
                * (self.horizon_minute / DAY + 1) as usize;
        }
        if cycles > MAX_CYCLES {
            return Err("intraday settlement cycle limit exceeded".into());
        }
        for p in &self.payments {
            if !money(p.amount, true)
                || p.minute > self.horizon_minute
                || !systems.contains(p.system.as_str())
                || p.netting_set.as_deref().is_some_and(|n| !sets.contains(n))
            {
                return Err(format!("payment {} is invalid", p.id));
            }
        }
        for x in &self.receipts {
            let netted = x.netting_set.is_some();
            if !money(x.amount, true)
                || x.minute > self.horizon_minute
                || x.netting_set.as_deref().is_some_and(|n| !sets.contains(n))
                || x.system.as_deref().is_some_and(|s| !systems.contains(s))
                || netted != x.system.is_some()
            {
                return Err(format!(
                    "receipt {} is invalid; a netted receipt names its system and a plain receipt names none",
                    x.id
                ));
            }
        }
        for f in &self.facilities {
            if !window(f.open, f.close)
                || !money(f.capacity, true)
                || f.location.is_empty()
                || f.settlement_lag_minutes > MAX_MINUTE
                || f.outages.len() > 64
                || f.outages.iter().any(|o| o[0] >= o[1] || o[1] > MAX_MINUTE)
            {
                return Err(format!("facility {} is invalid", f.id));
            }
        }
        for l in &self.lots {
            if !money(l.value, true)
                || l.location.is_empty()
                || l.location == "in_transit"
                || l.haircuts
                    .iter()
                    .any(|(f, h)| !facilities.contains(f.as_str()) || !(0. ..1.).contains(h))
            {
                return Err(format!("lot {} is invalid", l.id));
            }
        }
        for m in &self.moves {
            if !lots.contains(m.lot.as_str())
                || m.to.is_empty()
                || m.to == "in_transit"
                || m.minute > self.horizon_minute
                || m.transfer_minutes > MAX_MINUTE
                || !window(m.window_open, m.window_close)
            {
                return Err(format!("move {} is invalid", m.id));
            }
        }
        for d in &self.draws {
            let mut seen = BTreeSet::new();
            if !facilities.contains(d.facility.as_str())
                || d.minute > self.horizon_minute
                || d.lots.is_empty()
                || d.lots.len() > 1000
                || d.lots
                    .iter()
                    .any(|l| !lots.contains(l.as_str()) || !seen.insert(l))
                || d.amount.is_some_and(|a| !money(a, true))
                || d.only_if_cash_below.is_some_and(|a| !money(a, false))
            {
                return Err(format!("draw {} is invalid", d.id));
            }
        }
        Ok(())
    }
}

impl<'a> Sim<'a> {
    fn queued(&self) -> f64 {
        (0..self.remaining.len())
            .filter(|i| self.arrived[*i])
            .map(|i| self.remaining[i])
            .sum()
    }
    fn emit(&mut self, minute: u32, event: &str, id: &str, amount: f64, detail: &str) {
        let queued = self.queued();
        if queued > self.peak_queue {
            self.peak_queue = queued;
            self.peak_minute = Some(minute);
        }
        self.events.push(
            json!({"seq":self.events.len(),"minute":minute,"event":event,"id":id,
            "amount":amount,"cash":self.cash,"queued":queued,"detail":detail}),
        );
    }
    fn pay(&mut self, minute: u32, i: usize, amount: f64, detail: &str) {
        self.remaining[i] -= amount;
        self.paid += amount;
        self.first_paid[i].get_or_insert(minute);
        if self.remaining[i] <= 0. {
            self.remaining[i] = 0.;
            self.settled[i] = Some(minute);
        }
        let id = self.r.payments[i].id.clone();
        self.emit(minute, "payment_paid", &id, amount, detail);
    }
    fn draw(&mut self, minute: u32, i: usize, pending: &mut Vec<(u32, usize, f64)>) {
        let d = &self.r.draws[i];
        let (fi, f) = self
            .r
            .facilities
            .iter()
            .enumerate()
            .find(|(_, f)| f.id == d.facility)
            .expect("validated facility");
        let of_day = minute % DAY;
        let mut reason = "";
        if d.only_if_cash_below.is_some_and(|x| self.cash >= x) {
            reason = "not_needed";
        } else if f.outages.iter().any(|o| o[0] <= minute && minute < o[1]) {
            reason = "outage";
        } else if of_day < f.open || of_day > f.close {
            reason = "facility_closed";
        }
        let mut value = 0.;
        if reason.is_empty() {
            for id in &d.lots {
                let li = self.r.lots.iter().position(|l| &l.id == id).expect("lot");
                reason = if self.pledged[li] {
                    "already_pledged"
                } else if self.location[li] == "in_transit" {
                    "collateral_in_transit"
                } else if self.location[li] != f.location {
                    "collateral_not_at_facility"
                } else if let Some(h) = self.r.lots[li].haircuts.get(&f.id) {
                    value += self.r.lots[li].value * (1. - h);
                    ""
                } else {
                    "ineligible"
                };
                if !reason.is_empty() {
                    break;
                }
            }
        }
        let mut amount = 0.;
        if reason.is_empty() {
            amount = value.min(f.capacity - self.used[fi]);
            if let Some(cap) = d.amount {
                amount = amount.min(cap);
            }
            if amount <= 0. {
                reason = "no_capacity";
            }
        }
        if !reason.is_empty() {
            self.blocked += 1;
            self.emit(minute, "draw_blocked", &d.id, 0., reason);
            return;
        }
        for id in &d.lots {
            let li = self.r.lots.iter().position(|l| &l.id == id).expect("lot");
            self.pledged[li] = true;
        }
        self.used[fi] += amount;
        self.unsettled_draws += amount;
        pending.push((minute + f.settlement_lag_minutes, i, amount));
        self.emit(minute, "draw_requested", &d.id, amount, &f.id);
    }
    fn cycle(&mut self, minute: u32, si: usize) {
        let system = &self.r.systems[si];
        let mut capacity = system.capacity_per_cycle.unwrap_or(f64::INFINITY);
        let mine = |sim: &Sim, i: usize| {
            sim.arrived[i] && sim.remaining[i] > 0. && sim.r.payments[i].system == system.id
        };
        // Payments of a set that waits this cycle settle only with their set, never gross.
        let mut held = vec![false; self.r.payments.len()];
        let mut short = false;
        for set in &self.r.netting_sets {
            let receipts: Vec<usize> = (0..self.r.receipts.len())
                .filter(|j| {
                    let x = &self.r.receipts[*j];
                    !self.receipt_settled[*j]
                        && x.minute <= minute
                        && x.netting_set.as_ref() == Some(set)
                        && x.system.as_ref() == Some(&system.id)
                })
                .collect();
            if receipts.is_empty() {
                continue;
            }
            let payments: Vec<usize> = (0..self.r.payments.len())
                .filter(|i| mine(self, *i) && self.r.payments[*i].netting_set.as_ref() == Some(set))
                .collect();
            let out: f64 = payments.iter().map(|i| self.remaining[*i]).sum();
            let inn: f64 = receipts.iter().map(|j| self.r.receipts[*j].amount).sum();
            let need = out - inn;
            if need > 0. && (need > self.cash || need > capacity) {
                let reason = if need > self.cash { "cash" } else { "capacity" };
                short |= need > self.cash;
                for i in payments {
                    held[i] = true;
                }
                self.emit(minute, "netting_waiting", set, need, reason);
                continue;
            }
            if need > 0. {
                capacity -= need;
            }
            // Legs settle together: receipts first, then payments, so cash never shows the gross.
            for j in receipts {
                self.receipt_settled[j] = true;
                self.cash += self.r.receipts[j].amount;
                self.received += self.r.receipts[j].amount;
                let id = self.r.receipts[j].id.clone();
                let amount = self.r.receipts[j].amount;
                self.emit(minute, "receipt", &id, amount, set);
            }
            for i in payments {
                let amount = self.remaining[i];
                self.cash -= amount;
                self.pay(minute, i, amount, set);
            }
        }
        // Gross payments, strictly first in first out.
        let mut order: Vec<usize> = (0..self.r.payments.len())
            .filter(|i| mine(self, *i) && !held[*i])
            .collect();
        order.sort_by_key(|i| (self.r.payments[*i].minute, *i));
        for i in order {
            let amount = self.remaining[i].min(self.cash).min(capacity);
            if amount > 0. {
                self.cash -= amount;
                capacity -= amount;
                self.pay(minute, i, amount, &system.id);
            }
            if self.remaining[i] > 0. {
                let unmet: f64 = (0..self.r.payments.len())
                    .filter(|k| mine(self, *k))
                    .map(|k| self.remaining[k])
                    .sum();
                if capacity <= 0. && self.cash > 0. {
                    self.emit(minute, "capacity_limited", &system.id, unmet, "");
                } else {
                    short = true;
                    self.emit(minute, "shortfall", &system.id, unmet, "");
                }
                break;
            }
        }
        if short {
            self.first_shortfall.get_or_insert(minute);
            self.shortfall_cycles += 1;
        }
    }
}

pub fn run(r: &Request) -> Result<Value, String> {
    run_with_check(r, portfolio_compute_control::checkpoint)
}

pub(crate) fn run_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Value, String> {
    r.check()?;
    check()?;
    let end = r.stop_minute.unwrap_or(r.horizon_minute + 1);
    // (minute, kind, input index)
    let mut work: Vec<(u32, Kind, usize)> = Vec::new();
    for (i, x) in r.receipts.iter().enumerate() {
        if x.netting_set.is_none() {
            work.push((x.minute, Kind::Receipt, i));
        }
    }
    for (i, p) in r.payments.iter().enumerate() {
        work.push((p.minute, Kind::Payment, i));
    }
    let mut move_start = vec![0u32; r.moves.len()];
    for (i, m) in r.moves.iter().enumerate() {
        let of_day = m.minute % DAY;
        // An instruction outside the custodian window starts at the next opening.
        move_start[i] = if of_day < m.window_open {
            m.minute - of_day + m.window_open
        } else if of_day > m.window_close {
            m.minute - of_day + DAY + m.window_open
        } else {
            m.minute
        };
        work.push((m.minute, Kind::MoveStart, i));
    }
    for (i, d) in r.draws.iter().enumerate() {
        work.push((d.minute, Kind::Draw, i));
    }
    for (si, s) in r.systems.iter().enumerate() {
        for day in 0..=r.horizon_minute / DAY {
            if s.closed_days.contains(&day) {
                continue;
            }
            let mut t = s.open;
            while t <= s.close {
                work.push((day * DAY + t, Kind::Cycle, si));
                t += s.cycle_minutes;
            }
        }
    }
    work.sort();
    let mut sim = Sim {
        r,
        cash: r.opening_cash,
        events: Vec::new(),
        remaining: r.payments.iter().map(|p| p.amount).collect(),
        arrived: vec![false; r.payments.len()],
        first_paid: vec![None; r.payments.len()],
        settled: vec![None; r.payments.len()],
        after_cutoff: vec![false; r.payments.len()],
        receipt_settled: vec![false; r.receipts.len()],
        location: r.lots.iter().map(|l| l.location.clone()).collect(),
        pledged: vec![false; r.lots.len()],
        used: vec![0.; r.facilities.len()],
        paid: 0.,
        received: 0.,
        drawn: 0.,
        unsettled_draws: 0.,
        blocked: 0,
        first_shortfall: None,
        shortfall_cycles: 0,
        peak_queue: 0.,
        peak_minute: None,
    };
    // Work created while running: delayed move starts, move completions and draw settlements.
    let mut starts: Vec<(u32, usize)> = Vec::new();
    let mut completions: Vec<(u32, usize)> = Vec::new();
    let mut settlements: Vec<(u32, usize, f64)> = Vec::new();
    let mut next = 0usize;
    let mut steps = 0usize;
    loop {
        // The earliest pending item across the static list and the dynamic queues.
        let mut best: Option<(u32, Kind, usize)> = work.get(next).copied();
        let mut source = 0;
        for (k, (t, i)) in completions.iter().enumerate() {
            let c = (*t, Kind::MoveCompleted, *i);
            if best.is_none_or(|b| c < b) {
                best = Some(c);
                source = 1 + k;
            }
        }
        let offset = 1 + completions.len();
        for (k, (t, i, _)) in settlements.iter().enumerate() {
            let c = (*t, Kind::DrawSettled, *i);
            if best.is_none_or(|b| c < b) {
                best = Some(c);
                source = offset + k;
            }
        }
        let offset2 = offset + settlements.len();
        for (k, (t, i)) in starts.iter().enumerate() {
            let c = (*t, Kind::MoveStart, *i);
            if best.is_none_or(|b| c < b) {
                best = Some(c);
                source = offset2 + k;
            }
        }
        let Some((minute, kind, i)) = best else { break };
        if minute >= end {
            break;
        }
        steps += 1;
        if steps.is_multiple_of(1024) {
            check()?;
        }
        let delayed_start = source >= offset2 && source > 0;
        let mut amount = 0.;
        if source == 0 {
            next += 1;
        } else if source < offset {
            completions.remove(source - 1);
        } else if source < offset2 {
            amount = settlements.remove(source - offset).2;
        } else {
            starts.remove(source - offset2);
        }
        match kind {
            Kind::MoveCompleted => {
                let li = r
                    .lots
                    .iter()
                    .position(|l| l.id == r.moves[i].lot)
                    .expect("lot");
                sim.location[li] = r.moves[i].to.clone();
                sim.emit(minute, "move_completed", &r.moves[i].id, 0., &r.moves[i].to);
            }
            Kind::DrawSettled => {
                sim.cash += amount;
                sim.drawn += amount;
                sim.unsettled_draws -= amount;
                sim.emit(minute, "draw_settled", &r.draws[i].id, amount, "");
            }
            Kind::Receipt => {
                sim.cash += r.receipts[i].amount;
                sim.received += r.receipts[i].amount;
                sim.emit(
                    minute,
                    "receipt",
                    &r.receipts[i].id,
                    r.receipts[i].amount,
                    "",
                );
            }
            Kind::Payment => {
                let p = &r.payments[i];
                let s = r.systems.iter().find(|s| s.id == p.system).expect("system");
                let of_day = minute % DAY;
                // Last cycle of the day is the largest open + k*cycle not after close.
                let last = s.open + (s.close - s.open) / s.cycle_minutes * s.cycle_minutes;
                sim.after_cutoff[i] = s.closed_days.contains(&(minute / DAY)) || of_day > last;
                sim.arrived[i] = true;
                let detail = if sim.after_cutoff[i] {
                    "after_cutoff"
                } else {
                    ""
                };
                sim.emit(minute, "payment_requested", &p.id, p.amount, detail);
            }
            Kind::MoveStart => {
                let m = &r.moves[i];
                let li = r.lots.iter().position(|l| l.id == m.lot).expect("lot");
                if sim.pledged[li] || sim.location[li] == "in_transit" {
                    let reason = if sim.pledged[li] {
                        "already_pledged"
                    } else {
                        "collateral_in_transit"
                    };
                    sim.emit(minute, "move_blocked", &m.id, 0., reason);
                } else if !delayed_start && move_start[i] > minute {
                    starts.push((move_start[i], i));
                    sim.emit(minute, "move_waiting", &m.id, 0., "custodian_window_closed");
                } else {
                    sim.location[li] = "in_transit".into();
                    completions.push((minute + m.transfer_minutes, i));
                    sim.emit(minute, "move_started", &m.id, 0., &m.to);
                }
            }
            Kind::Draw => sim.draw(minute, i, &mut settlements),
            Kind::Cycle => sim.cycle(minute, i),
        }
    }
    if let Some(stop) = r.stop_minute {
        sim.emit(stop, "stopped", "", 0., "");
    }
    let requested: f64 = (0..r.payments.len())
        .filter(|i| sim.arrived[*i])
        .map(|i| r.payments[i].amount)
        .sum();
    let unpaid = sim.queued();
    let error = r.opening_cash + sim.received + sim.drawn - sim.paid - sim.cash;
    if !error.is_finite()
        || error.abs() > 1e-9 * 1_f64.max(r.opening_cash).max(sim.paid).max(sim.received)
        || sim.cash < -1e-9
        || (requested - sim.paid - unpaid).abs() > 1e-9 * 1_f64.max(requested)
    {
        return Err("intraday settlement failed to reconcile".into());
    }
    let payments: Vec<Value> = r
        .payments
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let unpaid = if sim.arrived[i] { sim.remaining[i] } else { 0. };
            json!({"id":p.id,"system":p.system,"minute":p.minute,"amount":p.amount,
                "requested":sim.arrived[i],"paid":if sim.arrived[i] {p.amount-sim.remaining[i]} else {0.},
                "unpaid":unpaid,"first_paid_minute":sim.first_paid[i],"settled_minute":sim.settled[i],
                "after_cutoff":sim.after_cutoff[i]})
        })
        .collect();
    Ok(
        json!({"schema":SCHEMA,"events":sim.events,"payments":payments,"summary":{
        "opening_cash":r.opening_cash,"closing_cash":sim.cash,"requested":requested,"paid":sim.paid,
        "unpaid":unpaid,"received":sim.received,"drawn":sim.drawn,"draws_unsettled":sim.unsettled_draws,
        "draws_blocked":sim.blocked,"first_shortfall_minute":sim.first_shortfall,
        "shortfall_cycles":sim.shortfall_cycles,"peak_queue":sim.peak_queue,"peak_queue_minute":sim.peak_minute,
        "stop_minute":r.stop_minute,"reconciliation_error":error}}),
    )
}
