//! Raw corporate and CD lifecycle: dates -> cashflows -> fixed-OAS risk.
use crate::cache::{resolve, Key};
use crate::calibration::factor_loadings;
use crate::market::{bootstrap, MarketContext};
use crate::market_cache::fit_abcd;
use crate::mortgage_risk::RiskResult;
use crate::quant::{corporate, oas, View};
use crate::random::SharedDraws;
use crate::term_deck::{Deck, DeckRequest, Product};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RateConfig {
    pub paths: usize,
    pub months: usize,
    pub forwards: usize,
    pub factors: usize,
    pub dt: f64,
    pub tenor: f64,
    pub shift: f64,
    pub curve_bump: f64,
    pub vol_bump: f64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TermRiskRequest {
    pub deck: DeckRequest,
    pub config: RateConfig,
    pub tenors: Vec<f64>,
    pub swap_rates: Vec<f64>,
    pub vol_quotes: Vec<f64>,
    pub seed: Vec<u32>,
    #[serde(default)]
    pub fixed_oas: Vec<f64>,
    #[serde(default)]
    pub withdrawal_parameters: Vec<f64>,
}
struct TermPaths {
    df: Vec<f64>,
    short: Vec<f64>,
    swap5: Vec<f64>,
}
struct RetainedPaths {
    base: Arc<TermPaths>,
    scenarios: Vec<(Arc<TermPaths>, Arc<TermPaths>)>,
}
fn vector(v: &[f64]) -> View<'_> {
    View {
        v,
        shape: [v.len(), 1, 1],
    }
}
fn matrix(v: &[f64], p: usize, t: usize) -> View<'_> {
    View {
        v,
        shape: [p, t, 1],
    }
}
impl TermRiskRequest {
    pub fn run(&self) -> Result<RiskResult, String> {
        std::panic::catch_unwind(|| self.calculate())
            .map_err(|_| "native term lifecycle failed validation".to_string())?
    }
    fn validate(&self) -> Result<(), String> {
        let c = &self.config;
        let n = self.deck.contracts.len();
        if n == 0
            || c.paths == 0
            || c.months != self.deck.months
            || !(2..=4096).contains(&c.months)
            || c.dt != 1. / 12.
            || c.tenor != 0.25
            || c.curve_bump != 0.0001
            || !c.shift.is_finite()
            || !c.vol_bump.is_finite()
            || c.vol_bump <= 0.
            || self.tenors.len() != self.swap_rates.len()
            || self.tenors.is_empty()
            || self.vol_quotes.is_empty()
            || !self.vol_quotes.len().is_multiple_of(3)
            || (!self.fixed_oas.is_empty() && self.fixed_oas.len() != n)
            || self
                .fixed_oas
                .iter()
                .chain(&self.vol_quotes)
                .any(|x| !x.is_finite())
            || (self.deck.product == Product::Cd
                && (self.withdrawal_parameters.len() != 5
                    || self.withdrawal_parameters.iter().any(|x| !x.is_finite())))
        {
            return Err(
                "invalid term risk grid, quotes, fixed OAS or withdrawal assumptions".into(),
            );
        }
        let count = n
            .checked_mul(self.tenors.len() + self.vol_quotes.len() / 3 + 3)
            .ok_or("term risk result size overflow")?;
        if count > 128 * 1024 * 1024 {
            return Err("term risk outputs exceed 1 GiB admission".into());
        }
        Ok(())
    }
    fn paths(
        &self,
        rates: &[f64],
        parameters: &[f64],
        b: &[f64],
        draws: &SharedDraws,
    ) -> Result<Arc<TermPaths>, String> {
        let c = &self.config;
        let seed: Vec<_> = self.seed.iter().map(|x| *x as usize).collect();
        let key = Key::new(20)
            .integers(&seed)
            .integers(&draws.shape)
            .integers(&[c.forwards])
            .floats(&self.tenors)
            .floats(rates)
            .floats(parameters)
            .floats(b)
            .floats(&[c.dt, c.tenor, c.shift]);
        resolve(key, || {
            let context = MarketContext::new(
                &self.tenors,
                rates,
                parameters,
                b,
                &draws.rates,
                draws.shape,
                [c.dt, c.tenor, c.shift],
            )?;
            let out = context.rates()?;
            let swap5 = (0..c.paths)
                .flat_map(|p| {
                    out.swaps[(p * 4 + 1) * c.months..(p * 4 + 2) * c.months]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>();
            let paths = TermPaths {
                df: out.df,
                short: out.short,
                swap5,
            };
            let bytes = (paths.df.capacity() + paths.short.capacity() + paths.swap5.capacity()) * 8
                + std::mem::size_of_val(&paths);
            Ok((paths, bytes))
        })
    }
    fn cashflows(&self, deck: &Deck, paths: &TermPaths) -> Vec<f64> {
        let c = &self.config;
        let mut a = vec![matrix(&paths.short, c.paths, c.months)];
        if self.deck.product == Product::Corporate {
            a.extend([
                matrix(&paths.swap5, c.paths, c.months),
                matrix(&paths.df, c.paths, c.months),
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
            ]);
        } else {
            a.extend([
                matrix(&paths.df, c.paths, c.months),
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
                vector(&self.withdrawal_parameters),
            ]);
        }
        corporate(&a, self.deck.product == Product::Cd).remove(0)
    }
    fn price(
        &self,
        deck: &Deck,
        cashflows: &[f64],
        parameters: &[f64],
        solve: bool,
    ) -> Vec<Vec<f64>> {
        oas(
            &[
                vector(&deck.per_off),
                vector(&deck.t_pay),
                vector(cashflows),
                vector(parameters),
                vector(&[self.config.paths as f64]),
                vector(&[1e-8]),
                vector(&[40.]),
                vector(&[-0.05]),
                vector(&[0.3]),
            ],
            solve,
        )
    }
    fn calculate(&self) -> Result<RiskResult, String> {
        self.validate()?;
        let c = &self.config;
        let n = self.deck.contracts.len();
        let b = resolve(
            Key::new(5)
                .integers(&[c.forwards, c.factors])
                .floats(&[c.tenor, 0.1]),
            || {
                let v = factor_loadings(c.forwards, c.factors, c.tenor, 0.1)?;
                let bytes = v.capacity() * 8 + std::mem::size_of_val(&v);
                Ok((v, bytes))
            },
        )?;
        let dfs = resolve(
            Key::new(6)
                .floats(&self.tenors)
                .floats(&self.swap_rates)
                .integers(&[c.forwards])
                .floats(&[c.tenor]),
            || {
                let v = bootstrap(&self.tenors, &self.swap_rates, c.forwards, c.tenor)?;
                let bytes = v.capacity() * 8 + std::mem::size_of_val(&v);
                Ok((v, bytes))
            },
        )?;
        let forwards: Vec<_> = dfs
            .windows(2)
            .map(|v| (v[0] / v[1] - 1.) / c.tenor)
            .collect();
        let abcd = fit_abcd(
            &self.vol_quotes,
            &forwards,
            &dfs,
            &b,
            &[0.05, 0.1, 0.5, 0.12],
            [c.tenor, c.shift],
        )?
        .x;
        // Each pair keeps one calibrated market description, not instrument tensors.
        let mut scenarios = Vec::new();
        for j in 0..self.tenors.len() {
            let mut down = self.swap_rates.clone();
            down[j] -= c.curve_bump;
            let mut up = self.swap_rates.clone();
            up[j] += c.curve_bump;
            scenarios.push(((down, abcd.clone()), (up, abcd.clone())));
        }
        for j in 0..self.vol_quotes.len() / 3 {
            let mut quotes = self.vol_quotes.clone();
            quotes[3 * j + 2] -= c.vol_bump;
            let down = fit_abcd(&quotes, &forwards, &dfs, &b, &abcd, [c.tenor, c.shift])?.x;
            quotes[3 * j + 2] = self.vol_quotes[3 * j + 2] + c.vol_bump;
            let up = fit_abcd(&quotes, &forwards, &dfs, &b, &abcd, [c.tenor, c.shift])?.x;
            scenarios.push((
                (self.swap_rates.clone(), down),
                (self.swap_rates.clone(), up),
            ));
        }
        let mut draws = SharedDraws::new(&self.seed, [c.paths, c.months, c.factors])?;
        draws.spread = Vec::new();
        draws.hpi = Vec::new();
        let mut out = RiskResult {
            oas: vec![0.; n],
            price: vec![0.; n],
            dv01: vec![0.; n],
            sensitivities: vec![0.; n * scenarios.len()],
        };
        // Resolve immutable small market grids once per request, not once per
        // 256-contract chunk. Large grids retain the existing bounded cache path.
        let retain_bytes = c
            .paths
            .checked_mul(c.months)
            .and_then(|v| v.checked_mul(3 * 8))
            .and_then(|v| v.checked_mul(1 + 2 * scenarios.len()))
            .unwrap_or(usize::MAX);
        let retained = if n > 256 && retain_bytes <= 32 * 1024 * 1024 {
            let base = self.paths(&self.swap_rates, &abcd, &b, &draws)?;
            let paths = scenarios
                .iter()
                .map(|((dr, dp), (ur, up))| {
                    Ok((
                        self.paths(dr, dp, &b, &draws)?,
                        self.paths(ur, up, &b, &draws)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Some(RetainedPaths {
                base,
                scenarios: paths,
            })
        } else {
            None
        };
        for first in (0..n).step_by(256) {
            portfolio_compute_control::checkpoint()?;
            let end = (first + 256).min(n);
            let deck = self.deck.build_rows(&self.deck.contracts[first..end])?;
            let base = match &retained {
                Some(paths) => paths.base.clone(),
                None => self.paths(&self.swap_rates, &abcd, &b, &draws)?,
            };
            let cashflows = self.cashflows(&deck, &base);
            drop(base);
            let solve = self.fixed_oas.is_empty();
            let mut priced = self.price(
                &deck,
                &cashflows,
                if solve {
                    &deck.tgt
                } else {
                    &self.fixed_oas[first..end]
                },
                solve,
            );
            if solve {
                out.oas[first..end].copy_from_slice(&priced.remove(0));
            } else {
                out.oas[first..end].copy_from_slice(&self.fixed_oas[first..end]);
            }
            out.price[first..end].copy_from_slice(&priced.remove(0));
            let pv = |rates: &[f64],
                      params: &[f64],
                      retained: Option<&Arc<TermPaths>>|
             -> Result<Vec<f64>, String> {
                let paths = match retained {
                    Some(paths) => paths.clone(),
                    None => self.paths(rates, params, &b, &draws)?,
                };
                Ok(self
                    .price(
                        &deck,
                        &self.cashflows(&deck, &paths),
                        &out.oas[first..end],
                        false,
                    )
                    .remove(0))
            };
            for (j, ((down_rates, down_params), (up_rates, up_params))) in
                scenarios.iter().enumerate()
            {
                let down = pv(
                    down_rates,
                    down_params,
                    retained.as_ref().map(|r| &r.scenarios[j].0),
                )?;
                let up = pv(
                    up_rates,
                    up_params,
                    retained.as_ref().map(|r| &r.scenarios[j].1),
                )?;
                for i in first..end {
                    let local = i - first;
                    let value = if j < self.tenors.len() {
                        deck.notional[local] * (down[local] - up[local]) / 2.
                    } else {
                        deck.notional[local] * (up[local] - down[local]) / (2. * c.vol_bump) * 0.01
                    };
                    out.sensitivities[j * n + i] = value;
                    if j < self.tenors.len() {
                        out.dv01[i] += value;
                    }
                }
            }
        }
        if out
            .oas
            .iter()
            .chain(&out.price)
            .chain(&out.dv01)
            .chain(&out.sensitivities)
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite term risk result".into());
        }
        Ok(out)
    }
}
