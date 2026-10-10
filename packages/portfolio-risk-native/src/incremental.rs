//! Native instrument dependency graph: immutable calibration inputs, temporary
//! valuation edits, bounded cashflow batches, fixed-OAS risk and frozen-yield NII.
use crate::accounting_lifecycle::{
    cd_flows, AccountingRequest, DepositBook, MortgageBook, TermBook,
};
use crate::deposit_lifecycle::{cashflows, deposit_paths, equilibrium, DepositDeck};
use crate::graph_cache::{Cache, Key, Stats};
use crate::hedge_lifecycle::{smear, term_flows};
use crate::lifecycle_market::{finite, matrix, vector, RateMarket};
use crate::quant::{income, oas};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

type Handles = HashMap<u64, Arc<Mutex<Cache>>>;
static HANDLES: OnceLock<Mutex<Handles>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Create {
        max_bytes: usize,
        max_entries: usize,
    },
    Clear {
        handle: u64,
    },
    Info {
        handle: u64,
    },
    Drop {
        handle: u64,
    },
    Compare {
        handle: u64,
        request: Box<PriceRequest>,
        patches: BTreeMap<String, Value>,
        calibration_mode: String,
    },
    Price {
        handle: u64,
        request: Box<PriceRequest>,
    },
}
impl Request {
    pub fn run(self) -> Result<Value, String> {
        let registry = HANDLES.get_or_init(|| Mutex::new(HashMap::new()));
        match self {
            Self::Create {
                max_bytes,
                max_entries,
            } => {
                let cache = Cache::new(max_bytes, max_entries)?;
                let mut handles = registry.lock().map_err(|_| "graph registry poisoned")?;
                if handles.len() >= 128 {
                    return Err("too many active graph caches".into());
                }
                let handle = NEXT.fetch_add(1, Ordering::Relaxed);
                if handle == 0 {
                    return Err("graph handle exhausted".into());
                }
                handles.insert(handle, Arc::new(Mutex::new(cache)));
                Ok(json!({"handle":handle}))
            }
            Self::Drop { handle } => {
                registry
                    .lock()
                    .map_err(|_| "graph registry poisoned")?
                    .remove(&handle);
                Ok(json!({}))
            }
            other => {
                let handle = match &other {
                    Self::Clear { handle }
                    | Self::Info { handle }
                    | Self::Compare { handle, .. }
                    | Self::Price { handle, .. } => *handle,
                    _ => unreachable!(),
                };
                let cache = registry
                    .lock()
                    .map_err(|_| "graph registry poisoned")?
                    .get(&handle)
                    .cloned()
                    .ok_or("unknown graph cache handle")?;
                let mut cache = cache.lock().map_err(|_| "graph cache poisoned")?;
                match other {
                    Self::Clear { .. } => {
                        cache.clear();
                        Ok(cache.info())
                    }
                    Self::Info { .. } => Ok(cache.info()),
                    Self::Price { request, .. } => request.run(&mut cache),
                    Self::Compare {
                        request,
                        patches,
                        calibration_mode,
                        ..
                    } => compare(*request, patches, &calibration_mode, &mut cache),
                    _ => unreachable!(),
                }
            }
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceRequest {
    pub base: AccountingRequest,
    pub current: AccountingRequest,
    pub spread_shift: f64,
    pub overrides: BTreeMap<String, BTreeMap<String, f64>>,
    pub include_analytics: bool,
    pub include_key_rates: bool,
    pub kpis: Option<crate::kpi_lifecycle::KpiRequest>,
    pub auxiliary: Option<crate::balance_risk::BalanceRiskRequest>,
}
#[derive(Clone)]
struct Flow {
    a: Vec<f64>,
    time: Vec<f64>,
    interest: Vec<f64>,
    principal: Vec<f64>,
}
impl Flow {
    fn bytes(&self) -> usize {
        (self.a.capacity()
            + self.time.capacity()
            + self.interest.capacity()
            + self.principal.capacity())
            * 8
            + std::mem::size_of::<Self>()
    }
}
#[derive(Clone, Copy)]
enum Book<'a> {
    Mortgage(&'a MortgageBook),
    Term(&'a TermBook),
    Deposit(&'a DepositBook),
}
impl<'a> Book<'a> {
    fn ids(&self) -> &[String] {
        match self {
            Self::Mortgage(b) => &b.ids,
            Self::Term(b) => &b.ids,
            Self::Deposit(b) => &b.ids,
        }
    }
    fn name(&self) -> &str {
        match self {
            Self::Mortgage(_) => "mbs",
            Self::Term(b) => &b.key,
            Self::Deposit(_) => "deposits",
        }
    }
    fn target(&self, i: usize) -> f64 {
        (match self {
            Self::Mortgage(b) => b.request.book[i * 13 + 10],
            Self::Term(b) => b.deck.contracts[i].price,
            Self::Deposit(b) => b.book[i].price,
        }) / 100.
    }
    fn notional(&self, i: usize) -> f64 {
        match self {
            Self::Mortgage(b) => b.request.book[i * 13 + 11],
            Self::Term(b) => b.deck.contracts[i].notional,
            Self::Deposit(b) => b.book[i].balance,
        }
    }
    fn yields(&self, i: usize) -> Option<f64> {
        match self {
            Self::Mortgage(b) => b.book_yields.as_ref().map(|v| v[i]),
            Self::Term(b) => b.book_yields.as_ref().map(|v| v[i]),
            Self::Deposit(_) => None,
        }
    }
    fn context(&self, a: &AccountingRequest, rates: &[f64], p: usize) -> Result<Arc<Key>, String> {
        let mut market = a.market.clone();
        market.config.paths = p;
        let data = match self {
            Self::Mortgage(b) => {
                let r = &b.request;
                let mut config = r.config.clone();
                config.base_paths = p;
                config.sensitivity_paths = p;
                json!((&r.cc_history, &r.ps_history, config, &r.prepay))
            }
            Self::Term(b) => {
                json!({"calendar":b.deck.calendar,"bdc":b.deck.bdc,"product":b.deck.product,
                "withdrawal":if b.key=="cds" {a.withdrawal_parameters.clone()} else {vec![]}})
            }
            Self::Deposit(b) => {
                let mut assumptions = b.assumptions.clone();
                assumptions.segments.clear();
                json!({"history":b.history,"assumptions":assumptions})
            }
        };
        Key::new("flow-context", (&market, rates, a.asof, data), vec![])
    }
    fn keys(&self, indices: &[usize], context: &Arc<Key>) -> Result<Vec<Arc<Key>>, String> {
        indices
            .iter()
            .map(|&i| {
                let row = match self {
                    Self::Mortgage(b) => {
                        let mut row = b.request.book[i * 13..(i + 1) * 13].to_vec();
                        row[10] = 0.;
                        row[11] = 0.;
                        let hpi = if b.request.original_hpi.is_empty() {
                            (1. + b.request.config.hpi[0]).powf(row[3] / 12.)
                        } else {
                            b.request.original_hpi[i]
                        };
                        // the speed multiplier changes cash flows, so it is part of the identity
                        let speed = b.request.prepay_multiplier.get(i).copied().unwrap_or(1.);
                        json!((row, hpi, speed))
                    }
                    Self::Term(b) => {
                        let mut c = b.deck.contracts[i].clone();
                        c.notional = 0.;
                        c.price = 100.;
                        json!(c)
                    }
                    Self::Deposit(b) => {
                        let mut c = b.book[i].clone();
                        c.balance = 0.;
                        c.price = 100.;
                        json!((
                            &c,
                            b.assumptions
                                .segments
                                .get(&c.segment)
                                .or_else(|| b.assumptions.segments.get("SAV"))
                        ))
                    }
                };
                Key::new(self.name(), (&self.ids()[i], row), vec![context.clone()])
            })
            .collect()
    }
    fn flows(
        &self,
        a: &AccountingRequest,
        indices: &[usize],
        rates: &[f64],
        p: usize,
    ) -> Result<Vec<Flow>, String> {
        let t = a.market.config.months;
        match self {
            Self::Mortgage(b) => {
                let selected: Vec<_> = indices
                    .iter()
                    .flat_map(|&i| b.request.book[i * 13..(i + 1) * 13].iter().copied())
                    .collect();
                let original: Vec<_> = if b.request.original_hpi.is_empty() {
                    vec![]
                } else {
                    indices.iter().map(|&i| b.request.original_hpi[i]).collect()
                };
                let speeds: Vec<_> = if b.request.prepay_multiplier.is_empty() {
                    vec![]
                } else {
                    indices
                        .iter()
                        .map(|&i| b.request.prepay_multiplier[i])
                        .collect()
                };
                let mut raw = b.request.borrow();
                raw.book = &selected;
                raw.original_hpi = &original;
                raw.prepay_multiplier = &speeds;
                raw.fixed_oas = &[];
                raw.config.base_paths = p;
                raw.config.sensitivity_paths = p;
                let values = raw.graph_flows(rates, p)?;
                Ok(indices
                    .iter()
                    .enumerate()
                    .map(|(j, &i)| Flow {
                        a: values[0][j * t..(j + 1) * t].to_vec(),
                        time: (1..=t)
                            .map(|m| m as f64 / 12. + b.request.book[i * 13 + 12] / 365.)
                            .collect(),
                        interest: values[1][j * t..(j + 1) * t]
                            .iter()
                            .map(|v| v / p as f64)
                            .collect(),
                        principal: values[2][j * t..(j + 1) * t]
                            .iter()
                            .map(|v| v / p as f64)
                            .collect(),
                    })
                    .collect())
            }
            Self::Term(b) => {
                let mut input = a.market.clone();
                input.config.paths = p;
                let market = RateMarket::new(&input)?;
                let paths = market.paths(rates, &market.abcd)?;
                let rows: Vec<_> = indices
                    .iter()
                    .map(|&i| b.deck.contracts[i].clone())
                    .collect();
                let deck = b.deck.build_rows(&rows)?;
                let values = if b.key == "cds" {
                    cd_flows(&deck, &paths, p, t, &a.withdrawal_parameters)
                } else {
                    term_flows(&deck, &paths, p, t)
                };
                let interest: Vec<_> = values[1].iter().map(|v| v / p as f64).collect();
                let principal: Vec<_> = values[2].iter().map(|v| v / p as f64).collect();
                let interest = smear(&deck, &interest, t, true);
                let principal = smear(&deck, &principal, t, false);
                Ok((0..indices.len())
                    .map(|j| {
                        let start = deck.per_off[j] as usize;
                        let end = deck.per_off[j + 1] as usize;
                        Flow {
                            a: values[0][start..end].to_vec(),
                            time: deck.t_pay[start..end].to_vec(),
                            interest: interest[j * t..(j + 1) * t].to_vec(),
                            principal: principal[j * t..(j + 1) * t].to_vec(),
                        }
                    })
                    .collect())
            }
            Self::Deposit(b) => {
                let mut input = a.market.clone();
                input.config.paths = p;
                let market = RateMarket::new(&input)?;
                let anchor = market.base()?;
                let paths = market.paths(rates, &market.abcd)?;
                let history: Vec<_> = b.history.iter().flatten().copied().collect();
                let fit =
                    crate::cache::resolve(crate::cache::Key::new(41).floats(&history), || {
                        let fit = crate::calibration::fit_deposits(&history)?;
                        let bytes = fit.x.capacity() * 8 + std::mem::size_of_val(&fit);
                        Ok((fit, bytes))
                    })?;
                let r0 = equilibrium(
                    &fit.x,
                    (0..p).map(|i| anchor.short[i * t]).sum::<f64>() / p as f64,
                );
                let paid = deposit_paths(&paths.short, &paths.df, &fit.x, r0, p, t);
                let rows: Vec<_> = indices.iter().map(|&i| b.book[i].clone()).collect();
                let mut deck = DepositDeck::new(&rows, &b.assumptions)?;
                for (j, &i) in indices.iter().enumerate() {
                    let r = &b.book[i];
                    if let Some(v) = r.attrition_base {
                        deck.base[j] = v;
                    }
                    if let Some(v) = r.attrition_amp {
                        deck.fl_amp[j] = v;
                    }
                    if let Some(v) = r.attrition_slope {
                        deck.fl_b[j] = v;
                    }
                    if let Some(v) = r.attrition_gap {
                        deck.fl_g0[j] = v;
                    }
                }
                let values = cashflows(
                    &deck,
                    &b.assumptions,
                    &paid,
                    r0,
                    p,
                    t,
                    &vec![0.; indices.len()],
                    &[0.],
                    false,
                    None,
                );
                Ok((0..indices.len())
                    .map(|j| Flow {
                        a: values[0][j * t..(j + 1) * t].to_vec(),
                        time: (1..=t).map(|m| m as f64 / 12.).collect(),
                        interest: values[5][j * t..(j + 1) * t]
                            .iter()
                            .map(|v| v / p as f64)
                            .collect(),
                        principal: values[1][j * t..(j + 1) * t]
                            .iter()
                            .map(|v| v / p as f64)
                            .collect(),
                    })
                    .collect())
            }
        }
    }
}
fn books(a: &AccountingRequest) -> BTreeMap<String, Book<'_>> {
    let mut out = BTreeMap::new();
    if let Some(b) = &a.mbs {
        out.insert("mbs".into(), Book::Mortgage(b));
    }
    if let Some(b) = &a.deposits {
        out.insert("deposits".into(), Book::Deposit(b));
    }
    for b in &a.terms {
        out.insert(b.key.clone(), Book::Term(b));
    }
    out
}
type Flows = (Vec<Arc<Key>>, Vec<Arc<Flow>>);
#[allow(clippy::too_many_arguments)]
fn flows(
    cache: &mut Cache,
    stats: &mut Stats,
    b: Book<'_>,
    a: &AccountingRequest,
    indices: &[usize],
    rates: &[f64],
    p: usize,
) -> Result<Flows, String> {
    let context = b.context(a, rates, p)?;
    let keys = b.keys(indices, &context)?;
    let values = cache.batch(
        &format!("cashflows:{}", b.name()),
        &keys,
        stats,
        |missing| {
            let indices: Vec<_> = missing.iter().map(|&j| indices[j]).collect();
            let rows = b.flows(a, &indices, rates, p)?;
            for row in &rows {
                for v in [&row.a, &row.time, &row.interest, &row.principal] {
                    finite(v)?;
                }
            }
            Ok(rows)
        },
        Flow::bytes,
    )?;
    Ok((keys, values))
}
fn price_rows(
    flows: &[Arc<Flow>],
    values: &[f64],
    p: usize,
    solve: bool,
    deposit: bool,
) -> Result<Vec<f64>, String> {
    if flows.is_empty() {
        return Ok(vec![]);
    }
    let mut offsets = vec![0.];
    let mut times = vec![];
    let mut amounts = vec![];
    for row in flows {
        times.extend(&row.time);
        amounts.extend(&row.a);
        offsets.push(times.len() as f64);
    }
    let out = oas(
        &[
            vector(&offsets),
            vector(&times),
            vector(&amounts),
            vector(values),
            vector(&[p as f64]),
            vector(&[1e-8]),
            vector(&[40.]),
            vector(&[if deposit { -0.15 } else { -0.05 }]),
            vector(&[0.3]),
        ],
        solve,
    );
    if solve && out[1].iter().zip(values).any(|(a, b)| (a - b).abs() > 1e-8) {
        return Err("OAS solve did not converge within the supported bracket".into());
    }
    finite(&out[0])?;
    Ok(out[0].clone())
}

/// Saved-book calibration controller: base market only, no scenario repricing.
pub fn calibrate_books(a: &AccountingRequest) -> Result<BTreeMap<String, Vec<f64>>, String> {
    a.validate()?;
    if a.forecast.is_some() || a.draws.is_some() {
        return Err("calibration requires the original seeded risk-neutral market".into());
    }
    let mut books = Vec::new();
    if let Some(b) = &a.mbs {
        books.push(Book::Mortgage(b));
    }
    books.extend(a.terms.iter().map(Book::Term));
    if let Some(b) = &a.deposits {
        books.push(Book::Deposit(b));
    }
    let mut output = BTreeMap::new();
    for book in books {
        let p = match book {
            Book::Mortgage(b) => b.request.config.base_paths,
            _ => a.market.config.paths,
        };
        let mut spreads = Vec::with_capacity(book.ids().len());
        for first in (0..book.ids().len()).step_by(256) {
            portfolio_compute_control::checkpoint()?;
            let rows: Vec<_> = (first..(first + 256).min(book.ids().len())).collect();
            let flows: Vec<_> = book
                .flows(a, &rows, &a.market.swap_rates, p)?
                .into_iter()
                .map(Arc::new)
                .collect();
            let targets: Vec<_> = rows.iter().map(|&i| book.target(i)).collect();
            spreads.extend(price_rows(
                &flows,
                &targets,
                p,
                true,
                matches!(book, Book::Deposit(_)),
            )?);
        }
        output.insert(book.name().to_owned(), spreads);
    }
    Ok(output)
}
#[allow(clippy::too_many_arguments)]
fn marks(
    cache: &mut Cache,
    stats: &mut Stats,
    name: &str,
    flows: &Flows,
    calibration: &[Arc<Key>],
    applied: &[f64],
    p: usize,
) -> Result<Vec<f64>, String> {
    let keys: Vec<_> = flows
        .0
        .iter()
        .zip(calibration)
        .zip(applied)
        .map(|((f, c), o)| Key::new("mark", o, vec![f.clone(), c.clone()]))
        .collect::<Result<_, _>>()?;
    Ok(cache
        .batch(
            &format!("marks:{name}"),
            &keys,
            stats,
            |missing| {
                price_rows(
                    &missing
                        .iter()
                        .map(|&i| flows.1[i].clone())
                        .collect::<Vec<_>>(),
                    &missing.iter().map(|&i| applied[i]).collect::<Vec<_>>(),
                    p,
                    false,
                    name == "deposits",
                )
            },
            |_| 8,
        )?
        .iter()
        .map(|v| **v)
        .collect())
}
struct Compact {
    spreads: [f64; 3],
    risk: Vec<f64>,
    interest: Vec<f64>,
    principal: Vec<f64>,
}
impl Compact {
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.risk.capacity() + self.interest.capacity() + self.principal.capacity()) * 8
    }
}
#[derive(Serialize)]
struct Position {
    id: String,
    base_oas_bp: f64,
    applied_oas_bp: f64,
    model_price: f64,
    notional: f64,
    market_value: f64,
    #[serde(flatten)]
    analytics: BTreeMap<String, f64>,
}
impl PriceRequest {
    fn run(mut self, cache: &mut Cache) -> Result<Value, String> {
        self.base.validate()?;
        self.current.validate()?;
        finite(&[self.spread_shift])?;
        if self.base.asof != self.current.asof
            || self.base.horizon != self.current.horizon
            || self.base.market.config.months != self.current.market.config.months
            || self.base.market.config.paths != self.current.market.config.paths
            || self.base.market.seed != self.current.market.seed
        {
            return Err("incremental graph grids must agree".into());
        }
        if self.base.forecast.is_some()
            || self.current.forecast.is_some()
            || self.base.draws.is_some()
            || self.current.draws.is_some()
        {
            return Err("incremental graph requires seeded risk-neutral inputs".into());
        }
        let base = books(&self.base);
        let current = books(&self.current);
        if base.keys().ne(current.keys()) || self.overrides.keys().any(|k| !base.contains_key(k)) {
            return Err("valuation books must match calibration scope".into());
        }
        let (p, h) = (self.base.market.config.paths, self.base.horizon);
        let mut stats = Stats::new();
        let mut positions = BTreeMap::new();
        let mut totals = BTreeMap::new();
        let mut dv01s = BTreeMap::new();
        let mut income_total = vec![0.; h];
        let mut expense_total = vec![0.; h];
        let mut runoff = BTreeMap::new();
        for (name, b) in &base {
            let v = current[name];
            if b.ids() != v.ids() {
                return Err("valuation instruments must match baseline IDs and order".into());
            }
            // Amounts and targets are deliberately excluded from cashflow keys.
            // Validate them on every request, including an entirely warm graph.
            for i in 0..b.ids().len() {
                for book in [*b, v] {
                    finite(&[book.target(i), book.notional(i)])?;
                    if book.target(i) <= 0. || book.notional(i) < 0. {
                        return Err("invalid graph price or notional".into());
                    }
                }
            }
            let shifts = self.overrides.get(name);
            if let Some(x) = shifts {
                for (id, spread) in x {
                    if !b.ids().contains(id) {
                        return Err("spread override refers to unknown position".into());
                    }
                    if !spread.is_finite() || spread.abs() > 2000. {
                        return Err(
                            "instrument spread shifts must be finite and within +/-2000 bp".into(),
                        );
                    }
                }
            }
            let pb = if let Book::Mortgage(b) = b {
                b.request.config.base_paths
            } else {
                p
            };
            let mut rows = vec![];
            let mut monthly = vec![0.; h];
            let mut principal = vec![0.; h];
            for first in (0..b.ids().len()).step_by(256) {
                portfolio_compute_control::checkpoint()?;
                let end = (first + 256).min(b.ids().len());
                let all_indices: Vec<_> = (first..end).collect();
                let base_keys = b.keys(
                    &all_indices,
                    &b.context(&self.base, &self.base.market.swap_rates, pb)?,
                )?;
                let current_keys = v.keys(
                    &all_indices,
                    &v.context(&self.current, &self.current.market.swap_rates, pb)?,
                )?;
                let result_keys: Vec<_> = all_indices
                    .iter()
                    .enumerate()
                    .map(|(j, &i)| {
                        Key::new(
                            "result",
                            (
                                b.target(i),
                                b.yields(i),
                                p,
                                h,
                                self.include_analytics,
                                self.include_key_rates,
                                if name == "deposits" {
                                    0.
                                } else {
                                    self.spread_shift
                                },
                                shifts
                                    .and_then(|s| s.get(&b.ids()[i]))
                                    .copied()
                                    .unwrap_or(0.),
                            ),
                            vec![base_keys[j].clone(), current_keys[j].clone()],
                        )
                    })
                    .collect::<Result<_, _>>()?;
                let mut retained: Vec<_> = result_keys
                    .iter()
                    .map(|k| cache.get::<Compact>(k))
                    .collect();
                let indices: Vec<_> = all_indices
                    .iter()
                    .enumerate()
                    .filter_map(|(j, &i)| retained[j].is_none().then_some(i))
                    .collect();
                let n = indices.len();
                let risk_names: Vec<String> = if self.include_analytics {
                    std::iter::once("dv01".into())
                        .chain(
                            self.current
                                .market
                                .tenors
                                .iter()
                                .filter(|_| self.include_key_rates)
                                .map(|t| format!("krd01_{}y", *t as usize)),
                        )
                        .collect()
                } else {
                    vec![]
                };
                if n > 0 {
                    let bf = flows(
                        cache,
                        &mut stats,
                        *b,
                        &self.base,
                        &indices,
                        &self.base.market.swap_rates,
                        pb,
                    )?;
                    let ck: Vec<_> = indices
                        .iter()
                        .enumerate()
                        .map(|(j, &i)| Key::new("oas", b.target(i), vec![bf.0[j].clone()]))
                        .collect::<Result<_, _>>()?;
                    let spread = cache.batch(
                        &format!("calibration:{name}"),
                        &ck,
                        &mut stats,
                        |missing| {
                            price_rows(
                                &missing.iter().map(|&j| bf.1[j].clone()).collect::<Vec<_>>(),
                                &missing
                                    .iter()
                                    .map(|&j| b.target(indices[j]))
                                    .collect::<Vec<_>>(),
                                pb,
                                true,
                                name == "deposits",
                            )
                        },
                        |_| 8,
                    )?;
                    let applied: Vec<_> = indices
                        .iter()
                        .enumerate()
                        .map(|(j, &i)| {
                            *spread[j]
                                + if name == "deposits" {
                                    0.
                                } else {
                                    self.spread_shift
                                }
                                + shifts
                                    .and_then(|s| s.get(&b.ids()[i]))
                                    .copied()
                                    .unwrap_or(0.)
                                    * 1e-4
                        })
                        .collect();
                    let current_keys = v.keys(
                        &indices,
                        &v.context(&self.current, &self.current.market.swap_rates, pb)?,
                    )?;
                    let cf = if current_keys == bf.0 {
                        bf.clone()
                    } else {
                        flows(
                            cache,
                            &mut stats,
                            v,
                            &self.current,
                            &indices,
                            &self.current.market.swap_rates,
                            pb,
                        )?
                    };
                    let values = marks(cache, &mut stats, name, &cf, &ck, &applied, pb)?;
                    let mut chunk: Vec<_> = indices
                        .iter()
                        .enumerate()
                        .map(|(j, &i)| Position {
                            id: b.ids()[i].clone(),
                            base_oas_bp: *spread[j] * 1e4,
                            applied_oas_bp: applied[j] * 1e4,
                            model_price: values[j] * 100.,
                            notional: 1.,
                            market_value: values[j],
                            analytics: BTreeMap::new(),
                        })
                        .collect();
                    let mut packed: Vec<_> = (0..n)
                        .map(|j| Compact {
                            spreads: [*spread[j] * 1e4, applied[j] * 1e4, values[j]],
                            risk: vec![],
                            interest: vec![],
                            principal: vec![],
                        })
                        .collect();
                    if self.include_analytics {
                        let legs = 1 + if self.include_key_rates {
                            self.current.market.tenors.len()
                        } else {
                            0
                        };
                        for leg in 0..legs {
                            let bump = if leg == 0 { 25. } else { 1. };
                            let mut prices = vec![];
                            for sign in [-1., 1.] {
                                let rates: Vec<_> = self
                                    .current
                                    .market
                                    .swap_rates
                                    .iter()
                                    .enumerate()
                                    .map(|(j, r)| {
                                        r + if leg == 0 || j == leg - 1 {
                                            sign * bump * 1e-4
                                        } else {
                                            0.
                                        }
                                    })
                                    .collect();
                                let f = flows(
                                    cache,
                                    &mut stats,
                                    v,
                                    &self.current,
                                    &indices,
                                    &rates,
                                    p,
                                )?;
                                prices.push(marks(cache, &mut stats, name, &f, &ck, &applied, p)?);
                            }
                            let label = if leg == 0 {
                                "dv01".into()
                            } else {
                                format!("krd01_{}y", self.current.market.tenors[leg - 1] as usize)
                            };
                            for j in 0..n {
                                let notional = chunk[j].notional;
                                chunk[j].analytics.insert(
                                    label.clone(),
                                    (prices[0][j] - prices[1][j]) / (2. * bump) * notional,
                                );
                            }
                        }
                        let nf = if pb == p {
                            cf.clone()
                        } else {
                            flows(
                                cache,
                                &mut stats,
                                v,
                                &self.current,
                                &indices,
                                &self.current.market.swap_rates,
                                p,
                            )?
                        };
                        let bk = if pb == p {
                            bf.clone()
                        } else {
                            let keys = b.keys(
                                &indices,
                                &b.context(&self.base, &self.base.market.swap_rates, p)?,
                            )?;
                            if keys == nf.0 {
                                nf.clone()
                            } else {
                                flows(
                                    cache,
                                    &mut stats,
                                    *b,
                                    &self.base,
                                    &indices,
                                    &self.base.market.swap_rates,
                                    p,
                                )?
                            }
                        };
                        let ik: Vec<_> = indices
                            .iter()
                            .enumerate()
                            .map(|(j, &i)| {
                                Key::new(
                                    "income",
                                    (b.target(i), b.yields(i), h),
                                    vec![nf.0[j].clone(), bk.0[j].clone()],
                                )
                            })
                            .collect::<Result<_, _>>()?;
                        let amounts = cache.batch(
                            &format!("income:{name}"),
                            &ik,
                            &mut stats,
                            |missing| {
                                missing
                                    .iter()
                                    .map(|&j| {
                                        let f = &nf.1[j];
                                        if name == "cds" || name == "deposits" {
                                            return Ok(f.interest[..h].to_vec());
                                        }
                                        let i = indices[j];
                                        let reference = &bk.1[j];
                                        let y = if let Some(y) = b.yields(i) {
                                            y
                                        } else {
                                            let cf: Vec<_> = reference
                                                .interest
                                                .iter()
                                                .zip(&reference.principal)
                                                .map(|(a, b)| a + b)
                                                .collect();
                                            income(&[
                                                matrix(&cf, 1, cf.len()),
                                                vector(&[b.target(i)]),
                                                vector(&[1.]),
                                                vector(&[60.]),
                                            ])[2][0]
                                        };
                                        let mut bv = if b.yields(i).is_some() {
                                            1.
                                        } else {
                                            b.target(i)
                                        };
                                        let mut out = vec![];
                                        for m in 0..h {
                                            let inc = bv * y / 12.;
                                            bv += inc - f.interest[m] - f.principal[m];
                                            out.push(inc);
                                        }
                                        finite(&out)?;
                                        Ok(out)
                                    })
                                    .collect::<Result<Vec<_>, String>>()
                            },
                            |v| v.capacity() * 8 + 24,
                        )?;
                        for j in 0..n {
                            packed[j].interest = amounts[j].to_vec();
                            packed[j].principal = nf.1[j].principal[..h].to_vec();
                            packed[j].risk =
                                risk_names.iter().map(|k| chunk[j].analytics[k]).collect();
                        }
                    }
                    for (i, value) in indices.iter().zip(packed) {
                        let bytes = value.bytes();
                        let value = Arc::new(value);
                        let j = i - first;
                        cache.put(result_keys[j].clone(), value.clone(), bytes);
                        retained[j] = Some(value);
                    }
                }
                for (&i, value) in all_indices.iter().zip(retained) {
                    let value = value.ok_or("missing native instrument result")?;
                    let notional = v.notional(i);
                    let sign = if name == "mbs" || name == "loans" {
                        1.
                    } else {
                        -1.
                    };
                    let mut analytics: BTreeMap<_, _> = risk_names
                        .iter()
                        .zip(&value.risk)
                        .map(|(k, r)| (k.clone(), r * notional))
                        .collect();
                    if self.include_analytics {
                        analytics.insert(
                            "nii_total".into(),
                            value.interest.iter().map(|v| v * notional).sum::<f64>() * sign,
                        );
                        for m in 0..h {
                            monthly[m] += value.interest[m] * notional;
                            principal[m] += value.principal[m] * notional;
                        }
                    }
                    rows.push(Position {
                        id: b.ids()[i].clone(),
                        base_oas_bp: value.spreads[0],
                        applied_oas_bp: value.spreads[1],
                        model_price: value.spreads[2] * 100.,
                        notional,
                        market_value: value.spreads[2] * notional,
                        analytics,
                    });
                }
            }
            totals.insert(
                name.clone(),
                rows.iter().map(|r| r.market_value).sum::<f64>(),
            );
            if self.include_analytics && !rows.is_empty() {
                dv01s.insert(
                    name.clone(),
                    rows.iter().map(|r| r.analytics["dv01"]).sum::<f64>(),
                );
                let target = if name == "mbs" || name == "loans" {
                    &mut income_total
                } else {
                    &mut expense_total
                };
                for m in 0..h {
                    target[m] += monthly[m];
                }
                runoff.insert(name.clone(), principal);
            }
            positions.insert(name.clone(), rows);
        }
        let net = totals
            .iter()
            .map(|(k, v)| v * if k == "mbs" || k == "loans" { 1. } else { -1. })
            .sum::<f64>();
        let mut out = json!({"positions":positions,"totals":totals,"scope_net_value":net,"graph":stats,"cache":cache.info()});
        if self.include_analytics {
            if let Some(aux) = &self.auxiliary {
                if aux.books.mbs.is_some()
                    || aux.books.deposits.is_some()
                    || !aux.books.terms.is_empty()
                {
                    return Err("auxiliary graph input overlaps selected books".into());
                }
                let result = aux.books.run()?;
                let dv = aux.run()?;
                for m in 0..h {
                    income_total[m] += result.monthly["interest_income"][m];
                    expense_total[m] += result.monthly["interest_expense"][m];
                }
                dv01s.extend(dv.dv01s);
            }
            let nii: Vec<_> = income_total
                .iter()
                .zip(&expense_total)
                .map(|(a, b)| a - b)
                .collect();
            out["nii"] = json!({"interest_income":income_total,"interest_expense":expense_total,"nii":nii,"runoff":runoff,"total":nii.iter().sum::<f64>()});
            let mut k = self
                .kpis
                .take()
                .ok_or("graph analytics requires KPI weights")?;
            if k.risk.is_some() {
                return Err("graph KPI input must not request another risk calculation".into());
            }
            k.nii = Some(nii);
            k.dv01s = dv01s.clone();
            // Capital and NSFR use supplied book balances; EVE/LCR use modeled prices.
            k.mode = crate::kpi_lifecycle::Mode::Nsfr;
            let nsfr = k.run()?;
            k.mode = crate::kpi_lifecycle::Mode::Capital;
            let capital = k.run()?;
            for (name, rows) in &positions {
                let book = k.books.get_mut(name).ok_or("missing graph KPI book")?;
                if book.len() != rows.len() || book.iter().zip(rows).any(|(a, b)| a.id != b.id) {
                    return Err("graph KPI instrument mismatch".into());
                }
                for (a, b) in book.iter_mut().zip(rows) {
                    a.price = b.model_price;
                }
            }
            k.mode = crate::kpi_lifecycle::Mode::Eve;
            let eve = k.run()?;
            k.mode = crate::kpi_lifecycle::Mode::Lcr;
            let lcr = k.run()?;
            out["kpis"] = json!({"eve":eve,"lcr":lcr,"nsfr":nsfr,"capital":capital,"dv01s":dv01s});
        }
        Ok(out)
    }
}

fn compare(
    mut request: PriceRequest,
    patches: BTreeMap<String, Value>,
    mode: &str,
    cache: &mut Cache,
) -> Result<Value, String> {
    if !["hold", "recalibrate"].contains(&mode) {
        return Err("unknown calibration mode".into());
    }
    request.base.validate()?;
    let identities: std::collections::HashSet<_> = books(&request.base)
        .iter()
        .flat_map(|(book, rows)| rows.ids().iter().map(move |id| format!("{book}:{id}")))
        .collect();
    for (key, patch) in &patches {
        if !identities.contains(key) {
            return Err("unknown instrument in assumption overrides".into());
        }
        let (book, _) = key.split_once(':').ok_or("invalid instrument key")?;
        crate::decision_inputs::validate_patch(book, patch)?;
        if patch
            .as_object()
            .is_some_and(|p| p.values().any(Value::is_null))
        {
            return Err("assumption overrides must be numeric".into());
        }
    }
    let mut baseline = request.clone();
    baseline.current = baseline.base.clone();
    baseline.spread_shift = 0.;
    baseline.overrides.clear();
    if let Some(aux) = &mut baseline.auxiliary {
        aux.books.market = baseline.base.market.clone();
    }
    let baseline = baseline.run(cache)?;
    crate::decision_inputs::apply(&mut request.current, &patches)?;
    if mode == "recalibrate" {
        let market = request.base.market.clone();
        request.base = request.current.clone();
        request.base.market = market;
        if let Some(b) = &mut request.base.mbs {
            b.request.swap_rates = request.base.market.swap_rates.clone();
            b.request.vol_quotes = request.base.market.vol_quotes.clone();
        }
    }
    let revised = request.run(cache)?;
    let mut comparison = BTreeMap::new();
    for (book, rows) in revised["positions"]
        .as_object()
        .ok_or("missing revised positions")?
    {
        let before = baseline["positions"][book]
            .as_array()
            .ok_or("missing baseline positions")?;
        let mut changes = vec![];
        for (row, old) in rows
            .as_array()
            .ok_or("invalid revised positions")?
            .iter()
            .zip(before)
        {
            let mut row = row.clone();
            let n = |v: &Value| v.as_f64().ok_or("nonfinite comparison output");
            row["original_price"] = old["model_price"].clone();
            row["original_value"] = old["market_value"].clone();
            row["price_change"] = json!(n(&row["model_price"])? - n(&old["model_price"])?);
            row["value_change"] = json!(n(&row["market_value"])? - n(&old["market_value"])?);
            changes.push(row);
        }
        comparison.insert(book.clone(), changes);
    }
    let delta = revised["scope_net_value"]
        .as_f64()
        .ok_or("invalid revised value")?
        - baseline["scope_net_value"]
            .as_f64()
            .ok_or("invalid baseline value")?;
    Ok(
        json!({"baseline":baseline,"revised":revised,"comparison":comparison,"net_value_change":delta,"calibration_mode":mode}),
    )
}
