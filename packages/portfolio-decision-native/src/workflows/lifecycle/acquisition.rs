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
    acquisition::{
        self as engine, AcquisitionJournal, AcquisitionRequest, ResolvedHedge, Sensitivity,
    },
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
/// The explicit `acceptance_60000` tier admits larger raw requests (#51); output is streamed.
pub const SCALE_TIER: &str = "acceptance_60000";
pub const MAX_TIER_BYTES: usize = 512 * 1024 * 1024;

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
    #[serde(default)]
    pub scale_tier: Option<String>,
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
    /// Stage I: native bucket sensitivities behind every research-method IM recomputation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub margin_sensitivities: Vec<Sensitivity>,
    pub formal_hedge_accounting: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale_tier: Option<String>,
    pub disclosure: &'static str,
}

pub struct Prepared {
    pub dated: CompiledDatedLedger,
    pub plan: Admitted,
    pub plan_sha256: String,
    pub hedge_entries: Vec<HedgeValuation>,
    pub journals: Vec<AcquisitionJournal>,
    pub margin_sensitivities: Vec<Sensitivity>,
    pub scale_tier: Option<String>,
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
        dynamic_margin: vec![],
        derivative_updates: vec![],
        variation_margin_reviews: vec![],
        facility_interest: vec![],
    }
}

/// The swap with every discount point in bucket `b` bumped by one basis point of continuously
/// compounded act/365 zero rate. The anchor (exactly one at asof) is unchanged.
fn bumped(swap: &SwapRequest, upper: &[i32], b: usize) -> SwapRequest {
    let mut s = swap.clone();
    let asof = s.discount_curve.asof_ordinal;
    for p in &mut s.discount_curve.points {
        let days = p.ordinal - asof;
        let bucket = upper
            .iter()
            .position(|u| days <= *u)
            .unwrap_or(upper.len() - 1);
        if bucket == b && days > 0 {
            p.discount_factor *= (-0.0001 * f64::from(days) / 365.).exp();
        }
    }
    s
}

/// Native research-method sensitivities of each priced derivative lot of an agreement on each
/// recomputation date: s_b = PV(bucket b bumped) - PV(base), clean value from the entity's side.
/// Lots without a contract priced that day are omitted; the engine refuses any that are live.
fn margin_sensitivities(
    plan: &Admitted,
    acquisitions: &[AcquisitionRequest],
    contracts: &[HedgeContract],
) -> Result<Vec<Sensitivity>, String> {
    let p = &plan.request;
    let mut dates: BTreeSet<(&str, i32)> = BTreeSet::new();
    for acq in acquisitions {
        for dm in &acq.dynamic_margin {
            dates.insert((dm.agreement_id.as_str(), dm.ordinal));
        }
    }
    let mut out: Vec<Sensitivity> = Vec::new();
    for (agreement_id, ordinal) in dates {
        let Some(a) = p
            .margin_agreements
            .iter()
            .find(|a| a.agreement_id == agreement_id)
        else {
            continue; // the engine names the unknown agreement
        };
        let Some(m) = a.margin_method.as_ref() else {
            continue; // the engine refuses a non-research agreement
        };
        for lot_id in &a.lot_ids {
            portfolio_compute_control::checkpoint()?;
            let Some(lot) = p.lots.iter().find(|l| &l.lot_id == lot_id) else {
                continue;
            };
            if lot.kind != LotKind::Derivative
                || out
                    .iter()
                    .any(|s| &s.lot_id == lot_id && s.ordinal == ordinal)
            {
                continue;
            }
            let Some(contract) = contracts
                .iter()
                .find(|c| &c.lot_id == lot_id && c.swap.asof_ordinal == ordinal)
            else {
                continue;
            };
            let swap = &contract.swap;
            if swap.position_id != lot.position_id
                || swap.currency != p.currency
                || swap.monetary_unit != p.monetary_unit
            {
                return Err(format!(
                    "LIFECYCLE_MAPPING_REQUIRED: swap contract for `{lot_id}` must price the lot's position in the plan currency"
                ));
            }
            let base = swaps::value_swap(swap)?.clean_value;
            let mut buckets = Vec::with_capacity(m.bucket_upper_days.len());
            for b in 0..m.bucket_upper_days.len() {
                let v = swaps::value_swap(&bumped(swap, &m.bucket_upper_days, b))?.clean_value;
                buckets.push(v - base);
            }
            out.push(Sensitivity {
                lot_id: lot_id.clone(),
                ordinal,
                buckets,
            });
        }
    }
    Ok(out)
}

fn validate_ongoing_contracts(
    request: &Request,
    plan: &lifecycle::LifecycleRequest,
) -> Result<(), String> {
    for acq in &request.acquisitions {
        let selected: BTreeSet<&str> = acq
            .derivative_updates
            .iter()
            .map(|u| u.lot_id.as_str())
            .collect();
        for id in selected {
            let lot = plan
                .lots
                .iter()
                .find(|l| l.lot_id == id)
                .ok_or("ongoing derivative lot missing")?;
            let contracts: Vec<_> = request
                .hedge_contracts
                .iter()
                .filter(|c| c.lot_id == id)
                .collect();
            let first = contracts
                .first()
                .ok_or("ongoing derivative needs raw contracts")?;
            let anchor = serde_json::to_value(&first.swap).map_err(|e| e.to_string())?;
            let mut seen = BTreeSet::new();
            for c in &contracts {
                let value = serde_json::to_value(&c.swap).map_err(|e| e.to_string())?;
                if !seen.insert(c.swap.asof_ordinal) {
                    return Err("duplicate ongoing derivative contract/date".into());
                }
                for key in [
                    "contract",
                    "position_id",
                    "currency",
                    "monetary_unit",
                    "notional",
                    "side",
                    "fixed_rate",
                    "fixed_periods",
                    "floating_periods",
                    "tail_end_ordinal",
                    "index_basis",
                    "rate_path",
                ] {
                    if value[key] != anchor[key] {
                        return Err(format!("ongoing derivative `{id}` changes its immutable supplied path/terms: {key}"));
                    }
                }
                if value["payment_timing"] != "after_scheduled_payments" {
                    return Err("ongoing derivative values must be after scheduled payments".into());
                }
            }
            let start = plan.as_of_ordinal.max(lot.acquisition_ordinal);
            let tail = acq.horizon_end_ordinal.min(first.swap.tail_end_ordinal);
            let mut required = BTreeSet::new();
            if tail > start {
                required.insert(tail);
            }
            for key in ["fixed_periods", "floating_periods"] {
                for period in anchor[key]
                    .as_array()
                    .ok_or("ongoing derivative periods missing")?
                {
                    let date = period["payment_ordinal"]
                        .as_i64()
                        .ok_or("payment date missing")? as i32;
                    if date > start && date <= tail {
                        required.insert(date);
                    }
                }
            }
            let provided: BTreeSet<_> = acq
                .derivative_updates
                .iter()
                .filter(|u| u.lot_id == id)
                .map(|u| u.ordinal)
                .collect();
            if !required.is_subset(&provided) {
                return Err(format!("ongoing derivative `{id}` must cover every coupon date and reporting/tail date, including failed-termination survival"));
            }
        }
    }
    Ok(())
}

pub fn prepare(bytes: &[u8]) -> Result<Prepared, String> {
    portfolio_compute_control::checkpoint()?;
    let original = strict_json::parse(bytes, MAX_TIER_BYTES)?;
    let request: Request = serde_json::from_value(original).map_err(|e| e.to_string())?;
    if request.schema != SCHEMA {
        return Err("explicit liquidity-lifecycle-1 selection required".into());
    }
    match request.scale_tier.as_deref() {
        None if bytes.len() > MAX_BYTES => {
            return Err(
                "BOUND_EXCEEDED: a lifecycle request above 8 MiB selects the acceptance_60000 tier"
                    .into(),
            )
        }
        None => {}
        Some(SCALE_TIER) => {}
        Some(other) => {
            return Err(format!(
                "UNSUPPORTED_POLICY: scale tier `{other}` is not supported"
            ))
        }
    }
    let scale_tier = request.scale_tier.clone();
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
        for u in &acq.derivative_updates {
            dates.insert((u.lot_id.clone(), u.ordinal));
        }
        for h in &acq.hedge_entries {
            dates.insert((h.lot_id.clone(), h.entry_ordinal));
        }
        for t in &acq.terminations {
            dates.insert((t.lot_id.clone(), t.effective_ordinal));
        }
    }
    let mut hedge_entries = Vec::new();
    let mut resolved = Vec::new();
    validate_ongoing_contracts(&request, p)?;
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
    let margin_sensitivities =
        margin_sensitivities(&plan, &request.acquisitions, &request.hedge_contracts)?;
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
            journals.push(
                engine::replay_with_margin(
                    &plan,
                    acq,
                    &resolved,
                    &margin_sensitivities,
                    &a.account_id,
                )
                .map_err(refused)?,
            );
        }
    }
    Ok(Prepared {
        dated,
        plan,
        plan_sha256,
        hedge_entries,
        journals,
        margin_sensitivities,
        scale_tier,
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

/// Runs without research-method IM keep their original disclosure bytes.
const DYNAMIC_DISCLOSURE: &str = "Native acquisition and unwind replay with research-method dynamic IM. Purchases carry cost; financing advances post once with their pledge; hedge entries carry the native clean value; cash VM settles after the agreement lag. Research-method IM (research-sensitivity-im-1, a reviewed research method, not ISDA SIMM or a provider model) is recomputed at each declared date on the actually live legal portfolio from native one-basis-point discount-bucket sensitivities of each live swap: signed sensitivities net across offsetting trades, the requirement is the correlated weighted norm, posted IM resets to it with cash and two-way received IM is segregated custody memo only. A population change without a recomputation, or a live lot without a native sensitivity, refuses. Floating projection is held to the contract's supplied rate path. Settlement is gated by declared dependencies; intents post once in the shared daily Sim beside the dated backbook. Undesignated economic treatment only.";

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
            .map(|j| JournalReceipt {
                account_id: j.account_id.clone(),
                scenario_id: j.scenario_id.clone(),
                receipt: j.receipt.clone(),
            })
            .collect(),
        margin_sensitivities: prepared.margin_sensitivities.clone(),
        formal_hedge_accounting: false,
        scale_tier: prepared.scale_tier.clone(),
        disclosure: if prepared.journals.iter().any(|j| {
            j.receipt.events.iter().any(|e| {
                e.event_id.starts_with("derivative_update:")
                    || e.event_id.starts_with("facility_accrual:")
                    || e.event_id.starts_with("vm_review:")
            })
        }) {
            "Explicit ongoing economic cashflows: immutable supplied swap coupon paths, native post-payment valuations, once-only coupons and termination consideration; current legal-population VM with reviewed thresholds and settlement lag; reviewed daily simple facility interest on prior-day closing drawn principal with separate accrual and payment. Mark P&L, coupon income and funding expense are separate. Unpaid facility interest survives the reporting cutoff. Research IM, when selected, retains its reviewed method and independent sensitivity verification. All intents share the dated journal and independent Rust persisted replay. No formal ASC 815 qualification, provider margin certification, observed calibration or regulatory acceptance is inferred."
        } else if !prepared.margin_sensitivities.is_empty() {
            DYNAMIC_DISCLOSURE
        } else {
            "Native acquisition and unwind replay. Purchases carry cost (no par gain); financing advances post once with their pledge; hedge entries carry the native clean value with equal upfront consideration; cash VM settles after the agreement lag and supplied exact IM binds to the live portfolio, with received IM as custody memo. Sales remove carrying pro rata and realize consideration less carrying; voluntary repayments carry actual accrued and break costs and release pledges; terminations stop rights at the effective date, hold consideration and due cash as an unpaid claim until settlement, and return or apply VM exactly once; bridge facilities draw and repay within their commitment. Settlement is gated by declared dependencies: a failed leg blocks its dependents and nothing settled is undone. Intents post once in the shared daily Sim beside the dated backbook. Undesignated economic treatment only: sale and termination facts are reported for the ASC 815 owner, which posts no basis or OCI here. Post-entry derivative marks and coupons, dynamic margin, the independent Rust verifier and the Rust service are later stages."
        },
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
