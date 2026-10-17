//! ASU 2025-09 Issue 4 (815-20-25-88) in hedge-accounting-1: a swap combined with a written
//! cash-settled European swaption of matching notional designated as one hedging instrument.
//!
//! Fixture: the shared hedge example (daily nodes 1 Jan-1 Apr 2024, 6% simple curve dropping to
//! 4% on 1 February, `swap-cfh` pays the 1 February par rate `k`). The written option `opt-cfh`
//! expires and cash-settles on 1 March on a one-period payer swap (1 March-1 April, notional
//! 100, strike `K`); one terminal state carries the node's projected March rate.
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::hedge_accounting::{self as hedge, *};
use serde_json::{json, Value};

const STRIKE: f64 = 0.0448;
fn day(month: u32, d: u32) -> i32 {
    ordinal_from_ymd(2024, month, d).unwrap()
}
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/hedge-accounting-1.json"
    ))
    .unwrap()
}
fn run(v: &Value) -> Result<HedgeAccountingResult, String> {
    let request: HedgeAccountingRequest = serde_json::from_value(v.clone()).expect("parses");
    hedge::evaluate(&request)
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1. + a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
fn df(rate: f64, days: i32) -> f64 {
    1. / (1. + rate * f64::from(days) / 360.)
}
fn utc(ordinal: i32, hour: i64) -> i64 {
    (i64::from(ordinal) - 719163) * 86400 + hour * 3600
}
fn curve_rate(t: i32) -> f64 {
    if t < day(2, 1) {
        0.06
    } else {
        0.04
    }
}
fn projected(t: i32) -> f64 {
    if t < day(2, 1) {
        0.06
    } else {
        0.045
    }
}
const TAU2: f64 = 29. / 360.;
const TAU3: f64 = 31. / 360.;
fn ident(id: &str) -> Value {
    json!({"id": id, "revision": "r1"})
}
/// Payer swap after-payment clean value at an accrual-free node for fixed `k` (March 4.5%).
fn swap_value(t: i32, k: f64) -> f64 {
    let mut v = 0.;
    if t < day(3, 1) {
        v += (0.04 - k) * 100. * TAU2 * df(0.04, day(3, 1) - t);
    }
    if t < day(4, 1) {
        v += (0.045 - k) * 100. * TAU3 * df(0.04, day(4, 1) - t);
    }
    v
}
/// Written option clean value at node t before 1 March settlement (after: zero).
fn option_value(t: i32) -> f64 {
    let r = curve_rate(t);
    let intrinsic = (projected(t) - STRIKE).max(0.) * 100. * TAU3 * df(r, 31);
    if t < day(3, 1) {
        -intrinsic * df(r, day(3, 1) - t)
    } else {
        0.
    }
}
fn cfh_fixed(v: &Value) -> f64 {
    v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"][0]["trades"][1]
        ["request"]["fixed_rate"]
        .as_f64()
        .unwrap()
}
fn nodes(v: &Value) -> &Vec<Value> {
    v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
        .as_array()
        .unwrap()
}
/// The option request as of `asof`, with the March period taken from the node dated `info`.
fn option_request(v: &Value, asof: i32, info: i32) -> Value {
    let node = nodes(v)
        .iter()
        .find(|n| n["ordinal"].as_i64().unwrap() == i64::from(info))
        .unwrap();
    let swap = &node["trades"][1]["request"];
    let r = curve_rate(info);
    let mut underlying = swap.clone();
    underlying["position_id"] = json!("opt-under");
    underlying["asof_ordinal"] = json!(day(3, 1));
    underlying["valuation_cutoff_unix_seconds"] = json!(utc(day(3, 1), 9));
    underlying["fixed_rate"] = json!(STRIKE);
    underlying["fixed_periods"] = json!([swap["fixed_periods"][2]]);
    underlying["floating_periods"] = json!([swap["floating_periods"][2]]);
    underlying["discount_curve"]["asof_ordinal"] = json!(day(3, 1));
    underlying["discount_curve"]["information_unix_seconds"] = json!(utc(day(3, 1), 9));
    underlying["discount_curve"]["points"] = json!([
        {"ordinal": day(3, 1), "discount_factor": 1.},
        {"ordinal": day(4, 1), "discount_factor": df(r, 31)}
    ]);
    let discount_to_expiry = if asof < day(3, 1) {
        df(r, day(3, 1) - asof)
    } else {
        1.
    };
    json!({"contract": ident("opt-cfh-contract"), "exercise_policy": ident("european-positive-value"),
        "conditional_model": ident("index-model"), "crn": ident("crn"), "position_id": "opt-cfh",
        "scenario_id": "physical", "currency": "USD", "monetary_unit": "currency_units", "measure": "pricing",
        "asof_ordinal": asof, "expiry_ordinal": day(3, 1), "settlement_ordinal": day(3, 1), "tail_end_ordinal": day(4, 1),
        "side": "short", "settlement": "collateralized_cash_price",
        "states": [{"state_id": "single", "probability": 1., "discount_to_expiry": discount_to_expiry,
                    "discount_source": ident("discount"), "underlying": underlying,
                    "delivered_position_id": "opt-under", "netting_set_id": "ns"}]})
}
fn adopt(v: &mut Value) {
    v["policy"]["adoption"]["asu_2025_09"] = json!({"adopted": true, "adoption_ordinal": day(1, 1),
        "elections": {"similar_risk_grouping": false, "selectable_rate_debt": false,
                      "swap_written_option_exception": true, "dual_hedge_basis": false}});
}
fn compound_variant() -> Value {
    let mut v = raw();
    adopt(&mut v);
    let terminal = option_request(&v, day(3, 1), day(3, 1));
    let trades: Vec<Value> = nodes(&v)
        .iter()
        .map(|n| {
            let t = n["ordinal"].as_i64().unwrap() as i32;
            if t < day(3, 1) {
                json!({"kind": "european_swaption", "request": option_request(&v, t, t)})
            } else {
                json!({"kind": "expired_european", "terminal": terminal.clone(), "election": ident("election"),
                       "delivered_underlying": null, "settlement_discount": null})
            }
        })
        .collect();
    let csa = &mut v["derivative"]["derivative"]["operation"]["request"];
    for (n, trade) in csa["paths"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(trades)
    {
        n["trades"].as_array_mut().unwrap().push(trade);
    }
    csa["mapping"].as_array_mut().unwrap().push(
        json!({"position_id": "opt-cfh", "contract": ident("opt-cfh-contract"),
        "entity": "bank", "counterparty": "cp", "currency": "USD", "netting_set_id": "ns",
        "legal_review": ident("legal"), "enforceable_netting": true}),
    );
    v["derivative"]["policy"]["trades"].as_array_mut().unwrap().push(json!({"position_id": "opt-cfh",
        "designation": "undesignated_economic_hedge", "designation_review": ident("designation"),
        "mark_recognition": "clean_value_changes_in_earnings", "coupon_recognition": "settle_derivative_carrying",
        "delivery_review": null}));
    let v0 = option_value(day(1, 1));
    v["derivative"]["opening"]["trades"]
        .as_array_mut()
        .unwrap()
        .push(json!({"position_id": "opt-cfh", "clean_value": v0}));
    let equity = v["derivative"]["opening"]["equity_credit_balance"]
        .as_f64()
        .unwrap();
    v["derivative"]["opening"]["equity_credit_balance"] = json!(equity + v0);
    let k = cfh_fixed(&v);
    let hyp: Vec<Value> = nodes(&v)
        .iter()
        .filter(|n| n["ordinal"].as_i64().unwrap() >= i64::from(day(2, 1)))
        .map(|n| {
            let mut req = n["trades"][1]["request"].clone();
            req["position_id"] = json!("hypothetical");
            req["fixed_rate"] = json!(k);
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    let rel = &mut v["relationships"][1];
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["hypothetical"] = json!({"evidence": ident("hyp-evidence"), "nodes": hyp});
    rel["written_option_position_id"] = json!("opt-cfh");
    v
}

#[test]
fn combined_swap_and_written_option_is_one_designated_instrument() {
    let v = compound_variant();
    let k = cfh_fixed(&v);
    let result = run(&v).unwrap();
    let o1 = option_value(day(2, 1));
    let o2 = -(0.045 - STRIKE) * 100. * TAU3 * df(0.04, 31);
    let feb1 = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(2, 1))
        .unwrap();
    close(feb1.relationships[1].instrument_designated_value, o1);
    let a = &result.assessments[2];
    assert_eq!(
        (a.relationship_id.as_str(), a.ordinal),
        ("cfh-1", day(3, 1))
    );
    let d = swap_value(day(3, 1), k) - o1;
    close(a.cumulative_instrument_change, d);
    close(a.cumulative_hedged_change, swap_value(day(3, 1), k));
    close(a.ratio.unwrap(), d / swap_value(day(3, 1), k));
    assert!(a.qualified && (1.05..=1.15).contains(&a.ratio.unwrap()));
    // The option settled in cash on 1 March; its carrying is zero after payment.
    let mar1 = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(3, 1))
        .unwrap();
    close(
        mar1.relationships[1].instrument_designated_value,
        swap_value(day(3, 1), k),
    );
    assert!(mar1
        .relationships
        .iter()
        .all(|x| x.status == Status::Qualified));
    // Option marks after inception are re-owned into OCI like the swap's.
    let option_marks: Vec<_> = result
        .intents
        .iter()
        .filter(|i| {
            i.event_id.starts_with("clean_mark")
                && i.mapped_position_id.as_deref() == Some("opt-cfh")
                && i.ordinal > day(2, 1)
        })
        .collect();
    assert!(!option_marks.is_empty());
    assert!(option_marks.iter().all(|i| i
        .postings
        .iter()
        .all(|p| p.account != Account::MarkGainLoss)
        && i.relationship_id.as_deref() == Some("cfh-1")));
    let s2 = (0.04 - k) * 100. * TAU2;
    let s3 = (0.045 - k) * 100. * TAU3;
    let closing = &result.closing[1];
    close(closing.oci_recognized, s2 + s3 + o2 - o1);
    close(closing.oci_reclassified, s2 + s3);
    close(closing.oci_closing, o2 - o1);
    assert_eq!(
        result.relationships[1]
            .written_option_position_id
            .as_deref(),
        Some("opt-cfh")
    );
    assert!(result.relationships[0].written_option_position_id.is_none());
    assert!(result.asu_2025_09_swap_written_option_supported);
    for intent in &result.intents {
        let sum: f64 = intent.postings.iter().map(|p| p.debit_amount).sum();
        assert!(sum.abs() < 1e-9, "{}", intent.event_id);
    }
    // Without the combination the same derivative source leaves the option's marks in earnings.
    let mut alone = v.clone();
    alone["relationships"][1]["written_option_position_id"] = Value::Null;
    let alone = run(&alone).unwrap();
    close(alone.assessments[2].ratio.unwrap(), 1.);
    close(alone.closing[1].oci_closing, 0.);
    assert!(alone
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("clean_mark")
            && i.mapped_position_id.as_deref() == Some("opt-cfh"))
        .all(|i| i.relationship_id.is_none()));
}

type Change = Box<dyn Fn(&mut Value)>;
#[test]
fn combination_refusals_without_fallback() {
    let cases: Vec<(&str, Change)> = vec![
        (
            "election off",
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["elections"]
                    ["swap_written_option_exception"] = json!(false)
            }),
        ),
        (
            "before adoption",
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["adoption_ordinal"] = json!(day(3, 1))
            }),
        ),
        (
            "purchased option",
            Box::new(|v| {
                for n in v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
                    .as_array_mut()
                    .unwrap()
                {
                    let t = &mut n["trades"][2];
                    let key = if t["kind"] == json!("expired_european") {
                        "terminal"
                    } else {
                        "request"
                    };
                    t[key]["side"] = json!("long");
                }
                let v0 = v["derivative"]["opening"]["trades"][2]["clean_value"]
                    .as_f64()
                    .unwrap();
                v["derivative"]["opening"]["trades"][2]["clean_value"] = json!(-v0);
                let e = v["derivative"]["opening"]["equity_credit_balance"]
                    .as_f64()
                    .unwrap();
                v["derivative"]["opening"]["equity_credit_balance"] = json!(e - 2. * v0);
            }),
        ),
        (
            "notional mismatch",
            Box::new(|v| {
                for n in v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
                    .as_array_mut()
                    .unwrap()
                {
                    let t = &mut n["trades"][2];
                    let key = if t["kind"] == json!("expired_european") {
                        "terminal"
                    } else {
                        "request"
                    };
                    t[key]["states"][0]["underlying"]["notional"] = json!(50.);
                }
            }),
        ),
        (
            "matched critical terms",
            Box::new(|v| {
                v["relationships"][1]["hypothetical"] = Value::Null;
                v["relationships"][1]["matched_critical_terms_evidence"] =
                    ident("cfh-1-critical-terms");
            }),
        ),
        (
            "option equals swap",
            Box::new(|v| v["relationships"][1]["written_option_position_id"] = json!("swap-cfh")),
        ),
        (
            "unmapped option",
            Box::new(|v| v["relationships"][1]["written_option_position_id"] = json!("opt-other")),
        ),
        (
            "option on the fair-value relationship too",
            Box::new(|v| v["relationships"][0]["written_option_position_id"] = json!("opt-cfh")),
        ),
        (
            "coupon policy",
            Box::new(|v| {
                v["derivative"]["policy"]["trades"][2]["coupon_recognition"] =
                    json!("contractual_income_at_due_with_mark_offset")
            }),
        ),
    ];
    for (label, change) in cases {
        let mut v = compound_variant();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
    hedge::validate_request(&serde_json::from_value(compound_variant()).unwrap()).unwrap();
}
