//! Joint additive Gaussian innovations. Regional HPI coordinates are log changes.
use super::{label, range};
use crate::random::SharedDraws;
use nalgebra::{linalg::SymmetricEigen, DMatrix};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_DRIVERS: usize = 16;
pub const MAX_VALUES: usize = 1_000_000;
pub const MAX_TRANSFORM_WORK: usize = 8_000_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DriverKind {
    RateFactor { tenor_years: f64 },
    MortgageSpread {},
    RegionalLogHpi { region_id: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDriver {
    pub driver_id: String,
    pub kind: DriverKind,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawSource {
    NativeSeed { seed: Vec<u32> },
    SuppliedStandardNormals { values: Vec<f64> },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointGaussianRequest {
    pub source_revision: String,
    pub shared_draw_id: String,
    pub drivers: Vec<JointDriver>,
    /// Symmetric annual innovation covariance in declared coordinate units, row-major.
    pub annual_covariance: Vec<f64>,
    pub durations_years: Vec<f64>,
    pub paths: usize,
    /// Eigenfactor truncation is explicit diagnostic/model choice; full driver-shaped CRN retained.
    pub retained_factors: usize,
    pub draws: DrawSource,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovarianceDiagnostics {
    pub correlation_eigenvalues_descending: Vec<f64>,
    pub roundoff_negative_eigenvalue: f64,
    pub roundoff_correlation_bound_adjustment: f64,
    pub retained_annual_covariance: Vec<f64>,
    pub discarded_covariance_frobenius: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JointGaussianResult {
    pub source_revision: String,
    pub shared_draw_id: String,
    pub drivers: Vec<JointDriver>,
    pub shape: [usize; 3],
    pub durations_years: Vec<f64>,
    pub retained_factors: usize,
    /// Row-major [path,step,driver]; no drift, tail clipping, or conversion from log HPI to level.
    pub increments: Vec<f64>,
    pub terminal_changes: Vec<f64>,
    pub diagnostics: CovarianceDiagnostics,
    pub sample_mean_annualized_innovation: Vec<f64>,
    pub sample_annual_covariance: Vec<f64>,
}

pub fn covariance_root(
    covariance: &[f64],
    n: usize,
    k: usize,
) -> Result<(Vec<f64>, CovarianceDiagnostics), String> {
    if n == 0 || n > MAX_DRIVERS || k == 0 || k > n || covariance.len() != n * n {
        return Err("joint covariance/factor dimensions invalid".into());
    }
    let mut stddev = vec![0.; n];
    let mut correlation = DMatrix::zeros(n, n);
    let mut bound_roundoff: f64 = 0.;
    for i in 0..n {
        range(covariance[i * n + i], 0., 4., "annual variance")?;
        stddev[i] = covariance[i * n + i].sqrt();
    }
    for i in 0..n {
        for j in 0..n {
            let v = covariance[i * n + j];
            range(v, -4., 4., "annual covariance")?;
            if v != covariance[j * n + i] {
                return Err("joint covariance must be exactly symmetric".into());
            }
            if stddev[i] == 0. || stddev[j] == 0. {
                if v != 0. {
                    return Err("zero-variance driver must have exactly zero covariance row".into());
                }
            } else {
                let rho = if i == j {
                    1.
                } else {
                    (v / stddev[i]) / stddev[j]
                };
                if !rho.is_finite() || rho.abs() > 1. + 4. * f64::EPSILON {
                    return Err("correlation violates covariance bounds".into());
                }
                bound_roundoff = bound_roundoff.max((rho.abs() - 1.).max(0.));
                correlation[(i, j)] = rho.clamp(-1., 1.);
            }
        }
    }
    let eigen = SymmetricEigen::try_new(correlation, f64::EPSILON, 100_000)
        .ok_or("joint PSD eigendecomposition failed")?;
    let tolerance = 8. * f64::EPSILON * n as f64;
    let minimum = eigen.eigenvalues.iter().copied().fold(0., f64::min);
    if minimum < -tolerance {
        return Err("joint covariance is not positive semidefinite".into());
    }
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|a, b| {
        eigen.eigenvalues[*b]
            .total_cmp(&eigen.eigenvalues[*a])
            .then(a.cmp(b))
    });
    // Symmetric correlation square root is invariant to eigenvector sign and uses
    // named driver-shaped z coordinates, even when k changes for diagnostics.
    let mut root = vec![0.; n * n];
    for &factor in order.iter().take(k) {
        let value = eigen.eigenvalues[factor].max(0.).sqrt();
        for i in 0..n {
            for j in 0..n {
                root[i * n + j] += stddev[i]
                    * eigen.eigenvectors[(i, factor)]
                    * value
                    * eigen.eigenvectors[(j, factor)];
            }
        }
    }
    let mut retained = vec![0.; n * n];
    for i in 0..n {
        for j in 0..n {
            retained[i * n + j] = (0..n).map(|a| root[i * n + a] * root[j * n + a]).sum();
        }
    }
    let discarded = covariance
        .iter()
        .zip(&retained)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>()
        .sqrt();
    Ok((
        root,
        CovarianceDiagnostics {
            correlation_eigenvalues_descending: order
                .iter()
                .map(|&j| eigen.eigenvalues[j])
                .collect(),
            roundoff_negative_eigenvalue: minimum,
            roundoff_correlation_bound_adjustment: bound_roundoff,
            retained_annual_covariance: retained,
            discarded_covariance_frobenius: discarded,
        },
    ))
}

pub fn simulate(r: &JointGaussianRequest) -> Result<JointGaussianResult, String> {
    simulate_with_check(r, || Ok(()))
}

/// Generate the bounded joint market tape with cooperative cancellation.
/// The default `simulate` entrypoint remains byte-for-byte equivalent for
/// callers that do not own a deadline or cancellation token.
pub fn simulate_with_check(
    r: &JointGaussianRequest,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<JointGaussianResult, String> {
    check()?;
    label(&r.source_revision, 256)?;
    label(&r.shared_draw_id, 256)?;
    let n = r.drivers.len();
    let steps = r.durations_years.len();
    if n == 0 || n > MAX_DRIVERS || steps == 0 || steps > 4096 || r.paths == 0 || r.paths > 100_000
    {
        return Err("joint driver/path/step admission exceeded".into());
    }
    let values = r
        .paths
        .checked_mul(steps)
        .and_then(|v| v.checked_mul(n))
        .ok_or("joint dimensions overflow")?;
    if values > MAX_VALUES || values.checked_mul(n).is_none_or(|v| v > MAX_TRANSFORM_WORK) {
        return Err("joint tape/transform work admission exceeded".into());
    }
    let mut names = HashSet::new();
    let (mut rate, mut spread, mut hpi) = (false, false, false);
    let mut regions = HashSet::new();
    for d in &r.drivers {
        label(&d.driver_id, 128)?;
        if !names.insert(&d.driver_id) {
            return Err("duplicate joint driver identity".into());
        }
        match &d.kind {
            DriverKind::RateFactor { tenor_years } => {
                range(*tenor_years, 1e-8, 100., "rate-factor tenor")?;
                rate = true;
            }
            DriverKind::MortgageSpread {} => spread = true,
            DriverKind::RegionalLogHpi { region_id } => {
                label(region_id, 128)?;
                if !regions.insert(region_id) {
                    return Err("duplicate regional HPI identity".into());
                }
                hpi = true;
            }
        }
    }
    if !rate || !spread || !hpi {
        return Err(
            "joint model requires explicit rate, mortgage spread and regional log-HPI coordinates"
                .into(),
        );
    }
    for &dt in &r.durations_years {
        range(dt, 1e-12, 100., "joint step duration")?;
    }
    range(
        r.durations_years.iter().sum(),
        1e-12,
        100.,
        "joint total duration",
    )?;
    let (root, diagnostics) = covariance_root(&r.annual_covariance, n, r.retained_factors)?;
    let native;
    let z = match &r.draws {
        DrawSource::NativeSeed { seed } => {
            if seed.is_empty() || seed.len() > 16 {
                return Err("joint seed requires 1..16 u32 words".into());
            }
            native = SharedDraws::new(seed, [r.paths, steps, n])?;
            &native.rates
        }
        DrawSource::SuppliedStandardNormals { values: z } => {
            if z.len() != values {
                return Err("controlled joint draw shape mismatch".into());
            }
            for &v in z {
                range(v, -100., 100., "controlled standard normal")?;
            }
            z
        }
    };
    let mut increments = vec![0.; values];
    let mut terminal = vec![0.; r.paths * n];
    let mut mean = vec![0.; n];
    for path in 0..r.paths {
        check()?;
        for (step, &dt) in r.durations_years.iter().enumerate() {
            check()?;
            let offset = (path * steps + step) * n;
            for i in 0..n {
                let raw = (0..n).map(|j| root[i * n + j] * z[offset + j]).sum::<f64>();
                let delta = raw * dt.sqrt();
                if !delta.is_finite() {
                    return Err("joint increment overflow".into());
                }
                increments[offset + i] = delta;
                terminal[path * n + i] += delta;
                mean[i] += raw;
            }
        }
    }
    let samples = (r.paths * steps) as f64;
    for v in &mut mean {
        *v /= samples;
    }
    let mut sample_cov = vec![0.; n * n];
    for path in 0..r.paths {
        check()?;
        for (step, &dt) in r.durations_years.iter().enumerate() {
            check()?;
            let offset = (path * steps + step) * n;
            for i in 0..n {
                let a = increments[offset + i] / dt.sqrt() - mean[i];
                for j in 0..n {
                    sample_cov[i * n + j] +=
                        a * (increments[offset + j] / dt.sqrt() - mean[j]) / samples;
                }
            }
        }
    }
    Ok(JointGaussianResult {
        source_revision: r.source_revision.clone(),
        shared_draw_id: r.shared_draw_id.clone(),
        drivers: r.drivers.clone(),
        shape: [r.paths, steps, n],
        durations_years: r.durations_years.clone(),
        retained_factors: r.retained_factors,
        increments,
        terminal_changes: terminal,
        diagnostics,
        sample_mean_annualized_innovation: mean,
        sample_annual_covariance: sample_cov,
    })
}
