use portfolio_risk_native::{
    advanced_graph::Role,
    funding_graph::{self, FundingGraph, Request},
};
use serde_json::{json, Value};
const ASOF: i32 = 739617;
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn id(s: &str) -> Value {
    json!({"id":s,"revision":"r1"})
}
fn raw(repo: bool) -> Value {
    let envelope: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let mut known = envelope["request"].clone();
    known["measure"] = json!("contractual");
    known["contracts"][0]["exit"] = json!({"kind":"maturity","rollover":null});
    known["contracts"][0]["collateral"]["margin_steps"] = json!([]);
    if !repo {
        known["collateral_assets"] = json!([]);
        known["contracts"][0]["funding_kind"] = json!("interbank");
        known["contracts"][0]["collateral"] = Value::Null;
    }
    let mut physical = known.clone();
    physical["scenario_id"] = json!("base-run");
    physical["measure"] = json!("physical");
    physical["assumption"] = id("supplied-p");
    let mut q = physical.clone();
    q["measure"] = json!("pricing");
    q["assumption"] = id("supplied-q");
    json!({"schema":"funding-product-graph-1","identity":{"model_revision":"funding-graph-1","input_revision":"input-1","paired_history_source":"supplied-synthetic","paired_history_revision":"r1","randomness_domain":"supplied_paired_histories-1"},"book_id":"funding-book","as_of_ordinal":ASOF,"currency":"USD","reporting_ordinals":[ASOF,ASOF+2],"base_scenario_id":"base","bump_bp":1,"cash_timing":"after_opening_actions","tail_policy":"complete_economic_cash_collateral_tail","known":known,"oas_target":{"kind":"locked","base_oas":-0.05,"source_id":"reviewed-lock","source_revision":"r1"},
        "scenarios":[{"scenario_id":"base","run_id":"base-run","severity":0,"collateral_curve":{"curve_id":"collateral","source_id":"synthetic-hand","source_revision":"r1","currency":"USD","role":{"kind":"collateral","collateral_index":"SOFR"},"times_years":[0,1],"discount_factors":[1,(-0.05f64).exp()],"right_tail":"reject"}}],
        "histories":[{"scenario_id":"base","physical":[{"path_id":"p-independent","weight":1,"request":physical}],"pricing":[{"path_id":"paired-1","weight":1,"request":q}],"pricing_down":[{"path_id":"paired-1","weight":1,"request":q}],"pricing_up":[{"path_id":"paired-1","weight":1,"request":q}]}]})
}
fn req(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn all(v: &mut Value, f: impl Fn(&mut Value)) {
    f(&mut v["known"]);
    for set in v["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for path in set[role].as_array_mut().unwrap() {
                f(&mut path["request"]);
            }
        }
    }
}
#[test]
fn actual_maturity_cash_price_income_and_borrowing_sign() {
    let out = funding_graph::analyze(&req(raw(false))).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 101.);
    close(m.opening_actions.amounts.advance_principal_cash, -100.);
    close(m.physical_periods[0].amounts.interest_cash, 0.);
    close(m.physical_tail.amounts.interest_cash, 1.);
    close(m.physical_tail.amounts.principal_cash, 100.);
    close(
        m.supplied_history_dv01,
        101. * ((0.0001f64 * 10. / 365.).exp() - (-0.0001f64 * 10. / 365.).exp()) / 2.,
    );
    let mut v = raw(false);
    all(&mut v, |r| {
        r["contracts"][0]["direction"] = json!("borrowing")
    });
    let borrow = funding_graph::analyze(&req(v)).unwrap();
    let b = &borrow.metrics[0];
    close(b.pv, -m.pv);
    close(b.supplied_history_dv01, -m.supplied_history_dv01);
    close(b.opening_actions.amounts.advance_principal_cash, 100.);
    close(b.physical_tail.amounts.interest_cash, -1.);
    close(b.physical_tail.amounts.principal_cash, -100.);
}
#[test]
fn positive_magnitude_target_borrowing_and_fixed_anchor_reprice() {
    for direction in ["lending", "borrowing"] {
        let mut v = raw(false);
        all(&mut v, |r| {
            r["contracts"][0]["direction"] = json!(direction)
        });
        v["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":101.*(-0.07f64*10./365.).exp(),"source_id":"target","source_revision":"r1"});
        let graph = FundingGraph::new(&req(v.clone())).unwrap();
        close(graph.base_anchor().oas[0].base_oas, 0.02);
        let mut scenario = v["scenarios"][0].clone();
        scenario["scenario_id"] = json!("stress");
        scenario["run_id"] = json!("stress-run");
        scenario["collateral_curve"]["discount_factors"][1] = json!((-0.15f64).exp());
        v["scenarios"].as_array_mut().unwrap().push(scenario);
        let mut histories = v["histories"][0].clone();
        histories["scenario_id"] = json!("stress");
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            histories[role][0]["request"]["scenario_id"] = json!("stress-run");
        }
        v["histories"].as_array_mut().unwrap().push(histories);
        let out = graph.reprice(&req(v.clone())).unwrap();
        let sign = if direction == "lending" { 1. } else { -1. };
        close(
            out.metrics[1].pv,
            sign * 101. * (-0.17f64 * 10. / 365.).exp(),
        );
        assert_eq!(
            serde_json::to_value(&out.base_anchor).unwrap(),
            serde_json::to_value(graph.base_anchor()).unwrap()
        );
        v["histories"][0]["pricing"][0]["request"]["assumption"]["revision"] = json!("r2");
        assert!(graph
            .reprice(&req(v))
            .err()
            .unwrap()
            .contains("BASE_ANCHOR_CHANGED"));
    }
}
#[test]
fn repo_security_margin_and_release_are_never_extra_pv() {
    let mut v = raw(true);
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let margin = example["request"]["contracts"][0]["collateral"]["margin_steps"].clone();
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        v["histories"][0][role][0]["request"]["contracts"][0]["collateral"]["margin_steps"] =
            margin.clone();
    }
    let out = funding_graph::analyze(&req(v)).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 101.);
    close(
        m.physical_periods[0].amounts.margin_call_value,
        100.2 - 112. * 0.9 * 0.8,
    );
    close(
        m.physical_periods[0].security_movements[0].quantity_delta,
        40.,
    );
    close(m.opening_actions.security_movements[0].quantity_delta, 112.);
    close(m.physical_tail.security_movements[0].quantity_delta, -152.);
    for source in &out.native_sources {
        close(
            source.projection.inventory[0].encumbered_quantity_after_reporting,
            152.,
        );
        close(
            source.projection.inventory[0].encumbered_quantity_at_tail_end,
            0.,
        );
    }
}
#[test]
fn margin_management_outage_retains_pending_call_and_maturity() {
    let mut v = raw(true);
    all(&mut v, |r| {
        r["contracts"][0]["collateral"]["margin_breach"] = json!("report")
    });
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let mut margin = example["request"]["contracts"][0]["collateral"]["margin_steps"].clone();
    margin[0]["management_access"] = json!("unavailable");
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["collateral"]["margin_steps"] =
        margin;
    let out = funding_graph::analyze(&req(v)).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 101.);
    close(m.physical_periods[0].amounts.unmet_margin_value, 19.56);
    assert!(m.physical_periods[0].security_movements.is_empty());
    close(m.physical_tail.security_movements[0].quantity_delta, -112.);
    let p = out
        .native_sources
        .iter()
        .find(|p| p.role == Role::Physical)
        .unwrap();
    assert!(p.projection.margin_calls[0].executed_transfers.is_empty());
    close(
        p.projection.inventory[0].encumbered_quantity_after_reporting,
        112.,
    );
}
fn default_exit() -> Value {
    json!({"kind":"counterparty_default","policy":{"policy":id("reviewed-default"),"ordinal":ASOF+2,"accrued_interest_claim":0.2,"accrued_right_source":id("accrual-state"),"economic_principal_loss":60.,"economic_interest_loss":0.1,"recovery_source":id("complete-recovery"),"complete_principal_recoveries":[{"ordinal":ASOF+20,"amount":40.}],"complete_interest_recoveries":[{"ordinal":ASOF+20,"amount":0.1}],"collateral_realization":null}})
}
#[test]
fn physical_default_recovery_tail_is_separate_from_q_maturity() {
    let mut v = raw(false);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["exit"] = default_exit();
    let out = funding_graph::analyze(&req(v.clone())).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 101.);
    close(m.physical_periods[0].amounts.default_principal, 100.);
    close(m.physical_periods[0].amounts.economic_principal_loss, 60.);
    close(
        m.physical_periods[0].amounts.default_accrued_interest_claim,
        0.2,
    );
    close(m.physical_tail.amounts.recovery_principal_cash, 40.);
    close(m.physical_tail.amounts.recovery_interest_cash, 0.1);
    let p = out
        .native_sources
        .iter()
        .find(|p| p.role == Role::Physical)
        .unwrap();
    close(
        p.projection.contracts[0].gross_default_claim_after_reporting,
        100.,
    );
    assert_eq!(
        p.projection.schedule.events.last().unwrap().payment_ordinal,
        ASOF + 20
    );
    v["histories"][0]["pricing"][0]["request"]["contracts"][0]["exit"] = default_exit();
    assert!(funding_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("Physical"));
}
#[test]
fn repo_default_sale_cost_surplus_cash_is_counted_once() {
    let mut v = raw(true);
    let mut exit = default_exit();
    let p = &mut exit["policy"];
    p["accrued_interest_claim"] = json!(0.4);
    p["economic_principal_loss"] = json!(0.);
    p["economic_interest_loss"] = json!(0.);
    p["complete_principal_recoveries"] = json!([]);
    p["complete_interest_recoveries"] = json!([]);
    p["collateral_realization"] = json!({"policy":id("liquidation"),"ordinal":ASOF+5,"snapshot":{"source":id("liquidation-mark"),"ordinal":ASOF+5,"values":[{"asset_id":"Treasury:A","market_price":1.2,"eligible":true,"haircut":0.9}]},"sale_discount":0.,"sale_cost":4.4,"waterfall":"principal_then_interest","surplus_return_ordinal":ASOF+5});
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["exit"] = exit;
    let out = funding_graph::analyze(&req(v)).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 101.);
    close(m.physical_tail.amounts.recovery_principal_cash, 100.);
    close(m.physical_tail.amounts.recovery_interest_cash, 0.4);
    close(
        m.physical_tail.amounts.collateral_gross_sale_proceeds,
        134.4,
    );
    close(m.physical_tail.amounts.collateral_sale_cost, 4.4);
    close(m.physical_tail.amounts.collateral_surplus_returned, 29.6);
    let p = out
        .native_sources
        .iter()
        .find(|p| p.role == Role::Physical)
        .unwrap();
    assert_eq!(p.projection.schedule.events.len(), 2);
    close(p.projection.inventory[0].disposed_quantity, 112.);
    close(p.projection.inventory[0].remaining_quantity, 88.);
}
#[test]
fn explicit_rollover_never_erases_old_maturity_or_infers_new_funding() {
    let mut v = raw(true);
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let exit = example["request"]["contracts"][0]["exit"].clone();
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        v["histories"][0][role][0]["request"]["contracts"][0]["exit"] = exit.clone();
    }
    let out = funding_graph::analyze(&req(v.clone())).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 102.2);
    close(m.physical_tail.amounts.advance_principal_cash, -60.);
    close(m.physical_tail.amounts.maturity_principal_cash, 160.);
    close(m.physical_tail.amounts.interest_cash, 2.2);
    close(m.physical_tail.amounts.executed_rollover_principal, 60.);
    v["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":102.2,"source_id":"target","source_revision":"r1"});
    assert!(funding_graph::analyze(&req(v.clone()))
        .err()
        .unwrap()
        .contains("signed directional"));
    v["oas_target"] = raw(true)["oas_target"].clone();
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        let roll = &mut v["histories"][0][role][0]["request"]["contracts"][0]["exit"]["rollover"];
        roll["market_access"] = json!("unavailable");
        roll["replacement"] = Value::Null;
    }
    let out = funding_graph::analyze(&req(v)).unwrap();
    close(out.metrics[0].pv, 101.);
    close(
        out.metrics[0]
            .physical_tail
            .amounts
            .rollover_access_shortfall,
        60.,
    );
    close(
        out.metrics[0].physical_tail.amounts.maturity_principal_cash,
        100.,
    );
}
#[test]
fn exact_pairing_known_opening_immutable_sources_and_terminal_admission() {
    let mut v = raw(false);
    v["histories"][0]["pricing_up"][0]["path_id"] = json!("different");
    assert!(funding_graph::analyze(&req(v)).is_err());
    let mut v = raw(false);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["principal"] = json!(99.);
    assert!(funding_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("known opening"));
    let mut v = raw(true);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["collateral"]["initial_snapshot"]
        ["values"][0]["market_price"] = json!(2.);
    assert!(funding_graph::analyze(&req(v)).is_err());
    let mut v = raw(false);
    all(&mut v, |r| r["tail_end_ordinal"] = json!(ASOF + 9));
    assert!(funding_graph::analyze(&req(v)).is_err());
    let mut v = raw(false);
    v["histories"][0]["physical"][0]["weight"] = json!(0.);
    let mut valid = v["histories"][0]["physical"][0].clone();
    valid["path_id"] = json!("valid-p");
    valid["weight"] = json!(1.);
    v["histories"][0]["physical"]
        .as_array_mut()
        .unwrap()
        .push(valid);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["currency"] = json!("EUR");
    assert!(funding_graph::analyze(&req(v)).is_err());
}

#[test]
fn supplied_index_measures_resets_and_paired_rate_risk_are_explicit() {
    let mut v = raw(false);
    let term: Value =
        serde_json::from_str(include_str!("../../../examples/models/term-indexed.json")).unwrap();
    let asof = 738917;
    v["as_of_ordinal"] = json!(asof);
    v["reporting_ordinals"] = json!([asof, asof + 1]);
    all(&mut v, |r| {
        r["asof_ordinal"] = json!(asof);
        r["reporting_end_ordinal"] = json!(asof + 1);
        r["tail_end_ordinal"] = json!(738946);
        r["observed_publication_cutoff_unix_seconds"] = json!(1706774400i64);
        r["contracts"][0]["start_ordinal"] = json!(asof);
        r["contracts"][0]["maturity_ordinal"] = json!(738946);
        r["contracts"][0]["interest_periods"] = term["request"]["periods"].clone();
        r["contracts"][0]["rate_path"] = term["request"]["path"].clone();
    });
    let mut original = v.clone();
    for (role, rate) in [
        ("physical", 0.04),
        ("pricing", 0.02),
        ("pricing_down", 0.01),
        ("pricing_up", 0.03),
    ] {
        let o = &mut v["histories"][0][role][0]["request"]["contracts"][0]["interest_periods"][0]
            ["coupon"]["observations"][0];
        o["rate"] = json!(rate);
        o["source"]["measure"] = json!(if role == "physical" {
            "physical"
        } else {
            "pricing"
        });
    }
    let out = funding_graph::analyze(&req(v.clone())).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 100. + 100. * 0.03 * 29. / 360.);
    close(
        m.physical_tail.amounts.interest_cash,
        100. * 0.05 * 29. / 360.,
    );
    let down = (100. + 100. * 0.02 * 29. / 360.) * (0.0001f64 * 29. / 365.).exp();
    let up = (100. + 100. * 0.04 * 29. / 360.) * (-0.0001f64 * 29. / 365.).exp();
    close(m.supplied_history_dv01, (down - up) / 2.);
    assert!(!out.native_sources[0].projection.contracts[0].rate_calendar_mapping_verified);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["interest_periods"][0]["coupon"]
        ["observations"][0]["source"]["measure"] = json!("pricing");
    assert!(funding_graph::analyze(&req(v)).is_err());
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        original["histories"][0][role][0]["request"]["contracts"][0]["interest_periods"][0]
            ["coupon"]["observations"][0]["source"]["information_unix_seconds"] =
            json!(1706778001i64);
    }
    assert!(funding_graph::analyze(&req(original)).is_err());
}

#[test]
fn work_budget_is_charged_before_large_margin_projection() {
    let mut v = raw(true);
    let base_asset = v["known"]["collateral_assets"][0].clone();
    let mut assets = Vec::new();
    for n in 0..512 {
        let mut a = base_asset.clone();
        a["asset_id"] = json!(if n == 0 {
            "Treasury:A".into()
        } else {
            format!("Treasury:{n}")
        });
        assets.push(a);
    }
    all(&mut v, |r| {
        r["collateral_assets"] = Value::Array(assets.clone())
    });
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let step = example["request"]["contracts"][0]["collateral"]["margin_steps"][0].clone();
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["collateral"]["margin_steps"] =
        Value::Array(vec![step; 4096]);
    assert!(funding_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("before projection"));
}

#[test]
fn signed_coupon_fees_and_existing_opening_anchor_remain_exact() {
    let mut v = raw(false);
    all(&mut v, |r| {
        r["contracts"][0]["status"] = json!("opening");
        r["contracts"][0]["start_ordinal"] = json!(ASOF - 5);
        r["contracts"][0]["interest_periods"][0]["start_ordinal"] = json!(ASOF - 5);
        r["contracts"][0]["interest_periods"][0]["coupon"]["annual_rate"] = json!(-0.36);
        r["contracts"][0]["interest_periods"][0]["fee"] = json!(0.5);
    });
    let out = funding_graph::analyze(&req(v)).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 99.);
    close(m.physical_tail.amounts.interest_cash, -1.5);
    close(m.physical_tail.amounts.fee_cash, 0.5);
    close(m.opening_actions.amounts.advance_principal_cash, 0.);
    assert_eq!(
        out.native_sources[0].request.contracts[0].interest_periods[0].start_ordinal,
        ASOF - 5
    );
}

#[test]
fn colliding_immutable_future_snapshot_revision_rejects_across_histories() {
    let mut v = raw(true);
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let mut margin = example["request"]["contracts"][0]["collateral"]["margin_steps"].clone();
    v["histories"][0]["pricing"][0]["request"]["contracts"][0]["collateral"]["margin_steps"] =
        margin.clone();
    margin[0]["snapshot"]["values"][0]["market_price"] = json!(1.);
    v["histories"][0]["physical"][0]["request"]["contracts"][0]["collateral"]["margin_steps"] =
        margin;
    assert!(funding_graph::analyze(&req(v))
        .err()
        .unwrap()
        .contains("immutable funding source"));
}

#[test]
fn weighted_physical_domains_and_reporting_edit_preserve_full_anchor() {
    let mut v = raw(false);
    let mut alt = v["histories"][0]["physical"][0].clone();
    alt["path_id"] = json!("default-p");
    alt["weight"] = json!(0.25);
    alt["request"]["contracts"][0]["exit"] = default_exit();
    v["histories"][0]["physical"][0]["weight"] = json!(0.75);
    v["histories"][0]["physical"]
        .as_array_mut()
        .unwrap()
        .push(alt);
    let graph = FundingGraph::new(&req(v.clone())).unwrap();
    let m = &graph.snapshot().metrics[0];
    close(m.pv, 101.);
    close(m.physical_periods[0].amounts.default_principal, 25.);
    close(m.physical_tail.amounts.principal_cash, 85.);
    close(m.physical_tail.amounts.interest_cash, 0.775);
    v["reporting_ordinals"] = json!([ASOF, ASOF + 10]);
    all(&mut v, |r| r["reporting_end_ordinal"] = json!(ASOF + 10));
    let out = graph.reprice(&req(v)).unwrap();
    assert_eq!(
        serde_json::to_value(graph.base_anchor()).unwrap(),
        serde_json::to_value(&out.base_anchor).unwrap()
    );
    close(
        out.metrics[0].physical_periods[0].amounts.principal_cash,
        75.,
    );
    close(out.metrics[0].physical_tail.amounts.principal_cash, 10.);
}

#[test]
fn checked_in_graph_example_keeps_roll_and_margin_hand_results() {
    let r: Request = serde_json::from_str(include_str!(
        "../../../examples/models/funding-product-graph-1.json"
    ))
    .unwrap();
    let out = funding_graph::analyze(&r).unwrap();
    let m = &out.metrics[0];
    close(m.pv, 102.2);
    close(m.physical_tail.amounts.interest_cash, 2.2);
    close(m.physical_periods[0].amounts.margin_call_value, 19.56);
    assert_eq!(out.native_sources[0].projection.contracts.len(), 2);
}
