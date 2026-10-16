//! Genuine native completed-prefix -> moved Sim -> future candidate journal.
//! Caller JSON can select raw contracts, never a cut monetary checkpoint.
use super::{candidate_holdings, candidates};
use portfolio_compute_control::checkpoint;
use portfolio_ledger_native::daily;
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

pub const SCHEMA: &str = "commercial-cut-mixed-journal-1";
const MAX_RAW: usize = 2 << 20;
const MAX_OUTPUT: usize = 32 << 20;
const MAX_COMPONENT: usize = 4 << 20;
const MAX_WORK: usize = 32_000_000;
const MAX_BYTES: usize = 128 << 20;
const CUT_WORK: usize = 8_000_000;
const CUT_BYTES: usize = 32 << 20;
const VALUE_PHASE_BYTES: usize = 64 << 20;
const VALUE_ENCODED_UPPER: usize = VALUE_PHASE_BYTES / 20;
type Tables = BTreeMap<String, Vec<Value>>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub cut_ordinal: i32,
    pub completed_candidate: candidates::Request,
    pub revised_candidate: candidates::Request,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Value,
    pub origin_asof_ordinal: i32,
    pub cut_ordinal: i32,
    pub cut_day: usize,
    pub full_end_ordinal: i32,
    pub completed_preparation: candidates::Preparation,
    pub revised_preparation: candidates::Preparation,
    pub cut_tables: Tables,
    pub producer_completion: Value,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub cut_component_work_reservation: usize,
    pub cut_component_bytes_reservation: usize,
    pub component_output_limit_bytes: usize,
    pub appended_candidates: usize,
    pub completed_prefix_daily_sim_executed_once: bool,
    pub resumed_future_daily_sim_executed_once: bool,
    pub complete_origin_source_owners_retained: bool,
    pub cut_started_mixed_sim_adoption: bool,
    pub original_credit_clock_restarted: bool,
    pub original_loan_anchor_refitted: bool,
    pub original_fee_anchor_refitted: bool,
    pub surviving_second_acquisition_posted: bool,
    pub prior_publication_verified: bool,
    pub opening_identity_verified: bool,
    pub general_mixed_book_adoption: bool,
    pub empirical_validation: bool,
    pub scope: &'static str,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("native cut admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("native cut admission overflow".into())
}
fn same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    checkpoint()?;
    if strict_json::encode(a, MAX_RAW)? != strict_json::encode(b, MAX_RAW)? {
        return Err(
            "native cut immutable owner/original anchor or observed raw prefix differs".into(),
        );
    }
    Ok(())
}
/// The surviving complete source cannot be replaced by a cut-started contract.
/// Appended sources have actual future acquisition dates and no observed groups.
pub fn validate_raw_prefix(r: &Request) -> Result<usize, String> {
    let a = &r.completed_candidate;
    let b = &r.revised_candidate;
    let n = a.commercial.units.len();
    if b.commercial.units.len() < n || b.allocation.len() < n || b.owners.len() < n {
        return Err("native cut cannot remove surviving units/allocation/owners".into());
    }
    same(&a.as_of_ordinal, &b.as_of_ordinal)?;
    same(
        &a.commercial
            .reporting_ordinals
            .iter()
            .filter(|d| **d <= r.cut_ordinal)
            .collect::<Vec<_>>(),
        &b.commercial
            .reporting_ordinals
            .iter()
            .filter(|d| **d <= r.cut_ordinal)
            .collect::<Vec<_>>(),
    )?;
    let mut folded: candidates::Request =
        serde_json::from_value(serde_json::to_value(b).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    folded.commercial.units.truncate(n);
    folded.allocation.truncate(n);
    folded.owners.truncate(n);
    folded.reporting_end_ordinal = a.reporting_end_ordinal;
    folded.commercial.reporting_ordinals = a.commercial.reporting_ordinals.clone();
    for (old, new) in a.commercial.units.iter().zip(&mut folded.commercial.units) {
        same(
            &old.graph
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= r.cut_ordinal)
                .collect::<Vec<_>>(),
            &new.graph
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= r.cut_ordinal)
                .collect::<Vec<_>>(),
        )?;
        new.graph.reporting_ordinals = old.graph.reporting_ordinals.clone();
    }
    let positions = folded
        .specification
        .get_mut("positions")
        .and_then(Value::as_array_mut)
        .ok_or("native cut placeholder array absent")?;
    if positions.len() < n {
        return Err("native cut surviving placeholder coverage differs".into());
    }
    positions.truncate(n);
    // The ordinary candidate route normalizes its reporting calendar to the
    // complete source tail. Both exact admissions prove the same full horizon.
    if let Some(days) = a.specification.get("horizon_days") {
        folded.specification["horizon_days"] = days.clone();
    }
    let completed: candidates::Request =
        serde_json::from_value(serde_json::to_value(a).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    candidate_holdings::raw_prefix(&candidate_holdings::Request {
        schema: candidate_holdings::SCHEMA.into(),
        cut_ordinal: r.cut_ordinal,
        completed_candidate: completed,
        revised_candidate: folded,
    })?;
    for unit in b.commercial.units.iter().skip(n) {
        if unit.graph.as_of_ordinal <= r.cut_ordinal {
            return Err("native cut appended candidate acquisition must follow cut".into());
        }
    }
    Ok(b.commercial.units.len() - n)
}
#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let mut out = self
            .0
            .lock()
            .map_err(|_| io::Error::other("native cut stream poisoned"))?;
        if out
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_COMPONENT)
        {
            return Err(io::Error::other("native cut stream exceeds4MiB component"));
        }
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        checkpoint().map_err(io::Error::other)
    }
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    checkpoint()?;
    if r.schema != SCHEMA || portfolio_compute_control::active().is_none() {
        return Err("explicit controlled native cut mixed journal required".into());
    }
    let raw = strict_json::serialized_size(r, MAX_RAW)?;
    let a = candidates::admit_finite_preparation(&r.completed_candidate)?;
    let b = candidates::admit_finite_preparation(&r.revised_candidate)?;
    let origin = r.completed_candidate.as_of_ordinal;
    let cut = usize::try_from(
        r.cut_ordinal
            .checked_sub(origin)
            .ok_or("native cut ordinal overflow")?,
    )
    .map_err(|_| "native cut precedes origin")?;
    if cut == 0 || r.cut_ordinal >= a.full_end_ordinal || a.full_end_ordinal != b.full_end_ordinal {
        return Err("native cut first tier requires a positive cut before the same complete joint horizon; differing tails require union admission".into());
    }
    // The entire checked cut component covers JSON-prefix scans as well as
    // numeric posting work. It is additional to both original source bounds.
    // The Value/transport phase coexists with both prepared native owners.
    let extra_work = add(CUT_WORK, add(mul(raw, 8)?, MAX_COMPONENT / 8)?)?;
    let extra_bytes = add(
        CUT_BYTES,
        add(VALUE_PHASE_BYTES, add(MAX_COMPONENT, mul(raw, 4)?)?)?,
    )?;
    let work = add(add(a.work_upper, b.work_upper)?, extra_work)?;
    let bytes = add(
        add(a.retained_bytes_upper, b.retained_bytes_upper)?,
        extra_bytes,
    )?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err(format!(
            "native cut joined32M/128MiB before dual graph generation: work={work},bytes={bytes}"
        ));
    }
    if mul(raw, 40)? > VALUE_PHASE_BYTES {
        return Err("native cut raw comparison Values exceed reserved64MiB phase".into());
    }
    let appended = validate_raw_prefix(r)?;
    let output = Buffer(Arc::default());
    let mut revised = None;
    let (completed, _) = candidates::with_finite_compiled(
        &r.completed_candidate,
        (
            add(b.work_upper, extra_work)?,
            add(b.retained_bytes_upper, extra_bytes)?,
        ),
        &mut |old_spec, _, old_owner| {
            let (prepared, _) = candidates::with_finite_compiled(
                &r.revised_candidate,
                (
                    add(a.work_upper, extra_work)?,
                    add(a.retained_bytes_upper, extra_bytes)?,
                ),
                &mut |new_spec, _, new_owner| {
                    daily::run_commercial_cut(
                        old_spec,
                        new_spec,
                        old_owner,
                        new_owner,
                        cut,
                        Box::new(output.clone()),
                    )
                },
            )?;
            strict_json::serialized_size(&prepared, MAX_COMPONENT)?;
            revised = Some(prepared);
            Ok(())
        },
    )?;
    strict_json::serialized_size(&completed, MAX_COMPONENT)?;
    let data = std::mem::take(&mut *output.0.lock().map_err(|_| "native cut stream poisoned")?);
    let (tables, completion) = candidates::decode_stream_bounded(&data, 8, VALUE_PHASE_BYTES)?;
    strict_json::serialized_size(&tables, MAX_COMPONENT)?;
    let result=ResultData{schema:SCHEMA,raw_source:serde_json::to_value(r).map_err(|e|e.to_string())?,origin_asof_ordinal:origin,cut_ordinal:r.cut_ordinal,cut_day:cut,full_end_ordinal:a.full_end_ordinal,
        completed_preparation:completed,revised_preparation:revised.ok_or("revised native cut preparation absent")?,cut_tables:tables,producer_completion:completion,admitted_work_upper:work,retained_bytes_upper:bytes,
        cut_component_work_reservation:CUT_WORK,cut_component_bytes_reservation:CUT_BYTES,component_output_limit_bytes:MAX_COMPONENT,appended_candidates:appended,
        completed_prefix_daily_sim_executed_once:true,resumed_future_daily_sim_executed_once:true,complete_origin_source_owners_retained:true,cut_started_mixed_sim_adoption:true,
        original_credit_clock_restarted:false,original_loan_anchor_refitted:false,original_fee_anchor_refitted:false,surviving_second_acquisition_posted:false,prior_publication_verified:false,opening_identity_verified:false,general_mixed_book_adoption:false,empirical_validation:false,
        scope:"one genuine native completed Sim prefix moves its complete live state into a distinct resumed Sim; original source clocks, gross rights, policy counters, original loan and fee anchors retained; new future commercial AC owners append zero state and acquire once; no deserialized monetary checkpoint, observed bank opening, native prior publication, arbitrary mixed owners or empirical/accounting approval"};
    strict_json::serialized_size(&result, MAX_OUTPUT)?;
    strict_json::serialized_size(&result, VALUE_ENCODED_UPPER)?;
    checkpoint()?;
    Ok(result)
}
pub fn run_envelope_started(bytes: &[u8], started: Instant) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Request,
    }
    let raw = strict_json::parse_checked(bytes, MAX_RAW, checkpoint)?;
    let original = raw["request"].clone();
    let envelope: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_commercial_cut_mixed_journal" || envelope.schema != SCHEMA {
        return Err("explicit native cut mixed journal operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(
            envelope.execution_timeout_ms,
            started,
        )?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut result = evaluate(&envelope.request)?;
        result.completed_preparation.raw_request = original["completed_candidate"].clone();
        result.revised_preparation.raw_request = original["revised_candidate"].clone();
        result.raw_source = original;
        strict_json::serialized_size(&result.completed_preparation, MAX_COMPONENT)?;
        strict_json::serialized_size(&result.revised_preparation, MAX_COMPONENT)?;
        strict_json::serialized_size(&result, MAX_OUTPUT)?;
        strict_json::serialized_size(&result, VALUE_ENCODED_UPPER)?;
        checkpoint()?;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
}
