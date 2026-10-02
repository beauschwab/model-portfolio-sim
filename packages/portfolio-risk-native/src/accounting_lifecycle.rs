//! Raw-book monthly accounting with bounded cashflow chunks and native aggregation.
use crate::deposit_lifecycle::{
    cashflows, deposit_paths, equilibrium, Assumptions, Cohort, DepositDeck,
};
use crate::forecast_lifecycle::Forecast;
use crate::hedge_lifecycle::{smear, swap_values, term_flows, HedgeDeck, Swap};
use crate::lifecycle_market::{admit, finite, matrix, vector, MarketInput, RateMarket};
use crate::market::{MarketContext, RatePaths};
use crate::mortgage_input::OwnedMortgage;
use crate::quant::{corporate, income};
use crate::random::SharedDraws;
use crate::term_deck::{Deck, DeckRequest, Product};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DrawInput {
    pub rates: Vec<f64>,
    pub spread: Vec<f64>,
    pub hpi: Vec<f64>,
    pub shape: [usize; 3],
}
impl DrawInput {
    fn build(&self, shape: [usize; 3]) -> Result<SharedDraws, String> {
        let [p, t, f] = shape;
        let count = admit(&shape, 128 * 1024 * 1024)?;
        if self.shape != shape
            || self.rates.len() != count
            || self.spread.len() != p * t
            || self.hpi.len() != p * t
        {
            return Err("accounting draw shape mismatch".into());
        }
        for v in [&self.rates, &self.spread, &self.hpi] {
            finite(v)?;
        }
        Ok(SharedDraws {
            rates: self.rates.clone(),
            spread: self.spread.clone(),
            hpi: self.hpi.clone(),
            shape: [p, t, f],
        })
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TermBook {
    pub key: String,
    pub ids: Vec<String>,
    pub book_yields: Option<Vec<f64>>,
    pub deck: DeckRequest,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MortgageBook {
    pub ids: Vec<String>,
    pub book_yields: Option<Vec<f64>>,
    pub request: OwnedMortgage,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DepositBook {
    pub ids: Vec<String>,
    pub book: Vec<Cohort>,
    pub assumptions: Assumptions,
    pub history: Vec<[f64; 2]>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoneyRow {
    pub id: String,
    pub balance: f64,
    pub side: String,
    pub spread_bp: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    #[serde(rename = "yield")]
    pub yields: Vec<f64>,
    pub opening: Vec<f64>,
    pub coupon_income: Option<Vec<f64>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingRequest {
    pub market: MarketInput,
    pub asof: i32,
    pub horizon: usize,
    pub mbs: Option<MortgageBook>,
    pub terms: Vec<TermBook>,
    pub deposits: Option<DepositBook>,
    pub mm: Option<Vec<MoneyRow>>,
    pub hedges: Option<Vec<Swap>>,
    pub withdrawal_parameters: Vec<f64>,
    pub anchors: BTreeMap<String, Anchor>,
    pub deposit_initial_rate: Option<f64>,
    pub forecast: Option<Forecast>,
    pub draws: Option<DrawInput>,
    pub capture_anchor: bool,
    pub capture_cashflows: bool,
}
#[derive(Serialize)]
pub struct YieldRow {
    pub book: String,
    pub id: String,
    pub book_yield: f64,
    pub balance: f64,
}
#[derive(Serialize)]
pub struct Opening {
    pub book: String,
    pub id: String,
    pub balance: f64,
    pub book_adjustment: f64,
    pub side: String,
    pub market_price: f64,
}
#[derive(Serialize)]
pub struct InstrumentFlow {
    pub book: String,
    pub id: String,
    pub month: usize,
    pub principal: f64,
    pub cash_interest: f64,
    pub accrual_interest: f64,
    pub book_amortization: f64,
}
#[derive(Serialize)]
pub struct AccountingResult {
    pub monthly: BTreeMap<String, Vec<f64>>,
    pub column_order: Vec<String>,
    pub runoff: BTreeMap<String, Vec<f64>>,
    pub runoff_order: Vec<String>,
    pub summary: [Option<f64>; 3],
    pub book_yields: Vec<YieldRow>,
    pub anchors: BTreeMap<String, Anchor>,
    pub deposit_initial_rate: Option<f64>,
    pub instrument_openings: Vec<Opening>,
    pub instrument_cashflows: Vec<InstrumentFlow>,
}
struct State<'a> {
    request: &'a AccountingRequest,
    out: AccountingResult,
    earning: Vec<f64>,
}
impl State<'_> {
    fn column(&mut self, key: &str) {
        if !self.out.monthly.contains_key(key) {
            self.out.column_order.push(key.into());
            self.out
                .monthly
                .insert(key.into(), vec![0.; self.request.horizon]);
        }
    }
    fn runoff(&mut self, key: &str) {
        if !self.out.runoff.contains_key(key) {
            self.out.runoff_order.push(key.into());
            self.out
                .runoff
                .insert(key.into(), vec![0.; self.request.horizon]);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn capture(
        &mut self,
        key: &str,
        ids: &[String],
        balances: &[f64],
        opening: &[f64],
        prices: &[f64],
        principal: &[f64],
        cash: &[f64],
        accrual: &[f64],
        effective: &[f64],
        side: &str,
        stride: usize,
    ) {
        if !self.request.capture_cashflows {
            return;
        }
        let h = self.request.horizon;
        for i in 0..ids.len() {
            let bal = balances[i];
            self.out.instrument_openings.push(Opening {
                book: key.into(),
                id: ids[i].clone(),
                balance: bal,
                book_adjustment: (opening[i] - 1.) * bal,
                side: side.into(),
                market_price: prices[i],
            });
            for m in 0..h {
                let j = i * stride + m;
                self.out.instrument_cashflows.push(InstrumentFlow {
                    book: key.into(),
                    id: ids[i].clone(),
                    month: m + 1,
                    principal: principal[j] * bal,
                    cash_interest: cash[j] * bal,
                    accrual_interest: accrual[j] * bal,
                    book_amortization: (effective[i * h + m] - accrual[j]) * bal,
                });
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn asset(
        &mut self,
        key: &str,
        label: &str,
        ids: &[String],
        first: usize,
        coupon: &[f64],
        cash: &[f64],
        principal: &[f64],
        prices: &[f64],
        balances: &[f64],
        book_yields: Option<&[f64]>,
        floating: Option<&[f64]>,
    ) -> Result<(), String> {
        let h = self.request.horizon;
        let t = self.request.market.config.months;
        let n = ids.len();
        let cf: Vec<_> = coupon.iter().zip(principal).map(|(a, b)| a + b).collect();
        let reference = self.request.anchors.get(key);
        let (inc, bvs, yields) = if reference.is_some() || book_yields.is_some() {
            let ys = if let Some(a) = reference {
                a.yields[first..first + n].to_vec()
            } else {
                book_yields.unwrap()[first..first + n].to_vec()
            };
            let mut bv = if let Some(a) = reference {
                a.opening[first..first + n].to_vec()
            } else {
                vec![1.; n]
            };
            let mut inc = vec![0.; n * h];
            let mut bvs = vec![0.; n * h];
            for i in 0..n {
                for m in 0..h {
                    let mut value = bv[i] * ys[i] / 12.;
                    if let (Some(a), Some(fl)) = (reference, floating) {
                        if fl[i] != 0. {
                            value += coupon[i * t + m]
                                - a.coupon_income
                                    .as_ref()
                                    .ok_or("floating anchor requires coupon income")?
                                    [(first + i) * t + m];
                        }
                    }
                    inc[i * h + m] = value;
                    bv[i] += value - cf[i * t + m];
                    bvs[i * h + m] = bv[i];
                }
            }
            (inc, bvs, ys)
        } else {
            let mut x = income(&[
                matrix(&cf, n, t),
                vector(prices),
                vector(&[h as f64]),
                vector(&[60.]),
            ]);
            let ys = x.pop().unwrap();
            let bv = x.pop().unwrap();
            (x.remove(0), bv, ys)
        };
        self.column(label);
        self.runoff(key);
        let opening = if book_yields.is_some() {
            vec![1.; n]
        } else {
            prices.to_vec()
        };
        if self.request.capture_anchor {
            let anchor = self.out.anchors.entry(key.into()).or_insert(Anchor {
                yields: vec![],
                opening: vec![],
                coupon_income: if floating.is_some() {
                    Some(vec![])
                } else {
                    None
                },
            });
            anchor.yields.extend_from_slice(&yields);
            anchor.opening.extend_from_slice(&opening);
            if let Some(v) = &mut anchor.coupon_income {
                v.extend_from_slice(coupon);
            }
        }
        let side = if key == "debt" { "liability" } else { "asset" };
        for i in 0..n {
            self.out.book_yields.push(YieldRow {
                book: key.into(),
                id: ids[i].clone(),
                book_yield: yields[i],
                balance: balances[i],
            });
            for m in 0..h {
                self.out.monthly.get_mut(label).unwrap()[m] += inc[i * h + m] * balances[i];
                self.out.runoff.get_mut(key).unwrap()[m] += principal[i * t + m] * balances[i];
                if side == "asset" {
                    self.earning[m] += bvs[i * h + m] * balances[i];
                }
            }
        }
        self.capture(
            key, ids, balances, &opening, prices, principal, cash, coupon, &inc, side, t,
        );
        Ok(())
    }
}
fn validate_rows(
    ids: &[String],
    n: usize,
    yields: Option<&Vec<f64>>,
    anchor: Option<&Anchor>,
    t: usize,
) -> Result<(), String> {
    if ids.len() != n {
        return Err("accounting identifier shape mismatch".into());
    }
    if let Some(y) = yields {
        if y.len() != n {
            return Err("accounting yield shape mismatch".into());
        }
        finite(y)?;
    }
    if let Some(a) = anchor {
        if a.yields.len() != n
            || a.opening.len() != n
            || a.coupon_income.as_ref().is_some_and(|x| x.len() != n * t)
        {
            return Err("accounting anchor shape mismatch".into());
        }
        finite(&a.yields)?;
        finite(&a.opening)?;
        if let Some(v) = &a.coupon_income {
            finite(v)?;
        }
    }
    Ok(())
}
pub(crate) fn cd_flows(
    deck: &Deck,
    paths: &RatePaths,
    p: usize,
    t: usize,
    parameters: &[f64],
) -> Vec<Vec<f64>> {
    corporate(
        &[
            matrix(&paths.short, p, t),
            matrix(&paths.df, p, t),
            vector(&deck.per_off),
            vector(&deck.pay_m),
            vector(&deck.pay_frac),
            vector(&deck.acc_m),
            vector(&deck.tau),
            vector(&deck.rem_y),
            vector(&deck.call_px),
            vector(&deck.cpn),
            vector(&deck.pen_m),
            vector(&deck.ew_mult),
            vector(&deck.call_thr),
            vector(parameters),
        ],
        true,
    )
}
impl AccountingRequest {
    pub fn validate(&self) -> Result<(), String> {
        let c = &self.market.config;
        let (t, h) = (c.months, self.horizon);
        if h == 0 || h > t {
            return Err("invalid accounting horizon".into());
        }
        let n = self.terms.iter().map(|b| b.ids.len()).sum::<usize>()
            + self.mbs.as_ref().map_or(0, |b| b.ids.len())
            + self.deposits.as_ref().map_or(0, |b| b.ids.len())
            + self.mm.as_ref().map_or(0, Vec::len);
        admit(
            &[n, if self.capture_cashflows { h * 8 } else { 1 }],
            16 * 1024 * 1024,
        )?;
        if self.capture_anchor {
            admit(&[n, t], 16 * 1024 * 1024)?;
        }
        if self.forecast.is_some()
            && self.anchors.is_empty()
            && self.deposit_initial_rate.is_none()
            && (self.mbs.is_some() || !self.terms.is_empty() || self.deposits.is_some())
        {
            return Err("conditional forecast requires base accounting anchors".into());
        }
        for term in &self.terms {
            if !["loans", "debt", "cds"].contains(&term.key.as_str())
                || term.deck.months != t
                || term.deck.asof != self.asof
                || (term.key == "cds") != (term.deck.product == Product::Cd)
            {
                return Err("invalid accounting term book".into());
            }
            validate_rows(
                &term.ids,
                term.deck.contracts.len(),
                term.book_yields.as_ref(),
                self.anchors.get(&term.key),
                t,
            )?;
        }
        for (i, b) in self.terms.iter().enumerate() {
            if self.terms[..i].iter().any(|other| other.key == b.key) {
                return Err("duplicate accounting term book".into());
            }
        }
        if self.terms.iter().any(|b| b.key == "cds") {
            if self.withdrawal_parameters.len() != 5 {
                return Err("invalid CD withdrawal parameters".into());
            }
            finite(&self.withdrawal_parameters)?;
        }
        if let Some(book) = &self.mbs {
            validate_rows(
                &book.ids,
                book.request.book.len() / 13,
                book.book_yields.as_ref(),
                self.anchors.get("mbs"),
                t,
            )?;
            self.market.validate_mortgage(&book.request)?;
        }
        if let Some(book) = &self.deposits {
            validate_rows(&book.ids, book.book.len(), None, None, t)?;
        }
        Ok(())
    }
    pub fn run(&self) -> Result<AccountingResult, String> {
        crate::compute_span!("accounting");
        self.validate()?;
        let c = &self.market.config;
        let (p, t, h) = (c.paths, c.months, self.horizon);
        let market = RateMarket::new(&self.market)?;
        let draws = if let Some(d) = &self.draws {
            d.build([p, t, c.factors])?
        } else {
            SharedDraws::new(&self.market.seed, [p, t, c.factors])?
        };
        let raw = if self.draws.is_none() {
            (*market.base()?).clone()
        } else {
            MarketContext::new(
                &self.market.tenors,
                &self.market.swap_rates,
                &market.abcd,
                &market.b,
                &draws.rates,
                draws.shape,
                [c.dt, c.tenor, c.shift],
            )?
            .rates()?
        };
        let paths = if let Some(plan) = &self.forecast {
            plan.rates(&raw, p, t, c.dt, c.shift)?
        } else {
            raw
        };
        let mut state = State {
            request: self,
            earning: vec![0.; h],
            out: AccountingResult {
                monthly: BTreeMap::new(),
                column_order: vec![],
                runoff: BTreeMap::new(),
                runoff_order: vec![],
                summary: [None; 3],
                book_yields: vec![],
                anchors: BTreeMap::new(),
                deposit_initial_rate: None,
                instrument_openings: vec![],
                instrument_cashflows: vec![],
            },
        };
        if let Some(book) = &self.mbs {
            validate_rows(
                &book.ids,
                book.request.book.len() / 13,
                book.book_yields.as_ref(),
                self.anchors.get("mbs"),
                t,
            )?;
            if book.request.config.months != t
                || book.request.config.factors != c.factors
                || book.request.seed != self.market.seed
                || book.request.swap_rates != self.market.swap_rates
            {
                return Err("accounting mortgage market mismatch".into());
            }
            book.request.borrow().income_chunks(
                &draws,
                self.forecast.as_ref(),
                |first, end, coupon, principal, prices, balances| {
                    state.asset(
                        "mbs",
                        "mbs_income",
                        &book.ids[first..end],
                        first,
                        coupon,
                        coupon,
                        principal,
                        prices,
                        balances,
                        book.book_yields.as_deref(),
                        None,
                    )
                },
            )?;
        }
        for key in ["loans", "debt"] {
            if let Some(book) = self.terms.iter().find(|b| b.key == key) {
                for first in (0..book.ids.len()).step_by(256) {
                    portfolio_compute_control::checkpoint()?;
                    let end = (first + 256).min(book.ids.len());
                    let deck = book.deck.build_rows(&book.deck.contracts[first..end])?;
                    let mut flows = term_flows(&deck, &paths, p, t);
                    for j in [1, 2] {
                        for v in &mut flows[j] {
                            *v /= p as f64;
                        }
                    }
                    let coupon = smear(&deck, &flows[1], t, true);
                    let cash = smear(&deck, &flows[1], t, false);
                    let principal = smear(&deck, &flows[2], t, false);
                    state.asset(
                        key,
                        if key == "loans" {
                            "loan_income"
                        } else {
                            "debt_expense"
                        },
                        &book.ids[first..end],
                        first,
                        &coupon,
                        &cash,
                        &principal,
                        &deck.tgt,
                        &deck.notional,
                        book.book_yields.as_deref(),
                        Some(&deck.is_float),
                    )?;
                }
            }
        }
        if let Some(book) = &self.deposits {
            validate_rows(&book.ids, book.book.len(), None, None, t)?;
            let history: Vec<_> = book.history.iter().flatten().copied().collect();
            let fit = crate::calibration::fit_deposits(&history)?;
            let r0 = self.deposit_initial_rate.unwrap_or_else(|| {
                equilibrium(
                    &fit.x,
                    (0..p).map(|i| paths.short[i * t]).sum::<f64>() / p as f64,
                )
            });
            finite(&[r0])?;
            if self.capture_anchor {
                state.out.deposit_initial_rate = Some(r0);
            }
            let dep = deposit_paths(&paths.short, &paths.df, &fit.x, r0, p, t);
            state.column("deposit_expense");
            state.runoff("deposits");
            for first in (0..book.ids.len()).step_by(256) {
                portfolio_compute_control::checkpoint()?;
                let end = (first + 256).min(book.ids.len());
                let deck = DepositDeck::new(&book.book[first..end], &book.assumptions)?;
                let mut flow = cashflows(
                    &deck,
                    &book.assumptions,
                    &dep,
                    r0,
                    p,
                    t,
                    &vec![0.; deck.n],
                    &[0.],
                    false,
                    None,
                );
                for j in [1, 5] {
                    for v in &mut flow[j] {
                        *v /= p as f64;
                    }
                }
                let mut effective = vec![0.; deck.n * h];
                for i in 0..deck.n {
                    for m in 0..h {
                        let x = i * t + m;
                        effective[i * h + m] = flow[5][x];
                        state.out.monthly.get_mut("deposit_expense").unwrap()[m] +=
                            flow[5][x] * deck.bal[i];
                        state.out.runoff.get_mut("deposits").unwrap()[m] +=
                            flow[1][x] * deck.bal[i];
                    }
                }
                state.capture(
                    "deposits",
                    &book.ids[first..end],
                    &deck.bal,
                    &vec![1.; deck.n],
                    &vec![1.; deck.n],
                    &flow[1],
                    &flow[5],
                    &flow[5],
                    &effective,
                    "liability",
                    t,
                );
            }
        }
        if let Some(book) = &self.mm {
            state.column("mm_income");
            state.column("mm_expense");
            let means: Vec<_> = (0..h)
                .map(|m| (0..p).map(|i| paths.short[i * t + m]).sum::<f64>() / p as f64)
                .collect();
            for row in book {
                finite(&[row.balance, row.spread_bp])?;
                if row.balance < 0. || !["asset", "liability"].contains(&row.side.as_str()) {
                    return Err("invalid money-market position".into());
                }
                let rates: Vec<_> = means
                    .iter()
                    .map(|r| (r + row.spread_bp * 1e-4).max(0.) / 12.)
                    .collect();
                let label = if row.side == "asset" {
                    "mm_income"
                } else {
                    "mm_expense"
                };
                for (m, rate) in rates.iter().enumerate() {
                    state.out.monthly.get_mut(label).unwrap()[m] += rate * row.balance;
                    if row.side == "asset" {
                        state.earning[m] += row.balance;
                    }
                }
                state.capture(
                    "mm",
                    std::slice::from_ref(&row.id),
                    &[row.balance],
                    &[1.],
                    &[1.],
                    &vec![0.; h],
                    &rates,
                    &rates,
                    &rates,
                    &row.side,
                    h,
                );
            }
        }
        if let Some(book) = self.terms.iter().find(|b| b.key == "cds") {
            state.column("cd_expense");
            for first in (0..book.ids.len()).step_by(256) {
                portfolio_compute_control::checkpoint()?;
                let end = (first + 256).min(book.ids.len());
                let deck = book.deck.build_rows(&book.deck.contracts[first..end])?;
                let mut flow = cd_flows(&deck, &paths, p, t, &self.withdrawal_parameters);
                for j in [1, 2] {
                    for v in &mut flow[j] {
                        *v /= p as f64;
                    }
                }
                let coupon = smear(&deck, &flow[1], h, true);
                let cash = smear(&deck, &flow[1], h, false);
                let principal = smear(&deck, &flow[2], h, false);
                for i in 0..deck.n {
                    for m in 0..h {
                        state.out.monthly.get_mut("cd_expense").unwrap()[m] +=
                            coupon[i * h + m] * deck.notional[i];
                    }
                }
                state.capture(
                    "cds",
                    &book.ids[first..end],
                    &deck.notional,
                    &vec![1.; deck.n],
                    &vec![1.; deck.n],
                    &principal,
                    &cash,
                    &coupon,
                    &coupon,
                    "liability",
                    h,
                );
            }
        }
        if let Some(book) = &self.hedges {
            state.column("hedge_carry_income");
            for first in (0..book.len()).step_by(256) {
                portfolio_compute_control::checkpoint()?;
                let end = (first + 256).min(book.len());
                let deck = HedgeDeck::new(&book[first..end], self.asof, t)?;
                let carry = swap_values(&deck, &paths, p, t, h).1;
                for i in 0..deck.n {
                    for m in 0..h {
                        state.out.monthly.get_mut("hedge_carry_income").unwrap()[m] +=
                            carry[i * h + m] * deck.notional[i];
                    }
                }
            }
        }
        let mut income = vec![0.; h];
        let mut expense = vec![0.; h];
        for key in &state.out.column_order {
            let values = &state.out.monthly[key];
            for m in 0..h {
                if key.ends_with("income") {
                    income[m] += values[m];
                } else {
                    expense[m] += values[m];
                }
            }
        }
        let nii: Vec<_> = income.iter().zip(&expense).map(|(a, b)| a - b).collect();
        let total = nii.iter().sum::<f64>();
        let annual = total * 12. / h as f64;
        let nim = if state.earning.iter().any(|v| *v != 0.) {
            Some(annual / (state.earning.iter().sum::<f64>() / h as f64).max(1e-9) * 100.)
        } else {
            None
        };
        state.out.summary = [Some(total), Some(annual), nim];
        for (name, value) in [
            ("interest_income", income),
            ("interest_expense", expense),
            ("nii", nii),
        ] {
            state.out.monthly.insert(name.into(), value);
            state.out.column_order.push(name.into());
        }
        for v in state.out.monthly.values().chain(state.out.runoff.values()) {
            finite(v)?;
        }
        finite(&[total, annual])?;
        if let Some(v) = nim {
            finite(&[v])?;
        }
        Ok(state.out)
    }
}
