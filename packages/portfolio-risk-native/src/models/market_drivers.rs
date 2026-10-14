//! Versioned observed market-driver fit with chronological holdout diagnostics.
//!
//! This is an analysis contract only. It estimates training moments from the
//! supplied rows, evaluates later rows without changing the fit, and retains
//! source/split identity. It does not select a production pricing measure,
//! bootstrap a curve, or silently replace the legacy market configuration.

use crate::runtime::controllers;
use portfolio_compute_control::checkpoint;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const SCHEMA: &str = "market-driver-calibration-1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_id: String,
    pub model_version: String,
    pub input_revision: String,
    pub source_id: String,
    pub source_revision: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub dates: Vec<i32>,
    pub values: Vec<[f64; 4]>,
    pub training_end: i32,
    pub scenario_count: Option<usize>,
    /// Optional Bartlett/HAC lag for mean-parameter uncertainty. Zero is an
    /// explicit iid moment estimate; no serial-dependence assumption is hidden.
    pub uncertainty_lag: Option<usize>,
}

fn label(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!("{name} must be nonempty bounded text"));
    }
    Ok(())
}

fn validate(request: &Request) -> Result<(), String> {
    if request.schema != SCHEMA || request.model_version != SCHEMA {
        return Err("market driver calibration schema/version mismatch".into());
    }
    for (value, name) in [
        (&request.model_id, "model_id"),
        (&request.model_version, "model_version"),
        (&request.input_revision, "input_revision"),
        (&request.source_id, "source_id"),
        (&request.source_revision, "source_revision"),
        (&request.training_split_id, "training_split_id"),
        (&request.holdout_split_id, "holdout_split_id"),
    ] {
        label(value, name)?;
    }
    if request.training_split_id == request.holdout_split_id {
        return Err("training and holdout split identities must differ".into());
    }
    if request.dates.len() != request.values.len() {
        return Err("market history dates and values must be aligned".into());
    }
    if request.dates.len() > 1_000_000 {
        return Err("market history admission exceeded".into());
    }
    if request.uncertainty_lag.is_some_and(|lag| lag > 64) {
        return Err("market uncertainty lag must be in [0,64]".into());
    }
    Ok(())
}

fn uncertainty(
    dates: &[i32],
    values: &[[f64; 4]],
    cutoff: i32,
    lag: usize,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Value, String> {
    let n = dates.partition_point(|&date| date <= cutoff);
    if n < 2 || lag >= n {
        return Err("market uncertainty lag must be less than training rows".into());
    }
    let mut mean = [0.0; 4];
    for row in &values[..n] {
        check()?;
        for (j, value) in row.iter().enumerate() {
            mean[j] += value;
        }
    }
    for value in &mut mean {
        *value /= n as f64;
    }
    let mut mean_standard_error = [0.0; 4];
    for j in 0..4 {
        check()?;
        let gamma0 = values[..n]
            .iter()
            .map(|row| (row[j] - mean[j]).powi(2))
            .sum::<f64>()
            / n as f64;
        let mut long_run = gamma0;
        for distance in 1..=lag {
            check()?;
            let gamma = values[distance..n]
                .iter()
                .zip(&values[..n - distance])
                .map(|(left, right)| (left[j] - mean[j]) * (right[j] - mean[j]))
                .sum::<f64>()
                / n as f64;
            let weight = (lag + 1 - distance) as f64 / (lag + 1) as f64;
            long_run += 2.0 * weight * gamma;
        }
        if !long_run.is_finite() || long_run < -1e-12 * gamma0.max(1.0) {
            return Err("market HAC long-run variance is outside its numeric domain".into());
        }
        mean_standard_error[j] = (long_run.max(0.0) / n as f64).sqrt();
    }
    Ok(json!({
        "method": "bartlett_hac_mean_standard_error",
        "lag": lag,
        "training_rows": n,
        "mean_standard_error": mean_standard_error,
        "serial_dependence_adjusted": lag > 0,
    }))
}

pub fn evaluate(request: &Request) -> Result<Value, String> {
    evaluate_with_check(request, checkpoint)
}

pub fn evaluate_with_check(
    request: &Request,
    check: impl FnMut() -> Result<(), String>,
) -> Result<Value, String> {
    let mut check = check;
    validate(request)?;
    let mut diagnostics = controllers::driver_fit_with_check(
        &request.dates,
        &request.values,
        request.training_end,
        request.scenario_count,
        &mut check,
    )?;
    let lag = request.uncertainty_lag.unwrap_or(0);
    let uncertainty = uncertainty(
        &request.dates,
        &request.values,
        request.training_end,
        lag,
        &mut check,
    )?;
    diagnostics["uncertainty"] = uncertainty;
    Ok(json!({
        "schema": SCHEMA,
        "model_id": request.model_id,
        "model_version": request.model_version,
        "input_revision": request.input_revision,
        "source_id": request.source_id,
        "source_revision": request.source_revision,
        "training_split_id": request.training_split_id,
        "holdout_split_id": request.holdout_split_id,
        "training_end": request.training_end,
        "source_status": "observed_rows_training_moments_chronological_holdout",
        "adoption_eligible": false,
        "diagnostics": diagnostics,
    }))
}
