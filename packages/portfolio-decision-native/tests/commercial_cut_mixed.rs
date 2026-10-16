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
