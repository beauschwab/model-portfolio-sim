//! Closing (or explicit report-date) capital from a standalone reviewed derivative journal.
//! The `derivative-accounting-1` source is re-executed natively; its cash, gross derivative
//! values, margin and earnings balances map once to reviewed bridge GLs, and the existing
//! treasury bridge computes capital under supplied policies. No hedge accounting is applied:
//! every mark stays in earnings. No FTP or approval.
use crate::products::derivatives::accounting::{
    self as derivative_accounting, DerivativeAccountingRequest, DerivativeAccountingResult,
    JournalLedger,
};
use crate::products::derivatives::contract::{amount, identity, near, text};
use crate::products::derivatives::hedge_accounting::Account;
use crate::products::derivatives::hedge_treasury::{Mapping, Row, ZERO_ROW};
use crate::treasury::{bridge, evaluate::Request as Treasury};
use portfolio_model_core::cashflow::ymd_from_ordinal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "derivative-treasury-1";
pub const MAX_INPUT: usize = 32 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub derivative: Box<DerivativeAccountingRequest>,
    pub account: String,
    pub mapping: Mapping,
    pub treasury: Treasury,
    pub policies: Vec<Value>,
    pub amount_multiplier: f64,
    /// Explicit report-date selection: `treasury.as_of` must then be a path node strictly
    /// before the complete-tail closing date.
    #[serde(default)]
    pub interim_report: bool,
}
#[derive(Debug, Serialize)]
pub struct DerivativeTreasuryResult {
    pub schema: String,
    pub closing_ordinal: i32,
    pub closing_date: String,
    pub scenario: String,
    pub account: String,
    pub mapping: Mapping,
    pub rows: Vec<Row>,
    pub policies: Vec<Value>,
    pub amount_multiplier: f64,
    pub derivative: DerivativeAccountingResult,
    pub capital: Value,
    pub closing_capital_selected: bool,
    pub interim_capital: bool,
    pub ftp_selected: bool,
    pub hedge_accounting_applied: bool,
    pub accounting_standard_approval: bool,
    pub regulatory_credit_rwa_approval: bool,
    pub empirical_validation: bool,
    pub persisted_journal_verified: bool,
    pub disclosure: String,
}

fn iso(ordinal: i32) -> Result<String, String> {
    let (y, m, d) = ymd_from_ordinal(ordinal)?;
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}
/// Reviewed GL for each derivative journal account; custody memo accounts map to nothing.
fn gl(account: Account) -> Result<Option<&'static str>, String> {
    Ok(Some(match account {
        Account::Cash => "cash",
        Account::DerivativeAsset | Account::DerivativeLiability => "derivative_value",
        Account::UnpaidReceivable => "derivative_unpaid_receivable",
        Account::UnpaidPayable => "derivative_unpaid_payable",
        Account::PostedVmAsset | Account::PostedImAsset => "posted_margin",
        Account::ReceivedVmLiability => "received_margin",
        Account::OpeningEquity => "opening_equity",
        Account::MarkGainLoss => "pnl:derivative_mark",
        Account::ContractualIncome => "pnl:derivative_contractual_income",
        Account::CollateralInterestIncome => "pnl:derivative_collateral_interest",
        Account::CounterpartyLossExpense => "pnl:derivative_counterparty_loss",
        Account::PostedCollateralReturnLossExpense => "pnl:derivative_collateral_return_loss",
        Account::MemoCustodianAsset | Account::MemoCustodianObligation => return Ok(None),
        _ => {
            return Err(
                "hedge accounting accounts cannot appear in a standalone derivative journal".into(),
            )
        }
    }))
}
pub fn evaluate(r: &Request) -> Result<DerivativeTreasuryResult, String> {
    let mut bytes = 0usize;
    if r.schema != SCHEMA {
        return Err("derivative treasury requires explicit derivative-treasury-1 selection".into());
    }
    text(&r.account, &mut bytes)?;
    identity(&r.mapping.identity, &mut bytes)?;
    if !r.amount_multiplier.is_finite() || r.amount_multiplier <= 0. || r.amount_multiplier > 1e12 {
        return Err("positive finite monetary multiplier required".into());
    }
    if !r.treasury.capital.is_empty()
        || !r.treasury.capital_deltas.is_empty()
        || !r.treasury.positions.is_empty()
        || !r.treasury.curves.is_empty()
        || !r.treasury.strategy_units.is_empty()
    {
        return Err("closing capital from a derivative journal admits no supplied capital, FTP positions, curves or units".into());
    }
    portfolio_compute_control::checkpoint()?;
    let derivative = derivative_accounting::evaluate(&r.derivative)?;
    portfolio_compute_control::checkpoint()?;
    let tail = derivative
        .daily
        .last()
        .ok_or("derivative journal has no closing day")?;
    let closing = if r.interim_report {
        let selected = derivative
            .daily
            .iter()
            .find(|d| iso(d.ordinal).is_ok_and(|date| date == r.treasury.as_of))
            .ok_or("interim report requires treasury as_of to be a derivative path node")?;
        if selected.ordinal >= tail.ordinal {
            return Err("interim report requires a reporting date before the complete-tail closing; select closing capital instead".into());
        }
        selected
    } else {
        tail
    };
    let closing_ordinal = closing.ordinal;
    let closing_date = iso(closing_ordinal)?;
    let scenario = derivative
        .native
        .paths
        .first()
        .map(|p| p.path_id.clone())
        .unwrap_or_else(|| derivative.policy.identity.id.clone());
    if r.treasury.as_of != closing_date {
        return Err("treasury as_of must equal the verified complete-tail closing date".into());
    }
    if r.policies.is_empty() || r.policies.len() > 16 {
        return Err("1..=16 reviewed ledger policies required".into());
    }
    let mut policies = Vec::with_capacity(r.policies.len());
    for value in &r.policies {
        let p: bridge::LedgerPolicy =
            serde_json::from_value(value.clone()).map_err(|e| format!("ledger policy: {e}"))?;
        if p.scenario != scenario || p.account != r.account {
            return Err("ledger policy must name the derivative scenario and account".into());
        }
        if p.capital.period != closing_date {
            return Err("ledger policy capital period must equal the closing date".into());
        }
        policies.push(p);
    }
    portfolio_compute_control::checkpoint()?;
    // Per-instrument balances through the reporting node from the single ordered journal.
    let mut balances: BTreeMap<(Account, String), f64> = BTreeMap::new();
    for intent in derivative
        .intents
        .iter()
        .filter(|i| i.ledger == JournalLedger::Financial && i.ordinal <= closing_ordinal)
    {
        for p in &intent.postings {
            let instrument = p
                .position_id
                .clone()
                .or_else(|| p.bucket.clone())
                .unwrap_or_else(|| "book".into());
            let entry = balances
                .entry((Account::from(p.account), instrument))
                .or_default();
            *entry = amount(*entry + p.debit_amount)?;
        }
    }
    let opening = &r.derivative.opening;
    for (account, value) in [
        (Account::Cash, opening.cash),
        (Account::PostedVmAsset, opening.posted_vm_asset),
        (Account::ReceivedVmLiability, -opening.received_vm_liability),
        (Account::PostedImAsset, opening.posted_im_asset),
        (Account::OpeningEquity, -opening.equity_credit_balance),
    ] {
        let entry = balances.entry((account, "book".to_string())).or_default();
        *entry = amount(*entry + value)?;
    }
    for t in &opening.trades {
        let entry = balances
            .entry((Account::DerivativeAsset, t.position_id.clone()))
            .or_default();
        *entry = amount(*entry + t.clean_value.max(0.))?;
        let entry = balances
            .entry((Account::DerivativeLiability, t.position_id.clone()))
            .or_default();
        *entry = amount(*entry - (-t.clean_value).max(0.))?;
    }
    let mut by_account: BTreeMap<Account, f64> = BTreeMap::new();
    for ((account, _), v) in &balances {
        *by_account.entry(*account).or_default() += v;
    }
    for b in &closing.financial_trial_balance {
        if !near(
            by_account
                .get(&Account::from(b.account))
                .copied()
                .unwrap_or(0.),
            b.debit_balance,
        ) {
            return Err(format!(
                "derivative closing {:?} does not reconcile its per-instrument postings",
                b.account
            ));
        }
    }
    let mut rows: BTreeMap<(String, String), f64> = BTreeMap::new();
    for ((account, instrument), v) in &balances {
        if let Some(name) = gl(*account)? {
            let entry = rows
                .entry((name.to_string(), instrument.clone()))
                .or_default();
            *entry = amount(*entry + v)?;
        }
    }
    let rows: Vec<Row> = rows
        .into_iter()
        .filter(|(_, v)| v.abs() > ZERO_ROW)
        .map(|((gl_account, instrument_id), balance)| Row {
            scenario: scenario.clone(),
            account: r.account.clone(),
            gl_account,
            instrument_id,
            balance,
        })
        .collect();
    if rows
        .iter()
        .any(|row| row.gl_account.starts_with("derivative_unpaid"))
    {
        return Err(
            "unpaid derivative rights at the reporting date require their own reviewed treatment"
                .into(),
        );
    }
    let treasury: Treasury =
        serde_json::from_value(serde_json::to_value(&r.treasury).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let trial = rows
        .iter()
        .map(|row| {
            serde_json::from_value::<bridge::Trial>(
                serde_json::to_value(row).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    portfolio_compute_control::checkpoint()?;
    let capital = bridge::ledger(treasury, trial, policies, r.amount_multiplier, 0)?;
    portfolio_compute_control::checkpoint()?;
    Ok(DerivativeTreasuryResult {
        schema: SCHEMA.into(),
        closing_ordinal,
        closing_date,
        scenario,
        account: r.account.clone(),
        mapping: r.mapping.clone(),
        rows,
        policies: r.policies.clone(),
        amount_multiplier: r.amount_multiplier,
        derivative,
        capital,
        closing_capital_selected: !r.interim_report,
        interim_capital: r.interim_report,
        ftp_selected: false,
        hedge_accounting_applied: false,
        accounting_standard_approval: false,
        regulatory_credit_rwa_approval: false,
        empirical_validation: false,
        persisted_journal_verified: false,
        disclosure: "Capital from the natively re-executed standalone reviewed derivative journal through the selected reporting node (the complete tail unless an explicit interim report is selected): derivative cash, gross values, margin and earnings balances map once to reviewed bridge GLs and reconcile to that day's trial balance. No formal hedge accounting is applied, so every mark is in earnings and no cash-flow-hedge AOCI arises. Regulatory eligibility, requirements, risk weights and the monetary unit are supplied inputs. Not FTP, accounting-standard or regulatory approval, empirical validation or persisted verification.".into(),
    })
}
