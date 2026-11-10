use portfolio_risk_native::commercial_credit_compiler as owner;
use serde_json::{json, Value};
fn raw() -> Value {
    let v: Value = serde_json::from_slice(include_bytes!(
        "../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"
    ))
    .unwrap();
    v["completed_candidate"]["commercial"]["units"][0]["graph"]["positions"][0]["histories"][0]
        ["physical"][0]["source"]
        .clone()
}
#[test]
fn selected_principal_only_cash_has_distinct_c_and_coupon_l_dimensions() {
    let mut v = raw();
    let at = v["source"]["current"]["asof_ordinal"].as_i64().unwrap();
    let end = at + 3;
    let future = json!({"event_id":"principal-final","payment_ordinal":end,"amount":80.});
    v["source"]["transfer"]["contract_origin"]["principal_plan"] = json!({"kind":"dated_supplied_period_opening_nominal-1","principal_events":[{"event_id":"principal-before-transfer","payment_ordinal":at,"amount":20.},future.clone()]});
    // The reviewed transfer occurs before acquisition-day cash, so the actual
    // twenty principal settles one day earlier inside the same coupon period.
    v["source"]["transfer"]["contract_origin"]["asof_ordinal"] = json!(at - 2);
    v["source"]["transfer"]["contract_origin"]["observed_publication_cutoff_unix_seconds"] =
        json!((at - 2 - 719163) * 86400);
    v["source"]["transfer"]["contract_origin"]["periods"][0]["start_ordinal"] = json!(at - 2);
    v["source"]["transfer"]["contract_origin"]["principal_plan"]["principal_events"][0]
        ["payment_ordinal"] = json!(at - 1);
    for role in ["original", "current"] {
        v["source"][role]["opening_principal"] = json!(80.);
        v["source"][role]["periods"][0]["start_ordinal"] = json!(at - 2);
        v["source"][role]["principal_plan"] = json!({"kind":"dated_supplied_period_opening_nominal-1","principal_events":[future.clone()]});
    }
    let accrued = 100_f64 * 0.12 * 2. / 360.;
    let paid = 80. + 100_f64 * 0.12 * 5. / 360.;
    let rate = ((paid / 80.).ln() * 365. / 3.).exp_m1();
    v["source"]["transfer"]["loan_clean_cost"] = json!(80. - accrued);
    v["source"]["transfer"]["coupon_allocations"][0]["purchased_accrued_cost"] = json!(accrued);
    v["source"]["acquisition"]["cost"] = json!(80.);
    v["source"]["acquisition"]["original_annual_effective_rate"] = json!(rate);
    v["accounting_rates"][0]["annual_effective_rates"] = json!([rate, rate, rate]);
    let r: owner::Request = serde_json::from_value(v).unwrap();
    let out = owner::evaluate(&r).unwrap();
    let value = serde_json::to_value(&out).unwrap();
    let census = &value["carry"]["tail_census"];
    assert_eq!(census["source_cash_per_path"], 2);
    assert_eq!(census["gross_coupon_claims_per_path"], 4);
    assert_eq!(out.carry().native_source_cash().len(), 2);
    assert_eq!(out.accounting().groups()[0].next.principal, 80.);
    assert_eq!(
        out.carry().earned_origin_contributions()[0].actual_principal,
        100.
    );
    assert!((out.carry().credit().result.paths[0].cash_received - paid).abs() < 1e-9);
}
