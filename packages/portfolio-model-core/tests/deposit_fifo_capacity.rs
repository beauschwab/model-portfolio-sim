use portfolio_model_core::{
    deposit_continuation as old, deposit_fifo_capacity as new, deposit_state as source,
};
fn canonical() -> source::DepositStateResult {
    let raw = serde_json::from_slice(include_bytes!(
        "fixtures/deposit-principal/canonical-current.json"
    ))
    .unwrap();
    source::simulate(&raw).unwrap()
}
fn compare(s: &source::DepositStateResult, day: i32) {
    let a = old::rights(s, day, &mut || Ok(())).unwrap();
    let b = new::allocate(s, day, &mut || Ok(())).unwrap();
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b.expand()).unwrap()
    );
    assert!(b.allocations.len() <= b.plan.allocation_bound);
    assert!(b.visits <= b.plan.visit_bound);
    assert!(b.work_used <= b.plan.work_reserved);
    assert!(b.retained_used <= b.plan.retained_reserved);
    assert_eq!(serde_json::to_vec(&b).unwrap().len(), b.serialized_used);
    assert!(b.retained_used + b.serialized_used <= b.plan.joined_reserved);
}
#[test]
fn every_32_source_and_every_checkpoint_preserves_exact_expanded_rights() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deposit-principal");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|x| x.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 32);
    for file in files {
        let r: source::DepositStateRequest =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        let s = source::simulate(&r).unwrap();
        for day in r.as_of_ordinal..=r.as_of_ordinal + r.horizon_days as i32 {
            compare(&s, day);
        }
    }
}
fn hand(origins: &[f64], cash: &[(i32, f64, &str)]) -> source::DepositStateResult {
    let mut s = canonical();
    let segment = s.segment_ids[0].clone();
    s.interest = origins
        .iter()
        .enumerate()
        .map(|(i, &amount)| source::DepositInterest {
            start_ordinal: i as i32,
            end_ordinal: i as i32 + 1,
            segment_id: segment.clone(),
            amount,
        })
        .collect();
    s.cashflows = cash
        .iter()
        .enumerate()
        .map(|(i, &(ordinal, amount, kind))| source::DepositCashEvent {
            event_id: format!("cash-{i}"),
            ordinal,
            segment_id: segment.clone(),
            kind: kind.into(),
            principal: 0.,
            cash_interest: -amount,
        })
        .collect();
    s
}
#[test]
fn partial_split_same_day_full_coupon_order_exact() {
    let s = hand(
        &[1., 2., 3.],
        &[
            (1, 0.25, "withdrawal"),
            (1, 0.75, "closure"),
            (3, 5., "interest_payment"),
        ],
    );
    compare(&s, 3);
    let b = new::allocate(&s, 3, &mut || Ok(())).unwrap();
    assert_eq!(
        b.allocations.iter().map(|a| a.amount).collect::<Vec<_>>(),
        vec![0.25, 0.75, 2., 3.]
    );
    assert_eq!(b.total_right, 0.);
}
#[test]
fn positive_zero_gap_origins_preserved_exactly() {
    compare(
        &hand(&[1., 0., 0., 2.], &[(4, 3., "contractual_maturity")]),
        4,
    );
}
#[test]
fn full_coupon_retains_roundoff_without_future_consumption() {
    let s = hand(&[0.1, 0.1, 0.1, 1.], &[(3, 0.3, "interest_payment")]);
    compare(&s, 4);
    let b = new::allocate(&s, 4, &mut || Ok(())).unwrap();
    assert_eq!(b.origins[3].remaining, 1.);
    assert_eq!(
        b.settlements[0].retained_cash_roundoff_right,
        0.1 + 0.1 + 0.1 - 0.3
    );
}
#[test]
fn cash_cannot_borrow_future_and_no_partial_result() {
    let s = hand(&[1., 1.], &[(1, 1.5, "withdrawal")]);
    assert!(old::rights(&s, 2, &mut || Ok(())).is_err());
    assert!(new::allocate(&s, 2, &mut || Ok(())).is_err());
}
#[test]
fn negative_coupon_refused_not_netted_into_linear_bound() {
    assert!(new::allocate(&hand(&[1., -1.], &[]), 2, &mut || Ok(()))
        .unwrap_err()
        .contains("nonnegative"));
}
#[test]
fn cancellation_during_actual_positive_allocation_no_result() {
    let s = hand(&[1.; 32], &[(32, 32., "interest_payment")]);
    let mut checks = 0;
    let e = new::allocate(&s, 32, &mut || {
        checks += 1;
        if checks == 70 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(e, "cancelled");
}
#[test]
fn checked_cardinality_overflow_and_exact_byte_boundary() {
    assert!(new::cardinality_plan(usize::MAX, 1, 0).is_err());
    assert!(new::cardinality_plan(0, 0, new::MAX_BYTES + 1).is_err());
    let bound = (new::MAX_WORK - 4 * 2048) / 25;
    assert!(new::cardinality_plan(0, 0, bound).is_ok());
    assert!(new::cardinality_plan(0, 0, bound + 1).is_err());
}
#[test]
fn cancellation_during_bounded_output_count_cannot_return_result() {
    let s = hand(&[], &[]);
    let mut calls = 0;
    let err = new::allocate(&s, 0, &mut || {
        calls += 1;
        if calls == 2 {
            Err("serialization cancelled".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(err.contains("serialization cancelled"));
}
#[test]
fn escaped_cash_identity_bytes_are_charged_before_serialization() {
    let mut s = hand(&[1.], &[(1, 1., "interest_payment")]);
    s.cashflows[0].event_id = "\"\\\u{0001}".repeat(32);
    compare(&s, 1);
}
#[test]
fn many_partial_payments_visit_count_is_linear() {
    let cash: Vec<_> = (0..1024).map(|_| (1, 1. / 1024., "withdrawal")).collect();
    let s = hand(&[1.], &cash);
    compare(&s, 1);
    let b = new::allocate(&s, 1, &mut || Ok(())).unwrap();
    assert_eq!(b.allocations.len(), 1024);
    assert_eq!(b.visits, 1024);
}
#[test]
fn nonchronological_cannot_reuse_cursor() {
    let mut s = hand(&[1., 2.], &[(2, 3., "interest_payment")]);
    s.interest.swap(0, 1);
    assert!(new::allocate(&s, 2, &mut || Ok(())).is_err());
}
