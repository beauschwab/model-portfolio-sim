//! Interval-censored panel likelihood. Snapshot endpoints are never fabricated transition times.
use crate::cashflows::dated::validate_ordinal;
use crate::models::credit::calibration::exact::{DataKind, ObservationTime};
use crate::models::credit::operator::{compile, CreditOperatorRequest};
use crate::models::credit::transition::{CreditMeasure, CreditState, MAX_ROW_HAZARD};
use crate::validation::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "credit-snapshot-calibration-1";
pub const MAX_SNAPSHOTS: usize = 20_000;
pub const MAX_PARAMETERS: usize = 12;
pub const MAX_WORK: usize = 10_000_000;
const YEAR_SECONDS: f64 = 365.0 * 86_400.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub initial: f64,
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogLinearEdge {
    pub destination: CreditState,
    pub intercept: Parameter,
    pub covariate_coefficients: Vec<Parameter>,
    /// One coefficient for each nonreference level; term_levels[0] has exactly zero effect.
    pub term_coefficients: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RowModel {
    FixedRow {
        hazards_per_year: [f64; 5],
        assumption_id: String,
        rationale: String,
    },
    LogLinear {
        edges: Vec<LogLinearEdge>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionPolicy {
    Reject,
    ExcludeWithDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotInterval {
    pub snapshot_id: String,
    pub account_id: String,
    pub source_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub origin: CreditState,
    pub destination: CreditState,
    pub covariates: Vec<f64>,
    pub term_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedRecovery {
    pub time: ObservationTime,
    pub principal: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutcomeResolution {
    Complete { resolved_time: ObservationTime },
    RightCensored { censor_time: ObservationTime },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultOutcome {
    pub outcome_id: String,
    pub account_id: String,
    pub source_id: String,
    /// Exact last live origin observed at default; never inferred from an earlier snapshot.
    pub default_origin: CreditState,
    pub default_time: ObservationTime,
    pub pre_default_drawn: f64,
    pub pre_default_undrawn: f64,
    pub realized_ead: f64,
    pub recoveries: Vec<ObservedRecovery>,
    pub resolution: OutcomeResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizerPolicy {
    pub max_search_evaluations: usize,
    pub initial_mesh: f64,
    pub final_mesh: f64,
    pub objective_improvement_tolerance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotCalibrationRequest {
    pub schema: String,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub data_kind: DataKind,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub crossing_split_policy: ExclusionPolicy,
    pub censored_outcome_policy: ExclusionPolicy,
    pub covariate_names: Vec<String>,
    pub term_levels: Vec<String>,
    pub rows: [RowModel; 3],
    pub optimizer: OptimizerPolicy,
    pub snapshots: Vec<SnapshotInterval>,
    pub default_outcomes: Vec<DefaultOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimatedParameter {
    pub name: String,
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordProvenance {
    pub record_id: String,
    pub account_id: String,
    pub source_id: String,
    pub partition: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelDiagnostics {
    pub intervals: usize,
    pub log_likelihood: Option<f64>,
    pub impossible_intervals: usize,
    pub domain_rejected_intervals: usize,
    pub observed_start_counts: [usize; 3],
    pub endpoint_counts: [[usize; 5]; 3],
    pub predicted_endpoint_counts: [[f64; 5]; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityFit {
    pub complete_outcomes: usize,
    pub outcome_ids: Vec<String>,
    pub total_ead: f64,
    pub total_recovered_principal: f64,
    pub ead_weighted_realized_lgd: Option<f64>,
    pub total_pre_default_undrawn: f64,
    pub total_contingent_draw: f64,
    pub undrawn_weighted_realized_ccf: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotCalibrationResult {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub data_kind: DataKind,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub clock: String,
    pub declared_rows: [RowModel; 3],
    pub covariate_names: Vec<String>,
    pub term_levels: Vec<String>,
    /// Estimates are diagnostics until every numerical gate below passes. Observed data is still required.
    pub adoption_eligible: bool,
    pub convergence: String,
    pub final_mesh: f64,
    pub parameters: Vec<EstimatedParameter>,
    pub active_bound_parameters: Vec<String>,
    pub observed_information_eigenvalues: Option<Vec<f64>>,
    pub information_rank: usize,
    pub objective_evaluations: usize,
    pub work_units: usize,
    pub training: PanelDiagnostics,
    pub holdout: PanelDiagnostics,
    pub excluded_cross_split_snapshot_ids: Vec<String>,
    pub snapshot_provenance: Vec<RecordProvenance>,
    pub outcome_provenance: Vec<RecordProvenance>,
    pub training_severity: SeverityFit,
    pub holdout_severity: SeverityFit,
    pub training_severity_by_origin: [SeverityFit; 3],
    pub holdout_severity_by_origin: [SeverityFit; 3],
    pub excluded_outcome_ids: Vec<String>,
}

impl SnapshotCalibrationResult {
    /// Native projection of supplied final coefficients. Does not activate a model
    /// or turn numerical diagnostics into empirical review/approval evidence.
    pub fn project_hazards(
        &self,
        covariates: &[f64],
        term_level: &str,
    ) -> Result<[[f64; 5]; 3], String> {
        if self.schema != SCHEMA
            || self.measure != CreditMeasure::Physical
            || self.covariate_names.len() > 4
            || self.term_levels.is_empty()
            || self.term_levels.len() > 4
            || covariates.len() != self.covariate_names.len()
        {
            return Err("unsupported calibrated model projection contract".into());
        }
        let term = self
            .term_levels
            .iter()
            .position(|t| t == term_level)
            .ok_or("unknown projection term level")?;
        for x in covariates {
            range(*x, -10.0, 10.0)?;
        }
        for (i, row) in self.declared_rows.iter().enumerate() {
            match row {
                RowModel::FixedRow {
                    hazards_per_year, ..
                } => {
                    for x in hazards_per_year {
                        range(*x, 0.0, MAX_ROW_HAZARD)?;
                    }
                    if hazards_per_year[i] != 0.0 {
                        return Err("projection fixed row diagonal must be zero".into());
                    }
                }
                RowModel::LogLinear { edges } => {
                    if edges.is_empty() || edges.len() > 4 {
                        return Err("invalid projection edge count".into());
                    }
                    let mut destinations = HashSet::new();
                    for edge in edges {
                        if state(edge.destination) == i
                            || !destinations.insert(state(edge.destination))
                            || edge.covariate_coefficients.len() != covariates.len()
                            || edge.term_coefficients.len() + 1 != self.term_levels.len()
                        {
                            return Err("invalid projection edge dimensions/destinations".into());
                        }
                    }
                }
            }
        }
        let declared = parameters(
            &self.declared_rows,
            &self.covariate_names,
            &self.term_levels,
        );
        if declared.len() != self.parameters.len() || declared.len() > MAX_PARAMETERS {
            return Err(
                "final coefficient projection dimensions do not match declared model".into(),
            );
        }
        for (p, d) in self.parameters.iter().zip(&declared) {
            range(d.lower, -20.0, 20.0)?;
            range(d.upper, -20.0, 20.0)?;
            if d.lower >= d.upper || p.name != d.name || p.lower != d.lower || p.upper != d.upper {
                return Err("final coefficient projection identity/bounds mismatch".into());
            }
            range(p.value, d.lower, d.upper)?;
        }
        let values: Vec<f64> = self.parameters.iter().map(|p| p.value).collect();
        hazards(
            &self.declared_rows,
            &values,
            &Case {
                duration: 0.0,
                covariates: covariates.to_vec(),
                term,
                counts: [[0; 5]; 3],
                absorbing: 0,
            },
        )
        .ok_or("projected model exceeds the finite outgoing hazard domain".into())
    }
}

fn time(t: ObservationTime) -> Result<i64, String> {
    validate_ordinal(t.ordinal)?;
    if t.seconds_of_day >= 86_400 {
        return Err("invalid observation seconds_of_day".into());
    }
    Ok(i64::from(t.ordinal) * 86_400 + i64::from(t.seconds_of_day))
}
fn state(s: CreditState) -> usize {
    match s {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        CreditState::Default => 3,
        CreditState::Prepaid => 4,
    }
}
fn range(v: f64, l: f64, h: f64) -> Result<(), String> {
    if !v.is_finite() || v < l || v > h {
        Err(format!("value must be finite in [{l},{h}]"))
    } else {
        Ok(())
    }
}
fn label(v: &str, metadata: &mut usize) -> Result<(), String> {
    if v.is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err("metadata requires 1..256 control-free bytes".into());
    }
    *metadata = metadata.checked_add(v.len()).ok_or("metadata overflow")?;
    if *metadata > MAX_DATED_METADATA_BYTES {
        Err("metadata byte admission exceeded".into())
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct Case {
    duration: f64,
    covariates: Vec<f64>,
    term: usize,
    counts: [[usize; 5]; 3],
    absorbing: usize,
}
fn cases(rows: &[&SnapshotInterval], levels: &HashMap<&str, usize>) -> Result<Vec<Case>, String> {
    let mut lookup = HashMap::new();
    let mut result: Vec<Case> = Vec::new();
    for r in rows {
        let duration = (time(r.end)? - time(r.start)?) as f64 / YEAR_SECONDS;
        let term = levels[r.term_level.as_str()];
        let mut key = vec![duration.to_bits(), term as u64];
        key.extend(r.covariates.iter().map(|x| x.to_bits()));
        let idx = *lookup.entry(key).or_insert_with(|| {
            result.push(Case {
                duration,
                covariates: r.covariates.clone(),
                term,
                counts: [[0; 5]; 3],
                absorbing: 0,
            });
            result.len() - 1
        });
        let i = state(r.origin);
        if i < 3 {
            result[idx].counts[i][state(r.destination)] += 1;
        } else {
            result[idx].absorbing += 1;
        }
    }
    if result.len() > 512 {
        return Err(
            "at most 512 distinct interval/covariate/term groups per split are supported".into(),
        );
    }
    Ok(result)
}
fn parameters(
    rows: &[RowModel; 3],
    covariates: &[String],
    terms: &[String],
) -> Vec<EstimatedParameter> {
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if let RowModel::LogLinear { edges } = row {
            for edge in edges {
                let prefix = format!("origin_{i}_to_{}", state(edge.destination));
                let mut push = |name: String, p: &Parameter| {
                    out.push(EstimatedParameter {
                        name,
                        value: p.initial,
                        lower: p.lower,
                        upper: p.upper,
                    })
                };
                push(format!("{prefix}:intercept"), &edge.intercept);
                for (name, p) in covariates.iter().zip(&edge.covariate_coefficients) {
                    push(format!("{prefix}:covariate:{name}"), p);
                }
                for (name, p) in terms.iter().skip(1).zip(&edge.term_coefficients) {
                    push(format!("{prefix}:term:{name}"), p);
                }
            }
        }
    }
    out
}
fn hazards(rows: &[RowModel; 3], values: &[f64], case: &Case) -> Option<[[f64; 5]; 3]> {
    let mut q = [[0.0; 5]; 3];
    let mut n = 0;
    for (i, row) in rows.iter().enumerate() {
        match row {
            RowModel::FixedRow {
                hazards_per_year, ..
            } => q[i] = *hazards_per_year,
            RowModel::LogLinear { edges } => {
                for edge in edges {
                    let mut eta = values[n];
                    n += 1;
                    for x in &case.covariates {
                        eta += values[n] * x;
                        n += 1;
                    }
                    for term in 1..=edge.term_coefficients.len() {
                        if case.term == term {
                            eta += values[n];
                        }
                        n += 1;
                    }
                    let intensity = eta.exp();
                    if intensity == 0.0 {
                        return None;
                    }
                    q[i][state(edge.destination)] = intensity;
                }
            }
        }
        if q[i].iter().any(|x| !x.is_finite()) || q[i].iter().sum::<f64>() > MAX_ROW_HAZARD {
            return None;
        }
    }
    Some(q)
}

struct Objective<'a> {
    rows: &'a [RowModel; 3],
    evaluations: usize,
    work: usize,
}
impl Objective<'_> {
    fn diagnostics(&mut self, values: &[f64], cases: &[Case]) -> Result<PanelDiagnostics, String> {
        self.evaluations += 1;
        if self.evaluations > 2400 {
            return Err("total objective evaluation limit exceeded".into());
        }
        let mut out = PanelDiagnostics {
            intervals: 0,
            log_likelihood: Some(0.0),
            impossible_intervals: 0,
            domain_rejected_intervals: 0,
            observed_start_counts: [0; 3],
            endpoint_counts: [[0; 5]; 3],
            predicted_endpoint_counts: [[0.0; 5]; 3],
        };
        for case in cases {
            let Some(q) = hazards(self.rows, values, case) else {
                out.log_likelihood = None;
                out.domain_rejected_intervals +=
                    case.counts.iter().flatten().sum::<usize>() + case.absorbing;
                out.intervals += case.absorbing;
                for (i, counts) in case.counts.iter().enumerate() {
                    let count: usize = counts.iter().sum();
                    out.intervals += count;
                    out.observed_start_counts[i] += count;
                    for (j, &observed) in counts.iter().enumerate() {
                        out.endpoint_counts[i][j] += observed;
                    }
                }
                continue;
            };
            let max = q.iter().map(|r| r.iter().sum::<f64>()).fold(0.0, f64::max);
            let charge = 128 * ((max * case.duration / 8.0).ceil() as usize).max(1);
            self.work = self.work.checked_add(charge).ok_or("work overflow")?;
            if self.work > MAX_WORK {
                return Err("panel calibration uniformization work admission exceeded".into());
            }
            let op = compile(&CreditOperatorRequest {
                schema: crate::models::credit::operator::SCHEMA.into(),
                measure: CreditMeasure::Physical,
                duration_years: case.duration,
                transition_hazards_per_year: q,
                accrual_rates: [0.0; 3],
            })?;
            out.intervals += case.absorbing;
            for (i, counts) in case.counts.iter().enumerate() {
                let count: usize = counts.iter().sum();
                out.intervals += count;
                out.observed_start_counts[i] += count;
                let p = op.transition();
                let probs = [
                    p[i][0],
                    p[i][1],
                    p[i][2],
                    p[i][3] + p[i][4] + p[i][5],
                    p[i][6],
                ];
                for (j, &observed) in counts.iter().enumerate() {
                    out.endpoint_counts[i][j] += observed;
                    out.predicted_endpoint_counts[i][j] += count as f64 * probs[j];
                    if observed > 0 {
                        if probs[j] <= 0.0 {
                            out.impossible_intervals += observed;
                            out.log_likelihood = None;
                        } else if let Some(ll) = &mut out.log_likelihood {
                            *ll += observed as f64 * probs[j].ln();
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    fn loss(&mut self, values: &[f64], cases: &[Case]) -> Result<f64, String> {
        Ok(self
            .diagnostics(values, cases)?
            .log_likelihood
            .map_or(f64::INFINITY, |x| -x))
    }
}

fn eigenvalues(mut a: Vec<Vec<f64>>) -> Vec<f64> {
    let n = a.len();
    for _ in 0..(100 * n * n) {
        let mut p = 0;
        let mut q = 0;
        let mut max = 0.0;
        for (i, row) in a.iter().enumerate() {
            for (j, &v) in row.iter().enumerate().skip(i + 1) {
                if v.abs() > max {
                    max = v.abs();
                    p = i;
                    q = j;
                }
            }
        }
        if max < 1e-10 {
            break;
        }
        let angle = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (c, s) = (angle.cos(), angle.sin());
        let (app, aqq, apq) = (a[p][p], a[q][q], a[p][q]);
        let mut mirrored = Vec::with_capacity(n);
        for (k, row) in a.iter_mut().enumerate() {
            if k != p && k != q {
                let (akp, akq) = (row[p], row[q]);
                row[p] = c * akp - s * akq;
                row[q] = s * akp + c * akq;
                mirrored.push((k, row[p], row[q]));
            }
        }
        for (k, pk, qk) in mirrored {
            a[p][k] = pk;
            a[q][k] = qk;
        }
        a[p][p] = c * c * app - 2.0 * c * s * apq + s * s * aqq;
        a[q][q] = s * s * app + 2.0 * c * s * apq + c * c * aqq;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
    }
    let mut out: Vec<f64> = a.iter().enumerate().map(|(i, r)| r[i]).collect();
    out.sort_by(f64::total_cmp);
    out
}
fn information(
    objective: &mut Objective<'_>,
    values: &[f64],
    params: &[EstimatedParameter],
    cases: &[Case],
) -> Result<Option<Vec<f64>>, String> {
    let n = values.len();
    let h = 1e-3;
    if values
        .iter()
        .zip(params)
        .any(|(x, p)| *x - h <= p.lower || *x + h >= p.upper)
    {
        return Ok(None);
    }
    let base = objective.loss(values, cases)?;
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        let mut v = values.to_vec();
        v[i] += h;
        let plus = objective.loss(&v, cases)?;
        v[i] -= 2.0 * h;
        let minus = objective.loss(&v, cases)?;
        matrix[i][i] = (plus - 2.0 * base + minus) / (h * h);
        for j in 0..i {
            v = values.to_vec();
            v[i] += h;
            v[j] += h;
            let pp = objective.loss(&v, cases)?;
            v[j] -= 2.0 * h;
            let pm = objective.loss(&v, cases)?;
            v[i] -= 2.0 * h;
            let mm = objective.loss(&v, cases)?;
            v[j] += 2.0 * h;
            let mp = objective.loss(&v, cases)?;
            let x = (pp - pm + mm - mp) / (4.0 * h * h);
            matrix[i][j] = x;
            matrix[j][i] = x;
        }
    }
    if matrix.iter().flatten().any(|x| !x.is_finite()) {
        return Ok(None);
    }
    Ok(Some(eigenvalues(matrix)))
}
fn severity(rows: &[&DefaultOutcome]) -> SeverityFit {
    let mut out = SeverityFit {
        complete_outcomes: rows.len(),
        outcome_ids: Vec::new(),
        total_ead: 0.0,
        total_recovered_principal: 0.0,
        ead_weighted_realized_lgd: None,
        total_pre_default_undrawn: 0.0,
        total_contingent_draw: 0.0,
        undrawn_weighted_realized_ccf: None,
    };
    for r in rows {
        out.outcome_ids.push(r.outcome_id.clone());
        out.total_ead += r.realized_ead;
        out.total_recovered_principal += r.recoveries.iter().map(|x| x.principal).sum::<f64>();
        out.total_pre_default_undrawn += r.pre_default_undrawn;
        out.total_contingent_draw += r.realized_ead - r.pre_default_drawn;
    }
    if out.total_ead > 0.0 {
        out.ead_weighted_realized_lgd =
            Some((out.total_ead - out.total_recovered_principal) / out.total_ead);
    }
    if out.total_pre_default_undrawn > 0.0 {
        out.undrawn_weighted_realized_ccf =
            Some(out.total_contingent_draw / out.total_pre_default_undrawn);
    }
    out
}

pub fn fit(request: &SnapshotCalibrationRequest) -> Result<SnapshotCalibrationResult, String> {
    if request.schema != SCHEMA {
        return Err("unsupported snapshot calibration schema".into());
    }
    let mut metadata = 0;
    for v in [
        &request.monetary_unit,
        &request.source_id,
        &request.source_revision,
        &request.population_id,
        &request.model_policy_id,
        &request.training_split_id,
        &request.holdout_split_id,
    ] {
        label(v, &mut metadata)?;
    }
    if request.training_split_id == request.holdout_split_id {
        return Err("train and holdout split identities must differ".into());
    }
    let (start, split, end) = (
        time(request.train_start)?,
        time(request.train_end)?,
        time(request.holdout_end)?,
    );
    if !(start < split && split < end) || (end - start) as f64 > 100.0 * YEAR_SECONDS {
        return Err("invalid chronological windows; total <=100 ACT/365 years".into());
    }
    if request.snapshots.is_empty()
        || request.snapshots.len() > MAX_SNAPSHOTS
        || request.default_outcomes.len() > MAX_SNAPSHOTS
        || request.covariate_names.len() > 4
        || request.term_levels.is_empty()
        || request.term_levels.len() > 4
    {
        return Err("snapshot/outcome/covariate/term admission exceeded".into());
    }
    let mut unique = HashSet::new();
    for v in &request.covariate_names {
        label(v, &mut metadata)?;
        if !unique.insert(v) {
            return Err("duplicate covariate names".into());
        }
    }
    unique.clear();
    let mut levels = HashMap::new();
    for (i, v) in request.term_levels.iter().enumerate() {
        label(v, &mut metadata)?;
        if levels.insert(v.as_str(), i).is_some() {
            return Err("duplicate term levels".into());
        }
    }
    for (i, row) in request.rows.iter().enumerate() {
        match row {
            RowModel::FixedRow {
                hazards_per_year,
                assumption_id,
                rationale,
            } => {
                label(assumption_id, &mut metadata)?;
                label(rationale, &mut metadata)?;
                for v in hazards_per_year {
                    range(*v, 0.0, MAX_ROW_HAZARD)?;
                }
                if hazards_per_year[i] != 0.0
                    || hazards_per_year.iter().sum::<f64>() > MAX_ROW_HAZARD
                {
                    return Err("fixed row needs zero diagonal and bounded outgoing hazard".into());
                }
            }
            RowModel::LogLinear { edges } => {
                if edges.is_empty() || edges.len() > 4 {
                    return Err("estimated origin requires 1..4 declared edges".into());
                }
                let mut dests = HashSet::new();
                for edge in edges {
                    let d = state(edge.destination);
                    if d == i || !dests.insert(d) {
                        return Err("duplicate/diagonal estimated edge".into());
                    }
                    if edge.covariate_coefficients.len() != request.covariate_names.len()
                        || edge.term_coefficients.len() + 1 != request.term_levels.len()
                    {
                        return Err(
                            "coefficient dimensions must match named covariates/terms".into()
                        );
                    }
                    for p in std::iter::once(&edge.intercept)
                        .chain(&edge.covariate_coefficients)
                        .chain(&edge.term_coefficients)
                    {
                        range(p.lower, -20.0, 20.0)?;
                        range(p.upper, -20.0, 20.0)?;
                        range(p.initial, p.lower, p.upper)?;
                        if p.lower >= p.upper {
                            return Err("free coefficient needs distinct finite bounds".into());
                        }
                    }
                }
            }
        }
    }
    let mut params = parameters(
        &request.rows,
        &request.covariate_names,
        &request.term_levels,
    );
    if params.len() > MAX_PARAMETERS {
        return Err("at most 12 free log-linear parameters are supported".into());
    }
    let opt = &request.optimizer;
    if opt.max_search_evaluations < 1 || opt.max_search_evaluations > 2000 {
        return Err("search evaluations must be 1..2000".into());
    }
    range(opt.initial_mesh, 1e-4, 2.0)?;
    range(opt.final_mesh, 1e-7, 1e-2)?;
    range(opt.objective_improvement_tolerance, 0.0, 1e-6)?;
    if opt.initial_mesh < opt.final_mesh {
        return Err("initial mesh must be >= final mesh".into());
    }
    let mut training = Vec::new();
    let mut holdout = Vec::new();
    let mut excluded = Vec::new();
    let mut account_rows: HashMap<&str, Vec<&SnapshotInterval>> = HashMap::new();
    unique.clear();
    for r in &request.snapshots {
        for v in [&r.snapshot_id, &r.account_id, &r.source_id] {
            label(v, &mut metadata)?;
        }
        if !unique.insert(&r.snapshot_id) {
            return Err("snapshot IDs must be unique".into());
        }
        let (a, b) = (time(r.start)?, time(r.end)?);
        if a < start || b > end || a >= b || (b - a) as f64 > YEAR_SECONDS {
            return Err(
                "snapshot interval must be within windows and duration in (0,1] ACT/365 years"
                    .into(),
            );
        }
        if r.covariates.len() != request.covariate_names.len()
            || !levels.contains_key(r.term_level.as_str())
        {
            return Err("unknown term level or covariate dimension".into());
        }
        for x in &r.covariates {
            range(*x, -10.0, 10.0)?;
        }
        if state(r.origin) >= 3 && r.origin != r.destination {
            return Err("absorbing snapshot state cannot transition".into());
        }
        account_rows.entry(&r.account_id).or_default().push(r);
        if a < split && b > split {
            match request.crossing_split_policy {
                ExclusionPolicy::Reject => {
                    return Err(
                        "snapshot interval crosses training split; boundary state is unknown"
                            .into(),
                    )
                }
                ExclusionPolicy::ExcludeWithDiagnostics => excluded.push(r.snapshot_id.clone()),
            }
        } else if b <= split {
            training.push(r);
        } else {
            holdout.push(r);
        }
    }
    for rows in account_rows.values_mut() {
        rows.sort_by_key(|r| r.start);
        for pair in rows.windows(2) {
            if pair[0].end != pair[1].start || pair[0].destination != pair[1].origin {
                return Err(
                    "each account panel must be contiguous with matching endpoint states".into(),
                );
            }
        }
    }
    if training.is_empty() {
        return Err("training snapshot intervals are required".into());
    }
    let train_cases = cases(&training, &levels)?;
    let holdout_cases = cases(&holdout, &levels)?;
    for (i, row) in request.rows.iter().enumerate() {
        if matches!(row, RowModel::LogLinear { .. })
            && !training.iter().any(|r| state(r.origin) == i)
        {
            return Err(format!(
                "unobserved training origin {i} requires explicit fixed-row policy"
            ));
        }
    }
    let mut values: Vec<f64> = params.iter().map(|p| p.value).collect();
    let mut objective = Objective {
        rows: &request.rows,
        evaluations: 0,
        work: 0,
    };
    let mut loss = objective.loss(&values, &train_cases)?;
    if !loss.is_finite() {
        return Err("initial model makes observed training intervals impossible or exceeds row hazard bounds".into());
    }
    let mut mesh = opt.initial_mesh;
    let mut converged = values.is_empty();
    while !converged && objective.evaluations < opt.max_search_evaluations {
        let mut improved = false;
        let mut complete = true;
        for i in 0..values.len() {
            let current = values[i];
            let mut best = current;
            let mut best_loss = loss;
            for direction in [-1.0, 1.0] {
                if objective.evaluations >= opt.max_search_evaluations {
                    complete = false;
                    break;
                }
                let trial = (current + direction * mesh)
                    .max(params[i].lower)
                    .min(params[i].upper);
                if trial == current {
                    continue;
                }
                values[i] = trial;
                let trial_loss = objective.loss(&values, &train_cases)?;
                if trial_loss + opt.objective_improvement_tolerance < best_loss {
                    best = trial;
                    best_loss = trial_loss;
                }
            }
            values[i] = best;
            if best != current {
                improved = true;
                loss = best_loss;
            }
        }
        if !improved && complete {
            if mesh <= opt.final_mesh {
                converged = true;
            } else {
                mesh = (mesh * 0.5).max(opt.final_mesh);
            }
        }
    }
    for (p, x) in params.iter_mut().zip(&values) {
        p.value = *x;
    }
    let active: Vec<String> = params
        .iter()
        .filter(|p| p.value - p.lower <= opt.final_mesh || p.upper - p.value <= opt.final_mesh)
        .map(|p| p.name.clone())
        .collect();
    let eigen = if converged {
        information(&mut objective, &values, &params, &train_cases)?
    } else {
        None
    };
    let scale = eigen
        .as_ref()
        .map_or(0.0, |e| e.iter().map(|x| x.abs()).fold(0.0, f64::max));
    let rank = eigen.as_ref().map_or(0, |e| {
        e.iter().filter(|x| **x > 1e-5 * scale.max(1.0)).count()
    });
    let train_diag = objective.diagnostics(&values, &train_cases)?;
    let hold_diag = objective.diagnostics(&values, &holdout_cases)?;
    let mut train_outcomes = Vec::new();
    let mut hold_outcomes = Vec::new();
    let mut excluded_outcomes = Vec::new();
    let mut outcome_provenance = Vec::new();
    let mut recovery_count = 0usize;
    let mut default_accounts = HashSet::new();
    unique.clear();
    for r in &request.default_outcomes {
        for v in [&r.outcome_id, &r.account_id, &r.source_id] {
            label(v, &mut metadata)?;
        }
        if !unique.insert(&r.outcome_id) {
            return Err("outcome IDs must be unique".into());
        }
        if !default_accounts.insert(&r.account_id) || state(r.default_origin) >= 3 {
            return Err("default outcomes require unique account IDs and an exactly observed live default origin".into());
        }
        let d = time(r.default_time)?;
        if d < start || d > end {
            return Err("default outcomes must lie within study windows".into());
        }
        for v in [r.pre_default_drawn, r.pre_default_undrawn, r.realized_ead] {
            range(v, 0.0, MAX_MONETARY_AMOUNT)?;
        }
        range(
            r.pre_default_drawn + r.pre_default_undrawn,
            0.0,
            MAX_MONETARY_AMOUNT,
        )?;
        if r.realized_ead < r.pre_default_drawn
            || r.realized_ead > r.pre_default_drawn + r.pre_default_undrawn
        {
            return Err(
                "realized EAD must be drawn plus a bounded observed contingent draw".into(),
            );
        }
        recovery_count = recovery_count
            .checked_add(r.recoveries.len())
            .ok_or("recovery history overflow")?;
        if r.recoveries.len() > 4096 || recovery_count > 200_000 {
            return Err("recovery history admission exceeded".into());
        }
        let (resolved, censored) = match r.resolution {
            OutcomeResolution::Complete { resolved_time } => (time(resolved_time)?, false),
            OutcomeResolution::RightCensored { censor_time } => (time(censor_time)?, true),
        };
        if resolved < d || resolved > end {
            return Err(
                "resolution/censor clock must follow default and not exceed study end".into(),
            );
        }
        let mut previous = d;
        let mut recovered = 0.0;
        for cash in &r.recoveries {
            range(cash.principal, 0.0, MAX_MONETARY_AMOUNT)?;
            let t = time(cash.time)?;
            if t < previous || t > resolved {
                return Err(
                    "recovery cash clocks must be ordered within default/resolution".into(),
                );
            }
            previous = t;
            recovered += cash.principal;
        }
        if recovered > r.realized_ead {
            return Err("principal recovery cannot exceed realized EAD".into());
        }
        let partition = if censored {
            match request.censored_outcome_policy {
                ExclusionPolicy::Reject => {
                    return Err(
                        "censored default recovery cannot enter a complete-outcome fit".into(),
                    )
                }
                ExclusionPolicy::ExcludeWithDiagnostics => {
                    excluded_outcomes.push(r.outcome_id.clone());
                    "excluded_right_censored"
                }
            }
        } else if d <= split && resolved <= split {
            train_outcomes.push(r);
            "training"
        } else if d > split {
            hold_outcomes.push(r);
            "holdout"
        } else {
            excluded_outcomes.push(r.outcome_id.clone());
            "excluded_training_default_resolved_after_split"
        };
        outcome_provenance.push(RecordProvenance {
            record_id: r.outcome_id.clone(),
            account_id: r.account_id.clone(),
            source_id: r.source_id.clone(),
            partition: partition.into(),
        });
    }
    let train_severity = severity(&train_outcomes);
    let hold_severity = severity(&hold_outcomes);
    let training_severity_by_origin = std::array::from_fn(|i| {
        severity(
            &train_outcomes
                .iter()
                .copied()
                .filter(|r| state(r.default_origin) == i)
                .collect::<Vec<_>>(),
        )
    });
    let holdout_severity_by_origin = std::array::from_fn(|i| {
        severity(
            &hold_outcomes
                .iter()
                .copied()
                .filter(|r| state(r.default_origin) == i)
                .collect::<Vec<_>>(),
        )
    });
    for s in [&train_severity, &hold_severity] {
        for v in [
            s.total_ead,
            s.total_recovered_principal,
            s.total_pre_default_undrawn,
            s.total_contingent_draw,
        ] {
            range(v, 0.0, MAX_MONETARY_AMOUNT)?;
        }
    }
    let eligible = converged
        && active.is_empty()
        && rank == params.len()
        && matches!(request.data_kind, DataKind::Observed);
    let snapshot_provenance = request
        .snapshots
        .iter()
        .map(|r| RecordProvenance {
            record_id: r.snapshot_id.clone(),
            account_id: r.account_id.clone(),
            source_id: r.source_id.clone(),
            partition: if r.start < request.train_end && r.end > request.train_end {
                "excluded_cross_split"
            } else if r.end <= request.train_end {
                "training"
            } else {
                "holdout"
            }
            .into(),
        })
        .collect();
    Ok(SnapshotCalibrationResult {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        monetary_unit: request.monetary_unit.clone(),
        source_id: request.source_id.clone(),
        source_revision: request.source_revision.clone(),
        population_id: request.population_id.clone(),
        model_policy_id: request.model_policy_id.clone(),
        training_split_id: request.training_split_id.clone(),
        holdout_split_id: request.holdout_split_id.clone(),
        data_kind: request.data_kind,
        train_start: request.train_start,
        train_end: request.train_end,
        holdout_end: request.holdout_end,
        clock: "act_365_elapsed_seconds".into(),
        declared_rows: request.rows.clone(),
        covariate_names: request.covariate_names.clone(),
        term_levels: request.term_levels.clone(),
        adoption_eligible: eligible,
        convergence: if converged {
            "coordinate_mesh_converged"
        } else {
            "search_budget_exhausted"
        }
        .into(),
        final_mesh: mesh,
        parameters: params,
        active_bound_parameters: active,
        observed_information_eigenvalues: eigen,
        information_rank: rank,
        objective_evaluations: objective.evaluations,
        work_units: objective.work,
        training: train_diag,
        holdout: hold_diag,
        excluded_cross_split_snapshot_ids: excluded,
        snapshot_provenance,
        outcome_provenance,
        training_severity: train_severity,
        holdout_severity: hold_severity,
        training_severity_by_origin,
        holdout_severity_by_origin,
        excluded_outcome_ids: excluded_outcomes,
    })
}
