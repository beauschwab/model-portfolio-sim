//! Raw forward program construction, shared market paths and report aggregation.
use crate::lifecycle_market::{admit, finite, matrix, vector, MarketInput, RateMarket};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub name: String,
    pub product: Option<String>,
    pub side: String,
    pub rate_ref: String,
    #[serde(default)]
    pub is_float: bool,
    pub spread_bp: f64,
    pub term_m: usize,
    #[serde(default = "bullet")]
    pub amort: String,
    #[serde(default = "cpr")]
    pub cpr_annual: f64,
    pub start_m: usize,
    pub end_m: usize,
    pub monthly_notional: Option<f64>,
    pub reinvest_frac: Option<f64>,
    pub reinvest_source: Option<String>,
}
fn bullet() -> String {
    "bullet".into()
}
fn cpr() -> f64 {
    0.06
}
impl Program {
    pub fn evaluate(
        &self,
        reference: &[f64],
        paths: usize,
        months: usize,
        horizon: usize,
        runoff: Option<&[f64]>,
    ) -> Result<Vec<Vec<f64>>, String> {
        if paths == 0
            || !(1..=360).contains(&horizon)
            || horizon > months
            || reference.len() != admit(&[paths, months], 128 * 1024 * 1024)?
            || !(1..=4096).contains(&self.term_m)
            || self.end_m < self.start_m
            || !matches!(self.side.as_str(), "asset" | "liability")
            || !matches!(self.amort.as_str(), "bullet" | "annuity" | "cpr")
            || !matches!(
                self.rate_ref.as_str(),
                "short" | "s2" | "s5" | "s10" | "s30"
            )
            || !self.spread_bp.is_finite()
            || !self.cpr_annual.is_finite()
            || !(0. ..=1.).contains(&self.cpr_annual)
        {
            return Err("invalid forward program".into());
        }
        finite(reference)?;
        let mut amounts = vec![0.; horizon];
        if let Some(n) = self.monthly_notional {
            if !n.is_finite() || n < 0. {
                return Err("invalid monthly notional".into());
            }
            for value in amounts
                .iter_mut()
                .take(self.end_m.saturating_add(1))
                .skip(self.start_m)
            {
                *value = n;
            }
        } else {
            let fraction = self.reinvest_frac.ok_or("missing reinvest fraction")?;
            let runoff = runoff.ok_or("missing reinvest runoff")?;
            if !fraction.is_finite() || fraction < 0. || runoff.len() < horizon {
                return Err("invalid reinvestment".into());
            }
            finite(runoff)?;
            for h in self.start_m..self.end_m.saturating_add(1).min(horizon) {
                amounts[h] = fraction * runoff[h];
            }
        }
        let smm = 1. - (1. - self.cpr_annual).powf(1. / 12.);
        let factors: Vec<_> = (0..self.term_m)
            .map(|k| match self.amort.as_str() {
                "bullet" => 1.,
                "annuity" => 1. - k as f64 / self.term_m as f64,
                _ => (1. - smm).powf(k as f64),
            })
            .collect();
        Ok(crate::quant::program(&[
            matrix(reference, paths, months),
            vector(&amounts),
            vector(&factors),
            vector(&[self.spread_bp * 1e-4]),
            vector(&[f64::from(self.is_float)]),
            vector(&[if self.side == "asset" { 1. } else { -1. }]),
        ]))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRequest {
    pub market: MarketInput,
    pub programs: Vec<Program>,
    pub horizon: usize,
    #[serde(default)]
    pub runoff: BTreeMap<String, Vec<f64>>,
}
#[derive(Serialize)]
pub struct ProgramResult {
    pub names: Vec<String>,
    pub income: Vec<Vec<f64>>,
    pub balances: Vec<Vec<f64>>,
    pub dv01: Vec<Vec<f64>>,
    pub net: Vec<f64>,
    pub summary: [f64; 4],
}
impl ProgramRequest {
    pub fn run(&self) -> Result<ProgramResult, String> {
        if !(1..=360).contains(&self.horizon) || self.programs.len() > 4096 {
            return Err("invalid program result dimensions".into());
        }
        admit(&[self.programs.len(), self.horizon, 3], 16 * 1024 * 1024)?;
        let mut names = HashSet::new();
        if self.programs.iter().any(|p| {
            p.name.is_empty()
                || matches!(p.name.as_str(), "month" | "net")
                || !names.insert(&p.name)
        }) {
            return Err("duplicate or reserved program name".into());
        }
        let market = RateMarket::new(&self.market)?;
        let paths = market.paths(&self.market.swap_rates, &market.abcd)?;
        let (count, months) = (self.market.config.paths, self.market.config.months);
        let mut out = ProgramResult {
            names: Vec::new(),
            income: Vec::new(),
            balances: Vec::new(),
            dv01: Vec::new(),
            net: vec![0.; self.horizon],
            summary: [0.; 4],
        };
        let mut total_balance = vec![0.; self.horizon];
        let mut total_dv = vec![0.; self.horizon];
        for p in &self.programs {
            let mut reference = Vec::new();
            let rates = if p.rate_ref == "short" {
                &paths.short
            } else {
                let index = match p.rate_ref.as_str() {
                    "s2" => 0,
                    "s5" => 1,
                    "s10" => 2,
                    "s30" => 3,
                    _ => return Err("unknown program rate reference".into()),
                };
                reference.reserve(count * months);
                for path in 0..count {
                    reference.extend_from_slice(
                        &paths.swaps[(path * 4 + index) * months..(path * 4 + index + 1) * months],
                    );
                }
                &reference
            };
            let values = p.evaluate(
                rates,
                count,
                months,
                self.horizon,
                p.reinvest_source
                    .as_ref()
                    .and_then(|k| self.runoff.get(k).map(Vec::as_slice)),
            )?;
            let sign = if p.side == "asset" { 1. } else { -1. };
            let income: Vec<_> = values[0].iter().map(|v| sign * v).collect();
            for m in 0..self.horizon {
                out.net[m] += income[m];
                total_balance[m] += values[1][m];
                total_dv[m] += values[2][m];
            }
            out.names.push(p.name.clone());
            out.income.push(income);
            out.balances.push(values[1].clone());
            out.dv01.push(values[2].clone());
        }
        let total = out.net.iter().sum();
        out.summary = [
            total,
            total * 12. / self.horizon as f64,
            total_balance.into_iter().fold(0., f64::max),
            total_dv[self.horizon - 1],
        ];
        Ok(out)
    }
}
