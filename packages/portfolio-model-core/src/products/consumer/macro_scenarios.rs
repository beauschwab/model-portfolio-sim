//! Explicit Physical conditional consumer macro paths and chronological innovation moments.
//! Supplied rate scenarios are retained; no LMM, Pricing conversion or stress response is inferred.
use super::behavior;
use super::macro_identity;
use crate::credit::CreditMeasure;
use crate::credit_accounting::Identity;
use crate::credit_covariate::{DriverInterval, DriverKind, FeatureRole};
use crate::observation::{DataKind, ObservationTime};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "consumer-macro-scenarios-1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    ShortRate,
    UnemploymentLevel,
    UnemploymentChange12m,
    UsedVehicleIndex,
    UsedVehicleIndexChange12m,
    PrimeMinusPolicy,
    CardAprSpread,
    ConsumerRateChange12m,
    RealDisposableIncomeGrowth,
    HpiChange12m,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    PercentagePoints,
    IndexLevel,
}
impl Unit {
    fn name(self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::PercentagePoints => "percentage_points",
            Self::IndexLevel => "index_level",
        }
    }
}
impl Metric {
    fn unit(self) -> Unit {
        match self {
            Self::UsedVehicleIndex => Unit::IndexLevel,
            Self::UnemploymentChange12m
            | Self::PrimeMinusPolicy
            | Self::CardAprSpread
            | Self::ConsumerRateChange12m => Unit::PercentagePoints,
            _ => Unit::Percent,
        }
    }
    fn column(self) -> Option<&'static str> {
        Some(match self {
            Self::ShortRate => "short_rate",
            Self::UnemploymentLevel => "unemp_level",
            Self::UnemploymentChange12m => "unemp_chg_12m",
            Self::UsedVehicleIndex => return None,
            Self::UsedVehicleIndexChange12m => "used_vehicle_idx_chg_12m",
            Self::PrimeMinusPolicy => "prime_minus_policy",
            Self::CardAprSpread => "card_apr_spread",
            Self::ConsumerRateChange12m => "consumer_rate_chg_12m",
            Self::RealDisposableIncomeGrowth => "real_dpi_growth",
            Self::HpiChange12m => "hpi_chg_12m",
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub driver_id: String,
    pub metric: Metric,
    /// National and regional series remain distinct, explicit source coordinates.
    pub region_id: String,
    pub unit: Unit,
    pub raw_domain: [f64; 2],
    pub availability_lag_days: u32,
    /// Frozen scale for (end - start) / (scale * sqrt(ACT/365 years)).
    pub innovation_scale: f64,
    pub scaling_source: Identity,
    pub scaling_training_end: ObservationTime,
    pub source: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub row_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub start_values: Vec<f64>,
    pub end_values: Vec<f64>,
    pub published_at: Vec<ObservationTime>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub source: Identity,
    pub data_kind: DataKind,
    pub training_split: Identity,
    pub holdout_split: Identity,
    pub training_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub observations: Vec<Observation>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    Baseline,
    ParallelUp200,
    ParallelDown200,
    SeverelyAdverse,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionalPath {
    pub path_key: u64,
    /// Same path key and interval order as the supplied conditional rate parent.
    pub short_rates_percent: Vec<f64>,
    pub values: Vec<Vec<f64>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario_id: String,
    pub kind: ScenarioKind,
    pub source: Identity,
    pub conditional_rate_source: Identity,
    pub issued_at: ObservationTime,
    pub paths: Vec<ConditionalPath>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub measure: CreditMeasure,
    pub as_of: ObservationTime,
    pub common_random_numbers: Identity,
    pub credit_draw_stream: crate::credit_path::DrawStream,
    pub policy: Identity,
    pub drivers: Vec<Driver>,
    pub history: History,
    pub interval_ends: Vec<ObservationTime>,
    /// All four templates are supplied, including their macro response.
    pub scenarios: Vec<Scenario>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Moments {
    pub row_ids: Vec<String>,
    pub mean_annualized_standardized_innovation: Vec<f64>,
    pub sample_annual_covariance: Vec<Vec<f64>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub training: Moments,
    pub holdout: Moments,
    pub holdout_mean_error_from_training: Vec<f64>,
    pub excluded_cross_split: Vec<String>,
    pub excluded_unavailable_at_split: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub identity_policy: &'static str,
    pub source_sha256: String,
    pub training_identity_sha256: String,
    pub raw_source: Request,
    pub diagnostics: Diagnostics,
    pub adoption_eligible: bool,
    pub empirical_accuracy_validated: bool,
    pub pricing_measure_permission: bool,
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        return Err("consumer macro bounded identity required".into());
    }
    Ok(())
}
fn identity(i: &Identity) -> Result<(), String> {
    label(&i.id)?;
    label(&i.revision)
}
fn row(values: &[f64], drivers: &[Driver]) -> Result<(), String> {
    if values.len() != drivers.len() {
        return Err("consumer macro driver dimension".into());
    }
    for (v, d) in values.iter().zip(drivers) {
        if !v.is_finite() || *v < d.raw_domain[0] || *v > d.raw_domain[1] {
            return Err("consumer macro out-of-domain value; no clipping".into());
        }
    }
    Ok(())
}
fn moments(
    rows: &[(String, Vec<f64>)],
    n: usize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Moments, String> {
    if rows.len() < 2 {
        return Err(
            "consumer macro requires at least two available rows in each chronological split"
                .into(),
        );
    }
    let mut mean = vec![0.; n];
    for (_, r) in rows {
        check()?;
        for (m, v) in mean.iter_mut().zip(r) {
            *m += v;
        }
    }
    for m in &mut mean {
        *m /= rows.len() as f64;
    }
    let mut cov = vec![vec![0.; n]; n];
    for (_, r) in rows {
        check()?;
        for i in 0..n {
            for j in 0..=i {
                cov[i][j] += (r[i] - mean[i]) * (r[j] - mean[j]);
            }
        }
    }
    let cov: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| cov[i.max(j)][i.min(j)] / (rows.len() - 1) as f64)
                .collect()
        })
        .collect();
    if cov.iter().flatten().any(|v| !v.is_finite()) {
        return Err("consumer macro covariance overflow".into());
    }
    Ok(Moments {
        row_ids: rows.iter().map(|r| r.0.clone()).collect(),
        mean_annualized_standardized_innovation: mean,
        sample_annual_covariance: cov,
    })
}
pub fn evaluate(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    crate::strict_json::serialized_size(r, 4 << 20)?;
    let asof = r.as_of.seconds()?;
    let split = r.history.training_end.seconds()?;
    let hold = r.history.holdout_end.seconds()?;
    if r.schema != SCHEMA
        || r.measure != CreditMeasure::Physical
        || r.drivers.len() < 2
        || r.drivers.len() > 16
        || r.scenarios.len() != 4
        || r.interval_ends.is_empty()
        || r.interval_ends.len() > 64
        || r.history.observations.len() > 4096
        || split >= hold
        || hold > asof
    {
        return Err("consumer macro schema/Physical dimensions/chronology".into());
    }
    for i in [
        &r.common_random_numbers,
        &r.policy,
        &r.history.source,
        &r.history.training_split,
        &r.history.holdout_split,
    ] {
        identity(i)?;
    }
    if r.history.training_split.id == r.history.holdout_split.id {
        return Err("consumer macro split identities must differ".into());
    }
    let mut names = HashSet::new();
    let mut coordinates = HashSet::new();
    let mut rate = None;
    for (i, d) in r.drivers.iter().enumerate() {
        check()?;
        label(&d.driver_id)?;
        label(&d.region_id)?;
        identity(&d.source)?;
        identity(&d.scaling_source)?;
        if !names.insert(&d.driver_id)
            || !coordinates.insert((d.metric, &d.region_id))
            || d.unit != d.metric.unit()
            || d.availability_lag_days > 36500
            || !d.innovation_scale.is_finite()
            || d.innovation_scale <= 0.
            || d.innovation_scale > 1e9
            || d.scaling_training_end.seconds()? > split
            || !d.raw_domain.iter().all(|v| v.is_finite())
            || d.raw_domain[0] >= d.raw_domain[1]
            || d.raw_domain[0] < -1e9
            || d.raw_domain[1] > 1e9
        {
            return Err("consumer macro definition/unit/frozen scale".into());
        }
        if d.metric == Metric::UsedVehicleIndex && d.raw_domain[0] <= 0.
            || d.metric == Metric::UnemploymentLevel
                && (d.raw_domain[0] < 0. || d.raw_domain[1] > 100.)
        {
            return Err("consumer macro metric domain".into());
        }
        if d.metric == Metric::ShortRate && rate.replace(i).is_some() {
            return Err("consumer macro exactly one conditional short-rate coordinate".into());
        }
    }
    let rate =
        rate.ok_or("consumer macro requires the shared conditional short-rate coordinate")?;
    let mut previous = asof;
    for end in &r.interval_ends {
        let t = end.seconds()?;
        if t <= previous || t - asof > 36600 * 86400 {
            return Err("consumer macro projection grid".into());
        }
        previous = t;
    }
    let mut train = Vec::new();
    let mut test = Vec::new();
    let mut cross = Vec::new();
    let mut unavailable = Vec::new();
    let mut ids = HashSet::new();
    let mut last: Option<&Observation> = None;
    for obs in &r.history.observations {
        check()?;
        label(&obs.row_id)?;
        let start = obs.start.seconds()?;
        let end = obs.end.seconds()?;
        row(&obs.start_values, &r.drivers)?;
        row(&obs.end_values, &r.drivers)?;
        if !ids.insert(&obs.row_id)
            || start >= end
            || end > hold
            || end - start > 36600 * 86400
            || obs.published_at.len() != r.drivers.len()
        {
            return Err("consumer macro observed interval/identity/coverage".into());
        }
        if let Some(p) = last {
            if p.end != obs.start
                || p.end_values
                    .iter()
                    .zip(&obs.start_values)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
            {
                return Err("consumer macro observed history must preserve complete contiguous level boundaries".into());
            }
        }
        last = Some(obs);
        let mut available = end;
        for (published, d) in obs.published_at.iter().zip(&r.drivers) {
            let t = published.seconds()?;
            if t < end {
                return Err("consumer macro publication precedes observation".into());
            }
            available = available.max(t.max(end + i64::from(d.availability_lag_days) * 86400));
        }
        if start < split && end > split {
            cross.push(obs.row_id.clone());
            continue;
        }
        let cutoff = if end <= split { split } else { hold };
        if available > cutoff {
            unavailable.push(obs.row_id.clone());
            continue;
        }
        let years = (end - start) as f64 / (365. * 86400.);
        let x = obs
            .start_values
            .iter()
            .zip(&obs.end_values)
            .zip(&r.drivers)
            .map(|((a, b), d)| (b - a) / (d.innovation_scale * years.sqrt()))
            .collect::<Vec<_>>();
        if x.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
            return Err("consumer macro innovation domain".into());
        }
        if end <= split {
            train.push((obs.row_id.clone(), x));
        } else {
            test.push((obs.row_id.clone(), x));
        }
    }
    let training = moments(&train, r.drivers.len(), check)?;
    let holdout = moments(&test, r.drivers.len(), check)?;
    let mut kinds = HashSet::new();
    let mut scenario_ids = HashSet::new();
    let baseline = r
        .scenarios
        .iter()
        .find(|s| s.kind == ScenarioKind::Baseline)
        .ok_or("consumer macro baseline missing")?;
    if baseline.paths.is_empty() || baseline.paths.len() > 16 {
        return Err("consumer macro shared path count".into());
    }
    let mut keys = HashSet::new();
    for p in &baseline.paths {
        if !keys.insert(p.path_key) {
            return Err("consumer macro duplicate shared path key".into());
        }
    }
    use crate::credit_path::DrawStream;
    let (stream_id, revision) = match &r.credit_draw_stream {
        DrawStream::CounterSplitmix64V1 {
            stream_id,
            stream_revision,
            ..
        } => (stream_id, stream_revision),
        DrawStream::Supplied {
            stream_id,
            stream_revision,
            paths,
        } => {
            if paths.len() != baseline.paths.len() {
                return Err("consumer macro exact supplied draw coverage".into());
            }
            let mut seen = HashSet::new();
            for p in paths {
                if !keys.contains(&p.path_key)
                    || !seen.insert(p.path_key)
                    || p.pairs.is_empty()
                    || p.pairs.len() > 128
                {
                    return Err("consumer macro supplied draw path/count".into());
                }
                for pair in &p.pairs {
                    if [pair.waiting_uniform, pair.destination_uniform]
                        .iter()
                        .any(|u| !u.is_finite() || *u <= 0. || *u >= 1.)
                    {
                        return Err("consumer macro uniform draw domain".into());
                    }
                }
            }
            (stream_id, stream_revision)
        }
    };
    if stream_id != &r.common_random_numbers.id || revision != &r.common_random_numbers.revision {
        return Err("consumer macro shared draw identity differs from raw draws".into());
    }
    for scenario in &r.scenarios {
        check()?;
        label(&scenario.scenario_id)?;
        identity(&scenario.source)?;
        identity(&scenario.conditional_rate_source)?;
        if !kinds.insert(scenario.kind)
            || !scenario_ids.insert(&scenario.scenario_id)
            || scenario.issued_at.seconds()? > asof
            || scenario.paths.len() != baseline.paths.len()
        {
            return Err("consumer macro scenario identity/availability/coverage".into());
        }
        for (path, base) in scenario.paths.iter().zip(&baseline.paths) {
            check()?;
            if path.path_key != base.path_key
                || path.values.len() != r.interval_ends.len()
                || path.short_rates_percent.len() != r.interval_ends.len()
            {
                return Err("consumer macro shared path/grid mismatch".into());
            }
            for (j, values) in path.values.iter().enumerate() {
                row(values, &r.drivers)?;
                if values[rate].to_bits() != path.short_rates_percent[j].to_bits() {
                    return Err(
                        "consumer macro short rate differs from supplied conditional parent".into(),
                    );
                }
                if matches!(
                    scenario.kind,
                    ScenarioKind::ParallelUp200 | ScenarioKind::ParallelDown200
                ) {
                    let base_rate = *base
                        .short_rates_percent
                        .get(j)
                        .ok_or("consumer macro missing base rate")?;
                    let bump = if scenario.kind == ScenarioKind::ParallelUp200 {
                        2.
                    } else {
                        -2.
                    };
                    if (base_rate + bump).to_bits() != path.short_rates_percent[j].to_bits() {
                        return Err("consumer macro parallel template must retain the supplied exact 200bp rate shift".into());
                    }
                }
            }
        }
    }
    let errors = holdout
        .mean_annualized_standardized_innovation
        .iter()
        .zip(&training.mean_annualized_standardized_innovation)
        .map(|(a, b)| a - b)
        .collect();
    check()?;
    let training_ids: HashSet<_> = training.row_ids.iter().collect();
    let training_rows: Vec<_> = r
        .history
        .observations
        .iter()
        .filter(|row| training_ids.contains(&row.row_id))
        .collect();
    let training_identity_sha256 = macro_identity::digest(
        &(
            SCHEMA,
            &r.drivers,
            &r.policy,
            &r.history.source,
            &r.history.training_split,
            r.history.training_end,
            r.history.data_kind,
            training_rows,
        ),
        check,
    )?;
    let out = ResultData {
        schema: SCHEMA,
        identity_policy: macro_identity::POLICY,
        source_sha256: macro_identity::digest(r, check)?,
        training_identity_sha256,
        raw_source: r.clone(),
        diagnostics: Diagnostics {
            training,
            holdout,
            holdout_mean_error_from_training: errors,
            excluded_cross_split: cross,
            excluded_unavailable_at_split: unavailable,
        },
        adoption_eligible: false,
        empirical_accuracy_validated: false,
        pricing_measure_permission: false,
    };
    crate::strict_json::serialized_size(&out, 8 << 20)?;
    check()?;
    Ok(out)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub column_name: String,
    pub driver_id: String,
}
impl ResultData {
    /// Bind every selected macro column to the same scenario/path. Borrower values stay intact.
    pub fn project_intervals(
        &self,
        spec: &behavior::Specification,
        template: &behavior::Projection,
        scenario_id: &str,
        path_key: u64,
        mappings: &[Mapping],
        check: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<Vec<DriverInterval>, String> {
        check()?;
        let r = &self.raw_source;
        crate::strict_json::serialized_size(template, 4 << 20)?;
        if self.identity_policy != macro_identity::POLICY
            || macro_identity::digest(r, check)? != self.source_sha256
        {
            return Err("consumer macro retained raw source changed".into());
        }
        if template.path_source.measure != CreditMeasure::Physical
            || template.path_source.origin_time != r.as_of
            || template.intervals.len() != r.interval_ends.len()
            || template.path_source.intervals.len() != r.interval_ends.len()
            || template.path_source.paths.len() != 1
            || template
                .path_source
                .paths
                .iter()
                .any(|p| p.path_key != path_key)
        {
            return Err(
                "consumer macro requires one shared Physical path and exact origin/grid".into(),
            );
        }
        let expected_draws = match &r.credit_draw_stream {
            crate::credit_path::DrawStream::Supplied {
                stream_id,
                stream_revision,
                paths,
            } => crate::credit_path::DrawStream::Supplied {
                stream_id: stream_id.clone(),
                stream_revision: stream_revision.clone(),
                paths: paths
                    .iter()
                    .filter(|p| p.path_key == path_key)
                    .cloned()
                    .collect(),
            },
            other => other.clone(),
        };
        if serde_json::to_vec(&expected_draws).map_err(|e| e.to_string())?
            != serde_json::to_vec(&template.path_source.draw_stream).map_err(|e| e.to_string())?
        {
            return Err("consumer macro actual shared draws differ".into());
        }
        let scenario = r
            .scenarios
            .iter()
            .find(|s| s.scenario_id == scenario_id)
            .ok_or("consumer macro unknown scenario")?;
        let path = scenario
            .paths
            .iter()
            .find(|p| p.path_key == path_key)
            .ok_or("consumer macro unknown path key")?;
        let mut mapping = HashMap::new();
        for m in mappings {
            label(&m.column_name)?;
            label(&m.driver_id)?;
            if mapping
                .insert(m.column_name.as_str(), m.driver_id.as_str())
                .is_some()
            {
                return Err("consumer macro duplicate column mapping".into());
            }
        }
        let macro_count = spec
            .columns
            .iter()
            .filter(|c| matches!(c.definition.role, FeatureRole::Macro))
            .count();
        if mapping.len() != macro_count {
            return Err("consumer macro exact macro-column coverage required".into());
        }
        let mut columns = Vec::new();
        for (i, c) in spec.columns.iter().enumerate() {
            if matches!(c.definition.role, FeatureRole::Macro) {
                let driver = mapping
                    .get(c.definition.name.as_str())
                    .ok_or("consumer macro missing column mapping")?;
                let (j, d) = r
                    .drivers
                    .iter()
                    .enumerate()
                    .find(|(_, d)| d.driver_id == *driver)
                    .ok_or("consumer macro unknown driver mapping")?;
                if d.metric.column() != Some(c.definition.name.as_str())
                    || d.availability_lag_days != c.availability_lag_days
                    || d.unit.name() != c.definition.unit
                {
                    return Err("consumer macro column metric/availability-lag mismatch".into());
                }
                columns.push((i, j));
            }
        }
        let mut result = template.intervals.clone();
        let mut previous = r.as_of;
        for (j, interval) in result.iter_mut().enumerate() {
            check()?;
            if interval.start != previous
                || interval.end != r.interval_ends[j]
                || interval.available_time > r.as_of
                || interval.covariates.len() != spec.columns.len()
                || ((interval.end.seconds()? - r.as_of.seconds()?) as f64 / (365. * 86400.))
                    .to_bits()
                    != template.path_source.intervals[j].end_year.to_bits()
            {
                return Err("consumer macro borrower/grid availability mismatch".into());
            }
            previous = interval.end;
            for (column, driver) in &columns {
                let value = path.values[j][*driver];
                let definition = &spec.columns[*column].definition;
                if value < definition.raw_lower || value > definition.raw_upper {
                    return Err("consumer macro value outside fitted column domain".into());
                }
                interval.covariates[*column] = value;
            }
            interval.available_time = interval.available_time.max(scenario.issued_at);
            interval.kind = DriverKind::SuppliedProjected;
            interval.source = Identity {
                id: format!("consumer-macro:{}", self.source_sha256),
                revision: scenario.source.revision.clone(),
            };
        }
        check()?;
        Ok(result)
    }
}
