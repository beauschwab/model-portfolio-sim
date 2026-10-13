use super::*;
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/term-product-graph-2.json"
    ))
    .unwrap()
}
#[test]
fn actual_solver_and_generator_call_counts_prove_no_initial_or_warm_refit() {
    evidence::reset();
    let mut v = raw();
    let r: Request = serde_json::from_value(v.clone()).unwrap();
    let g = TermGraph::new(&r).unwrap();
    assert_eq!(evidence::calls(), (0, 8));
    v["scenarios"][0]["collateral_curve"]["discount_factors"][1] = json!((-0.08_f64).exp());
    g.reprice(&serde_json::from_value(v).unwrap()).unwrap();
    assert_eq!(evidence::calls(), (0, 12));
    // Legacy locked requests still enter their historical solver helper.
    evidence::reset();
    let r: Request = serde_json::from_str(include_str!(
        "../../../../examples/models/term-product-graph-1.json"
    ))
    .unwrap();
    TermGraph::new(&r).unwrap();
    assert_eq!(evidence::calls(), (2, 8));
}
#[test]
fn aggregate_capacity_refusal_precedes_every_native_projection() {
    let mut v = raw();
    let base = v["positions"][0]["histories"][0].clone();
    let orig = v["original_base"]["histories"][0]["histories"].clone();
    for (scope, set) in [(false, base), (true, orig)] {
        let target = if scope {
            &mut v["original_base"]["histories"][0]["histories"]
        } else {
            &mut v["positions"][0]["histories"][0]
        };
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            let base = set[role][0].clone();
            let mut list = Vec::new();
            for i in 0..128 {
                let mut p = base.clone();
                p["path_id"] = json!(format!("pair-{i}"));
                p["weight"] = json!(1. / 128.);
                list.push(p);
            }
            target[role] = json!(list);
        }
    }
    evidence::reset();
    let e = TermGraph::new(&serde_json::from_value(v).unwrap())
        .err()
        .unwrap();
    assert!(e.contains("capacity exceeded before projection"), "{e}");
    assert_eq!(evidence::calls(), (0, 0));
}
