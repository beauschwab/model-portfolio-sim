//! Bounded, backward-checked least squares for native history and continuation.
//!
//! This checks a computed decomposition; it does not certify exact mathematical
//! rank when a singular value lies at a floating-point cutoff.
use nalgebra::{DMatrix, DVector};

const LEGACY_ITERATIONS: usize = 64;
const JACOBI_SWEEPS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkBound {
    candidate: usize,
    jacobi: usize,
    check: usize,
    bytes: usize,
    total: usize,
    transpose: usize,
}
impl WorkBound {
    pub(crate) fn new(m: usize, k: usize) -> Result<Self, String> {
        if m == 0 || k == 0 {
            return Err("checked least-squares requires nonempty dimensions".into());
        }
        let wide = m < k;
        let (m, k) = (m.max(k), m.min(k));
        let value = (|| {
            let mk = m.checked_mul(k)?;
            let k2 = k.checked_mul(k)?;
            let k3 = k2.checked_mul(k)?;
            let mk2 = mk.checked_mul(k)?;
            let candidate = 32usize
                .checked_mul(mk2)?
                .checked_add(8usize.checked_mul(k3)?)?
                .checked_add(
                    (LEGACY_ITERATIONS + 1).checked_mul(
                        2usize
                            .checked_mul(k2)?
                            .checked_add(8usize.checked_mul(k)?)?
                            .checked_mul(m.checked_add(k)?)?,
                    )?,
                )?
                .checked_add(256usize.checked_mul(LEGACY_ITERATIONS)?.checked_mul(k)?)?
                .checked_add(8usize.checked_mul(mk)?)?;
            let pairs = k.checked_mul(k - 1)? / 2;
            let jacobi = JACOBI_SWEEPS
                .checked_mul(pairs)?
                .checked_mul(2usize.checked_mul(m)?.checked_add(k)?.checked_add(32)?)?
                .checked_add(16usize.checked_mul(mk)?)?
                .checked_add(12usize.checked_mul(k2)?)?
                .checked_add(16usize.checked_mul(m)?)?
                .checked_add(128usize.checked_mul(k)?)?;
            let check = 3usize
                .checked_mul(mk2)?
                .checked_add(2usize.checked_mul(k3)?)?
                .checked_add(12usize.checked_mul(mk)?)?
                .checked_add(8usize.checked_mul(m)?)?
                .checked_add(16usize.checked_mul(k2)?)?
                .checked_add(64usize.checked_mul(k)?)?;
            let bytes = 5usize
                .checked_mul(mk)?
                .checked_add(4usize.checked_mul(k2)?)?
                .checked_add(4usize.checked_mul(m)?)?
                .checked_add(32usize.checked_mul(k)?)?
                .checked_mul(8)?
                .checked_add(1024)?;
            // Extra wide-only scans cover long coefficients and thin right factors.
            let check = if wide {
                check
                    .checked_add(4usize.checked_mul(mk)?)?
                    .checked_add(8usize.checked_mul(m)?)?
                    .checked_add(8usize.checked_mul(k)?)?
            } else {
                check
            };
            let transpose = if wide {
                4usize
                    .checked_mul(mk)?
                    .checked_add(2usize.checked_mul(k2)?)?
                    .checked_add(16usize.checked_mul(k)?)?
            } else {
                0
            };
            let bytes = if wide {
                bytes.checked_add(
                    8usize.checked_mul(
                        2usize
                            .checked_mul(mk)?
                            .checked_add(2usize.checked_mul(k2)?)?
                            .checked_add(4usize.checked_mul(m)?)?
                            .checked_add(16usize.checked_mul(k)?)?,
                    )?,
                )?
            } else {
                bytes
            };
            let total = candidate
                .checked_add(jacobi)?
                .checked_add(2usize.checked_mul(check)?)?
                .checked_add(transpose)?;
            Some(Self {
                candidate,
                jacobi,
                check,
                bytes,
                total,
                transpose,
            })
        })()
        .ok_or("checked least-squares capacity overflow")?;
        Ok(value)
    }
    pub(crate) fn total(self) -> usize {
        self.total
    }
    pub(crate) fn bytes(self) -> usize {
        self.bytes
    }
}

#[derive(Default, Debug)]
struct Meter {
    candidate: usize,
    jacobi: usize,
    check: usize,
    sweeps: usize,
    transpose: usize,
}
impl Meter {
    fn finish(&self, bound: WorkBound) -> Result<(), String> {
        let total = self
            .candidate
            .checked_add(self.jacobi)
            .and_then(|v| v.checked_add(self.check))
            .and_then(|v| v.checked_add(self.transpose))
            .ok_or("least-squares operation count overflow")?;
        if total > bound.total() || self.sweeps > JACOBI_SWEEPS {
            return Err("least-squares operation ticket exceeded".into());
        }
        Ok(())
    }
    fn jacobi(&mut self, amount: usize, upper: usize) -> Result<(), String> {
        self.jacobi = self
            .jacobi
            .checked_add(amount)
            .filter(|v| *v <= upper)
            .ok_or("Jacobi operation ticket exceeded")?;
        Ok(())
    }
    fn check(&mut self, amount: usize, upper: usize) -> Result<(), CheckError> {
        self.check = self
            .check
            .checked_add(amount)
            .filter(|v| *v <= upper)
            .ok_or_else(|| CheckError::Abort("numerical check operation ticket exceeded".into()))?;
        Ok(())
    }
}
#[derive(Debug)]
enum CheckError {
    Numerical(String),
    Abort(String),
}
impl From<String> for CheckError {
    fn from(value: String) -> Self {
        Self::Numerical(value)
    }
}
impl From<&str> for CheckError {
    fn from(value: &str) -> Self {
        Self::Numerical(value.into())
    }
}
impl CheckError {
    fn message(self) -> String {
        match self {
            Self::Numerical(s) | Self::Abort(s) => s,
        }
    }
}

#[derive(Clone, Debug)]
struct Factors {
    u: DMatrix<f64>,
    s: DVector<f64>,
    vt: DMatrix<f64>,
}
#[derive(Debug)]
pub(crate) struct Checked {
    pub(crate) coefficients: Vec<f64>,
    pub(crate) singular_values: Vec<f64>,
    pub(crate) rank: usize,
    #[cfg(test)]
    repaired: bool,
    #[cfg(test)]
    certificate: Certificate,
    #[cfg(test)]
    meter: Meter,
}
#[derive(Debug)]
struct Certificate {
    reconstruction: f64,
    u_orthogonality: f64,
    v_orthogonality: f64,
    relative_gradient: f64,
    minimum_norm_error: f64,
    retained_solve_error: f64,
}

// Neumaier summation is used only in the new checker/repair, never in the
// unchanged historical candidate's decomposition or coefficient reductions.
fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut e) = (0.0_f64, 0.0_f64);
    for x in values {
        let next = s + x;
        e += if s.abs() >= x.abs() {
            (s - next) + x
        } else {
            (x - next) + s
        };
        s = next;
    }
    s + e
}
fn norm(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0_f64, f64::hypot)
}
fn maximum(values: impl Iterator<Item = f64>) -> f64 {
    values.fold(0.0_f64, |a, b| a.max(b.abs()))
}
fn cutoff(m: usize, k: usize, s: f64) -> f64 {
    f64::EPSILON * m.max(k) as f64 * s
}

// Compute x*y/z without an avoidable intermediate overflow/underflow. Only
// the final, representable result is rescaled into the subnormal range.
fn multiply_divide(x: f64, y: f64, z: f64) -> Result<f64, String> {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() || z <= 0. {
        return Err("invalid scaled least-squares quotient".into());
    }
    if x == 0. || y == 0. {
        return Ok(0.);
    }
    fn parts(x: f64) -> (f64, i32) {
        let mut value = x.abs();
        let mut shift = 0;
        if value < f64::MIN_POSITIVE {
            value *= (1_u64 << 54) as f64;
            shift = -54;
        }
        let bits = value.to_bits();
        (
            f64::from_bits((bits & ((1_u64 << 52) - 1)) | (1023_u64 << 52)),
            ((bits >> 52) & 2047) as i32 - 1023 + shift,
        )
    }
    let (mx, ex) = parts(x);
    let (my, ey) = parts(y);
    let (mz, ez) = parts(z);
    let (mut mantissa, mut exponent) = (mx * my / mz, ex + ey - ez);
    if mantissa >= 2. {
        mantissa *= 0.5;
        exponent += 1;
    }
    if mantissa < 1. {
        mantissa *= 2.;
        exponent -= 1;
    }
    let value = if exponent > 1023 {
        f64::INFINITY
    } else if exponent >= -1022 {
        mantissa * f64::from_bits(((exponent + 1023) as u64) << 52)
    } else if exponent >= -1075 {
        mantissa * f64::MIN_POSITIVE * 2.0_f64.powi(exponent + 1022)
    } else {
        0.
    };
    if !value.is_finite() {
        return Err("least-squares coefficient exceeds finite domain".into());
    }
    Ok(value.copysign(x * y.signum()))
}

fn jacobi(
    a: &DMatrix<f64>,
    meter: &mut Meter,
    upper: usize,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Factors, String> {
    let (m, k) = a.shape();
    meter.jacobi(3 * m * k + k * k + 2 * k, upper)?;
    let scale = maximum(a.iter().copied());
    let mut b = a.map(|x| if scale == 0. { 0. } else { x / scale });
    let mut v = DMatrix::<f64>::identity(k, k);
    let mut norms: Vec<_> = (0..k).map(|j| norm((0..m).map(|i| b[(i, j)]))).collect();
    let mut converged = k == 1;
    for sweep in 0..JACOBI_SWEEPS {
        check()?;
        let mut changed = false;
        for p in 0..k {
            for q in p + 1..k {
                check()?;
                meter.jacobi(32, upper)?;
                let (np, nq) = (norms[p], norms[q]);
                if np == 0. || nq == 0. {
                    continue;
                }
                meter.jacobi(m, upper)?;
                let rho = sum((0..m).map(|i| (b[(i, p)] / np) * (b[(i, q)] / nq)));
                if !rho.is_finite() {
                    return Err("nonfinite Jacobi correlation".into());
                }
                if rho.abs() <= 8. * f64::EPSILON {
                    continue;
                }
                // alpha/beta/gamma share a common squared column scale. No
                // unscaled squared norm or product of large norms is formed.
                let r = np.min(nq) / np.max(nq);
                let delta = if np >= nq { r * r - 1. } else { 1. - r * r };
                let g = r * rho;
                let t = if delta == 0. {
                    rho.signum()
                } else {
                    2. * g / (delta + delta.signum() * delta.hypot(2. * g))
                };
                if !t.is_finite() || t == 0. {
                    return Err("Jacobi rotation cannot be represented safely".into());
                }
                let c = 1. / 1.0_f64.hypot(t);
                let s = c * t;
                let (mut new_p, mut new_q) = (0.0_f64, 0.0_f64);
                meter.jacobi(m + k, upper)?;
                for i in 0..m {
                    let (x, y) = (b[(i, p)], b[(i, q)]);
                    let (x, y) = (c * x - s * y, s * x + c * y);
                    b[(i, p)] = x;
                    b[(i, q)] = y;
                    new_p = new_p.hypot(x);
                    new_q = new_q.hypot(y);
                }
                for i in 0..k {
                    let (x, y) = (v[(i, p)], v[(i, q)]);
                    v[(i, p)] = c * x - s * y;
                    v[(i, q)] = s * x + c * y;
                }
                norms[p] = new_p;
                norms[q] = new_q;
                changed = true;
            }
        }
        meter.sweeps = sweep + 1;
        if !changed {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err("bounded Jacobi SVD did not converge".into());
    }
    let mut order: Vec<_> = (0..k).collect();
    meter.jacobi(3 * k * k + 4 * k, upper)?;
    // Deterministic insertion sort also gives a direct kÂ² comparison bound.
    for j in 1..k {
        let mut i = j;
        while i > 0 && norms[order[i - 1]] < norms[order[i]] {
            order.swap(i - 1, i);
            i -= 1;
        }
    }
    meter.jacobi(2 * m * k + 2 * k * k + 2 * k, upper)?;
    let mut u = DMatrix::zeros(m, k);
    let mut vt = DMatrix::zeros(k, k);
    let mut s = DVector::zeros(k);
    for (j, &original) in order.iter().enumerate() {
        s[j] = norms[original] * scale;
        if !s[j].is_finite() {
            return Err("Jacobi singular value exceeds finite domain".into());
        }
        for i in 0..m {
            if norms[original] != 0. {
                u[(i, j)] = b[(i, original)] / norms[original];
            }
        }
        for i in 0..k {
            vt[(j, i)] = v[(i, original)];
        }
    }
    Ok(Factors { u, s, vt })
}

fn certificate(
    a: &DMatrix<f64>,
    y: &[f64],
    factors: &Factors,
    beta: &[f64],
    meter: &mut Meter,
    upper: usize,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Certificate, CheckError> {
    let (m, n) = a.shape();
    let k = m.min(n);
    let Factors { u, s, vt } = factors;
    let bound = 64. * f64::EPSILON * (m + n) as f64;
    meter.check(m * n + m * k + k * n + m + n + 8 * k, upper)?;
    let amax = maximum(a.iter().copied());
    if !amax.is_finite()
        || beta
            .iter()
            .chain(y)
            .chain(s.iter())
            .chain(u.iter())
            .chain(vt.iter())
            .any(|x| !x.is_finite())
    {
        return Err("nonfinite least-squares decomposition or solve".into());
    }
    if s.iter().any(|x| *x < 0.) || s.as_slice().windows(2).any(|p| p[0] < p[1]) {
        return Err("unordered or negative singular spectrum".into());
    }
    let cut = cutoff(m, n, s[0]);
    let ascale = if amax == 0. { 1. } else { amax };
    let mut reconstruction = 0.0_f64;
    let mut anorm = 0.0_f64;
    for i in 0..m {
        check().map_err(CheckError::Abort)?;
        meter.check(n * k + 2 * n, upper)?;
        for j in 0..n {
            let rebuilt = sum((0..k).map(|l| u[(i, l)] * (s[l] / ascale) * vt[(l, j)]));
            reconstruction = reconstruction.hypot(rebuilt - a[(i, j)] / ascale);
            anorm = anorm.hypot(a[(i, j)] / ascale);
        }
    }
    let mut u_orthogonality = 0.0_f64;
    let mut v_orthogonality = 0.0_f64;
    for p in 0..k {
        check().map_err(CheckError::Abort)?;
        meter.check(m * k + n * k, upper)?;
        for q in 0..k {
            let target = if p == q { 1. } else { 0. };
            if m < n || (s[p] > cut && s[q] > cut) {
                u_orthogonality =
                    u_orthogonality.hypot(sum((0..m).map(|i| u[(i, p)] * u[(i, q)])) - target);
            }
            // A^T repair need not fabricate null right vectors: only retained
            // modes enter the minimum-norm solve. The square opposite factor
            // is still checked in full.
            if m >= n || (s[p] > cut && s[q] > cut) {
                v_orthogonality =
                    v_orthogonality.hypot(sum((0..n).map(|i| vt[(p, i)] * vt[(q, i)])) - target);
            }
        }
    }
    meter.check(4 * m * n + 6 * k * n + 6 * m + 8 * n, upper)?;
    let bmax = maximum(beta.iter().copied());
    let ymax = maximum(y.iter().copied());
    let product_scale = ascale * bmax;
    if !product_scale.is_finite() {
        return Err("least-squares solve scale exceeds finite domain".into());
    }
    let common = product_scale.max(ymax).max(f64::MIN_POSITIVE);
    let mut residual = vec![0.; m];
    for i in 0..m {
        check().map_err(CheckError::Abort)?;
        residual[i] = if bmax == 0. {
            0.
        } else {
            product_scale / common * sum((0..n).map(|j| (a[(i, j)] / ascale) * (beta[j] / bmax)))
        } - y[i] / common;
    }
    let mut gradient = 0.0_f64;
    for j in 0..n {
        check().map_err(CheckError::Abort)?;
        gradient = gradient.hypot(sum((0..m).map(|i| a[(i, j)] / ascale * residual[i])));
    }
    let ynorm = norm(y.iter().map(|x| x / common));
    let rnorm = norm(residual.iter().copied());
    let beta_norm = if bmax == 0. {
        0.
    } else {
        norm(beta.iter().map(|x| x / bmax))
    };
    let mut projection = vec![0.; n];
    for p in 0..k {
        check().map_err(CheckError::Abort)?;
        if s[p] > cut && bmax != 0. {
            let coefficient = sum((0..n).map(|j| vt[(p, j)] * (beta[j] / bmax)));
            for j in 0..n {
                projection[j] += coefficient * vt[(p, j)];
            }
        }
    }
    let minimum_norm_error = if bmax == 0. {
        0.
    } else {
        norm((0..n).map(|j| beta[j] / bmax - projection[j])) / beta_norm
    };
    meter.check(m * k + k * n + 4 * k, upper)?;
    let mut retained_solve_error = 0.0_f64;
    for p in 0..k {
        check().map_err(CheckError::Abort)?;
        if s[p] > cut {
            let lhs = sum((0..m).map(|i| u[(i, p)] * (y[i] / common)));
            let rhs = if bmax == 0. {
                0.
            } else {
                (s[p] / ascale)
                    * (product_scale / common)
                    * sum((0..n).map(|j| vt[(p, j)] * (beta[j] / bmax)))
            };
            retained_solve_error = retained_solve_error.hypot(lhs - rhs);
        }
    }
    let relative_gradient = gradient / (anorm * (rnorm + ynorm)).max(f64::MIN_POSITIVE);
    // Rank truncation intentionally leaves A^T r up to cutoff*||y||.
    let gradient_limit =
        cut / ascale * ynorm + bound * anorm * (rnorm + ynorm + product_scale / common * beta_norm);
    let result = Certificate {
        reconstruction: reconstruction / anorm.max(f64::MIN_POSITIVE),
        u_orthogonality,
        v_orthogonality,
        relative_gradient,
        minimum_norm_error,
        retained_solve_error: retained_solve_error
            / (ynorm + product_scale / common * anorm * beta_norm).max(f64::MIN_POSITIVE),
    };
    if [
        result.reconstruction,
        result.u_orthogonality,
        result.v_orthogonality,
        result.relative_gradient,
        result.minimum_norm_error,
        result.retained_solve_error,
        gradient,
        gradient_limit,
    ]
    .iter()
    .any(|x| !x.is_finite())
        || result.reconstruction > bound
        || result.u_orthogonality > bound
        || result.v_orthogonality > bound
        || gradient > gradient_limit
        || minimum_norm_error > bound
        || result.retained_solve_error > bound
    {
        return Err(format!("computed SVD/solve failed backward checks: {result:?}").into());
    }
    Ok(result)
}

#[derive(Clone, Copy)]
pub(crate) enum LegacyPolicy {
    History,
    Continuation,
}

pub(crate) fn solve(
    a: &DMatrix<f64>,
    y: &[f64],
    check: impl FnMut() -> Result<(), String>,
) -> Result<Checked, String> {
    solve_with_policy(a, y, LegacyPolicy::Continuation, check)
}

pub(crate) fn solve_with_policy(
    a: &DMatrix<f64>,
    y: &[f64],
    policy: LegacyPolicy,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Checked, String> {
    let (m, k) = a.shape();
    let bound = WorkBound::new(m, k)?;
    if y.len() != m || a.iter().chain(y).any(|x| !x.is_finite()) {
        return Err("invalid least-squares input".into());
    }
    check()?;
    let mut meter = Meter {
        candidate: bound.candidate,
        ..Meter::default()
    };
    let candidate = nalgebra::linalg::SVD::try_new(
        a.clone(),
        true,
        true,
        match policy {
            LegacyPolicy::History => f64::EPSILON,
            LegacyPolicy::Continuation => 5. * f64::EPSILON,
        },
        LEGACY_ITERATIONS,
    );
    check()?;
    if let Some(candidate) = candidate {
        let cut = cutoff(m, k, candidate.singular_values[0]);
        if let Ok(beta) = candidate.solve(&DVector::from_column_slice(y), cut) {
            let factors = Factors {
                u: candidate.u.unwrap(),
                s: candidate.singular_values,
                vt: candidate.v_t.unwrap(),
            };
            match certificate(
                a,
                y,
                &factors,
                beta.as_slice(),
                &mut meter,
                2 * bound.check,
                &mut check,
            ) {
                Ok(certificate) => {
                    meter.finish(bound)?;
                    let _ = &certificate;
                    return Ok(Checked {
                        coefficients: beta.as_slice().to_vec(),
                        singular_values: factors.s.as_slice().to_vec(),
                        rank: factors.s.iter().filter(|x| **x > cut).count(),
                        #[cfg(test)]
                        repaired: false,
                        #[cfg(test)]
                        certificate,
                        #[cfg(test)]
                        meter,
                    });
                }
                Err(CheckError::Abort(error)) => return Err(error),
                Err(CheckError::Numerical(_)) => {}
            }
        }
    }
    check()?;
    let factors = repair(a, &mut meter, bound, &mut check)?;
    let cut = cutoff(m, k, factors.s[0]);
    let beta = project(&factors, y, &mut meter, bound.jacobi, &mut check)?;
    let certificate = certificate(
        a,
        y,
        &factors,
        &beta,
        &mut meter,
        2 * bound.check,
        &mut check,
    )
    .map_err(CheckError::message)?;
    meter.finish(bound)?;
    let _ = &certificate;
    Ok(Checked {
        coefficients: beta,
        singular_values: factors.s.as_slice().to_vec(),
        rank: factors.s.iter().filter(|x| **x > cut).count(),
        #[cfg(test)]
        repaired: true,
        #[cfg(test)]
        certificate,
        #[cfg(test)]
        meter,
    })
}

fn repair(
    a: &DMatrix<f64>,
    meter: &mut Meter,
    bound: WorkBound,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Factors, String> {
    let (m, k) = a.shape();
    Ok(if m >= k {
        jacobi(a, meter, bound.jacobi, check)?
    } else {
        // Original candidate is gone before allocating A^T. Keep original A for
        // the final certificate. Debit the full transpose ticket before growth.
        meter.transpose = bound.transpose;
        check()?;
        let mut at = DMatrix::zeros(k, m);
        for i in 0..m {
            check()?;
            for j in 0..k {
                at[(j, i)] = a[(i, j)];
            }
        }
        let mut tall = jacobi(&at, meter, bound.jacobi, check)?;
        drop(at);
        tall.vt.transpose_mut();
        let mut vt = DMatrix::zeros(m, k);
        for i in 0..m {
            check()?;
            for j in 0..k {
                vt[(i, j)] = tall.u[(j, i)];
            }
        }
        Factors {
            u: tall.vt,
            s: tall.s,
            vt,
        }
    })
}

fn project(
    factors: &Factors,
    y: &[f64],
    meter: &mut Meter,
    upper: usize,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Vec<f64>, String> {
    let (m, k) = factors.u.shape();
    let n = factors.vt.ncols();
    let cut = cutoff(m, n, factors.s[0]);
    meter.jacobi(m * k + k * n + m + 4 * n, upper)?;
    let ymax = maximum(y.iter().copied());
    let mut beta = vec![0.; n];
    if ymax != 0. {
        for p in 0..k {
            check()?;
            if factors.s[p] > cut {
                let coefficient = multiply_divide(
                    sum((0..m).map(|i| factors.u[(i, p)] * (y[i] / ymax))),
                    ymax,
                    factors.s[p],
                )?;
                for (j, value) in beta.iter_mut().enumerate() {
                    *value += factors.vt[(p, j)] * coefficient;
                }
            }
        }
    }
    Ok(beta)
}

#[cfg(test)]
pub(crate) fn audit_computed_candidate(a: &DMatrix<f64>, y: &[f64]) -> serde_json::Value {
    let bound = WorkBound::new(a.nrows(), a.ncols()).unwrap();
    let candidate =
        nalgebra::linalg::SVD::try_new(a.clone(), true, true, 5. * f64::EPSILON, LEGACY_ITERATIONS);
    let candidate_converged = candidate.is_some();
    let candidate_check = candidate.map(|candidate| {
        let cut = cutoff(a.nrows(), a.ncols(), candidate.singular_values[0]);
        let beta = candidate
            .solve(&DVector::from_column_slice(y), cut)
            .unwrap();
        let factors = Factors {
            u: candidate.u.unwrap(),
            s: candidate.singular_values,
            vt: candidate.v_t.unwrap(),
        };
        let mut meter = Meter::default();
        match certificate(
            a,
            y,
            &factors,
            beta.as_slice(),
            &mut meter,
            2 * bound.check,
            &mut || Ok(()),
        ) {
            Ok(metric) => serde_json::json!({"passed":true,"metrics":format!("{metric:?}")}),
            Err(error) => serde_json::json!({"passed":false,"reason":error.message()}),
        }
    });
    let checked = solve(a, y, || Ok(())).unwrap();
    serde_json::json!({
        "bounded_candidate_converged": candidate_converged,
        "bounded_candidate_check": candidate_check,
        "checked_repaired": checked.repaired,
        "checked_metrics": format!("{:?}", checked.certificate),
        "checked_coefficients": checked.coefficients,
        "checked_singular_values": checked.singular_values,
        "checked_rank":checked.rank,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn work_workspace_and_header_cells_match_independent_structural_receipt() {
        for (m, k, work, bytes) in [
            (8, 1, 23_490, 1_888),
            (256, 3, 995_342, 40_992),
            (512, 11, 17_819_422, 249_376),
        ] {
            let bound = WorkBound::new(m, k).unwrap();
            assert_eq!((bound.total(), bound.bytes()), (work, bytes));
        }
        let headers = std::mem::size_of::<Checked>()
            + std::mem::size_of::<Meter>()
            + std::mem::size_of::<WorkBound>()
            + std::mem::size_of::<Certificate>()
            + 2 * std::mem::size_of::<Factors>()
            + 8 * std::mem::size_of::<Vec<f64>>()
            + 4 * std::mem::size_of::<DVector<f64>>();
        assert!(headers <= 1024, "{headers}");
        let a = DMatrix::from_element(8, 1, 1.);
        let bound = WorkBound::new(8, 1).unwrap();
        let mut meter = Meter::default();
        let factors = jacobi(&a, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
        let beta = project(&factors, &[2.; 8], &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
        certificate(
            &a,
            &[2.; 8],
            &factors,
            &beta,
            &mut meter,
            2 * bound.check,
            &mut || Ok(()),
        )
        .unwrap();
        assert!(meter.jacobi <= bound.jacobi);
        assert!(meter.check <= bound.check);
    }
    #[test]
    fn captured_actual_matrices_and_minimal_repro_are_repaired() {
        let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/commercial-regression-svd-matrices.json"
        ))
        .unwrap();
        for raw in fixture["cases"].as_array().unwrap() {
            let name = raw["id"].as_str().unwrap();
            let rows = raw["matrix"].as_array().unwrap();
            let (m, k) = (rows.len(), rows[0].as_array().unwrap().len());
            let data: Vec<_> = rows
                .iter()
                .flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()))
                .collect();
            let y: Vec<_> = raw["targets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            let fitted = solve(&DMatrix::from_row_slice(m, k, &data), &y, || Ok(())).unwrap();
            assert!(fitted.repaired, "{name}");
            assert_eq!(fitted.rank, 2);
            assert!(fitted.certificate.reconstruction < 1e-12);
            assert!(fitted.certificate.relative_gradient < 1e-12);
            assert!(fitted.certificate.minimum_norm_error < 1e-12);
            assert!(fitted.certificate.retained_solve_error < 1e-12);
            let bound = WorkBound::new(m, k).unwrap();
            assert!(
                fitted.meter.candidate + fitted.meter.jacobi + fitted.meter.check <= bound.total()
            );
            assert!(bound.bytes > 8 * m * k);
            assert!(fitted.singular_values[0] > fitted.singular_values[1]);
            assert!(fitted.coefficients.iter().all(|x| x.is_finite()));
            eprintln!("{name}: {fitted:?}");
        }
    }
    #[test]
    fn valid_legacy_result_is_returned_bit_for_bit() {
        let a = DMatrix::from_row_slice(
            8,
            2,
            &[
                1., 0., 1., 1., 1., 2., 1., 3., 1., 4., 1., 5., 1., 6., 1., 7.,
            ],
        );
        let y = [3., 5., 7., 9., 11., 13., 15., 17.];
        let old = a.clone().svd(true, true);
        let old_beta = old
            .solve(
                &DVector::from_column_slice(&y),
                cutoff(8, 2, old.singular_values[0]),
            )
            .unwrap();
        let fitted = solve(&a, &y, || Ok(())).unwrap();
        assert!(!fitted.repaired);
        for (x, y) in fitted.coefficients.iter().zip(old_beta.iter()) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
        for (x, y) in fitted
            .singular_values
            .iter()
            .zip(old.singular_values.iter())
        {
            assert_eq!(x.to_bits(), y.to_bits());
        }
    }
    #[test]
    fn checkpoint_abort_is_never_reclassified_or_retried() {
        let a = DMatrix::from_element(8, 1, 1.);
        for abort_at in [1, 2, 3, 4, 5] {
            let mut calls = 0;
            let result = solve(&a, &[2.; 8], || {
                calls += 1;
                if calls == abort_at {
                    Err("original one-shot abort".into())
                } else {
                    Ok(())
                }
            });
            assert_eq!(result.unwrap_err(), "original one-shot abort");
            assert_eq!(calls, abort_at);
        }
    }
    #[test]
    fn shape_overflow_and_operation_ticket_fail_before_work() {
        assert!(WorkBound::new(1, usize::MAX).is_err());
        assert!(WorkBound::new(usize::MAX, 1).is_err());
        assert!(WorkBound::new(0, 0).is_err());
        let a = DMatrix::from_element(8, 1, 1.);
        let mut meter = Meter::default();
        assert!(jacobi(&a, &mut meter, 0, &mut || Ok(()))
            .unwrap_err()
            .contains("ticket"));
        assert_eq!(meter.jacobi, 0);
        let out = solve(&a, &[2.; 8], || Ok(())).unwrap();
        assert!((out.coefficients[0] - 2.).abs() < 1e-12);
        let b = WorkBound::new(8, 1).unwrap();
        assert!(out.meter.candidate + out.meter.jacobi + out.meter.check <= b.total());
    }
    #[test]
    fn nonfinite_inputs_and_nonfinite_derived_checks_fail_closed() {
        let a = DMatrix::from_element(8, 1, 1.);
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(solve(&a, &[invalid; 8], || Ok(())).is_err());
        }
        let bad = Factors {
            u: DMatrix::from_element(8, 1, f64::MAX),
            s: DVector::from_element(1, 1.),
            vt: DMatrix::from_element(1, 1, f64::MAX),
        };
        let mut meter = Meter::default();
        assert!(matches!(
            certificate(
                &a,
                &[2.; 8],
                &bad,
                &[1.],
                &mut meter,
                usize::MAX,
                &mut || Ok(())
            ),
            Err(CheckError::Numerical(_))
        ));
    }
    #[test]
    fn scaled_quotient_avoids_intermediate_range_loss() {
        for (x, y, z, expected) in [
            (1., 1e-300, 1e-300, 1.),
            (1e-300, 1e300, 1., 1.),
            (1e300, 1e-300, 1., 1.),
            (-2., 1e-300, 1e-300, -2.),
        ] {
            assert!((multiply_divide(x, y, z).unwrap() - expected).abs() < 1e-12);
        }
        assert!(multiply_divide(f64::MAX, f64::MAX, 1.).is_err());
        assert_eq!(
            multiply_divide(f64::from_bits(1), 1., 1.)
                .unwrap()
                .to_bits(),
            1
        );
    }
    #[test]
    fn computed_cutoff_is_unchanged_on_both_sides_and_at_boundary() {
        let threshold = 8. * f64::EPSILON;
        for multiple in [0.5, 1., 2.] {
            let delta = multiple * threshold;
            let mut a = DMatrix::zeros(8, 2);
            a[(0, 0)] = 1.;
            a[(1, 1)] = delta;
            let mut y = vec![0.; 8];
            y[1] = delta;
            let fit = solve(&a, &y, || Ok(())).unwrap();
            assert_eq!(fit.rank, if multiple > 1. { 2 } else { 1 });
            assert_eq!(fit.coefficients[1], if multiple > 1. { 1. } else { 0. });
        }
    }
    #[test]
    fn complete_repair_is_scale_safe_and_perturbations_remain_explicit() {
        let count = 16;
        for perturbation in [0., 1e-14, 1e-10, 1e-6, 1e-3] {
            for scale in [1e-150, 1., 1e150] {
                let mut data = Vec::new();
                let mut y = Vec::new();
                for i in 0..count {
                    let x = ((i * 17 + 3) % 31) as f64 / 100.;
                    let z = if i % 2 == 0 { 1. } else { -1. };
                    data.extend([scale, scale * x, scale * (7. * x + perturbation * z)]);
                    y.push(scale * (100. + 10. * x));
                }
                let a = DMatrix::from_row_slice(count, 3, &data);
                let bound = WorkBound::new(count, 3).unwrap();
                let mut meter = Meter::default();
                let factors = jacobi(&a, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
                let beta = project(&factors, &y, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
                let metrics = certificate(
                    &a,
                    &y,
                    &factors,
                    &beta,
                    &mut meter,
                    2 * bound.check,
                    &mut || Ok(()),
                )
                .unwrap();
                assert!(
                    metrics.reconstruction < 1e-12,
                    "{perturbation}/{scale:e}: {metrics:?}"
                );
                for i in 0..count {
                    let prediction = sum((0..3).map(|j| a[(i, j)] / scale * beta[j]));
                    assert!(
                        (prediction - y[i] / scale).abs() < 1e-9,
                        "{perturbation}/{scale:e}: {beta:?}"
                    );
                }
                if perturbation >= 1e-10 {
                    assert_eq!(
                        factors
                            .s
                            .iter()
                            .filter(|v| **v > cutoff(count, 3, factors.s[0]))
                            .count(),
                        3
                    );
                }
                assert!(meter.jacobi <= bound.jacobi);
                assert!(meter.check <= 2 * bound.check);
            }
        }
    }
    #[test]
    fn wide_transpose_repair_is_minimum_norm_and_bounded() {
        for m in [1usize, 2, 3, 10] {
            for rank in [1usize, m] {
                let mut a = DMatrix::zeros(m, 11);
                for i in 0..m {
                    a[(i, i % rank)] = 1.;
                    a[(i, 10)] = 1.;
                }
                let y: Vec<_> = (0..m).map(|i| 2. + (i % rank) as f64).collect();
                let bound = WorkBound::new(m, 11).unwrap();
                let mut meter = Meter::default();
                let f = repair(&a, &mut meter, bound, &mut || Ok(())).unwrap();
                let beta = project(&f, &y, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
                certificate(&a, &y, &f, &beta, &mut meter, 2 * bound.check, &mut || {
                    Ok(())
                })
                .unwrap();
                meter.finish(bound).unwrap();
                assert_eq!(beta.len(), 11);
                for i in 0..m {
                    assert!((sum((0..11).map(|j| a[(i, j)] * beta[j])) - y[i]).abs() < 1e-12);
                }
                for coefficient in beta.iter().take(10).skip(rank) {
                    assert_eq!(*coefficient, 0.);
                }
                let fit = solve_with_policy(&a, &y, LegacyPolicy::History, || Ok(())).unwrap();
                for (x, y) in fit.coefficients.iter().zip(&beta) {
                    assert!((x - y).abs() < 1e-12);
                }
            }
        }
    }
    #[test]
    fn history_shapes_and_legacy_epsilon_are_preserved() {
        for m in [2usize, 3, 10, 11, 12, 21, 22, 64] {
            let a = DMatrix::from_fn(m, 11, |i, j| {
                if j == 10 {
                    1.
                } else {
                    ((i * 17 + j * 7) % 31) as f64 / 31.
                }
            });
            let y: Vec<_> = (0..m).map(|i| 0.03 + 0.001 * i as f64).collect();
            let old = nalgebra::linalg::SVD::try_new(a.clone(), true, true, f64::EPSILON, 100_000)
                .unwrap();
            let beta = old
                .solve(
                    &DVector::from_column_slice(&y),
                    cutoff(m, 11, old.singular_values[0]),
                )
                .unwrap();
            let fit = solve_with_policy(&a, &y, LegacyPolicy::History, || Ok(())).unwrap();
            if !fit.repaired {
                for (x, y) in fit.coefficients.iter().zip(beta.iter()) {
                    assert_eq!(x.to_bits(), y.to_bits());
                }
            }
        }
    }
    #[test]
    fn wide_repair_retains_original_shape_cutoff_and_scale_safe_solve() {
        for multiple in [0.5, 1., 2.] {
            let delta = multiple * 11. * f64::EPSILON;
            let mut a = DMatrix::zeros(2, 11);
            a[(0, 0)] = 1.;
            a[(1, 1)] = delta;
            let y = [0., delta];
            let bound = WorkBound::new(2, 11).unwrap();
            let mut meter = Meter::default();
            let f = repair(&a, &mut meter, bound, &mut || Ok(())).unwrap();
            let beta = project(&f, &y, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
            certificate(&a, &y, &f, &beta, &mut meter, 2 * bound.check, &mut || {
                Ok(())
            })
            .unwrap();
            assert_eq!(beta[1], if multiple > 1. { 1. } else { 0. });
        }
        for scale in [1e-150, 1., 1e150] {
            let mut a = DMatrix::zeros(3, 11);
            for i in 0..3 {
                a[(i, 0)] = scale;
                a[(i, 1)] = scale * (i as f64 - 1.);
                a[(i, 2)] = 7. * a[(i, 1)];
            }
            let y: Vec<_> = (0..3).map(|i| scale * (2. + i as f64)).collect();
            let bound = WorkBound::new(3, 11).unwrap();
            let mut meter = Meter::default();
            let f = repair(&a, &mut meter, bound, &mut || Ok(())).unwrap();
            let beta = project(&f, &y, &mut meter, bound.jacobi, &mut || Ok(())).unwrap();
            certificate(&a, &y, &f, &beta, &mut meter, 2 * bound.check, &mut || {
                Ok(())
            })
            .unwrap();
            meter.finish(bound).unwrap();
        }
    }
}
