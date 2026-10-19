//! `saved-hedge-mixed-1`: reviewed formal hedge-accounting-1 owners in the shared daily Sim.
//! Each saved-derivative run carries a hedge overlay (every hedge-accounting-1 field except
//! `derivative`); the hedge request is bound to the run's exact saved Physical source. The
//! formal owner's single journal (re-owned derivative postings plus basis, OCI and
//! reclassification) replaces the undesignated derivative intents once; hedge openings join
//! the shared opening. Hedged-item contractual cash stays with its own source owner.
use super::{opening_anchors, project_sources, raw_domain, source, work_upper, Owner, Run};
use portfolio_compute_control::checkpoint;
use portfolio_ledger_native::{
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    derivative_intent_owner as owner,
};
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    derivative_accounting::JournalLedger,
    hedge_accounting::{self, Account, HedgeAccountingRequest, HedgeAccountingResult},
    saved_derivative_source as saved,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{self, Write};

pub const SCHEMA: &str = "saved-hedge-mixed-1";
const OPENING: &str = "hedge_opening:";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HedgeRun {
    ledger_scenario: String,
    saved: saved::Request,
    hedge: Map<String, Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HedgeOwner {
    account_id: String,
    runs: Vec<HedgeRun>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    dated: DatedLedgerRequest,
    owners: Vec<HedgeOwner>,
    #[serde(default)]
    scale_tier: Option<super::stream::ScaleTier>,
}

/// One kind per formal hedge journal account; every account is owned exactly once.
fn kind(account: Account) -> owner::Kind {
    use owner::Kind as K;
    match account {
        Account::Cash => K::Cash,
        Account::OpeningEquity => K::OpeningEquity,
        Account::DerivativeAsset => K::Asset,
        Account::DerivativeLiability => K::Liability,
        Account::UnpaidReceivable => K::Receivable,
        Account::UnpaidPayable => K::Payable,
        Account::PostedVmAsset => K::PostedVm,
        Account::ReceivedVmLiability => K::ReceivedVm,
        Account::PostedImAsset => K::PostedIm,
        Account::MarkGainLoss => K::Mark,
        Account::ContractualIncome => K::Coupon,
        Account::CollateralInterestIncome => K::CollateralInterest,
        Account::CounterpartyLossExpense => K::CounterpartyLoss,
        Account::PostedCollateralReturnLossExpense => K::CollateralLoss,
        Account::MemoCustodianAsset => K::CustodyAsset,
        Account::MemoCustodianObligation => K::CustodyObligation,
        Account::CashFlowHedgeOci => K::HedgeOci,
        Account::HedgedItemBasisAdjustment => K::HedgeBasis,
        Account::HedgedItemBasisGainLoss => K::HedgeBasisPnl,
        Account::HedgedItemBasisAmortization => K::HedgeBasisAmortization,
        Account::HedgedItemBasisDerecognition => K::HedgeBasisDerecognition,
        Account::HedgedForecastReclassification => K::HedgeReclassification,
    }
}
fn hedge_account(k: owner::Kind) -> bool {
    use owner::Kind as K;
    matches!(
        k,
        K::HedgeOci
            | K::HedgeBasis
            | K::HedgeBasisPnl
            | K::HedgeBasisAmortization
            | K::HedgeBasisDerecognition
            | K::HedgeReclassification
    )
}
/// Derivative accounts keep the existing `@derivative:` keys; hedge accounts are keyed by
/// book and relationship under a separate `@hedge:` namespace.
fn line(
    key: &str,
    account: Account,
    position: Option<&str>,
    bucket: Option<&str>,
    relationship: Option<&str>,
    debit: f64,
) -> Result<owner::Line, String> {
    let k = kind(account);
    let instrument = if matches!(k, owner::Kind::Cash | owner::Kind::OpeningEquity) {
        String::new()
    } else if hedge_account(k) {
        // Basis and OCI belong to the relationship: recognition, reclassification, amortization
        // and derecognition net within one key, so the complete tail closes per instrument.
        let relationship = relationship.ok_or("hedge posting requires its relationship")?;
        format!(
            "@hedge:{}",
            serde_json::to_string(&(key, relationship)).map_err(|e| e.to_string())?
        )
    } else {
        format!(
            "@derivative:{}",
            serde_json::to_string(&(key, position, bucket)).map_err(|e| e.to_string())?
        )
    };
    Ok(owner::Line {
        kind: k,
        instrument,
        debit,
    })
}
fn bind(
    physical: &saved::Request,
    overlay: &Map<String, Value>,
) -> Result<HedgeAccountingRequest, String> {
    if overlay.contains_key("derivative") {
        return Err("hedge overlay cannot carry its own derivative; it is bound to the saved Physical source".into());
    }
    let mut value = overlay.clone();
    value.insert(
        "derivative".into(),
        serde_json::to_value(&physical.physical).map_err(|e| e.to_string())?,
    );
    let request: HedgeAccountingRequest =
        serde_json::from_value(Value::Object(value)).map_err(|e| format!("hedge overlay: {e}"))?;
    if request.entity != physical.physical.entity
        || request.currency != physical.physical.currency
        || request.monetary_unit != physical.physical.monetary_unit
    {
        return Err("hedge overlay entity/currency/unit differs from the saved source".into());
    }
    Ok(request)
}
fn hedge_source<'a>(
    run: &Run,
    derivative: &'a saved::ResultData,
    hedge: &'a HedgeAccountingResult,
    account: &str,
) -> Result<owner::Source<'a>, String> {
    // The derivative opening anchors are built once, by the existing derivative projection.
    let mut base = source(run, derivative, account)?;
    let key = base.book_key.clone();
    let mut opening = std::mem::take(&mut base.opening);
    let mut intents = Vec::with_capacity(hedge.intents.len());
    for intent in &hedge.intents {
        checkpoint()?;
        let lines = intent
            .postings
            .iter()
            .map(|p| {
                line(
                    &key,
                    p.account,
                    p.position_id.as_deref(),
                    p.bucket.as_deref(),
                    p.relationship_id.as_deref(),
                    p.debit_amount,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if intent.event_id.starts_with(OPENING) {
            // Retained seasoned balances join the single shared opening, never a daily intent.
            if intent.ordinal != base.asof || intent.ledger != JournalLedger::Financial {
                return Err("hedge opening must be a financial posting on the opening date".into());
            }
            opening.extend(lines);
            continue;
        }
        intents.push(owner::Intent {
            event_id: intent.event_id.as_str(),
            ordinal: intent.ordinal,
            memo: intent.ledger == JournalLedger::CustodyMemo,
            lines,
        });
    }
    let daily = hedge
        .daily
        .iter()
        .map(|day| owner::Day {
            ordinal: day.ordinal,
            financial: day
                .financial_trial_balance
                .iter()
                .map(|b| (kind(b.account), b.debit_balance))
                .collect(),
            custody: day
                .custody_trial_balance
                .iter()
                .map(|b| (kind(b.account), b.debit_balance))
                .collect(),
        })
        .collect();
    base.opening = opening;
    base.intents = intents;
    base.daily = daily;
    Ok(base)
}
pub fn execute(bytes: &[u8]) -> Result<Vec<u8>, String> {
    execute_with(bytes, None)?.ok_or_else(|| "buffered mixed result required".into())
}
/// `streaming` selects the acceptance tier's header-then-journal output; `None` keeps the
/// historical single buffered envelope.
pub(crate) fn execute_with(
    bytes: &[u8],
    streaming: Option<super::stream::Streaming<'_>>,
) -> Result<Option<Vec<u8>>, String> {
    let original = strict_json::parse(bytes, super::stream::MAX_TIER_BYTES)?;
    raw_domain::validate(&original)?;
    let request: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let limits = super::stream::limits(request.scale_tier);
    let tier = request.scale_tier;
    if request.schema != SCHEMA
        || request.owners.is_empty()
        || request.owners.len() > 64
        || bytes.len() > limits.raw_bytes
        || streaming.is_some() != tier.is_some()
    {
        return Err("explicit saved hedge mixed schema/owners/tier required".into());
    }
    // Bind every overlay to its exact saved Physical source before any valuation.
    let mut owners = Vec::with_capacity(request.owners.len());
    let mut hedge_requests = Vec::new();
    for o in request.owners {
        let first = o.runs.first().ok_or("owner has no complete run source")?;
        let reference = serde_json::to_vec(&first.hedge).map_err(|e| e.to_string())?;
        let mut runs = Vec::with_capacity(o.runs.len());
        for run in o.runs {
            checkpoint()?;
            if serde_json::to_vec(&run.hedge).map_err(|e| e.to_string())? != reference {
                return Err("reviewed hedge overlay changes across scenario runs".into());
            }
            hedge_requests.push(bind(&run.saved, &run.hedge)?);
            runs.push(Run {
                ledger_scenario: run.ledger_scenario,
                saved: run.saved,
            });
        }
        owners.push(Owner {
            account_id: o.account_id,
            runs,
        });
    }
    for r in &hedge_requests {
        hedge_accounting::validate_request(r)?;
    }
    // Formal hedge journals are generated under their own hedge-accounting-1 admission.
    let mut hedges = Vec::with_capacity(hedge_requests.len());
    for r in &hedge_requests {
        checkpoint()?;
        hedges.push(hedge_accounting::evaluate(r)?);
    }
    let mut opening = opening_anchors(&owners)?;
    let mut index = 0;
    for (o, anchor) in owners.iter().zip(opening.iter_mut()) {
        // Retained seasoned basis is a signed carrying adjustment in the opening balance sheet.
        let basis: f64 = hedges[index]
            .intents
            .iter()
            .filter(|i| i.event_id.starts_with(OPENING))
            .flat_map(|i| &i.postings)
            .filter(|p| p.account == Account::HedgedItemBasisAdjustment)
            .map(|p| p.debit_amount)
            .sum();
        anchor.assets += basis;
        index += o.runs.len();
    }
    let dated = CompiledDatedLedger::compile_with_native_opening(
        request.dated,
        limits.positions,
        limits.work,
        &opening,
    )?;
    if !dated.spec.netting_sets.is_empty() || !dated.spec.reverse_severities.is_empty() {
        return Err(
            "legacy netting/reverse overlays require explicit joint source coverage".into(),
        );
    }
    let mixed_days = usize::try_from(
        i64::from(dated.store.reporting_end_ordinal()) - i64::from(dated.store.as_of_ordinal()) + 1,
    )
    .map_err(|_| "source mixed days overflow")?;
    let mut reserve = dated.work_units;
    let mut count = 0usize;
    for o in &owners {
        for run in &o.runs {
            checkpoint()?;
            portfolio_risk_native::derivative_accounting::validate_request(&run.saved.physical)?;
            let replay = work_upper::replay_upper(&run.saved.physical, mixed_days)?;
            reserve = reserve
                .checked_add(250_000)
                .and_then(|n| n.checked_add(replay))
                .ok_or("source work overflow")?;
            count += 1;
        }
    }
    for h in &hedges {
        // Formal postings beyond the derivative owner: each line is scanned through the tail.
        let lines = h
            .intents
            .iter()
            .try_fold(0usize, |n, i| n.checked_add(i.postings.len()))
            .ok_or("hedge line overflow")?;
        reserve = lines
            .checked_mul(mixed_days)
            .and_then(|n| n.checked_mul(6))
            .and_then(|n| n.checked_add(reserve))
            .ok_or("hedge work overflow")?;
    }
    if count == 0 || count > 64 || reserve > limits.work {
        return Err("joined raw derivative/hedge source work admission".into());
    }
    let results = project_sources(&owners, &dated)?;
    for (result, hedge) in results.iter().zip(&hedges) {
        // The formal journal must derive from exactly the verified saved derivative source.
        if serde_json::to_vec(&result.source).map_err(|e| e.to_string())?
            != serde_json::to_vec(&hedge.derivative).map_err(|e| e.to_string())?
        {
            return Err("hedge journal derivative source differs from the saved source".into());
        }
    }
    let mut sources = Vec::new();
    let mut index = 0;
    for o in &owners {
        for run in &o.runs {
            sources.push(hedge_source(
                run,
                &results[index],
                &hedges[index],
                &o.account_id,
            )?);
            index += 1;
        }
    }
    let compiled = owner::Compiled::new_with_limit(&dated, sources, limits.work)?;
    if compiled.work > reserve {
        return Err("observed hedge work exceeds structural preflight reservation".into());
    }
    if let Some(s) = streaming {
        #[derive(Serialize)]
        struct Receipt<'a> {
            schema: &'static str,
            scale_tier: super::stream::ScaleTier,
            input_sha256: &'a str,
            native_sources: &'a [saved::ResultData],
            native_hedges: &'a [HedgeAccountingResult],
            joined_work_units: usize,
            owner_capacity_bytes: usize,
            empirical_validation: bool,
            accounting_standard_approval: bool,
            capital_ftp_eligible: bool,
        }
        let receipt = Receipt {
            schema: SCHEMA,
            scale_tier: tier.ok_or("tier")?,
            input_sha256: &s.input_sha256,
            native_sources: &results,
            native_hedges: &hedges,
            joined_work_units: compiled.work,
            owner_capacity_bytes: compiled.capacity,
            empirical_validation: false,
            accounting_standard_approval: false,
            capital_ftp_eligible: false,
        };
        super::stream::header(s.output, SCHEMA, &receipt, &s.input_sha256)?;
        super::stream::run_streaming(&dated, &compiled, s.output)?;
        return Ok(None);
    }
    #[derive(Clone)]
    struct Output(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for Output {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            checkpoint().map_err(io::Error::other)?;
            let mut buffer = self.0.lock().map_err(|_| io::Error::other("output lock"))?;
            if buffer
                .len()
                .checked_add(data.len())
                .is_none_or(|n| n > 32 * 1024 * 1024)
            {
                return Err(io::Error::other("mixed output byte admission"));
            }
            buffer.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let output = Output(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    portfolio_ledger_native::daily::run_dated_with_derivatives(
        &dated,
        &compiled,
        Box::new(output.clone()),
    )?;
    let ndjson = String::from_utf8(output.0.lock().map_err(|_| "output lock")?.clone())
        .map_err(|e| e.to_string())?;
    checkpoint()?;
    #[derive(Serialize)]
    struct Envelope<'a> {
        schema: &'static str,
        original_request: &'a Value,
        native_sources: &'a [saved::ResultData],
        native_hedges: &'a [HedgeAccountingResult],
        native_stream: &'a str,
        joined_work_units: usize,
        owner_capacity_bytes: usize,
        empirical_validation: bool,
        accounting_standard_approval: bool,
        capital_ftp_eligible: bool,
    }
    let data = Envelope {
        schema: SCHEMA,
        original_request: &original,
        native_sources: &results,
        native_hedges: &hedges,
        native_stream: &ndjson,
        joined_work_units: compiled.work,
        owner_capacity_bytes: compiled.capacity,
        empirical_validation: false,
        accounting_standard_approval: false,
        capital_ftp_eligible: false,
    };
    strict_json::encode(&data, 32 * 1024 * 1024).map(Some)
}
#[cfg(test)]
mod tests {
    use super::*;
    use portfolio_compute_control::{scope, Control};
    fn raw() -> Value {
        serde_json::from_str(include_str!("../../../../tests/fixtures/hedge-mixed.json")).unwrap()
    }
    fn run(v: &Value) -> Result<Value, String> {
        let bytes = serde_json::to_vec(v).unwrap();
        scope(Some(Control::new(120_000).unwrap()), || execute(&bytes))
            .map(|out| serde_json::from_slice(&out).unwrap())
    }
    #[test]
    fn formal_hedge_journal_posts_once_in_the_shared_sim() {
        let out = run(&raw()).unwrap();
        assert_eq!(out["schema"], SCHEMA);
        assert_eq!(out["native_hedges"].as_array().unwrap().len(), 2);
        assert_eq!(out["accounting_standard_approval"], false);
        let stream = out["native_stream"].as_str().unwrap();
        assert_eq!(
            stream.lines().last().unwrap(),
            "{\"complete\":true,\"protocol\":3}"
        );
        let mut gls = std::collections::BTreeSet::new();
        for line in stream.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            if v["table"] == "derivative_journal_lineage" {
                for row in v["rows"].as_array().unwrap() {
                    gls.insert(row["gl_account"].as_str().unwrap().to_string());
                    let id = row["instrument_id"].as_str().unwrap();
                    let hedge = row["gl_account"].as_str().unwrap().contains("hedge");
                    assert!(
                        id.is_empty()
                            || id.starts_with(if hedge { "@hedge:" } else { "@derivative:" })
                    );
                }
            }
        }
        for gl in [
            "cash_flow_hedge_oci",
            "hedged_item_basis_adjustment",
            "pnl:hedged_item_basis",
            "pnl:hedged_forecast_reclassification",
        ] {
            assert!(gls.contains(gl), "{gl} missing from {gls:?}");
        }
    }
    #[test]
    fn refusals_without_fallback() {
        type Change = Box<dyn Fn(&mut Value)>;
        let cases: Vec<(&str, Change)> = vec![
            (
                "schema",
                Box::new(|v| v["schema"] = "saved-derivative-mixed-1".into()),
            ),
            (
                "own derivative",
                Box::new(|v| {
                    let d = v["owners"][0]["runs"][0]["saved"]["physical"].clone();
                    for r in v["owners"][0]["runs"].as_array_mut().unwrap() {
                        r["hedge"]["derivative"] = d.clone();
                    }
                }),
            ),
            (
                "overlay differs",
                Box::new(|v| {
                    let mut second = v["owners"][0]["runs"][0].clone();
                    second["ledger_scenario"] = "zero".into();
                    second["hedge"]["run"]["revision"] = "r2".into();
                    v["owners"][0]["runs"].as_array_mut().unwrap().push(second);
                }),
            ),
            (
                "entity",
                Box::new(|v| {
                    for r in v["owners"][0]["runs"].as_array_mut().unwrap() {
                        r["hedge"]["entity"] = "other".into();
                    }
                }),
            ),
            (
                "unknown overlay field",
                Box::new(|v| {
                    for r in v["owners"][0]["runs"].as_array_mut().unwrap() {
                        r["hedge"]["extra"] = 1.into();
                    }
                }),
            ),
            (
                "no owners",
                Box::new(|v| v["owners"] = serde_json::json!([])),
            ),
        ];
        for (label, change) in cases {
            let mut v = raw();
            change(&mut v);
            assert!(run(&v).is_err(), "{label} must reject");
        }
    }
}
