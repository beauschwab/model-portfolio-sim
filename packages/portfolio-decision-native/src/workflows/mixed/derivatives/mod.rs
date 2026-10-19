//! Raw-only native coordinator: Risk generates sources, Ledger executes one Sim.
use portfolio_compute_control::checkpoint;
#[cfg(test)]
use portfolio_compute_control::{scope, Control};
use portfolio_ledger_native::{
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    derivative_intent_owner as owner,
};
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    derivative_accounting::{AccountCode as Code, JournalLedger},
    derivative_contract::{csa::Trade, Operation},
    saved_derivative_source as saved,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Run {
    pub(crate) ledger_scenario: String,
    pub(crate) saved: saved::Request,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) account_id: String,
    pub(crate) runs: Vec<Run>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    dated: DatedLedgerRequest,
    owners: Vec<Owner>,
    #[serde(default)]
    scale_tier: Option<stream::ScaleTier>,
}

fn kind(code: Code) -> owner::Kind {
    use owner::Kind as K;
    match code {
        Code::Cash => K::Cash,
        Code::OpeningEquity => K::OpeningEquity,
        Code::DerivativeAsset => K::Asset,
        Code::DerivativeLiability => K::Liability,
        Code::UnpaidReceivable => K::Receivable,
        Code::UnpaidPayable => K::Payable,
        Code::PostedVmAsset => K::PostedVm,
        Code::ReceivedVmLiability => K::ReceivedVm,
        Code::PostedImAsset => K::PostedIm,
        Code::MarkGainLoss => K::Mark,
        Code::ContractualIncome => K::Coupon,
        Code::CollateralInterestIncome => K::CollateralInterest,
        Code::CounterpartyLossExpense => K::CounterpartyLoss,
        Code::PostedCollateralReturnLossExpense => K::CollateralLoss,
        Code::MemoCustodianAsset => K::CustodyAsset,
        Code::MemoCustodianObligation => K::CustodyObligation,
    }
}
fn line(
    key: &str,
    k: owner::Kind,
    position: Option<&str>,
    bucket: Option<&str>,
    debit: f64,
) -> Result<owner::Line, String> {
    let instrument = if matches!(k, owner::Kind::Cash | owner::Kind::OpeningEquity) {
        String::new()
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
pub(crate) fn source<'a>(
    r: &Run,
    result: &'a saved::ResultData,
    account: &str,
) -> Result<owner::Source<'a>, String> {
    use owner::Kind as K;
    let physical = &r.saved.physical;
    let raw = &result.source;
    let Operation::Netting { request: csa } = &physical.derivative.operation else {
        return Err("raw netting required".into());
    };
    let key = serde_json::to_string(&r.saved.book.identity).map_err(|e| e.to_string())?;
    let bucket_for = |position: &str| -> Result<String, String> {
        let mapping = csa
            .mapping
            .iter()
            .find(|m| m.position_id == position)
            .ok_or("missing derivative legal owner")?;
        Ok(if mapping.enforceable_netting {
            mapping.netting_set_id.clone()
        } else {
            format!("trade:{position}")
        })
    };
    let buckets: BTreeSet<_> = csa
        .mapping
        .iter()
        .map(|m| bucket_for(&m.position_id))
        .collect::<Result<_, _>>()?;
    let first = buckets.first().ok_or("missing native collateral bucket")?;
    let opening = &raw.opening;
    let mut initial = vec![
        line(&key, K::Cash, None, None, opening.cash)?,
        line(
            &key,
            K::OpeningEquity,
            None,
            None,
            -opening.equity_credit_balance,
        )?,
        line(
            &key,
            K::PostedVm,
            None,
            Some(first),
            opening.posted_vm_asset,
        )?,
        line(
            &key,
            K::ReceivedVm,
            None,
            Some(first),
            -opening.received_vm_liability,
        )?,
        line(
            &key,
            K::PostedIm,
            None,
            Some(first),
            opening.posted_im_asset,
        )?,
        line(
            &key,
            K::CustodyAsset,
            None,
            Some(first),
            opening.received_im_custody,
        )?,
        line(
            &key,
            K::CustodyObligation,
            None,
            Some(first),
            -opening.received_im_custody,
        )?,
    ];
    for anchor in &opening.trades {
        let trade = r
            .saved
            .book
            .trades
            .iter()
            .find(|t| match t {
                Trade::Swap { request } => request.position_id == anchor.position_id,
                Trade::EuropeanSwaption { request } => request.position_id == anchor.position_id,
                Trade::ExpiredEuropean { terminal, .. } => {
                    terminal.position_id == anchor.position_id
                }
            })
            .ok_or("opening trade owner absent")?;
        let position = match trade {
            Trade::ExpiredEuropean {
                delivered_underlying: Some(u),
                ..
            } => u.position_id.as_str(),
            _ => anchor.position_id.as_str(),
        };
        let bucket = bucket_for(&anchor.position_id)?;
        initial.push(line(
            &key,
            K::Asset,
            Some(position),
            Some(&bucket),
            anchor.clean_value.max(0.),
        )?);
        initial.push(line(
            &key,
            K::Liability,
            Some(position),
            Some(&bucket),
            -(-anchor.clean_value).max(0.),
        )?);
    }
    let intents = raw
        .intents
        .iter()
        .map(|intent| {
            let lines = intent
                .postings
                .iter()
                .map(|p| {
                    line(
                        &key,
                        kind(p.account),
                        p.position_id.as_deref(),
                        p.bucket.as_deref(),
                        p.debit_amount,
                    )
                })
                .collect::<Result<_, _>>()?;
            Ok(owner::Intent {
                event_id: intent.event_id.as_str(),
                ordinal: intent.ordinal,
                memo: intent.ledger == JournalLedger::CustodyMemo,
                lines,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let daily = raw
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
    Ok(owner::Source {
        book_key: key,
        scenario: r.ledger_scenario.clone(),
        account_id: account.into(),
        entity: &raw.entity,
        currency: &raw.currency,
        monetary_unit: &raw.monetary_unit,
        asof: opening.ordinal,
        tail: csa.policy.tail_return_ordinal,
        opening_cash: opening.cash,
        opening: initial,
        intents,
        daily,
    })
}
pub(crate) fn opening_anchors(
    owners: &[Owner],
) -> Result<Vec<portfolio_ledger_native::spec::NativeOpening>, String> {
    let mut opening = Vec::new();
    for o in owners {
        let first = o
            .runs
            .first()
            .ok_or("missing raw derivative opening source")?;
        saved::validate_mapping(&first.saved)?;
        let anchor = &first.saved.physical.opening;
        opening.push(portfolio_ledger_native::spec::NativeOpening {
            account_id: o.account_id.clone(),
            assets: anchor
                .trades
                .iter()
                .map(|t| t.clean_value.max(0.))
                .sum::<f64>()
                + anchor.posted_vm_asset
                + anchor.posted_im_asset,
            liabilities: anchor
                .trades
                .iter()
                .map(|t| (-t.clean_value).max(0.))
                .sum::<f64>()
                + anchor.received_vm_liability,
        });
    }
    Ok(opening)
}
pub(crate) fn validate_owners_for_context(
    owners: &[Owner],
    spec: &portfolio_ledger_native::types::Spec,
    asof: i32,
    reporting_end: i32,
    expected_runs: &BTreeSet<String>,
) -> Result<(), String> {
    // Complete raw saved mappings and immutable book identity precede every source projection.
    let mut books = BTreeSet::new();
    let mut netting = BTreeSet::new();
    for o in owners {
        let first = o.runs.first().ok_or("owner has no complete run source")?;
        let account = spec
            .accounts
            .iter()
            .find(|a| a.id == o.account_id)
            .ok_or("saved derivative account absent")?;
        let key = serde_json::to_string(&first.saved.book.identity).map_err(|e| e.to_string())?;
        if !books.insert(key.clone()) {
            return Err("saved derivative book is owned twice".into());
        }
        let mut supplied_runs = BTreeSet::new();
        let book = serde_json::to_vec(&first.saved.book).map_err(|e| e.to_string())?;
        let original_opening =
            serde_json::to_vec(&first.saved.physical.opening).map_err(|e| e.to_string())?;
        for run in &o.runs {
            saved::validate_mapping(&run.saved)?;
            if !supplied_runs.insert(run.ledger_scenario.clone()) {
                return Err("duplicate derivative scenario source".into());
            }
            let Operation::Netting { request: csa } = &run.saved.physical.derivative.operation
            else {
                return Err("raw netting required".into());
            };
            if csa.asof_ordinal != asof
                || csa.policy.tail_return_ordinal > reporting_end
                || account.entity != run.saved.physical.entity
                || account.currency != run.saved.physical.currency
                || account.cash.to_bits() != run.saved.physical.opening.cash.to_bits()
            {
                return Err("complete derivative/date/entity/currency/opening pool differs".into());
            }
            if serde_json::to_vec(&run.saved.book).map_err(|e| e.to_string())? != book {
                return Err("saved derivative book changes across runs".into());
            }
            if serde_json::to_vec(&run.saved.physical.opening).map_err(|e| e.to_string())?
                != original_opening
            {
                return Err("reviewed derivative opening changes across runs".into());
            }
        }
        if &supplied_runs != expected_runs {
            return Err("incomplete derivative run coverage before projection".into());
        }
        let Operation::Netting { request: csa } = &first.saved.physical.derivative.operation else {
            return Err("raw netting required".into());
        };
        let buckets: BTreeSet<_> = csa
            .mapping
            .iter()
            .filter(|m| m.enforceable_netting)
            .map(|m| {
                (
                    o.account_id.clone(),
                    m.counterparty.clone(),
                    m.netting_set_id.clone(),
                )
            })
            .collect();
        for bucket in buckets {
            if !netting.insert(bucket) {
                return Err("legal netting bucket split across native owners".into());
            }
        }
    }
    Ok(())
}
#[cfg(test)]
thread_local! { pub(crate) static SOURCE_GENERATOR_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub(crate) fn project_sources(
    owners: &[Owner],
    dated: &CompiledDatedLedger,
) -> Result<Vec<saved::ResultData>, String> {
    let expected_runs = dated
        .store
        .runs()
        .iter()
        .map(|r| r.scenario_id.to_string())
        .collect();
    validate_owners_for_context(
        owners,
        &dated.spec,
        dated.store.as_of_ordinal(),
        dated.store.reporting_end_ordinal(),
        &expected_runs,
    )?;
    let mut results = Vec::new();
    let mut wire_bytes = 0usize;
    for o in owners {
        for run in &o.runs {
            checkpoint()?;
            #[cfg(test)]
            SOURCE_GENERATOR_CALLS.with(|c| c.set(c.get() + 1));
            let result = saved::evaluate(&run.saved)?;
            wire_bytes = wire_bytes
                .checked_add(strict_json::serialized_size(&result, 32 * 1024 * 1024)?)
                .ok_or("source wire count overflow")?;
            if wire_bytes > 64 * 1024 * 1024 {
                return Err("retained native derivative wire budget".into());
            }
            results.push(result);
        }
    }
    Ok(results)
}
pub fn execute(bytes: &[u8]) -> Result<Vec<u8>, String> {
    execute_with(bytes, None)?.ok_or_else(|| "buffered mixed result required".into())
}
/// `streaming` selects the acceptance tier's header-then-journal output; `None` keeps the
/// historical single buffered envelope.
pub(crate) fn execute_with(
    bytes: &[u8],
    streaming: Option<stream::Streaming<'_>>,
) -> Result<Option<Vec<u8>>, String> {
    let original = strict_json::parse(bytes, stream::MAX_TIER_BYTES)?;
    raw_domain::validate(&original)?;
    let request: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let limits = stream::limits(request.scale_tier);
    if request.schema != "saved-derivative-mixed-1"
        || request.owners.is_empty()
        || request.owners.len() > 64
        || bytes.len() > limits.raw_bytes
        || streaming.is_some() != request.scale_tier.is_some()
    {
        return Err("explicit saved derivative mixed schema/owners/tier required".into());
    }
    let opening = opening_anchors(&request.owners)?;
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
    let count = request.owners.iter().try_fold(0usize, |n, o| {
        n.checked_add(o.runs.len()).ok_or("source count overflow")
    })?;
    let mixed_days = usize::try_from(
        i64::from(dated.store.reporting_end_ordinal()) - i64::from(dated.store.as_of_ordinal()) + 1,
    )
    .map_err(|_| "source mixed days overflow")?;
    let mut reserve = dated.work_units;
    for o in &request.owners {
        for run in &o.runs {
            checkpoint()?;
            portfolio_risk_native::derivative_accounting::validate_request(&run.saved.physical)?;
            let replay = work_upper::replay_upper(&run.saved.physical, mixed_days)?;
            reserve = reserve
                .checked_add(250_000)
                .and_then(|n| n.checked_add(replay))
                .ok_or("source work overflow")?;
        }
    }
    if count == 0 || count > 64 || reserve > limits.work {
        return Err("joined raw derivative source work admission".into());
    }
    let results = project_sources(&request.owners, &dated)?;
    let mut index = 0;
    let mut sources = Vec::new();
    for o in &request.owners {
        for run in &o.runs {
            sources.push(source(run, &results[index], &o.account_id)?);
            index += 1;
        }
    }
    let compiled = owner::Compiled::new_with_limit(&dated, sources, limits.work)?;
    if compiled.work > reserve {
        return Err("observed derivative work exceeds structural preflight reservation".into());
    }
    if let Some(s) = streaming {
        #[derive(Serialize)]
        struct Receipt<'a> {
            schema: &'static str,
            scale_tier: stream::ScaleTier,
            input_sha256: &'a str,
            native_sources: &'a [saved::ResultData],
            joined_work_units: usize,
            owner_capacity_bytes: usize,
            empirical_validation: bool,
            capital_ftp_eligible: bool,
        }
        let tier = request.scale_tier.ok_or("tier")?;
        let receipt = Receipt {
            schema: "saved-derivative-mixed-1",
            scale_tier: tier,
            input_sha256: &s.input_sha256,
            native_sources: &results,
            joined_work_units: compiled.work,
            owner_capacity_bytes: compiled.capacity,
            empirical_validation: false,
            capital_ftp_eligible: false,
        };
        stream::header(
            s.output,
            "saved-derivative-mixed-1",
            &receipt,
            &s.input_sha256,
        )?;
        stream::run_streaming(&dated, &compiled, s.output)?;
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
        original_request: &'a serde_json::Value,
        native_sources: &'a [saved::ResultData],
        native_stream: &'a str,
        joined_work_units: usize,
        owner_capacity_bytes: usize,
        empirical_validation: bool,
        capital_ftp_eligible: bool,
    }
    let data = Envelope {
        schema: "saved-derivative-mixed-1",
        original_request: &original,
        native_sources: &results,
        native_stream: &ndjson,
        joined_work_units: compiled.work,
        owner_capacity_bytes: compiled.capacity,
        empirical_validation: false,
        capital_ftp_eligible: false,
    };
    strict_json::encode(&data, 32 * 1024 * 1024).map(Some)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../../tests/fixtures/derivative-mixed.json"
        ))
        .unwrap()
    }
    fn owned(f: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        scope(Some(Control::new(30_000).unwrap()), f)
    }
    #[test]
    fn physical_delivery_long_tail_work_refusal_precedes_generators() {
        owned(|| {
            let r: Request = serde_json::from_str(include_str!(
                "../../../../tests/fixtures/derivative-physical-option-longtail.json"
            ))
            .unwrap();
            let opening = opening_anchors(&r.owners)?;
            let dated = CompiledDatedLedger::compile_with_native_opening(
                r.dated,
                512,
                owner::MAX_WORK,
                &opening,
            )?;
            let count: usize = r.owners.iter().map(|o| o.runs.len()).sum();
            let old = dated.work_units + count * 250_000;
            let mut reserve = old;
            for o in &r.owners {
                for run in &o.runs {
                    portfolio_risk_native::derivative_accounting::validate_request(
                        &run.saved.physical,
                    )?;
                    reserve += work_upper::replay_upper(&run.saved.physical, 1081)?;
                }
            }
            assert!(old < owner::MAX_WORK);
            assert!(reserve > owner::MAX_WORK);
            SOURCE_GENERATOR_CALLS.with(|c| c.set(0));
            let e = execute(include_bytes!(
                "../../../../tests/fixtures/derivative-physical-option-longtail.json"
            ))
            .unwrap_err();
            assert!(
                e.contains("joined raw derivative source work admission"),
                "{e}"
            );
            assert_eq!(SOURCE_GENERATOR_CALLS.with(|c| c.get()), 0);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn delayed_multiple_pending_calls_settle_together_under_global_upper() {
        owned(|| {
            let r: Request = serde_json::from_str(include_str!(
                "../../../../tests/fixtures/derivative-multi-call-catchup.json"
            ))
            .unwrap();
            let run = &r.owners[0].runs[0];
            let result = saved::evaluate(&run.saved)?;
            let calls = &result.source.native.paths[0].calls;
            assert!(
                calls
                    .iter()
                    .filter(|c| c.settled_ordinal == Some(738891))
                    .count()
                    >= 3
            );
            assert!(calls
                .iter()
                .any(|c| c.due_ordinal < 738891 && c.settled_ordinal == Some(738891)));
            let mapped = source(run, &result, &r.owners[0].account_id)?;
            let lines: usize =
                mapped.opening.len() + mapped.intents.iter().map(|i| i.lines.len()).sum::<usize>();
            let bound = work_upper::replay_upper(&run.saved.physical, 31)?;
            assert!(lines * (1 + 6 * 31) <= bound);
            let output = execute(include_bytes!(
                "../../../../tests/fixtures/derivative-multi-call-catchup.json"
            ))?;
            let receipt: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert!(receipt["native_stream"]
                .as_str()
                .unwrap()
                .ends_with("{\"complete\":true,\"protocol\":3}\n"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn actual_complete_shared_sim() {
        owned(|| {
            let bytes = strict_json::encode(&raw(), 32 * 1024 * 1024)?;
            let output = execute(&bytes)?;
            let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert!(result["native_stream"]
                .as_str()
                .unwrap()
                .contains("\"cash\":200.8"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn complete_run_admission_precedes_projection() {
        owned(|| {
            let mut r = raw();
            r["owners"][0]["runs"].as_array_mut().unwrap().pop();
            assert!(execute(&serde_json::to_vec(&r).unwrap())
                .unwrap_err()
                .contains("run coverage before projection"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn changed_reviewed_opening_precedes_projection() {
        owned(|| {
            let mut r = raw();
            r["owners"][0]["runs"][1]["saved"]["physical"]["opening"]["equity_credit_balance"] =
                serde_json::json!(101.5);
            assert!(execute(&serde_json::to_vec(&r).unwrap())
                .unwrap_err()
                .contains("opening changes across runs"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn duplicate_source_closing_category_rejects() {
        owned(|| {
            let value = raw();
            let r: Request = serde_json::from_value(value).unwrap();
            let run = &r.owners[0].runs[0];
            let result = saved::evaluate(&run.saved)?;
            let opening = vec![portfolio_ledger_native::spec::NativeOpening {
                account_id: "a".into(),
                assets: 0.5,
                liabilities: 0.,
            }];
            let dated = CompiledDatedLedger::compile_with_native_opening(
                r.dated,
                512,
                owner::MAX_WORK,
                &opening,
            )?;
            let mut sources = vec![
                source(run, &result, "a")?,
                source(&r.owners[0].runs[1], &result, "a")?,
            ];
            sources[0].daily[0]
                .financial
                .push((owner::Kind::Cash, 100.5));
            assert!(owner::Compiled::new(&dated, sources)
                .err()
                .unwrap()
                .contains("uniqueness"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn generated_opening_must_bind_admitted_gross_stocks() {
        owned(|| {
            let r: Request = serde_json::from_value(raw()).unwrap();
            let run = &r.owners[0].runs[0];
            let result = saved::evaluate(&run.saved)?;
            let opening = vec![portfolio_ledger_native::spec::NativeOpening {
                account_id: "a".into(),
                assets: 0.5,
                liabilities: 0.,
            }];
            let mut dated = CompiledDatedLedger::compile_with_native_opening(
                r.dated,
                512,
                owner::MAX_WORK,
                &opening,
            )?;
            dated.native_opening[0].assets = 1.;
            let sources = vec![
                source(run, &result, "a")?,
                source(&r.owners[0].runs[1], &result, "a")?,
            ];
            assert!(owner::Compiled::new(&dated, sources)
                .err()
                .unwrap()
                .contains("admitted balance sheet"));
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn cancelled_control_does_not_produce_output() {
        let control = Control::new(30_000).unwrap();
        control.cancel();
        let bytes = serde_json::to_vec(&raw()).unwrap();
        assert!(scope(Some(control), || execute(&bytes)).is_err());
    }
}
pub mod hedges;
#[path = "public.rs"]
pub mod public;
pub mod stream;

#[path = "raw_domain.rs"]
pub(crate) mod raw_domain;
pub(crate) mod work_upper;
