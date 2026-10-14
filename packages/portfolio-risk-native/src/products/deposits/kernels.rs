use crate::math::arrays::assemble;
use crate::math::arrays::View;
use crate::math::functions::{sigmoid, spline};
use rayon::prelude::*;

/// Floor on the monthly tail decay (attrition + short/12) for `runoff_tail`,
/// keeping the closed-form perpetuity finite on zero/negative-rate paths.
pub(crate) const DEPOSIT_TAIL_MIN_DECAY: f64 = 1e-4;

/// Deposit terminal treatment codes (`deposit-terminal-1`), the last kernel input.
/// 0 `payout`: residual balance repaid at par in the final month (historical default).
/// 1 `truncate`: no terminal principal; the residual is excluded from the liability.
/// 2 `runoff_tail`: the residual R keeps running off at the final month's attrition a,
///   paying the final paid rate r plus servicing, discounted at the final short rate s
///   held flat. Closed form, valued at the final month's deflator:
///   sum_k R (1-a)^k (r/12 + svc/12 + a) / (1 + s/12)^(k+1) = R (r/12 + svc/12 + a) / (a + s/12),
///   with the denominator floored at DEPOSIT_TAIL_MIN_DECAY. The tail enters only A
///   (valuation), lumped into the final month column, so OAS discounts it to the
///   horizon date only; runoff, interest and balance outputs stay in-horizon.
pub(crate) fn deposits(a: &[View<'_>], stress: bool) -> Vec<Vec<f64>> {
    assert_eq!(a.len(), if stress { 21 } else { 20 });
    let (p, t) = (a[0].shape[0], a[0].shape[1]);
    let n = a[6].v.len();
    let hcount = if stress { 1 } else { a[17].v.len() };
    let forward = !stress && a[18].at(0) != 0.0;
    let terminal = a[a.len() - 1].ix(0);
    assert!(terminal <= 2);
    let rows = (0..n)
        .into_par_iter()
        .map(|s| {
            let mut out = if stress {
                vec![vec![0.0; 1]]
            } else {
                vec![
                    vec![0.0; t],
                    vec![0.0; t],
                    vec![0.0; hcount],
                    vec![0.0; hcount],
                    vec![0.0; p * hcount],
                    vec![0.0; t],
                    vec![0.0; 1],
                ]
            };
            let h = if stress { a[17].ix(0) } else { 0 };
            let oa = a[16].at(s);
            let div = (-oa * h as f64 / 12.0).exp();
            let eo = (-oa / 12.0).exp();
            let disc: Vec<_> = (0..t)
                .map(|m| (-oa * (m + 1) as f64 / 12.0).exp())
                .collect();
            for path in 0..p {
                let mut bal = if stress {
                    a[19].z(s, path, a[18].ix(0))
                } else {
                    1.0
                };
                if bal <= 1e-12 {
                    continue;
                }
                let mut kf = 0;
                let mut e = div * eo;
                let mut v = 0.0;
                let mut buf = vec![0.0; t];
                let mut last = t;
                for m in h..t {
                    if forward && kf < hcount && m == a[17].ix(kf) {
                        out[4][path * hcount + kf] = (bal as f32) as f64;
                        out[3][kf] += bal;
                        kf += 1;
                    }
                    let r = (a[0].x(path, m) + a[6].at(s)).max(0.0);
                    let mut attr =
                        a[7].at(s) * spline(a[12].at(s) + m as f64, a[4].v, a[5].v) * a[8].at(s);
                    attr *= 1.0
                        + a[9].at(s)
                            * sigmoid(a[10].at(s) * (a[1].x(path, m) - r - a[11].at(s)), true);
                    let vel = a[2].x(path, m);
                    if vel > 0.0 {
                        attr *= 1.0 + a[14].at(0) * vel;
                    }
                    attr = attr.min(a[15].at(0));
                    let mut principal = bal * attr;
                    let mut tail = 0.0;
                    if m == t - 1 {
                        let residual = bal - principal;
                        if !stress {
                            out[6][0] += residual;
                        }
                        if terminal == 0 {
                            principal = bal;
                        } else if terminal == 2 {
                            let decay = (attr + a[1].x(path, m) / 12.0).max(DEPOSIT_TAIL_MIN_DECAY);
                            tail = residual * (r / 12.0 + a[13].at(s) / 12.0 + attr) / decay;
                        }
                    }
                    let cf = bal * (r / 12.0 + a[13].at(s) / 12.0) + principal + tail;
                    let intr = bal * r / 12.0;
                    bal -= principal;
                    if m == t - 1 {
                        // Truncated/tail residuals leave the in-horizon balance state.
                        bal = 0.0;
                    }
                    if stress {
                        v += cf * a[3].x(path, m) * e;
                        e *= eo;
                    } else {
                        buf[m] = cf * a[3].x(path, m);
                        out[0][m] += buf[m];
                        out[1][m] += principal;
                        out[5][m] += intr;
                    }
                    if bal <= 1e-12 {
                        last = m + 1;
                        break;
                    }
                }
                if stress {
                    out[0][0] += v / (a[3].x(path, h - 1) * div);
                } else if forward {
                    let mut k = hcount;
                    while k > 0 && a[17].ix(k - 1) >= last {
                        k -= 1;
                    }
                    let mut value = 0.0;
                    for m in (0..last).rev() {
                        value += buf[m] * disc[m];
                        while k > 0 && a[17].ix(k - 1) == m {
                            let prev = if m == 0 { t - 1 } else { m - 1 };
                            out[2][k - 1] += value / (a[3].x(path, prev) * disc[prev]);
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
            vec![t, t, hcount, hcount, p * hcount, t, 1]
        },
    )
}
