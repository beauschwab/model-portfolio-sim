//! ASU 2022-01 portfolio-layer hedges: closed-portfolio layers on the reviewed example, actual and
//! anticipated breaches under both reviewed orders, an amortizing layer, and explicit refusals.
//!
//! The closed portfolio `pool-1` has the example loan's terms (7% coupon, three monthly ACT/360
//! periods, principal 1 April); layers are proportions of `swap-fvh` against the 6% benchmark, so
//! every bullet layer's basis is the example fair-value hedge's basis scaled by its layer amount.
use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::hedge_accounting::{self as hedge, *};
use serde_json::{json, Value};

fn day(month: u32, d: u32) -> i32 {
    ordinal_from_ymd(2024, month, d).unwrap()
}
fn example() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/hedge-accounting-1.json"
    ))
    .unwrap()
}
fn run(v: &Value) -> Result<HedgeAccountingResult, String> {
    hedge::evaluate(&serde_json::from_value(v.clone()).expect("layer case parses"))
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * (1. + a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
fn id(s: &str) -> Value {
    json!({"id": s, "revision": "r1"})
}
/// Observed balances on every node and reviewed projections, as (ordinal, term end, anticipated).
fn portfolio(balance: impl Fn(i32) -> f64, projections: &[(i32, i32, f64)]) -> Value {
    json!({
        "members": [{"asset_id": "loan-a", "source": id("loan-a"), "prepayable": true},
                    {"asset_id": "loan-b", "source": id("loan-b"), "prepayable": true}],
        "balance_source": id("servicing-balances"),
        "balances": (day(1, 1)..=day(4, 1)).map(|t| json!({"ordinal": t, "outstanding": balance(t)})).collect::<Vec<_>>(),
        "projection_evidence": id("prepayment-projection"),
        "projections": projections.iter().map(|(o, e, a)| json!({"ordinal": o, "term_end_ordinal": e, "anticipated": a})).collect::<Vec<_>>(),
    })
}
fn layer(rid: &str, proportion: f64, hedged: f64, end: Option<i32>) -> Value {
    let mut v = json!({
        "relationship_id": rid, "predecessor_relationship_id": null, "designation": "fair_value_hedge",
        "hedging_instrument_position_id": "swap-fvh", "designated_proportion": proportion, "hedged_item_id": "pool-1",
        "hedged_notional": hedged, "forecast_exposure_id": null, "hedged_risk": "benchmark_interest_rate",
        "benchmark_rate": 0.06, "inception_ordinal": day(1, 1), "risk_management_objective": id(&format!("{rid}-objective")),
        "documentation": id(&format!("{rid}-documentation")), "hypothetical": null, "matched_critical_terms_evidence": null});
    if let Some(e) = end {
        v["hedged_term_end_ordinal"] = json!(e);
    }
    v
}
fn assess(rid: &str, ordinal: i32, sequence: u32) -> Value {
    json!({"event_id": format!("assess-{rid}-{ordinal}"), "relationship_id": rid, "ordinal": ordinal,
           "sequence": sequence, "kind": "assess", "review": id("assessment")})
}
/// The example with its loan replaced by the closed portfolio and only layer relationships.
fn case(order: &str, pool: Value, layers: Vec<Value>, events: Vec<Value>) -> Value {
    let mut v = example();
    v["policy"]["adoption"]["asu_2022_01_portfolio_layer"] = json!(true);
    v["policy"]["portfolio_layer"] = json!({"identity": id("layer-policy"), "breach_order": order});
    let mut item = v["hedged_items"][0].clone();
    item["item_id"] = json!("pool-1");
    item["kind"] = json!("closed_portfolio");
    item["portfolio"] = pool;
    v["hedged_items"] = json!([item]);
    v["forecast_exposures"] = json!([]);
    v["relationships"] = Value::Array(layers);
    v["events"] = Value::Array(events);
    v
}
fn closing<'a>(
    r: &'a HedgeAccountingResult,
    rid: &str,
) -> (&'a RelationshipClosing, &'a RollForward) {
    (
        r.relationships
            .iter()
            .find(|c| c.relationship_id == rid)
            .unwrap(),
        r.closing.iter().find(|c| c.relationship_id == rid).unwrap(),
    )
}
fn basis_at(r: &HedgeAccountingResult, rid: &str, t: i32) -> f64 {
    let d = r.daily.iter().find(|d| d.ordinal == t).unwrap();
    d.relationships
        .iter()
        .find(|x| x.relationship_id == rid)
        .unwrap()
        .basis_adjustment_balance
}

#[test]
fn last_designated_first_anticipated_then_actual_breach_scale_the_example_basis() {
    let base = run(&example()).unwrap();
    let ex = |t: i32| basis_at(&base, "fvh-1", t);
    let (feb1, mar1, mar15, apr1) = (day(2, 1), day(3, 1), day(3, 15), day(4, 1));
    let pool = portfolio(
        |t| if t < mar15 { 100. } else { 70. },
        &[
            (day(1, 1), apr1, 100.),
            (feb1, apr1, 80.),
            (mar1, apr1, 80.),
        ],
    );
    let v = case(
        "last_designated_first",
        pool,
        vec![
            layer("layer-1", 0.7, 70., None),
            layer("layer-2", 0.3, 30., None),
        ],
        vec![
            assess("layer-1", feb1, 0),
            assess("layer-2", feb1, 1),
            assess("layer-1", mar1, 0),
            assess("layer-2", mar1, 1),
        ],
    );
    let r = run(&v).unwrap();
    assert!(r.portfolio_layer_supported);
    // Layer 1 is never breached: its basis is 70% of the example hedge's basis throughout.
    let (c1, f1) = closing(&r, "layer-1");
    close(f1.basis_closing, 0.7 * base.closing[0].basis_closing);
    close(c1.layer_amount.unwrap(), 70.);
    assert_eq!(c1.basis_excluded_from_credit_loss_measurement, Some(true));
    // Anticipated breach on 1 Feb: the projection (80) is 20 short of 100, all from layer 2 (last).
    let breaches: Vec<&LifecycleRecord> = r
        .lifecycle
        .iter()
        .filter(|x| x.kind == "portfolio_layer_breach")
        .collect();
    assert_eq!(breaches.len(), 2);
    assert_eq!(
        (breaches[0].ordinal, breaches[0].reason.as_deref()),
        (feb1, Some("anticipated_breach"))
    );
    close(breaches[0].breach_amount.unwrap(), 20.);
    close(breaches[0].frozen_basis, 0.2 * ex(feb1));
    close(breaches[0].proportion_after, 0.1);
    // Actual breach on 15 Mar: 70 outstanding against 70 + 10; the rest of layer 2 goes, basis to earnings.
    assert_eq!(
        (breaches[1].ordinal, breaches[1].reason.as_deref()),
        (mar15, Some("actual_breach"))
    );
    close(breaches[1].breach_amount.unwrap(), 10.);
    close(breaches[1].derecognized_basis, 0.1 * ex(mar15));
    assert_eq!(breaches[1].status_after, Status::Discontinued);
    let (c2, f2) = closing(&r, "layer-2");
    close(c2.anticipated_breach_amount.unwrap(), 20.);
    close(c2.actual_breach_amount.unwrap(), 10.);
    close(c2.layer_amount.unwrap(), 0.);
    // The anticipated tranche amortizes straight-line to the portfolio's final maturity (fully by 1 Apr).
    close(f2.basis_amortized, 0.2 * ex(feb1));
    close(f2.basis_derecognized, 0.1 * ex(mar15));
    close(f2.basis_closing, 0.);
    let mid = day(3, 2);
    let frozen_mid = r
        .daily
        .iter()
        .find(|d| d.ordinal == mid)
        .unwrap()
        .relationships[1]
        .frozen_basis_balance;
    close(
        frozen_mid,
        0.2 * ex(feb1) * (1. - f64::from(mid - feb1) / f64::from(apr1 - feb1)),
    );
    for intent in &r.intents {
        let sum: f64 = intent.postings.iter().map(|p| p.debit_amount).sum();
        assert!(sum.abs() < 1e-9);
    }
}

#[test]
fn pro_rata_actual_breach_and_an_amortizing_layer() {
    let (feb1, feb15, mar1, apr1) = (day(2, 1), day(2, 15), day(3, 1), day(4, 1));
    let pool = portfolio(
        |t| if t < feb15 { 100. } else { 50. },
        &[(day(1, 1), mar1, 100.), (day(1, 1), apr1, 70.)],
    );
    let mut amortizing = layer("layer-a", 0.6, 60., None);
    amortizing["layer_amortization"] = json!([
        {"period_end_ordinal": feb1, "amount": 60.}, {"period_end_ordinal": mar1, "amount": 40.},
        {"period_end_ordinal": apr1, "amount": 20.}]);
    let v = case(
        "pro_rata",
        pool,
        vec![amortizing, layer("layer-b", 0.3, 30., Some(mar1))],
        vec![],
    );
    let r = run(&v).unwrap();
    // The amortizing layer's inception value: coupons on 60/40/20 and principal as it amortizes.
    let df = |e: i32| 1. / (1. + 0.06 * f64::from(e - day(1, 1)) / 360.);
    let ends = [
        (day(1, 1), feb1, 60., 20.),
        (feb1, mar1, 40., 20.),
        (mar1, apr1, 20., 20.),
    ];
    let expected: f64 = ends
        .iter()
        .map(|(s, e, a, repaid)| a * 0.06 * f64::from(e - s) / 360. * df(*e) + repaid * df(*e))
        .sum();
    let d0 = &r.daily[0].relationships[0];
    close(d0.hedged_item_clean_value.unwrap(), expected);
    // 15 Feb: 50 outstanding against 40 + 30; the 20 excess is shared pro rata (2/7 of each).
    let breaches: Vec<&LifecycleRecord> = r
        .lifecycle
        .iter()
        .filter(|x| x.kind == "portfolio_layer_breach")
        .collect();
    assert_eq!(breaches.len(), 2);
    for (b, cur, rid) in [(breaches[0], 40., "layer-a"), (breaches[1], 30., "layer-b")] {
        assert_eq!(
            (b.ordinal, b.reason.as_deref(), b.relationship_id.as_str()),
            (feb15, Some("actual_breach"), rid)
        );
        close(b.breach_amount.unwrap(), cur * 20. / 70.);
        close(
            b.derecognized_basis,
            2. / 7. * basis_before(&r, rid, feb15, b.derecognized_basis),
        );
    }
    close(
        closing(&r, "layer-a").0.layer_amount.unwrap(),
        60. * 5. / 7.,
    );
    close(
        closing(&r, "layer-b").0.layer_amount.unwrap(),
        30. * 5. / 7.,
    );
}
/// Basis just before the breach: what remains after the node plus what was derecognized.
fn basis_before(r: &HedgeAccountingResult, rid: &str, t: i32, derecognized: f64) -> f64 {
    basis_at(r, rid, t) + derecognized
}

#[test]
fn explicit_layer_refusals_without_fallback() {
    let (feb1, apr1) = (day(2, 1), day(4, 1));
    let good = || {
        case(
            "last_designated_first",
            portfolio(|_| 100., &[(day(1, 1), apr1, 100.)]),
            vec![layer("layer-1", 1., 100., None)],
            vec![],
        )
    };
    run(&good()).unwrap();
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Change)> = vec![
        (
            "no adoption",
            Box::new(|v| v["policy"]["adoption"]["asu_2022_01_portfolio_layer"] = json!(false)),
        ),
        (
            "no policy",
            Box::new(|v| {
                v["policy"]
                    .as_object_mut()
                    .unwrap()
                    .remove("portfolio_layer");
            }),
        ),
        (
            "unknown order",
            Box::new(|v| v["policy"]["portfolio_layer"]["breach_order"] = json!("fifo")),
        ),
        (
            "block on fixed item",
            Box::new(|v| v["hedged_items"][0]["kind"] = json!("fixed_rate_instrument")),
        ),
        (
            "missing block",
            Box::new(|v| {
                v["hedged_items"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("portfolio");
            }),
        ),
        (
            "liability portfolio",
            Box::new(|v| v["hedged_items"][0]["direction"] = json!("liability")),
        ),
        (
            "duplicate member",
            Box::new(|v| {
                v["hedged_items"][0]["portfolio"]["members"][1]["asset_id"] = json!("loan-a")
            }),
        ),
        (
            "increasing balance",
            Box::new(|v| {
                v["hedged_items"][0]["portfolio"]["balances"][5]["outstanding"] = json!(100.5)
            }),
        ),
        (
            "missing balance",
            Box::new(|v| {
                v["hedged_items"][0]["portfolio"]["balances"]
                    .as_array_mut()
                    .unwrap()
                    .remove(10);
            }),
        ),
        (
            "stale projection",
            Box::new(|v| {
                v["hedged_items"][0]["portfolio"]["projections"][0]["term_end_ordinal"] =
                    json!(day(1, 1))
            }),
        ),
        (
            "missing inception projection",
            Box::new(|v| v["hedged_items"][0]["portfolio"]["projections"] = json!([])),
        ),
        (
            "not expected to remain",
            Box::new(|v| {
                v["hedged_items"][0]["portfolio"]["projections"][0]["anticipated"] = json!(90.)
            }),
        ),
        (
            "exceeds portfolio",
            Box::new(|v| {
                for b in v["hedged_items"][0]["portfolio"]["balances"]
                    .as_array_mut()
                    .unwrap()
                {
                    b["outstanding"] = json!(90.);
                }
            }),
        ),
        (
            "schedule on bullet item",
            Box::new(move |v| {
                v["hedged_items"][0]["kind"] = json!("fixed_rate_instrument");
                v["hedged_items"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("portfolio");
                v["relationships"][0]["layer_amortization"] =
                    json!([{"period_end_ordinal": feb1, "amount": 100.}]);
            }),
        ),
        (
            "schedule periods",
            Box::new(move |v| {
                v["relationships"][0]["layer_amortization"] =
                    json!([{"period_end_ordinal": feb1, "amount": 100.}])
            }),
        ),
        (
            "schedule increases",
            Box::new(move |v| {
                v["relationships"][0]["layer_amortization"] = json!([
            {"period_end_ordinal": feb1, "amount": 100.}, {"period_end_ordinal": day(3, 1), "amount": 100.5},
            {"period_end_ordinal": apr1, "amount": 50.}])
            }),
        ),
        (
            "seasoned layer",
            Box::new(|v| {
                v["relationships"][0]["opening"] = json!({"evidence": id("o"), "hedged_value": 1.,
            "cumulative_instrument_change": 0., "cumulative_hedged_change": 0., "basis_balance": 0., "oci_balance": 0.})
            }),
        ),
    ];
    for (label, change) in cases {
        let mut v = good();
        change(&mut v);
        let outcome = serde_json::from_value::<HedgeAccountingRequest>(v)
            .map_err(|e| e.to_string())
            .and_then(|r| hedge::evaluate(&r).map(|_| ()));
        assert!(outcome.is_err(), "{label} must reject");
    }
}
