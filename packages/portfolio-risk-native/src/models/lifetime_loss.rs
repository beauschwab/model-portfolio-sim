//! Physical economic loss sensitivity on complete supplied credit paths.
//! Scenario bands are sensitivities, never confidence intervals or allowances.
use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_path::{self as path, CreditPathRequest, CreditPathResult};
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "lifetime-credit-loss-1";
const MAX_WORK: usize = 32_000_000;
const MAX_BYTES: usize = 32 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub driver_id: String,
    pub unit: String,
    pub scaling_source_id: String,
    pub scaling_training_end: i32,
    pub availability_lag_days: u32,
    pub offset: f64,
    pub scale: f64,
    pub minimum: f64,
    pub maximum: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub forecast_as_of_ordinal: i32,
    /// Complete supplied, lag-aligned Physical projections at interval starts.
    /// These are forecast inputs, never observed fixings or Pricing paths.
    pub lag_aligned_values: Vec<Vec<f64>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Model {
    ConstantRow {
        model_id: String,
        model_version: String,
        hazards_per_year: [[f64; 5]; 3],
    },
    SuppliedCovariate {
        model_id: String,
        model_version: String,
        coefficient_source_id: String,
        coefficient_source_revision: String,
        training_end: i32,
        /// [origin * 5 + destination][driver], on frozen standardized values.
        beta: Vec<Vec<f64>>,
    },
    FixedRowAssumption {
        model_id: String,
        model_version: String,
        assumption_id: String,
        hazards_per_year: Vec<[[f64; 5]; 3]>,
    },
}
impl Model {
    fn identity(&self) -> (&str, &str, &'static str) {
        match self {
            Self::ConstantRow {
                model_id,
                model_version,
                ..
            } => (model_id, model_version, "constant_row"),
            Self::SuppliedCovariate {
                model_id,
                model_version,
                ..
            } => (model_id, model_version, "supplied_covariate"),
            Self::FixedRowAssumption {
                model_id,
                model_version,
                ..
            } => (model_id, model_version, "fixed_row_assumption"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub cohort_id: String,
    pub product: String,
    pub vintage: String,
    pub monetary_multiplier: f64,
    pub probability_weights: Vec<f64>,
    pub source: CreditPathRequest,
    pub models: Vec<Model>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub source_id: String,
    pub source_revision: String,
    pub data_kind: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub training_end: i32,
    pub assumptions: Vec<String>,
    pub drivers: Vec<Driver>,
    pub base_scenario_id: String,
    pub scenarios: Vec<Scenario>,
    pub cohorts: Vec<Cohort>,
    pub outcomes: Vec<ObservedOutcome>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedOutcome {
    pub cohort_id: String,
    pub vintage: String,
    pub source_id: String,
    pub source_revision: String,
    pub monetary_unit: String,
    /// Complete follow-up age from the cohort's original clock, not a new origin.
    pub observed_through_year: f64,
    /// Supplied cohort-scaled outcomes; no allowance/chargeoff conversion here.
    pub reported_principal_loss: f64,
    pub reported_signed_interest_loss: f64,
}
#[derive(Debug, Serialize)]
pub struct Cell {
    pub cohort_id: String,
    pub product: String,
    pub vintage: String,
    pub model_id: String,
    pub model_version: String,
    pub model_kind: &'static str,
    pub scenario_id: String,
    pub expected_principal_loss: f64,
    pub expected_signed_interest_loss: f64,
    pub expected_total_loss: f64,
    pub expected_default_probability: f64,
    pub full_paths: Vec<CreditPathResult>,
}
#[derive(Debug, Serialize)]
pub struct Band {
    pub cohort_id: String,
    pub dimension: &'static str,
    pub fixed_selection: String,
    pub minimum_loss: f64,
    pub maximum_loss: f64,
    pub spread: f64,
}
#[derive(Debug, Serialize)]
pub struct ProductTotal {
    pub product: String,
    pub model_kind: &'static str,
    pub scenario_id: String,
    pub expected_principal_loss: f64,
    pub expected_signed_interest_loss: f64,
    pub expected_total_loss: f64,
    pub cohorts: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Backtest {
    pub cohort_id: String,
    pub vintage: String,
    pub model_id: String,
    pub outcome_source_id: String,
    pub outcome_source_revision: String,
    pub projected_total_loss: f64,
    pub reported_total_loss: f64,
    pub reported_minus_projected: f64,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub cells: Vec<Cell>,
    pub sensitivity: Vec<Band>,
    pub product_totals: Vec<ProductTotal>,
    pub holdout_outcomes: Vec<Backtest>,
    pub admitted_work_upper: usize,
    pub admitted_output_bytes_upper: usize,
    pub complete_unweighted_paths_replayed: bool,
    pub common_random_numbers_retained: bool,
    pub observed_accuracy_validated: bool,
    pub allowance_or_accounting_approval: bool,
    pub adoption_eligible: bool,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        Err("lifetime loss requires bounded explicit identity".into())
    } else {
        Ok(())
    }
}
fn finite(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || v < lo || v > hi {
        Err("lifetime loss numeric domain".into())
    } else {
        Ok(())
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("lifetime loss admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("lifetime loss admission overflow".into())
}
fn validate(r: &Request) -> Result<(usize, usize), String> {
    check()?;
    if r.schema != SCHEMA
        || r.model_version != SCHEMA
        || !matches!(r.data_kind.as_str(), "synthetic" | "observed")
    {
        return Err("lifetime loss schema/version/data kind".into());
    }
    for text in [
        &r.source_id,
        &r.source_revision,
        &r.training_split_id,
        &r.holdout_split_id,
        &r.base_scenario_id,
    ] {
        label(text)?;
    }
    portfolio_model_core::cashflow::validate_ordinal(r.training_end)?;
    if r.training_split_id == r.holdout_split_id
        || r.assumptions.is_empty()
        || r.assumptions.len() > 64
        || r.cohorts.is_empty()
        || r.cohorts.len() > 16
        || r.scenarios.is_empty()
        || r.scenarios.len() > 8
        || r.drivers.is_empty()
        || r.drivers.len() > 16
    {
        return Err("lifetime loss source/split/count admission".into());
    }
    for a in &r.assumptions {
        label(a)?;
    }
    let mut ids = HashSet::new();
    for d in &r.drivers {
        for text in [&d.driver_id, &d.unit, &d.scaling_source_id] {
            label(text)?;
        }
        portfolio_model_core::cashflow::validate_ordinal(d.scaling_training_end)?;
        finite(d.offset, -1e12, 1e12)?;
        finite(d.scale, f64::MIN_POSITIVE, 1e12)?;
        finite(d.minimum, -1e12, 1e12)?;
        finite(d.maximum, d.minimum, 1e12)?;
        if !ids.insert(&d.driver_id)
            || d.scaling_training_end > r.training_end
            || d.availability_lag_days > 36500
        {
            return Err("lifetime loss driver identity/training/lag".into());
        }
    }
    let first = &r.cohorts[0].source;
    let mut scenario_ids = HashSet::new();
    for s in &r.scenarios {
        for text in [&s.scenario_id, &s.source_id, &s.source_revision] {
            label(text)?;
        }
        portfolio_model_core::cashflow::validate_ordinal(s.forecast_as_of_ordinal)?;
        if !scenario_ids.insert(&s.scenario_id)
            || s.forecast_as_of_ordinal > first.origin_time.ordinal
            || s.lag_aligned_values.len() != first.intervals.len()
        {
            return Err("lifetime loss scenario identity/date/grid".into());
        }
        for values in &s.lag_aligned_values {
            if values.len() != r.drivers.len() {
                return Err("lifetime loss driver dimension".into());
            }
            for (value, driver) in values.iter().zip(&r.drivers) {
                finite(*value, driver.minimum, driver.maximum)?;
            }
        }
    }
    if !scenario_ids.contains(&r.base_scenario_id) {
        return Err("lifetime loss base scenario absent".into());
    }
    let raw_bytes = strict_json::serialized_size(r, 2 << 20)?;
    let (mut work, mut bytes) = (raw_bytes, mul(raw_bytes, 4)?);
    let mut cohort_ids = HashSet::new();
    for c in &r.cohorts {
        check()?;
        for text in [&c.cohort_id, &c.vintage] {
            label(text)?;
        }
        if !cohort_ids.insert(&c.cohort_id)
            || !matches!(c.product.as_str(), "auto" | "personal" | "credit_card")
            || c.models.len() != 3
            || c.source.measure != CreditMeasure::Physical
            || c.source.origin_time.ordinal <= r.training_end
            || c.source.intervals.len() > 128
            || c.source.boundaries.len() > 128
            || c.source.max_jumps_per_path > 128
            || c.source.origin_time != first.origin_time
            || c.source.monetary_unit != first.monetary_unit
            || c.source.intervals.len() != first.intervals.len()
            || c.source
                .intervals
                .iter()
                .zip(&first.intervals)
                .any(|(a, b)| a.end_year.to_bits() != b.end_year.to_bits())
        {
            return Err("lifetime loss requires paired Physical cohorts, complete model choices and chronological grid".into());
        }
        path::validate_request_with_check(&c.source, &mut check)?;
        finite(c.monetary_multiplier, 0., 1e12)?;
        if c.probability_weights.len() != c.source.paths.len() {
            return Err("lifetime loss weights shape".into());
        }
        let mut total = 0.;
        for &weight in &c.probability_weights {
            finite(weight, 0., 1.)?;
            total += weight;
        }
        if total > 1. || (total - 1.).abs() > 1e-12 {
            return Err(
                "lifetime loss complete probability weights required; no normalization".into(),
            );
        }
        let mut kinds = HashSet::new();
        let mut models = HashSet::new();
        for m in &c.models {
            let (id, version, kind) = m.identity();
            label(id)?;
            label(version)?;
            if !models.insert(id) || !kinds.insert(kind) {
                return Err("lifetime loss repeated model choice".into());
            }
            match m {
                Model::SuppliedCovariate {
                    coefficient_source_id,
                    coefficient_source_revision,
                    training_end,
                    beta,
                    ..
                } => {
                    label(coefficient_source_id)?;
                    label(coefficient_source_revision)?;
                    portfolio_model_core::cashflow::validate_ordinal(*training_end)?;
                    if *training_end > r.training_end
                        || beta.len() != 15
                        || beta.iter().any(|b| b.len() != r.drivers.len())
                    {
                        return Err("lifetime loss covariate training/beta dimensions".into());
                    }
                    for value in beta.iter().flatten() {
                        finite(*value, -100., 100.)?;
                    }
                }
                Model::FixedRowAssumption {
                    assumption_id,
                    hazards_per_year,
                    ..
                } => {
                    label(assumption_id)?;
                    if !r.assumptions.contains(assumption_id)
                        || hazards_per_year.len() != c.source.intervals.len()
                    {
                        return Err("lifetime loss fixed-row assumption/grid absent".into());
                    }
                }
                Model::ConstantRow { .. } => (),
            }
        }
        let recoveries = c
            .source
            .intervals
            .iter()
            .flat_map(|i| &i.complete_recovery_tails)
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        let events = add(
            add(c.source.max_jumps_per_path, c.source.boundaries.len())?,
            recoveries,
        )?;
        let path_bytes = add(4096, mul(add(events, 1)?, 2048)?)?;
        let calls = mul(
            mul(c.source.paths.len(), c.models.len())?,
            r.scenarios.len(),
        )?;
        let source_bytes = strict_json::serialized_size(&c.source, 1 << 20)?;
        bytes = add(bytes, mul(calls, path_bytes)?)?;
        work = add(work, mul(calls, add(mul(source_bytes, 3)?, path_bytes)?)?)?;
        work = add(
            work,
            mul(
                mul(c.models.len() * r.scenarios.len(), c.source.intervals.len())?,
                r.drivers.len() * 64,
            )?,
        )?;
    }
    if bytes > MAX_BYTES || work > MAX_WORK {
        return Err("lifetime loss aggregate work/output admission exceeded".into());
    }
    if r.outcomes.len() > r.cohorts.len() {
        return Err("lifetime loss outcome count".into());
    }
    let mut outcomes = HashSet::new();
    for outcome in &r.outcomes {
        for text in [&outcome.source_id, &outcome.source_revision] {
            label(text)?;
        }
        let cohort = r
            .cohorts
            .iter()
            .find(|c| c.cohort_id == outcome.cohort_id)
            .ok_or("lifetime loss outcome cohort absent")?;
        let end = cohort
            .source
            .intervals
            .last()
            .ok_or("lifetime loss intervals absent")?
            .end_year;
        let lag = cohort
            .source
            .intervals
            .iter()
            .flat_map(|i| &i.complete_recovery_tails)
            .flatten()
            .map(|p| p.lag_years)
            .fold(0., f64::max);
        finite(outcome.observed_through_year, end + lag, 200.)?;
        finite(outcome.reported_principal_loss, 0., 1e15)?;
        finite(outcome.reported_signed_interest_loss, -1e15, 1e15)?;
        if !outcomes.insert(&outcome.cohort_id)
            || outcome.vintage != cohort.vintage
            || outcome.monetary_unit != cohort.source.monetary_unit
        {
            return Err("lifetime loss outcome vintage/unit/uniqueness".into());
        }
    }
    Ok((work, bytes))
}

fn source_for(
    c: &Cohort,
    m: &Model,
    s: &Scenario,
    drivers: &[Driver],
) -> Result<CreditPathRequest, String> {
    let mut source = c.source.clone();
    for (index, period) in source.intervals.iter_mut().enumerate() {
        check()?;
        period.hazards_per_year = match m {
            Model::ConstantRow {
                hazards_per_year, ..
            } => *hazards_per_year,
            Model::FixedRowAssumption {
                hazards_per_year, ..
            } => hazards_per_year[index],
            Model::SuppliedCovariate { beta, .. } => {
                let mut hazards = period.hazards_per_year;
                for (origin, row) in hazards.iter_mut().enumerate() {
                    for (destination, value) in row.iter_mut().enumerate() {
                        let b = &beta[origin * 5 + destination];
                        if *value == 0. {
                            if b.iter().any(|v| *v != 0.) {
                                return Err(
                                    "zero baseline hazard requires zero coefficients".into()
                                );
                            }
                        } else if b.iter().any(|v| *v != 0.) {
                            let adjustment = b
                                .iter()
                                .zip(&s.lag_aligned_values[index])
                                .zip(drivers)
                                .map(|((b, x), d)| b * ((x - d.offset) / d.scale))
                                .sum::<f64>();
                            *value *= adjustment.exp();
                            finite(*value, f64::MIN_POSITIVE, 100.)?;
                        }
                    }
                }
                hazards
            }
        };
    }
    // Validate every model/scenario, including zero-weight/zero-scale cases,
    // before source generation. Invalid hazards are rejected, never clipped.
    path::validate_request_with_check(&source, &mut check)?;
    Ok(source)
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let (work, bytes) = validate(r)?;
    // Complete cross-product preflight before generating any path.
    for c in &r.cohorts {
        for m in &c.models {
            for s in &r.scenarios {
                source_for(c, m, s, &r.drivers)?;
            }
        }
    }
    let mut cells = Vec::new();
    for c in &r.cohorts {
        for m in &c.models {
            for s in &r.scenarios {
                check()?;
                let source = source_for(c, m, s, &r.drivers)?;
                let (id, version, kind) = m.identity();
                let mut cell = Cell {
                    cohort_id: c.cohort_id.clone(),
                    product: c.product.clone(),
                    vintage: c.vintage.clone(),
                    model_id: id.into(),
                    model_version: version.into(),
                    model_kind: kind,
                    scenario_id: s.scenario_id.clone(),
                    expected_principal_loss: 0.,
                    expected_signed_interest_loss: 0.,
                    expected_total_loss: 0.,
                    expected_default_probability: 0.,
                    full_paths: Vec::new(),
                };
                for (opening, weight) in source.paths.iter().zip(&c.probability_weights) {
                    check()?;
                    let mut one = source.clone();
                    one.paths = vec![opening.clone()];
                    if let path::DrawStream::Supplied { paths, .. } = &mut one.draw_stream {
                        paths.retain(|p| p.path_key == opening.path_key);
                    }
                    let result = path::evaluate(&one)?
                        .paths
                        .into_iter()
                        .next()
                        .ok_or("lifetime loss path absent")?;
                    check()?;
                    if matches!(
                        result.outcome,
                        path::PathOutcome::Performing
                            | path::PathOutcome::Watch
                            | path::PathOutcome::Delinquent
                    ) {
                        return Err("lifetime loss requires complete contractual exits, not a truncated report horizon".into());
                    }
                    if let Some(default) = &result.default {
                        let interest_loss = default.accrued_interest
                            - default.recoveries.iter().map(|p| p.interest).sum::<f64>();
                        cell.expected_principal_loss +=
                            weight * default.economic_principal_loss * c.monetary_multiplier;
                        cell.expected_signed_interest_loss +=
                            weight * interest_loss * c.monetary_multiplier;
                        cell.expected_default_probability += weight;
                        finite(cell.expected_principal_loss, 0., 1e15)?;
                        finite(cell.expected_signed_interest_loss, -1e15, 1e15)?;
                    }
                    cell.full_paths.push(result);
                }
                cell.expected_total_loss =
                    cell.expected_principal_loss + cell.expected_signed_interest_loss;
                finite(cell.expected_total_loss, -1e15, 1e15)?;
                cells.push(cell);
            }
        }
    }
    let mut sensitivity = Vec::new();
    for c in &r.cohorts {
        let selections = c.models.iter().map(|m| ("macro", m.identity().0)).chain(
            r.scenarios
                .iter()
                .map(|s| ("model", s.scenario_id.as_str())),
        );
        for (dimension, selection) in selections {
            let values: Vec<_> = cells
                .iter()
                .filter(|v| {
                    v.cohort_id == c.cohort_id
                        && if dimension == "macro" {
                            v.model_id == selection
                        } else {
                            v.scenario_id == selection
                        }
                })
                .map(|v| v.expected_total_loss)
                .collect();
            let low = values.iter().copied().fold(f64::INFINITY, f64::min);
            let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            sensitivity.push(Band {
                cohort_id: c.cohort_id.clone(),
                dimension,
                fixed_selection: selection.into(),
                minimum_loss: low,
                maximum_loss: high,
                spread: high - low,
            });
        }
    }
    let mut product_totals: Vec<ProductTotal> = Vec::new();
    let mut holdout_outcomes = Vec::new();
    for cell in &cells {
        check()?;
        let index = product_totals.iter().position(|t| {
            t.product == cell.product
                && t.model_kind == cell.model_kind
                && t.scenario_id == cell.scenario_id
        });
        let total = if let Some(i) = index {
            &mut product_totals[i]
        } else {
            product_totals.push(ProductTotal {
                product: cell.product.clone(),
                model_kind: cell.model_kind,
                scenario_id: cell.scenario_id.clone(),
                expected_principal_loss: 0.,
                expected_signed_interest_loss: 0.,
                expected_total_loss: 0.,
                cohorts: Vec::new(),
            });
            product_totals
                .last_mut()
                .ok_or("lifetime loss aggregate absent")?
        };
        total.expected_principal_loss += cell.expected_principal_loss;
        total.expected_signed_interest_loss += cell.expected_signed_interest_loss;
        total.expected_total_loss =
            total.expected_principal_loss + total.expected_signed_interest_loss;
        total.cohorts.push(cell.cohort_id.clone());
        finite(total.expected_principal_loss, 0., 1e15)?;
        finite(total.expected_signed_interest_loss, -1e15, 1e15)?;
        finite(total.expected_total_loss, -1e15, 1e15)?;
        if cell.scenario_id == r.base_scenario_id {
            if let Some(observed) = r.outcomes.iter().find(|o| o.cohort_id == cell.cohort_id) {
                let reported =
                    observed.reported_principal_loss + observed.reported_signed_interest_loss;
                finite(reported, -1e15, 1e15)?;
                holdout_outcomes.push(Backtest {
                    cohort_id: cell.cohort_id.clone(),
                    vintage: cell.vintage.clone(),
                    model_id: cell.model_id.clone(),
                    outcome_source_id: observed.source_id.clone(),
                    outcome_source_revision: observed.source_revision.clone(),
                    projected_total_loss: cell.expected_total_loss,
                    reported_total_loss: reported,
                    reported_minus_projected: reported - cell.expected_total_loss,
                });
            }
        }
    }
    check()?;
    let result = ResultData {
        schema: SCHEMA,
        raw_source: r.clone(),
        cells,
        sensitivity,
        product_totals,
        holdout_outcomes,
        admitted_work_upper: work,
        admitted_output_bytes_upper: bytes,
        complete_unweighted_paths_replayed: true,
        common_random_numbers_retained: true,
        observed_accuracy_validated: false,
        allowance_or_accounting_approval: false,
        adoption_eligible: false,
    };
    if strict_json::serialized_size(&result, MAX_BYTES)? > bytes {
        return Err("lifetime loss output exceeds prospective bound".into());
    }
    check()?;
    Ok(result)
}
