//! Scalar logistic, lookup-table and spline arithmetic.

pub(crate) fn sigmoid(z: f64, rational: bool) -> f64 {
    if !rational {
        return if z > 30.0 {
            1.0
        } else if z < -30.0 {
            0.0
        } else {
            1.0 / (1.0 + (-z).exp())
        };
    }
    let x = z * 0.5;
    if x >= 6.0 {
        return 1.0;
    }
    if x <= -6.0 {
        return 0.0;
    }
    let x2 = x * x;
    let t = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)))
        / (135135.0 + x2 * (62370.0 + x2 * (3150.0 + 28.0 * x2)));
    0.5 + 0.5 * t.clamp(-1.0, 1.0)
}

pub(crate) fn lut(u: f64, v: &[f64]) -> f64 {
    if u <= 0.0 {
        return v[0];
    }
    let i = u as usize;
    if i >= v.len() - 1 {
        return v[v.len() - 1];
    }
    let f = u - i as f64;
    v[i] * (1.0 - f) + v[i + 1] * f
}

pub(crate) fn spline(x: f64, knots: &[f64], c: &[f64]) -> f64 {
    let x = x.clamp(knots[0], knots[knots.len() - 1]);
    let mut i = 0;
    for (j, k) in knots.iter().enumerate().take(knots.len() - 1) {
        if x >= *k {
            i = j;
        }
    }
    let dx = x - knots[i];
    let o = 4 * i;
    ((c[o] * dx + c[o + 1]) * dx + c[o + 2]) * dx + c[o + 3]
}
