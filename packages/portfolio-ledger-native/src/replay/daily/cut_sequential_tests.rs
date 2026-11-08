// Included inside the established cut component tests to reuse their independent
// full-origin/dual-owner journal decoder and reviewed hand controls.
use crate::daily::SequentialCommercialCut;

#[test]
fn complete_sim_container_inventory_is_paid_and_cannot_prune_zero_money_or_labels() {
    let s = spec();
    let owner = source(17);
    let mut c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
    let measured = super::super::cut_shape::with_budget(100000, || {
        let bound = super::super::cut_sim_capacity::inventory(&s, &c)?;
        assert_eq!(bound.census_work, super::super::cut_shape::spent().unwrap());
        Ok(bound)
    })
    .unwrap();
    assert!(measured.all_runs_and_scratch_bytes > 32768);
    for binding in &mut c.bindings {
        binding.allocated_face = 0.;
    }
    let zero = super::super::cut_sim_capacity::inventory(&s, &c).unwrap();
    assert_eq!(
        measured.all_runs_and_scratch_bytes,
        zero.all_runs_and_scratch_bytes
    );
    let mut longer = s.clone();
    longer.accounts[0].entity.push_str(&"雪😀".repeat(100));
    assert!(
        super::super::cut_sim_capacity::inventory(&longer, &c)
            .unwrap()
            .all_runs_and_scratch_bytes
            > measured.all_runs_and_scratch_bytes
    );
    assert!(
        super::super::cut_shape::with_budget(measured.census_work - 1, || {
            super::super::cut_sim_capacity::inventory(&s, &c)
        })
        .is_err()
    );
    assert!(super::super::cut_shape::spent().is_none());
    let mut overlay = s.clone();
    overlay.positions[0].collateral_position = "other".into();
    assert!(super::super::cut_sim_capacity::inventory(&overlay, &c).is_err());
}

#[test]
fn finite_runtime_capacity_retains_full_fee_and_loan_state_at_zero_participation() {
    use crate::owners::commercial::intents::{finite_runtime_capacity, FeeBinding};
    let s = spec();
    let loan = source(17);
    let id = s.positions[0].id.as_str();
    let fee = prepaid(-1., id);
    let years = [0., 20. / 365.];
    let mut c = Compiled::new(&s, bindings(&loan, id, 17, 0)).unwrap();
    c.attach_fees(
        &s,
        vec![
            FeeBinding {
                loan_binding: 0,
                native_position_id: id,
                source: &fee,
                native_years: &years,
            },
            FeeBinding {
                loan_binding: 1,
                native_position_id: id,
                source: &fee,
                native_years: &years,
            },
        ],
    )
    .unwrap();
    let mut paid = 0;
    let a = finite_runtime_capacity(&c, |work| {
        paid += work;
        Ok(())
    })
    .unwrap();
    assert_eq!(a.census_work, paid);
    assert!(a.runtime_bytes_upper > 10000);
    assert!(a.temporary_bytes_upper > 0);
    for binding in &mut c.bindings {
        binding.allocated_face = 0.;
    }
    let b = finite_runtime_capacity(&c, |_| Ok(())).unwrap();
    assert_eq!(a.runtime_bytes_upper, b.runtime_bytes_upper);
    assert_eq!(a.temporary_bytes_upper, b.temporary_bytes_upper);
    assert_eq!(a.census_work, b.census_work);
    let error = finite_runtime_capacity(&c, |_| Err("original deadline".into()))
        .err()
        .unwrap();
    assert_eq!(error, "original deadline");
    let mut spent = 0;
    assert!(finite_runtime_capacity(&c, |work| {
        if spent + work > paid - 1 {
            return Err("exhausted".into());
        }
        spent += work;
        Ok(())
    })
    .is_err());
    assert_eq!(spent, paid - 1);
}

struct DroppedSource(HandSource, Arc<AtomicBool>);
impl Drop for DroppedSource {
    fn drop(&mut self) {
        self.1.store(true, Ordering::Relaxed);
    }
}

#[test]
fn sequential_releases_original_sources_and_moves_all_runs_with_exact_dual_owner_output() {
    for face in [0., 50., 100.] {
        for cut in [1, 5, 10, 15, 20, 29] {
            let s = spec();
            let owner = source(17);
            let id = s.positions[0].id.as_str();
            let mut rows = bindings(&owner, id, 17, 0);
            for row in &mut rows {
                row.allocated_face = face;
            }
            let c = Compiled::new(&s, rows).unwrap();
            let expected = buffer();
            run_commercial_cut(&s, &s, &c, &c, cut, Box::new(expected.clone())).unwrap();
            let actual = buffer();
            let dropped = Arc::new(AtomicBool::new(false));
            let checkpoint = {
                let original_spec = spec();
                let original = DroppedSource(source(17), dropped.clone());
                let mut rows = bindings(&original.0, original_spec.positions[0].id.as_str(), 17, 0);
                for row in &mut rows {
                    row.allocated_face = face;
                }
                let old = Compiled::new(&original_spec, rows).unwrap();
                SequentialCommercialCut::capture(
                    &original_spec,
                    &old,
                    cut,
                    Box::new(actual.clone()),
                )
                .unwrap()
            };
            assert!(dropped.load(Ordering::Relaxed));
            assert!(
                actual.0.lock().unwrap().is_empty(),
                "all prefix output stays muted"
            );
            let receipt = checkpoint.resume(&s, &c).unwrap();
            assert_eq!(receipt.runs_moved, 2);
            assert!(!receipt.completed_source_borrow_retained);
            assert!(!receipt.raw_coordinator_admission_granted);
            assert!(!receipt.independent_persisted_verification);
            assert!(receipt.component_work_upper <= 8_000_000);
            assert!(receipt.component_retained_bytes_upper <= 32 << 20);
            assert_eq!(tables(&actual), tables(&expected), "face={face},cut={cut}");
        }
    }
}

#[test]
fn sequential_future_acquisition_preserves_prior_money_and_does_not_reacquire_backbook() {
    let original_spec = spec();
    let mut revised = original_spec.clone();
    let mut new_position = revised.positions[0].clone();
    new_position.id = "appended".into();
    revised.positions.push(new_position);
    let old_owner = source(17);
    let new_owner = source(29);
    let old = Compiled::new(
        &original_spec,
        bindings(&old_owner, original_spec.positions[0].id.as_str(), 17, 0),
    )
    .unwrap();
    let mut rows = bindings(&old_owner, revised.positions[0].id.as_str(), 17, 0);
    rows.extend(bindings(&new_owner, "appended", 29, 5));
    let future = Compiled::new(&revised, rows).unwrap();
    let expected = buffer();
    run_commercial_cut(
        &original_spec,
        &revised,
        &old,
        &future,
        3,
        Box::new(expected.clone()),
    )
    .unwrap();
    let actual = buffer();
    let checkpoint =
        SequentialCommercialCut::capture(&original_spec, &old, 3, Box::new(actual.clone()))
            .unwrap();
    checkpoint.resume(&revised, &future).unwrap();
    assert_eq!(tables(&actual), tables(&expected));
}

#[test]
fn sequential_keeps_signed_unweighted_prepaid_fee_rights_and_fee_closing_after_owner_release() {
    use crate::owners::commercial::intents::FeeBinding;
    for sign in [1., -1.] {
        for face in [0., 50., 100.] {
            let s = spec();
            let owner = source(17);
            let id = s.positions[0].id.as_str();
            let fee = prepaid(sign, id);
            let years = [0., 20. / 365.];
            let mut rows = bindings(&owner, id, 17, 0);
            for row in &mut rows {
                row.allocated_face = face;
            }
            let mut c = Compiled::new(&s, rows).unwrap();
            c.attach_fees(
                &s,
                vec![
                    FeeBinding {
                        loan_binding: 0,
                        native_position_id: id,
                        source: &fee,
                        native_years: &years,
                    },
                    FeeBinding {
                        loan_binding: 1,
                        native_position_id: id,
                        source: &fee,
                        native_years: &years,
                    },
                ],
            )
            .unwrap();
            for cut in [1, 10, 20, 29] {
                let all = super::super::commercial_sequential_cut_inventory(&s, &c, cut).unwrap();
                let prefix = super::super::cut_inventory::sequential_phase_inventory(&s, &c, cut, true).unwrap();
                let future = super::super::cut_inventory::sequential_phase_inventory(&s, &c, cut, false).unwrap();
                assert!(prefix.visible.is_empty());
                assert!(future.prefix.is_empty());
                assert_eq!(serde_json::to_value(&prefix.prefix).unwrap(), serde_json::to_value(&all.prefix).unwrap());
                assert_eq!(serde_json::to_value(&future.visible).unwrap(), serde_json::to_value(&all.visible).unwrap());
                assert_eq!(serde_json::to_value(prefix.dictionary_upper).unwrap(), serde_json::to_value(all.dictionary_upper).unwrap());
                assert_eq!(serde_json::to_value(future.dictionary_upper).unwrap(), serde_json::to_value(all.dictionary_upper).unwrap());
                let expected = buffer();
                run_commercial_cut(&s, &s, &c, &c, cut, Box::new(expected.clone())).unwrap();
                let actual = buffer();
                let checkpoint = {
                    let s = spec();
                    let owner = source(17);
                    let id = s.positions[0].id.as_str();
                    let fee = prepaid(sign, id);
                    let mut rows = bindings(&owner, id, 17, 0);
                    for row in &mut rows {
                        row.allocated_face = face;
                    }
                    let mut c = Compiled::new(&s, rows).unwrap();
                    c.attach_fees(
                        &s,
                        vec![
                            FeeBinding {
                                loan_binding: 0,
                                native_position_id: id,
                                source: &fee,
                                native_years: &years,
                            },
                            FeeBinding {
                                loan_binding: 1,
                                native_position_id: id,
                                source: &fee,
                                native_years: &years,
                            },
                        ],
                    )
                    .unwrap();
                    SequentialCommercialCut::capture(&s, &c, cut, Box::new(actual.clone())).unwrap()
                };
                checkpoint.resume(&s, &c).unwrap();
                assert_eq!(tables(&actual), tables(&expected));
                future.verify_tables(&tables(&actual)).unwrap();
                assert!(actual.0.lock().unwrap().len() <= future.visible_stream.encoded_bytes_upper);
            }
        }
    }
}

#[test]
fn sequential_observed_amount_clock_policy_allocation_and_signed_zero_changes_refuse_before_visibility(
) {
    for mutation in 0..6 {
        let s = spec();
        let owner = source(17);
        let id = s.positions[0].id.as_str();
        let old = Compiled::new(&s, bindings(&owner, id, 17, 0)).unwrap();
        let actual = buffer();
        let checkpoint =
            SequentialCommercialCut::capture(&s, &old, 15, Box::new(actual.clone())).unwrap();
        let mut changed = source(17);
        match mutation {
            0 => changed.0[1].effective_interest += 1.,
            1 => changed.0[1].year = 10.5 / 365.,
            2 => changed.0[1].policy.revision = "changed".into(),
            3 => changed.0[1].cash = -0.,
            4 => changed.0[1].next.lots.push(
                portfolio_model_core::commercial_coupon_accounting::CouponLot {
                    coupon_id: "extra".into(),
                    source_event_id: "extra".into(),
                    source_id: "source".into(),
                    source_revision: "1".into(),
                    native_audit_index: 0,
                    native_cash_index: 0,
                    earning_origin_index: 0,
                    contractual_earned: 0.,
                    legal_cash_collected: 0.,
                    legal_remaining: 0.,
                    recognized_earned: 0.,
                    recognized_cash_applied: 0.,
                    recognized_remaining: 0.,
                    default_claim: false,
                },
            ),
            5 => {}
            _ => unreachable!(),
        }
        let mut rows = bindings(&changed, id, 17, 0);
        if mutation == 5 {
            for row in &mut rows {
                row.allocated_face = 50.;
            }
        }
        let future = Compiled::new(&s, rows).unwrap();
        assert!(
            checkpoint.resume(&s, &future).is_err(),
            "mutation={mutation}"
        );
        assert!(actual.0.lock().unwrap().is_empty());
    }
}

#[test]
fn sequential_original_cancellation_remains_effective_across_source_phases() {
    let s = spec();
    let owner = source(17);
    let c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
    let actual = buffer();
    let control = portfolio_compute_control::Control::new(10_000).unwrap();
    let checkpoint = portfolio_compute_control::scope(Some(control.clone()), || {
        SequentialCommercialCut::capture(&s, &c, 5, Box::new(actual.clone()))
    })
    .unwrap();
    control.cancel();
    let result = portfolio_compute_control::scope(Some(control), || checkpoint.resume(&s, &c));
    assert_eq!(result.err().unwrap(), "COMPUTE_CANCELLED");
    assert!(actual.0.lock().unwrap().is_empty());
}

#[test]
fn sequential_original_deadline_expires_between_source_phases_without_visible_output() {
    let s = spec();
    let owner = source(17);
    let c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
    let actual = buffer();
    let control = portfolio_compute_control::Control::new(1_000).unwrap();
    let checkpoint = portfolio_compute_control::scope(Some(control.clone()), || {
        SequentialCommercialCut::capture(&s, &c, 5, Box::new(actual.clone()))
    })
    .unwrap();
    // The original absolute deadline survives leaving the first source scope.
    // A fresh deadline would incorrectly let this resumed phase execute.
    std::thread::sleep(std::time::Duration::from_millis(1_005));
    let result = portfolio_compute_control::scope(Some(control), || checkpoint.resume(&s, &c));
    assert_eq!(result.err().unwrap(), "COMPUTE_DEADLINE_EXCEEDED");
    assert!(actual.0.lock().unwrap().is_empty());
}

#[test]
fn sequential_prefix_proof_and_real_sim_work_share_one_nonreplaceable_budget() {
    let s = spec();
    let owner = source(17);
    let c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
    let expected = buffer();
    run_commercial_cut(&s, &s, &c, &c, 5, Box::new(expected.clone())).unwrap();
    let actual = buffer();
    super::super::cut_shape::with_budget(8_000_000, || {
        let checkpoint = SequentialCommercialCut::capture(&s, &c, 5, Box::new(actual.clone()))?;
        assert!(actual.0.lock().unwrap().is_empty());
        let prefix_spent = super::super::cut_shape::spent().unwrap();
        assert!(prefix_spent > 0);
        let receipt = checkpoint.resume(&s, &c)?;
        let spent = super::super::cut_shape::spent().unwrap();
        assert!(spent > prefix_spent);
        assert!(
            spent <= receipt.component_work_upper,
            "paid={spent},upper={}",
            receipt.component_work_upper
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(tables(&actual), tables(&expected));
    assert_eq!(super::super::cut_shape::spent(), None);
}

#[test]
fn sequential_exhausted_prefix_sizing_budget_refuses_before_any_visible_output() {
    let s = spec();
    let owner = source(17);
    let c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
    for limit in [0, 1, 100] {
        let actual = buffer();
        assert!(
            super::super::cut_shape::with_budget(limit, || SequentialCommercialCut::capture(
                &s,
                &c,
                5,
                Box::new(actual.clone())
            ))
            .is_err()
        );
        assert!(actual.0.lock().unwrap().is_empty());
        assert_eq!(super::super::cut_shape::spent(), None);
    }
}
