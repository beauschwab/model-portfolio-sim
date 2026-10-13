//! Exact raw shape at this additive request field, preserving the shared enum.
//! Duplicate keys are rejected when presented to this map visitor. An outer
//! serde_json::Value parse can already have erased duplicates; this helper
//! does not claim byte-level rejection through such an outer boundary.
use super::ObligationDisposition;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserializer,
};
use serde_json::{Map, Value};
use std::fmt;

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ObligationDisposition, D::Error> {
    struct Object;
    impl<'de> Visitor<'de> for Object {
        type Value = Map<String, Value>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("disposition must be an object with an explicit kind string")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut fields = Map::new();
            while let Some(key) = access.next_key::<String>()? {
                if fields.contains_key(&key) {
                    return Err(de::Error::custom(format!(
                        "disposition: duplicate field `{key}`"
                    )));
                }
                fields.insert(key, access.next_value::<Value>()?);
            }
            Ok(fields)
        }
    }
    let fields = deserializer.deserialize_map(Object)?;
    let kind = fields
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| de::Error::custom("disposition.kind must be an enum string"))?;
    let allowed: &[&str] = match kind {
        "retain_to_closeout" => &["kind"],
        "settle_on_node" => &["kind", "ordinal", "evidence"],
        _ => {
            return Err(de::Error::custom(format!(
                "unsupported disposition.kind `{kind}`"
            )))
        }
    };
    for key in fields.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(de::Error::custom(format!(
                "unknown field `{key}` in disposition `{kind}`"
            )));
        }
    }
    serde_json::from_value(Value::Object(fields)).map_err(de::Error::custom)
}
