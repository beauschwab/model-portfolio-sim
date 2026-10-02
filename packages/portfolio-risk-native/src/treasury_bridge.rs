//! Source-to-report preparation. Inputs are ledger balances and captured product
//! flows, not guessed regulatory exposures. Eligibility adjustments remain explicit.
use crate::treasury::{Capital, Position, Request as Treasury};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Ledger {
        treasury: Box<Treasury>,
        trial_balance: Vec<Trial>,
        policies: Vec<LedgerPolicy>,
        amount_multiplier: f64,
    },
    Cashflows {
        treasury: Box<Treasury>,
        openings: Vec<Opening>,
        cashflows: Vec<Flow>,
        mappings: Vec<FlowPolicy>,
        horizon: usize,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trial {
    pub scenario: String,
    pub account: String,
    pub gl_account: String,
    pub instrument_id: String,
    pub balance: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerPolicy {
    pub account: String,
    pub scenario: String,
    pub capital: Capital,
    pub include_aoci: bool,
    /// Book equity not eligible as common equity (including preferred equity).
    pub equity_exclusions: f64,
    pub intangible_assets: f64,
    pub leverage_addon: f64,
    pub leverage_deductions: f64,
    pub gl_risk_weights: BTreeMap<String, f64>,
    pub instrument_risk_weights: BTreeMap<String, f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub book: String,
    pub id: String,
    pub balance: f64,
    pub book_adjustment: f64,
    pub side: String,
    pub market_price: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub book: String,
    pub id: String,
    pub month: usize,
    pub principal: f64,
    pub cash_interest: f64,
    pub accrual_interest: f64,
    pub book_amortization: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowPolicy {
    pub book: String,
    pub id: String,
    pub entity: String,
    pub currency: String,
    pub scenario: String,
    pub curve_id: String,
    pub loan_id: Option<String>,
    pub cohort_id: Option<String>,
    /// None means fixed-rate funding WAL. Otherwise explicit contractual/behavioral reset.
    pub repricing_years: Option<f64>,
    /// Remaining principal pays this many years AFTER the last captured month.
    pub residual_tail_years: Option<f64>,
    pub operating_cost_rate: f64,
    pub expected_loss_rate: f64,
    pub capital_ratio: f64,
    pub cost_of_capital: f64,
    pub option_spread: f64,
    pub annual_fee_rate: f64,
    pub contingent_charge_rate: f64,
}
fn check(v: f64, nonnegative: bool) -> Result<(), String> {
    if !v.is_finite() || v.abs() > 1e18 || nonnegative && v < 0. {
        Err("invalid bridge amount".into())
    } else {
        Ok(())
    }
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 200 {
        Err("invalid bridge identifier".into())
    } else {
        Ok(())
    }
}
const ASSET_GLS: &[&str] = &[
    "cash",
    "restricted_cash",
    "asset_principal",
    "book_adjustment",
    "allowance",
    "accrued_interest",
    "fair_value_adjustment",
    "recovery_receivable",
    "derivative_value",
    "posted_margin",
    "intercompany_receivable",
];
const LIAB_GLS: &[&str] = &[
    "funding_principal",
    "secured_funding",
    "received_margin",
    "intercompany_payable",
];
impl Request {
    pub fn run(self) -> Result<Value, String> {
        match self {
            Self::Ledger {
                treasury,
                trial_balance,
                policies,
                amount_multiplier,
            } => ledger(*treasury, trial_balance, policies, amount_multiplier),
            Self::Cashflows {
                treasury,
                openings,
                cashflows,
                mappings,
                horizon,
            } => flows(*treasury, openings, cashflows, mappings, horizon),
        }
    }
}
fn ledger(
    mut t: Treasury,
    mut rows: Vec<Trial>,
    policies: Vec<LedgerPolicy>,
    amount_multiplier: f64,
) -> Result<Value, String> {
    check(amount_multiplier, true)?;
    if amount_multiplier == 0. {
        return Err("ledger amount multiplier must be positive".into());
    }
    for r in &mut rows {
        r.balance *= amount_multiplier;
        check(r.balance, false)?;
    }
    if rows.len() > 250000
        || policies.is_empty()
        || policies.len() > 1000
        || !t.capital.is_empty()
        || !t.capital_deltas.is_empty()
    {
        return Err("ledger bridge admission or nonempty capital inputs".into());
    }
    let mut rules = BTreeMap::new();
    for p in &policies {
        label(&p.account)?;
        label(&p.scenario)?;
        p.capital.components()?;
        for v in [
            p.equity_exclusions,
            p.intangible_assets,
            p.leverage_addon,
            p.leverage_deductions,
        ] {
            check(v, true)?;
        }
        for (k, v) in p.gl_risk_weights.iter().chain(&p.instrument_risk_weights) {
            label(k)?;
            check(*v, true)?;
        }
        if p.gl_risk_weights
            .keys()
            .any(|k| !ASSET_GLS.contains(&k.as_str()))
        {
            return Err("unknown asset GL risk mapping".into());
        }
        if rules
            .insert((p.scenario.as_str(), p.account.as_str()), p)
            .is_some()
        {
            return Err("duplicate ledger account policy".into());
        }
    }
    let mut keys = BTreeSet::new();
    let mut funding = BTreeSet::new();
    for r in &rows {
        for s in [&r.scenario, &r.account, &r.gl_account] {
            label(s)?;
        }
        check(r.balance, false)?;
        if !rules.contains_key(&(r.scenario.as_str(), r.account.as_str())) {
            return Err("missing ledger account policy".into());
        }
        if !keys.insert((&r.scenario, &r.account, &r.gl_account, &r.instrument_id)) {
            return Err("duplicate trial balance key".into());
        }
        if r.gl_account == "funding_principal" {
            funding.insert((&r.scenario, &r.account, &r.instrument_id));
        }
    }
    // GL debit signs determine balances. Retained earnings includes all posted
    // P&L (including tax/provision); distributions reduce common equity once.
    let mut sums: BTreeMap<(&str, &str), [f64; 7]> = BTreeMap::new();
    let mut contributions = Vec::new();
    for r in &rows {
        let p = rules[&(r.scenario.as_str(), r.account.as_str())];
        let s = sums.entry((&r.scenario, &r.account)).or_default();
        let gl = r.gl_account.as_str();
        let v = r.balance;
        if gl == "opening_equity" || gl == "equity_distributions" {
            s[2] -= v;
        } else if gl.starts_with("pnl:") {
            s[3] -= v;
        } else if gl == "oci" {
            s[4] -= v;
        } else if LIAB_GLS.contains(&gl)
            || (["book_adjustment", "accrued_interest"].contains(&gl)
                && funding.contains(&(&r.scenario, &r.account, &r.instrument_id)))
            || gl == "derivative_value" && v < 0.
        {
            s[1] -= v;
        } else if ASSET_GLS.contains(&gl) {
            let weight = p
                .instrument_risk_weights
                .get(&r.instrument_id)
                .or_else(|| p.gl_risk_weights.get(gl));
            let weight = if v == 0. {
                weight.copied().unwrap_or(0.)
            } else {
                *weight.ok_or("missing asset risk-weight mapping")?
            };
            s[0] += v;
            s[5] += v * weight;
            contributions.push(json!({"scenario":r.scenario,"account":r.account,"gl_account":gl,
                "instrument_id":r.instrument_id,"balance":v,"risk_weight":weight,"rwa_contribution":v*weight}));
        } else {
            return Err(format!("unmapped ledger GL {gl}"));
        }
        s[6] += v;
    }
    if sums.is_empty()
        || policies
            .iter()
            .any(|p| !sums.contains_key(&(p.scenario.as_str(), p.account.as_str())))
    {
        return Err("empty or missing ledger account balances".into());
    }
    let mut audit = Vec::new();
    for ((scenario, account), s) in sums {
        let p = rules[&(scenario, account)];
        let equity = s[2] + s[3] + s[4];
        let tolerance = 1e-8 * s[0].abs().max(s[1].abs()).max(1.);
        if s.iter().any(|v| !v.is_finite())
            || s[6].abs() > tolerance
            || (s[0] - s[1] - equity).abs() > tolerance
            || s[0] < 0.
            || s[1] < 0.
            || s[5] < 0.
        {
            return Err("unbalanced or invalid ledger bridge".into());
        }
        let mut c = p.capital.clone();
        c.scenario = scenario.into();
        c.common_equity = s[2] - p.equity_exclusions;
        c.retained_earnings = s[3];
        c.eligible_aoci = if p.include_aoci { s[4] } else { 0. };
        c.credit_rwa = s[5];
        c.total_leverage_exposure = s[0] + p.leverage_addon - p.leverage_deductions;
        c.tangible_common_equity = equity - p.equity_exclusions - p.intangible_assets;
        c.tangible_assets = s[0] - p.intangible_assets;
        audit.push(
            json!({"scenario":scenario,"account":account,"entity":c.entity,"currency":c.currency,
            "assets":s[0],"liabilities":s[1],"book_equity":equity,"common_equity":c.common_equity,
            "retained_earnings":s[3],"book_aoci":s[4],"eligible_aoci":c.eligible_aoci,
            "equity_exclusions":p.equity_exclusions,"credit_rwa":s[5],"reconciliation_error":s[6]}),
        );
        t.capital.push(c);
    }
    let mut out = t.run()?;
    out["prepared_specification"] = serde_json::to_value(t).map_err(|e| e.to_string())?;
    out["capital_bridge"] = json!(audit);
    out["rwa_contributions"] = json!(contributions);
    out["bridge_basis"]=json!("Closing trial balance; net carrying-value risk weights supplied by policy; no automatic regulatory eligibility or consolidation");
    Ok(out)
}
fn flows(
    mut t: Treasury,
    openings: Vec<Opening>,
    rows: Vec<Flow>,
    mappings: Vec<FlowPolicy>,
    horizon: usize,
) -> Result<Value, String> {
    if horizon == 0
        || horizon > 120
        || openings.is_empty()
        || openings.len() > 60000 / horizon
        || rows.len() > 60000
        || !t.positions.is_empty()
    {
        return Err("cashflow bridge admission or nonempty FTP positions".into());
    }
    let mut policies = BTreeMap::new();
    for p in &mappings {
        for s in [
            &p.book,
            &p.id,
            &p.entity,
            &p.currency,
            &p.scenario,
            &p.curve_id,
        ] {
            label(s)?;
        }
        for v in [
            p.operating_cost_rate,
            p.expected_loss_rate,
            p.capital_ratio,
            p.cost_of_capital,
            p.option_spread,
            p.contingent_charge_rate,
        ] {
            check(v, true)?;
            if v > 1. {
                return Err("bridge rates must be decimal fractions <=1".into());
            }
        }
        check(p.annual_fee_rate, false)?;
        for v in [p.repricing_years, p.residual_tail_years]
            .into_iter()
            .flatten()
        {
            check(v, true)?;
            if v > 100. {
                return Err("FTP tenor exceeds 100 years".into());
            }
        }
        if policies
            .insert((p.book.as_str(), p.id.as_str()), p)
            .is_some()
        {
            return Err("duplicate FTP mapping".into());
        }
    }
    if policies.len() != openings.len() {
        return Err("FTP mappings must cover every opening exactly".into());
    }
    let mut schedules: BTreeMap<(&str, &str), Vec<Option<&Flow>>> = BTreeMap::new();
    for o in &openings {
        label(&o.book)?;
        label(&o.id)?;
        check(o.balance, true)?;
        check(o.book_adjustment, false)?;
        check(o.market_price, true)?;
        if !policies.contains_key(&(o.book.as_str(), o.id.as_str())) {
            return Err("missing FTP position mapping".into());
        }
        if schedules
            .insert((&o.book, &o.id), vec![None; horizon])
            .is_some()
        {
            return Err("duplicate instrument opening".into());
        }
    }
    for r in &rows {
        check(r.principal, true)?;
        for v in [r.cash_interest, r.accrual_interest, r.book_amortization] {
            check(v, false)?;
        }
        let schedule = schedules
            .get_mut(&(r.book.as_str(), r.id.as_str()))
            .ok_or("unknown cashflow instrument")?;
        if r.month == 0 || r.month > horizon || schedule[r.month - 1].replace(r).is_some() {
            return Err("invalid or duplicate cashflow month".into());
        }
    }
    let mut audit = Vec::new();
    for o in &openings {
        let key = (o.book.as_str(), o.id.as_str());
        let p = policies[&key];
        if o.book == "deposits" && p.repricing_years.is_none() {
            return Err(
                "FTP_RESET_REQUIRED: non-maturity deposits need an explicit repricing tenor".into(),
            );
        }
        let schedule = &schedules[&key];
        if schedule.iter().any(Option::is_none) {
            return Err("missing captured cashflow month".into());
        }
        let schedule: Vec<_> = schedule.iter().map(|r| r.unwrap()).collect();
        let sign = match o.side.as_str() {
            "asset" => 1.,
            "liability" => -1.,
            _ => return Err("invalid opening side".into()),
        };
        let principal: f64 = schedule.iter().map(|r| r.principal).sum();
        let tolerance = 1e-9 * o.balance.max(1.);
        if principal > o.balance + tolerance {
            return Err("captured principal exceeds opening balance".into());
        }
        let residual = (o.balance - principal).max(0.);
        if residual > tolerance && p.residual_tail_years.is_none() {
            return Err("FTP_TAIL_REQUIRED: projection does not extinguish principal".into());
        }
        let tail = p.residual_tail_years.unwrap_or(0.);
        let mut remaining = o.balance;
        let mut moment: f64 = schedule
            .iter()
            .map(|r| r.principal * r.month as f64 / 12.)
            .sum::<f64>()
            + residual * (horizon as f64 / 12. + tail);
        for (m, r) in schedule.iter().enumerate() {
            let closing = (remaining - r.principal).max(0.);
            let average = (remaining + closing) / 2.;
            let funding = if remaining > tolerance {
                (moment / remaining - m as f64 / 12.).max(0.)
            } else {
                0.
            };
            let position_id = format!("{}:{}", o.book, o.id);
            let period = format!("{}+M{}", t.as_of, m + 1);
            t.positions.push(Position {
                id: position_id.clone(),
                loan_id: p.loan_id.clone(),
                cohort_id: p.cohort_id.clone(),
                entity: p.entity.clone(),
                currency: p.currency.clone(),
                scenario: p.scenario.clone(),
                period: period.clone(),
                curve_id: p.curve_id.clone(),
                side: o.side.clone(),
                average_balance: average,
                accrual_fraction: 1. / 12.,
                repricing_years: p.repricing_years.unwrap_or(funding),
                funding_years: funding,
                external_interest: sign * (r.accrual_interest + r.book_amortization),
                fees: average * p.annual_fee_rate / 12.,
                operating_cost: average * p.operating_cost_rate / 12.,
                expected_loss: average * p.expected_loss_rate / 12.,
                allocated_capital: average * p.capital_ratio,
                cost_of_capital: p.cost_of_capital,
                option_spread: p.option_spread,
                contingent_liquidity_charge: average * p.contingent_charge_rate / 12.,
            });
            audit.push(json!({"book":o.book,"id":o.id,"entity":p.entity,"currency":p.currency,"scenario":p.scenario,
                "period":period,"month":m+1,"opening_balance":remaining,"principal":r.principal,"closing_balance":closing,
                "average_balance":average,"funding_years":funding,"residual_tail_years":p.residual_tail_years,
                "tail_principal":residual,"tail_share":if remaining>tolerance{residual/remaining}else{0.},
                "external_interest":sign*(r.accrual_interest+r.book_amortization)}));
            moment -= r.principal * r.month as f64 / 12.;
            remaining = closing;
        }
    }
    let mut out = t.run()?;
    out["prepared_specification"] = serde_json::to_value(t).map_err(|e| e.to_string())?;
    out["ftp_preparation"] = json!(audit);
    out["bridge_basis"]=json!("Captured monthly principal and effective-interest income; trapezoidal balances, remaining principal-weighted funding life, explicit residual tail and reset assumptions");
    Ok(out)
}
