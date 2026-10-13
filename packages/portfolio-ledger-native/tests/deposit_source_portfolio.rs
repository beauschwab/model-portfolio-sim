use portfolio_ledger_native::deposit_source_portfolio as owner;
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn raw(two: bool) -> Value {
    serde_json::from_str(if two {
        include_str!("fixtures/deposit-portfolio-v2/portfolio-40-2.json")
    } else {
        include_str!("fixtures/deposit-portfolio-v2/portfolio-90-1.json")
    })
    .unwrap()
}
fn prepare(v: &Value) -> Result<owner::Prepared, String> {
    owner::prepare(owner::parse(&serde_json::to_vec(v).unwrap())?.0)
}
fn run(v: &Value) -> Result<Vec<Value>, String> {
    let p = prepare(v)?;
    let b = Arc::new(Mutex::new(Vec::new()));
    owner::run_public(v, &p, Box::new(Capture(b.clone())))?;
    let bytes = b.lock().unwrap();
    bytes
        .split(|c| *c == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).map_err(|e| e.to_string()))
        .collect()
}
fn rows<'a>(v: &'a [Value], name: &str) -> Vec<&'a Value> {
    v.iter()
        .filter(|f| f["table"] == name)
        .flat_map(|f| f["rows"].as_array().unwrap())
        .collect()
}
#[test]
fn longer_actual_same_sim_and_signed_residues() {
    let v = run(&raw(false)).unwrap();
    assert_eq!(v.last().unwrap(), &json!({"complete":true,"protocol":3}));
    let states = rows(&v, "deposit_continuation_state");
    assert_eq!(states.len(), 180);
    let final_states = states.iter().filter(|s| s["day"] == 90);
    for s in final_states {
        assert_eq!(s["principal"], 0.);
        assert!(s["gross_rights"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_f64().unwrap().abs() < 1e-9));
    }
    assert!(states
        .iter()
        .any(|s| s["book_adjustment"].as_f64().unwrap() < 0.));
}
#[test]
fn two_distinct_owners_pool_one_account_without_double_acquisition() {
    let v = run(&raw(true)).unwrap();
    assert_eq!(rows(&v, "deposit_revision").len(), 4);
    let states = rows(&v, "deposit_continuation_state");
    assert_eq!(states.len(), 160);
    for id in ["deposit-1", "deposit-2"] {
        for scene in ["baseline", "zero"] {
            assert_eq!(
                states
                    .iter()
                    .filter(|s| s["position_id"] == id && s["scenario"] == scene)
                    .count(),
                40
            );
        }
    }
    assert!(rows(&v, "deposit_principal_lineage")
        .iter()
        .all(|s| s["cash"] == 0. && s["income"] == 0.));
}
#[test]
fn observed_actual_capacity_and_work_below_reserved() {
    let v = raw(true);
    let p = prepare(&v).unwrap();
    assert!(p.usage().source_retained_used < p.admission().source_retained);
    assert!(p.usage().source_charged_work < p.admission().source_work);
    assert!(p.usage().dated_store_capacity < p.admission().ledger_retained);
    assert_eq!(p.admission().work_units, 7_572_752);
}
#[test]
fn core128_cannot_bypass_joined_preflight() {
    let v: Value = serde_json::from_str(include_str!(
        "fixtures/deposit-portfolio-v2/portfolio-128-1.json"
    ))
    .unwrap();
    assert!(prepare(&v).err().unwrap().contains("pre-generation"));
}
#[test]
fn missing_duplicate_or_overlapping_owner_refuse() {
    let mut v = raw(true);
    v["positions"][1]["position_id"] = json!("deposit-1");
    assert!(prepare(&v).is_err());
    let mut v = raw(true);
    v["positions"][1]["paths"].as_array_mut().unwrap().pop();
    assert!(prepare(&v).is_err());
    let mut v = raw(true);
    v["positions"][0]["paths"][1]["run_id"] = json!("base-path");
    assert!(prepare(&v).is_err());
}
#[test]
fn unweighted_unused_named_tail_and_original_anchors_mandatory() {
    let mut v = raw(true);
    v["positions"][1]["paths"][1]["source"]["continuation"]["current"]["segments"][0]
        ["maturity_ordinal"] = json!(999999);
    assert!(prepare(&v).is_err());
    let mut v = raw(true);
    v["positions"][1]["paths"][1]["source"]["continuation"]["anchor"]
        ["original_gross_carrying_value"] = json!(101.);
    assert!(prepare(&v).is_err());
}
#[test]
fn raw_mutation_refuses_before_header() {
    let v = raw(true);
    let p = prepare(&v).unwrap();
    let mut changed = v;
    changed["positions"][0]["paths"][0]["source"]["review"]["revision"] = json!("changed");
    let b = Arc::new(Mutex::new(Vec::new()));
    assert!(owner::run_public(&changed, &p, Box::new(Capture(b.clone()))).is_err());
    assert!(b.lock().unwrap().is_empty());
}
#[test]
fn source_generation_cancellation_before_header() {
    let c = portfolio_compute_control::Control::new(60_000).unwrap();
    c.cancel();
    let b = Arc::new(Mutex::new(Vec::new()));
    assert!(portfolio_compute_control::scope(Some(c), || {
        let v = raw(true);
        owner::run_public(&v, &prepare(&v)?, Box::new(Capture(b.clone())))
    })
    .is_err());
    assert!(b.lock().unwrap().is_empty());
}
#[test]
fn final_writer_cancellation_refuses_success() {
    struct Stop(portfolio_compute_control::Control);
    impl Write for Stop {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.cancel();
            Ok(())
        }
    }
    let v = raw(true);
    let p = prepare(&v).unwrap();
    let c = portfolio_compute_control::Control::new(60000).unwrap();
    assert!(
        portfolio_compute_control::scope(Some(c.clone()), || owner::run_public(
            &v,
            &p,
            Box::new(Stop(c))
        ))
        .unwrap_err()
        .contains("CANCELLED")
    );
}
#[test]
fn overlays_unknownweights_and_literal_duplicates_rejected() {
    let mut v = raw(true);
    v["positions"][0]["source_weight"] = json!(0.);
    assert!(prepare(&v).is_err());
    let mut v = raw(true);
    v["specification"]["positions"][1]["annual_pd"] = json!(0.1);
    assert!(prepare(&v).is_err());
    let text = serde_json::to_string(&raw(true)).unwrap();
    let d=text.replacen("\"schema\":\"deposit-performing-portfolio-ledger-2\"","\"schema\":\"deposit-performing-portfolio-ledger-2\",\"schema\":\"deposit-performing-portfolio-ledger-2\"",1);
    assert!(owner::parse(d.as_bytes()).is_err());
}
#[test]
fn exact_gross_opposing_positions_cannot_net_around_bound() {
    let mut v = raw(true);
    v["specification"]["accounts"][0]["cash"] = json!(10_000.);
    v["specification"]["accounts"][0]["equity"] = json!(10_000. - 204.);
    assert!(prepare(&v).is_err());
}
#[test]
fn typed_outer_capacity_is_charged() {
    let v = raw(true);
    let (mut r, _) = owner::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
    r.positions.reserve(10000);
    assert!(owner::audit(&r).unwrap_err().contains("capacity"));
}
