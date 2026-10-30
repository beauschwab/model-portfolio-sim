//! Actual short finite transfer sources remain refused: this is not adoption.
use portfolio_decision_native::commercial_cut_mixed as cut;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"
    ))
    .unwrap()
}
fn wire(mut request: Value, selected: bool) -> Vec<u8> {
    if selected {
        request["preparation_receipt_policy"] =
            "encode_native_preparations_before_value_decode-1".into();
    }
    serde_json::to_vec(&json!({"op":"analyze_commercial_cut_mixed_journal","schema":cut::SCHEMA,"execution_timeout_ms":180000,"request":request})).unwrap()
}
fn census(request: Value) -> Value {
    let error = cut::run_owned_envelope_started(wire(request, true), Instant::now()).unwrap_err();
    let prefix = "selected cut pre-Sim structural admission refused: ";
    serde_json::from_str(
        error
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("unexpected refusal: {error}")),
    )
    .unwrap()
}
#[test]
fn genuine_selected_transfer_reaches_complete_census_but_never_sim_authority() {
    let meta = census(raw());
    println!("CUT_ADMISSION_CENSUS={meta}");
    assert_eq!(meta["initial_source_work_upper"], 26_801_980);
    assert_eq!(meta["final_joined_work_upper"], 65_036_571);
    let w = &meta["cut_ledger_census"];
    assert_eq!(w["work_upper"], 11_343_032);
    assert_eq!(w["retained_bytes_upper"], 2_876_652);
    assert_eq!(w["comparison_work_upper"], 11_141_120);
    assert_eq!(
        meta["output_inventory"]["schema"],
        "commercial-cut-output-inventory-2"
    );
    assert_eq!(
        meta["retention_phases"]["source_owner_phase_bytes"],
        132_084_182
    );
    assert_eq!(
        meta["output_inventory"]["stream_batch_retained_bytes_upper"],
        20_392_098
    );
    let historical: Value = serde_json::from_str(include_str!(
        "../../portfolio-risk/tests/fixtures/commercial-cut-inventory-1-refusal.json"
    ))
    .unwrap();
    for key in [
        "prefix",
        "visible",
        "all_stream",
        "visible_stream",
        "decoded_tables",
        "dictionary_upper",
    ] {
        assert_eq!(
            meta["output_inventory"][key],
            historical["output_inventory"][key]
        );
    }
    for key in [
        "initial_source_work_upper",
        "source_owner_retained_bytes_upper",
        "cut_ledger_census",
        "completed_receipt_bytes",
        "revised_receipt_bytes",
    ] {
        assert_eq!(meta[key], historical[key]);
    }
    assert!(meta.get("producer_completion").is_none());
    assert!(meta.get("cut_tables").is_none());
}
#[test]
fn zero_allocations_retain_all_unweighted_lots_fee_groups_and_table_inventory() {
    let expected = census(raw());
    let mut request = raw();
    for key in ["completed_candidate", "revised_candidate"] {
        for allocation in request[key]["allocation"].as_array_mut().unwrap() {
            allocation["face"] = json!(0.);
        }
    }
    let actual = census(request);
    assert_eq!(expected["output_inventory"], actual["output_inventory"]);
    assert_eq!(expected["cut_ledger_census"], actual["cut_ledger_census"]);
}
#[test]
fn omitted_short_refusal_is_exact_and_selected_wire_never_bypasses_control() {
    let old = wire(raw(), false);
    let error = cut::run_envelope_started(&old, Instant::now()).unwrap_err();
    assert_eq!(
        error,
        "native cut joined32M/128MiB before dual graph generation: work=35324604,bytes=147721280"
    );
    let selected = wire(raw(), true);
    assert!(cut::run_envelope_started(&selected, Instant::now())
        .unwrap_err()
        .contains("owned wire"));
    assert_eq!(
        cut::run_owned_envelope_started(selected, Instant::now() - Duration::from_secs(181))
            .unwrap_err(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
}
#[test]
fn selected_policy_cannot_invent_transfer_work_authority() {
    let mut request = raw();
    request["completed_candidate"]["commercial"]["units"][0]["graph"]["positions"][0]
        ["original_pricing"][0]["source"]["source"]["transfer"]
        .as_object_mut()
        .unwrap()
        .remove("source_work_policy");
    let error = cut::run_owned_envelope_started(wire(request, true), Instant::now()).unwrap_err();
    assert!(error.contains("genuine counted finite transfer authority"));
}
