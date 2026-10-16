//! Measured native owner/binding census for the explicitly selected fee tier.
use super::*;
fn request() -> Request {
    let graph: crate::commercial_graph::Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-generated-fee-graph-1.json"
    ))
    .unwrap();
    graph.positions[0]
        .generated_policy
        .as_deref()
        .unwrap()
        .clone()
}
#[test]
fn fee_template_counts_every_q_and_p_owner_and_binding_without_p_fit() {
    let r = request();
    let admitted = admit(&r).unwrap();
    owner::OWNER_EVALUATIONS.with(|n| n.set(0));
    owner::COUNTED_OWNER_WORK.with(|n| n.set(0));
    stochastic::fees::BINDINGS.with(|n| n.set([0, 0]));
    crate::commercial_factor_source::owner::SOURCE_CALLS.with(|n| n.set(0));
    let result = prepare(&r).unwrap();
    let n = 128;
    // Each Q role: train prefix+trial, evaluation prefix+trial+selected stop.
    // P original: prefix+trial+selected stop+immutable original proof rebuild.
    // P current: prefix+trial+selected accounting. Every path is generated.
    let q_roles = r.current_pricing.len() + 1;
    let p_trials = result
        .original_physical_sources
        .iter()
        .filter(|s| !s.stops().is_empty())
        .count()
        + result
            .current_physical
            .iter()
            .flat_map(|r| &r.sources)
            .filter(|s| !s.stops().is_empty())
            .count();
    let expected = q_roles * 5 * n + 3 * n + r.current_physical.len() * 2 * n + p_trials;
    let owners = owner::OWNER_EVALUATIONS.with(|v| v.get());
    assert_eq!(owners, expected);
    assert_eq!(
        crate::commercial_factor_source::owner::SOURCE_CALLS.with(|v| v.get()),
        owners * 2
    );
    assert!(owner::COUNTED_OWNER_WORK.with(|v| v.get()) <= admitted.work_upper);
    assert_eq!(
        stochastic::fees::BINDINGS.with(|v| v.get()),
        [
            q_roles * 6 * n,
            (r.current_physical.len() + 1) * 2 * n + p_trials
        ]
    );
    assert_eq!(result.current_physical.len(), 2);
    for role in &result.current_physical {
        assert_eq!(role.sources.len(), n);
        assert!(role
            .sources
            .iter()
            .all(|s| s.generated_fee_projection.is_some()));
    }
    assert!(!result.physical_outcomes_used_for_policy_fit);
    assert!(admitted.work_upper < 4_000_000_000);
    assert!(admitted.retained_bytes_upper < 128 << 20);
}
#[test]
fn fee_template_rejects_missing_counted_selector_and_conflicting_catalog() {
    let mut r = request();
    r.fee_acquisition
        .as_mut()
        .unwrap()
        .source_work_policy
        .clear();
    assert!(admit(&r).unwrap_err().contains("fee acquisition tier"));
    let mut r = request();
    r.current_physical[0]
        .nonintegral_fees
        .as_mut()
        .unwrap()
        .discount_spread += 0.01;
    assert!(admit(&r)
        .unwrap_err()
        .contains("identical explicit service"));
}
