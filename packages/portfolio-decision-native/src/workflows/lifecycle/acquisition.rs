//! Stages B and C (issues #44 and #45): native acquisition and unwind coordination. The raw
//! envelope carries the reviewed dated backbook, the admitted plan, one execution block per run
//! scenario and the raw swap contracts priced at every entry and termination date. Risk prices
//! each contract; the model-core engine replays the execution; the ledger owner posts its intents
//! in the existing daily Sim. No caller intents or values.
use portfolio_ledger_native::{
    daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    lifecycle_intent_owner as owner,
    validation::specification::NativeOpening,
};
use portfolio_model_core::lifecycle::{
    self,
    acquisition::{self as engine, AcquisitionJournal, AcquisitionRequest, ResolvedHedge},
    journal::Kind,
    Admitted, LotKind, Revision,
};
use portfolio_model_core::strict_json;
use portfolio_risk_native::derivative_contract::swaps::{self, PaymentTiming, SwapRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub const SCHEMA: &str = "liquidity-lifecycle-1";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeContract {
    pub lot_id: String,
    pub swap: SwapRequest,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub dated: DatedLedgerRequest,
    pub plan: Value,
    pub acquisitions: Vec<AcquisitionRequest>,
    #[serde(default)]
    pub hedge_contracts: Vec<HedgeContract>,
}

/// Native valuation of a derivative lot at an entry or termination date.
#[derive(Debug, Clone, Serialize)]
pub struct HedgeValuation {
    pub lot_id: String,
    pub position_id: String,
    pub ordinal: i32,
    pub clean_value: f64,
    pub due_cash: f64,
    pub pricing_source: Revision,
}
#[derive(Debug, Clone, Serialize)]
pub struct JournalReceipt {
    pub account_id: String,
    pub scenario_id: String,
    pub receipt: engine::Receipt,
}
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub schema: &'static str,
    pub stage: &'static str,
    pub plan_sha256: String,
    pub hedge_entries: Vec<HedgeValuation>,
    pub journals: Vec<JournalReceipt>,
    pub formal_hedge_accounting: bool,
    pub disclosure: &'static str,
}

pub struct Prepared {
    pub dated: CompiledDatedLedger,
    pub plan: Admitted,
    pub plan_sha256: String,
    pub hedge_entries: Vec<HedgeValuation>,
    pub journals: Vec<AcquisitionJournal>,
}

fn refused(r: lifecycle::Refusal) -> String {
    r.to_string()
}

fn empty_block(asof: i32) -> AcquisitionRequest {
    AcquisitionRequest {
        scenario_id: String::new(),
        horizon_end_ordinal: asof,
        purchases: vec![],
        sales: vec![],
        financings: vec![],
        repayments: vec![],
        hedge_entries: vec![],
        terminations: vec![],
        facility_draws: vec![],
        facility_repayments: vec![],
        initial_margin: vec![],
        business_plan: None,
    }
}

pub fn prepare(bytes: &[u8]) -> Result<Prepared, String> {
    portfolio_compute_control::checkpoint()?;
    let original = strict_json::parse(bytes, MAX_BYTES)?;
    let request: Request = serde_json::from_value(original).map_err(|e| e.to_string())?;
    if request.schema != SCHEMA {
        return Err("explicit liquidity-lifecycle-1 selection required".into());
    }
    let plan_bytes = serde_json::to_vec(&request.plan).map_err(|e| e.to_string())?;
    let plan = lifecycle::admit(&plan_bytes).map_err(refused)?;
    let plan_sha256 = format!("{:x}", Sha256::digest(&plan_bytes));
    let p = &plan.request;
    if request.dated.monetary_unit != p.monetary_unit {
        return Err("plan and dated backbook monetary units differ".into());
    }
    // The shared opening is posted once: every plan account is a dated account with the same cash
    // and equity, and this owner's opening stocks enter the admitted balance sheet.
    let spec = request
        .dated
        .specification
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("dated specification accounts required")?;
    for a in &p.accounts {
        let found = spec
            .iter()
            .find(|x| x.get("id").and_then(Value::as_str) == Some(a.account_id.as_str()))
            .ok_or_else(|| {
                format!(
                    "plan account `{}` is not a dated backbook account",
                    a.account_id
                )
            })?;
        let same = |field: &str, value: f64| {
            found
                .get(field)
                .and_then(Value::as_f64)
                .is_some_and(|x| x.to_bits() == value.to_bits())
        };
        if !same("cash", a.cash)
            || !same("equity", a.equity)
            || found.get("entity").and_then(Value::as_str) != Some(p.entity.as_str())
            || found.get("currency").and_then(Value::as_str) != Some(p.currency.as_str())
        {
            return Err(format!(
                "plan account `{}` opening cash/equity/entity/currency differ from the dated backbook",
                a.account_id
            ));
        }
    }
    // Native valuations from the raw contracts at every entry and termination date.
    let mut dates: BTreeSet<(String, i32)> = BTreeSet::new();
    for acq in &request.acquisitions {
        for h in &acq.hedge_entries {
            dates.insert((h.lot_id.clone(), h.entry_ordinal));
        }
        for t in &acq.terminations {
            dates.insert((t.lot_id.clone(), t.effective_ordinal));
        }
    }
    let mut hedge_entries = Vec::new();
    let mut resolved = Vec::new();
    for (lot_id, ordinal) in &dates {
        portfolio_compute_control::checkpoint()?;
        let lot = p.lots.iter().find(|l| &l.lot_id == lot_id).ok_or_else(|| {
            format!(
                "LIFECYCLE_MAPPING_REQUIRED: derivative instruction names unknown lot `{lot_id}`"
            )
        })?;
        if lot.kind != LotKind::Derivative {
            return Err(format!(
                "UNSUPPORTED_POLICY: `{lot_id}` is not a derivative lot"
            ));
        }
        let contract = request
            .hedge_contracts
            .iter()
            .find(|c| &c.lot_id == lot_id && c.swap.asof_ordinal == *ordinal)
            .ok_or_else(|| format!("LIFECYCLE_MAPPING_REQUIRED: `{lot_id}` has no raw swap contract priced at its entry or termination date {ordinal}"))?;
        let swap = &contract.swap;
        if swap.position_id != lot.position_id
            || swap.currency != p.currency
            || swap.monetary_unit != p.monetary_unit
        {
            return Err(format!(
                "LIFECYCLE_MAPPING_REQUIRED: swap contract for `{lot_id}` must price the lot's position in the plan currency"
            ));
        }
        let value = swaps::value_swap(swap)?;
        // Contractual cash due on the valuation date, signed from the entity's side.
        let mut before = swap.clone();
        before.payment_timing = PaymentTiming::BeforeScheduledPayments;
        let due_cash = swaps::value_swap(&before)?
            .schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal == *ordinal)
            .try_fold(0., |a, e| e.components.cash_total().map(|c| a + c))?;
        let pricing_source = Revision {
            id: swap.discount_curve.source.id.clone(),
            revision: swap.discount_curve.source.revision.clone(),
        };
        hedge_entries.push(HedgeValuation {
            lot_id: lot_id.clone(),
            position_id: lot.position_id.clone(),
            ordinal: *ordinal,
            clean_value: value.clean_value,
            due_cash,
            pricing_source: pricing_source.clone(),
        });
        resolved.push(ResolvedHedge {
            lot_id: lot_id.clone(),
            ordinal: *ordinal,
            clean_value: value.clean_value,
            due_cash,
            pricing_source,
        });
    }
    // This owner's opening stocks (opening lots and collateral) enter the admitted balance sheet.
    let mut native_opening: Vec<NativeOpening> = Vec::new();
    for a in &p.accounts {
        let opening = engine::replay(&plan, &empty_block(p.as_of_ordinal), &[], &a.account_id)
            .map_err(refused)?;
        native_opening.push(NativeOpening {
            account_id: a.account_id.clone(),
            assets: opening.opening_stocks.0,
            liabilities: opening.opening_stocks.1,
        });
    }
    let dated = CompiledDatedLedger::compile_with_native_opening(
        request.dated,
        60_000,
        90_000_000,
        &native_opening,
    )?;
    let runs: Vec<String> = dated
        .store
        .runs()
        .iter()
        .map(|r| r.scenario_id.to_string())
        .collect();
    let mut by_scenario: BTreeMap<&str, &AcquisitionRequest> = BTreeMap::new();
    for acq in &request.acquisitions {
        if !runs.contains(&acq.scenario_id) || by_scenario.insert(&acq.scenario_id, acq).is_some() {
            return Err(format!(
                "execution scenario `{}` must name each dated run exactly once",
                acq.scenario_id
            ));
        }
    }
    if by_scenario.len() != runs.len() {
        return Err("every dated run needs its execution block".into());
    }
    let tail = dated.store.reporting_end_ordinal();
    let mut journals = Vec::new();
    for scenario in &runs {
        let acq = by_scenario[scenario.as_str()];
        if acq.horizon_end_ordinal != tail {
            return Err(format!(
                "execution horizon for `{scenario}` must end at the reporting date {tail}"
            ));
        }
        for a in &p.accounts {
            portfolio_compute_control::checkpoint()?;
            journals.push(engine::replay(&plan, acq, &resolved, &a.account_id).map_err(refused)?);
        }
    }
    Ok(Prepared {
        dated,
        plan,
        plan_sha256,
        hedge_entries,
        journals,
    })
}

fn namespaced(kind: Kind, instrument: &str) -> String {
    if matches!(kind, Kind::Cash | Kind::OpeningEquity) {
        String::new()
    } else {
        format!("{}{}", owner::NAMESPACE, instrument)
    }
}

fn sources(prepared: &Prepared) -> Vec<owner::Source<'_>> {
    let p = &prepared.plan.request;
    prepared
        .journals
        .iter()
        .map(|j| owner::Source {
            plan_key: prepared.plan_sha256.clone(),
            scenario: j.scenario_id.clone(),
            account_id: j.account_id.clone(),
            entity: &p.entity,
            currency: &p.currency,
            monetary_unit: &p.monetary_unit,
            asof: j.as_of_ordinal,
            tail: j.tail_ordinal,
            opening_cash: j.opening_cash,
            opening: j
                .opening
                .iter()
                .map(|l| owner::Line {
                    kind: l.kind,
                    instrument: namespaced(l.kind, &l.instrument),
                    debit: l.debit,
                })
                .collect(),
            intents: j
                .intents
                .iter()
                .map(|i| owner::Intent {
                    event_id: &i.event_id,
                    ordinal: i.ordinal,
                    memo: i.memo,
                    lines: i
                        .lines
                        .iter()
                        .map(|l| owner::Line {
                            kind: l.kind,
                            instrument: namespaced(l.kind, &l.instrument),
                            debit: l.debit,
                        })
                        .collect(),
                })
                .collect(),
            daily: j
                .daily
                .iter()
                .map(|d| owner::Day {
                    ordinal: d.ordinal,
                    financial: d.financial.clone(),
                    custody: d.custody.clone(),
                })
                .collect(),
        })
        .collect()
}

pub fn run(bytes: &[u8], mut output: Box<dyn Write + Send>) -> Result<Receipt, String> {
    let prepared = prepare(bytes)?;
    let sources = sources(&prepared);
    let compiled = owner::Compiled::new(&prepared.dated, sources)?;
    let receipt = Receipt {
        schema: SCHEMA,
        stage: "C",
        plan_sha256: prepared.plan_sha256.clone(),
        hedge_entries: prepared.hedge_entries.clone(),
        journals: prepared
            .journals
            .iter()
            .map(|j| JournalReceipt { account_id: j.account_id.clone(), scenario_id: j.scenario_id.clone(), receipt: j.receipt.clone() })
            .collect(),
        formal_hedge_accounting: false,
        disclosure: "Native acquisition and unwind replay. Purchases carry cost (no par gain); financing advances post once with their pledge; hedge entries carry the native clean value with equal upfront consideration; cash VM settles after the agreement lag and supplied exact IM binds to the live portfolio, with received IM as custody memo. Sales remove carrying pro rata and realize consideration less carrying; voluntary repayments carry actual accrued and break costs and release pledges; terminations stop rights at the effective date, hold consideration and due cash as an unpaid claim until settlement, and return or apply VM exactly once; bridge facilities draw and repay within their commitment. Settlement is gated by declared dependencies: a failed leg blocks its dependents and nothing settled is undone. Intents post once in the shared daily Sim beside the dated backbook. Undesignated economic treatment only: sale and termination facts are reported for the ASC 815 owner, which posts no basis or OCI here. Post-entry derivative marks and coupons, dynamic margin, the independent Rust verifier and the Rust service are later stages.",
    };
    serde_json::to_writer(
        &mut output,
        &json!({"workflow": {"schema": "model-ledger-preparation-1", "model_schema": SCHEMA, "source_receipt": &receipt}}),
    )
    .map_err(|e| e.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())?;
    daily::run_dated_with_lifecycle(&prepared.dated, &compiled, output)?;
    Ok(receipt)
}
