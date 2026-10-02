//! Published monthly driver preparation, separate from risk-neutral valuation.
use crate::conventions::Date;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
const DRIVERS: [&str; 6] = [
    "short_rate",
    "policy_rate",
    "rate_5y",
    "rate_10y",
    "mortgage_rate",
    "hpi",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub rows: Vec<Value>,
    pub scenario: String,
    pub start_period: String,
    pub horizon: usize,
    pub months: usize,
}
fn month(value: &str) -> Result<(i32, i32), String> {
    let parts: Vec<_> = value.split('-').collect();
    if value.len() != 10 || parts.len() != 3 {
        return Err("date must use YYYY-MM-DD".into());
    }
    let numbers: Result<Vec<i32>, _> = parts.iter().map(|v| v.parse()).collect();
    let v = numbers.map_err(|_| "invalid forecast date")?;
    Date::new(v[0], v[1], v[2])?;
    Ok((v[0] * 12 + v[1] - 1, v[2]))
}
fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row[key]
        .as_str()
        .ok_or_else(|| format!("missing forecast {key}"))
}
fn interpolate(points: &[(i32, f64)], x: i32) -> f64 {
    let j = points.partition_point(|p| p.0 < x);
    if j == 0 {
        points[0].1
    } else if j == points.len() {
        points[j - 1].1
    } else {
        let (a, b) = (points[j - 1], points[j]);
        a.1 + (b.1 - a.1) * (x - a.0) as f64 / (b.0 - a.0) as f64
    }
}
impl Request {
    pub fn run(self) -> Result<Value, String> {
        let (start, day) = month(&self.start_period)?;
        if !(1..=120).contains(&self.horizon)
            || day != 1
            || self.months < self.horizon
            || self.months > 4096
            || self.rows.len() > 100_000
        {
            return Err("invalid forecast horizon, month-start, or row budget".into());
        }
        if !["baseline", "adverse", "median"].contains(&self.scenario.as_str())
            || !self.rows.iter().any(|r| r["scenario"] == self.scenario)
        {
            return Err("select a published baseline, adverse or median scenario".into());
        }
        let selected: Vec<_> = self
            .rows
            .iter()
            .filter(|r| r["scenario"] == self.scenario && r["convention"] != "longer_run")
            .collect();
        let mut valid_start = false;
        for r in &selected {
            if ["short_rate", "policy_rate"].contains(&r["variable"].as_str().unwrap_or(""))
                && month(text(r, "date")?)?.0 == start
            {
                valid_start = true;
            }
        }
        if !valid_start {
            return Err("start period must be a published rate-anchor month".into());
        }
        let mut targets = BTreeMap::new();
        let mut coverage = BTreeMap::new();
        for variable in DRIVERS {
            let mut values: Vec<_> = selected
                .iter()
                .copied()
                .filter(|r| r["variable"] == variable)
                .collect();
            if values.is_empty() {
                continue;
            }
            let conventions: Result<BTreeSet<_>, _> =
                values.iter().map(|r| text(r, "convention")).collect();
            let conventions = conventions?;
            if conventions.len() != 1 {
                return Err(format!("mixed period conventions for {variable}"));
            }
            let convention = *conventions.first().unwrap();
            if !["quarter_average", "quarter_end", "year_end", "date_end"].contains(&convention) {
                return Err("unsupported driver convention".into());
            }
            if variable == "hpi" {
                values.extend(
                    self.rows
                        .iter()
                        .filter(|r| r["scenario"] == "history" && r["variable"] == "hpi"),
                );
            }
            let mut anchors = BTreeMap::new();
            for r in values {
                let unit = text(r, "unit")?;
                if if variable == "hpi" {
                    unit != "index"
                } else {
                    !["percent", "decimal_rate"].contains(&unit)
                } {
                    return Err(format!("unexpected unit for {variable}"));
                }
                let v = r["value"]
                    .as_f64()
                    .ok_or("forecast value must be numeric")?
                    / if unit == "percent" { 100. } else { 1. };
                if !v.is_finite()
                    || if variable == "hpi" {
                        v <= 0.
                    } else {
                        v <= -0.019 || v > 1.
                    }
                {
                    return Err(format!("invalid forecast value for {variable}"));
                }
                let x = month(text(r, "date")?)?.0 - start + 1;
                let positions = if convention == "quarter_average" {
                    (x..x + 3).collect::<Vec<_>>()
                } else {
                    vec![if convention == "quarter_end" {
                        x + 2
                    } else {
                        x
                    }]
                };
                for pos in positions {
                    if anchors.insert(pos, v).is_some_and(|old| old != v) {
                        return Err(format!("conflicting forecast anchors for {variable}"));
                    }
                }
            }
            let first = *anchors.first_key_value().unwrap().0;
            let last = *anchors.last_key_value().unwrap().0;
            if last < 1 {
                return Err(format!("{variable} has no forward coverage"));
            }
            if variable == "hpi" && first > 0 {
                return Err("HPI requires a preceding quarter/history anchor".into());
            }
            let points: Vec<_> = anchors
                .into_iter()
                .map(|(x, y)| (x, if variable == "hpi" { y.ln() } else { y }))
                .collect();
            let origin = interpolate(&points, 0);
            let target: Vec<_> = (1..=self.months)
                .map(|m| {
                    let v = interpolate(&points, m as i32);
                    if variable == "hpi" {
                        (v - origin).exp()
                    } else {
                        v
                    }
                })
                .collect();
            crate::lifecycle_market::finite(&target)?;
            targets.insert(variable, target);
            coverage.insert(variable,json!({"first_month":first,"last_month":last,"tail_months_in_report":(self.horizon as i32-last).max(0)}));
        }
        if !targets.contains_key("short_rate") && !targets.contains_key("policy_rate") {
            return Err("forecast has no supported short-rate driver".into());
        }
        let mut unused = BTreeSet::new();
        for row in selected {
            let name = row
                .get("variable")
                .unwrap_or(&row["series"])
                .as_str()
                .ok_or("missing forecast variable/series")?;
            if !DRIVERS.contains(&name) {
                unused.insert(name);
            }
        }
        Ok(json!({"targets":targets,"coverage":coverage,"unused_variables":unused}))
    }
}
