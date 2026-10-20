//! Borrowed native lifecycle acquisition intents for the existing daily Sim (issue #44).
//! The journal kinds are shared with the model-core acquisition engine, so the owner cannot
//! drift from the engine that produced the intents. Raw-only native coordinators build sources.
use crate::replay::dated::CompiledDatedLedger;
use portfolio_compute_control::checkpoint;
pub use portfolio_model_core::lifecycle::journal::Kind;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_WORK: usize = 8_000_000;
pub const MAX_CAPACITY: usize = 128 * 1024 * 1024;
/// Lifecycle instruments live in their own namespace; a dated position cannot alias one.
pub const NAMESPACE: &str = "@lifecycle:";

pub struct Line {
    pub kind: Kind,
    pub instrument: String,
    pub debit: f64,
}
pub struct Intent<'a> {
    pub event_id: &'a str,
    pub ordinal: i32,
    pub memo: bool,
    pub lines: Vec<Line>,
}
pub struct Day {
    pub ordinal: i32,
    pub financial: Vec<(Kind, f64)>,
    pub custody: Vec<(Kind, f64)>,
}
pub struct Source<'a> {
    pub plan_key: String,
    pub scenario: String,
    pub account_id: String,
    pub entity: &'a str,
    pub currency: &'a str,
    pub monetary_unit: &'a str,
    pub asof: i32,
    pub tail: i32,
    pub opening_cash: f64,
    pub opening: Vec<Line>,
    pub intents: Vec<Intent<'a>>,
    pub daily: Vec<Day>,
}
pub struct Compiled<'a> {
    pub sources: Vec<Source<'a>>,
    pub accounts: Vec<usize>,
    pub work: usize,
    pub capacity: usize,
}

fn bounded(value: f64) -> Result<(), String> {
    if value.is_finite() && value.abs() <= 1e15 {
        Ok(())
    } else {
        Err("lifecycle owner monetary domain".into())
    }
}
pub fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "lifecycle owner work/capacity overflow".into())
}

impl<'a> Compiled<'a> {
    pub fn new(c: &CompiledDatedLedger, sources: Vec<Source<'a>>) -> Result<Self, String> {
        checkpoint()?;
        if sources.is_empty()
            || sources.len() > 64
            || !c.spec.reverse_severities.is_empty()
            || !c.spec.netting_sets.is_empty()
        {
            return Err("lifecycle owner requires complete source runs and no legacy/reverse netting overlays".into());
        }
        let runs: BTreeSet<_> = c
            .store
            .runs()
            .iter()
            .map(|r| r.scenario_id.to_string())
            .collect();
        let mut coverage: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
        let mut opening_stocks: BTreeMap<(usize, &str), (f64, f64)> = BTreeMap::new();
        let mut accounts = Vec::new();
        let (mut work, mut capacity) = (
            c.work_units,
            sources.capacity() * std::mem::size_of::<Source>(),
        );
        let asof = c.store.as_of_ordinal();
        let tail = c.store.reporting_end_ordinal();
        for source in &sources {
            checkpoint()?;
            if source.plan_key.is_empty()
                || source.plan_key.len() > 16384
                || source.asof != asof
                || source.tail < asof
                || source.tail > tail
                || source.monetary_unit != "currency_units"
                || !runs.contains(&source.scenario)
                || source.daily.len() != (i64::from(source.tail) - i64::from(asof) + 1) as usize
            {
                return Err("lifecycle owner source/date/run/units coverage differs".into());
            }
            if !coverage
                .entry((&source.plan_key, &source.account_id))
                .or_default()
                .insert(&source.scenario)
            {
                return Err("duplicate lifecycle plan/run owner".into());
            }
            let account = c
                .spec
                .accounts
                .iter()
                .position(|a| a.id == source.account_id)
                .ok_or("lifecycle owner account missing")?;
            let a = &c.spec.accounts[account];
            if a.entity != source.entity
                || a.currency != source.currency
                || a.cash.to_bits() != source.opening_cash.to_bits()
            {
                return Err(
                    "lifecycle owner account/opening cash differs: the opening is posted once"
                        .into(),
                );
            }
            accounts.push(account);
            let stocks = opening_stocks
                .entry((account, &source.scenario))
                .or_default();
            let mut opening_total = 0.;
            for line in &source.opening {
                bounded(line.debit)?;
                opening_total += line.debit;
                if line.kind.pnl() {
                    return Err("opening lifecycle P&L is not an anchor".into());
                }
                if line.kind.stock() {
                    if !line.kind.signed()
                        && ((line.kind.liability() && line.debit > 0.)
                            || (!line.kind.liability() && line.debit < 0.))
                    {
                        return Err("opening lifecycle gross direction differs".into());
                    }
                    if line.kind.liability() {
                        stocks.1 -= line.debit;
                    } else {
                        stocks.0 += line.debit;
                    }
                }
            }
            if !near(opening_total, 0.) {
                return Err("unbalanced lifecycle opening source".into());
            }
            work = add(work, 250_000)?;
            capacity = add(
                capacity,
                source.plan_key.capacity()
                    + source.scenario.capacity()
                    + source.account_id.capacity(),
            )?;
            capacity = add(
                capacity,
                source.opening.capacity() * std::mem::size_of::<Line>(),
            )?;
            capacity = add(
                capacity,
                source.intents.capacity() * std::mem::size_of::<Intent>(),
            )?;
            capacity = add(
                capacity,
                source.daily.capacity() * std::mem::size_of::<Day>(),
            )?;
            let mut ids = BTreeSet::new();
            let mut prior = asof;
            for intent in &source.intents {
                checkpoint()?;
                if intent.ordinal < prior
                    || intent.ordinal > source.tail
                    || intent.event_id.is_empty()
                    || !ids.insert(intent.event_id)
                    || intent.lines.is_empty()
                    || intent.lines.iter().any(|l| {
                        l.kind.memo() != intent.memo
                            || l.kind == Kind::OpeningEquity
                            || (l.kind != Kind::Cash && !l.instrument.starts_with(NAMESPACE))
                    })
                {
                    return Err("lifecycle source intent order/ownership/namespace invalid".into());
                }
                prior = intent.ordinal;
                capacity = add(
                    capacity,
                    intent.lines.capacity() * std::mem::size_of::<Line>(),
                )?;
                work = add(work, intent.lines.len())?;
                let mut total = 0.;
                for l in &intent.lines {
                    bounded(l.debit)?;
                    total += l.debit;
                }
                if !near(total, 0.) {
                    return Err("unbalanced lifecycle source intent".into());
                }
            }
            for (offset, day) in source.daily.iter().enumerate() {
                if day.ordinal != asof + offset as i32 {
                    return Err("missing lifecycle closing day".into());
                }
                capacity = add(
                    capacity,
                    (day.financial.capacity() + day.custody.capacity())
                        * std::mem::size_of::<(Kind, f64)>(),
                )?;
                work = add(work, day.financial.len() + day.custody.len())?;
                let mut kinds = BTreeSet::new();
                for (rows, memo) in [(&day.financial, false), (&day.custody, true)] {
                    for &(kind, value) in rows {
                        bounded(value)?;
                        if kind.memo() != memo || !kinds.insert(kind) {
                            return Err("lifecycle closing ledger kind/uniqueness differs".into());
                        }
                    }
                }
                if !kinds.contains(&Kind::Cash) || !kinds.contains(&Kind::OpeningEquity) {
                    return Err("lifecycle closing cash/equity coverage differs".into());
                }
            }
            let line_bound = source
                .intents
                .iter()
                .try_fold(source.opening.len(), |n, i| add(n, i.lines.len()))?;
            let days = (i64::from(tail) - i64::from(asof) + 1) as usize;
            work = add(
                work,
                line_bound
                    .checked_mul(days)
                    .and_then(|n| n.checked_mul(6))
                    .ok_or("lifecycle daily replay work overflow")?,
            )?;
            if work > MAX_WORK || capacity > MAX_CAPACITY {
                return Err("lifecycle joined work/capacity admission exceeded".into());
            }
        }
        if coverage
            .values()
            .any(|set| set.len() != runs.len() || set.iter().any(|name| !runs.contains(*name)))
        {
            return Err("incomplete lifecycle plan/scenario coverage".into());
        }
        for ((account, _), &(assets, liabilities)) in &opening_stocks {
            let expected = c
                .native_opening
                .iter()
                .filter(|s| s.account_id == c.spec.accounts[*account].id)
                .fold((0., 0.), |sum, s| (sum.0 + s.assets, sum.1 + s.liabilities));
            if !near(assets, expected.0) || !near(liabilities, expected.1) {
                return Err(
                    "native lifecycle opening stocks differ from admitted balance sheet".into(),
                );
            }
        }
        capacity = add(capacity, accounts.capacity() * std::mem::size_of::<usize>())?;
        if capacity > MAX_CAPACITY {
            return Err("lifecycle joined capacity admission exceeded".into());
        }
        if c.spec.positions.iter().any(|p| p.id.starts_with(NAMESPACE)) {
            return Err("dated position aliases the lifecycle source namespace".into());
        }
        Ok(Self {
            sources,
            accounts,
            work,
            capacity,
        })
    }
}

pub struct State {
    pub source: usize,
    pub cursor: usize,
    pub book: BTreeMap<(Kind, String), f64>,
}
pub struct Runtime {
    pub states: Vec<State>,
}
impl Runtime {
    pub fn new(compiled: &Compiled<'_>, scenario: &str) -> Result<Self, String> {
        let mut states = Vec::new();
        for (source, s) in compiled
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| s.scenario == scenario)
        {
            let mut book = BTreeMap::new();
            for line in &s.opening {
                *book
                    .entry((line.kind, line.instrument.clone()))
                    .or_default() += line.debit;
            }
            states.push(State {
                source,
                cursor: 0,
                book,
            });
        }
        if states.is_empty() {
            return Err("missing lifecycle native source run".into());
        }
        Ok(Self { states })
    }
}
impl State {
    pub fn reconcile(&self, source: &Source<'_>, day: usize) -> Result<(), String> {
        checkpoint()?;
        let expected = source
            .daily
            .get(day)
            .ok_or("lifecycle closing day outside source")?;
        let mut actual = BTreeMap::new();
        for ((kind, _), value) in &self.book {
            *actual.entry(*kind).or_insert(0.) += value;
        }
        let reported: BTreeMap<_, _> = expected
            .financial
            .iter()
            .chain(&expected.custody)
            .copied()
            .collect();
        for kind in actual.keys().chain(reported.keys()) {
            if !near(
                *actual.get(kind).unwrap_or(&0.),
                *reported.get(kind).unwrap_or(&0.),
            ) {
                return Err(format!(
                    "lifecycle source/state mismatch day{day}: {kind:?}"
                ));
            }
        }
        if day + 1 == source.daily.len() && self.cursor != source.intents.len() {
            return Err("lifecycle intents beyond the source tail".into());
        }
        Ok(())
    }
}
