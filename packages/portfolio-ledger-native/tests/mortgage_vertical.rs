use portfolio_ledger_native::{dated_ledger::CompiledDatedLedger, mortgage_pending_owner as owner};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/mortgage_mixed_pending.json")).unwrap()
}
fn audit(v: Value) -> Result<owner::Admission, String> {
    let r: owner::Request = serde_json::from_value(v).unwrap();
    let d = CompiledDatedLedger::compile(r.dated.clone(), 512, owner::MAX_WORK)?;
    owner::audit(&r, &d)
}
#[derive(Clone)]
struct Bytes(Arc<Mutex<Vec<u8>>>);
impl Write for Bytes {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn schema_money_policy_is_required() {
    let mut r = raw();
    r["policy"]["money_domain"] = json!("implicit");
    assert!(audit(r).unwrap_err().contains("policy"));
}
#[test]
fn opposite_signed_money_cannot_net_at_preflight() {
    let mut r = raw();
    r["dated"]["specification"]["accounts"][0]["cash"] = json!(6000.);
    r["dated"]["specification"]["accounts"][0]["equity"] = json!(6050.);
    assert!(audit(r).unwrap_err().contains("10k gross"));
}
#[test]
fn large_backbook_principal_rejects_before_projection() {
    let mut r = raw();
    r["dated"]["specification"]["positions"][0]["balance"] = json!(1e10);
    r["dated"]["specification"]["accounts"][0]["equity"] = json!(1e10 + 1000.);
    let e = audit(r).unwrap_err();
    assert!(e.contains("10k monetary"), "{e}");
}
#[test]
fn large_basis_and_signed_opening_accrual_reject_before_projection() {
    for field in [
        "book_adjustment",
        "opening_accrued",
        "allowance",
        "commitment",
        "pledged_face",
    ] {
        let mut r = raw();
        r["dated"]["specification"]["positions"][0][field] = json!(-10001.);
        assert!(audit(r).is_err(), "{field}");
    }
}
#[test]
fn recognition_monetary_component_domain_is_separate_from_dates() {
    let mut r = raw();
    r["dated"]["dated"]["accrual_plans"][0]["segments"][0]["book_amortization"] = json!(10001.);
    assert!(audit(r).is_err());
}
#[test]
fn full_header_is_borrowed_source_then_same_ordered_journal() {
    let r: owner::Request = serde_json::from_value(raw()).unwrap();
    let p = owner::prepare(r).unwrap();
    let bytes = Bytes(Arc::new(Mutex::new(Vec::new())));
    owner::run(&p, Box::new(bytes.clone())).unwrap();
    let bytes = bytes.0.lock().unwrap();
    let lines = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice::<Value>(b).unwrap())
        .collect::<Vec<_>>();
    let h = &lines[0]["workflow"];
    assert_eq!(h["model_schema"], "mortgage-mixed-accounting-1");
    assert_eq!(h["source_receipt"]["sources"].as_array().unwrap().len(), 1);
    assert_eq!(h["source_receipt"]["production_adoption"], false);
    assert_eq!(
        h["source_receipt"]["raw_request"]["policy"]["money_domain"],
        "combined_gross_state_10k-1"
    );
    assert!(lines.iter().any(|x| x["table"] == "journal"));
    assert!(bytes.len() < 32 * 1024 * 1024);
}
#[test]
fn failed_writer_cannot_complete_stream() {
    struct Fail;
    impl Write for Fail {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("publication write failure"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let p = owner::prepare(serde_json::from_value(raw()).unwrap()).unwrap();
    assert!(owner::run(&p, Box::new(Fail))
        .unwrap_err()
        .contains("write failure"));
}
