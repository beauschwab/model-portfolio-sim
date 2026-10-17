//! Source-to-report preparation. Inputs are ledger balances and captured product
//! flows, not guessed regulatory exposures. Eligibility adjustments remain explicit.
use crate::treasury::evaluate::{Capital, Position, Request as Treasury};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Accounting asset channels admitted only by the reviewed mortgage helper.
pub const MORTGAGE_PENDING_ASSET_GLS: &[&str] = &[
    "mortgage_pending_default_principal",
    "mortgage_pending_default_interest_receivable",
    "mortgage_pending_default_basis",
    "mortgage_pending_prepay_principal",
    "mortgage_pending_prepay_interest_receivable",
    "mortgage_pending_prepay_basis",
];

fn default_credit_protocol() -> u8 {
    4
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    CreditLedger {
        #[serde(default = "default_credit_protocol")]
        credit_protocol: u8,
        treasury: Box<Treasury>,
        trial_balance: Vec<Trial>,
        policies: Vec<LedgerPolicy>,
        amount_multiplier: f64,
    },
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
/// Explicit successor policy. The string wire admits no unit-enum object aliases.
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExposurePolicy {
    pub identity: portfolio_model_core::credit_accounting::Identity,
    pub negative_net_carrying: String,
}
impl ExposurePolicy {
    pub fn validate(&self) -> Result<(), String> {
        for s in [&self.identity.id, &self.identity.revision] {
            if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
                return Err("reviewed exposure policy identity required".into());
            }
        }
        if ![
            "reject_negative_exposure_groups",
            "research_signed_exposure_groups",
            "reviewed_nonnegative_group_exposure",
        ]
        .contains(&self.negative_net_carrying.as_str())
        {
            return Err("unsupported reviewed negative net carrying policy".into());
        }
        Ok(())
    }
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
/// Reviewed AOCI capital treatment: deferred tax on gross AOCI, the regulatory
/// opt-out election and a phase-in fraction are supplied policy, never inferred.
/// The DTA/DTL itself is disclosed, not posted to the bridge balance sheet.
#[derive(Deserialize, serde::Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct AociEligibility {
    pub identity: portfolio_model_core::credit_accounting::Identity,
    pub deferred_tax_rate: f64,
    pub opt_out_elected: bool,
    pub phase_in_fraction: f64,
}
impl AociEligibility {
    pub fn validate(&self) -> Result<(), String> {
        for s in [&self.identity.id, &self.identity.revision] {
            if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
                return Err("reviewed AOCI eligibility identity required".into());
            }
        }
        if !self.deferred_tax_rate.is_finite()
            || !(0. ..1.).contains(&self.deferred_tax_rate)
            || !self.phase_in_fraction.is_finite()
            || !(0. ..=1.).contains(&self.phase_in_fraction)
        {
            return Err(
                "AOCI deferred tax rate in [0,1) and phase-in fraction in [0,1] required".into(),
            );
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerPolicy {
    pub account: String,
    pub scenario: String,
    pub capital: Capital,
    pub include_aoci: bool,
    /// Cash-flow-hedge AOCI (`cash_flow_hedge_oci` rows) is excluded from eligible
    /// AOCI unless this reviewed policy flag is set, even when `include_aoci` is true.
    #[serde(default)]
    pub cash_flow_hedge_aoci_eligible: bool,
    /// Optional reviewed deferred-tax / opt-out / phase-in treatment of included AOCI.
    #[serde(default)]
    pub aoci_eligibility: Option<AociEligibility>,
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
    "dated_interest_receivable",
    "dated_prepaid_expense",
    "credit_default_principal",
    "credit_default_book_adjustment",
    "credit_default_interest_receivable",
    // Reviewed interest-right basis is a signed asset adjustment. A credit
    // balance reduces the gross right; it is not an interest payable or income.
    // Existing explicit instrument/GL policy weights apply to each signed row.
    "live_interest_basis_adjustment",
    "default_interest_basis_adjustment",
    // Formal fair-value hedge basis adjustment on an asset item (signed carrying adjustment).
    "hedged_item_basis_adjustment",
];
const LIAB_GLS: &[&str] = &[
    // Formal fair-value hedge basis adjustment on a liability item.
    "hedged_item_basis_liability",
    "funding_principal",
    "secured_funding",
    "received_margin",
    "intercompany_payable",
    "dated_interest_payable",
    "dated_deferred_income",
    "credit_default_deferred_income",
];
const CREDIT_ASSET_GLS: &[&str] = &[
    "credit_default_principal",
    "credit_default_accrued_interest",
    "credit_default_book_adjustment",
];
const CREDIT_PNL_GLS: &[&str] = &[
    "interest_income",
    "credit_loss_expense",
    "basis_recognition_expense",
    "recovery_income",
];
impl Request {
    pub fn run(self) -> Result<Value, String> {
        match self {
            Self::CreditLedger {
                credit_protocol,
                treasury,
                trial_balance,
                policies,
                amount_multiplier,
            } => {
                if [6, 7].contains(&credit_protocol) {
                    return Err(
                        "mortgage closing protocol cannot be selected on credit ledger".into(),
                    );
                }
                ledger(
                    *treasury,
                    trial_balance,
                    policies,
                    amount_multiplier,
                    credit_protocol,
                )
            }
            Self::Ledger {
                treasury,
                trial_balance,
                policies,
                amount_multiplier,
            } => ledger(*treasury, trial_balance, policies, amount_multiplier, 0),
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
pub(crate) fn ledger(
    t: Treasury,
    rows: Vec<Trial>,
    policies: Vec<LedgerPolicy>,
    amount_multiplier: f64,
    credit_protocol: u8,
) -> Result<Value, String> {
    ledger_with_exposure(t, rows, policies, amount_multiplier, credit_protocol, None)
}
pub(crate) fn ledger_with_exposure(
    mut t: Treasury,
    mut rows: Vec<Trial>,
    policies: Vec<LedgerPolicy>,
    amount_multiplier: f64,
    credit_protocol: u8,
    exposure_policy: Option<&ExposurePolicy>,
) -> Result<Value, String> {
    if ![0, 4, 5, 6, 7].contains(&credit_protocol) {
        return Err("unsupported credit ledger protocol".into());
    }
    let credit_source = [4, 5].contains(&credit_protocol);
    let mortgage_closing = credit_protocol == 6;
    let commercial_closing = credit_protocol == 7;
    if let Some(p) = exposure_policy {
        p.validate()?;
        if !mortgage_closing && !commercial_closing {
            return Err("reviewed exposure policy requires native reviewed closing".into());
        }
    }

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
        if let Some(a) = &p.aoci_eligibility {
            a.validate()?;
        }
        if p.gl_risk_weights.keys().any(|k| {
            !(ASSET_GLS.contains(&k.as_str())
                || mortgage_closing && MORTGAGE_PENDING_ASSET_GLS.contains(&k.as_str())
                || credit_source && CREDIT_ASSET_GLS.contains(&k.as_str()))
        }) {
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
    let mut sums: BTreeMap<(&str, &str), [f64; 8]> = BTreeMap::new();
    let mut contributions = Vec::new();
    let mut groups: BTreeMap<(&str, &str, &str), (f64, f64)> = BTreeMap::new();
    for r in &rows {
        let p = rules[&(r.scenario.as_str(), r.account.as_str())];
        let s = sums.entry((&r.scenario, &r.account)).or_default();
        let gl = r.gl_account.as_str();
        let v = r.balance;
        if credit_source
            && !(CREDIT_ASSET_GLS.contains(&gl)
                || CREDIT_PNL_GLS.contains(&gl)
                || [
                    "cash",
                    "asset_principal",
                    "accrued_interest",
                    "book_adjustment",
                    "allowance",
                    "equity_opening",
                ]
                .contains(&gl))
        {
            return Err(format!("unsupported credit source GL {gl}"));
        }
        if (!credit_source && (gl == "opening_equity" || gl == "equity_distributions"))
            || (credit_source && gl == "equity_opening")
        {
            s[2] -= v;
        } else if gl.starts_with("pnl:") || credit_source && CREDIT_PNL_GLS.contains(&gl) {
            s[3] -= v;
        } else if gl == "oci" {
            s[4] -= v;
        } else if gl == "cash_flow_hedge_oci" {
            s[4] -= v;
            s[7] -= v;
        } else if LIAB_GLS.contains(&gl)
            || (["book_adjustment", "accrued_interest"].contains(&gl)
                && funding.contains(&(&r.scenario, &r.account, &r.instrument_id)))
            || gl == "derivative_value" && v < 0.
            || credit_protocol == 5
                && ["accrued_interest", "credit_default_accrued_interest"].contains(&gl)
                && v < 0.
        {
            s[1] -= v;
        } else if ASSET_GLS.contains(&gl)
            || credit_source && CREDIT_ASSET_GLS.contains(&gl)
            || mortgage_closing && MORTGAGE_PENDING_ASSET_GLS.contains(&gl)
        {
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
            if exposure_policy.is_some() {
                let group = groups
                    .entry((&r.scenario, &r.account, &r.instrument_id))
                    .or_insert((0., weight));
                if group.1 != weight {
                    return Err("one reviewed risk weight required per exposure group".into());
                }
                group.0 += v;
            }
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
    let mut group_audit = Vec::new();
    let mut research_scopes = BTreeSet::new();
    if let Some(p) = exposure_policy {
        for s in sums.values_mut() {
            s[5] = 0.;
        }
        for ((scenario, account, instrument), (net, weight)) in groups {
            if net < 0. && p.negative_net_carrying == "reject_negative_exposure_groups" {
                return Err(
                    "negative net carrying exposure group requires different reviewed policy"
                        .into(),
                );
            }
            let exposure = if p.negative_net_carrying == "reviewed_nonnegative_group_exposure" {
                net.max(0.)
            } else {
                net
            };
            if net < 0. && p.negative_net_carrying == "research_signed_exposure_groups" {
                research_scopes.insert((scenario, account));
            }
            let rwa = exposure * weight;
            sums.get_mut(&(scenario, account))
                .ok_or("missing exposure scope")?[5] += rwa;
            group_audit.push(json!({"scenario":scenario,"account":account,"instrument_id":instrument,"accounting_net_carrying":net,"policy_exposure":exposure,"risk_weight":weight,"policy_rwa_contribution":rwa,"negative_accounting_exposure":net<0.}));
        }
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
        let included = if p.include_aoci {
            s[4] - if p.cash_flow_hedge_aoci_eligible {
                0.
            } else {
                s[7]
            }
        } else {
            0.
        };
        let (deferred_tax, eligible) = match &p.aoci_eligibility {
            None => (0., included),
            Some(a) => (
                -included * a.deferred_tax_rate,
                if a.opt_out_elected {
                    0.
                } else {
                    included * (1. - a.deferred_tax_rate) * a.phase_in_fraction
                },
            ),
        };
        c.eligible_aoci = eligible;
        c.credit_rwa = s[5];
        c.total_leverage_exposure = s[0] + p.leverage_addon - p.leverage_deductions;
        c.tangible_common_equity = equity - p.equity_exclusions - p.intangible_assets;
        c.tangible_assets = s[0] - p.intangible_assets;
        let mut row = json!({"scenario":scenario,"account":account,"entity":c.entity,"currency":c.currency,
            "assets":s[0],"liabilities":s[1],"book_equity":equity,"common_equity":c.common_equity,
            "retained_earnings":s[3],"book_aoci":s[4],"eligible_aoci":c.eligible_aoci,
            "equity_exclusions":p.equity_exclusions,"credit_rwa":s[5],"reconciliation_error":s[6]});
        if let Some(a) = &p.aoci_eligibility {
            // Extended disclosure only under the reviewed AOCI treatment; historical audits are unchanged.
            row["cash_flow_hedge_aoci"] = json!(s[7]);
            row["included_aoci"] = json!(included);
            row["deferred_tax_on_aoci"] = json!(deferred_tax);
            row["aoci_opt_out_elected"] = json!(a.opt_out_elected);
            row["aoci_phase_in_fraction"] = json!(a.phase_in_fraction);
            row["aoci_eligibility_policy"] = json!(a.identity);
        }
        audit.push(row);
        t.capital.push(c);
    }
    let mut out = t.run()?;
    out["prepared_specification"] = serde_json::to_value(t).map_err(|e| e.to_string())?;
    out["capital_bridge"] = json!(audit);
    out["rwa_contributions"] = json!(contributions);
    if let Some(p) = exposure_policy {
        for metric in out["capital_metrics"]
            .as_array_mut()
            .ok_or("capital metrics missing")?
        {
            let affected = research_scopes.iter().any(|(scenario, account)| {
                rules.get(&(*scenario, *account)).is_some_and(|rule| {
                    metric["scenario"] == *scenario
                        && metric["entity"] == rule.capital.entity
                        && metric["currency"] == rule.capital.currency
                        && metric["period"] == rule.capital.period
                })
            });
            if affected
                && ["cet1", "tier1", "total_capital", "tlac_rwa", "ltd_rwa"]
                    .contains(&metric["metric"].as_str().unwrap_or(""))
            {
                metric["status"] = json!("research_only_signed_exposure");
            }
        }
        out["exposure_policy"] = serde_json::to_value(p).map_err(|e| e.to_string())?;
        out["exposure_groups"] = json!(group_audit);
        out["signed_exposure_research_only"] = json!(!research_scopes.is_empty());
        out["regulatory_credit_rwa_approval"] = json!(false);
        out["rwa_contribution_basis"]=json!("Row contributions retain signed accounting diagnostics; only separately disclosed exposure-group contributions determine policy credit RWA. No accounting or regulatory approval inferred.");
    }
    out["bridge_basis"]=json!("Closing trial balance; net carrying-value risk weights supplied by policy; no automatic regulatory eligibility or consolidation");
    if credit_source {
        out["credit_source_basis"] = json!("Physical credit journal; surviving and default principal/accrual/basis retain separate GL lineage; posted allowance/provision/chargeoff/recovery included exactly once. Economic modeled loss is not an additional capital deduction. Undrawn CCF/leverage policy remains explicit.");
    }
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

/// Pure reviewed exposure mathematics. This helper does not authorize a source
/// publication; the Decision raw coordinator must derive complete-tail ownership.
pub fn validate_reviewed_mortgage_inputs(
    treasury: &Treasury,
    policies: &[LedgerPolicy],
    amount_multiplier: f64,
    exposure_policy: &ExposurePolicy,
) -> Result<(), String> {
    exposure_policy.validate()?;
    check(amount_multiplier, true)?;
    if amount_multiplier == 0.
        || policies.is_empty()
        || policies.len() > 16
        || !treasury.capital.is_empty()
        || !treasury.capital_deltas.is_empty()
        || !treasury.positions.is_empty()
        || !treasury.curves.is_empty()
        || !treasury.strategy_units.is_empty()
    {
        return Err("reviewed mortgage closing input admission".into());
    }
    label(&treasury.as_of)?;
    label(&treasury.policy_id)?;
    let mut scopes = BTreeSet::new();
    for p in policies {
        portfolio_compute_control::checkpoint()?;
        label(&p.account)?;
        label(&p.scenario)?;
        p.capital.components()?;
        if !scopes.insert((&p.scenario, &p.account))
            || p.gl_risk_weights.len() + p.instrument_risk_weights.len() > 128
        {
            return Err("duplicate or oversized mortgage closing policy".into());
        }
        for value in [
            p.equity_exclusions,
            p.intangible_assets,
            p.leverage_addon,
            p.leverage_deductions,
        ] {
            check(value, true)?;
        }
        for (key, value) in p.gl_risk_weights.iter().chain(&p.instrument_risk_weights) {
            label(key)?;
            check(*value, true)?;
        }
        if p.gl_risk_weights.keys().any(|key| {
            !(ASSET_GLS.contains(&key.as_str())
                || MORTGAGE_PENDING_ASSET_GLS.contains(&key.as_str()))
        }) {
            return Err("unknown asset GL risk mapping".into());
        }
    }
    Ok(())
}

/// Checked mathematical helper. Source closure remains the coordinator's job.
pub fn prepare_reviewed_mortgage_exposure(
    treasury: Treasury,
    trial: Vec<Trial>,
    policies: Vec<LedgerPolicy>,
    amount_multiplier: f64,
    exposure_policy: &ExposurePolicy,
) -> Result<Value, String> {
    validate_reviewed_mortgage_inputs(&treasury, &policies, amount_multiplier, exposure_policy)?;
    ledger_with_exposure(
        treasury,
        trial,
        policies,
        amount_multiplier,
        6,
        Some(exposure_policy),
    )
}

/// Additive reviewed commercial helper. The coordinator must independently
/// replay its source/journal and remove only balanced legal-right memo pairs.
/// Public CreditLedger never admits this internal source capability.
pub fn validate_reviewed_commercial_inputs(
    treasury: &Treasury,
    policies: &[LedgerPolicy],
    amount_multiplier: f64,
    exposure_policy: &ExposurePolicy,
) -> Result<(), String> {
    validate_reviewed_mortgage_inputs(treasury, policies, amount_multiplier, exposure_policy)?;
    if policies.iter().any(|p| {
        p.gl_risk_weights
            .keys()
            .any(|k| !ASSET_GLS.contains(&k.as_str()))
    }) {
        return Err("unknown commercial financial asset GL risk mapping".into());
    }
    Ok(())
}
pub fn prepare_reviewed_commercial_exposure(
    treasury: Treasury,
    trial: Vec<Trial>,
    policies: Vec<LedgerPolicy>,
    amount_multiplier: f64,
    exposure_policy: &ExposurePolicy,
) -> Result<Value, String> {
    validate_reviewed_commercial_inputs(&treasury, &policies, amount_multiplier, exposure_policy)?;
    ledger_with_exposure(
        treasury,
        trial,
        policies,
        amount_multiplier,
        7,
        Some(exposure_policy),
    )
}
