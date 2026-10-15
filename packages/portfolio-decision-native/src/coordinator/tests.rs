use super::{actors::dispatch, session::Session};
use crate::{decision_free, decision_request};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
fn input() -> Value {
    let mut v = vec![vec![0.; 2]; 10];
    v[0] = vec![0.1, 0.1];
    v[1] = vec![1., 1.];
    v[8] = vec![1., 1.];
    json!({"units":[{"template":"loan","h":0,"side":1.}],"scenarios":[{"base":{"nii":10.,"dv01":0.,"eve":1000.,"l1":1000.,"l2":0.,"nco":1.,"asf":1000.,"rsf":1.,"cet1":100.,"rwa":1.,"ni":0.2365,"mv_assets":1000.},"vectors":[v]}],"constraints":{"max_total_assets":100.,"cash_budget":100.},"records":[{"key":"loans:a","metrics":[[10.,0.,1.,0.,10.]]}]})
}
#[test]
fn transaction_solver_and_rollback() {
    let mut s = Session::new(&input()).unwrap();
    let plan = s
        .call(json!({"op":"plan","version":0,"edits":{},"templates":{}}))
        .unwrap();
    let r = s
        .call(json!({"op":"stage","version":0,"token":plan["token"],"records":[],"columns":{}}))
        .unwrap();
    assert!((r["worst_case_nii_$"].as_f64().unwrap() - 30.).abs() < 1e-6);
    assert_eq!(s.version, 0);
    s.call(json!({"op":"publish","version":0,"token":plan["token"]}))
        .unwrap();
    assert_eq!(s.version, 1);
    assert!(s
        .call(json!({"op":"plan","version":0,"edits":{},"templates":{}}))
        .is_err());
    let p=s.call(json!({"op":"plan","version":1,"edits":{"loans:a":{"coupon":0.2}},"templates":{},"constraints":{"max_total_assets":50.,"cash_budget":100.}})).unwrap();
    let r=s.call(json!({"op":"stage","version":1,"token":p["token"],"records":[{"key":"loans:a","metrics":[[11.,0.,2.,0.,11.]]}],"columns":{}})).unwrap();
    assert!(r["solver"]["model_reused"].as_bool().unwrap());
    assert!((r["worst_case_nii_$"].as_f64().unwrap() - 21.).abs() < 1e-6);
    s.call(json!({"op":"abort","version":1,"token":p["token"]}))
        .unwrap();
    assert_eq!(s.scenarios[0].base.nii, 10.);
}
#[test]
fn capital_limits_require_refresh_after_edits() {
    let mut v = input();
    v["constraints"]["capital_limits"] = json!([{"label":"slr","policy_id":"v1","metric":"slr",
            "scenario":0,"month":0,"units":v["units"],"numerator":12.,"denominator":100.,
            "required_ratio":0.1,"numerator_per_unit":[0.],"denominator_per_unit":[1.]}]);
    let mut s = Session::new(&v).unwrap();
    let request =
        json!({"op":"plan","version":0,"edits":{"loans:a":{"coupon":0.2}},"templates":{}});
    assert!(s
        .call(request.clone())
        .unwrap_err()
        .contains("CAPITAL_REFRESH_REQUIRED"));
    assert!(s.pending.is_none());
    let mut unchanged = request.clone();
    unchanged["constraints"] = v["constraints"].clone();
    assert!(s
        .call(unchanged)
        .unwrap_err()
        .contains("CAPITAL_REFRESH_REQUIRED"));
    let mut refreshed = request;
    refreshed["constraints"] = v["constraints"].clone();
    refreshed["constraints"]["capital_limits"][0]["numerator"] = json!(13.);
    assert!(s.call(refreshed).is_ok());
}
#[test]
fn rejects_shapes_and_duplicate_records() {
    let mut v = input();
    v["records"][0]["metrics"] = json!([]);
    assert!(Session::new(&v).is_err());
    let mut v = input();
    let r = v["records"][0].clone();
    v["records"].as_array_mut().unwrap().push(r);
    assert!(Session::new(&v).is_err());
}

#[test]
fn malformed_ffi_and_overflow_fail_closed() {
    let bad = CString::new("not json").unwrap();
    unsafe {
        let pointer = decision_request(bad.as_ptr());
        let v: Value = serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()).unwrap();
        assert!(v.get("error").is_some());
        decision_free(pointer);
        let pointer = decision_request(std::ptr::null());
        assert!(CStr::from_ptr(pointer)
            .to_str()
            .unwrap()
            .contains("null input"));
        decision_free(pointer);
    }
    let mut s = Session::new(&input()).unwrap();
    assert!(s.call(json!({"op":"eval","version":0,"allocation":[{"template":"loan","purchase_m":0,"notional":1e308}]})).is_err());
}

#[test]
fn actor_lifecycle_and_simultaneous_sessions() {
    let mut v = input();
    v["op"] = json!("create");
    let a = dispatch(v.clone()).unwrap()["handle"].as_u64().unwrap();
    let b = dispatch(v).unwrap()["handle"].as_u64().unwrap();
    let threads:Vec<_>=[a,b].into_iter().map(|handle|std::thread::spawn(move||{
            let p=dispatch(json!({"op":"plan","handle":handle,"version":0,"edits":{},"templates":{}})).unwrap();
            let r=dispatch(json!({"op":"stage","handle":handle,"version":0,"token":p["token"],"records":[],"columns":{}})).unwrap();
            assert!(r["feasible"].as_bool().unwrap());
            dispatch(json!({"op":"publish","handle":handle,"version":0,"token":p["token"]})).unwrap();
            dispatch(json!({"op":"close","handle":handle})).unwrap();
            assert!(dispatch(json!({"op":"status","handle":handle})).is_err());
        })).collect();
    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn failed_actor_initialization_releases_capacity() {
    for _ in 0..16 {
        let error = dispatch(json!({"op":"create_owned","input":{}})).unwrap_err();
        assert!(!error.contains("capacity"), "{error}");
    }
    let mut v = input();
    v["op"] = json!("create");
    let handle = dispatch(v).unwrap()["handle"].as_u64().unwrap();
    dispatch(json!({"op":"close","handle":handle})).unwrap();
}
