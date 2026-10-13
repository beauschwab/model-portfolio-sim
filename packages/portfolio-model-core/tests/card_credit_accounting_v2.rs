use portfolio_model_core::card_credit_accounting_v2 as v2;
use serde_json::{json, Value};

fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/card_credit_v2_proof.json")).unwrap()
}
fn raw(i: usize) -> v2::Request {
    serde_json::from_value(cases()["cases"][i]["raw"].clone()).unwrap()
}
fn check_case(i: usize) {
    let c = &cases()["cases"][i];
    let r: v2::Request = serde_json::from_value(c["raw"].clone()).unwrap();
    if c["expected"].is_null() {
        assert!(v2::evaluate(&r).is_err(), "{}", c["name"]);
        return;
    }
    let out = v2::evaluate(&r).unwrap();
    let trace: Vec<_> = out.actions.iter().map(|a| json!({
        "atomic_action":a.atomic_action,"ordinal":a.ordinal,
        "source_kind":a.source.kind,"recognition_indices":a.source.recognition_indices,
        "cash_event_id":a.source.cash_event_id,"interest_recovery_index":a.source.interest_recovery_index
    })).collect();
    let daily: Vec<_> = out
        .daily
        .iter()
        .map(|d| json!({"ordinal":d.ordinal,"snapshot":d.snapshot}))
        .collect();
    let expanded = json!({"economic":out.economic,"trace":trace,"daily":daily,
        "events":out.events,"shared_intents":out.shared_intents,"income_steps":out.income_steps,
        "reporting":out.reporting,"closing":out.closing,"complete":out.complete});
    assert_eq!(
        expanded, c["expected"],
        "{}: full compact-expanded proof mismatch",
        c["name"]
    );
    assert!(serde_json::to_vec(&out).unwrap().len() <= out.reserved_bytes);
    assert!(out.retained_capacity_charge <= out.reserved_bytes);
    assert!(!out.mixed_ledger_adoption && !out.accounting_standard_approval);
    for a in &out.actions {
        let group = &out.events[a.events.start..a.events.end];
        assert!(group.iter().all(|e| e.atomic_action == a.atomic_action));
        for e in group {
            let net = e.lines.iter().map(|l| l.amount).sum::<f64>();
            assert!(net.abs() <= 1e-9);
        }
    }
}
macro_rules! proof {
    ($name:ident,$i:expr) => {
        #[test]
        fn $name() {
            check_case($i)
        }
    };
}
proof!(positive_original_eir_complete_source, 0);
proof!(negative_original_yield_complete_source, 1);
proof!(premium_original_cost_complete_source, 2);
proof!(terminal_basis_transfer_atomic, 3);
proof!(fixed_yield_anchors_every_step, 4);
proof!(zero_yield_control, 5);
proof!(native_compound_billed_default, 6);
proof!(default_reversal_and_basis, 7);
proof!(delayed_claim_collection_stops_yield, 8);
proof!(partial_live_interest_exact_tail, 9);
proof!(partial_claim_exact_tail, 10);
proof!(same_day_default_collection, 11);
proof!(negative_interest_carry_reject, 12);
proof!(partial_principal_reject, 13);
proof!(unreviewed_grace_reject, 14);
proof!(missing_actual_limit_closure_reject, 15);
proof!(rounded_collection_reject, 16);
proof!(duplicate_collection_reject, 17);

#[test]
fn hand_original_yield_and_disposal_are_distinct() {
    let out = v2::evaluate(&raw(0)).unwrap();
    let g = (1.02f64).powf(10. / 365.);
    let c10 = 100. * g - 100.;
    let state = &out.daily[10].snapshot;
    assert!((state.core.live[0].accrued_interest + state.live_interest_basis - c10).abs() < 1e-10);
    assert!(
        (out.reporting.core.live[0].accrued_interest + out.reporting.live_interest_basis
            - c10 * (1.02f64).powf(5. / 365.))
        .abs()
            < 1e-10
    );
    assert!(
        (out.closing.original_eir_income + out.closing.reviewed_basis_income - 1.).abs() < 1e-10
    );
    assert!(out
        .income_steps
        .iter()
        .all(|s| s.request.events.len() == 1 && s.result.rows.len() == 1));
}

#[test]
fn raw_policies_measure_anchors_and_omitted_fields_fail_closed() {
    let r = raw(0);
    for field in ["review", "policy", "termination"] {
        let mut v = serde_json::to_value(&r).unwrap();
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<v2::Request>(v).is_err());
    }
    let mut wrong = r.clone();
    wrong.policy.principal_basis = "implicit".into();
    assert!(v2::audit_request(&wrong).is_err());
    wrong = r.clone();
    wrong.contract.identity.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
    assert!(v2::audit_request(&wrong).is_err());
    wrong = r.clone();
    wrong.review.principal = 99.;
    assert!(v2::audit_request(&wrong).is_err());
    wrong = r.clone();
    wrong.review.economic_source.revision = "other".into();
    assert!(v2::audit_request(&wrong).is_err());
}

#[test]
fn bounded_tail_source_actions_collections_and_metadata_preflight() {
    let r = raw(0);
    let mut bad = r.clone();
    bad.contract.horizon_days = 65;
    assert!(v2::audit_request(&bad).is_err());
    bad = r.clone();
    bad.contract.steps = vec![bad.contract.steps[0].clone(); 65];
    assert!(v2::audit_request(&bad).is_err());
    bad = raw(8);
    bad.interest_recoveries = vec![bad.interest_recoveries[0].clone(); 17];
    assert!(v2::audit_request(&bad).is_err());
    bad = r.clone();
    bad.policy.review.id = "x".repeat(257);
    assert!(v2::audit_request(&bad).is_err());
    bad = r;
    bad.review.cost = f64::INFINITY;
    assert!(v2::audit_request(&bad).is_err());
}

#[test]
fn cooperative_failure_returns_no_partial_receipt_or_mutated_request() {
    let r = raw(8);
    let before = serde_json::to_value(&r).unwrap();
    let mut audit_checks = 0usize;
    v2::audit_request_with_check(&r, &mut || {
        audit_checks += 1;
        Ok(())
    })
    .unwrap();
    // Cancel after projection and the first local accounting action, before
    // retaining its daily row or exposing any receipt to the caller.
    let cancel_after = audit_checks + 5;
    let mut count = 0usize;
    let result = v2::evaluate_with_check(&r, || {
        count += 1;
        if count == cancel_after {
            Err("cancel-review".into())
        } else {
            Ok(())
        }
    });
    assert!(result.unwrap_err().contains("cancel-review"));
    assert_eq!(serde_json::to_value(&r).unwrap(), before);
    assert!(v2::evaluate(&r).is_ok());
}

#[test]
fn retention_is_flat_and_statement_source_not_duplicated_in_snapshots() {
    let short = v2::evaluate(&raw(0)).unwrap();
    let mut r = raw(0);
    r.contract.horizon_days = 64;
    let long = v2::evaluate(&r).unwrap();
    assert_eq!(short.events.len(), long.events.len());
    assert_eq!(short.income_steps.len(), long.income_steps.len());
    let row = serde_json::to_value(&long.daily[63]).unwrap();
    assert!(row["snapshot"].get("events").is_none());
    assert!(row["snapshot"].get("income_steps").is_none());
    assert!(row["snapshot"].get("review").is_none());
    let difference =
        serde_json::to_vec(&long).unwrap().len() - serde_json::to_vec(&short).unwrap().len();
    assert!(
        difference < 100_000,
        "unexpected cumulative source/history retention: {difference}"
    );
}

#[test]
fn unowned_actions_pcd_and_unpaired_default_policy_reject_in_audit() {
    let mut r = raw(6);
    r.contract
        .steps
        .push(r.contract.steps.last().unwrap().clone());
    assert!(v2::audit_request(&r)
        .unwrap_err()
        .contains("after source default"));
    let mut r = raw(0);
    r.review.accounting_basis = "pcd".into();
    assert!(v2::audit_request(&r).is_err());
    let mut r = raw(6);
    r.review.policy.basis =
        portfolio_model_core::credit_accounting::BasisPolicy::RecognizeInEarningsAtDefault;
    assert!(v2::audit_request(&r).unwrap_err().contains("paired"));
    let mut r = raw(6);
    r.contract.collection_fraction = 0.5;
    assert!(v2::audit_request(&r)
        .unwrap_err()
        .contains("principal collection"));
}

#[test]
fn maximum_escaped_review_metadata_is_charged_before_generation() {
    let mut r = raw(0);
    r.contract.horizon_days = 64;
    let escaped = "\\\"".repeat(128);
    for i in [
        &mut r.review.identity,
        &mut r.review.original_yield_anchor,
        &mut r.review.policy.identity,
        &mut r.policy.review,
    ] {
        i.id = escaped.clone();
        i.revision = escaped.clone();
    }
    let admission = v2::audit_request(&r).unwrap();
    let out = v2::evaluate(&r).unwrap();
    assert!(out.retained_capacity_charge <= admission.reserved_bytes);
    assert!(admission.reserved_bytes <= v2::MAX_RESULT_BYTES);
    assert!(out.events.iter().all(|e| e.review == r.review.identity));
}
