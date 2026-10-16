//! Complete-origin continuation of genuinely acquired finite loan/fee owners.
//! A cut is a verified journal window, never a caller monetary checkpoint.
use super::{candidates, holdings};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    commercial_continuation as continuation, commercial_fee_source::adoption as fee,
    commercial_graph as graph,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, time::Instant};

pub const SCHEMA: &str = "commercial-candidate-holding-journal-1";
const MAX_RAW: usize = 2 << 20;
const MAX_OUTPUT: usize = 32 << 20;
const MAX_WORK: usize = 32_000_000;
const MAX_RETAINED: usize = 128 << 20;
const MAX_COMPONENT: usize = 8 << 20;
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
    pub completed_candidate: candidates::ResultData,
    pub revised_candidate: candidates::ResultData,
    pub window_tables: Tables,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub component_output_limit_bytes: usize,
    pub complete_origin_daily_sim_adoption: bool,
    pub exact_observed_monetary_prefix_retained: bool,
    pub original_credit_clock_restarted: bool,
    pub original_loan_anchor_refitted: bool,
    pub original_fee_anchor_refitted: bool,
    pub second_acquisition_posted: bool,
    pub prior_publication_verified: bool,
    pub cut_started_mixed_sim_adoption: bool,
    pub secondary_acquisition_adoption: bool,
    pub empirical_validation: bool,
    pub scope: &'static str,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("candidate holding admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("candidate holding admission overflow".into())
}
fn same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    checkpoint()?;
    if strict_json::encode(a, MAX_OUTPUT)? != strict_json::encode(b, MAX_OUTPUT)? {
        return Err(
            "candidate holding immutable original anchor or observed prefix changed".into(),
        );
    }
    Ok(())
}
fn loan_prefix(a: &graph::History, b: &graph::History, cut: i32) -> Result<(), String> {
    same(&(&a.path_id, a.weight), &(&b.path_id, b.weight))?;
    if cut <= a.source.source.current.asof_ordinal {
        return same(&a.source, &b.source);
    }
    continuation::validate_raw_prefix(&continuation::Request {
        schema: continuation::SCHEMA.into(),
        cut_ordinal: cut,
        completed_source: a.source.clone(),
        revised_source: b.source.clone(),
    })?;
    Ok(())
}
fn fee_prefix(a: &fee::History, b: &fee::History, cut: i32) -> Result<(), String> {
    same(&a.path_id, &b.path_id)?;
    let mut policy = b.policy.clone();
    policy.events = a.policy.events.clone();
    same(&a.policy, &policy)?;
    same(
        &a.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )?;
    same(
        &a.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )
}
/// Retain all original Q/P proofs, model/contract identities, costs, quotes,
/// original yields/spreads and allocation authorities. Only future histories
/// may change; their observed source prefix is checked before generation.
fn raw_prefix(r: &Request) -> Result<(), String> {
    let a = &r.completed_candidate;
    let b = &r.revised_candidate;
    if a.commercial.units.len() != b.commercial.units.len() {
        return Err("candidate holding unit coverage changed".into());
    }
    let mut normalized = b.commercial.clone();
    for ((old, new), folded) in a
        .commercial
        .units
        .iter()
        .zip(&b.commercial.units)
        .zip(&mut normalized.units)
    {
        if old.graph.positions.len() != new.graph.positions.len() {
            return Err("candidate holding position coverage changed".into());
        }
        for ((p, q), folded) in old
            .graph
            .positions
            .iter()
            .zip(&new.graph.positions)
            .zip(&mut folded.graph.positions)
        {
            if p.histories.len() != q.histories.len() {
                return Err("candidate holding scenario coverage changed".into());
            }
            for (x, y) in p.histories.iter().zip(&q.histories) {
                same(&x.scenario_id, &y.scenario_id)?;
                for (xs, ys) in [
                    (&x.physical, &y.physical),
                    (&x.pricing, &y.pricing),
                    (&x.pricing_down, &y.pricing_down),
                    (&x.pricing_up, &y.pricing_up),
                ] {
                    if xs.len() != ys.len() {
                        return Err("candidate holding path coverage changed".into());
                    }
                    for (x, y) in xs.iter().zip(ys) {
                        loan_prefix(x, y, r.cut_ordinal)?;
                    }
                }
            }
            folded.histories = p.histories.clone();
            match (&p.fees, &q.fees, &mut folded.fees) {
                (None, None, None) => {}
                (Some(x), Some(y), Some(z)) => {
                    if x.histories.len() != y.histories.len() {
                        return Err("candidate holding fee scenario coverage changed".into());
                    }
                    for (x, y) in x.histories.iter().zip(&y.histories) {
                        same(&x.scenario_id, &y.scenario_id)?;
                        for (xs, ys) in [
                            (&x.physical, &y.physical),
                            (&x.pricing, &y.pricing),
                            (&x.pricing_down, &y.pricing_down),
                            (&x.pricing_up, &y.pricing_up),
                        ] {
                            if xs.len() != ys.len() {
                                return Err("candidate holding fee path coverage changed".into());
                            }
                            for (x, y) in xs.iter().zip(ys) {
                                fee_prefix(x, y, r.cut_ordinal)?;
                            }
                        }
                    }
                    z.histories = x.histories.clone();
                }
                _ => return Err("candidate holding fee ownership changed".into()),
            }
        }
    }
    same(&a.commercial, &normalized)?;
    let mut av = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(b).map_err(|e| e.to_string())?;
    av.as_object_mut()
        .ok_or("candidate holding request object")?
        .remove("commercial");
    bv.as_object_mut()
        .ok_or("candidate holding request object")?
        .remove("commercial");
    same(&av, &bv)
}
fn prefix_rows<'a>(tables: &'a Tables, name: &str, cut: usize) -> Vec<&'a Value> {
    tables
        .get(name)
        .into_iter()
        .flatten()
        .filter(|r| r["day"].as_u64().is_some_and(|d| d <= cut as u64))
        .collect()
}
fn monetary_prefix(a: &Tables, b: &Tables, cut: usize) -> Result<(), String> {
    // Every dated source/group, report and transaction row is compared in its
    // original native order. This includes unweighted loan/fee snapshots when
    // allocation is zero, and exact fractional source clocks inside those rows.
    let names = a
        .keys()
        .chain(b.keys())
        .collect::<std::collections::BTreeSet<_>>();
    for name in names {
        same(&prefix_rows(a, name, cut), &prefix_rows(b, name, cut))?;
    }
    Ok(())
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    checkpoint()?;
    if portfolio_compute_control::active().is_none() || r.schema != SCHEMA {
        return Err("explicit controlled complete-origin candidate holding required".into());
    }
    let raw = strict_json::serialized_size(r, MAX_RAW)?;
    let a = candidates::admit_finite(&r.completed_candidate)?;
    let b = candidates::admit_finite(&r.revised_candidate)?;
    let origin = r.completed_candidate.as_of_ordinal;
    let cut = usize::try_from(
        r.cut_ordinal
            .checked_sub(origin)
            .ok_or("candidate holding calendar overflow")?,
    )
    .map_err(|_| "candidate holding cut precedes origin")?;
    if cut == 0 || r.cut_ordinal > a.full_end_ordinal || r.cut_ordinal > b.full_end_ordinal {
        return Err("candidate holding cut must follow origin within both complete tails".into());
    }
    raw_prefix(r)?;
    // Both actual candidate receipts and the prospective window coexist. The
    // bounded final encoder/transport remains a separate32MiB component.
    let output = add(
        add(
            a.output_bytes_upper.min(MAX_COMPONENT),
            b.output_bytes_upper.min(MAX_COMPONENT),
        )?,
        MAX_COMPONENT,
    )?;
    let extra_work = add(mul(raw, 8)?, output / 8)?;
    let extra_bytes = add(output, mul(raw, 4)?)?;
    let work = add(add(a.work_upper, b.work_upper)?, extra_work)?;
    let bytes = add(
        add(a.retained_bytes_upper, b.retained_bytes_upper)?,
        extra_bytes,
    )?;
    if work > MAX_WORK || bytes > MAX_RETAINED {
        return Err(format!("candidate holding joined32M/128MiB before dual source generation: work={work},bytes={bytes}"));
    }
    let completed = candidates::evaluate_reserved_bounded(
        &r.completed_candidate,
        (
            add(b.work_upper, extra_work)?,
            add(b.retained_bytes_upper, extra_bytes)?,
        ),
        MAX_COMPONENT,
    )?;
    let revised = candidates::evaluate_reserved_bounded(
        &r.revised_candidate,
        (
            add(a.work_upper, extra_work)?,
            add(a.retained_bytes_upper, extra_bytes)?,
        ),
        MAX_COMPONENT,
    )?;
    monetary_prefix(&completed.tables, &revised.tables, cut)?;
    let window = holdings::window(
        &revised.tables,
        &revised.preparation.resolved_specification,
        cut,
    )?;
    strict_json::serialized_size(&window, MAX_COMPONENT)?;
    let full_end = revised.full_end_ordinal;
    let result=ResultData{schema:SCHEMA,raw_source:serde_json::to_value(r).map_err(|e|e.to_string())?,origin_asof_ordinal:origin,cut_ordinal:r.cut_ordinal,cut_day:cut,full_end_ordinal:full_end,
        completed_candidate:completed,revised_candidate:revised,window_tables:window,admitted_work_upper:work,retained_bytes_upper:bytes,component_output_limit_bytes:MAX_COMPONENT,
        complete_origin_daily_sim_adoption:true,exact_observed_monetary_prefix_retained:true,original_credit_clock_restarted:false,original_loan_anchor_refitted:false,original_fee_anchor_refitted:false,
        second_acquisition_posted:false,prior_publication_verified:false,cut_started_mixed_sim_adoption:false,secondary_acquisition_adoption:false,empirical_validation:false,
        scope:"two bounded complete-origin native candidate owners verify immutable loan/fee anchors and the complete observed ordered monetary prefix; revised acquired gross rights/basis and prior P&L form one prospective window; actual future loan/fee groups occur once; no caller checkpoint, verified prior publication, cut-started mixed Sim, secondary rights or empirical evidence"};
    strict_json::serialized_size(&result, MAX_OUTPUT)?;
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
    if envelope.op != "analyze_commercial_candidate_holding_journal" || envelope.schema != SCHEMA {
        return Err("explicit candidate holding operation required".into());
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
        result.completed_candidate.preparation.raw_request =
            original["completed_candidate"].clone();
        result.revised_candidate.preparation.raw_request = original["revised_candidate"].clone();
        result.raw_source = original;
        strict_json::serialized_size(&result.completed_candidate, MAX_COMPONENT)?;
        strict_json::serialized_size(&result.revised_candidate, MAX_COMPONENT)?;
        strict_json::serialized_size(&result, MAX_OUTPUT)?;
        checkpoint()?;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
}
