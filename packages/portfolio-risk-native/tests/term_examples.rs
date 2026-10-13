use portfolio_risk_native::term_enhancements::{cd::*, exercise::*, indexed::*};
fn raw(s: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(s).unwrap()["request"].clone()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn checked_in_protocol_examples_match_independent_hand_amounts() {
    let indexed: ContractPathRequest = serde_json::from_value(raw(include_str!(
        "../../../examples/models/term-indexed.json"
    )))
    .unwrap();
    let x = build_path(&indexed).unwrap();
    close(x.audit[0].interest, 100. * (0.02 + 0.01) * 29. / 360.);
    close(x.principal_after_reporting, 100.);
    let exercise: ExerciseRequest = serde_json::from_value(raw(include_str!(
        "../../../examples/models/term-exercise.json"
    )))
    .unwrap();
    let x = value_exercise(&exercise).unwrap();
    close(x.value, 0.98 * (5. + 100.));
    assert_eq!(x.nodes[1].decision, Decision::Call);
    close(x.nodes[2].live_pricing_probability, 0.);
    let cd: CdElectionRequest = serde_json::from_value(raw(include_str!(
        "../../../examples/models/term-cd-election.json"
    )))
    .unwrap();
    let x = step_cd(&cd).unwrap();
    close(x.penalty, 40. * 0.05 * 90. / 365.);
    close(x.net_external_cash, 40.2 - 40. * 0.05 * 90. / 365.);
    close(x.remaining_schedule.events[0].components.cash_interest, 0.6);
}
