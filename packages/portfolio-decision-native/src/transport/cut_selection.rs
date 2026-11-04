//! Borrow the original wire while selecting the cut CLI before any Value tree.
//! Every top-level operation is examined; strict parsing still rejects duplicates.
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::fmt;

const OP: &str = "analyze_commercial_cut_mixed_journal";

struct Key(bool);
impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl Visitor<'_> for Name {
            type Value = Key;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("operation key")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Key, E> {
                Ok(Key(value == "op"))
            }
        }
        d.deserialize_str(Name)
    }
}
struct Operation(bool);
impl<'de> Deserialize<'de> for Operation {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl Visitor<'_> for Name {
            type Value = Operation;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("operation string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Operation, E> {
                Ok(Operation(value == OP))
            }
        }
        d.deserialize_str(Name)
    }
}
struct Selection(bool);
impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Select;
        impl<'de> Visitor<'de> for Select {
            type Value = Selection;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("strategy envelope")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Selection, A::Error> {
                let mut selected = false;
                while let Some(Key(operation)) = map.next_key()? {
                    // Borrow even nested request bodies. No Box<RawValue>, key
                    // String, full Value or typed financial request is retained.
                    let raw: &'de RawValue = map.next_value()?;
                    if operation {
                        selected |=
                            serde_json::from_str::<Operation>(raw.get()).is_ok_and(|value| value.0);
                    }
                }
                Ok(Selection(selected))
            }
        }
        d.deserialize_map(Select)
    }
}

/// This only selects the existing handler, never authorizes a request. Malformed
/// and non-object inputs retain the historical dispatcher's validation/error path.
pub fn is_selected(bytes: &[u8]) -> Result<bool, String> {
    if bytes.len() > 128 << 20 {
        return Err("request exceeds 128 MiB".into());
    }
    Ok(serde_json::from_slice::<Selection>(bytes).is_ok_and(|s| s.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_original_operation_is_checked_including_escaped_duplicates() {
        for raw in [
            r#"{"op":"analyze_commercial_cut_mixed_journal"}"#,
            r#"{"op":"legacy","op":"analyze_commercial_cut_mixed_journal"}"#,
            r#"{"op":"analyze_commercial_cut_mixed_journal","op":"legacy"}"#,
            r#"{"o\u0070":"analyze_commercial_cut_mixed_journal","op":null}"#,
            r#"{"op":"\u0061nalyze_commercial_cut_mixed_journal"}"#,
        ] {
            assert!(is_selected(raw.as_bytes()).unwrap(), "{raw}");
        }
        let raw = br#"{"op":"analyze_commercial_cut_mixed_journal","op":"legacy"}"#;
        assert!(portfolio_model_core::strict_json::parse(raw, 1000)
            .unwrap_err()
            .contains("duplicate object key"));
    }

    #[test]
    fn nested_operations_and_unrelated_or_malformed_inputs_keep_legacy_routing() {
        for raw in [
            r#"{"request":{"op":"analyze_commercial_cut_mixed_journal"},"op":"legacy"}"#,
            r#"{"op":"legacy","schema":"commercial-cut-mixed-journal-1"}"#,
            r#"{"op":["analyze_commercial_cut_mixed_journal"]}"#,
            "null",
            "[]",
            "{",
            "{} true",
        ] {
            assert!(!is_selected(raw.as_bytes()).unwrap(), "{raw}");
        }
    }
}
