//! Unit-grid raw contract construction, pricing and coefficient preparation.
use crate::accounting_lifecycle::cd_flows;
use crate::conventions::{Bdc, CalendarSpec, DayCount};
use crate::deposit_lifecycle::{
    cashflows, deposit_paths, equilibrium, Assumptions, Cohort, DepositDeck,
};
use crate::hedge_lifecycle::{bullet, smear, term_flows, term_price};
use crate::lifecycle_market::{admit, finite, monthly_price, MarketInput, RateMarket};
use crate::mortgage_input::OwnedMortgage;
use crate::random::SharedDraws;
use crate::term_deck::{Amortization, DeckRequest, Product};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub kind: String,
    #[serde(default)]
    pub spread_bp: f64,
    pub term_y: Option<f64>,
    pub is_float: Option<u8>,
    pub amort: Option<Amortization>,
    pub segment: Option<String>,
    pub side: Option<f64>,
    #[serde(default)]
    pub hqla_l2a: f64,
    #[serde(default)]
    pub outflow30: f64,
    #[serde(default)]
    pub asf: f64,
    #[serde(default)]
    pub rsf: f64,
    #[serde(default)]
    pub rwa: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NamedTemplate {
    pub name: String,
    pub template: Template,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRequest {
    pub market: MarketInput,
    pub mortgage: OwnedMortgage,
    pub templates: Vec<NamedTemplate>,
    pub grid_m: Vec<i64>,
    pub horizon: usize,
    pub asof: i32,
    pub deposit_assumptions: Assumptions,
    pub deposit_history: Vec<[f64; 2]>,
    pub withdrawal_parameters: Vec<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Unit {
    pub template: String,
    pub h: usize,
    #[serde(default)]
    pub kind: String,
    pub side: f64,
}
#[derive(Clone, Serialize)]
pub struct Coefficients {
    pub template: String,
    pub purchase_m: usize,
    pub values: Vec<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoefficientRequest {
    pub units: Vec<Unit>,
    pub templates: Vec<NamedTemplate>,
    pub horizon: usize,
    pub nii: Vec<f64>,
    pub balance: Vec<f64>,
    pub dv01: Vec<f64>,
}
impl CoefficientRequest {
    pub fn run(self) -> Result<Vec<Coefficients>, String> {
        let (n, h) = (self.units.len(), self.horizon);
        if !(1..=360).contains(&h)
            || n > 4096
            || self.nii.len() != n * h
            || self.balance.len() != n * h
            || self.dv01.len() != n
        {
            return Err("invalid coefficient dimensions".into());
        }
        admit(&[self.templates.len(), h, h, 10], 16 * 1024 * 1024)?;
        for x in [&self.nii, &self.balance, &self.dv01] {
            finite(x)?;
        }
        for (i, u) in self.units.iter().enumerate() {
            if u.h >= h
                || ![-1., 1.].contains(&u.side)
                || !self.templates.iter().any(|t| t.name == u.template)
                || self.units[..i]
                    .iter()
                    .any(|p| p.template == u.template && (p.h >= u.h || p.side != u.side))
            {
                return Err("invalid unit coefficient grid".into());
            }
        }
        let mut lib = UnitLibrary {
            units: self.units,
            nii: self.nii,
            balance: self.balance,
            dv01: self.dv01,
            horizon: h,
            runoff: vec![],
            cash_interest: vec![],
            grid_m: vec![],
            vectors: vec![],
        };
        lib.prepare(&self.templates)?;
        for v in &lib.vectors {
            finite(&v.values)?;
        }
        Ok(lib.vectors)
    }
}
#[derive(Clone, Serialize)]
pub struct UnitLibrary {
    pub units: Vec<Unit>,
    pub nii: Vec<f64>,
    pub runoff: Vec<f64>,
    pub balance: Vec<f64>,
    pub cash_interest: Vec<f64>,
    pub dv01: Vec<f64>,
    pub grid_m: Vec<usize>,
    pub horizon: usize,
    pub vectors: Vec<Coefficients>,
}
fn rounded(v: f64) -> f64 {
    format!("{v:.4}").parse().expect("finite decimal")
}
fn interpolate(x: f64, grid: &[f64], step: f64) -> f64 {
    let i = (x / step).floor().max(0.) as usize;
    if i >= grid.len() - 1 {
        return grid[grid.len() - 1];
    }
    grid[i] + (grid[i + 1] - grid[i]) * (x - i as f64 * step) / step
}
fn forward(dfs: &[f64], month: usize, tenor: f64) -> Result<f64, String> {
    if !tenor.is_finite() || !(1. ..=100.).contains(&tenor) {
        return Err("invalid unit template tenor".into());
    }
    let time = month as f64 / 12.;
    let mut sum = 0.;
    let mut last = 0.;
    for k in 1..=tenor as usize {
        last = interpolate(time + k as f64, dfs, 0.25);
        sum += last;
    }
    Ok((interpolate(time, dfs, 0.25) - last) / sum)
}
impl UnitLibrary {
    #[allow(clippy::too_many_arguments)] // Aligned unit cashflow columns and metadata.
    fn append(
        &mut self,
        template: &str,
        month: usize,
        kind: &str,
        side: f64,
        coupon: &[f64],
        cash: &[f64],
        principal: &[f64],
        dv: f64,
    ) {
        self.units.push(Unit {
            template: template.into(),
            h: month,
            kind: kind.into(),
            side,
        });
        let mut cumulative = 0.;
        for m in 0..self.horizon {
            self.nii.push(coupon[m]);
            self.cash_interest.push(cash[m]);
            self.runoff.push(principal[m]);
            cumulative += principal[m];
            let opening = 1. - cumulative + principal[m];
            self.balance.push(if kind == "mbs" {
                opening
            } else {
                opening.max(0.)
            });
        }
        self.dv01.push(dv);
    }
    fn prepare(&mut self, templates: &[NamedTemplate]) -> Result<(), String> {
        let h = self.horizon;
        for named in templates {
            let indices: Vec<_> = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| u.template == named.name)
                .map(|(i, _)| i)
                .collect();
            if indices.is_empty() {
                return Err("unit template has no priced rows".into());
            }
            for month in 0..h {
                let at = indices
                    .partition_point(|&i| self.units[i].h <= month)
                    .saturating_sub(1)
                    .min(indices.len().saturating_sub(2));
                let a = indices[at];
                let b = indices[(at + 1).min(indices.len() - 1)];
                let weight = ((month as f64 - self.units[a].h as f64)
                    / (self.units[b].h - self.units[a].h).max(1) as f64)
                    .clamp(0., 1.);
                let mix = |values: &[f64], m| {
                    (1. - weight) * values[a * h + m] + weight * values[b * h + m]
                };
                let dv = (1. - weight) * self.dv01[a] + weight * self.dv01[b];
                let side = self.units[a].side;
                let base = mix(&self.balance, 0).max(1e-12);
                let mut values = vec![0.; 10 * h];
                for m in month..h {
                    let local = m - month;
                    let balance = mix(&self.balance, local).max(0.);
                    values[m] = side * mix(&self.nii, local);
                    values[h + m] = balance;
                    values[2 * h + m] = side * dv * balance / base;
                    for (j, v) in [
                        named.template.hqla_l2a,
                        named.template.outflow30,
                        named.template.asf,
                        named.template.rsf,
                        named.template.rwa,
                    ]
                    .iter()
                    .enumerate()
                    {
                        values[(j + 3) * h + m] = v * balance;
                    }
                    values[if side > 0. { 8 * h + m } else { 9 * h + m }] = balance;
                }
                self.vectors.push(Coefficients {
                    template: named.name.clone(),
                    purchase_m: month,
                    values,
                });
            }
        }
        Ok(())
    }
}
impl UnitRequest {
    pub fn run(&self) -> Result<UnitLibrary, String> {
        self.market.validate_mortgage(&self.mortgage)?;
        let c = &self.market.config;
        let (p, t, h) = (c.paths, c.months, self.horizon);
        if h == 0 || h >= t || self.templates.is_empty() {
            return Err("invalid unit horizon or templates".into());
        }
        let mut grid: Vec<_> = self
            .grid_m
            .iter()
            .copied()
            .filter(|x| *x >= 0 && *x < h as i64)
            .map(|x| x as usize)
            .collect();
        grid.sort_unstable();
        grid.dedup();
        if grid.is_empty() {
            grid.push(0);
        }
        admit(&[self.templates.len(), h, h, 10], 16 * 1024 * 1024)?;
        for (i, named) in self.templates.iter().enumerate() {
            let v = &named.template;
            if named.name.is_empty()
                || self.templates[..i].iter().any(|x| x.name == named.name)
                || !["mbs", "corp", "cd", "deposit"].contains(&v.kind.as_str())
                || v.is_float.is_some_and(|x| x > 1)
            {
                return Err("invalid unit template".into());
            }
            finite(&[v.spread_bp, v.hqla_l2a, v.outflow30, v.asf, v.rsf, v.rwa])?;
        }
        let market = RateMarket::new(&self.market)?;
        let base = market.base()?;
        let down_rates: Vec<_> = self.market.swap_rates.iter().map(|r| r - 0.0025).collect();
        let up_rates: Vec<_> = self.market.swap_rates.iter().map(|r| r + 0.0025).collect();
        let down = market.paths(&down_rates, &market.abcd)?;
        let up = market.paths(&up_rates, &market.abcd)?;
        let mut out = UnitLibrary {
            units: vec![],
            nii: vec![],
            runoff: vec![],
            balance: vec![],
            cash_interest: vec![],
            dv01: vec![],
            grid_m: grid.clone(),
            horizon: h,
            vectors: vec![],
        };
        let mut mortgage_book = vec![];
        let mut mortgage_meta = vec![];
        for named in self.templates.iter().filter(|x| x.template.kind == "mbs") {
            for &month in &grid {
                let net = forward(&market.dfs, month, 10.)? + named.template.spread_bp * 1e-4;
                mortgage_book.extend([
                    rounded(net + 0.005),
                    rounded(net),
                    358.,
                    1.,
                    0.78,
                    1.,
                    745.,
                    3.2e5,
                    0.,
                    0.,
                    100.,
                    1.,
                    0.,
                ]);
                mortgage_meta.push((&named.name, month));
            }
        }
        if !mortgage_book.is_empty() {
            let mut request = self.mortgage.borrow();
            request.book = &mortgage_book;
            request.fixed_oas = &[];
            request.original_hpi = &[];
            request.config.base_paths = p;
            request.config.sensitivity_paths = p;
            let crate::mortgage_risk::ParallelValues { down: d, up: u, .. } =
                request.parallel_values(25.)?;
            let draws = SharedDraws::new(&self.market.seed, [p, t, c.factors])?;
            request.income_chunks(&draws, None, |first, end, coupon, principal, _, _| {
                for i in first..end {
                    let local = i - first;
                    out.append(
                        mortgage_meta[i].0,
                        mortgage_meta[i].1,
                        "mbs",
                        1.,
                        &coupon[local * t..],
                        &coupon[local * t..],
                        &principal[local * t..],
                        (d[i] - u[i]) / 50.,
                    );
                }
                Ok(())
            })?;
        }
        for kind in ["corp", "cd"] {
            let mut contracts = vec![];
            let mut metadata = vec![];
            for named in self.templates.iter().filter(|x| x.template.kind == kind) {
                let v = &named.template;
                let tenor = v.term_y.ok_or("unit term template requires maturity")?;
                for &month in &grid {
                    let reference = forward(&market.dfs, month, tenor)?;
                    let floating = v.is_float == Some(1);
                    let mut contract = bullet(
                        self.asof + (tenor * 365.25) as i32,
                        rounded(if floating {
                            v.spread_bp * 1e-4
                        } else {
                            reference + v.spread_bp * 1e-4
                        }),
                        floating,
                    );
                    if kind == "corp" {
                        contract.daycount = DayCount::Act360;
                        contract.amort_type = v.amort.unwrap_or(Amortization::Bullet);
                    } else {
                        contract.daycount = DayCount::Act365;
                        contract.freq_months = Some(0);
                        contract.penalty_months = 6.;
                        contract.channel = "retail".into();
                    }
                    contracts.push(contract);
                    metadata.push((&named.name, month));
                }
            }
            if contracts.is_empty() {
                continue;
            }
            if kind == "cd" {
                if self.withdrawal_parameters.len() != 5 {
                    return Err("invalid unit CD assumptions".into());
                }
                finite(&self.withdrawal_parameters)?;
            }
            let deck = DeckRequest {
                product: if kind == "corp" {
                    Product::Corporate
                } else {
                    Product::Cd
                },
                asof: self.asof,
                months: t,
                calendar: CalendarSpec {
                    name: "US".into(),
                    extra_holidays: vec![],
                },
                bdc: Bdc::ModifiedFollowing,
                contracts,
            }
            .build()?;
            let flow = |paths| {
                if kind == "corp" {
                    term_flows(&deck, paths, p, t)
                } else {
                    cd_flows(&deck, paths, p, t, &self.withdrawal_parameters)
                }
            };
            let base_flow = flow(&base);
            let spreads = term_price(&deck, &base_flow[0], &deck.tgt, p, true).remove(0);
            let d = term_price(&deck, &flow(&down)[0], &spreads, p, false).remove(0);
            let u = term_price(&deck, &flow(&up)[0], &spreads, p, false).remove(0);
            let ic: Vec<_> = base_flow[1].iter().map(|x| x / p as f64).collect();
            let pc: Vec<_> = base_flow[2].iter().map(|x| x / p as f64).collect();
            let coupon = smear(&deck, &ic, h, true);
            let cash = smear(&deck, &ic, h, false);
            let principal = smear(&deck, &pc, h, false);
            for i in 0..deck.n {
                out.append(
                    metadata[i].0,
                    metadata[i].1,
                    kind,
                    if kind == "corp" { 1. } else { -1. },
                    &coupon[i * h..],
                    &cash[i * h..],
                    &principal[i * h..],
                    (d[i] - u[i]) / 50.,
                );
            }
        }
        let mut cohorts = vec![];
        let mut metadata = vec![];
        let deposits: Vec<_> = self
            .templates
            .iter()
            .filter(|x| x.template.kind == "deposit")
            .collect();
        if !deposits.is_empty() {
            let history: Vec<_> = self.deposit_history.iter().flatten().copied().collect();
            let params = crate::calibration::fit_deposits(&history)?.x;
            for named in deposits {
                for &month in &grid {
                    cohorts.push(Cohort {
                        attrition_base: None,
                        attrition_amp: None,
                        attrition_slope: None,
                        attrition_gap: None,
                        balance: 1.,
                        segment: named
                            .template
                            .segment
                            .clone()
                            .ok_or("deposit template requires segment")?,
                        age_months: 1.,
                        avg_account_size: 5e4,
                        rate_paid: rounded(
                            equilibrium(&params, forward(&market.dfs, month, 1.)?)
                                + named.template.spread_bp * 1e-4,
                        ),
                        price: 97.,
                        svc_cost: 0.0015,
                    });
                    metadata.push((&named.name, month));
                }
            }
            let deck = DepositDeck::new(&cohorts, &self.deposit_assumptions)?;
            let r0 = equilibrium(
                &params,
                (0..p).map(|i| base.short[i * t]).sum::<f64>() / p as f64,
            );
            let flow = |paths: &crate::market::RatePaths| {
                let dep = deposit_paths(&paths.short, &paths.df, &params, r0, p, t);
                cashflows(
                    &deck,
                    &self.deposit_assumptions,
                    &dep,
                    r0,
                    p,
                    t,
                    &vec![0.; deck.n],
                    &[0.],
                    false,
                    None,
                )
            };
            let base_flow = flow(&base);
            let spreads =
                monthly_price(&base_flow[0], deck.n, t, p, &deck.tgt, true, -0.15).remove(0);
            let d = monthly_price(&flow(&down)[0], deck.n, t, p, &spreads, false, -0.15).remove(0);
            let u = monthly_price(&flow(&up)[0], deck.n, t, p, &spreads, false, -0.15).remove(0);
            for i in 0..deck.n {
                let coupon: Vec<_> = base_flow[5][i * t..i * t + h]
                    .iter()
                    .map(|x| x / p as f64)
                    .collect();
                let principal: Vec<_> = base_flow[1][i * t..i * t + h]
                    .iter()
                    .map(|x| x / p as f64)
                    .collect();
                out.append(
                    metadata[i].0,
                    metadata[i].1,
                    "deposit",
                    -1.,
                    &coupon,
                    &coupon,
                    &principal,
                    (d[i] - u[i]) / 50.,
                );
            }
        }
        out.prepare(&self.templates)?;
        for v in [
            &out.nii,
            &out.cash_interest,
            &out.runoff,
            &out.balance,
            &out.dv01,
        ] {
            finite(v)?;
        }
        for v in &out.vectors {
            finite(&v.values)?;
        }
        Ok(out)
    }
}
