//! Temporary assumption domains and derived defaults. Saved rows are immutable.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
type Rows = BTreeMap<String, Vec<BTreeMap<String, Value>>>;
type Changes = BTreeMap<String, BTreeMap<String, BTreeMap<String, Value>>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub books: Rows,
    pub overrides: Changes,
    pub hpi_mu: f64,
}
pub fn domain(book: &str, field: &str) -> Option<(f64, f64)> {
    Some(match (book, field) {
        ("mbs", "wac") => (0.0001, 0.5),
        ("mbs", "net_coupon") => (0., 0.5),
        ("mbs", "wam") => (1., 359.),
        ("mbs", "age") => (0., 600.),
        ("mbs", "oltv") => (0.01, 2.),
        ("mbs", "factor") => (0.001, 1.),
        ("mbs", "fico") => (300., 850.),
        ("mbs", "avg_loan_size") => (1., 1e8),
        ("mbs", "hpi_orig_ratio") => (0.01, 100.),
        ("mbs", "prepay_mult") => (0., 10.),
        ("loans" | "debt", "coupon_or_spread") => (-0.1, 0.5),
        ("loans" | "debt", "cap" | "floor") => (-0.1, 1.),
        ("loans" | "debt" | "cds", "call_threshold") => (0., 0.5),
        ("cds", "rate") => (0., 0.5),
        ("cds", "penalty_months") => (0., 120.),
        ("cds", "ew_mult") => (0., 10.),
        ("deposits", "rate_paid") => (0., 0.5),
        ("deposits", "age_months") => (0., 600.),
        ("deposits", "avg_account_size") => (1., 1e10),
        ("deposits", "svc_cost") => (0., 0.1),
        ("deposits", "attrition_base" | "attrition_amp" | "attrition_gap") => (0., 1.),
        ("deposits", "attrition_slope") => (0., 1000.),
        _ => return None,
    })
}
fn default(field: &str) -> Option<f64> {
    match field {
        "cap" => Some(10.),
        "floor" => Some(-10.),
        "call_threshold" => Some(0.005),
        "ew_mult" | "prepay_mult" => Some(1.),
        "svc_cost" => Some(0.),
        _ => None,
    }
}
impl Request {
    pub fn run(self) -> Result<Value, String> {
        if !self.hpi_mu.is_finite() || self.hpi_mu <= -1. {
            return Err("invalid HPI default".into());
        }
        let mut output = BTreeMap::new();
        for (book, changes) in &self.overrides {
            let rows = self
                .books
                .get(book)
                .ok_or("assumption overrides must refer to selected books")?;
            let id_field = if book == "mbs" { "cusip" } else { "id" };
            let ids: Vec<_> = rows
                .iter()
                .map(|r| {
                    r.get(id_field)
                        .and_then(Value::as_str)
                        .ok_or("invalid instrument ID")
                })
                .collect::<Result<_, _>>()?;
            let index: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            if index.len() != rows.len() {
                return Err("duplicate instrument ID".into());
            }
            let mut fields = BTreeSet::new();
            for (id, patch) in changes {
                if !index.contains_key(id.as_str()) {
                    return Err(format!(
                        "{book}: unknown instrument in assumption overrides"
                    ));
                }
                for (field, value) in patch {
                    let (lo, hi) = domain(book, field)
                        .ok_or_else(|| format!("{book}: unsupported assumption {field}"))?;
                    let number = value.as_f64().ok_or_else(|| {
                        format!("{book}/{id}: {field} must be finite in [{lo}, {hi}]")
                    })?;
                    if !number.is_finite() || number < lo || number > hi {
                        return Err(format!(
                            "{book}/{id}: {field} must be finite in [{lo}, {hi}]"
                        ));
                    }
                    if field == "wam" && number.fract() != 0. {
                        return Err("wam must be an integer number of months".into());
                    }
                    fields.insert(field.clone());
                }
            }
            let mut columns: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();
            for field in fields
                .iter()
                .filter(|f| f.as_str() != "hpi_orig_ratio")
                .chain(fields.iter().filter(|f| f.as_str() == "hpi_orig_ratio"))
            {
                let values = rows
                    .iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let value = changes
                            .get(ids[i])
                            .and_then(|p| p.get(field))
                            .or_else(|| row.get(field))
                            .and_then(Value::as_f64)
                            .or_else(|| default(field));
                        if field == "hpi_orig_ratio" && value.is_none() {
                            let age = columns
                                .get("age")
                                .and_then(|v| v[i])
                                .or_else(|| row.get("age").and_then(Value::as_f64))
                                .ok_or("missing mortgage age")?;
                            Ok(Some((1. + self.hpi_mu).powf(age / 12.)))
                        } else {
                            Ok(value)
                        }
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                columns.insert(field.clone(), values);
            }
            if book == "loans" || book == "debt" {
                for (i, row) in rows.iter().enumerate() {
                    let get = |field: &str| {
                        columns
                            .get(field)
                            .and_then(|v| v[i])
                            .or_else(|| row.get(field).and_then(Value::as_f64))
                    };
                    if let (Some(floor), Some(cap)) = (get("floor"), get("cap")) {
                        if floor > cap {
                            return Err("coupon floor cannot exceed cap".into());
                        }
                    }
                }
            }
            output.insert(book.clone(), columns);
        }
        Ok(json!(output))
    }
}
