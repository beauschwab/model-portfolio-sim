//! Controlled direct-encoding prototype. No raw CLI/publication selector exists.
//! Original proofs are emitted once; current proofs remain in full preparations.
use super::{candidates, cut_mixed};
use portfolio_compute_control::{checkpoint, scope, Control};
use portfolio_ledger_native::daily::{self, SequentialCommercialCut};
use portfolio_model_core::{strict_json, transport::prepaid_json};
use portfolio_risk_native::{
    commercial_graph::ProofScope, commercial_units::UnitSourceObservation,
};
use serde::Serialize;
use serde_json::{json, Value};
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

fn exact(actual: &Value, expected: &Value) {
    // Independent two-encoding oracle: integer/float categories and signed zero
    // must survive the direct transport, even where Value equality coalesces them.
    assert_eq!(
        serde_json::to_vec(actual).unwrap(),
        serde_json::to_vec(expected).unwrap()
    );
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
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    side: &'static str,
    #[serde(flatten)]
    source: UnitSourceObservation<'a>,
}

#[derive(Debug, Serialize)]
struct Usage {
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

/// The decode reservation still uses the retained transport convention. This
/// deliberately remains an unpublished producer-budget probe, not independent
/// raw-derived source/allocator/whole-Sim admission or a persisted receipt.
fn write_packet(
    request: &cut_mixed::Request,
    output: &mut dyn Write,
    limit: Option<usize>,
) -> Result<Usage, String> {
    write_packet_format(request, output, limit, false)
}

fn write_packet_format(
    request: &cut_mixed::Request,
    output: &mut dyn Write,
    limit: Option<usize>,
    stream_transport: bool,
) -> Result<Usage, String> {
    scope(Some(Control::new(180_000)?), || {
        if request.schema != cut_mixed::SCHEMA || request.preparation_receipt_policy.is_some() {
            return Err(
                "direct packet prototype requires its explicit ordinary cut contract".into(),
            );
        }
        let a = candidates::admit_indexed_finite_preparation(&request.completed_candidate)?;
        let b = candidates::admit_indexed_finite_preparation(&request.revised_candidate)?;
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
        // Test capture and native stream each reserve4MiB. A production pipe
        // would retain only its bounded writer; no such CLI route is enabled.
        let phase = add(
            a.retained_bytes_upper.max(b.retained_bytes_upper),
            add(32 << 20, add(2 * COMPONENT, add(mul(raw, 40)?, 64 << 10)?)?)?,
        )?;
        if phase > 128 << 20 {
            return Err("packet source phase exceeds128MiB".into());
        }
        let stream = Stream::new()?;
        let output = RefCell::new(output);
        let spent = daily::with_cut_census_budget(budget, || {
            syntax(
                &output,
                if stream_transport {
                    b"{\"schema\":\"commercial-sequential-packet-witness-2\",\"raw_request_header\":"
                } else {
                    b"{\"schema\":\"commercial-sequential-packet-witness-1\",\"raw_request_header\":"
                },
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
                candidates::with_indexed_finite_compiled_observer(
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
            emit(&output, &old).map_err(|e| format!("completed preparation encoding: {e}"))?;
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
                candidates::with_indexed_finite_compiled_observer(
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
            emit(&output, &new).map_err(|e| format!("revised preparation encoding: {e}"))?;
            drop(new);
            let data = std::mem::take(&mut *stream.0.lock().map_err(|_| "packet stream lock")?);
            if stream_transport {
                // Move the original columnar stream into an ordinary JSON string.
                // UTF-8 validation is prepaid. No native parse, Value expansion,
                // dictionary clone or second encoded stream is necessary.
                tick(data.len())?;
                let data = String::from_utf8(data).map_err(|e| e.to_string())?;
                syntax(&output, b"},\"cut_stream\":")?;
                emit(&output, &data).map_err(|e| format!("cut-stream encoding: {e}"))?;
            } else {
                syntax(&output, b"},\"cut_tables\":")?;
                // Historical prototype retained for exact reader compatibility.
                tick(mul(data.len(), 4)?)?;
                let (tables, completion) = candidates::decode_stream_bounded(&data, 9, 64 << 20)?;
                drop(data);
                emit(&output, &tables).map_err(|e| format!("cut-table encoding: {e}"))?;
                syntax(&output, b",\"producer_completion\":")?;
                emit(&output, &completion)?;
            }
            syntax(&output, b",\"component_receipt\":")?;
            emit(&output, &receipt.ok_or("packet receipt absent")?)?;
            syntax(
                &output,
                b",\"joined_raw_admission_proved\":false,\"publication_eligible\":false}",
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
            complete_resource_proof: false,
            publication_eligible: false,
        })
    })
}

#[test]
fn full_original_direct_packet_keeps_all_proofs_without_an_encoded_preparation_copy() {
    let request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    let mut output = Vec::new();
    output.try_reserve_exact(COMPONENT).unwrap();
    let usage = write_packet(&request, &mut output, None).unwrap();
    assert!(output.len() < COMPONENT);
    assert!(usage.joined_reported_work <= ROOT_WORK);
    assert!(usage.controlled_work_spent <= usage.controlled_work_budget);
    assert!(!usage.complete_resource_proof && !usage.publication_eligible);
    // Parsing belongs to the test reader, after every native source is dropped.
    let packet: Value = serde_json::from_slice(&output).unwrap();
    let reconstructed = json!({"schema":packet["raw_request_header"]["schema"],
        "cut_ordinal":packet["raw_request_header"]["cut_ordinal"],
        "completed_candidate":packet["completed_phase"]["preparation"]["raw_request"],
        "revised_candidate":packet["revised_phase"]["preparation"]["raw_request"]});
    exact(&reconstructed, &serde_json::to_value(&request).unwrap());
    let full = super::sequential_cut_tests::witness(&request, true).unwrap();
    for (phase, side, preparation) in [
        ("completed_phase", "completed", "completed_preparation"),
        ("revised_phase", "revised", "revised_preparation"),
    ] {
        exact(&packet[phase]["preparation"], &full[preparation]);
        let expected_original = full["source_records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["side"] == side && r["scope"] != "current")
            .cloned()
            .collect::<Vec<_>>();
        exact(
            &packet[phase]["original_sources"],
            &json!(expected_original),
        );
        for (unit, graph) in packet[phase]["preparation"]["native_preparation"]["source"]
            ["native_graphs"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let expected = full["source_records"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r["side"] == side
                        && r["scope"] == "current"
                        && r["unit_index"].as_u64() == Some(unit as u64)
                })
                .map(|r| r["proof"].clone())
                .collect::<Vec<_>>();
            exact(&graph["native_sources"], &json!(expected));
        }
    }
    exact(&packet["cut_tables"], &full["cut_tables"]);
    exact(&packet["producer_completion"], &full["producer_completion"]);
    exact(&packet["component_receipt"], &full["component_receipt"]);
    let mut original = 0;
    let mut current = 0;
    for phase in ["completed_phase", "revised_phase"] {
        original += packet[phase]["original_sources"].as_array().unwrap().len();
        for graph in packet[phase]["preparation"]["native_preparation"]["source"]["native_graphs"]
            .as_array()
            .unwrap()
        {
            current += graph["native_sources"].as_array().unwrap().len();
        }
    }
    assert_eq!((original, current), (3, 24));
    assert!(packet["cut_tables"]["journal"].as_array().unwrap().len() > 20);
    assert_eq!(packet["publication_eligible"], false);
    eprintln!(
        "direct packet usage: {}",
        serde_json::to_string(&usage).unwrap()
    );
    if let Some(path) = std::env::var_os("COMMERCIAL_DIRECT_PACKET_ARTIFACT") {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&output).unwrap();
        let mut name = path;
        name.push(".usage.json");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(name)
            .unwrap();
        serde_json::to_writer(file, &usage).unwrap();
    }
}

#[test]
fn insufficient_direct_packet_budget_refuses_before_source_preparation_and_output() {
    let request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    let mut output = Vec::new();
    assert!(write_packet(&request, &mut output, Some(1)).is_err());
    assert!(output.is_empty());
    assert_eq!(daily::cut_census_work_spent(), None);
}

#[test]
fn zero_allocation_direct_packet_retains_every_original_and_current_source() {
    let mut request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    for candidate in [
        &mut request.completed_candidate,
        &mut request.revised_candidate,
    ] {
        for allocation in &mut candidate.allocation {
            allocation.face = 0.;
        }
    }
    let mut output = Vec::new();
    output.try_reserve_exact(COMPONENT).unwrap();
    let usage = write_packet(&request, &mut output, None).unwrap();
    assert!(usage.joined_reported_work <= ROOT_WORK);
    let packet: Value = serde_json::from_slice(&output).unwrap();
    let full = super::sequential_cut_tests::witness(&request, true).unwrap();
    exact(&packet["cut_tables"], &full["cut_tables"]);
    let mut count = 0;
    for (phase, side) in [
        ("completed_phase", "completed"),
        ("revised_phase", "revised"),
    ] {
        count += packet[phase]["original_sources"].as_array().unwrap().len();
        for (unit, graph) in packet[phase]["preparation"]["native_preparation"]["source"]
            ["native_graphs"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let expected = full["source_records"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r["side"] == side
                        && r["scope"] == "current"
                        && r["unit_index"].as_u64() == Some(unit as u64)
                })
                .map(|r| r["proof"].clone())
                .collect::<Vec<_>>();
            exact(&graph["native_sources"], &json!(expected));
            count += expected.len();
        }
    }
    assert_eq!(count, 27);
    if let Some(path) = std::env::var_os("COMMERCIAL_ZERO_DIRECT_PACKET_ARTIFACT") {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&output).unwrap();
    }
}

#[test]
fn moved_columnar_stream_preserves_all_source_and_journal_bytes_without_native_decoding() {
    for zero in [false, true] {
        let mut request: cut_mixed::Request = serde_json::from_str(include_str!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        if zero {
            for candidate in [
                &mut request.completed_candidate,
                &mut request.revised_candidate,
            ] {
                for allocation in &mut candidate.allocation {
                    allocation.face = 0.;
                }
            }
        }
        let mut old = Vec::new();
        let previous = write_packet(&request, &mut old, None).unwrap();
        let old: Value = serde_json::from_slice(&old).unwrap();
        let mut output = Vec::new();
        output.try_reserve_exact(COMPONENT).unwrap();
        let usage = write_packet_format(&request, &mut output, None, true).unwrap();
        assert!(output.len() < COMPONENT);
        assert!(usage.joined_reported_work < previous.joined_reported_work);
        let packet: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(packet["schema"], "commercial-sequential-packet-witness-2");
        assert!(packet.get("cut_tables").is_none());
        assert!(packet.get("producer_completion").is_none());
        for key in [
            "raw_request_header",
            "completed_phase",
            "revised_phase",
            "component_receipt",
            "joined_raw_admission_proved",
            "publication_eligible",
        ] {
            exact(&packet[key], &old[key]);
        }
        // Independent test reader only: production of this packet never calls
        // the decoder and therefore owns no parsed stream or expanded Values.
        let wire = packet["cut_stream"].as_str().unwrap().as_bytes();
        let (tables, completion) = candidates::decode_stream(wire, 9).unwrap();
        exact(&serde_json::to_value(tables).unwrap(), &old["cut_tables"]);
        exact(&completion, &old["producer_completion"]);
        eprintln!(
            "stream packet zero={zero}: {} bytes={}",
            serde_json::to_string(&usage).unwrap(),
            output.len()
        );
        let name = if zero {
            "COMMERCIAL_ZERO_STREAM_PACKET_ARTIFACT"
        } else {
            "COMMERCIAL_STREAM_PACKET_ARTIFACT"
        };
        if let Some(path) = std::env::var_os(name) {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            file.write_all(&output).unwrap();
            let mut name = path;
            name.push(".usage.json");
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(name)
                .unwrap();
            serde_json::to_writer(file, &usage).unwrap();
        }
    }
}
