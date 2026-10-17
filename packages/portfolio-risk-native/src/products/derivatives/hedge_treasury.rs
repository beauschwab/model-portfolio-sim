//! Complete-tail hedge closing balances as explicit treasury capital inputs.
//! The hedge journal is re-executed natively; its closing accounts map to reviewed
//! bridge GLs, and the existing treasury bridge computes closing capital under
//! supplied policies. Cash-flow-hedge AOCI keeps its own GL so eligibility is a
//! policy decision, never an inference. No FTP, interim capital or approval.
use crate::products::derivatives::accounting::JournalLedger;
use crate::products::derivatives::contract::{amount, identity, near, text, Identity};
use crate::products::derivatives::hedge_accounting::{
    self, Account, HedgeAccountingRequest, HedgeAccountingResult, ItemDirection,
};
use crate::treasury::{bridge, evaluate::Request as Treasury};
use portfolio_model_core::cashflow::ymd_from_ordinal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA: &str = "hedge-treasury-1";
pub const MAX_INPUT: usize = 32 << 20;
pub const MAX_OUTPUT: usize = 32 << 20;
/// Closing balances within the comparison tolerance of zero are omitted from bridge rows.
pub const ZERO_ROW: f64 = 1e-9;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub identity: Identity,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub hedge: Box<HedgeAccountingRequest>,
    pub account: String,
    pub mapping: Mapping,
    pub treasury: Treasury,
    pub policies: Vec<Value>,
    pub amount_multiplier: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub scenario: String,
    pub account: String,
    pub gl_account: String,
    pub instrument_id: String,
    pub balance: f64,
}
#[derive(Debug, Serialize)]
pub struct HedgeTreasuryResult {
    pub schema: String,
    pub closing_ordinal: i32,
    pub closing_date: String,
    pub scenario: String,
    pub account: String,
    pub mapping: Mapping,
    pub rows: Vec<Row>,
    pub policies: Vec<Value>,
    pub amount_multiplier: f64,
    pub cash_flow_hedge_aoci_eligible: bool,
    pub hedge: HedgeAccountingResult,
    pub capital: Value,
    pub closing_capital_selected: bool,
    pub interim_capital: bool,
    pub ftp_selected: bool,
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
/// Reviewed GL for each hedge journal account; hedged-item basis depends on the item direction.
fn gl(
    account: Account,
    instrument: &str,
    request: &HedgeAccountingRequest,
) -> Result<Option<&'static str>, String> {
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
        Account::CashFlowHedgeOci => "cash_flow_hedge_oci",
        Account::HedgedItemBasisAdjustment => {
            let item = request
                .hedged_items
                .iter()
                .find(|i| i.item_id == instrument)
                .ok_or("hedged item basis row lacks its item")?;
            match item.direction {
                ItemDirection::Asset => "hedged_item_basis_adjustment",
                ItemDirection::Liability => "hedged_item_basis_liability",
            }
        }
        Account::HedgedItemBasisGainLoss => "pnl:hedged_item_basis",
        Account::HedgedItemBasisAmortization => "pnl:hedged_item_basis_amortization",
        Account::HedgedItemBasisDerecognition => "pnl:hedged_item_basis_derecognition",
        Account::HedgedForecastReclassification => "pnl:hedged_forecast_reclassification",
    }))
}

pub fn evaluate(r: &Request) -> Result<HedgeTreasuryResult, String> {
    let mut bytes = 0usize;
    if r.schema != SCHEMA {
        return Err("hedge treasury requires explicit hedge-treasury-1 selection".into());
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
        return Err("closing capital from a hedge journal admits no supplied capital, FTP positions, curves or units".into());
    }
    portfolio_compute_control::checkpoint()?;
    let hedge = hedge_accounting::evaluate(&r.hedge)?;
    portfolio_compute_control::checkpoint()?;
    let closing = hedge
        .daily
        .last()
        .ok_or("hedge journal has no closing day")?;
    let closing_ordinal = closing.ordinal;
    let closing_date = iso(closing_ordinal)?;
    let scenario = hedge
        .intents
        .first()
        .map(|i| i.scenario_id.clone())
        .unwrap_or_else(|| hedge.derivative.policy.identity.id.clone());
    let scenario = hedge
        .derivative
        .native
        .paths
        .first()
        .map(|p| p.path_id.clone())
        .unwrap_or(scenario);
    if r.treasury.as_of != closing_date {
        return Err("treasury as_of must equal the verified complete-tail closing date".into());
    }
    if r.policies.is_empty() || r.policies.len() > 16 {
        return Err("1..=16 reviewed ledger policies required".into());
    }
    let mut policies = Vec::with_capacity(r.policies.len());
    let mut eligible: Option<bool> = None;
    for value in &r.policies {
        let p: bridge::LedgerPolicy =
            serde_json::from_value(value.clone()).map_err(|e| format!("ledger policy: {e}"))?;
        if p.scenario != scenario || p.account != r.account {
            return Err("ledger policy must name the hedge scenario and account".into());
        }
        if p.capital.period != closing_date {
            return Err("ledger policy capital period must equal the closing date".into());
        }
        if eligible.is_some_and(|e| e != p.cash_flow_hedge_aoci_eligible) {
            return Err(
                "one reviewed cash-flow-hedge AOCI eligibility decision per closing".into(),
            );
        }
        eligible = Some(p.cash_flow_hedge_aoci_eligible);
        policies.push(p);
    }
    // Per-instrument closing balances from the single ordered journal.
    let mut balances: BTreeMap<(Account, String), f64> = BTreeMap::new();
    for intent in hedge
        .intents
        .iter()
        .filter(|i| i.ledger == JournalLedger::Financial)
    {
        for p in &intent.postings {
            let instrument = p
                .position_id
                .clone()
                .or_else(|| p.bucket.clone())
                .unwrap_or_else(|| "book".into());
            let entry = balances.entry((p.account, instrument)).or_default();
            *entry = amount(*entry + p.debit_amount)?;
        }
    }
    // The opening anchors are not postings; include them so rows equal the closing trial balance.
    let opening = &r.hedge.derivative.opening;
    for (account, instrument, value) in [
        (Account::Cash, "book".to_string(), opening.cash),
        (
            Account::PostedVmAsset,
            "book".to_string(),
            opening.posted_vm_asset,
        ),
        (
            Account::ReceivedVmLiability,
            "book".to_string(),
            -opening.received_vm_liability,
        ),
        (
            Account::PostedImAsset,
            "book".to_string(),
            opening.posted_im_asset,
        ),
        (
            Account::OpeningEquity,
            "book".to_string(),
            -opening.equity_credit_balance,
        ),
    ] {
        let entry = balances.entry((account, instrument)).or_default();
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
    // Reconcile every account against the owner's closing trial balance before mapping.
    let mut by_account: BTreeMap<Account, f64> = BTreeMap::new();
    for ((account, _), v) in &balances {
        *by_account.entry(*account).or_default() += v;
    }
    for b in &closing.financial_trial_balance {
        if !near(
            by_account.get(&b.account).copied().unwrap_or(0.),
            b.debit_balance,
        ) {
            return Err(format!(
                "hedge closing {:?} does not reconcile its per-instrument postings",
                b.account
            ));
        }
    }
    let mut rows: BTreeMap<(String, String), f64> = BTreeMap::new();
    for ((account, instrument), v) in &balances {
        if let Some(name) = gl(*account, instrument, &r.hedge)? {
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
            "unpaid derivative rights at the closing date require their own reviewed treatment"
                .into(),
        );
    }
    // A hedged-item basis adjustment is a signed carrying adjustment to an item whose own
    // principal rows are owned outside this journal. A negative basis cannot carry a
    // positive risk weight here: that would be negative credit RWA with no exposure.
    for row in rows
        .iter()
        .filter(|row| row.gl_account == "hedged_item_basis_adjustment" && row.balance < 0.)
    {
        for p in &policies {
            let weight = p
                .instrument_risk_weights
                .get(&row.instrument_id)
                .or_else(|| p.gl_risk_weights.get(&row.gl_account))
                .copied()
                .unwrap_or(0.);
            if weight != 0. {
                return Err("negative hedged-item basis requires the item's own carrying rows for a nonzero risk weight; supply zero here".into());
            }
        }
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
    Ok(HedgeTreasuryResult {
        schema: SCHEMA.into(),
        closing_ordinal,
        closing_date,
        scenario,
        account: r.account.clone(),
        mapping: r.mapping.clone(),
        rows,
        policies: r.policies.clone(),
        amount_multiplier: r.amount_multiplier,
        cash_flow_hedge_aoci_eligible: eligible.unwrap_or(false),
        hedge,
        capital,
        closing_capital_selected: true,
        interim_capital: false,
        ftp_selected: false,
        accounting_standard_approval: false,
        regulatory_credit_rwa_approval: false,
        empirical_validation: false,
        persisted_journal_verified: false,
        disclosure: "Complete-tail closing capital from the natively re-executed formal hedge journal. Derivative cash, margin, marks and hedge basis/OCI/reclassification balances map to reviewed bridge GLs once; cash-flow-hedge AOCI stays under its own GL and enters eligible AOCI only when the supplied policy says so. Regulatory eligibility, requirements, risk weights and the monetary unit are supplied inputs. Not interim capital, FTP, accounting-standard or regulatory approval, empirical validation or persisted verification.".into(),
    })
}
