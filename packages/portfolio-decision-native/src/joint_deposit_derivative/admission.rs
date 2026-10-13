use super::{deposit, Request, MAX_INPUT};
use crate::saved_derivative_mixed::work_upper;
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::derivative_contract::Operation;
use serde::Serialize;
use serde_json::Value;
#[derive(Serialize)]
pub struct Admission {
    pub deposit: deposit::Admission,
    pub derivative_work_upper: usize,
    pub derivative_retained_upper: usize,
    pub joined_work_upper: usize,
    pub joined_retained_upper: usize,
    pub stream_and_header_staging: usize,
    pub derivative_sources: usize,
    pub raw_joint_arena_upper: usize,
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("joint admission overflow".into())
}
pub(super) fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or("joint admission overflow".into())
}
fn strings(v: &Value, maximum: &mut usize) -> Result<(), String> {
    checkpoint()?;
    match v {
        Value::String(s) => {
            *maximum = (*maximum).max(serde_json::to_vec(s).map_err(|e| e.to_string())?.len());
        }
        Value::Array(a) => {
            for x in a {
                strings(x, maximum)?;
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                strings(&Value::String(k.clone()), maximum)?;
                strings(x, maximum)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Whole supported saved/accounting domain: existing structural admission is called
/// without native marks/cash generation. Dynamic records cover all trades, states,
/// MPOR slices, curves, observations, collateral/default paths and raw copies.
fn array_items(v: &Value) -> Result<usize, String> {
    checkpoint()?;
    match v {
        Value::Array(a) => a.iter().try_fold(a.len(), |n, x| add(n, array_items(x)?)),
        Value::Object(o) => o.values().try_fold(0, |n, x| add(n, array_items(x)?)),
        _ => Ok(0),
    }
}
pub(super) fn derivative_upper(
    r: &Request,
    mixed_days: usize,
) -> Result<(usize, usize, usize), String> {
    if r.derivative_owners.is_empty() || r.derivative_owners.len() > 64 {
        return Err("joint bounded complete derivative owners required".into());
    }
    let mut work = 0;
    let mut bytes = 0;
    let mut count = 0;
    for o in &r.derivative_owners {
        if o.runs.is_empty() || o.runs.len() > 64 {
            return Err("joint bounded complete derivative runs required".into());
        }
        for run in &o.runs {
            checkpoint()?;
            portfolio_risk_native::saved_derivative_source::validate_mapping(&run.saved)?;
            portfolio_risk_native::derivative_accounting::validate_request(&run.saved.physical)?;
            let mut node_records = 0;
            for raw in std::iter::once(&run.saved.physical.derivative).chain(
                run.saved
                    .physical
                    .pricing_xva_reference
                    .iter()
                    .map(Box::as_ref),
            ) {
                let Operation::Netting { request: csa } = &raw.operation else {
                    return Err("joint raw netting source required".into());
                };
                for path in &csa.paths {
                    //64 per-date fixed channels+16 per legal trade/bucket, plus all
                    // raw nested element slots below. No option/default domain pruning.
                    node_records = add(
                        node_records,
                        mul(path.nodes.len(), add(64, mul(csa.mapping.len(), 16)?)?)?,
                    )?;
                }
            }
            let value = serde_json::to_value(&run.saved).map_err(|e| e.to_string())?;
            let mut escaped = 0;
            strings(&value, &mut escaped)?;
            let identity = serde_json::to_vec(&run.saved.book.identity)
                .map_err(|e| e.to_string())?
                .len();
            let cell = add(1024, add(mul(escaped, 8)?, mul(identity, 4)?)?)?;
            //8 slots per raw nested element covers generated schedules, state/payoff
            // rows, distinct obligation origins/actions and source-map rows. Fourfold
            // charge includes arenas/indexes and bounded serialization temporaries.
            let records = mul(add(node_records, mul(array_items(&value)?, 8)?)?, 4)?;
            let generated = mul(records, cell)?;
            let raw_bytes = strict_json::serialized_size(&run.saved, MAX_INPUT)?;
            bytes = add(bytes, add(generated, mul(raw_bytes, 16)?)?)?;
            let replay = work_upper::replay_upper(&run.saved.physical, mixed_days)?;
            work = add(work, add(add(250_000, mul(raw_bytes, 8)?)?, replay)?)?;
            count = add(count, 1)?;
        }
    }
    Ok((work, bytes, count))
}
