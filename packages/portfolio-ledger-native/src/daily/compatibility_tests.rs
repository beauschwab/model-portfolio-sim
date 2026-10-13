//! Exact same-platform compatibility with the frozen pre-policy daily runner.
use crate::{deposit_revision as m, historical_daily};
use serde_json::Value;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
fn raw() -> m::Request {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/deposit_revised_income.json"
    ))
    .unwrap()
}
#[derive(Clone)]
struct Sink(Arc<Mutex<Vec<u8>>>);
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn optional_none_preserves_frozen_financial_output_with_explicit_liquidity_status() {
    let mut r = raw();
    for run in &mut r.source.runs {
        for d in &mut run.revised.drivers {
            d.reference_rate = 0.24;
        }
    }
    let historical = m::prepare(r.clone()).unwrap();
    let old = Arc::new(Mutex::new(vec![]));
    historical_daily::run_dated(historical.dated, Box::new(Sink(old.clone()))).unwrap();
    let expected = old.lock().unwrap();
    #[cfg(target_os = "windows")]
    assert_eq!(
        std::str::from_utf8(&expected).unwrap(),
        // Git may check this text fixture out with CRLF. Only normalize its
        // record separators; every serialized field remains exact.
        include_str!("../../tests/fixtures/deposit_revision_none.ndjson").replace("\r\n", "\n")
    );
    let new = m::prepare(r).unwrap();
    let b = Arc::new(Mutex::new(vec![]));
    crate::daily::run_dated(new.dated, Box::new(Sink(b.clone()))).unwrap();
    let actual = b.lock().unwrap();
    let expected_lines: Vec<_> = std::str::from_utf8(&expected).unwrap().lines().collect();
    let actual_lines: Vec<_> = std::str::from_utf8(&actual).unwrap().lines().collect();
    assert_eq!(expected_lines.len(), actual_lines.len());
    for (before, after) in expected_lines.into_iter().zip(actual_lines) {
        let mut historical: Value = serde_json::from_str(before).unwrap();
        match historical["table"].as_str() {
            Some("path") => {
                // Upstream liquidity applicability explicitly makes headroom
                // unavailable when no limit is configured. No financial amount
                // or journal field is removed from the comparison.
                for row in historical["rows"].as_array_mut().unwrap() {
                    row["lcr_headroom"] = Value::Null;
                    row["nsfr_headroom"] = Value::Null;
                    row["lcr_status"] = "unconfigured".into();
                    row["nsfr_status"] = "unconfigured".into();
                }
                assert_eq!(historical, serde_json::from_str::<Value>(after).unwrap());
            }
            Some("summary") => {
                for row in historical["rows"].as_array_mut().unwrap() {
                    row["lcr_applicability"] = "unconfigured".into();
                    row["nsfr_applicability"] = "unconfigured".into();
                    row["lcr_status"] = "unconfigured".into();
                    row["nsfr_status"] = "unconfigured".into();
                    row["htm_reclassification_day"] = Value::Null;
                }
                assert_eq!(historical, serde_json::from_str::<Value>(after).unwrap());
            }
            _ => assert_eq!(before, after),
        }
    }
}
