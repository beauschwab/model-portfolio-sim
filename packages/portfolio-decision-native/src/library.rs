//! Strategy ownership boundary: raw unit cashflows and base KPIs enter; Rust
//! constructs allocation tensors, constraint rows, solves and replays the result.
//! No Python-prepared coefficient tensors or callbacks are accepted here.
use crate::solver::{Base, Constraints, Scenario, Solver, Unit};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Weights {
    hqla_l2a: f64,
    outflow30: f64,
    asf: f64,
    rsf: f64,
    rwa: f64,
}

#[derive(Deserialize)]
struct Library {
    units: Vec<Unit>,
    horizon: usize,
    nii: Vec<Vec<f64>>,
    balance: Vec<Vec<f64>>,
    dv01: Vec<f64>,
    templates: HashMap<String, Weights>,
}

impl Library {
    fn vectors(&self) -> Result<Vec<Vec<Vec<f64>>>, String> {
        let n = self.units.len();
        let h = self.horizon;
        if n == 0
            || n > 1024
            || h == 0
            || h > 120
            || self.nii.len() != n
            || self.balance.len() != n
            || self.dv01.len() != n
            || self
                .nii
                .iter()
                .chain(&self.balance)
                .any(|v| v.len() < h || v.iter().any(|n| !n.is_finite()))
            || self.dv01.iter().any(|n| !n.is_finite())
        {
            return Err("invalid raw unit-library shape or values".into());
        }
        let mut previous = HashMap::new();
        let mut out = Vec::with_capacity(n);
        for (i, u) in self.units.iter().enumerate() {
            if u.h >= h || ![-1., 1.].contains(&u.side) || u.template.is_empty() {
                return Err("invalid library unit".into());
            }
            if let Some((last_h, side)) = previous.insert(&u.template, (u.h, u.side)) {
                if u.h <= last_h || u.side != side {
                    return Err("template grid must be increasing with one side".into());
                }
            }
            let w = self
                .templates
                .get(&u.template)
                .ok_or("missing template weights")?;
            let weights = [w.hqla_l2a, w.outflow30, w.asf, w.rsf, w.rwa];
            if weights.iter().any(|n| !n.is_finite() || *n < 0.) {
                return Err("invalid regulatory weight".into());
            }
            let mut v = vec![vec![0.; h]; 10];
            for (age, (income, opening)) in self.nii[i]
                .iter()
                .zip(&self.balance[i])
                .take(h - u.h)
                .enumerate()
            {
                let m = u.h + age;
                let balance = opening.max(0.);
                v[0][m] = u.side * income;
                v[1][m] = balance;
                v[2][m] = u.side * self.dv01[i] * balance / self.balance[i][0].max(1e-12);
                for (j, weight) in weights.iter().enumerate() {
                    v[3 + j][m] = weight * balance;
                }
                v[if u.side > 0. { 8 } else { 9 }][m] = balance;
            }
            out.push(v);
        }
        Ok(out)
    }
}

fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| "missing or nonfinite base KPI".into())
}

pub(crate) fn base(v: &Value) -> Result<Base, String> {
    let e = &v["eve"];
    let l = &v["lcr"];
    let n = &v["nsfr"];
    let c = &v["capital"];
    let capital = c["cet1_path"]
        .as_array()
        .and_then(|a| a.last())
        .ok_or("missing capital path")?;
    Ok(Base {
        nii: number(v.get("nii_total_$").unwrap_or(&json!(0.)))?,
        dv01: number(&e["dv01_net_$"])?,
        eve: number(&e["eve_$"])?,
        mv_assets: number(&e["mv_assets_$"])?,
        l1: number(l.get("hqla_l1_$").unwrap_or(&l["hqla_$"]))?,
        l2: number(
            l.get("hqla_l2a_uncapped_$")
                .or_else(|| l.get("hqla_l2a_$"))
                .unwrap_or(&json!(0.)),
        )?,
        nco: number(&l["net_outflows_$"])?,
        asf: number(&n["asf_$"])?,
        rsf: number(&n["rsf_$"])?,
        cet1: number(&capital["cet1_$"])?,
        rwa: number(&c["rwa_total_$"])?,
        // Existing model assumptions, versioned with this library contract.
        ni: 0.43 * (1. - 0.45),
    })
}

pub fn optimize(request: &Value) -> Result<Value, String> {
    if request["schema"].as_str() != Some("strategy-library-1") {
        return Err("unsupported strategy library schema".into());
    }
    let inputs = request["scenarios"].as_array().ok_or("missing scenarios")?;
    if inputs.is_empty() || inputs.len() > 13 {
        return Err("unsupported scenario count".into());
    }
    let constraints: Constraints =
        serde_json::from_value(request["constraints"].clone()).map_err(|e| e.to_string())?;
    let mut units = Vec::new();
    let mut scenarios = Vec::new();
    let mut horizon = 0;
    for (i, input) in inputs.iter().enumerate() {
        let lib: Library =
            serde_json::from_value(input["library"].clone()).map_err(|e| e.to_string())?;
        let vectors = lib.vectors()?;
        if i == 0 {
            horizon = lib.horizon;
            units = lib.units;
        } else if units != lib.units || horizon != lib.horizon {
            return Err("scenario unit grids and horizons must match".into());
        }
        scenarios.push(Scenario {
            base: base(&input["base"])?,
            vectors,
        });
    }
    let mut answer = Solver::new().solve(&units, &scenarios, &constraints)?;
    answer["execution"] = json!({"schema":"strategy-library-1", "coefficient_construction":"rust", "constraint_construction":"rust", "allocation_replay":"rust", "solver":"HiGHS (C++)"});
    if answer["feasible"] == true {
        answer["validated"] = json!(true);
        answer["validation_scope"] = json!("linear coefficient replay only");
        answer["dynamic_validated"] = json!(false);
        answer["dynamic_validation_status"] = json!("not_run");
        answer["cash_budget_$"] = json!(constraints.cash_budget);
        answer["horizon_months"] = json!(horizon);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purchase_shift_and_liability_signs_are_hand_checked() {
        let lib: Library = serde_json::from_value(json!({
            "units":[{"template":"deposit","h":2,"side":-1.}],
            "horizon":4,"nii":[[0.02,0.03,0.04,0.05]],
            "balance":[[2.,1.,0.,0.]],"dv01":[0.1],
            "templates":{"deposit":{"asf":0.9,"outflow30":0.2}}
        }))
        .unwrap();
        let v = lib.vectors().unwrap();
        assert_eq!(v[0][0], [0., 0., -0.02, -0.03]);
        assert_eq!(v[0][2], [0., 0., -0.1, -0.05]);
        assert_eq!(v[0][4], [0., 0., 0.4, 0.2]);
        assert_eq!(v[0][5], [0., 0., 1.8, 0.9]);
        assert_eq!(v[0][8], [0.; 4]);
        assert_eq!(v[0][9], [0., 0., 2., 1.]);
    }
}
