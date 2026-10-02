//! Built-in calibration controllers. Histories enter as raw columns; regression,
//! rank decisions, dynamics, diagnostics and PCA stay inside Rust.
pub use crate::least_squares::Fit;
use crate::least_squares::{self, Model};
use crate::quant::{volatility, View};
use nalgebra::{
    linalg::{SymmetricEigen, SVD},
    DMatrix, DVector,
};

/// Raw observed (fed funds, deposit rate) rows. Both the static initialization
/// and joint asymmetric recurrence fit are owned here, including differentiation.
pub fn fit_deposits(history: &[f64]) -> Result<Fit, String> {
    crate::compute_span!("deposit_fit");
    if history.len() < 6
        || !history.len().is_multiple_of(2)
        || history.iter().any(|v| !v.is_finite())
    {
        return Err("deposit history requires at least three finite rate pairs".into());
    }
    struct Deposit<'a> {
        history: &'a [f64],
        dynamic: bool,
    }
    impl Model for Deposit<'_> {
        fn jacobian(&self, p: &[f64]) -> Option<DMatrix<f64>> {
            let mut out = DMatrix::zeros(self.history.len() / 2, p.len());
            let mut previous = self.history[1];
            let mut gradient = [0.; 7];
            for (i, row) in self.history.chunks_exact(2).enumerate() {
                let x = row[0];
                let s = 1. / (1. + (-p[3] * (x - p[4])).exp());
                let common = x * (p[2] - p[1]) * s * (1. - s);
                let eq_gradient = [
                    1.,
                    x * (1. - s),
                    x * s,
                    common * (x - p[4]),
                    -common * p[3],
                    0.,
                    0.,
                ];
                if self.dynamic {
                    if i == 0 {
                        continue;
                    }
                    let eq = p[0] + x * (p[1] + (p[2] - p[1]) * s);
                    let gap = eq - previous;
                    let index = if gap > 0. { 5 } else { 6 };
                    let lambda = p[index];
                    for j in 0..7 {
                        gradient[j] = (1. - lambda) * gradient[j] + lambda * eq_gradient[j];
                    }
                    gradient[index] += gap;
                    previous += lambda * gap;
                } else {
                    gradient = eq_gradient;
                }
                for j in 0..p.len() {
                    out[(i, j)] = gradient[j];
                }
            }
            Some(out)
        }
        fn residual(&self, p: &[f64]) -> Result<DVector<f64>, String> {
            let mut previous = self.history[1];
            Ok(DVector::from_iterator(
                self.history.len() / 2,
                self.history.chunks_exact(2).enumerate().map(|(i, r)| {
                    let eq =
                        p[0] + r[0] * (p[1] + (p[2] - p[1]) / (1. + (-p[3] * (r[0] - p[4])).exp()));
                    let predicted = if self.dynamic {
                        if i > 0 {
                            let gap = eq - previous;
                            previous += (if gap > 0. { p[5] } else { p[6] }) * gap;
                        }
                        previous
                    } else {
                        eq
                    };
                    predicted - r[1]
                }),
            ))
        }
    }
    let lower = [-0.01, 0., 0.05, 10., 0., 0.01, 0.01];
    let upper = [0.02, 0.5, 1., 500., 0.08, 1., 1.];
    let mut initial = least_squares::fit(
        &Deposit {
            history,
            dynamic: false,
        },
        &[0.001, 0.05, 0.5, 100., 0.02],
        &lower[..5],
        &upper[..5],
    )?
    .x;
    initial.extend_from_slice(&[0.25, 0.25]);
    least_squares::fit(
        &Deposit {
            history,
            dynamic: true,
        },
        &initial,
        &lower,
        &upper,
    )
}

/// Rebonato volatility fit from quote triples (expiry, tenor, lognormal vol).
/// Analytic value/Jacobian, trust-region iterations and diagnostics run in Rust.
pub fn fit_abcd(
    quotes: &[f64],
    forwards: &[f64],
    dfs: &[f64],
    loadings: &[f64],
    initial: &[f64],
    grid: [f64; 2],
) -> Result<Fit, String> {
    let n = forwards.len();
    if n == 0
        || dfs.len() != n + 1
        || loadings.is_empty()
        || !loadings.len().is_multiple_of(n)
        || initial.len() != 4
        || quotes.is_empty()
        || !quotes.len().is_multiple_of(3)
        || quotes
            .iter()
            .chain(forwards)
            .chain(dfs)
            .chain(loadings)
            .chain(&grid)
            .any(|x| !x.is_finite())
        || grid[0] <= 0.
        || dfs.iter().any(|v| *v <= 0.)
    {
        return Err("invalid volatility fit inputs".into());
    }
    let mut queries = Vec::with_capacity(quotes.len() / 3 * 4);
    for row in quotes.chunks_exact(3) {
        let first = (row[0] / grid[0]).round_ties_even();
        let count = (row[1] / grid[0]).round_ties_even();
        if row[0] <= 0. || row[1] <= 0. || row[2] < 0. || first >= n as f64 || count < 1. {
            return Err("invalid volatility quote expiry, tenor or target".into());
        }
        let end = (first as usize).saturating_add(count as usize).min(n);
        if dfs[first as usize] == dfs[end] {
            return Err("zero swap rate in volatility calibration".into());
        }
        queries.extend_from_slice(&[0., row[0], first, count.min(n as f64)]);
    }
    struct Vol<'a> {
        quotes: &'a [f64],
        queries: &'a [f64],
        forwards: &'a [f64],
        dfs: &'a [f64],
        loadings: &'a [f64],
        grid: [f64; 2],
    }
    impl Vol<'_> {
        fn values(&self, p: &[f64]) -> Vec<f64> {
            let n = self.forwards.len();
            volatility(&[
                View {
                    v: self.queries,
                    shape: [self.quotes.len() / 3, 4, 1],
                },
                View {
                    v: p,
                    shape: [4, 1, 1],
                },
                View {
                    v: self.forwards,
                    shape: [n, 1, 1],
                },
                View {
                    v: self.dfs,
                    shape: [n + 1, 1, 1],
                },
                View {
                    v: self.loadings,
                    shape: [n, self.loadings.len() / n, 1],
                },
                View {
                    v: &self.grid[..1],
                    shape: [1, 1, 1],
                },
                View {
                    v: &self.grid[1..],
                    shape: [1, 1, 1],
                },
            ])
            .remove(0)
        }
    }
    impl Model for Vol<'_> {
        fn residual(&self, p: &[f64]) -> Result<DVector<f64>, String> {
            let values = self.values(p);
            Ok(DVector::from_iterator(
                self.quotes.len() / 3,
                values
                    .chunks_exact(5)
                    .zip(self.quotes.chunks_exact(3))
                    .map(|(v, q)| v[0] - q[2]),
            ))
        }
        fn jacobian(&self, p: &[f64]) -> Option<DMatrix<f64>> {
            let values = self.values(p);
            Some(DMatrix::from_fn(self.quotes.len() / 3, 4, |r, c| {
                values[r * 5 + c + 1]
            }))
        }
    }
    least_squares::fit(
        &Vol {
            quotes,
            queries: &queries,
            forwards,
            dfs,
            loadings,
            grid,
        },
        initial,
        &[-0.5, -0.5, 0.01, 0.],
        &[1., 1., 5., 1.],
    )
}

fn least_squares(x: DMatrix<f64>, y: &[f64]) -> Result<Vec<f64>, String> {
    let size = x.nrows().max(x.ncols());
    let svd = SVD::try_new(x, true, true, f64::EPSILON, 100_000)
        .ok_or("regression SVD did not converge")?;
    // NumPy's rcond=None: machine epsilon times max(m,n), relative to sigma_max.
    let cutoff = f64::EPSILON * size as f64 * svd.singular_values[0];
    let fit = svd
        .solve(&DVector::from_column_slice(y), cutoff)
        .map_err(str::to_string)?;
    if fit.iter().any(|x| !x.is_finite()) {
        return Err("nonfinite regression fit".into());
    }
    Ok(fit.as_slice().to_vec())
}

fn variance(x: &[f64]) -> f64 {
    let mean = x.iter().sum::<f64>() / x.len() as f64;
    x.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / x.len() as f64
}

#[derive(Clone)]
pub struct CurrentCouponFit {
    pub beta: Vec<f64>,
    pub lambda: f64,
    pub r_squared: f64,
}

/// History rows: four swap rates, six volatility features, observed coupon.
pub fn fit_current_coupon(history: &[f64]) -> Result<CurrentCouponFit, String> {
    if !history.len().is_multiple_of(11)
        || history.len() < 22
        || history.iter().any(|x| !x.is_finite())
    {
        return Err("current coupon history requires finite rows with 11 columns".into());
    }
    let n = history.len() / 11;
    let mut design = Vec::with_capacity(history.len());
    let mut y = Vec::with_capacity(n);
    for row in history.chunks_exact(11) {
        design.extend_from_slice(&row[..10]);
        design.push(1.);
        y.push(row[10]);
    }
    let beta = least_squares(DMatrix::from_row_slice(n, 11, &design), &y)?;
    let fair: Vec<_> = design
        .chunks_exact(11)
        .map(|r| r.iter().zip(&beta).map(|(a, b)| a * b).sum::<f64>())
        .collect();
    let (mut numerator, mut denominator) = (0., 0.);
    for i in 1..n {
        let gap = fair[i] - y[i - 1];
        numerator += gap * (y[i] - y[i - 1]);
        denominator += gap * gap;
    }
    if denominator == 0. {
        return Err("current coupon adjustment is unidentified in constant history".into());
    }
    let lambda = (numerator / denominator).clamp(0.02, 1.);
    let residual: Vec<_> = y.iter().zip(&fair).map(|(a, b)| a - b).collect();
    let var = variance(&y);
    let r_squared = if var == 0. {
        0.
    } else {
        1. - variance(&residual) / var
    };
    if !lambda.is_finite() || !r_squared.is_finite() {
        return Err("nonfinite current coupon fit".into());
    }
    Ok(CurrentCouponFit {
        beta,
        lambda,
        r_squared,
    })
}

/// AR(1) fit, preserving the reference's clipped phi and residual convention.
pub fn fit_ps_spread(history: &[f64], dt: f64) -> Result<[f64; 3], String> {
    if history.len() < 3 || !dt.is_finite() || dt <= 0. || history.iter().any(|x| !x.is_finite()) {
        return Err(
            "spread history requires at least three finite observations and positive dt".into(),
        );
    }
    let design: Vec<_> = history[..history.len() - 1]
        .iter()
        .flat_map(|x| [*x, 1.])
        .collect();
    let fit = least_squares(
        DMatrix::from_row_slice(history.len() - 1, 2, &design),
        &history[1..],
    )?;
    let phi = fit[0].clamp(0., 0.9995);
    let residual: Vec<_> = history
        .windows(2)
        .map(|x| x[1] - (phi * x[0] + fit[1]))
        .collect();
    let out = [
        (1. - phi) / dt,
        fit[1] / (1. - phi),
        variance(&residual).sqrt() / dt.sqrt(),
    ];
    if out.iter().any(|x| !x.is_finite()) {
        return Err("nonfinite spread fit".into());
    }
    Ok(out)
}

/// PCA-reduced exp-decay correlation. First-row signs (-,-,+) define the built-in
/// factor/draw orientation, independently of an eigensolver's arbitrary signs.
pub fn factor_loadings(n: usize, k: usize, tenor: f64, decay: f64) -> Result<Vec<f64>, String> {
    if n < k
        || n == 0
        || n > 1024
        || k == 0
        || k > 3
        || !tenor.is_finite()
        || tenor <= 0.
        || !decay.is_finite()
        || decay <= 0.
    {
        return Err("unsupported PCA dimensions or correlation parameters".into());
    }
    let matrix = DMatrix::from_fn(n, n, |i, j| {
        (-decay * ((i + 1) as f64 * tenor - (j + 1) as f64 * tenor).abs()).exp()
    });
    let eigen = SymmetricEigen::try_new(matrix, f64::EPSILON, 100_000)
        .ok_or("PCA eigensolver did not converge")?;
    let mut order: Vec<_> = (0..n).collect();
    order.sort_by(|&a, &b| eigen.eigenvalues[b].total_cmp(&eigen.eigenvalues[a]));
    let mut out = vec![0.; n * k];
    for (j, &column) in order.iter().take(k).enumerate() {
        let value = eigen.eigenvalues[column];
        if value <= 0. {
            return Err("nonpositive retained PCA eigenvalue".into());
        }
        let desired = if j < 2 { -1. } else { 1. };
        let sign = if eigen.eigenvectors[(0, column)].is_sign_positive() {
            desired
        } else {
            -desired
        };
        for i in 0..n {
            out[i * k + j] = sign * eigen.eigenvectors[(i, column)] * value.sqrt();
        }
    }
    for row in out.chunks_exact_mut(k) {
        let norm = row.iter().map(|x| x * x).sum::<f64>().sqrt();
        if !norm.is_finite() || norm == 0. {
            return Err("invalid PCA loading norm".into());
        }
        for x in row {
            *x /= norm;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standalone_joint_deposit_calibration() {
        let mut history = Vec::new();
        let mut rate = 0.001;
        for i in 0..360 {
            let ff = 0.04 + 0.035 * (i as f64 / 30.).sin();
            let eq = 0.001 + ff * (0.05 + 0.55 / (1. + (-150. * (ff - 0.025)).exp()));
            let gap = eq - rate;
            rate += if gap > 0. { 0.12 * gap } else { 0.35 * gap };
            history.extend_from_slice(&[ff, rate]);
        }
        let fit = fit_deposits(&history).unwrap();
        assert!(fit.rmse < 1e-6);
        assert!((fit.x[5] - 0.12).abs() < 1e-4);
        assert!((fit.x[6] - 0.35).abs() < 1e-4);
        assert!(fit_deposits(&[f64::INFINITY; 12]).is_err());
    }
    #[test]
    fn standalone_volatility_fit_and_invalid_inputs() {
        let forwards = vec![0.04; 80];
        let dfs: Vec<_> = (0..=80).map(|i| 1.01_f64.powi(-i)).collect();
        let loadings = vec![1.; 80];
        // Constant sigma=.1 and shift=0 gives exactly .1 Rebonato ATM vols.
        let quotes = [0.5, 1., 0.1, 1., 3., 0.1, 2., 5., 0.1, 3., 10., 0.1];
        let fit = fit_abcd(
            &quotes,
            &forwards,
            &dfs,
            &loadings,
            &[0.05, 0.1, 0.5, 0.12],
            [0.25, 0.],
        )
        .unwrap();
        assert!(fit.rmse < 1e-6);
        assert!(fit_abcd(
            &quotes,
            &forwards,
            &vec![1.; 81],
            &loadings,
            &[0.05, 0.1, 0.5, 0.12],
            [0.25, 0.]
        )
        .is_err());
    }
    #[test]
    fn exact_linear_regression_and_minimum_norm_rank_deficiency() {
        let x = DMatrix::from_row_slice(3, 2, &[1., 1., 2., 1., 3., 1.]);
        let fit = least_squares(x, &[3., 5., 7.]).unwrap();
        assert!((fit[0] - 2.).abs() < 1e-12 && (fit[1] - 1.).abs() < 1e-12);
        let fit = least_squares(
            DMatrix::from_row_slice(3, 2, &[1., 1., 2., 2., 3., 3.]),
            &[2., 4., 6.],
        )
        .unwrap();
        assert!((fit[0] - 1.).abs() < 1e-12 && (fit[1] - 1.).abs() < 1e-12);
    }
    #[test]
    fn pca_rows_and_orientation() {
        let b = factor_loadings(161, 3, 0.25, 0.1).unwrap();
        assert!(b[0] < 0. && b[1] < 0. && b[2] > 0.);
        for r in b.chunks_exact(3) {
            assert!((r.iter().map(|x| x * x).sum::<f64>() - 1.).abs() < 1e-12);
        }
        assert!((b[0] + 0.506548097).abs() < 1e-9);
    }
}
