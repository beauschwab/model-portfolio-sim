//! Independent serializer layout/leaf mutation controls for reviewed states.
use super::{tests::*, *};
use serde_json::{json, Value};

fn identity(text: &str) -> Identity {
    Identity {
        id: text.into(),
        revision: "review-1".into(),
    }
}

fn reviewed() -> book::reviews::ReviewedSnapshot {
    book::reviews::ReviewedSnapshot {
        legal_principal: 71.,
        model_gross_carry: 73.,
        allowance: 4.,
        claims: vec![credit::RecoveryClaim {
            claim_id: "claim\"\n非".into(),
            economic_source: identity("economic"),
            recovery_schedule_review: identity("recovery"),
            policy: credit::AccountingPolicy {
                identity: identity("policy"),
                accrued: credit::AccruedPolicy::TransferUntilReviewedReversal,
                basis: credit::BasisPolicy::TransferUntilReviewedChargeoff,
                allowance_shortfall: credit::AllowanceShortfall::Reject,
                excess_cash_recovery: credit::ExcessCashRecovery::CreditAllowance,
            },
            default_ordinal: 730001,
            initial_principal: 100.,
            economic_principal_loss: 40.,
            initial_accrued_interest: -2.,
            initial_book_adjustment: 3.,
            outstanding_principal: 71.,
            outstanding_accrued_interest: -0.,
            outstanding_book_adjustment: 2.,
            principal_charged_off: 20.,
            accrued_reversed: -1.,
            basis_recognized: 1.,
            recovery_cash_collected: 9.,
            recovery_applied_to_principal: 9.,
            interest_recovery_cash_collected: -0.5,
            interest_recovery_applied: -0.5,
            settled_interest_recovery_ids: vec![identity("interest-a"), identity("interest-b")],
            remaining_recoveries: vec![
                credit::RecoveryPoint {
                    payment_ordinal: 730100,
                    amount: 11.,
                },
                credit::RecoveryPoint {
                    payment_ordinal: 730200,
                    amount: 12.,
                },
            ],
        }],
        legal_waivers: vec![book::reviews::LegalWaiver {
            path_key: 7,
            review: identity("waiver"),
            recognition_ordinal: 730002,
            claim_id: "waived".into(),
            principal_amount: 8.,
            accrued_interest_amount: -0.,
            future_cash_disposition: "retain_separate_voluntary_cash".into(),
        }],
    }
}

// Mutate each serialized leaf independently, retaining valid enum variants.
// This oracle walks the real serde output rather than the Typed field list.
fn leaf_mutations(value: &Value) -> Vec<Value> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(name, child)| {
                leaf_mutations(child).into_iter().map(move |changed| {
                    let mut result = value.clone();
                    result[name] = changed;
                    result
                })
            })
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .flat_map(|(index, child)| {
                leaf_mutations(child).into_iter().map(move |changed| {
                    let mut result = value.clone();
                    result[index] = changed;
                    result
                })
            })
            .collect(),
        Value::String(text) => vec![json!(match text.as_str() {
            "transfer_until_reviewed_reversal" => "reverse_income_at_default".into(),
            "transfer_until_reviewed_chargeoff" => "recognize_in_earnings_at_default".into(),
            "reject" => "expense_at_reviewed_chargeoff".into(),
            "credit_allowance" => "recognize_in_earnings".into(),
            _ => format!("{text}!"),
        })],
        Value::Number(number) if number.is_i64() => vec![json!(number.as_i64().unwrap() + 1)],
        Value::Number(number) if number.is_u64() => vec![json!(number.as_u64().unwrap() + 1)],
        Value::Number(number) => vec![json!(number.as_f64().unwrap() + 1.)],
        Value::Bool(value) => vec![json!(!value)],
        Value::Null => vec![json!(false)],
    }
}

#[test]
fn every_claim_and_waiver_leaf_and_policy_variant_has_exact_layout_and_equality() {
    cut_shape::with_budget(2_000_000, || {
        let mut original = group();
        let state = reviewed();
        original.next.reviewed_credit_state = Some(state.clone());
        layout_matches(&original);
        let shape = commercial_loan_prefix_shape(&original)?;
        let serde_shape = cut_shape::shape(&original)?;
        assert!(shape.encoded_bytes_upper >= serde_shape.encoded_bytes_upper);
        assert_eq!(
            add(shape.cells, shape.emitted_fields)?,
            serde_shape.cells_upper
        );
        let claims = leaf_mutations(&serde_json::to_value(&state.claims[0]).unwrap());
        assert_eq!(claims.len(), 34);
        for changed in claims {
            let mut other = original.clone();
            other.next.reviewed_credit_state.as_mut().unwrap().claims[0] =
                serde_json::from_value(changed).unwrap();
            layout_matches(&other);
            assert!(!commercial_loan_prefix_equal(&original, &other)?);
            assert_ne!(
                serde_json::to_vec(&original).unwrap(),
                serde_json::to_vec(&other).unwrap()
            );
        }
        let waivers = leaf_mutations(&serde_json::to_value(&state.legal_waivers[0]).unwrap());
        assert_eq!(waivers.len(), 8);
        for changed in waivers {
            let mut other = original.clone();
            other
                .next
                .reviewed_credit_state
                .as_mut()
                .unwrap()
                .legal_waivers[0] = serde_json::from_value(changed).unwrap();
            assert!(!commercial_loan_prefix_equal(&original, &other)?);
        }
        for accrued in [
            credit::AccruedPolicy::ReverseIncomeAtDefault,
            credit::AccruedPolicy::TransferUntilReviewedReversal,
        ] {
            for basis in [
                credit::BasisPolicy::TransferUntilReviewedChargeoff,
                credit::BasisPolicy::RecognizeInEarningsAtDefault,
            ] {
                for allowance_shortfall in [
                    credit::AllowanceShortfall::Reject,
                    credit::AllowanceShortfall::ExpenseAtReviewedChargeoff,
                ] {
                    for excess_cash_recovery in [
                        credit::ExcessCashRecovery::CreditAllowance,
                        credit::ExcessCashRecovery::RecognizeInEarnings,
                    ] {
                        let mut other = original.clone();
                        other.next.reviewed_credit_state.as_mut().unwrap().claims[0].policy =
                            credit::AccountingPolicy {
                                identity: identity("policy"),
                                accrued,
                                basis,
                                allowance_shortfall,
                                excess_cash_recovery,
                            };
                        layout_matches(&other);
                        assert_eq!(
                            commercial_loan_prefix_equal(&original, &other)?,
                            serde_json::to_vec(&original).unwrap()
                                == serde_json::to_vec(&other).unwrap()
                        );
                    }
                }
            }
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn reviewed_state_presence_signed_zero_recovery_order_and_complete_tail_are_preserved() {
    cut_shape::with_budget(500_000, || {
        let mut a = group();
        a.next.reviewed_credit_state = Some(reviewed());
        for change in 0..9 {
            let mut b = a.clone();
            let state = b.next.reviewed_credit_state.as_mut().unwrap();
            match change {
                0 => state.claims[0].outstanding_accrued_interest = 0.,
                1 => state.legal_waivers[0].accrued_interest_amount = 0.,
                2 => state.claims[0].remaining_recoveries.reverse(),
                3 => state.claims[0].settled_interest_recovery_ids.reverse(),
                4 => {
                    state.claims[0].remaining_recoveries.pop();
                }
                5 => b.next.reviewed_credit_state = None,
                6 => state.legal_principal += 1.,
                7 => state.model_gross_carry += 1.,
                8 => state.allowance += 1.,
                _ => unreachable!(),
            }
            assert!(!commercial_loan_prefix_equal(&a, &b)?);
        }
        assert!(commercial_loan_prefix_equal(&a, &a)?);
        let before = cut_shape::spent().unwrap();
        let shape = commercial_loan_prefix_shape(&a)?;
        assert_eq!(
            cut_shape::spent().unwrap() - before,
            shape.census_work_upper()?
        );
        let before = cut_shape::spent().unwrap();
        assert!(commercial_loan_prefix_equal(&a, &a)?);
        assert!(cut_shape::spent().unwrap() - before <= shape.comparison_work_upper(shape)?);
        a.next.reviewed_credit_state.as_mut().unwrap().claims[0].initial_principal = f64::NAN;
        assert!(commercial_loan_prefix_shape(&a).is_err());
        Ok(())
    })
    .unwrap();
    let a = reviewed();
    assert!(cut_shape::with_budget(1, || census(&a)).is_err());
}

/// Count actual requested capacities in the native clone independently of the
/// Typed visitor and JSON serializer. Inline fields are counted by size_of.
fn observed_snapshot_heap(s: &Snapshot) -> usize {
    use std::mem::size_of;
    let identity = |i: &Identity| i.id.capacity() + i.revision.capacity();
    let mut n = s.lots.capacity() * size_of::<CouponLot>()
        + s.pending_coupon_obligation_indices.capacity() * size_of::<usize>()
        + s.canonical_legal_coupons.capacity() * size_of::<CanonicalCouponLegalBalance>();
    for lot in &s.lots {
        n += lot.coupon_id.capacity()
            + lot.source_event_id.capacity()
            + lot.source_id.capacity()
            + lot.source_revision.capacity();
    }
    for c in &s.canonical_legal_coupons {
        n += c.coupon_id.capacity();
    }
    if let Some(r) = &s.reviewed_credit_state {
        n += r.claims.capacity() * size_of::<credit::RecoveryClaim>()
            + r.legal_waivers.capacity() * size_of::<book::reviews::LegalWaiver>();
        for c in &r.claims {
            n += c.claim_id.capacity()
                + identity(&c.economic_source)
                + identity(&c.recovery_schedule_review)
                + identity(&c.policy.identity)
                + c.settled_interest_recovery_ids.capacity() * size_of::<Identity>()
                + c.remaining_recoveries.capacity() * size_of::<credit::RecoveryPoint>();
            for i in &c.settled_interest_recovery_ids {
                n += identity(i);
            }
        }
        for w in &r.legal_waivers {
            n += identity(&w.review) + w.claim_id.capacity() + w.future_cash_disposition.capacity();
        }
    }
    n
}

#[test]
fn complete_snapshot_clone_payload_matches_actual_owned_capacities_with_reviewed_full_tails() {
    let mut source = group().next;
    source.reviewed_credit_state = Some(reviewed());
    source.pending_coupon_obligation_indices = vec![0, 9, 17];
    source.canonical_legal_coupons = vec![CanonicalCouponLegalBalance {
        coupon_id: "canonical 雪".into(),
        native_audit_index: 1,
        native_cash_index: 2,
        opening_entitlement_principal: 100.,
        cumulative_factor: 0.02,
        contractual_earned: 2.,
        actual_interest_collected: 0.,
        legal_remaining: 2.,
    }];
    // Original spare capacity is intentionally NOT cloned by immutable Snapshot::clone.
    source.lots.reserve(80);
    source.pending_coupon_obligation_indices.reserve(80);
    let reviewed = source.reviewed_credit_state.as_mut().unwrap();
    reviewed.claims.reserve(80);
    reviewed.legal_waivers.reserve(80);
    reviewed.claims[0].settled_interest_recovery_ids.reserve(80);
    reviewed.claims[0].remaining_recoveries.reserve(80);
    reviewed.claims[0].claim_id.reserve(1000);
    let layout = commercial_snapshot_clone_layout(&source).unwrap();
    let actual = source.clone();
    assert_eq!(layout.inline_bytes, std::mem::size_of::<Snapshot>());
    assert_eq!(layout.heap_bytes, observed_snapshot_heap(&actual));
    assert!(observed_snapshot_heap(&source) > layout.heap_bytes);
    assert!(layout.census_work > 100);
    let n = layout.census_work;
    cut_shape::with_budget(n, || {
        assert_eq!(
            commercial_snapshot_clone_layout(&source)?.heap_bytes,
            layout.heap_bytes
        );
        assert_eq!(cut_shape::spent(), Some(n));
        Ok(())
    })
    .unwrap();
    assert!(cut_shape::with_budget(n - 1, || commercial_snapshot_clone_layout(&source)).is_err());
    assert_eq!(cut_shape::spent(), None);
}

#[test]
fn empty_omitted_snapshot_vectors_and_numeric_values_do_not_hide_owned_payload() {
    let mut a = group().next;
    a.lots.clear();
    a.pending_coupon_obligation_indices.clear();
    a.canonical_legal_coupons.clear();
    a.reviewed_credit_state = None;
    assert_eq!(commercial_snapshot_clone_layout(&a).unwrap().heap_bytes, 0);
    let before = commercial_snapshot_clone_layout(&a).unwrap();
    a.principal = 0.;
    a.recognized_interest = -0.;
    a.in_default = true;
    assert_eq!(
        commercial_snapshot_clone_layout(&a).unwrap().heap_bytes,
        before.heap_bytes
    );
    assert_eq!(observed_snapshot_heap(&a.clone()), 0);
    assert_eq!(
        cut_shape::with_budget(1000, || fee::State::default().cloned_heap()).unwrap(),
        0
    );
}
