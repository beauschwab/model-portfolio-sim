//! Conditional income-only paths. These must never enter risk/OAS valuation.
use crate::market::lifecycle::finite;
use crate::market::{MortgagePaths, RatePaths};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Forecast {
    #[serde(default)]
    pub targets: BTreeMap<String, Vec<f64>>,
    /// Parallel ramp of the mean simulated income paths. Mutually exclusive with targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp: Option<Ramp>,
}
/// Linear ramp: the mean of month `m` (zero-based) moves by `shift * min((m + 1) / months, 1)`.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    pub shift: f64,
    pub months: usize,
}
impl Ramp {
    fn at(&self, m: usize) -> f64 {
        // The final level is the exact shift, not a rounded product.
        if m + 1 >= self.months {
            self.shift
        } else {
            self.shift * (m + 1) as f64 / self.months as f64
        }
    }
    fn check(&self) -> Result<(), String> {
        if !self.shift.is_finite() || self.shift.abs() > 0.2 || !(1..=120).contains(&self.months) {
            return Err("ramp requires a finite shift within +/-2000 bp over 1..120 months".into());
        }
        Ok(())
    }
    /// Move the mean of each month by the ramp and rescale shifted-rate deviations around it,
    /// as conditional forecasts do, so the model rate floor is preserved on every path.
    fn rates(
        &self,
        base: &RatePaths,
        p: usize,
        t: usize,
        dt: f64,
        floor: f64,
    ) -> Result<RatePaths, String> {
        self.check()?;
        if p == 0
            || base.short.len() != p * t
            || base.df.len() != p * t
            || base.swaps.len() != p * 4 * t
        {
            return Err("forecast grid mismatch".into());
        }
        let mut out = base.clone();
        if self.shift == 0. {
            return Ok(out); // a zero ramp is the base, bit for bit
        }
        for m in 0..t {
            let d = self.at(m);
            rescale(&mut out.short, p, |i| i * t + m, d, floor)?;
            for j in 0..4 {
                rescale(&mut out.swaps, p, |i| (i * 4 + j) * t + m, d, floor)?;
            }
        }
        for i in 0..p {
            let mut df = 1.;
            for m in 0..t {
                df /= 1. + out.short[i * t + m] * dt;
                out.df[i * t + m] = df;
            }
        }
        finite(&out.df)?;
        finite(&out.swaps)?;
        Ok(out)
    }
}
/// Shift the cross-path mean by `d`, keeping every shifted rate positive.
fn rescale(
    values: &mut [f64],
    p: usize,
    index: impl Fn(usize) -> usize,
    d: f64,
    floor: f64,
) -> Result<(), String> {
    let mean = (0..p).map(|i| values[index(i)] + floor).sum::<f64>() / p as f64;
    let target = mean + d;
    if !(mean > 0. && target > 0.) {
        return Err("forecast crosses shifted-rate floor".into());
    }
    for i in 0..p {
        let k = index(i);
        values[k] = (values[k] + floor) / mean * target - floor;
    }
    Ok(())
}
fn recenter(
    values: &mut [f64],
    target: &[f64],
    p: usize,
    t: usize,
    shift: f64,
) -> Result<(), String> {
    if values.len() != p * t || target.len() != t {
        return Err("forecast grid mismatch".into());
    }
    finite(target)?;
    for m in 0..t {
        let mean = (0..p).map(|i| values[i * t + m] + shift).sum::<f64>() / p as f64;
        if target[m] + shift <= 0. || (0..p).any(|i| values[i * t + m] + shift <= 0.) {
            return Err("forecast crosses shifted-rate floor".into());
        }
        for i in 0..p {
            values[i * t + m] = (values[i * t + m] + shift) / mean * (target[m] + shift) - shift;
        }
    }
    Ok(())
}
impl Forecast {
    pub(crate) fn rates(
        &self,
        base: &RatePaths,
        p: usize,
        t: usize,
        dt: f64,
        shift: f64,
    ) -> Result<RatePaths, String> {
        if let (Some(ramp), true) = (&self.ramp, self.targets.is_empty()) {
            return ramp.rates(base, p, t, dt, shift);
        }
        let short = self
            .targets
            .get("short_rate")
            .or_else(|| self.targets.get("policy_rate"))
            .ok_or("forecast missing short rate")?;
        if self.ramp.is_some() {
            return Err("forecast ramp and targets are mutually exclusive".into());
        }
        for values in self.targets.values() {
            if values.len() != t {
                return Err("forecast grid mismatch".into());
            }
            finite(values)?;
        }
        let mut out = base.clone();
        recenter(&mut out.short, short, p, t, shift)?;
        for i in 0..p {
            let mut df = 1.;
            for m in 0..t {
                df /= 1. + out.short[i * t + m] * dt;
                out.df[i * t + m] = df;
            }
        }
        let short0 = (0..p).map(|i| base.short[i * t]).sum::<f64>() / p as f64;
        let mut anchors = vec![(0.25, short)];
        for (tenor, key) in [(5., "rate_5y"), (10., "rate_10y")] {
            if let Some(v) = self.targets.get(key) {
                anchors.push((tenor, v));
            }
        }
        for (j, tenor) in [2., 5., 10., 30.].iter().enumerate() {
            let slope =
                (0..p).map(|i| base.swaps[(i * 4 + j) * t]).sum::<f64>() / p as f64 - short0;
            let target: Vec<_> = (0..t)
                .map(|m| {
                    if anchors.len() == 1 {
                        return short[m] + slope;
                    }
                    let index = anchors.partition_point(|(x, _)| x <= tenor);
                    if index == anchors.len() {
                        return anchors[index - 1].1[m];
                    }
                    if index == 0 {
                        return anchors[0].1[m];
                    }
                    let (a, lo) = anchors[index - 1];
                    let (b, hi) = anchors[index];
                    lo[m] + (hi[m] - lo[m]) * (tenor - a) / (b - a)
                })
                .collect();
            let mut column: Vec<_> = (0..p)
                .flat_map(|i| {
                    base.swaps[(i * 4 + j) * t..(i * 4 + j + 1) * t]
                        .iter()
                        .copied()
                })
                .collect();
            recenter(&mut column, &target, p, t, shift)?;
            for i in 0..p {
                out.swaps[(i * 4 + j) * t..(i * 4 + j + 1) * t]
                    .copy_from_slice(&column[i * t..(i + 1) * t]);
            }
        }
        finite(&out.df)?;
        finite(&out.swaps)?;
        Ok(out)
    }
    pub(crate) fn mortgage(
        &self,
        base: &MortgagePaths,
        p: usize,
        t: usize,
        dt: f64,
        shift: f64,
        lag: usize,
    ) -> Result<MortgagePaths, String> {
        let mut out = base.clone();
        out.rates = self.rates(&base.rates, p, t, dt, shift)?;
        if let Some(ramp) = &self.ramp {
            // Mortgage rates move with the curve at zero basis, keeping the incentive lag.
            for i in 0..p {
                for m in lag..t {
                    out.mtg[i * t + m] += ramp.at(m - lag);
                }
            }
        }
        if let Some(target) = self.targets.get("mortgage_rate") {
            for m in 0..t {
                let mean = (0..p).map(|i| base.mtg[i * t + m]).sum::<f64>() / p as f64;
                let level = if m < lag { mean } else { target[m - lag] };
                for i in 0..p {
                    out.mtg[i * t + m] = base.mtg[i * t + m] - mean + level;
                }
            }
        }
        if let Some(target) = self.targets.get("hpi") {
            recenter(&mut out.hpi, target, p, t, 0.)?;
            for i in 0..p {
                for m in 0..t {
                    out.yoy[i * t + m] = out.hpi[i * t + m]
                        / if m >= 12 { out.hpi[i * t + m - 12] } else { 1. }
                        - 1.;
                }
            }
        }
        finite(&out.mtg)?;
        finite(&out.hpi)?;
        finite(&out.yoy)?;
        Ok(out)
    }
}
