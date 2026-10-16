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
#[test]
fn full_origin_midcoupon_transfer_creates_buyer_right_without_seller_income() {
    let r: owner::Request = serde_json::from_value(raw()).unwrap();
    let out = owner::evaluate(&r).unwrap();
    let rows = out.accounting().groups();
    let opening = &rows[0];
    near(opening.next.principal, 100.);
    near(opening.next.recognized_interest, 1.);
    near(opening.next.legal_interest, 1.);
    near(opening.next.book_adjustment, -2.);
    near(opening.next.gross_carry, 99.);
    near(opening.effective_interest, 0.);
    near(opening.cash, -99.);
    assert_eq!(opening.next.lots[0].earning_origin_index, 3);
    near(rows.iter().map(|g| g.effective_interest).sum(), 4.);
    near(rows.iter().skip(1).map(|g| g.cash).sum(), 103.);
    let closing = out.accounting().closing().last().unwrap();
    near(closing.gross_carry, 0.);
    near(closing.legal_interest, 0.);
    near(closing.recognized_interest, 0.);
    assert!(closing.lots.iter().any(|l| l.earning_origin_index == 3));
    assert!(closing.lots.iter().any(|l| l.earning_origin_index == 0));
    let receipt = serde_json::to_value(&out).unwrap();
    assert_eq!(
        receipt["secondary_acquisition"]["seller_income_inherited"],
        false
    );
}
#[test]
fn secondary_unsupported_policies_and_omitted_prefix_reject() {
    for mutation in [
        "omitted", "split", "dates", "regime", "accrued", "clean", "state", "origin", "costs",
    ] {
        let mut x = raw();
        match mutation {
            "omitted" => {
                x["source"].as_object_mut().unwrap().remove("transfer");
            }
            "split" => x["source"]["transfer"]["conveyance"] = json!("partial"),
            "dates" => x["source"]["transfer"]["settlement_ordinal"] = json!(738887),
            "regime" => {
                x["source"]["transfer"]["regime"]["income_policy"] = json!("inherit_seller_yield")
            }
            "accrued" => {
                x["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] =
                    json!(2.)
            }
            "clean" => x["source"]["transfer"]["loan_clean_cost"] = json!(99.),
            "state" => x["credit"]["paths"][0]["initial_live_state"] = json!("watch"),
            "origin" => {
                x["source"]["transfer"]["contract_origin"]["opening_principal"] = json!(101.)
            }
            "costs" => x["source"]["transfer"]["transaction_cost"] = json!(1.),
            _ => unreachable!(),
        }
        let request: owner::Request = serde_json::from_value(x).unwrap();
        assert!(owner::evaluate(&request).is_err(), "{mutation}");
    }
}

#[test]
fn existing_graph_and_unit_routes_price_full_conveyed_cash_with_separate_fee_discount() {
    use portfolio_risk_native::{commercial_graph as graph, commercial_units as units};
    let candidate: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-secondary-candidate-journal-1.json"
    ))
    .unwrap();
    let g: graph::Request =
        serde_json::from_value(candidate["commercial"]["units"][0]["graph"].clone()).unwrap();
    let out = graph::analyze(&g).unwrap();
    near(out.metrics[0].pv, 104.);
    let physical = out
        .native_sources
        .iter()
        .find(|s| s.physical_accounting_source().is_some())
        .unwrap();
    let fee = physical.fee_projection.as_ref().unwrap();
    let stocks = fee
        .bound_path
        .source()
        .groups()
        .iter()
        .find(|g| g.event.action == portfolio_model_core::nonintegral_fee::Action::Acquire)
        .unwrap();
    near(stocks.next.fee_receivable, 6.);
    near(stocks.next.acquired_fee_asset_valuation_adjustment, -1.);
    near(stocks.next.acquired_fee_liability, 0.);
    let units: units::Request = serde_json::from_value(candidate["commercial"].clone()).unwrap();
    let prepared = units::prepare(&units).unwrap();
    near(prepared.source().units[0].quote_price_per_face_unit, 1.04);
    assert!(prepared.source().fee_income_values.iter().all(|x| *x == 0.));
}
#[test]
fn later_holding_cut_retains_buyer_original_anchor_and_purchased_lot() {
    use portfolio_risk_native::products::commercial::continuation;
    let original: owner::Request = serde_json::from_value(raw()).unwrap();
    let request = continuation::Request {
        schema: continuation::SCHEMA.into(),
        cut_ordinal: 738901,
        completed_source: original.clone(),
        revised_source: original,
    };
    let out = portfolio_compute_control::scope(
        Some(portfolio_compute_control::Control::new(30000).unwrap()),
        || continuation::evaluate(&request),
    )
    .unwrap();
    assert!(!out.original_anchor_refitted);
    assert!(!out.second_acquisition_posted);
    let changed = serde_json::to_value(&out).unwrap();
    assert!(changed["openings"][0]["book"]["lots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["earning_origin_index"] == 3));
}

#[test]
fn default_after_purchase_keeps_acquired_and_newly_earned_recovery_claims_separate() {
    let mut x = raw();
    x["credit"]["intervals"][0]["hazards_per_year"][0][3] = json!(100.);
    let r: owner::Request = serde_json::from_value(x).unwrap();
    let out = owner::evaluate(&r).unwrap();
    assert!(out.carry().credit().result.paths[0].default.is_some());
    let recoveries = out.carry().native_coupon_recoveries();
    assert!(recoveries.iter().any(|r| r.earning_origin_index == 3));
    assert!(recoveries.iter().any(|r| r.earning_origin_index == 0));
    near(
        recoveries
            .iter()
            .filter(|r| r.earning_origin_index == 3)
            .map(|r| r.interest)
            .sum(),
        0.5,
    );
    let closing = out.accounting().closing().last().unwrap();
    let purchased = closing
        .lots
        .iter()
        .find(|l| l.earning_origin_index == 3)
        .unwrap();
    near(purchased.legal_remaining, 0.5);
    assert!(purchased.default_claim);
}
#[test]
fn amortized_original_prefix_is_replayed_without_ever_importing_a_seller_book() {
    let mut x = raw();
    let mut old = x["source"]["original"]["periods"][0].clone();
    old["period_id"] = json!("settled-before-buyer");
    old["start_ordinal"] = json!(738826);
    old["end_ordinal"] = json!(738856);
    old["payment_ordinal"] = json!(738856);
    x["source"]["transfer"]["contract_origin"]["asof_ordinal"] = json!(738826);
    x["source"]["transfer"]["contract_origin"]["observed_publication_cutoff_unix_seconds"] =
        json!((738826_i64 - 719163) * 86400);
    let active = x["source"]["original"]["periods"][0].clone();
    x["source"]["transfer"]["contract_origin"]["periods"] = json!([old, active]);
    x["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_payments"] =
        json!([20., 80.]);
    for role in ["original", "current"] {
        x["source"][role]["opening_principal"] = json!(80.);
        x["source"][role]["principal_plan"]["principal_payments"] = json!([80.]);
    }
    x["source"]["transfer"]["loan_clean_cost"] = json!(79.);
    x["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(0.8);
    x["source"]["acquisition"]["cost"] = json!(79.8);
    let y = ((82.4_f64 / 79.8).ln() * 365. / 60.).exp_m1();
    x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    let r: owner::Request = serde_json::from_value(x).unwrap();
    let out = owner::evaluate(&r).unwrap();
    let opening = &out.accounting().groups()[0].next;
    near(opening.principal, 80.);
    near(opening.recognized_interest, 0.8);
    near(opening.gross_carry, 79.8);
    assert_eq!(opening.lots.len(), 1);
    near(
        out.accounting()
            .groups()
            .iter()
            .map(|g| g.effective_interest)
            .sum(),
        2.6,
    );
}

#[test]
fn old_capital_bridge_rejects_secondary_contra_channels_until_reviewed_adoption() {
    use portfolio_risk_native::treasury_bridge as bridge;
    let r: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-candidate-fee-treasury-1.json"
    ))
    .unwrap();
    for name in [
        "commercial_acquired_fee_asset_valuation_adjustment",
        "commercial_acquired_fee_liability_valuation_adjustment",
    ] {
        let c = &r["capital"];
        let e = bridge::prepare_reviewed_commercial_fee_exposure(
            serde_json::from_value(c["treasury"].clone()).unwrap(),
            vec![bridge::Trial {
                scenario: "baseline".into(),
                account: "bank".into(),
                gl_account: name.into(),
                instrument_id: "commercial".into(),
                balance: -1.,
            }],
            serde_json::from_value(c["policies"].clone()).unwrap(),
            1.,
            &serde_json::from_value(c["exposure_policy"].clone()).unwrap(),
        )
        .unwrap_err();
        assert!(e.contains("valuation adjustments"), "{e}");
    }
}

fn indexed_transfer() -> (Value, f64, f64) {
    let mut r = raw();
    let at = 738886;
    let midnight = |d: i32| (i64::from(d) - 719163) * 86400;
    let path = r["source"]["current"]["path"].clone();
    let observations:Vec<_>=[0.04,0.05,0.06].into_iter().enumerate().map(|(i,rate)| {
        let day=at-1+i as i32;let publication=midnight(day+1);
        let source=if i==0 {json!({"kind":"observed","fixing":{"id":"reviewed-prior-fixing","revision":"1"},"publication_unix_seconds":publication})}
            else {json!({"kind":"conditional_published_projection","model":{"id":"supplied-future","revision":"1"},"path":path,"measure":"physical","state_information_unix_seconds":midnight(day),"publication_unix_seconds":publication})};
        json!({"interest_start_ordinal":day,"interest_end_ordinal":day+1,"observation_start_ordinal":day,"observation_end_ordinal":day+1,"fixing_ordinal":day,"selection_cutoff_unix_seconds":publication,"rate":rate,"source":source})
    }).collect();
    let coupon = json!({"kind":"indexed","index":{"index_id":"reviewed-overnight","kind":"overnight","tenor_months":null,"average_calendar_days":null},"calendar":{"id":"reviewed-calendar","revision":"1"},"complete_calendar_mapping":true,"calculation":"daily_compounded","margin_treatment":"simple","margin":0.01,"index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},"reset_cutoff_unix_seconds":null,"lookback_business_days":0,"observation_shift":false,"observations":observations});
    for role in ["original", "current"] {
        let source = &mut r["source"][role];
        source["reporting_end_ordinal"] = json!(at + 1);
        source["tail_end_ordinal"] = json!(at + 2);
        source["observed_publication_cutoff_unix_seconds"] = json!(midnight(at));
        let period = &mut source["periods"][0];
        period["start_ordinal"] = json!(at - 1);
        period["end_ordinal"] = json!(at + 2);
        period["payment_ordinal"] = json!(at + 2);
        period["day_count"] = json!("act365_fixed");
        period["coupon"] = coupon.clone();
    }
    let mut origin = r["source"]["original"].clone();
    origin["asof_ordinal"] = json!(at - 1);
    r["source"]["transfer"]["contract_origin"] = origin;
    let accrued = 100. * (0.04 / 365. + 0.01 / 365.);
    let full = 100.
        * ((1. + 0.04 / 365.) * (1. + 0.05 / 365.) * (1. + 0.06 / 365.) - 1. + 0.01 * 3. / 365.);
    let cost: f64 = 100. + accrued;
    let y = (((100. + full) / cost).ln() * 365. / 2.).exp_m1();
    r["source"]["transfer"]["loan_clean_cost"] = json!(100.);
    r["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(accrued);
    r["source"]["acquisition"]["cost"] = json!(cost);
    r["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    r["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    r["credit"]["intervals"][0]["end_year"] = json!(2. / 365.);
    r["factor_policy"]["overnight"] = json!("observable_index_prefix_simple_margin-1");
    r["factor_policy"]["observation_mapping"] =
        json!("selected_observation_weight_over_interest_daycount-1");
    (r, accrued, full)
}
#[test]
fn acquired_compounded_prefix_uses_only_available_fixing_and_full_product_once() {
    let (r, accrued, full) = indexed_transfer();
    let request: owner::Request = serde_json::from_value(r.clone()).unwrap();
    let actual = owner::evaluate(&request).unwrap();
    near(actual.accounting().groups()[0].next.legal_interest, accrued);
    near(
        actual
            .accounting()
            .groups()
            .iter()
            .skip(1)
            .map(|g| g.cash)
            .sum(),
        100. + full,
    );
    near(
        actual
            .accounting()
            .groups()
            .iter()
            .map(|g| g.effective_interest)
            .sum(),
        full - accrued,
    );
    // A historical fixing supplied only after transfer cannot become acquired
    // numeric stock, even if the caller also changes the accrued allocation.
    let mut unavailable = r;
    for source in ["original", "current"] {
        unavailable["source"][source]["periods"][0]["coupon"]["observations"][0]["source"]
            ["publication_unix_seconds"] = json!((738887_i64 - 719163) * 86400);
    }
    let request: owner::Request = serde_json::from_value(unavailable).unwrap();
    assert!(owner::evaluate(&request).is_err());
}

#[test]
fn negative_purchased_coupon_is_gross_obligation_without_reearning_it() {
    let mut r = raw();
    for role in ["original", "current"] {
        r["source"][role]["periods"][0]["coupon"]["annual_rate"] = json!(-0.12);
    }
    r["source"]["transfer"]["contract_origin"]["periods"][0]["coupon"]["annual_rate"] =
        json!(-0.12);
    r["source"]["transfer"]["loan_clean_cost"] = json!(100.);
    r["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(-1.);
    let y = ((97_f64 / 99.).ln() * 365. / 60.).exp_m1();
    r["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
    r["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    let raw: owner::Request = serde_json::from_value(r).unwrap();
    let out = owner::evaluate(&raw).unwrap();
    let groups = out.accounting().groups();
    near(groups[0].next.legal_interest, -1.);
    near(groups[0].next.recognized_interest, -1.);
    near(groups[0].effective_interest, 0.);
    near(groups[0].next.gross_carry, 99.);
    near(groups.iter().skip(1).map(|g| g.cash).sum(), 97.);
    near(groups.iter().map(|g| g.effective_interest).sum(), -2.);
}

#[test]
fn acquired_unit_face_tracks_live_eighty_after_replayed_origin_principal_payment() {
    use portfolio_risk_native::commercial_units as units;
    let mut raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-secondary-candidate-journal-1.json"
    ))
    .unwrap();
    let position = &mut raw["commercial"]["units"][0]["graph"]["positions"][0];
    position.as_object_mut().unwrap().remove("fees");
    fn amend(h: &mut Value) {
        let x = &mut h["source"];
        let mut settled = x["source"]["original"]["periods"][0].clone();
        settled["period_id"] = json!("settled-before-buyer");
        settled["start_ordinal"] = json!(738826);
        settled["end_ordinal"] = json!(738856);
        settled["payment_ordinal"] = json!(738856);
        let active = x["source"]["original"]["periods"][0].clone();
        let origin = &mut x["source"]["transfer"]["contract_origin"];
        origin["asof_ordinal"] = json!(738826);
        origin["observed_publication_cutoff_unix_seconds"] = json!((738826_i64 - 719163) * 86400);
        origin["periods"] = json!([settled, active]);
        origin["principal_plan"]["principal_payments"] = json!([20., 80.]);
        for role in ["original", "current"] {
            x["source"][role]["opening_principal"] = json!(80.);
            x["source"][role]["principal_plan"]["principal_payments"] = json!([80.]);
        }
        x["source"]["transfer"]["loan_clean_cost"] = json!(79.);
        x["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(0.8);
        x["source"]["acquisition"]["cost"] = json!(79.8);
        let y = ((82.4_f64 / 79.8).ln() * 365. / 60.).exp_m1();
        x["source"]["acquisition"]["original_annual_effective_rate"] = json!(y);
        x["accounting_rates"][0]["annual_effective_rates"] = json!([y, y, y]);
    }
    for h in position["original_pricing"].as_array_mut().unwrap() {
        amend(h);
    }
    for histories in position["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in histories[role].as_array_mut().unwrap() {
                amend(h);
            }
        }
    }
    position["original_signed_cash_quote"] = json!(79.8);
    position["oas_target"]["base_oas"] = json!((82.4_f64 / 79.8).ln() * 365. / 60.);
    raw["commercial"]["units"][0]["unit_face"] = json!(80.);
    let request: units::Request = serde_json::from_value(raw["commercial"].clone()).unwrap();
    let prepared = units::prepare(&request).unwrap();
    near(
        prepared.source().units[0].quote_price_per_face_unit,
        79.8 / 80.,
    );
    raw["commercial"]["units"][0]["unit_face"] = json!(100.);
    let wrong: units::Request = serde_json::from_value(raw["commercial"].clone()).unwrap();
    assert!(units::prepare(&wrong).err().unwrap().contains("unit face"));
}

#[test]
fn distinct_fixed_coupon_entitlement_one_hundred_live_eighty_requires_explicit_separate_owner() {
    let mut r = raw();
    // The full contractual coupon source still has entitlement100. A caller
    // cannot substitute live80 without a separately owned principal cash event.
    for role in ["original", "current"] {
        r["source"][role]["opening_principal"] = json!(80.);
        r["source"][role]["principal_plan"]["principal_payments"] = json!([80.]);
    }
    let request: owner::Request = serde_json::from_value(r).unwrap();
    assert!(owner::evaluate(&request).is_err());
}

#[test]
fn omitted_transfer_and_explicit_none_preserve_complete_historical_owner_receipts() {
    let old: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-credit-ledger-1.json"
    ))
    .unwrap();
    let a: owner::Request = serde_json::from_value(old["commercial"][0].clone()).unwrap();
    let mut explicit = old["commercial"][0].clone();
    explicit["source"]["transfer"] = Value::Null;
    let b: owner::Request = serde_json::from_value(explicit).unwrap();
    let a = serde_json::to_vec(&owner::evaluate(&a).unwrap()).unwrap();
    let b = serde_json::to_vec(&owner::evaluate(&b).unwrap()).unwrap();
    assert_eq!(a, b);
    let receipt: Value = serde_json::from_slice(&a).unwrap();
    assert!(receipt.get("secondary_acquisition").is_none());
    assert!(receipt["raw_request"]["source"].get("transfer").is_none());
}
