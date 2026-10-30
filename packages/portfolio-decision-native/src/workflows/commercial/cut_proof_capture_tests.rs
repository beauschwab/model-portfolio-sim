use super::*;
use portfolio_compute_control::{scope, Control};
use serde_json::{json, Value};

fn raw() -> cut_mixed::Request {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"
    ))
    .unwrap();
    value["preparation_receipt_policy"] = json!("encode_native_preparations_before_value_decode-1");
    serde_json::from_value(value).unwrap()
}
fn controlled<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    scope(Some(Control::new(180_000).unwrap()), f)
}
fn decoded(bytes: &[u8]) -> Vec<Value> {
    bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|line| strict_json::parse(line, MAX_STREAM).unwrap())
        .collect()
}
fn run(r: &cut_mixed::Request) -> (CaptureReceipt, Vec<u8>) {
    controlled(|| {
        let mut bytes = Vec::new();
        let receipt = capture(r, &mut bytes)?;
        Ok((receipt, bytes))
    })
    .unwrap()
}

#[test]
fn genuine_capture_keeps_all27_complete_roles_and_original_fee_proofs_with_paid_single_encoding() {
    let r = raw();
    let (receipt, bytes) = run(&r);
    assert_eq!(receipt.source_records, 27);
    assert_eq!(
        receipt.counts,
        SourceCounts {
            original_pricing: 3,
            original_physical: 0,
            physical: 6,
            pricing: 6,
            pricing_down: 6,
            pricing_up: 6
        }
    );
    assert_eq!(receipt.source_preparation_work_upper, 24_846_612);
    assert_eq!(receipt.capture_work, 2 * bytes.len() + 16 * 27 + 2048);
    assert_eq!(receipt.encoded_bytes, bytes.len());
    assert!(receipt.admitted_work_upper <= MAX_WORK);
    assert!(receipt.reported_retained_bytes_upper <= MAX_BYTES);
    assert!(
        !receipt.publication_eligible
            && !receipt.cut_sim_executed
            && !receipt.cut_admission_granted
    );
    assert!(
        !receipt.complete_source_sim_codec_bounds
            && !receipt.retains_typed_original_economic_owners
    );
    let rows = decoded(&bytes);
    assert_eq!(rows.len(), 28);
    let expected = serde_json::to_value(&receipt.counts).unwrap();
    assert_eq!(rows[27]["counts"], expected);
    assert_eq!(rows[27]["capture_complete"], true);
    for record in &rows[..27] {
        assert_eq!(record["schema"], "commercial-cut-source-record-1");
        let proof = &record["proof"];
        assert!(proof["projection"].is_object());
        assert!(proof["cash"].is_array());
        assert!(proof["source_request"].is_object());
        assert!(proof["fee_projection"].is_object());
        assert!(proof["projection"]["carry"].is_object());
        if record["scope"] == "original_pricing" {
            assert!(record["scenario_index"].is_null());
            assert_eq!(record["role"], "pricing");
            assert_eq!(proof["role"], "pricing");
            assert_eq!(proof["projection"]["physical_journal_eligible"], false);
            assert!(proof["projection"]["original_source"].is_object());
            assert!(proof["projection"]["current_source"].is_object());
        }
    }
    println!(
        "CUT_SOURCE_CAPTURE_RECEIPT={}",
        serde_json::to_string(&receipt).unwrap()
    );
    if let Ok(folder) = std::env::var("CUT_PROOF_CAPTURE_ARTIFACT") {
        let folder = std::path::Path::new(&folder);
        let mut source = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join("sources.jsonl"))
            .unwrap();
        source.write_all(&bytes).unwrap();
        let mut result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join("capture-receipt.json"))
            .unwrap();
        result
            .write_all(&serde_json::to_vec_pretty(&receipt).unwrap())
            .unwrap();
    }
}

#[test]
fn captured_current_sources_are_exact_unchanged_legacy_preparation_values() {
    let r = raw();
    let (_, bytes) = run(&r);
    let rows = decoded(&bytes);
    controlled(|| {
        for (side, request) in [
            ("completed", &r.completed_candidate),
            ("revised", &r.revised_candidate),
        ] {
            let ordinary = units::prepare(&request.commercial)?;
            for row in rows
                .iter()
                .filter(|row| row["side"] == side && row["scope"] == "current")
            {
                let ui = row["unit_index"].as_u64().unwrap() as usize;
                let si = row["scenario_index"].as_u64().unwrap() as usize;
                let hi = row["history_index"].as_u64().unwrap() as usize;
                let graph = &ordinary.source().native_graphs[ui];
                let scenario = &request.commercial.units[ui].graph.scenarios[si].scenario_id;
                let source = graph
                    .native_sources
                    .iter()
                    .filter(|s| &s.scenario_id == scenario)
                    .nth(hi)
                    .unwrap();
                assert_eq!(row["unit_id"], request.commercial.units[ui].unit_id);
                assert_eq!(row["proof"], serde_json::to_value(source).unwrap());
            }
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn zero_allocations_never_remove_or_change_an_unweighted_capture_record() {
    let r = raw();
    let (expected, bytes) = run(&r);
    let mut zero = raw();
    for candidate in [&mut zero.completed_candidate, &mut zero.revised_candidate] {
        for allocation in &mut candidate.allocation {
            allocation.face = 0.;
        }
    }
    let (actual, zero_bytes) = run(&zero);
    assert_eq!(actual.counts, expected.counts);
    assert_eq!(zero_bytes, bytes);
}

#[test]
fn unsupported_or_counterfeit_inputs_refuse_before_staging() {
    let mut request = raw();
    request.preparation_receipt_policy = None;
    let mut bytes = Vec::new();
    assert!(controlled(|| capture(&request, &mut bytes))
        .unwrap_err()
        .contains("explicit controlled"));
    assert!(bytes.is_empty());
    let mut request = raw();
    request.completed_candidate.commercial.units[0]
        .graph
        .positions[0]
        .original_pricing[0]
        .source
        .source
        .transfer
        .as_mut()
        .unwrap()
        .source_work_policy = None;
    assert!(controlled(|| capture(&request, &mut bytes))
        .unwrap_err()
        .contains("genuine counted finite"));
    assert!(bytes.is_empty());
    let mut request = raw();
    request.revised_candidate.owners[0].ownership = "foreign".into();
    assert!(controlled(|| capture(&request, &mut bytes)).is_err());
    assert!(bytes.is_empty());
    let mut request = raw();
    request.revised_candidate.commercial.units[0].unit_id = "x".repeat(MAX_RAW);
    assert!(controlled(|| capture(&request, &mut bytes)).is_err());
    assert!(bytes.is_empty());
    assert!(capture(&raw(), &mut bytes)
        .unwrap_err()
        .contains("explicit controlled"));
    assert!(bytes.is_empty());
}

struct Failing {
    bytes: Vec<u8>,
    limit: usize,
    cancel: Option<Control>,
    fail_flush: bool,
}
impl Write for Failing {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(control) = self.cancel.take() {
            control.cancel();
        }
        let count = bytes.len().min(self.limit.saturating_sub(self.bytes.len()));
        if count == 0 {
            return Err(io::Error::other("fixture staged write failed"));
        }
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.fail_flush {
            Err(io::Error::other("fixture staged flush failed"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn writer_failure_cancellation_and_flush_error_never_return_a_capture_receipt() {
    for limit in [0, 1, 32_768] {
        let mut output = Failing {
            bytes: vec![],
            limit,
            cancel: None,
            fail_flush: false,
        };
        assert!(controlled(|| capture(&raw(), &mut output))
            .unwrap_err()
            .contains("fixture staged write failed"));
        assert!(!String::from_utf8_lossy(&output.bytes)
            .contains("commercial-cut-source-capture-complete-1"));
    }
    let control = Control::new(180_000).unwrap();
    let mut output = Failing {
        bytes: vec![],
        limit: usize::MAX,
        cancel: Some(control.clone()),
        fail_flush: false,
    };
    assert_eq!(
        scope(Some(control), || capture(&raw(), &mut output)).unwrap_err(),
        "COMPUTE_CANCELLED"
    );
    let mut output = Failing {
        bytes: vec![],
        limit: usize::MAX,
        cancel: None,
        fail_flush: true,
    };
    assert!(controlled(|| capture(&raw(), &mut output))
        .unwrap_err()
        .contains("fixture staged flush failed"));
}

#[test]
fn capture_writer_requires_prepaid_work_and_refuses_before_growth_or_external_write() {
    let mut out = Vec::new();
    let mut sink = Capture {
        output: &mut out,
        encoded: 0,
        records: 0,
        counts: SourceCounts::default(),
        limit: 4,
    };
    assert!(sink
        .write_all(b"null")
        .unwrap_err()
        .to_string()
        .contains("remaining-work ticket"));
    assert!(daily::with_cut_census_budget(7, || sink
        .write_all(b"null")
        .map_err(|e| e.to_string()))
    .is_err());
    assert_eq!(sink.encoded, 0);
    assert!(daily::with_cut_census_budget(100, || sink
        .write_all(b"false")
        .map_err(|e| e.to_string()))
    .is_err());
    assert_eq!(sink.encoded, 0);
    daily::with_cut_census_budget(8, || sink.write_all(b"null").map_err(|e| e.to_string()))
        .unwrap();
    assert_eq!(sink.encoded, 4);
    assert_eq!(out, b"null");
    assert!(add(usize::MAX, 1).is_err() && mul(usize::MAX, 2).is_err());
}
