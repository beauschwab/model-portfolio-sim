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

/// Exact serializer-byte equality with only the smaller encoding retained.
///
/// Both complete sizes are admitted before staging. The other serializer writes
/// directly against that immutable encoding: no second encoding, Value tree,
/// digest, financial projection or thread is created. A mismatch does not stop
/// traversal, so later serializer, capacity and cancellation errors still fail.
/// These are byte/component limits, not allocator overhead or process RSS bounds.
pub fn serialized_equal_checked<A: Serialize, B: Serialize>(
    left: &A,
    right: &B,
    maximum_bytes: usize,
    maximum_combined_bytes: usize,
    mut probe: impl FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    probe()?;
    let left_size = checked_size(left, maximum_bytes, &mut probe)?;
    let right_size = checked_size(right, maximum_bytes, &mut probe)?;
    let combined = left_size
        .checked_add(right_size)
        .ok_or("JSON comparison byte count overflow")?;
    if combined > maximum_combined_bytes {
        return Err("JSON comparison exceeds combined byte admission".into());
    }
    let equal = if left_size <= right_size {
        compare_encoding(left, right, left_size, right_size, &mut probe)?
    } else {
        compare_encoding(right, left, right_size, left_size, &mut probe)?
    };
    probe()?;
    Ok(equal)
}

struct ProbedWriter<'a, W, F> {
    output: W,
    probe: &'a mut F,
}
impl<W: Write, F: FnMut() -> Result<(), String>> Write for ProbedWriter<'_, W, F> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // A serializer may submit an entire unescaped string in one write.
        // Bound the interval between probes even for that case.
        for chunk in bytes.chunks(32768) {
            (self.probe)().map_err(io::Error::other)?;
            self.output.write_all(chunk)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        (self.probe)().map_err(io::Error::other)?;
        self.output.flush()
    }
}

fn checked_size<T: Serialize, F: FnMut() -> Result<(), String>>(
    value: &T,
    maximum: usize,
    probe: &mut F,
) -> Result<usize, String> {
    let mut writer = ProbedWriter {
        output: BoundedWriter {
            size: 0,
            maximum,
            retained: None,
        },
        probe,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    Ok(writer.output.size)
}

struct CompareWriter<'a> {
    expected: &'a [u8],
    offset: usize,
    maximum: usize,
    equal: bool,
    overrun: &'static str,
}
impl Write for CompareWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("JSON comparison offset overflow"))?;
        if end > self.maximum {
            return Err(io::Error::other(self.overrun));
        }
        self.equal &= self.expected.get(self.offset..end) == Some(bytes);
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn compare_encoding<A: Serialize, B: Serialize, F: FnMut() -> Result<(), String>>(
    retained: &A,
    streamed: &B,
    retained_size: usize,
    streamed_size: usize,
    probe: &mut F,
) -> Result<bool, String> {
    // Reserve once before encoding; no growing Vec or second encoding can
    // coexist with this owner. Refuse an allocator-reported excess capacity.
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(retained_size)
        .map_err(|e| e.to_string())?;
    if bytes.capacity() > retained_size {
        return Err("JSON comparison buffer exceeds exact reservation".into());
    }
    let mut writer = ProbedWriter {
        output: BoundedWriter {
            size: 0,
            maximum: retained_size,
            retained: Some(bytes),
        },
        probe,
    };
    serde_json::to_writer(&mut writer, retained).map_err(|e| e.to_string())?;
    if writer.output.size != retained_size {
        return Err("JSON serialization changed after sizing".into());
    }
    let bytes = writer
        .output
        .retained
        .ok_or("missing JSON comparison encoding")?;
    let mut writer = ProbedWriter {
        output: CompareWriter {
            expected: &bytes,
            offset: 0,
            maximum: streamed_size,
            equal: retained_size == streamed_size,
            overrun: "JSON serialization changed after sizing",
        },
        probe: writer.probe,
    };
    serde_json::to_writer(&mut writer, streamed).map_err(|e| e.to_string())?;
    if writer.output.offset != streamed_size {
        return Err("JSON serialization changed after sizing".into());
    }
    Ok(writer.output.equal)
}

/// Compare a serializer directly with previously retained immutable bytes.
/// No encoding or Value tree is allocated. The caller owns the provenance of
/// `expected`; this function establishes byte equality, not source authority.
/// Complete traversal is retained until an explicit byte/cancellation refusal.
pub fn serialized_matches_checked<T: Serialize>(
    expected: &[u8],
    value: &T,
    maximum_bytes: usize,
    mut probe: impl FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    if expected.len() > maximum_bytes {
        return Err("JSON comparison reference exceeds byte admission".into());
    }
    probe()?;
    let mut writer = ProbedWriter {
        output: CompareWriter {
            expected,
            offset: 0,
            maximum: maximum_bytes,
            equal: true,
            overrun: "serialized JSON exceeds selected byte admission",
        },
        probe: &mut probe,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    let equal = writer.output.equal && writer.output.offset == expected.len();
    probe()?;
    Ok(equal)
}

/// Exact comparison with retained bytes, funding the audited traversal and
/// every compared byte before visiting/forwarding it. No reference encoding,
/// parsed Value or monetary checkpoint is constructed. Provenance remains the
/// caller's responsibility; a mismatch cannot grant resource/publication rights.
pub fn serialized_matches_counted<T: Serialize>(
    expected: &[u8],
    value: &T,
    maximum_bytes: usize,
    charge: impl FnMut(usize) -> Result<(), String>,
) -> Result<(bool, super::prepaid_json::WriteWork), String> {
    if expected.len() > maximum_bytes {
        return Err("JSON comparison reference exceeds byte admission".into());
    }
    let mut writer = CompareWriter {
        expected,
        offset: 0,
        maximum: maximum_bytes,
        equal: true,
        overrun: "serialized JSON exceeds selected byte admission",
    };
    let work = super::prepaid_json::write_with_output_charge(&mut writer, value, charge)?;
    Ok((writer.equal && writer.offset == expected.len(), work))
}
