//! Actual-purchase/aged fixed-contract coefficients for the existing native solver.
pub mod cache;
pub mod funding;
use crate::advanced_product::{self as product, FixedTermDefinition, SourceIdentity};
use crate::market::analysis::curves::SuppliedCurve;
use portfolio_model_core::cashflow::{validate_ordinal, MAX_HORIZON_DAYS};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "aged-units-1";
pub const METRICS: [&str; 10] = [
    "nii",
    "balance",
    "dv01",
    "hqla_l2a",
    "outflow30",
    "asf",
    "rsf",
    "rwa",
    "assets",
    "liabilities",
];
pub const MAX_VALUES: usize = 1_000_000;
pub const MAX_PRICING_WORK: usize = 8_000_000;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedIdentity {
    pub model_revision: String,
    pub grid_revision: String,
    pub crn_id: String,
    pub input_revision: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    pub hqla_l2a: f64,
    pub outflow30: f64,
    pub asf: f64,
    pub rsf: f64,
    pub rwa: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedTemplate {
    pub template_id: String,
    pub side: f64,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub weights: Weights,
    pub term: FixedTermDefinition,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedPurchase {
    pub template_id: String,
    pub purchase_ordinal: i32,
    pub quote_price_per_unit: f64,
    pub original_annual_effective_yield: f64,
    pub original_yield_anchor: SourceIdentity,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedScenario {
    pub scenario_id: String,
    pub collateral_curve: SuppliedCurve,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ValuationPolicy {
    ContractualDiscounting {},
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedUnitsRequest {
    pub schema: String,
    pub identity: AgedIdentity,
    pub valuation_policy: ValuationPolicy,
    pub curve_as_of_ordinal: i32,
    pub currency: String,
    pub reporting_ordinals: Vec<i32>,
    pub base_scenario_id: String,
    pub scenarios: Vec<AgedScenario>,
    pub templates: Vec<AgedTemplate>,
    pub purchases: Vec<AgedPurchase>,
    pub validation_purchases: Vec<AgedPurchase>,
    pub bump_bp: f64,
    pub interpolation_absolute_tolerance: f64,
    pub interpolation_relative_tolerance: f64,
}
#[derive(Clone, Serialize)]
pub struct AgedUnit {
    pub template: String,
    pub h: usize,
    pub side: f64,
    pub purchase_ordinal: i32,
    pub quote_price_per_unit: f64,
    pub base_oas: f64,
    pub original_annual_effective_yield: f64,
    pub original_yield_anchor: SourceIdentity,
    pub accounting_category: String,
    pub capital_policy_revision: String,
    pub full_contract_end_ordinal: i32,
    pub opening_states: Vec<product::ReplayedBookState>,
}
#[derive(Clone, Serialize)]
pub struct InterpolationDiagnostic {
    pub scenario_id: String,
    pub template: String,
    pub purchase_ordinal: i32,
    pub metric: String,
    pub max_absolute_error: f64,
    pub max_relative_error: Option<f64>,
    pub max_absolute_error_period: usize,
    pub within_supplied_tolerance: bool,
}
#[derive(Clone, Serialize)]
pub struct AgedUnitsResult {
    pub schema: String,
    pub identity: AgedIdentity,
    pub native_projection_model: String,
    pub curve_as_of_ordinal: i32,
    pub currency: String,
    pub bump_bp: f64,
    pub curve_scenarios: Vec<AgedScenario>,
    pub template_definitions: Vec<AgedTemplate>,
    pub funding: funding::FundingResult,
    pub valuation_policy: ValuationPolicy,
    pub scenario_ids: Vec<String>,
    pub base_scenario_id: String,
    pub reporting_ordinals: Vec<i32>,
    pub units: Vec<AgedUnit>,
    pub horizon: usize,
    pub shape: [usize; 4],
    /// Dense row-major [scenario,unit,metric,reporting period]. No age-risk proxy.
    pub values: Vec<f64>,
    pub interpolation_diagnostics: Vec<InterpolationDiagnostic>,
    pub income_policy: String,
    pub scope: String,
}
fn bounded(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        Err("aged-unit numeric policy outside bounds".into())
    } else {
        Ok(())
    }
}
fn validate(r: &AgedUnitsRequest) -> Result<(usize, usize), String> {
    if r.schema != SCHEMA {
        return Err("unsupported aged-unit schema".into());
    }
    for s in [
        &r.identity.model_revision,
        &r.identity.grid_revision,
        &r.identity.crn_id,
        &r.identity.input_revision,
        &r.base_scenario_id,
        &r.currency,
    ] {
        product::label(s)?;
    }
    validate_ordinal(r.curve_as_of_ordinal)?;
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("reporting boundaries required")?;
    if !(1..=120).contains(&h)
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || r.templates.is_empty()
        || r.templates.len() > 32
        || r.purchases.is_empty()
        || r.purchases.len() > 512
        || r.validation_purchases.len() > 128
    {
        return Err("aged-unit shape admission exceeded".into());
    }
    for &day in &r.reporting_ordinals {
        validate_ordinal(day)?;
    }
    if r.reporting_ordinals[0] < r.curve_as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
        || (r.reporting_ordinals[h] - r.curve_as_of_ordinal) as u32 > MAX_HORIZON_DAYS
    {
        return Err("reporting boundaries must increase within full admitted dates".into());
    }
    bounded(r.bump_bp, 0.001, 1000.)?;
    bounded(r.interpolation_absolute_tolerance, 0., 1e15)?;
    bounded(r.interpolation_relative_tolerance, 0., 100.)?;
    let mut names = HashSet::new();
    for s in &r.scenarios {
        product::label(&s.scenario_id)?;
        if !names.insert(&s.scenario_id)
            || s.collateral_curve.currency != r.currency
            || !matches!(
                s.collateral_curve.role,
                crate::market::analysis::curves::CurveRole::Collateral { .. }
            )
        {
            return Err("duplicate scenario/currency mismatch".into());
        }
        s.collateral_curve.validate()?;
    }
    let base = r
        .scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing explicit base scenario")?;
    names.clear();
    for t in &r.templates {
        for s in [
            &t.template_id,
            &t.accounting_category,
            &t.capital_policy_revision,
        ] {
            product::label(s)?;
        }
        if !names.insert(&t.template_id)
            || t.side
                != if t.term.product == crate::term_deck::Product::Corporate {
                    1.
                } else {
                    -1.
                }
        {
            return Err("duplicate template or product direction mismatch".into());
        }
        for x in [
            t.weights.hqla_l2a,
            t.weights.outflow30,
            t.weights.asf,
            t.weights.rsf,
            t.weights.rwa,
        ] {
            bounded(x, 0., 100.)?;
        }
    }
    let mut pairs = HashSet::new();
    let mut last_purchase = HashMap::new();
    for p in &r.purchases {
        if last_purchase
            .insert(&p.template_id, p.purchase_ordinal)
            .is_some_and(|last| last >= p.purchase_ordinal)
        {
            return Err("prepared purchase nodes must increase within each template".into());
        }
    }
    for p in r.purchases.iter().chain(&r.validation_purchases) {
        product::label(&p.template_id)?;
        product::label(&p.original_yield_anchor.id)?;
        product::label(&p.original_yield_anchor.revision)?;
        if !names.contains(&p.template_id)
            || !pairs.insert((&p.template_id, p.purchase_ordinal))
            || !r.reporting_ordinals[..h].contains(&p.purchase_ordinal)
        {
            return Err(
                "unknown/duplicate purchase or purchase outside reporting boundary grid".into(),
            );
        }
        bounded(p.quote_price_per_unit, 1e-12, 1e15)?;
        bounded(p.original_annual_effective_yield, -0.99, 10.)?;
    }
    for t in &r.templates {
        if !r.purchases.iter().any(|p| p.template_id == t.template_id) {
            return Err("template has no prepared purchase nodes".into());
        }
    }
    let count = r
        .scenarios
        .len()
        .checked_mul(r.purchases.len() + r.validation_purchases.len())
        .and_then(|v| v.checked_mul(10 * h))
        .ok_or("aged unit work overflow")?;
    if count > MAX_VALUES {
        return Err("aged-unit values/work admission exceeded".into());
    }
    Ok((h, base))
}
fn build_row(
    r: &AgedUnitsRequest,
    p: &AgedPurchase,
    h: usize,
    base: usize,
    remaining_work: usize,
    unit_index: usize,
) -> Result<(AgedUnit, Vec<f64>, usize, funding::UnitCashPlan), String> {
    portfolio_compute_control::checkpoint()?;
    let t = r
        .templates
        .iter()
        .find(|t| t.template_id == p.template_id)
        .ok_or("missing template")?;
    let projection = product::project_fixed(
        &t.term,
        p.purchase_ordinal,
        &p.template_id,
        &r.currency,
        &product::SourceIdentity {
            id: p.template_id.clone(),
            revision: r.identity.input_revision.clone(),
        },
    )?;
    let work = (100 + (r.scenarios.len() * 2 + 2) * h)
        .checked_mul(projection.events.len())
        .ok_or("aged price work overflow")?;
    if work > remaining_work {
        return Err("aged price row work admission exceeded".into());
    }
    product::validate_original_yield(
        &projection,
        p.quote_price_per_unit,
        p.original_annual_effective_yield,
    )?;
    let oas = product::solve_purchase_oas(
        &projection,
        r.curve_as_of_ordinal,
        &r.scenarios[base].collateral_curve,
        p.quote_price_per_unit,
    )?;
    let purchase_h = r.reporting_ordinals[..h]
        .iter()
        .position(|x| *x == p.purchase_ordinal)
        .ok_or("purchase not on grid")?;
    let mut states = Vec::new();
    for &day in &r.reporting_ordinals[purchase_h..] {
        states.push(product::replay_book(
            &projection,
            day,
            p.quote_price_per_unit,
            p.original_annual_effective_yield,
        )?);
    }
    let mut values = vec![0.; r.scenarios.len() * 10 * h];
    for (s, scenario) in r.scenarios.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        for m in purchase_h..h {
            let left = &states[m - purchase_h];
            let right = &states[m - purchase_h + 1];
            let income = right.gross_carrying_value - left.gross_carrying_value
                + right.cash_paid_to_date
                - left.cash_paid_to_date;
            let bump = r.bump_bp * 1e-4;
            let down = product::price_remaining(
                &projection,
                r.curve_as_of_ordinal,
                left.ordinal,
                &scenario.collateral_curve,
                oas,
                -bump,
            )?;
            let up = product::price_remaining(
                &projection,
                r.curve_as_of_ordinal,
                left.ordinal,
                &scenario.collateral_curve,
                oas,
                bump,
            )?;
            let dv = (down - up) / (2. * r.bump_bp);
            let balance = left.principal;
            let array = [
                t.side * income,
                balance,
                t.side * dv,
                t.weights.hqla_l2a * balance,
                t.weights.outflow30 * balance,
                t.weights.asf * balance,
                t.weights.rsf * balance,
                t.weights.rwa * balance,
                if t.side > 0. { balance } else { 0. },
                if t.side < 0. { balance } else { 0. },
            ];
            for (j, x) in array.into_iter().enumerate() {
                values[s * 10 * h + j * h + m] = product::amount(x)?;
            }
        }
    }
    let cash_plan = funding::plan(
        &projection,
        p.quote_price_per_unit,
        t.side,
        unit_index,
        SourceIdentity {
            id: p.template_id.clone(),
            revision: r.identity.input_revision.clone(),
        },
    )?;
    Ok((
        AgedUnit {
            template: p.template_id.clone(),
            h: purchase_h,
            side: t.side,
            purchase_ordinal: p.purchase_ordinal,
            quote_price_per_unit: p.quote_price_per_unit,
            base_oas: oas,
            original_annual_effective_yield: p.original_annual_effective_yield,
            original_yield_anchor: p.original_yield_anchor.clone(),
            accounting_category: t.accounting_category.clone(),
            capital_policy_revision: t.capital_policy_revision.clone(),
            full_contract_end_ordinal: projection.full_end_ordinal,
            opening_states: states,
        },
        values,
        work,
        cash_plan,
    ))
}
pub fn prepare(r: &AgedUnitsRequest) -> Result<AgedUnitsResult, String> {
    prepare_with_work(r, MAX_PRICING_WORK).map(|(result, _)| result)
}
pub(crate) fn prepare_with_work(
    r: &AgedUnitsRequest,
    pricing_limit: usize,
) -> Result<(AgedUnitsResult, usize), String> {
    if pricing_limit > MAX_PRICING_WORK {
        return Err("aged pricing limit outside admission".into());
    }
    let (h, base) = validate(r)?;
    let n = r.purchases.len();
    let s = r.scenarios.len();
    let mut values = vec![0.; s * n * 10 * h];
    let mut units = Vec::new();
    let mut cash_plans = Vec::new();
    let mut work = 0usize;
    for (i, p) in r.purchases.iter().enumerate() {
        let (unit, row, w, cash_plan) = build_row(r, p, h, base, pricing_limit - work, i)?;
        work = work
            .checked_add(w)
            .ok_or("aggregate aged pricing work overflow")?;
        if work > pricing_limit {
            return Err("aggregate aged pricing work admission exceeded".into());
        }
        for scenario in 0..s {
            values[(scenario * n + i) * 10 * h..(scenario * n + i + 1) * 10 * h]
                .copy_from_slice(&row[scenario * 10 * h..(scenario + 1) * 10 * h]);
        }
        units.push(unit);
        cash_plans.push(cash_plan);
    }
    let mut diagnostics = Vec::new();
    for p in &r.validation_purchases {
        let mut nodes: Vec<_> = units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.template == p.template_id)
            .collect();
        nodes.sort_by_key(|(_, u)| u.purchase_ordinal);
        let hi = nodes.partition_point(|(_, u)| u.purchase_ordinal < p.purchase_ordinal);
        if hi == 0 || hi == nodes.len() {
            return Err(
                "off-grid validation requires bracketing prepared purchase nodes; no flat tails"
                    .into(),
            );
        }
        let (a, b) = (nodes[hi - 1], nodes[hi]);
        let weight = (p.purchase_ordinal - a.1.purchase_ordinal) as f64
            / (b.1.purchase_ordinal - a.1.purchase_ordinal) as f64;
        let (_, native, w, _) = build_row(r, p, h, base, pricing_limit - work, n)?;
        work = work
            .checked_add(w)
            .ok_or("aggregate aged pricing work overflow")?;
        if work > pricing_limit {
            return Err("aggregate aged pricing work admission exceeded".into());
        }
        for scenario in 0..s {
            for (metric, name) in METRICS.iter().enumerate() {
                let mut absolute = 0.;
                let mut relative: Option<f64> = None;
                let mut peak = 0;
                let mut within = true;
                for m in 0..h {
                    let mix = (1. - weight)
                        * values[(scenario * n + a.0) * 10 * h + metric * h + m]
                        + weight * values[(scenario * n + b.0) * 10 * h + metric * h + m];
                    let exact = native[scenario * 10 * h + metric * h + m];
                    let error = (mix - exact).abs();
                    if error > absolute {
                        absolute = error;
                        peak = m;
                    }
                    if exact != 0. {
                        let ratio = error / exact.abs();
                        if !ratio.is_finite() {
                            return Err(
                                "interpolation relative-error diagnostic exceeds finite domain"
                                    .into(),
                            );
                        }
                        relative = Some(relative.unwrap_or(0.).max(ratio));
                    }
                    within &= error
                        <= r.interpolation_absolute_tolerance
                            + r.interpolation_relative_tolerance * exact.abs();
                }
                diagnostics.push(InterpolationDiagnostic {
                    scenario_id: r.scenarios[scenario].scenario_id.clone(),
                    template: p.template_id.clone(),
                    purchase_ordinal: p.purchase_ordinal,
                    metric: (*name).into(),
                    max_absolute_error: absolute,
                    max_relative_error: relative,
                    max_absolute_error_period: peak,
                    within_supplied_tolerance: within,
                });
            }
        }
    }
    let funding = funding::compile(
        cash_plans,
        r.scenarios.iter().map(|s| s.scenario_id.clone()).collect(),
        &r.reporting_ordinals,
        r.identity.input_revision.clone(),
    )?;
    Ok((AgedUnitsResult { schema:SCHEMA.into(),identity:r.identity.clone(), funding,
        native_projection_model:"advanced-fixed-contracts-1".into(),curve_as_of_ordinal:r.curve_as_of_ordinal,currency:r.currency.clone(),bump_bp:r.bump_bp,
        curve_scenarios:r.scenarios.clone(),template_definitions:r.templates.clone(),
        valuation_policy:r.valuation_policy.clone(),scenario_ids:r.scenarios.iter().map(|s|s.scenario_id.clone()).collect(),
        base_scenario_id:r.base_scenario_id.clone(),reporting_ordinals:r.reporting_ordinals.clone(),units,horizon:h,shape:[s,n,10,h],values,interpolation_diagnostics:diagnostics,
        income_policy:"contractual rights with validated frozen original effective yield; reporting (left,right], closing-after-settlement opening state".into(),
        scope:"fixed nonoptional corporate/CD actual-date slice; supplied weights/category dependencies, no credit/behavior conditioning or daily candidate acceptance".into() }, work))
}
