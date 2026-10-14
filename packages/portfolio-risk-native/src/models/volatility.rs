use crate::math::arrays::View;
use rayon::prelude::*;

pub(crate) fn volatility(a: &[View<'_>]) -> Vec<Vec<f64>> {
    // Requests: valuation time, expiry, first forward, number of quarters.
    let q = a[1].v;
    let n = a[2].v.len();
    let nf = a[4].shape[1];
    let tenor = a[5].at(0);
    let shift = a[6].at(0);
    let out = (0..a[0].shape[0])
        .into_par_iter()
        .map(|r| {
            let t = a[0].x(r, 0);
            let expiry = a[0].x(r, 1);
            let start = a[0].x(r, 2) as usize;
            let end = (start + a[0].x(r, 3) as usize).min(n);
            assert!(expiry > 0.0 && end > start);
            let ann = tenor * (start..end).map(|i| a[3].at(i + 1)).sum::<f64>();
            let s0 = (a[3].at(start) - a[3].at(end)) / ann;
            let mut variances = [0.0; 21];
            let mut jacobian = [[0.0; 4]; 21];
            for (g, var) in variances.iter_mut().enumerate() {
                let time = t + expiry * g as f64 / 20.0;
                let mut factors = vec![0.0; nf];
                let mut derivatives = vec![[0.0; 4]; nf];
                for i in start..end {
                    let tau = (i as f64 * tenor - time).max(1e-6);
                    let sigma = (q[0] + q[1] * tau) * (-q[2] * tau).exp() + q[3];
                    let exp = (-q[2] * tau).exp();
                    let ds = [exp, tau * exp, -(q[0] + q[1] * tau) * tau * exp, 1.0];
                    let weight = tenor * a[3].at(i + 1) / ann * (a[2].at(i) + shift) / s0;
                    for (k, f) in factors.iter_mut().enumerate() {
                        *f += sigma * weight * a[4].x(i, k);
                        for (j, d) in ds.iter().enumerate() {
                            derivatives[k][j] += d * weight * a[4].x(i, k);
                        }
                    }
                }
                *var = factors.iter().map(|x| x * x).sum();
                for j in 0..4 {
                    jacobian[g][j] = (0..nf).map(|k| factors[k] * derivatives[k][j]).sum();
                }
            }
            let vol = (variances
                .windows(2)
                .map(|w| (w[0] + w[1]) * 0.5 / 20.0)
                .sum::<f64>())
            .sqrt();
            let mut out = vec![vol];
            for j in 0..4 {
                out.push(
                    jacobian
                        .windows(2)
                        .map(|w| (w[0][j] + w[1][j]) * 0.5 / 20.0)
                        .sum::<f64>()
                        / if vol == 0.0 { 1.0 } else { vol },
                );
            }
            out
        })
        .collect::<Vec<Vec<f64>>>();
    vec![out.into_iter().flatten().collect()]
}
