//! Explicit product-unit adoption. HiGHS receives exact native coefficients;
//! retained evaluation never reprices and scenario cash is replayed separately.
use crate::solver::{self, DatedFunding, ProductFaceLimit, Scenario, Solver, Unit};
pub use crate::solver::{Base, Constraints};
use portfolio_risk_native::{
    product_strategy::{self, Allocation, Prepared, UnitIdentity},
    product_units::ProductUnitsRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
mod session;
mod strict_selector;
pub use session::Session;

pub const SCHEMA: &str = "strategy-product-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioBase {
    pub scenario_id: String,
    pub base: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Unit { unit_id: String },
    Template { template_id: String },
    AccountingCategory { accounting_category: String },
    AllAssets,
    AllLiabilities,
}
impl Selector {
    fn selects(&self, u: &UnitIdentity) -> bool {
        match self {
            Self::Unit { unit_id } => *unit_id == u.unit_id,
            Self::Template { template_id } => *template_id == u.template_id,
            Self::AccountingCategory {
                accounting_category,
            } => *accounting_category == u.accounting_category,
            Self::AllAssets => u.side > 0.,
            Self::AllLiabilities => u.side < 0.,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceLimit {
    pub label: String,
    pub policy_id: String,
    #[serde(deserialize_with = "strict_selector::deserialize")]
    pub selector: Selector,
    pub sense: String,
    pub amount: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub op: String,
    pub schema: String,
    pub product: Box<ProductUnitsRequest>,
    pub bases: Vec<ScenarioBase>,
    pub constraints: Constraints,
    pub max_purchase_cost_assets: Option<f64>,
    pub face_limits: Vec<FaceLimit>,
    pub capital_dependency: Option<Box<ProductUnitsRequest>>,
}

/// Owns immutable native preparation and a HiGHS model on its owner thread.
pub struct PreparedLibrary {
    request: Request,
    prepared: Arc<Prepared>,
    units: Vec<Unit>,
    scenarios: Vec<Scenario>,
    funding: DatedFunding,
    limits: Vec<ProductFaceLimit>,
    solver: Solver,
}
impl PreparedLibrary {
    pub fn new(value: &Value) -> Result<Self, String> {
        let r: Request = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        Self::build(r, None)
    }
    fn build(r: Request, retained: Option<Arc<Prepared>>) -> Result<Self, String> {
        if r.schema != SCHEMA || r.op != "optimize_product" {
            return Err("unsupported product strategy contract".into());
        }
        if r.face_limits.len() > 100
            || r.bases.len() > 13
            || r.bases.is_empty()
            || r.max_purchase_cost_assets
                .is_some_and(|x| !(0. ..=product_strategy::MAX_ALLOCATION_AMOUNT).contains(&x))
            || r.constraints.cash_budget.abs() > product_strategy::MAX_ALLOCATION_AMOUNT
            || r.constraints
                .max_total_assets
                .is_some_and(|x| x > product_strategy::MAX_ALLOCATION_AMOUNT)
            || !r.constraints.commercial.is_empty()
        {
            return Err("product admission/cap requires explicit typed face limits, not legacy commercial selectors".into());
        }
        if !r.constraints.capital_limits.is_empty() {
            let d = r
                .capital_dependency
                .as_ref()
                .ok_or("CAPITAL_REFRESH_REQUIRED: immutable product dependency required")?;
            if serde_json::to_value(d).map_err(|e| e.to_string())?
                != serde_json::to_value(&r.product).map_err(|e| e.to_string())?
            {
                return Err("CAPITAL_REFRESH_REQUIRED: product preparation changed".into());
            }
        } else if r.capital_dependency.is_some() {
            return Err("capital dependency requires explicit capital limits".into());
        }
        let prepared = match retained {
            Some(p) => {
                if serde_json::to_value(&p.source().raw_source).map_err(|e| e.to_string())?
                    != serde_json::to_value(&r.product).map_err(|e| e.to_string())?
                {
                    return Err("retained product source identity mismatch".into());
                }
                p
            }
            None => Arc::new(product_strategy::prepare(&r.product)?),
        };
        let p = prepared.source();
        let [s, n, m, h] = p.shape;
        if m != 10 || p.metrics != portfolio_risk_native::aged_units::METRICS || r.bases.len() != s
        {
            return Err("product scenario or metric contract mismatch".into());
        }
        let units: Vec<_> = prepared
            .units()
            .iter()
            .map(|u| Unit {
                template: u.unit_id.clone(),
                h: u.purchase_period,
                side: u.side,
            })
            .collect();
        let mut bases = BTreeMap::new();
        for b in &r.bases {
            let base: Base = if b.base["eve"].is_number() {
                serde_json::from_value(b.base.clone()).map_err(|e| e.to_string())?
            } else {
                crate::library::base(&b.base)?
            };
            if bases.insert(b.scenario_id.as_str(), base).is_some() {
                return Err("duplicate product scenario base".into());
            }
        }
        let mut scenarios = Vec::with_capacity(s);
        for (si, id) in p.scenario_ids.iter().enumerate() {
            let base = bases
                .remove(id.as_str())
                .ok_or("missing product scenario base")?;
            let vectors = (0..n)
                .map(|u| {
                    (0..m)
                        .map(|k| {
                            let offset = ((si * n + u) * m + k) * h;
                            p.values[offset..offset + h].to_vec()
                        })
                        .collect()
                })
                .collect();
            scenarios.push(Scenario { base, vectors });
        }
        if !bases.is_empty() {
            return Err("unknown product scenario base".into());
        }
        let dates = p.funding.ordinals.len();
        if p.funding.shape != [s, n, dates] || p.funding.scenario_ids != p.scenario_ids {
            return Err("product full-tail funding identity mismatch".into());
        }
        let demand = (0..s)
            .map(|si| {
                (0..n)
                    .map(|ui| {
                        let at = (si * n + ui) * dates;
                        p.funding.demand[at..at + dates].to_vec()
                    })
                    .collect()
            })
            .collect();
        let funding = DatedFunding {
            ordinals: p.funding.ordinals.clone(),
            purchase_costs: prepared
                .units()
                .iter()
                .map(|u| u.quote_price_per_face_unit)
                .collect(),
            demand,
            max_purchase_cost_assets: r.max_purchase_cost_assets,
        };
        let mut limits = Vec::with_capacity(r.face_limits.len());
        for l in &r.face_limits {
            if !["<=", ">="].contains(&l.sense.as_str()) {
                return Err("product face-limit sense must be <= or >=".into());
            }
            limits.push(ProductFaceLimit {
                label: l.label.clone(),
                policy_id: l.policy_id.clone(),
                members: prepared
                    .units()
                    .iter()
                    .map(|u| l.selector.selects(u))
                    .collect(),
                minimum: l.sense == ">=",
                amount: l.amount,
            });
        }
        solver::validate(&units, &scenarios, &r.constraints)?;
        solver::validate_product_limits(&limits, units.len())?;
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
    pub fn preparation(&self) -> &Prepared {
        &self.prepared
    }
    pub fn snapshot(&self) -> Result<Value, String> {
        Ok(
            json!({"schema":SCHEMA,"preparation":*self.prepared,"constraints":self.request.constraints,
            "bases":self.request.bases,"face_limits":self.request.face_limits,
            "max_purchase_cost_assets":self.request.max_purchase_cost_assets}),
        )
    }
    /// Complete ordered selected allocation, coefficients only. Financial
    /// infeasibility is an error; no solver, graph or yield reconstruction runs.
    pub fn evaluate(&self, allocation: &[Allocation]) -> Result<Value, String> {
        let source = product_strategy::evaluate(&self.prepared, allocation)?;
        let result = solver::evaluate_product_allocation(
            &self.units,
            &self.scenarios,
            &self.request.constraints,
            &self.funding,
            &self.limits,
            &source.allocated_face,
        )?;
        self.finish(result, source)
    }
    pub fn solve(&mut self) -> Result<Value, String> {
        let result = self.solver.solve_with_product_limits(
            &self.units,
            &self.scenarios,
            &self.request.constraints,
            &self.funding,
            &self.limits,
        )?;
        if result["feasible"] != true {
            let mut result = result;
            result["validated"] = json!(false);
            result["dynamic_validated"] = json!(false);
            result["product_preparation"] =
                serde_json::to_value(&*self.prepared).map_err(|e| e.to_string())?;
            result["execution"] = json!({"schema":SCHEMA,"preparation":"rust product-units-1","solver":"HiGHS (C++)","validation":"non-feasible; no allocation accepted"});
            return Ok(result);
        }
        let x: Vec<f64> = serde_json::from_value(result["native_allocated_face"].clone())
            .map_err(|e| e.to_string())?;
        let a: Vec<_> = self
            .prepared
            .units()
            .iter()
            .zip(x)
            .map(|(u, face)| Allocation {
                unit_id: u.unit_id.clone(),
                face,
            })
            .collect();
        let source = product_strategy::evaluate(&self.prepared, &a)?;
        self.finish(result, source)
    }
    fn finish(&self, mut result: Value, source: product_strategy::Replay) -> Result<Value, String> {
        // Compare independently reduced metric amounts and full source cash to
        // the solver's retained coefficient evaluation, before returning either.
        let h = self.prepared.source().shape[3];
        for (si, s) in source.scenarios.iter().enumerate() {
            let r = &result["replay"][si];
            for period in 0..h {
                near(
                    r["nii_incremental"][period]
                        .as_f64()
                        .ok_or("missing product NII replay")?,
                    s.values[period],
                )?;
            }
            near(
                r["nii_total_$"]
                    .as_f64()
                    .ok_or("missing product NII amount")?,
                s.incremental_external_nii,
            )?;
            for (di, c) in s.cash.iter().enumerate() {
                near(
                    r["funding_gap"][di]
                        .as_f64()
                        .ok_or("missing product funding replay")?,
                    c.funding_demand,
                )?;
            }
        }
        let allocation: Vec<_> = source
            .units
            .iter()
            .zip(&source.allocated_face)
            .map(|(u, face)| json!({"unit":u,"face":face}))
            .collect();
        result["allocation"] = json!(allocation);
        result["source_replay"] = serde_json::to_value(source).map_err(|e| e.to_string())?;
        result["validated"] = json!(true);
        result["dynamic_validated"] = json!(false);
        result["validation_scope"]=json!("native exact product coefficients plus independent metric/source-cash/limit amounts; no mixed candidate daily journal acceptance");
        result["execution"] = json!({"schema":SCHEMA,"preparation":"rust product-units-1","solver":"HiGHS (C++)","funding":"scenario-specific full physical cash tail","objective":"current worst-case external effective-interest NII with supplied scenario baselines"});
        Ok(result)
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("product independent amount replay mismatch".into())
    } else {
        Ok(())
    }
}
pub fn optimize(value: &Value) -> Result<Value, String> {
    let mut library = PreparedLibrary::new(value)?;
    let mut result = library.solve()?;
    result["product_preparation"] =
        serde_json::to_value(library.preparation()).map_err(|e| e.to_string())?;
    Ok(result)
}
