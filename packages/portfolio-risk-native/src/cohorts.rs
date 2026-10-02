//! Deterministic, product-separated tape cohort construction. No pricing proxies.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const HARD: [&str; 5] = [
    "product",
    "currency",
    "entity",
    "accounting_category",
    "assumption_set",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dimension {
    pub field: String,
    #[serde(default)]
    pub edges: Option<Vec<f64>>,
    #[serde(default)]
    pub separate_missing: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub dimensions: Vec<Dimension>,
    pub averages: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub rows: Vec<BTreeMap<String, Value>>,
    pub rules: BTreeMap<String, Rule>,
}
fn number(v: Option<&Value>, field: &str) -> Result<f64, String> {
    v.and_then(Value::as_f64)
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("{field} must be a finite number"))
}
impl Request {
    pub fn run(self) -> Result<Value, String> {
        if self.rows.is_empty() || self.rows.len() > 100_000 {
            return Err("tape requires 1..100000 rows per build".into());
        }
        for (product, rule) in &self.rules {
            if !["mortgage", "auto", "personal", "credit_card", "deposit"]
                .contains(&product.as_str())
                || rule.dimensions.len() > 24
                || rule.averages.len() > 32
            {
                return Err("unsupported product or excessive cohort dimensions".into());
            }
            let mut fields = BTreeSet::new();
            for d in &rule.dimensions {
                if !fields.insert(&d.field) || HARD.contains(&d.field.as_str()) {
                    return Err("duplicate or reserved dimension".into());
                }
                if let Some(edges) = &d.edges {
                    if edges.len() > 100
                        || edges.iter().any(|x| !x.is_finite())
                        || edges.windows(2).any(|w| w[0] >= w[1])
                    {
                        return Err("bucket edges must be finite and strictly increasing".into());
                    }
                }
            }
            if rule.averages.iter().collect::<BTreeSet<_>>().len() != rule.averages.len() {
                return Err("duplicate average field".into());
            }
        }
        let mut ids = BTreeSet::new();
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, row) in self.rows.iter().enumerate() {
            let id = row
                .get("loan_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or("loan_id must be a nonempty string")?;
            if !ids.insert(id) {
                return Err(format!("duplicate loan_id: {id}"));
            }
            if number(row.get("balance"), "balance")? <= 0. {
                return Err(format!(
                    "{id}: balance must be positive; exclude closed accounts explicitly"
                ));
            }
            let mut keys = BTreeMap::new();
            for field in HARD {
                let value = row
                    .get(field)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| format!("{id}: missing required {field}"))?;
                keys.insert(field.to_string(), json!(value));
            }
            let product = row["product"].as_str().unwrap();
            let rule = self
                .rules
                .get(product)
                .ok_or_else(|| format!("no rule for {product}"))?;
            for d in &rule.dimensions {
                let v = row.get(&d.field).unwrap_or(&Value::Null);
                let label = if v.is_null() {
                    if !d.separate_missing {
                        return Err(format!("{id}: missing dimension {}", d.field));
                    }
                    Value::Null
                } else if let Some(edges) = &d.edges {
                    let x = number(Some(v), &d.field)?;
                    // Left-closed intervals: [-inf,e0), [e0,e1), ..., [last,+inf).
                    json!({"bucket":edges.partition_point(|e| *e <= x)})
                } else if v.is_string() || v.is_boolean() || v.is_number() {
                    v.clone()
                } else {
                    return Err("categorical dimensions must be scalar".into());
                };
                keys.insert(d.field.clone(), label);
            }
            let key = serde_json::to_string(&keys).map_err(|e| e.to_string())?;
            groups.entry(key).or_default().push(i);
        }
        if groups.len() > 60_000 {
            return Err("build exceeds 60000 cohorts; coarsen rules".into());
        }
        let mut cohorts = Vec::new();
        let mut lineage = Vec::with_capacity(self.rows.len());
        let mut dispersion = Vec::new();
        for (key, mut members) in groups {
            // Stable reductions and identity under tape row permutation.
            members.sort_by_key(|i| self.rows[*i]["loan_id"].as_str().unwrap());
            let first = &self.rows[members[0]];
            let product = first["product"].as_str().unwrap();
            let balance: f64 = members
                .iter()
                .map(|i| self.rows[*i]["balance"].as_f64().unwrap())
                .sum();
            if !balance.is_finite() {
                return Err("cohort balance overflow".into());
            }
            let mut means = BTreeMap::new();
            for field in &self.rules[product].averages {
                let values: Vec<_> = members
                    .iter()
                    .map(|i| number(self.rows[*i].get(field), field))
                    .collect::<Result<_, _>>()?;
                let mean: f64 = values
                    .iter()
                    .zip(&members)
                    .map(|(v, i)| v * (self.rows[*i]["balance"].as_f64().unwrap() / balance))
                    .sum();
                let variance: f64 = values
                    .iter()
                    .zip(&members)
                    .map(|(v, i)| {
                        (v - mean).powi(2) * (self.rows[*i]["balance"].as_f64().unwrap() / balance)
                    })
                    .sum();
                if !mean.is_finite() || !variance.is_finite() {
                    return Err("cohort moment overflow".into());
                }
                means.insert(field.clone(), mean);
                dispersion.push(
                    json!({"key":key,"field":field,"mean":mean,"stddev":variance.sqrt(),
                    "min":values.iter().copied().fold(f64::INFINITY,f64::min),
                    "max":values.iter().copied().fold(f64::NEG_INFINITY,f64::max)}),
                );
            }
            cohorts.push(json!({"key":key,"product":product,"balance":balance,"members":members.len(),"means":means}));
            for i in members {
                let row = &self.rows[i];
                lineage.push(json!({"loan_id":row["loan_id"],"key":key,"product":product,
                    "balance":row["balance"],"weight":row["balance"].as_f64().unwrap()/balance}));
            }
        }
        Ok(json!({"cohorts":cohorts,"lineage":lineage,"dispersion":dispersion}))
    }
}
