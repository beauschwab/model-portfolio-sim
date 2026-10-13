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
    run: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Option<Result<Vec<u8>, String>> {
    if input.len() > crate::term_protocol::MAX_INPUT {
        return None;
    }
    let probe = serde_json::from_slice::<Probe>(input).ok()?;
    if !(probe.selected || probe.graph && probe.timeout.is_some()) {
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
                (if probe.selected { 1 << 20 } else { 32 << 20 }) + 4096,
                checkpoint,
            )?;
            checkpoint()?;
            let result = run()?;
            checkpoint()?;
            Ok(result)
        })
    })())
}

pub(crate) struct Output(pub Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        checkpoint().map_err(std::io::Error::other)?;
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > (32 << 20) + 4096)
        {
            return Err(std::io::Error::other("generated response exceeds32MiB"));
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
        assert!(selected(raw, Instant::now(), || panic!(
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
            selected(raw, past, || panic!("expired entry reached model"))
                .unwrap()
                .unwrap_err()
                .contains("DEADLINE")
        );
        let parent = Control::from_started(1, past).unwrap();
        assert!(scope(Some(parent), || {
            let mut out = Output(vec![]);
            out.write_all(b"never").map_err(|e| e.to_string())
        })
        .is_err());
    }
}
