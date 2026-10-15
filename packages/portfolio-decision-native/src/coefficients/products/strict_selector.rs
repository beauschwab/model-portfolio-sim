//! Exact raw shape at this additive request field, preserving the shared enum.
//! Duplicate keys are rejected when presented to this map visitor. An outer
//! serde_json::Value parse can already have erased duplicates; this helper
//! does not claim byte-level rejection through such an outer boundary.
use super::Selector;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserializer,
};
use serde_json::{Map, Value};
use std::fmt;

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Selector, D::Error> {
    struct Object;
    impl<'de> Visitor<'de> for Object {
        type Value = Map<String, Value>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("selector must be an object with an explicit kind string")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut fields = Map::new();
            while let Some(key) = access.next_key::<String>()? {
                if fields.contains_key(&key) {
                    return Err(de::Error::custom(format!(
                        "selector: duplicate field `{key}`"
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
        .ok_or_else(|| de::Error::custom("selector.kind must be an enum string"))?;
    let allowed: &[&str] = match kind {
        "all_assets" => &["kind"],
        "all_liabilities" => &["kind"],
        "unit" => &["kind", "unit_id"],
        "template" => &["kind", "template_id"],
        "accounting_category" => &["kind", "accounting_category"],
        _ => {
            return Err(de::Error::custom(format!(
                "unsupported selector.kind `{kind}`"
            )))
        }
    };
    for key in fields.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(de::Error::custom(format!(
                "unknown field `{key}` in selector `{kind}`"
            )));
        }
    }
    serde_json::from_value(Value::Object(fields)).map_err(de::Error::custom)
}
