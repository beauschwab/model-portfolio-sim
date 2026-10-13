use portfolio_ledger_native::deposit_source_principal as owner;
#[cfg(feature = "historical-output-control")]
use portfolio_ledger_native::{
    consumer_intent_owner, daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
    deposit_revision, historical_daily, product_event_owner,
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let mut v = self.0.lock().unwrap();
        if v.len() + b.len() > 32 * 1024 * 1024 {
            return Err(io::Error::other("test cap"));
        }
        v.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/deposit-source-principal/canonical.json")).unwrap()
}
fn prepare(v: &Value) -> Result<owner::Prepared, String> {
    owner::prepare(owner::parse_request(&serde_json::to_vec(v).unwrap())?.0)
}
fn evaluate(v: &Value) -> Result<Vec<Value>, String> {
    let p = prepare(v)?;
    let b = Arc::new(Mutex::new(Vec::new()));
    owner::run_public(p, v.clone(), Box::new(Capture(b.clone())))?;
    let bytes = b.lock().unwrap();
    bytes
        .split(|x| *x == b'\n')
        .filter(|x| !x.is_empty())
        .map(|x| serde_json::from_slice(x).map_err(|e| e.to_string()))
        .collect()
}
fn rows<'a>(out: &'a [Value], table: &str) -> Vec<&'a Value> {
    out.iter()
        .filter(|v| v["table"] == table)
        .flat_map(|v| v["rows"].as_array().unwrap())
        .collect()
}
#[test]
fn actual_partial_runoff_source_atoms_close_with_signed_residues() {
    let out = evaluate(&fixture()).unwrap();
    assert_eq!(out.last().unwrap(), &json!({"complete":true,"protocol":3}));
    let states = rows(&out, "deposit_continuation_state");
    assert_eq!(states.len(), 62);
    let last = states
        .iter()
        .find(|v| v["scenario"] == "baseline" && v["day"] == 31)
        .unwrap();
    assert_eq!(last["principal"], 0.);
    assert!(last["book_adjustment"].as_f64().unwrap() != 0.);
    assert!(last["gross_rights"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v.as_f64().unwrap().abs() < 1e-9));
    let lineage = rows(&out, "deposit_principal_lineage");
    assert!(lineage
        .iter()
        .any(|v| v["transition"]["principal_partition_delta"]
            .as_f64()
            .unwrap()
            != 0.));
    assert!(lineage.iter().all(|v| v["cash"] == 0. && v["income"] == 0.));
    for row in rows(&out, "deposit_revision") {
        assert_eq!(row["day"], 10);
        assert_eq!(row["original_yield_retained"], true)
    }
}
#[test]
fn source_changed_cash_prefix_is_refused_before_any_result() {
    let mut v = fixture();
    v["runs"][1]["source"]["continuation"]["current"]["drivers"][4]["reference_rate"] = json!(0.8);
    assert!(prepare(&v).is_err());
}
#[test]
fn original_anchor_mismatch_refuses_all_runs() {
    let mut v = fixture();
    v["runs"][1]["source"]["continuation"]["anchor"]["original_gross_carrying_value"] = json!(103.);
    assert!(prepare(&v).unwrap_err_string().contains("anchor"));
}
trait ErrorString<T> {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorString<T> for Result<T, String> {
    fn unwrap_err_string(self) -> String {
        self.err().expect("expected rejection")
    }
}
#[test]
fn missing_or_duplicate_run_coverage_refuses() {
    let mut v = fixture();
    v["runs"].as_array_mut().unwrap().pop();
    assert!(prepare(&v).is_err());
    let mut v = fixture();
    v["runs"][1]["run_id"] = v["runs"][0]["run_id"].clone();
    assert!(prepare(&v).is_err());
}
#[test]
fn zero_weight_cannot_hide_bad_tail() {
    let mut v = fixture();
    v["runs"][1]["source_weight"] = json!(0.);
    assert!(prepare(&v).is_err());
}
#[test]
fn full_64day_component_cannot_bypass_joined_capacity() {
    let v: Value = serde_json::from_str(include_str!("fixtures/deposit-source-principal/exact_64_day_tail.json")).unwrap();
    assert!(prepare(&v).unwrap_err_string().contains("pre-generation"));
}
#[test]
fn original_quote_and_legacy_overlay_refuse() {
    let mut v = fixture();
    v["specification"]["positions"][0]["book_adjustment"] = json!(0.);
    assert!(prepare(&v).is_err());
    let mut v = fixture();
    v["specification"]["positions"][0]["monthly_runoff"] = json!(0.1);
    assert!(prepare(&v).is_err());
}
#[test]
fn global_flight_cannot_apply_again_to_dated_owner() {
    let v = fixture();
    let a = evaluate(&v).unwrap();
    let mut changed = v.clone();
    changed["specification"]["scenarios"][0]["deposit_flight"] = json!(0.9);
    let b = evaluate(&changed).unwrap();
    assert_eq!(
        rows(&a, "deposit_continuation_state"),
        rows(&b, "deposit_continuation_state")
    );
}
#[test]
fn literal_duplicate_and_enum_alias_refuse() {
    let raw = serde_json::to_string(&fixture()).unwrap();
    let duplicate=raw.replacen("\"schema\":\"deposit-performing-income-ledger-1\"","\"schema\":\"deposit-performing-income-ledger-1\",\"schema\":\"deposit-performing-income-ledger-1\"",1);
    assert!(owner::parse_request(duplicate.as_bytes()).is_err());
    let mut v = fixture();
    v["runs"][0]["source"]["continuation"]["original"]["identity"]["measure"] =
        json!({"physical":null});
    assert!(prepare(&v).is_err());
}
#[test]
fn failed_writer_never_emits_completion() {
    struct Refuse;
    impl Write for Refuse {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("sink failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let v = fixture();
    assert!(owner::run_public(prepare(&v).unwrap(), v, Box::new(Refuse))
        .unwrap_err()
        .contains("sink failed"));
}
#[test]
fn cancelled_source_admission_has_no_partial_output() {
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let b = Arc::new(Mutex::new(Vec::new()));
    let result = portfolio_compute_control::scope(Some(control), || {
        let v = fixture();
        owner::run_public(prepare(&v)?, v, Box::new(Capture(b.clone())))
    });
    assert!(result.is_err());
    assert!(b.lock().unwrap().is_empty());
}
#[test]
fn raw_source_change_after_preparation_refuses_before_header() {
    let v = fixture();
    let p = prepare(&v).unwrap();
    let mut changed = v;
    changed["runs"][0]["source"]["review"]["revision"] = json!("changed");
    let b = Arc::new(Mutex::new(Vec::new()));
    assert!(owner::run_public(p, changed, Box::new(Capture(b.clone()))).is_err());
    assert!(b.lock().unwrap().is_empty());
}
#[test]
fn public_envelope_preserves_raw_types_and_refuses_duplicates_aliases() {
    let v = fixture();
    let envelope = json!({"request":v,"execution_timeout_ms":30000});
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let (_, t, raw) = owner::parse_envelope(&bytes).unwrap();
    assert_eq!(t, 30000);
    assert_eq!(raw, v);
    for bad in [json!(true), json!(30000.), json!(0), json!(1800001)] {
        let mut e = envelope.clone();
        e["execution_timeout_ms"] = bad;
        assert!(owner::parse_envelope(&serde_json::to_vec(&e).unwrap()).is_err());
    }
    let text = String::from_utf8(bytes).unwrap();
    let duplicate = text.replacen(
        "\"execution_timeout_ms\":30000",
        "\"execution_timeout_ms\":30000,\"execution_timeout_ms\":30000",
        1,
    );
    assert!(owner::parse_envelope(duplicate.as_bytes()).is_err());
    let mut e = envelope;
    e["request"]["runs"][0]["source"]["continuation"]["original"]["identity"]["measure"] =
        json!({"physical":null});
    assert!(owner::parse_envelope(&serde_json::to_vec(&e).unwrap()).is_err());
}
#[test]
fn selected_owner_checks_cancellation_after_completion_flush() {
    struct CancelOnComplete {
        control: portfolio_compute_control::Control,
        complete: bool,
    }
    impl Write for CancelOnComplete {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.complete |= b.windows(15).any(|w| w == b"\"complete\":true");
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            if self.complete {
                self.control.cancel()
            }
            Ok(())
        }
    }
    let control = portfolio_compute_control::Control::new(30000).unwrap();
    let writer = CancelOnComplete {
        control: control.clone(),
        complete: false,
    };
    let mut inner_refused = false;
    let result = portfolio_compute_control::scope(Some(control), || {
        let v = fixture();
        let result = owner::run_public(prepare(&v)?, v, Box::new(writer));
        inner_refused = result.is_err();
        result
    });
    assert!(result.is_err());
    assert!(
        inner_refused,
        "run_public must fail before outer scope final probe"
    );
}
#[cfg(feature = "historical-output-control")]
fn product_stream(raw: &Value, historical: bool) -> Vec<u8> {
    let dated = CompiledDatedLedger::compile(
        serde_json::from_value::<DatedLedgerRequest>(raw["dated"].clone()).unwrap(),
        512,
        8_000_000,
    )
    .unwrap();
    let owner=product_event_owner::Compiled::compile(serde_json::from_value(json!({"schema":product_event_owner::SCHEMA,"reporting_end_ordinal":raw["reporting_end_ordinal"],"owners":raw["product_owners"]})).unwrap(),&dated).unwrap();
    let b = Arc::new(Mutex::new(Vec::new()));
    if historical {
        historical_daily::run_dated_with_products(dated, owner, Box::new(Capture(b.clone())))
            .unwrap()
    } else {
        daily::run_dated_with_products(dated, owner, Box::new(Capture(b.clone()))).unwrap()
    };
    let result = b.lock().unwrap().clone();
    result
}
#[cfg(feature = "historical-output-control")]
#[test]
fn historical_card1_optional_none_full_ndjson_bytes() {
    let raw: Value =
        serde_json::from_str(include_str!("fixtures/deposit-source-principal/product_event_owner.json")).unwrap();
    assert_eq!(product_stream(&raw, true), product_stream(&raw, false));
}
#[cfg(feature = "historical-output-control")]
#[test]
fn historical_card2_optional_none_full_ndjson_bytes() {
    let raw: Value =
        serde_json::from_str(include_str!("fixtures/deposit-source-principal/product_event_owner_v2.json")).unwrap();
    assert_eq!(product_stream(&raw, true), product_stream(&raw, false));
}
#[cfg(feature = "historical-output-control")]
#[test]
fn historical_consumer_optional_none_full_ndjson_bytes() {
    let mut raw: Value =
        serde_json::from_str(include_str!("fixtures/deposit-source-principal/consumer_intent_owner.json")).unwrap();
    raw["dated"]["specification"]["positions"].as_array_mut().unwrap().push(json!({"id":"credit-loan","account":"a","kind":"loan","classification":"ac","balance":0.,"rate":0.,"maturity_day":0,"risk_weight":0.,"rsf_weight":0.,"asf_weight":0.,"hqla_weight":0.,"lcr_inflow_weight":0.,"lcr_outflow_weight":0.}));
    let mut owners = raw["consumer_owners"].clone();
    for o in owners.as_array_mut().unwrap() {
        let n = o["runs"][0]["paths"][0]["request"].clone();
        o["accounting_category"] = json!("ac");
        o["source_identity"] = n["economic_source"].clone();
        o["original_yield_anchor"] = n["acquisition"]["original_yield_anchor"].clone();
    }
    let build = |historical| {
        let dated = CompiledDatedLedger::compile(
            serde_json::from_value::<DatedLedgerRequest>(raw["dated"].clone()).unwrap(),
            512,
            8_000_000,
        )
        .unwrap();
        let o=consumer_intent_owner::Compiled::compile(serde_json::from_value(json!({"schema":consumer_intent_owner::SCHEMA,"reporting_end_ordinal":raw["reporting_end_ordinal"],"owners":owners})).unwrap(),&dated).unwrap();
        let b = Arc::new(Mutex::new(Vec::new()));
        if historical {
            historical_daily::run_dated_with_consumers(dated, o, Box::new(Capture(b.clone())))
                .unwrap()
        } else {
            daily::run_dated_with_consumers(dated, o, Box::new(Capture(b.clone()))).unwrap()
        };
        let r = b.lock().unwrap().clone();
        r
    };
    assert_eq!(build(true), build(false));
}
#[cfg(feature = "historical-output-control")]
#[test]
fn historical_purchase_asof_deposit_optional_none_full_ndjson_bytes() {
    let raw: deposit_revision::Request =
        serde_json::from_str(include_str!("fixtures/deposit-source-principal/deposit_revised_income.json")).unwrap();
    let build = |historical| {
        let p = deposit_revision::prepare(raw.clone()).unwrap();
        let b = Arc::new(Mutex::new(Vec::new()));
        if historical {
            historical_daily::run_dated_with_deposit_revision(
                &p.dated,
                &p.revisions,
                Box::new(Capture(b.clone())),
            )
            .unwrap()
        } else {
            daily::run_dated_with_deposit_revision(
                &p.dated,
                &p.revisions,
                Box::new(Capture(b.clone())),
            )
            .unwrap()
        };
        let r = b.lock().unwrap().clone();
        r
    };
    assert_eq!(build(true), build(false));
}
