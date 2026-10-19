//! Remaining ASU 2025-09 interest-rate items in hedge-accounting-1: a tenor-changing rate election
//! (815-30-35-37I), a forecasted choose-your-rate issuance with a best-estimate change and the
//! issuance documentation update (35-37C through 35-37H), replacement debt (35-37K), and the
//! elected qualitative similarity assessment (815-20-55-23C).
//!
//! Fixtures: scripts/make_hedge_asu_followthrough_examples.py on the reviewed example (4% simple
//! curve from 1 February; February fixes 4%, March 4.5% on `term-1m`, 4.6% on `term-1m-alt`).
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::hedge_accounting::{self as hedge, *};
use serde_json::{json, Value};

fn day(month: u32, d: u32) -> i32 {
    ordinal_from_ymd(2024, month, d).unwrap()
}
fn fixture(name: &str) -> Value {
    let text = match name {
        "tenor" => include_str!("fixtures/hedge-cyr-tenor.json"),
        "issuance" => include_str!("fixtures/hedge-cyr-issuance.json"),
        "replacement" => include_str!("fixtures/hedge-cyr-replacement.json"),
        _ => include_str!("fixtures/hedge-qualitative-similarity.json"),
    };
    serde_json::from_str(text).unwrap()
}
fn run(v: &Value) -> Result<HedgeAccountingResult, String> {
    serde_json::from_value::<HedgeAccountingRequest>(v.clone())
        .map_err(|e| e.to_string())
        .and_then(|r| hedge::evaluate(&r))
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1. + a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
fn df(days: i32) -> f64 {
    1. / (1. + 0.04 * f64::from(days) / 360.)
}
/// Par fixed rate on 1 February over (start, end, rate) periods on the 4% curve.
fn par(periods: &[(i32, i32, f64)]) -> f64 {
    let w: Vec<f64> = periods
        .iter()
        .map(|(s, e, _)| f64::from(e - s) / 360. * df(e - day(2, 1)))
        .collect();
    periods
        .iter()
        .zip(&w)
        .map(|((_, _, r), w)| r * w)
        .sum::<f64>()
        / w.iter().sum::<f64>()
}
fn reclass(r: &HedgeAccountingResult, rid: &str) -> Vec<(i32, f64)> {
    r.intents
        .iter()
        .filter(|i| {
            i.event_id.starts_with("cash_flow_hedge_reclassification")
                && i.relationship_id.as_deref() == Some(rid)
        })
        .map(|i| (i.ordinal, i.postings[0].debit_amount))
        .collect()
}
fn record<'a>(r: &'a HedgeAccountingResult, kind: &str) -> &'a LifecycleRecord {
    r.lifecycle.iter().find(|l| l.kind == kind).unwrap()
}
fn cyr(v: &mut Value) -> &mut Value {
    &mut v["forecast_exposures"][0]["choose_your_rate"]
}
fn events(v: &mut Value) -> &mut Vec<Value> {
    v["events"].as_array_mut().unwrap()
}
fn rejects(mut v: Value, change: impl Fn(&mut Value), label: &str) {
    change(&mut v);
    assert!(run(&v).is_err(), "{label} must reject");
}

#[test]
fn tenor_change_moves_the_hedged_payments_to_the_elected_schedule() {
    let v = fixture("tenor");
    let r = run(&v).unwrap();
    let (feb1, mar1, mar15, apr1) = (day(2, 1), day(3, 1), day(3, 15), day(4, 1));
    let k = par(&[(feb1, mar1, 0.04), (mar1, apr1, 0.045)]);
    let k_w = par(&[
        (feb1, mar1, 0.04),
        (mar1, mar15, 0.045),
        (mar15, apr1, 0.045),
    ]);
    let rec = record(&r, "rate_election");
    assert_eq!(
        (rec.index_id.as_deref(), rec.reason.as_deref()),
        (Some("term-2w"), Some("qualified"))
    );
    // February pays monthly before the election; March then pays on the two-week schedule.
    let got = reclass(&r, "cfh-1");
    assert_eq!(
        got.iter().map(|x| x.0).collect::<Vec<_>>(),
        vec![mar1, mar15, apr1]
    );
    close(got[0].1, (0.04 - k) * 100. * 29. / 360.);
    close(got[1].1, (0.045 - k_w) * 100. * 14. / 360.);
    close(got[2].1, (0.045 - k_w) * 100. * 17. / 360.);
    let closing = r
        .relationships
        .iter()
        .find(|c| c.relationship_id == "cfh-1")
        .unwrap();
    assert!(closing.pending_reclassification_periods.is_empty());
    rejects(
        v.clone(),
        |v| cyr(v)["option_schedules"][0]["index_id"] = json!("term-1m"),
        "schedule on the initial index",
    );
    rejects(
        v.clone(),
        |v| cyr(v)["option_schedules"][0]["index_id"] = json!("term-9m"),
        "undocumented schedule",
    );
    rejects(
        v.clone(),
        |v| {
            cyr(v)["option_schedules"] = json!([]);
        },
        "hypothetical off the exposure periods",
    );
    rejects(
        v.clone(),
        |v| {
            v["policy"]["forecast_timing"] =
                json!({"identity": {"id": "t", "revision": "r1"}, "additional_delay_days": 30});
            v["events"].as_array_mut().unwrap().push(json!({"event_id": "delay", "relationship_id": "cfh-1",
            "ordinal": day(3, 1), "sequence": 3, "kind": "forecast_status", "review": {"id": "d", "revision": "r1"},
            "status": "delayed", "period_id": "e3b", "new_payment_ordinal": day(4, 1)}));
        },
        "period status with tenor schedules",
    );
}

#[test]
fn forecasted_issuance_best_estimate_change_and_documentation_update() {
    let v = fixture("issuance");
    let r = run(&v).unwrap();
    let best = record(&r, "best_estimate_change");
    assert_eq!(
        (
            best.ordinal,
            best.index_id.as_deref(),
            best.reason.as_deref()
        ),
        (day(2, 1), Some("term-1m-alt"), Some("qualified"))
    );
    let issued = record(&r, "debt_issued");
    assert_eq!(
        (
            issued.ordinal,
            issued.index_id.as_deref(),
            issued.reason.as_deref()
        ),
        (
            day(3, 1),
            Some("term-1m-alt"),
            Some("documentation_updated")
        )
    );
    assert_eq!(issued.status_after, Status::Qualified);
    // The issued debt accrues at the alternative March rate: the hypothetical settles 4.6% against 6%.
    let got = reclass(&r, "cfh-fwd");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, day(4, 1));
    close(got[0].1, (0.046 - 0.06) * 100. * 31. / 360.);
    rejects(
        v.clone(),
        |v| {
            events(v).retain(|e| e["kind"] != "debt_issued");
        },
        "issuance never recorded",
    );
    rejects(
        v.clone(),
        |v| {
            for e in events(v) {
                if e["kind"] == "debt_issued" {
                    e["index_id"] = json!("term-9m");
                }
            }
        },
        "undocumented issued index",
    );
    rejects(
        v.clone(),
        |v| {
            for e in events(v) {
                if e["kind"] == "debt_issued" {
                    e["relationship_id"] = json!("cfh-1");
                }
            }
        },
        "issuance on a plain hedge",
    );
    rejects(
        v.clone(),
        |v| {
            for e in events(v) {
                if e["kind"] == "debt_issued" {
                    e["kind"] = json!("debt_replacement");
                }
            }
        },
        "replacement before issuance",
    );
    rejects(
        v.clone(),
        |v| {
            v["forecast_exposures"][1]["choose_your_rate"]
                .as_object_mut()
                .unwrap()
                .remove("forecasted_issuance");
        },
        "issuance of existing debt",
    );
}

#[test]
fn replacement_debt_at_a_documented_option_continues_the_hedge() {
    let v = fixture("replacement");
    let r = run(&v).unwrap();
    let (feb1, mar1, apr1) = (day(2, 1), day(3, 1), day(4, 1));
    let k_alt = par(&[(feb1, mar1, 0.04), (mar1, apr1, 0.046)]);
    let rec = record(&r, "debt_replacement");
    assert_eq!(
        (
            rec.index_id.as_deref(),
            rec.reason.as_deref(),
            rec.status_after
        ),
        (Some("term-1m-alt"), Some("qualified"), Status::Qualified)
    );
    let got = reclass(&r, "cfh-1");
    close(got.last().unwrap().1, (0.046 - k_alt) * 100. * 31. / 360.);
    rejects(
        v.clone(),
        |v| {
            v["events"].as_array_mut().unwrap().last_mut().unwrap()["index_id"] = json!("fixed");
        },
        "undocumented replacement rate",
    );
    rejects(
        v.clone(),
        |v| {
            v["events"].as_array_mut().unwrap().last_mut().unwrap()["relationship_id"] =
                json!("fvh-1");
        },
        "replacement on a fair-value hedge",
    );
}

#[test]
fn qualitative_similarity_follows_an_initial_quantitative_pass() {
    let v = fixture("qualitative");
    let r = run(&v).unwrap();
    let group: Vec<&Assessment> = r
        .assessments
        .iter()
        .filter(|a| a.relationship_id == "cfh-1")
        .collect();
    assert_eq!(group.len(), 2);
    let first = group[0].similar_risk.as_ref().unwrap();
    assert!(first.similar && first.basis.is_none());
    let second = group[1].similar_risk.as_ref().unwrap();
    assert_eq!(second.basis.as_deref(), Some("qualitative"));
    assert!(second.similar && second.members.len() == 2);
    rejects(
        v.clone(),
        |v| {
            v["relationships"][1]["group"]
                .as_object_mut()
                .unwrap()
                .remove("qualitative_similarity_election");
        },
        "support without election",
    );
    rejects(
        v.clone(),
        |v| {
            let support = json!({"id": "s", "revision": "r1"});
            for e in events(v) {
                if e["relationship_id"] == "cfh-1" {
                    e["qualitative_similarity_support"] = support.clone();
                }
            }
        },
        "qualitative before a quantitative pass",
    );
    rejects(
        v.clone(),
        |v| {
            for e in events(v) {
                if e["relationship_id"] == "fvh-1" && e["ordinal"] == json!(day(3, 1)) {
                    e["qualitative_similarity_support"] = json!({"id": "s", "revision": "r1"});
                }
            }
        },
        "support on a fair-value assessment",
    );
}
