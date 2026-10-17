//! ASU 2025-09 elections in hedge-accounting-1: similar-risk groups (815-20-55-23A through
//! 55-23D) and choose-your-rate debt (815-30-35-37I through 35-37M), on the shared fixture.
//!
//! Fixture (scripts/make_hedge_accounting_example.py): daily nodes 1 Jan-1 Apr 2024, three
//! monthly ACT/360 periods, 4% simple curve from 1 February, `swap-cfh` pays the 1 February par
//! rate `k` on term-1m (4% fixed for February, 4.5% for March). The alternative index
//! `term-1m-alt` is the same exposure with a 4.6% March fixing and its own par rate `k'`.
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
const TAU2: f64 = 29. / 360.;
const TAU3: f64 = 31. / 360.;
/// Par fixed rate on 1 February for a 4% February fixing and `march` for March.
fn par(march: f64) -> f64 {
    let (w2, w3) = (TAU2 * df(0.04, 29), TAU3 * df(0.04, 60));
    (0.04 * w2 + march * w3) / (w2 + w3)
}
/// Payer after-payment clean value at an accrual-free node for fixed `k` and March rate `march`.
fn value(t: i32, notional: f64, k: f64, march: f64) -> f64 {
    let mut v = 0.;
    if t < day(3, 1) {
        v += (0.04 - k) * notional * TAU2 * df(0.04, day(3, 1) - t);
    }
    if t < day(4, 1) {
        v += (march - k) * notional * TAU3 * df(0.04, day(4, 1) - t);
    }
    v
}
fn settlement(period: usize, notional: f64, k: f64, march: f64) -> f64 {
    if period == 2 {
        (0.04 - k) * notional * TAU2
    } else {
        (march - k) * notional * TAU3
    }
}
fn cfh_fixed(v: &Value) -> f64 {
    v["derivative"]["derivative"]["operation"]["request"]["paths"][0]["nodes"][0]["trades"][1]
        ["request"]["fixed_rate"]
        .as_f64()
        .unwrap()
}
/// Hypothetical nodes from 1 February mirroring the exposure on `index` with `notional`.
fn hypothetical(v: &Value, id: &str, index: &str, notional: f64, fixed: f64, march: f64) -> Value {
    let nodes: Vec<Value> = v["derivative"]["derivative"]["operation"]["request"]["paths"][0]
        ["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["ordinal"].as_i64().unwrap() >= i64::from(day(2, 1)))
        .map(|n| {
            let mut req = n["trades"][1]["request"].clone();
            req["position_id"] = json!(id);
            req["notional"] = json!(notional);
            req["fixed_rate"] = json!(fixed);
            for p in req["floating_periods"].as_array_mut().unwrap() {
                p["coupon"]["index"]["index_id"] = json!(index);
                if p["period_id"] == json!("l3") {
                    p["coupon"]["observations"][0]["rate"] = json!(march);
                }
            }
            json!({"ordinal": n["ordinal"], "request": req})
        })
        .collect();
    json!({"evidence": {"id": format!("{id}-evidence"), "revision": "r1"}, "nodes": nodes})
}
fn adopt(v: &mut Value, election: &str) {
    v["policy"]["adoption"]["asu_2025_09"] = json!({"adopted": true, "adoption_ordinal": day(1, 1),
        "elections": {"similar_risk_grouping": election == "similar_risk_grouping",
                      "selectable_rate_debt": election == "selectable_rate_debt",
                      "swap_written_option_exception": false, "dual_hedge_basis": false}});
}
fn group_variant(method: &str) -> Value {
    let mut v = raw();
    adopt(&mut v, "similar_risk_grouping");
    let k = cfh_fixed(&v);
    v["forecast_exposures"][0]["notional"] = json!(60.);
    let mut second = v["forecast_exposures"][0].clone();
    second["exposure_id"] = json!("float-2");
    second["notional"] = json!(40.);
    second["index_id"] = json!("term-1m-alt");
    v["forecast_exposures"].as_array_mut().unwrap().push(second);
    let group = json!({"member_exposure_ids": ["float-1", "float-2"], "similar_risk_method": method,
        "hypotheticals": [
            {"exposure_id": "float-1", "hypothetical": hypothetical(&v, "hyp-1", "term-1m", 60., k, 0.045)},
            {"exposure_id": "float-2", "hypothetical": hypothetical(&v, "hyp-2", "term-1m-alt", 40., par(0.046), 0.046)}]});
    let rel = &mut v["relationships"][1];
    rel["forecast_exposure_id"] = Value::Null;
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["group"] = group;
    v
}
fn rate_variant(base_march: f64) -> Value {
    let mut v = raw();
    adopt(&mut v, "selectable_rate_debt");
    let k = if base_march == 0.045 {
        cfh_fixed(&v)
    } else {
        par(base_march)
    };
    v["forecast_exposures"][0]["choose_your_rate"] = json!({"evidence": {"id": "cyr-terms", "revision": "r1"},
        "documented_index_ids": ["term-1m", "term-1m-alt"], "debt_classification": "liability"});
    let base = hypothetical(&v, "hyp-base", "term-1m", 100., k, base_march);
    let alt = json!([{"index_id": "term-1m-alt",
        "hypothetical": hypothetical(&v, "hyp-alt", "term-1m-alt", 100., par(0.046), 0.046)}]);
    let rel = &mut v["relationships"][1];
    rel["matched_critical_terms_evidence"] = Value::Null;
    rel["hypothetical"] = base;
    rel["rate_hypotheticals"] = alt;
    let events = v["events"].as_array_mut().unwrap();
    events.pop();
    events.push(json!({"event_id": "elect", "relationship_id": "cfh-1", "ordinal": day(3, 1), "sequence": 1,
        "kind": "rate_election", "review": {"id": "election-review", "revision": "r1"}, "index_id": "term-1m-alt"}));
    v
}
fn record<'a>(result: &'a HedgeAccountingResult, kind: &str) -> &'a LifecycleRecord {
    result
        .lifecycle
        .iter()
        .find(|l| l.kind == kind && l.relationship_id == "cfh-1")
        .unwrap()
}
fn intents<'a>(result: &'a HedgeAccountingResult, kind: &str) -> Vec<&'a Intent> {
    result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with(kind))
        .collect()
}

#[test]
fn similar_risk_group_assesses_each_member_and_reclassifies_per_member() {
    let v = group_variant("instrument_highly_effective_each_risk");
    let k = cfh_fixed(&v);
    let kp = par(0.046);
    let result = run(&v).unwrap();
    let a = &result.assessments[2];
    assert_eq!(
        (a.relationship_id.as_str(), a.ordinal),
        ("cfh-1", day(3, 1))
    );
    let d = value(day(3, 1), 100., k, 0.045);
    let e1 = value(day(3, 1), 60., k, 0.045);
    let e2 = value(day(3, 1), 40., kp, 0.046);
    close(a.cumulative_instrument_change, d);
    close(a.cumulative_hedged_change, e1 + e2);
    close(a.ratio.unwrap(), d / (e1 + e2));
    let sr = a.similar_risk.as_ref().unwrap();
    assert!(sr.similar && a.qualified);
    assert_eq!(
        sr.method,
        SimilarRiskMethod::InstrumentHighlyEffectiveEachRisk
    );
    close(sr.members[0].ratio.unwrap(), 1.);
    close(sr.members[1].ratio.unwrap(), 0.4 * d / e2);
    close(sr.members[1].cumulative_hedged_change, e2);
    assert!((0.8..=0.9).contains(&sr.members[1].ratio.unwrap()));
    assert!(sr.members.iter().all(|m| m.similar));
    // Method (b) compares per-unit changes: member 2 against member 1.
    let b = run(&group_variant("hedged_risks_similar_to_each_other")).unwrap();
    let sb = b.assessments[2].similar_risk.as_ref().unwrap();
    close(sb.members[1].ratio.unwrap(), (e2 / 40.) / (e1 / 60.));
    assert!(sb.similar && (1.1..=1.25).contains(&sb.members[1].ratio.unwrap()));
    // Reclassification per member at each hedged payment date; OCI keeps the residual offset.
    let reclass = intents(&result, "cash_flow_hedge_reclassification");
    let amounts: Vec<(i32, String, f64)> = reclass
        .iter()
        .map(|i| {
            (
                i.ordinal,
                i.contract_position_id.clone().unwrap(),
                i.postings[0].debit_amount,
            )
        })
        .collect();
    let expected = [
        (day(3, 1), "float-1", settlement(2, 60., k, 0.045)),
        (day(3, 1), "float-2", settlement(2, 40., kp, 0.046)),
        (day(4, 1), "float-1", settlement(3, 60., k, 0.045)),
        (day(4, 1), "float-2", settlement(3, 40., kp, 0.046)),
    ];
    assert_eq!(amounts.len(), 4);
    for ((o, id, amt), (eo, eid, eamt)) in amounts.iter().zip(expected.iter()) {
        assert_eq!((o, id.as_str()), (eo, *eid));
        close(*amt, *eamt);
    }
    let s2 = settlement(2, 100., k, 0.045);
    let s3 = settlement(3, 100., k, 0.045);
    let closing = &result.closing[1];
    close(closing.oci_recognized, s2 + s3);
    close(
        closing.oci_reclassified,
        expected.iter().map(|x| x.2).sum::<f64>(),
    );
    close(
        closing.oci_closing,
        closing.oci_recognized - closing.oci_reclassified,
    );
    assert_eq!(
        result.relationships[1].member_exposure_ids,
        Some(vec!["float-1".into(), "float-2".into()])
    );
    let mar1 = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(3, 1))
        .unwrap();
    let members = mar1.relationships[1].members.as_ref().unwrap();
    assert_eq!(members[1].index_id, "term-1m-alt");
    close(members[1].hypothetical_value, e2);
    close(members[0].cumulative_hedged_change, e1);
    assert!(result.daily[0].relationships[0].members.is_none());
    assert!(result.asu_2025_09_similar_risk_supported);
}

#[test]
fn dissimilar_group_dedesignates_as_of_the_last_similar_date() {
    let mut v = group_variant("instrument_highly_effective_each_risk");
    let k = cfh_fixed(&v);
    let kp = par(0.046);
    v["policy"]["assessment"]["lower_bound"] = json!(0.95);
    v["policy"]["assessment"]["upper_bound"] = json!(1.05);
    v["events"] = json!([{"event_id": "assess", "relationship_id": "cfh-1", "ordinal": day(3, 1), "sequence": 0,
        "kind": "assess", "review": {"id": "review", "revision": "r1"}}]);
    let result = run(&v).unwrap();
    let a = &result.assessments[0];
    let sr = a.similar_risk.as_ref().unwrap();
    assert!(!sr.similar && sr.members[0].similar && !sr.members[1].similar);
    let rec = record(&result, "assessment");
    // Default effective date: the last similar assessment, here inception (1 February).
    assert_eq!(rec.effective_ordinal, Some(day(2, 1)));
    assert_eq!(rec.reason.as_deref(), Some("similar_risk_failed"));
    assert_eq!(rec.status_after, Status::Discontinued);
    let s2 = settlement(2, 100., k, 0.045);
    let g = s2 + value(day(3, 1), 100., k, 0.045);
    close(rec.earnings_catch_up.unwrap(), g);
    let r = settlement(2, 60., k, 0.045) + settlement(2, 40., kp, 0.046);
    close(rec.reversed_reclassification.unwrap(), r);
    close(rec.frozen_oci, 0.);
    close(rec.released_oci, 0.);
    let catch = intents(&result, "similar_risk_earnings_catch_up");
    assert_eq!(catch.len(), 1);
    assert_eq!(catch[0].postings[1].account, Account::MarkGainLoss);
    close(catch[0].postings[1].debit_amount, -g);
    let reversal = intents(&result, "similar_risk_reclassification_reversal");
    assert_eq!(reversal.len(), 1);
    assert_eq!(reversal[0].postings.len(), 4);
    let closing = &result.closing[1];
    close(closing.oci_recognized, 0.);
    close(closing.oci_reclassified, 0.);
    close(closing.oci_closing, 0.);
    assert!(result
        .intents
        .iter()
        .filter(|i| i.event_id.starts_with("clean_mark")
            && i.mapped_position_id.as_deref() == Some("swap-cfh")
            && i.ordinal > day(3, 1))
        .all(|i| i
            .postings
            .iter()
            .all(|p| p.account != Account::CashFlowHedgeOci)));
    // A specific last-similar date keeps OCI through that date and releases it pro rata.
    v["events"][0]["effective_ordinal"] = json!(day(2, 15));
    let result = run(&v).unwrap();
    let rec = record(&result, "assessment");
    assert_eq!(rec.effective_ordinal, Some(day(2, 15)));
    let through_feb15 = value(day(2, 15), 100., k, 0.045);
    close(rec.earnings_catch_up.unwrap(), g - through_feb15);
    close(rec.frozen_oci, through_feb15);
    close(rec.released_oci, through_feb15 * 29. / 60.);
    let releases = intents(&result, "frozen_oci_release");
    assert_eq!(
        releases.iter().map(|i| i.ordinal).collect::<Vec<_>>(),
        [day(3, 1), day(3, 1), day(4, 1), day(4, 1)]
    );
    close(releases[0].postings[0].debit_amount, through_feb15 * 0.29);
    close(
        releases[1].postings[0].debit_amount,
        through_feb15 * 0.4 * 29. / 60.,
    );
    close(
        releases
            .iter()
            .map(|i| i.postings[0].debit_amount)
            .sum::<f64>(),
        through_feb15,
    );
    let closing = &result.closing[1];
    close(closing.oci_recognized, through_feb15);
    close(closing.oci_closing, 0.);
    for intent in &result.intents {
        let sum: f64 = intent.postings.iter().map(|p| p.debit_amount).sum();
        assert!(sum.abs() < 1e-9);
    }
}

#[test]
fn rate_election_switches_the_hypothetical_with_terms_as_of_inception() {
    let v = rate_variant(0.045);
    let k = cfh_fixed(&v);
    let kp = par(0.046);
    let result = run(&v).unwrap();
    let a = &result.assessments[2];
    assert_eq!(a.index_id.as_deref(), Some("term-1m"));
    close(a.ratio.unwrap(), 1.);
    assert!(a.qualified && a.similar_risk.is_none());
    let rec = record(&result, "rate_election");
    assert_eq!(rec.index_id.as_deref(), Some("term-1m-alt"));
    assert_eq!(rec.reason.as_deref(), Some("qualified"));
    assert!(rec.earnings_catch_up.is_none() && rec.effective_ordinal.is_none());
    assert_eq!(rec.status_after, Status::Qualified);
    let h_alt = value(day(3, 1), 100., kp, 0.046);
    let mar1 = result
        .daily
        .iter()
        .find(|d| d.ordinal == day(3, 1))
        .unwrap();
    close(mar1.relationships[1].hypothetical_value.unwrap(), h_alt);
    close(mar1.relationships[1].cumulative_hedged_change, h_alt);
    assert_eq!(
        mar1.relationships[1].members.as_ref().unwrap()[0].index_id,
        "term-1m-alt"
    );
    let s2 = settlement(2, 100., k, 0.045);
    let s3 = settlement(3, 100., k, 0.045);
    let s3_alt = settlement(3, 100., kp, 0.046);
    let reclass = intents(&result, "cash_flow_hedge_reclassification");
    assert_eq!(reclass.len(), 2);
    close(reclass[0].postings[0].debit_amount, s2);
    close(reclass[1].postings[0].debit_amount, s3_alt);
    let closing = &result.closing[1];
    close(closing.oci_recognized, s2 + s3);
    close(closing.oci_reclassified, s2 + s3_alt);
    close(closing.oci_closing, s3 - s3_alt);
    assert_eq!(
        result.relationships[1].selected_index_id.as_deref(),
        Some("term-1m-alt")
    );
    assert!(result.asu_2025_09_selectable_rate_supported);
    // An ineffective final period on the prior rate returns its instrument changes to earnings
    // while the relationship continues on the newly selected rate (815-30-35-37L).
    let v = rate_variant(0.05);
    let k50 = par(0.05);
    let result = run(&v).unwrap();
    let a = &result.assessments[2];
    assert!(!a.qualified && a.ratio.unwrap() < 0.8);
    let rec = record(&result, "rate_election");
    assert_eq!(
        rec.reason.as_deref(),
        Some("ineffective_period_in_earnings")
    );
    assert_eq!(rec.effective_ordinal, Some(day(2, 1)));
    let g = s2 + value(day(3, 1), 100., k, 0.045);
    close(rec.earnings_catch_up.unwrap(), g);
    assert_eq!(rec.status_after, Status::Qualified);
    assert_eq!(intents(&result, "rate_election_earnings_catch_up").len(), 1);
    let closing = &result.closing[1];
    close(closing.oci_recognized, s2 + s3 - g);
    close(
        closing.oci_reclassified,
        settlement(2, 100., k50, 0.05) + s3_alt,
    );
    close(
        closing.oci_closing,
        closing.oci_recognized - closing.oci_reclassified,
    );
}

type Change = Box<dyn Fn(&mut Value)>;
type Case = (&'static str, fn() -> Value, Change);
#[test]
fn election_refusals_without_fallback() {
    let cases: Vec<Case> = vec![
        (
            "group without election",
            || group_variant("instrument_highly_effective_each_risk"),
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["elections"]["similar_risk_grouping"] =
                    json!(false)
            }),
        ),
        (
            "group before adoption",
            || group_variant("instrument_highly_effective_each_risk"),
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["adoption_ordinal"] = json!(day(3, 1))
            }),
        ),
        (
            "group member choose-your-rate",
            || group_variant("instrument_highly_effective_each_risk"),
            Box::new(|v| {
                v["forecast_exposures"][1]["choose_your_rate"] = json!({"evidence": {"id": "x", "revision": "r1"},
                    "documented_index_ids": ["term-1m-alt", "term-1m"], "debt_classification": "liability"})
            }),
        ),
        (
            "group hypothetical order",
            || group_variant("instrument_highly_effective_each_risk"),
            Box::new(|v| {
                v["relationships"][1]["group"]["member_exposure_ids"] =
                    json!(["float-2", "float-1"])
            }),
        ),
        (
            "group member index mirror",
            || group_variant("instrument_highly_effective_each_risk"),
            Box::new(|v| v["forecast_exposures"][1]["index_id"] = json!("term-1m")),
        ),
        (
            "effective date on a single relationship",
            || rate_variant(0.045),
            Box::new(|v| {
                v["events"][2] = json!({"event_id": "a", "relationship_id": "cfh-1", "ordinal": day(3, 1), "sequence": 1,
                    "kind": "assess", "review": {"id": "r", "revision": "r1"}, "effective_ordinal": day(2, 15)})
            }),
        ),
        (
            "election without policy",
            || rate_variant(0.045),
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["elections"]["selectable_rate_debt"] =
                    json!(false)
            }),
        ),
        (
            "undocumented index",
            || rate_variant(0.045),
            Box::new(|v| v["events"][2]["index_id"] = json!("prime")),
        ),
        (
            "unchanged index",
            || rate_variant(0.045),
            Box::new(|v| v["events"][2]["index_id"] = json!("term-1m")),
        ),
        (
            "election on an ordinary relationship",
            || raw(),
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["adopted"] = json!(true);
                v["policy"]["adoption"]["asu_2025_09"]["adoption_ordinal"] = json!(day(1, 1));
                v["policy"]["adoption"]["asu_2025_09"]["elections"]["selectable_rate_debt"] =
                    json!(true);
                v["events"][2]["kind"] = json!("rate_election");
                v["events"][2]["index_id"] = json!("term-1m-alt");
            }),
        ),
        (
            "choose-your-rate asset",
            || rate_variant(0.045),
            Box::new(|v| v["forecast_exposures"][0]["direction"] = json!("receipt")),
        ),
        (
            "missing alternative hypothetical",
            || rate_variant(0.045),
            Box::new(|v| v["relationships"][1]["rate_hypotheticals"] = json!([])),
        ),
        (
            "off-market alternative hypothetical",
            || rate_variant(0.045),
            Box::new(|v| {
                for n in v["relationships"][1]["rate_hypotheticals"][0]["hypothetical"]["nodes"]
                    .as_array_mut()
                    .unwrap()
                {
                    n["request"]["fixed_rate"] = json!(0.05);
                }
            }),
        ),
        (
            "election at an accrual date",
            || rate_variant(0.045),
            Box::new(|v| v["events"][2]["ordinal"] = json!(day(2, 15))),
        ),
        (
            "swap/written-option election",
            || raw(),
            Box::new(|v| {
                v["policy"]["adoption"]["asu_2025_09"]["elections"]
                    ["swap_written_option_exception"] = json!(true)
            }),
        ),
        (
            "unknown relationship field",
            || raw(),
            Box::new(|v| v["relationships"][1]["similar_risk_method"] = json!("x")),
        ),
    ];
    for (label, base, change) in cases {
        let mut v = base();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
    // Elections alone, with no relationship using them, do not change the base result.
    let mut v = raw();
    adopt(&mut v, "similar_risk_grouping");
    let elected = run(&v).unwrap();
    let base = run(&raw()).unwrap();
    assert_eq!(elected.intents.len(), base.intents.len());
    close(elected.closing[1].oci_closing, base.closing[1].oci_closing);
    assert!(elected
        .assessments
        .iter()
        .all(|a| a.similar_risk.is_none() && a.index_id.is_none()));
}
