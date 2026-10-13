use portfolio_ledger_native::deposit_source_portfolio as owner;
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/deposit-portfolio-v2/portfolio-40-2.json"
    ))
    .unwrap()
}
fn envelope() -> Vec<u8> {
    serde_json::to_vec(&json!({"request":raw(),"execution_timeout_ms":60_000})).unwrap()
}
#[test]
fn strict_raw_envelope_retains_typed_sources_and_original_signed_zero() {
    let mut v = raw();
    v["specification"]["positions"][0]["rate"] = json!(-0.0);
    let (r, t, retained) = owner::parse_envelope(
        &serde_json::to_vec(&json!({"request":v,"execution_timeout_ms":60_000})).unwrap(),
    )
    .unwrap();
    assert_eq!(t, 60_000);
    assert!(retained["specification"]["positions"][0]["rate"]
        .as_f64()
        .unwrap()
        .is_sign_negative());
    assert_eq!(r.positions.len(), 2);
    assert!(owner::audit(&r).unwrap().work_units < owner::MAX_WORK);
}
#[test]
fn literal_duplicates_refuse_in_original_envelope_and_nested_values() {
    let original = String::from_utf8(envelope()).unwrap();
    for edited in [original.replace("\"execution_timeout_ms\":60000","\"execution_timeout_ms\":60000,\"execution_timeout_ms\":60000"),
 original.replace("\"schema\":\"deposit-performing-portfolio-ledger-2\"","\"schema\":\"deposit-performing-portfolio-ledger-2\",\"schema\":\"deposit-performing-portfolio-ledger-2\"")]{
  assert_ne!(edited,original);assert!(owner::parse_envelope(edited.as_bytes()).is_err());
 }
}
#[test]
fn envelope_unknown_fields_and_noninteger_or_unbounded_deadlines_refuse() {
    for timeout in [json!(0), json!(true), json!(60000.0), json!(1_800_001)] {
        assert!(owner::parse_envelope(
            &serde_json::to_vec(&json!({"request":raw(),"execution_timeout_ms":timeout})).unwrap()
        )
        .is_err());
    }
    assert!(owner::parse_envelope(
        &serde_json::to_vec(
            &json!({"request":raw(),"execution_timeout_ms":60000,"caller_state":{}})
        )
        .unwrap()
    )
    .is_err());
}
#[test]
fn source_bool_cannot_be_normalized_to_financial_number() {
    let mut r = raw();
    r["positions"][0]["paths"][0]["source"]["continuation"]["anchor"]["original_principal"] =
        json!(true);
    assert!(owner::parse_envelope(
        &serde_json::to_vec(&json!({"request":r,"execution_timeout_ms":60000})).unwrap()
    )
    .is_err());
}
#[test]
fn old_schema_cannot_select_new_owner_without_explicit_source2() {
    let mut r = raw();
    r["schema"] = json!("deposit-performing-income-ledger-1");
    let (r, _, _) = owner::parse_envelope(
        &serde_json::to_vec(&json!({"request":r,"execution_timeout_ms":60000})).unwrap(),
    )
    .unwrap();
    assert!(owner::audit(&r).is_err());
}
