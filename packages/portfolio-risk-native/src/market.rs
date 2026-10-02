//! Rust-owned market path lifecycle from explicit calibrated parameters and CRN.
//! Calibration controllers and random-number generation are separate inputs;
//! no Python callback or FFI round trip occurs between stages in this module.
use crate::quant::{behavioral, lmm, volatility, View};

fn interp(x: f64, times: &[f64], values: &[f64]) -> f64 {
    let i = times.partition_point(|t| *t <= x);
    if i == 0 {
        return values[0];
    }
    if i == times.len() {
        return values[i - 1];
    }
    values[i - 1] + (values[i] - values[i - 1]) * (x - times[i - 1]) / (times[i] - times[i - 1])
}

/// Annual fixed-leg par bootstrap and log-linear DF interpolation, with the
/// reference model's flat-zero extrapolation beyond the final quoted pillar.
pub fn bootstrap(
    tenors: &[f64],
    rates: &[f64],
    forwards: usize,
    tenor: f64,
) -> Result<Vec<f64>, String> {
    if tenors.is_empty()
        || tenors.len() != rates.len()
        || forwards == 0
        || forwards > 4096
        || !tenor.is_finite()
        || tenor <= 0.
        || tenors[0] < 1.
        || tenors.iter().chain(rates).any(|x| !x.is_finite())
        || tenors.windows(2).any(|w| w[1] <= w[0])
        || tenors[tenors.len() - 1] > 200.
    {
        return Err("invalid bootstrap pillars or forward grid".into());
    }
    let mut times = vec![0.];
    let mut logs = vec![0.];
    for (&t, &r) in tenors.iter().zip(rates) {
        times.push(t);
        logs.push(0.);
        let residual = |log: f64, values: &mut Vec<f64>| {
            *values.last_mut().unwrap() = log;
            let payments = (t - 0.5).ceil() as usize;
            let mut sum = 0.;
            let mut terminal = 0.;
            for p in 1..=payments {
                terminal = interp(p as f64, &times, values).exp();
                sum += terminal;
            }
            r * sum + terminal - 1.
        };
        let (mut lo, mut hi) = (-5., 0.5);
        let (mut flo, fhi) = (residual(lo, &mut logs), residual(hi, &mut logs));
        if flo == 0. {
            *logs.last_mut().unwrap() = lo;
            continue;
        }
        if fhi == 0. {
            *logs.last_mut().unwrap() = hi;
            continue;
        }
        if flo.is_sign_positive() == fhi.is_sign_positive() {
            return Err("par curve root is outside the model bracket".into());
        }
        for _ in 0..100 {
            let mid = (lo + hi) * 0.5;
            let fm = residual(mid, &mut logs);
            if fm == 0. || hi - lo < 2e-15 {
                break;
            }
            if fm.is_sign_positive() == flo.is_sign_positive() {
                lo = mid;
                flo = fm;
            } else {
                hi = mid;
            }
        }
    }
    let last = times.len() - 1;
    let curve: Vec<_> = (0..=forwards)
        .map(|i| {
            let t = i as f64 * tenor;
            if t <= times[last] {
                interp(t, &times, &logs).exp()
            } else {
                (logs[last] / times[last] * t).exp()
            }
        })
        .collect();
    if curve.iter().any(|x| !x.is_finite() || *x <= 0.) {
        return Err("invalid bootstrapped discounts".into());
    }
    Ok(curve)
}

fn vector(v: &[f64]) -> View<'_> {
    View {
        v,
        shape: [v.len(), 1, 1],
    }
}
fn array(v: &[f64], shape: [usize; 3]) -> View<'_> {
    View { v, shape }
}

#[derive(Clone)]
pub struct RatePaths {
    pub df: Vec<f64>,
    pub swaps: Vec<f64>,
    pub short: Vec<f64>,
}

#[derive(Clone)]
pub struct MortgagePaths {
    pub rates: RatePaths,
    pub mtg: Vec<f64>,
    pub hpi: Vec<f64>,
    pub yoy: Vec<f64>,
}

pub struct ShockPaths {
    pub mtg: Vec<f64>,
    pub hpi: Vec<f64>,
    pub yoy: Vec<f64>,
    pub df: Vec<f64>,
}

/// Default-model forward parallel templates, with original rate/swap draws held.
pub struct ParallelShock {
    pub horizon: usize,
    pub shock_bp: f64,
    pub dt: f64,
    pub incentive_lag: usize,
    pub cc_lambda: f64,
    pub cc_rate_beta_sum: f64,
    pub hpi_beta: f64,
}

impl ParallelShock {
    pub fn apply(
        &self,
        mtg: &[f64],
        hpi: &[f64],
        df: &[f64],
        shape: [usize; 2],
    ) -> Result<ShockPaths, String> {
        let [p, t] = shape;
        if p == 0
            || t == 0
            || p.checked_mul(t) != Some(mtg.len())
            || hpi.len() != mtg.len()
            || df.len() != mtg.len()
            || self.horizon >= t
            || self.incentive_lag > t
            || self.dt <= 0.
            || [
                self.shock_bp,
                self.dt,
                self.cc_lambda,
                self.cc_rate_beta_sum,
                self.hpi_beta,
            ]
            .iter()
            .chain(mtg)
            .chain(hpi)
            .chain(df)
            .any(|x| !x.is_finite())
            || !(0. ..=1.).contains(&self.cc_lambda)
            || hpi.iter().any(|x| *x <= 0.)
        {
            return Err("invalid parallel shock inputs".into());
        }
        let d = self.shock_bp * 1e-4;
        if 1. + d * self.dt <= 0. {
            return Err("invalid parallel shock discount factor".into());
        }
        let mut out = ShockPaths {
            mtg: vec![0.; mtg.len()],
            hpi: vec![0.; mtg.len()],
            yoy: vec![0.; mtg.len()],
            df: vec![0.; mtg.len()],
        };
        let mut cs = vec![0.; t];
        let mut gh = vec![1.; t];
        let mut gd = vec![1.; t];
        for m in self.horizon..t {
            let k = (m - self.horizon + 1) as f64;
            cs[m] = d * self.cc_rate_beta_sum * (1. - (1. - self.cc_lambda).powf(k));
            gh[m] = (self.hpi_beta * d * self.dt * k).exp();
            gd[m] = (1. + d * self.dt).powf(-k);
        }
        for path in 0..p {
            for m in 0..t {
                let i = path * t + m;
                out.mtg[i] = mtg[i]
                    + if m >= self.incentive_lag {
                        cs[m - self.incentive_lag]
                    } else {
                        0.
                    };
                out.hpi[i] = hpi[i] * gh[m];
                out.df[i] = df[i] * gd[m];
                out.yoy[i] = out.hpi[i] / if m < 12 { 1. } else { out.hpi[i - 12] } - 1.;
            }
        }
        if out
            .mtg
            .iter()
            .chain(&out.hpi)
            .chain(&out.yoy)
            .chain(&out.df)
            .any(|x| !x.is_finite())
        {
            return Err("nonfinite parallel shock paths".into());
        }
        Ok(out)
    }
}

pub struct MortgageInputs<'a> {
    pub vol_points: &'a [f64], // expiry, tenor pairs
    pub cc_beta: &'a [f64],
    pub cc_lambda: f64,
    pub ps: [f64; 4], // kappa, theta, sigma, spot
    pub eps_ps: &'a [f64],
    pub eps_h: &'a [f64],
    pub hpi: [f64; 3], // mu, beta, sigma
    pub incentive_lag: usize,
}

/// One run's borrowed common draws and calibrated model parameters, plus an
/// owned bootstrapped curve and volatility table. Calling rates/mortgage cannot
/// reseed or invoke a different calibration implicitly.
pub struct MarketContext<'a> {
    p: &'a [f64],
    loadings: &'a [f64],
    draws: &'a [f64],
    shape: [usize; 3], // paths, months, factors
    grid: [f64; 3],    // dt, tenor, shift
    curve: Vec<f64>,
    forwards: Vec<f64>,
    sigma: Vec<f64>,
}

impl<'a> MarketContext<'a> {
    pub fn new(
        tenors: &[f64],
        rates: &[f64],
        p: &'a [f64],
        loadings: &'a [f64],
        draws: &'a [f64],
        shape: [usize; 3],
        grid: [f64; 3],
    ) -> Result<Self, String> {
        let [paths, months, factors] = shape;
        let [dt, tenor, shift] = grid;
        if p.len() != 4
            || shape.contains(&0)
            || factors > 32
            || !loadings.len().is_multiple_of(factors)
            || shape.into_iter().try_fold(1usize, |a, b| a.checked_mul(b)) != Some(draws.len())
            || p.iter()
                .chain(loadings)
                .chain(draws)
                .chain(&grid)
                .any(|x| !x.is_finite())
            || dt <= 0.
            || tenor != 0.25
            || shift < 0.
            || paths.checked_mul(months).is_none()
        {
            return Err("invalid market context inputs".into());
        }
        let n = loadings.len() / factors;
        if n == 0 || (months - 1) as f64 * dt / tenor >= n as f64 {
            return Err("market path horizon exceeds forward grid".into());
        }
        let curve = bootstrap(tenors, rates, n, tenor)?;
        let forwards: Vec<_> = curve
            .windows(2)
            .map(|d| (d[0] / d[1] - 1.) / tenor)
            .collect();
        if forwards
            .iter()
            .any(|f| *f + shift <= 0. || 1. + f * dt <= 0.)
        {
            return Err("curve outside shifted rate domain".into());
        }
        let mut sigma = Vec::with_capacity(months * n);
        for m in 0..months {
            for i in 0..n {
                let tau = ((i + 1) as f64 * tenor - m as f64 * dt).max(1e-6);
                sigma.push((p[0] + p[1] * tau) * (-p[2] * tau).exp() + p[3]);
            }
        }
        Ok(Self {
            p,
            loadings,
            draws,
            shape,
            grid,
            curve,
            forwards,
            sigma,
        })
    }

    pub fn rates(&self) -> Result<RatePaths, String> {
        let [_, t, k] = self.shape;
        let [dt, tenor, shift] = self.grid;
        let mut out = lmm(&[
            vector(&self.forwards),
            array(&self.sigma, [t, self.forwards.len(), 1]),
            array(self.loadings, [self.forwards.len(), k, 1]),
            array(self.draws, self.shape),
            vector(&[shift]),
            vector(&[dt]),
            vector(&[tenor]),
        ])
        .into_iter();
        let df = out.next().unwrap();
        let swaps = out.next().unwrap();
        let short = out.next().unwrap();
        if df
            .iter()
            .chain(&swaps)
            .chain(&short)
            .any(|x| !x.is_finite())
        {
            return Err("nonfinite rate paths".into());
        }
        Ok(RatePaths { df, swaps, short })
    }

    pub fn mortgage(&self, model: &MortgageInputs<'_>) -> Result<MortgagePaths, String> {
        let [paths, t, k] = self.shape;
        let [dt, tenor, shift] = self.grid;
        let n = self.forwards.len();
        if model.vol_points.len() != 12
            || model.cc_beta.len() != 11
            || model.eps_ps.len() != paths * t
            || model.eps_h.len() != paths * t
            || model.incentive_lag > t
            || !model.cc_lambda.is_finite()
            || !(0. ..=1.).contains(&model.cc_lambda)
            || model
                .vol_points
                .iter()
                .chain(model.cc_beta)
                .chain(model.eps_ps)
                .chain(model.eps_h)
                .chain(&model.ps)
                .chain(&model.hpi)
                .any(|x| !x.is_finite())
        {
            return Err("invalid mortgage path inputs".into());
        }
        let rates = self.rates()?;
        let mut queries = Vec::with_capacity(6 * t * 4);
        let mut valid = vec![false; 6 * t];
        for (j, point) in model.vol_points.chunks_exact(2).enumerate() {
            let nq = (point[1] / tenor).round_ties_even() as usize;
            if point[0] <= 0. || point[1] <= 0. || nq == 0 || nq + 1 >= n {
                return Err("invalid volatility feature tenor".into());
            }
            for m in 0..t {
                let time = m as f64 * dt;
                let first = ((time + point[0]) / tenor).round_ties_even() as usize;
                valid[j * t + m] = first + nq < n;
                queries.extend([time, point[0], first.min(n - nq - 1) as f64, nq as f64]);
            }
        }
        let values = volatility(&[
            array(&queries, [6 * t, 4, 1]),
            vector(self.p),
            vector(&self.forwards),
            vector(&self.curve),
            array(self.loadings, [n, k, 1]),
            vector(&[tenor]),
            vector(&[shift]),
        ]);
        let mut features: Vec<_> = values[0].chunks_exact(5).map(|v| v[0]).collect();
        for j in 0..6 {
            let last = (0..t)
                .rfind(|m| valid[j * t + m])
                .ok_or("volatility feature has no valid horizon")?;
            for m in 0..t {
                if !valid[j * t + m] {
                    features[j * t + m] = features[j * t + last];
                }
            }
        }
        let cc = behavioral(&[
            vector(&[0.]),
            array(&rates.swaps, [paths, 4, t]),
            array(&features, [6, t, 1]),
            vector(model.cc_beta),
            vector(&[model.cc_lambda]),
        ])
        .remove(0);
        let ps = behavioral(&[
            vector(&[1.]),
            array(model.eps_ps, [paths, t, 1]),
            vector(&[model.ps[0], model.ps[1], model.ps[2], dt]),
            vector(&[model.ps[3]]),
        ])
        .remove(0);
        let s10: Vec<_> = rates
            .swaps
            .chunks_exact(4 * t)
            .flat_map(|p| p[2 * t..3 * t].iter().copied())
            .collect();
        let hpi = behavioral(&[
            vector(&[2.]),
            array(&s10, [paths, t, 1]),
            array(model.eps_h, [paths, t, 1]),
            vector(&[model.hpi[0], model.hpi[1], model.hpi[2], dt]),
        ])
        .remove(0);
        let mut mtg = vec![0.; paths * t];
        let mut yoy = vec![0.; paths * t];
        for p in 0..paths {
            for m in 0..t {
                let i = p * t + m;
                let lagged = p * t + m.saturating_sub(model.incentive_lag);
                mtg[i] = cc[lagged] + ps[lagged];
                yoy[i] = hpi[i] / if m < 12 { 1. } else { hpi[i - 12] } - 1.;
            }
        }
        if mtg.iter().chain(&hpi).chain(&yoy).any(|x| !x.is_finite()) {
            return Err("nonfinite mortgage paths".into());
        }
        Ok(MortgagePaths {
            rates,
            mtg,
            hpi,
            yoy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_curve_reprices_annual_par_and_flat_zero_tail() {
        for r in [-0.01, 0., 0.04] {
            let dfs = bootstrap(&[1., 2., 5., 10., 30.], &[r; 5], 161, 0.25).unwrap();
            for (i, d) in dfs.iter().enumerate() {
                assert!((d - (1. + r).powf(-(i as f64) * 0.25)).abs() < 1e-12);
            }
        }
        assert!(bootstrap(&[2., 1.], &[0.04; 2], 10, 0.25).is_err());
        assert!(bootstrap(&[1.], &[-2.], 10, 0.25).is_err());
    }

    #[test]
    fn standalone_context_runs_rates_and_mortgage_without_callbacks() {
        let draws = vec![0.; 3 * 24];
        let loadings = vec![1.; 161];
        let shocks = vec![0.; 3 * 24];
        let ctx = MarketContext::new(
            &[1., 2., 5., 10., 30.],
            &[0.04; 5],
            &[0., 0., 1., 0.],
            &loadings,
            &draws,
            [3, 24, 1],
            [1. / 12., 0.25, 0.02],
        )
        .unwrap();
        let mut beta = [0.; 11];
        beta[10] = 0.06;
        let model = MortgageInputs {
            vol_points: &[1., 10., 2., 10., 5., 10., 1., 2., 2., 5., 5., 5.],
            cc_beta: &beta,
            cc_lambda: 0.35,
            ps: [1., 0.01, 0., 0.01],
            eps_ps: &shocks,
            eps_h: &shocks,
            hpi: [0.03, -0.5, 0.],
            incentive_lag: 2,
        };
        let out = ctx.mortgage(&model).unwrap();
        let forward = (1.04_f64.powf(0.25) - 1.) / 0.25;
        for p in 0..3 {
            for m in 0..24 {
                let i = p * 24 + m;
                assert!(
                    (out.rates.df[i] - (1. + forward / 12.).powi(-(m as i32 + 1))).abs() < 1e-12
                );
                assert!((out.mtg[i] - 0.07).abs() < 1e-14);
                assert!((out.hpi[i] - (0.03 * (m + 1) as f64 / 12.).exp()).abs() < 1e-14);
                assert!(
                    (out.yoy[i] - (0.03 * (m + 1).min(12) as f64 / 12.).exp() + 1.).abs() < 1e-14
                );
            }
        }
        assert_eq!(ctx.rates().unwrap().df, out.rates.df);
    }
}
