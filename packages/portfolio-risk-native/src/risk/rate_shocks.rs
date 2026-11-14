//! Standard interest-rate shock set, EVE/NII deltas and policy-limit status (`rate-shock-1`).
//!
//! The scenario set and every convention are explicit policy inputs. Instantaneous shocks move
//! the par swap pillars and are revalued by the caller at fixed base OAS on shared draws. Ramps
//! are income-only path shifts and never produce an EVE.
use crate::market::lifecycle::finite;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const SCHEMA: &str = "rate-shock-1";
const METRICS: [&str; 3] = ["eve", "nii_12m", "nii_24m"];

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    /// Instantaneous parallel magnitudes in bp; each produces an up and a down scenario.
    pub parallel_bp: Vec<f64>,
    /// Ramp terminal magnitudes in bp; each produces an up and a down income-only scenario.
    pub ramp_bp: Vec<f64>,
    pub ramp_months: usize,
    /// Short and long components of the non-parallel shocks, in bp.
    pub short_bp: f64,
    pub long_bp: f64,
    /// Decay of the short shock along the curve, in years.
    pub decay_years: f64,
    /// Weights on the short and long components; signs are part of the definition.
    pub steepener: [f64; 2],
    pub flattener: [f64; 2],
    /// Lowest post-shock pillar rate as a decimal; null applies no floor.
    pub floor: Option<f64>,
    #[serde(default)]
    pub limits: Vec<Limit>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub metric: String,
    pub scenario: String,
    /// `internal_limit` or `not_applicable`; a missing entry is `unconfigured`.
    pub applicability: String,
    /// Largest permitted decline, percent of the base metric. Required and positive for a limit.
    pub max_decline_pct: Option<f64>,
}

fn label(value: f64) -> String {
    if value.fract() == 0. {
        format!("{}", value as i64)
    } else {
        format!("{value}").replace('.', "p")
    }
}

impl Policy {
    fn check(&self) -> Result<(), String> {
        if self.schema != SCHEMA {
            return Err(format!("rate shock policy must select {SCHEMA}"));
        }
        for values in [&self.parallel_bp, &self.ramp_bp] {
            finite(values)?;
            if values.len() > 8
                || values.iter().any(|v| !(*v > 0. && *v <= 2000.))
                || values.windows(2).any(|w| w[0] >= w[1])
            {
                return Err(
                    "shock magnitudes must be increasing positive bp, at most 2000, at most eight"
                        .into(),
                );
            }
        }
        finite(&[self.short_bp, self.long_bp, self.decay_years])?;
        finite(&self.steepener)?;
        finite(&self.flattener)?;
        let positive = |v: f64, max: f64| v > 0. && v <= max;
        if !(positive(self.short_bp, 2000.)
            && positive(self.long_bp, 2000.)
            && positive(self.decay_years, 100.))
            || self
                .steepener
                .iter()
                .chain(&self.flattener)
                .any(|w| w.abs() > 4.)
        {
            return Err(
                "short/long shocks, decay or shape weights are outside their supported domain"
                    .into(),
            );
        }
        if !(1..=120).contains(&self.ramp_months) {
            return Err("ramp_months must be within 1..120".into());
        }
        if let Some(floor) = self.floor {
            if !floor.is_finite() || floor.abs() > 0.2 {
                return Err("rate floor must be a finite decimal within +/-20%".into());
            }
        }
        Ok(())
    }

    /// (name, family, direction, shift in bp by pillar) for every instantaneous scenario.
    fn instantaneous(&self, tenors: &[f64]) -> Vec<(String, &'static str, Vec<f64>)> {
        let mut out = Vec::new();
        for bp in &self.parallel_bp {
            for (side, sign) in [("up", 1.), ("down", -1.)] {
                out.push((
                    format!("parallel_{side}_{}", label(*bp)),
                    "parallel",
                    vec![sign * bp; tenors.len()],
                ));
            }
        }
        let short = |t: f64| self.short_bp * (-t / self.decay_years).exp();
        let long = |t: f64| self.long_bp * (1. - (-t / self.decay_years).exp());
        out.push((
            "short_up".into(),
            "short",
            tenors.iter().map(|t| short(*t)).collect(),
        ));
        out.push((
            "short_down".into(),
            "short",
            tenors.iter().map(|t| -short(*t)).collect(),
        ));
        out.push((
            "steepener".into(),
            "steepener",
            tenors
                .iter()
                .map(|t| self.steepener[0] * short(*t) + self.steepener[1] * long(*t))
                .collect(),
        ));
        out.push((
            "flattener".into(),
            "flattener",
            tenors
                .iter()
                .map(|t| self.flattener[0] * short(*t) + self.flattener[1] * long(*t))
                .collect(),
        ));
        out
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .instantaneous(&[1.])
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        for bp in &self.ramp_bp {
            for side in ["up", "down"] {
                names.push(format!("ramp_{side}_{}", label(*bp)));
            }
        }
        names
    }

    fn check_limits(&self) -> Result<(), String> {
        let names: BTreeSet<_> = self.names().into_iter().collect();
        let mut seen = BTreeSet::new();
        if self.limits.len() > 256 {
            return Err("at most 256 rate shock limits".into());
        }
        for limit in &self.limits {
            if !METRICS.contains(&limit.metric.as_str()) {
                return Err(format!("unknown rate shock metric {}", limit.metric));
            }
            if !names.contains(&limit.scenario) {
                return Err(format!(
                    "limit refers to scenario {} outside the policy set",
                    limit.scenario
                ));
            }
            if !seen.insert((limit.metric.clone(), limit.scenario.clone())) {
                return Err(format!(
                    "duplicate limit for {} under {}",
                    limit.metric, limit.scenario
                ));
            }
            match (limit.applicability.as_str(), limit.max_decline_pct) {
                ("internal_limit", Some(v)) if v.is_finite() && v > 0. => {}
                ("not_applicable", None) => {}
                ("internal_limit", _) => {
                    return Err("an internal limit requires a positive max_decline_pct".into())
                }
                ("not_applicable", _) => {
                    return Err("a not_applicable entry must not carry a limit".into())
                }
                _ => return Err("applicability must be internal_limit or not_applicable".into()),
            }
        }
        Ok(())
    }
}

/// Floor a shocked pillar without ever moving a rate that already sits below the floor upward
/// on a down shock: the floor binds at `min(base, floor)`.
fn floored(base: f64, shift: f64, floor: Option<f64>) -> f64 {
    let shocked = base + shift;
    match floor {
        Some(f) if shift < 0. => shocked.max(f.min(base)),
        _ => shocked,
    }
}

pub fn scenarios(policy: &Policy, tenors: &[f64], swap_rates: &[f64]) -> Result<Value, String> {
    policy.check()?;
    policy.check_limits()?;
    finite(tenors)?;
    finite(swap_rates)?;
    if tenors.is_empty()
        || tenors.len() != swap_rates.len()
        || tenors.len() > 64
        || tenors[0] <= 0.
        || tenors.windows(2).any(|w| w[0] >= w[1])
    {
        return Err("rate shocks require aligned increasing positive pillar tenors".into());
    }
    let mut out = Vec::new();
    for (name, family, shift_bp) in policy.instantaneous(tenors) {
        let rates: Vec<_> = swap_rates
            .iter()
            .zip(&shift_bp)
            .map(|(r, s)| floored(*r, s * 1e-4, policy.floor))
            .collect();
        let applied: Vec<_> = rates
            .iter()
            .zip(swap_rates)
            .map(|(a, b)| (a - b) * 1e4)
            .collect();
        let hit = rates
            .iter()
            .zip(swap_rates.iter().zip(&shift_bp))
            .any(|(a, (b, s))| *a != b + s * 1e-4);
        finite(&rates)?;
        out.push(
            json!({"name":name,"family":family,"timing":"instantaneous","swap_rates":rates,
            "requested_shift_bp":shift_bp,"applied_shift_bp":applied,"floor_applied":hit}),
        );
    }
    // A ramp is one parallel number applied to simulated paths; it is floored at the shortest
    // pillar so its terminal level matches the instantaneous convention there.
    for bp in &policy.ramp_bp {
        for (side, sign) in [("up", 1.), ("down", -1.)] {
            let requested = sign * bp * 1e-4;
            let level = floored(swap_rates[0], requested, policy.floor);
            let bound = level != swap_rates[0] + requested;
            // Unbound ramps carry the requested shift exactly, not a rounded difference.
            let shift = if bound {
                level - swap_rates[0]
            } else {
                requested
            };
            out.push(
                json!({"name":format!("ramp_{side}_{}",label(*bp)),"family":"ramp","timing":"ramp",
                "ramp":{"shift":shift,"months":policy.ramp_months},"requested_shift_bp":sign*bp,
                "applied_shift_bp":shift*1e4,"floor_applied":bound}),
            );
        }
    }
    Ok(json!({"schema":SCHEMA,"scenarios":out,"conventions":conventions(policy),"tenors":tenors}))
}

fn conventions(policy: &Policy) -> Value {
    json!({
        "curve":"shocks move the par swap pillars; every other market input, including volatility, is unchanged",
        "instantaneous":"applied at the valuation date to the whole curve; the book is unchanged",
        "ramp":format!("parallel shift reaching its full size in month {} and held; income only, no EVE",policy.ramp_months),
        "non_parallel":format!("short = {} bp x exp(-t/{}); long = {} bp x (1 - exp(-t/{})); steepener = {} x short + {} x long; flattener = {} x short + {} x long",
            policy.short_bp,policy.decay_years,policy.long_bp,policy.decay_years,policy.steepener[0],policy.steepener[1],policy.flattener[0],policy.flattener[1]),
        "floor":match policy.floor{Some(f)=>format!("down shocks stop at {:.4}% or the base rate if lower",f*100.),None=>"no floor; the model rate domain still applies".into()},
        "valuation":"base OAS is solved once on the base market and held; all scenarios share the base random draws",
        "eve":"market value of assets less liabilities plus the hedge book mark-to-market at each scenario market; non-maturity deposits use modelled behavioural runoff at their fixed base OAS; equity is excluded",
        "nii":"sum of the first 12 and 24 projected months on the unchanged book",
        "limits":"a limit is the largest permitted decline as a percent of the base metric; no configured limit is never a pass",
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub name: String,
    /// Absent for income-only scenarios.
    pub eve: Option<f64>,
    pub nii: Vec<f64>,
    /// Why the model could not evaluate this scenario; then no value is supplied or invented.
    #[serde(default)]
    pub unavailable: Option<String>,
}

fn window(nii: &[f64], months: usize) -> Option<f64> {
    (nii.len() >= months).then(|| nii[..months].iter().sum())
}

/// (delta, percent of base); the percent is undefined unless the base is positive.
fn change(base: Option<f64>, value: Option<f64>) -> (Option<f64>, Option<f64>) {
    match (base, value) {
        (Some(b), Some(v)) => (Some(v - b), (b > 0.).then(|| (v - b) / b * 100.)),
        _ => (None, None),
    }
}

/// A breach outranks unavailable inputs, which outrank a pass; no limit is never a pass.
fn status(limit: Option<&Limit>, pct: Option<f64>) -> (&'static str, Option<f64>) {
    let Some(limit) = limit else {
        return ("unconfigured", None);
    };
    if limit.applicability != "internal_limit" {
        return ("not_applicable", None);
    }
    let max = limit.max_decline_pct.unwrap_or(f64::NAN);
    match pct {
        None => ("unavailable", None),
        Some(p) if -p > max => ("breach", Some(max + p)),
        Some(p) => ("within_limit", Some(max + p)),
    }
}

pub fn report(policy: &Policy, base: &Outcome, outcomes: &[Outcome]) -> Result<Value, String> {
    policy.check()?;
    policy.check_limits()?;
    let names = policy.names();
    if outcomes.len() != names.len() || outcomes.iter().zip(&names).any(|(o, n)| &o.name != n) {
        return Err("rate shock outcomes must cover the policy set in order".into());
    }
    finite(&base.nii)?;
    let base_eve = base.eve.ok_or("base EVE is required")?;
    finite(&[base_eve])?;
    let base_values = [Some(base_eve), window(&base.nii, 12), window(&base.nii, 24)];
    let mut rows = Vec::new();
    let mut counts = std::collections::BTreeMap::new();
    for outcome in outcomes {
        finite(&outcome.nii)?;
        let ramp = outcome.name.starts_with("ramp_");
        let values = if let Some(reason) = &outcome.unavailable {
            if outcome.eve.is_some() || !outcome.nii.is_empty() || reason.is_empty() {
                return Err("an unevaluated scenario carries a reason and no values".into());
            }
            [None; 3]
        } else {
            if outcome.nii.len() != base.nii.len() {
                return Err("scenario and base NII must share one monthly grid".into());
            }
            if ramp == outcome.eve.is_some() {
                return Err(
                    "EVE is required for instantaneous shocks and refused for ramps".into(),
                );
            }
            if let Some(eve) = outcome.eve {
                finite(&[eve])?;
            }
            [
                outcome.eve,
                window(&outcome.nii, 12),
                window(&outcome.nii, 24),
            ]
        };
        for (i, metric) in METRICS.iter().enumerate() {
            let (delta, pct) = change(base_values[i], values[i]);
            let limit = policy
                .limits
                .iter()
                .find(|l| l.metric == *metric && l.scenario == outcome.name);
            let (state, headroom) = status(limit, pct);
            *counts.entry(state).or_insert(0usize) += 1;
            let reason = if let Some(reason) = &outcome.unavailable {
                Some(reason.as_str())
            } else if values[i].is_none() {
                Some(if i == 0 {
                    "ramps are income only"
                } else {
                    "projection horizon is shorter than the metric window"
                })
            } else if pct.is_none() {
                Some("base metric is not positive")
            } else {
                None
            };
            rows.push(json!({"scenario":outcome.name,"metric":metric,"base":base_values[i],"value":values[i],
                "delta":delta,"delta_pct":pct,"applicability":limit.map_or("unconfigured",|l|l.applicability.as_str()),
                "max_decline_pct":limit.and_then(|l|l.max_decline_pct),"headroom_pct":headroom,"status":state,
                "unavailable_reason":reason}));
        }
    }
    let breaches = counts.get("breach").copied().unwrap_or(0);
    let unavailable = counts.get("unavailable").copied().unwrap_or(0);
    let within = counts.get("within_limit").copied().unwrap_or(0);
    let overall = if breaches > 0 {
        "breach"
    } else if unavailable > 0 {
        "unavailable"
    } else if within > 0 {
        "within_limit"
    } else {
        "unconfigured"
    };
    Ok(json!({"schema":SCHEMA,"rows":rows,"status":overall,"counts":counts}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        serde_json::from_value(json!({"schema":SCHEMA,"parallel_bp":[100.,200.],"ramp_bp":[200.],"ramp_months":12,
            "short_bp":300.,"long_bp":150.,"decay_years":4.,"steepener":[-0.65,0.9],"flattener":[0.8,-0.6],
            "floor":0.0,"limits":[
                {"metric":"eve","scenario":"parallel_up_200","applicability":"internal_limit","max_decline_pct":10.},
                {"metric":"nii_12m","scenario":"parallel_down_200","applicability":"internal_limit","max_decline_pct":5.},
                {"metric":"nii_24m","scenario":"ramp_up_200","applicability":"not_applicable","max_decline_pct":null}]}))
        .unwrap()
    }

    #[test]
    fn hand_computed_shapes_and_floor() {
        let tenors = [1., 10.];
        let out = scenarios(&policy(), &tenors, &[0.015, 0.04]).unwrap();
        let find = |name: &str| {
            out["scenarios"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == name)
                .unwrap()
                .clone()
        };
        // -200 bp from 1.5% stops at the 0% floor; the 4% pillar moves the full amount.
        let down = find("parallel_down_200");
        assert_eq!(down["swap_rates"][0], 0.0);
        assert!((down["swap_rates"][1].as_f64().unwrap() - 0.02).abs() < 1e-15);
        assert_eq!(down["floor_applied"], true);
        // steepener at 1y: -0.65*300*e^-0.25 + 0.9*150*(1-e^-0.25)
        let e: f64 = (-0.25f64).exp();
        let expected = -0.65 * 300. * e + 0.9 * 150. * (1. - e);
        let steep = find("steepener");
        assert!((steep["requested_shift_bp"][0].as_f64().unwrap() - expected).abs() < 1e-12);
        // flattener at 10y: 0.8*300*e^-2.5 - 0.6*150*(1-e^-2.5)
        let e: f64 = (-2.5f64).exp();
        let expected = 0.8 * 300. * e - 0.6 * 150. * (1. - e);
        assert!(
            (find("flattener")["requested_shift_bp"][1].as_f64().unwrap() - expected).abs() < 1e-12
        );
        // The down ramp is floored at the shortest pillar: -150 bp, not -200.
        let ramp = find("ramp_down_200");
        assert!((ramp["ramp"]["shift"].as_f64().unwrap() + 0.015).abs() < 1e-15);
        assert_eq!(find("ramp_up_200")["floor_applied"], false);
        assert_eq!(find("ramp_up_200")["ramp"]["shift"], 0.02);
        assert_eq!(out["scenarios"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn a_rate_below_the_floor_is_not_raised() {
        assert_eq!(floored(-0.005, -0.01, Some(0.)), -0.005);
        assert_eq!(floored(0.03, 0.01, Some(0.05)), 0.04);
        assert!((floored(0.03, -0.01, None) - 0.02).abs() < 1e-15);
        assert_eq!(floored(0.03, -0.04, Some(0.)), 0.);
    }

    #[test]
    fn status_never_passes_without_a_limit_or_a_metric() {
        let p = policy();
        let nii = |v: f64| vec![v; 18];
        let base = Outcome {
            name: "base".into(),
            eve: Some(1000.),
            nii: nii(10.),
            unavailable: None,
        };
        let outcomes: Vec<_> = p
            .names()
            .into_iter()
            .map(|name| {
                let (eve, n) = match name.as_str() {
                    "parallel_up_200" => (Some(880.), 10.5),
                    "parallel_down_200" => (Some(1100.), 9.6),
                    n if n.starts_with("ramp_") => (None, 10.2),
                    _ => (Some(1000.), 10.),
                };
                Outcome {
                    name,
                    eve,
                    nii: nii(n),
                    unavailable: None,
                }
            })
            .collect();
        let out = report(&p, &base, &outcomes).unwrap();
        let row = |s: &str, m: &str| {
            out["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["scenario"] == s && r["metric"] == m)
                .unwrap()
                .clone()
        };
        // EVE falls 12% against a 10% limit.
        let r = row("parallel_up_200", "eve");
        assert_eq!(r["status"], "breach");
        assert!((r["delta_pct"].as_f64().unwrap() + 12.).abs() < 1e-12);
        assert!((r["headroom_pct"].as_f64().unwrap() + 2.).abs() < 1e-12);
        // NII falls 4% against a 5% limit.
        let r = row("parallel_down_200", "nii_12m");
        assert_eq!(r["status"], "within_limit");
        assert!((r["delta"].as_f64().unwrap() + 4.8).abs() < 1e-12);
        // An explicit not_applicable entry and a missing entry are both never a pass.
        assert_eq!(row("ramp_up_200", "nii_24m")["status"], "not_applicable");
        assert_eq!(row("parallel_up_100", "nii_24m")["status"], "unconfigured");
        assert!(row("parallel_up_100", "nii_24m")["value"].is_null());
        assert!(row("ramp_up_200", "eve")["value"].is_null());
        assert_eq!(row("parallel_up_100", "eve")["status"], "unconfigured");
        assert_eq!(out["status"], "breach");
    }

    #[test]
    fn a_limit_on_an_undefined_metric_is_unavailable() {
        let mut p = policy();
        p.limits = vec![Limit {
            metric: "nii_24m".into(),
            scenario: "parallel_up_100".into(),
            applicability: "internal_limit".into(),
            max_decline_pct: Some(5.),
        }];
        let base = Outcome {
            name: "base".into(),
            eve: Some(-5.),
            nii: vec![1.; 12],
            unavailable: None,
        };
        let outcomes: Vec<_> = p
            .names()
            .into_iter()
            .map(|name| Outcome {
                eve: (!name.starts_with("ramp_")).then_some(-6.),
                name,
                nii: vec![1.; 12],
                unavailable: None,
            })
            .collect();
        let out = report(&p, &base, &outcomes).unwrap();
        assert_eq!(out["status"], "unavailable");
        // A negative base EVE has a delta but no percentage.
        let eve = &out["rows"][0];
        assert_eq!(eve["delta"], -1.0);
        assert!(eve["delta_pct"].is_null());
    }

    #[test]
    fn a_scenario_the_model_cannot_value_is_unavailable_not_a_pass() {
        let p = policy();
        let base = Outcome {
            name: "base".into(),
            eve: Some(1000.),
            nii: vec![10.; 24],
            unavailable: None,
        };
        let mut outcomes: Vec<_> = p
            .names()
            .into_iter()
            .map(|name| Outcome {
                eve: (!name.starts_with("ramp_")).then_some(1000.),
                name,
                nii: vec![10.; 24],
                unavailable: None,
            })
            .collect();
        // parallel_down_200 carries a 5% NII limit in the fixture policy.
        let failed = outcomes
            .iter_mut()
            .find(|o| o.name == "parallel_down_200")
            .unwrap();
        failed.eve = None;
        failed.nii = vec![];
        failed.unavailable = Some("zero swap rate in volatility calibration".into());
        let out = report(&p, &base, &outcomes).unwrap();
        let rows: Vec<_> = out["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["scenario"] == "parallel_down_200")
            .collect();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r["value"].is_null()
            && r["unavailable_reason"] == "zero swap rate in volatility calibration"));
        assert_eq!(rows[1]["status"], "unavailable"); // limited metric
        assert_eq!(rows[0]["status"], "unconfigured"); // no limit: still not a pass
        assert_eq!(out["status"], "unavailable");
        // A reason with values, or values without the grid, is refused.
        outcomes[0].unavailable = Some("x".into());
        assert!(report(&p, &base, &outcomes).is_err());
    }

    #[test]
    fn refuses_malformed_policy_and_coverage() {
        let mut p = policy();
        p.limits[0].scenario = "parallel_up_300".into();
        assert!(scenarios(&p, &[1.], &[0.03]).is_err());
        let mut p = policy();
        p.limits[0].max_decline_pct = None;
        assert!(scenarios(&p, &[1.], &[0.03]).is_err());
        let mut p = policy();
        p.limits.push(p.limits[0].clone());
        assert!(scenarios(&p, &[1.], &[0.03]).is_err());
        let mut p = policy();
        p.parallel_bp = vec![200., 100.];
        assert!(scenarios(&p, &[1.], &[0.03]).is_err());
        let p = policy();
        let base = Outcome {
            name: "base".into(),
            eve: Some(1.),
            nii: vec![1.; 24],
            unavailable: None,
        };
        assert!(report(&p, &base, &[]).is_err());
    }
}
