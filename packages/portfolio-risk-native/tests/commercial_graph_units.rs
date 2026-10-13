use portfolio_risk_native::{
    commercial_credit_compiler as compiler, commercial_graph as graph, commercial_units as units,
    product_strategy::Allocation,
};
use serde_json::json;
fn owner() -> compiler::Request {
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-credit-ledger-1.json"
    ))
    .unwrap();
    serde_json::from_value(raw["commercial"][0].clone()).unwrap()
}
fn scene(id: &str) -> portfolio_risk_native::advanced_graph::Scenario {
    serde_json::from_value(json!({"scenario_id":id,"run_id":format!("{id}-run"),"severity":0.,"collateral_curve":{"curve_id":"discount","source_id":"hand-flat","source_revision":"1","currency":"USD","role":{"kind":"collateral","collateral_index":"SOFR"},"times_years":[0.,1.],"discount_factors":[1.,1.],"right_tail":"reject"}})).unwrap()
}
fn indexed_coupon(source: &mut compiler::Request, original: bool, rate: f64) {
    let path = if original {
        &mut source.source.original
    } else {
        &mut source.source.current
    };
    let p = &mut path.periods[0];
    p.coupon=serde_json::from_value(json!({"kind":"indexed","index":{"index_id":"supplied-term","kind":"term","tenor_months":1,"average_calendar_days":null},"calendar":{"id":"supplied-calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"term_simple","margin_treatment":"simple","margin":0.,"index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":1704067200_i64,"lookback_business_days":0,"observation_shift":false,"observations":[{"interest_start_ordinal":p.start_ordinal,"interest_end_ordinal":p.end_ordinal,"observation_start_ordinal":p.start_ordinal,"observation_end_ordinal":p.end_ordinal,"fixing_ordinal":p.start_ordinal,"selection_cutoff_unix_seconds":1704067200_i64,"rate":rate,"source":{"kind":"conditional_projection","model":{"id":"supplied-projection","revision":"1"},"path":path.path,"information_unix_seconds":1704067200_i64,"measure":path.measure}}]})).unwrap();
}
fn history(physical: bool) -> graph::History {
    let mut source = owner();
    if !physical {
        source.source.original.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
        source.source.current.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
    }
    graph::History {
        path_id: source.credit.paths[0].path_id.clone(),
        weight: 1.,
        source,
    }
}
fn request() -> graph::Request {
    let p = history(true);
    let mut q = history(false);
    q.source.source.current.scenario_id = "baseline".into();
    let mut original = q.clone();
    original.source.source.current = original.source.source.original.clone();
    graph::Request{schema:graph::SCHEMA.into(),identity:serde_json::from_value(json!({"model_revision":"commercial-credit-1","input_revision":"1","paired_history_source":"hand","paired_history_revision":"1","randomness_domain":"shared-crn"})).unwrap(),
        as_of_ordinal:738886,currency:"USD".into(),reporting_ordinals:vec![738886,738901],base_scenario_id:"baseline".into(),bump_bp:1.,original_scenario:scene("original"),scenarios:vec![scene("baseline")],positions:vec![graph::Position{position_id:"commercial".into(),credit_identity:graph::CreditIdentity::CommercialLending,
        oas_target:serde_json::from_value(json!({"kind":"locked","base_oas":0.,"source_id":"quote","source_revision":"1"})).unwrap(),original_signed_cash_quote:p.source.source.acquisition.cost,
        original_pricing:vec![original],original_physical:Vec::new(),histories:vec![graph::Histories{scenario_id:"baseline".into(),physical:vec![p],pricing:vec![q.clone()],pricing_down:vec![q.clone()],pricing_up:vec![q]}],exercise:None,generated_policy:None}]}
}
#[test]
fn credit_off_exact_source_cash_and_hand_dv01() {
    let r = request();
    let out = graph::analyze(&r).unwrap();
    let price = r.positions[0].original_signed_cash_quote;
    assert!((out.metrics[0].pv - price).abs() < 1e-10);
    let t = 35. / 365.;
    let expected = price * ((1e-4_f64 * t).exp() - (-1e-4_f64 * t).exp()) / 2.;
    assert!((out.metrics[0].dv01 - expected).abs() < 1e-10);
    for source in &out.native_sources {
        let total: f64 = source.cash.iter().map(|c| c.total().unwrap()).sum();
        assert!((total - price).abs() < 1e-10);
        assert_eq!(source.cash.len(), 1);
    }
    assert!(out.shared_draws_retained);
    assert!(!out.empirical_validation);
}
#[test]
fn original_oas_does_not_refit_in_changed_current_market() {
    let mut r = request();
    r.scenarios[0].collateral_curve.discount_factors[1] = (-0.05_f64).exp();
    let out = graph::analyze(&r).unwrap();
    assert_eq!(out.original_oas[0].base_oas.to_bits(), 0f64.to_bits());
    assert!(
        (out.metrics[0].pv
            - r.positions[0].original_signed_cash_quote * (-0.05_f64 * 35. / 365.).exp())
        .abs()
            < 1e-10
    );
}

#[test]
fn supplied_floating_response_can_have_negative_dv01_without_sign_clamping() {
    let mut r = request();
    for h in &mut r.positions[0].original_pricing {
        indexed_coupon(&mut h.source, true, 0.1);
        indexed_coupon(&mut h.source, false, 0.1);
    }
    let hs = &mut r.positions[0].histories[0];
    for h in hs
        .physical
        .iter_mut()
        .chain(&mut hs.pricing)
        .chain(&mut hs.pricing_down)
        .chain(&mut hs.pricing_up)
    {
        indexed_coupon(&mut h.source, true, 0.1);
        indexed_coupon(&mut h.source, false, 0.1);
    }
    // Explicit supplied index beta: a 1bp discount bump changes the index by
    // 2bp. Contract terms and the original acquisition source remain fixed.
    indexed_coupon(&mut hs.pricing_down[0].source, false, 0.1 - 0.0002);
    indexed_coupon(&mut hs.pricing_up[0].source, false, 0.1 + 0.0002);
    let out = graph::analyze(&r).unwrap();
    let accrual = 30. / 365.;
    let payment = 35. / 365.;
    let down = (100. + 100. * (0.1 - 0.0002) * accrual) * (1e-4_f64 * payment).exp();
    let up = (100. + 100. * (0.1 + 0.0002) * accrual) * (-1e-4_f64 * payment).exp();
    let expected = (down - up) / 2.;
    assert!(expected < 0.);
    assert!((out.metrics[0].dv01 - expected).abs() < 1e-10);
    assert_eq!(out.original_oas[0].base_oas.to_bits(), 0_f64.to_bits());
}

#[test]
fn option_free_leap_year_stubs_retain_actual_principal_payment_dates() {
    let mut r = request();
    r.as_of_ordinal = 738944;
    r.reporting_ordinals = vec![738944, 738945, 738948];
    let quote = 100. + 0.1 * (100. + 2. * 40.) / 366.;
    r.positions[0].original_signed_cash_quote = quote;
    let edit = |source: &mut compiler::Request| {
        for p in [&mut source.source.original, &mut source.source.current] {
            p.asof_ordinal = 738944;
            p.reporting_end_ordinal = 738948;
            p.tail_end_ordinal = 738948;
            let mut second = p.periods[0].clone();
            p.periods[0].start_ordinal = 738944;
            p.periods[0].end_ordinal = 738945;
            p.periods[0].payment_ordinal = 738945;
            p.periods[0].day_count =
                portfolio_model_core::cashflow::ActualDayCount::ActActIsda;
            second.period_id = "stub-two".into();
            second.start_ordinal = 738945;
            second.end_ordinal = 738947;
            second.payment_ordinal = 738948;
            second.day_count = portfolio_model_core::cashflow::ActualDayCount::ActActIsda;
            p.periods.push(second);
            p.principal_plan =
                serde_json::from_value(json!({"kind":"supplied","principal_payments":[60.,40.]}))
                    .unwrap();
        }
        source.source.acquisition.cost = quote;
        source.credit.intervals[0].end_year = 4. / 365.;
    };
    for h in &mut r.positions[0].original_pricing {
        edit(&mut h.source);
    }
    for hs in &mut r.positions[0].histories {
        for h in hs
            .physical
            .iter_mut()
            .chain(&mut hs.pricing)
            .chain(&mut hs.pricing_down)
            .chain(&mut hs.pricing_up)
        {
            edit(&mut h.source);
        }
    }
    let out = graph::analyze(&r).unwrap();
    assert!((out.metrics[0].pv - quote).abs() < 1e-10);
    for s in &out.native_sources {
        assert_eq!(s.cash.len(), 2);
        assert_eq!(s.cash[0].principal, 60.);
        assert_eq!(s.cash[1].principal, 40.);
        assert!((s.cash[0].interest - 100. * 0.1 / 366.).abs() < 1e-10);
        assert!((s.cash[1].interest - 40. * 0.1 * 2. / 366.).abs() < 1e-10);
        assert_eq!(
            units::settlement_ordinal(r.as_of_ordinal, s.cash[0].year).unwrap(),
            738945
        );
        assert_eq!(
            units::settlement_ordinal(r.as_of_ordinal, s.cash[1].year).unwrap(),
            738948
        );
    }
}
#[test]
fn credit_enabled_recovery_has_no_fake_default_principal_cash() {
    let mut r = request();
    let wait = (-100_f64 * 2. / 365.).exp();
    let stream:portfolio_model_core::credit_path::DrawStream=serde_json::from_value(json!({"kind":"supplied","stream_id":"exact","stream_revision":"1","paths":[{"path_key":17,"pairs":[{"waiting_uniform":wait,"destination_uniform":0.5}]}]})).unwrap();
    r.positions[0].original_pricing[0].source.credit.draw_stream = stream.clone();
    let h = &mut r.positions[0].histories[0];
    for histories in [
        &mut h.physical,
        &mut h.pricing,
        &mut h.pricing_down,
        &mut h.pricing_up,
    ] {
        for p in histories {
            p.source.credit.draw_stream = stream.clone();
            p.source.credit.intervals[0].hazards_per_year[0][3] = 100.;
            p.source.credit.intervals[0].loss_given_default = [0.4; 3];
            for tail in &mut p.source.credit.intervals[0].complete_recovery_tails {
                tail[0].lag_years = 40. / 365.;
            }
        }
    }
    let out = graph::analyze(&r).unwrap();
    let expected_interest = 100. * 0.1 * 2. / 365. * 0.5;
    for source in &out.native_sources {
        assert_eq!(source.cash.len(), 1);
        assert_eq!(source.cash[0].kind, "recovery");
        assert!((source.cash[0].principal - 60.).abs() < 1e-10);
        assert!((source.cash[0].interest - expected_interest).abs() < 1e-10);
        assert!((source.cash[0].year - 42. / 365.).abs() < 1e-12);
    }
    assert!((out.metrics[0].pv - (60. + expected_interest)).abs() < 1e-10);
    let physical = out
        .native_sources
        .iter()
        .find(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
        .unwrap();
    let default = physical.projection["carry"]["credit"]["result"]["paths"][0]["default"].clone();
    assert!((default["economic_principal_loss"].as_f64().unwrap() - 40.).abs() < 1e-10);
}
#[test]
fn reject_changed_crn_original_anchor_and_credit_free_credit() {
    let mut r = request();
    r.positions[0].histories[0].pricing_up[0]
        .source
        .credit
        .paths[0]
        .path_key = 18;
    assert!(graph::analyze(&r).err().unwrap().contains("shared draw"));
    let mut r = request();
    r.positions[0].histories[0].pricing_up[0]
        .source
        .source
        .acquisition
        .cost += 1.;
    assert!(graph::analyze(&r).err().unwrap().contains("bitwise"));
    let mut r = request();
    r.positions[0].credit_identity = graph::CreditIdentity::CreditFreeReference;
    r.positions[0].original_pricing[0]
        .source
        .credit
        .instrument_kind = compiler::InstrumentKind::CreditFreeReference;
    let h = &mut r.positions[0].histories[0];
    for hs in [
        &mut h.physical,
        &mut h.pricing,
        &mut h.pricing_down,
        &mut h.pricing_up,
    ] {
        for x in hs {
            x.source.credit.instrument_kind = compiler::InstrumentKind::CreditFreeReference;
        }
    }
    r.positions[0].histories[0].pricing[0]
        .source
        .credit
        .intervals[0]
        .hazards_per_year[0][3] = 1.;
    assert!(graph::analyze(&r).err().unwrap().contains("credit-free"));
}
fn unit_request() -> units::Request {
    serde_json::from_value(json!({"schema":units::SCHEMA,"identity":{"model_revision":"commercial-units-1","grid_revision":"actual","crn_id":"shared-crn","input_revision":"1"},"currency":"USD","reporting_ordinals":[738886,738901],"base_scenario_id":"baseline","units":[{"unit_id":"commercial","template_id":"commercial","unit_face":100.,"accounting_category":"HTM","capital_policy_revision":"explicit-zero","weights":{"hqla_l2a":0.,"outflow30":0.,"asf":0.,"rsf":0.,"rwa":0.},"graph":request()}]})).unwrap()
}
#[test]
fn coefficients_and_retained_source_cash_reconcile_and_zero_allocation_preserves_tail() {
    let r = unit_request();
    let p = units::prepare(&r).unwrap();
    assert_eq!(p.source().shape, [1, 1, 10, 1]);
    assert_eq!(p.source().values[0], 0.);
    assert_eq!(p.source().values[1], 1.);
    let full = units::evaluate(
        &p,
        &[Allocation {
            unit_id: "commercial".into(),
            face: 100.,
        }],
    )
    .unwrap();
    assert!(
        (full["scenarios"][0]["cash"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["cumulative_bank_cash"]
            .as_f64()
            .unwrap())
        .abs()
            < 1e-9
    );
    let zero = units::evaluate(
        &p,
        &[Allocation {
            unit_id: "commercial".into(),
            face: 0.,
        }],
    )
    .unwrap();
    assert_eq!(zero["total_asset_face"], 0.);
    assert!(!p.source().native_graphs[0].native_sources.is_empty());
    assert!(p.source().funding.ordinals.contains(&738921));
    assert!(units::evaluate(
        &p,
        &[Allocation {
            unit_id: "wrong".into(),
            face: 0.
        }]
    )
    .is_err());
}
#[test]
fn actual_cash_days_do_not_drift_on_native_day_bits() {
    for day in 0..366 {
        assert_eq!(
            units::settlement_ordinal(738886, f64::from(day) / 365.).unwrap(),
            738886 + day
        );
    }
    assert_eq!(
        units::settlement_ordinal(738886, 1.5 / 365.).unwrap(),
        738888
    );
}
#[test]
fn full_original_oas_calibration_preserves_price_and_curve_scenario_risk() {
    let mut r = request();
    let target = 95.;
    r.positions[0].original_signed_cash_quote = target;
    r.positions[0].oas_target=serde_json::from_value(json!({"kind":"calibrate_positive_cash","target_cash_price":target,"source_id":"original-only","source_revision":"1"})).unwrap();
    let out = graph::analyze(&r).unwrap();
    let expected = (r.positions[0].original_pricing[0]
        .source
        .source
        .acquisition
        .cost
        / target)
        .ln()
        / (35. / 365.);
    assert!((out.original_oas[0].base_oas - expected).abs() < 1e-10);
    assert!((out.metrics[0].pv - target).abs() < 1e-9);
}
#[test]
fn continuation_uses_owned_credit_cash_and_stops_physical_accounting_once() {
    let mut r = request();
    r.positions[0].exercise=Some(serde_json::from_value(json!({"schema":"commercial-native-exercise-1","identity":{"id":"call","revision":"1"},"information_policy":"native_full_observed_prefix-1","exercise_dates":[{"ordinal":738896,"timing":"after_scheduled_cash","call_clean_price_factor":1.,"put_clean_price_factor":null,"call_fee":0.,"put_fee":0.}],"simultaneous_priority":"issuer"})).unwrap());
    r.positions[0].original_signed_cash_quote = 100. + 100. * 0.1 * 10. / 365.;
    let out = graph::analyze(&r).unwrap();
    assert_eq!(out.exercise_policies.len(), 4);
    assert!((out.metrics[0].pv - r.positions[0].original_signed_cash_quote).abs() < 1e-9);
    for s in &out.native_sources {
        assert_eq!(s.cash.len(), 1);
        assert!((s.cash[0].year - 10. / 365.).abs() < 1e-12);
        assert!((s.cash[0].principal - 100.).abs() < 1e-10);
        assert!((s.cash[0].interest - 100. * 0.1 * 10. / 365.).abs() < 1e-10);
    }
    let physical = out
        .native_sources
        .iter()
        .find(|s| s.role == portfolio_risk_native::advanced_graph::Role::Physical)
        .unwrap();
    assert_eq!(
        physical.projection["native_stops"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        physical.projection["accounting"]["closing"][0]["principal"]
            .as_f64()
            .unwrap()
            .abs()
            < 1e-12
    );
    assert!(
        physical.projection["accounting"]["closing"][0]["legal_interest"]
            .as_f64()
            .unwrap()
            .abs()
            < 1e-12
    );
    let mut units = unit_request();
    units.units[0].graph = r;
    let error = units::prepare(&units).err().unwrap();
    assert!(error.contains("original stopped Physical acquisition"));
}

#[test]
fn acquired_exercise_uses_original_stopped_physical_cost_and_yield() {
    let mut u = unit_request();
    let r = &mut u.units[0].graph;
    let rate: f64 = 0.03;
    let cash = 100. + 100. * 0.1 * 10. / 365.;
    let cost = cash * (-rate.ln_1p() * 10. / 365.).exp();
    r.positions[0].exercise=Some(serde_json::from_value(json!({"schema":"commercial-native-exercise-1","identity":{"id":"call","revision":"1"},"information_policy":"native_full_observed_prefix-1","exercise_dates":[{"ordinal":738896,"timing":"after_scheduled_cash","call_clean_price_factor":1.,"put_clean_price_factor":null,"call_fee":0.,"put_fee":0.}],"simultaneous_priority":"issuer"})).unwrap());
    r.positions[0].original_signed_cash_quote = cost;
    r.positions[0].oas_target=serde_json::from_value(json!({"kind":"locked","base_oas":rate.ln_1p(),"source_id":"original-only","source_revision":"1"})).unwrap();
    for h in &mut r.positions[0].original_pricing {
        h.source.source.acquisition.cost = cost;
        h.source.source.acquisition.original_annual_effective_rate = rate;
        for x in &mut h.source.accounting_rates {
            x.annual_effective_rates = [rate; 3];
        }
    }
    for hs in &mut r.positions[0].histories {
        for h in hs
            .physical
            .iter_mut()
            .chain(&mut hs.pricing)
            .chain(&mut hs.pricing_down)
            .chain(&mut hs.pricing_up)
        {
            h.source.source.acquisition.cost = cost;
            h.source.source.acquisition.original_annual_effective_rate = rate;
            for x in &mut h.source.accounting_rates {
                x.annual_effective_rates = [rate; 3];
            }
        }
    }
    r.positions[0].original_physical = r.positions[0].histories[0].physical.clone();
    for h in &mut r.positions[0].original_physical {
        h.source.source.current = h.source.source.original.clone();
    }
    let p = units::prepare(&u).unwrap();
    let g = &p.source().native_graphs[0];
    assert_eq!(g.original_oas[0].base_oas.to_bits(), rate.ln_1p().to_bits());
    assert_eq!(g.original_physical_sources.len(), 1);
    assert_eq!(g.original_acquisition_proofs.len(), 1);
    assert!((p.source().values[0] - (cash - cost) / 100.).abs() < 1e-10);
    assert!(!p.source().funding.ordinals.contains(&738921));
    assert!(p.source().funding.ordinals.contains(&738896));
    let mut bad = u.clone();
    bad.units[0].graph.positions[0].original_physical[0]
        .source
        .source
        .acquisition
        .cost += 1.;
    assert!(units::prepare(&bad).is_err());
    let mut bad = u;
    bad.units[0].graph.positions[0].histories[0].physical[0]
        .source
        .source
        .acquisition
        .original_annual_effective_rate += 0.01;
    assert!(units::prepare(&bad).is_err());
}
