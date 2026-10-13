//! Original-byte JSON admission shared by explicitly selected native contracts.
//! Historical Value-first decoders are not changed by this additive utility.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use std::fmt;
use std::io::{self, Write};

struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(value.into())))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Unique, E> {
                Ok(Unique(Value::Number(value.into())))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Unique, E> {
                Number::from_f64(value)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(value.into())))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Unique, E> {
                Ok(Unique(Value::String(value)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(value)) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some(key) = object.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate object key `{key}`")));
                    }
                    let Unique(value) = object.next_value()?;
                    values.insert(key, value);
                }
                Ok(Unique(Value::Object(values)))
            }
        }
        decoder.deserialize_any(JsonVisitor)
    }
}

/// Byte and serde recursion limits precede typed financial calculation.
/// Integer/float categories and signed zero are retained in the returned Value;
/// textual spelling/whitespace is not a byte-preserving JSON representation.
pub fn parse(bytes: &[u8], maximum_bytes: usize) -> Result<Value, String> {
    if bytes.len() > maximum_bytes {
        return Err("original JSON exceeds selected byte admission".into());
    }
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|e| e.to_string())
}

/// Additive selected boundary: same duplicate-preserving decoder, with a native
/// probe at each bounded read. Historical parse() retains its exact implementation.
pub fn parse_checked(
    bytes: &[u8],
    maximum_bytes: usize,
    mut probe: impl FnMut() -> Result<(), String>,
) -> Result<Value, String> {
    if bytes.len() > maximum_bytes {
        return Err("original JSON exceeds selected byte admission".into());
    }
    struct Checked<'a, F> {
        bytes: &'a [u8],
        probe: F,
    }
    impl<F: FnMut() -> Result<(), String>> std::io::Read for Checked<'_, F> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            (self.probe)().map_err(io::Error::other)?;
            let count = output.len().min(32768).min(self.bytes.len());
            output[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            Ok(count)
        }
    }
    probe()?;
    let reader = std::io::BufReader::with_capacity(
        32768,
        Checked {
            bytes,
            probe: &mut probe,
        },
    );
    let value = serde_json::from_reader::<_, Unique>(reader)
        .map(|v| v.0)
        .map_err(|e| e.to_string())?;
    probe()?;
    Ok(value)
}

struct BoundedWriter {
    size: usize,
    maximum: usize,
    retained: Option<Vec<u8>>,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let size = self
            .size
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("JSON byte count overflow"))?;
        if size > self.maximum {
            return Err(io::Error::other(
                "serialized JSON exceeds selected byte admission",
            ));
        }
        if let Some(retained) = &mut self.retained {
            retained.extend_from_slice(bytes);
        }
        self.size = size;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Count escaped, typed JSON before allocation of a financial source or clone.
pub fn serialized_size<T: Serialize>(value: &T, maximum_bytes: usize) -> Result<usize, String> {
    let mut writer = BoundedWriter {
        size: 0,
        maximum: maximum_bytes,
        retained: None,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    Ok(writer.size)
}

/// Stage a bounded serialization without first cloning it into a JSON Value.
/// The cap concerns serialized bytes, not total allocator capacity or process RSS.
pub fn encode<T: Serialize>(value: &T, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let mut writer = BoundedWriter {
        size: 0,
        maximum: maximum_bytes,
        retained: Some(Vec::new()),
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    writer
        .retained
        .ok_or_else(|| "missing JSON staging buffer".into())
}
