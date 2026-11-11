//! Built-in calibration controllers. Histories enter as raw columns; regression,
//! rank decisions, dynamics, diagnostics and PCA stay inside Rust.
use crate::math::arrays::View;
pub use crate::math::least_squares::Fit;
use crate::math::least_squares::{self, Model};
use crate::models::volatility::volatility;
use nalgebra::{linalg::SymmetricEigen, DMatrix, DVector};

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

// History borrows caller input. Reserve every design/copy/solver/report phase
// before allocating the design; no request can supply a numerical allowance.
fn history_admission(m: usize, n: usize) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    let bound = crate::math::checked_least_squares::WorkBound::new(m, n)?;
    let preprocessing = m
        .checked_mul(n)
        .and_then(|v| v.checked_mul(8))
        .and_then(|v| v.checked_add(32usize.checked_mul(m)?))
        .and_then(|v| v.checked_add(64usize.checked_mul(n)?))
        .ok_or("history numerical capacity overflow")?;
    let work = bound
        .total()
        .checked_add(preprocessing)
        .ok_or("history numerical capacity overflow")?;
    if work > 4_000_000_000 || bound.bytes() > 128 << 20 {
        return Err(format!(
            "history numerical admission exceeded: work={work}, bytes={}",
            bound.bytes()
        ));
    }
    Ok(())
}
fn least_squares(x: DMatrix<f64>, y: &[f64]) -> Result<Vec<f64>, String> {
    use crate::math::checked_least_squares::{solve_with_policy, LegacyPolicy};
    Ok(solve_with_policy(
        &x,
        y,
        LegacyPolicy::History,
        portfolio_compute_control::checkpoint,
    )?
    .coefficients)
}

fn variance(x: &[f64]) -> Result<f64, String> {
    let total = x.iter().try_fold(-0., |s, v| {
        portfolio_compute_control::checkpoint()?;
        Ok::<_, String>(s + v)
    })?;
    let mean = total / x.len() as f64;
    let squared = x.iter().try_fold(-0., |s, v| {
        portfolio_compute_control::checkpoint()?;
        Ok::<_, String>(s + (v - mean).powi(2))
    })?;
    Ok(squared / x.len() as f64)
}

#[derive(Clone)]
pub struct CurrentCouponFit {
    pub beta: Vec<f64>,
    pub lambda: f64,
    pub r_squared: f64,
}

/// History rows: four swap rates, six volatility features, observed coupon.
pub fn fit_current_coupon(history: &[f64]) -> Result<CurrentCouponFit, String> {
    if !history.len().is_multiple_of(11) || history.len() < 22 {
        return Err("current coupon history requires finite rows with 11 columns".into());
    }
    let n = history.len() / 11;
    history_admission(n, 11)?;
    if history.iter().any(|x| !x.is_finite()) {
        return Err("current coupon history requires finite rows with 11 columns".into());
    }
    let mut design = Vec::with_capacity(history.len());
    let mut y = Vec::with_capacity(n);
    for row in history.chunks_exact(11) {
        portfolio_compute_control::checkpoint()?;
        design.extend_from_slice(&row[..10]);
        design.push(1.);
        y.push(row[10]);
    }
    let beta = least_squares(DMatrix::from_row_slice(n, 11, &design), &y)?;
    let fair: Vec<_> = design
        .chunks_exact(11)
        .map(|r| {
            portfolio_compute_control::checkpoint()?;
            Ok::<_, String>(r.iter().zip(&beta).map(|(a, b)| a * b).sum::<f64>())
        })
        .collect::<Result<_, _>>()?;
    let (mut numerator, mut denominator) = (0., 0.);
    for i in 1..n {
        portfolio_compute_control::checkpoint()?;
        let gap = fair[i] - y[i - 1];
        numerator += gap * (y[i] - y[i - 1]);
        denominator += gap * gap;
    }
    if denominator == 0. {
        return Err("current coupon adjustment is unidentified in constant history".into());
    }
    let lambda = (numerator / denominator).clamp(0.02, 1.);
    let residual: Vec<_> = y.iter().zip(&fair).map(|(a, b)| a - b).collect();
    let var = variance(&y)?;
    let r_squared = if var == 0. {
        0.
    } else {
        1. - variance(&residual)? / var
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
    if history.len() < 3 || !dt.is_finite() || dt <= 0. {
        return Err(
            "spread history requires at least three finite observations and positive dt".into(),
        );
    }
    history_admission(history.len() - 1, 2)?;
    if history.iter().any(|x| !x.is_finite()) {
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
        variance(&residual)?.sqrt() / dt.sqrt(),
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

#[cfg(test)]
mod history_regression_tests {
    use super::*;
    #[test]
    fn history_budget_boundaries_are_checked_without_allocating_designs() {
        for m in [2usize, 3, 10, 11, 12, 21, 22, 100_000, 118_437] {
            history_admission(m, 11).unwrap();
        }
        assert!(history_admission(118_438, 11)
            .unwrap_err()
            .contains("work=4000006734"));
        history_admission(1_198_357, 2).unwrap();
        assert!(history_admission(1_198_358, 2)
            .unwrap_err()
            .contains("bytes="));
        assert!(history_admission(usize::MAX, 11).is_err());
    }
    #[test]
    fn actual_collinear_current_coupon_histories_retain_financial_outputs() {
        for n in [2usize, 3, 10, 11, 12, 16, 21, 22, 32, 64, 128, 256, 512] {
            for shape in 0..4 {
                let x: Vec<_> = (0..n).map(|i| i as f64 - (n - 1) as f64 / 2.).collect();
                let sd = (x.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
                let mut history = Vec::new();
                for x in x.iter().map(|x| x / sd) {
                    for j in 0..10 {
                        history.push(match shape {
                            0 => {
                                if j < 2 {
                                    x
                                } else {
                                    0.
                                }
                            }
                            1 => {
                                if j == 0 {
                                    x
                                } else if j == 1 {
                                    7. * x
                                } else {
                                    0.
                                }
                            }
                            2 => {
                                if j < 4 {
                                    0.02 + 0.001 * (j + 1) as f64 * x
                                } else {
                                    0.1 + 0.01 * (j - 3) as f64 * x
                                }
                            }
                            _ => {
                                if j < 4 {
                                    0.02 + 0.001 * x
                                } else {
                                    0.1 + 0.01 * x
                                }
                            }
                        });
                    }
                    history.push(0.03 + 0.001 * x);
                }
                let fit = fit_current_coupon(&history).unwrap();
                for row in history.chunks_exact(11) {
                    let predicted = row[..10]
                        .iter()
                        .chain(std::iter::once(&1.))
                        .zip(&fit.beta)
                        .map(|(x, b)| x * b)
                        .sum::<f64>();
                    assert!(
                        (predicted - row[10]).abs() <= 1e-13 + 1e-11 * row[10].abs(),
                        "n{n}/shape{shape}: {predicted} vs {}",
                        row[10]
                    );
                }
                assert!(
                    (fit.lambda - 1.).abs() <= 1e-12 + 1e-10,
                    "n{n}/shape{shape}: {}",
                    fit.lambda
                );
                assert!(
                    (fit.r_squared - 1.).abs() <= 1e-12 + 1e-10,
                    "n{n}/shape{shape}: {}",
                    fit.r_squared
                );
            }
        }
    }
    #[test]
    fn valid_history_legacy_coefficients_are_bit_identical() {
        for (m, n) in [(2, 11), (10, 11), (11, 11), (12, 11), (64, 11), (4, 2)] {
            let a = DMatrix::from_fn(m, n, |i, j| {
                if j + 1 == n {
                    1.
                } else {
                    ((i * 17 + j * 7) % 31) as f64 / 31.
                }
            });
            let y: Vec<_> = (0..m).map(|i| 0.03 + 0.001 * i as f64).collect();
            let old = nalgebra::linalg::SVD::try_new(a.clone(), true, true, f64::EPSILON, 100_000)
                .unwrap();
            let old_beta = old
                .solve(
                    &DVector::from_column_slice(&y),
                    f64::EPSILON * m.max(n) as f64 * old.singular_values[0],
                )
                .unwrap();
            let fit = least_squares(a, &y).unwrap();
            for (x, y) in fit.iter().zip(old_beta.iter()) {
                assert_eq!(x.to_bits(), y.to_bits());
            }
        }
    }
    #[test]
    fn cancelled_history_does_not_construct_regression() {
        let control = portfolio_compute_control::Control::new(1000).unwrap();
        control.cancel();
        let error = portfolio_compute_control::scope(Some(control), || {
            fit_current_coupon(&[0.; 22]).map(|_| ())
        })
        .unwrap_err();
        assert_eq!(error, "COMPUTE_CANCELLED");
    }
}
