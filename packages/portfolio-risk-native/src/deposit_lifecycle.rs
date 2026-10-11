//! Raw NMD cohorts/history -> calibration, fixed-OAS risk and forward stress.
use crate::cache::{resolve, Key};
use crate::calibration::fit_deposits;
use crate::lifecycle_market::{
    admit, finite, matrix, monthly_price, tensor, vector, MarketInput, RateMarket,
};
use crate::mortgage_risk::RiskResult;
use crate::quant::{behavioral, deposits, View};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub balance: f64,
    pub segment: String,
    pub age_months: f64,
    pub avg_account_size: f64,
    pub rate_paid: f64,
    pub price: f64,
    #[serde(default)]
    pub svc_cost: f64,
    #[serde(default)]
    pub attrition_base: Option<f64>,
    #[serde(default)]
    pub attrition_amp: Option<f64>,
    #[serde(default)]
    pub attrition_slope: Option<f64>,
    #[serde(default)]
    pub attrition_gap: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub base: f64,
    pub amp: f64,
    pub b: f64,
    pub g0: f64,
}
/// Explicit treatment of the balance left after the final simulated month
/// (`deposit-terminal-1`). See `quant::deposits` for the cash-flow formulas.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Terminal {
    /// Residual balance repaid at par in the final month (historical default).
    #[default]
    Payout,
    /// No terminal principal; the residual is excluded from the liability.
    Truncate,
    /// Closed-form run-off at the final attrition/paid/short rates.
    RunoffTail,
}
impl Terminal {
    pub const ALL: [Terminal; 3] = [Terminal::Payout, Terminal::Truncate, Terminal::RunoffTail];
    pub fn code(self) -> f64 {
        match self {
            Terminal::Payout => 0.,
            Terminal::Truncate => 1.,
            Terminal::RunoffTail => 2.,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Terminal::Payout => "payout",
            Terminal::Truncate => "truncate",
            Terminal::RunoffTail => "runoff_tail",
        }
    }
    fn is_payout(&self) -> bool {
        *self == Terminal::Payout
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assumptions {
    pub segments: BTreeMap<String, Segment>,
    pub size_x: Vec<f64>,
    pub size_y: Vec<f64>,
    pub age_knots: Vec<f64>,
    pub age_coefficients: Vec<f64>,
    pub velocity_coefficient: f64,
    pub attrition_cap: f64,
    /// Omitted (and never serialized) when `payout`, so default identities,
    /// cache keys and stored request hashes are unchanged.
    #[serde(default, skip_serializing_if = "Terminal::is_payout")]
    pub terminal: Terminal,
}
#[derive(Serialize)]
pub struct DepositDeck {
    pub base: Vec<f64>,
    pub fl_amp: Vec<f64>,
    pub fl_b: Vec<f64>,
    pub fl_g0: Vec<f64>,
    pub size_m: Vec<f64>,
    pub age0: Vec<f64>,
    pub rate_paid: Vec<f64>,
    pub svc: Vec<f64>,
    pub bal: Vec<f64>,
    pub tgt: Vec<f64>,
    pub n: usize,
}
impl Assumptions {
    fn validate(&self) -> Result<(), String> {
        for data in [
            &self.size_x,
            &self.size_y,
            &self.age_knots,
            &self.age_coefficients,
        ] {
            finite(data)?;
        }
        if !self.segments.contains_key("SAV")
            || self.size_x.len() < 2
            || self.size_x.len() != self.size_y.len()
            || self.size_x.windows(2).any(|x| x[0] >= x[1])
            || self.size_y.iter().any(|x| *x < 0.)
            || self.age_knots.len() < 2
            || self.age_coefficients.len() != 4 * (self.age_knots.len() - 1)
            || self.age_knots.windows(2).any(|x| x[0] >= x[1])
            || !self.velocity_coefficient.is_finite()
            || self.velocity_coefficient < 0.
            || !(0.0..=1.0).contains(&self.attrition_cap)
        {
            return Err("invalid deposit assumption tables".into());
        }
        for s in self.segments.values() {
            finite(&[s.base, s.amp, s.b, s.g0])?;
            if s.base < 0. || s.amp < 0. || s.b < 0. {
                return Err("invalid deposit segment".into());
            }
        }
        Ok(())
    }
}
impl DepositDeck {
    pub fn new(book: &[Cohort], assumptions: &Assumptions) -> Result<Self, String> {
        assumptions.validate()?;
        let mut deck = Self {
            base: vec![],
            fl_amp: vec![],
            fl_b: vec![],
            fl_g0: vec![],
            size_m: vec![],
            age0: vec![],
            rate_paid: vec![],
            svc: vec![],
            bal: vec![],
            tgt: vec![],
            n: book.len(),
        };
        for row in book {
            finite(&[
                row.balance,
                row.age_months,
                row.avg_account_size,
                row.rate_paid,
                row.price,
                row.svc_cost,
            ])?;
            if row.balance < 0.
                || row.age_months < 0.
                || row.avg_account_size <= 0.
                || row.price <= 0.
                || row.svc_cost < 0.
            {
                return Err("invalid deposit cohort".into());
            }
            for value in [
                row.attrition_base,
                row.attrition_amp,
                row.attrition_slope,
                row.attrition_gap,
            ]
            .into_iter()
            .flatten()
            {
                finite(&[value])?;
            }
            if [row.attrition_base, row.attrition_amp, row.attrition_slope]
                .into_iter()
                .flatten()
                .any(|v| v < 0.)
            {
                return Err("invalid deposit attrition override".into());
            }
            let s = assumptions
                .segments
                .get(&row.segment)
                .unwrap_or(&assumptions.segments["SAV"]);
            let x = &assumptions.size_x;
            let y = &assumptions.size_y;
            let size = row.avg_account_size.clamp(x[0], x[x.len() - 1]);
            let i = x.partition_point(|v| *v < size).clamp(1, x.len() - 1);
            deck.size_m
                .push(y[i - 1] + (y[i] - y[i - 1]) * (size - x[i - 1]) / (x[i] - x[i - 1]));
            deck.base.push(row.attrition_base.unwrap_or(s.base));
            deck.fl_amp.push(row.attrition_amp.unwrap_or(s.amp));
            deck.fl_b.push(row.attrition_slope.unwrap_or(s.b));
            deck.fl_g0.push(row.attrition_gap.unwrap_or(s.g0));
            deck.age0.push(row.age_months);
            deck.rate_paid.push(row.rate_paid);
            deck.svc.push(row.svc_cost);
            deck.bal.push(row.balance);
            deck.tgt.push(row.price / 100.);
        }
        Ok(deck)
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DepositRequest {
    pub book: Vec<Cohort>,
    pub assumptions: Assumptions,
    pub history: Vec<[f64; 2]>,
    pub market: MarketInput,
    #[serde(default)]
    pub fixed_oas: Vec<f64>,
    #[serde(default)]
    pub horizons: Vec<usize>,
    #[serde(default)]
    pub shocks: Vec<f64>,
}
#[derive(Serialize)]
pub struct DepositRisk {
    pub risk: RiskResult,
    pub wal: Vec<f64>,
    pub premium: Vec<f64>,
    /// Expected balance ($) left after the final month's attrition, before the
    /// terminal treatment.
    pub residual: Vec<f64>,
}
#[derive(Serialize)]
pub struct DepositStress {
    pub oas: Vec<f64>,
    pub base_value: Vec<f64>,
    pub balance: Vec<f64>,
    pub pnl: Vec<f64>,
    pub eve: Vec<f64>,
    pub aggregate_eve: Vec<f64>,
    pub aggregate_value: Vec<f64>,
    pub aggregate_balance: Vec<f64>,
    pub profile: Option<Vec<f64>>,
}
pub(crate) struct DepositPaths {
    pub rate: Vec<f64>,
    pub short: Vec<f64>,
    pub df: Vec<f64>,
    pub velocity: Vec<f64>,
}
pub(crate) fn deposit_paths(
    short: &[f64],
    df: &[f64],
    params: &[f64],
    r0: f64,
    p: usize,
    t: usize,
) -> DepositPaths {
    let rate = behavioral(&[
        vector(&[3.]),
        matrix(short, p, t),
        vector(params),
        vector(&[r0]),
    ])
    .remove(0);
    let mut velocity = vec![0.; p * t];
    for path in 0..p {
        for m in 12..t {
            velocity[path * t + m] = short[path * t + m] - short[path * t + m - 12];
        }
    }
    DepositPaths {
        rate,
        short: short.to_vec(),
        df: df.to_vec(),
        velocity,
    }
}
pub(crate) fn equilibrium(params: &[f64], ff: f64) -> f64 {
    params[0]
        + ff * (params[1] + (params[2] - params[1]) / (1. + (-params[3] * (ff - params[4])).exp()))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn cashflows(
    deck: &DepositDeck,
    a: &Assumptions,
    paths: &DepositPaths,
    r0: f64,
    p: usize,
    t: usize,
    oas: &[f64],
    horizons: &[f64],
    forward: bool,
    restart: Option<(usize, usize, &[f64])>,
) -> Vec<Vec<f64>> {
    crate::compute_span!("deposit_cashflows");
    let offsets: Vec<_> = deck.rate_paid.iter().map(|r| r - r0).collect();
    let vc = [a.velocity_coefficient];
    let ac = [a.attrition_cap];
    let want = [f64::from(forward)];
    let terminal = [a.terminal.code()];
    let mut args: Vec<View<'_>> = vec![
        matrix(&paths.rate, p, t),
        matrix(&paths.short, p, t),
        matrix(&paths.velocity, p, t),
        matrix(&paths.df, p, t),
        vector(&a.age_knots),
        vector(&a.age_coefficients),
        vector(&offsets),
        vector(&deck.base),
        vector(&deck.size_m),
        vector(&deck.fl_amp),
        vector(&deck.fl_b),
        vector(&deck.fl_g0),
        vector(&deck.age0),
        vector(&deck.svc),
        vector(&vc),
        vector(&ac),
        vector(oas),
    ];
    let hh;
    let hi;
    if let Some((h, index, balances)) = restart {
        hh = [h as f64];
        hi = [index as f64];
        args.extend([
            vector(&hh),
            vector(&hi),
            tensor(balances, [deck.n, p, horizons.len()]),
        ]);
    } else {
        args.extend([vector(horizons), vector(&want)]);
    }
    args.push(vector(&terminal));
    deposits(&args, restart.is_some())
}
impl DepositRequest {
    fn prepare(&self) -> Result<(RateMarket<'_>, Vec<f64>, f64), String> {
        if self.book.is_empty()
            || (!self.fixed_oas.is_empty() && self.fixed_oas.len() != self.book.len())
        {
            return Err("invalid deposit book or OAS shape".into());
        }
        finite(&self.fixed_oas)?;
        self.assumptions.validate()?;
        let market = RateMarket::new(&self.market)?;
        let history: Vec<_> = self.history.iter().flatten().copied().collect();
        let fit = resolve(Key::new(41).floats(&history), || {
            let fit = fit_deposits(&history)?;
            let bytes = fit.x.capacity() * 8 + std::mem::size_of_val(&fit);
            Ok((fit, bytes))
        })?;
        let base = market.base()?;
        let c = &self.market.config;
        let short0 = (0..c.paths).map(|p| base.short[p * c.months]).sum::<f64>() / c.paths as f64;
        let r0 = equilibrium(&fit.x, short0);
        Ok((market, fit.x.clone(), r0))
    }
    pub fn risk(&self) -> Result<DepositRisk, String> {
        let n = self.book.len();
        let c = &self.market.config;
        let cols = self.market.tenors.len() + self.market.vol_quotes.len() / 3;
        admit(&[n, cols + 5], 128 * 1024 * 1024)?;
        let (market, params, r0) = self.prepare()?;
        let scenarios = market.scenarios()?;
        let base = market.base()?;
        let dep = deposit_paths(&base.short, &base.df, &params, r0, c.paths, c.months);
        let mut out = DepositRisk {
            risk: RiskResult {
                oas: vec![0.; n],
                price: vec![0.; n],
                dv01: vec![0.; n],
                sensitivities: vec![0.; cols * n],
            },
            wal: vec![0.; n],
            premium: vec![0.; n],
            residual: vec![0.; n],
        };
        for first in (0..n).step_by(256) {
            let end = (first + 256).min(n);
            let count = end - first;
            let deck = DepositDeck::new(&self.book[first..end], &self.assumptions)?;
            let flows = cashflows(
                &deck,
                &self.assumptions,
                &dep,
                r0,
                c.paths,
                c.months,
                &vec![0.; count],
                &[0.],
                false,
                None,
            );
            let solve = self.fixed_oas.is_empty();
            let mut priced = monthly_price(
                &flows[0],
                count,
                c.months,
                c.paths,
                if solve {
                    &deck.tgt
                } else {
                    &self.fixed_oas[first..end]
                },
                solve,
                -0.15,
            );
            if solve {
                out.risk.oas[first..end].copy_from_slice(&priced.remove(0));
            } else {
                out.risk.oas[first..end].copy_from_slice(&self.fixed_oas[first..end]);
            }
            out.risk.price[first..end].copy_from_slice(&priced.remove(0));
            for i in 0..count {
                let runoff = &flows[1][i * c.months..(i + 1) * c.months];
                out.wal[first + i] = runoff
                    .iter()
                    .enumerate()
                    .map(|(m, v)| v * (m + 1) as f64 / 12.)
                    .sum::<f64>()
                    / runoff.iter().sum::<f64>().max(1e-12);
                out.premium[first + i] = (1. - out.risk.price[first + i]) * 100.;
                out.residual[first + i] = deck.bal[i] * flows[6][i] / c.paths as f64;
            }
            for (j, (down, up)) in scenarios.iter().enumerate() {
                let price = |rates: &[f64], abcd: &[f64]| -> Result<Vec<f64>, String> {
                    let paths = market.paths(rates, abcd)?;
                    let dep =
                        deposit_paths(&paths.short, &paths.df, &params, r0, c.paths, c.months);
                    let flows = cashflows(
                        &deck,
                        &self.assumptions,
                        &dep,
                        r0,
                        c.paths,
                        c.months,
                        &out.risk.oas[first..end],
                        &[0.],
                        false,
                        None,
                    );
                    Ok(monthly_price(
                        &flows[0],
                        count,
                        c.months,
                        c.paths,
                        &out.risk.oas[first..end],
                        false,
                        -0.15,
                    )
                    .remove(0))
                };
                let down = price(&down.rates, &down.abcd)?;
                let up = price(&up.rates, &up.abcd)?;
                for i in 0..count {
                    let value = if j < self.market.tenors.len() {
                        deck.bal[i] * (down[i] - up[i]) / 2.
                    } else {
                        deck.bal[i] * (up[i] - down[i]) / (2. * c.vol_bump) * 0.01
                    };
                    out.risk.sensitivities[j * n + first + i] = value;
                    if j < self.market.tenors.len() {
                        out.risk.dv01[first + i] += value;
                    }
                }
            }
        }
        for v in [
            &out.risk.oas,
            &out.risk.price,
            &out.risk.dv01,
            &out.risk.sensitivities,
            &out.wal,
            &out.premium,
            &out.residual,
        ] {
            finite(v)?;
        }
        Ok(out)
    }
    pub fn stress(&self) -> Result<DepositStress, String> {
        let n = self.book.len();
        let c = &self.market.config;
        let h = self.horizons.len();
        let s = self.shocks.len();
        if h == 0
            || s == 0
            || self.horizons.iter().any(|&x| x == 0 || x >= c.months)
            || self.horizons.windows(2).any(|x| x[0] >= x[1])
        {
            return Err("invalid deposit stress horizons".into());
        }
        finite(&self.shocks)?;
        if self
            .shocks
            .iter()
            .enumerate()
            .any(|(j, x)| 1. + x * 1e-4 / 12. <= 0. || self.shocks[..j].contains(x))
        {
            return Err("invalid deposit stress shocks".into());
        }
        admit(&[n, h, s + 1, 4], 128 * 1024 * 1024)?;
        let (market, params, r0) = self.prepare()?;
        let base = market.base()?;
        let dep = deposit_paths(&base.short, &base.df, &params, r0, c.paths, c.months);
        let horizons: Vec<_> = self.horizons.iter().map(|&x| x as f64).collect();
        let minus = self.shocks.iter().position(|x| *x == -100.);
        let plus = self.shocks.iter().position(|x| *x == 100.);
        let mut out = DepositStress {
            oas: vec![0.; n],
            base_value: vec![0.; h * n],
            balance: vec![0.; h * n],
            pnl: vec![0.; s * h * n],
            eve: vec![0.; s * h * n],
            aggregate_eve: vec![0.; s * h],
            aggregate_value: vec![0.; s * h],
            aggregate_balance: vec![0.; s * h],
            profile: if minus.is_some() && plus.is_some() {
                Some(vec![0.; h])
            } else {
                None
            },
        };
        let scratch = admit(&[c.paths, h], 128 * 1024 * 1024)?
            .saturating_add(c.months * 4)
            .max(1);
        let chunk = (8 * 1024 * 1024 / scratch).clamp(1, 256);
        for first in (0..n).step_by(chunk) {
            let end = (first + chunk).min(n);
            let count = end - first;
            let deck = DepositDeck::new(&self.book[first..end], &self.assumptions)?;
            let spreads = if self.fixed_oas.is_empty() {
                let flows = cashflows(
                    &deck,
                    &self.assumptions,
                    &dep,
                    r0,
                    c.paths,
                    c.months,
                    &vec![0.; count],
                    &[0.],
                    false,
                    None,
                );
                monthly_price(&flows[0], count, c.months, c.paths, &deck.tgt, true, -0.15).remove(0)
            } else {
                self.fixed_oas[first..end].to_vec()
            };
            out.oas[first..end].copy_from_slice(&spreads);
            let flows = cashflows(
                &deck,
                &self.assumptions,
                &dep,
                r0,
                c.paths,
                c.months,
                &spreads,
                &horizons,
                true,
                None,
            );
            for hi in 0..h {
                for i in 0..count {
                    out.base_value[hi * n + first + i] =
                        flows[2][i * h + hi] / c.paths as f64 * deck.bal[i];
                    out.balance[hi * n + first + i] =
                        flows[3][i * h + hi] / c.paths as f64 * deck.bal[i];
                }
            }
            for (j, shock) in self.shocks.iter().enumerate() {
                for (hi, &horizon) in self.horizons.iter().enumerate() {
                    // A zero shock is the identical base state. Replaying from
                    // rounded checkpoints would introduce spurious P&L.
                    if *shock == 0. {
                        for i in 0..count {
                            out.aggregate_value[j * h + hi] += out.base_value[hi * n + first + i];
                            out.aggregate_balance[j * h + hi] += out.balance[hi * n + first + i];
                        }
                        continue;
                    }
                    let mut short = base.short.clone();
                    let mut df = base.df.clone();
                    let d = shock * 1e-4;
                    for path in 0..c.paths {
                        for m in horizon..c.months {
                            short[path * c.months + m] += d;
                            df[path * c.months + m] *=
                                (1. + d / 12.).powf(-((m - horizon + 1) as f64));
                        }
                    }
                    let shifted = deposit_paths(&short, &df, &params, r0, c.paths, c.months);
                    let values = cashflows(
                        &deck,
                        &self.assumptions,
                        &shifted,
                        r0,
                        c.paths,
                        c.months,
                        &spreads,
                        &horizons,
                        false,
                        Some((horizon, hi, &flows[4])),
                    )
                    .remove(0);
                    for i in 0..count {
                        let value = values[i] / c.paths as f64 * deck.bal[i];
                        let index = (j * h + hi) * n + first + i;
                        // Match the reference subtraction before notional scaling.
                        let pnl = (values[i] / c.paths as f64
                            - flows[2][i * h + hi] / c.paths as f64)
                            * deck.bal[i];
                        out.pnl[index] = pnl;
                        out.eve[index] = -pnl;
                        out.aggregate_eve[j * h + hi] -= pnl;
                        out.aggregate_value[j * h + hi] += out.base_value[hi * n + first + i];
                        out.aggregate_balance[j * h + hi] += out.balance[hi * n + first + i];
                        if let Some(profile) = &mut out.profile {
                            if Some(j) == minus {
                                profile[hi] += value / 200.;
                            } else if Some(j) == plus {
                                profile[hi] -= value / 200.;
                            }
                        }
                    }
                }
            }
        }
        for values in [
            &out.oas,
            &out.base_value,
            &out.balance,
            &out.pnl,
            &out.eve,
            &out.aggregate_eve,
            &out.aggregate_value,
            &out.aggregate_balance,
        ] {
            finite(values)?;
        }
        if let Some(profile) = &out.profile {
            finite(profile)?;
        }
        Ok(out)
    }
}

/// Horizon-sensitivity report (`deposit-horizon-1`): every report horizon and
/// terminal treatment for one deposit book.
#[derive(Serialize)]
pub struct DepositHorizon {
    pub policy: &'static str,
    pub base_mode: &'static str,
    pub modes: Vec<&'static str>,
    pub horizons: Vec<usize>,
    /// OAS held fixed across horizons/modes: supplied, or solved to price at the
    /// first horizon under the configured terminal treatment.
    pub oas: Vec<f64>,
    /// [horizon][cohort] expected residual balance ($) before terminal treatment.
    pub residual: Vec<Vec<f64>>,
    /// [horizon][mode][cohort] liability PV ($) at the fixed OAS.
    pub pv: Vec<Vec<Vec<f64>>>,
    /// [horizon][mode][cohort] PV ($) of the terminal treatment, PV(mode) - PV(truncate).
    pub terminal_pv: Vec<Vec<Vec<f64>>>,
    /// [horizon][mode][cohort] OAS that would reprice the cohort to its price.
    pub implied_oas: Vec<Vec<Vec<f64>>>,
}
/// Leading `h` months of each path, so all report horizons share one draw tape.
fn leading(paths: &DepositPaths, p: usize, t: usize, h: usize) -> DepositPaths {
    let cut = |v: &[f64]| -> Vec<f64> {
        (0..p)
            .flat_map(|path| v[path * t..path * t + h].iter().copied())
            .collect()
    };
    DepositPaths {
        rate: cut(&paths.rate),
        short: cut(&paths.short),
        df: cut(&paths.df),
        velocity: cut(&paths.velocity),
    }
}
impl DepositRequest {
    /// Liability PV, horizon residual and terminal-treatment PV for each mode at
    /// each report horizon (`horizons`, months, increasing; the first is the base
    /// horizon). One rate tape is simulated at the longest horizon and every
    /// shorter horizon uses its leading months (common random numbers across
    /// horizons); with a single horizon equal to the production grid the tape
    /// and the configured-mode PV are those of `risk`.
    pub fn horizon_sensitivity(&self) -> Result<DepositHorizon, String> {
        let n = self.book.len();
        let hz = &self.horizons;
        if hz.is_empty()
            || hz.len() > 8
            || hz[0] < 2
            || hz.windows(2).any(|x| x[0] >= x[1])
            || !self.shocks.is_empty()
        {
            return Err("invalid deposit horizon report horizons".into());
        }
        let longest = hz[hz.len() - 1];
        let modes = Terminal::ALL.len();
        admit(&[n, hz.len(), modes, 3], 128 * 1024 * 1024)?;
        let mut request = self.clone();
        request.market.config.months = longest;
        let (market, params, r0) = request.prepare()?;
        let p = request.market.config.paths;
        let base = market.base()?;
        let dep = deposit_paths(&base.short, &base.df, &params, r0, p, longest);
        let tapes: Vec<_> = hz.iter().map(|&h| leading(&dep, p, longest, h)).collect();
        let configured = Terminal::ALL
            .iter()
            .position(|m| *m == self.assumptions.terminal)
            .unwrap_or(0);
        let mut out = DepositHorizon {
            policy: "deposit-terminal-1",
            base_mode: self.assumptions.terminal.name(),
            modes: Terminal::ALL.iter().map(|m| m.name()).collect(),
            horizons: hz.clone(),
            oas: vec![0.; n],
            residual: vec![vec![0.; n]; hz.len()],
            pv: vec![vec![vec![0.; n]; modes]; hz.len()],
            terminal_pv: vec![vec![vec![0.; n]; modes]; hz.len()],
            implied_oas: vec![vec![vec![0.; n]; modes]; hz.len()],
        };
        for first in (0..n).step_by(256) {
            let end = (first + 256).min(n);
            let count = end - first;
            let deck = DepositDeck::new(&self.book[first..end], &self.assumptions)?;
            let zero = vec![0.; count];
            // flows[horizon][mode] = (A, residual path sums)
            let mut flows = Vec::with_capacity(hz.len());
            for (tape, &h) in tapes.iter().zip(hz) {
                let mut row = Vec::with_capacity(modes);
                for mode in Terminal::ALL {
                    let mut a = self.assumptions.clone();
                    a.terminal = mode;
                    let mut f = cashflows(&deck, &a, tape, r0, p, h, &zero, &[0.], false, None);
                    let residual = f.swap_remove(6);
                    row.push((f.swap_remove(0), residual));
                }
                flows.push(row);
            }
            let spreads = if self.fixed_oas.is_empty() {
                monthly_price(
                    &flows[0][configured].0,
                    count,
                    hz[0],
                    p,
                    &deck.tgt,
                    true,
                    -0.15,
                )
                .remove(0)
            } else {
                self.fixed_oas[first..end].to_vec()
            };
            out.oas[first..end].copy_from_slice(&spreads);
            for (hi, &h) in hz.iter().enumerate() {
                for i in 0..count {
                    out.residual[hi][first + i] = deck.bal[i] * flows[hi][0].1[i] / p as f64;
                }
                let truncated =
                    monthly_price(&flows[hi][1].0, count, h, p, &spreads, false, -0.15).remove(0);
                for (mi, (a, _)) in flows[hi].iter().enumerate() {
                    let price = monthly_price(a, count, h, p, &spreads, false, -0.15).remove(0);
                    let implied = monthly_price(a, count, h, p, &deck.tgt, true, -0.15).remove(0);
                    for i in 0..count {
                        out.pv[hi][mi][first + i] = deck.bal[i] * price[i];
                        out.terminal_pv[hi][mi][first + i] =
                            deck.bal[i] * (price[i] - truncated[i]);
                        out.implied_oas[hi][mi][first + i] = implied[i];
                    }
                }
            }
        }
        finite(&out.oas)?;
        for hi in 0..hz.len() {
            finite(&out.residual[hi])?;
            for mi in 0..modes {
                finite(&out.pv[hi][mi])?;
                finite(&out.terminal_pv[hi][mi])?;
                finite(&out.implied_oas[hi][mi])?;
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quant::DEPOSIT_TAIL_MIN_DECAY;

    const SVC: f64 = 0.0024;

    /// One path, constant paid rate r, short s and attrition a (flat age curve,
    /// no flight/velocity) over t months; returns (A row, residual path sum).
    fn fixture(terminal: Terminal, t: usize, r: f64, s: f64, attr: f64) -> (Vec<f64>, f64) {
        let deck = DepositDeck {
            base: vec![attr],
            fl_amp: vec![0.],
            fl_b: vec![0.],
            fl_g0: vec![0.],
            size_m: vec![1.],
            age0: vec![0.],
            rate_paid: vec![r],
            svc: vec![SVC],
            bal: vec![1.],
            tgt: vec![1.],
            n: 1,
        };
        let a = Assumptions {
            segments: BTreeMap::new(),
            size_x: vec![1., 2.],
            size_y: vec![1., 1.],
            age_knots: vec![0., 10_000.],
            age_coefficients: vec![0., 0., 0., 1.],
            velocity_coefficient: 0.,
            attrition_cap: 1.,
            terminal,
        };
        // r0 = 0: the cohort offset is the paid rate and the model path is zero.
        let paths = DepositPaths {
            rate: vec![0.; t],
            short: vec![s; t],
            df: (0..t)
                .map(|m| (1. + s / 12.).powi(-(m as i32 + 1)))
                .collect(),
            velocity: vec![0.; t],
        };
        let f = cashflows(&deck, &a, &paths, 0., 1, t, &[0.], &[0.], false, None);
        (f[0].clone(), f[6][0])
    }

    fn hand(terminal: Terminal, t: usize, r: f64, s: f64, a: f64) -> f64 {
        let c = r / 12. + SVC / 12.;
        let d = |m: usize| (1. + s / 12.).powi(-(m as i32 + 1));
        let mut pv = 0.;
        for m in 0..t {
            pv += (1. - a).powi(m as i32) * (c + a) * d(m);
        }
        let resid = (1. - a).powi(t as i32);
        pv + match terminal {
            Terminal::Payout => resid * d(t - 1),
            Terminal::Truncate => 0.,
            Terminal::RunoffTail => {
                resid * (c + a) / (a + s / 12.).max(DEPOSIT_TAIL_MIN_DECAY) * d(t - 1)
            }
        }
    }

    #[test]
    fn terminal_modes_match_hand_values() {
        let (t, r, s, a) = (24, 0.015, 0.04, 0.02);
        for mode in Terminal::ALL {
            let (row, resid) = fixture(mode, t, r, s, a);
            let pv: f64 = row.iter().sum();
            assert!((pv - hand(mode, t, r, s, a)).abs() < 1e-13, "{mode:?}");
            assert!((resid - (1. - a).powi(t as i32)).abs() < 1e-14);
        }
    }

    #[test]
    fn stationary_runoff_tail_equals_long_horizon() {
        // In a stationary market the closed-form tail is exactly the continuation.
        let (r, s, a) = (0.015, 0.04, 0.02);
        let tail: f64 = fixture(Terminal::RunoffTail, 24, r, s, a).0.iter().sum();
        let long: f64 = fixture(Terminal::Truncate, 4000, r, s, a).0.iter().sum();
        assert!((tail - long).abs() < 1e-12, "{tail} {long}");
    }

    #[test]
    fn tail_decay_floor_keeps_value_finite() {
        let (row, _) = fixture(Terminal::RunoffTail, 12, 0.0, -0.01, 0.0);
        assert!(row.iter().all(|v| v.is_finite()));
        let expected = (SVC / 12.) / DEPOSIT_TAIL_MIN_DECAY * (1.0_f64 - 0.01 / 12.).powi(-12);
        assert!((row[11] - (SVC / 12.) * (1.0_f64 - 0.01 / 12.).powi(-12) - expected).abs() < 1e-9);
    }

    #[test]
    fn terminal_serde_default_and_rejection() {
        let base = r#"{"segments":{},"size_x":[1,2],"size_y":[1,1],"age_knots":[0,1],
            "age_coefficients":[0,0,0,1],"velocity_coefficient":0,"attrition_cap":0.5"#;
        let a: Assumptions = serde_json::from_str(&format!("{base}}}")).unwrap();
        assert_eq!(a.terminal, Terminal::Payout);
        // payout is never serialized, so default identities are unchanged
        assert!(!serde_json::to_string(&a).unwrap().contains("terminal"));
        let t: Assumptions =
            serde_json::from_str(&format!(r#"{base},"terminal":"runoff_tail"}}"#)).unwrap();
        assert_eq!(t.terminal, Terminal::RunoffTail);
        assert!(serde_json::to_string(&t).unwrap().contains("runoff_tail"));
        let bad = format!(r#"{base},"terminal":"par"}}"#);
        assert!(serde_json::from_str::<Assumptions>(&bad).is_err());
    }
}
