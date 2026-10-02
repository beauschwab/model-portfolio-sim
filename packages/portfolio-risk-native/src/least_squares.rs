//! Dense, bounded trust-region-reflective least squares for built-in calibrations.
//! Algorithm adapted from SciPy 1.17.1 optimize/_lsq/{trf,common}.py and
//! optimize/_numdiff.py. See THIRD_PARTY_NOTICES.md for the BSD license.
//! Deliberately supports only finite bounds, linear loss and unit x_scale.
use nalgebra::{linalg::SVD, DMatrix, DVector};
type Vector = DVector<f64>;
type Matrix = DMatrix<f64>;

pub(crate) trait Model {
    fn residual(&self, x: &[f64]) -> Result<Vector, String>;
    fn jacobian(&self, _x: &[f64]) -> Option<Matrix> {
        None
    }
}

#[derive(Clone)]
pub struct Fit {
    pub x: Vec<f64>,
    pub rmse: f64,
    pub evaluations: usize,
    /// SciPy-compatible termination: 1 gradient, 2 cost, 3 step, 4 cost and step.
    pub status: u32,
}

struct Bounds {
    lo: Vector,
    hi: Vector,
}
impl Bounds {
    fn contains(&self, x: &Vector) -> bool {
        x.iter()
            .enumerate()
            .all(|(i, &v)| v >= self.lo[i] && v <= self.hi[i])
    }
    fn interior(&self, x: &Vector, rstep: f64) -> Vector {
        Vector::from_iterator(
            x.len(),
            x.iter().enumerate().map(|(i, &v)| {
                let (lo, hi) = (self.lo[i], self.hi[i]);
                let lower = v - lo;
                let upper = hi - v;
                let mut y = v;
                if lower <= upper.min(rstep * lo.abs().max(1.)) {
                    y = if rstep == 0. {
                        lo.next_up()
                    } else {
                        lo + rstep * lo.abs().max(1.)
                    };
                }
                if upper <= lower.min(rstep * hi.abs().max(1.)) {
                    y = if rstep == 0. {
                        hi.next_down()
                    } else {
                        hi - rstep * hi.abs().max(1.)
                    };
                }
                if y < lo || y > hi {
                    0.5 * (lo + hi)
                } else {
                    y
                }
            }),
        )
    }
    fn scaling(&self, x: &Vector, g: &Vector) -> (Vector, Vector) {
        let mut v = Vector::from_element(x.len(), 1.);
        let mut dv = Vector::zeros(x.len());
        for i in 0..x.len() {
            if g[i] < 0. {
                v[i] = self.hi[i] - x[i];
                dv[i] = -1.;
            }
            if g[i] > 0. {
                v[i] = x[i] - self.lo[i];
                dv[i] = 1.;
            }
        }
        (v, dv)
    }
    fn stride(&self, x: &Vector, p: &Vector) -> (f64, Vec<bool>) {
        let steps = Vector::from_iterator(
            x.len(),
            (0..x.len()).map(|i| {
                if p[i] == 0. {
                    f64::INFINITY
                } else {
                    ((self.lo[i] - x[i]) / p[i]).max((self.hi[i] - x[i]) / p[i])
                }
            }),
        );
        let step = steps.min();
        (step, steps.iter().map(|&s| s == step).collect())
    }
}

fn jacobian(model: &impl Model, x: &Vector, f: &Vector, bounds: &Bounds) -> Result<Matrix, String> {
    let j = if let Some(j) = model.jacobian(x.as_slice()) {
        j
    } else {
        let mut j = Matrix::zeros(f.len(), x.len());
        for i in 0..x.len() {
            let mut h = f64::EPSILON.sqrt() * if x[i] < 0. { -1. } else { 1. } * x[i].abs().max(1.);
            let (lower, upper) = (x[i] - bounds.lo[i], bounds.hi[i] - x[i]);
            if h.abs() <= lower.max(upper) {
                if x[i] + h < bounds.lo[i] || x[i] + h > bounds.hi[i] {
                    h = -h;
                }
            } else {
                h = if upper >= lower { upper } else { -lower };
            }
            let mut next = x.clone();
            next[i] += h;
            let dx = next[i] - x[i];
            let fi = model.residual(next.as_slice())?;
            if dx == 0. || fi.len() != f.len() {
                return Err("invalid finite difference".into());
            }
            j.set_column(i, &((fi - f) / dx));
        }
        j
    };
    if j.shape() != (f.len(), x.len()) || j.iter().any(|v| !v.is_finite()) {
        return Err("invalid calibration Jacobian".into());
    }
    Ok(j)
}

fn trust_step(
    m: usize,
    uf: &Vector,
    s: &Vector,
    v: &Matrix,
    radius: f64,
    mut alpha: f64,
) -> (Vector, f64) {
    let n = s.len();
    let full_rank = m >= n && s[n - 1] > f64::EPSILON * m as f64 * s[0];
    if full_rank {
        let p = -v * uf.component_div(s);
        if p.norm() <= radius {
            return (p, 0.);
        }
    }
    let suf = s.component_mul(uf);
    let phi = |alpha: f64| {
        let denom = s.map(|v| v * v + alpha);
        let norm = suf.component_div(&denom).norm();
        let deriv = -(0..n)
            .map(|i| suf[i].powi(2) / denom[i].powi(3))
            .sum::<f64>()
            / norm;
        (norm - radius, deriv)
    };
    let mut upper = suf.norm() / radius;
    let mut lower = if full_rank {
        let (p, d) = phi(0.);
        -p / d
    } else {
        0.
    };
    if !full_rank && alpha == 0. {
        alpha = (0.001 * upper).max((lower * upper).sqrt());
    }
    for _ in 0..10 {
        if alpha < lower || alpha > upper {
            alpha = (0.001 * upper).max((lower * upper).sqrt());
        }
        let (p, d) = phi(alpha);
        if p < 0. {
            upper = alpha;
        }
        let ratio = p / d;
        lower = lower.max(alpha - ratio);
        alpha -= (p + radius) * ratio / radius;
        if p.abs() < 0.01 * radius {
            break;
        }
    }
    let mut p = -v * suf.component_div(&s.map(|v| v * v + alpha));
    p *= radius / p.norm();
    (p, alpha)
}

struct Quadratic<'a> {
    j: &'a Matrix,
    g: &'a Vector,
    diag: &'a Vector,
}
impl Quadratic<'_> {
    fn value(&self, p: &Vector) -> f64 {
        0.5 * ((self.j * p).norm_squared() + p.component_mul(self.diag).dot(p)) + p.dot(self.g)
    }
    fn line(&self, p: &Vector, start: Option<&Vector>, lo: f64, hi: f64) -> (f64, f64) {
        let jp = self.j * p;
        let a = 0.5 * (jp.norm_squared() + p.component_mul(self.diag).dot(p));
        let mut b = self.g.dot(p);
        let mut c = 0.;
        if let Some(start) = start {
            b += (self.j * start).dot(&jp) + start.component_mul(self.diag).dot(p);
            c = self.value(start);
        }
        let value = |t: f64| t * (a * t + b) + c;
        let mut best = (lo, value(lo));
        if value(hi) < best.1 {
            best = (hi, value(hi));
        }
        let extremum = -0.5 * b / a;
        if a != 0. && lo < extremum && extremum < hi && value(extremum) < best.1 {
            best = (extremum, value(extremum));
        }
        best
    }
}

#[allow(clippy::too_many_arguments)]
fn select_step(
    x: &Vector,
    q: &Quadratic<'_>,
    mut ph: Vector,
    d: &Vector,
    radius: f64,
    bounds: &Bounds,
    theta: f64,
) -> (Vector, Vector, f64) {
    let p = ph.component_mul(d);
    if bounds.contains(&(x + &p)) {
        let value = q.value(&ph);
        return (p, ph, -value);
    }
    let (stride, hits) = bounds.stride(x, &p);
    let mut rh = ph.clone();
    for i in 0..rh.len() {
        if hits[i] {
            rh[i] = -rh[i];
        }
    }
    ph *= stride;
    let r = rh.component_mul(d);
    let on_bound = x + p * stride;
    let a = rh.norm_squared();
    let b = ph.dot(&rh);
    // Guard roundoff at the trust boundary without relaxing financial bounds.
    let c = (ph.norm_squared() - radius * radius).min(0.);
    let root = -(b + (b * b - a * c).sqrt().copysign(b));
    let to_tr = if root == 0. {
        0.
    } else {
        (root / a).max(c / root)
    };
    let to_bound = bounds.stride(&on_bound, &r).0;
    let r_stride = to_bound.min(to_tr);
    let (lo, hi) = if r_stride > 0. {
        (
            (1. - theta) * stride / r_stride,
            if r_stride == to_bound {
                theta * to_bound
            } else {
                to_tr
            },
        )
    } else {
        (0., -1.)
    };
    let mut rvalue = f64::INFINITY;
    if lo <= hi {
        let (t, value) = q.line(&rh, Some(&ph), lo, hi);
        rh = rh * t + &ph;
        rvalue = value;
    }
    ph *= theta;
    let pvalue = q.value(&ph);
    let mut ag = -q.g;
    let to_tr = radius / ag.norm();
    let to_bound = bounds.stride(x, &ag.component_mul(d)).0;
    let hi = if to_bound < to_tr {
        theta * to_bound
    } else {
        to_tr
    };
    let (t, agvalue) = q.line(&ag, None, 0., hi);
    ag *= t;
    let (chosen, value) = if pvalue < rvalue && pvalue < agvalue {
        (ph, pvalue)
    } else if rvalue < pvalue && rvalue < agvalue {
        (rh, rvalue)
    } else {
        (ag, agvalue)
    };
    (chosen.component_mul(d), chosen, -value)
}

/// Fail closed on invalid input, decomposition failure or exhausted iterations.
/// Counts exclude finite-difference Jacobian evaluations, as in SciPy's TRF.
pub(crate) fn fit(
    model: &impl Model,
    initial: &[f64],
    lower: &[f64],
    upper: &[f64],
) -> Result<Fit, String> {
    let n = initial.len();
    if n == 0
        || n > 16
        || lower.len() != n
        || upper.len() != n
        || initial
            .iter()
            .chain(lower)
            .chain(upper)
            .any(|x| !x.is_finite())
        || lower.iter().zip(upper).any(|(l, u)| l >= u)
    {
        return Err("invalid fit bounds".into());
    }
    let bounds = Bounds {
        lo: Vector::from_column_slice(lower),
        hi: Vector::from_column_slice(upper),
    };
    let x0 = Vector::from_column_slice(initial);
    if !bounds.contains(&x0) {
        return Err("initial fit parameters outside bounds".into());
    }
    let mut x = bounds.interior(&x0, 1e-10);
    let mut f = model.residual(x.as_slice())?;
    let m = f.len();
    if m == 0 || f.iter().any(|v| !v.is_finite()) {
        return Err("invalid initial fit residual".into());
    }
    let mut j = jacobian(model, &x, &f, &bounds)?;
    let mut cost = 0.5 * f.norm_squared();
    if !cost.is_finite() {
        return Err("nonfinite calibration objective".into());
    }
    let mut g = j.transpose() * &f;
    let mut radius = x
        .component_div(&bounds.scaling(&x, &g).0.map(f64::sqrt))
        .norm();
    if radius == 0. {
        radius = 1.;
    }
    let mut alpha = 0.;
    let mut evaluations = 1;
    let mut status = 0;
    loop {
        let (v, dv) = bounds.scaling(&x, &g);
        let norm = g.component_mul(&v).amax();
        if norm < 1e-8 {
            status = 1;
        }
        if status != 0 {
            return Ok(Fit {
                x: x.as_slice().to_vec(),
                rmse: (2. * cost / m as f64).sqrt(),
                evaluations,
                status,
            });
        }
        if evaluations >= 100 * n {
            return Err("nonlinear calibration exhausted evaluations".into());
        }
        let d = v.map(f64::sqrt);
        let diag = g.component_mul(&dv);
        let gh = g.component_mul(&d);
        let jh = Matrix::from_fn(m, n, |r, c| j[(r, c)] * d[c]);
        let augmented = Matrix::from_fn(m + n, n, |r, c| {
            if r < m {
                jh[(r, c)]
            } else if r - m == c {
                diag[c].sqrt()
            } else {
                0.
            }
        });
        let svd = SVD::try_new(augmented, true, true, f64::EPSILON, 100_000)
            .ok_or("fit SVD did not converge")?;
        let u = svd.u.unwrap();
        let vv = svd.v_t.unwrap().transpose();
        let uf = u.rows(0, m).transpose() * &f;
        let q = Quadratic {
            j: &jh,
            g: &gh,
            diag: &diag,
        };
        let theta = 0.995_f64.max(1. - norm);
        let mut accepted = None;
        while evaluations < 100 * n {
            let (ph, next_alpha) = trust_step(m, &uf, &svd.singular_values, &vv, radius, alpha);
            alpha = next_alpha;
            if ph.iter().any(|v| !v.is_finite()) {
                return Err("nonfinite trust-region step".into());
            }
            let (step, sh, predicted) = select_step(&x, &q, ph, &d, radius, &bounds, theta);
            let xn = bounds.interior(&(&x + &step), 0.);
            let fnn = model.residual(xn.as_slice())?;
            evaluations += 1;
            if fnn.len() != m {
                return Err("changed residual dimensions".into());
            }
            let snorm = sh.norm();
            if fnn.iter().any(|v| !v.is_finite()) {
                radius = 0.25 * snorm;
                continue;
            }
            let cn = 0.5 * fnn.norm_squared();
            let actual = cost - cn;
            let ratio = if predicted > 0. {
                actual / predicted
            } else if predicted == 0. && actual == 0. {
                1.
            } else {
                0.
            };
            let next_radius = if ratio < 0.25 {
                0.25 * snorm
            } else if ratio > 0.75 && snorm > 0.95 * radius {
                2. * radius
            } else {
                radius
            };
            let ftol = actual < 1e-8 * cost && ratio > 0.25;
            let xtol = step.norm() < 1e-8 * (1e-8 + x.norm());
            status = match (ftol, xtol) {
                (true, true) => 4,
                (true, false) => 2,
                (false, true) => 3,
                _ => 0,
            };
            if actual > 0. {
                accepted = Some((xn, fnn, cn));
            }
            if status != 0 {
                break;
            }
            alpha *= radius / next_radius;
            radius = next_radius;
            if accepted.is_some() {
                break;
            }
        }
        if let Some((xn, fnn, cn)) = accepted {
            x = xn;
            f = fnn;
            cost = cn;
            j = jacobian(model, &x, &f, &bounds)?;
            g = j.transpose() * &f;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Rosenbrock;
    impl Model for Rosenbrock {
        fn residual(&self, x: &[f64]) -> Result<Vector, String> {
            Ok(Vector::from_vec(vec![
                10. * (x[1] - x[0] * x[0]),
                1. - x[0],
            ]))
        }
    }
    #[test]
    fn nonlinear_fit_and_active_boundary() {
        let f = fit(&Rosenbrock, &[2., 2.], &[-3., -3.], &[3., 3.]).unwrap();
        assert!((f.x[0] - 1.).abs() < 1e-7 && f.rmse < 1e-7);
        let f = fit(&Rosenbrock, &[0., 0.], &[-3., -3.], &[0.5, 3.]).unwrap();
        assert!((f.x[0] - 0.5).abs() < 1e-7 && (f.x[1] - 0.25).abs() < 1e-7);
        assert!(fit(&Rosenbrock, &[5., 2.], &[-3., -3.], &[3., 3.]).is_err());
    }
}
