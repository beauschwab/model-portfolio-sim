//! Supplied absolute-strike quote grid. No ATM-only or stochastic-volatility claim.
use super::{label, range};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VolConvention {
    Normal {},
    ShiftedLognormal { shift: f64 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VolTail {
    Reject,
    FlatVolatility,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmileSurface {
    pub surface_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub convention: VolConvention,
    pub expiries_years: Vec<f64>,
    pub tenors_years: Vec<f64>,
    pub strikes: Vec<f64>,
    /// Row-major [expiry,underlying tenor,absolute strike]. Normal is rate/sqrt(year);
    /// shifted-lognormal is relative volatility/sqrt(year), never normal basis points.
    pub volatilities: Vec<f64>,
    pub expiry_tail: VolTail,
    pub tenor_tail: VolTail,
    pub strike_tail: VolTail,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionSide {
    Call,
    Put,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionQuery {
    pub expiry_years: f64,
    pub tenor_years: f64,
    pub strike: f64,
    pub forward: f64,
    pub payoff_unit: f64,
    pub side: OptionSide,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionResult {
    pub surface_id: String,
    pub convention: VolConvention,
    pub volatility: f64,
    pub value: f64,
    pub expiry_tail_used: bool,
    pub tenor_tail_used: bool,
    pub strike_tail_used: bool,
    pub interpolated: bool,
}
impl SmileSurface {
    pub fn validate(&self) -> Result<(), String> {
        for s in [&self.surface_id, &self.source_id, &self.source_revision] {
            label(s, 256)?;
        }
        if self.expiries_years.is_empty()
            || self.tenors_years.is_empty()
            || self.strikes.is_empty()
            || self.expiries_years.len() > 128
            || self.tenors_years.len() > 128
            || self.strikes.len() > 256
        {
            return Err("smile grid dimension admission exceeded".into());
        }
        let count = self
            .expiries_years
            .len()
            .checked_mul(self.tenors_years.len())
            .and_then(|v| v.checked_mul(self.strikes.len()))
            .ok_or("smile dimension overflow")?;
        if count > 65_536 || self.volatilities.len() != count {
            return Err(
                "smile must provide complete bounded expiry/tenor/absolute-strike grid".into(),
            );
        }
        grid(&self.expiries_years, 1e-8, 100., "expiry")?;
        grid(&self.tenors_years, 1e-8, 100., "underlying tenor")?;
        grid(&self.strikes, -2., 2., "absolute strike")?;
        if let VolConvention::ShiftedLognormal { shift } = self.convention {
            range(shift, 0., 2., "Black shift")?;
            for &strike in &self.strikes {
                range(strike + shift, 1e-12, 4., "shifted strike")?;
            }
        }
        for &v in &self.volatilities {
            range(
                v,
                0.,
                match self.convention {
                    VolConvention::Normal {} => 2.,
                    VolConvention::ShiftedLognormal { .. } => 5.,
                },
                "quoted volatility",
            )?;
        }
        Ok(())
    }
    pub fn price(&self, q: &OptionQuery) -> Result<OptionResult, String> {
        self.validate()?;
        range(q.expiry_years, 0., 100., "option expiry")?;
        range(q.tenor_years, 1e-8, 100., "option tenor")?;
        range(q.strike, -2., 2., "option strike")?;
        range(q.forward, -2., 2., "option forward")?;
        range(
            q.payoff_unit,
            0.,
            MAX_MONETARY_AMOUNT,
            "discounted payoff unit",
        )?;
        let (a, b, w, et, en) =
            bracket(&self.expiries_years, q.expiry_years, self.expiry_tail, true)?;
        let (c, d, u, tt, tn) = bracket(&self.tenors_years, q.tenor_years, self.tenor_tail, false)?;
        let (e, f, v, st, sn) = bracket(&self.strikes, q.strike, self.strike_tail, false)?;
        let variance = |i: usize| {
            let at = |j, k| {
                let vol =
                    self.volatilities[(i * self.tenors_years.len() + j) * self.strikes.len() + k];
                vol * vol * self.expiries_years[i]
            };
            let low = at(c, e) + (at(c, f) - at(c, e)) * v;
            let high = at(d, e) + (at(d, f) - at(d, e)) * v;
            low + (high - low) * u
        };
        let (total, vol) = if a == b {
            let quote_vol = (variance(a) / self.expiries_years[a]).sqrt();
            (quote_vol * quote_vol * q.expiry_years, quote_vol)
        } else {
            let total = variance(a) + (variance(b) - variance(a)) * w;
            (total, (total / q.expiry_years).sqrt())
        };
        let value = option_value(
            q.forward,
            q.strike,
            total.sqrt(),
            q.payoff_unit,
            self.convention,
            q.side,
        )?;
        Ok(OptionResult {
            surface_id: self.surface_id.clone(),
            convention: self.convention,
            volatility: vol,
            value,
            expiry_tail_used: et,
            tenor_tail_used: tt,
            strike_tail_used: st,
            interpolated: !en || !tn || !sn,
        })
    }
}
fn grid(v: &[f64], min: f64, max: f64, name: &str) -> Result<(), String> {
    for &x in v {
        range(x, min, max, name)?;
    }
    if v.windows(2).any(|w| w[1] <= w[0]) {
        return Err(format!("{name} grid must be strictly increasing"));
    }
    Ok(())
}
fn bracket(
    g: &[f64],
    x: f64,
    tail: VolTail,
    allow_zero: bool,
) -> Result<(usize, usize, f64, bool, bool), String> {
    let i = g.partition_point(|v| *v < x);
    if i < g.len() && g[i] == x {
        return Ok((i, i, 0., false, true));
    }
    if i == 0 || i == g.len() {
        if tail == VolTail::Reject && !(allow_zero && x == 0.) {
            return Err("volatility query beyond explicit grid/tail".into());
        }
        let j = if i == 0 { 0 } else { g.len() - 1 };
        return Ok((j, j, 0., x != 0., false));
    }
    Ok((i - 1, i, (x - g[i - 1]) / (g[i] - g[i - 1]), false, false))
}

/// Undiscounted normalized standard deviation supplied directly for hand/controlled tests.
pub fn option_value(
    forward: f64,
    strike: f64,
    stddev: f64,
    payoff_unit: f64,
    convention: VolConvention,
    side: OptionSide,
) -> Result<f64, String> {
    range(forward, -2., 2., "option forward")?;
    range(strike, -2., 2., "option strike")?;
    range(stddev, 0., 50., "option standard deviation")?;
    range(payoff_unit, 0., MAX_MONETARY_AMOUNT, "payoff unit")?;
    let intrinsic = match side {
        OptionSide::Call => (forward - strike).max(0.),
        OptionSide::Put => (strike - forward).max(0.),
    };
    let (f, k) = match convention {
        VolConvention::Normal {} => (forward, strike),
        VolConvention::ShiftedLognormal { shift } => {
            range(shift, 0., 2., "Black shift")?;
            range(forward + shift, 1e-12, 4., "shifted forward")?;
            range(strike + shift, 1e-12, 4., "shifted strike")?;
            (forward + shift, strike + shift)
        }
    };
    let otm = if stddev == 0. {
        0.
    } else {
        match convention {
            VolConvention::Normal {} => {
                let z = (f - k).abs() / stddev;
                if z > 38. {
                    0.
                } else {
                    stddev * (normal_pdf(z) - z * normal_survival(z))
                }
            }
            VolConvention::ShiftedLognormal { .. } if f == k => {
                2. * f * normal_center(stddev * 0.5)
            }
            VolConvention::ShiftedLognormal { .. } => {
                let log_ratio = ((f - k) / k).ln_1p();
                let d1 = log_ratio / stddev + stddev * 0.5;
                let d2 = d1 - stddev;
                if f < k {
                    f * normal_cdf(d1) - k * normal_cdf(d2)
                } else {
                    k * normal_cdf(-d2) - f * normal_cdf(-d1)
                }
            }
        }
    };
    if !otm.is_finite() || otm < -128. * f64::EPSILON * (f.abs() + k.abs() + stddev) {
        return Err("option formula outside stable finite domain".into());
    }
    let value = payoff_unit * (intrinsic + otm.max(0.));
    range(value, 0., MAX_MONETARY_AMOUNT, "option value")?;
    Ok(value)
}
fn normal_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / (2. * std::f64::consts::PI).sqrt()
}
// Positive convergent series for Phi(x)-1/2, avoiding near-ATM cancellation.
fn normal_center(x: f64) -> f64 {
    if x < 0. {
        return -normal_center(-x);
    }
    if x >= 1.5 {
        return 0.5 - normal_survival(x);
    }
    let mut term = x;
    let mut sum = term;
    for n in 1..128 {
        term *= x * x / (2 * n + 1) as f64;
        sum += term;
        if term.abs() <= sum.abs() * f64::EPSILON {
            break;
        }
    }
    normal_pdf(x) * sum
}
// Laplace continued fraction for the Gaussian Mills ratio. At >=1.5, 512
// levels converge well inside double precision; no cancellation of tiny tails.
fn normal_survival(x: f64) -> f64 {
    if x < 0. {
        return 1. - normal_survival(-x);
    }
    if x < 1.5 {
        return 0.5 - normal_center(x);
    }
    if x > 38. {
        return 0.;
    }
    let mut tail = 0.;
    for n in (1..=512).rev() {
        tail = n as f64 / (x + tail);
    }
    normal_pdf(x) / (x + tail)
}
fn normal_cdf(x: f64) -> f64 {
    if x <= 0. {
        normal_survival(-x)
    } else {
        1. - normal_survival(x)
    }
}
