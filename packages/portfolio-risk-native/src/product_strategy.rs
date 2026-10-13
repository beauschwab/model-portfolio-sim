//! Native product preparation and coefficient-only allocation replay.
//! No constructor accepts caller-produced coefficients or substituted cash amounts.
use crate::product_units::{self, ProductUnitsRequest, ProductUnitsResult};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "product-strategy-preparation-1";
pub const MAX_ALLOCATION_AMOUNT: f64 = MAX_MONETARY_AMOUNT;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UnitIdentity {
    pub unit_id: String,
    pub template_id: String,
    pub purchase_ordinal: i32,
    pub purchase_period: usize,
    pub side: f64,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub quote_price_per_face_unit: f64,
}

/// Fields are private and there is no Deserialize implementation: only native
/// preparation can establish this immutable financial source.
#[derive(Serialize)]
pub struct Prepared {
    schema: &'static str,
    units: Vec<UnitIdentity>,
    preparation: ProductUnitsResult,
}
impl Prepared {
    pub fn units(&self) -> &[UnitIdentity] {
        &self.units
    }
    pub fn source(&self) -> &ProductUnitsResult {
        &self.preparation
    }
}

pub fn prepare(request: &ProductUnitsRequest) -> Result<Prepared, String> {
    let preparation = product_units::prepare(request)?;
    from_preparation(request, preparation)
}

/// A native prepared owner is the only source of retained financial receipts.
pub fn prepare_update(
    request: &ProductUnitsRequest,
    previous: &Prepared,
) -> Result<(Prepared, usize, bool), String> {
    let (preparation, rebuilt, full) =
        product_units::prepare_reusing(request, Some(previous.source()))?;
    Ok((from_preparation(request, preparation)?, rebuilt, full))
}

fn from_preparation(
    request: &ProductUnitsRequest,
    preparation: ProductUnitsResult,
) -> Result<Prepared, String> {
    let mut ids = BTreeSet::new();
    let mut units = Vec::with_capacity(preparation.units.len());
    for u in &preparation.units {
        if !ids.insert(&u.unit_id) {
            return Err("duplicate strategy unit identity".into());
        }
        let purchase_period = request.reporting_ordinals[..request.reporting_ordinals.len() - 1]
            .iter()
            .position(|day| *day == u.purchase_ordinal)
            .ok_or("strategy purchase requires an exact reporting opening boundary")?;
        units.push(UnitIdentity {
            unit_id: u.unit_id.clone(),
            template_id: u.template_id.clone(),
            purchase_ordinal: u.purchase_ordinal,
            purchase_period,
            side: u.side,
            accounting_category: u.accounting_category.clone(),
            capital_policy_revision: u.capital_policy_revision.clone(),
            quote_price_per_face_unit: u.quote_price_per_face_unit,
        });
    }
    Ok(Prepared {
        schema: SCHEMA,
        units,
        preparation,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub unit_id: String,
    pub face: f64,
}

#[derive(Serialize)]
pub struct CashReplay {
    pub ordinal: i32,
    pub purchase_cash: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub fee_cash: f64,
    pub cumulative_bank_cash: f64,
    pub funding_demand: f64,
}
#[derive(Serialize)]
pub struct ScenarioReplay {
    pub scenario_id: String,
    /// Row-major [10,H], metric order exactly aged_units::METRICS.
    pub shape: [usize; 2],
    pub values: Vec<f64>,
    pub incremental_external_nii: f64,
    pub cash: Vec<CashReplay>,
}
#[derive(Serialize)]
pub struct Replay {
    pub schema: &'static str,
    pub units: Vec<UnitIdentity>,
    pub allocated_face: Vec<f64>,
    pub total_asset_face: f64,
    pub total_liability_face: f64,
    pub total_asset_purchase_cost: f64,
    pub template_face: BTreeMap<String, f64>,
    pub category_face: BTreeMap<String, f64>,
    pub scenarios: Vec<ScenarioReplay>,
    pub worst_incremental_external_nii: f64,
    pub scope: &'static str,
}
fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err("product allocation finite bounded monetary amount required".into())
    }
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}

/// Pure retained-coefficient evaluation. Requires every unit once, in exact
/// prepared order, including zero allocations; no graph or pricing calls.
pub fn evaluate(prepared: &Prepared, allocation: &[Allocation]) -> Result<Replay, String> {
    portfolio_compute_control::checkpoint()?;
    let p = &prepared.preparation;
    let [s, n, m, h] = p.shape;
    if allocation.len() != n {
        return Err("complete ordered allocation required".into());
    }
    let mut face = Vec::with_capacity(n);
    let (mut assets, mut liabilities, mut cost) = (0., 0., 0.);
    let mut templates = BTreeMap::new();
    let mut categories = BTreeMap::new();
    for (a, u) in allocation.iter().zip(&prepared.units) {
        if a.unit_id != u.unit_id || !a.face.is_finite() || a.face < 0. {
            return Err("allocation identity/order or nonnegative face mismatch".into());
        }
        let amount = money(a.face)?;
        face.push(amount);
        if u.side > 0. {
            assets = money(assets + amount)?;
            cost = money(cost + amount * u.quote_price_per_face_unit)?;
        } else {
            liabilities = money(liabilities + amount)?;
        }
        let old = templates.entry(u.template_id.clone()).or_insert(0.);
        *old = money(*old + amount)?;
        let old = categories
            .entry(u.accounting_category.clone())
            .or_insert(0.);
        *old = money(*old + amount)?;
    }
    let dates = p.funding.ordinals.len();
    let mut scenarios = Vec::with_capacity(s);
    let mut worst = f64::INFINITY;
    for si in 0..s {
        portfolio_compute_control::checkpoint()?;
        let mut values = vec![0.; m * h];
        for (ui, &amount) in face.iter().enumerate() {
            for metric in 0..m {
                for period in 0..h {
                    let at = metric * h + period;
                    values[at] = money(
                        values[at] + amount * p.values[((si * n + ui) * m + metric) * h + period],
                    )?;
                }
            }
        }
        let nii = money(values[..h].iter().sum())?;
        worst = worst.min(nii);
        // Source amounts already contain their physical path weights. Do not
        // multiply source_weight again and do not combine different scenarios.
        let events: Vec<_> = p
            .funding
            .cash_events
            .iter()
            .filter(|e| e.scenario_id == p.scenario_ids[si])
            .collect();
        let (mut cursor, mut cumulative) = (0, 0.);
        let mut cash = Vec::with_capacity(dates);
        for (di, &ordinal) in p.funding.ordinals.iter().enumerate() {
            if di % 64 == 0 {
                portfolio_compute_control::checkpoint()?;
            }
            let (mut purchase, mut principal, mut interest, mut fees) = (0., 0., 0., 0.);
            while cursor < events.len() && events[cursor].ordinal <= ordinal {
                let e = events[cursor];
                let amount = face[e.unit_index];
                purchase = money(purchase + amount * e.purchase_cash)?;
                principal = money(principal + amount * e.principal_cash)?;
                interest = money(interest + amount * e.interest_cash)?;
                fees = money(fees + amount * e.fee_cash)?;
                cumulative = money(cumulative + amount * e.bank_cash)?;
                cursor += 1;
            }
            let mut demand = 0.;
            for (ui, &amount) in face.iter().enumerate() {
                demand = money(demand + amount * p.funding.demand[(si * n + ui) * dates + di])?;
            }
            if !close(demand, -cumulative) {
                return Err("product allocation source-cash funding replay mismatch".into());
            }
            cash.push(CashReplay {
                ordinal,
                purchase_cash: purchase,
                principal_cash: principal,
                interest_cash: interest,
                fee_cash: fees,
                cumulative_bank_cash: cumulative,
                funding_demand: demand,
            });
        }
        if cursor != events.len() {
            return Err("product allocation full cash tail not replayed".into());
        }
        scenarios.push(ScenarioReplay {
            scenario_id: p.scenario_ids[si].clone(),
            shape: [m, h],
            values,
            incremental_external_nii: nii,
            cash,
        });
    }
    Ok(Replay {schema:"product-strategy-replay-1",units:prepared.units.clone(),allocated_face:face,
        total_asset_face:assets,total_liability_face:liabilities,total_asset_purchase_cost:cost,
        template_face:templates,category_face:categories,scenarios,worst_incremental_external_nii:worst,
        scope:"retained actual-purchase coefficients and scenario-specific full physical cash tails; reviewed performing effective yield only; no daily journal, empirical or joint credit acceptance"})
}
