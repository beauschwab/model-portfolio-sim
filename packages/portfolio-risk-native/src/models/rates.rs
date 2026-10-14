use crate::math::arrays::assemble;
use crate::math::arrays::View;
use rayon::prelude::*;

pub(crate) fn lmm(a: &[View<'_>]) -> Vec<Vec<f64>> {
    assert_eq!(a.len(), 7); // F0,sigma,B,Z,shift,dt,tenor
    let (p, t, k) = (a[3].shape[0], a[3].shape[1], a[3].shape[2]);
    let n = a[0].v.len();
    let dt = a[5].at(0);
    let tenor = a[6].at(0);
    assert_eq!(a[1].shape, [t, n, 1]);
    assert_eq!(a[2].shape, [n, k, 1]);
    let rows = (0..p)
        .into_par_iter()
        .map(|p| {
            let mut f = a[0].v.to_vec();
            let mut df = 1.0;
            let mut out = vec![vec![0.0; t], vec![0.0; 4 * t], vec![0.0; t]];
            for m in 0..t {
                let eta = (m as f64 * dt / tenor + 1e-9) as usize;
                for (l, nq) in [8, 20, 40, 120].iter().enumerate() {
                    let (mut d, mut ann) = (1.0, 0.0);
                    for j in eta..eta + nq {
                        d /= 1.0 + tenor * f[j.min(n - 1)];
                        ann += tenor * d;
                    }
                    out[1][l * t + m] = (1.0 - d) / ann;
                }
                out[2][m] = f[eta];
                df /= 1.0 + f[eta] * dt;
                out[0][m] = df;
                let mut sum = vec![0.0; k];
                for (i, fi) in f.iter_mut().enumerate().skip(eta) {
                    let si = a[1].x(m, i);
                    let x = *fi + a[4].at(0);
                    let w = tenor * x * si / (1.0 + tenor * *fi);
                    let (mut mu, mut dw) = (0.0, 0.0);
                    for (q, sq) in sum.iter_mut().enumerate() {
                        let b = a[2].x(i, q);
                        *sq += w * b;
                        mu += b * *sq;
                        dw += b * a[3].z(p, m, q);
                    }
                    mu *= si;
                    *fi = x * ((mu - 0.5 * si * si) * dt + si * dt.sqrt() * dw).exp() - a[4].at(0);
                }
            }
            out
        })
        .collect();
    assemble(rows, &[t, 4 * t, t])
}
