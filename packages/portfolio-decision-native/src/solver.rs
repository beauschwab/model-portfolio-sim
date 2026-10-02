use highs::{HighsModelStatus, Model, RowProblem, Sense};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Legacy numerical LP ABI. The final column is free; other columns are
/// nonnegative. The public optimizer now uses library::optimize so financial
/// construction and validation stay inside Rust as well.
pub fn linear_program(v: &Value) -> Result<Value, String> {
    let costs: Vec<f64> = serde_json::from_value(v["costs"].clone()).map_err(|e| e.to_string())?;
    let rows: Vec<Vec<f64>> =
        serde_json::from_value(v["rows"].clone()).map_err(|e| e.to_string())?;
    let rhs: Vec<f64> = serde_json::from_value(v["rhs"].clone()).map_err(|e| e.to_string())?;
    if costs.is_empty()
        || costs.len() > 1025
        || rows.len() > 100000
        || rows.len() != rhs.len()
        || costs.iter().chain(&rhs).any(|x| !x.is_finite())
        || rows
            .iter()
            .any(|r| r.len() != costs.len() || r.iter().any(|x| !x.is_finite()))
    {
        return Err("invalid linear program".into());
    }
    let mut problem = RowProblem::default();
    let mut columns = Vec::new();
    for (i, cost) in costs.iter().enumerate() {
        columns.push(if i == costs.len() - 1 {
            problem.add_column::<f64, _>(*cost, ..)
        } else {
            problem.add_column(*cost, 0.0..)
        });
    }
    for (row, b) in rows.iter().zip(&rhs) {
        let sparse: Vec<_> = row
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0.0)
            .map(|(i, v)| (columns[i], *v))
            .collect();
        problem.add_row(..=*b / 1e6, &sparse);
    }
    let mut model = problem.optimise(Sense::Minimise);
    model.make_quiet();
    model.set_option("threads", 1);
    model.set_option("time_limit", 15.0);
    let solved = model
        .try_solve()
        .map_err(|e| format!("HiGHS solve: {e:?}"))?;
    if solved.status() != HighsModelStatus::Optimal {
        return Ok(json!({"success":false,"message":format!("{:?}",solved.status())}));
    }
    let solution = solved.get_solution();
    let x: Vec<_> = solution.columns().iter().map(|v| v * 1e6).collect();
    if x.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite solver output".into());
    }
    Ok(json!({"success":true,"x":x,"marginals":solution.dual_rows(),"message":"Optimal"}))
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
pub struct Unit {
    pub template: String,
    pub h: usize,
    pub side: f64,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Scenario {
    pub base: Base,
    pub vectors: Vec<Vec<Vec<f64>>>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Base {
    pub nii: f64,
    pub dv01: f64,
    pub eve: f64,
    pub l1: f64,
    pub l2: f64,
    pub nco: f64,
    pub asf: f64,
    pub rsf: f64,
    pub cet1: f64,
    pub rwa: f64,
    pub ni: f64,
    pub mv_assets: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Constraints {
    pub lcr_min: f64,
    pub nsfr_min: f64,
    pub cet1_min: f64,
    pub eve_limit: f64,
    pub cash_budget: f64,
    pub max_total_assets: Option<f64>,
    pub commercial: Vec<Commercial>,
    pub capital_limits: Vec<CapitalLimit>,
}
/// Prepared eligible-capital/exposure deltas, dollars per dollar allocated.
/// Explicit unit identities prevent applying a vector to a different library.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapitalLimit {
    pub label: String,
    pub policy_id: String,
    pub metric: String,
    pub scenario: usize,
    pub month: usize,
    pub units: Vec<Unit>,
    pub numerator: f64,
    pub denominator: f64,
    pub required_ratio: f64,
    pub numerator_per_unit: Vec<f64>,
    pub denominator_per_unit: Vec<f64>,
}
impl CapitalLimit {
    fn replay(&self, x: &[f64]) -> (f64, f64) {
        let numerator = self.numerator
            + self
                .numerator_per_unit
                .iter()
                .zip(x)
                .map(|(a, b)| a * b)
                .sum::<f64>();
        let denominator = self.denominator
            + self
                .denominator_per_unit
                .iter()
                .zip(x)
                .map(|(a, b)| a * b)
                .sum::<f64>();
        (numerator, denominator)
    }
}
impl Default for Constraints {
    fn default() -> Self {
        Self {
            lcr_min: 1.10,
            nsfr_min: 1.05,
            cet1_min: 0.10,
            eve_limit: 0.15,
            cash_budget: 0.,
            max_total_assets: Some(1e8),
            commercial: vec![],
            capital_limits: vec![],
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Commercial {
    pub label: String,
    pub template: String,
    pub sense: String,
    pub rhs: f64,
}
pub struct Row {
    pub label: String,
    pub coefs: Vec<f64>,
    pub rhs: f64,
}
pub struct Solver {
    model: Option<Model>,
    previous: Vec<Row>,
}
impl Solver {
    pub fn new() -> Self {
        Self {
            model: None,
            previous: vec![],
        }
    }
    pub fn solve(
        &mut self,
        units: &[Unit],
        scenarios: &[Scenario],
        c: &Constraints,
    ) -> Result<Value, String> {
        let result = self.solve_inner(units, scenarios, c);
        if result.is_err() {
            self.model = None;
            self.previous.clear();
        }
        result
    }
    fn solve_inner(
        &mut self,
        units: &[Unit],
        scenarios: &[Scenario],
        c: &Constraints,
    ) -> Result<Value, String> {
        validate(units, scenarios, c)?;
        let rows = rows(units, scenarios, c);
        if rows
            .iter()
            .any(|r| !r.rhs.is_finite() || r.coefs.iter().any(|x| !x.is_finite()))
        {
            return Err("LP coefficient overflow".into());
        }
        let n = units.len();
        let reused = self.model.is_some()
            && rows.len() == self.previous.len()
            && rows
                .iter()
                .zip(&self.previous)
                .all(|(a, b)| a.label == b.label);
        let mut changed = 0;
        let mut model = if reused {
            let mut m = self.model.take().unwrap();
            for (i, (a, b)) in rows.iter().zip(&self.previous).enumerate() {
                // Indices come exclusively from validated, equal-sized models. HiGHS owns
                // the pointer for this actor thread; no pointer escapes this module.
                if a.rhs != b.rhs {
                    let status = unsafe {
                        highs_sys::Highs_changeRowBounds(
                            m.as_mut_ptr(),
                            i as _,
                            f64::NEG_INFINITY,
                            a.rhs / 1e6,
                        )
                    };
                    if status < 0 {
                        return Err("HiGHS row update failed".into());
                    }
                    changed += 1;
                }
                for (j, (x, y)) in a.coefs.iter().zip(&b.coefs).enumerate() {
                    if x != y {
                        let status = unsafe {
                            highs_sys::Highs_changeCoeff(m.as_mut_ptr(), i as _, j as _, *x)
                        };
                        if status < 0 {
                            return Err("HiGHS coefficient update failed".into());
                        }
                        changed += 1;
                    }
                }
            }
            m
        } else {
            let mut p = RowProblem::default();
            let mut columns = Vec::new();
            for _ in 0..n {
                columns.push(p.add_column(0., 0.0..));
            }
            columns.push(p.add_column::<f64, _>(-1., ..));
            for r in &rows {
                let sparse: Vec<_> = r
                    .coefs
                    .iter()
                    .enumerate()
                    .filter(|(_, x)| **x != 0.)
                    .map(|(j, x)| (columns[j], *x))
                    .collect();
                p.add_row(..=r.rhs / 1e6, &sparse);
            }
            p.optimise(Sense::Minimise)
        };
        model.make_quiet();
        model.set_option("threads", 1);
        model.set_option("solver", "simplex");
        model.set_option("time_limit", 15.0);
        let solved = model
            .try_solve()
            .map_err(|e| format!("HiGHS solve: {e:?}"))?;
        let status = solved.status();
        let iterations = solved
            .int_info_value(c"simplex_iteration_count")
            .unwrap_or(0);
        let result = if status == HighsModelStatus::Optimal {
            let sol = solved.get_solution();
            if !solved.objective_value().is_finite()
                || sol
                    .columns()
                    .iter()
                    .chain(sol.dual_rows())
                    .any(|x| !x.is_finite())
            {
                return Err("nonfinite solver output".into());
            }
            let x: Vec<f64> = sol.columns()[..n].iter().map(|x| x.max(0.) * 1e6).collect();
            let t = -solved.objective_value() * 1e6;
            if !t.is_finite() {
                return Err("solver objective overflow".into());
            }
            let allocations: Vec<_> = units
                .iter()
                .zip(&x)
                .filter(|(_, x)| **x > 1e-8)
                .map(|(u, x)| json!({"template":u.template,"purchase_m":u.h,"notional":x}))
                .collect();
            let mut binding = Vec::new();
            let mut max_violation: f64 = 0.;
            for (i, r) in rows.iter().enumerate() {
                let lhs =
                    r.coefs[..n].iter().zip(&x).map(|(a, b)| a * b).sum::<f64>() + r.coefs[n] * t;
                let slack = r.rhs - lhs;
                max_violation = max_violation.max(-slack);
                if slack < 0.01 && sol.dual_rows()[i].abs() > 1e-12 {
                    binding.push(json!({"constraint":r.label,"shadow_price":-sol.dual_rows()[i]}));
                }
            }
            if max_violation > 0.1 {
                self.model = Some(Model::from(solved));
                return Err(format!("native LP replay violation {max_violation}"));
            }
            let replay = evaluate(scenarios, &x);
            check_replay(&replay)?;
            validate_allocation(units, &x, &replay, c)?;
            let actual = replay
                .iter()
                .zip(scenarios)
                .map(|(r, s)| r["nii_total_$"].as_f64().unwrap() + s.base.nii)
                .fold(f64::INFINITY, f64::min);
            if (actual - t).abs() > 0.01_f64.max(t.abs() * 1e-7) {
                self.model = Some(Model::from(solved));
                return Err("native objective replay mismatch".into());
            }
            let capital_replay: Vec<_> = c.capital_limits.iter().map(|r| {
                let (numerator,denominator)=r.replay(&x);
                json!({"label":r.label,"policy_id":r.policy_id,"metric":r.metric,"scenario":r.scenario,"month":r.month,
                    "numerator":numerator,"denominator":denominator,"ratio":numerator/denominator,
                    "required_ratio":r.required_ratio,"headroom":numerator-r.required_ratio*denominator})
            }).collect();
            json!({"feasible":true,"worst_case_nii_$":t,"allocation":allocations,"binding_constraints":binding,"replay":replay,"capital_replay":capital_replay,"native_max_violation_$":max_violation,"total_new_assets_$":units.iter().zip(&x).filter(|(u,_)|u.side>0.).map(|(_,x)|x).sum::<f64>()})
        } else if matches!(
            status,
            HighsModelStatus::Infeasible
                | HighsModelStatus::Unbounded
                | HighsModelStatus::UnboundedOrInfeasible
        ) {
            json!({"feasible":false,"allocation":[],"message":format!("{status:?}"),"labels":rows.iter().map(|r| &r.label).collect::<Vec<_>>()})
        } else {
            self.model = Some(Model::from(solved));
            return Err(format!(
                "solver did not finish with a validated result: {status:?}"
            ));
        };
        self.model = Some(Model::from(solved));
        self.previous = rows;
        let mut result = result;
        result["solver"] = json!({"backend":"Rust-owned HiGHS (C++)","model_reused":reused,"changed_matrix_entries":changed,"simplex_iterations":iterations});
        Ok(result)
    }
}
pub fn validate(units: &[Unit], scenarios: &[Scenario], c: &Constraints) -> Result<(), String> {
    if units.is_empty() || units.len() > 1024 || scenarios.is_empty() || scenarios.len() > 13 {
        return Err("unsupported unit or scenario count".into());
    }
    let h = scenarios[0]
        .vectors
        .first()
        .and_then(|x| x.first())
        .map_or(0, Vec::len);
    if h == 0 || h > 120 {
        return Err("horizon must be 1..120".into());
    }
    if [
        c.lcr_min,
        c.nsfr_min,
        c.cet1_min,
        c.eve_limit,
        c.cash_budget,
    ]
    .iter()
    .any(|x| !x.is_finite() || *x < 0.)
        || c.max_total_assets.is_some_and(|x| !x.is_finite() || x < 0.)
        || c.commercial.len() > 100
    {
        return Err("invalid constraints".into());
    }
    let mut keys = std::collections::HashSet::new();
    for u in units {
        if !keys.insert((&u.template, u.h)) || u.h >= h || ![-1., 1.].contains(&u.side) {
            return Err("invalid or duplicate unit".into());
        }
    }
    for s in scenarios {
        if [
            s.base.nii,
            s.base.dv01,
            s.base.eve,
            s.base.l1,
            s.base.l2,
            s.base.nco,
            s.base.asf,
            s.base.rsf,
            s.base.cet1,
            s.base.rwa,
            s.base.ni,
            s.base.mv_assets,
        ]
        .iter()
        .any(|x| !x.is_finite())
            || s.base.eve <= 0.
            || s.vectors.len() != units.len()
            || s.vectors.iter().any(|u| {
                u.len() != 10
                    || u.iter()
                        .any(|v| v.len() != h || v.iter().any(|x| !x.is_finite()))
            })
        {
            return Err("invalid scenario shape or base EVE".into());
        }
    }
    for x in &c.commercial {
        if !x.rhs.is_finite()
            || ![">=", "<="].contains(&x.sense.as_str())
            || !(units.iter().any(|u| u.template == x.template)
                || ["ALL_ASSET", "ALL_LIAB"].contains(&x.template.as_str()))
        {
            return Err("invalid commercial constraint".into());
        }
    }
    if c.capital_limits.len() > 2048 {
        return Err("too many capital limits".into());
    }
    let mut labels = std::collections::HashSet::new();
    for r in &c.capital_limits {
        if r.label.is_empty()
            || r.label.len() > 200
            || r.policy_id.is_empty()
            || r.policy_id.len() > 200
            || !labels.insert(&r.label)
            || !portfolio_risk_native::treasury::METRICS.contains(&r.metric.as_str())
            || r.scenario >= scenarios.len()
            || r.month >= h
            || r.units != units
            || !r.numerator.is_finite()
            || !r.denominator.is_finite()
            || r.denominator <= 0.
            || !r.required_ratio.is_finite()
            || !(0. ..=1.).contains(&r.required_ratio)
            || r.numerator_per_unit.len() != units.len()
            || r.denominator_per_unit.len() != units.len()
            || r.numerator_per_unit.iter().any(|v| !v.is_finite())
            || r.denominator_per_unit
                .iter()
                .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err("invalid capital limit or stale unit grid".into());
        }
    }
    Ok(())
}
fn rows(units: &[Unit], scenarios: &[Scenario], c: &Constraints) -> Vec<Row> {
    let n = units.len();
    let mut out = Vec::new();
    let mut add = |mut coefs: Vec<f64>, t: f64, rhs: f64, label: String| {
        coefs.push(t);
        out.push(Row { label, coefs, rhs });
    };
    for (si, s) in scenarios.iter().enumerate() {
        let b = &s.base;
        let v = &s.vectors;
        let h = v[0][0].len();
        let nii: Vec<f64> = v.iter().map(|u| u[0].iter().sum()).collect();
        add(
            nii.iter().map(|x| -x).collect(),
            1.,
            b.nii,
            format!("s{si}:worst_case_nii"),
        );
        for m in 0..h {
            let vec = |f: &dyn Fn(&Vec<Vec<f64>>) -> f64| v.iter().map(f).collect::<Vec<_>>();
            add(
                vec(&|u| c.lcr_min * u[4][m] - u[3][m]),
                0.,
                b.l1 + b.l2 - c.lcr_min * b.nco,
                format!("s{si}:m{m}:lcr_assets"),
            );
            add(
                vec(&|u| c.lcr_min * u[4][m]),
                0.,
                b.l1 / 0.6 - c.lcr_min * b.nco,
                format!("s{si}:m{m}:lcr_cap"),
            );
            add(
                vec(&|u| c.nsfr_min * u[6][m] - u[5][m]),
                0.,
                b.asf - c.nsfr_min * b.rsf,
                format!("s{si}:m{m}:nsfr"),
            );
            add(
                vec(&|u| u[2][m]),
                0.,
                c.eve_limit * b.eve / 200. - b.dv01,
                format!("s{si}:m{m}:eve_lo"),
            );
            add(
                vec(&|u| -u[2][m]),
                0.,
                c.eve_limit * b.eve / 200. + b.dv01,
                format!("s{si}:m{m}:eve_hi"),
            );
            add(
                vec(&|u| u[8][m] - u[9][m]),
                0.,
                c.cash_budget,
                format!("s{si}:m{m}:funding"),
            );
        }
        add(
            (0..n)
                .map(|i| c.cet1_min * v[i][7][h - 1] - b.ni * nii[i])
                .collect(),
            0.,
            b.cet1 - c.cet1_min * b.rwa,
            format!("s{si}:cet1_horizon"),
        );
    }
    for r in &c.capital_limits {
        add(
            r.denominator_per_unit
                .iter()
                .zip(&r.numerator_per_unit)
                .map(|(d, n)| r.required_ratio * d - n)
                .collect(),
            0.,
            r.numerator - r.required_ratio * r.denominator,
            format!("capital:{}", r.label),
        );
    }
    if let Some(cap) = c.max_total_assets {
        add(
            units
                .iter()
                .map(|u| if u.side > 0. { 1. } else { 0. })
                .collect(),
            0.,
            cap,
            "cap:total_assets".into(),
        );
    }
    for (i, x) in c.commercial.iter().enumerate() {
        let sign = if x.sense == ">=" { -1. } else { 1. };
        add(
            units
                .iter()
                .map(|u| {
                    if x.template == u.template
                        || (x.template == "ALL_ASSET" && u.side > 0.)
                        || (x.template == "ALL_LIAB" && u.side < 0.)
                    {
                        sign
                    } else {
                        0.
                    }
                })
                .collect(),
            0.,
            sign * x.rhs,
            format!("comm:{i}:{}:{}", x.label, x.sense),
        );
    }
    out
}
pub fn evaluate(scenarios: &[Scenario], x: &[f64]) -> Vec<Value> {
    scenarios.iter().map(|s|{
 let h=s.vectors[0][0].len();let mut a=vec![vec![0.;h];10];for (v,n) in s.vectors.iter().zip(x){for k in 0..10{for m in 0..h{a[k][m]+=v[k][m]*n;}}}
 let b=&s.base;let nii:f64=a[0].iter().sum();let lcr:Vec<f64>=(0..h).map(|m|(b.l1+(b.l2+a[3][m]).min(b.l1*0.4/0.6))/(b.nco+a[4][m]).max(1e-9)*100.).collect();let nsfr:Vec<f64>=(0..h).map(|m|(b.asf+a[5][m])/(b.rsf+a[6][m]).max(1e-9)*100.).collect();let eve:Vec<f64>=(0..h).map(|m|-(b.dv01+a[2][m])*200./b.eve*100.).collect();
 json!({"nii_total_$":nii,"nii_incremental":a[0],"funding_gap":(0..h).map(|m|a[8][m]-a[9][m]).collect::<Vec<_>>(),"kpi_path":{"lcr_pct":lcr,"nsfr_pct":nsfr,"d_eve_pct_eve_+200":eve},"kpis":{"cet1_horizon_pct":(b.cet1+nii*b.ni)/(b.rwa+a[7][h-1])*100.}})
}).collect()
}

pub fn check_replay(values: &[Value]) -> Result<(), String> {
    fn invalid(v: &Value) -> bool {
        match v {
            Value::Null => true, // serde_json maps nonfinite numerical results to null.
            Value::Array(a) => a.iter().any(invalid),
            Value::Object(o) => o.values().any(invalid),
            _ => false,
        }
    }
    if values.iter().any(invalid) {
        Err("nonfinite allocation replay".into())
    } else {
        Ok(())
    }
}

/// Validate financial results independently of the assembled LP rows.
/// Keep this check at the owner of the solve, including standalone execution.
fn validate_allocation(
    units: &[Unit],
    x: &[f64],
    replay: &[Value],
    c: &Constraints,
) -> Result<(), String> {
    if x.iter().any(|n| !n.is_finite() || *n < 0.) {
        return Err("invalid solver allocation".into());
    }
    for r in &c.capital_limits {
        let (numerator, denominator) = r.replay(x);
        let need = r.required_ratio * denominator;
        if !numerator.is_finite()
            || !denominator.is_finite()
            || denominator <= 0.
            || numerator < need - 0.01_f64.max(need.abs() * 1e-8)
        {
            return Err("native allocation replay failed explicit capital limit".into());
        }
    }
    for r in replay {
        for (key, floor) in [("lcr_pct", c.lcr_min), ("nsfr_pct", c.nsfr_min)] {
            if r["kpi_path"][key]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_f64().unwrap() < floor * 100. - 1e-5)
            {
                return Err(format!("native allocation replay failed {key}"));
            }
        }
        if r["kpi_path"]["d_eve_pct_eve_+200"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_f64().unwrap().abs() > c.eve_limit * 100. + 1e-5)
            || r["kpis"]["cet1_horizon_pct"].as_f64().unwrap() < c.cet1_min * 100. - 1e-5
            || r["funding_gap"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_f64().unwrap() > c.cash_budget + 0.01_f64.max(c.cash_budget * 1e-7))
        {
            return Err("native allocation replay failed EVE, capital or funding".into());
        }
    }
    let amount = |target: &str| {
        units
            .iter()
            .zip(x)
            .filter(|(u, _)| {
                u.template == target
                    || target == "ALL_ASSET" && u.side > 0.
                    || target == "ALL_LIAB" && u.side < 0.
            })
            .map(|(_, n)| n)
            .sum::<f64>()
    };
    if c.max_total_assets
        .is_some_and(|cap| amount("ALL_ASSET") > cap + 0.01_f64.max(cap * 1e-8))
    {
        return Err("native allocation replay failed asset cap".into());
    }
    for row in &c.commercial {
        let n = amount(&row.template);
        let tol = 0.01_f64.max(row.rhs.abs() * 1e-8);
        if row.sense == ">=" && n < row.rhs - tol || row.sense == "<=" && n > row.rhs + tol {
            return Err("native allocation replay failed commercial constraint".into());
        }
    }
    Ok(())
}
