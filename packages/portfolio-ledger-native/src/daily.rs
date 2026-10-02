//! Full daily research state machine; bounded table batches over a local pipe.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
#[path = "bin/report.rs"]
mod report;
use crate::types;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Write};
use types::*;

type R<T> = Result<T, String>;
// Dictionary codes avoid allocating contract labels for every daily reconciliation.
type Key = (usize, usize, usize);
type Changes = Vec<(String, String, f64)>;
fn entry(gl: &str, id: &str, v: f64) -> (String, String, f64) {
    (gl.into(), id.into(), v)
}
fn add(v: &mut Changes, gl: &str, id: &str, amount: f64) {
    if let Some(x) = v.iter_mut().find(|x| x.0 == gl && x.1 == id) {
        x.2 += amount;
    } else {
        v.push(entry(gl, id, amount));
    }
}
fn asset(p: &Position) -> bool {
    matches!(p.kind.as_str(), "loan" | "security" | "reverse_repo")
}
fn marked(p: &Position) -> bool {
    matches!(p.classification.as_str(), "afs" | "trading")
}
fn principal(p: &Position) -> &'static str {
    if asset(p) {
        "asset_principal"
    } else {
        "funding_principal"
    }
}
fn sign(p: &Position) -> f64 {
    if asset(p) {
        1.
    } else {
        -1.
    }
}
fn ratio(a: f64, b: f64) -> Option<f64> {
    if b > 0. {
        Some(a / b)
    } else {
        None
    }
}
fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-8_f64.max(1e-9 * a.abs().max(b.abs()))
}
fn sum_accurate(values: impl Iterator<Item = f64>) -> R<f64> {
    let mut parts: Vec<f64> = Vec::new();
    for mut x in values {
        if !x.is_finite() {
            return Err("nonfinite posting".into());
        }
        let mut keep = 0;
        for j in 0..parts.len() {
            let mut y = parts[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if !hi.is_finite() {
                return Err("posting overflow".into());
            }
            if lo != 0. {
                parts[keep] = lo;
                keep += 1;
            }
            x = hi;
        }
        parts.truncate(keep);
        parts.push(x);
    }
    Ok(parts.iter().rev().sum())
}

#[derive(Default, serde::Serialize)]
struct JournalColumns {
    transaction_number: Vec<usize>,
    scenario: Vec<usize>,
    day: Vec<usize>,
    account: Vec<usize>,
    event: Vec<usize>,
    gl_account: Vec<usize>,
    instrument_id: Vec<usize>,
    value: Vec<f64>,
}
#[derive(serde::Serialize)]
struct JournalMessage<'a> {
    table: &'static str,
    columns: &'a JournalColumns,
    dictionary: &'a [String],
}
struct Sink {
    out: io::BufWriter<Box<dyn Write + Send>>,
    rows: BTreeMap<String, Vec<Value>>,
    count: usize,
    journal: JournalColumns,
    dictionary: Vec<String>,
    ids: HashMap<String, usize>,
    sent: usize,
    reports: report::Reports,
}
impl Sink {
    fn new(output: Box<dyn Write + Send>) -> Self {
        Self {
            out: io::BufWriter::new(output),
            rows: BTreeMap::new(),
            count: 0,
            journal: JournalColumns::default(),
            dictionary: vec![],
            ids: HashMap::new(),
            sent: 0,
            reports: report::Reports::default(),
        }
    }
    fn intern(&mut self, text: &str) -> usize {
        if let Some(&id) = self.ids.get(text) {
            return id;
        }
        let id = self.dictionary.len();
        self.dictionary.push(text.into());
        self.ids.insert(text.into(), id);
        id
    }
    fn journal(
        &mut self,
        seq: usize,
        scenario: &str,
        day: usize,
        account: &str,
        event: &str,
        gl: &str,
        id: &str,
        value: f64,
    ) -> R<()> {
        let (s, a, e, g, i) = (
            self.intern(scenario),
            self.intern(account),
            self.intern(event),
            self.intern(gl),
            self.intern(id),
        );
        self.reports.posting(a, g, i, value);
        let j = &mut self.journal;
        j.transaction_number.push(seq);
        j.scenario.push(s);
        j.day.push(day);
        j.account.push(a);
        j.event.push(e);
        j.gl_account.push(g);
        j.instrument_id.push(i);
        j.value.push(value);
        self.count += 1;
        if self.count >= 16384 {
            self.flush()?;
        }
        Ok(())
    }
    fn emit(&mut self, table: &str, row: Value) -> R<()> {
        if table == "ledger" {
            self.reports.event(
                row["scenario"].as_str().ok_or("missing scenario")?,
                row["account"].as_str().ok_or("missing account")?,
                row["event"].as_str().ok_or("missing event")?,
                [
                    row["cash"].as_f64().ok_or("invalid cash")?,
                    row["earnings"].as_f64().ok_or("invalid earnings")?,
                    row["aoci"].as_f64().ok_or("invalid aoci")?,
                ],
            );
        }
        self.rows.entry(table.into()).or_default().push(row);
        self.count += 1;
        if self.count >= 16384 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> R<()> {
        if !self.journal.value.is_empty() {
            serde_json::to_writer(
                &mut self.out,
                &JournalMessage {
                    table: "journal",
                    columns: &self.journal,
                    dictionary: &self.dictionary[self.sent..],
                },
            )
            .map_err(|e| e.to_string())?;
            self.out.write_all(b"\n").map_err(|e| e.to_string())?;
            self.sent = self.dictionary.len();
            self.journal = JournalColumns::default();
        }
        for (name, rows) in self.rows.iter_mut() {
            if rows.is_empty() {
                continue;
            }
            serde_json::to_writer(&mut self.out, &json!({"table":name,"rows":rows}))
                .map_err(|e| e.to_string())?;
            self.out.write_all(b"\n").map_err(|e| e.to_string())?;
            rows.clear();
        }
        self.count = 0;
        self.out.flush().map_err(|e| e.to_string())
    }
}

#[derive(Clone)]
struct Claim {
    id: String,
    account: usize,
    position: usize,
    face: f64,
    balance: f64,
    rate: f64,
    due: usize,
}
struct Sim<'a> {
    spec: &'a Spec,
    scenario: Scenario,
    severity: f64,
    detail: bool,
    sink: &'a mut Sink,
    accounts: Vec<Account>,
    positions: Vec<Position>,
    netting: Vec<NettingSet>,
    aid: HashMap<String, usize>,
    pid: HashMap<String, usize>,
    market: Vec<f64>,
    marks: Vec<f64>,
    accrued: Vec<f64>,
    commitments: Vec<f64>,
    schedule_balance: Vec<f64>,
    aoci: Vec<f64>,
    extra_assets: Vec<f64>,
    extra_debt: Vec<f64>,
    funding_interest: Vec<f64>,
    restricted: Vec<f64>,
    opening_equity: Vec<f64>,
    minimum_cash: Vec<f64>,
    htm_sold: Vec<f64>,
    gl: HashMap<Key, f64>,
    earnings: HashMap<Key, f64>,
    sequence: usize,
    aggregate: Vec<(usize, String, [f64; 3])>,
    aggregate_index: HashMap<(usize, String), usize>,
    claims: Vec<Claim>,
    claims_by_position: Vec<Vec<usize>>,
    claim_maturities: HashMap<usize, Vec<usize>>,
    opening_links: Vec<Vec<usize>>,
    recovery_claims: HashMap<(usize, String), f64>,
    recoveries: HashMap<usize, Vec<(usize, String, f64)>>,
    intercompany: HashMap<Key, f64>,
    flows: HashMap<usize, Vec<Cashflow>>,
    scheduled_ids: HashSet<String>,
    streamed_flows: Option<&'a crate::flow_store::FlowStore>,
    margin_targets: HashMap<usize, Vec<(usize, f64)>>,
    pending: HashMap<usize, Vec<usize>>,
    used: Vec<f64>,
    scheduled: HashSet<usize>,
    first: HashMap<(usize, String), usize>,
    max_error: f64,
}
impl<'a> Sim<'a> {
    fn new(
        spec: &'a Spec,
        scenario: Scenario,
        severity: f64,
        detail: bool,
        sink: &'a mut Sink,
        streamed_flows: Option<&'a crate::flow_store::FlowStore>,
    ) -> R<Self> {
        let n = spec.positions.len();
        let a = spec.accounts.len();
        let aid: HashMap<_, _> = spec
            .accounts
            .iter()
            .enumerate()
            .map(|(i, x)| (x.id.to_string(), i))
            .collect();
        let pid: HashMap<_, _> = spec
            .positions
            .iter()
            .enumerate()
            .map(|(i, x)| (x.id.to_string(), i))
            .collect();
        let mut links = vec![vec![]; n];
        for (i, p) in spec.positions.iter().enumerate() {
            if !p.collateral_position.is_empty() {
                links[*pid
                    .get(p.collateral_position.as_str())
                    .ok_or("invalid collateral")?]
                .push(i);
            }
        }
        let mut flows: HashMap<usize, Vec<Cashflow>> = HashMap::new();
        for f in &spec.cashflows {
            if f.scenario == "all" || f.scenario == scenario.name {
                flows.entry(f.day).or_default().push(f.clone());
            }
        }
        let mut sim = Self {
            spec,
            scenario,
            severity,
            detail,
            sink,
            accounts: spec.accounts.clone(),
            positions: spec.positions.clone(),
            netting: spec.netting_sets.clone(),
            aid,
            pid,
            market: spec
                .positions
                .iter()
                .map(|p| p.opening_market_price)
                .collect(),
            marks: vec![0.; n],
            accrued: spec.positions.iter().map(|p| p.opening_accrued).collect(),
            commitments: spec.positions.iter().map(|p| p.commitment).collect(),
            schedule_balance: spec.positions.iter().map(|p| p.balance).collect(),
            aoci: vec![0.; a],
            extra_assets: vec![0.; a],
            extra_debt: vec![0.; a],
            funding_interest: vec![0.; a],
            restricted: vec![0.; a],
            opening_equity: spec.accounts.iter().map(|x| x.equity).collect(),
            minimum_cash: spec.accounts.iter().map(|x| x.cash).collect(),
            htm_sold: vec![0.; n],
            gl: HashMap::new(),
            earnings: HashMap::new(),
            sequence: 0,
            aggregate: vec![],
            aggregate_index: HashMap::new(),
            claims: vec![],
            claims_by_position: vec![vec![]; n],
            claim_maturities: HashMap::new(),
            opening_links: links,
            recovery_claims: HashMap::new(),
            recoveries: HashMap::new(),
            intercompany: HashMap::new(),
            flows,
            scheduled_ids: spec
                .cashflows
                .iter()
                .map(|f| f.position.clone())
                .chain(
                    streamed_flows
                        .into_iter()
                        .flat_map(|s| s.scheduled(&spec.positions)),
                )
                .collect(),
            streamed_flows,
            margin_targets: HashMap::new(),
            pending: HashMap::new(),
            used: vec![0.; spec.policies.len()],
            scheduled: HashSet::new(),
            first: HashMap::new(),
            max_error: 0.,
        };
        for (i, p) in sim.positions.iter_mut().enumerate() {
            if p.start_day > 0 {
                p.balance = 0.;
                p.book_adjustment = 0.;
            }
            if p.kind == "security" && p.classification == "afs" {
                sim.marks[i] = p.balance * (sim.market[i] - 1.) - p.book_adjustment;
                sim.aoci[sim.aid[p.account.as_str()]] += sim.marks[i];
            }
        }
        for i in 0..a {
            sim.opening_equity[i] -= sim.aoci[i];
        }
        Ok(sim)
    }
    fn raw_post(&mut self, day: usize, a: usize, event: &str, entries: Changes) -> R<()> {
        let entries: Changes = entries.into_iter().filter(|x| x.2 != 0.).collect();
        if entries.is_empty() {
            return Ok(());
        }
        let scale: f64 = entries.iter().map(|x| x.2.abs()).sum();
        if !scale.is_finite()
            || sum_accurate(entries.iter().map(|x| x.2))?.abs() > 1e-9 * scale.max(1.)
        {
            return Err(format!("unbalanced {event} day {day}"));
        }
        self.sequence += 1;
        for (gl, id, value) in entries {
            let balance = self
                .gl
                .entry((a, self.sink.intern(&gl), self.sink.intern(&id)))
                .or_default();
            *balance += value;
            if !balance.is_finite() {
                return Err("GL overflow".into());
            }
            if self.detail {
                self.sink.journal(
                    self.sequence,
                    &self.scenario.name,
                    day,
                    &self.accounts[a].id,
                    event,
                    &gl,
                    &id,
                    value,
                )?;
            }
        }
        if self.gl.len() > 2_000_000 {
            return Err("native GL key budget exceeded".into());
        }
        Ok(())
    }
    fn post(
        &mut self,
        day: usize,
        a: usize,
        event: &str,
        cash: f64,
        earnings: f64,
        oci: f64,
        mut changes: Changes,
        id: &str,
    ) -> R<()> {
        let egl = if event == "dividend" {
            "equity_distributions".into()
        } else {
            format!("pnl:{event}")
        };
        add(&mut changes, "cash", "", cash);
        add(&mut changes, &egl, id, -earnings);
        add(&mut changes, "oci", "", -oci);
        self.raw_post(day, a, event, changes)?;
        *self
            .earnings
            .entry((a, self.sink.intern(&egl), self.sink.intern(id)))
            .or_default() -= earnings;
        self.accounts[a].cash += cash;
        self.accounts[a].equity += earnings + oci;
        self.aoci[a] += oci;
        let key = (a, event.to_owned());
        let j = if let Some(&j) = self.aggregate_index.get(&key) {
            j
        } else {
            let j = self.aggregate.len();
            self.aggregate.push((a, event.into(), [0.; 3]));
            self.aggregate_index.insert(key, j);
            j
        };
        let r = &mut self.aggregate[j].2;
        r[0] += cash;
        r[1] += earnings;
        r[2] += oci;
        Ok(())
    }
    fn opening(&mut self) -> R<()> {
        for a in 0..self.accounts.len() {
            let mut e = vec![
                entry("cash", "", self.accounts[a].cash),
                entry("opening_equity", "", -self.opening_equity[a]),
                entry("oci", "", -self.aoci[a]),
            ];
            for (i, p) in self.positions.iter().enumerate() {
                if self.aid[p.account.as_str()] != a {
                    continue;
                }
                e.extend([
                    entry(principal(p), &p.id, sign(p) * p.balance),
                    entry("book_adjustment", &p.id, sign(p) * p.book_adjustment),
                    entry("allowance", &p.id, -p.allowance),
                    entry("accrued_interest", &p.id, sign(p) * self.accrued[i]),
                ]);
                if p.kind == "security" && marked(p) {
                    e.push(entry(
                        "fair_value_adjustment",
                        &p.id,
                        p.balance * (self.market[i] - 1.) - p.book_adjustment,
                    ));
                }
            }
            for n in &self.netting {
                if self.aid[&n.account] == a {
                    e.extend([
                        entry("derivative_value", &n.id, n.fair_value),
                        entry("posted_margin", &n.id, n.posted_margin),
                        entry("received_margin", &n.id, -n.received_margin),
                    ]);
                }
            }
            self.raw_post(0, a, "opening", e)?;
        }
        self.observe(0)
    }
    fn observe(&mut self, day: usize) -> R<()> {
        let mut expected = self.earnings.clone();
        for a in 0..self.accounts.len() {
            for (g, v) in [
                ("cash", self.accounts[a].cash),
                ("restricted_cash", self.restricted[a]),
                ("opening_equity", -self.opening_equity[a]),
                ("oci", -self.aoci[a]),
            ] {
                expected.insert((a, self.sink.intern(g), self.sink.intern("")), v);
            }
        }
        for (i, p) in self.positions.iter().enumerate() {
            let a = self.aid[p.account.as_str()];
            for (g, v) in [
                (principal(p), sign(p) * p.balance),
                ("book_adjustment", sign(p) * p.book_adjustment),
                ("allowance", -p.allowance),
                ("accrued_interest", sign(p) * self.accrued[i]),
            ] {
                expected.insert((a, self.sink.intern(g), self.sink.intern(&p.id)), v);
            }
            if p.kind == "security" && marked(p) {
                expected.insert(
                    (
                        a,
                        self.sink.intern("fair_value_adjustment"),
                        self.sink.intern(&p.id),
                    ),
                    p.balance * (self.market[i] - 1.) - p.book_adjustment,
                );
            }
        }
        for n in &self.netting {
            let a = self.aid[&n.account];
            for (g, v) in [
                ("derivative_value", n.fair_value),
                ("posted_margin", n.posted_margin),
                ("received_margin", -n.received_margin),
            ] {
                expected.insert((a, self.sink.intern(g), self.sink.intern(&n.id)), v);
            }
        }
        for ((a, id), v) in &self.recovery_claims {
            expected.insert(
                (
                    *a,
                    self.sink.intern("recovery_receivable"),
                    self.sink.intern(id),
                ),
                *v,
            );
        }
        expected.extend(self.intercompany.iter().map(|(k, v)| (*k, *v)));
        for c in &self.claims {
            expected.insert(
                (
                    c.account,
                    self.sink.intern("secured_funding"),
                    self.sink.intern(&c.id),
                ),
                -c.balance,
            );
        }
        for (k, v) in &self.gl {
            if !near(*v, *expected.get(k).unwrap_or(&0.)) {
                return Err(format!(
                    "subledger mismatch day {day}: {k:?} {v} vs {:?}",
                    expected.get(k)
                ));
            }
        }
        for (k, v) in &expected {
            if !near(*v, *self.gl.get(k).unwrap_or(&0.)) {
                return Err(format!("missing GL day {day}: {k:?}"));
            }
        }
        if self.detail && (day == 0 || day.is_multiple_of(30) || day == self.spec.horizon_days) {
            for (i, p) in self.positions.iter().enumerate() {
                self.sink.emit("exposures",json!({"scenario":self.scenario.name,"day":day,"account":p.account,"position":p.id,"kind":p.kind,"balance":p.balance,"allowance":p.allowance,"watch_fraction":p.watch_fraction,"commitment":p.commitment,"market_factor":self.market[i],"encumbered_fraction":p.encumbered_fraction,"accrued_interest":self.accrued[i]}))?;
            }
        }
        for ai in 0..self.accounts.len() {
            let a = &self.accounts[ai];
            let mut assets = a.cash + self.restricted[ai] + self.extra_assets[ai];
            let mut liabilities = self.extra_debt[ai];
            let (
                mut rwa,
                mut hqla,
                mut outflow,
                mut asf,
                mut rsf,
                mut collateral,
                mut undrawn,
                mut l2,
                mut inflow,
                mut htm,
            ) = (
                0.,
                a.cash.max(0.),
                0.,
                a.equity.max(0.),
                0.,
                0.,
                0.,
                0.,
                0.,
                0.,
            );
            for (i, p) in self.positions.iter().enumerate() {
                if self.aid[p.account.as_str()] != ai {
                    continue;
                }
                if asset(p) {
                    let carrying = if p.kind == "security" && marked(p) {
                        p.balance * self.market[i]
                    } else {
                        p.balance + p.book_adjustment - p.allowance
                    };
                    assets += carrying;
                    assets += self.accrued[i];
                    rwa += carrying.max(0.) * p.risk_weight * (1. + p.watch_fraction);
                    rsf += carrying.max(0.) * p.rsf_weight;
                    let free = p.balance * (1. - p.encumbered_fraction).max(0.) * self.market[i];
                    if p.hqla_level == "level2a" {
                        l2 += free * p.hqla_weight;
                    } else {
                        hqla += free * p.hqla_weight;
                    }
                    inflow += p.balance * p.lcr_inflow_weight;
                    collateral += p.balance
                        * (p.eligible_fraction - p.encumbered_fraction).max(0.)
                        * self.market[i]
                        * (1.
                            - (p.haircut
                                + self.scenario.haircut_add
                                    * self.severity
                                    * f64::from(day >= self.scenario.start_day))
                            .min(0.99));
                    undrawn += p.commitment;
                    rwa += p.commitment * p.risk_weight;
                } else {
                    liabilities += p.balance + p.book_adjustment;
                    liabilities += self.accrued[i];
                    asf += p.balance * p.asf_weight;
                    outflow += p.balance * p.lcr_outflow_weight;
                }
                if p.classification == "htm" {
                    htm += p.balance + p.book_adjustment;
                }
            }
            for n in &self.netting {
                if self.aid[&n.account] == ai {
                    assets += n.fair_value.max(0.) + n.posted_margin;
                    liabilities += (-n.fair_value).max(0.) + n.received_margin;
                    rwa += (n.fair_value + n.posted_margin - n.received_margin).max(0.)
                        * n.risk_weight;
                    rsf += n.posted_margin;
                }
            }
            rwa += self.extra_assets[ai];
            rsf += self.extra_assets[ai];
            hqla += l2.min(hqla * 0.4 / (1. - 0.4));
            outflow = (outflow - inflow.min(0.75 * outflow)).max(0.);
            let error = assets - liabilities - a.equity;
            self.max_error = self.max_error.max(error.abs());
            if !error.is_finite() || error.abs() > 1e-8 * 1_f64.max(assets.abs()).max(liabilities) {
                return Err(format!("balance sheet mismatch day {day}: {error}"));
            }
            let cet1 =
                a.equity - a.capital_deductions - if a.include_aoci { 0. } else { self.aoci[ai] };
            let exposure = assets.max(0.) + undrawn;
            self.minimum_cash[ai] = self.minimum_cash[ai].min(a.cash);
            let margins = [
                ("cash", a.cash - a.cash_floor),
                ("cet1", cet1 - a.cet1_floor * rwa),
                ("leverage", cet1 - a.leverage_floor * exposure),
                ("lcr", hqla - a.lcr_floor * outflow),
                ("nsfr", asf - a.nsfr_floor * rsf),
                ("htm", a.htm_asset_limit * assets.max(0.) - htm),
            ];
            for (metric, v) in margins {
                if v < -1e-9 && !self.first.contains_key(&(ai, metric.into())) {
                    self.first.insert((ai, metric.into()), day);
                    if self.detail {
                        self.sink.emit("breaches",json!({"scenario":self.scenario.name,"account":a.id,"day":day,"metric":metric}))?;
                    }
                }
            }
            if self.detail && (day <= 30 || day.is_multiple_of(30) || day == self.spec.horizon_days)
            {
                let mut row = json!({"scenario":self.scenario.name,"account":a.id,"entity":a.entity,"currency":a.currency,"day":day,"cash":a.cash,"restricted_cash":self.restricted[ai],"cash_floor":a.cash_floor,"assets":assets,"liabilities":liabilities,"equity":a.equity,"aoci":self.aoci[ai],"cet1":cet1,"rwa":rwa,"cet1_ratio":ratio(cet1,rwa),"leverage_ratio":ratio(cet1,exposure),"usable_collateral":collateral,"undrawn":undrawn,"hqla_proxy":hqla,"lcr_proxy":ratio(hqla,outflow),"nsfr_proxy":ratio(asf,rsf),"reconciliation_error":error});
                for (m, v) in margins {
                    row[format!("{m}_headroom")] = json!(v);
                }
                self.sink.emit("path", row)?;
            }
        }
        Ok(())
    }
}

impl Sim<'_> {
    fn repay_claims(&mut self, day: usize, i: usize, fraction: f64, maturity: bool) -> R<()> {
        for ci in self.claims_by_position[i].clone() {
            let c = self.claims[ci].clone();
            if c.balance == 0. || fraction == 0. {
                continue;
            }
            let repay = c.balance * fraction;
            let proceeds = c.face * fraction;
            self.restricted[c.account] -= proceeds;
            self.extra_debt[c.account] -= repay;
            self.funding_interest[c.account] -= repay * c.rate;
            self.claims[ci].balance -= repay;
            self.claims[ci].face -= proceeds;
            self.post(
                day,
                c.account,
                if maturity {
                    "collateral_maturity_repayment"
                } else {
                    "collateral_principal_repayment"
                },
                proceeds - repay,
                0.,
                0.,
                vec![
                    entry("restricted_cash", "", -proceeds),
                    entry("secured_funding", &c.id, repay),
                ],
                &c.id,
            )?;
        }
        Ok(())
    }
    fn position_day(
        &mut self,
        day: usize,
        i: usize,
        stress: f64,
        credit_mult: f64,
        migration_mult: f64,
        processed: &HashSet<usize>,
    ) -> R<()> {
        let mut p = self.positions[i].clone();
        let a = self.aid[p.account.as_str()];
        let rate_shift = self.scenario.rate_shift * stress;
        if p.start_day > day {
            return Ok(());
        }
        if p.start_day == day {
            p.balance = self.spec.positions[i].balance;
            p.book_adjustment = self.spec.positions[i].book_adjustment;
            self.post(
                day,
                a,
                "origination",
                -sign(&p) * (p.balance + p.book_adjustment),
                0.,
                0.,
                vec![
                    entry(principal(&p), &p.id, sign(&p) * p.balance),
                    entry("book_adjustment", &p.id, sign(&p) * p.book_adjustment),
                ],
                &p.id,
            )?;
            if p.kind == "security" && p.balance != 0. {
                self.market[i] = (p.balance + p.book_adjustment) / p.balance;
            }
        }
        if p.kind == "security" {
            let target = p.opening_market_price
                * (1. - p.duration * (rate_shift + self.scenario.spread_shift * stress)).max(0.01);
            let change = p.balance * (target - self.market[i]);
            self.market[i] = target;
            if change != 0. && marked(&p) {
                let oci = if p.classification == "afs" {
                    change
                } else {
                    0.
                };
                self.marks[i] += oci;
                self.post(
                    day,
                    a,
                    "security_mark",
                    0.,
                    change - oci,
                    oci,
                    vec![entry("fair_value_adjustment", &p.id, change)],
                    &p.id,
                )?;
            }
        }
        if p.kind == "loan" {
            let draw = if day < self.scenario.start_day + 30 {
                p.commitment.min(
                    self.commitments[i] * p.draw_fraction * self.scenario.draw_multiplier * stress
                        / 30.,
                )
            } else {
                0.
            };
            p.balance += draw;
            p.commitment -= draw;
            if draw != 0. {
                self.post(
                    day,
                    a,
                    "commitment_draw",
                    -draw,
                    0.,
                    0.,
                    vec![entry(principal(&p), &p.id, draw)],
                    &p.id,
                )?;
            }
            let migrate = (1. - p.watch_fraction)
                * (1. - (1. - (p.migration_rate * migration_mult).min(0.999999)).powf(1. / 365.));
            p.watch_fraction += migrate;
            let pd = (p.annual_pd
                * credit_mult
                * (1. + p.watch_fraction * (p.watch_pd_multiplier - 1.)))
                .min(0.999999);
            let default = p.balance * (1. - (1. - pd).powf(1. / 365.));
            let lost = if p.balance != 0. {
                self.accrued[i] * default / p.balance
            } else {
                0.
            };
            if lost != 0. {
                self.accrued[i] -= lost;
                self.post(
                    day,
                    a,
                    "default_interest_writeoff",
                    0.,
                    -lost,
                    0.,
                    vec![entry("accrued_interest", &p.id, -lost)],
                    &p.id,
                )?;
            }
            let adj = if p.balance != 0. {
                p.book_adjustment * default / p.balance
            } else {
                0.
            };
            p.book_adjustment -= adj;
            if adj != 0. {
                self.post(
                    day,
                    a,
                    "default_book_writeoff",
                    0.,
                    -adj,
                    0.,
                    vec![entry("book_adjustment", &p.id, -adj)],
                    &p.id,
                )?;
            }
            let lgd = (p.lgd + self.scenario.lgd_add * stress).clamp(0., 1.);
            let loss = default * lgd;
            let recovery = default * (1. - lgd);
            p.balance -= default;
            p.allowance -= loss;
            let cid = format!("{}:default:{day}", p.id);
            if recovery != 0. {
                self.extra_assets[a] += recovery;
                *self.recovery_claims.entry((a, cid.clone())).or_default() += recovery;
                self.recoveries
                    .entry(day + p.recovery_days.max(1))
                    .or_default()
                    .push((a, cid.clone(), recovery));
            }
            if default != 0. {
                let mut changes = vec![
                    entry(principal(&p), &p.id, -default),
                    entry("allowance", &p.id, loss),
                ];
                if recovery != 0. {
                    changes.push(entry("recovery_receivable", &cid, recovery));
                }
                self.post(day, a, "credit_chargeoff", 0., 0., 0., changes, &p.id)?;
            }
            let target = p.balance * pd * lgd;
            let provision = target - p.allowance;
            p.allowance = target;
            if provision != 0. {
                self.post(
                    day,
                    a,
                    "credit_provision",
                    0.,
                    -provision,
                    0.,
                    vec![entry("allowance", &p.id, -provision)],
                    &p.id,
                )?;
            }
            if default != 0. && self.detail {
                self.sink.emit("ledger",json!({"scenario":self.scenario.name,"day":day,"account":p.account,"event":"default_gross","cash":0.,"earnings":0.,"aoci":0.,"memo_amount":default}))?;
            }
        }
        let scheduled = self.scheduled_ids.contains(p.id.as_str());
        let s = sign(&p);
        let interest = if scheduled {
            0.
        } else {
            p.balance * (p.rate + p.floating_beta * rate_shift) / 365.
        };
        if interest != 0. {
            self.accrued[i] += interest;
            self.post(
                day,
                a,
                if s == 1. {
                    "interest_income"
                } else {
                    "interest_expense"
                },
                0.,
                s * interest,
                0.,
                vec![entry("accrued_interest", &p.id, s * interest)],
                &p.id,
            )?;
        }
        if !scheduled && (day.is_multiple_of(p.payment_interval_days) || p.maturity_day == day) {
            self.post(
                day,
                a,
                "interest_settlement",
                s * self.accrued[i],
                0.,
                0.,
                vec![entry("accrued_interest", &p.id, -s * self.accrued[i])],
                &p.id,
            )?;
            self.accrued[i] = 0.;
        }
        if p.kind == "deposit" {
            let sensitivity = (0.25 + 0.75 * p.uninsured_fraction)
                * (1. + p.concentration)
                * (1. + 0.5 * p.digital_fraction)
                * (1. - 0.5 * p.operational_fraction);
            let flight = self.scenario.deposit_flight * stress * sensitivity;
            let competition = (rate_shift * (1. - p.floating_beta)).max(0.);
            let runoff = p.balance
                * (1.
                    - (1.
                        - ((if scheduled { 0. } else { p.monthly_runoff })
                            + flight
                            + competition)
                            .min(0.999999))
                    .powf(1. / 30.));
            p.balance -= runoff;
            if runoff != 0. {
                self.post(
                    day,
                    a,
                    "deposit_withdrawal",
                    -runoff,
                    0.,
                    0.,
                    vec![entry(principal(&p), &p.id, runoff)],
                    &p.id,
                )?;
            }
        }
        if p.maturity_day == day && !scheduled {
            if s == 1. {
                let balance = p.balance;
                let gain = if p.kind == "security" && marked(&p) {
                    balance * (1. - self.market[i])
                } else {
                    0.
                };
                let held = balance * p.encumbered_fraction;
                self.restricted[a] += held;
                let adjustment = p.book_adjustment;
                let mut changes = vec![
                    entry(principal(&p), &p.id, -balance),
                    entry("allowance", &p.id, p.allowance),
                    entry("book_adjustment", &p.id, -adjustment),
                    entry("restricted_cash", "", held),
                ];
                if marked(&p) {
                    changes.push(entry("fair_value_adjustment", &p.id, gain + adjustment));
                }
                self.post(
                    day,
                    a,
                    "asset_maturity",
                    balance - held,
                    p.allowance
                        + if p.classification == "trading" {
                            gain
                        } else {
                            -adjustment
                        },
                    -self.marks[i],
                    changes,
                    &p.id,
                )?;
                self.repay_claims(day, i, 1., true)?;
                for di in self.opening_links[i].clone() {
                    let d = self.positions[di].clone();
                    if d.pledged_face == 0. {
                        continue;
                    }
                    let proceeds = d.pledged_face;
                    let amount = d.balance;
                    if !processed.contains(&di) && !self.scheduled_ids.contains(d.id.as_str()) {
                        let interest = d.balance * (d.rate + d.floating_beta * rate_shift) / 365.;
                        self.accrued[di] += interest;
                        self.post(
                            day,
                            a,
                            "interest_expense",
                            0.,
                            -interest,
                            0.,
                            vec![entry("accrued_interest", &d.id, -interest)],
                            &d.id,
                        )?;
                    }
                    self.restricted[a] -= proceeds;
                    self.post(
                        day,
                        a,
                        "opening_collateral_repayment",
                        proceeds - amount,
                        d.book_adjustment,
                        0.,
                        vec![
                            entry("restricted_cash", "", -proceeds),
                            entry(principal(&d), &d.id, amount),
                            entry("book_adjustment", &d.id, d.book_adjustment),
                        ],
                        &d.id,
                    )?;
                    if self.accrued[di] != 0. {
                        self.post(
                            day,
                            a,
                            "interest_settlement",
                            -self.accrued[di],
                            0.,
                            0.,
                            vec![entry("accrued_interest", &d.id, self.accrued[di])],
                            &d.id,
                        )?;
                        self.accrued[di] = 0.;
                    }
                    self.positions[di].balance = 0.;
                    self.positions[di].pledged_face = 0.;
                    self.positions[di].book_adjustment = 0.;
                }
                p.balance = 0.;
                p.allowance = 0.;
                self.marks[i] = 0.;
                p.book_adjustment = 0.;
                p.encumbered_fraction = 0.;
                p.commitment = 0.;
            } else {
                let rollover = (p.rollover - self.scenario.rollover_loss * stress).clamp(0., 1.);
                let repay = p.balance * (1. - rollover);
                if !p.collateral_position.is_empty() && p.balance != 0. {
                    let released = p.pledged_face * (1. - rollover);
                    let ci = self.pid[p.collateral_position.as_str()];
                    let c = &mut self.positions[ci];
                    c.encumbered_fraction = if c.balance != 0. {
                        (c.encumbered_fraction - released / c.balance).max(0.)
                    } else {
                        0.
                    };
                    p.pledged_face -= released;
                }
                p.balance -= repay;
                p.maturity_day = 0;
                p.rate += (self.scenario.spread_shift * stress).max(0.);
                self.post(
                    day,
                    a,
                    "funding_maturity",
                    -repay,
                    0.,
                    0.,
                    vec![entry(principal(&p), &p.id, repay)],
                    &p.id,
                )?;
            }
        }
        self.positions[i] = p;
        Ok(())
    }
    fn cashflow(&mut self, day: usize, f: Cashflow) -> R<()> {
        let i = self.pid[&f.position];
        let mut p = self.positions[i].clone();
        let a = self.aid[p.account.as_str()];
        let s = sign(&p);
        let reference = self.schedule_balance[i];
        let factor = if reference > 1e-12 {
            p.balance / reference
        } else {
            0.
        };
        let amount = (f.principal * factor).min(p.balance);
        self.schedule_balance[i] = (reference - f.principal).max(0.);
        let fraction = if p.balance != 0. {
            amount / p.balance
        } else {
            0.
        };
        let held = amount * p.encumbered_fraction;
        self.restricted[a] += held;
        let amort = f.book_amortization * factor;
        let release = if marked(&p) {
            -amount * (self.market[i] - 1.) - amort
        } else {
            0.
        };
        let oci = if p.classification == "afs" {
            release
        } else {
            0.
        };
        self.marks[i] += oci;
        p.balance -= amount;
        let interest = f.cash_interest * factor;
        let accrual = f.accrual_interest * factor;
        p.book_adjustment += amort;
        self.accrued[i] += accrual - interest;
        let mut changes = vec![
            entry(principal(&p), &p.id, -s * amount),
            entry("accrued_interest", &p.id, s * (accrual - interest)),
            entry("book_adjustment", &p.id, s * amort),
            entry("restricted_cash", "", held),
        ];
        if release != 0. {
            changes.push(entry("fair_value_adjustment", &p.id, release));
        }
        self.post(
            day,
            a,
            "contractual_cashflow",
            s * (amount + interest) - held,
            s * (accrual + amort)
                + if p.classification == "trading" {
                    release
                } else {
                    0.
                },
            oci,
            changes,
            &p.id,
        )?;
        // Provisioning precedes scheduled principal. Release the paid share on
        // the same day so a closing-date payoff cannot retain a stale allowance.
        let allowance_release = if p.kind == "loan" {
            p.allowance * fraction
        } else {
            0.
        };
        if allowance_release != 0. {
            p.allowance -= allowance_release;
            self.post(
                day,
                a,
                "repayment_allowance_release",
                0.,
                allowance_release,
                0.,
                vec![entry("allowance", &p.id, allowance_release)],
                &p.id,
            )?;
        }
        self.repay_claims(day, i, fraction, false)?;
        for di in self.opening_links[i].clone() {
            if fraction == 0. {
                continue;
            }
            let d = self.positions[di].clone();
            let repay = d.balance * fraction;
            let proceeds = d.pledged_face * fraction;
            self.positions[di].balance -= repay;
            self.positions[di].pledged_face -= proceeds;
            self.restricted[a] -= proceeds;
            self.post(
                day,
                a,
                "opening_collateral_principal_repayment",
                proceeds - repay,
                0.,
                0.,
                vec![
                    entry("restricted_cash", "", -proceeds),
                    entry(principal(&d), &d.id, repay),
                ],
                &d.id,
            )?;
        }
        self.positions[i] = p;
        Ok(())
    }
    fn log_action(
        &mut self,
        day: usize,
        p: &Policy,
        status: &str,
        amount: f64,
        reason: &str,
    ) -> R<()> {
        if self.detail {
            self.sink.emit("actions",json!({"scenario":self.scenario.name,"day":day,"policy":p.id,"account":p.account,"destination":p.destination,"kind":p.kind,"status":status,"amount":amount,"reason":reason}))?;
        }
        Ok(())
    }
    fn policies(&mut self, day: usize, stress: f64) -> R<()> {
        for pi in 0..self.spec.policies.len() {
            let p = &self.spec.policies[pi];
            let a = self.aid[if p.kind == "transfer" {
                &p.destination
            } else {
                &p.account
            }];
            if self.accounts[a].cash < p.trigger_cash
                && self.used[pi] < p.limit
                && !self.scheduled.contains(&pi)
            {
                self.pending
                    .entry(day + p.delay_days.max(1))
                    .or_default()
                    .push(pi);
                self.scheduled.insert(pi);
                self.log_action(day, p, "scheduled", 0., "")?;
            }
        }
        for pi in self.pending.remove(&day).unwrap_or_default() {
            self.scheduled.remove(&pi);
            let p = self.spec.policies[pi].clone();
            if day >= self.scenario.start_day
                && self.severity > 0.
                && day
                    < self.scenario.start_day
                        + (self.scenario.outage_days as f64 * self.severity).round_ties_even()
                            as usize
            {
                self.pending.entry(day + 1).or_default().push(pi);
                self.scheduled.insert(pi);
                self.log_action(day, &p, "delayed", 0., "operational outage")?;
                continue;
            }
            let a = self.aid[p.account.as_str()];
            let ta = if p.kind == "transfer" {
                self.aid[&p.destination]
            } else {
                a
            };
            let need = (p.trigger_cash - self.accounts[ta].cash).max(0.);
            let mut amount = need.min(p.limit - self.used[pi]);
            if amount == 0. {
                self.log_action(day, &p, "skipped", 0., "trigger cleared or limit exhausted")?;
                continue;
            }
            if p.kind == "cut_dividend" {
                self.accounts[a].annual_dividends = 0.;
                self.used[pi] = p.limit;
                self.log_action(day, &p, "executed", 0., "future dividends suspended")?;
                continue;
            }
            if p.kind == "transfer" {
                amount = amount
                    .min((self.accounts[a].cash - self.accounts[a].cash_floor.max(0.)).max(0.));
                self.extra_assets[a] += amount;
                self.extra_debt[ta] += amount;
                let cid = format!("{}:{day}", p.id);
                *self
                    .intercompany
                    .entry((
                        a,
                        self.sink.intern("intercompany_receivable"),
                        self.sink.intern(&cid),
                    ))
                    .or_default() += amount;
                *self
                    .intercompany
                    .entry((
                        ta,
                        self.sink.intern("intercompany_payable"),
                        self.sink.intern(&cid),
                    ))
                    .or_default() -= amount;
                self.post(
                    day,
                    a,
                    "intercompany_out",
                    -amount,
                    0.,
                    0.,
                    vec![entry("intercompany_receivable", &cid, amount)],
                    &cid,
                )?;
                self.post(
                    day,
                    ta,
                    "intercompany_in",
                    amount,
                    0.,
                    0.,
                    vec![entry("intercompany_payable", &cid, -amount)],
                    &cid,
                )?;
            } else {
                let i = self.pid[&p.position];
                let mut pos = self.positions[i].clone();
                let mut free = pos.balance * (1. - pos.encumbered_fraction);
                let price = self.market[i];
                if p.kind == "secured_funding" {
                    let haircut = (pos.haircut + self.scenario.haircut_add * stress).min(0.99);
                    let proceeds = price * (1. - haircut);
                    let eligible =
                        pos.balance * (pos.eligible_fraction - pos.encumbered_fraction).max(0.);
                    let face = eligible.min(amount / proceeds);
                    amount = face * proceeds;
                    pos.encumbered_fraction += if pos.balance != 0. {
                        face / pos.balance
                    } else {
                        0.
                    };
                    self.extra_debt[a] += amount;
                    self.funding_interest[a] += amount * p.funding_rate;
                    let cid = format!("{}:{day}", p.id);
                    let ci = self.claims.len();
                    let due = day + p.funding_tenor_days;
                    self.claims.push(Claim {
                        id: cid.clone(),
                        account: a,
                        position: i,
                        face,
                        balance: amount,
                        rate: p.funding_rate,
                        due,
                    });
                    self.claims_by_position[i].push(ci);
                    self.claim_maturities.entry(due).or_default().push(ci);
                    self.post(
                        day,
                        a,
                        "secured_funding",
                        amount,
                        0.,
                        0.,
                        vec![entry("secured_funding", &cid, -amount)],
                        &cid,
                    )?;
                } else {
                    if pos.classification == "htm" {
                        free = if p.allow_htm_sale {
                            free.min((p.htm_sale_limit - self.htm_sold[i]).max(0.))
                        } else {
                            0.
                        };
                    }
                    let face = free.min(amount / (price * (1. - p.execution_cost)));
                    amount = face * price * (1. - p.execution_cost);
                    let adjustment = if pos.balance != 0. {
                        pos.book_adjustment * face / pos.balance
                    } else {
                        0.
                    };
                    let settled = if pos.balance != 0. {
                        self.accrued[i] * face / pos.balance
                    } else {
                        0.
                    };
                    if settled != 0. {
                        self.accrued[i] -= settled;
                        self.post(
                            day,
                            a,
                            "sale_interest_settlement",
                            settled,
                            0.,
                            0.,
                            vec![entry("accrued_interest", &pos.id, -settled)],
                            &pos.id,
                        )?;
                    }
                    let carry = if marked(&pos) {
                        face * price
                    } else {
                        face + adjustment
                    };
                    let reclass = if pos.balance != 0. {
                        self.marks[i] * face / pos.balance
                    } else {
                        0.
                    };
                    let pledged = pos.balance * pos.encumbered_fraction;
                    pos.balance -= face;
                    pos.book_adjustment -= adjustment;
                    pos.encumbered_fraction = if pos.balance != 0. {
                        pledged / pos.balance
                    } else {
                        0.
                    };
                    self.marks[i] -= reclass;
                    if pos.classification == "htm" {
                        self.htm_sold[i] += face;
                    }
                    let mut changes = vec![
                        entry(principal(&pos), &pos.id, -face),
                        entry("book_adjustment", &pos.id, -adjustment),
                    ];
                    if marked(&pos) {
                        changes.push(entry(
                            "fair_value_adjustment",
                            &pos.id,
                            -face * (price - 1.) + adjustment,
                        ));
                    }
                    self.post(
                        day,
                        a,
                        "security_sale",
                        amount,
                        amount - carry + reclass,
                        -reclass,
                        changes,
                        &pos.id,
                    )?;
                }
                self.positions[i] = pos;
            }
            self.used[pi] += amount;
            self.log_action(
                day,
                &p,
                if amount != 0. { "executed" } else { "blocked" },
                amount,
                if amount != 0. {
                    ""
                } else {
                    "cash, collateral or HTM limit"
                },
            )?;
        }
        Ok(())
    }
    fn run_days(mut self) -> R<()> {
        self.opening()?;
        for day in 1..=self.spec.horizon_days {
            portfolio_compute_control::checkpoint()?;
            let stress = if day >= self.scenario.start_day {
                self.severity
            } else {
                0.
            };
            let credit_mult = (1. + (self.scenario.pd_multiplier - 1.) * stress).max(0.);
            let migration_mult = (1. + (self.scenario.migration_multiplier - 1.) * stress).max(0.);
            for (a, cid, amount) in self.recoveries.remove(&day).unwrap_or_default() {
                self.extra_assets[a] -= amount;
                *self
                    .recovery_claims
                    .get_mut(&(a, cid.clone()))
                    .ok_or("missing recovery")? -= amount;
                self.post(
                    day,
                    a,
                    "credit_recovery",
                    amount,
                    0.,
                    0.,
                    vec![entry("recovery_receivable", &cid, -amount)],
                    &cid,
                )?;
            }
            for ci in self.claim_maturities.remove(&day).unwrap_or_default() {
                let c = self.claims[ci].clone();
                if c.balance == 0. {
                    continue;
                }
                let p = &mut self.positions[c.position];
                p.encumbered_fraction = if p.balance != 0. {
                    (p.encumbered_fraction - c.face / p.balance).max(0.)
                } else {
                    0.
                };
                self.extra_debt[c.account] -= c.balance;
                self.funding_interest[c.account] -= c.balance * c.rate;
                self.claims[ci].balance = 0.;
                self.claims[ci].face = 0.;
                self.post(
                    day,
                    c.account,
                    "secured_repayment",
                    -c.balance,
                    0.,
                    0.,
                    vec![entry("secured_funding", &c.id, c.balance)],
                    &c.id,
                )?;
            }
            let mut processed = HashSet::new();
            for i in 0..self.positions.len() {
                if self.positions[i].start_day <= day {
                    processed.insert(i);
                }
                self.position_day(day, i, stress, credit_mult, migration_mult, &processed)?;
            }
            if let Some(source) = self.streamed_flows {
                for f in source.day(day, &self.spec.positions) {
                    self.cashflow(day, f)?;
                }
            }
            for f in self.flows.remove(&day).unwrap_or_default() {
                self.cashflow(day, f)?;
            }
            for i in 0..self.netting.len() {
                let mut n = self.netting[i].clone();
                let a = self.aid[&n.account];
                if day == self.scenario.start_day {
                    let loss = n.stress_loss * self.scenario.market_shock * self.severity;
                    n.fair_value -= loss;
                    if loss != 0. {
                        self.post(
                            day,
                            a,
                            "derivative_mark",
                            0.,
                            -loss,
                            0.,
                            vec![entry("derivative_value", &n.id, -loss)],
                            &n.id,
                        )?;
                    }
                }
                let exposure = (n.fair_value - n.received_margin).max(0.);
                let pd = (n.annual_pd
                    * credit_mult
                    * (1. + (n.wrong_way_multiplier - 1.) * self.scenario.market_shock * stress)
                        .max(0.))
                .min(0.999999);
                let loss = exposure * n.lgd * (1. - (1. - pd.max(0.)).powf(1. / 365.));
                n.fair_value -= loss;
                if loss != 0. {
                    self.post(
                        day,
                        a,
                        "counterparty_loss",
                        0.,
                        -loss,
                        0.,
                        vec![entry("derivative_value", &n.id, -loss)],
                        &n.id,
                    )?;
                }
                self.margin_targets
                    .entry(day + n.margin_delay)
                    .or_default()
                    .push((i, (-n.fair_value - n.margin_threshold).max(0.)));
                self.netting[i] = n;
            }
            for (i, target) in self.margin_targets.remove(&day).unwrap_or_default() {
                let n = self.netting[i].clone();
                let delta = target - n.posted_margin;
                self.netting[i].posted_margin = target;
                if delta != 0. {
                    self.post(
                        day,
                        self.aid[&n.account],
                        "variation_margin",
                        -delta,
                        0.,
                        0.,
                        vec![entry("posted_margin", &n.id, delta)],
                        &n.id,
                    )?;
                }
            }
            for a in 0..self.accounts.len() {
                let carry = self.funding_interest[a] / 365.;
                if carry != 0. {
                    self.post(
                        day,
                        a,
                        "policy_funding_interest",
                        -carry,
                        -carry,
                        0.,
                        vec![],
                        "",
                    )?;
                }
                let op = (self.accounts[a].annual_fees - self.accounts[a].annual_costs) / 365.;
                if op != 0. {
                    self.post(day, a, "fees_less_costs", op, op, 0., vec![], "")?;
                }
                if self.accounts[a].annual_dividends != 0. {
                    let amount = self.accounts[a].annual_dividends / 365.;
                    self.post(day, a, "dividend", -amount, -amount, 0., vec![], "")?;
                }
            }
            self.policies(day, stress)?;
            for a in 0..self.accounts.len() {
                let income: f64 = self
                    .aggregate
                    .iter()
                    .filter(|r| r.0 == a && r.1 != "dividend")
                    .map(|r| r.2[1])
                    .sum();
                let tax = income.max(0.) * self.accounts[a].tax_rate;
                if tax != 0. {
                    self.post(day, a, "tax", -tax, -tax, 0., vec![], "")?;
                }
            }
            self.observe(day)?;
            if self.detail {
                for (a, event, r) in &self.aggregate {
                    if r.iter().any(|v| *v != 0.) {
                        self.sink.emit("ledger",json!({"scenario":self.scenario.name,"day":day,"account":self.accounts[*a].id,"event":event,"cash":r[0],"earnings":r[1],"aoci":r[2],"memo_amount":0.}))?;
                    }
                }
            }
            self.aggregate.clear();
            self.aggregate_index.clear();
        }
        for (ai, a) in self.accounts.iter().enumerate() {
            let mut row = json!({"scenario":self.scenario.name,"account":a.id,"entity":a.entity,"currency":a.currency,"final_cash":a.cash,"final_equity":a.equity,"minimum_cash":self.minimum_cash[ai],"peak_cash_shortfall":(a.cash_floor-self.minimum_cash[ai]).max(0.),"max_reconciliation_error":self.max_error});
            for (field, metric) in [
                ("first_cash_breach_day", "cash"),
                ("first_capital_breach_day", "cet1"),
                ("first_leverage_breach_day", "leverage"),
                ("first_lcr_breach_day", "lcr"),
                ("first_nsfr_breach_day", "nsfr"),
                ("first_htm_breach_day", "htm"),
            ] {
                row[field] = json!(self.first.get(&(ai, metric.into())));
            }
            if !self.detail {
                row["severity"] = json!(self.severity);
                row["breached"] = json!(self.first.keys().any(|k| k.0 == ai));
            }
            self.sink.emit(
                if self.detail {
                    "summary"
                } else {
                    "reverse_grid"
                },
                row,
            )?;
        }
        if self.detail {
            let mut keys: Vec<_> = self.gl.keys().collect();
            keys.sort_by(|x, y| {
                (
                    &self.accounts[x.0].id,
                    &self.sink.dictionary[x.1],
                    &self.sink.dictionary[x.2],
                )
                    .cmp(&(
                        &self.accounts[y.0].id,
                        &self.sink.dictionary[y.1],
                        &self.sink.dictionary[y.2],
                    ))
            });
            for k in keys {
                self.sink.emit("trial_balance",json!({"scenario":self.scenario.name,"account":self.accounts[k.0].id,"gl_account":self.sink.dictionary[k.1],"instrument_id":self.sink.dictionary[k.2],"balance":self.gl[k]}))?;
            }
            for c in &self.claims {
                self.sink.emit("funding_claims",json!({"scenario":self.scenario.name,"claim_id":c.id,"account":self.accounts[c.account].id,"position":self.positions[c.position].id,"face":c.face,"balance":c.balance,"rate":c.rate,"due":c.due}))?;
            }
            let trial: Vec<_> = self
                .gl
                .iter()
                .map(|(k, &v)| (self.sink.ids[&self.accounts[k.0].id], k.1, k.2, v))
                .collect();
            for (table, row) in self.sink.reports.close(
                &self.scenario.name,
                &self.accounts,
                &trial,
                &self.sink.dictionary,
            )? {
                self.sink.emit(table, row)?;
            }
        }
        Ok(())
    }
}

pub fn run(spec: Spec, output: Box<dyn Write + Send>) -> R<()> {
    run_with_flows(spec, None, output)
}

pub fn run_with_flows(
    spec: Spec,
    flows: Option<crate::flow_store::FlowStore>,
    output: Box<dyn Write + Send>,
) -> R<()> {
    crate::spec::validate(
        &spec,
        if flows.is_some() { 62000 } else { 60000 },
        if flows.is_some() {
            120_000_000
        } else {
            90_000_000
        },
    )?;
    if let Some(source) = &flows {
        source.validate(&spec)?;
    }
    if spec.ruleset != "research-weights-v2"
        || spec.positions.len() > if flows.is_some() { 62000 } else { 60000 }
        || spec.accounts.is_empty()
        || spec.accounts.len() > 50
        || spec.horizon_days > 1080
    {
        return Err("invalid native input limits".into());
    }
    let baseline = Scenario {
        name: "baseline".into(),
        start_day: 1,
        rate_shift: 0.,
        spread_shift: 0.,
        deposit_flight: 0.,
        pd_multiplier: 1.,
        migration_multiplier: 1.,
        lgd_add: 0.,
        draw_multiplier: 0.,
        rollover_loss: 0.,
        haircut_add: 0.,
        market_shock: 0.,
        outage_days: 0,
    };
    let mut sink = Sink::new(output);
    for scenario in std::iter::once(&baseline).chain(spec.scenarios.iter()) {
        Sim::new(&spec, scenario.clone(), 1., true, &mut sink, flows.as_ref())?.run_days()?;
    }
    for scenario in &spec.scenarios {
        for &severity in &spec.reverse_severities {
            Sim::new(
                &spec,
                scenario.clone(),
                severity,
                false,
                &mut sink,
                flows.as_ref(),
            )?
            .run_days()?;
        }
    }
    for row in sink.reports.attribution(&spec.scenarios) {
        sink.emit("attribution", row)?;
    }
    sink.flush()?;
    serde_json::to_writer(&mut sink.out, &json!({"complete":true,"protocol":2}))
        .map_err(|e| e.to_string())?;
    sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
    sink.out.flush().map_err(|e| e.to_string())
}
