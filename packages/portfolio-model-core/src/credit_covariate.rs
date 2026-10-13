//! Exact-spell physical competing hazards with frozen supplied feature transforms.
//! Chronological holdout contributes diagnostics only; projections never convert P to Q.
use crate::cashflow::validate_ordinal;
use crate::credit::{CreditMeasure, CreditState, MAX_ROW_HAZARD};
use crate::credit_accounting::Identity;
use crate::credit_calibration::{DataKind, ObservationMode, ObservationTime};
use crate::limits::MAX_DATED_METADATA_BYTES;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

pub const SCHEMA: &str = "credit-covariate-1";
pub const MAX_SEGMENTS: usize = 50_000;
pub const MAX_PARAMETERS: usize = 24;
pub const MAX_WORK: usize = 20_000_000;
const YEAR_SECONDS: f64 = 365.0 * 86400.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureRole {
    Borrower,
    Macro,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub role: FeatureRole,
    pub unit: String,
    pub offset: f64,
    pub scale: f64,
    pub raw_lower: f64,
    pub raw_upper: f64,
    pub scaling_review: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub initial: f64,
    pub lower: f64,
    pub upper: f64,
    pub ridge_weight: f64,
    pub ridge_center: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub destination: CreditState,
    pub intercept: Parameter,
    pub covariate_coefficients: Vec<Parameter>,
    pub term_coefficients: Vec<Parameter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OriginModel {
    FixedRow {
        hazards_per_year: [f64; 5],
        review: Identity,
        rationale: String,
    },
    LogLinear {
        edges: Vec<Edge>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Ending {
    Transition { destination: CreditState },
    CovariateBoundary {},
    RightCensored {},
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub segment_id: String,
    pub account_id: String,
    pub source: Identity,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub origin: CreditState,
    pub ending: Ending,
    pub available_time: ObservationTime,
    pub covariates: Vec<f64>,
    pub term_level: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Optimizer {
    pub max_iterations: usize,
    pub max_backtracks: usize,
    pub timeout_ms: u64,
    pub projected_gradient_tolerance: f64,
    pub relative_information_tolerance: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CovariateFitRequest {
    pub schema: String,
    pub measure: CreditMeasure,
    pub data_kind: DataKind,
    pub observation_mode: ObservationMode,
    pub population: Identity,
    pub model_policy: Identity,
    pub training_source: Identity,
    pub holdout_source: Identity,
    pub training_split: Identity,
    pub holdout_split: Identity,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub columns: Vec<Column>,
    pub term_levels: Vec<String>,
    pub rows: [OriginModel; 3],
    pub optimizer: Optimizer,
    pub segments: Vec<Segment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimatedParameter {
    pub name: String,
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
    pub ridge_weight: f64,
    pub ridge_center: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedEdge {
    pub destination: CreditState,
    pub intercept: EstimatedParameter,
    pub covariate_coefficients: Vec<EstimatedParameter>,
    pub term_coefficients: Vec<EstimatedParameter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedRow {
    FixedRow {
        hazards_per_year: [f64; 5],
        review: Identity,
        rationale: String,
    },
    LogLinear {
        edges: Vec<ResolvedEdge>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FittedModel {
    pub schema: String,
    pub measure: CreditMeasure,
    pub data_kind: DataKind,
    pub population: Identity,
    pub model_policy: Identity,
    pub training_source: Identity,
    pub training_split: Identity,
    pub columns: Vec<Column>,
    pub term_levels: Vec<String>,
    pub rows: [ResolvedRow; 3],
    pub training_digest_sha256: String,
    pub numerical_eligible: bool,
    pub adoption_eligible: bool,
    /// Digest of this normalized typed model with the hash field set to the empty string.
    pub coefficient_identity_sha256: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub segments: usize,
    pub exposure_years_by_origin: [f64; 3],
    pub event_counts: [[u64; 5]; 3],
    pub expected_event_counts: [[f64; 5]; 3],
    pub log_likelihood: Option<f64>,
    pub impossible_events: usize,
    pub right_censored_segments: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct CovariateFitResult {
    pub schema: String,
    pub model: FittedModel,
    pub holdout_source: Identity,
    pub holdout_split: Identity,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub training: Diagnostics,
    pub holdout: Diagnostics,
    pub convergence: String,
    pub projected_gradient_norm: f64,
    pub iterations: usize,
    pub objective_evaluations: usize,
    pub work_units: usize,
    pub unpenalized_information_eigenvalues: Vec<f64>,
    pub information_rank: usize,
    pub free_parameters: usize,
    pub active_bound_parameters: Vec<String>,
    pub eligibility_reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionUse {
    Adoption,
    AnalyticalOnly,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverKind {
    SuppliedObserved,
    SuppliedProjected,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriverInterval {
    pub interval_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub available_time: ObservationTime,
    pub kind: DriverKind,
    pub source: Identity,
    pub covariates: Vec<f64>,
    pub term_level: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CovariateProjectionRequest {
    pub schema: String,
    pub measure: CreditMeasure,
    pub model: FittedModel,
    pub model_revision: Identity,
    pub review: Identity,
    pub use_policy: ProjectionUse,
    pub position_id: String,
    pub scenario_id: String,
    pub origin_time: ObservationTime,
    pub end_time: ObservationTime,
    pub timeout_ms: u64,
    pub intervals: Vec<DriverInterval>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectedInterval {
    pub interval_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub end_year: f64,
    pub source: Identity,
    pub kind: DriverKind,
    pub hazards_per_year: [[f64; 5]; 3],
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectedCreditHazards {
    pub schema: String,
    pub measure: CreditMeasure,
    pub position_id: String,
    pub scenario_id: String,
    pub model_revision: Identity,
    pub review: Identity,
    pub coefficient_identity_sha256: String,
    pub adoption_output: bool,
    pub analytical_only: bool,
    pub intervals: Vec<ProjectedInterval>,
    pub work_units: usize,
}
struct Budget {
    start: Instant,
    timeout: Duration,
    work: usize,
}
impl Budget {
    fn new(timeout_ms: u64) -> Result<Self, String> {
        if !(1..=3_600_000).contains(&timeout_ms) {
            return Err("timeout_ms outside bounded domain".into());
        }
        Ok(Self {
            start: Instant::now(),
            timeout: Duration::from_millis(timeout_ms),
            work: 0,
        })
    }
    fn charge(&mut self, n: usize) -> Result<(), String> {
        self.work = self.work.checked_add(n).ok_or("covariate work overflow")?;
        if self.work > MAX_WORK {
            return Err("covariate work admission exceeded".into());
        }
        if self.start.elapsed() > self.timeout {
            return Err("COMPUTE_DEADLINE_EXCEEDED".into());
        }
        Ok(())
    }
}
fn text(s: &str, bytes: &mut usize) -> Result<(), String> {
    *bytes = bytes.checked_add(s.len()).ok_or("metadata overflow")?;
    if s.trim().is_empty()
        || s.len() > 256
        || s.chars().any(char::is_control)
        || *bytes > MAX_DATED_METADATA_BYTES
    {
        Err("covariate metadata admission exceeded".into())
    } else {
        Ok(())
    }
}
fn identity(id: &Identity, bytes: &mut usize) -> Result<(), String> {
    text(&id.id, bytes)?;
    text(&id.revision, bytes)
}
fn sec(t: ObservationTime) -> Result<i64, String> {
    validate_ordinal(t.ordinal)?;
    if t.seconds_of_day >= 86400 {
        return Err("invalid seconds_of_day".into());
    }
    Ok(i64::from(t.ordinal) * 86400 + i64::from(t.seconds_of_day))
}
fn index(s: CreditState) -> usize {
    match s {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        CreditState::Default => 3,
        CreditState::Prepaid => 4,
    }
}
fn finite(x: f64) -> Result<(), String> {
    if x.is_finite() {
        Ok(())
    } else {
        Err("nonfinite covariate/parameter".into())
    }
}
fn validate_columns(columns: &[Column], terms: &[String], bytes: &mut usize) -> Result<(), String> {
    if columns.len() > 8 || terms.is_empty() || terms.len() > 32 {
        return Err("feature/term admission exceeded".into());
    }
    let mut names = HashSet::new();
    for c in columns {
        text(&c.name, bytes)?;
        text(&c.unit, bytes)?;
        identity(&c.scaling_review, bytes)?;
        if !names.insert(&c.name) {
            return Err("duplicate column".into());
        }
        for v in [c.offset, c.scale, c.raw_lower, c.raw_upper] {
            finite(v)?;
        }
        if c.scale <= 0.0
            || c.raw_lower > c.raw_upper
            || ((c.raw_lower - c.offset) / c.scale).abs() > 1e6
            || ((c.raw_upper - c.offset) / c.scale).abs() > 1e6
        {
            return Err("invalid frozen feature scaling/domain".into());
        }
    }
    let mut names = HashSet::new();
    for t in terms {
        text(t, bytes)?;
        if !names.insert(t) {
            return Err("duplicate term level".into());
        }
    }
    Ok(())
}
fn feature(
    columns: &[Column],
    terms: &[String],
    values: &[f64],
    term: &str,
) -> Result<(Vec<f64>, usize), String> {
    if values.len() != columns.len() {
        return Err("feature column coverage mismatch".into());
    }
    let mut z = Vec::with_capacity(values.len());
    for (c, &x) in columns.iter().zip(values) {
        finite(x)?;
        if x < c.raw_lower || x > c.raw_upper {
            return Err("feature outside declared projection/training domain".into());
        }
        let v = (x - c.offset) / c.scale;
        finite(v)?;
        z.push(v);
    }
    Ok((
        z,
        terms
            .iter()
            .position(|t| t == term)
            .ok_or("unknown term level")?,
    ))
}
#[derive(Clone, Serialize)]
struct Case {
    segment_id: String,
    account_id: String,
    source: Identity,
    start_seconds: i64,
    end_seconds: i64,
    available_seconds: i64,
    origin: usize,
    destination: Option<usize>,
    z: Vec<f64>,
    raw_covariates: Vec<f64>,
    term: usize,
    right_censored: bool,
}
impl Case {
    fn years(&self) -> f64 {
        (self.end_seconds - self.start_seconds) as f64 / YEAR_SECONDS
    }
}
#[derive(Clone)]
struct P {
    name: String,
    definition: Parameter,
}
struct E {
    destination: usize,
    parameters: Vec<usize>,
}
struct R {
    fixed: Option<[f64; 5]>,
    edges: Vec<E>,
}
fn parameter(p: &Parameter, name: String, ps: &mut Vec<P>) -> Result<usize, String> {
    for x in [p.initial, p.lower, p.upper, p.ridge_weight, p.ridge_center] {
        finite(x)?;
    }
    if p.lower > p.initial
        || p.initial > p.upper
        || p.lower < -100.0
        || p.upper > 100.0
        || p.ridge_weight < 0.0
        || p.ridge_weight > 1e6
        || p.ridge_center.abs() > 100.0
    {
        return Err("invalid coefficient box/ridge policy".into());
    }
    if ps.len() >= MAX_PARAMETERS {
        return Err("coefficient admission exceeded".into());
    }
    let i = ps.len();
    ps.push(P {
        name,
        definition: p.clone(),
    });
    Ok(i)
}
fn fixed_row(q: &[f64; 5], origin: usize) -> Result<(), String> {
    if q.iter().any(|v| !v.is_finite() || *v < 0.0)
        || q[origin] != 0.0
        || q.iter().sum::<f64>() > MAX_ROW_HAZARD
    {
        return Err("invalid fixed row hazards".into());
    }
    Ok(())
}
fn compile_rows(
    rows: &[OriginModel; 3],
    columns: &[Column],
    terms: &[String],
    bytes: &mut usize,
) -> Result<([R; 3], Vec<P>), String> {
    let mut ps = Vec::new();
    let mut out: [R; 3] = std::array::from_fn(|_| R {
        fixed: None,
        edges: Vec::new(),
    });
    for (i, row) in rows.iter().enumerate() {
        match row {
            OriginModel::FixedRow {
                hazards_per_year,
                review,
                rationale,
            } => {
                fixed_row(hazards_per_year, i)?;
                identity(review, bytes)?;
                text(rationale, bytes)?;
                out[i].fixed = Some(*hazards_per_year);
            }
            OriginModel::LogLinear { edges } => {
                if edges.is_empty() || edges.len() > 4 {
                    return Err("fitted origin needs explicit edges or supplied fixed row".into());
                }
                let mut destinations = HashSet::new();
                let mut maximum = 0.0;
                for e in edges {
                    let d = index(e.destination);
                    if d == i
                        || !destinations.insert(d)
                        || e.covariate_coefficients.len() != columns.len()
                        || e.term_coefficients.len() != terms.len() - 1
                    {
                        return Err("edge/state/feature/term coverage mismatch".into());
                    }
                    let mut ids = vec![parameter(
                        &e.intercept,
                        format!("{i}:{d}:intercept"),
                        &mut ps,
                    )?];
                    for (k, p) in e.covariate_coefficients.iter().enumerate() {
                        ids.push(parameter(
                            p,
                            format!("{i}:{d}:{}", columns[k].name),
                            &mut ps,
                        )?);
                    }
                    for (k, p) in e.term_coefficients.iter().enumerate() {
                        ids.push(parameter(
                            p,
                            format!("{i}:{d}:term:{}", terms[k + 1]),
                            &mut ps,
                        )?);
                    }
                    let (mut lo, mut hi) = (e.intercept.lower, e.intercept.upper);
                    for (c, p) in columns.iter().zip(&e.covariate_coefficients) {
                        let a = (c.raw_lower - c.offset) / c.scale;
                        let b = (c.raw_upper - c.offset) / c.scale;
                        let v = [a * p.lower, a * p.upper, b * p.lower, b * p.upper];
                        lo += v.iter().copied().fold(f64::INFINITY, f64::min);
                        hi += v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    }
                    lo += e
                        .term_coefficients
                        .iter()
                        .map(|p| p.lower)
                        .fold(0.0, f64::min);
                    hi += e
                        .term_coefficients
                        .iter()
                        .map(|p| p.upper)
                        .fold(0.0, f64::max);
                    if lo < -700.0 || hi > MAX_ROW_HAZARD.ln() {
                        return Err("coefficient box cannot guarantee finite admitted hazards over declared feature domain".into());
                    }
                    maximum += hi.exp();
                    out[i].edges.push(E {
                        destination: d,
                        parameters: ids,
                    });
                }
                if maximum > MAX_ROW_HAZARD {
                    return Err(
                        "coefficient box row envelope exceeds shared row hazard limit".into(),
                    );
                }
            }
        }
    }
    Ok((out, ps))
}
fn design(case: &Case, columns: usize, terms: usize) -> Vec<f64> {
    let mut x = vec![0.0; 1 + columns + terms - 1];
    x[0] = 1.0;
    x[1..1 + columns].copy_from_slice(&case.z);
    if case.term > 0 {
        x[columns + case.term] = 1.0;
    }
    x
}
fn rates(rows: &[R; 3], values: &[f64], case: &Case, columns: usize, terms: usize) -> [f64; 5] {
    let row = &rows[case.origin];
    if let Some(q) = row.fixed {
        return q;
    }
    let x = design(case, columns, terms);
    let mut q = [0.0; 5];
    for edge in &row.edges {
        q[edge.destination] = edge
            .parameters
            .iter()
            .zip(&x)
            .map(|(p, x)| values[*p] * x)
            .sum::<f64>()
            .exp();
    }
    q
}
struct Evaluation {
    loss: f64,
    gradient: Vec<f64>,
    information: Vec<Vec<f64>>,
}
fn objective(
    rows: &[R; 3],
    ps: &[P],
    values: &[f64],
    cases: &[Case],
    dimensions: (usize, usize),
    budget: &mut Budget,
    derivatives: bool,
) -> Result<Evaluation, String> {
    let (columns, terms) = dimensions;
    let n = ps.len();
    budget.charge(n * n)?;
    let mut out = Evaluation {
        loss: 0.0,
        gradient: vec![0.0; n],
        information: vec![vec![0.0; n]; n],
    };
    for case in cases {
        let row = &rows[case.origin];
        let q = rates(rows, values, case, columns, terms);
        let t = case.years();
        out.loss += t * q.iter().sum::<f64>();
        if let Some(d) = case.destination {
            if q[d] == 0.0 {
                return Err("training event has impossible declared edge".into());
            }
            out.loss -= q[d].ln();
        }
        let x = design(case, columns, terms);
        for edge in &row.edges {
            budget.charge(if derivatives {
                1 + 2 * edge.parameters.len() + edge.parameters.len() * edge.parameters.len()
            } else {
                1 + 2 * edge.parameters.len()
            })?;
            let expected = t * q[edge.destination];
            let observed = f64::from(case.destination == Some(edge.destination));
            if derivatives {
                for (k, &p) in edge.parameters.iter().enumerate() {
                    out.gradient[p] += (expected - observed) * x[k];
                    for (l, &r) in edge.parameters.iter().enumerate() {
                        out.information[p][r] += expected * x[k] * x[l];
                    }
                }
            }
        }
        budget.charge(1)?;
    }
    for (i, p) in ps.iter().enumerate() {
        let delta = values[i] - p.definition.ridge_center;
        out.loss += 0.5 * p.definition.ridge_weight * delta * delta;
        if derivatives {
            out.gradient[i] += p.definition.ridge_weight * delta;
        }
    }
    finite(out.loss)?;
    Ok(out)
}
fn projected_gradient(ps: &[P], values: &[f64], g: &[f64]) -> Vec<f64> {
    ps.iter()
        .zip(values)
        .zip(g)
        .map(|((p, &x), &v)| {
            if p.definition.lower == p.definition.upper
                || (x <= p.definition.lower && v > 0.0)
                || (x >= p.definition.upper && v < 0.0)
            {
                0.0
            } else {
                v
            }
        })
        .collect()
}
fn solve(mut h: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for k in 0..n {
        let p = (k..n).max_by(|&a, &b| h[a][k].abs().total_cmp(&h[b][k].abs()))?;
        if h[p][k].abs() < 1e-14 {
            return None;
        }
        h.swap(p, k);
        b.swap(p, k);
        let pivot = h[k].clone();
        for i in k + 1..n {
            let v = h[i][k] / h[k][k];
            for (value, pivot_value) in h[i].iter_mut().zip(&pivot).skip(k) {
                *value -= v * pivot_value;
            }
            b[i] -= v * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|j| h[i][j] * x[j]).sum::<f64>()) / h[i][i];
    }
    Some(x)
}
fn eigenvalues(mut h: Vec<Vec<f64>>, budget: &mut Budget) -> Result<Vec<f64>, String> {
    let n = h.len();
    for _ in 0..10 * n * n {
        budget.charge(n * n)?;
        let mut best = (0, 0, 0.0_f64);
        for (i, row) in h.iter().enumerate() {
            for (j, &v) in row.iter().enumerate().skip(i + 1) {
                if v.abs() > best.2 {
                    best = (i, j, v.abs());
                }
            }
        }
        let scale = h
            .iter()
            .enumerate()
            .map(|(i, r)| r[i].abs())
            .fold(1.0_f64, f64::max);
        if best.2 <= 1e-12 * scale {
            break;
        }
        let (p, q, _) = best;
        let angle = 0.5 * (2.0 * h[p][q]).atan2(h[q][q] - h[p][p]);
        let (c, s) = (angle.cos(), angle.sin());
        let (a, b, v) = (h[p][p], h[q][q], h[p][q]);
        let mut mirrored = Vec::with_capacity(n);
        for (k, row) in h.iter_mut().enumerate() {
            if k != p && k != q {
                let (x, y) = (row[p], row[q]);
                row[p] = c * x - s * y;
                row[q] = s * x + c * y;
                mirrored.push((k, row[p], row[q]));
            }
        }
        for (k, pk, qk) in mirrored {
            h[p][k] = pk;
            h[q][k] = qk;
        }
        h[p][p] = c * c * a - 2.0 * c * s * v + s * s * b;
        h[q][q] = s * s * a + 2.0 * c * s * v + c * c * b;
        h[p][q] = 0.0;
        h[q][p] = 0.0;
    }
    let scale = h
        .iter()
        .enumerate()
        .map(|(i, r)| r[i].abs())
        .fold(1.0_f64, f64::max);
    if h.iter().enumerate().any(|(i, r)| {
        r.iter()
            .enumerate()
            .any(|(j, v)| i != j && v.abs() > 1e-12 * scale)
    }) {
        return Err("bounded information eigensolver did not converge".into());
    }
    let mut values = (0..n).map(|i| h[i][i]).collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    Ok(values)
}
fn diagnostics(
    rows: &[R; 3],
    values: &[f64],
    cases: &[Case],
    columns: usize,
    terms: usize,
    budget: &mut Budget,
) -> Result<Diagnostics, String> {
    let mut d = Diagnostics {
        segments: cases.len(),
        exposure_years_by_origin: [0.0; 3],
        event_counts: [[0; 5]; 3],
        expected_event_counts: [[0.0; 5]; 3],
        log_likelihood: Some(0.0),
        impossible_events: 0,
        right_censored_segments: 0,
    };
    for case in cases {
        budget.charge(1 + columns + terms)?;
        let q = rates(rows, values, case, columns, terms);
        d.exposure_years_by_origin[case.origin] += case.years();
        for (j, &v) in q.iter().enumerate() {
            d.expected_event_counts[case.origin][j] += case.years() * v;
        }
        let mut ll = -case.years() * q.iter().sum::<f64>();
        if let Some(j) = case.destination {
            d.event_counts[case.origin][j] += 1;
            if q[j] == 0.0 {
                d.impossible_events += 1;
                d.log_likelihood = None;
            } else {
                ll += q[j].ln();
            }
        }
        if let Some(total) = d.log_likelihood.as_mut() {
            *total += ll;
        }
        d.right_censored_segments += usize::from(case.right_censored);
    }
    Ok(d)
}
fn resolved(rows: &[OriginModel; 3], ps: &[P], values: &[f64]) -> [ResolvedRow; 3] {
    let mut cursor = 0;
    let mut estimate = || {
        let p = &ps[cursor];
        let out = EstimatedParameter {
            name: p.name.clone(),
            value: values[cursor],
            lower: p.definition.lower,
            upper: p.definition.upper,
            ridge_weight: p.definition.ridge_weight,
            ridge_center: p.definition.ridge_center,
        };
        cursor += 1;
        out
    };
    std::array::from_fn(|i| match &rows[i] {
        OriginModel::FixedRow {
            hazards_per_year,
            review,
            rationale,
        } => ResolvedRow::FixedRow {
            hazards_per_year: *hazards_per_year,
            review: review.clone(),
            rationale: rationale.clone(),
        },
        OriginModel::LogLinear { edges } => ResolvedRow::LogLinear {
            edges: edges
                .iter()
                .map(|e| ResolvedEdge {
                    destination: e.destination,
                    intercept: estimate(),
                    covariate_coefficients: e
                        .covariate_coefficients
                        .iter()
                        .map(|_| estimate())
                        .collect(),
                    term_coefficients: e.term_coefficients.iter().map(|_| estimate()).collect(),
                })
                .collect(),
        },
    })
}

pub fn fit(request: &CovariateFitRequest) -> Result<CovariateFitResult, String> {
    let mut budget = Budget::new(request.optimizer.timeout_ms)?;
    let mut bytes = 0;
    if request.schema != SCHEMA
        || request.measure != CreditMeasure::Physical
        || request.segments.is_empty()
        || request.segments.len() > MAX_SEGMENTS
    {
        return Err("exact physical covariate contract/admission required".into());
    }
    for id in [
        &request.population,
        &request.model_policy,
        &request.training_source,
        &request.holdout_source,
        &request.training_split,
        &request.holdout_split,
    ] {
        identity(id, &mut bytes)?;
    }
    if request.training_split == request.holdout_split {
        return Err("training and holdout split identities must differ".into());
    }
    let (a, b, c) = (
        sec(request.train_start)?,
        sec(request.train_end)?,
        sec(request.holdout_end)?,
    );
    if !(a < b && b < c) || c - a > 200 * 365 * 86400 {
        return Err("ordered bounded chronological windows required".into());
    }
    let op = &request.optimizer;
    if op.max_iterations == 0
        || op.max_iterations > 256
        || op.max_backtracks == 0
        || op.max_backtracks > 64
        || !op.projected_gradient_tolerance.is_finite()
        || !(1e-12..=1e-3).contains(&op.projected_gradient_tolerance)
        || !op.relative_information_tolerance.is_finite()
        || !(1e-12..=1e-3).contains(&op.relative_information_tolerance)
    {
        return Err("invalid bounded optimizer policy".into());
    }
    validate_columns(&request.columns, &request.term_levels, &mut bytes)?;
    let (rows, ps) = compile_rows(
        &request.rows,
        &request.columns,
        &request.term_levels,
        &mut bytes,
    )?;
    let mut ids = HashSet::new();
    let mut accounts: BTreeMap<&str, Vec<&Segment>> = BTreeMap::new();
    let (mut training, mut holdout) = (Vec::new(), Vec::new());
    for s in &request.segments {
        budget.charge(1 + request.columns.len())?;
        text(&s.segment_id, &mut bytes)?;
        text(&s.account_id, &mut bytes)?;
        identity(&s.source, &mut bytes)?;
        if !ids.insert(&s.segment_id) || index(s.origin) >= 3 {
            return Err("duplicate segment or nonlive observed origin".into());
        }
        let (start, end) = (sec(s.start)?, sec(s.end)?);
        if start >= end || start < a || end > c || sec(s.available_time)? > start {
            return Err("exact segment clock/features unavailable or outside windows".into());
        }
        if (start < b && s.source != request.training_source)
            || (start >= b && s.source != request.holdout_source)
        {
            return Err("observed segment partition source identity mismatch".into());
        }
        let (z, term) = feature(
            &request.columns,
            &request.term_levels,
            &s.covariates,
            &s.term_level,
        )?;
        let dest = match s.ending {
            Ending::Transition { destination } => {
                let d = index(destination);
                if d == index(s.origin) {
                    return Err("self transitions are not observed exits".into());
                }
                Some(d)
            }
            _ => None,
        };
        for (window_start, window_end, cases) in [(a, b, &mut training), (b, c, &mut holdout)] {
            let lo = start.max(window_start);
            let hi = end.min(window_end);
            if lo < hi {
                cases.push(Case {
                    segment_id: s.segment_id.clone(),
                    account_id: s.account_id.clone(),
                    source: s.source.clone(),
                    start_seconds: lo,
                    end_seconds: hi,
                    available_seconds: sec(s.available_time)?,
                    origin: index(s.origin),
                    destination: if end <= window_end && end > window_start {
                        dest
                    } else {
                        None
                    },
                    z: z.clone(),
                    raw_covariates: s.covariates.clone(),
                    term,
                    right_censored: matches!(s.ending, Ending::RightCensored {}) && hi == end,
                });
            }
        }
        accounts.entry(&s.account_id).or_default().push(s);
    }
    for path in accounts.values_mut() {
        path.sort_by_key(|s| (s.start, s.end));
        for pair in path.windows(2) {
            budget.charge(1)?;
            let (prev, next) = (pair[0], pair[1]);
            if prev.end != next.start {
                return Err("exact account path must be continuous; separate observation identities required for censoring gaps".into());
            }
            let expected = match prev.ending {
                Ending::Transition { destination } => destination,
                Ending::CovariateBoundary {} => prev.origin,
                Ending::RightCensored {} => {
                    return Err("right censoring must end an observed account path".into())
                }
            };
            if expected != next.origin || index(expected) >= 3 {
                return Err(
                    "next observed origin contradicts known transition/absorbing exit".into(),
                );
            }
        }
        if matches!(
            path.last().expect("nonempty grouped path").ending,
            Ending::CovariateBoundary {}
        ) {
            return Err("covariate boundary requires an observed continuation".into());
        }
    }
    // Canonical order is independent of caller record ordering; only clipped train data enters identity.
    training.sort_by(|a, b| {
        (&a.account_id, a.start_seconds, &a.segment_id).cmp(&(
            &b.account_id,
            b.start_seconds,
            &b.segment_id,
        ))
    });
    holdout.sort_by(|a, b| {
        (&a.account_id, a.start_seconds, &a.segment_id).cmp(&(
            &b.account_id,
            b.start_seconds,
            &b.segment_id,
        ))
    });
    if training.is_empty() {
        return Err("training window has no exact observed exposure".into());
    }
    let n = ps.len();
    let per_evaluation = n * n
        + training
            .iter()
            .map(|case| {
                1 + rows[case.origin]
                    .edges
                    .iter()
                    .map(|e| 1 + 2 * e.parameters.len() + e.parameters.len() * e.parameters.len())
                    .sum::<usize>()
            })
            .sum::<usize>();
    let per_loss = n * n
        + training
            .iter()
            .map(|case| {
                1 + rows[case.origin]
                    .edges
                    .iter()
                    .map(|e| 1 + 2 * e.parameters.len())
                    .sum::<usize>()
            })
            .sum::<usize>();
    // Reserve the declared worst-case optimizer, eigensolver and diagnostics before fitting.
    if per_evaluation
        .checked_mul(op.max_iterations + 1)
        .and_then(|v| v.checked_add(per_loss * op.max_iterations * op.max_backtracks))
        .and_then(|v| v.checked_add(n * n * n * op.max_iterations + 10 * n * n * n * n))
        .and_then(|v| {
            v.checked_add(
                (training.len() + holdout.len())
                    * (1 + request.columns.len() + request.term_levels.len()),
            )
        })
        .and_then(|v| v.checked_add(2 * MAX_DATED_METADATA_BYTES / 64 + 2))
        .is_none_or(|v| v > MAX_WORK - budget.work)
    {
        return Err("declared complete-fit worst-case work admission exceeded".into());
    }
    let mut exposure = [0.0; 3];
    let mut events = [[0usize; 5]; 3];
    for t in &training {
        exposure[t.origin] += t.years();
        if let Some(j) = t.destination {
            events[t.origin][j] += 1;
        }
    }
    for (i, row) in rows.iter().enumerate() {
        if row.fixed.is_none() {
            if exposure[i] == 0.0 {
                return Err(
                    "unobserved fitted origin requires explicit reviewed fixed-row assumption"
                        .into(),
                );
            }
            for e in &row.edges {
                if events[i][e.destination] == 0 {
                    return Err("zero-event fitted edge has no finite unpenalized MLE; declare reviewed fixed row or remove edge with explicit policy".into());
                }
            }
        }
    }
    let training_digest = sha256_checked(
        &serde_json::to_vec(&(
            SCHEMA,
            &request.training_source,
            &request.training_split,
            request.train_start,
            request.train_end,
            &request.population,
            &request.model_policy,
            &request.columns,
            &request.term_levels,
            &request.rows,
            &request.optimizer,
            &training,
        ))
        .map_err(|e| e.to_string())?,
        &mut budget,
    )?;
    let mut values = ps.iter().map(|p| p.definition.initial).collect::<Vec<_>>();
    let mut evaluations = 0;
    let mut iterations = 0;
    let mut convergence = "iteration_limit".to_owned();
    for iteration in 0..op.max_iterations {
        budget.charge(1)?;
        iterations = iteration + 1;
        let objective_value = objective(
            &rows,
            &ps,
            &values,
            &training,
            (request.columns.len(), request.term_levels.len()),
            &mut budget,
            true,
        )?;
        evaluations += 1;
        let pg = projected_gradient(&ps, &values, &objective_value.gradient);
        let norm = pg.iter().map(|v| v.abs()).fold(0.0, f64::max);
        if norm <= op.projected_gradient_tolerance {
            convergence = "converged_projected_gradient".into();
            break;
        }
        let active = pg
            .iter()
            .enumerate()
            .filter_map(|(i, v)| (*v != 0.0).then_some(i))
            .collect::<Vec<_>>();
        let h = active
            .iter()
            .map(|&i| {
                active
                    .iter()
                    .map(|&j| {
                        objective_value.information[i][j]
                            + if i == j {
                                ps[i].definition.ridge_weight
                            } else {
                                0.0
                            }
                    })
                    .collect()
            })
            .collect();
        let rhs = active.iter().map(|&i| -pg[i]).collect::<Vec<_>>();
        budget.charge(n * n * n)?;
        let local = solve(h, rhs.clone()).unwrap_or(rhs);
        let mut direction = vec![0.0; n];
        for (&i, &d) in active.iter().zip(&local) {
            direction[i] = d;
        }
        if direction.iter().zip(&pg).map(|(a, b)| a * b).sum::<f64>() >= 0.0 {
            for (d, g) in direction.iter_mut().zip(&pg) {
                *d = -g;
            }
        }
        let mut accepted = false;
        let mut alpha = 1.0;
        for _ in 0..op.max_backtracks {
            let candidate = values
                .iter()
                .zip(&direction)
                .zip(&ps)
                .map(|((&x, &d), p)| (x + alpha * d).clamp(p.definition.lower, p.definition.upper))
                .collect::<Vec<_>>();
            let slope = objective_value
                .gradient
                .iter()
                .zip(candidate.iter().zip(&values))
                .map(|(g, (a, b))| g * (a - b))
                .sum::<f64>();
            if slope < 0.0 {
                let trial = objective(
                    &rows,
                    &ps,
                    &candidate,
                    &training,
                    (request.columns.len(), request.term_levels.len()),
                    &mut budget,
                    false,
                )?;
                evaluations += 1;
                if trial.loss <= objective_value.loss + 1e-4 * slope {
                    values = candidate;
                    accepted = true;
                    break;
                }
            }
            alpha *= 0.5;
        }
        if !accepted {
            convergence = "line_search_stalled".into();
            break;
        }
    }
    let final_value = objective(
        &rows,
        &ps,
        &values,
        &training,
        (request.columns.len(), request.term_levels.len()),
        &mut budget,
        true,
    )?;
    evaluations += 1;
    let pg = projected_gradient(&ps, &values, &final_value.gradient);
    let norm = pg.iter().map(|v| v.abs()).fold(0.0, f64::max);
    if norm <= op.projected_gradient_tolerance {
        convergence = "converged_projected_gradient".into();
    }
    let free = ps
        .iter()
        .enumerate()
        .filter_map(|(i, p)| (p.definition.lower < p.definition.upper).then_some(i))
        .collect::<Vec<_>>();
    let information = free
        .iter()
        .map(|&i| {
            free.iter()
                .map(|&j| final_value.information[i][j])
                .collect()
        })
        .collect();
    let eigen = eigenvalues(information, &mut budget)?;
    let largest = eigen.iter().copied().fold(0.0, f64::max);
    let rank = eigen
        .iter()
        .filter(|&&v| v > op.relative_information_tolerance * largest.max(1e-12))
        .count();
    let active = ps
        .iter()
        .zip(&values)
        .filter(|(p, x)| {
            p.definition.lower < p.definition.upper
                && (**x - p.definition.lower)
                    .abs()
                    .min((**x - p.definition.upper).abs())
                    <= 1e-10
        })
        .map(|(p, _)| p.name.clone())
        .collect::<Vec<_>>();
    let mut reasons = Vec::new();
    if convergence != "converged_projected_gradient" {
        reasons.push("optimizer_not_converged".into());
    }
    if rank != free.len() {
        reasons.push("training_information_rank_deficient".into());
    }
    if !active.is_empty() {
        reasons.push("estimated_parameter_at_bound".into());
    }
    let numerical_eligible = reasons.is_empty();
    if request.data_kind == DataKind::Synthetic {
        reasons.push("synthetic_data_not_empirical_adoption".into());
    }
    let training_diag = diagnostics(
        &rows,
        &values,
        &training,
        request.columns.len(),
        request.term_levels.len(),
        &mut budget,
    )?;
    let holdout_diag = diagnostics(
        &rows,
        &values,
        &holdout,
        request.columns.len(),
        request.term_levels.len(),
        &mut budget,
    )?;
    if training_diag.impossible_events > 0 {
        return Err("impossible training transition".into());
    }
    let mut model = FittedModel {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        data_kind: request.data_kind,
        population: request.population.clone(),
        model_policy: request.model_policy.clone(),
        training_source: request.training_source.clone(),
        training_split: request.training_split.clone(),
        columns: request.columns.clone(),
        term_levels: request.term_levels.clone(),
        rows: resolved(&request.rows, &ps, &values),
        training_digest_sha256: training_digest,
        numerical_eligible,
        adoption_eligible: numerical_eligible && request.data_kind == DataKind::Observed,
        coefficient_identity_sha256: String::new(),
    };
    model.coefficient_identity_sha256 = sha256_checked(
        &serde_json::to_vec(&model).map_err(|e| e.to_string())?,
        &mut budget,
    )?;
    Ok(CovariateFitResult {
        schema: SCHEMA.into(),
        model,
        holdout_source: request.holdout_source.clone(),
        holdout_split: request.holdout_split.clone(),
        train_start: request.train_start,
        train_end: request.train_end,
        holdout_end: request.holdout_end,
        training: training_diag,
        holdout: holdout_diag,
        convergence,
        projected_gradient_norm: norm,
        iterations,
        objective_evaluations: evaluations,
        work_units: budget.work,
        unpenalized_information_eigenvalues: eigen,
        information_rank: rank,
        free_parameters: free.len(),
        active_bound_parameters: active,
        eligibility_reasons: reasons,
    })
}

pub fn project(request: &CovariateProjectionRequest) -> Result<ProjectedCreditHazards, String> {
    let mut budget = Budget::new(request.timeout_ms)?;
    let mut bytes = 0;
    if request.schema != SCHEMA
        || request.measure != CreditMeasure::Physical
        || request.model.schema != SCHEMA
        || request.model.measure != CreditMeasure::Physical
        || request.intervals.is_empty()
        || request.intervals.len() > 4096
    {
        return Err("physical covariate projection schema/admission required".into());
    }
    let analytical = matches!(request.use_policy, ProjectionUse::AnalyticalOnly);
    if !analytical
        && (!request.model.adoption_eligible
            || !request.model.numerical_eligible
            || request.model.data_kind != DataKind::Observed)
    {
        return Err("ineligible fitted model cannot produce adoption hazards; explicit analytical_only required".into());
    }
    text(&request.position_id, &mut bytes)?;
    text(&request.scenario_id, &mut bytes)?;
    identity(&request.review, &mut bytes)?;
    identity(&request.model_revision, &mut bytes)?;
    for id in [
        &request.model.population,
        &request.model.model_policy,
        &request.model.training_source,
        &request.model.training_split,
    ] {
        identity(id, &mut bytes)?;
    }
    for hash in [
        &request.model.training_digest_sha256,
        &request.model.coefficient_identity_sha256,
    ] {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("invalid lowercase SHA256 identity".into());
        }
    }
    for row in &request.model.rows {
        if let ResolvedRow::LogLinear { edges } = row {
            if edges.len() > 4 {
                return Err("projected fitted edge admission exceeded".into());
            }
            for e in edges {
                if e.covariate_coefficients.len() > 8 || e.term_coefficients.len() > 31 {
                    return Err("projected coefficient coverage admission exceeded".into());
                }
                for p in std::iter::once(&e.intercept)
                    .chain(&e.covariate_coefficients)
                    .chain(&e.term_coefficients)
                {
                    text(&p.name, &mut bytes)?;
                }
            }
        }
    }
    validate_columns(
        &request.model.columns,
        &request.model.term_levels,
        &mut bytes,
    )?;
    let p = |x: &EstimatedParameter| Parameter {
        initial: x.value,
        lower: x.lower,
        upper: x.upper,
        ridge_weight: x.ridge_weight,
        ridge_center: x.ridge_center,
    };
    let declared: [OriginModel; 3] = std::array::from_fn(|i| match &request.model.rows[i] {
        ResolvedRow::FixedRow {
            hazards_per_year,
            review,
            rationale,
        } => OriginModel::FixedRow {
            hazards_per_year: *hazards_per_year,
            review: review.clone(),
            rationale: rationale.clone(),
        },
        ResolvedRow::LogLinear { edges } => OriginModel::LogLinear {
            edges: edges
                .iter()
                .map(|e| Edge {
                    destination: e.destination,
                    intercept: p(&e.intercept),
                    covariate_coefficients: e.covariate_coefficients.iter().map(p).collect(),
                    term_coefficients: e.term_coefficients.iter().map(p).collect(),
                })
                .collect(),
        },
    });
    let (rows, ps) = compile_rows(
        &declared,
        &request.model.columns,
        &request.model.term_levels,
        &mut bytes,
    )?;
    let mut normalized = request.model.clone();
    let digest = std::mem::take(&mut normalized.coefficient_identity_sha256);
    if digest
        != sha256_checked(
            &serde_json::to_vec(&normalized).map_err(|e| e.to_string())?,
            &mut budget,
        )?
    {
        return Err("coefficient identity hash mismatch".into());
    }
    let values = ps.iter().map(|p| p.definition.initial).collect::<Vec<_>>();
    let start = sec(request.origin_time)?;
    let end = sec(request.end_time)?;
    if end <= start || end - start > 100 * 365 * 86400 {
        return Err("invalid bounded projection horizon".into());
    }
    let mut prior = start;
    let mut ids = HashSet::new();
    let mut intervals = Vec::with_capacity(request.intervals.len());
    for interval in &request.intervals {
        budget.charge(1 + ps.len() + request.model.columns.len())?;
        text(&interval.interval_id, &mut bytes)?;
        identity(&interval.source, &mut bytes)?;
        let (a, b) = (sec(interval.start)?, sec(interval.end)?);
        if a != prior
            || b <= a
            || b > end
            || sec(interval.available_time)? > a
            || !ids.insert(&interval.interval_id)
        {
            return Err(
                "complete ordered driver intervals with available feature vintage required".into(),
            );
        }
        let (z, term) = feature(
            &request.model.columns,
            &request.model.term_levels,
            &interval.covariates,
            &interval.term_level,
        )?;
        let mut hazards = [[0.0; 5]; 3];
        for (i, row) in hazards.iter_mut().enumerate() {
            *row = rates(
                &rows,
                &values,
                &Case {
                    segment_id: String::new(),
                    account_id: String::new(),
                    source: interval.source.clone(),
                    start_seconds: a,
                    end_seconds: b,
                    available_seconds: sec(interval.available_time)?,
                    origin: i,
                    destination: None,
                    z: z.clone(),
                    raw_covariates: interval.covariates.clone(),
                    term,
                    right_censored: false,
                },
                request.model.columns.len(),
                request.model.term_levels.len(),
            );
            fixed_row(row, i)?;
        }
        intervals.push(ProjectedInterval {
            interval_id: interval.interval_id.clone(),
            start: interval.start,
            end: interval.end,
            end_year: (b - start) as f64 / YEAR_SECONDS,
            source: interval.source.clone(),
            kind: interval.kind.clone(),
            hazards_per_year: hazards,
        });
        prior = b;
    }
    if prior != end {
        return Err("incomplete projected driver horizon".into());
    }
    Ok(ProjectedCreditHazards {
        schema: SCHEMA.into(),
        measure: CreditMeasure::Physical,
        position_id: request.position_id.clone(),
        scenario_id: request.scenario_id.clone(),
        model_revision: request.model_revision.clone(),
        review: request.review.clone(),
        coefficient_identity_sha256: digest,
        adoption_output: !analytical,
        analytical_only: analytical,
        intervals,
        work_units: budget.work,
    })
}

// FIPS 180-4 SHA-256 over canonical serde typed bytes, without a new dependency.
#[cfg(test)]
fn sha256(input: &[u8]) -> String {
    sha256_impl(input, None).expect("standard bounded test input")
}
fn sha256_checked(input: &[u8], budget: &mut Budget) -> Result<String, String> {
    if input.len() > MAX_DATED_METADATA_BYTES {
        return Err("canonical model identity payload admission exceeded".into());
    }
    sha256_impl(input, Some(budget))
}
fn sha256_impl(input: &[u8], mut budget: Option<&mut Budget>) -> Result<String, String> {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut bytes = input.to_vec();
    let bits = (input.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&bits.to_be_bytes());
    for block in bytes.chunks_exact(64) {
        if let Some(budget) = budget.as_deref_mut() {
            budget.charge(1)?;
        }
        let mut w = [0u32; 64];
        for (i, b) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let a = w[i - 15];
            let b = w[i - 2];
            w[i] = w[i - 16]
                .wrapping_add(a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3))
                .wrapping_add(w[i - 7])
                .wrapping_add(b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10));
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut z) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let t = z
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let u = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            z = g;
            g = f;
            f = e;
            e = d.wrapping_add(t);
            d = c;
            c = b;
            b = a;
            a = t.wrapping_add(u);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, z]) {
            *x = x.wrapping_add(y);
        }
    }
    Ok(h.iter().map(|v| format!("{v:08x}")).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_sha256_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn analytic_poisson_score_and_information_match_independent_equations_and_differences() {
        let def = |initial, weight, center| Parameter {
            initial,
            lower: -5.0,
            upper: 1.0,
            ridge_weight: weight,
            ridge_center: center,
        };
        let ps = vec![
            P {
                name: "intercept".into(),
                definition: def(-1.0, 0.5, -0.5),
            },
            P {
                name: "beta".into(),
                definition: def(0.2, 0.25, 0.1),
            },
        ];
        let rows = [
            R {
                fixed: None,
                edges: vec![E {
                    destination: 3,
                    parameters: vec![0, 1],
                }],
            },
            R {
                fixed: Some([0.0; 5]),
                edges: vec![],
            },
            R {
                fixed: Some([0.0; 5]),
                edges: vec![],
            },
        ];
        let make = |id: &str, years: f64, z: f64, destination| Case {
            segment_id: id.into(),
            account_id: id.into(),
            source: Identity {
                id: "hand".into(),
                revision: "1".into(),
            },
            start_seconds: 0,
            end_seconds: (years * YEAR_SECONDS) as i64,
            available_seconds: 0,
            origin: 0,
            destination,
            z: vec![z],
            raw_covariates: vec![z],
            term: 0,
            right_censored: destination.is_none(),
        };
        let cases = vec![make("one", 1.0, 0.0, Some(3)), make("two", 2.0, 1.0, None)];
        let mut budget = Budget::new(60_000).unwrap();
        let values = vec![-1.0, 0.2];
        let value = objective(&rows, &ps, &values, &cases, (1, 1), &mut budget, true).unwrap();
        let (a, b) = ((-1.0_f64).exp(), (-0.8_f64).exp());
        assert!((value.gradient[0] - (a + 2.0 * b - 1.0 - 0.25)).abs() < 1e-12);
        assert!((value.gradient[1] - (2.0 * b + 0.025)).abs() < 1e-12);
        assert!((value.information[0][0] - (a + 2.0 * b)).abs() < 1e-12);
        assert!((value.information[0][1] - 2.0 * b).abs() < 1e-12);
        for i in 0..2 {
            let h = 1e-5;
            let mut plus = values.clone();
            let mut minus = values.clone();
            plus[i] += h;
            minus[i] -= h;
            let p = objective(&rows, &ps, &plus, &cases, (1, 1), &mut budget, true).unwrap();
            let m = objective(&rows, &ps, &minus, &cases, (1, 1), &mut budget, true).unwrap();
            assert!(((p.loss - m.loss) / (2.0 * h) - value.gradient[i]).abs() < 1e-9);
            for j in 0..2 {
                let expected = value.information[j][i]
                    + if j == i {
                        ps[i].definition.ridge_weight
                    } else {
                        0.0
                    };
                assert!(((p.gradient[j] - m.gradient[j]) / (2.0 * h) - expected).abs() < 1e-9);
            }
        }
    }
}
