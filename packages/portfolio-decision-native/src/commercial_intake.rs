//! Original-byte admission for commercial strategy creation and retained actors.
//! Routing examines every original selector before Value can erase duplicates.
//! Historical noncommercial Value-first decoding retains its existing behavior.
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{value::RawValue, Value};
use std::fmt;

const MAX_RAW_BYTES: usize = 128 << 20;
const MAX_COMMERCIAL_BYTES: usize = 32 << 20;
struct Selection {
    commercial: bool,
}
impl<'de> Deserialize<'de> for Selection {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct Select;
        impl<'de> Visitor<'de> for Select {
            type Value = Selection;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a strategy request object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Selection, A::Error> {
                let mut commercial = false;
                while let Some(key) = map.next_key::<String>()? {
                    let value: Box<RawValue> = map.next_value()?;
                    match key.as_str() {
                        "op" => {
                            if let Ok(op) = serde_json::from_str::<String>(value.get()) {
                                commercial |= matches!(
                                    op.as_str(),
                                    "create_commercial"
                                        | "optimize_commercial"
                                        | "analyze_commercial_treasury"
                                        | "analyze_commercial_candidate_journal"
                                        | "analyze_commercial_holding_journal"
                                        | "analyze_commercial_fee_journal"
                                );
                            }
                        }
                        "schema" => {
                            commercial |=
                                serde_json::from_str::<String>(value.get()).is_ok_and(|s| {
                                    matches!(
                                        s.as_str(),
                                        "strategy-commercial-1"
                                            | "commercial-treasury-1"
                                            | "commercial-candidate-journal-1"
                                            | "commercial-holding-journal-1"
                                    )
                                });
                        }
                        "handle" => {
                            if let Ok(handle) = serde_json::from_str::<u64>(value.get()) {
                                commercial |=
                                    crate::commercial_handle(handle).map_err(de::Error::custom)?;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Selection { commercial })
            }
        }
        decoder.deserialize_map(Select)
    }
}
pub fn parse(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > MAX_RAW_BYTES {
        return Err("request exceeds 128 MiB".into());
    }
    // Preserve the legacy malformed/non-object response and Value number behavior.
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if value.is_object() {
        let selection: Selection = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if selection.commercial {
            return portfolio_model_core::strict_json::parse_checked(
                bytes,
                MAX_COMMERCIAL_BYTES,
                portfolio_compute_control::checkpoint,
            );
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_selectors_cannot_hide_commercial_duplicates() {
        for raw in [
            r#"{"op":"optimize_commercial","op":"linear_program"}"#,
            r#"{"op":"linear_program","op":"optimize_commercial"}"#,
            r#"{"schema":"strategy-commercial-1","schema":"legacy","op":"x"}"#,
            r#"{"op":"create_commercial","commercial":{"cost":1,"cost":2}}"#,
            r#"{"op":"create_commercial","commercial":{"co\u0073t":1,"cost":2}}"#,
            r#"{"op":"analyze_commercial_candidate_journal","op":"linear_program"}"#,
            r#"{"op":"analyze_commercial_treasury","op":"linear_program"}"#,
        ] {
            assert!(parse(raw.as_bytes())
                .unwrap_err()
                .contains("duplicate object key"));
        }
    }
    #[test]
    fn historical_value_first_and_numeric_identity_are_preserved() {
        assert_eq!(parse(br#"{"op":"legacy","x":1,"x":2}"#).unwrap()["x"], 2);
        let v =
            parse(br#"{"op":"optimize_commercial","path_key":18446744073709551615,"zero":-0.0}"#)
                .unwrap();
        assert_eq!(v["path_key"].as_u64(), Some(u64::MAX));
        assert!(v["zero"].as_f64().unwrap().is_sign_negative());
    }
    #[test]
    fn retained_commercial_handle_selects_original_byte_validation() {
        let handle = u64::MAX;
        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);
        let registry =
            crate::REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        registry.lock().unwrap().insert(
            handle,
            crate::ActorHandle {
                sender,
                commercial: true,
            },
        );
        let result = parse(format!(r#"{{"op":"eval","handle":{handle},"handle":0,"allocation":[{{"face":1,"face":2}}]}}"#).as_bytes());
        registry.lock().unwrap().remove(&handle);
        assert!(result.unwrap_err().contains("duplicate object key"));
    }
    #[test]
    fn ffi_refuses_selected_duplicates_before_financial_dispatch() {
        let raw = std::ffi::CString::new(
            r#"{"op":"optimize_commercial","commercial":{"amount":1,"amount":2}}"#,
        )
        .unwrap();
        unsafe {
            let pointer = crate::decision_request(raw.as_ptr());
            let response: Value =
                serde_json::from_slice(std::ffi::CStr::from_ptr(pointer).to_bytes()).unwrap();
            crate::decision_free(pointer);
            assert!(response["error"]
                .as_str()
                .unwrap()
                .contains("duplicate object key"));
        }
    }
}
