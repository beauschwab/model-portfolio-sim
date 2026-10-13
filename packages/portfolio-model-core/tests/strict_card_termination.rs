use portfolio_model_core::{card_credit_accounting as v1, card_credit_accounting_v2 as v2};
use serde_json::{json, Value};

fn examples() -> [Value; 2] {
    [
        serde_json::from_str(include_str!(
            "../../../examples/models/card-credit-accounting-1.json"
        ))
        .unwrap(),
        serde_json::from_str(include_str!(
            "../../../examples/models/card-credit-accounting-2.json"
        ))
        .unwrap(),
    ]
}
fn decode(raw: Value, version: usize) -> Result<Value, String> {
    if version == 0 {
        serde_json::from_value::<v1::Request>(raw)
            .map(|r| serde_json::to_value(r).unwrap())
            .map_err(|e| e.to_string())
    } else {
        serde_json::from_value::<v2::Request>(raw)
            .map(|r| serde_json::to_value(r).unwrap())
            .map_err(|e| e.to_string())
    }
}
#[test]
fn both_canonical_termination_variants_preserve_card1_and_card2_wire() {
    for (version, mut raw) in examples().into_iter().enumerate() {
        for termination in [
            json!({"kind":"source_default"}),
            json!({"kind":"reviewed_zero_rights_closure","ordinal":738917,"review":{"id":"review","revision":"r1"}}),
        ] {
            raw["termination"] = termination;
            assert_eq!(decode(raw.clone(), version).unwrap(), raw);
        }
    }
}
#[test]
fn both_new_card_requests_reject_all_ignored_source_default_fields() {
    for (version, raw) in examples().into_iter().enumerate() {
        for (key, value) in [
            ("ordinal", json!(0)),
            ("review", Value::Null),
            ("amount", json!(0.)),
            ("economic_override", json!(false)),
        ] {
            let mut bad = raw.clone();
            bad["termination"] = json!({"kind":"source_default"});
            bad["termination"][key] = value;
            let e = decode(bad, version).unwrap_err();
            assert!(
                e.contains(&format!("unknown field `{key}` in termination")),
                "{e}"
            );
        }
    }
}
#[test]
fn reviewed_closure_requires_its_fields_and_strict_kind_string() {
    for (version, raw) in examples().into_iter().enumerate() {
        for (termination, expected) in [
            (
                json!({"kind":"reviewed_zero_rights_closure","ordinal":738917}),
                "missing field",
            ),
            (
                json!({"kind":"reviewed_zero_rights_closure","review":{"id":"r","revision":"1"}}),
                "missing field",
            ),
            (json!({"kind":"unowned"}), "unsupported termination.kind"),
            (
                json!({"kind":{"source_default":null}}),
                "termination.kind must be an enum string",
            ),
        ] {
            let mut bad = raw.clone();
            bad["termination"] = termination;
            assert!(decode(bad, version).unwrap_err().contains(expected));
        }
    }
}
#[test]
fn both_direct_card_byte_requests_reject_duplicate_termination_kind() {
    for (version, mut raw) in examples().into_iter().enumerate() {
        raw["termination"] = json!({"kind":"source_default"});
        let bytes = serde_json::to_string(&raw).unwrap().replace(
            "{\"kind\":\"source_default\"}",
            "{\"kind\":\"source_default\",\"kind\":\"source_default\"}",
        );
        let e = if version == 0 {
            serde_json::from_str::<v1::Request>(&bytes)
                .err()
                .unwrap()
                .to_string()
        } else {
            serde_json::from_str::<v2::Request>(&bytes)
                .err()
                .unwrap()
                .to_string()
        };
        assert!(e.contains("termination: duplicate field `kind`"), "{e}");
    }
}
#[test]
fn shared_historical_termination_enum_is_not_rewritten() {
    let old: v1::Termination =
        serde_json::from_value(json!({"kind":"source_default","ordinal":0})).unwrap();
    assert_eq!(
        serde_json::to_value(old).unwrap(),
        json!({"kind":"source_default"})
    );
}
