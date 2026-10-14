use crate::math::arrays::assemble;
use crate::math::arrays::View;
use rayon::prelude::*;

#[allow(clippy::needless_range_loop)] // Month indexes address multiple output rows.
pub(crate) fn income(a: &[View<'_>]) -> Vec<Vec<f64>> {
    crate::compute_span!("effective_yield");
    let (n, t) = (a[0].shape[0], a[0].shape[1]);
    let h = a[2].ix(0);
    let rows = (0..n)
        .into_par_iter()
        .map(|s| {
            let (mut y, mut lo, mut hi) = (0.05_f64, -0.5_f64, 1.0_f64);
            for _ in 0..a[3].ix(0) {
                let (mut pv, mut dpv) = (0.0, 0.0);
                let discount = 1.0 / (1.0 + y / 12.0);
                let mut factor = discount;
                for m in 0..t {
                    let tm = (m + 1) as f64;
                    let cf = a[0].x(s, m) * factor;
                    pv += cf;
                    dpv -= cf * tm * discount / 12.0;
                    factor *= discount;
                }
                let err = pv - a[1].at(s);
                if err.abs() < 1e-12 {
                    break;
                }
                if err > 0.0 {
                    lo = lo.max(y);
                }
                if err < 0.0 {
                    hi = hi.min(y);
                }
                let candidate = y - if dpv.abs() > 1e-16 { err / dpv } else { 0.0 };
                y = if candidate <= lo || candidate >= hi || !candidate.is_finite() {
                    (lo + hi) * 0.5
                } else {
                    candidate
                };
            }
            let mut bv = a[1].at(s);
            let mut out = vec![vec![0.0; h], vec![0.0; h], vec![y]];
            for m in 0..h {
                let inc = bv * y / 12.0;
                bv += inc - a[0].x(s, m);
                out[0][m] = inc;
                out[1][m] = bv;
            }
            out
        })
        .collect();
    assemble(rows, &[h, h, 1])
}

pub(crate) fn accrual(a: &[View<'_>]) -> Vec<Vec<f64>> {
    // offsets, accrual month, payment month, cash amounts, horizon, smear
    let n = a[0].v.len() - 1;
    let h = a[4].ix(0);
    let smear = a[5].at(0) != 0.0;
    let rows = (0..n)
        .into_par_iter()
        .map(|s| {
            let mut out = vec![0.0; h];
            for j in a[0].ix(s)..a[0].ix(s + 1) {
                let end = a[2].ix(j);
                let start = if smear { a[1].ix(j).min(end) } else { end };
                let val = a[3].at(j) / (end - start + 1) as f64;
                for cell in out.iter_mut().take((end + 1).min(h)).skip(start.min(h)) {
                    *cell += val;
                }
            }
            vec![out]
        })
        .collect();
    assemble(rows, &[h])
}
