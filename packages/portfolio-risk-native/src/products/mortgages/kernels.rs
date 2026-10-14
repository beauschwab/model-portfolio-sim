use crate::math::arrays::assemble;
use crate::math::arrays::View;
use crate::math::functions::{lut, sigmoid, spline};
use rayon::prelude::*;

// All three mortgage entrypoints use the same native month event.
#[allow(clippy::too_many_arguments)] // Explicit model inputs and three evolving states.
fn mortgage_step(
    a: &[View<'_>],
    s: usize,
    p: usize,
    m: usize,
    bal: &mut f64,
    burn: &mut f64,
    q: &mut f64,
    rational: bool,
) -> (f64, f64, f64) {
    let pp = a[6].v;
    let r = a[13].at(s) / 12.0;
    let inc = a[13].at(s) - a[0].x(p, m);
    let mut refi = pp[0] * sigmoid(pp[1] + pp[2] * inc, rational) * *burn;
    let lock = pp[7] + (1.0 - pp[7]) * sigmoid(pp[8] * inc, rational);
    if inc > 0.0 {
        *burn *= lut(inc * a[12].at(0), a[11].v);
    }
    let cltv = a[17].at(s) * a[18].at(s) / a[19].at(s) * *bal / a[1].x(p, m);
    refi *= spline(cltv, a[7].v, a[8].v) * a[20].at(s);
    let ramp = ((a[16].at(s) + m as f64) / 30.0).min(1.0);
    // a[21]: per-pool speed multiplier on turnover + refi, applied before the cap
    let cpr = ((pp[4] * ramp * a[5].at(a[4].ix(m)) * (1.0 + pp[6] * a[2].x(p, m)).max(0.3) * lock
        + refi)
        * a[21].at(s))
    .min(pp[5]);
    let smm = lut(cpr * a[10].at(0), a[9].v);
    let pmt = *bal * r / (1.0 - *q);
    *q *= 1.0 + r;
    let sched = (pmt - *bal * r).min(*bal);
    let prepay = (*bal - sched) * smm;
    let intr = *bal * (a[14].at(s) / 12.0);
    let principal = sched + prepay;
    *bal -= principal;
    (intr + principal, intr, principal)
}

pub(crate) fn mortgage(a: &[View<'_>], stress: bool) -> Vec<Vec<f64>> {
    crate::compute_span!("mortgage_cashflows");
    assert_eq!(a.len(), if stress { 28 } else { 26 });
    let (p, t) = (a[0].shape[0], a[0].shape[1]);
    let n = a[13].v.len();
    let hcount = if stress { 1 } else { a[23].v.len() };
    let forward = !stress && a[24].at(0) != 0.0;
    let rational = a[if stress { 27 } else { 25 }].at(0) != 0.0;
    let rows = (0..n)
        .into_par_iter()
        .map(|s| {
            let mut out = if stress {
                vec![vec![0.0; 1]]
            } else {
                vec![
                    vec![0.0; t],
                    vec![0.0; hcount],
                    vec![0.0; hcount],
                    vec![0.0; p * hcount],
                    vec![0.0; p * hcount],
                    vec![0.0; t],
                    vec![0.0; t],
                ]
            };
            let h = if stress { a[23].ix(0) } else { 0 };
            let oa = a[22].at(s);
            let eo = (-oa / 12.0).exp();
            let div = (-oa * h as f64 / 12.0).exp();
            let disc: Vec<_> = (0..t)
                .map(|m| (-oa * ((m + 1) as f64 / 12.0)).exp())
                .collect();
            for path in 0..p {
                let (mut bal, mut burn) = if stress {
                    (a[25].z(s, path, a[24].ix(0)), a[26].z(s, path, a[24].ix(0)))
                } else {
                    (1.0, 1.0)
                };
                if bal <= 1e-10 {
                    continue;
                }
                let mut q = (1.0 + a[13].at(s) / 12.0).powi(h as i32 - a[15].at(s) as i32);
                let mut kf = 0;
                let mut buf = vec![0.0; t];
                let mut last = t;
                let mut e = div * eo;
                let mut v = 0.0;
                for m in h..t {
                    if forward && kf < hcount && m == a[23].ix(kf) {
                        out[3][path * hcount + kf] = (bal as f32) as f64;
                        out[4][path * hcount + kf] = (burn as f32) as f64;
                        out[2][kf] += bal;
                        kf += 1;
                    }
                    let (cf, intr, principal) =
                        mortgage_step(a, s, path, m, &mut bal, &mut burn, &mut q, rational);
                    if stress {
                        v += cf * a[3].x(path, m) * e;
                        e *= eo;
                    } else {
                        buf[m] = cf * a[3].x(path, m);
                        out[0][m] += buf[m];
                        out[5][m] += intr;
                        out[6][m] += principal;
                    }
                    if bal <= 1e-10 {
                        last = m + 1;
                        break;
                    }
                }
                if stress {
                    out[0][0] += v / (a[3].x(path, h - 1) * div);
                } else if forward {
                    let mut k = hcount;
                    while k > 0 && a[23].ix(k - 1) >= last {
                        k -= 1;
                    }
                    let mut v = 0.0;
                    for m in (0..last).rev() {
                        v += buf[m] * disc[m];
                        while k > 0 && a[23].ix(k - 1) == m {
                            let prev = if m == 0 { t - 1 } else { m - 1 };
                            out[1][k - 1] += v / (a[3].x(path, prev) * disc[prev]);
                            k -= 1;
                        }
                    }
                }
            }
            out
        })
        .collect();
    assemble(
        rows,
        &if stress {
            vec![1]
        } else {
            vec![t, hcount, hcount, p * hcount, p * hcount, t, t]
        },
    )
}

pub(crate) fn mortgage_batch(a: &[View<'_>]) -> Vec<Vec<f64>> {
    assert_eq!(a.len(), 27);
    let n = a[15].v.len();
    let ns = a[5].ix(0);
    let t = a[0].shape[1];
    // Reuse the month event with the standard mortgage argument positions.
    let b: Vec<_> = a[..4]
        .iter()
        .chain(a[6..25].iter())
        .map(|x| View {
            v: x.v,
            shape: x.shape,
        })
        .collect();
    let rows: Vec<Vec<f64>> = (0..n)
        .into_par_iter()
        .map(|s| {
            let mut out = vec![0.0; ns];
            let oa = a[24].at(s);
            let eo = (-oa / 12.0).exp();
            for p in 0..a[0].shape[0] {
                let (mut bal, mut burn) = (1.0, 1.0);
                let mut q = (1.0 + a[15].at(s) / 12.0).powi(-a[17].at(s) as i32);
                let mut e = (-oa * a[25].at(s)).exp() * eo;
                let mut v = 0.0;
                for m in 0..t {
                    let (cf, _, _) =
                        mortgage_step(&b, s, p, m, &mut bal, &mut burn, &mut q, a[26].at(0) != 0.0);
                    v += cf * a[3].x(p, m) * e;
                    e *= eo;
                    if bal <= 1e-12 {
                        break;
                    }
                }
                out[a[4].ix(p)] += v;
            }
            out
        })
        .collect();
    let mut out = vec![0.0; ns * n];
    for s in 0..n {
        for k in 0..ns {
            out[k * n + s] = rows[s][k];
        }
    }
    vec![out]
}
