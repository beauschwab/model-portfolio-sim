//! Original-byte, original-clock generated model selection and serialization.
use portfolio_compute_control::{checkpoint, scope, Control};
use portfolio_model_core::strict_json;
use serde::de::{MapAccess, Visitor};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{fmt, io::Write, time::Instant};

#[derive(Default)]
struct Probe {
    selected: bool,
    graph: bool,
    lifetime_loss: bool,
    consumer_behavior: bool,
    consumer_products: bool,
    consumer_macro: bool,
    consumer_macro_products: bool,
    timeout: Option<u64>,
}
impl<'de> Deserialize<'de> for Probe {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Probe;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("model envelope")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Probe, A::Error> {
                let mut out = Probe::default();
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<Box<RawValue>>()?;
                    if key == "schema" {
                        out.consumer_macro |= serde_json::from_str::<String>(value.get())
                            .is_ok_and(|v| v == portfolio_model_core::consumer_macro::SCHEMA);
                        out.consumer_macro_products |= serde_json::from_str::<String>(value.get())
                            .is_ok_and(|v| {
                                v == portfolio_model_core::consumer_macro_products::SCHEMA
                            });
                        out.consumer_products |= serde_json::from_str::<String>(value.get())
                            .is_ok_and(|v| {
                                v == portfolio_model_core::consumer_product_paths::SCHEMA
                            });
                        out.consumer_behavior |= serde_json::from_str::<String>(value.get())
                            .is_ok_and(|v| v == crate::models::consumer_behavior::SCHEMA);
                        out.lifetime_loss |= serde_json::from_str::<String>(value.get())
                            .is_ok_and(|v| v == crate::models::lifetime_loss::SCHEMA);
                        out.selected |=
                            serde_json::from_str::<String>(value.get()).is_ok_and(|v| {
                                v == crate::products::commercial::stochastic_exercise::SCHEMA
                                    || v == crate::products::commercial::continuation::SCHEMA
                            });
                        out.graph |= serde_json::from_str::<String>(value.get()).is_ok_and(|v| {
                            matches!(
                                v.as_str(),
                                "commercial-product-graph-1" | "commercial-units-1"
                            )
                        });
                    }
                    if key == "execution_timeout_ms" {
                        out.timeout = serde_json::from_str::<u64>(value.get()).ok();
                    }
                }
                Ok(out)
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

pub(crate) fn selected(
    input: &[u8],
    started: Instant,
    run: impl FnOnce(usize) -> Result<Vec<u8>, String>,
) -> Option<Result<Vec<u8>, String>> {
    if input.len() > crate::transport::protocol::MAX_INPUT {
        return None;
    }
    let probe = serde_json::from_slice::<Probe>(input).ok()?;
    if !(probe.selected
        || probe.lifetime_loss
        || probe.consumer_behavior
        || probe.consumer_macro
        || probe.consumer_macro_products
        || probe.consumer_products
        || probe.graph && probe.timeout.is_some())
    {
        return None;
    }
    Some((|| {
        let timeout = probe
            .timeout
            .ok_or("explicit generated execution_timeout_ms required")?;
        let control = match portfolio_compute_control::active() {
            Some(parent) => parent.child_started(timeout, started)?,
            None => Control::from_started(timeout, started)?,
        };
        scope(Some(control), || {
            strict_json::parse_checked(
                input,
                (if probe.consumer_macro_products {
                    8 << 20
                } else if probe.consumer_macro {
                    4 << 20
                } else if probe.consumer_products {
                    1 << 20
                } else if probe.consumer_behavior {
                    8 << 20
                } else if probe.lifetime_loss {
                    2 << 20
                } else if probe.selected {
                    1 << 20
                } else {
                    32 << 20
                }) + 4096,
                checkpoint,
            )?;
            checkpoint()?;
            let result = run(if probe.consumer_macro_products {
                96 << 20
            } else {
                32 << 20
            })?;
            checkpoint()?;
            Ok(result)
        })
    })())
}

pub(crate) struct Output(pub Vec<u8>, usize);
impl Output {
    pub(crate) fn new(selected_limit: usize) -> Self {
        // Reuse the original selected probe; no second raw-source traversal.
        Self(Vec::new(), selected_limit)
    }
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        checkpoint().map_err(std::io::Error::other)?;
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.1 + 4096)
        {
            return Err(std::io::Error::other(if self.1 == 32 << 20 {
                "generated response exceeds32MiB"
            } else {
                "consumer macro product response exceeds96MiB"
            }));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        checkpoint().map_err(std::io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_selected_duplicate_cannot_hide_under_legacy_schema() {
        let raw=br#"{"schema":"commercial-generated-exercise-1","schema":"kpi-1","execution_timeout_ms":30000}"#;
        assert!(selected(raw, Instant::now(), |_| panic!(
            "duplicate reached financial model"
        ))
        .unwrap()
        .unwrap_err()
        .contains("duplicate"));
    }
    #[test]
    fn original_entry_clock_is_not_restarted_by_decode_or_serialization() {
        let raw = br#"{"schema":"commercial-generated-exercise-1","execution_timeout_ms":1}"#;
        let past = Instant::now() - std::time::Duration::from_millis(5);
        assert!(
            selected(raw, past, |_| panic!("expired entry reached model"))
                .unwrap()
                .unwrap_err()
                .contains("DEADLINE")
        );
        let parent = Control::from_started(1, past).unwrap();
        assert!(scope(Some(parent), || {
            let mut out = Output(vec![], 32 << 20);
            out.write_all(b"never").map_err(|e| e.to_string())
        })
        .is_err());
    }
    #[test]
    fn original_probe_selects_only_the_explicit_new_output_limit() {
        for (schema, expected) in [
            ("commercial-generated-exercise-1", 32 << 20),
            ("consumer-macro-scenarios-1", 32 << 20),
            ("consumer-macro-products-1", 96 << 20),
        ] {
            let raw = format!("{{\"schema\":\"{schema}\",\"execution_timeout_ms\":30000}}");
            selected(raw.as_bytes(), Instant::now(), |limit| {
                assert_eq!(limit, expected);
                Ok(Vec::new())
            })
            .unwrap()
            .unwrap();
        }
        let mut out = Output::new(1);
        assert!(out.write_all(&vec![0; 4098]).is_err());
        assert!(out.0.is_empty());
    }
}
