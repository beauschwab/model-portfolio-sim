//! Strict raw shape only at the new acquired-owner boundary. Shared historical
//! mortgage deserialization and all financial validation remain unchanged.
use crate::mortgage_default_interest::DefaultInterestOwnership;
use crate::mortgage_state::MortgageRequest;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::fmt;

// A plain Value would silently replace duplicate object keys before the shared
// typed parser could reject them. Preserve that strict contract recursively.
struct Raw(Value);
impl<'de> Deserialize<'de> for Raw {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RawVisitor;
        impl<'de> Visitor<'de> for RawVisitor {
            type Value = Raw;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON with unique object fields")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Raw, E> {
                Ok(Raw(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Raw, E> {
                Ok(Raw(Value::Number(v.into())))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Raw, E> {
                Ok(Raw(Value::Number(v.into())))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Raw, E> {
                Number::from_f64(v)
                    .map(|n| Raw(Value::Number(n)))
                    .ok_or_else(|| E::custom("finite raw JSON number required"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Raw, E> {
                Ok(Raw(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Raw, E> {
                Ok(Raw(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Raw, E> {
                Ok(Raw(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Raw, A::Error> {
                let mut values = Vec::new();
                while let Some(Raw(value)) = seq.next_element::<Raw>()? {
                    values.push(value);
                }
                Ok(Raw(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Raw, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate field `{key}`")));
                    }
                    let Raw(value) = map.next_value::<Raw>()?;
                    values.insert(key, value);
                }
                Ok(Raw(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(RawVisitor)
    }
}

fn enum_string(value: &Value, fields: &[&str], path: &str) -> Result<(), String> {
    let mut at = value;
    for field in fields {
        let Some(next) = at.get(field) else {
            // Missing fields/invalid parent shapes are rejected by the shared
            // typed parser rather than supplied with any new default.
            return Ok(());
        };
        at = next;
    }
    if !at.is_string() {
        return Err(format!(
            "{path}.{} must be an enum string",
            fields.join(".")
        ));
    }
    Ok(())
}

fn contract<'de, D: Deserializer<'de>>(
    deserializer: D,
    path: &str,
) -> Result<MortgageRequest, D::Error> {
    let Raw(value) =
        Raw::deserialize(deserializer).map_err(|e| de::Error::custom(format!("{path}: {e}")))?;
    if value["rate"]["kind"] == "fixed" {
        if let Some(fields) = value["rate"].as_object() {
            if let Some(extra) = fields.keys().find(|key| key.as_str() != "kind") {
                return Err(de::Error::custom(format!(
                    "unknown field `{extra}` in {path}.rate; fixed accepts only `kind`"
                )));
            }
        }
    }
    for fields in [
        ["identity", "measure"].as_slice(),
        ["day_count"].as_slice(),
        ["rate", "kind"].as_slice(),
        ["ownership", "kind"].as_slice(),
        ["rate", "terms", "index", "kind"].as_slice(),
        ["rate", "terms", "payment_recast"].as_slice(),
        ["ownership", "default_coupon_policy"].as_slice(),
    ] {
        enum_string(&value, fields, path).map_err(de::Error::custom)?;
    }
    serde_json::from_value(value).map_err(|e| de::Error::custom(format!("{path}: {e}")))
}

pub(super) fn current<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<MortgageRequest, D::Error> {
    contract(deserializer, "contract")
}
pub(super) fn original<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<MortgageRequest, D::Error> {
    contract(deserializer, "original_contract")
}

pub(super) fn interest<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<DefaultInterestOwnership, D::Error> {
    let path = "policy.interest";
    let Raw(value) =
        Raw::deserialize(deserializer).map_err(|e| de::Error::custom(format!("{path}: {e}")))?;
    for field in ["accounting_accrued", "economic_remainder"] {
        enum_string(&value, &[field], path).map_err(de::Error::custom)?;
    }
    serde_json::from_value(value).map_err(|e| de::Error::custom(format!("{path}: {e}")))
}
