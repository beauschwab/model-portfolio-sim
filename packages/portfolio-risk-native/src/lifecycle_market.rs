//! Shared raw market calibration and scenario ownership for product lifecycles.
use crate::cache::{resolve, Key};
use crate::calibration::factor_loadings;
use crate::market::{bootstrap, MarketContext, RatePaths};
use crate::market_cache::fit_abcd;
use crate::quant::{oas, View};
use crate::random::SharedDraws;
use crate::term_risk::RateConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MarketInput {
    pub config: RateConfig,
    pub tenors: Vec<f64>,
    pub swap_rates: Vec<f64>,
    pub vol_quotes: Vec<f64>,
    pub seed: Vec<u32>,
}
impl MarketInput {
    pub(crate) fn validate_mortgage(
        &self,
        book: &crate::mortgage_input::OwnedMortgage,
    ) -> Result<(), String> {
        let (a, b) = (&self.config, &book.config);
        if !book.book.len().is_multiple_of(13)
            || a.months != b.months
            || a.factors != b.factors
            || a.forwards != b.forwards
            || a.dt != b.dt
            || a.tenor != b.tenor
            || a.shift != b.shift
            || a.curve_bump != b.curve_bump
            || a.vol_bump != b.vol_bump
            || self.seed != book.seed
            || self.tenors != book.tenors
            || self.swap_rates != book.swap_rates
            || self.vol_quotes != book.vol_quotes
        {
            return Err("mortgage market or simulation grid mismatch".into());
        }
        Ok(())
    }
}
pub struct RateMarket<'a> {
    pub input: &'a MarketInput,
    pub b: Arc<Vec<f64>>,
    pub dfs: Arc<Vec<f64>>,
    pub forwards: Vec<f64>,
    pub abcd: Vec<f64>,
    pub draws: SharedDraws,
}
pub struct Scenario {
    pub rates: Vec<f64>,
    pub abcd: Vec<f64>,
}
pub(crate) fn vector(v: &[f64]) -> View<'_> {
    View {
        v,
        shape: [v.len(), 1, 1],
    }
}
pub(crate) fn matrix(v: &[f64], rows: usize, cols: usize) -> View<'_> {
    View {
        v,
        shape: [rows, cols, 1],
    }
}
pub(crate) fn tensor(v: &[f64], shape: [usize; 3]) -> View<'_> {
    View { v, shape }
}
pub fn finite(v: &[f64]) -> Result<(), String> {
    if v.iter().any(|x| !x.is_finite()) {
        Err("nonfinite lifecycle value".into())
    } else {
        Ok(())
    }
}
pub fn admit(dimensions: &[usize], maximum: usize) -> Result<usize, String> {
    let count = dimensions
        .iter()
        .try_fold(1usize, |n, d| n.checked_mul(*d))
        .ok_or("lifecycle size overflow")?;
    if count > maximum {
        Err("lifecycle allocation exceeds admission budget".into())
    } else {
        Ok(count)
    }
}
impl<'a> RateMarket<'a> {
    pub fn new(input: &'a MarketInput) -> Result<Self, String> {
        crate::compute_span!("market_calibration");
        let c = &input.config;
        if c.paths == 0
            || !(2..=4096).contains(&c.months)
            || c.dt != 1. / 12.
            || c.tenor != 0.25
            || c.curve_bump != 0.0001
            || !c.shift.is_finite()
            || !c.vol_bump.is_finite()
            || c.vol_bump <= 0.
            || input.tenors.len() != input.swap_rates.len()
            || input.tenors.is_empty()
            || input.vol_quotes.is_empty()
            || !input.vol_quotes.len().is_multiple_of(3)
        {
            return Err("invalid lifecycle market or simulation grid".into());
        }
        finite(&input.vol_quotes)?;
        let b = resolve(
            Key::new(5)
                .integers(&[c.forwards, c.factors])
                .floats(&[c.tenor, 0.1]),
            || {
                let x = factor_loadings(c.forwards, c.factors, c.tenor, 0.1)?;
                let bytes = x.capacity() * 8 + std::mem::size_of_val(&x);
                Ok((x, bytes))
            },
        )?;
        let dfs = resolve(
            Key::new(6)
                .floats(&input.tenors)
                .floats(&input.swap_rates)
                .integers(&[c.forwards])
                .floats(&[c.tenor]),
            || {
                let x = bootstrap(&input.tenors, &input.swap_rates, c.forwards, c.tenor)?;
                let bytes = x.capacity() * 8 + std::mem::size_of_val(&x);
                Ok((x, bytes))
            },
        )?;
        let forwards: Vec<_> = dfs
            .windows(2)
            .map(|v| (v[0] / v[1] - 1.) / c.tenor)
            .collect();
        let abcd = fit_abcd(
            &input.vol_quotes,
            &forwards,
            &dfs,
            &b,
            &[0.05, 0.1, 0.5, 0.12],
            [c.tenor, c.shift],
        )?
        .x;
        let mut draws = SharedDraws::new(&input.seed, [c.paths, c.months, c.factors])?;
        draws.spread = Vec::new();
        draws.hpi = Vec::new();
        Ok(Self {
            input,
            b,
            dfs,
            forwards,
            abcd,
            draws,
        })
    }
    pub fn fit_quotes(&self, quotes: &[f64]) -> Result<Vec<f64>, String> {
        let c = &self.input.config;
        Ok(fit_abcd(
            quotes,
            &self.forwards,
            &self.dfs,
            &self.b,
            &self.abcd,
            [c.tenor, c.shift],
        )?
        .x)
    }
    pub fn paths(&self, rates: &[f64], abcd: &[f64]) -> Result<Arc<RatePaths>, String> {
        crate::compute_span!("rate_paths");
        let i = self.input;
        let c = &i.config;
        let seed: Vec<_> = i.seed.iter().map(|&x| x as usize).collect();
        let key = Key::new(40)
            .integers(&seed)
            .integers(&self.draws.shape)
            .floats(&i.tenors)
            .floats(rates)
            .floats(abcd)
            .floats(&self.b)
            .floats(&[c.dt, c.tenor, c.shift]);
        resolve(key, || {
            let paths = MarketContext::new(
                &i.tenors,
                rates,
                abcd,
                &self.b,
                &self.draws.rates,
                self.draws.shape,
                [c.dt, c.tenor, c.shift],
            )?
            .rates()?;
            let bytes = (paths.df.capacity() + paths.short.capacity() + paths.swaps.capacity()) * 8
                + std::mem::size_of_val(&paths);
            Ok((paths, bytes))
        })
    }
    pub fn base(&self) -> Result<Arc<RatePaths>, String> {
        self.paths(&self.input.swap_rates, &self.abcd)
    }
    pub fn scenarios(&self) -> Result<Vec<(Scenario, Scenario)>, String> {
        let i = self.input;
        let mut out = Vec::new();
        for j in 0..i.tenors.len() {
            let mut down = i.swap_rates.clone();
            down[j] -= i.config.curve_bump;
            let mut up = i.swap_rates.clone();
            up[j] += i.config.curve_bump;
            out.push((
                Scenario {
                    rates: down,
                    abcd: self.abcd.clone(),
                },
                Scenario {
                    rates: up,
                    abcd: self.abcd.clone(),
                },
            ));
        }
        for j in 0..i.vol_quotes.len() / 3 {
            let mut quotes = i.vol_quotes.clone();
            quotes[j * 3 + 2] -= i.config.vol_bump;
            let down = self.fit_quotes(&quotes)?;
            quotes[j * 3 + 2] = i.vol_quotes[j * 3 + 2] + i.config.vol_bump;
            let up = self.fit_quotes(&quotes)?;
            out.push((
                Scenario {
                    rates: i.swap_rates.clone(),
                    abcd: down,
                },
                Scenario {
                    rates: i.swap_rates.clone(),
                    abcd: up,
                },
            ));
        }
        Ok(out)
    }
}
pub(crate) fn monthly_price(
    a: &[f64],
    n: usize,
    t: usize,
    paths: usize,
    parameters: &[f64],
    solve: bool,
    lower: f64,
) -> Vec<Vec<f64>> {
    let off: Vec<_> = (0..=n).map(|i| (i * t) as f64).collect();
    let times: Vec<_> = (0..n * t).map(|j| (j % t + 1) as f64 / 12.).collect();
    oas(
        &[
            vector(&off),
            vector(&times),
            vector(a),
            vector(parameters),
            vector(&[paths as f64]),
            vector(&[1e-8]),
            vector(&[40.]),
            vector(&[lower]),
            vector(&[0.3]),
        ],
        solve,
    )
}
