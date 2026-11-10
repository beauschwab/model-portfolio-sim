use portfolio_risk_native::commercial_credit_compiler as owner;
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-secondary-acquisition-1.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}")
}
// A real principal payment inside the continuing coupon precedes acquisition.
fn split_entitlement() -> Value {
    let mut x = raw();
    let tail = 738946;
    let future = json!({"event_id":"principal-final","payment_ordinal":tail,"amount":80.});
    x["source"]["transfer"]["contract_origin"]["principal_plan"] = json!({"kind":"dated_supplied_period_opening_nominal-1","principal_events":[{"event_id":"principal-before-buyer","payment_ordinal":738871,"amount":20.},future.clone()]});
    for role in ["original", "current"] {
        x["source"][role]["opening_principal"] = json!(80.);
        x["source"][role]["principal_plan"] = json!({"kind":"dated_supplied_period_opening_nominal-1","principal_events":[future.clone()]});
    }
    x["source"]["transfer"]["loan_clean_cost"] = json!(78.);
    x["source"]["acquisition"]["cost"] = json!(79.);
    let y = ((83_f64 / 79.).ln() * 365. / 60.).exp_m1();
    x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    x
}
#[test]
fn dated_principal_retains_coupon_entitlement_one_hundred_live_eighty() {
    let r: owner::Request = serde_json::from_value(split_entitlement()).unwrap();
    let out = owner::evaluate(&r).unwrap();
    let opening = &out.accounting().groups()[0].next;
    near(opening.principal, 80.);
    near(opening.recognized_interest, 1.);
    near(opening.legal_interest, 1.);
    near(opening.gross_carry, 79.);
    near(
        out.accounting()
            .groups()
            .iter()
            .map(|g| g.effective_interest)
            .sum(),
        4.,
    );
    near(
        out.accounting()
            .groups()
            .iter()
            .skip(1)
            .map(|g| g.cash)
            .sum(),
        83.,
    );
    let receipt = serde_json::to_value(out).unwrap();
    let transfer = &receipt["secondary_acquisition"];
    near(transfer["principal"].as_f64().unwrap(), 80.);
    near(
        transfer["coupons"][0]["opening_entitlement_principal"]
            .as_f64()
            .unwrap(),
        100.,
    );
    let events = transfer["full_contractual_source"]["schedule"]["events"]
        .as_array()
        .unwrap();
    near(
        events
            .iter()
            .map(|e| e["components"]["principal"].as_f64().unwrap())
            .sum(),
        100.,
    );
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["event_id"], "principal-before-buyer");
    assert_eq!(events[1]["event_id"], "coupon");
    assert_eq!(events[2]["event_id"], "principal-final");
}

#[test]
fn dated_principal_is_real_cash_at_original_dates_and_coupon_nominal_is_period_opening() {
    use portfolio_risk_native::term_enhancements::indexed;
    let mut x = split_entitlement();
    let origin = &mut x["source"]["transfer"]["contract_origin"];
    let mut next = origin["periods"][0].clone();
    next["period_id"] = json!("next-coupon");
    next["start_ordinal"] = json!(738946);
    next["end_ordinal"] = json!(738976);
    next["payment_ordinal"] = json!(738976);
    origin["periods"].as_array_mut().unwrap().push(next);
    origin["tail_end_ordinal"] = json!(738976);
    origin["principal_plan"]["principal_events"][1]["payment_ordinal"] = json!(738976);
    let out = indexed::build_path(&serde_json::from_value(origin.clone()).unwrap()).unwrap();
    near(out.audit[0].opening_principal, 100.);
    near(out.audit[0].interest, 3.);
    near(out.audit[1].opening_principal, 80.);
    near(out.audit[1].interest, 0.8);
    assert_eq!(out.schedule.events.len(), 4);
    near(out.principal_after_reporting, 80.);
    assert!(out.schedule.events[0].accrual_period.is_none());
}
#[test]
fn principal_cash_on_acquisition_day_follows_single_buyer_acquisition() {
    let mut x = split_entitlement();
    let day = json!({"event_id":"principal-before-buyer","payment_ordinal":738886,"amount":20.});
    x["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_events"][0] =
        day.clone();
    for role in ["original", "current"] {
        x["source"][role]["opening_principal"] = json!(100.);
        x["source"][role]["principal_plan"]["principal_events"]
            .as_array_mut()
            .unwrap()
            .insert(0, day.clone());
    }
    x["source"]["transfer"]["loan_clean_cost"] = json!(98.);
    x["source"]["acquisition"]["cost"] = json!(99.);
    let out = owner::evaluate(&serde_json::from_value(x).unwrap()).unwrap();
    let groups = out.accounting().groups();
    near(groups[0].next.principal, 100.);
    let paid = groups
        .iter()
        .find(|g| g.source_event_id.as_deref() == Some("principal-before-buyer"))
        .unwrap();
    near(paid.year, 0.);
    near(paid.next.principal, 80.);
    near(paid.cash, 20.);
    near(groups.iter().skip(1).map(|g| g.cash).sum(), 103.);
    assert_eq!(groups.iter().filter(|g| g.cash < 0.).count(), 1);
}
#[test]
fn principal_event_origin_suffix_identity_and_full_cash_conservation_fail_closed() {
    for mutation in [
        "origin_missing",
        "buyer_missing",
        "amount",
        "date",
        "identity",
        "collision",
        "extra",
        "entitlement",
    ] {
        let mut x = split_entitlement();
        match mutation {
            "origin_missing" => {
                x["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_events"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            "buyer_missing" => {
                x["source"]["current"]["principal_plan"]["principal_events"] = json!([])
            }
            "amount" => {
                x["source"]["current"]["principal_plan"]["principal_events"][0]["amount"] =
                    json!(81.)
            }
            "date" => {
                x["source"]["current"]["principal_plan"]["principal_events"][0]["payment_ordinal"] =
                    json!(738945)
            }
            "identity" => {
                x["source"]["current"]["principal_plan"]["principal_events"][0]["event_id"] =
                    json!("other")
            }
            "collision" => {
                x["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_events"]
                    [0]["event_id"] = json!("coupon")
            }
            "extra" => x["source"]["current"]["principal_plan"]["principal_events"]
                .as_array_mut()
                .unwrap()
                .push(json!({"event_id":"extra","payment_ordinal":738946,"amount":1.})),
            "entitlement" => {
                x["source"]["current"]["principal_plan"]["opening_entitlement_principal"] =
                    json!(100.)
            }
            _ => unreachable!(),
        }
        let result = serde_json::from_value::<owner::Request>(x)
            .map_err(|e| e.to_string())
            .and_then(|r| owner::evaluate(&r).map(|_| ()));
        assert!(result.is_err(), "{mutation}");
    }
}
#[test]
fn default_uses_live_eighty_and_separately_owned_purchased_nominal_one_hundred() {
    let mut x = split_entitlement();
    x["credit"]["intervals"][0]["hazards_per_year"][0][3] = json!(100.);
    let out = owner::evaluate(&serde_json::from_value(x).unwrap()).unwrap();
    let receipt = serde_json::to_value(&out).unwrap();
    near(
        receipt["secondary_acquisition"]["coupons"][0]["opening_entitlement_principal"]
            .as_f64()
            .unwrap(),
        100.,
    );
    let recovered = out
        .carry()
        .native_coupon_recoveries()
        .iter()
        .filter(|r| r.earning_origin_index == 3)
        .map(|r| r.interest)
        .sum();
    near(recovered, 0.5);
    assert!(out.accounting().closing()[0]
        .lots
        .iter()
        .any(|l| l.earning_origin_index == 3 && l.default_claim));
}
#[test]
fn dated_principal_enum_uses_existing_inline_vec_capacity() {
    use portfolio_risk_native::term_enhancements::indexed::PrincipalPlan;
    assert_eq!(std::mem::size_of::<PrincipalPlan>(), 32);
}

#[test]
fn negative_coupon_nominal_retains_gross_payable_separate_from_live_eighty() {
    let mut x = split_entitlement();
    for role in ["original", "current"] {
        x["source"][role]["periods"][0]["coupon"]["annual_rate"] = json!(-0.12);
    }
    x["source"]["transfer"]["contract_origin"]["periods"][0]["coupon"]["annual_rate"] =
        json!(-0.12);
    x["source"]["transfer"]["loan_clean_cost"] = json!(80.);
    x["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(-1.);
    let y = ((77_f64 / 79.).ln() * 365. / 60.).exp_m1();
    x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    let out = owner::evaluate(&serde_json::from_value(x).unwrap()).unwrap();
    let g = out.accounting().groups();
    near(g[0].next.principal, 80.);
    near(g[0].next.legal_interest, -1.);
    near(g.iter().skip(1).map(|g| g.cash).sum(), 77.);
    near(g.iter().map(|g| g.effective_interest).sum(), -2.);
    near(out.accounting().closing()[0].legal_interest, 0.);
}
#[test]
fn coupon_nominal_survives_principal_settlement_with_actual_later_coupon_cash() {
    let mut x = split_entitlement();
    let early = 738916;
    for role in ["original", "current"] {
        x["source"][role]["principal_plan"]["principal_events"][0]["payment_ordinal"] =
            json!(early);
    }
    x["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_events"][1]
        ["payment_ordinal"] = json!(early);
    let mut lo = -0.98_f64;
    let mut hi = 9.9_f64;
    for _ in 0..100 {
        let m = (lo + hi) * 0.5;
        let quote = 80. * (1. + m).powf(-30. / 365.) + 3. * (1. + m).powf(-60. / 365.);
        if quote > 79. {
            lo = m
        } else {
            hi = m
        }
    }
    let y = (lo + hi) * 0.5;
    x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    let out = owner::evaluate(&serde_json::from_value(x).unwrap()).unwrap();
    let paid = out
        .accounting()
        .groups()
        .iter()
        .find(|g| g.source_event_id.as_deref() == Some("principal-final"))
        .unwrap();
    near(paid.next.principal, 0.);
    let last = out
        .accounting()
        .groups()
        .iter()
        .find(|g| g.source_event_id.as_deref() == Some("coupon"))
        .unwrap();
    near(last.cash, 3.);
    near(last.next.legal_interest, 0.);
    near(
        out.accounting()
            .groups()
            .iter()
            .map(|g| g.effective_interest)
            .sum(),
        4.,
    );
}

#[test]
fn actual_conditional_call_settles_live_principal_and_nominal_one_hundred_coupon() {
    use portfolio_model_core::credit::CreditMeasure;
    use portfolio_risk_native::products::commercial::exercise;
    let physical: owner::Request = serde_json::from_value(split_entitlement()).unwrap();
    let mut q = split_entitlement();
    for role in ["original", "current"] {
        q["source"][role]["measure"] = json!("pricing");
    }
    q["source"]["transfer"]["contract_origin"]["measure"] = json!("pricing");
    let pricing: owner::Request = serde_json::from_value(q).unwrap();
    let base = owner::evaluate_economic_with_measure(&pricing, CreditMeasure::Pricing).unwrap();
    let policy=serde_json::from_value(json!({"schema":"commercial-native-exercise-1","identity":{"id":"reviewed-call","revision":"1"},"information_policy":"native_full_observed_prefix-1","exercise_dates":[{"ordinal":738901,"timing":"after_scheduled_cash","call_clean_price_factor":1.,"put_clean_price_factor":null,"call_fee":0.,"put_fee":0.}],"simultaneous_priority":"issuer"})).unwrap();
    let curve=serde_json::from_value(json!({"curve_id":"flat","source_id":"reviewed","source_revision":"1","currency":"USD","role":{"kind":"collateral","collateral_index":"cash"},"times_years":[0.,1.],"discount_factors":[1.,1.],"right_tail":"reject"})).unwrap();
    let prepared = exercise::compile(
        &policy,
        &[exercise::PricingView {
            raw: &pricing,
            carry: base.carry(),
            weight: 1.,
        }],
        &curve,
        0.,
        0.,
    )
    .unwrap();
    near(prepared.value, 81.5);
    let stops = prepared.stops_for(&pricing, base.carry()).unwrap();
    assert_eq!(stops.len(), 1);
    let stopped = owner::evaluate_with_stops(&physical, &stops).unwrap();
    near(
        stopped
            .accounting()
            .groups()
            .iter()
            .skip(1)
            .map(|g| g.cash)
            .sum(),
        81.5,
    );
    let closing = &stopped.accounting().closing()[0];
    near(closing.principal, 0.);
    near(closing.legal_interest, 0.);
    near(closing.recognized_interest, 0.);
}

#[test]
fn indexed_coupon_keeps_native_compounded_entitlement_after_real_principal_payment() {
    for treatment in ["simple", "compounded"] {
        let mut x = split_entitlement();
        let at = 738886_i32;
        let midnight = |d: i32| (i64::from(d) - 719163) * 86400;
        let path = x["source"]["original"]["path"].clone();
        let rates = [0.04, 0.045, 0.05, 0.06];
        let observations:Vec<Value>=(at-2..at+2).zip(rates).map(|(day,rate)|{
            let publication=midnight(day+1);
            let source=if day<at{json!({"kind":"observed","fixing":{"id":format!("historical-{day}"),"revision":"1"},"publication_unix_seconds":publication})}else{json!({"kind":"conditional_published_projection","model":{"id":"future","revision":"1"},"path":path,"measure":"physical","state_information_unix_seconds":midnight(day),"publication_unix_seconds":publication})};
            json!({"interest_start_ordinal":day,"interest_end_ordinal":day+1,"observation_start_ordinal":day,"observation_end_ordinal":day+1,"fixing_ordinal":day,"selection_cutoff_unix_seconds":publication,"rate":rate,"source":source})
        }).collect();
        let coupon = json!({"kind":"indexed","index":{"index_id":"overnight","kind":"overnight","tenor_months":null,"average_calendar_days":null},"calendar":{"id":"calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"daily_compounded","margin_treatment":treatment,"margin":0.01,"index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":null,"lookback_business_days":0,"observation_shift":false,"observations":observations});
        for role in ["original", "current"] {
            let source = &mut x["source"][role];
            source["reporting_end_ordinal"] = json!(at + 1);
            source["tail_end_ordinal"] = json!(at + 2);
            source["observed_publication_cutoff_unix_seconds"] = json!(midnight(at));
            source["principal_plan"]["principal_events"][0]["payment_ordinal"] = json!(at + 2);
            source["periods"][0]["start_ordinal"] = json!(at - 2);
            source["periods"][0]["end_ordinal"] = json!(at + 2);
            source["periods"][0]["payment_ordinal"] = json!(at + 2);
            source["periods"][0]["day_count"] = json!("act365_fixed");
            source["periods"][0]["coupon"] = coupon.clone();
        }
        let mut origin = x["source"]["original"].clone();
        origin["asof_ordinal"] = json!(at - 2);
        origin["opening_principal"] = json!(100.);
        origin["principal_plan"]["principal_events"]
            .as_array_mut()
            .unwrap()
            .insert(
                0,
                json!({"event_id":"past20","payment_ordinal":at-1,"amount":20.}),
            );
        x["source"]["transfer"]["contract_origin"] = origin;
        let factor = |count: usize| {
            let margin = if treatment == "compounded" { 0.01 } else { 0. };
            rates[..count]
                .iter()
                .fold(1., |p, r| p * (1. + (r + margin) / 365.))
                - 1.
                + if treatment == "simple" {
                    0.01 * count as f64 / 365.
                } else {
                    0.
                }
        };
        let accrued = 100. * factor(2);
        let coupon_cash = 100. * factor(4);
        let cost = 80. + accrued;
        let y = (((80. + coupon_cash) / cost).ln() * 365. / 2.).exp_m1();
        x["source"]["transfer"]["loan_clean_cost"] = json!(80.);
        x["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(accrued);
        x["source"]["acquisition"]["cost"] = json!(cost);
        x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
        x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
        x["credit"]["intervals"][0]["end_year"] = json!(2. / 365.);
        x["factor_policy"]["overnight"] = json!("observable_index_prefix_simple_margin-1");
        x["factor_policy"]["observation_mapping"] =
            json!("selected_observation_weight_over_interest_daycount-1");
        let out = owner::evaluate(&serde_json::from_value(x.clone()).unwrap()).unwrap();
        near(out.accounting().groups()[0].next.legal_interest, accrued);
        near(
            out.accounting()
                .groups()
                .iter()
                .skip(1)
                .map(|g| g.cash)
                .sum(),
            80. + coupon_cash,
        );
        for role in ["original", "current"] {
            x["source"][role]["periods"][0]["coupon"]["observations"][0]["source"]
                ["publication_unix_seconds"] = json!(midnight(at + 1));
        }
        assert!(owner::evaluate(&serde_json::from_value(x).unwrap()).is_err());
    }
}
