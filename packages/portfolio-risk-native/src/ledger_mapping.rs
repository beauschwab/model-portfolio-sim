//! Explicit monthly product-to-ledger mapping. No accounting/risk classifications are inferred.
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
type R<T> = Result<T, String>;
const REQUIRED: [&str; 8] = [
    "account",
    "kind",
    "classification",
    "risk_weight",
    "asf_weight",
    "rsf_weight",
    "lcr_outflow_weight",
    "hqla_weight",
];
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    book: String,
    id: String,
    balance: f64,
    book_adjustment: f64,
    side: String,
    market_price: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    book: String,
    id: String,
    month: usize,
    principal: f64,
    cash_interest: f64,
    accrual_interest: f64,
    book_amortization: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Leg {
    template: String,
    purchase_m: usize,
    notional: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    template: String,
    h: usize,
    side: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    units: Vec<Unit>,
    horizon: usize,
    runoff: Vec<Vec<f64>>,
    cash_interest: Vec<Vec<f64>>,
    nii: Vec<Vec<f64>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingRequest {
    specification: Value,
    mapping: BTreeMap<String, Map<String, Value>>,
    amount_scale: f64,
    openings: Option<Vec<Opening>>,
    flows: Option<Vec<Flow>>,
    allocation: Option<Vec<Leg>>,
    library: Option<Library>,
}
fn metadata(meta: &Map<String, Value>, candidate: bool) -> R<bool> {
    let forbidden = if candidate {
        vec![
            "id",
            "source_id",
            "balance",
            "start_day",
            "book_adjustment",
            "opening_accrued",
        ]
    } else {
        vec![
            "id",
            "source_id",
            "balance",
            "book_adjustment",
            "start_day",
            "opening_market_price",
        ]
    };
    if REQUIRED.iter().any(|k| !meta.contains_key(*k))
        || forbidden.iter().any(|k| meta.contains_key(*k))
    {
        return Err("incomplete or conflicting accounting/risk mapping".into());
    }
    Ok(matches!(
        meta.get("kind").and_then(Value::as_str),
        Some("loan" | "security" | "reverse_repo")
    ))
}
fn finite(values: &[f64]) -> R<()> {
    crate::lifecycle_market::finite(values)
}
impl MappingRequest {
    pub fn run(self) -> R<Value> {
        if !self.amount_scale.is_finite() || self.amount_scale <= 0. {
            return Err("amount_scale must be positive and finite".into());
        }
        let mut spec = self
            .specification
            .as_object()
            .ok_or("specification must be an object")?
            .clone();
        let mut positions = match spec.get("positions") {
            None => Vec::new(),
            Some(v) => v.as_array().ok_or("positions must be an array")?.clone(),
        };
        let mut flows = match spec.get("cashflows") {
            None => Vec::new(),
            Some(v) => v.as_array().ok_or("cashflows must be an array")?.clone(),
        };
        if let Some(openings) = self.openings {
            if !positions.is_empty() || !flows.is_empty() {
                return Err(
                    "saved-book specification must not also supply positions or cashflows".into(),
                );
            }
            if self.allocation.is_some() || self.library.is_some() {
                return Err("ambiguous ledger mapping mode".into());
            }
            let mut seen = HashSet::new();
            for row in openings {
                let id = format!("{}:{}", row.book, row.id);
                if !seen.insert(id.clone()) {
                    return Err("duplicate opening identity".into());
                }
                let mut meta = self
                    .mapping
                    .get(&id)
                    .ok_or("missing explicit risk mapping")?
                    .clone();
                if metadata(&meta, false)? != (row.side == "asset") {
                    return Err("side mismatch".into());
                }
                let security = meta["kind"] == "security";
                if security
                    && matches!(meta["classification"].as_str(), Some("afs" | "trading"))
                    && row.market_price.is_none()
                {
                    return Err("marked security requires an explicit engine opening quote".into());
                }
                let price = if security {
                    row.market_price.unwrap_or(1.)
                } else {
                    1.
                };
                let balance = row.balance * self.amount_scale;
                let adjustment = row.book_adjustment * self.amount_scale;
                finite(&[balance, adjustment, price])?;
                for (key, value) in [
                    ("id", json!(id)),
                    ("source_id", json!(id)),
                    ("balance", json!(balance)),
                    ("book_adjustment", json!(adjustment)),
                    ("opening_market_price", json!(price)),
                ] {
                    meta.insert(key.into(), value);
                }
                positions.push(Value::Object(meta));
            }
            for row in self.flows.ok_or("missing captured cashflows")? {
                let id = format!("{}:{}", row.book, row.id);
                if !seen.contains(&id) {
                    return Err("cashflow names unknown opening".into());
                }
                let day = row.month.checked_mul(30).ok_or("cashflow day overflow")?;
                let values = [
                    row.principal,
                    row.cash_interest,
                    row.accrual_interest,
                    row.book_amortization,
                ]
                .map(|v| v * self.amount_scale);
                finite(&values)?;
                flows.push(json!({"position":id,"day":day,"principal":values[0],"cash_interest":values[1],"accrual_interest":values[2],"book_amortization":values[3]}));
            }
            spec.insert("provenance".into(),json!({"adapter":"saved-book-monthly-v1","amount_scale":self.amount_scale,
                "timing":"30-day reporting months; expected cashflows conditional on surviving principal",
                "scenario_cashflows":"base product paths plus explicit daily stress overlays","calibrated":false}));
        } else {
            if self.flows.is_some() {
                return Err("cashflows require openings".into());
            }
            let legs = self.allocation.ok_or("missing candidate allocation")?;
            let lib = self.library.ok_or("missing unit library")?;
            if legs.len() > 2000
                || !(1..=360).contains(&lib.horizon)
                || lib.units.len() != lib.runoff.len()
                || lib.units.len() != lib.cash_interest.len()
                || lib.units.len() != lib.nii.len()
            {
                return Err("invalid candidate library dimensions".into());
            }
            let mut seen = HashSet::new();
            for (i, leg) in legs.iter().enumerate() {
                if !leg.notional.is_finite()
                    || leg.notional <= 0.
                    || !seen.insert((&leg.template, leg.purchase_m))
                {
                    return Err("invalid or duplicate candidate leg".into());
                }
                let indices: Vec<_> = lib
                    .units
                    .iter()
                    .enumerate()
                    .filter(|(_, u)| u.template == leg.template && u.h == leg.purchase_m)
                    .collect();
                if indices.len() != 1 {
                    return Err("candidate requires an exact unit grid".into());
                }
                let (j, unit) = indices[0];
                let mut meta = self
                    .mapping
                    .get(&leg.template)
                    .ok_or("missing explicit template mapping")?
                    .clone();
                if metadata(&meta, true)? != (unit.side > 0.) {
                    return Err("candidate template side mismatch".into());
                }
                let amount = leg.notional * self.amount_scale;
                finite(&[amount])?;
                let day = leg
                    .purchase_m
                    .checked_mul(30)
                    .and_then(|v| v.checked_add(1))
                    .ok_or("candidate day overflow")?;
                let id = format!("candidate:{i}:{}@{}", leg.template, leg.purchase_m);
                for (key, value) in [
                    ("id", json!(id)),
                    ("source_id", json!(id)),
                    ("balance", json!(amount)),
                    ("start_day", json!(day)),
                ] {
                    meta.insert(key.into(), value);
                }
                positions.push(Value::Object(meta));
                let count = lib.horizon.saturating_sub(leg.purchase_m);
                if [&lib.runoff[j], &lib.cash_interest[j], &lib.nii[j]]
                    .iter()
                    .any(|v| v.len() < count)
                {
                    return Err("candidate coefficient shape mismatch".into());
                }
                for m in 0..count {
                    let values = [lib.runoff[j][m], lib.cash_interest[j][m], lib.nii[j][m]]
                        .map(|v| v * amount);
                    finite(&values)?;
                    flows.push(json!({"position":id,"day":(leg.purchase_m+m+1)*30,"principal":values[0],"cash_interest":values[1],"accrual_interest":values[2]}));
                }
            }
        }
        if positions.len() > 60000 || flows.len() > 250000 {
            return Err("ledger mapping output budget exceeded".into());
        }
        spec.insert("positions".into(), Value::Array(positions));
        spec.insert("cashflows".into(), Value::Array(flows));
        Ok(Value::Object(spec))
    }
}
