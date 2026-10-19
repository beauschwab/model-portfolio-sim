//! saved-derivative-risk-1: saved swaps as cached graph nodes with paired curve risk.
use portfolio_risk_native::graph::incremental::Request;
use serde_json::{json, Value};

type Edit = Box<dyn Fn(&mut Value)>;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/saved-derivative-risk.json")).unwrap()
}
fn call(v: Value) -> Result<Value, String> {
    serde_json::from_value::<Request>(v)
        .map_err(|e| e.to_string())?
        .run()
}
fn create(request: &Value) -> Result<Value, String> {
    call(
        json!({"op":"create_saved_derivative_risk","max_bytes":1048576,"max_entries":4096,"request":request}),
    )
}
fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol * (1. + b.abs()), "{a} vs {b}");
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn risk_aggregates_offsets_and_is_additive_across_key_rates() {
    let created = create(&fixture()).unwrap();
    let r = &created["initial"];
    let positions = r["positions"].as_array().unwrap();
    assert_eq!(positions.len(), 2);
    assert!(f(&r["projected_observations_bumped"]) > 0.);
    assert_eq!(r["observed_fixings_bumped"], json!(false));
    assert_eq!(r["accounting_designation_output"], json!(false));
    // Totals and the single netting set equal the sum of positions, scenario by scenario.
    for key in ["clean_value", "dv01"] {
        let sum: f64 = positions.iter().map(|p| f(&p[key])).sum();
        close(f(&r["total"][key]), sum, 1e-12);
        close(f(&r["netting_sets"][0][key]), sum, 1e-12);
    }
    for p in positions {
        // Payer swaps gain when rates rise: negative DV01 in the falls-positive convention.
        assert!(f(&p["dv01"]) < 0., "{p}");
        // Triangular key-rate weights sum to one, so key-rate risk adds up to the parallel bump.
        let krd: f64 = p["key_rate_dv01"].as_array().unwrap().iter().map(f).sum();
        close(krd, f(&p["dv01"]), 1e-6);
    }
    let fvh = &r["relationships"][0];
    assert_eq!(fvh["relationship_id"], json!("fvh-1"));
    let item = f(&fvh["hedged_item"]["dv01"]);
    let instrument = f(&fvh["instrument"]["dv01"]);
    assert!(item > 0. && instrument < 0.);
    close(f(&fvh["net_dv01"]), instrument + item, 1e-12);
    let ratio = f(&fvh["dv01_offset_ratio"]);
    // The swap's first floating coupon is already fixed and it exchanges no principal, so it
    // offsets about two thirds of the item's three-month benchmark risk (hand check: item
    // ~ 100 * 0.25y * 0.985 * 1bp, swap ~ 100 * 2 projected months * 1bp).
    assert!(
        (0.6..=0.7).contains(&ratio),
        "economic offset ratio {ratio}"
    );
    let rel = |a: f64, b: f64| ((a - b) / b).abs();
    assert!(
        rel(item, 100. * (91. / 365.) * 0.985_059_924_478_739 * 1e-4) < 0.02,
        "{item}"
    );
    assert!(
        rel(-instrument, 100. * (59. / 360.) * 1e-4) < 0.02,
        "{instrument}"
    );
    let cfh = &r["relationships"][1];
    assert!(cfh["hedged_item"].is_null() && cfh["dv01_offset_ratio"].is_null());
    assert_eq!(cfh["net_dv01"], cfh["instrument"]["dv01"]);
    assert_eq!(positions[0]["relationship_id"], json!("fvh-1"));
}

#[test]
fn warm_cold_and_cleared_calls_agree_and_reuse_cached_nodes() {
    let request = fixture();
    let created = create(&request).unwrap();
    let handle = created["handle"].as_u64().unwrap();
    let price = || {
        call(json!({"op":"price_saved_derivative_risk","handle":handle,"request":request})).unwrap()
    };
    let warm = price();
    for stage in ["swap-clean", "hedged-item-benchmark"] {
        assert_eq!(warm["counts"][stage]["computed"], json!(0), "{stage}");
        assert!(warm["counts"][stage]["reused"].as_u64().unwrap() > 0);
    }
    let strip = |mut v: Value| {
        for k in ["counts", "cache"] {
            v.as_object_mut().unwrap().remove(k);
        }
        v
    };
    assert_eq!(strip(warm.clone()), strip(created["initial"].clone()));
    call(json!({"op":"clear","handle":handle})).unwrap();
    let cold = price();
    assert!(cold["counts"]["swap-clean"]["computed"].as_u64().unwrap() > 0);
    assert_eq!(strip(cold), strip(warm));
    // A tiny cache that evicts constantly cannot change economics.
    let tiny = call(json!({"op":"create_saved_derivative_risk","max_bytes":1024,"max_entries":2,"request":request})).unwrap();
    assert_eq!(
        strip(tiny["initial"].clone()),
        strip(created["initial"].clone())
    );
    // A curve edit on the same source reprices only what changed.
    let mut edited = request.clone();
    edited["positions"][1]["swap"]["discount_curve"]["points"][3]["discount_factor"] = json!(0.98);
    let moved =
        call(json!({"op":"price_saved_derivative_risk","handle":handle,"request":edited})).unwrap();
    assert!(moved["counts"]["swap-clean"]["reused"].as_u64().unwrap() > 0);
    assert_ne!(
        moved["positions"][1]["clean_value"],
        created["initial"]["positions"][1]["clean_value"]
    );
    assert_eq!(
        moved["positions"][0]["clean_value"],
        created["initial"]["positions"][0]["clean_value"]
    );
    // A different saved source needs its own owner; other owners' operations are refused.
    let mut other = request.clone();
    other["source"]["revision"] = json!("r2");
    let err = call(json!({"op":"price_saved_derivative_risk","handle":handle,"request":other}))
        .unwrap_err();
    assert!(err.contains("BASE_SOURCE_CHANGED"), "{err}");
    let err = call(
        json!({"op":"price_market_indexed","handle":handle,"request":{"schema":"market-indexed-graph-1","product":"vanilla_swap","source":{}}}),
    );
    assert!(err.is_err());
    call(json!({"op":"drop","handle":handle})).unwrap();
}

#[test]
fn invalid_requests_fail_before_valuation() {
    let cases: Vec<Edit> = vec![
        Box::new(|r| r["schema"] = json!("saved-derivative-risk-0")),
        Box::new(|r| r["bumps"]["bump_bp"] = json!(0.)),
        Box::new(|r| r["bumps"]["bump_bp"] = json!(150.)),
        Box::new(|r| r["bumps"]["key_rate_ordinals"] = json!([738946, 738917])),
        Box::new(|r| r["bumps"]["key_rate_ordinals"] = json!([738886])),
        Box::new(|r| {
            let p = r["positions"][0].clone();
            r["positions"].as_array_mut().unwrap().push(p);
        }),
        Box::new(|r| r["positions"][0]["position_id"] = json!("renamed")),
        Box::new(|r| r["relationships"][1]["instrument_position_ids"] = json!(["swap-fvh"])),
        Box::new(|r| r["relationships"][0]["instrument_position_ids"] = json!(["missing"])),
        Box::new(|r| r["relationships"][0]["hedged_item"]["hedged_notional"] = json!(1000.)),
        Box::new(|r| {
            r["relationships"][0]["hedged_item"]["item"]["kind"] = json!("closed_portfolio")
        }),
        Box::new(|r| r["relationships"][0]["hedged_item"]["benchmark_rate"] = json!(f64::NAN)),
        Box::new(|r| r["positions"][1]["swap"]["currency"] = json!("EUR")),
        Box::new(|r| r["positions"] = json!([])),
        Box::new(|r| r["unknown"] = json!(1)),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut r = fixture();
        change(&mut r);
        assert!(create(&r).is_err(), "case {i} must be refused");
    }
}

#[test]
fn projection_policy_is_explicit_and_named_curves_must_reproduce_fixings() {
    let named = create(&fixture()).unwrap();
    assert!(named["initial"]["pairing_policy"]
        .as_str()
        .unwrap()
        .ends_with("named_projection_curves"));
    // The additive policy is a separate, explicit selection with no curves.
    let mut additive = fixture();
    additive["projection_policy"] = json!("additive_projected_observations");
    additive
        .as_object_mut()
        .unwrap()
        .remove("projection_curves");
    let additive = create(&additive).unwrap();
    for (a, b) in additive["initial"]["positions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(named["initial"]["positions"].as_array().unwrap())
    {
        // Same base value; risk differs only by how a bumped curve maps to a simple forward.
        close(f(&a["clean_value"]), f(&b["clean_value"]), 1e-12);
        let (x, y) = (f(&a["dv01"]), f(&b["dv01"]));
        assert!(x < 0. && y < 0. && ((x - y) / y).abs() < 0.05, "{x} {y}");
    }
    let cases: Vec<Edit> = vec![
        Box::new(|r| {
            r.as_object_mut()
                .unwrap()
                .remove("projection_policy")
                .map(|_| ())
                .unwrap()
        }),
        Box::new(|r| r["projection_policy"] = json!("market_standard")),
        Box::new(|r| {
            r.as_object_mut().unwrap().remove("projection_curves");
        }),
        Box::new(|r| r["projection_curves"][0]["points"][2]["discount_factor"] = json!(0.9895)),
        Box::new(|r| r["projection_curves"][0]["index_id"] = json!("term-3m")),
        Box::new(|r| {
            let points = r["projection_curves"][0]["points"].as_array_mut().unwrap();
            points.remove(2);
        }),
        Box::new(|r| r["projection_policy"] = json!("additive_projected_observations")),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut r = fixture();
        change(&mut r);
        assert!(create(&r).is_err(), "projection case {i} must be refused");
    }
}
