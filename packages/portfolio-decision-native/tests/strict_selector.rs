use portfolio_decision_native::product_library::{FaceLimit, Selector};
use serde_json::{json, Value};
fn variants() -> Vec<Value> {
    serde_json::from_str(r#"[{"kind":"all_assets"},{"kind":"all_liabilities"},{"kind":"unit","unit_id":"u"},{"kind":"template","template_id":"t"},{"kind":"accounting_category","accounting_category":"ac"}]"#).unwrap()
}
fn raw(inner: Value) -> Value {
    let mut v: Value =
        serde_json::from_str(r#"{"label":"cap","policy_id":"review","sense":"<=","amount":100.0}"#)
            .unwrap();
    v["selector"] = inner;
    v
}
fn error(v: Value, contains: &str) {
    let e = serde_json::from_value::<FaceLimit>(v)
        .map(|_| ())
        .expect_err("invalid raw shape must reject")
        .to_string();
    assert!(e.contains(contains), "{e}");
}
#[test]
fn canonical_variants_preserve_exact_typed_wire() {
    for v in variants() {
        let input = raw(v);
        let typed: FaceLimit = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), input);
    }
}
#[test]
fn unit_variant_extras_reject_including_zero_null_and_narrowing_keys() {
    for v in variants()
        .into_iter()
        .filter(|v| v.as_object().unwrap().len() == 1)
    {
        for (key, value) in [
            ("amount", json!(0)),
            ("ordinal", Value::Null),
            ("unit_id", json!("narrow-only")),
            ("principal_fraction", json!([0, 0, 0])),
        ] {
            let mut bad = v.clone();
            bad[key] = value;
            error(raw(bad), &format!("unknown field `{key}` in selector"));
        }
    }
}
#[test]
fn tagged_struct_required_fields_unknown_kind_and_kind_alias_reject() {
    for v in variants() {
        let mut bad = v.clone();
        bad["unowned"] = json!(0);
        error(raw(bad), "unknown field `unowned`");
        for key in v
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| k.as_str() != "kind")
        {
            let mut bad = v.clone();
            bad.as_object_mut().unwrap().remove(key);
            error(raw(bad), "missing field");
        }
    }
    error(raw(json!({"kind":"unowned"})), "unsupported selector.kind");
    error(
        raw(json!({"kind":{"unowned":null}})),
        "selector.kind must be an enum string",
    );
    error(raw(json!({})), "selector.kind must be an enum string");
}
#[test]
fn direct_raw_bytes_reject_duplicate_field_before_typed_enum_delegation() {
    let v = variants().remove(0);
    let one = serde_json::to_string(&v).unwrap();
    let duplicate = format!("{},\"kind\":{}}}", &one[..one.len() - 1], v["kind"]);
    let bytes = serde_json::to_string(&raw(v))
        .unwrap()
        .replace(&one, &duplicate);
    let e = serde_json::from_str::<FaceLimit>(&bytes)
        .err()
        .unwrap()
        .to_string();
    assert!(e.contains("selector: duplicate field `kind`"), "{e}");
}
#[test]
fn historical_shared_enum_remains_compatible() {
    let mut v = variants().remove(0);
    v["ignored_historical_field"] = json!(0);
    let old: Selector = serde_json::from_value(v).unwrap();
    assert_eq!(serde_json::to_value(old).unwrap(), variants()[0]);
}
