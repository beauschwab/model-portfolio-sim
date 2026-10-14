//! Capital ratio and FTP management accounting. Eligibility and requirements are
//! explicit, versioned inputs; this is not a regulatory eligibility calculator.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub minimum: f64,
    pub buffer: f64,
    pub management: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capital {
    pub entity: String,
    pub currency: String,
    pub scenario: String,
    pub period: String,
    /// Eligible amounts after instrument-level eligibility adjustments.
    pub common_equity: f64,
    pub retained_earnings: f64,
    pub eligible_aoci: f64,
    pub cet1_deductions: f64,
    pub additional_tier1: f64,
    pub tier2: f64,
    pub eligible_tlac: f64,
    pub eligible_ltd: f64,
    pub credit_rwa: f64,
    pub market_rwa: f64,
    pub operational_rwa: f64,
    pub advanced_rwa: Option<f64>,
    pub average_assets: f64,
    pub total_leverage_exposure: f64,
    pub tangible_common_equity: f64,
    pub tangible_assets: f64,
    pub requirements: BTreeMap<String, Requirement>,
}
pub const METRICS: [&str; 13] = [
    "cet1",
    "tier1",
    "total_capital",
    "advanced_cet1",
    "advanced_tier1",
    "advanced_total_capital",
    "tier1_leverage",
    "slr",
    "tlac_rwa",
    "tlac_leverage",
    "ltd_rwa",
    "ltd_leverage",
    "tce_ta",
];
pub type CapitalComponent = (&'static str, f64, Option<f64>);
impl Capital {
    pub fn key(&self) -> (&str, &str, &str, &str) {
        (&self.entity, &self.currency, &self.scenario, &self.period)
    }
    pub fn components(&self) -> Result<Vec<CapitalComponent>, String> {
        labels(&[&self.entity, &self.currency, &self.scenario, &self.period])?;
        numbers(
            &[
                self.common_equity,
                self.retained_earnings,
                self.eligible_aoci,
                self.tangible_common_equity,
            ],
            false,
        )?;
        numbers(
            &[
                self.cet1_deductions,
                self.additional_tier1,
                self.tier2,
                self.eligible_tlac,
                self.eligible_ltd,
                self.credit_rwa,
                self.market_rwa,
                self.operational_rwa,
                self.average_assets,
                self.total_leverage_exposure,
                self.tangible_assets,
            ],
            true,
        )?;
        if let Some(x) = self.advanced_rwa {
            numbers(&[x], true)?;
        }
        if self
            .requirements
            .keys()
            .any(|k| !METRICS.contains(&k.as_str()))
        {
            return Err("unknown capital requirement metric".into());
        }
        for r in self.requirements.values() {
            numbers(&[r.minimum, r.buffer, r.management], true)?;
            if r.minimum + r.buffer + r.management > 1. {
                return Err(
                    "capital requirements must be decimal fractions with total <= 1".into(),
                );
            }
        }
        let cet1 =
            self.common_equity + self.retained_earnings + self.eligible_aoci - self.cet1_deductions;
        let tier1 = cet1 + self.additional_tier1;
        let total = tier1 + self.tier2;
        let rwa = Some(self.credit_rwa + self.market_rwa + self.operational_rwa);
        // TLAC/LTD inputs are already eligible totals, not increments to CET1.
        Ok(vec![
            ("cet1", cet1, rwa),
            ("tier1", tier1, rwa),
            ("total_capital", total, rwa),
            ("advanced_cet1", cet1, self.advanced_rwa),
            ("advanced_tier1", tier1, self.advanced_rwa),
            ("advanced_total_capital", total, self.advanced_rwa),
            ("tier1_leverage", tier1, Some(self.average_assets)),
            ("slr", tier1, Some(self.total_leverage_exposure)),
            ("tlac_rwa", self.eligible_tlac, rwa),
            (
                "tlac_leverage",
                self.eligible_tlac,
                Some(self.total_leverage_exposure),
            ),
            ("ltd_rwa", self.eligible_ltd, rwa),
            (
                "ltd_leverage",
                self.eligible_ltd,
                Some(self.total_leverage_exposure),
            ),
            (
                "tce_ta",
                self.tangible_common_equity,
                Some(self.tangible_assets),
            ),
        ])
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Curve {
    pub id: String,
    pub entity: String,
    pub currency: String,
    pub as_of: String,
    pub tenors: Vec<f64>,
    pub reference_rates: Vec<f64>,
    pub liquidity_spreads: Vec<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub id: String,
    pub loan_id: Option<String>,
    pub cohort_id: Option<String>,
    pub entity: String,
    pub currency: String,
    pub scenario: String,
    pub period: String,
    pub curve_id: String,
    pub side: String,
    pub average_balance: f64,
    pub accrual_fraction: f64,
    pub repricing_years: f64,
    pub funding_years: f64,
    pub external_interest: f64,
    pub fees: f64,
    pub operating_cost: f64,
    pub expected_loss: f64,
    pub allocated_capital: f64,
    pub cost_of_capital: f64,
    pub option_spread: f64,
    pub contingent_liquidity_charge: f64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub as_of: String,
    pub policy_id: String,
    pub capital: Vec<Capital>,
    pub curves: Vec<Curve>,
    pub positions: Vec<Position>,
    #[serde(default)]
    pub strategy_units: Vec<StrategyUnit>,
    #[serde(default)]
    pub capital_deltas: Vec<CapitalDelta>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyUnit {
    pub template: String,
    pub h: usize,
    pub side: f64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapitalDelta {
    pub snapshot: usize,
    pub metric: String,
    /// Index in the strategy scenario list; explicit rather than guessed by name.
    pub scenario: usize,
    pub month: usize,
    pub numerator_per_unit: Vec<f64>,
    pub denominator_per_unit: Vec<f64>,
    pub include_management_buffer: bool,
}
fn labels(values: &[&str]) -> Result<(), String> {
    if values.iter().any(|x| x.trim().is_empty() || x.len() > 200) {
        return Err("identifiers must contain 1..200 characters".into());
    }
    Ok(())
}
fn numbers(values: &[f64], nonnegative: bool) -> Result<(), String> {
    if values
        .iter()
        .any(|x| !x.is_finite() || x.abs() > 1e18 || (nonnegative && *x < 0.))
    {
        return Err("invalid financial amount or rate".into());
    }
    Ok(())
}
fn interpolate(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    let i = xs.partition_point(|v| *v < x);
    if i == 0 {
        ys[0]
    } else if i == xs.len() {
        ys[i - 1]
    } else {
        ys[i - 1] + (ys[i] - ys[i - 1]) * (x - xs[i - 1]) / (xs[i] - xs[i - 1])
    }
}
impl Request {
    pub fn run(&self) -> Result<Value, String> {
        labels(&[&self.as_of, &self.policy_id])?;
        if self.capital.len() > 10000 || self.curves.len() > 1000 || self.positions.len() > 60000 {
            return Err(
                "treasury admission: maximum 10000 capital snapshots, 1000 curves, 60000 FTP rows"
                    .into(),
            );
        }
        if self.strategy_units.len() > 1024 || self.capital_deltas.len() > 2048 {
            return Err("capital coefficient admission exceeded".into());
        }
        let mut unit_keys = BTreeSet::new();
        for u in &self.strategy_units {
            labels(&[&u.template])?;
            if u.h >= 120 || ![-1., 1.].contains(&u.side) || !unit_keys.insert((&u.template, u.h)) {
                return Err("invalid strategy unit grid".into());
            }
        }
        let mut capital_rows = Vec::new();
        let mut capital_keys = BTreeSet::new();
        for c in &self.capital {
            if !capital_keys.insert(c.key()) {
                return Err("duplicate capital snapshot".into());
            }
            for (metric, numerator, denominator) in c.components()? {
                let denominator = denominator.filter(|d| *d > 0.);
                let r = c.requirements.get(metric);
                let required = r.map(|r| r.minimum + r.buffer);
                let target = r.map(|r| r.minimum + r.buffer + r.management);
                let ratio = denominator.map(|d| numerator / d);
                if ratio.is_some_and(|v| !v.is_finite() || !(v * 10000.).is_finite()) {
                    return Err("capital ratio overflow".into());
                }
                let headroom = denominator.zip(required).map(|(d, r)| numerator - r * d);
                let target_headroom = denominator.zip(target).map(|(d, r)| numerator - r * d);
                capital_rows.push(json!({"entity":c.entity,"currency":c.currency,"scenario":c.scenario,
                    "period":c.period,"metric":metric,"numerator":numerator,"denominator":denominator,
                    "ratio":ratio,"minimum":r.map(|r|r.minimum),"buffer":r.map(|r|r.buffer),
                    "required_ratio":required,"management_target":target,"headroom":headroom,
                    "management_headroom":target_headroom,"headroom_bp":ratio.zip(required).map(|(v,r)|(v-r)*10000.),
                    "status":if denominator.is_none(){"unavailable"} else if r.is_none(){"unconfigured"}
                        else if headroom.unwrap()<0.{"breach"}else if target_headroom.unwrap()<0.{"below_management_target"}else{"pass"}}));
            }
        }
        let mut capital_limits = Vec::new();
        let mut limit_keys = BTreeSet::new();
        for (i, d) in self.capital_deltas.iter().enumerate() {
            if self.strategy_units.is_empty()
                || d.month >= 120
                || d.scenario >= 13
                || d.numerator_per_unit.len() != self.strategy_units.len()
                || d.denominator_per_unit.len() != self.strategy_units.len()
                || !limit_keys.insert((d.snapshot, &d.metric, d.scenario, d.month))
            {
                return Err("invalid capital coefficient shape or duplicate limit".into());
            }
            numbers(&d.numerator_per_unit, false)?;
            numbers(&d.denominator_per_unit, true)?;
            let c = self
                .capital
                .get(d.snapshot)
                .ok_or("unknown capital snapshot")?;
            // The current strategy library is USD. Reporting itself is currency-scoped.
            if c.currency != "USD" {
                return Err("strategy capital coefficients currently require USD".into());
            }
            let (_, n, den) = c
                .components()?
                .into_iter()
                .find(|(m, _, _)| *m == d.metric)
                .ok_or("unknown capital metric")?;
            let den = den
                .filter(|v| *v > 0.)
                .ok_or("capital limit requires a positive observed denominator")?;
            let r = c
                .requirements
                .get(&d.metric)
                .ok_or("capital limit requires an explicit requirement")?;
            capital_limits.push(json!({"label":format!("{i}:{}",d.metric),"policy_id":self.policy_id,
                "metric":d.metric,"scenario":d.scenario,"month":d.month,"units":self.strategy_units,
                "numerator":n,"denominator":den,
                "required_ratio":r.minimum+r.buffer+if d.include_management_buffer{r.management}else{0.},
                "numerator_per_unit":d.numerator_per_unit,"denominator_per_unit":d.denominator_per_unit}));
        }
        let mut curves = BTreeMap::new();
        for c in &self.curves {
            labels(&[&c.id, &c.entity, &c.currency, &c.as_of])?;
            if c.as_of != self.as_of {
                return Err("FTP curve vintage must equal request as_of".into());
            }
            if c.tenors.is_empty()
                || c.tenors.len() > 256
                || c.tenors.len() != c.reference_rates.len()
                || c.tenors.len() != c.liquidity_spreads.len()
                || c.tenors.windows(2).any(|w| w[1] <= w[0])
            {
                return Err(
                    "FTP curve requires matching arrays and strictly increasing tenors".into(),
                );
            }
            numbers(&c.tenors, true)?;
            numbers(&c.reference_rates, false)?;
            numbers(&c.liquidity_spreads, false)?;
            if curves.insert(&c.id, c).is_some() {
                return Err("duplicate FTP curve ID".into());
            }
        }
        let mut positions = Vec::new();
        let mut keys = BTreeSet::new();
        let mut totals: BTreeMap<(&str, &str, &str, &str), [f64; 4]> = BTreeMap::new();
        for p in &self.positions {
            labels(&[
                &p.id,
                &p.entity,
                &p.currency,
                &p.scenario,
                &p.period,
                &p.curve_id,
            ])?;
            for label in [&p.loan_id, &p.cohort_id].into_iter().flatten() {
                labels(&[label])?;
            }
            if !keys.insert((&p.id, &p.entity, &p.currency, &p.scenario, &p.period)) {
                return Err("duplicate FTP position key".into());
            }
            numbers(
                &[
                    p.average_balance,
                    p.accrual_fraction,
                    p.repricing_years,
                    p.funding_years,
                    p.operating_cost,
                    p.expected_loss,
                    p.allocated_capital,
                    p.cost_of_capital,
                    p.option_spread,
                    p.contingent_liquidity_charge,
                ],
                true,
            )?;
            numbers(&[p.external_interest, p.fees], false)?;
            if p.accrual_fraction <= 0.
                || p.accrual_fraction > 1.
                || p.cost_of_capital > 1.
                || p.option_spread > 1.
            {
                return Err("FTP accrual_fraction must be in (0,1], cost/option rates <=1".into());
            }
            let sign = match p.side.as_str() {
                "asset" => 1.,
                "liability" => -1.,
                _ => return Err("FTP side must be asset or liability".into()),
            };
            let c = curves.get(&p.curve_id).ok_or("unknown FTP curve")?;
            if c.entity != p.entity || c.currency != p.currency {
                return Err("FTP curve entity/currency mismatch".into());
            }
            let reference = interpolate(p.repricing_years, &c.tenors, &c.reference_rates);
            let liquidity = interpolate(p.funding_years, &c.tenors, &c.liquidity_spreads);
            let basis = p.average_balance * p.accrual_fraction;
            let reference_charge = sign * basis * reference;
            let liquidity_charge = sign * basis * liquidity;
            let option_charge = basis * p.option_spread;
            let charge =
                reference_charge + liquidity_charge + option_charge + p.contingent_liquidity_charge;
            let external = p.external_interest + p.fees - p.operating_cost - p.expected_loss;
            let profit = external - charge;
            let capital_cost = p.allocated_capital * p.cost_of_capital * p.accrual_fraction;
            numbers(&[charge, external, profit, capital_cost], false)?;
            let raroc = if p.allocated_capital > 0. {
                Some(profit / p.allocated_capital / p.accrual_fraction)
            } else {
                None
            };
            if raroc.is_some_and(|v| !v.is_finite()) {
                return Err("FTP RAROC overflow".into());
            }
            positions.push(json!({"id":p.id,"loan_id":p.loan_id,"cohort_id":p.cohort_id,"entity":p.entity,
                "currency":p.currency,"scenario":p.scenario,"period":p.period,"curve_id":p.curve_id,
                "reference_rate":reference,"liquidity_spread":liquidity,"reference_charge":reference_charge,
                "liquidity_charge":liquidity_charge,"option_charge":option_charge,
                "contingent_liquidity_charge":p.contingent_liquidity_charge,"ftp_charge":charge,
                "external_profit":external,"business_profit":profit,"capital_cost":capital_cost,
                "economic_profit":profit-capital_cost,
                "raroc":raroc,
                "flat_tail_used":p.repricing_years<c.tenors[0]||p.funding_years<c.tenors[0]
                    ||p.repricing_years>*c.tenors.last().unwrap()||p.funding_years>*c.tenors.last().unwrap()}));
            let t = totals
                .entry((&p.entity, &p.currency, &p.scenario, &p.period))
                .or_default();
            t[0] += external;
            t[1] += profit;
            t[2] += charge;
            t[3] += capital_cost;
        }
        let mut reconciliation = Vec::new();
        for ((entity, currency, scenario, period), v) in totals {
            numbers(&v, false)?;
            let error = v[1] + v[2] - v[0];
            if error.abs() > 1e-8 * v[0].abs().max(1.) {
                return Err("FTP treasury elimination failed".into());
            }
            reconciliation.push(
                json!({"entity":entity,"currency":currency,"scenario":scenario,"period":period,
                "external_profit":v[0],"business_profit":v[1],"treasury_profit":v[2],
                "consolidated_profit":v[1]+v[2],"capital_cost":v[3],"elimination_error":error}),
            );
        }
        Ok(
            json!({"schema":"treasury-1","as_of":self.as_of,"policy_id":self.policy_id,
            "capital_metrics":capital_rows,"capital_limits":capital_limits,"ftp_positions":positions,"ftp_reconciliation":reconciliation,
            "scope":"Explicit eligible capital and scenario snapshots; pretax management FTP; no regulatory eligibility certification or journal posting",
            "interpolation":"linear rates with disclosed flat tails; separate repricing and behavioral funding tenors"}),
        )
    }
}
