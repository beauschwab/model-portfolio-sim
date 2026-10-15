//! Transactional contribution records, staged edits and coefficient-only evaluation.
use crate::optimization::highs::{self as solver, Constraints, Scenario, Solver, Unit};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Record {
    pub(crate) key: String,
    pub(crate) metrics: Vec<Vec<f64>>,
}
pub(super) struct Pending {
    token: u64,
    patches: HashMap<String, Value>,
    templates: HashMap<String, Value>,
    records: Vec<Record>,
    scenarios: Option<Vec<Scenario>>,
    constraints: Constraints,
    result: Option<Value>,
}
pub(crate) struct Session {
    pub(crate) version: u64,
    nonce: u64,
    pub(crate) records: HashMap<String, Record>,
    pub(crate) patches: HashMap<String, Value>,
    templates: HashMap<String, Value>,
    units: Vec<Unit>,
    pub(super) scenarios: Vec<Scenario>,
    constraints: Constraints,
    pub(super) pending: Option<Pending>,
    solver: Solver,
}
pub(crate) fn decode<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, String> {
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
    pub(crate) fn new(v: &Value) -> Result<Self, String> {
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
    pub(crate) fn call(&mut self, v: Value) -> Result<Value, String> {
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
