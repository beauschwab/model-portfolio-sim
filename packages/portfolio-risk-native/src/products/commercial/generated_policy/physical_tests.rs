//! Genuine generated draw source, acquisition proof and action ownership.
use super::*;
use crate::term_enhancements::indexed::{Coupon, PrincipalPlan};
use physical::{PathInputs, RoleInputs, ScenarioInputs, SourceRecipe};
fn draw_source(raw: &owner::Request) -> owner::PerformingDrawTranche {
    let mut source = raw.source.clone();
    let at = source.current.asof_ordinal + 1;
    for contract in [&mut source.original, &mut source.current] {
        contract.contract.id = "generated-draw-contract".into();
        contract.asof_ordinal = at;
        contract.reporting_end_ordinal = at + 4;
        contract.tail_end_ordinal = at + 4;
        contract.opening_principal = 20.;
        let mut period = contract.periods[0].clone();
        period.period_id = "generated-draw-coupon".into();
        period.start_ordinal = at;
        period.end_ordinal = at + 4;
        period.payment_ordinal = at + 4;
        period.coupon = Coupon::Fixed { annual_rate: 0. };
        contract.periods = vec![period];
        contract.principal_plan = PrincipalPlan::Supplied {
            principal_payments: vec![20.],
        };
    }
    source.acquisition.identity.id = "generated-draw-acquisition".into();
    source.acquisition.cost = 20.;
    source.acquisition.original_annual_effective_rate = 0.;
    owner::PerformingDrawTranche {
        review: crate::term_enhancements::Identity {
            id: "generated-draw".into(),
            revision: "1".into(),
        },
        draw_ordinal: at,
        eligible_origins: [true, false, false],
        source,
    }
}
fn add_draw(law: &mut stochastic::Request) {
    // Small complete contractual owner remains within existing aggregate caps.
    for contract in [
        &mut law.source.source.original,
        &mut law.source.source.current,
    ] {
        let mut period = contract.periods[0].clone();
        period.end_ordinal = contract.asof_ordinal + 30;
        period.payment_ordinal = contract.asof_ordinal + 35;
        contract.periods = vec![period];
        contract.principal_plan = PrincipalPlan::Supplied {
            principal_payments: vec![100.],
        };
    }
    law.coupon_loadings.clear();
    law.hazard_loadings.clear();
    law.source.credit.intervals.truncate(1);
    law.source.credit.intervals[0].end_year = 35. / 365.;
    law.source.accounting.recognition.truncate(1);
    law.source.accounting_rates.truncate(1);
    let draw = draw_source(&law.source);
    law.source.credit.opening_undrawn_commitment = 20.;
    law.source.credit.max_jumps_per_path = 1;
    law.source.performing_draw_tranches = vec![draw];
}
fn inputs(law: &stochastic::Request) -> RoleInputs {
    let mut recipes = Vec::new();
    for variant in 0..2 {
        let mut draws = law.source.performing_draw_tranches.clone();
        for draw in &mut draws {
            draw.source.original.path.id = format!("reviewed-draw-anchor-{variant}");
            draw.source.current.path.id = format!("reviewed-draw-anchor-{variant}");
        }
        recipes.push(SourceRecipe {
            identity: crate::term_enhancements::Identity {
                id: format!("recipe-{variant}"),
                revision: "1".into(),
            },
            performing_draw_tranches: draws,
        });
    }
    RoleInputs {
        source_recipes: recipes,
        paths: (0..law.evaluation.paths)
            .map(|i| PathInputs {
                path_key: i as u64 + 1,
                review: crate::term_enhancements::Identity {
                    id: format!("path-review-{i}"),
                    revision: "1".into(),
                },
                source_recipe_index: i % 2,
                reviewed_credit_actions: None,
            })
            .collect(),
    }
}
fn request() -> Request {
    let mut r = tests::request();
    for law in std::iter::once(&mut r.pricing_training)
        .chain(std::iter::once(&mut r.original_physical))
        .chain(&mut r.current_physical)
        .chain(&mut r.current_pricing)
    {
        add_draw(law);
    }
    r.physical_path_inputs = Some(physical::PhysicalPathInputs {
        schema: physical::SCHEMA.into(),
        identity: crate::term_enhancements::Identity {
            id: "explicit-physical-path-inputs".into(),
            revision: "1".into(),
        },
        original_physical: inputs(&r.original_physical),
        current_physical: r
            .current_physical
            .iter()
            .map(|law| ScenarioInputs {
                scenario_id: law.source.source.current.scenario_id.clone(),
                inputs: inputs(law),
            })
            .collect(),
    });
    r
}
#[test]
fn generated_physical_catalog_draws_acquire_once_and_keep_path_specific_original_anchors() {
    let r = request();
    let result = prepare(&r).unwrap();
    assert_eq!(result.current_physical[0].sources.len(), 128);
    for source in &result.current_physical[0].sources {
        let projection = source
            .materialize(&result.original_acquisition_proof)
            .unwrap();
        let tranches = projection
            .carry()
            .carry_points()
            .iter()
            .filter(|p| p.kind == "funded_draw")
            .count();
        assert_eq!(tranches, 1);
        assert_eq!(
            source.raw().performing_draw_tranches[0]
                .source
                .acquisition
                .cost,
            20.
        );
        assert_eq!(source.raw().source.acquisition.cost, 100.);
    }
    let proof = serde_json::to_value(&result.original_acquisition_proof).unwrap();
    assert_eq!(
        proof["conditional_tranche_proofs"][0]["activation_probability"],
        1.
    );
    assert_eq!(
        proof["conditional_tranche_proofs"][0]["conditional_cash_quote"],
        20.
    );
    let mut swapped = result.current_physical[0].sources[0].raw().clone();
    swapped.performing_draw_tranches[0].source.original = result.current_physical[0].sources[1]
        .raw()
        .performing_draw_tranches[0]
        .source
        .original
        .clone();
    assert!(owner::evaluate_with_native_original_acquisition(
        &swapped,
        result.current_physical[0].sources[0].stops(),
        &result.original_acquisition_proof
    )
    .unwrap_err()
    .contains("immutable original"));
}
#[test]
fn generated_physical_incomplete_catalog_foreign_actions_and_anchor_edits_reject_before_generation()
{
    let r = request();
    let mut bad = r.clone();
    bad.physical_path_inputs
        .as_mut()
        .unwrap()
        .original_physical
        .paths
        .pop();
    assert!(admit(&bad).unwrap_err().contains("complete bounded"));
    let mut bad = r.clone();
    bad.physical_path_inputs.as_mut().unwrap().current_physical[0]
        .inputs
        .paths[0]
        .source_recipe_index = 99;
    assert!(admit(&bad).unwrap_err().contains("recipe absent"));
    let mut bad = r.clone();
    bad.physical_path_inputs.as_mut().unwrap().current_physical[0]
        .inputs
        .source_recipes[1]
        .performing_draw_tranches[0]
        .source
        .original
        .path
        .id = "changed-original".into();
    assert!(admit(&bad).unwrap_err().contains("per-path original"));
    let mut bad = r.clone();
    bad.current_physical[0].source.performing_draw_tranches[0]
        .source
        .current
        .periods[0]
        .period_id = "different-feature-inventory".into();
    assert!(admit(&bad).is_err());
}
#[test]
fn generated_tranche_features_hide_future_acquisition_and_labels_until_observable() {
    let r = request();
    let law = &r.current_physical[0];
    let tape = stochastic::generate_tape(law, &law.evaluation).unwrap();
    let raw = physical::generated_path(law, physical::current(&r, law).unwrap(), &tape, 0, false)
        .unwrap();
    let carry = owner::evaluate_physical_prefix_for_acquisition(&raw).unwrap();
    let mut date = law.policy.exercise_dates[0].clone();
    date.ordinal = law.source.source.current.asof_ordinal;
    let mut grid_law = law.clone(); // Origin is in the existing observable driver grid.
    let before =
        stochastic::compiled_point_features(&grid_law, &raw, carry.carry(), &tape, 0, &date)
            .unwrap();
    assert_eq!(
        &before.features[before.features.len() - 4..],
        &[0., 0., 0., 0.]
    );
    let mut changed = raw.clone();
    changed.performing_draw_tranches[0].source.current.path.id =
        "future-lineage-must-not-leak".into();
    let same =
        stochastic::compiled_point_features(&grid_law, &changed, carry.carry(), &tape, 0, &date)
            .unwrap();
    assert_eq!(before.features, same.features);
    date = law.policy.exercise_dates[0].clone();
    grid_law.time_ordinals = law.time_ordinals.clone();
    let after =
        stochastic::compiled_point_features(&grid_law, &raw, carry.carry(), &tape, 0, &date)
            .unwrap();
    assert_eq!(after.features[after.features.len() - 4], 1.);
}

#[test]
fn generated_physical_reviewed_actions_are_path_specific_without_fitting_q() {
    let mut r = request();
    let policy = stochastic::fit_training(&r.pricing_training).unwrap();
    let at = r.original_physical.source.source.current.asof_ordinal;
    r.current_physical[0].source.credit.intervals[0].hazards_per_year[0][3] = 100.;
    r.current_physical[0].source.credit.intervals[0].loss_given_default = [1.; 3];
    let actions:portfolio_model_core::commercial_coupon_accounting::reviews::ReviewedActions=serde_json::from_value(serde_json::json!({
        "policy":{"identity":{"id":"path-1-credit-review","revision":"1"},"accrued":"transfer_until_reviewed_reversal","basis":"transfer_until_reviewed_chargeoff","allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"},
        "allowance_reviews":[{"path_key":1,"review":{"review":{"id":"path-1-allowance","revision":"1"},"recognition_ordinal":at+30,"phase":"before_credit","target_amount":50.}}],
        "chargeoffs":[{"path_key":1,"review":{"review":{"id":"path-1-chargeoff","revision":"1"},"claim_id":"commercial-default:1","recognition_ordinal":at+30,"principal_amount":10.,"book_adjustment_amount":0.,"accrued_interest_reversal":0.}}],
        "legal_waivers":[{"path_key":1,"review":{"id":"path-1-waiver","revision":"1"},"recognition_ordinal":at+30,"claim_id":"commercial-default:1","principal_amount":5.,"accrued_interest_amount":0.,"future_cash_disposition":"retain_supplied_voluntary_recovery_cash-1"}]
    })).unwrap();
    r.physical_path_inputs.as_mut().unwrap().current_physical[0]
        .inputs
        .paths[0]
        .reviewed_credit_actions = Some(actions);
    let result = prepare(&r).unwrap();
    assert_eq!(
        strict_json::encode(&policy, 1 << 20).unwrap(),
        strict_json::encode(&result.original_pricing.fitted_policy, 1 << 20).unwrap()
    );
    let source = &result.current_physical[0].sources[0];
    let projection = source
        .materialize(&result.original_acquisition_proof)
        .unwrap();
    assert!(projection.carry().credit().result.paths[0]
        .default
        .is_some());
    let close = projection.accounting().closing()[0]
        .reviewed_credit_state
        .as_ref()
        .unwrap();
    assert_eq!(close.allowance, 40.);
    assert_eq!(close.legal_waivers.len(), 1);
    assert!(result.current_physical[0].sources[1]
        .raw()
        .reviewed_credit_actions
        .is_none());
    let mut foreign = r.clone();
    foreign
        .physical_path_inputs
        .as_mut()
        .unwrap()
        .current_physical[0]
        .inputs
        .paths[0]
        .reviewed_credit_actions
        .as_mut()
        .unwrap()
        .allowance_reviews[0]
        .path_key = 2;
    assert!(admit(&foreign)
        .unwrap_err()
        .contains("exact supplied path key"));
}

#[test]
fn generated_selected_role_pass_reservation_covers_measured_true_owner_and_source_generations() {
    use crate::products::commercial::factor_source::owner::SOURCE_CALLS;
    let r = request();
    owner::OWNER_EVALUATIONS.with(|n| n.set(0));
    SOURCE_CALLS.with(|n| n.set(0));
    let result = prepare(&r).unwrap();
    let q_passes = r.pricing_training.training.paths
        + 2 * r.pricing_training.evaluation.paths
        + r.current_pricing
            .iter()
            .map(|q| q.training.paths + 2 * q.evaluation.paths)
            .sum::<usize>();
    let original_passes = 3 * r.original_physical.evaluation.paths;
    let current_passes = r
        .current_physical
        .iter()
        .map(|p| 2 * p.evaluation.paths)
        .sum::<usize>();
    let measured = owner::OWNER_EVALUATIONS.with(|n| n.get());
    assert_eq!(measured, q_passes + original_passes + current_passes);
    // Each genuine owner materializes original/current main and original/current
    // draw sources. Admission and all prospective payoffs materialize none.
    assert_eq!(SOURCE_CALLS.with(|n| n.get()), measured * 4);
    let mut reserved = 0usize;
    for law in std::iter::once(&r.pricing_training).chain(&r.current_pricing) {
        let owner = stochastic::admitted_source(law, CreditMeasure::Pricing).unwrap();
        let (component, _) = stochastic::admitted_law(law, CreditMeasure::Pricing).unwrap();
        let (selected, _) = pricing_admission(&r, law).unwrap();
        assert_eq!(
            component - selected,
            law.training.paths * owner.joined_work_upper
        );
        reserved += (law.training.paths + 2 * law.evaluation.paths) * owner.joined_work_upper;
    }
    let original = physical::upper(&r.original_physical, physical::original(&r), true).unwrap();
    reserved += 2 * original.0;
    for law in &r.current_physical {
        reserved += physical::upper(law, physical::current(&r, law).unwrap(), false)
            .unwrap()
            .0;
    }
    assert!(result.admission.work_upper >= reserved);
    assert!(result.admission.work_upper <= MAX_WORK);
    let mut omitted = r.clone();
    omitted.physical_path_inputs = None;
    let (component, _) =
        stochastic::admitted_law(&omitted.pricing_training, CreditMeasure::Pricing).unwrap();
    assert_eq!(
        pricing_admission(&omitted, &omitted.pricing_training)
            .unwrap()
            .0,
        component
    );
}

#[test]
fn generated_draw_loading_uses_own_source_and_keeps_unobserved_features_zero() {
    let mut q = request().pricing_training;
    let at = q.source.source.current.asof_ordinal;
    let mut coupon = tests::request()
        .pricing_training
        .source
        .source
        .current
        .periods[1]
        .coupon
        .clone();
    let Coupon::Indexed {
        observations,
        reset_cutoff_unix_seconds,
        ..
    } = &mut coupon
    else {
        unreachable!()
    };
    let clock = i64::from(at + 1 - 719163) * 86400;
    observations[0].interest_start_ordinal = at + 1;
    observations[0].interest_end_ordinal = at + 5;
    observations[0].observation_start_ordinal = at + 1;
    observations[0].observation_end_ordinal = at + 5;
    observations[0].fixing_ordinal = at + 1;
    observations[0].selection_cutoff_unix_seconds = clock;
    observations[0].rate = 0.02;
    observations[0].source = crate::term_enhancements::indexed::RateSource::ConditionalProjection {
        model: q.identity.clone(),
        path: q.source.performing_draw_tranches[0]
            .source
            .current
            .path
            .clone(),
        information_unix_seconds: clock,
        measure: portfolio_model_core::cashflow::CashflowMeasure::Pricing,
    };
    *reset_cutoff_unix_seconds = Some(clock);
    q.source.performing_draw_tranches[0].source.original.periods[0].coupon = coupon.clone();
    q.source.performing_draw_tranches[0].source.current.periods[0].coupon = coupon;
    q.time_ordinals.insert(1, at + 1);
    q.coupon_loadings = vec![stochastic::CouponLoading {
        draw_tranche_id: Some("generated-draw".into()),
        period_id: "generated-draw-coupon".into(),
        observation_index: 0,
        driver_index: 0,
        loading: 1.,
    }];
    stochastic::admit(&q).unwrap();
    let tape = stochastic::generate_tape(&q, &q.evaluation).unwrap();
    let raw =
        stochastic::generated_path(&q, &q.evaluation, &tape, 0, CreditMeasure::Pricing).unwrap();
    let Coupon::Indexed { observations, .. } =
        &raw.performing_draw_tranches[0].source.current.periods[0].coupon
    else {
        unreachable!()
    };
    assert_ne!(observations[0].rate, 0.02);
    let projection = owner::evaluate_economic_with_measure(&raw, CreditMeasure::Pricing).unwrap();
    let mut date = q.policy.exercise_dates[0].clone();
    date.ordinal = at;
    let point =
        stochastic::compiled_point_features(&q, &raw, projection.carry(), &tape, 0, &date).unwrap();
    assert_eq!(
        &point.features[point.features.len() - 5..],
        &[0., 0., 0., 0., 0.]
    );
    q.coupon_loadings[0].draw_tranche_id = None;
    assert!(stochastic::admit(&q)
        .unwrap_err()
        .contains("loading period absent"));
}
#[test]
fn generated_draw_calibration_refits_all_selected_bases_without_regenerating_sources() {
    use crate::products::commercial::factor_source::owner::SOURCE_CALLS;
    let mut q = request().pricing_training;
    q.annual_covariance = vec![0.];
    for basis in [
        "native_observable_linear_svd_train_evaluate-1",
        "native_observable_quadratic_svd_train_evaluate-1",
        "native_observable_cubic_svd_train_evaluate-1",
    ] {
        q.regression_policy = basis.into();
        let policy = stochastic::calibration::Policy {
            policy: "native_retained_q_refit_bracket-1".into(),
            lower_oas: -0.01,
            upper_oas: 0.01,
            max_trials: 3,
        };
        owner::OWNER_EVALUATIONS.with(|n| n.set(0));
        SOURCE_CALLS.with(|n| n.set(0));
        let solved = stochastic::calibration::solve(&q, &policy, 100.).unwrap();
        assert_eq!(solved.receipt.trials.len(), 3);
        let measured = owner::OWNER_EVALUATIONS.with(|n| n.get());
        assert_eq!(measured, q.training.paths + 2 * q.evaluation.paths);
        assert_eq!(SOURCE_CALLS.with(|n| n.get()), measured * 4);
        let (work, _) = stochastic::calibration::admitted(&q, &policy).unwrap();
        assert!(
            work >= measured
                * stochastic::admitted_source(&q, CreditMeasure::Pricing)
                    .unwrap()
                    .joined_work_upper
        );
    }
}
