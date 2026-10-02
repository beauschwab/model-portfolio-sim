//! Versioned actor sessions: native solver pointers stay on their owner thread.
mod library;
mod owned;
mod solver;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solver::{Constraints, Scenario, Solver, Unit};
use std::{
    collections::HashMap,
    ffi::{c_char, CStr, CString},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
};
#[derive(Clone, Deserialize, Serialize)]
struct Record {
    key: String,
    metrics: Vec<Vec<f64>>,
}
struct Pending {
    token: u64,
    patches: HashMap<String, Value>,
    templates: HashMap<String, Value>,
    records: Vec<Record>,
    scenarios: Option<Vec<Scenario>>,
    constraints: Constraints,
    result: Option<Value>,
}
struct Session {
    version: u64,
    nonce: u64,
    records: HashMap<String, Record>,
    patches: HashMap<String, Value>,
    templates: HashMap<String, Value>,
    units: Vec<Unit>,
    scenarios: Vec<Scenario>,
    constraints: Constraints,
    pending: Option<Pending>,
    solver: Solver,
}
fn decode<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(
        v.get(key)
            .cloned()
            .ok_or_else(|| format!("missing {key}"))?,
    )
    .map_err(|e| e.to_string())
}
fn merge(old: Option<&Value>, patch: &Value) -> Result<Value, String> {
    let mut v = old.cloned().unwrap_or(json!({}));
    let obj = patch.as_object().ok_or("patch must be an object")?;
    for (k, x) in obj {
        if x.is_null() {
            v.as_object_mut().unwrap().remove(k);
        } else if x.is_number() {
            v[k] = x.clone();
        } else {
            return Err("patch values must be numeric or null".into());
        }
    }
    Ok(v)
}
fn check_record(r: &Record, n: usize) -> Result<(), String> {
    if r.key.is_empty()
        || r.metrics.len() != n
        || r.metrics
            .iter()
            .any(|m| m.len() != 5 || m.iter().any(|v| !v.is_finite()))
    {
        Err("invalid position contribution".into())
    } else {
        Ok(())
    }
}
impl Session {
    fn new(v: &Value) -> Result<Self, String> {
        let units: Vec<Unit> = decode(v, "units")?;
        let scenarios: Vec<Scenario> = decode(v, "scenarios")?;
        let constraints: Constraints = decode(v, "constraints")?;
        solver::validate(&units, &scenarios, &constraints)?;
        let records: Vec<Record> = decode(v, "records")?;
        if records.len() > 200_000 {
            return Err("position limit exceeded".into());
        }
        let mut map = HashMap::new();
        for r in records {
            check_record(&r, scenarios.len())?;
            if map.insert(r.key.clone(), r).is_some() {
                return Err("duplicate position key".into());
            }
        }
        Ok(Self {
            version: 0,
            nonce: 0,
            records: map,
            patches: HashMap::new(),
            templates: HashMap::new(),
            units,
            scenarios,
            constraints,
            pending: None,
            solver: Solver::new(),
        })
    }
    fn call(&mut self, v: Value) -> Result<Value, String> {
        let op = v["op"].as_str().ok_or("missing operation")?;
        if op == "status" {
            return Ok(
                json!({"version":self.version,"positions":self.records.len(),"pending":self.pending.is_some()}),
            );
        }
        if v["version"].as_u64() != Some(self.version) {
            return Err("stale session version".into());
        }
        match op {
            "plan" => {
                if self.pending.is_some() {
                    return Err("another transaction is pending".into());
                }
                let edits: HashMap<String, Value> = decode(&v, "edits")?;
                let changes: HashMap<String, Value> = decode(&v, "templates")?;
                if edits.len() > 10_000 {
                    return Err("edit batch limit exceeded".into());
                }
                let constraints = if let Some(c) = v.get("constraints") {
                    serde_json::from_value(c.clone()).map_err(|e| e.to_string())?
                } else {
                    self.constraints.clone()
                };
                solver::validate(&self.units, &self.scenarios, &constraints)?;
                let mut patches = HashMap::new();
                let mut templates = HashMap::new();
                let mut dirty = Vec::new();
                for (key, patch) in edits {
                    let r = self.records.get(&key).ok_or("unknown position key")?;
                    let p = merge(self.patches.get(&key), &patch)?;
                    if self.patches.get(&key).unwrap_or(&json!({})) != &p {
                        dirty.push(json!({"key":key,"patch":p,"old_metrics":r.metrics}));
                        patches.insert(key, p);
                    }
                }
                for (key, patch) in changes {
                    if !self.units.iter().any(|u| u.template == key) {
                        return Err("unknown template".into());
                    }
                    let p = merge(self.templates.get(&key), &patch)?;
                    if self.templates.get(&key).unwrap_or(&json!({})) != &p {
                        templates.insert(key, p);
                    }
                }
                if (!dirty.is_empty() || !templates.is_empty())
                    && !self.constraints.capital_limits.is_empty()
                    && (v.get("constraints").is_none()
                        || serde_json::to_value(&constraints.capital_limits)
                            .map_err(|e| e.to_string())?
                            == serde_json::to_value(&self.constraints.capital_limits)
                                .map_err(|e| e.to_string())?)
                {
                    return Err("CAPITAL_REFRESH_REQUIRED: supply refreshed capital limits after position or template edits".into());
                }
                self.nonce += 1;
                let token = self.nonce;
                let out = json!({"token":token,"dirty":dirty,"templates":templates,"constraints":constraints});
                self.pending = Some(Pending {
                    token,
                    patches,
                    templates,
                    records: vec![],
                    scenarios: None,
                    constraints,
                    result: None,
                });
                Ok(out)
            }
            "abort" => {
                self.check_token(&v)?;
                self.pending = None;
                self.solver = Solver::new();
                Ok(json!({"aborted":true,"version":self.version}))
            }
            "stage" => {
                self.check_token(&v)?;
                let p = self.pending.as_ref().unwrap();
                let records: Vec<Record> = decode(&v, "records")?;
                let mut seen = std::collections::HashSet::new();
                if records.len() != p.patches.len() {
                    return Err("dirty position set mismatch".into());
                }
                let mut scenarios = self.scenarios.clone();
                for r in &records {
                    check_record(r, scenarios.len())?;
                    if !p.patches.contains_key(&r.key) || !seen.insert(&r.key) {
                        return Err("dirty position set mismatch".into());
                    }
                    let old = &self.records[&r.key];
                    for (i, s) in scenarios.iter_mut().enumerate() {
                        let d: Vec<f64> = r.metrics[i]
                            .iter()
                            .zip(&old.metrics[i])
                            .map(|(a, b)| a - b)
                            .collect();
                        s.base.eve += d[0];
                        s.base.dv01 += d[1];
                        s.base.nii += d[2];
                        s.base.cet1 += d[2] * s.base.ni;
                        s.base.l2 += d[3];
                        s.base.mv_assets += d[4];
                    }
                }
                let columns: HashMap<String, Vec<Vec<Vec<Vec<f64>>>>> = decode(&v, "columns")?;
                if columns.len() != p.templates.len()
                    || columns.keys().any(|k| !p.templates.contains_key(k))
                {
                    return Err("dirty template set mismatch".into());
                }
                for (name, values) in columns {
                    let indices: Vec<usize> = self
                        .units
                        .iter()
                        .enumerate()
                        .filter(|(_, u)| u.template == name)
                        .map(|(i, _)| i)
                        .collect();
                    if values.len() != scenarios.len()
                        || values.iter().any(|x| x.len() != indices.len())
                    {
                        return Err("template column shape mismatch".into());
                    }
                    for (si, s) in scenarios.iter_mut().enumerate() {
                        for (j, index) in indices.iter().enumerate() {
                            s.vectors[*index] = values[si][j].clone();
                        }
                    }
                }
                let mut result = self.solver.solve(&self.units, &scenarios, &p.constraints)?;
                result["version"] = json!(self.version + 1);
                result["changed_positions"] =
                    json!(records.iter().map(|r| &r.key).collect::<Vec<_>>());
                result["changed_templates"] = json!(p.templates.keys().collect::<Vec<_>>());
                result["bases"] = json!(scenarios.iter().map(|s| &s.base).collect::<Vec<_>>());
                result["token"] = json!(p.token);
                let p = self.pending.as_mut().unwrap();
                p.records = records;
                p.scenarios = Some(scenarios);
                p.result = Some(result.clone());
                Ok(result)
            }
            "publish" => {
                self.check_token(&v)?;
                if self.pending.as_ref().unwrap().result.is_none() {
                    return Err("candidate must be staged before publication".into());
                }
                let p = self.pending.take().unwrap();
                for r in p.records {
                    self.records.insert(r.key.clone(), r);
                }
                self.patches.extend(p.patches);
                self.templates.extend(p.templates);
                self.scenarios = p.scenarios.unwrap();
                self.constraints = p.constraints;
                self.version += 1;
                Ok(p.result.unwrap())
            }
            "eval" => {
                let alloc: Vec<Value> = decode(&v, "allocation")?;
                if alloc.len() > 4096 {
                    return Err("allocation limit exceeded".into());
                }
                let mut x = vec![0.; self.units.len()];
                for a in alloc {
                    let idx = self
                        .units
                        .iter()
                        .position(|u| {
                            Some(u.template.as_str()) == a["template"].as_str()
                                && Some(u.h as u64) == a["purchase_m"].as_u64()
                        })
                        .ok_or("allocation must use a supported template and purchase grid")?;
                    let n = a["notional"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.)
                        .ok_or("invalid notional")?;
                    x[idx] += n;
                }
                if x.iter().any(|n| !n.is_finite()) {
                    return Err("allocation overflow".into());
                }
                let replay = solver::evaluate(&self.scenarios, &x);
                solver::check_replay(&replay)?;
                Ok(json!({"version":self.version,"replay":replay}))
            }
            _ => Err("unknown operation".into()),
        }
    }
    fn check_token(&self, v: &Value) -> Result<(), String> {
        if self.pending.as_ref().map(|p| p.token) != v["token"].as_u64() || self.pending.is_none() {
            Err("stale transaction token".into())
        } else {
            Ok(())
        }
    }
}
type Message = (Value, mpsc::SyncSender<Result<Value, String>>);
static REGISTRY: OnceLock<Mutex<HashMap<u64, mpsc::SyncSender<Message>>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn dispatch(v: Value) -> Result<Value, String> {
    let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    let op = v["op"].as_str().ok_or("missing operation")?;
    if op == "run_owned" {
        return owned::run(v);
    }
    if op == "optimize_library" {
        return library::optimize(&v);
    }
    if op == "linear_program" {
        return solver::linear_program(&v);
    }
    if op == "create" || op == "create_owned" {
        let mut slots = registry.lock().map_err(|_| "registry unavailable")?;
        if slots.len() >= 8 {
            return Err("native session capacity reached (8); close a session".into());
        }
        let handle = NEXT.fetch_add(1, Ordering::Relaxed);
        if handle == 0 {
            return Err("native session handle exhausted".into());
        }
        let (tx, rx) = mpsc::sync_channel::<Message>(8);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        // Reserve capacity atomically, then release the registry before pricing.
        // Opening a book must not block evaluation/close on unrelated sessions.
        slots.insert(handle, tx);
        drop(slots);
        let spawned = std::thread::Builder::new()
            .name(format!("decision-{handle}"))
            .spawn(move || {
                let mut v = v;
                let created = if v["op"] == "create_owned" {
                    owned::Owner::new(&mut v)
                        .map(|(owner, session, snapshot)| (Some(owner), session, snapshot))
                } else {
                    Session::new(&v).map(|session| (None, session, json!({})))
                };
                let (mut owner, mut s) = match created {
                    Ok((owner, session, snapshot)) => {
                        let _ = ready_tx.send(Ok(snapshot));
                        (owner, session)
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok((msg, reply)) = rx.recv() {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Some(owner) = owner.as_mut() {
                            match msg["op"].as_str() {
                                Some("update_owned") => owner.update(&mut s, msg),
                                Some("publish") => {
                                    let result = s.call(msg)?;
                                    owner.publish();
                                    Ok(result)
                                }
                                Some("abort") => {
                                    let result = s.call(msg)?;
                                    owner.abort();
                                    Ok(result)
                                }
                                Some("eval" | "status") => s.call(msg),
                                _ => Err("owned session requires native orchestration".into()),
                            }
                        } else {
                            s.call(msg)
                        }
                    })) {
                        Ok(r) => {
                            let _ = reply.send(r);
                        }
                        Err(_) => {
                            let _ =
                                reply.send(Err("native session panicked and was closed".into()));
                            break;
                        }
                    }
                }
                if let Ok(mut slots) = registry.lock() {
                    slots.remove(&handle);
                }
            })
            .map_err(|e| e.to_string());
        let ready =
            spawned.and_then(|_| ready_rx.recv().map_err(|e| e.to_string()).and_then(|v| v));
        let snapshot = match ready {
            Ok(snapshot) => snapshot,
            Err(error) => {
                registry
                    .lock()
                    .map_err(|_| "registry unavailable")?
                    .remove(&handle);
                return Err(error);
            }
        };
        return Ok(json!({"handle":handle,"version":0,"snapshot":snapshot}));
    }
    let handle = v["handle"].as_u64().ok_or("missing handle")?;
    if op == "close" {
        registry
            .lock()
            .map_err(|_| "registry unavailable")?
            .remove(&handle)
            .ok_or("unknown session")?;
        return Ok(json!({"closed":true}));
    }
    let tx = registry
        .lock()
        .map_err(|_| "registry unavailable")?
        .get(&handle)
        .cloned()
        .ok_or("unknown session")?;
    let (reply, rx) = mpsc::sync_channel(1);
    tx.try_send((v, reply)).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
/// Caller owns the returned string and releases it with decision_free.
///
/// # Safety
/// Input must be a valid NUL-terminated UTF-8 buffer for this call.
#[no_mangle]
pub unsafe extern "C" fn decision_request(input: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if input.is_null() {
            return Err("null input".into());
        }
        let bytes = CStr::from_ptr(input).to_bytes();
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("request exceeds 128 MiB".into());
        }
        let v = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        dispatch(v)
    })
    .unwrap_or_else(|_| Err("native request failed".into()));
    let value = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e}),
    };
    CString::new(value.to_string()).unwrap().into_raw()
}
/// Pointer must be an unfreed result from decision_request.
///
/// # Safety
/// The caller must not free the pointer twice or pass another allocator's memory.
#[no_mangle]
pub unsafe extern "C" fn decision_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(CString::from_raw(pointer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}

/// Full raw graph/unit/solve/accounting/mapping/daily-ledger stream, no callbacks.
pub fn run_workflow(request: Value, output: Box<dyn std::io::Write + Send>) -> Result<(), String> {
    owned::run_streamed(request, output)
}
