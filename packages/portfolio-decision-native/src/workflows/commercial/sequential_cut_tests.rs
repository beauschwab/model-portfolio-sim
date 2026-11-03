//! Whole unchanged public request, with genuinely disjoint native preparations.
//! An unpublished component witness; no joined raw admission/publication permit.
use super::{candidates, cut_codec, cut_mixed};
use portfolio_compute_control::{checkpoint, scope, Control};
use portfolio_ledger_native::daily::{self, SequentialCommercialCut};
use portfolio_model_core::strict_json;
use portfolio_risk_native::commercial_units::UnitSourceObservation;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let mut data = self
            .0
            .lock()
            .map_err(|_| io::Error::other("test buffer lock"))?;
        if data
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > 4 << 20)
        {
            return Err(io::Error::other("test component exceeds4MiB"));
        }
        data.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Serialize)]
struct Record<'a> {
    schema: &'static str,
    side: &'static str,
    #[serde(flatten)]
    source: UnitSourceObservation<'a>,
}
struct RecordWriter(Vec<u8>);
impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        daily::charge_cut_census_work(
            bytes
                .len()
                .checked_mul(2)
                .ok_or_else(|| io::Error::other("test capture work overflow"))?,
        )
        .map_err(io::Error::other)?;
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > 4 << 20)
        {
            return Err(io::Error::other("test source record exceeds4MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn witness(request: &cut_mixed::Request, indexed: bool) -> Result<Value, String> {
    scope(Some(Control::new(180_000)?), || {
        let cut = cut_mixed::validate_raw_prefix(request).and_then(|_| {
            usize::try_from(request.cut_ordinal - request.completed_candidate.as_of_ordinal)
                .map_err(|e| e.to_string())
        })?;
        let stream = Buffer::default();
        let mut state = None;
        let mut receipt = None;
        let mut records = Vec::<Vec<u8>>::new();
        let mut record_bytes = 0usize;
        let mut traversal_work = 0usize;
        let mut output_inventory = None;
        // A separate bounded capture component. The sum of all components is
        // reported, never represented as an admitted joined raw calculation.
        let (completed, revised, capture_work) = daily::with_cut_census_budget(8_000_000, || {
            daily::charge_cut_census_work(2048)?;
            let mut observe = |side, source: UnitSourceObservation<'_>| {
                daily::charge_cut_census_work(16)?;
                let mut writer = RecordWriter(Vec::new());
                let traversal = cut_codec::write(
                    &mut writer,
                    &Record {
                        schema: "commercial-cut-source-record-1",
                        side,
                        source,
                    },
                    |n| {
                        checkpoint()?;
                        daily::charge_cut_census_work(n)
                    },
                )?;
                traversal_work += traversal;
                record_bytes = record_bytes
                    .checked_add(writer.0.len())
                    .ok_or("test record byte overflow")?;
                if record_bytes > 4 << 20 {
                    return Err("test complete source stream exceeds4MiB".into());
                }
                records.push(writer.0);
                Ok(())
            };
            let prepare = if indexed {
                candidates::with_indexed_finite_compiled_observer
            } else {
                candidates::with_finite_compiled_observer
            };
            let (old, a) = prepare(
                &request.completed_candidate,
                (0, 0),
                &mut |source| observe("completed", source),
                &mut |spec, _, owner, _| {
                    let inventory = daily::commercial_sequential_cut_inventory(spec, owner, cut)?;
                    assert!(inventory.stream_batch_retained_bytes_upper <= 32 << 20);
                    eprintln!(
                        "completed sequential Sink bound={} bytes, census={} work",
                        inventory.stream_batch_retained_bytes_upper, inventory.census_work_upper
                    );
                    state = Some(SequentialCommercialCut::capture(
                        spec,
                        owner,
                        cut,
                        Box::new(stream.clone()),
                    )?);
                    Ok(())
                },
            )?;
            let completed = strict_json::encode(&old, 4 << 20)?;
            // Preparation owns its graph. This explicit drop precedes ANY
            // revised source generation; the opaque state has no graph borrow.
            drop(old);
            let (new, b) = prepare(
                &request.revised_candidate,
                (0, 0),
                &mut |source| observe("revised", source),
                &mut |spec, _, owner, _| {
                    let inventory = daily::commercial_sequential_cut_inventory(spec, owner, cut)?;
                    assert!(inventory.stream_batch_retained_bytes_upper <= 32 << 20);
                    eprintln!(
                        "revised sequential Sink bound={} bytes, census={} work",
                        inventory.stream_batch_retained_bytes_upper, inventory.census_work_upper
                    );
                    output_inventory = Some(inventory);
                    receipt = Some(
                        state
                            .take()
                            .ok_or("missing prefix state")?
                            .resume(spec, owner)?,
                    );
                    Ok(())
                },
            )?;
            let revised = strict_json::encode(&new, 4 << 20)?;
            drop(new);
            Ok((
                (completed, a),
                (revised, b),
                daily::cut_census_work_spent().ok_or("missing capture ticket")?,
            ))
        })?;
        let (completed, a) = completed;
        let (revised, b) = revised;
        let data = std::mem::take(&mut *stream.0.lock().map_err(|_| "test stream lock")?);
        let (tables, completion) = candidates::decode_stream_bounded(&data, 9, 64 << 20)?;
        let inventory = output_inventory.ok_or("missing sequential output inventory")?;
        assert!(data.len() <= inventory.visible_stream.encoded_bytes_upper);
        inventory.verify_tables(&tables)?;
        let receipt = receipt.ok_or("sequential component receipt absent")?;
        let source_records = records
            .into_iter()
            .map(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(
            json!({"schema":"commercial-sequential-cut-witness-2","raw_request":request,
            "construction_census":if indexed {"immutable-index-1"} else {"historical-1"},
            "completed_preparation":serde_json::from_slice::<Value>(&completed).map_err(|e|e.to_string())?,
            "revised_preparation":serde_json::from_slice::<Value>(&revised).map_err(|e|e.to_string())?,
            "cut_tables":tables,"producer_completion":completion,"component_receipt":receipt,
            "source_records":source_records,"source_record_bytes":record_bytes,
            "source_preparation_work_upper_reported":a.work_upper+b.work_upper,
            "source_preparation_sum_bytes_upper_reported":a.retained_bytes_upper+b.retained_bytes_upper,
            "source_preparation_phase_bytes_upper_reported":a.retained_bytes_upper.max(b.retained_bytes_upper),
            "source_capture_work":capture_work,"source_traversal_work":traversal_work,
            "joined_raw_admission_proved":false,"publication_eligible":false}),
        )
    })
}

#[test]
fn full_original_raw_request_runs_real_sequential_sim_and_retains_every_source_proof() {
    let request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    let actual = witness(&request, false).unwrap();
    assert_eq!(actual["source_records"].as_array().unwrap().len(), 27);
    assert_eq!(actual["component_receipt"]["runs_moved"], 2);
    assert_eq!(
        actual["component_receipt"]["completed_source_borrow_retained"],
        false
    );
    assert_eq!(actual["joined_raw_admission_proved"], false);
    assert_eq!(actual["publication_eligible"], false);
    assert!(actual["cut_tables"]["journal"].as_array().unwrap().len() > 20);
    if let Some(path) = std::env::var_os("COMMERCIAL_SEQUENTIAL_CUT_ARTIFACT") {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer(file, &actual).unwrap();
    }
}

#[test]
fn indexed_construction_preserves_every_full_original_source_and_financial_row() {
    let request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    let (old, new) = scope(Some(Control::new(180_000).unwrap()), || {
        let mut saved = 0;
        for candidate in [&request.completed_candidate, &request.revised_candidate] {
            let a = candidates::admit_finite_preparation(candidate).unwrap();
            let b = candidates::admit_indexed_finite_preparation(candidate).unwrap();
            assert_eq!(a.retained_bytes_upper, b.retained_bytes_upper);
            saved += a.work_upper.checked_sub(b.work_upper).unwrap();
        }
        assert!(saved > 4_000_000, "indexed construction saving={saved}");
        Ok((
            witness(&request, false).unwrap(),
            witness(&request, true).unwrap(),
        ))
    })
    .unwrap();
    for field in [
        "raw_request",
        "source_records",
        "source_record_bytes",
        "cut_tables",
        "producer_completion",
        "component_receipt",
        "source_preparation_sum_bytes_upper_reported",
        "source_preparation_phase_bytes_upper_reported",
        "source_capture_work",
        "source_traversal_work",
    ] {
        assert_eq!(old[field], new[field], "field={field}");
    }
    for field in ["completed_preparation", "revised_preparation"] {
        let mut a = old[field].clone();
        let mut b = new[field].clone();
        a.as_object_mut().unwrap().remove("admitted_work_upper");
        b.as_object_mut().unwrap().remove("admitted_work_upper");
        assert_eq!(a, b, "field={field}");
    }
    if let Some(path) = std::env::var_os("COMMERCIAL_INDEXED_SEQUENTIAL_CUT_ARTIFACT") {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer(file, &new).unwrap();
    }
}

#[test]
fn zero_allocation_keeps_full_original_source_proofs_and_unweighted_native_books() {
    let mut request: cut_mixed::Request = serde_json::from_str(include_str!(
        "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap();
    let full = witness(&request, true).unwrap();
    for candidate in [
        &mut request.completed_candidate,
        &mut request.revised_candidate,
    ] {
        for allocation in &mut candidate.allocation {
            allocation.face = 0.;
        }
    }
    let zero = witness(&request, true).unwrap();
    assert_eq!(zero["source_records"], full["source_records"]);
    assert_eq!(zero["source_record_bytes"], full["source_record_bytes"]);
    assert_eq!(zero["source_traversal_work"], full["source_traversal_work"]);
    assert_eq!(zero["producer_completion"], full["producer_completion"]);
    for side in ["completed_preparation", "revised_preparation"] {
        assert_eq!(
            zero[side]["native_preparation"],
            full[side]["native_preparation"]
        );
    }
    for family in ["commercial_closing", "commercial_daily"] {
        let a = full["cut_tables"][family].as_array().unwrap();
        let b = zero["cut_tables"][family].as_array().unwrap();
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a["unweighted_book"], b["unweighted_book"]);
        }
    }
    assert_eq!(zero["publication_eligible"], false);
    if let Some(path) = std::env::var_os("COMMERCIAL_ZERO_SEQUENTIAL_CUT_ARTIFACT") {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer(file, &zero).unwrap();
    }
}
