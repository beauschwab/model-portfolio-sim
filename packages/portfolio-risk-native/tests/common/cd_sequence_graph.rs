#![allow(dead_code)]
use serde_json::{json, Value};
pub fn raw() -> Value {
    serde_json::from_str(include_str!("../fixtures/term-product-graph-2.json")).unwrap()
}
pub fn policy() -> Value {
    serde_json::from_str(include_str!("../fixtures/cd_sequence_policy.json")).unwrap()
}
pub fn all(v: &mut Value, f: impl Fn(&mut Value)) {
    for p in v["positions"].as_array_mut().unwrap() {
        for s in p["histories"].as_array_mut().unwrap() {
            roles(s, &f);
        }
    }
    for p in v["original_base"]["histories"].as_array_mut().unwrap() {
        roles(&mut p["histories"], &f);
    }
}
pub fn roles(v: &mut Value, f: &impl Fn(&mut Value)) {
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        for p in v[role].as_array_mut().unwrap() {
            f(&mut p["product"]["request"]);
        }
    }
}
pub fn partial(v: &mut Value) {
    let policy = policy();
    v["positions"][0]["known"]["contract"]["policies"] = json!([policy]);
    all(v, |q| {
        q["policies"] = json!([policy]);
        q["actions"] = json!([
 {"action_id":"first","position_id":"cd:example","ordinal":739622,"source":{"id":"first","revision":"r1"},"election":{"kind":"withdraw","principal":40.}},
 {"action_id":"second","position_id":"cd:example","ordinal":739625,"source":{"id":"second","revision":"r1"},"election":{"kind":"withdraw","principal":20.}}]);
    });
    v["original_quotes"][0]["signed_cash_price"] = json!(-100.76);
}
pub fn req(v: Value) -> portfolio_risk_native::term_product_graph::Request {
    serde_json::from_value(v).unwrap()
}
pub fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs())),
        "{a} != {b}"
    );
}
