//! Additive selected branch. Input bytes, not an outer generic Value peek, own the request.
use super::*;
use serde::{
    de::{Error, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Number;
use std::{fmt, io::Write, time::Instant};
pub const OP: &str = "prepare_modeled_product_ftp";
pub const MAX_BODY: usize = 1_048_576;
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "duplicate-free finite JSON")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Strict, M::Error> {
                let mut x = serde_json::Map::new();
                while let Some(k) = m.next_key::<String>()? {
                    if x.contains_key(&k) {
                        return Err(M::Error::custom(format!("duplicate field {k}")));
                    }
                    let v = m.next_value::<Strict>()?;
                    x.insert(k, v.0);
                }
                Ok(Strict(Value::Object(x)))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Strict, S::Error> {
                let mut x = Vec::new();
                while let Some(v) = s.next_element::<Strict>()? {
                    x.push(v.0)
                }
                Ok(Strict(Value::Array(x)))
            }
            fn visit_bool<E: Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(Value::Number(Number::from(v))))
            }
            fn visit_u64<E: Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(Value::Number(Number::from(v))))
            }
            fn visit_f64<E: Error>(self, v: f64) -> Result<Strict, E> {
                Number::from_f64(v)
                    .map(|x| Strict(Value::Number(x)))
                    .ok_or(E::custom("finite binary64 required"))
            }
            fn visit_str<E: Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(Value::String(v.into())))
            }
            fn visit_string<E: Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(Value::String(v)))
            }
            fn visit_unit<E: Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn strings(v: &Value, path: &str) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let p = format!("{path}.{k}");
                if [
                    "measure",
                    "day_count",
                    "kind",
                    "initial_state",
                    "payment_recast",
                    "default_coupon_policy",
                ]
                .contains(&k.as_str())
                    && !v.is_string()
                {
                    return Err(format!("{p} must be an enum string"));
                }
                strings(v, &p)?;
            }
        }
        Value::Array(xs) => {
            for (i, x) in xs.iter().enumerate() {
                strings(x, &format!("{path}[{i}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    op: String,
    schema: String,
    execution_timeout_ms: u64,
    request: Value,
}
pub fn execute(input: &[u8], mut output: impl Write) -> Result<(), String> {
    let start = Instant::now();
    if input.len() > MAX_BODY {
        return Err("selected FTP envelope exceeds1MiB".into());
    }
    let original: Strict = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let envelope: Envelope = serde_json::from_value(original.0).map_err(|e| e.to_string())?;
    if envelope.op != OP || envelope.schema != SCHEMA {
        return Err("unsupported raw FTP operation/schema".into());
    }
    strings(&envelope.request, "request")?;
    let request: Request =
        serde_json::from_value(envelope.request.clone()).map_err(|e| e.to_string())?;
    if !(1..=1_800_000).contains(&envelope.execution_timeout_ms) {
        return Err("finite bounded original execution timeout required".into());
    }
    let elapsed = u64::try_from(start.elapsed().as_millis()).map_err(|_| "deadline overflow")?;
    let remaining = envelope
        .execution_timeout_ms
        .checked_sub(elapsed)
        .filter(|t| *t > 0)
        .ok_or("COMPUTE_DEADLINE_EXCEEDED")?;
    let control = portfolio_compute_control::Control::new(remaining)?;
    portfolio_compute_control::scope(Some(control.clone()), || {
        let mut result = evaluate(request)?;
        result["raw_request"] = envelope.request.clone();
        result["source_preparation"]["raw_request"] = envelope.request["source"].clone();
        let value = json!({"raw_input":envelope.request,"result":result});
        let bytes = serde_json::to_vec(&json!({"ok":value})).map_err(|e| e.to_string())?;
        if bytes.len() >= 32 * 1024 * 1024 {
            return Err("complete FTP output exceeds32MiB".into());
        }
        portfolio_compute_control::checkpoint()?;
        output.write_all(&bytes).map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()?;
        output.write_all(b"\n").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()
    })?;
    control.check()
}
