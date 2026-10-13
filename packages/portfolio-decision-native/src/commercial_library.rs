//! Commercial native preparation, HiGHS construction and retained allocation
//! replay. Constraint updates do not invoke product/credit/accounting generators.
use crate::solver::{
    self, Base, Constraints, DatedFunding, ProductFaceLimit, Scenario, Solver, Unit,
};
use portfolio_risk_native::{commercial_units as product, product_strategy::Allocation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, rc::Rc};
pub const SCHEMA: &str = "strategy-commercial-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioBase {
    pub scenario_id: String,
    pub base: Base,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Unit { unit_id: String },
    Template { template_id: String },
    AccountingCategory { accounting_category: String },
    AllAssets {},
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceLimit {
    pub label: String,
    pub policy_id: String,
    pub selector: Selector,
    pub sense: String,
    pub amount: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub op: String,
    pub schema: String,
    pub commercial: Box<product::Request>,
    pub bases: Vec<ScenarioBase>,
    pub constraints: Constraints,
    pub max_purchase_cost_assets: Option<f64>,
    pub face_limits: Vec<FaceLimit>,
    pub capital_dependency: Option<Box<product::Request>>,
}
pub struct PreparedLibrary {
    request: Request,
    // Session creation and every staged update run inside the dedicated actor
    // thread. Only raw Values and replies cross its channel; financial owners
    // remain thread-local, including their RefCell source caches.
    prepared: Rc<product::Prepared>,
    units: Vec<Unit>,
    scenarios: Vec<Scenario>,
    funding: DatedFunding,
    limits: Vec<ProductFaceLimit>,
    solver: Solver,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("commercial independent source/solver replay mismatch".into())
    } else {
        Ok(())
    }
}
fn exact<T: Serialize>(r: &T) -> Result<Vec<u8>, String> {
    portfolio_model_core::strict_json::encode(r, 32 << 20)
}
impl PreparedLibrary {
    pub fn new(v: &Value) -> Result<Self, String> {
        let r = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        Self::build(r, None)
    }
    fn build(r: Request, retained: Option<Rc<product::Prepared>>) -> Result<Self, String> {
        check()?;
        if r.op != "optimize_commercial"
            || r.schema != SCHEMA
            || r.bases.is_empty()
            || r.bases.len() > 13
            || r.face_limits.len() > 100
            || !r.constraints.commercial.is_empty()
        {
            return Err(
                "commercial strategy requires explicit native units and typed limits".into(),
            );
        }
        if !r.constraints.capital_limits.is_empty() {
            let dependency = r
                .capital_dependency
                .as_ref()
                .ok_or("CAPITAL_REFRESH_REQUIRED: commercial source identity missing")?;
            if exact(dependency)? != exact(&r.commercial)? {
                return Err(
                    "CAPITAL_REFRESH_REQUIRED: immutable commercial preparation changed".into(),
                );
            }
        } else if r.capital_dependency.is_some() {
            return Err("commercial capital dependency needs configured limits".into());
        }
        let prepared = if let Some(p) = retained {
            if exact(&p.source().raw_source)? != exact(&r.commercial)? {
                return Err("retained commercial source identity mismatch".into());
            }
            p
        } else {
            Rc::new(product::prepare(&r.commercial)?)
        };
        let p = prepared.source();
        let [s, n, m, h] = p.shape;
        if m != 10 || r.bases.len() != s {
            return Err("commercial source metric/scenario mismatch".into());
        }
        let units = p
            .units
            .iter()
            .map(|u| Unit {
                template: u.unit_id.clone(),
                h: u.purchase_period,
                side: u.side,
            })
            .collect::<Vec<_>>();
        let mut bases = BTreeMap::new();
        for b in &r.bases {
            if bases.insert(b.scenario_id.as_str(), &b.base).is_some() {
                return Err("duplicate commercial scenario base".into());
            }
        }
        let mut scenarios = Vec::new();
        for (si, id) in p.scenario_ids.iter().enumerate() {
            let base = bases
                .remove(id.as_str())
                .ok_or("commercial scenario baseline absent")?
                .clone();
            let vectors = (0..n)
                .map(|ui| {
                    (0..m)
                        .map(|mi| {
                            let at = ((si * n + ui) * m + mi) * h;
                            p.values[at..at + h].to_vec()
                        })
                        .collect()
                })
                .collect();
            scenarios.push(Scenario { base, vectors });
        }
        if !bases.is_empty() {
            return Err("unbound commercial scenario baseline".into());
        }
        let f = p.funding.ordinals.len();
        let demand = (0..s)
            .map(|si| {
                (0..n)
                    .map(|ui| {
                        let at = (si * n + ui) * f;
                        p.funding.demand[at..at + f].to_vec()
                    })
                    .collect()
            })
            .collect();
        let funding = DatedFunding {
            ordinals: p.funding.ordinals.clone(),
            purchase_costs: p
                .units
                .iter()
                .map(|u| u.quote_price_per_face_unit)
                .collect(),
            demand,
            max_purchase_cost_assets: r.max_purchase_cost_assets,
        };
        let mut limits = Vec::new();
        for l in &r.face_limits {
            if !["<=", ">="].contains(&l.sense.as_str()) {
                return Err("commercial typed face sense requires <= or >=".into());
            }
            let members = p
                .units
                .iter()
                .map(|u| match &l.selector {
                    Selector::Unit { unit_id } => *unit_id == u.unit_id,
                    Selector::Template { template_id } => *template_id == u.template_id,
                    Selector::AccountingCategory {
                        accounting_category,
                    } => *accounting_category == u.accounting_category,
                    Selector::AllAssets {} => true,
                })
                .collect();
            limits.push(ProductFaceLimit {
                label: l.label.clone(),
                policy_id: l.policy_id.clone(),
                members,
                minimum: l.sense == ">=",
                amount: l.amount,
            });
        }
        solver::validate(&units, &scenarios, &r.constraints)?;
        solver::validate_product_limits(&limits, n)?;
        check()?;
        Ok(Self {
            request: r,
            prepared,
            units,
            scenarios,
            funding,
            limits,
            solver: Solver::new(),
        })
    }
    pub fn preparation(&self) -> &product::Prepared {
        &self.prepared
    }
    pub fn snapshot(&self) -> Value {
        json!({"schema":SCHEMA,"preparation":self.prepared,"constraints":self.request.constraints,"bases":self.request.bases,"face_limits":self.request.face_limits,"max_purchase_cost_assets":self.request.max_purchase_cost_assets})
    }
    pub fn evaluate(&self, allocation: &[Allocation]) -> Result<Value, String> {
        let source = product::evaluate_coefficients(&self.prepared, allocation)?;
        let x = allocation.iter().map(|a| a.face).collect::<Vec<_>>();
        let result = solver::evaluate_product_allocation(
            &self.units,
            &self.scenarios,
            &self.request.constraints,
            &self.funding,
            &self.limits,
            &x,
        )?;
        self.finish(result, source)
    }
    pub fn solve(&mut self) -> Result<Value, String> {
        check()?;
        let result = self.solver.solve_with_product_limits(
            &self.units,
            &self.scenarios,
            &self.request.constraints,
            &self.funding,
            &self.limits,
        )?;
        if result["feasible"] != true {
            let mut out = result;
            out["validated"] = json!(false);
            out["commercial_preparation"] =
                serde_json::to_value(&*self.prepared).map_err(|e| e.to_string())?;
            return Ok(out);
        }
        let x: Vec<f64> = serde_json::from_value(result["native_allocated_face"].clone())
            .map_err(|e| e.to_string())?;
        let allocation = self
            .prepared
            .source()
            .units
            .iter()
            .zip(x)
            .map(|(u, face)| Allocation {
                unit_id: u.unit_id.clone(),
                face,
            })
            .collect::<Vec<_>>();
        let source = product::evaluate(&self.prepared, &allocation)?;
        self.finish(result, source)
    }
    fn finish(&self, mut result: Value, source: Value) -> Result<Value, String> {
        check()?;
        let h = self.prepared.source().shape[3];
        let replay = source["scenarios"]
            .as_array()
            .ok_or("commercial source replay absent")?;
        for (si, s) in replay.iter().enumerate() {
            for pi in 0..h {
                near(
                    result["replay"][si]["nii_incremental"][pi]
                        .as_f64()
                        .ok_or("commercial solver NII absent")?,
                    s["values"][pi]
                        .as_f64()
                        .ok_or("commercial source NII absent")?,
                )?;
            }
            near(
                result["replay"][si]["nii_total_$"]
                    .as_f64()
                    .ok_or("commercial solver total NII absent")?,
                s["incremental_external_nii"]
                    .as_f64()
                    .ok_or("commercial source total NII absent")?,
            )?;
            for (di, c) in s["cash"]
                .as_array()
                .ok_or("commercial source cash absent")?
                .iter()
                .enumerate()
            {
                near(
                    result["replay"][si]["funding_gap"][di]
                        .as_f64()
                        .ok_or("commercial solver funding absent")?,
                    c["funding_demand"]
                        .as_f64()
                        .ok_or("commercial source funding absent")?,
                )?;
            }
        }
        result["source_replay"] = source;
        result["validated"] = json!(true);
        result["dynamic_validated"] = json!(false);
        result["execution"] = json!({"schema":SCHEMA,"preparation":"Rust commercial-units-1","solver":"HiGHS (C++)","objective":"worst-case authoritative Physical InterestIncome NII with supplied baselines","evaluation":"retained native metric/funding coefficients; full source cash replay during preparation and solving"});
        check()?;
        Ok(result)
    }
}
pub fn optimize(value: &Value) -> Result<Value, String> {
    let mut library = PreparedLibrary::new(value)?;
    let mut out = library.solve()?;
    out["commercial_preparation"] =
        serde_json::to_value(library.preparation()).map_err(|e| e.to_string())?;
    check()?;
    Ok(out)
}

pub struct Session {
    version: u64,
    nonce: u64,
    library: PreparedLibrary,
    pending: Option<(u64, PreparedLibrary, Value)>,
}
impl Session {
    pub fn new(value: &Value) -> Result<(Self, Value), String> {
        let mut raw = value.clone();
        raw["op"] = json!("optimize_commercial");
        let library = PreparedLibrary::new(&raw)?;
        let snapshot = library.snapshot();
        Ok((
            Self {
                version: 0,
                nonce: 0,
                library,
                pending: None,
            },
            snapshot,
        ))
    }
    pub fn call(&mut self, value: Value) -> Result<Value, String> {
        if value["version"].as_u64() != Some(self.version) {
            return Err("stale commercial session version".into());
        }
        match value["op"].as_str().ok_or("commercial operation absent")? {
            "status"=>Ok(json!({"schema":SCHEMA,"version":self.version,"pending":self.pending.is_some(),"snapshot":self.library.snapshot()})),
            "eval"=>{let allocation:Vec<Allocation>=serde_json::from_value(value["allocation"].clone()).map_err(|e|e.to_string())?;let mut out=self.library.evaluate(&allocation)?;out["version"]=json!(self.version);out["work"]=json!({"units_rebuilt":0,"pricing_calls":0,"credit_calls":0});Ok(out)},
            "optimize"=>{let mut out=self.library.solve()?;out["version"]=json!(self.version);out["work"]=json!({"units_rebuilt":0,"pricing_calls":0,"credit_calls":0});Ok(out)},
            "update_constraints"=>{
                if self.pending.is_some(){return Err("commercial transaction already pending".into());}
                for key in value.as_object().ok_or("commercial update object required")?.keys(){if !["op","version","handle","constraints","bases","face_limits","max_purchase_cost_assets","capital_dependency"].contains(&key.as_str()){return Err(format!("unsupported commercial constraint update {key}"));}}
                let mut request=self.library.request.clone();
                if let Some(v)=value.get("constraints"){request.constraints=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}
                if let Some(v)=value.get("bases"){request.bases=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}
                if let Some(v)=value.get("face_limits"){request.face_limits=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}
                if let Some(v)=value.get("max_purchase_cost_assets"){request.max_purchase_cost_assets=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}
                if let Some(v)=value.get("capital_dependency"){request.capital_dependency=serde_json::from_value(v.clone()).map_err(|e|e.to_string())?;}
                let mut staged=PreparedLibrary::build(request,Some(self.library.prepared.clone()))?;let mut out=staged.solve()?;self.nonce=self.nonce.checked_add(1).ok_or("commercial transaction token exhausted")?;
                out["token"]=json!(self.nonce);out["version"]=json!(self.version);out["work"]=json!({"units_rebuilt":0,"pricing_calls":0,"credit_calls":0});self.pending=Some((self.nonce,staged,out.clone()));Ok(out)
            },
            "publish"|"abort"=>{
                let pending=self.pending.as_ref().ok_or("commercial transaction absent")?;if value["token"].as_u64()!=Some(pending.0){return Err("stale commercial transaction token".into());}
                let (token,library,mut result)=self.pending.take().unwrap();
                if value["op"]=="abort"{return Ok(json!({"aborted":true,"version":self.version,"token":token}));}
                self.version=self.version.checked_add(1).ok_or("commercial session version exhausted")?;self.library=library;result["version"]=json!(self.version);result["published"]=json!(true);Ok(result)
            },
            _=>Err("unsupported commercial retained session operation; changed financial units require explicit full rebuild".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_local_constraint_staging_shares_exact_native_source_owner() {
        let value: Value = serde_json::from_str(include_str!(
            "../../../examples/models/strategy-commercial-1.json"
        ))
        .unwrap();
        let library = PreparedLibrary::new(&value).unwrap();
        let mut request = library.request.clone();
        request.max_purchase_cost_assets = Some(70.);
        let staged =
            PreparedLibrary::build(request, Some(Rc::clone(&library.prepared))).unwrap();
        assert!(Rc::ptr_eq(&library.prepared, &staged.prepared));
        assert_eq!(Rc::strong_count(&library.prepared), 2);

        let mut substituted = library.request.clone();
        substituted.commercial.identity.input_revision = "different-source".into();
        assert!(PreparedLibrary::build(substituted, Some(Rc::clone(&library.prepared)))
            .err()
            .unwrap()
            .contains("retained commercial source identity mismatch"));
        assert_eq!(Rc::strong_count(&library.prepared), 2);
        drop(staged);
        assert_eq!(Rc::strong_count(&library.prepared), 1);
    }
}
