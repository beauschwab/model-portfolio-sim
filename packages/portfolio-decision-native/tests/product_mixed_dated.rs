use portfolio_decision_native::product_mixed_dated::{evaluate, run, Request};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-mixed-dated-1.json"
    ))
    .unwrap()
}
fn request(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
#[test]
fn actual_opening_purchase_and_complete_cash_share_one_balanced_gl() {
    let r = request(raw());
    let out = evaluate(&r).unwrap();
    let cost = 100. * r.candidate.product.units[0].quote_price_per_face_unit;
    // Reviewed backbook principal100/coupon1 plus candidate contractual104.45.
    let cash = 1000. - cost + 104.45 + 101.;
    assert!(out.complete && out.protocol == 3);
    assert_eq!(out.full_end_ordinal, 738976);
    assert_eq!(
        out.reporting_end_ordinal,
        out.preparation.reporting_end_ordinal
    );
    assert_eq!(out.acquisition_lineage.len(), 2);
    for scenario in ["baseline", "stress"] {
        let close = out
            .closing_statements
            .iter()
            .find(|x| x["scenario"] == scenario)
            .unwrap();
        near(close["cash"].as_f64().unwrap(), cash);
        near(
            close["equity"].as_f64().unwrap(),
            1100. + 104.45 - cost + 1.,
        );
        near(
            out.journal
                .iter()
                .filter(|j| {
                    j.scenario == scenario && j.event == "opening" && j.gl_account == "cash"
                })
                .map(|j| j.value)
                .sum(),
            1000.,
        );
        let mut tx = BTreeMap::<usize, f64>::new();
        for j in out.journal.iter().filter(|j| j.scenario == scenario) {
            *tx.entry(j.transaction_number).or_default() += j.value;
        }
        assert_eq!(tx.first_key_value().unwrap().0, &1);
        for total in tx.values() {
            near(*total, 0.);
        }
    }
    assert!(!out.independent_persisted_verification && !out.dynamic_mixed_acceptance);
    assert!(out
        .trial_balance
        .iter()
        .filter(|x| matches!(
            x.gl_account.as_str(),
            "asset_principal"
                | "book_adjustment"
                | "dated_interest_receivable"
                | "dated_deferred_income"
        ))
        .all(|x| x.balance.abs() < 1e-9));
}
fn reporting(v: &mut Value, days: i32) {
    let start = 738916;
    let end = start + days;
    v["combined_specification"]["horizon_days"] = json!(days);
    v["backbook"]["specification"]["horizon_days"] = json!(days);
    v["backbook"]["dated"]["reporting_horizon_days"] = json!(days);
    v["candidate"]["product"]["reporting_ordinals"] = json!([start, end]);
    for u in v["candidate"]["product"]["units"].as_array_mut().unwrap() {
        u["purchase_graph"]["request"]["reporting_ordinals"] = json!([start, end]);
        u["age_branches"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["ordinal"] == start);
        for b in u["age_branches"].as_array_mut().unwrap() {
            b["graph"]["request"]["reporting_ordinals"] = json!([start, end]);
        }
    }
}
#[test]
fn actual_day45_report_retains_day60_tail_without_terminal_plug() {
    let mut v = raw();
    reporting(&mut v, 45);
    let out = evaluate(&request(v)).unwrap();
    assert_eq!(out.full_end_ordinal, 738976);
    assert_eq!(out.reporting_end_ordinal, 738961);
    assert_eq!(out.reported_daily_balance.len(), 2);
    assert!(out.reported_daily_balance.iter().all(|v| v["day"] == 45));
    let backbook = out
        .reporting_rights
        .iter()
        .find(|v| v["position_id"] == "backbook-loan" && v["scenario"] == "baseline")
        .unwrap();
    near(backbook["signed_interest_right"].as_f64().unwrap(), 0.75);
    assert!(out
        .journal
        .iter()
        .any(|j| j.day > 45 && j.event == "dated_settlement"));
    assert_eq!(
        out.preparation.backbook_source_ends[0]["original_container_end_ordinal"],
        738976
    );
}
#[test]
fn zero_candidate_allocation_keeps_paths_and_actual_backbook_opening() {
    let mut v = raw();
    v["candidate"]["allocation"][0]["face"] = json!(0.);
    let out = evaluate(&request(v)).unwrap();
    assert_eq!(out.preparation.candidate_capture.paths.len(), 2);
    assert_eq!(out.acquisition_lineage.len(), 2);
    assert!(out
        .acquisition_lineage
        .iter()
        .all(|v| v["principal"] == 0. && v["cost"] == 0.));
    for row in &out.closing_statements {
        near(row["cash"].as_f64().unwrap(), 1101.);
    }
}
#[test]
fn incomplete_gross_backbook_tail_and_unsupported_ownership_fail_before_output() {
    for case in 0..6 {
        let mut v = raw();
        match case {
            0 => {
                v["backbook"]["dated"]["sources"][0]["schedule"]["events"][0]["components"]
                    ["principal"] = json!(99.)
            }
            1 => v["combined_specification"]["accounts"][0]["cash"] = json!(999.),
            2 => v["policy"]["defaults"] = json!("derive"),
            3 => v["combined_specification"]["reverse_severities"] = json!([2.]),
            4 => {
                v["candidate"]["owners"][0]["source"] =
                    json!({"id":"reviewed-backbook","revision":"synthetic-v1"})
            }
            _ => {
                v["backbook"]["dated"]["sources"][0]["schedule"]["horizon_days"] = json!(1081);
            }
        }
        let mut bytes = vec![];
        assert!(run(&request(v), &mut bytes).is_err(), "case {case}");
        assert!(bytes.is_empty());
    }
}
#[test]
fn reviewed_end_of_day_cash_scope_requires_actual_settlement_without_funding_injection() {
    let mut v = raw();
    for key in ["combined_specification", "backbook"] {
        let spec = if key == "backbook" {
            &mut v[key]["specification"]
        } else {
            &mut v[key]
        };
        spec["accounts"][0]["cash"] = json!(10.);
        spec["accounts"][0]["equity"] = json!(110.);
    }
    // Without same-day contractual settlement the actual day0 cash cannot fund
    // the purchase; no replacement borrowing or balancing cash is generated.
    let mut bytes = vec![];
    assert!(run(&request(v.clone()), &mut bytes)
        .unwrap_err()
        .contains("cash floor"));
    assert!(bytes.is_empty());
    // Reviewed scope is end-of-day cash. Early backbook payment actually settles
    // AFTER acquisition on day0; its deferred coupon survives until day60.
    let event = &mut v["backbook"]["dated"]["sources"][0]["schedule"]["events"][0];
    event["payment_ordinal"] = json!(738916);
    event["day_offset"] = json!(0);
    let out = evaluate(&request(v)).unwrap();
    near(
        out.reported_daily_balance[0]["cash"].as_f64().unwrap(),
        111.2506993647357,
    );
    let lines: Vec<_> = out
        .journal
        .iter()
        .filter(|j| j.scenario == "baseline" && j.day == 0 && j.gl_account == "cash")
        .collect();
    assert_eq!(lines.len(), 3);
    near(lines[0].value, 10.);
    near(lines[1].value, -104.19930063526422);
    near(lines[2].value, 101.);
    assert!(out
        .journal
        .iter()
        .all(|j| !j.gl_account.contains("funding")));
}
