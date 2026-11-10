//! Count original JSON before allocating its strict Value tree.
//! This bounds the pinned strict_json::Unique parser's requested heap capacity.
//! Typed deserialization, financial owners and process RSS are separate.
//! Layout/growth assumptions are pinned to Rust 1.93.1 and serde_json 1.0.151.
//! This utility does not grant whole-request resource or publication authority.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{fmt, io, mem::size_of};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub values: usize,
    pub object_members: usize,
    pub array_elements: usize,
    pub string_bytes: usize,
    pub tree_nodes_upper: usize,
    pub vector_slots_upper: usize,
}

fn add<E: de::Error>(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b)
        .ok_or_else(|| E::custom("JSON capacity count overflow"))
}

impl Shape {
    fn merge<E: de::Error>(&mut self, b: Self) -> Result<(), E> {
        self.values = add(self.values, b.values)?;
        self.object_members = add(self.object_members, b.object_members)?;
        self.array_elements = add(self.array_elements, b.array_elements)?;
        self.string_bytes = add(self.string_bytes, b.string_bytes)?;
        self.tree_nodes_upper = add(self.tree_nodes_upper, b.tree_nodes_upper)?;
        self.vector_slots_upper = add(self.vector_slots_upper, b.vector_slots_upper)?;
        Ok(())
    }
}

struct Count;
struct CountKey;

impl<'de> DeserializeSeed<'de> for CountKey {
    type Value = usize;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<usize, D::Error> {
        struct Key;
        impl Visitor<'_> for Key {
            type Value = usize;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON object key")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<usize, E> {
                Ok(s.len())
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<usize, E> {
                Ok(s.len())
            }
        }
        d.deserialize_str(Key)
    }
}

impl<'de> DeserializeSeed<'de> for Count {
    type Value = Shape;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Shape, D::Error> {
        struct Value;
        impl<'de> Visitor<'de> for Value {
            type Value = Shape;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("bounded JSON value")
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Shape, E> {
                Ok(leaf())
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Shape, E> {
                Ok(leaf())
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Shape, E> {
                Ok(leaf())
            }
            fn visit_f64<E: de::Error>(self, n: f64) -> Result<Shape, E> {
                if !n.is_finite() {
                    return Err(E::custom("nonfinite JSON value"));
                }
                Ok(leaf())
            }
            fn visit_unit<E: de::Error>(self) -> Result<Shape, E> {
                Ok(leaf())
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Shape, E> {
                Ok(Shape {
                    string_bytes: s.len(),
                    ..leaf()
                })
            }
            fn visit_string<E: de::Error>(self, s: String) -> Result<Shape, E> {
                self.visit_str(&s)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Shape, A::Error> {
                let mut result = leaf();
                let mut n = 0usize;
                while let Some(child) = a.next_element_seed(Count)? {
                    n = add(n, 1)?;
                    result.merge(child)?;
                }
                result.array_elements = add(result.array_elements, n)?;
                // Unique uses Vec::new followed by individual push. Rust's
                // pinned minimum is four Value slots, then capacity doubles.
                // Include both allocations during its last growth, even though
                // old and new allocations need not coexist in the allocator.
                let cap = if n == 0 {
                    0
                } else {
                    n.max(4)
                        .checked_next_power_of_two()
                        .ok_or_else(|| de::Error::custom("JSON vector capacity overflow"))?
                };
                result.vector_slots_upper = add(result.vector_slots_upper, add(cap, cap / 2)?)?;
                Ok(result)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Shape, A::Error> {
                let mut result = leaf();
                let mut n = 0usize;
                while let Some(key_bytes) = a.next_key_seed(CountKey)? {
                    n = add(n, 1)?;
                    result.string_bytes = add(result.string_bytes, key_bytes)?;
                    result.merge(a.next_value_seed(Count)?)?;
                }
                result.object_members = add(result.object_members, n)?;
                // A non-root node has >=5 keys in the pinned B=6 tree. Counting
                // each node at internal-node size also covers leaf/internal
                // differences. Duplicate keys overcount, then strict parsing
                // rejects them; this scanner never grants duplicate acceptance.
                let nodes = if n == 0 { 0 } else { 1 + (n - 1) / 5 };
                result.tree_nodes_upper = add(result.tree_nodes_upper, nodes)?;
                Ok(result)
            }
        }
        d.deserialize_any(Value)
    }
}

fn leaf() -> Shape {
    Shape {
        values: 1,
        ..Shape::default()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Capacity {
    pub shape: Shape,
    pub value_tree_bytes_upper: usize,
    pub parser_workspace_bytes_upper: usize,
}

/// No complete Value, key set, typed DTO, financial projection or clone is
/// constructed here. The parser's one scratch string and read buffer are bounded
/// separately. Its default recursion limit remains enabled.
pub fn estimate(
    bytes: &[u8],
    maximum: usize,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Capacity, String> {
    check()?;
    if bytes.len() > maximum {
        return Err("original JSON exceeds capacity input limit".into());
    }
    if size_of::<usize>() != 8 || size_of::<String>() != 24 || size_of::<serde_json::Value>() != 32
    {
        return Err("JSON capacity census requires the pinned 64-bit Value layout".into());
    }
    struct Checked<'a, F> {
        bytes: &'a [u8],
        check: F,
    }
    impl<F: FnMut() -> Result<(), String>> io::Read for Checked<'_, F> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            (self.check)().map_err(io::Error::other)?;
            let n = output.len().min(32768).min(self.bytes.len());
            output[..n].copy_from_slice(&self.bytes[..n]);
            self.bytes = &self.bytes[n..];
            Ok(n)
        }
    }
    let input = io::BufReader::with_capacity(
        32768,
        Checked {
            bytes,
            check: &mut check,
        },
    );
    let mut parser = serde_json::Deserializer::from_reader(input);
    let shape = Count.deserialize(&mut parser).map_err(|e| e.to_string())?;
    parser.end().map_err(|e| e.to_string())?;
    check()?;
    let checked =
        |x: Option<usize>| x.ok_or_else(|| "JSON capacity arithmetic overflow".to_string());
    // A String copy requests its complete UTF-8 length. Map nodes own the
    // String/Value headers. Parent/edge/header padding is bounded by 128 bytes.
    let node = 11 * (size_of::<String>() + size_of::<serde_json::Value>()) + 128;
    let trees = checked(shape.tree_nodes_upper.checked_mul(node))?;
    let vectors = checked(
        shape
            .vector_slots_upper
            .checked_mul(size_of::<serde_json::Value>()),
    )?;
    let heap = checked(checked(trees.checked_add(vectors))?.checked_add(shape.string_bytes))?;
    // The scratch Vec can reach at most twice the original input plus its
    // minimum capacity. Budget old/new overlap during growth and the IO buffer.
    let workspace = checked(checked(bytes.len().checked_mul(3))?.checked_add(32768 + 8))?;
    Ok(Capacity {
        shape,
        value_tree_bytes_upper: heap,
        parser_workspace_bytes_upper: workspace,
    })
}

/// Refuse the requested strict-parser payload before constructing its tree.
/// The input slice's owner and later clones/typed DTOs are caller reservations.
/// Duplicate-key, numeric-category and recursion semantics remain strict_json's.
pub fn parse_checked(
    bytes: &[u8],
    maximum_input_bytes: usize,
    maximum_parser_bytes: usize,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<(serde_json::Value, Capacity), String> {
    let capacity = estimate(bytes, maximum_input_bytes, &mut check)?;
    let requested = capacity
        .value_tree_bytes_upper
        .checked_add(capacity.parser_workspace_bytes_upper)
        .ok_or("JSON parser capacity overflow")?;
    if requested > maximum_parser_bytes {
        return Err(format!("strict JSON parser capacity exceeds selected bound: requested={requested},limit={maximum_parser_bytes}"));
    }
    let value = super::strict_json::parse_checked(bytes, maximum_input_bytes, &mut check)?;
    Ok((value, capacity))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn census(s: &str) -> Capacity {
        estimate(s.as_bytes(), 2 << 20, || Ok(())).unwrap()
    }
    #[test]
    fn scalar_and_empty_containers_have_no_retained_heap() {
        for s in ["0", "-0.0", "true", "null", "[]", "{}", "\"\""] {
            let c = census(s);
            assert_eq!(c.shape.values, 1);
            assert_eq!(c.value_tree_bytes_upper, 0);
        }
    }
    #[test]
    fn string_escapes_count_decoded_utf8_and_all_keys() {
        let c = census(r#"{"a\u00e9":["💰", "\uD83D\uDE00"]}"#);
        assert_eq!(c.shape.string_bytes, 3 + 4 + 4);
        assert_eq!(c.shape.object_members, 1);
        assert_eq!(c.shape.array_elements, 2);
        assert_eq!(c.shape.vector_slots_upper, 6);
        assert_eq!(c.shape.tree_nodes_upper, 1);
        assert_eq!(c.value_tree_bytes_upper, 744 + 6 * 32 + 11);
    }
    #[test]
    fn nested_arrays_retain_each_growth_allocation() {
        let c = census("[[0,1,2,3,4],[false]]");
        assert_eq!(c.shape.values, 9);
        assert_eq!(c.shape.array_elements, 8);
        assert_eq!(c.shape.vector_slots_upper, 6 + 12 + 6);
    }
    #[test]
    fn malformed_trailing_and_excessively_deep_inputs_reject() {
        for s in ["{", "[0,]", "0 1", "1e999", "{\"x\":}"] {
            assert!(estimate(s.as_bytes(), 1000, || Ok(())).is_err(), "{s}");
        }
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        assert!(estimate(deep.as_bytes(), 1000, || Ok(())).is_err());
    }
    #[test]
    fn byte_limit_and_cancellation_remain_external_authority() {
        assert!(estimate(b"[0]", 2, || Ok(())).is_err());
        let raw = format!("\"{}\"", "x".repeat(100000));
        let mut calls = 0;
        let result = estimate(raw.as_bytes(), raw.len(), || {
            calls += 1;
            if calls == 4 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        });
        assert!(result.unwrap_err().contains("cancelled"));
        assert_eq!(calls, 4);
    }
    #[test]
    fn duplicate_members_overcount_instead_of_replacing_a_value() {
        let c = census(r#"{"x":0,"x":[1,2]}"#);
        assert_eq!(c.shape.object_members, 2);
        assert_eq!(c.shape.values, 5);
        assert_eq!(c.shape.string_bytes, 2);
    }
    #[test]
    fn original_complete_request_has_independently_counted_raw_dimensions() {
        let raw = include_str!("../../../../examples/models/commercial-cut-mixed-journal-1.json");
        let c = census(raw);
        // Independent Python traversal of the original unmodified JSON, before
        // typed normalization or financial projection. All27 source uses remain.
        assert_eq!(c.shape.object_members, 4506);
        assert_eq!(c.shape.array_elements, 1381);
        assert_eq!(c.shape.string_bytes, 45958 + 30797);
        assert_eq!(c.shape.tree_nodes_upper, 1464);
        assert_eq!(c.shape.values, 1108 + 2270 + 539 + 644 + 1327);
        println!("{c:?}");
    }

    #[test]
    fn parser_bound_precedes_tree_construction_and_keeps_strict_semantics() {
        let raw = br#"{"x":1,"x":2}"#;
        let c = estimate(raw, 1000, || Ok(())).unwrap();
        let bound = c.value_tree_bytes_upper + c.parser_workspace_bytes_upper;
        assert!(parse_checked(raw, 1000, bound - 1, || Ok(()))
            .unwrap_err()
            .contains("capacity exceeds"));
        assert!(parse_checked(raw, 1000, bound, || Ok(()))
            .unwrap_err()
            .contains("duplicate object key"));
        let raw = br#"{"path":18446744073709551615,"zero":-0.0,"f":1.0}"#;
        let (value, _) = parse_checked(raw, 1000, 1 << 20, || Ok(())).unwrap();
        assert_eq!(value["path"].as_u64(), Some(u64::MAX));
        assert!(value["zero"].as_f64().unwrap().is_sign_negative());
        assert!(value["f"].is_f64());
    }
}
