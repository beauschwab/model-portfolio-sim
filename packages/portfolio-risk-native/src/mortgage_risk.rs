//! Mortgage spot-risk lifecycle: raw numeric books/histories -> calibrated fixed
//! OAS -> shared-draw curve/volatility revaluation -> dollar KRD, DV01 and vega.
//! Only one scenario's paths and a 256-position base cashflow chunk are retained.
use crate::cache::{resolve, Key};
use crate::calibration::{factor_loadings, fit_current_coupon, fit_ps_spread, CurrentCouponFit};
use crate::market::{
    bootstrap, MarketContext, MortgageInputs, MortgagePaths, ParallelShock, RatePaths,
};
use crate::market_cache::fit_abcd;
use crate::quant::{mortgage, mortgage_batch, oas, View};
use crate::random::SharedDraws;
use std::sync::Arc;

fn vector(v: &[f64]) -> View<'_> {
    View {
        v,
        shape: [v.len(), 1, 1],
    }
}
fn matrix(v: &[f64], rows: usize, cols: usize) -> View<'_> {
    View {
        v,
        shape: [rows, cols, 1],
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    pub base_paths: usize,
    pub sensitivity_paths: usize,
    pub months: usize,
    pub forwards: usize,
    pub factors: usize,
    pub dt: f64,
    pub tenor: f64,
    pub shift: f64,
    pub float32_paths: bool,
    pub curve_bump: f64,
    pub vol_bump: f64,
    pub hpi: [f64; 3],
    pub incentive_lag: usize,
    pub ps_spot: f64,
    pub rational_sigmoid: bool,
}

/// Already-tabulated model data is immutable input, not callbacks or calculated
/// instrument coefficients. Scalar spline evaluation/normalization stays native.
#[derive(Clone)]
pub struct PrepayData<'a> {
    pub month_of_year: &'a [f64],
    pub seasonality: &'a [f64],
    pub parameters: &'a [f64],
    pub ltv_knots: &'a [f64],
    pub ltv_coefficients: &'a [f64],
    pub smm_table: &'a [f64],
    pub smm_scale: f64,
    pub burnout_table: &'a [f64],
    pub burnout_scale: f64,
    pub cc_vol_points: &'a [f64],
    pub fico_x: &'a [f64],
    pub fico_y: &'a [f64],
    pub size_x: &'a [f64],
    pub size_y: &'a [f64],
    pub state_multipliers: &'a [f64],
    pub channel_multipliers: &'a [f64],
}

/// Book rows: wac, net_coupon, wam, age, oltv, factor, fico, loan_size,
/// state and channel indices into the supplied multiplier tables (other=0),
/// price per 100, current face, payment-delay days. Optional original HPI is
/// supplied separately; absence invokes the existing age/HPI-growth convention.
/// Optional per-pool prepay speed multipliers scale turnover plus refi before the
/// CPR cap; absence means 1 for every pool.
#[derive(Clone)]
pub struct MortgageRiskRequest<'a> {
    pub tenors: &'a [f64],
    pub swap_rates: &'a [f64],
    pub vol_quotes: &'a [f64],
    pub cc_history: &'a [f64],
    pub ps_history: &'a [f64],
    pub book: &'a [f64],
    pub original_hpi: &'a [f64],
    pub prepay_multiplier: &'a [f64],
    pub seed: &'a [u32],
    pub fixed_oas: &'a [f64],
    pub config: RiskConfig,
    pub prepay: PrepayData<'a>,
}
#[derive(serde::Serialize)]
pub struct RiskResult {
    pub oas: Vec<f64>,
    pub price: Vec<f64>,
    pub dv01: Vec<f64>,
    /// Row-major risk column x instrument, KRD followed by vega.
    pub sensitivities: Vec<f64>,
}

struct Prepared {
    cc: CurrentCouponFit,
    ps: [f64; 3],
    b: Vec<f64>,
    dfs: Vec<f64>,
    forwards: Vec<f64>,
    abcd: Vec<f64>,
    sec: Vec<Vec<f64>>,
    faces: Vec<f64>,
    delays: Vec<f64>,
    spreads: Vec<f64>,
    prices: Vec<f64>,
}

#[derive(serde::Serialize)]
pub struct StressResult {
    pub oas: Vec<f64>,
    /// Horizon x instrument, in dollars and price per 100 respectively.
    pub base_value: Vec<f64>,
    pub base_price: Vec<f64>,
    /// Shock x horizon x instrument, in dollars.
    pub shock_value: Vec<f64>,
    pub pnl: Vec<f64>,
    /// Shock x horizon aggregates, in dollars.
    pub aggregate_base: Vec<f64>,
    pub aggregate_pnl: Vec<f64>,
    /// Horizon totals, empty unless both -100bp and +100bp were requested.
    pub forward_dv01: Vec<f64>,
}

struct NaturalSpline<'a> {
    x: &'a [f64],
    y: &'a [f64],
    second: Vec<f64>,
}
impl<'a> NaturalSpline<'a> {
    fn new(x: &'a [f64], y: &'a [f64]) -> Result<Self, String> {
        let n = x.len();
        if n < 2
            || n != y.len()
            || x.iter().chain(y).any(|v| !v.is_finite())
            || x.windows(2).any(|v| v[0] >= v[1])
        {
            return Err("invalid natural spline anchors".into());
        }
        let mut upper = vec![0.; n];
        let mut second = vec![0.; n];
        for i in 1..n - 1 {
            let left = x[i] - x[i - 1];
            let right = x[i + 1] - x[i];
            let denom = 2. * (left + right) - left * upper[i - 1];
            upper[i] = right / denom;
            second[i] = (6. * ((y[i + 1] - y[i]) / right - (y[i] - y[i - 1]) / left)
                - left * second[i - 1])
                / denom;
        }
        for i in (1..n - 1).rev() {
            second[i] -= upper[i] * second[i + 1];
        }
        Ok(Self { x, y, second })
    }
    fn value(&self, value: f64) -> f64 {
        let x = value.clamp(self.x[0], self.x[self.x.len() - 1]);
        let i = self
            .x
            .partition_point(|&v| v <= x)
            .saturating_sub(1)
            .min(self.x.len() - 2);
        let h = self.x[i + 1] - self.x[i];
        let dx = x - self.x[i];
        let a = (self.second[i + 1] - self.second[i]) / (6. * h);
        let b = self.second[i] / 2.;
        let c =
            (self.y[i + 1] - self.y[i]) / h - h * (2. * self.second[i] + self.second[i + 1]) / 6.;
        ((a * dx + b) * dx + c) * dx + self.y[i]
    }
}

pub(crate) struct ParallelValues {
    pub base: Vec<f64>,
    pub down: Vec<f64>,
    pub up: Vec<f64>,
}

impl MortgageRiskRequest<'_> {
    /// Incremental graph batch: the request anchors model calibration while the
    /// supplied rates select one CRN scenario. No OAS or book target is consumed.
    pub(crate) fn graph_flows(
        &self,
        rates: &[f64],
        paths_count: usize,
    ) -> Result<Vec<Vec<f64>>, String> {
        let prepared = self.prepare(false, false)?;
        let c = &self.config;
        let draws = SharedDraws::new(self.seed, [paths_count, c.months, c.factors])?;
        let paths = self.paths(
            rates,
            &prepared.abcd,
            &prepared.b,
            &draws,
            &prepared.cc,
            prepared.ps,
        )?;
        let zero = vec![0.; prepared.faces.len()];
        let rational = [f64::from(c.rational_sigmoid)];
        let mut args = self.kernel_inputs(&paths, &prepared.sec, paths_count);
        args.extend([vector(&zero), vector(&[]), vector(&[0.]), vector(&rational)]);
        let mut values = mortgage(&args, false);
        Ok(vec![
            std::mem::take(&mut values[0]),
            std::mem::take(&mut values[5]),
            std::mem::take(&mut values[6]),
        ])
    }
    pub(crate) fn parallel_values(&self, bump_bp: f64) -> Result<ParallelValues, String> {
        let prepared = self.prepare(true, true)?;
        let c = &self.config;
        let p = c.sensitivity_paths;
        let draws = SharedDraws::new(self.seed, [p, c.months, c.factors])?;
        let scenarios = vec![0.; p];
        let rational = [f64::from(c.rational_sigmoid)];
        let price = |bump: f64| -> Result<Vec<f64>, String> {
            let rates: Vec<_> = self.swap_rates.iter().map(|r| r + bump * 1e-4).collect();
            let paths = self.paths(
                &rates,
                &prepared.abcd,
                &prepared.b,
                &draws,
                &prepared.cc,
                prepared.ps,
            )?;
            let mut args = self.kernel_inputs(&paths, &prepared.sec, p);
            args.insert(4, vector(&scenarios));
            args.insert(5, vector(&[1.]));
            args.extend([
                vector(&prepared.spreads),
                vector(&prepared.delays),
                vector(&rational),
            ]);
            Ok(mortgage_batch(&args)
                .remove(0)
                .iter()
                .map(|v| v / p as f64)
                .collect())
        };
        Ok(ParallelValues {
            base: prepared.prices.clone(),
            down: price(-bump_bp)?,
            up: price(bump_bp)?,
        })
    }
    pub(crate) fn income_chunks(
        &self,
        draws: &SharedDraws,
        forecast: Option<&crate::forecast_lifecycle::Forecast>,
        mut consume: impl FnMut(usize, usize, &[f64], &[f64], &[f64], &[f64]) -> Result<(), String>,
    ) -> Result<(), String> {
        let prepared = self.prepare(false, false)?;
        let c = &self.config;
        let p = draws.shape[0];
        let t = c.months;
        if draws.shape != [p, t, c.factors] {
            return Err("mortgage income draw grid mismatch".into());
        }
        let context = MarketContext::new(
            self.tenors,
            self.swap_rates,
            &prepared.abcd,
            &prepared.b,
            &draws.rates,
            draws.shape,
            [c.dt, c.tenor, c.shift],
        )?;
        let mut paths = context.mortgage(&MortgageInputs {
            vol_points: self.prepay.cc_vol_points,
            cc_beta: &prepared.cc.beta,
            cc_lambda: prepared.cc.lambda,
            ps: [prepared.ps[0], prepared.ps[1], prepared.ps[2], c.ps_spot],
            eps_ps: &draws.spread,
            eps_h: &draws.hpi,
            hpi: c.hpi,
            incentive_lag: c.incentive_lag,
        })?;
        if c.float32_paths {
            for x in paths
                .rates
                .df
                .iter_mut()
                .chain(&mut paths.rates.short)
                .chain(&mut paths.mtg)
                .chain(&mut paths.hpi)
                .chain(&mut paths.yoy)
            {
                *x = (*x as f32) as f64;
            }
        }
        if let Some(plan) = forecast {
            paths = plan.mortgage(&paths, p, t, c.dt, c.shift, c.incentive_lag)?;
        }
        for first in (0..prepared.faces.len()).step_by(256) {
            portfolio_compute_control::checkpoint()?;
            let end = (first + 256).min(prepared.faces.len());
            let n = end - first;
            let sec: Vec<_> = prepared
                .sec
                .iter()
                .map(|row| row[first..end].to_vec())
                .collect();
            let zero = vec![0.; n];
            let rational = [f64::from(c.rational_sigmoid)];
            let mut inputs = self.kernel_inputs(&paths, &sec, p);
            inputs.extend([vector(&zero), vector(&[]), vector(&[0.]), vector(&rational)]);
            let mut flows = mortgage(&inputs, false);
            for j in [5, 6] {
                for v in &mut flows[j] {
                    *v /= p as f64;
                }
            }
            let prices: Vec<_> = self.book[first * 13..end * 13]
                .chunks_exact(13)
                .map(|r| r[10] / 100.)
                .collect();
            consume(
                first,
                end,
                &flows[5],
                &flows[6],
                &prices,
                &prepared.faces[first..end],
            )?;
        }
        Ok(())
    }
    /// No Python callbacks. Failed stages publish no partial risk result.
    pub fn run(&self) -> Result<RiskResult, String> {
        std::panic::catch_unwind(|| self.calculate())
            .map_err(|_| "native mortgage lifecycle failed validation".to_string())?
    }
    fn validate(&self) -> Result<(), String> {
        let c = &self.config;
        let p = &self.prepay;
        let n = self.book.len() / 13;
        let finite = self
            .tenors
            .iter()
            .chain(self.swap_rates)
            .chain(self.vol_quotes)
            .chain(self.book)
            .chain(self.original_hpi)
            .chain(self.prepay_multiplier)
            .chain(self.fixed_oas)
            .chain(p.month_of_year)
            .chain(p.seasonality)
            .chain(p.parameters)
            .chain(p.ltv_knots)
            .chain(p.ltv_coefficients)
            .chain(p.smm_table)
            .chain(p.burnout_table)
            .chain(p.cc_vol_points)
            .chain(p.state_multipliers)
            .chain(p.channel_multipliers)
            .all(|v| v.is_finite());
        if !finite
            || n == 0
            || !self.book.len().is_multiple_of(13)
            || self.vol_quotes.is_empty()
            || !self.vol_quotes.len().is_multiple_of(3)
            || (!self.fixed_oas.is_empty() && self.fixed_oas.len() != n)
            || (!self.original_hpi.is_empty() && self.original_hpi.len() != n)
            || self.original_hpi.iter().any(|v| *v <= 0.)
            || (!self.prepay_multiplier.is_empty() && self.prepay_multiplier.len() != n)
            || self.prepay_multiplier.iter().any(|v| *v < 0.)
            || c.months == 0
            || c.months > 4096
            || c.dt != 1. / 12.
            || c.tenor != 0.25
            || c.curve_bump != 0.0001
            || !c.vol_bump.is_finite()
            || c.vol_bump <= 0.
            || c.base_paths == 0
            || c.sensitivity_paths == 0
            || p.month_of_year.len() != c.months
            || p.seasonality.len() != 12
            || p.parameters.len() != 9
            || p.state_multipliers.is_empty()
            || p.channel_multipliers.is_empty()
            || p.month_of_year
                .iter()
                .any(|&v| !(0. ..12.).contains(&v) || v.fract() != 0.)
            || p.ltv_knots.len() < 2
            || p.ltv_coefficients.len() != 4 * (p.ltv_knots.len() - 1)
            || p.ltv_knots.windows(2).any(|v| v[0] >= v[1])
            || p.smm_table.len() < 2
            || p.burnout_table.len() < 2
            || !p.smm_scale.is_finite()
            || p.smm_scale <= 0.
            || !p.burnout_scale.is_finite()
            || p.burnout_scale <= 0.
        {
            return Err("invalid mortgage risk inputs or model grid".into());
        }
        for r in self.book.chunks_exact(13) {
            if r[0] <= 0.
                || r[2] <= 0.
                || r[2] > 4096.
                || r[3] < 0.
                || r[4] < 0.
                || r[5] <= 0.
                || r[10] <= 0.
                || r[11] < 0.
                || r[12] < 0.
                || !(0. ..p.state_multipliers.len() as f64).contains(&r[8])
                || r[8].fract() != 0.
                || !(0. ..p.channel_multipliers.len() as f64).contains(&r[9])
                || r[9].fract() != 0.
            {
                return Err("invalid mortgage book row".into());
            }
        }
        Ok(())
    }
    fn paths(
        &self,
        rates: &[f64],
        abcd: &[f64],
        loadings: &[f64],
        draws: &SharedDraws,
        cc: &CurrentCouponFit,
        ps: [f64; 3],
    ) -> Result<Arc<MortgagePaths>, String> {
        let c = &self.config;
        let seed: Vec<_> = self.seed.iter().map(|v| *v as usize).collect();
        let key = Key::new(2)
            .integers(&seed)
            .integers(&draws.shape)
            .integers(&[c.forwards, c.incentive_lag, usize::from(c.float32_paths)])
            .floats(self.tenors)
            .floats(rates)
            .floats(abcd)
            .floats(loadings)
            .floats(self.prepay.cc_vol_points)
            .floats(&cc.beta)
            .floats(&ps)
            .floats(&[c.dt, c.tenor, c.shift, cc.lambda, c.ps_spot])
            .floats(&c.hpi);
        resolve(key, || {
            let context = MarketContext::new(
                self.tenors,
                rates,
                abcd,
                loadings,
                &draws.rates,
                draws.shape,
                [c.dt, c.tenor, c.shift],
            )?;
            let mut paths = context.mortgage(&MortgageInputs {
                vol_points: self.prepay.cc_vol_points,
                cc_beta: &cc.beta,
                cc_lambda: cc.lambda,
                ps: [ps[0], ps[1], ps[2], c.ps_spot],
                eps_ps: &draws.spread,
                eps_h: &draws.hpi,
                hpi: c.hpi,
                incentive_lag: c.incentive_lag,
            })?;
            if c.float32_paths {
                for v in paths
                    .rates
                    .df
                    .iter_mut()
                    .chain(&mut paths.mtg)
                    .chain(&mut paths.hpi)
                    .chain(&mut paths.yoy)
                {
                    *v = (*v as f32) as f64;
                }
            }
            // Not needed by this spot-risk lifecycle after behavior paths are built.
            paths.rates.swaps = Vec::new();
            paths.rates.short = Vec::new();
            let bytes = (paths.rates.df.capacity()
                + paths.mtg.capacity()
                + paths.hpi.capacity()
                + paths.yoy.capacity())
                * 8
                + std::mem::size_of_val(&paths);
            Ok((paths, bytes))
        })
    }
    fn kernel_inputs<'a>(
        &'a self,
        paths: &'a MortgagePaths,
        sec: &'a [Vec<f64>],
        paths_count: usize,
    ) -> Vec<View<'a>> {
        let p = &self.prepay;
        let mut a = vec![
            matrix(&paths.mtg, paths_count, self.config.months),
            matrix(&paths.hpi, paths_count, self.config.months),
            matrix(&paths.yoy, paths_count, self.config.months),
            matrix(&paths.rates.df, paths_count, self.config.months),
            vector(p.month_of_year),
            vector(p.seasonality),
            vector(p.parameters),
            vector(p.ltv_knots),
            vector(p.ltv_coefficients),
            vector(p.smm_table),
        ];
        // Scalar scales are borrowed from this request, not temporary arrays.
        a.push(vector(std::slice::from_ref(&p.smm_scale)));
        a.push(vector(p.burnout_table));
        a.push(vector(std::slice::from_ref(&p.burnout_scale)));
        a.extend(sec.iter().map(|v| vector(v)));
        a
    }
    fn prepare(&self, need_prices: bool, need_spreads: bool) -> Result<Prepared, String> {
        crate::compute_span!("mortgage_prepare");
        self.validate()?;
        let c = &self.config;
        let n = self.book.len() / 13;
        let cc = (*resolve(Key::new(3).floats(self.cc_history), || {
            let fit = fit_current_coupon(self.cc_history)?;
            let bytes = fit.beta.capacity() * 8 + std::mem::size_of_val(&fit);
            Ok((fit, bytes))
        })?)
        .clone();
        let ps = *resolve(Key::new(4).floats(self.ps_history).floats(&[c.dt]), || {
            Ok((fit_ps_spread(self.ps_history, c.dt)?, 24))
        })?;
        let b = (*resolve(
            Key::new(5)
                .integers(&[c.forwards, c.factors])
                .floats(&[c.tenor, 0.1]),
            || {
                let value = factor_loadings(c.forwards, c.factors, c.tenor, 0.1)?;
                let bytes = value.capacity() * 8 + std::mem::size_of_val(&value);
                Ok((value, bytes))
            },
        )?)
        .clone();
        let dfs = (*resolve(
            Key::new(6)
                .floats(self.tenors)
                .floats(self.swap_rates)
                .integers(&[c.forwards])
                .floats(&[c.tenor]),
            || {
                let value = bootstrap(self.tenors, self.swap_rates, c.forwards, c.tenor)?;
                let bytes = value.capacity() * 8 + std::mem::size_of_val(&value);
                Ok((value, bytes))
            },
        )?)
        .clone();
        let forwards: Vec<_> = dfs
            .windows(2)
            .map(|v| (v[0] / v[1] - 1.) / c.tenor)
            .collect();
        let abcd = fit_abcd(
            self.vol_quotes,
            &forwards,
            &dfs,
            &b,
            &[0.05, 0.1, 0.5, 0.12],
            [c.tenor, c.shift],
        )?
        .x;
        let fico = NaturalSpline::new(self.prepay.fico_x, self.prepay.fico_y)?;
        let size = NaturalSpline::new(self.prepay.size_x, self.prepay.size_y)?;
        let mut sec: Vec<Vec<f64>> = (0..9).map(|_| Vec::with_capacity(n)).collect();
        let (mut targets, mut faces, mut delays) = (
            Vec::with_capacity(n),
            Vec::with_capacity(n),
            Vec::with_capacity(n),
        );
        for (i, r) in self.book.chunks_exact(13).enumerate() {
            for j in 0..6 {
                sec[j].push(r[j]);
            }
            sec[6].push(if self.original_hpi.is_empty() {
                (1. + c.hpi[0]).powf(r[3] / 12.)
            } else {
                self.original_hpi[i]
            });
            sec[7].push(
                fico.value(r[6])
                    * size.value(r[7])
                    * self.prepay.state_multipliers[r[8] as usize]
                    * self.prepay.channel_multipliers[r[9] as usize],
            );
            sec[8].push(self.prepay_multiplier.get(i).copied().unwrap_or(1.));
            targets.push(r[10] / 100.);
            faces.push(r[11]);
            delays.push(r[12] / 365.);
        }
        if !need_prices && (!need_spreads || !self.fixed_oas.is_empty()) {
            return Ok(Prepared {
                cc,
                ps,
                b,
                dfs,
                forwards,
                abcd,
                sec,
                faces,
                delays,
                spreads: self.fixed_oas.to_vec(),
                prices: Vec::new(),
            });
        }
        let base_draws = SharedDraws::new(self.seed, [c.base_paths, c.months, c.factors])?;
        let base = self.paths(self.swap_rates, &abcd, &b, &base_draws, &cc, ps)?;
        let (mut spreads, mut prices) = (Vec::with_capacity(n), Vec::with_capacity(n));
        let rational = [f64::from(c.rational_sigmoid)];
        for first in (0..n).step_by(256) {
            portfolio_compute_control::checkpoint()?;
            let end = (first + 256).min(n);
            let width = end - first;
            let chunk: Vec<_> = sec.iter().map(|v| v[first..end].to_vec()).collect();
            let zeros = vec![0.; width];
            let mut inputs = self.kernel_inputs(&base, &chunk, c.base_paths);
            inputs.extend([
                vector(&zeros),
                vector(&[]),
                vector(&[0.]),
                vector(&rational),
            ]);
            let a = mortgage(&inputs, false).remove(0);
            let offsets: Vec<_> = (0..=width).map(|v| (v * c.months) as f64).collect();
            let times: Vec<_> = delays[first..end]
                .iter()
                .flat_map(|d| (0..c.months).map(move |m| (m + 1) as f64 / 12. + d))
                .collect();
            let params = if self.fixed_oas.is_empty() {
                &targets[first..end]
            } else {
                &self.fixed_oas[first..end]
            };
            let mut fit = oas(
                &[
                    vector(&offsets),
                    vector(&times),
                    vector(&a),
                    vector(params),
                    vector(&[c.base_paths as f64]),
                    vector(&[1e-8]),
                    vector(&[40.]),
                    vector(&[-0.05]),
                    vector(&[0.3]),
                ],
                self.fixed_oas.is_empty(),
            );
            if self.fixed_oas.is_empty() {
                spreads.extend(fit.remove(0));
            } else {
                spreads.extend_from_slice(params);
            }
            prices.extend(fit.remove(0));
        }
        drop(base);
        drop(base_draws);
        Ok(Prepared {
            cc,
            ps,
            b,
            dfs,
            forwards,
            abcd,
            sec,
            faces,
            delays,
            spreads,
            prices,
        })
    }
    fn calculate(&self) -> Result<RiskResult, String> {
        let Prepared {
            cc,
            ps,
            b,
            dfs,
            forwards,
            abcd,
            sec,
            faces,
            delays,
            spreads,
            prices,
        } = self.prepare(true, true)?;
        let c = &self.config;
        let n = faces.len();
        let rational = [f64::from(c.rational_sigmoid)];
        let draws = SharedDraws::new(self.seed, [c.sensitivity_paths, c.months, c.factors])?;
        let scenarios = vec![0.; c.sensitivity_paths];
        let pv = |rates: &[f64], params: &[f64]| -> Result<Vec<f64>, String> {
            let paths = self.paths(rates, params, &b, &draws, &cc, ps)?;
            let mut inputs = self.kernel_inputs(&paths, &sec, c.sensitivity_paths);
            inputs.insert(4, vector(&scenarios));
            inputs.insert(5, vector(&[1.]));
            inputs.extend([vector(&spreads), vector(&delays), vector(&rational)]);
            let values = mortgage_batch(&inputs).remove(0);
            Ok(values
                .into_iter()
                .map(|v| v / c.sensitivity_paths as f64)
                .collect())
        };
        let mut sensitivities =
            Vec::with_capacity((self.tenors.len() + self.vol_quotes.len() / 3) * n);
        let mut dv01 = vec![0.; n];
        for j in 0..self.tenors.len() {
            let mut rates = self.swap_rates.to_vec();
            rates[j] -= c.curve_bump;
            let down = pv(&rates, &abcd)?;
            rates[j] = self.swap_rates[j] + c.curve_bump;
            let up = pv(&rates, &abcd)?;
            for i in 0..n {
                let krd = faces[i] * (down[i] - up[i]) / 2.;
                dv01[i] += krd;
                sensitivities.push(krd);
            }
        }
        for j in 0..self.vol_quotes.len() / 3 {
            let mut quotes = self.vol_quotes.to_vec();
            quotes[3 * j + 2] -= c.vol_bump;
            let down_fit = fit_abcd(&quotes, &forwards, &dfs, &b, &abcd, [c.tenor, c.shift])?.x;
            let down = pv(self.swap_rates, &down_fit)?;
            quotes[3 * j + 2] = self.vol_quotes[3 * j + 2] + c.vol_bump;
            let up_fit = fit_abcd(&quotes, &forwards, &dfs, &b, &abcd, [c.tenor, c.shift])?.x;
            let up = pv(self.swap_rates, &up_fit)?;
            for i in 0..n {
                sensitivities.push(faces[i] * (up[i] - down[i]) / (2. * c.vol_bump) * 0.01);
            }
        }
        if spreads
            .iter()
            .chain(&prices)
            .chain(&dv01)
            .chain(&sensitivities)
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite mortgage risk result".into());
        }
        Ok(RiskResult {
            oas: spreads,
            price: prices,
            dv01,
            sensitivities,
        })
    }
}

impl MortgageRiskRequest<'_> {
    /// Complete forward stress lifecycle; checkpoints are retained for only one
    /// bounded instrument chunk. Base OAS and random draws never change by shock.
    pub fn run_stress(&self, horizons: &[usize], shocks: &[f64]) -> Result<StressResult, String> {
        std::panic::catch_unwind(|| self.calculate_stress(horizons, shocks))
            .map_err(|_| "native mortgage stress failed validation".to_string())?
    }

    fn calculate_stress(&self, horizons: &[usize], shocks: &[f64]) -> Result<StressResult, String> {
        self.validate()?;
        let c = &self.config;
        let (n, nh, nj) = (self.book.len() / 13, horizons.len(), shocks.len());
        if nh == 0
            || nj == 0
            || horizons.iter().any(|&h| h == 0 || h >= c.months)
            || horizons.windows(2).any(|h| h[0] >= h[1])
            || shocks.iter().enumerate().any(|(j, s)| {
                !s.is_finite() || 1. + s * 1e-4 * c.dt <= 0. || shocks[..j].contains(s)
            })
        {
            return Err("invalid mortgage stress horizons or shocks".into());
        }
        let cells = n
            .checked_mul(nh)
            .and_then(|v| v.checked_mul(nj))
            .ok_or("mortgage stress output dimensions overflow")?;
        // Two position arrays, two base arrays and two aggregates. Bound before
        // allocation; the transport layer enforces the same result admission.
        let total = cells
            .checked_mul(2)
            .and_then(|v| v.checked_add(2 * n * nh))
            .and_then(|v| v.checked_add(2 * nj * nh))
            .and_then(|v| v.checked_add(n + nh))
            .ok_or("mortgage stress output dimensions overflow")?;
        if total > 128 * 1024 * 1024 {
            return Err("mortgage stress outputs exceed 1 GiB admission limit".into());
        }
        let scratch_per_position = c
            .sensitivity_paths
            .checked_mul(nh)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_add(6 * c.months))
            .and_then(|v| v.checked_add(4 * nh))
            .and_then(|v| v.checked_mul(8))
            .ok_or("mortgage checkpoint dimensions overflow")?;
        let chunk_size = (64 * 1024 * 1024 / scratch_per_position).min(256);
        if chunk_size == 0 {
            return Err("one mortgage checkpoint exceeds 64 MiB scratch admission".into());
        }
        let Prepared {
            cc,
            ps,
            b,
            abcd,
            sec,
            faces,
            spreads,
            ..
        } = self.prepare(false, true)?;
        let draws = SharedDraws::new(self.seed, [c.sensitivity_paths, c.months, c.factors])?;
        let base = self.paths(self.swap_rates, &abcd, &b, &draws, &cc, ps)?;
        drop(draws);
        let mut out = StressResult {
            oas: spreads,
            base_value: vec![0.; nh * n],
            base_price: vec![0.; nh * n],
            shock_value: vec![0.; cells],
            pnl: vec![0.; cells],
            aggregate_base: vec![0.; nj * nh],
            aggregate_pnl: vec![0.; nj * nh],
            forward_dv01: Vec::new(),
        };
        let rational = [f64::from(c.rational_sigmoid)];
        let hz: Vec<_> = horizons.iter().map(|&h| h as f64).collect();
        let denominator = c.sensitivity_paths as f64;
        for first in (0..n).step_by(chunk_size) {
            portfolio_compute_control::checkpoint()?;
            let end = (first + chunk_size).min(n);
            let width = end - first;
            let chunk: Vec<_> = sec.iter().map(|v| v[first..end].to_vec()).collect();
            let mut inputs = self.kernel_inputs(&base, &chunk, c.sensitivity_paths);
            inputs.extend([
                vector(&out.oas[first..end]),
                vector(&hz),
                vector(&[1.]),
                vector(&rational),
            ]);
            let mut forward = mortgage(&inputs, false);
            let ck_burn = std::mem::take(&mut forward[4]);
            let ck_bal = std::mem::take(&mut forward[3]);
            let balance = std::mem::take(&mut forward[2]);
            let base_values = std::mem::take(&mut forward[1]);
            drop(forward);
            for hi in 0..nh {
                for i in first..end {
                    let value = base_values[(i - first) * nh + hi] / denominator;
                    let bal = balance[(i - first) * nh + hi] / denominator;
                    out.base_value[hi * n + i] = faces[i] * value;
                    out.base_price[hi * n + i] = 100. * value / bal.max(1e-12);
                }
            }
            for (j, &shock_bp) in shocks.iter().enumerate() {
                for (hi, &horizon) in horizons.iter().enumerate() {
                    let shocked = ParallelShock {
                        horizon,
                        shock_bp,
                        dt: c.dt,
                        incentive_lag: c.incentive_lag,
                        cc_lambda: cc.lambda,
                        cc_rate_beta_sum: cc.beta[..4].iter().sum(),
                        hpi_beta: c.hpi[1],
                    }
                    .apply(
                        &base.mtg,
                        &base.hpi,
                        &base.rates.df,
                        [c.sensitivity_paths, c.months],
                    )?;
                    let mut paths = MortgagePaths {
                        mtg: shocked.mtg,
                        hpi: shocked.hpi,
                        yoy: shocked.yoy,
                        rates: RatePaths {
                            df: shocked.df,
                            swaps: Vec::new(),
                            short: Vec::new(),
                        },
                    };
                    if c.float32_paths {
                        for v in paths
                            .mtg
                            .iter_mut()
                            .chain(&mut paths.hpi)
                            .chain(&mut paths.yoy)
                            .chain(&mut paths.rates.df)
                        {
                            *v = (*v as f32) as f64;
                        }
                    }
                    let h = [horizon as f64];
                    let hidx = [hi as f64];
                    let checkpoint = |v| View {
                        v,
                        shape: [width, c.sensitivity_paths, nh],
                    };
                    let mut inputs = self.kernel_inputs(&paths, &chunk, c.sensitivity_paths);
                    inputs.extend([
                        vector(&out.oas[first..end]),
                        vector(&h),
                        vector(&hidx),
                        checkpoint(&ck_bal),
                        checkpoint(&ck_burn),
                        vector(&rational),
                    ]);
                    let values = mortgage(&inputs, true).remove(0);
                    for i in first..end {
                        let idx = (j * nh + hi) * n + i;
                        out.shock_value[idx] = faces[i] * (values[i - first] / denominator);
                        // Keep the reference's subtract-before-notional order.
                        let base_unit = base_values[(i - first) * nh + hi] / denominator;
                        out.pnl[idx] = faces[i] * (values[i - first] / denominator - base_unit);
                    }
                }
            }
        }
        for j in 0..nj {
            for hi in 0..nh {
                out.aggregate_base[j * nh + hi] = out.base_value[hi * n..(hi + 1) * n].iter().sum();
                out.aggregate_pnl[j * nh + hi] = out.pnl[(j * nh + hi) * n..(j * nh + hi + 1) * n]
                    .iter()
                    .sum();
            }
        }
        if let (Some(jm), Some(jp)) = (
            shocks.iter().position(|s| *s == -100.),
            shocks.iter().position(|s| *s == 100.),
        ) {
            out.forward_dv01 = (0..nh)
                .map(|hi| {
                    (0..n)
                        .map(|i| {
                            (out.shock_value[(jm * nh + hi) * n + i]
                                - out.shock_value[(jp * nh + hi) * n + i])
                                / 200.
                        })
                        .sum()
                })
                .collect();
        }
        if out
            .oas
            .iter()
            .chain(&out.base_value)
            .chain(&out.base_price)
            .chain(&out.shock_value)
            .chain(&out.pnl)
            .chain(&out.aggregate_base)
            .chain(&out.aggregate_pnl)
            .chain(&out.forward_dv01)
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite mortgage stress result".into());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natural_spline_independent_values() {
        let s = NaturalSpline::new(&[0., 1., 2.], &[0., 1., 0.]).unwrap();
        assert_eq!(s.value(-1.), 0.);
        assert_eq!(s.value(1.), 1.);
        assert_eq!(s.value(3.), 0.);
        assert!((s.value(0.5) - 0.6875).abs() < 1e-14);
        assert!(NaturalSpline::new(&[1., 1.], &[0., 1.]).is_err());
    }
    #[test]
    fn standalone_raw_mortgage_lifecycle_reprices_and_sums_risk() {
        let mut history = vec![0.04; 22];
        history[10] = 0.03;
        history[11..21].fill(0.05);
        history[21] = 0.04;
        let quotes: Vec<_> = [1_f64, 3., 5.]
            .into_iter()
            .flat_map(|e| {
                [2_f64, 5., 10.]
                    .into_iter()
                    .flat_map(move |t| [e, t, 0.26 - 0.012 * e.ln() - 0.01 * t.ln()])
            })
            .collect();
        let month: Vec<_> = (0..24).map(|m| (m % 12) as f64).collect();
        let request = MortgageRiskRequest {
            tenors: &[1., 2., 3., 5., 10., 30.],
            swap_rates: &[0.04; 6],
            vol_quotes: &quotes,
            cc_history: &history,
            ps_history: &[0.012, 0.013, 0.012, 0.014],
            book: &[
                0.05, 0.045, 24., 0., 0.8, 1., 720., 200000., 0., 0., 96., 1000000., 24.,
            ],
            original_hpi: &[],
            prepay_multiplier: &[],
            seed: &[19],
            fixed_oas: &[],
            config: RiskConfig {
                base_paths: 1,
                sensitivity_paths: 1,
                months: 24,
                forwards: 161,
                factors: 3,
                dt: 1. / 12.,
                tenor: 0.25,
                shift: 0.02,
                float32_paths: false,
                curve_bump: 0.0001,
                vol_bump: 0.0025,
                hpi: [0.03, -1., 0.03],
                incentive_lag: 3,
                ps_spot: 0.012,
                rational_sigmoid: true,
            },
            prepay: PrepayData {
                month_of_year: &month,
                seasonality: &[1.; 12],
                parameters: &[0., 0., 0., 0., 0., 0., 0., 1., 0.],
                ltv_knots: &[0., 2.],
                ltv_coefficients: &[0., 0., 0., 1.],
                smm_table: &[0., 0.],
                smm_scale: 1.,
                burnout_table: &[1., 1.],
                burnout_scale: 1.,
                cc_vol_points: &[0.5, 1., 0.5, 1., 0.5, 1., 0.5, 1., 0.5, 1., 0.5, 1.],
                fico_x: &[580., 800.],
                fico_y: &[1., 1.],
                size_x: &[50000., 600000.],
                size_y: &[1., 1.],
                state_multipliers: &[1.],
                channel_multipliers: &[1.],
            },
        };
        let out = request.run().unwrap();
        assert!((out.price[0] - 0.96).abs() < 1e-8);
        assert!((out.sensitivities[..6].iter().sum::<f64>() - out.dv01[0]).abs() < 1e-10);
        let fixed = out.oas;
        let replay_request = MortgageRiskRequest {
            fixed_oas: &fixed,
            ..request
        };
        let replay = replay_request.run().unwrap();
        assert_eq!(replay.oas, fixed);
        assert!((replay.price[0] - out.price[0]).abs() < 1e-12);
        let stress = replay_request
            .run_stress(&[1, 12, 23], &[-100., 0., 100.])
            .unwrap();
        assert_eq!(stress.oas, fixed);
        assert_eq!(stress.aggregate_pnl, stress.pnl);
        assert_eq!(&stress.aggregate_base[..3], &stress.base_value);
        for hi in 0..3 {
            assert!(stress.pnl[3 + hi].abs() < 0.1); // f32 checkpoint quantization
            assert_eq!(
                stress.forward_dv01[hi],
                (stress.shock_value[hi] - stress.shock_value[6 + hi]) / 200.
            );
        }
        assert!(replay_request.run_stress(&[0], &[0.]).is_err());
        assert!(replay_request.run_stress(&[1, 1], &[0.]).is_err());
        assert!(replay_request.run_stress(&[1], &[0., 0.]).is_err());

        // Per-pool prepay speed multiplier. The fixture above has prepayment off, so
        // switch on turnover (6% CPR) and refi under the fixed OAS. An explicit 1
        // is the model's own speed. The pool is priced at 96, a discount at its
        // fixed OAS, so faster prepayment returns par sooner and raises its value;
        // 0 stops prepayment.
        // Bad lengths and negative or non-finite values are rejected.
        let prepaying = PrepayData {
            parameters: &[0.3, -2., 100., 0., 0.06, 0.6, 0., 1., 0.],
            smm_table: &[0., 0.1],
            ..replay_request.prepay.clone()
        };
        let with = |speed: &'static [f64]| MortgageRiskRequest {
            prepay_multiplier: speed,
            prepay: prepaying.clone(),
            ..replay_request.clone()
        };
        let model = with(&[]).run().unwrap();
        let unit = with(&[1.]).run().unwrap();
        assert_eq!(unit.price, model.price);
        assert_eq!(unit.sensitivities, model.sensitivities);
        let fast = with(&[3.]).run().unwrap();
        let none = with(&[0.]).run().unwrap();
        assert_eq!(fast.oas, fixed);
        assert!(fast.price[0] > model.price[0] && model.price[0] > none.price[0]);
        let none_off = MortgageRiskRequest {
            prepay_multiplier: &[0.],
            ..replay_request.clone()
        };
        // with prepayment already off, a zero multiplier changes nothing
        assert_eq!(none_off.run().unwrap().price, replay.price);
        let unit_stress = with(&[1.])
            .run_stress(&[1, 12, 23], &[-100., 0., 100.])
            .unwrap();
        let model_stress = with(&[])
            .run_stress(&[1, 12, 23], &[-100., 0., 100.])
            .unwrap();
        assert_eq!(unit_stress.pnl, model_stress.pnl);
        assert!(with(&[-0.5]).run().is_err());
        assert!(with(&[1., 1.]).run().is_err());
        assert!(with(&[f64::NAN]).run().is_err());
    }
}
