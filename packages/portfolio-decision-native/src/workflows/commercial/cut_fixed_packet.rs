//! Controlled fixed allocation packet with complete raw-derived admission.
//! Original proofs are emitted once; current proofs remain in full preparations.
use super::{candidates, cut_mixed};
use portfolio_compute_control::checkpoint;
use portfolio_ledger_native::daily::{self, SequentialCommercialCut};
use portfolio_model_core::{strict_json, transport::prepaid_json};
use portfolio_risk_native::{
    commercial_graph::ProofScope, commercial_units::UnitSourceObservation,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    io::{self, Write},
    sync::{Arc, Mutex},
};

const ROOT_WORK: usize = 32_000_000;
const COMPONENT: usize = 4 << 20;
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("packet overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or("packet overflow".into())
}
fn tick(work: usize) -> Result<(), String> {
    checkpoint()?;
    daily::charge_cut_census_work(work)
}

#[derive(Clone)]
struct Stream(Arc<Mutex<Vec<u8>>>);
impl Stream {
    fn new() -> Result<Self, String> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(COMPONENT)
            .map_err(|e| e.to_string())?;
        if bytes.capacity() != COMPONENT {
            return Err("packet stream capacity differs".into());
        }
        Ok(Self(Arc::new(Mutex::new(bytes))))
    }
}
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let mut out = self
            .0
            .lock()
            .map_err(|_| io::Error::other("packet stream lock"))?;
        if out
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > COMPONENT)
        {
            return Err(io::Error::other("packet stream exceeds4MiB"));
        }
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn syntax(output: &RefCell<&mut dyn Write>, bytes: &[u8]) -> Result<(), String> {
    tick(mul(bytes.len(), 2)?)?;
    output
        .borrow_mut()
        .write_all(bytes)
        .map_err(|e| e.to_string())
}
fn emit<T: Serialize>(output: &RefCell<&mut dyn Write>, value: &T) -> Result<(), String> {
    prepaid_json::write_with_output_charge(&mut **output.borrow_mut(), value, tick)?;
    Ok(())
}
struct Limited<'a> {
    output: &'a mut dyn Write,
    remaining: usize,
}
impl Write for Limited<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        if bytes.len() > self.remaining {
            return Err(io::Error::other(
                "fixed packet output exceeds its existing limit",
            ));
        }
        let n = self.output.write(bytes)?;
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        checkpoint().map_err(io::Error::other)?;
        self.output.flush()
    }
}
fn preparation(
    output: &RefCell<&mut dyn Write>,
    value: &candidates::Preparation,
) -> Result<(), String> {
    let mut output = output.borrow_mut();
    let mut bounded = Limited {
        output: &mut **output,
        remaining: COMPONENT,
    };
    prepaid_json::write_with_output_charge(&mut bounded, value, tick)?;
    Ok(())
}
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    side: &'static str,
    #[serde(flatten)]
    source: UnitSourceObservation<'a>,
}

#[derive(Debug, Serialize)]
pub(super) struct Usage {
    source_preparation_work_upper: usize,
    raw_prefix_work_reservation: usize,
    controlled_work_budget: usize,
    controlled_work_spent: usize,
    joined_reported_work: usize,
    source_phase_bytes_upper_reported: usize,
    complete_resource_proof: bool,
    publication_eligible: bool,
}

#[derive(Serialize)]
struct Header<'a> {
    schema: &'a str,
    cut_ordinal: i32,
}

#[derive(Serialize)]
struct ResourceAdmission {
    completed_preparation_work_upper: usize,
    revised_preparation_work_upper: usize,
    completed_preparation_bytes_upper: usize,
    revised_preparation_bytes_upper: usize,
    typed_input_bytes_upper: usize,
    source_phase_bytes_upper: usize,
    source_preparation_work_upper: usize,
    raw_prefix_work_reservation: usize,
    controlled_work_budget: usize,
    maximum_work: usize,
    maximum_bytes: usize,
}

/// One original Control and one funded ticket span source preparation, the
/// complete moved Sim and direct serialization. No native Value reconstruction.
pub(super) fn write_packet(
    request: &cut_mixed::Request,
    typed_bytes_upper: usize,
    output: &mut dyn Write,
    limit: Option<usize>,
) -> Result<Usage, String> {
    checkpoint()?;
    if portfolio_compute_control::active().is_none() {
        return Err("fixed packet requires original execution control".into());
    }
    let preliminary = candidates::fixed_raw_reservation(&request.completed_candidate)?.max(
        candidates::fixed_raw_reservation(&request.revised_candidate)?,
    );
    if add(
        preliminary,
        add(typed_bytes_upper, add(32 << 20, add(COMPONENT, 64 << 10)?)?)?,
    )? > 128 << 20
    {
        return Err("fixed input/source expansion exceeds128MiB before preparation".into());
    }
    if request.schema != cut_mixed::SCHEMA || request.preparation_receipt_policy.is_some() {
        return Err("fixed packet requires its explicit ordinary cut contract".into());
    }
    let a = candidates::admit_fixed_finite_preparation(&request.completed_candidate)?;
    let b = candidates::admit_fixed_finite_preparation(&request.revised_candidate)?;
    cut_mixed::validate_raw_prefix(request)?;
    if a.full_end_ordinal != b.full_end_ordinal {
        return Err("packet complete horizons differ".into());
    }
    let cut = usize::try_from(request.cut_ordinal - request.completed_candidate.as_of_ordinal)
        .map_err(|e| e.to_string())?;
    let raw = strict_json::serialized_size(request, 2 << 20)?;
    let sources = add(a.work_upper, b.work_upper)?;
    let raw_work = mul(raw, 8)?;
    let remaining = ROOT_WORK
        .checked_sub(add(sources, raw_work)?)
        .ok_or("packet initial work exceeds32M")?;
    let budget = limit.unwrap_or(remaining).min(remaining);
    // Completed financial owners are dropped before revised construction.
    // Detached Sim/proof/sink storage stays within its separate32MiB cap.
    // The direct writer retains no encoded preparation or decoded Value.
    let phase = add(
        a.retained_bytes_upper.max(b.retained_bytes_upper),
        add(32 << 20, add(COMPONENT, add(typed_bytes_upper, 64 << 10)?)?)?,
    )?;
    if phase > 128 << 20 {
        return Err("packet source phase exceeds128MiB".into());
    }
    let stream = Stream::new()?;
    let mut bounded_output = Limited {
        output,
        remaining: (32 << 20) - 16,
    };
    let output = RefCell::new(&mut bounded_output as &mut dyn Write);
    let spent = daily::with_cut_census_budget(budget, || {
        syntax(
            &output,
            b"{\"schema\":\"commercial-fixed-sequential-cut-1\",\"raw_request_header\":",
        )?;
        // Complete candidate requests remain in their preparation receipts.
        // The reader reconstructs the outer request by identity, with no
        // monetary checkpoint, omitted history or financial calculation.
        emit(
            &output,
            &Header {
                schema: &request.schema,
                cut_ordinal: request.cut_ordinal,
            },
        )?;
        syntax(&output, b",\"completed_phase\":{\"original_sources\":[")?;
        let mut state = None;
        let (old, _) = {
            let mut count = 0;
            let mut observer = |source: UnitSourceObservation<'_>| {
                if source.source.scope == ProofScope::Current {
                    return Ok(());
                }
                if count > 0 {
                    syntax(&output, b",")?;
                }
                emit(
                    &output,
                    &Record {
                        schema: "commercial-cut-source-record-1",
                        side: "completed",
                        source,
                    },
                )?;
                count += 1;
                Ok(())
            };
            candidates::with_fixed_finite_compiled_observer(
                &request.completed_candidate,
                (0, 0),
                &mut observer,
                &mut |spec, _, owner, _| {
                    state = Some(SequentialCommercialCut::capture(
                        spec,
                        owner,
                        cut,
                        Box::new(stream.clone()),
                    )?);
                    Ok(())
                },
            )?
        };
        syntax(&output, b"],\"preparation\":")?;
        preparation(&output, &old).map_err(|e| format!("completed preparation encoding: {e}"))?;
        drop(old); // No completed graph/source borrow reaches revised preparation.
        syntax(&output, b"},\"revised_phase\":{\"original_sources\":[")?;
        let mut receipt = None;
        let (new, _) = {
            let mut count = 0;
            let mut observer = |source: UnitSourceObservation<'_>| {
                if source.source.scope == ProofScope::Current {
                    return Ok(());
                }
                if count > 0 {
                    syntax(&output, b",")?;
                }
                emit(
                    &output,
                    &Record {
                        schema: "commercial-cut-source-record-1",
                        side: "revised",
                        source,
                    },
                )?;
                count += 1;
                Ok(())
            };
            candidates::with_fixed_finite_compiled_observer(
                &request.revised_candidate,
                (0, 0),
                &mut observer,
                &mut |spec, _, owner, _| {
                    receipt = Some(
                        state
                            .take()
                            .ok_or("packet prefix state absent")?
                            .resume(spec, owner)?,
                    );
                    Ok(())
                },
            )?
        };
        syntax(&output, b"],\"preparation\":")?;
        preparation(&output, &new).map_err(|e| format!("revised preparation encoding: {e}"))?;
        drop(new);
        let data = std::mem::take(&mut *stream.0.lock().map_err(|_| "packet stream lock")?);
        tick(data.len())?;
        let data = String::from_utf8(data).map_err(|e| e.to_string())?;
        syntax(&output, b"},\"cut_stream\":")?;
        emit(&output, &data).map_err(|e| format!("cut-stream encoding: {e}"))?;
        syntax(&output, b",\"component_receipt\":")?;
        emit(&output, &receipt.ok_or("packet receipt absent")?)?;
        syntax(&output, b",\"resource_admission\":")?;
        emit(
            &output,
            &ResourceAdmission {
                completed_preparation_work_upper: a.work_upper,
                revised_preparation_work_upper: b.work_upper,
                completed_preparation_bytes_upper: a.retained_bytes_upper,
                revised_preparation_bytes_upper: b.retained_bytes_upper,
                typed_input_bytes_upper: typed_bytes_upper,
                source_phase_bytes_upper: phase,
                source_preparation_work_upper: sources,
                raw_prefix_work_reservation: raw_work,
                controlled_work_budget: budget,
                maximum_work: ROOT_WORK,
                maximum_bytes: 128 << 20,
            },
        )?;
        syntax(
            &output,
            b",\"joined_raw_admission_proved\":true,\"publication_eligible\":false}",
        )?;
        Ok(daily::cut_census_work_spent().ok_or("packet work ticket absent")?)
    })?;
    Ok(Usage {
        source_preparation_work_upper: sources,
        raw_prefix_work_reservation: raw_work,
        controlled_work_budget: budget,
        controlled_work_spent: spent,
        joined_reported_work: add(add(sources, raw_work)?, spent)?,
        source_phase_bytes_upper_reported: phase,
        complete_resource_proof: true,
        publication_eligible: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portfolio_compute_control::{scope, Control};
    use serde_json::{json, Value};
    #[test]
    fn complete_fixed_packet_respects_original_authority_and_all_sources() {
        let raw: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let envelope = json!({"op":"analyze_commercial_cut_mixed_journal","schema":cut_mixed::SCHEMA,"execution_timeout_ms":180000,"request":raw});
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let input =
            super::super::cut_fixed_intake::derive(&bytes, bytes.capacity(), &envelope).unwrap();
        let request: cut_mixed::Request =
            serde_json::from_value(envelope["request"].clone()).unwrap();
        let mut output = Vec::new();
        output.try_reserve_exact(COMPONENT).unwrap();
        assert!(write_packet(&request, input.typed_bytes_upper, &mut output, None).is_err());
        assert!(output.is_empty());
        let usage = scope(Some(Control::new(180000).unwrap()), || {
            write_packet(&request, input.typed_bytes_upper, &mut output, None)
        })
        .unwrap();
        assert!(usage.joined_reported_work <= ROOT_WORK);
        // Extra test capture is outside the production pipe inventory.
        assert!(usage.source_phase_bytes_upper_reported + COMPONENT <= 128 << 20);
        let packet: Value = serde_json::from_slice(&output).unwrap();
        let mut originals = 0;
        let mut current = 0;
        for side in ["completed_phase", "revised_phase"] {
            originals += packet[side]["original_sources"].as_array().unwrap().len();
            for g in packet[side]["preparation"]["native_preparation"]["source"]["native_graphs"]
                .as_array()
                .unwrap()
            {
                current += g["native_sources"].as_array().unwrap().len();
            }
        }
        assert_eq!((originals, current), (3, 24));
        eprintln!("fixed packet: {}", serde_json::to_string(&usage).unwrap());
        if let Some(path) = std::env::var_os("COMMERCIAL_FIXED_PACKET_ARTIFACT") {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            file.write_all(&output).unwrap();
        }
    }
}
