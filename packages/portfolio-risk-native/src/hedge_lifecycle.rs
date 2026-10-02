//! Raw swap legs and cash-settled swaptions, carry and complete risk orchestration.
use crate::conventions::{Bdc, CalendarSpec, DayCount};
use crate::lifecycle_market::{admit, finite, matrix, tensor, vector, MarketInput, RateMarket};
use crate::market::RatePaths;
use crate::quant::{accrual, corporate, oas, swaption};
use crate::term_deck::{Amortization, Contract, Deck, DeckRequest, Product};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Payer,
    Receiver,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Swap {
    pub notional: f64,
    pub side: Side,
    pub fixed_rate: f64,
    pub maturity: i32,
    #[serde(default)]
    pub float_spread: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Swaption {
    pub notional: f64,
    pub side: Side,
    pub strike: f64,
    pub expiry_m: usize,
    pub tenor_y: f64,
}
#[derive(Serialize)]
pub struct HedgeDeck {
    pub fix: Deck,
    pub flt: Deck,
    pub side: Vec<f64>,
    pub notional: Vec<f64>,
    pub n: usize,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeDeckInput {
    pub swaps: Vec<Swap>,
    pub asof: i32,
    pub months: usize,
}
pub(crate) fn bullet(maturity: i32, coupon: f64, floating: bool) -> Contract {
    Contract {
        maturity,
        freq_months: Some(if floating { 3 } else { 6 }),
        daycount: if floating {
            DayCount::Act360
        } else {
            DayCount::Thirty360
        },
        coupon,
        notional: 1.,
        price: 100.,
        is_float: floating,
        cap: 10.,
        floor: -10.,
        call_threshold: 0.005,
        amort_type: Amortization::Bullet,
        sink_schedule: None,
        call_schedule: None,
        put_schedule: None,
        channel: String::new(),
        penalty_months: 0.,
        ew_mult: 1.,
    }
}
impl HedgeDeck {
    pub fn new(swaps: &[Swap], asof: i32, months: usize) -> Result<Self, String> {
        for s in swaps {
            finite(&[s.notional, s.fixed_rate, s.float_spread])?;
            if s.notional < 0. {
                return Err("negative hedge notional".into());
            }
        }
        let leg = |floating| {
            DeckRequest {
                product: Product::Corporate,
                asof,
                months,
                calendar: CalendarSpec {
                    name: "US".into(),
                    extra_holidays: vec![],
                },
                bdc: Bdc::ModifiedFollowing,
                contracts: swaps
                    .iter()
                    .map(|s| {
                        bullet(
                            s.maturity,
                            if floating {
                                s.float_spread
                            } else {
                                s.fixed_rate
                            },
                            floating,
                        )
                    })
                    .collect(),
            }
            .build()
        };
        Ok(Self {
            fix: leg(false)?,
            flt: leg(true)?,
            side: swaps
                .iter()
                .map(|s| {
                    if matches!(s.side, Side::Receiver) {
                        1.
                    } else {
                        -1.
                    }
                })
                .collect(),
            notional: swaps.iter().map(|s| s.notional).collect(),
            n: swaps.len(),
        })
    }
}
pub(crate) fn term_flows(deck: &Deck, paths: &RatePaths, p: usize, t: usize) -> Vec<Vec<f64>> {
    crate::compute_span!("term_cashflows");
    let swap5: Vec<_> = (0..p)
        .flat_map(|i| {
            paths.swaps[(i * 4 + 1) * t..(i * 4 + 2) * t]
                .iter()
                .copied()
        })
        .collect();
    corporate(
        &[
            matrix(&paths.short, p, t),
            matrix(&swap5, p, t),
            matrix(&paths.df, p, t),
            vector(&deck.per_off),
            vector(&deck.pay_m),
            vector(&deck.pay_frac),
            vector(&deck.fix_m),
            vector(&deck.fix_w),
            vector(&deck.tau),
            vector(&deck.prin),
            vector(&deck.call_px),
            vector(&deck.put_px),
            vector(&deck.is_float),
            vector(&deck.cpn),
            vector(&deck.cap),
            vector(&deck.floor),
            vector(&deck.call_thr),
        ],
        false,
    )
}
pub(crate) fn term_price(
    deck: &Deck,
    flows: &[f64],
    spreads: &[f64],
    p: usize,
    solve: bool,
) -> Vec<Vec<f64>> {
    oas(
        &[
            vector(&deck.per_off),
            vector(&deck.t_pay),
            vector(flows),
            vector(spreads),
            vector(&[p as f64]),
            vector(&[1e-8]),
            vector(&[40.]),
            vector(&[-0.05]),
            vector(&[0.3]),
        ],
        solve,
    )
}
pub(crate) fn smear(deck: &Deck, flows: &[f64], horizon: usize, do_smear: bool) -> Vec<f64> {
    accrual(&[
        vector(&deck.per_off),
        vector(&deck.acc_m),
        vector(&deck.pay_m),
        vector(flows),
        vector(&[horizon as f64]),
        vector(&[f64::from(do_smear)]),
    ])
    .remove(0)
}
pub(crate) fn swap_values(
    deck: &HedgeDeck,
    paths: &RatePaths,
    p: usize,
    t: usize,
    horizon: usize,
) -> (Vec<f64>, Vec<f64>) {
    let fixed = term_flows(&deck.fix, paths, p, t);
    let floating = term_flows(&deck.flt, paths, p, t);
    let a = term_price(&deck.fix, &fixed[0], &vec![0.; deck.n], p, false).remove(0);
    let b = term_price(&deck.flt, &floating[0], &vec![0.; deck.n], p, false).remove(0);
    let fi: Vec<_> = fixed[1].iter().map(|x| x / p as f64).collect();
    let fl: Vec<_> = floating[1].iter().map(|x| x / p as f64).collect();
    let fa = smear(&deck.fix, &fi, horizon, true);
    let fb = smear(&deck.flt, &fl, horizon, true);
    (
        (0..deck.n).map(|i| deck.side[i] * (a[i] - b[i])).collect(),
        (0..deck.n * horizon)
            .map(|j| deck.side[j / horizon] * (fa[j] - fb[j]))
            .collect(),
    )
}
pub(crate) fn option_values(
    book: &[Swaption],
    paths: &RatePaths,
    p: usize,
    t: usize,
) -> Result<Vec<f64>, String> {
    let mut terms = Vec::with_capacity(book.len() * 5);
    for row in book {
        finite(&[row.notional, row.strike, row.tenor_y])?;
        let index = [2., 5., 10., 30.]
            .iter()
            .position(|&x| x == row.tenor_y)
            .ok_or("unsupported swaption tenor")?;
        if row.notional < 0. || row.expiry_m >= t {
            return Err("invalid swaption notional or expiry".into());
        }
        terms.extend([
            index as f64,
            row.expiry_m as f64,
            row.tenor_y * 2.,
            if matches!(row.side, Side::Payer) {
                1.
            } else {
                -1.
            },
            row.strike,
        ]);
    }
    Ok(swaption(&[
        tensor(&paths.swaps, [p, 4, t]),
        matrix(&paths.df, p, t),
        matrix(&terms, book.len(), 5),
        vector(&[p as f64]),
    ])
    .remove(0))
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeRequest {
    pub swaps: Vec<Swap>,
    pub swaptions: Option<Vec<Swaption>>,
    pub asof: i32,
    pub horizon: usize,
    pub market: MarketInput,
}
#[derive(Serialize)]
pub struct HedgeRisk {
    pub mtm: Vec<f64>,
    pub carry_y1: Vec<f64>,
    pub option_values: Option<Vec<f64>>,
    pub dv01: f64,
    pub krd: Vec<f64>,
    pub vega: Option<f64>,
    pub carry_monthly: Vec<f64>,
    pub mtm_total: f64,
}
impl HedgeRequest {
    pub fn run(&self) -> Result<HedgeRisk, String> {
        let c = &self.market.config;
        let n = self.swaps.len();
        let h = self.horizon;
        if h == 0 || h > c.months {
            return Err("invalid hedge horizon".into());
        }
        admit(&[n, h + 2], 128 * 1024 * 1024)?;
        let market = RateMarket::new(&self.market)?;
        let base = market.base()?;
        let mut out = HedgeRisk {
            mtm: vec![0.; n],
            carry_y1: vec![0.; n],
            option_values: None,
            dv01: 0.,
            krd: vec![0.; self.market.tenors.len()],
            vega: None,
            carry_monthly: vec![0.; h],
            mtm_total: 0.,
        };
        let mut scenarios = Vec::new();
        for j in 0..self.market.tenors.len() {
            let mut down = self.market.swap_rates.clone();
            let mut up = down.clone();
            down[j] -= c.curve_bump;
            up[j] += c.curve_bump;
            scenarios.push((down, up));
        }
        let vol_pair = if self.swaptions.is_some() {
            let mut down = self.market.vol_quotes.clone();
            let mut up = down.clone();
            for j in (2..down.len()).step_by(3) {
                down[j] -= c.vol_bump;
                up[j] += c.vol_bump;
            }
            Some((market.fit_quotes(&down)?, market.fit_quotes(&up)?))
        } else {
            None
        };
        let mut vega = 0.;
        for first in (0..n).step_by(256) {
            let end = (first + 256).min(n);
            let deck = HedgeDeck::new(&self.swaps[first..end], self.asof, c.months)?;
            let (mtm, carry) = swap_values(&deck, &base, c.paths, c.months, h);
            for i in 0..deck.n {
                out.mtm[first + i] = mtm[i] * deck.notional[i];
                out.mtm_total += out.mtm[first + i];
                out.carry_y1[first + i] =
                    carry[i * h..i * h + h.min(12)].iter().sum::<f64>() * deck.notional[i];
                for m in 0..h {
                    out.carry_monthly[m] += carry[i * h + m] * deck.notional[i];
                }
            }
            let value = |rates: &[f64], abcd: &[f64]| -> Result<f64, String> {
                let paths = market.paths(rates, abcd)?;
                let values = swap_values(&deck, &paths, c.paths, c.months, 1).0;
                Ok(values.iter().zip(&deck.notional).map(|(a, b)| a * b).sum())
            };
            for (j, (down, up)) in scenarios.iter().enumerate() {
                out.krd[j] += (value(down, &market.abcd)? - value(up, &market.abcd)?) / 2.;
            }
            if let Some((down, up)) = &vol_pair {
                vega += (value(&self.market.swap_rates, up)?
                    - value(&self.market.swap_rates, down)?)
                    / (2. * c.vol_bump)
                    * 0.01;
            }
        }
        if let Some(book) = &self.swaptions {
            let values = option_values(book, &base, c.paths, c.months)?;
            let dollar: Vec<_> = values
                .iter()
                .zip(book)
                .map(|(a, b)| a * b.notional)
                .collect();
            out.mtm_total += dollar.iter().sum::<f64>();
            out.option_values = Some(dollar);
            let value = |rates: &[f64], abcd: &[f64]| -> Result<f64, String> {
                let paths = market.paths(rates, abcd)?;
                Ok(option_values(book, &paths, c.paths, c.months)?
                    .iter()
                    .zip(book)
                    .map(|(a, b)| a * b.notional)
                    .sum())
            };
            for (j, (down, up)) in scenarios.iter().enumerate() {
                out.krd[j] += (value(down, &market.abcd)? - value(up, &market.abcd)?) / 2.;
            }
            let (down, up) = vol_pair.as_ref().unwrap();
            vega += (value(&self.market.swap_rates, up)? - value(&self.market.swap_rates, down)?)
                / (2. * c.vol_bump)
                * 0.01;
            out.vega = Some(vega);
        }
        out.dv01 = out.krd.iter().sum();
        for values in [&out.mtm, &out.carry_y1, &out.krd, &out.carry_monthly] {
            finite(values)?;
        }
        finite(&[out.dv01, out.mtm_total, vega])?;
        if let Some(values) = &out.option_values {
            finite(values)?;
        }
        Ok(out)
    }
}
