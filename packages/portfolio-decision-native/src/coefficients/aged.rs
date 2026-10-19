//! Actual-purchase native preparation feeds HiGHS directly; no metric input route.
use crate::optimization::highs::{Base, Constraints, DatedFunding, Scenario, Solver, Unit};
use portfolio_risk_native::aged_units::{self, AgedUnitsRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

struct Pending {
    token: u64,
    request: Request,
    cache: aged_units::cache::Cache,
    result: Value,
}

/// Native preparation and HiGHS remain on one actor owner thread. Staging does
/// not replace retained economics until the caller's independent publication gate.
pub struct Session {
    version: u64,
    nonce: u64,
    request: Request,
    cache: aged_units::cache::Cache,
    pending: Option<Pending>,
    solver: Solver,
}

impl Session {
    pub fn new(value: &Value) -> Result<(Self, Value), String> {
        let mut raw = value.clone();
        raw["op"] = json!("optimize_aged");
        let request: Request = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        if request.schema != "strategy-aged-1" {
            return Err("unsupported aged strategy contract".into());
        }
        validate_dependencies(&request)?;
        let cache = aged_units::cache::Cache::new(&request.aged)?;
        let snapshot = json!({"preparation":cache.snapshot(),"constraints":request.constraints});
        Ok((
            Self {
                version: 0,
                nonce: 0,
                request,
                cache,
                pending: None,
                solver: Solver::new(),
            },
            snapshot,
        ))
    }

    pub fn call(&mut self, value: Value) -> Result<Value, String> {
        if value["version"].as_u64() != Some(self.version) {
            return Err("stale aged session version".into());
        }
        match value["op"].as_str().ok_or("missing operation")? {
            "status" => Ok(
                json!({"version":self.version,"pending":self.pending.is_some(),
                "schema":"strategy-aged-1","preparation":self.cache.snapshot()}),
            ),
            "eval" => {
                let mut result =
                    solve_prepared(&self.request, self.cache.snapshot(), &mut self.solver)?;
                result["version"] = json!(self.version);
                result["work"] =
                    json!({"templates_rebuilt":0,"units_rebuilt":0,"full_rebuild":false});
                Ok(result)
            }
            "update_aged" => {
                if self.pending.is_some() {
                    return Err("aged update already pending".into());
                }
                for key in value
                    .as_object()
                    .ok_or("aged update must be an object")?
                    .keys()
                {
                    if ![
                        "op",
                        "handle",
                        "version",
                        "aged",
                        "bases",
                        "constraints",
                        "capital_dependency",
                        "max_purchase_cost_assets",
                    ]
                    .contains(&key.as_str())
                    {
                        return Err(format!("unsupported aged update field {key}"));
                    }
                }
                let mut request = self.request.clone();
                if let Some(raw) = value.get("aged") {
                    request.aged =
                        Box::new(serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?);
                }
                if let Some(raw) = value.get("bases") {
                    request.bases =
                        serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
                }
                if let Some(raw) = value.get("constraints") {
                    request.constraints =
                        serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
                }
                if let Some(raw) = value.get("capital_dependency") {
                    request.capital_dependency =
                        serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
                }
                if let Some(raw) = value.get("max_purchase_cost_assets") {
                    request.max_purchase_cost_assets =
                        serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
                }
                validate_dependencies(&request)?;
                let mut cache = self.cache.clone();
                let work = cache.update(&request.aged)?;
                let mut result = solve_prepared(&request, cache.snapshot(), &mut self.solver)?;
                self.nonce = self.nonce.checked_add(1).ok_or("aged token exhausted")?;
                result["token"] = json!(self.nonce);
                result["version"] = json!(self.version);
                result["work"] = serde_json::to_value(work).map_err(|e| e.to_string())?;
                self.pending = Some(Pending {
                    token: self.nonce,
                    request,
                    cache,
                    result: result.clone(),
                });
                Ok(result)
            }
            "publish" | "abort" => {
                let pending = self.pending.as_ref().ok_or("no aged update pending")?;
                if value["token"].as_u64() != Some(pending.token) {
                    return Err("stale aged transaction token".into());
                }
                if value["op"] == "abort" {
                    self.pending = None;
                    return Ok(json!({"aborted":true,"version":self.version}));
                }
                let next = self
                    .version
                    .checked_add(1)
                    .ok_or("aged version exhausted")?;
                let pending = self.pending.take().unwrap();
                self.request = pending.request;
                self.cache = pending.cache;
                self.version = next;
                let mut result = pending.result;
                result["version"] = json!(next);
                Ok(result)
            }
            _ => Err("aged session requires native aged orchestration".into()),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioBase {
    scenario_id: String,
    base: Value,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    op: String,
    schema: String,
    aged: Box<AgedUnitsRequest>,
    bases: Vec<ScenarioBase>,
    constraints: Constraints,
    /// Exact immutable native preparation input against which retained capital
    /// deltas were prepared. Required when capital rows are supplied.
    capital_dependency: Option<Box<AgedUnitsRequest>>,
    max_purchase_cost_assets: Option<f64>,
}

pub fn optimize(value: &Value) -> Result<Value, String> {
    let r: Request = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if r.op != "optimize_aged" || r.schema != "strategy-aged-1" {
        return Err("unsupported aged strategy contract".into());
    }
    validate_dependencies(&r)?;
    let prepared = aged_units::prepare(&r.aged)?;
    solve_prepared(&r, &prepared, &mut Solver::new())
}

fn validate_dependencies(r: &Request) -> Result<(), String> {
    if !r.constraints.capital_limits.is_empty() {
        let dependency = r
            .capital_dependency
            .as_ref()
            .ok_or("CAPITAL_REFRESH_REQUIRED: aged model/grid dependencies required")?;
        if serde_json::to_value(dependency).map_err(|e| e.to_string())?
            != serde_json::to_value(&r.aged).map_err(|e| e.to_string())?
        {
            return Err("CAPITAL_REFRESH_REQUIRED: aged product preparation changed".into());
        }
    } else if r.capital_dependency.is_some() {
        return Err("capital dependency requires explicit capital limits".into());
    }
    Ok(())
}

fn solve_prepared(
    r: &Request,
    prepared: &aged_units::AgedUnitsResult,
    solver: &mut Solver,
) -> Result<Value, String> {
    let [s, n, m, h] = prepared.shape;
    if m != 10 || prepared.values.len() != s * n * m * h || r.bases.len() != s {
        return Err("aged scenario shape mismatch".into());
    }
    let mut bases = BTreeMap::new();
    for row in &r.bases {
        let base: Base = if row.base["eve"].is_number() {
            serde_json::from_value(row.base.clone()).map_err(|e| e.to_string())?
        } else {
            crate::coefficients::monthly::base(&row.base)?
        };
        if bases.insert(row.scenario_id.clone(), base).is_some() {
            return Err("duplicate aged scenario base".into());
        }
    }
    let units: Vec<_> = prepared
        .units
        .iter()
        .map(|u| Unit {
            template: u.template.clone(),
            h: u.h,
            side: u.side,
            swap: false,
        })
        .collect();
    let mut scenarios = Vec::with_capacity(s);
    for (scenario_index, id) in prepared.scenario_ids.iter().enumerate() {
        let base = bases.remove(id).ok_or("missing aged scenario base")?;
        let vectors = (0..n)
            .map(|unit| {
                (0..m)
                    .map(|metric| {
                        let offset = ((scenario_index * n + unit) * m + metric) * h;
                        prepared.values[offset..offset + h].to_vec()
                    })
                    .collect()
            })
            .collect();
        scenarios.push(Scenario { base, vectors });
    }
    if !bases.is_empty() {
        return Err("unknown aged scenario base".into());
    }
    let [fs, fn_, dates] = prepared.funding.shape;
    if fs != s || fn_ != n || dates != prepared.funding.ordinals.len() {
        return Err("native dated funding identity mismatch".into());
    }
    let demand = (0..s)
        .map(|scenario| {
            (0..n)
                .map(|unit| {
                    let start = (scenario * n + unit) * dates;
                    prepared.funding.demand[start..start + dates].to_vec()
                })
                .collect()
        })
        .collect();
    let funding = DatedFunding {
        ordinals: prepared.funding.ordinals.clone(),
        purchase_costs: prepared
            .funding
            .purchase_costs
            .iter()
            .map(|p| p.cash_price_per_face_unit)
            .collect(),
        demand,
        max_purchase_cost_assets: r.max_purchase_cost_assets,
    };
    let mut result =
        solver.solve_with_funding(&units, &scenarios, &r.constraints, Some(&funding))?;
    if result["feasible"] == true {
        let allocation: Vec<f64> = serde_json::from_value(result["native_allocated_face"].clone())
            .map_err(|e| e.to_string())?;
        let mut cumulative = 0.;
        let mut event = 0;
        let mut cash_replay = Vec::with_capacity(dates);
        for (d, ordinal) in prepared.funding.ordinals.iter().enumerate() {
            let mut purchase = 0.;
            let mut principal = 0.;
            let mut interest = 0.;
            while event < prepared.funding.cash_events.len()
                && prepared.funding.cash_events[event].ordinal <= *ordinal
            {
                let e = &prepared.funding.cash_events[event];
                let amount = allocation[e.unit_index];
                purchase += amount * e.purchase_cash;
                principal += amount * e.principal_cash;
                interest += amount * e.interest_cash;
                cumulative += amount * e.bank_cash;
                event += 1;
            }
            for replay in result["replay"].as_array().unwrap() {
                let demand = replay["funding_gap"][d]
                    .as_f64()
                    .ok_or("invalid dated funding replay")?;
                if !cumulative.is_finite()
                    || (demand + cumulative).abs() > 0.01_f64.max(demand.abs() * 1e-7)
                {
                    return Err("native allocation cash-event replay mismatch".into());
                }
            }
            cash_replay.push(json!({"ordinal":ordinal,"purchase_cash":purchase,"principal_cash":principal,
                "interest_cash":interest,"cumulative_bank_cash":cumulative,"funding_demand":-cumulative}));
        }
        if event != prepared.funding.cash_events.len() {
            return Err("native allocation cash tail not replayed".into());
        }
        result["cash_event_replay"] = json!(cash_replay);
        result["funding_policy"] = json!(prepared.funding.policy);
    }
    result["execution"] = json!({"schema":"strategy-aged-1","product_preparation":"rust","coefficient_construction":"rust","constraint_construction":"rust","allocation_replay":"rust","solver":"HiGHS (C++)","dv01_method":"native repricing of actual aged remaining rights at fixed purchase OAS"});
    result["aged_preparation"] = serde_json::to_value(prepared).map_err(|e| e.to_string())?;
    result["dynamic_validated"] = json!(false);
    result["dynamic_validation_status"] = json!("not_run");
    if result["feasible"] == true {
        result["validated"] = json!(true);
        result["validation_scope"]=json!("native actual-date coefficient and allocation replay; separate daily journal acceptance required");
    }
    Ok(result)
}
