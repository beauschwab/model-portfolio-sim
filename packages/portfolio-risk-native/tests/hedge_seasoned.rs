//! Seasoned relationships in hedge-accounting-1: designated before the derivative opening date
//! with reviewed retained cumulative changes and basis/OCI balances posted once against opening
//! equity. Fixture: the shared hedge example with one extra period 1 Dec 2023-1 Jan 2024 paid on
//! 2 January on both swaps, the loan and the forecast exposure.
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::hedge_accounting::{self as hedge, *};
use serde_json::{json, Value};

fn day(month: u32, d: u32) -> i32 {
    ordinal_from_ymd(2024, month, d).unwrap()
}
fn dec1() -> i32 {
    ordinal_from_ymd(2023, 12, 1).unwrap()
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
fn ident(id: &str) -> Value {
    json!({"id": id, "revision": "r1"})
}
const TAU0: f64 = 31. / 360.;
fn curve_rate(t: i32) -> f64 {
    if t < day(2, 1) {
        0.06
    } else {
        0.04
    }
}
fn periods() -> [(i32, i32); 3] {
    [
        (day(1, 1), day(2, 1)),
        (day(2, 1), day(3, 1)),
        (day(3, 1), day(4, 1)),
    ]
}
fn floating_rate(period: usize, t: i32) -> f64 {
    match period {
        0 => 0.065,
        1 => {
            if t < day(2, 1) {
                0.06
            } else {
                0.04
            }
        }
        _ => {
            if t < day(2, 1) {
                0.06
            } else {
                0.045
            }
        }
    }
}
fn item_unit(t: i32) -> f64 {
    let r = curve_rate(t);
    let mut dirty = 0.;
    let mut accrued = 0.;
    for (s, e) in periods() {
        let tau = f64::from(e - s) / 360.;
        if e > t {
            dirty += 0.06 * tau * df(r, e - t);
        }
        if s <= t && t < e {
            accrued += 0.06 * f64::from(t - s) / 360.;
        }
    }
    if day(4, 1) > t {
        dirty += df(r, day(4, 1) - t);
    }
    dirty - accrued
}
/// 1 January after-payment unit value including the fully accrued December period paid on 2 January.
fn item_unit_opening() -> f64 {
    item_unit(day(1, 1)) + 0.06 * TAU0 * (df(0.06, 1) - 1.)
}
fn swap_value(t: i32, fixed: f64) -> f64 {
    let r = curve_rate(t);
    periods()
        .iter()
        .enumerate()
        .filter(|(_, (_, e))| *e > t)
        .map(|(i, (s, e))| {
            (floating_rate(i, t) - fixed) * 100. * f64::from(e - s) / 360. * df(r, e - t)
        })
        .sum()
}
fn cfh_fixed(v: &Value) -> f64 {
    v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"][0]["trades"][1]
        ["request"]["fixed_rate"]
        .as_f64()
        .unwrap()
}
fn december_observation(template: &Value) -> Value {
    let mut o = template.clone();
    o["interest_start_ordinal"] = json!(dec1());
    o["interest_end_ordinal"] = json!(day(1, 1));
    o["observation_start_ordinal"] = json!(dec1());
    o["observation_end_ordinal"] = json!(day(1, 1));
    o["fixing_ordinal"] = json!(dec1());
    o["selection_cutoff_unix_seconds"] = json!(utc(dec1(), 9));
    o["rate"] = json!(0.06);
    o["source"] = json!({"kind": "observed", "fixing": ident("observed-reset-0"), "publication_unix_seconds": utc(dec1(), 8)});
    o
}
fn period0(prefix: &str) -> Value {
    json!({"period_id": format!("{prefix}0"), "start_ordinal": dec1(), "end_ordinal": day(1, 1),
           "payment_ordinal": day(1, 2), "day_count": "act360"})
}
fn seasoned_variant() -> Value {
    let mut v = raw();
    let k = cfh_fixed(&v);
    let nodes = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
        .as_array_mut()
        .unwrap();
    for n in nodes.iter_mut() {
        let ordinal = n["ordinal"].as_i64().unwrap() as i32;
        for t in n["trades"].as_array_mut().unwrap() {
            let s = &mut t["request"];
            s["fixed_periods"]
                .as_array_mut()
                .unwrap()
                .insert(0, period0("f"));
            let mut l0 = s["floating_periods"][0].clone();
            l0["period_id"] = json!("l0");
            l0["start_ordinal"] = json!(dec1());
            l0["end_ordinal"] = json!(day(1, 1));
            l0["payment_ordinal"] = json!(day(1, 2));
            l0["coupon"]["reset_cutoff_unix_seconds"] = json!(utc(dec1(), 9));
            let obs = december_observation(&l0["coupon"]["observations"][0]);
            l0["coupon"]["observations"] = json!([obs]);
            s["floating_periods"].as_array_mut().unwrap().insert(0, l0);
            if ordinal == day(1, 1) {
                s["discount_curve"]["points"]
                    .as_array_mut()
                    .unwrap()
                    .insert(
                        1,
                        json!({"ordinal": day(1, 2), "discount_factor": df(0.06, 1)}),
                    );
            }
        }
    }
    let template = nodes[0]["trades"][0]["request"]["floating_periods"][0]["coupon"]
        ["observations"][0]
        .clone();
    for position in ["swap-fvh", "swap-cfh"] {
        v["derivative"]["physical_index_paths"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"position_id": position, "period_id": "l0", "path": ident("physical-path"),
                         "observations": [december_observation(&template)]}),
            );
    }
    // The derivative owner's after-payment clean value carries the matured unpaid December coupon.
    let c0 = (0.06 - k) * 100. * TAU0 * df(0.06, 1);
    let open = v["derivative"]["opening"]["trades"][1]["clean_value"]
        .as_f64()
        .unwrap()
        + c0;
    v["derivative"]["opening"]["trades"][1]["clean_value"] = json!(open);
    let equity = v["derivative"]["opening"]["equity_credit_balance"]
        .as_f64()
        .unwrap();
    v["derivative"]["opening"]["equity_credit_balance"] = json!(equity + c0);
    v["hedged_items"][0]["periods"]
        .as_array_mut()
        .unwrap()
        .insert(0, period0("p"));
    v["forecast_exposures"][0]["periods"]
        .as_array_mut()
        .unwrap()
        .insert(0, period0("e"));
    v["relationships"][0]["inception_ordinal"] = json!(dec1());
    v["relationships"][0]["opening"] = json!({"evidence": ident("fvh-1-opening"), "hedged_value": item_unit_opening(),
        "cumulative_instrument_change": 0.5, "cumulative_hedged_change": 0.5, "basis_balance": -0.5, "oci_balance": 0.});
    let hyp: Vec<Value> = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]
        ["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let mut req = n["trades"][1]["request"].clone();
            req["position_id"] = json!("hypothetical");
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    let rel = &mut v["relationships"][1];
    rel["inception_ordinal"] = json!(dec1());
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["hypothetical"] = json!({"evidence": ident("hyp-evidence"), "nodes": hyp});
    rel["opening"] = json!({"evidence": ident("cfh-1-opening"), "hedged_value": open,
        "cumulative_instrument_change": 0.3, "cumulative_hedged_change": 0.3, "basis_balance": 0., "oci_balance": 0.2});
    v
}

#[test]
fn seasoned_relationships_post_retained_balances_once_and_continue_measurement() {
    let v = seasoned_variant();
    let result = run(&v).unwrap();
    let kinds: Vec<&str> = result.lifecycle.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "opening",
            "opening",
            "assessment",
            "assessment",
            "assessment"
        ]
    );
    assert!(result.lifecycle[..2]
        .iter()
        .all(|l| l.ordinal == day(1, 1) && l.status_after == Status::Qualified));
    assert!(
        result.intents[0].event_id.starts_with("hedge_opening")
            && result.intents[1].event_id.starts_with("hedge_opening")
    );
    close(result.intents[0].postings[0].debit_amount, -0.5);
    assert_eq!(
        result.intents[0].postings[1].account,
        Account::OpeningEquity
    );
    assert_eq!(
        result.intents[1].postings[0].account,
        Account::CashFlowHedgeOci
    );
    close(result.intents[1].postings[0].debit_amount, -0.2);
    let jan1 = item_unit_opening();
    for (a, t) in result
        .assessments
        .iter()
        .take(2)
        .zip([day(2, 1), day(3, 1)])
    {
        let d = 0.5 + swap_value(t, 0.06) - swap_value(day(1, 1), 0.06);
        let e = 0.5 - 100. * (item_unit(t) - jan1);
        close(a.cumulative_instrument_change, d);
        close(a.cumulative_hedged_change, e);
        close(a.ratio.unwrap(), d / e);
        assert!(a.qualified);
    }
    close(result.assessments[2].ratio.unwrap(), 1.);
    close(result.closing[0].basis_closing, -0.5 + 100. * (1. - jan1));
    close(
        result.closing[0].basis_adjustments,
        result.closing[0].basis_closing,
    );
    let open = v["derivative"]["opening"]["trades"][1]["clean_value"]
        .as_f64()
        .unwrap();
    close(result.closing[1].oci_closing, 0.2 - open);
    let marks: Vec<_> = result
        .intents
        .iter()
        .filter(|i| {
            i.event_id.starts_with("clean_mark")
                && i.mapped_position_id.as_deref() == Some("swap-cfh")
        })
        .collect();
    assert!(!marks.is_empty());
    assert!(marks.iter().all(|i| i
        .postings
        .iter()
        .all(|p| p.account != Account::MarkGainLoss)));
    let reclass: Vec<i32> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("cash_flow_hedge_reclassification"))
        .map(|i| i.ordinal)
        .collect();
    assert_eq!(reclass, [day(1, 2), day(2, 1), day(3, 1), day(4, 1)]);
    assert!(result.relationships.iter().all(|r| r
        .opening_evidence
        .as_ref()
        .is_some_and(|e| e.id.ends_with("-opening"))));
    let first = &result.daily[0].relationships;
    close(first[0].cumulative_hedged_change, 0.5);
    close(first[1].oci_balance, 0.2);
    let sum: f64 = result.daily[0]
        .financial_trial_balance
        .iter()
        .map(|b| b.debit_balance)
        .sum();
    close(sum, 0.);
    assert!(result.seasoned_openings_supported);
    assert!(run(&raw()).unwrap().relationships[0]
        .opening_evidence
        .is_none());
}

type Change = Box<dyn Fn(&mut Value)>;
#[test]
fn seasoned_refusals_without_fallback() {
    let cases: Vec<(&str, Change)> = vec![
        (
            "seasoned without opening",
            Box::new(|v| v["relationships"][0]["opening"] = Value::Null),
        ),
        (
            "opening without seasoning",
            Box::new(|v| v["relationships"][0]["inception_ordinal"] = json!(day(1, 1))),
        ),
        (
            "basis not the signed cumulative change",
            Box::new(|v| v["relationships"][0]["opening"]["basis_balance"] = json!(-0.4)),
        ),
        (
            "cash-flow basis",
            Box::new(|v| v["relationships"][1]["opening"]["basis_balance"] = json!(0.1)),
        ),
        (
            "fair-value OCI",
            Box::new(|v| v["relationships"][0]["opening"]["oci_balance"] = json!(0.1)),
        ),
        (
            "predecessor",
            Box::new(|v| {
                let mut second = v["relationships"][1].clone();
                second["relationship_id"] = json!("cfh-2");
                second["predecessor_relationship_id"] = json!("cfh-1");
                v["relationships"].as_array_mut().unwrap().push(second);
            }),
        ),
        (
            "matched opening value",
            Box::new(|v| {
                v["relationships"][1]["hypothetical"] = Value::Null;
                v["relationships"][1]["matched_critical_terms_evidence"] =
                    ident("cfh-1-critical-terms");
                let h = v["relationships"][1]["opening"]["hedged_value"]
                    .as_f64()
                    .unwrap();
                v["relationships"][1]["opening"]["hedged_value"] = json!(h + 0.01);
            }),
        ),
        (
            "inception boundary",
            Box::new(|v| v["relationships"][0]["inception_ordinal"] = json!(dec1() + 10)),
        ),
        (
            "unknown opening field",
            Box::new(|v| v["relationships"][0]["opening"]["frozen_basis"] = json!(0.)),
        ),
    ];
    for (label, change) in cases {
        let mut v = seasoned_variant();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
    hedge::validate_request(&serde_json::from_value(seasoned_variant()).unwrap()).unwrap();
}

/// Partially discontinued fair-value hedge (scale 0.8, frozen basis tranche started 16 December)
/// and a cash-flow hedge with a frozen OCI tranche released over the remaining payments.
fn frozen_variant() -> Value {
    let mut v = seasoned_variant();
    let dec16 = dec1() + 15;
    let span = f64::from(day(4, 1) - dec16);
    let o = &mut v["relationships"][0]["opening"];
    o["scale"] = json!(0.8);
    o["basis_balance"] = json!(-0.6);
    o["frozen_basis"] =
        json!([{"amount": 0.1, "start_ordinal": dec16, "amortized": 0.1 * 15. / span}]);
    v["relationships"][1]["opening"]["frozen_oci"] = json!([{"amount": 0.05, "released": 0., "entries": [
        {"payment_ordinal": day(2, 1), "weight": 31. / 91.},
        {"payment_ordinal": day(3, 1), "weight": 29. / 91.},
        {"payment_ordinal": day(4, 1), "weight": 31. / 91.}]}]);
    v
}

#[test]
fn frozen_tranches_and_scale_at_the_opening_continue_amortization_and_release() {
    let v = frozen_variant();
    let span = f64::from(day(4, 1) - dec1() - 15);
    let result = run(&v).unwrap();
    let jan1 = item_unit_opening();
    for (a, t) in result
        .assessments
        .iter()
        .take(2)
        .zip([day(2, 1), day(3, 1)])
    {
        let d = 0.5 + 0.8 * (swap_value(t, 0.06) - swap_value(day(1, 1), 0.06));
        let e = 0.5 - 0.8 * 100. * (item_unit(t) - jan1);
        close(a.cumulative_instrument_change, d);
        close(a.cumulative_hedged_change, e);
        assert!(a.qualified);
    }
    close(result.lifecycle[0].frozen_basis, 0.1 - 0.1 * 15. / span);
    close(result.lifecycle[1].frozen_oci, 0.05);
    close(result.lifecycle[0].proportion_after, 0.8);
    close(
        result.intents[0].postings[0].debit_amount,
        -0.6 + 0.1 - 0.1 * 15. / span,
    );
    close(result.intents[1].postings[0].debit_amount, -0.25);
    let first = &result.daily[0].relationships;
    close(first[0].frozen_basis_balance, 0.1 * (span - 16.) / span);
    close(first[0].designated_proportion, 0.8);
    close(first[1].frozen_oci_balance, 0.05);
    close(result.closing[0].basis_amortized, 0.1);
    close(
        result.closing[0].basis_closing,
        -0.6 + 0.8 * 100. * (1. - jan1),
    );
    close(result.relationships[0].frozen_basis_balance, 0.);
    let releases: Vec<&Intent> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("frozen_oci_release"))
        .collect();
    assert_eq!(
        releases.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(2, 1), day(3, 1), day(4, 1)]
    );
    close(releases[0].postings[0].debit_amount, 0.05 * 31. / 91.);
    close(
        releases
            .iter()
            .map(|i| i.postings[0].debit_amount)
            .sum::<f64>(),
        0.05,
    );
    let open = v["derivative"]["opening"]["trades"][1]["clean_value"]
        .as_f64()
        .unwrap();
    close(result.closing[1].oci_closing, 0.2 - open);
    close(result.relationships[1].frozen_oci_balance, 0.);
    for intent in &result.intents {
        let sum: f64 = intent.postings.iter().map(|p| p.debit_amount).sum();
        assert!(sum.abs() < 1e-9, "{}", intent.event_id);
    }
    let cases: Vec<(&str, Change)> = vec![
        (
            "scale",
            Box::new(|v| v["relationships"][0]["opening"]["scale"] = json!(1.5)),
        ),
        (
            "identity",
            Box::new(|v| v["relationships"][0]["opening"]["basis_balance"] = json!(-0.5)),
        ),
        (
            "weights",
            Box::new(|v| {
                v["relationships"][1]["opening"]["frozen_oci"][0]["released"] = json!(0.01)
            }),
        ),
        (
            "early entry",
            Box::new(|v| {
                v["relationships"][1]["opening"]["frozen_oci"][0]["entries"][0]["payment_ordinal"] =
                    json!(dec1())
            }),
        ),
        (
            "foreign entry",
            Box::new(|v| {
                v["relationships"][1]["opening"]["frozen_oci"][0]["entries"][0]["payment_ordinal"] =
                    json!(day(2, 6))
            }),
        ),
        (
            "fair-value OCI tranche",
            Box::new(|v| {
                let t = v["relationships"][1]["opening"]["frozen_oci"].clone();
                v["relationships"][0]["opening"]["frozen_oci"] = t;
            }),
        ),
        (
            "cash-flow basis tranche",
            Box::new(|v| {
                let t = v["relationships"][0]["opening"]["frozen_basis"].clone();
                v["relationships"][1]["opening"]["frozen_basis"] = t;
            }),
        ),
        (
            "late start",
            Box::new(|v| {
                v["relationships"][0]["opening"]["frozen_basis"][0]["start_ordinal"] =
                    json!(day(1, 1))
            }),
        ),
    ];
    for (label, change) in cases {
        let mut v = frozen_variant();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
}
