use portfolio_risk_native::{commercial_graph as graph, commercial_units as units};
use serde_json::{json, Value};
fn raw() -> graph::Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-generated-graph-1.json"
    ))
    .unwrap()
}
fn calibrated() -> graph::Request {
    let mut r = raw();
    let spread: f64 = 0.017;
    let quote = 100. * (-spread * 35. / 365.).exp();
    let p = &mut r.positions[0];
    p.original_signed_cash_quote = quote;
    p.oas_target = serde_json::from_value(json!({"kind":"calibrate_positive_cash","target_cash_price":quote,"source_id":"original-quote","source_revision":"1"})).unwrap();
    let g = p.generated_policy.as_mut().unwrap();
    g.oas_calibration = Some(serde_json::from_value(json!({"policy":"native_retained_q_refit_bracket-1","lower_oas":-0.02,"upper_oas":0.06,"max_trials":48})).unwrap());
    for law in std::iter::once(&mut g.pricing_training)
        .chain(std::iter::once(&mut g.original_physical))
        .chain(g.current_pricing.iter_mut())
        .chain(g.current_physical.iter_mut())
    {
        law.policy.exercise_dates.clear();
        law.annual_covariance[0] = 0.;
        law.source.source.acquisition.cost = quote;
        law.source.source.acquisition.original_annual_effective_rate = spread.exp() - 1.;
        for rate in &mut law.source.accounting_rates {
            rate.annual_effective_rates = [spread.exp() - 1.; 3];
        }
    }
    r
}
#[test]
fn original_generated_calibration_retains_full_cash_tail_fixed_oas_and_physical_yield() {
    let r = calibrated();
    let result = graph::analyze(&r).unwrap();
    let oas = result.original_oas[0].base_oas;
    assert!((oas - 0.017).abs() < 2e-10);
    assert!((result.metrics[0].pv - r.positions[0].original_signed_cash_quote).abs() < 1.1e-9);
    let receipt = result.generated_oas_calibration.as_ref().unwrap();
    assert_eq!(receipt.selected_base_oas.to_bits(), oas.to_bits());
    assert!(receipt.continuation_refitted_each_trial);
    assert!(!receipt.physical_outcomes_used_for_calibration);
    for policy in &result.exercise_policies {
        assert_eq!(
            policy.policy["request"]["fixed_base_oas"]
                .as_f64()
                .unwrap()
                .to_bits(),
            oas.to_bits()
        );
    }
    let physical = result
        .native_sources
        .iter()
        .find(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
        .unwrap();
    assert_eq!(
        physical.source_request.source.acquisition.cost.to_bits(),
        r.positions[0].original_signed_cash_quote.to_bits()
    );
    assert_eq!(
        physical
            .source_request
            .source
            .acquisition
            .original_annual_effective_rate
            .to_bits(),
        (0.017_f64.exp() - 1.).to_bits()
    );
    assert!(
        (physical
            .materialize_generated_physical()
            .unwrap()
            .unwrap()
            .original_cash_quote()
            - r.positions[0].original_signed_cash_quote)
            .abs()
            < 1e-9
    );
    assert!(result.admitted_work_upper <= 4_000_000_000);
}
#[test]
fn calibrated_generated_units_reuse_original_anchors_and_full_tail_coefficients() {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-units-1.json"
    ))
    .unwrap();
    v["units"][0]["graph"] = serde_json::to_value(calibrated()).unwrap();
    let r: units::Request = serde_json::from_value(v).unwrap();
    let result = units::prepare(&r).unwrap();
    assert!(result.source().values[0] > 0.);
    assert!((result.source().values[1] - 1.).abs() < 1e-12);
    assert!(result.source().native_graphs[0]
        .generated_oas_calibration
        .is_some());
}
#[test]
fn native_generated_graph_prices_paired_crn_and_retains_sealed_physical_recipes() {
    let r = raw();
    let result = graph::analyze(&r).unwrap();
    assert_eq!(result.original_oas[0].base_oas.to_bits(), 0_f64.to_bits());
    assert!(result.shared_draws_retained);
    assert!(!result.physical_accounting_source_retained);
    assert_eq!(result.generated_physical_recipes_retained, Some(true));
    assert_eq!(result.metrics.len(), 2);
    assert!((result.metrics[0].pv - 100.).abs() < 1e-9);
    let source = result
        .native_sources
        .iter()
        .find(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
        .unwrap();
    assert!(source.physical_accounting_source().is_none());
    let generated = source.materialize_generated_physical().unwrap().unwrap();
    assert_eq!(generated.original_cash_quote(), 100.);
    assert_eq!(source.physical_period(0., 15. / 365.).unwrap().1, 0.);
    assert_eq!(result.original_physical_sources.len(), 128);
    assert_eq!(result.exercise_policies.len(), 4);
}
#[test]
fn generated_coefficients_prepare_from_exact_native_book_without_journal_json_owner() {
    let mut v: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-units-1.json"
    ))
    .unwrap();
    v["units"][0]["graph"] = serde_json::to_value(raw()).unwrap();
    let r: units::Request = serde_json::from_value(v).unwrap();
    let result = units::prepare(&r).unwrap();
    assert_eq!(result.source().shape, [1, 1, 10, 1]);
    assert_eq!(result.source().values[0], 0.);
    assert!((result.source().values[1] - 1.).abs() < 1e-12);
    assert_eq!(
        result
            .source()
            .funding
            .cash_events
            .iter()
            .filter(|c| c.path_id == "purchase")
            .count(),
        1
    );
}
#[test]
fn generated_graph_refuses_history_overlap_changed_original_oas_and_unpaired_laws() {
    let mut r = raw();
    r.positions[0]
        .generated_policy
        .as_mut()
        .unwrap()
        .current_pricing[2]
        .evaluation
        .credit_seed += 1;
    assert!(graph::audit_admitted_work(&r).is_err());
    let mut r = raw();
    r.positions[0].original_signed_cash_quote += 1.;
    assert!(graph::analyze(&r).err().unwrap().contains("original Q"));
    let mut r = raw();
    r.positions[0].oas_target=serde_json::from_value(json!({"kind":"calibrate_positive_cash","target_cash_price":100.,"source_id":"quote","source_revision":"1"})).unwrap();
    assert!(graph::audit_admitted_work(&r)
        .err()
        .unwrap()
        .contains("locked"));
}

#[test]
fn generated_current_q_exercise_stops_separate_p_cash_without_refitting_original_acquisition() {
    let mut r = raw();
    let g = r.positions[0].generated_policy.as_mut().unwrap();
    g.pricing_training.policy.exercise_dates[0].call_clean_price_factor = Some(1.01);
    g.original_physical.policy = g.pricing_training.policy.clone();
    for law in g.current_pricing.iter_mut().chain(&mut g.current_physical) {
        law.policy = g.pricing_training.policy.clone();
        law.drivers[0].initial_level = 0.;
        law.drivers[0].long_run_mean = 1.;
        law.drivers[0].mean_reversion = 100.;
    }
    let result = graph::analyze(&r).unwrap();
    assert!(
        (result.metrics[0].pv - 101.).abs() < 1e-9,
        "actual {} first P cash {:?}",
        result.metrics[0].pv,
        result
            .native_sources
            .iter()
            .find(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
            .unwrap()
            .cash
            .iter()
            .map(|c| (c.year, c.principal, c.interest, c.fees))
            .collect::<Vec<_>>()
    );
    assert_eq!(result.original_oas[0].original_signed_cash_quote, 100.);
    for source in result
        .native_sources
        .iter()
        .filter(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
    {
        assert_eq!(
            source.projection["native_stops"].as_array().unwrap().len(),
            1
        );
        assert_eq!(source.source_request.source.acquisition.cost, 100.);
        assert_eq!(
            source
                .source_request
                .source
                .acquisition
                .original_annual_effective_rate,
            0.
        );
        assert!(source.cash.iter().all(|c| c.year <= 15. / 365.));
        assert!((source.cash.iter().map(|c| c.total().unwrap()).sum::<f64>() - 101.).abs() < 1e-9);
    }
}
