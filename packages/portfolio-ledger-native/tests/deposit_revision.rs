use portfolio_ledger_native::deposit_revision::{self as m, Request};
use serde_json::Value;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
fn raw() -> Request {
    serde_json::from_str(include_str!("fixtures/deposit_revised_income.json")).unwrap()
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
fn wire(r: Request) -> Vec<Value> {
    let p = m::prepare(r).unwrap();
    let b = Arc::new(Mutex::new(vec![]));
    m::run(p, Box::new(Sink(b.clone()))).unwrap();
    let bytes = b.lock().unwrap();
    let mut dictionary = Vec::<Value>::new();
    std::str::from_utf8(&bytes).unwrap().lines().map(|l|{let mut v:Value=serde_json::from_str(l).unwrap();if v["table"]=="journal" {dictionary.extend(v["dictionary"].as_array().unwrap().iter().cloned());let c=&v["columns"];let decoded:Vec<_>=(0..c["value"].as_array().unwrap().len()).map(|i|serde_json::json!({"scenario":dictionary[c["scenario"][i].as_u64().unwrap()as usize],"day":c["day"][i],"event":dictionary[c["event"][i].as_u64().unwrap()as usize],"gl":dictionary[c["gl_account"][i].as_u64().unwrap()as usize],"amount":c["value"][i]})).collect();v=serde_json::json!({"table":"journal","rows":decoded});}v}).collect()
}
fn rows<'a>(wire: &'a [Value], table: &str) -> Vec<&'a Value> {
    wire.iter()
        .filter(|v| v["table"] == table)
        .flat_map(|v| v["rows"].as_array().unwrap())
        .collect()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}")
}
#[test]
fn one_native_revision_after_original_opening_before_daily_interest() {
    let r = raw();
    let p = m::prepare(r.clone()).unwrap();
    near(p.revisions[0].amount, 1. / 1.01);
    assert_eq!(
        p.dated.spec.positions[0].book_adjustment,
        r.source.anchor.original_gross_carrying_value - 100.
    );
    let w = wire(r);
    assert_eq!(
        w.last().unwrap(),
        &serde_json::json!({"complete":true,"protocol":3})
    );
    let journal = rows(&w, "journal");
    let baseline: Vec<_> = journal
        .iter()
        .filter(|x| x["scenario"] == "baseline")
        .collect();
    assert!(baseline
        .iter()
        .any(|x| x["event"] == "opening" && x["gl"] == "book_adjustment"));
    let rev: Vec<_> = baseline
        .iter()
        .filter(|x| x["event"] == "deposit_basis_revision")
        .collect();
    assert_eq!(rev.len(), 2);
    assert!(rev.iter().all(|x| x["day"] == 0));
    let pnl = rev
        .iter()
        .find(|x| x["gl"] == "pnl:deposit_basis_revision")
        .unwrap();
    near(pnl["amount"].as_f64().unwrap(), 1. / 1.01);
    assert_eq!(rows(&w, "deposit_revision").len(), 2);
}
#[test]
fn original_cash_and_original_cost_reconcile_full_tail_once() {
    let r = raw();
    let cost = r.source.anchor.original_gross_carrying_value;
    let w = wire(r);
    let closes = rows(&w, "summary");
    let close = closes.iter().find(|x| x["scenario"] == "baseline").unwrap();
    near(close["final_cash"].as_f64().unwrap(), 1000. + cost - 103.);
    near(close["final_equity"].as_f64().unwrap(), 1000. + cost - 103.);
    assert!(rows(&w, "path")
        .iter()
        .all(|x| x["reconciliation_error"].as_f64().unwrap().abs() < 1e-9));
}
#[test]
fn missing_named_run_and_duplicate_run_reject_before_generation() {
    let mut r = raw();
    r.source.runs.pop();
    assert!(m::prepare(r).err().unwrap().contains("cover exactly"));
    let mut r = raw();
    r.source.runs.push(r.source.runs[0].clone());
    assert!(m::prepare(r).err().unwrap().contains("duplicate"));
}
#[test]
fn original_spec_anchors_not_overwritten() {
    let mut r = raw();
    r.specification["positions"][0]["book_adjustment"] = serde_json::json!(7.);
    assert!(m::prepare(r).err().unwrap().contains("opening"));
}
#[test]
fn generic_stress_does_not_own_native_expected_cash() {
    let r = raw();
    let a = wire(r.clone());
    let mut b = r;
    b.specification["scenarios"][0]["deposit_flight"] = serde_json::json!(0.9);
    let b = wire(b);
    let financial = |w: &[Value]| rows(w, "journal").into_iter().cloned().collect::<Vec<_>>();
    assert_eq!(financial(&a), financial(&b));
}
#[test]
fn unsupported_legacy_runoff_fees_and_reverse_reject() {
    let mut r = raw();
    r.specification["positions"][0]["monthly_runoff"] = serde_json::json!(0.1);
    assert!(m::prepare(r).is_err());
    let mut r = raw();
    r.specification["accounts"][0]["annual_fees"] = serde_json::json!(1.);
    assert!(m::prepare(r).is_err());
    let mut r = raw();
    r.specification["reverse_severities"] = serde_json::json!([0.5]);
    assert!(m::prepare(r).is_err());
}
#[test]
fn request_and_income_source_original_identities_retained() {
    let r = raw();
    let before = serde_json::to_vec(&r).unwrap();
    let p = m::prepare(r).unwrap();
    assert_eq!(before, serde_json::to_vec(&p.raw_request).unwrap());
    assert_eq!(
        p.native.paths[0].income.original_yield_anchor.id,
        "original-eir"
    );
    assert!(p.admission.work_units <= 8_000_000 && p.admission.reserved_bytes <= 32 * 1024 * 1024);
}
#[test]
fn zero_revision_has_no_extra_money_transaction() {
    let mut r = raw();
    for run in &mut r.source.runs {
        for d in &mut run.revised.drivers {
            d.reference_rate = 0.24;
        }
    }
    let w = wire(r);
    assert!(rows(&w, "journal")
        .iter()
        .all(|x| x["event"] != "deposit_basis_revision"));
    assert!(rows(&w, "deposit_revision")
        .iter()
        .all(|x| x["basis_revision"].as_f64().unwrap() == 0.));
}

#[test]
fn literal_duplicate_and_unit_enum_alias_refused() {
    let bytes = include_str!("fixtures/deposit_revised_income.json");
    let duplicate=bytes.replacen("\"schema\": \"deposit-revised-income-ledger-1\",", "\"schema\": \"deposit-revised-income-ledger-1\",\"schema\": \"deposit-revised-income-ledger-1\",",1);
    assert!(m::parse_request(duplicate.as_bytes())
        .unwrap_err()
        .contains("duplicate"));
    let alias = bytes.replace(
        "\"measure\": \"physical\"",
        "\"measure\": {\"physical\":null}",
    );
    assert!(m::parse_request(alias.as_bytes())
        .unwrap_err()
        .contains("string enum"));
    let mut value: Value = serde_json::from_str(bytes).unwrap();
    value["source"]["original"]["as_of_ordinal"] = serde_json::json!(738916.0);
    assert!(m::parse_request(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn staged_bad_revision_never_emits_completion_or_failed_money_lines() {
    let mut p = m::prepare(raw()).unwrap();
    p.revisions[0].amount = 20_000.;
    let b = Arc::new(Mutex::new(vec![]));
    let err = m::run(p, Box::new(Sink(b.clone()))).unwrap_err();
    assert!(err.contains("runtime money"));
    let bytes = b.lock().unwrap();
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("\"complete\":true"));
    assert!(!std::str::from_utf8(&bytes).unwrap().contains("20000"));
}
#[test]
fn failed_output_write_cannot_complete_native_publication() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("persist failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(m::run(m::prepare(raw()).unwrap(), Box::new(Broken))
        .unwrap_err()
        .contains("persist failed"));
}

#[test]
fn public_original_raw_identity_and_four_key_header() {
    let mut original = serde_json::to_value(raw()).unwrap();
    original["source"]["anchor"]["original_principal"] = serde_json::json!(100);
    let input = serde_json::to_vec(
        &serde_json::json!({"request": original, "execution_timeout_ms": 30000}),
    )
    .unwrap();
    let (typed, timeout, retained) = m::parse_envelope(&input).unwrap();
    assert_eq!(timeout, 30000);
    assert!(retained["source"]["anchor"]["original_principal"].is_u64());
    let output = Arc::new(Mutex::new(vec![]));
    m::run_public(
        m::prepare(typed).unwrap(),
        retained.clone(),
        Box::new(Sink(output.clone())),
    )
    .unwrap();
    let bytes = output.lock().unwrap();
    let first = bytes.split(|x| *x == b'\n').next().unwrap();
    let header: serde_json::Value = serde_json::from_slice(first).unwrap();
    let workflow = &header["workflow"];
    assert_eq!(workflow.as_object().unwrap().len(), 4);
    assert_eq!(workflow["schema"], "model-ledger-preparation-1");
    assert_eq!(workflow["model_schema"], m::SCHEMA);
    assert_eq!(workflow["source_receipt"]["raw_request"], retained);
    assert!(
        workflow["source_receipt"]["raw_request"]["source"]["anchor"]["original_principal"]
            .is_u64()
    );
    assert!(bytes.ends_with(b"{\"complete\":true,\"protocol\":3}\n"));
}

#[test]
fn selected_original_byte_envelope_rejects_duplicates_and_timeout_alias() {
    let r = serde_json::to_string(&raw()).unwrap();
    for input in [
        format!(
            "{{\"request\":{r},\"execution_timeout_ms\":30000,\"execution_timeout_ms\":30000}}"
        ),
        format!("{{\"request\":{r},\"execution_timeout_ms\":30000.0}}"),
        format!("{{\"request\":{r},\"execution_timeout_ms\":true}}"),
    ] {
        assert!(m::parse_envelope(input.as_bytes()).is_err());
    }
}

#[test]
fn public_changed_retained_source_emits_no_header() {
    let r = raw();
    let prepared = m::prepare(r.clone()).unwrap();
    let mut raw = serde_json::to_value(&r).unwrap();
    raw["source"]["anchor"]["original_annual_effective_rate"] = serde_json::json!(0.);
    let output = Arc::new(Mutex::new(vec![]));
    let err = m::run_public(prepared, raw, Box::new(Sink(output.clone()))).unwrap_err();
    assert!(err.contains("raw request differs"));
    assert!(output.lock().unwrap().is_empty());
}

#[test]
fn complete_coupon_has_gross_payable_then_every_origin_retires() {
    let p = m::prepare(raw()).unwrap();
    let run = &p.dated.store.runs()[0];
    let at = p.dated.store.as_of_ordinal();
    let rights: Vec<_> = p
        .dated
        .store
        .events()
        .iter()
        .filter(|e| e.run_index == Some(0))
        .map(|e| {
            p.dated
                .store
                .rights_at(
                    &run.run_id,
                    e.index,
                    at + 29,
                    portfolio_ledger_native::dated_store::Boundary::AfterEvents,
                )
                .unwrap()
                .signed_interest_right
        })
        .collect();
    near(rights.iter().sum(), -2.9);
    assert!(rights.iter().all(|x| *x <= 1e-9));
    for e in p
        .dated
        .store
        .events()
        .iter()
        .filter(|e| e.run_index == Some(0))
    {
        near(
            p.dated
                .store
                .rights_at(
                    &run.run_id,
                    e.index,
                    at + 31,
                    portfolio_ledger_native::dated_store::Boundary::AfterEvents,
                )
                .unwrap()
                .signed_interest_right,
            0.,
        );
    }
    let w = wire(raw());
    for row in rows(&w, "dated_rights") {
        near(row["signed_interest_right"].as_f64().unwrap(), 0.);
    }
}

#[test]
fn partial_native_runoff_interest_keeps_earned_origins_and_closes_gross() {
    let mut r = raw();
    r.source
        .policy
        .changed_driver_dimensions
        .push("confidence_stress".into());
    r.source.original.segments[0]
        .retention
        .closure_stress_sensitivity = 1.;
    for run in &mut r.source.runs {
        run.revised.segments[0].retention.closure_stress_sensitivity = 1.;
        run.revised.drivers[2].confidence_stress[0] = 365. * 1.25_f64.ln();
    }
    let p = m::prepare(r.clone()).unwrap();
    let source = &p.native.paths[0].source;
    let day1: f64 = source
        .cashflows
        .iter()
        .filter(|c| c.ordinal == r.source.original.as_of_ordinal + 1)
        .map(|c| c.principal + c.cash_interest)
        .sum();
    near(day1, -20.02);
    let w = wire(r);
    for row in rows(&w, "dated_rights") {
        near(row["signed_interest_right"].as_f64().unwrap(), 0.);
    }
    assert!(rows(&w, "path")
        .iter()
        .all(|x| x["reconciliation_error"].as_f64().unwrap().abs() < 1e-9));
}

#[test]
fn zero_rate_span_keeps_explicit_zero_recognition_calendar_partition() {
    let mut r = raw();
    let asof = r.source.original.as_of_ordinal;
    for run in &mut r.source.runs {
        for driver in &mut run.revised.drivers {
            if (asof + 5..asof + 10).contains(&driver.ordinal) {
                driver.reference_rate = 0.;
            }
        }
    }
    let p = m::prepare(r.clone()).unwrap();
    near(p.native.paths[0].revised_carrying_value, 102.5 / 1.01);
    assert!(p
        .prepared_request
        .dated
        .accrual_plans
        .iter()
        .flat_map(|x| &x.segments)
        .any(|s| s.start_ordinal == asof + 5
            && s.end_ordinal == asof + 10
            && s.accrued_interest == 0.
            && s.book_amortization == 0.));
    let w = wire(r);
    for row in rows(&w, "dated_rights") {
        near(row["signed_interest_right"].as_f64().unwrap(), 0.);
    }
}
#[test]
fn multiple_full_coupon_dates_allocate_all_current_lots_without_future_borrowing() {
    let mut r = raw();
    let asof = r.source.original.as_of_ordinal;
    let cost = (100. * 0.24 * 10. / 360.) / 1.01_f64.powf(1. / 3.)
        + (100. + 100. * 0.24 * 20. / 360.) / 1.01;
    r.source.anchor.original_gross_carrying_value = cost;
    r.source.anchor.original_price_per_100 = cost;
    r.specification["positions"][0]["book_adjustment"] = serde_json::json!(cost - 100.);
    r.specification["accounts"][0]["cash"] = serde_json::json!(1000. + cost);
    r.source.original.segments[0].interest_payment_ordinals = vec![asof + 10, asof + 30];
    for run in &mut r.source.runs {
        run.revised.segments[0].interest_payment_ordinals = vec![asof + 10, asof + 30];
    }
    let p = m::prepare(r.clone()).unwrap();
    let first = p
        .cash_origins
        .iter()
        .find(|o| o.run_id == "baseline" && o.native_kind == "interest_payment")
        .unwrap();
    assert!(first.full_native_accrued_settlement);
    assert_eq!(first.allocations.len(), 10);
    assert_eq!(first.allocations.last().unwrap().native_interest_index, 9);
    near(first.allocations.iter().map(|a| a.amount).sum(), 1.);
    let w = wire(r);
    for row in rows(&w, "dated_rights") {
        near(row["signed_interest_right"].as_f64().unwrap(), 0.);
    }
}
