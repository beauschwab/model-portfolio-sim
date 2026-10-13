//! New selected graph2 boundary only. Historical graph1 serde is unchanged.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::fmt;
pub struct Raw(pub Value);
impl<'de> Deserialize<'de> for Raw {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Raw;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("duplicate-free raw graph2 JSON")
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
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Raw, E> {
                Ok(Raw(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Raw, E> {
                Ok(Raw(Value::String(v)))
            }
            fn visit_none<E: de::Error>(self) -> Result<Raw, E> {
                Ok(Raw(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Raw, E> {
                Ok(Raw(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Raw, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_element::<Raw>()? {
                    portfolio_compute_control::checkpoint().map_err(de::Error::custom)?;
                    v.push(x.0);
                }
                Ok(Raw(Value::Array(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Raw, A::Error> {
                let mut v = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    portfolio_compute_control::checkpoint().map_err(de::Error::custom)?;
                    if v.contains_key(&k) {
                        return Err(de::Error::custom(format!("duplicate field `{k}`")));
                    }
                    v.insert(k, a.next_value::<Raw>()?.0);
                }
                Ok(Raw(Value::Object(v)))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn validate(v: &Value, path: &str) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let p = format!("{path}.{k}");
                if matches!(
                    k.as_str(),
                    "direction"
                        | "measure"
                        | "day_count"
                        | "right_tail"
                        | "cash_timing"
                        | "channel"
                        | "penalty_limit"
                ) && !x.is_string()
                {
                    return Err(format!("{p} must be an enum string"));
                }
                validate(x, &p)?;
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                validate(x, &format!("{path}[{i}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn typed(v: Value) -> Result<crate::products::term::graph::Request, String> {
    validate(&v, "request")?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Strict raw deserialization is selected only by the new outer protocol variant.
#[derive(serde::Serialize)]
#[serde(transparent)]
pub(crate) struct SequenceRequest(pub crate::products::term::graph::Request);
impl<'de> serde::Deserialize<'de> for SequenceRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Raw::deserialize(d)?;
        typed(raw.0)
            .map(SequenceRequest)
            .map_err(serde::de::Error::custom)
    }
}
