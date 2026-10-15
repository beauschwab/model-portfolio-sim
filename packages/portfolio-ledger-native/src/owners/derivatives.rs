//! Borrowed native derivative intents for the existing daily Sim.
//! No Deserialize implementation: raw-only native coordinators generate sources.
use crate::replay::dated::CompiledDatedLedger;
use portfolio_compute_control::checkpoint;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_WORK: usize = 8_000_000;
pub const MAX_CAPACITY: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Cash,
    OpeningEquity,
    Asset,
    Liability,
    Receivable,
    Payable,
    PostedVm,
    ReceivedVm,
    PostedIm,
    Mark,
    Coupon,
    CollateralInterest,
    CounterpartyLoss,
    CollateralLoss,
    CustodyAsset,
    CustodyObligation,
}
impl Kind {
    pub fn gl(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::OpeningEquity => "opening_equity",
            Self::Asset => "derivative_asset",
            Self::Liability => "derivative_liability",
            Self::Receivable => "derivative_unpaid_receivable",
            Self::Payable => "derivative_unpaid_payable",
            Self::PostedVm => "derivative_posted_vm_asset",
            Self::ReceivedVm => "derivative_received_vm_liability",
            Self::PostedIm => "derivative_posted_im_asset",
            Self::Mark => "pnl:derivative_mark",
            Self::Coupon => "pnl:derivative_contractual_income",
            Self::CollateralInterest => "pnl:derivative_collateral_interest",
            Self::CounterpartyLoss => "pnl:derivative_counterparty_loss",
            Self::CollateralLoss => "pnl:derivative_collateral_return_loss",
            Self::CustodyAsset => "memo:derivative_custodian_asset",
            Self::CustodyObligation => "memo:derivative_custodian_obligation",
        }
    }
    pub fn memo(self) -> bool {
        matches!(self, Self::CustodyAsset | Self::CustodyObligation)
    }
    pub fn pnl(self) -> bool {
        self.gl().starts_with("pnl:")
    }
    pub fn stock(self) -> bool {
        !self.memo() && !self.pnl() && !matches!(self, Self::Cash | Self::OpeningEquity)
    }
    pub fn liability(self) -> bool {
        matches!(self, Self::Liability | Self::Payable | Self::ReceivedVm)
    }
}
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
    pub book_key: String,
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
        Err("derivative owner monetary domain".into())
    }
}
pub fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("derivative owner admission overflow".into())
}
fn reserve(
    lines: &[Line],
    retained: usize,
    bytes: &mut usize,
    work: &mut usize,
) -> Result<(), String> {
    *work = add(*work, lines.len())?;
    *bytes = add(
        *bytes,
        retained
            .checked_mul(std::mem::size_of::<Line>())
            .ok_or("derivative owner capacity overflow")?,
    )?;
    for line in lines {
        checkpoint()?;
        bounded(line.debit)?;
        // Charge each source string and a conservative per-line shadow/GL key allowance.
        // Repeated keys are deliberately charged repeatedly; this is no RSS guarantee.
        *bytes = add(
            *bytes,
            add(line.instrument.capacity(), add(line.instrument.len(), 256)?)?,
        )?;
    }
    Ok(())
}
impl<'a> Compiled<'a> {
    pub fn new(c: &CompiledDatedLedger, sources: Vec<Source<'a>>) -> Result<Self, String> {
        checkpoint()?;
        if sources.is_empty()
            || sources.len() > 64
            || !c.spec.reverse_severities.is_empty()
            || !c.spec.netting_sets.is_empty()
        {
            return Err("explicit derivative owner requires complete source runs and no legacy/reverse netting overlays".into());
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
            if source.book_key.is_empty()
                || source.book_key.len() > 16384
                || source.asof != asof
                || source.tail < asof
                || source.tail > tail
                || source.monetary_unit != "currency_units"
                || !runs.contains(&source.scenario)
                || source.daily.len() != (i64::from(source.tail) - i64::from(asof) + 1) as usize
            {
                return Err("derivative owner source/date/run/units coverage differs".into());
            }
            if !coverage
                .entry((&source.book_key, &source.account_id))
                .or_default()
                .insert(&source.scenario)
            {
                return Err("duplicate derivative book/run owner".into());
            }
            let account = c
                .spec
                .accounts
                .iter()
                .position(|a| a.id == source.account_id)
                .ok_or("derivative owner account missing")?;
            let a = &c.spec.accounts[account];
            if a.entity != source.entity
                || a.currency != source.currency
                || a.cash.to_bits() != source.opening_cash.to_bits()
            {
                return Err("derivative owner account/opening cash differs".into());
            }
            accounts.push(account);
            let stocks = opening_stocks
                .entry((account, &source.scenario))
                .or_default();
            let mut opening_total = 0.;
            for line in &source.opening {
                opening_total += line.debit;
                if line.kind.pnl() {
                    return Err("opening derivative P&L is not an anchor".into());
                }
                if line.kind.stock() {
                    if (line.kind.liability() && line.debit > 0.)
                        || (!line.kind.liability() && line.debit < 0.)
                    {
                        return Err("opening derivative gross direction differs".into());
                    }
                    if line.kind.liability() {
                        stocks.1 -= line.debit;
                    } else {
                        stocks.0 += line.debit;
                    }
                }
            }
            if !near(opening_total, 0.) {
                return Err("unbalanced derivative opening source".into());
            }
            work = add(work, 250_000)?; // Raw native source budget charged for every selected run.
            capacity = add(
                capacity,
                source.book_key.capacity()
                    + source.scenario.capacity()
                    + source.account_id.capacity(),
            )?;
            reserve(
                &source.opening,
                source.opening.capacity(),
                &mut capacity,
                &mut work,
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
                    || intent
                        .lines
                        .iter()
                        .any(|l| l.kind.memo() != intent.memo || l.kind == Kind::OpeningEquity)
                {
                    return Err("derivative source intent order/ownership invalid".into());
                }
                prior = intent.ordinal;
                reserve(
                    &intent.lines,
                    intent.lines.capacity(),
                    &mut capacity,
                    &mut work,
                )?;
                let total: f64 = intent.lines.iter().map(|l| l.debit).sum();
                if !near(total, 0.) {
                    return Err("unbalanced derivative source intent".into());
                }
            }
            for (offset, day) in source.daily.iter().enumerate() {
                if day.ordinal != asof + offset as i32 {
                    return Err("missing derivative closing day".into());
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
                            return Err("derivative closing ledger kind/uniqueness differs".into());
                        }
                    }
                }
                if !kinds.contains(&Kind::Cash) || !kinds.contains(&Kind::OpeningEquity) {
                    return Err("derivative closing cash/equity coverage differs".into());
                }
            }
            // Daily reconciliation, expected-GL and gross-balance scans visit retained
            // keys through the complete mixed tail, even after this source has closed.
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
                    .ok_or("derivative daily replay work overflow")?,
            )?;
            if work > MAX_WORK || capacity > MAX_CAPACITY {
                return Err("derivative joined work/capacity admission exceeded".into());
            }
        }
        if coverage
            .values()
            .any(|set| set.len() != runs.len() || set.iter().any(|name| !runs.contains(*name)))
        {
            return Err("incomplete derivative book/scenario coverage".into());
        }
        for ((account, _), &(assets, liabilities)) in &opening_stocks {
            let expected = c
                .native_opening
                .iter()
                .filter(|s| s.account_id == c.spec.accounts[*account].id)
                .fold((0., 0.), |sum, s| (sum.0 + s.assets, sum.1 + s.liabilities));
            if !near(assets, expected.0) || !near(liabilities, expected.1) {
                return Err(
                    "native derivative opening stocks differ from admitted balance sheet".into(),
                );
            }
        }
        capacity = add(capacity, accounts.capacity() * std::mem::size_of::<usize>())?;
        if capacity > MAX_CAPACITY {
            return Err("derivative joined capacity admission exceeded".into());
        }
        // Namespaced derivative instruments cannot alias a contractual dated owner.
        if c.spec
            .positions
            .iter()
            .any(|p| p.id.starts_with("@derivative:"))
        {
            return Err("dated position aliases derivative source namespace".into());
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
                bounded(line.debit)?;
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
            return Err("missing derivative native source run".into());
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
            .ok_or("derivative closing day outside source")?;
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
                    "derivative source/state mismatch day{day}: {kind:?}"
                ));
            }
        }
        if day + 1 == source.daily.len()
            && (self.cursor != source.intents.len()
                || self
                    .book
                    .iter()
                    .any(|((kind, _), value)| (kind.stock() || kind.memo()) && !near(*value, 0.)))
        {
            return Err("unresolved gross derivative/rights/collateral tail".into());
        }
        Ok(())
    }
}
