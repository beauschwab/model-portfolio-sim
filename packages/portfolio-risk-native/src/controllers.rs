//! Public financial controllers. Data adapters never schedule dependent stages.
use crate::{
    accounting_lifecycle::AccountingRequest, forecast_lifecycle::Forecast, lifecycle_market::finite,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    ForecastCompile {
        request: Box<crate::forecast_input::Request>,
    },
    Calibrate {
        books: Box<AccountingRequest>,
    },
    Spread {
        oas: BTreeMap<String, Vec<f64>>,
        shift: f64,
    },
    ForecastReplay {
        books: Box<AccountingRequest>,
        forecast: Forecast,
    },
    DriverFit {
        dates: Vec<i32>,
        values: Vec<[f64; 4]>,
        training_end: i32,
        count: Option<usize>,
    },
}
impl Request {
    pub fn run(self) -> Result<Value, String> {
        match self {
            Self::ForecastCompile { request } => request.run(),
            Self::Calibrate { books } => Ok(json!(crate::incremental::calibrate_books(&books)?)),
            Self::Spread { mut oas, shift } => {
                finite(&[shift])?;
                for (name, values) in &mut oas {
                    finite(values)?;
                    if ["mbs", "loans", "debt", "cds"].contains(&name.as_str()) {
                        for v in values.iter_mut() {
                            *v += shift;
                        }
                    }
                    finite(values)?;
                }
                Ok(json!(oas))
            }
            Self::ForecastReplay {
                mut books,
                forecast,
            } => {
                if books.forecast.is_some()
                    || books.draws.is_some()
                    || !books.anchors.is_empty()
                    || books.deposit_initial_rate.is_some()
                {
                    return Err("forecast replay requires original unconditioned inputs".into());
                }
                books.capture_anchor = true;
                let mut base = books.run()?;
                books.anchors = std::mem::take(&mut base.anchors);
                books.deposit_initial_rate = base.deposit_initial_rate;
                books.forecast = Some(forecast);
                books.capture_anchor = false;
                portfolio_compute_control::checkpoint()?;
                let conditional = books.run()?;
                let delta: Vec<_> = conditional.monthly["nii"]
                    .iter()
                    .zip(&base.monthly["nii"])
                    .map(|(a, b)| a - b)
                    .collect();
                finite(&delta)?;
                Ok(json!({"base":base,"conditional":conditional,"delta_nii":delta}))
            }
            Self::DriverFit {
                dates,
                values,
                training_end,
                count,
            } => driver_fit(&dates, &values, training_end, count),
        }
    }
}
fn driver_fit(
    dates: &[i32],
    values: &[[f64; 4]],
    cutoff: i32,
    count: Option<usize>,
) -> Result<Value, String> {
    crate::conventions::Date::ordinal(cutoff)?;
    if dates.len() != values.len()
        || dates.len() > 1_000_000
        || dates.windows(2).any(|w| w[0] >= w[1])
    {
        return Err("history must have unique increasing dates and aligned observations".into());
    }
    for (&date, row) in dates.iter().zip(values) {
        crate::conventions::Date::ordinal(date)?;
        finite(row)?;
    }
    let n = dates.partition_point(|&d| d <= cutoff);
    let held = dates.len() - n;
    if n < 30 || held < 10 {
        return Err("at least 30 training and 10 subsequent holdout observations required".into());
    }
    let mut mean = [0.; 4];
    let mut covariance = [[0.; 4]; 4];
    for row in &values[..n] {
        for j in 0..4 {
            mean[j] += row[j];
        }
    }
    for v in &mut mean {
        *v /= n as f64;
    }
    for row in &values[..n] {
        for j in 0..4 {
            for k in 0..4 {
                covariance[j][k] += (row[j] - mean[j]) * (row[k] - mean[k]);
            }
        }
    }
    for row in &mut covariance {
        for v in row {
            *v /= (n - 1) as f64;
        }
    }
    let mut bias = [0.; 4];
    let mut rmse = [0.; 4];
    let mut outside = [0.; 4];
    for row in &values[n..] {
        for j in 0..4 {
            let d = row[j] - mean[j];
            bias[j] += row[j];
            rmse[j] += d * d;
            if d.abs() / covariance[j][j].max(1e-16).sqrt() > 3. {
                outside[j] += 1.;
            }
        }
    }
    for j in 0..4 {
        bias[j] = bias[j] / held as f64 - mean[j];
        rmse[j] = (rmse[j] / held as f64).sqrt();
        outside[j] /= held as f64;
    }
    let mut selected = Vec::new();
    if let Some(count) = count {
        if !(1..=8).contains(&count) {
            return Err("scenario count must be in [1,8]".into());
        }
        let cov = nalgebra::Matrix4::from_fn(|r, c| covariance[r][c]);
        let svd = cov.svd(true, true);
        let tolerance = svd.singular_values.max() * 1e-15;
        let inverse = svd.pseudo_inverse(tolerance).map_err(|e| e.to_string())?;
        let mut distances = Vec::with_capacity(n);
        for (i, row) in values[..n].iter().enumerate() {
            let delta = nalgebra::Vector4::from_fn(|j, _| row[j] - mean[j]);
            let distance = delta.dot(&(inverse * delta));
            finite(&[distance])?;
            distances.push((i, distance));
        }
        distances.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        for (i, _) in distances.into_iter().rev().take(count) {
            let v = values[i];
            if !(-0.5..=0.5).contains(&v[0])
                || !(-0.5..=0.5).contains(&v[1])
                || !(0. ..=1.).contains(&v[2])
                || !(0. ..=20.).contains(&v[3])
            {
                return Err("observed drivers exceed the stress specification domain".into());
            }
            selected.push(i);
        }
    }
    for v in [&mean[..], &bias[..], &rmse[..], &outside[..]] {
        finite(v)?;
    }
    for row in &covariance {
        finite(row)?;
    }
    Ok(
        json!({"training_rows":n,"holdout_rows":held,"mean":mean,"covariance":covariance,
        "holdout_bias":bias,"holdout_rmse":rmse,"holdout_outside_three_sigma":outside,"selected":selected}),
    )
}
