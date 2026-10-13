//! Original-byte selected public stream. No second financial preparation owner.
use portfolio_compute_control::{checkpoint, scope, Control};
use portfolio_model_core::strict_json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;

pub const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    request: Value,
    execution_timeout_ms: u64,
}

pub fn run(bytes: &[u8], output: &mut dyn Write) -> Result<(), String> {
    run_started(bytes, output, std::time::Instant::now())
}
/// Uses the selected dispatcher's original clock, including stdin and routing decode.
pub fn run_started(
    bytes: &[u8],
    output: &mut dyn Write,
    parse_started: std::time::Instant,
) -> Result<(), String> {
    checkpoint()?;
    let parent = portfolio_compute_control::active();
    let original = strict_json::parse_checked(bytes, MAX_BYTES, checkpoint)?;
    let envelope: Envelope = serde_json::from_value(original).map_err(|e| e.to_string())?;
    if envelope.request.get("schema").and_then(Value::as_str) != Some("saved-derivative-mixed-1") {
        return Err("explicit saved derivative mixed schema required".into());
    }
    let control = match parent {
        Some(p) => p.child_started(envelope.execution_timeout_ms, parse_started)?,
        None => Control::from_started(envelope.execution_timeout_ms, parse_started)?,
    };
    scope(Some(control), || {
        checkpoint()?;
        let request = strict_json::encode(&envelope.request, MAX_BYTES)?;
        let encoded = super::execute(&request)?;
        let mut receipt: Value = serde_json::from_slice(&encoded).map_err(|e| e.to_string())?;
        let stream = receipt
            .as_object_mut()
            .ok_or("mixed result object required")?
            .remove("native_stream")
            .ok_or("mixed actual stream required")?;
        let Value::String(stream) = stream else {
            return Err("mixed stream string required".into());
        };
        let header = strict_json::encode(
            &json!({"workflow": {
                "schema": "model-ledger-preparation-1", "model_schema": "saved-derivative-mixed-1",
                "source_receipt": receipt, "prepared_request": envelope.request
            }}),
            MAX_BYTES,
        )?;
        if header
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_add(stream.len()))
            .is_none_or(|n| n > MAX_BYTES)
        {
            return Err("complete public mixed stream exceeds32MiB".into());
        }
        // Finish raw financial/source/shared-Sim admission before emitting a header.
        checkpoint()?;
        output
            .write_all(&header)
            .and_then(|_| output.write_all(b"\n"))
            .map_err(|e| e.to_string())?;
        for chunk in stream.as_bytes().chunks(32 * 1024) {
            checkpoint()?;
            output.write_all(chunk).map_err(|e| e.to_string())?;
            checkpoint()?;
        }
        output.flush().map_err(|e| e.to_string())?;
        checkpoint()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Value {
        serde_json::from_str(include_str!("../../tests/fixtures/derivative-mixed.json")).unwrap()
    }
    #[test]
    fn public_original_source_and_actual_completion() {
        let raw = request();
        let bytes = strict_json::encode(
            &json!({"request":raw,"execution_timeout_ms":30000}),
            MAX_BYTES,
        )
        .unwrap();
        let mut output = vec![];
        run(&bytes, &mut output).unwrap();
        let stream = std::str::from_utf8(&output).unwrap();
        let mut lines = stream.lines();
        let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(
            header["workflow"]["source_receipt"]["original_request"],
            raw
        );
        assert_eq!(header["workflow"]["prepared_request"], raw);
        assert_eq!(lines.last().unwrap(), "{\"complete\":true,\"protocol\":3}");
    }
    #[test]
    fn nested_duplicate_rejects_before_header() {
        let bytes = br#"{"request":{"schema":"saved-derivative-mixed-1","owners":[{"x":1,"x":2}]},"execution_timeout_ms":30000}"#;
        let mut output = vec![];
        assert!(run(bytes, &mut output)
            .unwrap_err()
            .contains("duplicate object key"));
        assert!(output.is_empty());
    }
    #[test]
    fn invalid_timeout_and_owner_coverage_emit_nothing() {
        for timeout in [json!(false), json!(0), json!(30000.5)] {
            let bytes = strict_json::encode(
                &json!({"request":request(),"execution_timeout_ms":timeout}),
                MAX_BYTES,
            )
            .unwrap();
            let mut output = vec![];
            assert!(run(&bytes, &mut output).is_err());
            assert!(output.is_empty());
        }
        let mut raw = request();
        raw["owners"][0]["runs"].as_array_mut().unwrap().pop();
        let bytes = strict_json::encode(
            &json!({"request":raw,"execution_timeout_ms":30000}),
            MAX_BYTES,
        )
        .unwrap();
        let mut output = vec![];
        assert!(run(&bytes, &mut output).is_err());
        assert!(output.is_empty());
    }
    #[test]
    fn failed_public_write_and_flush_are_errors() {
        struct Fail {
            flush: bool,
        }
        impl Write for Fail {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.flush {
                    Ok(bytes.len())
                } else {
                    Err(std::io::Error::other("test pipe closed"))
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("test flush failed"))
            }
        }
        let bytes = strict_json::encode(
            &json!({"request":request(),"execution_timeout_ms":30000}),
            MAX_BYTES,
        )
        .unwrap();
        assert!(run(&bytes, &mut Fail { flush: false }).is_err());
        assert!(run(&bytes, &mut Fail { flush: true }).is_err());
    }
    #[test]
    fn inherited_cancelled_or_expired_control_never_parses_or_emits() {
        let cancelled = Control::new(30000).unwrap();
        cancelled.cancel();
        let expired = Control::from_started(
            1,
            std::time::Instant::now() - std::time::Duration::from_secs(2),
        )
        .unwrap();
        for parent in [cancelled, expired] {
            let mut output = Vec::new();
            let e = scope(Some(parent), || run(b"not JSON", &mut output)).unwrap_err();
            assert!(e.starts_with("COMPUTE_"));
            assert!(output.is_empty());
        }
    }
    #[test]
    fn dispatcher_entry_budget_includes_parse_and_is_never_renewed() {
        let bytes =
            serde_json::to_vec(&json!({"request":request(),"execution_timeout_ms":1})).unwrap();
        let mut output = Vec::new();
        let old = std::time::Instant::now() - std::time::Duration::from_secs(2);
        assert!(run_started(&bytes, &mut output, old)
            .unwrap_err()
            .contains("COMPUTE_DEADLINE_EXCEEDED"));
        assert!(output.is_empty());
    }
    #[test]
    fn inherited_flush_cancellation_and_none_stream_identity() {
        struct CancelFlush {
            control: Control,
        }
        impl Write for CancelFlush {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.control.cancel();
                Ok(())
            }
        }
        let raw = request();
        let bytes =
            serde_json::to_vec(&json!({"request":raw,"execution_timeout_ms":30000})).unwrap();
        let mut none = Vec::new();
        run(&bytes, &mut none).unwrap();
        let parent = Control::new(30000).unwrap();
        let mut inherited = Vec::new();
        scope(Some(parent), || run(&bytes, &mut inherited)).unwrap();
        assert_eq!(none, inherited);
        let direct: Value = serde_json::from_slice(
            &super::super::execute(&serde_json::to_vec(&raw).unwrap()).unwrap(),
        )
        .unwrap();
        let body = none.splitn(2, |c| *c == b'\n').nth(1).unwrap();
        assert_eq!(body, direct["native_stream"].as_str().unwrap().as_bytes());
        let parent = Control::new(30000).unwrap();
        let mut sink = CancelFlush {
            control: parent.clone(),
        };
        assert_eq!(
            scope(Some(parent), || run(&bytes, &mut sink)).unwrap_err(),
            "COMPUTE_CANCELLED"
        );
    }
}
