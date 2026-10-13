use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_risk_native::{
    derivative_contract::swaps::*, market_indexed_adapter::*, term_enhancements::indexed::*,
};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap()["request"]
        .clone()
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} vs {b}");
}
fn reject(v: Value, text: &str) {
    let error = evaluate(&request(v)).unwrap_err();
    assert!(error.contains(text), "{error}");
}

#[test]
fn actual_native_indexed_and_swap_cash_match_independent_hand() {
    let r = evaluate(&request(raw())).unwrap();
    let c = &r.cases[1];
    let f = [(1. / 0.97 - 1.) / 0.5, (0.97 / 0.94 - 1.) / 0.5];
    let dfs = [0.98, 0.95];
    let mut hand = 0.;
    for i in 0..2 {
        near(c.indexed_result.audit[i].interest, 100. * f[i] * 0.5);
        near(
            c.indexed_result.schedule.events[i].components.cash_interest,
            100. * f[i] * 0.5,
        );
        hand += (100. * f[i] * 0.5 - 2.) * dfs[i];
        assert!(!c.indexed_result.audit[i].calendar_mapping_verified);
    }
    near(c.swap_result.clean_value, hand);
    near(c.spread_adjusted_swap_pv, hand);
    near(
        c.indexed_result
            .schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        100.,
    );
    assert!(c
        .swap_result
        .schedule
        .events
        .iter()
        .all(|e| e.components.principal == 0.));
    assert_eq!(r.cases.len(), 8);
    assert_eq!(r.cache_parent.bridge.original.locked_oas, 0.);
}
#[test]
fn actual_native_conditional_source_dates_reset_calendar_and_measure_retained() {
    let r = evaluate(&request(raw())).unwrap();
    for c in &r.cases {
        for (p, raw) in c
            .swap_request
            .floating_periods
            .iter()
            .zip(&r.cache_parent.bridge.contract.periods)
        {
            if let Coupon::Indexed {
                index,
                calendar,
                reset_cutoff_unix_seconds,
                observations,
                ..
            } = &p.coupon
            {
                assert_eq!(index.index_id, r.cache_parent.bridge.binding.index_id);
                assert_eq!(index.tenor_months, Some(6));
                assert_eq!(calendar.as_ref(), &r.cache_parent.calendar);
                assert_eq!(
                    *reset_cutoff_unix_seconds,
                    Some(raw.reset_cutoff_unix_seconds)
                );
                let o = &observations[0];
                assert_eq!(o.fixing_ordinal, raw.reset_ordinal);
                assert_eq!(o.observation_start_ordinal, raw.start_ordinal);
                assert_eq!(
                    o.selection_cutoff_unix_seconds,
                    raw.reset_cutoff_unix_seconds
                );
                match &o.source {
                    RateSource::ConditionalProjection {
                        model,
                        path,
                        information_unix_seconds,
                        measure,
                    } => {
                        assert_eq!(model, &r.cache_parent.projection_model);
                        assert_eq!(path, &c.swap_request.rate_path);
                        assert_eq!(
                            *information_unix_seconds,
                            c.swap_request.valuation_cutoff_unix_seconds
                        );
                        assert_eq!(*measure, CashflowMeasure::Pricing);
                    }
                    _ => panic!("future observed fixing substituted"),
                }
            } else {
                panic!("floating proxy substituted");
            }
        }
    }
}
#[test]
fn original_oas_overlay_is_separate_from_collateral_clean_discount() {
    let mut v = raw();
    v["bridge"]["original"]["locked_oas"] = json!(0.037);
    let r = evaluate(&request(v.clone())).unwrap();
    let old = r.cases[0].swap_result.clean_value;
    let c = &r.cases[1];
    let mut hand = 0.;
    for e in &c.swap_result.schedule.events {
        let t = (e.payment_ordinal - c.swap_request.asof_ordinal) as f64 / 365.;
        hand += e.components.cash_total().unwrap()
            * c.swap_request.discount_curve.at(e.payment_ordinal).unwrap()
            * (-0.037 * t).exp();
    }
    near(c.spread_adjusted_swap_pv, hand);
    assert_ne!(c.swap_result.clean_value, c.spread_adjusted_swap_pv);
    assert_eq!(c.swap_request.discount_curve.basis, "collateralized_clean");
    v["bridge"]["current_market"]["curves"][1]["discount_factors"][1] = json!(0.96);
    let changed = evaluate(&request(v)).unwrap();
    assert_eq!(
        changed.cases[0].swap_result.clean_value.to_bits(),
        old.to_bits()
    );
    assert_eq!(
        changed.cases[0].original_parent.market.curves[1].discount_factors[1],
        0.97
    );
    assert_eq!(changed.cases[1].original_locked_oas, 0.037);
}
#[test]
fn actual_swap_payer_receiver_par_and_pair_bumps() {
    let payer = evaluate(&request(raw())).unwrap();
    let mut v = raw();
    v["bridge"]["contract"]["direction"] = json!("receiver_fixed");
    let receiver = evaluate(&request(v)).unwrap();
    for (p, r) in payer.cases.iter().zip(&receiver.cases) {
        near(p.swap_result.clean_value, -r.swap_result.clean_value);
    }
    let mut v = raw();
    v["bridge"]["contract"]["fixed_rate"] = json!(payer.cases[1].swap_result.par_rate.unwrap());
    near(
        evaluate(&request(v)).unwrap().cases[1]
            .swap_result
            .clean_value,
        0.,
    );
    assert_eq!(
        payer.cases[2].swap_result.floating_audit[0]
            .interest
            .to_bits(),
        payer.cases[1].swap_result.floating_audit[0]
            .interest
            .to_bits()
    );
    assert_ne!(
        payer.cases[4].swap_result.floating_audit[0]
            .interest
            .to_bits(),
        payer.cases[1].swap_result.floating_audit[0]
            .interest
            .to_bits()
    );
}
#[test]
fn explicit_binding_currency_source_calendar_and_tenor_failures() {
    let mut v = raw();
    v["bridge"]["binding"]["index_id"] = json!("other");
    reject(v, "binding");
    let mut v = raw();
    v["bridge"]["current_market"]["curves"][1]["currency"] = json!("EUR");
    reject(v, "currency");
    let mut v = raw();
    v["calendar"]["id"] = json!("unrelated");
    reject(v, "calendar");
    let mut v = raw();
    v["projection_model"]["revision"] = json!("wrong");
    reject(v, "identity");
    let mut v = raw();
    v["index_tenor_months"] = json!(0);
    reject(v, "tenor");
    let mut v = raw();
    v["original_oas_review"]["id"] = json!("different");
    reject(v, "review");
}
#[test]
fn unavailable_past_reset_measure_and_complete_tail_failures() {
    let mut v = raw();
    v["bridge"]["contract"]["periods"][0]["reset_cutoff_unix_seconds"] = json!(1704092400i64);
    reject(v, "reset cutoff");
    let mut v = raw();
    v["bridge"]["contract"]["periods"][0]["reset_ordinal"] = json!(738885);
    reject(v, "dated period");
    let mut v = raw();
    v["bridge"]["measure"] = json!("physical");
    reject(v, "measure");
    let mut v = raw();
    v["bridge"]["contract"]["periods"]
        .as_array_mut()
        .unwrap()
        .pop();
    reject(v, "maturity");
}
#[test]
fn explicit_payment_lag_tail_points_are_not_inferred_by_swap() {
    let mut v = raw();
    v["bridge"]["contract"]["periods"][1]["payment_ordinal"] = json!(739251);
    reject(v.clone(), "tail");
    for k in ["current_market", "original"] {
        if k == "original" {
            v["bridge"][k]["market"]["curves"][0]["right_tail"] = json!("flat_zero");
        } else {
            v["bridge"][k]["curves"][0]["right_tail"] = json!("flat_zero");
        }
    }
    let r = evaluate(&request(v)).unwrap();
    let c = &r.cases[1];
    assert!(r.bridge_receipt.current.projections[1].collateral_tail_used);
    assert_eq!(c.swap_request.tail_end_ordinal, 739251);
    let mut corrupt = c.swap_request.clone();
    corrupt.discount_curve.points.pop();
    assert!(value_swap(&corrupt)
        .unwrap_err()
        .contains("missing exact payment discount"));
}
#[test]
fn copied_product_rejects_mutated_future_source_path_and_measure() {
    let r = evaluate(&request(raw())).unwrap();
    let p = &r.cases[1].indexed_request;
    for domain in 0..3 {
        let mut altered = p.clone();
        if let Coupon::Indexed { observations, .. } = &mut altered.periods[0].coupon {
            if let RateSource::ConditionalProjection {
                path,
                information_unix_seconds,
                measure,
                ..
            } = &mut observations[0].source
            {
                if domain == 0 {
                    *information_unix_seconds += 86400;
                }
                if domain == 1 {
                    path.id = "foreign".into();
                }
                if domain == 2 {
                    *measure = CashflowMeasure::Physical;
                }
            }
        }
        assert!(build_path(&altered).is_err());
    }
}
#[test]
fn negative_coupon_and_distinct_contractual_daycounts_use_native_product() {
    let mut v = raw();
    v["bridge"]["current_market"]["curves"][1]["discount_factors"] = json!([1., 1.01, 1.02]);
    v["bridge"]["contract"]["fixed_rate"] = json!(-0.02);
    v["bridge"]["contract"]["fixed_day_count"] = json!("actual365_fixed");
    let r = evaluate(&request(v)).unwrap();
    assert!(r.cases[1].indexed_result.audit[0].interest < 0.);
    near(
        r.cases[1].swap_result.fixed_leg_pv,
        100. * (-0.02) * 180. / 365. * (0.98 + 0.95),
    );
}
#[test]
fn cancellation_at_every_adapter_checkpoint_never_returns_output() {
    let r = request(raw());
    let mut calls = 0;
    evaluate_with_check(&r, || {
        calls += 1;
        false
    })
    .unwrap();
    for n in 1..=calls {
        let mut i = 0;
        assert_eq!(
            evaluate_with_check(&r, || {
                i += 1;
                i == n
            })
            .unwrap_err(),
            "COMPUTE_CANCELLED"
        );
    }
}
#[test]
fn joint_adapter_reservation_precedes_product_allocation_and_generation() {
    let mut v = raw();
    let mut periods = Vec::new();
    for n in 0..40 {
        periods.push(
            json!({"period_id":format!("{}-{n}", "escaped\\\"".repeat(10)),
            "start_ordinal":738886+n,"end_ordinal":738887+n,"payment_ordinal":738887+n,
            "reset_ordinal":738886,"reset_cutoff_unix_seconds":1704099600i64}),
        );
    }
    v["bridge"]["contract"]["periods"] = json!(periods);
    v["bridge"]["contract"]["maturity_ordinal"] = json!(738926);
    reject(v, "aggregate retained/work admission");
}
#[test]
fn absent_adapter_retains_manual_native_swap_full_output_bytes() {
    // The manual DTO is frozen independently of generated adapter DTOs. Component-only parity.
    let swap: SwapRequest = serde_json::from_str(include_str!(
        "fixtures/market_indexed/manual-native-swap.json"
    ))
    .unwrap();
    let before = serde_json::to_vec(&value_swap(&swap).unwrap()).unwrap();
    evaluate(&request(raw())).unwrap();
    let after = serde_json::to_vec(&value_swap(&swap).unwrap()).unwrap();
    assert_eq!(before, after);
}
