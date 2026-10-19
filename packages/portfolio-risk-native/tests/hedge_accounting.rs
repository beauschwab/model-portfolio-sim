//! Hand values, lifecycle mechanics and explicit refusals for hedge-accounting-1.
//!
//! Fixture (scripts/make_hedge_accounting_example.py): daily nodes 1 Jan-1 Apr 2024,
//! three monthly ACT/360 periods, 6% simple curve dropping to 4% on 1 February,
//! floating 6.5% (fixed), 4% (fixes 1 Feb), 4.5% (projected 6% before 1 Feb).
//! `swap-fvh` pays 6% against a 7% loan with a 6% benchmark component from 1 Jan;
//! `swap-cfh` pays the 1 February par rate and is a matched cash-flow hedge from 1 Feb.
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::hedge_accounting::{self as hedge, *};
use serde_json::{json, Value};

fn day(month: u32, d: u32) -> i32 {
    ordinal_from_ymd(2024, month, d).unwrap()
}
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/hedge-accounting-1.json"
    ))
    .unwrap()
}
fn request(v: &Value) -> HedgeAccountingRequest {
    serde_json::from_value(v.clone()).expect("example parses")
}
fn run(v: &Value) -> Result<HedgeAccountingResult, String> {
    hedge::evaluate(&request(v))
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
fn curve_rate(t: i32) -> f64 {
    if t < day(2, 1) {
        0.06
    } else {
        0.04
    }
}
/// (start, end) of the three periods; payment at end; principal 1 April.
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
/// Benchmark-component clean value per unit after payments at node t (6% coupon).
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
/// Payer swap after-payment clean value at node t for a fixed rate.
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
fn with_events(mut v: Value, events: Vec<Value>) -> Value {
    v["events"] = Value::Array(events);
    v
}
fn assess(id: &str, ordinal: i32, sequence: u32) -> Value {
    json!({"event_id":format!("assess-{id}-{ordinal}-{sequence}"),"relationship_id":id,"ordinal":ordinal,"sequence":sequence,"kind":"assess","review":{"id":"review","revision":"r1"}})
}
fn balanced(result: &HedgeAccountingResult) {
    for intent in &result.intents {
        let sum: f64 = intent.postings.iter().map(|p| p.debit_amount).sum();
        assert!(sum.abs() < 1e-9, "unbalanced {}", intent.event_id);
        assert!(intent.postings.iter().all(|p| p.debit_amount != 0.));
    }
}

#[test]
fn example_fair_value_and_matched_cash_flow_hedges_reproduce_hand_values() {
    let v = raw();
    let result = run(&v).unwrap();
    balanced(&result);
    assert_eq!(result.daily.len(), 92);
    assert_eq!(result.assessments.len(), 3);
    let jan1 = item_unit(day(1, 1));
    for (a, ordinal) in result
        .assessments
        .iter()
        .take(2)
        .zip([day(2, 1), day(3, 1)])
    {
        assert_eq!((a.relationship_id.as_str(), a.ordinal), ("fvh-1", ordinal));
        let d = swap_value(ordinal, 0.06) - swap_value(day(1, 1), 0.06);
        let e = -100. * (item_unit(ordinal) - jan1);
        close(a.cumulative_instrument_change, d);
        close(a.cumulative_hedged_change, e);
        close(a.ratio.unwrap(), d / e);
        assert!(a.qualified && (0.8..=1.25).contains(&(d / e)), "{}", d / e);
    }
    let cfh = &result.assessments[2];
    let k = cfh_fixed(&v);
    assert_eq!(
        (cfh.relationship_id.as_str(), cfh.ordinal),
        ("cfh-1", day(3, 1))
    );
    close(
        cfh.cumulative_instrument_change,
        swap_value(day(3, 1), k) - swap_value(day(2, 1), k),
    );
    assert!(swap_value(day(2, 1), k).abs() < 1e-12, "par at inception");
    close(cfh.ratio.unwrap(), 1.);
    assert!(cfh.qualified);
    // Closing: the item returns to par so the basis equals the inception premium reversal;
    // the par swap's OCI is fully reclassified by the two hedged settlements.
    let fvh_close = &result.closing[0];
    close(fvh_close.basis_closing, 100. * (1. - jan1));
    close(fvh_close.basis_adjustments, fvh_close.basis_closing);
    close(fvh_close.basis_amortized, 0.);
    let s2 = (0.04 - k) * 100. * 29. / 360.;
    let s3 = (0.045 - k) * 100. * 31. / 360.;
    let cfh_close = &result.closing[1];
    close(cfh_close.oci_recognized, s2 + s3);
    close(cfh_close.oci_reclassified, s2 + s3);
    close(cfh_close.oci_closing, 0.);
    for rel in &result.relationships {
        assert_eq!(rel.status, Status::Qualified);
        assert!(rel.pending_reclassification_periods.is_empty());
    }
    // Re-owned marks: after inception the CFH instrument keeps no mark earnings.
    let cfh_marks: Vec<_> = result
        .intents
        .iter()
        .filter(|i| {
            i.event_id.starts_with("clean_mark")
                && i.mapped_position_id.as_deref() == Some("swap-cfh")
                && i.ordinal > day(2, 1)
        })
        .collect();
    assert!(!cfh_marks.is_empty());
    for intent in &cfh_marks {
        assert!(
            intent
                .postings
                .iter()
                .all(|p| p.account != Account::MarkGainLoss),
            "{}",
            serde_json::to_string(intent).unwrap()
        );
        assert!(intent
            .postings
            .iter()
            .filter(|p| p.account == Account::CashFlowHedgeOci)
            .all(|p| p.relationship_id.as_deref() == Some("cfh-1")));
    }
    let before_inception = result.intents.iter().filter(|i| {
        i.event_id.starts_with("clean_mark")
            && i.mapped_position_id.as_deref() == Some("swap-cfh")
            && i.ordinal <= day(2, 1)
    });
    assert!(
        before_inception.clone().count() > 0
            && before_inception.clone().all(|i| i
                .postings
                .iter()
                .all(|p| p.account != Account::CashFlowHedgeOci))
    );
    let reclass: Vec<_> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("cash_flow_hedge_reclassification"))
        .collect();
    assert_eq!(
        reclass.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(3, 1), day(4, 1)]
    );
    close(
        reclass[0]
            .postings
            .iter()
            .find(|p| p.account == Account::HedgedForecastReclassification)
            .unwrap()
            .debit_amount,
        -s2,
    );
    close(
        reclass[1]
            .postings
            .iter()
            .find(|p| p.account == Account::HedgedForecastReclassification)
            .unwrap()
            .debit_amount,
        -s3,
    );
    let fvh_marks = result.intents.iter().filter(|i| {
        i.event_id.starts_with("clean_mark") && i.mapped_position_id.as_deref() == Some("swap-fvh")
    });
    assert!(
        fvh_marks.clone().count() > 0
            && fvh_marks.clone().all(|i| i
                .postings
                .iter()
                .all(|p| p.account != Account::CashFlowHedgeOci))
    );
    let basis_total: f64 = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("hedged_item_basis:"))
        .flat_map(|i| i.postings.iter())
        .filter(|p| p.account == Account::HedgedItemBasisAdjustment)
        .map(|p| p.debit_amount)
        .sum();
    close(basis_total, fvh_close.basis_closing);
    assert_eq!(
        result
            .lifecycle
            .iter()
            .map(|l| l.kind.as_str())
            .collect::<Vec<_>>(),
        [
            "inception",
            "inception",
            "assessment",
            "assessment",
            "assessment"
        ]
    );
    assert_eq!(result.lifecycle[1].ordinal, day(2, 1));
    let last = result.daily.last().unwrap();
    close(
        last.financial_trial_balance
            .iter()
            .map(|b| b.debit_balance)
            .sum(),
        0.,
    );
    assert!(
        !result.accounting_standard_approval
            && !result.persisted_journal_verified
            && !result.saved_book_adoption
    );
    assert_eq!(
        result.derivative.intents.len()
            + result
                .intents
                .iter()
                .filter(|i| i.relationship_id.is_some() && !i.event_id.starts_with("clean_mark"))
                .count(),
        result.intents.len()
    );
}

#[test]
fn partial_dedesignation_freezes_half_the_basis_and_amortizes_it_straight_line_to_principal() {
    let v = with_events(
        raw(),
        vec![
            assess("fvh-1", day(2, 1), 0),
            json!({"event_id":"split","relationship_id":"fvh-1","ordinal":day(2,1),"sequence":1,"kind":"discontinue","review":{"id":"review","revision":"r1"},"reason":"dedesignation","proportion":0.5}),
        ],
    );
    let result = run(&v).unwrap();
    balanced(&result);
    let basis_feb1 = 100. * (item_unit(day(2, 1)) - item_unit(day(1, 1)));
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "discontinuation")
        .unwrap();
    close(record.frozen_basis, 0.5 * basis_feb1);
    close(record.proportion_after, 0.5);
    assert_eq!(record.status_after, Status::Qualified);
    let mid = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(2, 15))
        .unwrap();
    close(
        mid.relationships[0].frozen_basis_balance,
        0.5 * basis_feb1 * (1. - 14. / 60.),
    );
    close(mid.relationships[0].designated_proportion, 0.5);
    let closing = &result.closing[0];
    close(closing.basis_amortized, 0.5 * basis_feb1);
    close(
        closing.basis_closing,
        0.5 * 100. * (1. - item_unit(day(1, 1))),
    );
    close(
        closing.basis_adjustments - closing.basis_amortized,
        closing.basis_closing,
    );
    assert_eq!(result.relationships[0].status, Status::Qualified);
    close(result.relationships[0].frozen_basis_balance, 0.);
    let sold = with_events(
        raw(),
        vec![
            json!({"event_id":"sold","relationship_id":"fvh-1","ordinal":day(2,1),"sequence":0,"kind":"discontinue","review":{"id":"review","revision":"r1"},"reason":"hedged_item_sold"}),
        ],
    );
    let result = run(&sold).unwrap();
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "discontinuation")
        .unwrap();
    close(record.derecognized_basis, basis_feb1);
    assert_eq!(record.status_after, Status::Discontinued);
    close(result.closing[0].basis_derecognized, basis_feb1);
    close(result.closing[0].basis_closing, 0.);
    assert!(result
        .intents
        .iter()
        .any(|i| i.event_id.starts_with("hedged_item_basis_derecognition")));
    assert!(result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("hedged_item_basis:"))
        .all(|i| i.ordinal <= day(2, 1)));
}

#[test]
fn forecast_not_occurring_releases_oci_and_a_narrow_band_fails_the_assessment() {
    let v = raw();
    let k = cfh_fixed(&v);
    let v = with_events(
        v,
        vec![
            json!({"event_id":"pnto","relationship_id":"cfh-1","ordinal":day(2,10),"sequence":0,"kind":"forecast_status","review":{"id":"review","revision":"r1"},"status":"probable_not_to_occur"}),
        ],
    );
    let result = run(&v).unwrap();
    balanced(&result);
    let oci_feb10 = swap_value(day(2, 10), k) - swap_value(day(2, 1), k);
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "forecast_status")
        .unwrap();
    close(record.released_oci, oci_feb10);
    assert_eq!(record.status_after, Status::Discontinued);
    assert!(result
        .intents
        .iter()
        .any(
            |i| i.event_id.starts_with("forecast_not_occurring_release") && i.ordinal == day(2, 10)
        ));
    assert!(!result
        .intents
        .iter()
        .any(|i| i.event_id.starts_with("cash_flow_hedge_reclassification")));
    close(result.closing[1].oci_closing, 0.);
    close(result.closing[1].oci_recognized, oci_feb10);
    let later = result.intents.iter().filter(|i| {
        i.event_id.starts_with("clean_mark")
            && i.mapped_position_id.as_deref() == Some("swap-cfh")
            && i.ordinal > day(2, 10)
    });
    assert!(
        later.clone().count() > 0
            && later.clone().all(|i| i
                .postings
                .iter()
                .all(|p| p.account != Account::CashFlowHedgeOci))
    );
    let mut narrow = with_events(
        raw(),
        vec![assess("fvh-1", day(2, 1), 0), assess("cfh-1", day(3, 1), 0)],
    );
    narrow["policy"]["assessment"]["lower_bound"] = json!(0.99);
    narrow["policy"]["assessment"]["upper_bound"] = json!(1.01);
    let result = run(&narrow).unwrap();
    assert!(!result.assessments[0].qualified && result.assessments[1].qualified);
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.relationship_id == "fvh-1" && l.kind == "assessment")
        .unwrap();
    assert_eq!(
        (record.reason.as_deref(), record.status_after),
        (Some("failed_assessment"), Status::Discontinued)
    );
    close(
        record.frozen_basis,
        100. * (item_unit(day(2, 1)) - item_unit(day(1, 1))),
    );
    close(result.closing[0].basis_amortized, record.frozen_basis);
    close(result.relationships[0].frozen_basis_balance, 0.);
    assert!(result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("hedged_item_basis:"))
        .all(|i| i.ordinal <= day(2, 1)));
    // No-longer-probable discontinues prospectively but keeps OCI for the hedged periods.
    let v = with_events(
        raw(),
        vec![
            json!({"event_id":"nlp","relationship_id":"cfh-1","ordinal":day(2,10),"sequence":0,"kind":"forecast_status","review":{"id":"review","revision":"r1"},"status":"no_longer_probable"}),
        ],
    );
    let result = run(&v).unwrap();
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "forecast_status")
        .unwrap();
    close(record.frozen_oci, oci_feb10);
    assert_eq!(record.status_after, Status::Discontinued);
    let releases: Vec<_> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("frozen_oci_release"))
        .collect();
    assert_eq!(
        releases.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(3, 1), day(4, 1)]
    );
    close(releases[0].postings[0].debit_amount, oci_feb10 * 29. / 60.);
    close(result.closing[1].oci_closing, 0.);
}

#[test]
fn supplied_hypothetical_matches_matched_terms_and_rejects_off_market_inception() {
    let mut v = raw();
    let nodes = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
        .as_array()
        .unwrap()
        .clone();
    let hyp: Vec<Value> = nodes
        .iter()
        .filter(|n| n["ordinal"].as_i64().unwrap() >= i64::from(day(2, 1)))
        .map(|n| {
            let mut req = n["trades"][1]["request"].clone();
            req["position_id"] = json!("hypothetical");
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    v["relationships"][1]["matched_critical_terms_evidence"] = Value::Null;
    v["relationships"][1]["hypothetical"] =
        json!({"evidence":{"id":"hyp-evidence","revision":"r1"},"nodes":hyp});
    let supplied = run(&v).unwrap();
    let matched = run(&raw()).unwrap();
    close(supplied.assessments[2].ratio.unwrap(), 1.);
    close(
        supplied.closing[1].oci_reclassified,
        matched.closing[1].oci_reclassified,
    );
    close(supplied.closing[1].oci_closing, 0.);
    assert_eq!(supplied.intents.len(), matched.intents.len());
    let mut off = v.clone();
    for n in off["relationships"][1]["hypothetical"]["nodes"]
        .as_array_mut()
        .unwrap()
    {
        n["request"]["fixed_rate"] = json!(0.05);
    }
    assert!(run(&off)
        .unwrap_err()
        .contains("zero clean value at inception"));
    let mut short = v.clone();
    short["relationships"][1]["hypothetical"]["nodes"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(run(&short).unwrap_err().contains("cover every node"));
}

type Change = Box<dyn Fn(&mut Value)>;
#[test]
fn explicit_refusals_without_fallback() {
    let cases: Vec<(&str, Change)> = vec![
        (
            "rule matrix",
            Box::new(|v| v["policy"]["rule_version"] = json!("asc815-2024")),
        ),
        (
            "ASU 2025-09 elections",
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["elections"]["dual_hedge_basis"] =
                    json!(true)
            }),
        ),
        (
            "portfolio-layer",
            Box::new(|v| v["policy"]["adoption"]["asu_2022_01_portfolio_layer"] = json!(true)),
        ),
        (
            "adoption date",
            Box::new(|v| v["policy"]["adoption"]["asu_2025_09"]["adopted"] = json!(true)),
        ),
        (
            "designated proportion",
            Box::new(|v| v["relationships"][0]["designated_proportion"] = json!(1.5)),
        ),
        (
            "whole instrument",
            Box::new(|v| {
                v["relationships"][1]["hedging_instrument_position_id"] = json!("swap-fvh")
            }),
        ),
        (
            "assessment at inception",
            Box::new(|v| v["events"][0]["ordinal"] = json!(day(1, 1))),
        ),
        (
            "final settlement",
            Box::new(|v| v["events"][1]["ordinal"] = json!(day(4, 1))),
        ),
        (
            "accrual-free",
            Box::new(|v| v["events"][0]["ordinal"] = json!(day(1, 20))),
        ),
        (
            "hypothetical or matched",
            Box::new(|v| v["relationships"][1]["matched_critical_terms_evidence"] = Value::Null),
        ),
        (
            "undesignated review",
            Box::new(|v| {
                v["derivative"]["policy"]["trades"][0]["designation"] =
                    json!("undesignated_trading")
            }),
        ),
        (
            "event order",
            Box::new(|v| {
                let e = v["events"].as_array_mut().unwrap();
                e.reverse();
            }),
        ),
        (
            "inception boundary",
            Box::new(|v| v["relationships"][0]["inception_ordinal"] = json!(day(1, 2))),
        ),
        (
            "benchmark rate",
            Box::new(|v| v["relationships"][0]["benchmark_rate"] = Value::Null),
        ),
        (
            "hedged notional",
            Box::new(|v| v["relationships"][0]["hedged_notional"] = json!(150.)),
        ),
        (
            "forecast status on FVH",
            Box::new(
                |v| v["events"][0] = json!({"event_id":"x","relationship_id":"fvh-1","ordinal":day(2,1),"sequence":0,"kind":"forecast_status","review":{"id":"r","revision":"r1"},"status":"no_longer_probable"}),
            ),
        ),
        (
            "band",
            Box::new(|v| v["policy"]["assessment"]["lower_bound"] = json!(1.3)),
        ),
        ("unit", Box::new(|v| v["monetary_unit"] = json!("USD"))),
        (
            "CFH coupon policy",
            Box::new(|v| {
                v["derivative"]["policy"]["trades"][1]["coupon_recognition"] =
                    json!("contractual_income_at_due_with_mark_offset")
            }),
        ),
        (
            "matched notional",
            Box::new(|v| v["forecast_exposures"][0]["notional"] = json!(50.)),
        ),
    ];
    for (label, change) in cases {
        let mut v = raw();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
    // Matched critical terms require an at-market instrument at inception.
    let mut off = raw();
    let v0 = swap_value(day(1, 1), 0.06);
    for n in off["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"]
        .as_array_mut()
        .unwrap()
    {
        n["trades"][1]["request"]["fixed_rate"] = json!(0.06);
    }
    off["derivative"]["opening"]["trades"][1]["clean_value"] = json!(v0);
    off["derivative"]["opening"]["equity_credit_balance"] = json!(100. + 2. * v0);
    assert!(run(&off)
        .unwrap_err()
        .contains("zero instrument fair value at inception"));
    let mut unknown = raw();
    unknown["relationships"][0]["layer"] = json!("portfolio");
    assert!(serde_json::from_value::<HedgeAccountingRequest>(unknown).is_err());
    let v = with_events(
        raw(),
        vec![
            json!({"event_id":"end","relationship_id":"fvh-1","ordinal":day(1,20),"sequence":0,"kind":"discontinue","review":{"id":"r","revision":"r1"},"reason":"instrument_terminated"}),
            assess("fvh-1", day(2, 1), 0),
        ],
    );
    assert!(run(&v).unwrap_err().contains("not qualified"));
    hedge::validate_request(&request(&raw())).unwrap();
}

#[test]
fn protocol_envelope_dispatch_bounds_and_rejections() {
    let envelope = json!({"schema":"hedge-accounting-1","threads":1,"execution_timeout_ms":60000,"request":raw()});
    let (out, ok) = portfolio_risk_native::term_protocol::respond(
        serde_json::to_vec(&envelope).unwrap().as_slice(),
    );
    assert!(ok, "{}", String::from_utf8_lossy(&out));
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["ok"], json!(true));
    assert_eq!(value["result"]["schema"], json!("hedge-accounting-1"));
    assert_eq!(value["result"]["assessments"].as_array().unwrap().len(), 3);
    assert_eq!(
        value["result"]["accounting_standard_approval"],
        json!(false)
    );
    let mut bad = envelope.clone();
    bad["execution_timeout_ms"] = json!(true);
    let (_, ok) =
        portfolio_risk_native::term_protocol::respond(serde_json::to_vec(&bad).unwrap().as_slice());
    assert!(!ok);
    let text = serde_json::to_string(&envelope).unwrap().replace(
        "\"rule_version\":",
        "\"rule_version\":\"x\",\"rule_version\":",
    );
    let (_, ok) = portfolio_risk_native::term_protocol::respond(text.as_bytes());
    assert!(!ok, "duplicate keys must reject");
}

/// Benchmark-component unit value with the principal assumed repaid at the hedged term end.
fn item_unit_partial(t: i32, term_end: i32) -> f64 {
    let r = curve_rate(t);
    let mut dirty = 0.;
    let mut accrued = 0.;
    for (s, e) in periods() {
        if e > term_end {
            continue;
        }
        let tau = f64::from(e - s) / 360.;
        if e > t {
            dirty += 0.06 * tau * df(r, e - t);
        }
        if s <= t && t < e {
            accrued += 0.06 * f64::from(t - s) / 360.;
        }
    }
    if term_end > t {
        dirty += df(r, term_end - t);
    }
    dirty - accrued
}

#[test]
fn partial_term_fair_value_hedge_measures_to_the_assumed_maturity() {
    let mut v = with_events(raw(), vec![assess("fvh-1", day(2, 1), 0)]);
    v["relationships"][0]["hedged_term_end_ordinal"] = json!(day(3, 1));
    let result = run(&v).unwrap();
    let jan1 = item_unit_partial(day(1, 1), day(3, 1));
    let a = &result.assessments[0];
    let d = swap_value(day(2, 1), 0.06) - swap_value(day(1, 1), 0.06);
    let e = -100. * (item_unit_partial(day(2, 1), day(3, 1)) - jan1);
    close(a.cumulative_instrument_change, d);
    close(a.cumulative_hedged_change, e);
    close(a.ratio.unwrap(), d / e);
    assert!(!a.qualified && a.ratio.unwrap() > 1.25);
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "assessment")
        .unwrap();
    let frozen = 100. * (item_unit_partial(day(2, 1), day(3, 1)) - jan1);
    close(record.frozen_basis, frozen);
    let mid = &result
        .daily
        .iter()
        .find(|x| x.ordinal == day(2, 15))
        .unwrap()
        .relationships[0];
    close(mid.frozen_basis_balance, frozen * (1. - 14. / 29.));
    let early = &result
        .daily
        .iter()
        .find(|x| x.ordinal == day(1, 15))
        .unwrap()
        .relationships[0];
    close(
        early.hedged_item_clean_value.unwrap(),
        100. * item_unit_partial(day(1, 15), day(3, 1)),
    );
    let at_end = &result
        .daily
        .iter()
        .find(|x| x.ordinal == day(3, 1))
        .unwrap()
        .relationships[0];
    close(at_end.frozen_basis_balance, 0.);
    close(result.closing[0].basis_amortized, frozen);
    close(result.closing[0].basis_closing, 0.);
    assert_eq!(
        result.relationships[0].hedged_term_end_ordinal,
        Some(day(3, 1))
    );
    assert!(result.relationships[1].hedged_term_end_ordinal.is_none());
    for end in [day(2, 4), day(4, 1), day(1, 1)] {
        let mut bad = v.clone();
        bad["relationships"][0]["hedged_term_end_ordinal"] = json!(end);
        assert!(run(&bad).is_err(), "{end} must reject");
    }
    let mut cfh = v.clone();
    cfh["relationships"][1]["hedged_term_end_ordinal"] = json!(day(3, 1));
    assert!(run(&cfh).is_err());
}

/// `swap-cfh` and an identical `swap-cfh2` jointly designated at half proportion each.
fn combination_variant() -> Value {
    let mut v = raw();
    let csa = &mut v["derivative"]["derivative"]["operation"]["request"];
    for n in csa["paths"][0]["nodes"].as_array_mut().unwrap() {
        let mut second = n["trades"][1].clone();
        second["request"]["position_id"] = json!("swap-cfh2");
        second["request"]["contract"] = json!({"id": "swap-cfh2-contract", "revision": "r1"});
        n["trades"].as_array_mut().unwrap().push(second);
    }
    let mut m = csa["mapping"][1].clone();
    m["position_id"] = json!("swap-cfh2");
    m["contract"] = json!({"id": "swap-cfh2-contract", "revision": "r1"});
    csa["mapping"].as_array_mut().unwrap().push(m);
    let mut review = v["derivative"]["policy"]["trades"][1].clone();
    review["position_id"] = json!("swap-cfh2");
    v["derivative"]["policy"]["trades"]
        .as_array_mut()
        .unwrap()
        .push(review);
    let value = v["derivative"]["opening"]["trades"][1]["clean_value"]
        .as_f64()
        .unwrap();
    v["derivative"]["opening"]["trades"]
        .as_array_mut()
        .unwrap()
        .push(json!({"position_id": "swap-cfh2", "clean_value": value}));
    let equity = v["derivative"]["opening"]["equity_credit_balance"]
        .as_f64()
        .unwrap();
    v["derivative"]["opening"]["equity_credit_balance"] = json!(equity + value);
    let extra: Vec<Value> = v["derivative"]["physical_index_paths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["position_id"] == json!("swap-cfh"))
        .map(|p| {
            let mut q = p.clone();
            q["position_id"] = json!("swap-cfh2");
            q
        })
        .collect();
    v["derivative"]["physical_index_paths"]
        .as_array_mut()
        .unwrap()
        .extend(extra);
    let hyp: Vec<Value> = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]
        ["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["ordinal"].as_i64().unwrap() >= i64::from(day(2, 1)))
        .map(|n| {
            let mut req = n["trades"][1]["request"].clone();
            req["position_id"] = json!("hypothetical");
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    let rel = &mut v["relationships"][1];
    rel["designated_proportion"] = json!(0.5);
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["hypothetical"] =
        json!({"evidence": {"id": "hyp-evidence", "revision": "r1"}, "nodes": hyp});
    rel["additional_instrument_position_ids"] = json!(["swap-cfh2"]);
    v
}

#[test]
fn jointly_designated_swaps_form_one_hedging_instrument() {
    let v = combination_variant();
    let k = cfh_fixed(&v);
    let result = run(&v).unwrap();
    let cfh = &result.assessments[2];
    close(cfh.ratio.unwrap(), 1.);
    assert!(cfh.qualified);
    close(
        cfh.cumulative_instrument_change,
        swap_value(day(3, 1), k) - swap_value(day(2, 1), k),
    );
    let mar1 = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(3, 1))
        .unwrap();
    close(
        mar1.relationships[1].instrument_designated_value,
        swap_value(day(3, 1), k),
    );
    for position in ["swap-cfh", "swap-cfh2"] {
        let marks: Vec<_> = result
            .intents
            .iter()
            .filter(|i| {
                i.event_id.starts_with("clean_mark")
                    && i.mapped_position_id.as_deref() == Some(position)
                    && i.ordinal > day(2, 1)
            })
            .collect();
        assert!(!marks.is_empty());
        for i in marks {
            let oci: f64 = i
                .postings
                .iter()
                .filter(|p| p.account == Account::CashFlowHedgeOci)
                .map(|p| p.debit_amount)
                .sum();
            let kept: f64 = i
                .postings
                .iter()
                .filter(|p| p.account == Account::MarkGainLoss)
                .map(|p| p.debit_amount)
                .sum();
            close(oci, kept);
        }
    }
    close(result.closing[1].oci_closing, 0.);
    assert_eq!(
        result.relationships[1].additional_instrument_position_ids,
        vec!["swap-cfh2".to_string()]
    );
    assert!(result.relationships[0]
        .additional_instrument_position_ids
        .is_empty());
    let cases: Vec<(&str, Change)> = vec![
        (
            "duplicate of the primary",
            Box::new(|v| {
                v["relationships"][1]["additional_instrument_position_ids"] = json!(["swap-cfh"])
            }),
        ),
        (
            "repeated additional",
            Box::new(|v| {
                v["relationships"][1]["additional_instrument_position_ids"] =
                    json!(["swap-cfh2", "swap-cfh2"])
            }),
        ),
        (
            "over-designation",
            Box::new(|v| {
                v["relationships"][1]["designated_proportion"] = json!(0.8);
                let mut second = v["relationships"][1].clone();
                second["relationship_id"] = json!("cfh-2");
                second["designated_proportion"] = json!(0.3);
                v["relationships"].as_array_mut().unwrap().push(second);
            }),
        ),
        (
            "matched critical terms",
            Box::new(|v| {
                v["relationships"][1]["hypothetical"] = Value::Null;
                v["relationships"][1]["matched_critical_terms_evidence"] =
                    json!({"id": "cfh-1-critical-terms", "revision": "r1"});
            }),
        ),
        (
            "unknown instrument",
            Box::new(|v| {
                v["relationships"][1]["additional_instrument_position_ids"] = json!(["swap-other"])
            }),
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
        let mut v = combination_variant();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
}

/// Forward-starting payer swap at 6% on period 3 (projected 6% before February, then 4.5%).
fn forward_value(t: i32) -> f64 {
    let r = curve_rate(t);
    periods()
        .iter()
        .enumerate()
        .filter(|(i, (_, e))| *i >= 2 && *e > t)
        .map(|(i, (s, e))| {
            (floating_rate(i, t) - 0.06) * 100. * f64::from(e - s) / 360. * df(r, e - t)
        })
        .sum()
}
/// Hedge of forecast fixed-rate debt issued 1 March with a forward-starting swap designated
/// 1 January and terminated at issuance.
fn forecast_issuance_variant() -> Value {
    let mut v = raw();
    let csa = &mut v["derivative"]["derivative"]["operation"]["request"];
    for n in csa["paths"][0]["nodes"].as_array_mut().unwrap() {
        let mut fwd = n["trades"][1].clone();
        let r = &mut fwd["request"];
        r["position_id"] = json!("swap-fwd");
        r["contract"] = json!({"id": "swap-fwd-contract", "revision": "r1"});
        r["fixed_rate"] = json!(0.06);
        let fixed = r["fixed_periods"].as_array().unwrap()[2..].to_vec();
        let floating = r["floating_periods"].as_array().unwrap()[2..].to_vec();
        r["fixed_periods"] = Value::Array(fixed);
        r["floating_periods"] = Value::Array(floating);
        n["trades"].as_array_mut().unwrap().push(fwd);
    }
    let mut m = csa["mapping"][1].clone();
    m["position_id"] = json!("swap-fwd");
    m["contract"] = json!({"id": "swap-fwd-contract", "revision": "r1"});
    csa["mapping"].as_array_mut().unwrap().push(m);
    let mut review = v["derivative"]["policy"]["trades"][1].clone();
    review["position_id"] = json!("swap-fwd");
    v["derivative"]["policy"]["trades"]
        .as_array_mut()
        .unwrap()
        .push(review);
    v["derivative"]["opening"]["trades"]
        .as_array_mut()
        .unwrap()
        .push(json!({"position_id": "swap-fwd", "clean_value": 0.}));
    let extra: Vec<Value> = v["derivative"]["physical_index_paths"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["position_id"] == json!("swap-cfh") && p["period_id"] == json!("l3"))
        .map(|p| {
            let mut q = p.clone();
            q["position_id"] = json!("swap-fwd");
            q
        })
        .collect();
    v["derivative"]["physical_index_paths"]
        .as_array_mut()
        .unwrap()
        .extend(extra);
    let mut exposure = v["forecast_exposures"][0].clone();
    exposure["exposure_id"] = json!("issue-1");
    let periods = exposure["periods"].as_array().unwrap()[2..].to_vec();
    exposure["periods"] = Value::Array(periods);
    v["forecast_exposures"]
        .as_array_mut()
        .unwrap()
        .push(exposure);
    let hyp: Vec<Value> = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]
        ["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let mut req = n["trades"][2]["request"].clone();
            req["position_id"] = json!("hypothetical");
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    let mut rel = v["relationships"][1].clone();
    rel["relationship_id"] = json!("cfh-fwd");
    rel["hedging_instrument_position_id"] = json!("swap-fwd");
    rel["forecast_exposure_id"] = json!("issue-1");
    rel["inception_ordinal"] = json!(day(1, 1));
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["hypothetical"] =
        json!({"evidence": {"id": "fwd-evidence", "revision": "r1"}, "nodes": hyp});
    v["relationships"].as_array_mut().unwrap().push(rel);
    v["events"] = json!([
        {"event_id": "assess-fwd", "relationship_id": "cfh-fwd", "ordinal": day(3, 1), "sequence": 0, "kind": "assess", "review": {"id": "review", "revision": "r1"}},
        {"event_id": "issued", "relationship_id": "cfh-fwd", "ordinal": day(3, 1), "sequence": 1, "kind": "discontinue", "review": {"id": "review", "revision": "r1"}, "reason": "instrument_terminated"}
    ]);
    v
}

#[test]
fn forward_starting_hedge_of_forecast_issuance_is_assessed_and_released() {
    let v = forecast_issuance_variant();
    let result = run(&v).unwrap();
    let kinds: Vec<&str> = result.lifecycle.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "inception",
            "inception",
            "inception",
            "assessment",
            "discontinuation"
        ]
    );
    let a = &result.assessments[0];
    assert_eq!(
        (a.relationship_id.as_str(), a.ordinal),
        ("cfh-fwd", day(3, 1))
    );
    assert!(a.qualified);
    close(a.ratio.unwrap(), 1.);
    close(
        a.cumulative_instrument_change,
        forward_value(day(3, 1)) - forward_value(day(1, 1)),
    );
    let oci = forward_value(day(3, 1)) - forward_value(day(1, 1));
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "discontinuation")
        .unwrap();
    close(record.frozen_oci, oci);
    assert_eq!(record.status_after, Status::Discontinued);
    let releases: Vec<&Intent> = result
        .intents
        .iter()
        .filter(|i| {
            i.event_id.starts_with("frozen_oci_release")
                && i.relationship_id.as_deref() == Some("cfh-fwd")
        })
        .collect();
    assert_eq!(
        releases.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(4, 1)]
    );
    close(releases[0].postings[0].debit_amount, oci);
    close(
        releases
            .iter()
            .map(|i| i.postings[0].debit_amount)
            .sum::<f64>(),
        oci,
    );
    close(result.closing[2].oci_recognized, oci);
    close(result.closing[2].oci_closing, 0.);
    let later = result.intents.iter().filter(|i| {
        i.event_id.starts_with("clean_mark")
            && i.mapped_position_id.as_deref() == Some("swap-fwd")
            && i.ordinal > day(3, 1)
    });
    assert!(
        later.clone().count() > 0
            && later.clone().all(|i| i
                .postings
                .iter()
                .all(|p| p.account != Account::CashFlowHedgeOci))
    );
    let mut inside = v.clone();
    inside["relationships"][2]["inception_ordinal"] = json!(day(3, 4));
    assert!(run(&inside).is_err());
    let mut mid = v.clone();
    mid["events"][0]["ordinal"] = json!(day(3, 4));
    assert!(run(&mid).is_err());
}

#[test]
fn delayed_and_cancelled_forecast_periods_follow_the_timing_policy() {
    let mut v = raw();
    let k = cfh_fixed(&v);
    v["policy"]["forecast_timing"] =
        json!({"identity": {"id": "timing-policy", "revision": "r1"}, "additional_delay_days": 62});
    v["events"] = json!([{"event_id": "delay", "relationship_id": "cfh-1", "ordinal": day(2, 10), "sequence": 0, "kind": "forecast_status",
        "review": {"id": "review", "revision": "r1"}, "status": "delayed", "period_id": "e2", "new_payment_ordinal": day(3, 15)}]);
    let result = run(&v).unwrap();
    let s2 = (0.04 - k) * 100. * 29. / 360.;
    let s3 = (0.045 - k) * 100. * 31. / 360.;
    let reclass: Vec<&Intent> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("cash_flow_hedge_reclassification"))
        .collect();
    assert_eq!(
        reclass.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(3, 15), day(4, 1)]
    );
    close(reclass[0].postings[0].debit_amount, s2);
    close(reclass[1].postings[0].debit_amount, s3);
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "forecast_status")
        .unwrap();
    assert_eq!(record.reason.as_deref(), Some("forecast_delayed"));
    assert_eq!(record.period_id.as_deref(), Some("e2"));
    assert_eq!(record.new_payment_ordinal, Some(day(3, 15)));
    assert_eq!(record.status_after, Status::Qualified);
    close(result.closing[1].oci_closing, 0.);
    let mut cancelled = raw();
    cancelled["events"] = json!([{"event_id": "cancel", "relationship_id": "cfh-1", "ordinal": day(2, 10), "sequence": 0, "kind": "forecast_status",
        "review": {"id": "review", "revision": "r1"}, "status": "period_probable_not_to_occur", "period_id": "e3"}]);
    let result = run(&cancelled).unwrap();
    let oci = swap_value(day(2, 10), k) - swap_value(day(2, 1), k);
    let release = oci * 31. / 60.;
    let record = result
        .lifecycle
        .iter()
        .find(|l| l.kind == "forecast_status")
        .unwrap();
    assert_eq!(
        record.reason.as_deref(),
        Some("forecast_period_probable_not_to_occur")
    );
    assert_eq!(record.period_id.as_deref(), Some("e3"));
    assert_eq!(record.status_after, Status::Qualified);
    close(record.released_oci, release);
    let releases: Vec<&Intent> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("forecast_not_occurring_release"))
        .collect();
    assert_eq!(
        releases.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(2, 10)]
    );
    close(releases[0].postings[0].debit_amount, release);
    let reclass: Vec<i32> = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("cash_flow_hedge_reclassification"))
        .map(|i| i.ordinal)
        .collect();
    assert_eq!(reclass, [day(3, 1)]);
    close(result.closing[1].oci_closing, s3 - release);
    assert!(result.relationships[1]
        .pending_reclassification_periods
        .is_empty());
    let cases: Vec<(&str, Value, Change)> = vec![
        (
            "window",
            v.clone(),
            Box::new(|x| x["events"][0]["new_payment_ordinal"] = json!(day(5, 3))),
        ),
        (
            "late",
            v.clone(),
            Box::new(|x| x["events"][0]["ordinal"] = json!(day(3, 1))),
        ),
        (
            "no policy",
            v.clone(),
            Box::new(|x| x["policy"]["forecast_timing"] = Value::Null),
        ),
        (
            "unknown period",
            v.clone(),
            Box::new(|x| x["events"][0]["period_id"] = json!("e9")),
        ),
        (
            "twice",
            cancelled.clone(),
            Box::new(|x| {
                x["events"].as_array_mut().unwrap().push(json!({"event_id": "cancel2", "relationship_id": "cfh-1", "ordinal": day(2, 11), "sequence": 0,
                    "kind": "forecast_status", "review": {"id": "review", "revision": "r1"}, "status": "period_probable_not_to_occur", "period_id": "e3"}));
            }),
        ),
    ];
    for (label, base, change) in cases {
        let mut x = base;
        change(&mut x);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(x)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
}

#[test]
fn hedged_item_maturity_derecognizes_the_remaining_basis_on_the_principal_date() {
    let base = run(&raw()).unwrap();
    let before = base
        .closing
        .iter()
        .find(|c| c.relationship_id == "fvh-1")
        .unwrap()
        .basis_closing;
    assert!(before.abs() > 1e-6);
    let mut events = raw()["events"].as_array().unwrap().clone();
    let matured = json!({"event_id":"matured-fvh-1","relationship_id":"fvh-1","ordinal":day(4, 1),"sequence":0,
        "kind":"discontinue","review":{"id":"maturity","revision":"r1"},"reason":"hedged_item_matured"});
    events.push(matured.clone());
    let result = run(&with_events(raw(), events.clone())).unwrap();
    balanced(&result);
    let fvh = result
        .closing
        .iter()
        .find(|c| c.relationship_id == "fvh-1")
        .unwrap();
    close(fvh.basis_closing, 0.);
    close(fvh.basis_derecognized, before);
    let derecognized: f64 = result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("hedged_item_basis_derecognition:"))
        .flat_map(|i| &i.postings)
        .filter(|p| p.account == Account::HedgedItemBasisDerecognition)
        .map(|p| p.debit_amount)
        .sum();
    close(derecognized, before);
    // Refusals: a cash-flow hedge, a date other than the principal date and a partial proportion.
    for change in [
        json!({"relationship_id":"cfh-1"}),
        json!({"ordinal": day(3, 1), "sequence": 2}),
        json!({"proportion": 0.5}),
    ] {
        let mut e = events.clone();
        let last = e.last_mut().unwrap();
        for (k, v) in change.as_object().unwrap() {
            last[k] = v.clone();
        }
        assert!(run(&with_events(raw(), e)).is_err(), "{change} must reject");
    }
}
