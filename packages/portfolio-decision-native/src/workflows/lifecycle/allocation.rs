//! Stage H (issue #50): prepared lifecycle allocation over reviewed discrete action packages.
//!
//! Preparation replays the hold baseline and every package exactly, alone, through the native
//! lifecycle (Risk pricing, the model-core engine, the ledger owner in the shared Sim), and records
//! per-scenario coefficients against the baseline: external NII (dated recognition, unchanged by
//! the lifecycle owner), trading P&L (realized on exits), settlement and funding cost (fees and
//! funding interest), and the worst daily deterioration of the minimum usable cash.
//!
//! Selection is a HiGHS MIP: at most one package per group, maximizing the worst-case estimate of
//! the selected objective (worst-case external NII unless an explicitly selected objective says
//! otherwise), subject to a conservative usable-cash bound: the baseline minimum plus the sum of each
//! selected package's worst deterioration. Netting, margin, funding policy and inventory are not
//! additive, so an estimate is never a validated plan. The selected packages are merged
//! structurally and replayed exactly; only a replay that runs and keeps every scenario's minimum
//! usable cash at the floor is validated by this producer (the service then verifies its stream
//! independently). A failed candidate is excluded by an exact combinatorial cut (that selection
//! only; no linearized margin cut is invented) and the problem is re-solved, a bounded number of
//! times. Exhaustion returns no validated plan, with the violating scenario, date, event and value.
use super::acquisition;
use highs::{HighsModelStatus, RowProblem, Sense};
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::sync::{Arc, Mutex};

pub const SCHEMA: &str = "liquidity-allocation-1";
pub const MAX_PACKAGES: usize = 32;
pub const MAX_REFINEMENTS: u32 = 8;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const INSTRUCTION_KINDS: [&str; 9] = [
    "purchases",
    "financings",
    "facility_draws",
    "repayments",
    "sales",
    "hedge_entries",
    "terminations",
    "facility_repayments",
    "initial_margin",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Objective {
    /// The existing strategy objective.
    WorstCaseExternalNii,
    /// Explicitly selected: external NII plus trading P&L less settlement and funding cost.
    WorstCaseTotalEarnings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub package_id: String,
    pub group: String,
    pub review: Value,
    pub request: Value,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraints {
    pub minimum_usable_cash: f64,
    pub objective: Objective,
    pub max_refinements: u32,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub base: Value,
    pub packages: Vec<Package>,
    pub constraints: Constraints,
}

/// Exact measures of one replay in one scenario.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Measures {
    pub external_nii: f64,
    pub trading_pnl: f64,
    pub funding_cost: f64,
    pub minimum_usable_cash: f64,
    /// Minimum usable cash after any lifecycle event on each day of the horizon.
    #[serde(skip)]
    pub daily_minimum: Vec<f64>,
    #[serde(skip)]
    pub worst: Option<(i64, String, f64)>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Coefficients {
    pub external_nii: f64,
    pub trading_pnl: f64,
    pub funding_cost: f64,
    /// Worst daily change of the minimum usable cash against the baseline (conservative bound).
    pub usable_cash_deterioration: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct PreparedPackage {
    pub package_id: String,
    pub group: String,
    pub review: Value,
    pub request_sha256: String,
    pub eligible: bool,
    /// Why the package cannot be selected (its exact replay refused).
    pub refusal: Option<String>,
    pub coefficients: BTreeMap<String, Coefficients>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub scenario_id: Option<String>,
    pub ordinal: Option<i64>,
    pub event_id: Option<String>,
    pub measure: String,
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Attempt {
    pub selection: Vec<String>,
    pub estimate: BTreeMap<String, Value>,
    pub exact: Option<BTreeMap<String, Measures>>,
    pub violation: Option<Violation>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub schema: &'static str,
    pub identity_sha256: String,
    pub objective: Objective,
    pub minimum_usable_cash: f64,
    pub scenarios: Vec<String>,
    pub base: BTreeMap<String, Measures>,
    pub base_daily_minimum: BTreeMap<String, f64>,
    pub packages: Vec<PreparedPackage>,
    pub attempts: Vec<Attempt>,
    /// `validated`, `estimate_infeasible` or `exhausted`.
    pub outcome: &'static str,
    /// Why no plan was validated, when none was.
    pub violation: Option<Violation>,
    pub selected: Option<Vec<String>>,
    /// The producer's exact replay passed; independent verification of the stream is separate.
    pub exact_replay_passed: bool,
    pub combined_request: Option<Value>,
    pub disclosure: &'static str,
}

struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("capture").extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn sha(v: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).expect("value"))
    )
}

/// Runs one lifecycle request exactly; returns its stream bytes or the native refusal.
fn replay(request: &Value) -> Result<Vec<u8>, String> {
    portfolio_compute_control::checkpoint()?;
    let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let out = Arc::new(Mutex::new(Vec::new()));
    acquisition::run(&bytes, Box::new(Capture(out.clone())))?;
    let stream = std::mem::take(&mut *out.lock().expect("capture"));
    Ok(stream)
}

fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.)
}

/// Exact measures per scenario from a lifecycle stream.
fn measure(stream: &[u8], asof: i64, end: i64) -> Result<BTreeMap<String, Measures>, String> {
    let text = std::str::from_utf8(stream).map_err(|_| "stream is not UTF-8")?;
    let mut out: BTreeMap<String, Measures> = BTreeMap::new();
    let mut nii: BTreeMap<String, f64> = BTreeMap::new();
    let mut interest: BTreeMap<String, f64> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if let Some(w) = v.get("workflow") {
            for j in w["source_receipt"]["journals"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let r = &j["receipt"];
                let scenario = j["scenario_id"].as_str().unwrap_or("").to_string();
                let days = (end - asof + 1).max(1) as usize;
                let mut daily = vec![f64::INFINITY; days];
                let mut usable = f(r, "opening_cash");
                let mut minimum = usable;
                let mut worst: Option<(i64, String, f64)> = None;
                let steps = r["steps"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let mut cursor = 0usize;
                for (d, slot) in daily.iter_mut().enumerate() {
                    let ordinal = asof + d as i64;
                    let mut day_min = usable;
                    while cursor < steps.len() && steps[cursor]["ordinal"].as_i64() == Some(ordinal)
                    {
                        usable = f(&steps[cursor], "usable_cash_after");
                        day_min = day_min.min(usable);
                        if usable < minimum {
                            minimum = usable;
                            worst = Some((
                                ordinal,
                                steps[cursor]["event_id"].as_str().unwrap_or("").to_string(),
                                usable,
                            ));
                        }
                        cursor += 1;
                    }
                    *slot = day_min;
                }
                out.insert(
                    scenario,
                    Measures {
                        external_nii: 0.,
                        trading_pnl: f(r, "realized"),
                        funding_cost: f(r, "fees_expensed"),
                        minimum_usable_cash: minimum,
                        daily_minimum: daily,
                        worst,
                    },
                );
            }
            continue;
        }
        match v.get("table").and_then(Value::as_str) {
            Some("ledger") => {
                for row in v["rows"].as_array().into_iter().flatten() {
                    // External NII: earnings of every non-lifecycle event (dated recognition).
                    if row["event"].as_str() != Some("native_lifecycle_intent") {
                        *nii.entry(row["scenario"].as_str().unwrap_or("").to_string())
                            .or_default() += f(row, "earnings");
                    }
                }
            }
            Some("lifecycle_journal_lineage") => {
                for row in v["rows"].as_array().into_iter().flatten() {
                    if row["gl_account"].as_str() == Some("pnl:lifecycle_interest") {
                        *interest
                            .entry(row["scenario"].as_str().unwrap_or("").to_string())
                            .or_default() += f(row, "debit_amount");
                    }
                }
            }
            _ => {}
        }
    }
    for (s, m) in out.iter_mut() {
        m.external_nii = nii.get(s).copied().unwrap_or(0.);
        m.funding_cost += interest.get(s).copied().unwrap_or(0.);
    }
    Ok(out)
}

fn objective_of(m: &Measures, o: Objective) -> f64 {
    match o {
        Objective::WorstCaseExternalNii => m.external_nii,
        Objective::WorstCaseTotalEarnings => m.external_nii + m.trading_pnl - m.funding_cost,
    }
}

/// Structural merge of the selected package requests: one book, concatenated actions and
/// instructions per scenario. Packages must agree on the book, allocations and business plan.
pub fn merge(base: &Value, selected: &[&Value]) -> Result<Value, String> {
    let Some((first, rest)) = selected.split_first() else {
        return Ok(base.clone());
    };
    let mut out = (*first).clone();
    let strip = |v: &Value| -> Value {
        let mut p = v["plan"].clone();
        if let Some(o) = p.as_object_mut() {
            o.remove("actions");
        }
        p
    };
    for other in rest {
        if other["dated"] != out["dated"]
            || other["hedge_contracts"] != out["hedge_contracts"]
            || strip(other) != strip(&out)
        {
            return Err(
                "packages differ in their book, allocations or treatments and cannot be combined"
                    .into(),
            );
        }
        let actions = other["plan"]["actions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        out["plan"]["actions"]
            .as_array_mut()
            .ok_or("plan actions")?
            .extend(actions);
        let theirs = other["acquisitions"].as_array().ok_or("acquisitions")?;
        let mine = out["acquisitions"].as_array_mut().ok_or("acquisitions")?;
        if theirs.len() != mine.len() {
            return Err("packages declare different scenarios".into());
        }
        for (m, t) in mine.iter_mut().zip(theirs) {
            if m["scenario_id"] != t["scenario_id"]
                || m["horizon_end_ordinal"] != t["horizon_end_ordinal"]
                || m.get("business_plan") != t.get("business_plan")
            {
                return Err("packages differ in scenario horizons or business plans".into());
            }
            for k in INSTRUCTION_KINDS {
                if let Some(list) = t.get(k).and_then(Value::as_array) {
                    let entry = m
                        .as_object_mut()
                        .ok_or("block")?
                        .entry(k)
                        .or_insert_with(|| json!([]));
                    entry
                        .as_array_mut()
                        .ok_or("instruction list")?
                        .extend(list.iter().cloned());
                }
            }
        }
    }
    let ids: Vec<&str> = out["plan"]["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["action_id"].as_str())
        .collect();
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("packages reuse an action id".into());
    }
    Ok(out)
}

struct Problem<'a> {
    scenarios: &'a [String],
    base: &'a BTreeMap<String, Measures>,
    packages: &'a [PreparedPackage],
    objective: Objective,
    floor: f64,
}

/// The MIP over eligible packages; `cuts` are exact selections already refused.
fn solve(p: &Problem, cuts: &[BTreeSet<usize>]) -> Result<Option<BTreeSet<usize>>, String> {
    let mut pb = RowProblem::default();
    let x: Vec<_> = p
        .packages
        .iter()
        .map(|pk| pb.add_integer_column(0., 0..=if pk.eligible { 1 } else { 0 }))
        .collect();
    let t = pb.add_column::<f64, _>(-1., ..);
    for s in p.scenarios {
        // t <= base objective + sum(delta x)
        let mut row = vec![(t, 1.)];
        for (i, pk) in p.packages.iter().enumerate() {
            let c = &pk.coefficients[s];
            let delta = match p.objective {
                Objective::WorstCaseExternalNii => c.external_nii,
                Objective::WorstCaseTotalEarnings => {
                    c.external_nii + c.trading_pnl - c.funding_cost
                }
            };
            row.push((x[i], -delta));
        }
        pb.add_row(..=objective_of(&p.base[s], p.objective), &row);
        // Conservative usable cash: base minimum + sum of worst deteriorations >= floor.
        let row: Vec<_> = p
            .packages
            .iter()
            .enumerate()
            .map(|(i, pk)| (x[i], pk.coefficients[s].usable_cash_deterioration))
            .collect();
        pb.add_row((p.floor - daily_min(&p.base[s])).., &row);
    }
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, pk) in p.packages.iter().enumerate() {
        groups.entry(pk.group.as_str()).or_default().push(i);
    }
    for members in groups.values() {
        let row: Vec<_> = members.iter().map(|&i| (x[i], 1.)).collect();
        pb.add_row(..=1., &row);
    }
    for cut in cuts {
        // Exactly this selection is excluded: sum_{in S} x - sum_{not in S} x <= |S| - 1.
        let row: Vec<_> = (0..p.packages.len())
            .map(|i| (x[i], if cut.contains(&i) { 1. } else { -1. }))
            .collect();
        pb.add_row(..=(cut.len() as f64 - 1.), &row);
    }
    let mut model = pb.optimise(Sense::Minimise);
    model.set_option("threads", 1);
    model.set_option("time_limit", 15.0);
    model.set_option("output_flag", false);
    let solved = model
        .try_solve()
        .map_err(|e| format!("HiGHS failed: {e:?}"))?;
    match solved.status() {
        HighsModelStatus::Optimal => {
            let sol = solved.get_solution();
            Ok(Some(
                (0..p.packages.len())
                    .filter(|&i| sol.columns()[i] > 0.5)
                    .collect(),
            ))
        }
        HighsModelStatus::Infeasible => Ok(None),
        other => Err(format!("HiGHS returned {other:?}")),
    }
}

fn daily_min(m: &Measures) -> f64 {
    m.daily_minimum
        .iter()
        .copied()
        .fold(m.minimum_usable_cash, f64::min)
}

fn estimate(p: &Problem, selection: &BTreeSet<usize>) -> BTreeMap<String, Value> {
    p.scenarios
        .iter()
        .map(|s| {
            let b = &p.base[s];
            let mut nii = b.external_nii;
            let (mut trading, mut funding, mut usable) = (b.trading_pnl, b.funding_cost, daily_min(b));
            for &i in selection {
                let c = &p.packages[i].coefficients[s];
                nii += c.external_nii;
                trading += c.trading_pnl;
                funding += c.funding_cost;
                usable += c.usable_cash_deterioration;
            }
            (s.clone(), json!({"external_nii": nii, "trading_pnl": trading, "funding_cost": funding, "minimum_usable_cash_bound": usable}))
        })
        .collect()
}

pub fn run(bytes: &[u8], mut output: Box<dyn Write + Send>) -> Result<Receipt, String> {
    portfolio_compute_control::checkpoint()?;
    let original = strict_json::parse(bytes, MAX_BYTES)?;
    let request: Request = serde_json::from_value(original.clone()).map_err(|e| e.to_string())?;
    if request.schema != SCHEMA {
        return Err("explicit liquidity-allocation-1 selection required".into());
    }
    let c = &request.constraints;
    if !c.minimum_usable_cash.is_finite() || c.max_refinements > MAX_REFINEMENTS {
        return Err(format!("MALFORMED_INPUT: a finite floor and at most {MAX_REFINEMENTS} refinements are required"));
    }
    if request.packages.is_empty() || request.packages.len() > MAX_PACKAGES {
        return Err(format!(
            "BOUND_EXCEEDED: an allocation names 1 to {MAX_PACKAGES} packages"
        ));
    }
    let ids: BTreeSet<&str> = request
        .packages
        .iter()
        .map(|p| p.package_id.as_str())
        .collect();
    if ids.len() != request.packages.len() {
        return Err("DUPLICATE_IDENTITY: package ids repeat".into());
    }
    let asof = request.base["plan"]["as_of_ordinal"]
        .as_i64()
        .ok_or("base as-of date")?;
    let end = request.base["acquisitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["horizon_end_ordinal"].as_i64())
        .max()
        .ok_or("base horizon")?;
    // Preparation: exact replays of the baseline and every package alone.
    let base_stream =
        replay(&request.base).map_err(|e| format!("the hold baseline does not run: {e}"))?;
    let base = measure(&base_stream, asof, end)?;
    let scenarios: Vec<String> = base.keys().cloned().collect();
    let mut packages = Vec::new();
    for p in &request.packages {
        let mut prepared = PreparedPackage {
            package_id: p.package_id.clone(),
            group: p.group.clone(),
            review: p.review.clone(),
            request_sha256: sha(&p.request),
            eligible: false,
            refusal: None,
            coefficients: BTreeMap::new(),
        };
        match replay(&p.request).and_then(|s| measure(&s, asof, end)) {
            Ok(m) if m.keys().eq(base.keys()) => {
                prepared.eligible = true;
                for s in &scenarios {
                    let (a, b) = (&m[s], &base[s]);
                    let deterioration = a
                        .daily_minimum
                        .iter()
                        .zip(&b.daily_minimum)
                        .map(|(x, y)| x - y)
                        .fold(f64::INFINITY, f64::min);
                    prepared.coefficients.insert(
                        s.clone(),
                        Coefficients {
                            external_nii: a.external_nii - b.external_nii,
                            trading_pnl: a.trading_pnl - b.trading_pnl,
                            funding_cost: a.funding_cost - b.funding_cost,
                            usable_cash_deterioration: if deterioration.is_finite() {
                                deterioration.min(0.)
                            } else {
                                0.
                            },
                        },
                    );
                }
            }
            Ok(_) => {
                prepared.refusal =
                    Some("the package declares different scenarios from the baseline".into())
            }
            // A refused package (for example FORMAL_OWNER_REQUIRED) is never selectable and never
            // approximated.
            Err(e) => prepared.refusal = Some(e),
        }
        if !prepared.eligible {
            for s in &scenarios {
                prepared.coefficients.insert(
                    s.clone(),
                    Coefficients {
                        external_nii: 0.,
                        trading_pnl: 0.,
                        funding_cost: 0.,
                        usable_cash_deterioration: 0.,
                    },
                );
            }
        }
        packages.push(prepared);
    }
    let problem = Problem {
        scenarios: &scenarios,
        base: &base,
        packages: &packages,
        objective: c.objective,
        floor: c.minimum_usable_cash,
    };
    let mut cuts: Vec<BTreeSet<usize>> = Vec::new();
    let mut attempts = Vec::new();
    let mut validated: Option<(BTreeSet<usize>, Value, Vec<u8>)> = None;
    let mut infeasible = false;
    for _ in 0..=c.max_refinements {
        let Some(selection) = solve(&problem, &cuts)? else {
            infeasible = true;
            break;
        };
        let names: Vec<String> = selection
            .iter()
            .map(|&i| packages[i].package_id.clone())
            .collect();
        let mut attempt = Attempt {
            selection: names,
            estimate: estimate(&problem, &selection),
            exact: None,
            violation: None,
        };
        let chosen: Vec<&Value> = selection
            .iter()
            .map(|&i| &request.packages[i].request)
            .collect();
        match merge(&request.base, &chosen)
            .and_then(|combined| replay(&combined).map(|s| (combined, s)))
        {
            Err(e) => {
                attempt.violation = Some(Violation {
                    scenario_id: None,
                    ordinal: None,
                    event_id: None,
                    measure: "exact_replay".into(),
                    value: None,
                    limit: None,
                    detail: e,
                });
            }
            Ok((combined, stream)) => {
                let exact = measure(&stream, asof, end)?;
                let breach = exact.iter().find(|(_, m)| {
                    m.minimum_usable_cash
                        < c.minimum_usable_cash - 1e-9 * c.minimum_usable_cash.abs().max(1.)
                });
                match breach {
                    Some((s, m)) => {
                        attempt.violation = Some(Violation {
                            scenario_id: Some(s.clone()),
                            ordinal: m.worst.as_ref().map(|w| w.0),
                            event_id: m.worst.as_ref().map(|w| w.1.clone()),
                            measure: "usable_cash".into(),
                            value: Some(m.minimum_usable_cash),
                            limit: Some(c.minimum_usable_cash),
                            detail: format!(
                                "usable cash falls to {} below the floor {}",
                                m.minimum_usable_cash, c.minimum_usable_cash
                            ),
                        });
                        attempt.exact = Some(exact);
                    }
                    None => {
                        attempt.exact = Some(exact);
                        attempts.push(attempt);
                        validated = Some((selection, combined, stream));
                        break;
                    }
                }
            }
        }
        attempts.push(attempt);
        cuts.push(selection);
    }
    let (outcome, violation) = if validated.is_some() {
        ("validated", None)
    } else if infeasible && attempts.is_empty() {
        // No selection meets the conservative bound: report the binding scenario's baseline.
        let (s, m) = base
            .iter()
            .min_by(|a, b| daily_min(a.1).total_cmp(&daily_min(b.1)))
            .ok_or("no scenarios")?;
        let best: f64 = packages
            .iter()
            .filter(|p| p.eligible)
            .map(|p| p.coefficients[s].usable_cash_deterioration.max(0.))
            .sum();
        (
            "estimate_infeasible",
            Some(Violation {
                scenario_id: Some(s.clone()),
                ordinal: m.worst.as_ref().map(|w| w.0),
                event_id: m.worst.as_ref().map(|w| w.1.clone()),
                measure: "usable_cash_bound".into(),
                value: Some(daily_min(m) + best),
                limit: Some(c.minimum_usable_cash),
                detail: format!(
                    "no package selection keeps the conservative usable-cash bound at the floor {}; the baseline minimum is {}",
                    c.minimum_usable_cash,
                    daily_min(m)
                ),
            }),
        )
    } else {
        (
            "exhausted",
            attempts.last().and_then(|a| a.violation.clone()),
        )
    };
    let receipt = Receipt {
        schema: SCHEMA,
        identity_sha256: sha(&json!({"base": request.base, "packages": request.packages.iter().map(|p| (&p.package_id, &p.group, &p.request)).collect::<Vec<_>>()})),
        objective: c.objective,
        minimum_usable_cash: c.minimum_usable_cash,
        scenarios: scenarios.clone(),
        base_daily_minimum: base.iter().map(|(s, m)| (s.clone(), daily_min(m))).collect(),
        base: base.clone(),
        packages: packages.clone(),
        attempts,
        outcome,
        violation,
        selected: validated.as_ref().map(|(sel, _, _)| sel.iter().map(|&i| packages[i].package_id.clone()).collect()),
        exact_replay_passed: validated.is_some(),
        combined_request: validated.as_ref().map(|(_, r, _)| r.clone()),
        disclosure: "Prepared allocation over reviewed discrete packages. Coefficients come from exact native replays of each package alone against the hold baseline; selection estimates are linear and conservative for usable cash only within that estimate, because netting, margin, funding policy and inventory are not additive. A plan is validated only by an exact replay of the merged packages that keeps every scenario at the usable-cash floor, then by independent verification of that stream. Failed candidates are excluded by exact selection cuts; no linearized margin cut is made. Formal hedge qualification is never estimated: a refused package is not selectable.",
    };
    serde_json::to_writer(&mut output, &json!({"allocation": &receipt}))
        .map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    if let Some((_, _, stream)) = &validated {
        output.write_all(stream).map_err(|e| e.to_string())?;
    }
    output.flush().map_err(|e| e.to_string())?;
    Ok(receipt)
}
