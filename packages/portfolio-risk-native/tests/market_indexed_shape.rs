use portfolio_risk_native::market_indexed_adapter::{evaluate, Request};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap()["request"]
        .clone()
}
#[test]
fn new_curve_tail_requires_string_and_unknown_fields_reject() {
    for field in ["original", "current_market"] {
        let r = raw();
        for name in ["reject", "flat_zero", "flat_forward"] {
            let mut bad = r.clone();
            let market = if field == "original" {
                &mut bad["bridge"]["original"]["market"]
            } else {
                &mut bad["bridge"][field]
            };
            market["curves"][0]["right_tail"] = json!({name:null});
            assert!(serde_json::from_value::<Request>(bad).is_err());
        }
    }
    let mut r = raw();
    r["bridge"]["current_market"]["curves"][0]["ignored_money"] = json!(0);
    assert!(serde_json::from_value::<Request>(r)
        .unwrap_err()
        .to_string()
        .contains("unknown field"));
}
#[test]
fn byte_parse_preserves_duplicate_curve_fields() {
    let text = serde_json::to_string(&raw()).unwrap().replacen(
        "\"right_tail\":\"reject\"",
        "\"right_tail\":\"reject\",\"right_tail\":\"flat_zero\"",
        1,
    );
    assert!(serde_json::from_str::<Request>(&text)
        .unwrap_err()
        .to_string()
        .contains("duplicate field"));
    let text = serde_json::to_string(&raw()).unwrap().replacen(
        "\"collateral_index\":\"SOFR\"",
        "\"collateral_index\":\"SOFR\",\"collateral_index\":\"OTHER\"",
        1,
    );
    assert!(serde_json::from_str::<Request>(&text)
        .unwrap_err()
        .to_string()
        .contains("duplicate field"));
}
#[test]
fn false_adoption_flags_remain_explicit() {
    let r = evaluate(&serde_json::from_value(raw()).unwrap()).unwrap();
    assert!(
        !r.production_adoption
            && !r.observed_fixings
            && !r.physical_accounting
            && !r.joint_dynamics
    );
    assert!(
        !r.calendar_mapping_verified
            && !r.accounting_adoption
            && !r.capital_ftp_eligible
            && !r.empirical_validation
    );
}
