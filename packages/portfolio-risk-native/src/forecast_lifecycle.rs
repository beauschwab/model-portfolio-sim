//! Conditional income-only paths. These must never enter risk/OAS valuation.
use crate::lifecycle_market::finite;
use crate::market::{MortgagePaths, RatePaths};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Forecast {
    pub targets: BTreeMap<String, Vec<f64>>,
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
        let short = self
            .targets
            .get("short_rate")
            .or_else(|| self.targets.get("policy_rate"))
            .ok_or("forecast missing short rate")?;
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
