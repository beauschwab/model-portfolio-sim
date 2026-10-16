//! Source construction/query ledgers and authority controls in genuine owners.
use super::*;
use crate::commercial_factor_source::owner::NativeFactors;
use portfolio_model_core::cashflow::{ActualDayCount, CashflowMeasure};
fn selected() -> Request {
    let mut r = physical_tests::request();
    r.physical_path_inputs.as_mut().unwrap().source_work_policy =
        Some(physical::SourceWorkPolicy::CountedNativeContractFactorWork);
    r
}
#[test]
fn counted_catalog_true_owners_keep_fixed_source_cash_and_legacy_omission() {
    let legacy = physical_tests::request();
    let before = admit(&legacy).unwrap();
    let original = prepare(&legacy).unwrap();
    let selected = selected();
    let after = admit(&selected).unwrap();
    assert!(after.work_upper < before.work_upper);
    owner::OWNER_EVALUATIONS.with(|n| n.set(0));
    owner::COUNTED_OWNER_WORK.with(|n| n.set(0));
    crate::commercial_factor_source::owner::SOURCE_CALLS.with(|n| n.set(0));
    let counted = prepare(&selected).unwrap();
    let owners = owner::OWNER_EVALUATIONS.with(|n| n.get());
    assert_eq!(
        crate::commercial_factor_source::owner::SOURCE_CALLS.with(|n| n.get()),
        owners * 4
    );
    assert!(owner::COUNTED_OWNER_WORK.with(|n| n.get()) <= after.work_upper);
    assert_eq!(
        strict_json::encode(&original.current_physical[0].sources[0].cash, 1 << 20).unwrap(),
        strict_json::encode(&counted.current_physical[0].sources[0].cash, 1 << 20).unwrap()
    );
    let raw = counted.current_physical[0].sources[0].raw();
    let result = owner::evaluate_economic_with_measure(raw, CreditMeasure::Physical).unwrap();
    for source in std::iter::once(&result.original_source)
        .chain(std::iter::once(&result.current_source))
        .chain(
            result
                .draw_sources
                .iter()
                .flat_map(|d| [&d.original, &d.current]),
        )
    {
        let ops = source.counted_operations().unwrap();
        let bound = source.admission().counted_work.as_ref().unwrap();
        assert!(ops.work().unwrap() <= bound.upper.work().unwrap());
        assert_eq!(ops.periods, source.raw().periods.len());
        assert_eq!(ops.segments, source.segments().len());
        assert!(ops.input_bytes > 0 && ops.output_bytes > 0);
        assert!(ops.binding > 0 && ops.calendar > 0);
    }
    assert!(
        result
            .current_source
            .counted_operations()
            .unwrap()
            .canonical_calls
            > 0
    );
    assert_eq!(admit(&legacy).unwrap().work_upper, before.work_upper);
    assert_eq!(
        admit(&legacy).unwrap().retained_bytes_upper,
        before.retained_bytes_upper
    );
}
#[test]
fn counted_source_authority_is_private_and_exact_after_generated_expansion() {
    let selected = selected_request(&selected()).unwrap().into_owned();
    let law = &selected.current_physical[0];
    let mut raw =
        physical::source(law, physical::current(&selected, law).unwrap(), 0, false).unwrap();
    for field in 0..5 {
        let mut changed = raw.clone();
        match field {
            0 => changed.source.current.path.id.push('x'),
            1 => changed.source.original.path.revision.push('x'),
            2 => changed.source.current.periods[0].payment_ordinal -= 1,
            3 => changed.performing_draw_tranches[0].draw_ordinal += 1,
            _ => changed.credit.paths[0].path_key += 1,
        };
        assert!(owner::admit(&changed)
            .unwrap_err()
            .contains("stale counted"));
    }
    let mut encoded = serde_json::to_value(&raw).unwrap();
    encoded["counted_work"] = serde_json::json!({"allowance":0});
    assert!(serde_json::from_value::<owner::Request>(encoded).is_err());
    let bytes = raw.counted_work.as_ref().unwrap().retained_bytes();
    assert!(bytes >= strict_json::serialized_size(&raw, 1 << 20).unwrap());
    raw.counted_work = None;
    assert!(owner::admit(&raw).unwrap().original_source_work >= 250_000);
}
#[test]
fn counted_indexed_source_calendar_and_binding_ledgers_retain_numerics() {
    let mut q: stochastic::Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-generated-exercise-1.json"
    ))
    .unwrap();
    let mut raw = (*q.source).clone();
    bind_counted_source(&mut raw).unwrap();
    let policy = raw.factor_policy.clone();
    let legacy = NativeFactors::compile_for_measure(
        raw.source.current.clone(),
        policy.clone(),
        100,
        CashflowMeasure::Pricing,
    )
    .unwrap();
    let selected = NativeFactors::compile_for_measure_with_work(
        raw.source.current.clone(),
        policy,
        100,
        CashflowMeasure::Pricing,
        raw.counted_work.as_deref(),
    )
    .unwrap();
    assert_eq!(
        strict_json::encode(legacy.source(), 1 << 20).unwrap(),
        strict_json::encode(selected.source(), 1 << 20).unwrap()
    );
    let end =
        f64::from(raw.source.current.tail_end_ordinal - raw.source.current.asof_ordinal) / 365.;
    selected.earned(0., end).unwrap();
    selected.cumulative_coupon_factors(end / 2.).unwrap();
    let ops = selected.counted_operations().unwrap();
    assert!(ops.observations > 0 && ops.earned_calls == 1 && ops.canonical_calls == 1);
    assert!(ops.canonical_steps >= raw.source.current.periods.len());
    // An ACT/ACT multi-year fixed source forces real January-boundary scans.
    let at = q.source.source.current.asof_ordinal;
    let c = &mut q.source.source.current;
    c.periods.truncate(1);
    c.periods[0].coupon = crate::term_enhancements::indexed::Coupon::Fixed { annual_rate: 0. };
    c.periods[0].start_ordinal = at;
    c.periods[0].end_ordinal = at + 800;
    c.periods[0].payment_ordinal = at + 800;
    c.periods[0].day_count = ActualDayCount::ActActIsda;
    c.tail_end_ordinal = at + 800;
    c.reporting_end_ordinal = at + 800;
    c.principal_plan = crate::term_enhancements::indexed::PrincipalPlan::Supplied {
        principal_payments: vec![c.opening_principal],
    };
    let source = NativeFactors::compile_for_measure_with_work(
        c.clone(),
        q.source.factor_policy.clone(),
        10,
        CashflowMeasure::Pricing,
        raw.counted_work.as_deref(),
    )
    .unwrap();
    source.earned(0., 799. / 365.).unwrap();
    assert!(source.counted_operations().unwrap().calendar >= 5);
}

#[test]
fn counted_shared_authority_deduction_requires_the_same_sealed_allocation() {
    let r = selected_request(&selected()).unwrap().into_owned();
    let original = &r.pricing_training.source;
    let cloned = original.clone();
    let a = original.counted_work.as_ref().unwrap();
    assert!(a.shared_body_lower_with(cloned.counted_work.as_ref().unwrap()) > 0);
    let mut separate = (**original).clone();
    bind_counted_source(&mut separate).unwrap();
    assert_eq!(
        a.shared_body_lower_with(separate.counted_work.as_ref().unwrap()),
        0
    );
    separate.source.current.path.id.push('x');
    bind_counted_source(&mut separate).unwrap();
    assert_eq!(
        a.shared_body_lower_with(separate.counted_work.as_ref().unwrap()),
        0
    );
    assert!(
        std::mem::size_of::<crate::products::term::contracts::indexed::work::ContractWorkSummary>()
            + std::mem::size_of::<crate::products::term::contracts::indexed::work::WorkMeter>()
            < 1024
    );
}

#[test]
fn counted_calibration_retains_all_source_passes_for_each_selected_degree() {
    let r = selected_request(&selected()).unwrap().into_owned();
    let mut q = r.pricing_training.clone();
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
        owner::COUNTED_OWNER_WORK.with(|n| n.set(0));
        crate::commercial_factor_source::owner::SOURCE_CALLS.with(|n| n.set(0));
        let solved = stochastic::calibration::solve(&q, &policy, 100.).unwrap();
        assert_eq!(solved.receipt.trials.len(), 3);
        let owners = owner::OWNER_EVALUATIONS.with(|n| n.get());
        assert_eq!(owners, q.training.paths + 2 * q.evaluation.paths);
        assert_eq!(
            crate::commercial_factor_source::owner::SOURCE_CALLS.with(|n| n.get()),
            owners * 4
        );
        assert!(
            owner::COUNTED_OWNER_WORK.with(|n| n.get())
                <= stochastic::calibration::admitted(&q, &policy).unwrap().0
        );
    }
}

#[test]
fn counted_inline_runtime_fields_fit_charged_source_and_recipe_cells() {
    use std::mem::size_of;
    // A source always has P>=1 and S>=1: its historical 4096*(P+S+Q*S)
    // source allocation already includes these complete inline owner headers.
    assert!(size_of::<NativeFactors>() < 4096);
    // Count the new pointer plus the full native stop header, <=256 byte event
    // id, four 128 byte review strings, six replaced global identity strings,
    // <=13 byte generated path label, String over-allocation and allocation
    // headers. Per-observation model/path changes have their own 2048 reserve.
    let inline = size_of::<
        Option<Box<crate::products::term::contracts::indexed::work::GeneratedCatalogAuthority>>,
    >();
    let labels_and_stop =
        inline + size_of::<owner::NativeStop>() + 2 * (256 + 4 * 128 + 6 * 128 + 13) + 64;
    assert!(labels_and_stop <= 8192);
}
