use crate::math::arrays::View;
use crate::math::functions::sigmoid;
use rayon::prelude::*;

pub(crate) fn corporate(a: &[View<'_>], cd: bool) -> Vec<Vec<f64>> {
    crate::compute_span!("term_kernel");
    assert_eq!(a.len(), if cd { 14 } else { 17 });
    let off = if cd { 2 } else { 3 };
    let pm = off + 1;
    let n = a[off].v.len() - 1;
    let t = a[0].shape[1];
    let periods = a[pm].v.len();
    assert_eq!(a[off].ix(0), 0);
    assert_eq!(a[off].ix(n), periods);
    // Split flat CSR storage into disjoint row slices before parallel work.
    // Avoid three heap allocations per instrument and subsequent assembly copies
    // on every scenario; each worker still reduces paths in the same order.
    let mut output = [vec![0.; periods], vec![0.; periods], vec![0.; periods]];
    let [values, income, principal] = &mut output;
    let (mut v_tail, mut i_tail, mut p_tail) = (
        values.as_mut_slice(),
        income.as_mut_slice(),
        principal.as_mut_slice(),
    );
    let mut rows = Vec::with_capacity(n);
    for s in 0..n {
        let j0 = a[off].ix(s);
        let j1 = a[off].ix(s + 1);
        assert!(j1 >= j0 && j1 <= periods);
        let (v, rest) = v_tail.split_at_mut(j1 - j0);
        v_tail = rest;
        let (i, rest) = i_tail.split_at_mut(j1 - j0);
        i_tail = rest;
        let (p, rest) = p_tail.split_at_mut(j1 - j0);
        p_tail = rest;
        rows.push((s, [v, i, p]));
    }
    rows.into_par_iter().for_each(|(s, out)| {
        let j0 = a[off].ix(s);
        let j1 = a[off].ix(s + 1);
        for p in 0..a[0].shape[0] {
            let mut bal = 1.0;
            for j in j0..j1 {
                let m = a[pm].ix(j);
                assert!(m < t);
                let (cf, intr, principal) = if cd {
                    let c = a[9].at(s);
                    let penalty = c * a[10].at(s) / 12.0;
                    let intr = bal * c * a[6].at(j);
                    let mut principal = 0.0;
                    if a[11].at(s) > 0.0 {
                        let inc = a[0].x(p, a[5].ix(j)) - c - penalty / a[7].at(j);
                        let q = a[13].v;
                        let annual = (q[0] + q[1] * sigmoid(q[2] * (inc - q[3]), true)).min(q[4]);
                        let wd = bal * (annual * a[11].at(s) * a[6].at(j)).min(0.95);
                        principal += wd * (1.0 - penalty);
                        bal -= wd;
                    }
                    let call = a[8].at(j);
                    if call >= 0.0 && bal > 0.0 && a[0].x(p, m) < c - a[12].at(s) {
                        principal += bal * call;
                        bal = 0.0;
                    }
                    if j == j1 - 1 && bal > 0.0 {
                        principal += bal;
                        bal = 0.0;
                    }
                    (intr + principal, intr, principal)
                } else {
                    let fl = a[12].at(s) == 1.0;
                    let mut c = a[13].at(s);
                    if fl {
                        let fm = a[6].ix(j);
                        let w = a[7].at(j);
                        c = (a[0].x(p, fm) * (1.0 - w) + a[0].x(p, fm + 1) * w + c)
                            .clamp(a[15].at(s), a[14].at(s));
                    }
                    let intr = bal * c * a[8].at(j);
                    let mut principal = a[9].at(j);
                    bal -= principal;
                    if bal < 0.0 {
                        principal += bal;
                        bal = 0.0;
                    }
                    let call = a[10].at(j);
                    let put = a[11].at(j);
                    let thr = a[16].at(s);
                    if call >= 0.0 && bal > 0.0 && !fl && a[1].x(p, m) < c - thr {
                        principal += bal * call;
                        bal = 0.0;
                    }
                    if put >= 0.0 && bal > 0.0 && a[1].x(p, m) > c + thr {
                        principal += bal * put;
                        bal = 0.0;
                    }
                    (intr + principal, intr, principal)
                };
                let df = &a[if cd { 1 } else { 2 }];
                let dprev = if m > 0 { df.x(p, m - 1) } else { 1.0 };
                out[0][j - j0] += cf * dprev / (1.0 + a[0].x(p, m) * a[pm + 1].at(j));
                out[1][j - j0] += intr;
                out[2][j - j0] += principal;
                if bal <= 0.0 {
                    break;
                }
            }
        }
    });
    output.into()
}
