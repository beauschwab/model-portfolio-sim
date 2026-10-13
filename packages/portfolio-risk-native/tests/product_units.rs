use portfolio_risk_native::product_units::{self, ProductUnitsRequest, ProductUnitsResult};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> ProductUnitsRequest {
    serde_json::from_value(v).unwrap()
}
fn prepare(v: Value) -> ProductUnitsResult {
    product_units::prepare(&request(v)).unwrap()
}
fn error(v: Value) -> String {
    product_units::prepare(&request(v)).err().unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn metric(r: &ProductUnitsResult, scenario: usize, unit: usize, m: usize, period: usize) -> f64 {
    r.values[((scenario * r.shape[1] + unit) * 10 + m) * r.shape[3] + period]
}
fn all_graphs(v: &mut Value, f: impl Fn(&mut Value)) {
    for u in v["units"].as_array_mut().unwrap() {
        f(&mut u["purchase_graph"]["request"]);
        for b in u["age_branches"].as_array_mut().unwrap() {
            f(&mut b["graph"]["request"]);
        }
    }
}
fn paths(g: &mut Value, f: impl Fn(&str, &mut Value)) {
    for s in g["positions"][0]["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in s[role].as_array_mut().unwrap() {
                f(role, &mut h["request"]);
            }
        }
    }
}
#[test]
fn premium_liability_effective_interest_is_independent_of_coupon_and_cash_with_full_tail() {
    let v = raw();
    let r = prepare(v.clone());
    let quote = 1002. * (-0.02f64.ln_1p() * 2. / 365.).exp();
    let daily = (0.02f64.ln_1p() / 365.).exp_m1();
    near(metric(&r, 0, 0, 0, 0), -quote * daily / 1000.);
    near(
        metric(&r, 0, 0, 0, 1),
        -quote * (1. + daily) * daily / 1000.,
    );
    assert!((metric(&r, 0, 0, 0, 0) + 0.001).abs() > 1e-4); // Coupon expense is 1/1000, a different quantity.
    near(metric(&r, 0, 0, 1, 0), 1.);
    near(metric(&r, 0, 0, 9, 0), 1.);
    near(metric(&r, 0, 0, 8, 0), 0.);
    let q = quote / 1000.;
    near(r.funding.demand[0], -q);
    near(*r.funding.demand.last().unwrap(), 1.002 - q);
    assert_eq!(r.funding.shape, [1, 1, 3]);
    assert_eq!(r.funding.cash_events.len(), 3);
    near(
        r.units[0].income[0].opening_states[2].gross_carrying_value,
        0.,
    );
    near(
        r.units[0].income[0].opening_states[1].book_adjustment,
        quote * (1. + daily) - 1001.,
    );
    let age_pv = -1.002 * (-0.02f64.ln_1p() / 365.).exp();
    near(r.units[0].age_marks[1].pv_per_face, age_pv);
    near(
        metric(&r, 0, 0, 2, 1),
        age_pv * ((0.0001f64 / 365.).exp() - (-0.0001f64 / 365.).exp()) / 2.,
    );
    near(r.units[0].locked_purchase_oas, 0.02f64.ln_1p() - 0.05);
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        serde_json::to_value(prepare(v)).unwrap()
    );
}
#[test]
fn original_yield_quote_and_complete_branch_identity_domains_fail_closed() {
    let mut v = raw();
    v["units"][0]["income_policy"]["original_annual_effective_rate"] = json!(0.03);
    assert!(error(v).contains("reconcile"));
    let mut v = raw();
    v["units"][0]["age_branches"].as_array_mut().unwrap().pop();
    assert!(error(v).contains("coverage"));
    let mut v = raw();
    v["units"][0]["age_branches"][1]["physical_path_id"] = json!("foreign");
    assert!(error(v).contains("branch"));
    let mut v = raw();
    v["units"][0]["age_branches"][1]["graph"]["request"]["positions"][0]["oas_target"]
        ["base_oas"] = json!(0.);
    assert!(error(v).contains("OAS"));
    let mut v = raw();
    v["units"][0]["age_branches"][1]["graph"]["request"]["positions"][0]["histories"][0]
        ["pricing"][0]["request"]["drivers"][0]["reference_rate"] = json!(0.1);
    assert!(error(v).contains("prefix"));
    let mut v = raw();
    v["units"][0]["purchase_graph"]["request"]["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":1002.,"source_id":"quote","source_revision":"1"});
    assert!(error(v).contains("locked"));
    let mut v = raw();
    v["units"][0]["income_policy"]["review"]["extra"] = json!(1);
    assert!(serde_json::from_value::<ProductUnitsRequest>(v).is_err());
    let mut v = raw();
    v["units"][0]["quote_price_per_face_unit"] = json!(-1.);
    assert!(error(v).contains("quote"));
}
#[test]
fn negative_coupon_liability_signed_effective_yield_and_retained_basis() {
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(g, |_, r| {
            r["segments"][0]["opening_paid_rate"] = json!(-0.1825);
            r["segments"][0]["repricing"]["target"]["intercept"] = json!(-0.1825);
            r["segments"][0]["repricing"]["floor"] = json!(-0.2);
        })
    });
    let y = -0.01f64;
    let quote = 999. * (-y.ln_1p() * 2. / 365.).exp();
    v["units"][0]["income_policy"]["original_annual_effective_rate"] = json!(y);
    v["units"][0]["quote_price_per_face_unit"] = json!(quote / 1000.);
    all_graphs(&mut v, |g| {
        g["positions"][0]["oas_target"]["base_oas"] = json!(y.ln_1p() - 0.05)
    });
    let r = prepare(v);
    near(
        metric(&r, 0, 0, 0, 0),
        -quote * (y.ln_1p() / 365.).exp_m1() / 1000.,
    );
    assert!(metric(&r, 0, 0, 0, 0) > 0.);
    near(
        r.funding
            .cash_events
            .iter()
            .map(|e| e.interest_cash)
            .sum::<f64>(),
        0.001,
    );
}
#[test]
fn unsupported_migration_external_inflow_and_same_asof_settlement_are_explicit_errors() {
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(g, |_, r| r["drivers"][1]["external_inflows"][0] = json!(1.))
    });
    assert!(error(v).contains("inflow"));
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(g, |_, r| {
            r["segments"][0]["maturity_ordinal"] = json!(738916)
        })
    });
    assert!(error(v).contains("same-asof"));
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(
            g,
            |_, r| r["migrations"] = json!([{"from_segment":"retail","to_segment":"retail","base_per_year":0.,"destination_rate_gap_sensitivity":0.}]),
        )
    });
    assert!(error(v).contains("migration"));
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    assert_eq!(
        portfolio_compute_control::scope(Some(c), || product_units::prepare(&request(raw())))
            .err()
            .unwrap(),
        "COMPUTE_CANCELLED"
    );
}
fn from_graph(kind: &str, mut g: Value, face: f64, quote: f64, y: f64) -> Value {
    let curve = &g["scenarios"][0]["collateral_curve"];
    let rate = -curve["discount_factors"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .as_f64()
        .unwrap()
        .ln()
        / curve["times_years"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_f64()
            .unwrap();
    g["positions"][0]["oas_target"]["base_oas"] = json!(y.ln_1p() - rate);
    let mut v = raw();
    let u = &mut v["units"][0];
    let id = g["positions"][0]["position_id"].clone();
    let asof = g["as_of_ordinal"].clone();
    let reporting = g["reporting_ordinals"].as_array().unwrap().clone();
    let scenario = g["base_scenario_id"].clone();
    let sets = if kind == "consumer_installment" {
        &g["positions"][0]["product"]["histories"]
    } else {
        &g["positions"][0]["histories"]
    };
    let path = sets[0]["physical"][0]["path_id"].clone();
    u["unit_id"] = id;
    u["unit_face"] = json!(face);
    u["purchase_ordinal"] = asof;
    u["quote_price_per_face_unit"] = json!(quote / face);
    u["purchase_graph"] = json!({"kind":kind,"request":g});
    u["income_policy"]["original_annual_effective_rate"] = json!(y);
    u["age_branches"]=Value::Array(reporting[..reporting.len()-1].iter().map(|day|json!({"ordinal":day,"scenario_id":scenario,"physical_path_id":path,"graph":u["purchase_graph"]})).collect());
    v["reporting_ordinals"] = json!(reporting);
    v["base_scenario_id"] = scenario;
    v
}
#[test]
fn consumer_effective_income_purchase_to_age_and_full_cash_match_independent_annuity_math() {
    let g: Value = serde_json::from_str(include_str!(
        "../../../examples/models/advanced-product-graph-1.json"
    ))
    .unwrap();
    let y = 0.02f64;
    let quote = 529. * (-y.ln_1p() * 29. / 365.).exp() + 515.5 * (-y.ln_1p() * 60. / 365.).exp();
    let v = from_graph("consumer_installment", g, 1000., quote, y);
    let r = prepare(v.clone());
    let earned1 = quote * (y.ln_1p() * 29. / 365.).exp_m1();
    let b = quote + earned1 - 529.;
    let earned2 = b * (y.ln_1p() * 31. / 365.).exp_m1();
    near(metric(&r, 0, 0, 0, 0), earned1 / 1000.);
    near(metric(&r, 0, 0, 0, 1), earned2 / 1000.);
    near(metric(&r, 0, 0, 1, 1), 0.5);
    near(
        r.units[0].age_marks[1].pv_per_face,
        0.5155 * (-y.ln_1p() * 31. / 365.).exp(),
    );
    near(*r.funding.demand.last().unwrap(), (quote - 1044.5) / 1000.);
    // These gross contractual interest amounts are retained but are not the NII coefficients.
    assert!((earned1 - 29.).abs() > 1.);
    assert!((earned2 - 15.5).abs() > 1.);
    for action in [
        json!({"kind":"default"}),
        json!({"kind":"set_state","state":"nonaccrual"}),
        json!({"kind":"fee","amount":1.}),
        json!({"kind":"change_payment","amount":10.}),
    ] {
        let mut bad = v.clone();
        bad["units"][0]["purchase_graph"]["request"]["positions"][0]["product"]["histories"][0]
            ["physical"][0]["steps"] = json!([{"ordinal":738945,"action":action}]);
        assert!(error(bad).contains("unsupported"));
    }
}
#[test]
fn mortgage_retains_remittance_receivables_actual_tail_and_original_yield_income() {
    let g: Value = serde_json::from_str(include_str!(
        "../../../examples/models/mortgage-product-graph-1.json"
    ))
    .unwrap();
    let y = 0.02f64;
    let final_cash = 32.1661 + 0.0321661;
    let quote = 34. * (-y.ln_1p() * 12. / 365.).exp()
        + 34. * (-y.ln_1p() * 22. / 365.).exp()
        + final_cash * (-y.ln_1p() * 32. / 365.).exp();
    let v = from_graph("mortgage", g, 100., quote, y);
    let r = prepare(v.clone());
    near(
        metric(&r, 0, 0, 0, 0),
        quote * (y.ln_1p() * 10. / 365.).exp_m1() / 100.,
    );
    near(metric(&r, 0, 0, 1, 1), 1.); // Entitled principal has not remitted at day 10.
    near(
        r.units[0].age_marks[1].pv_per_face,
        quote * (y.ln_1p() * 10. / 365.).exp() / 100.,
    );
    assert_eq!(*r.funding.ordinals.last().unwrap(), 739649); // day 32, after last reporting day 20.
    near(
        *r.funding.demand.last().unwrap(),
        (quote - 100.1982661) / 100.,
    );
    let mut bad = v;
    bad["units"][0]["purchase_graph"]["request"]["positions"][0]["histories"][0]["physical"][0]
        ["path"][0]["credit_hazards_per_year"][0][3] = json!(0.1);
    assert!(error(bad).contains("ownership open"));
}
#[test]
fn card_repayment_only_no_grace_has_real_effective_income_and_rejects_unowned_earning() {
    let mut g: Value = serde_json::from_str(include_str!(
        "../../../examples/models/card-product-graph-1.json"
    ))
    .unwrap();
    g["positions"][0]["contract"]["initial_purchase_grace"] = json!(false);
    g["positions"][0]["contract"]["regain_grace_after_full_statement_payment"] = json!(false);
    g["positions"][0]["contract"]["horizon_days"] = json!(4);
    g["reporting_ordinals"] = json!([738916, 738917, 738918, 738919, 738920]);
    g["positions"][0]["contract"]["cycles"] = json!([
        {"statement_ordinal":738917,"due_ordinal":738918},
        {"statement_ordinal":738919,"due_ordinal":738920}
    ]);
    for s in g["positions"][0]["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in s[role].as_array_mut().unwrap() {
                h["steps"] = json!([{"ordinal":738918,"action":{"kind":"payment","amount":1001.}},{"ordinal":738920,"action":{"kind":"payment","amount":1.}},{"ordinal":738920,"action":{"kind":"set_limit","amount":0.}}]);
            }
        }
    }
    let y = 0.02f64;
    let quote = 1001. * (-y.ln_1p() * 2. / 365.).exp() + (-y.ln_1p() * 4. / 365.).exp();
    let v = from_graph("card", g, 1000., quote, y);
    let r = prepare(v.clone());
    near(
        metric(&r, 0, 0, 0, 0),
        quote * (y.ln_1p() / 365.).exp_m1() / 1000.,
    );
    near(
        r.units[0].age_marks[1].pv_per_face,
        (1001. * (-y.ln_1p() / 365.).exp() + (-y.ln_1p() * 3. / 365.).exp()) / 1000.,
    );
    near(metric(&r, 0, 0, 1, 2), 0.);
    near(
        metric(&r, 0, 0, 0, 2),
        (-y.ln_1p() * 2. / 365.).exp() * (y.ln_1p() / 365.).exp_m1() / 1000.,
    );
    assert!(metric(&r, 0, 0, 0, 2) > 0.); // Explicit unbilled right still earns EIR after principal retirement.
    for action in [
        json!({"kind":"draw","balance_class":"cash_advance","amount":1.}),
        json!({"kind":"default"}),
        json!({"kind":"set_state","state":"nonaccrual"}),
    ] {
        let mut bad = v.clone();
        bad["units"][0]["purchase_graph"]["request"]["positions"][0]["histories"][0]["physical"][0]
            ["steps"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"ordinal":738917,"action":action}));
        assert!(error(bad).contains("unsupported"));
    }
    let mut bad = v;
    bad["units"][0]["purchase_graph"]["request"]["positions"][0]["contract"]
        ["initial_purchase_grace"] = json!(true);
    assert!(error(bad).contains("grace"));
}
#[test]
fn physical_path_weights_and_per_face_amounts_apply_exactly_once() {
    let mut v = raw();
    let g = &mut v["units"][0]["purchase_graph"]["request"];
    let mut second = g["positions"][0]["histories"][0]["physical"][0].clone();
    second["path_id"] = json!("p-2");
    second["weight"] = json!(0.75);
    g["positions"][0]["histories"][0]["physical"][0]["weight"] = json!(0.25);
    g["positions"][0]["histories"][0]["physical"]
        .as_array_mut()
        .unwrap()
        .push(second.clone());
    let g = g.clone();
    let mut branches = Vec::new();
    for day in [738916, 738917] {
        for (path, index) in [("p-1", 0), ("p-2", 1)] {
            let mut branch = g.clone();
            let mut p = g["positions"][0]["histories"][0]["physical"][index].clone();
            p["weight"] = json!(1.);
            branch["positions"][0]["histories"][0]["physical"] = json!([p]);
            branches.push(json!({"ordinal":day,"scenario_id":"baseline","physical_path_id":path,"graph":{"kind":"deposit","request":branch}}));
        }
    }
    v["units"][0]["age_branches"] = json!(branches);
    let r = prepare(v);
    let base = prepare(raw());
    for (a, b) in r.values.iter().zip(&base.values) {
        near(*a, *b);
    }
    for (a, b) in r.funding.demand.iter().zip(&base.funding.demand) {
        near(*a, *b);
    }
    assert_eq!(
        r.funding
            .cash_events
            .iter()
            .filter(|e| e.purchase_cash != 0.)
            .count(),
        1
    );
    near(
        r.funding
            .cash_events
            .iter()
            .filter(|e| e.path_id == "p-1")
            .map(|e| e.interest_cash)
            .sum(),
        -0.0005,
    );
    near(
        r.funding
            .cash_events
            .iter()
            .filter(|e| e.path_id == "p-2")
            .map(|e| e.interest_cash)
            .sum(),
        -0.0015,
    );
}
#[test]
fn supplied_q_future_is_conditioned_on_exact_physical_prefix_instead_of_clipped_purchase_q() {
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(g, |role, r| {
            r["segments"][0]["repricing"]["target"]["intercept"] = json!(0.);
            r["segments"][0]["repricing"]["target"]["beta"] = json!(1.);
            for d in r["drivers"].as_array_mut().unwrap() {
                d["reference_rate"] = json!(if role != "physical"
                    && d["ordinal"].as_i64().unwrap() >= 738916
                {
                    0.73
                } else {
                    0.365
                });
            }
        })
    });
    // At age one, already-earned physical coupon is 1, not purchase-Q's coupon 2.
    for role in ["pricing", "pricing_down", "pricing_up"] {
        v["units"][0]["age_branches"][1]["graph"]["request"]["positions"][0]["histories"][0]
            [role][0]["request"]["drivers"][1]["reference_rate"] = json!(0.365);
    }
    let quote = v["units"][0]["quote_price_per_face_unit"].as_f64().unwrap() * 1000.;
    let oas = (1004. / quote).ln() * 365. / 2. - 0.05;
    all_graphs(&mut v, |g| {
        g["positions"][0]["oas_target"]["base_oas"] = json!(oas)
    });
    let r = prepare(v.clone());
    near(
        r.units[0].age_marks[1].pv_per_face,
        -1.003 * (-(oas + 0.05) / 365.).exp(),
    );
    assert!(
        (r.units[0].age_marks[1].pv_per_face - (-1.004 * (-(oas + 0.05) / 365.).exp())).abs()
            > 0.0009
    );
    near(metric(&r, 0, 0, 0, 0), metric(&prepare(raw()), 0, 0, 0, 0)); // Q does not set physical NII.
    v["units"][0]["age_branches"][1]["graph"]["request"]["positions"][0]["histories"][0]
        ["pricing"][0]["request"]["drivers"][1]["reference_rate"] = json!(0.73);
    assert!(error(v).contains("prefix"));
}
#[test]
fn scenario_specific_runoff_funding_is_not_replayed_as_a_shared_contractual_schedule() {
    let mut v = raw();
    v["units"][0]["quote_price_per_face_unit"] = json!(1.);
    v["units"][0]["income_policy"]["original_annual_effective_rate"] = json!(0.);
    let g = &mut v["units"][0]["purchase_graph"]["request"];
    g["positions"][0]["oas_target"]["base_oas"] = json!(-0.05);
    paths(g, |_, r| {
        r["segments"][0]["opening_paid_rate"] = json!(0.);
        r["segments"][0]["repricing"]["target"]["intercept"] = json!(0.);
        r["segments"][0]["retention"]["uninsured_stress_sensitivity"] = json!(100.);
    });
    let mut scene = g["scenarios"][0].clone();
    scene["scenario_id"] = json!("stress");
    scene["run_id"] = json!("stress-run");
    scene["severity"] = json!(1.);
    g["scenarios"].as_array_mut().unwrap().push(scene);
    let mut set = g["positions"][0]["histories"][0].clone();
    set["scenario_id"] = json!("stress");
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        set[role][0]["request"]["identity"]["run_id"] = json!("stress-run");
        set[role][0]["request"]["drivers"][2]["confidence_stress"][0] = json!(1.);
    }
    g["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
        .push(set);
    let g = g.clone();
    let mut branches = Vec::new();
    for (scenario, index) in [("baseline", 0), ("stress", 1)] {
        for day in [738916, 738917] {
            let mut b = g.clone();
            b["base_scenario_id"] = json!(scenario);
            b["scenarios"] = json!([g["scenarios"][index]]);
            b["positions"][0]["histories"] = json!([g["positions"][0]["histories"][index]]);
            branches.push(json!({"ordinal":day,"scenario_id":scenario,"physical_path_id":"p-1","graph":{"kind":"deposit","request":b}}));
        }
    }
    v["units"][0]["age_branches"] = json!(branches);
    let r = prepare(v);
    assert_eq!(r.funding.shape, [2, 1, 3]);
    near(r.funding.demand[1], -1.);
    near(r.funding.demand[4], -(-50.0f64 / 365.).exp());
    near(r.funding.demand[2], 0.);
    near(r.funding.demand[5], 0.);
    near(metric(&r, 1, 0, 1, 1), (-50.0f64 / 365.).exp());
    assert!(r
        .funding
        .cash_events
        .iter()
        .any(|e| e.scenario_id == "stress" && e.ordinal == 738917 && e.principal_cash < 0.));
}
#[test]
fn aggregate_income_cross_product_work_is_admitted_before_dynamic_generation() {
    let mut v = raw();
    all_graphs(&mut v, |g| {
        paths(g, |_, r| {
            r["horizon_days"] = json!(2000);
            r["segments"][0]["maturity_ordinal"] = json!(740916);
            r["segments"][0]["interest_payment_ordinals"] = json!([740916]);
            r["segments"][0]["retention"]["withdrawal_per_year"] = json!(100.);
            r["segments"][0]["retention"]["closure_per_year"] = json!(100.);
            let template = r["drivers"][0].clone();
            r["drivers"] = Value::Array(
                (738915..740916)
                    .map(|day| {
                        let mut d = template.clone();
                        d["ordinal"] = json!(day);
                        d
                    })
                    .collect(),
            );
        })
    });
    assert!(error(v).contains("work")); // Dynamic combined intensity would fail if generated.
}

#[test]
fn actual_supplied_future_purchase_has_zero_coefficients_and_cash_before_purchase() {
    fn shift(v: &mut Value, key: &str) {
        match v {
            Value::Object(o) => {
                for (k, v) in o {
                    shift(v, k)
                }
            }
            Value::Array(a) => {
                for v in a {
                    shift(v, key)
                }
            }
            Value::Number(n)
                if [
                    "as_of_ordinal",
                    "ordinal",
                    "maturity_ordinal",
                    "interest_payment_ordinals",
                    "reporting_ordinals",
                ]
                .contains(&key) =>
            {
                *v = json!(n.as_i64().unwrap() + 1);
            }
            _ => {}
        }
    }
    let mut v = raw();
    let u = &mut v["units"][0];
    u["purchase_ordinal"] = json!(738917);
    let g = &mut u["purchase_graph"]["request"];
    shift(g, "");
    g["reporting_ordinals"] = json!([738917, 738918]);
    let g = g.clone();
    u["age_branches"] = json!([{"ordinal":738917,"scenario_id":"baseline","physical_path_id":"p-1","graph":{"kind":"deposit","request":g}}]);
    let r = prepare(v);
    for m in 0..10 {
        near(metric(&r, 0, 0, m, 0), 0.);
    }
    near(r.funding.demand[0], 0.);
    assert_eq!(*r.funding.ordinals.last().unwrap(), 738919);
    assert_eq!(r.units[0].age_marks.len(), 1);
    near(metric(&r, 0, 0, 1, 1), 1.);
    assert!(metric(&r, 0, 0, 0, 1) < 0.);
}
