use portfolio_risk_native::{
    deposit_graph,
    graph_cohort::{self, Request, ResultData},
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/graph-cohort-audit-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn evaluate(v: Value) -> ResultData {
    graph_cohort::evaluate(&request(v)).unwrap()
}
fn error(v: Value) -> String {
    graph_cohort::evaluate(&request(v)).err().unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn amount<'a>(r: &'a ResultData, metric: &str) -> &'a graph_cohort::ErrorRow {
    r.errors.iter().find(|e| e.metric == metric).unwrap()
}
fn paths(row: &mut Value, f: impl Fn(&str, &mut Value)) {
    for set in row["graph"]["request"]["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
    {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for h in set[role].as_array_mut().unwrap() {
                f(role, &mut h["request"]);
            }
        }
    }
}
#[test]
fn independent_full_liability_cash_accrual_and_signed_member_reduction() {
    let v = raw();
    let r = evaluate(v.clone());
    assert!(r.all_metrics_within_tolerance);
    assert!(r.diagnostic_only);
    assert!(!r.empirical_validation);
    assert!(!r.cohort_publication_allowed);
    // Each supplied 1000 note earns 1000*.365/365 = 1 daily, then settles 1002.
    near(amount(&r, "liability_pv").representative, 2004.);
    near(amount(&r, "bank_cash_pv").summed_members, -2004.);
    near(
        amount(&r, "source_full_cash/principal").summed_members,
        -2000.,
    );
    near(amount(&r, "source_full_cash/interest").representative, -4.);
    near(amount(&r, "source_full_gross_interest").summed_members, 4.);
    near(
        amount(&r, "physical_periods/0/interest_expense").representative,
        2.,
    );
    near(
        amount(&r, "physical_tail/interest_expense").summed_members,
        2.,
    );
    near(
        amount(&r, "supplied_history_liability_dv01").summed_members,
        2004. * ((0.0001f64 * 2. / 365.).exp() - (-0.0001f64 * 2. / 365.).exp()) / 2.,
    );
    assert_eq!(
        r.rows.iter().map(|r| r.row_id.as_str()).collect::<Vec<_>>(),
        vec!["representative", "member-1", "member-2"]
    );
    assert_eq!(
        serde_json::to_value(&r.raw_source).unwrap(),
        serde_json::to_value(request(v.clone())).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        serde_json::to_value(evaluate(v)).unwrap()
    );
}
#[test]
fn heterogeneous_supplied_members_retain_independent_p_q_and_fail_explicit_tolerance() {
    let mut v = raw();
    paths(&mut v["cohorts"][0]["members"][1], |role, r| {
        if role != "physical" {
            r["segments"][0]["repricing"]["target"]["intercept"] = json!(0.1825);
        }
    });
    let r = evaluate(v);
    let e = amount(&r, "liability_pv");
    near(e.representative, 2004.);
    near(e.summed_members, 2003.);
    near(e.signed_error, 1.);
    near(e.relative_error.unwrap(), 1. / 2003.);
    assert!(!e.passes);
    assert!(!r.all_metrics_within_tolerance);
    near(amount(&r, "bank_cash_pv").signed_error, -1.);
    near(amount(&r, "source_full_gross_interest").summed_members, 4.);
    assert!(!r.cohort_publication_allowed);
}
#[test]
fn negative_coupon_amounts_are_not_absolute_values_or_survival_scaled() {
    let mut v = raw();
    for row in std::iter::once(&mut v["cohorts"][0]["representative"]) {
        paths(row, |_, r| {
            r["segments"][0]["opening_paid_rate"] = json!(-0.1825);
            r["segments"][0]["repricing"]["target"]["intercept"] = json!(-0.1825);
            r["segments"][0]["repricing"]["floor"] = json!(-0.2);
        });
    }
    for row in v["cohorts"][0]["members"].as_array_mut().unwrap() {
        paths(row, |_, r| {
            r["segments"][0]["opening_paid_rate"] = json!(-0.1825);
            r["segments"][0]["repricing"]["target"]["intercept"] = json!(-0.1825);
            r["segments"][0]["repricing"]["floor"] = json!(-0.2);
        });
    }
    let r = evaluate(v);
    near(amount(&r, "liability_pv").summed_members, 1998.);
    near(amount(&r, "source_full_gross_interest").summed_members, -2.);
    near(amount(&r, "source_full_cash/interest").summed_members, 2.);
    assert!(r.all_metrics_within_tolerance);
}
#[test]
fn frozen_oas_context_and_complete_tolerance_contract_reject_ambiguity() {
    let mut v = raw();
    v["cohorts"][0]["tolerances"].as_array_mut().unwrap().pop();
    assert!(error(v).contains("coverage"));
    let mut v = raw();
    v["cohorts"][0]["tolerances"][0]["metric"] = json!("unknown");
    assert!(error(v).contains("coverage"));
    let mut v = raw();
    let t = v["cohorts"][0]["tolerances"][0].clone();
    v["cohorts"][0]["tolerances"]
        .as_array_mut()
        .unwrap()
        .push(t);
    assert!(error(v).contains("tolerance"));
    let mut v = raw();
    v["cohorts"][0]["tolerances"][0]["absolute_amount"] = json!(-1.);
    assert!(error(v).contains("tolerance"));
    let mut v = raw();
    v["monetary_unit"] = json!("dollars");
    assert!(error(v).contains("currency units"));
    for key in ["currency", "reporting_ordinals", "bump_bp", "identity"] {
        let mut v = raw();
        let g = &mut v["cohorts"][0]["members"][0]["graph"]["request"];
        match key {
            "currency" => g[key] = json!("EUR"),
            "reporting_ordinals" => g[key] = json!([738916, 738918]),
            "bump_bp" => g[key] = json!(2.),
            _ => g[key]["paired_history_revision"] = json!("changed"),
        }
        assert!(error(v).contains("context"));
    }
    let mut v = raw();
    v["cohorts"][0]["members"][0]["graph"]["request"]["positions"][0]["histories"][0]["pricing"]
        [0]["path_id"] = json!("unpaired");
    assert!(error(v).contains("context"));
    let mut v = raw();
    v["cohorts"][0]["representative"]["graph"]["request"]["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":1002.,"source_id":"target","source_revision":"1"});
    assert!(error(v).contains("no calibration"));
    let mut v = raw();
    v["cohorts"][0]["members"][0]["row_id"] = json!("member-2");
    assert!(error(v).contains("identify"));
    let mut v = raw();
    v["extra"] = json!(1);
    assert!(serde_json::from_value::<Request>(v).is_err());
}
#[test]
fn own_original_oas_can_differ_without_being_recalibrated() {
    let mut v = raw();
    v["cohorts"][0]["members"][0]["graph"]["request"]["positions"][0]["oas_target"]["base_oas"] =
        json!(0.05);
    let r = evaluate(v);
    near(
        amount(&r, "liability_pv").summed_members,
        1002. + 1002. * (-0.10f64 * 2. / 365.).exp(),
    );
    near(
        r.rows[1].native_result["base_anchor"]["oas"][0]["base_oas"]
            .as_f64()
            .unwrap(),
        0.05,
    );
}
#[test]
fn aggregate_work_is_rejected_before_dynamic_state_generation_and_cancelled_work_stops() {
    let mut v = raw();
    for row in std::iter::once(&mut v["cohorts"][0]["representative"]) {
        paths(row, |_, r| {
            r["horizon_days"] = json!(365);
            r["segments"][0]["maturity_ordinal"] = json!(739281);
            r["segments"][0]["interest_payment_ordinals"] = json!([739281]);
            r["segments"][0]["retention"]["withdrawal_per_year"] = json!(100.);
            r["segments"][0]["retention"]["closure_per_year"] = json!(100.);
            let template = r["drivers"][0].clone();
            r["drivers"] = Value::Array(
                (738915..739281)
                    .map(|day| {
                        let mut d = template.clone();
                        d["ordinal"] = json!(day);
                        d
                    })
                    .collect(),
            );
        });
    }
    let template = v["cohorts"][0]["representative"].clone();
    let native: deposit_graph::Request =
        serde_json::from_value(template["graph"]["request"].clone()).unwrap();
    assert!(deposit_graph::audit_admitted_work(&native).is_ok());
    v["cohorts"][0]["members"] = Value::Array(
        (0..100)
            .map(|i| {
                let mut row = template.clone();
                let name = format!("member-{i}");
                row["row_id"] = json!(name);
                row["graph"]["request"]["positions"][0]["position_id"] = json!(name);
                row
            })
            .collect(),
    );
    assert!(error(v).contains("aggregate work"));
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    let e = portfolio_compute_control::scope(Some(c), || graph_cohort::evaluate(&request(raw())))
        .err()
        .unwrap();
    assert_eq!(e, "COMPUTE_CANCELLED");
}

fn numeric_labels(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Number(_) => {
            out.insert(prefix.to_owned());
        }
        Value::Object(m) => {
            for (k, x) in m {
                if k != "position_id" && k != "scenario_id" {
                    numeric_labels(
                        x,
                        &if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}/{k}")
                        },
                        out,
                    );
                }
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                numeric_labels(x, &format!("{prefix}/{i}"), out);
            }
        }
        _ => panic!("unexpected nonnumeric test metric"),
    }
}
#[test]
fn all_four_raw_product_dispatches_preserve_sources_and_exact_additive_amounts() {
    // This fixture tests audit dispatch/reduction, not an independent pricing implementation.
    // Each product's pricing/economics already has dedicated hand fixtures; deposit hand math is above.
    for (kind, text) in [
        (
            "consumer_installment",
            include_str!("../../../examples/models/advanced-product-graph-1.json"),
        ),
        (
            "card",
            include_str!("../../../examples/models/card-product-graph-1.json"),
        ),
        (
            "mortgage",
            include_str!("../../../examples/models/mortgage-product-graph-1.json"),
        ),
        (
            "deposit",
            include_str!("../../../examples/models/deposit-product-graph-1.json"),
        ),
    ] {
        let base: Value = serde_json::from_str(text).unwrap();
        let result = match kind {
            "consumer_installment" => serde_json::to_value(
                portfolio_risk_native::advanced_graph::analyze(
                    &serde_json::from_value(base.clone()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
            "card" => serde_json::to_value(
                portfolio_risk_native::card_graph::analyze(
                    &serde_json::from_value(base.clone()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
            "mortgage" => serde_json::to_value(
                portfolio_risk_native::mortgage_graph::analyze(
                    &serde_json::from_value(base.clone()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
            _ => serde_json::to_value(
                deposit_graph::analyze(&serde_json::from_value(base.clone()).unwrap()).unwrap(),
            )
            .unwrap(),
        };
        let mut labels = BTreeSet::new();
        numeric_labels(&result["metrics"][0], "", &mut labels);
        for name in [
            "source_full_cash/principal",
            "source_full_cash/interest",
            "source_full_cash/fees",
            "source_full_gross_interest",
        ] {
            labels.insert(name.into());
        }
        if kind == "consumer_installment" || kind == "card" {
            for name in [
                "source_full_default/principal",
                "source_full_default/principal_loss",
            ] {
                labels.insert(name.into());
            }
        }
        let row = |name: &str, m: f64| {
            let mut g = base.clone();
            g["positions"][0]["position_id"] = json!(name);
            if kind == "consumer_installment" {
                g["positions"][0]["product"]["contract"]["identity"]["position_id"] = json!(name);
            }
            if kind == "card" || kind == "mortgage" {
                g["positions"][0]["contract"]["identity"]["position_id"] = json!(name);
            }
            json!({"row_id":name,"amount_multiplier":m,"graph":{"kind":kind,"request":g}})
        };
        let v = json!({"schema":graph_cohort::SCHEMA,"source_revision":"synthetic-dispatch-v1","monetary_unit":"currency_units","cohorts":[{
            "cohort_id":kind,"representative":row("rep",2.),"members":[row("m1",1.),row("m2",1.)],
            "tolerances":labels.into_iter().map(|metric|json!({"metric":metric,"absolute_amount":1e-9,"relative":1e-12})).collect::<Vec<_>>()
        }]});
        let r = evaluate(v);
        assert!(r.all_metrics_within_tolerance, "{kind}");
        assert!(r.rows.iter().all(|r| r.product == kind
            && r.native_result["native_sources"].as_array().unwrap().len() == 4));
        assert!(r.errors.iter().all(|e| e.signed_error == 0.));
    }
}

#[test]
fn currency_units_preserve_non_usd_graph_currency_and_signed_amounts() {
    let mut v = raw();
    let update = |row: &mut Value| {
        let g = &mut row["graph"]["request"];
        g["currency"] = json!("EUR");
        g["scenarios"][0]["collateral_curve"]["currency"] = json!("EUR");
        paths(row, |_, r| r["identity"]["currency"] = json!("EUR"));
    };
    update(&mut v["cohorts"][0]["representative"]);
    for row in v["cohorts"][0]["members"].as_array_mut().unwrap() {
        update(row);
    }
    let r = evaluate(v);
    assert!(r.rows.iter().all(|r| r.currency == "EUR"));
    assert!(r.errors.iter().all(|e| e.currency == "EUR"));
    near(amount(&r, "bank_cash_pv").summed_members, -2004.);
}

#[test]
fn independent_default_and_recovery_cash_tail_is_counted_once_beyond_reporting() {
    let mut base: Value = serde_json::from_str(include_str!(
        "../../../examples/models/advanced-product-graph-1.json"
    ))
    .unwrap();
    base["reporting_ordinals"] = json!([738916, 738945]);
    base["positions"][0]["product"]["histories"][0]["physical"][0]["steps"] =
        json!([{"ordinal":738945,"action":{"kind":"default"}}]);
    let row = |name: &str, m: f64| {
        let mut g = base.clone();
        g["positions"][0]["position_id"] = json!(name);
        g["positions"][0]["product"]["contract"]["identity"]["position_id"] = json!(name);
        json!({"row_id":name,"amount_multiplier":m,"graph":{"kind":"consumer_installment","request":g}})
    };
    let mut labels = vec!["pv".to_owned(), "supplied_history_dv01".to_owned()];
    for name in [
        "gross_physical_contract_interest",
        "physical_principal_cash",
        "physical_interest_cash",
        "physical_fee_cash",
        "physical_default_principal",
        "physical_principal_loss",
    ] {
        labels.push(format!("{name}/0"));
    }
    for name in [
        "source_full_cash/principal",
        "source_full_cash/interest",
        "source_full_cash/fees",
        "source_full_gross_interest",
        "source_full_default/principal",
        "source_full_default/principal_loss",
    ] {
        labels.push(name.into());
    }
    let r = evaluate(
        json!({"schema":graph_cohort::SCHEMA,"source_revision":"synthetic-default-tail-v1","monetary_unit":"currency_units","cohorts":[{"cohort_id":"default-tail","representative":row("rep",2.),"members":[row("m1",1.),row("m2",1.)],"tolerances":labels.into_iter().map(|metric|json!({"metric":metric,"absolute_amount":1e-9,"relative":1e-12})).collect::<Vec<_>>() }]}),
    );
    // Each default is 1000; recovery .5*1000 - 50 = 450 arrives 20 days later.
    near(
        amount(&r, "source_full_default/principal").summed_members,
        2000.,
    );
    near(
        amount(&r, "source_full_default/principal_loss").summed_members,
        1100.,
    );
    near(
        amount(&r, "source_full_cash/principal").summed_members,
        900.,
    );
    near(amount(&r, "physical_principal_cash/0").summed_members, 0.);
    near(amount(&r, "source_full_gross_interest").summed_members, 58.);
    assert!(r.all_metrics_within_tolerance);
}
