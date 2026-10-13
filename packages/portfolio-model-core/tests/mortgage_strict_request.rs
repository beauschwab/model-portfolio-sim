//! Parsing gates for the new acquired source; historical shared parsing is kept.
use portfolio_model_core::{mortgage_sequential_carry::Request, mortgage_state::MortgageRequest};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/mortgage_receipts/request.json")).unwrap()
}
fn rejected(v: Value, path: &str, field: &str) {
    let error = serde_json::from_value::<Request>(v)
        .unwrap_err()
        .to_string();
    assert!(error.contains(path), "{error}");
    assert!(error.contains(field), "{error}");
}
fn fixed_extras(role: &str) {
    for (field, value) in [
        ("annual_rate", json!(0)),
        ("annual_rate", Value::Null),
        ("terms", json!({})),
        ("resets", json!([])),
    ] {
        let mut v = raw();
        v[role]["rate"][field] = value;
        // The historical parser still accepts the ignored unit-variant field.
        // The stricter acquired contract is the only changed entrypoint.
        assert!(serde_json::from_value::<MortgageRequest>(v[role].clone()).is_ok());
        rejected(
            v,
            &format!("{role}.rate"),
            &format!("unknown field `{field}`"),
        );
    }
}
#[test]
fn current_fixed_rate_rejects_all_ignored_extra_fields() {
    fixed_extras("contract");
}
#[test]
fn original_fixed_rate_rejects_all_ignored_extra_fields() {
    fixed_extras("original_contract");
}
#[test]
fn ordinary_wire_roundtrip_preserves_all_typed_input_bytes() {
    let v = raw();
    let request: Request = serde_json::from_value(v.clone()).unwrap();
    for (role, strict) in [
        ("contract", &request.contract),
        ("original_contract", &request.original_contract),
    ] {
        let historical: MortgageRequest = serde_json::from_value(v[role].clone()).unwrap();
        assert_eq!(
            serde_json::to_vec(strict).unwrap(),
            serde_json::to_vec(&historical).unwrap()
        );
    }
    assert_eq!(serde_json::to_value(request).unwrap(), v);
}
#[test]
fn shared_required_unknown_and_numeric_domains_still_reject_with_role() {
    for role in ["contract", "original_contract"] {
        let mut v = raw();
        v[role].as_object_mut().unwrap().remove("rate");
        rejected(v, role, "missing field `rate`");
        let mut v = raw();
        v[role]["identity"]["unused"] = json!(0);
        rejected(v, role, "unknown field `unused`");
        let mut v = raw();
        v[role]["rate"] = json!({"kind":"arm", "resets":[]});
        rejected(v, role, "missing field `terms`");
        let mut v = raw();
        v[role]["horizon_days"] = json!(true);
        rejected(v, role, "invalid type");
        let mut v = raw();
        v[role]["as_of_ordinal"] = json!(739617.5);
        rejected(v, role, "invalid type");
    }
}
#[test]
fn contract_plain_unit_enums_require_string_wire_shape() {
    for role in ["contract", "original_contract"] {
        for fields in [["identity", "measure"].as_slice(), ["day_count"].as_slice()] {
            let mut v = raw();
            let mut at = &mut v[role];
            for field in fields {
                at = &mut at[*field];
            }
            let variant = at.as_str().unwrap().to_owned();
            *at = Value::Object([(variant, Value::Null)].into_iter().collect());
            assert!(serde_json::from_value::<MortgageRequest>(v[role].clone()).is_ok());
            rejected(v, &format!("{role}.{}", fields.join(".")), "enum string");
        }
    }
}
#[test]
fn arm_and_agency_plain_unit_enums_are_strict_only_at_new_boundary() {
    let files = std::fs::read_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mortgage_receipts/native"),
    )
    .unwrap();
    let mut checked = [false; 3];
    for file in files {
        let case: Value =
            serde_json::from_slice(&std::fs::read(file.unwrap().path()).unwrap()).unwrap();
        let contract = &case["raw_request"];
        for (index, fields) in [
            ["rate", "terms", "index", "kind"].as_slice(),
            ["rate", "terms", "payment_recast"].as_slice(),
            ["ownership", "default_coupon_policy"].as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            let mut at = contract;
            for field in fields {
                at = &at[*field];
            }
            let Some(variant) = at.as_str() else {
                continue;
            };
            for role in ["contract", "original_contract"] {
                let mut v = raw();
                v[role] = contract.clone();
                let mut at = &mut v[role];
                for field in fields {
                    at = &mut at[*field];
                }
                *at = Value::Object([(variant.to_owned(), Value::Null)].into_iter().collect());
                assert!(serde_json::from_value::<MortgageRequest>(v[role].clone()).is_ok());
                rejected(v, &format!("{role}.{}", fields.join(".")), "enum string");
            }
            checked[index] = true;
        }
    }
    assert_eq!(checked, [true; 3]);
}
#[test]
fn reviewed_interest_unit_enums_require_string_wire_shape() {
    for field in ["accounting_accrued", "economic_remainder"] {
        let mut v = raw();
        let variant = v["policy"]["interest"][field].as_str().unwrap().to_owned();
        v["policy"]["interest"][field] =
            Value::Object([(variant, Value::Null)].into_iter().collect());
        rejected(v, &format!("policy.interest.{field}"), "enum string");
    }
}
#[test]
fn duplicate_raw_keys_are_not_erased_by_shape_validation() {
    for role in ["contract", "original_contract"] {
        let contract = serde_json::to_string(&raw()[role]).unwrap();
        for (needle, duplicate) in [
            (
                "\"rate\":{\"kind\":\"fixed\"}",
                "\"rate\":{\"kind\":\"fixed\",\"kind\":\"fixed\"}",
            ),
            (
                "\"measure\":\"physical\"",
                "\"measure\":\"physical\",\"measure\":\"physical\"",
            ),
        ] {
            let replaced = contract.replace(needle, duplicate);
            assert_ne!(contract, replaced);
            let mut v = raw();
            v.as_object_mut().unwrap().remove(role);
            let body = serde_json::to_string(&v).unwrap();
            let text = format!("{},\"{role}\":{replaced}}}", &body[..body.len() - 1]);
            let error = serde_json::from_str::<Request>(&text)
                .unwrap_err()
                .to_string();
            assert!(error.contains("duplicate field"), "{error}");
            assert!(error.contains(role), "{error}");
        }
    }
    let text = serde_json::to_string(&raw()).unwrap().replace(
        "\"accounting_accrued\":\"reverse_income_at_default\"",
        "\"accounting_accrued\":\"reverse_income_at_default\",\"accounting_accrued\":\"reverse_income_at_default\"",
    );
    let error = serde_json::from_str::<Request>(&text)
        .unwrap_err()
        .to_string();
    assert!(error.contains("policy.interest"), "{error}");
    assert!(
        error.contains("duplicate field `accounting_accrued`"),
        "{error}"
    );
}
