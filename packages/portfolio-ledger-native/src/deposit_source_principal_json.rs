use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::fmt;
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

pub fn value(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > 1_048_576 {
        return Err("raw envelope exceeds1MiB".into());
    }
    let Raw(v) = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    Ok(v)
}

pub fn parse(bytes: &[u8]) -> Result<(crate::deposit_source_principal::Request, Value), String> {
    if bytes.len() > 1_048_576 {
        return Err("raw request exceeds1MiB".into());
    }
    let Raw(v) = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if !v["recognition_policy"]["fees"].is_string() {
        return Err("fees string required".into());
    }
    for run in v["runs"].as_array().ok_or("explicit runs array required")? {
        crate::deposit_source_principal::source_parse(
            &serde_json::to_vec(&run["source"]).map_err(|e| e.to_string())?,
        )?;
    }
    let request = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok((request, v))
}
