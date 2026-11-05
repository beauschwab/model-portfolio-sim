use portfolio_compute_control::{scope, Control};
use portfolio_decision_native::commercial_cut_mixed as cut;
use std::time::Instant;
fn fixture() -> cut::Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-cut-mixed-journal-1.json"
    ))
    .unwrap()
}
#[test]
fn complete_raw_graph_census_refuses_before_dual_generation_without_cap_waiver() {
    let raw = fixture();
    let control = Control::from_started(60_000, Instant::now()).unwrap();
    let error = scope(Some(control), || cut::evaluate(&raw)).err().unwrap();
    assert_eq!(
        error,
        "native cut joined32M/128MiB before dual graph generation: work=36844632,bytes=223261940"
    );
}
#[test]
fn unchanged_origin_and_explicit_future_candidate_pass_raw_authority_only() {
    assert_eq!(cut::validate_raw_prefix(&fixture()).unwrap(), 1);
}
#[test]
fn zero_allocation_does_not_erase_immutable_unweighted_source_policy() {
    let mut raw = fixture();
    raw.completed_candidate.allocation[0].face = 0.;
    raw.revised_candidate.allocation[0].face = 0.;
    assert_eq!(cut::validate_raw_prefix(&raw).unwrap(), 1);
    raw.revised_candidate.commercial.units[0].graph.positions[0].original_signed_cash_quote += 1.;
    assert!(cut::validate_raw_prefix(&raw).is_err());
}
#[test]
fn appended_past_acquisition_and_changed_held_allocation_are_refused() {
    let mut raw = fixture();
    raw.revised_candidate.commercial.units[1]
        .graph
        .as_of_ordinal = raw.cut_ordinal;
    assert!(cut::validate_raw_prefix(&raw).is_err());
    let mut raw = fixture();
    raw.revised_candidate.allocation[0].face += 1.;
    assert!(cut::validate_raw_prefix(&raw).is_err());
}

fn envelope() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "op":"analyze_commercial_cut_mixed_journal", "schema":cut::SCHEMA,
        "execution_timeout_ms":60000, "request":fixture()
    }))
    .unwrap()
}

#[test]
fn early_owned_cli_admits_complete_original_within_unchanged_limits() {
    let mut output = Vec::new();
    cut::run_cli_started(envelope(), Instant::now(), &mut output).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"]["schema"], "commercial-fixed-sequential-cut-1");
    assert_eq!(value["ok"]["joined_raw_admission_proved"], true);
    assert_eq!(value["ok"]["publication_eligible"], false);
    assert_eq!(
        value["ok"]["resource_admission"]["maximum_work"],
        32_000_000
    );
    assert_eq!(
        value["ok"]["resource_admission"]["maximum_bytes"],
        128 << 20
    );
}

#[test]
fn overbudget_complete_sources_refuse_before_any_success_bytes() {
    let mut input: serde_json::Value = serde_json::from_slice(&envelope()).unwrap();
    for side in ["completed_candidate", "revised_candidate"] {
        for unit in input["request"][side]["commercial"]["units"]
            .as_array_mut()
            .unwrap()
        {
            let position = &mut unit["graph"]["positions"][0];
            for h in position["original_pricing"].as_array_mut().unwrap() {
                h["source"]["credit"]["max_jumps_per_path"] = 4096.into();
            }
            for scenario in position["histories"].as_array_mut().unwrap() {
                for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                    for h in scenario[role].as_array_mut().unwrap() {
                        h["source"]["credit"]["max_jumps_per_path"] = 4096.into();
                    }
                }
            }
        }
    }
    let mut output = Vec::new();
    let error = cut::run_cli_started(
        serde_json::to_vec(&input).unwrap(),
        Instant::now(),
        &mut output,
    )
    .unwrap_err();
    assert!(error.contains("exceeds128MiB"), "{error}");
    assert!(output.is_empty());
}

#[test]
fn early_cli_keeps_original_deadline_and_strict_original_bytes() {
    let mut output = Vec::new();
    let expired = Instant::now() - std::time::Duration::from_secs(61);
    assert_eq!(
        cut::run_cli_started(envelope(), expired, &mut output).unwrap_err(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
    assert!(output.is_empty());
    let raw = br#"{"op":"analyze_commercial_cut_mixed_journal","op":"legacy"}"#.to_vec();
    assert!(cut::run_cli_started(raw, Instant::now(), &mut output)
        .unwrap_err()
        .contains("duplicate object key"));
    assert!(output.is_empty());
    // The selected-only library entrypoint keeps its historical policy guard.
    assert!(
        cut::run_owned_cli_started(envelope(), Instant::now(), &mut output)
            .unwrap_err()
            .contains("explicit native receipt policy")
    );
    let parent = Control::new(60000).unwrap();
    parent.cancel();
    let error = scope(Some(parent), || {
        cut::run_cli_started(envelope(), Instant::now(), &mut output)
    })
    .unwrap_err();
    assert!(error.contains("COMPUTE_CANCELLED"), "{error}");
    assert!(output.is_empty());
}

#[test]
fn real_cli_dispatches_original_cut_before_generic_tree_and_rejects_hidden_duplicates() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for raw in [
        envelope(),
        br#"{"op":"analyze_commercial_cut_mixed_journal","op":"legacy"}"#.to_vec(),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_portfolio-strategy"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&raw).unwrap();
        let result = child.wait_with_output().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        if raw == envelope() {
            assert_eq!(value["ok"]["schema"], "commercial-fixed-sequential-cut-1");
            assert!(value.get("error").is_none());
        } else {
            assert!(value["error"]
                .as_str()
                .unwrap()
                .contains("duplicate object key"));
            assert!(value.get("ok").is_none());
        }
    }
}
