use portfolio_risk_native::{
    product_strategy::{evaluate, prepare, Allocation},
    product_units::ProductUnitsRequest,
};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/product-units-1.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} != {b}");
}
fn request(v: Value) -> ProductUnitsRequest {
    serde_json::from_value(v).unwrap()
}
fn duplicate_scenario(g: &mut Value) {
    let mut s = g["scenarios"][0].clone();
    s["scenario_id"] = json!("stress");
    s["run_id"] = json!("stress");
    g["scenarios"].as_array_mut().unwrap().push(s);
    let mut h = g["positions"][0]["histories"][0].clone();
    h["scenario_id"] = json!("stress");
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        for path in h[role].as_array_mut().unwrap() {
            path["request"]["identity"]["run_id"] = json!("stress");
        }
    }
    g["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
        .push(h);
}
#[test]
fn selected_deposit_replay_matches_independent_money_and_original_yield() {
    let p = prepare(&request(raw())).unwrap();
    let x = 250.;
    let a = [Allocation {
        unit_id: "deposit-1".into(),
        face: x,
    }];
    let r = evaluate(&p, &a).unwrap();
    let quote = 1002. * (-0.02f64.ln_1p() * 2. / 365.).exp() / 1000.;
    let daily = (0.02f64.ln_1p() / 365.).exp_m1();
    near(r.total_liability_face, x);
    near(r.total_asset_face, 0.);
    near(r.total_asset_purchase_cost, 0.);
    near(r.scenarios[0].values[0], -quote * daily * x);
    near(r.scenarios[0].values[1], -quote * (1. + daily) * daily * x);
    near(r.scenarios[0].cash[0].purchase_cash, quote * x);
    near(
        r.scenarios[0].cash.last().unwrap().cumulative_bank_cash,
        (quote - 1.002) * x,
    );
    near(r.category_face["supplied-amortized-cost"], x);
    assert_eq!(r.scenarios[0].cash.last().unwrap().ordinal, 738918);
    assert_eq!(p.units()[0].purchase_period, 0);
}
#[test]
fn each_scenario_cash_is_replayed_once_without_cross_scenario_netting() {
    let mut v = raw();
    duplicate_scenario(&mut v["units"][0]["purchase_graph"]["request"]);
    let branches = v["units"][0]["age_branches"].as_array_mut().unwrap();
    for b in branches.iter_mut() {
        duplicate_scenario(&mut b["graph"]["request"]);
    }
    let extras: Vec<_> = branches
        .iter()
        .map(|b| {
            let mut b = b.clone();
            b["scenario_id"] = json!("stress");
            b
        })
        .collect();
    branches.extend(extras);
    for b in branches.iter_mut() {
        let id = b["scenario_id"].clone();
        let g = &mut b["graph"]["request"];
        g["base_scenario_id"] = id.clone();
        g["scenarios"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s["scenario_id"] == id);
        g["positions"][0]["histories"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s["scenario_id"] == id);
    }
    let p = prepare(&request(v)).unwrap();
    let r = evaluate(
        &p,
        &[Allocation {
            unit_id: "deposit-1".into(),
            face: 1000.,
        }],
    )
    .unwrap();
    assert_eq!(r.scenarios.len(), 2);
    for s in &r.scenarios {
        near(
            s.cash[0].purchase_cash,
            1002. * (-0.02f64.ln_1p() * 2. / 365.).exp(),
        );
        near(s.cash[2].principal_cash, -1000.);
    }
}
#[test]
fn zero_allocation_and_identity_finite_complete_admission() {
    let p = prepare(&request(raw())).unwrap();
    let r = evaluate(
        &p,
        &[Allocation {
            unit_id: "deposit-1".into(),
            face: 0.,
        }],
    )
    .unwrap();
    assert!(r.scenarios[0].values.iter().all(|v| *v == 0.));
    assert!(evaluate(&p, &[]).is_err());
    for x in [-1., f64::NAN, f64::INFINITY, 1e308] {
        assert!(evaluate(
            &p,
            &[Allocation {
                unit_id: "deposit-1".into(),
                face: x
            }]
        )
        .is_err());
    }
    assert!(evaluate(
        &p,
        &[Allocation {
            unit_id: "foreign".into(),
            face: 1.
        }]
    )
    .is_err());
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    assert_eq!(
        portfolio_compute_control::scope(Some(c), || evaluate(
            &p,
            &[Allocation {
                unit_id: "deposit-1".into(),
                face: 0.
            }]
        ))
        .err()
        .unwrap(),
        "COMPUTE_CANCELLED"
    );
}
