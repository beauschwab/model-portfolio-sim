//! Native product kernels. Inputs are borrowed contiguous f64 arrays; outputs
//! publish only after the entire call has passed shape and finite-value checks.
use rayon::prelude::*;
use std::slice;

#[repr(C)]
pub struct Buffer {
    pub data: *mut f64,
    pub len: usize,
    pub shape: [usize; 3],
}

pub(crate) struct View<'a> {
    pub(crate) v: &'a [f64],
    pub(crate) shape: [usize; 3],
}
impl View<'_> {
    fn at(&self, i: usize) -> f64 {
        self.v[i]
    }
    fn ix(&self, i: usize) -> usize {
        let x = self.v[i];
        assert!(x >= 0.0 && x.fract() == 0.0 && x < usize::MAX as f64);
        x as usize
    }
    fn x(&self, p: usize, m: usize) -> f64 {
        assert!(p < self.shape[0] && m < self.shape[1] && self.shape[2] == 1);
        self.v[p * self.shape[1] + m]
    }
    fn z(&self, s: usize, p: usize, h: usize) -> f64 {
        assert!(s < self.shape[0] && p < self.shape[1] && h < self.shape[2]);
        self.v[(s * self.shape[1] + p) * self.shape[2] + h]
    }
}

fn sigmoid(z: f64, rational: bool) -> f64 {
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
fn lut(u: f64, v: &[f64]) -> f64 {
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
fn spline(x: f64, knots: &[f64], c: &[f64]) -> f64 {
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
fn assemble(rows: Vec<Vec<Vec<f64>>>, widths: &[usize]) -> Vec<Vec<f64>> {
    let mut out: Vec<Vec<f64>> = widths
        .iter()
        .map(|w| Vec::with_capacity(rows.len() * w))
        .collect();
    for row in rows {
        for (dst, src) in out.iter_mut().zip(row) {
            dst.extend(src);
        }
    }
    out
}

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
    let cpr = (pp[4] * ramp * a[5].at(a[4].ix(m)) * (1.0 + pp[6] * a[2].x(p, m)).max(0.3) * lock
        + refi)
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
    assert_eq!(a.len(), if stress { 27 } else { 25 });
    let (p, t) = (a[0].shape[0], a[0].shape[1]);
    let n = a[13].v.len();
    let hcount = if stress { 1 } else { a[22].v.len() };
    let forward = !stress && a[23].at(0) != 0.0;
    let rational = a[if stress { 26 } else { 24 }].at(0) != 0.0;
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
            let h = if stress { a[22].ix(0) } else { 0 };
            let oa = a[21].at(s);
            let eo = (-oa / 12.0).exp();
            let div = (-oa * h as f64 / 12.0).exp();
            let disc: Vec<_> = (0..t)
                .map(|m| (-oa * ((m + 1) as f64 / 12.0)).exp())
                .collect();
            for path in 0..p {
                let (mut bal, mut burn) = if stress {
                    (a[24].z(s, path, a[23].ix(0)), a[25].z(s, path, a[23].ix(0)))
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
                    if forward && kf < hcount && m == a[22].ix(kf) {
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
                    while k > 0 && a[22].ix(k - 1) >= last {
                        k -= 1;
                    }
                    let mut v = 0.0;
                    for m in (0..last).rev() {
                        v += buf[m] * disc[m];
                        while k > 0 && a[22].ix(k - 1) == m {
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

pub(crate) fn deposits(a: &[View<'_>], stress: bool) -> Vec<Vec<f64>> {
    assert_eq!(a.len(), if stress { 20 } else { 19 });
    let (p, t) = (a[0].shape[0], a[0].shape[1]);
    let n = a[6].v.len();
    let hcount = if stress { 1 } else { a[17].v.len() };
    let forward = !stress && a[18].at(0) != 0.0;
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
                    let principal = if m == t - 1 { bal } else { bal * attr };
                    let cf = bal * (r / 12.0 + a[13].at(s) / 12.0) + principal;
                    let intr = bal * r / 12.0;
                    bal -= principal;
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
            vec![t, t, hcount, hcount, p * hcount, t]
        },
    )
}

pub(crate) fn mortgage_batch(a: &[View<'_>]) -> Vec<Vec<f64>> {
    assert_eq!(a.len(), 26);
    let n = a[15].v.len();
    let ns = a[5].ix(0);
    let t = a[0].shape[1];
    // Reuse the month event with the standard mortgage argument positions.
    let b: Vec<_> = a[..4]
        .iter()
        .chain(a[6..24].iter())
        .map(|x| View {
            v: x.v,
            shape: x.shape,
        })
        .collect();
    let rows: Vec<Vec<f64>> = (0..n)
        .into_par_iter()
        .map(|s| {
            let mut out = vec![0.0; ns];
            let oa = a[23].at(s);
            let eo = (-oa / 12.0).exp();
            for p in 0..a[0].shape[0] {
                let (mut bal, mut burn) = (1.0, 1.0);
                let mut q = (1.0 + a[15].at(s) / 12.0).powi(-a[17].at(s) as i32);
                let mut e = (-oa * a[24].at(s)).exp() * eo;
                let mut v = 0.0;
                for m in 0..t {
                    let (cf, _, _) =
                        mortgage_step(&b, s, p, m, &mut bal, &mut burn, &mut q, a[25].at(0) != 0.0);
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

pub(crate) fn behavioral(a: &[View<'_>]) -> Vec<Vec<f64>> {
    let mode = a[0].ix(0);
    let p = a[1].shape[0];
    let t = if mode == 0 {
        a[1].shape[2]
    } else {
        a[1].shape[1]
    };
    let rows = (0..p)
        .into_par_iter()
        .map(|path| {
            let mut out = vec![0.0; t];
            let mut x = if mode == 1 || mode == 3 {
                a[3].at(0)
            } else {
                0.0
            };
            for (m, v) in out.iter_mut().enumerate() {
                *v = match mode {
                    0 => {
                        let b = a[3].v;
                        let mut fair = b[10];
                        for (k, coef) in b.iter().enumerate().take(4) {
                            fair += a[1].z(path, k, m) * coef;
                        }
                        for k in 0..6 {
                            fair += a[2].x(k, m) * b[k + 4];
                        }
                        x = if m == 0 {
                            fair
                        } else {
                            x + a[4].at(0) * (fair - x)
                        };
                        x
                    }
                    1 => {
                        let q = a[2].v;
                        x = x + q[0] * (q[1] - x) * q[3] + q[2] * q[3].sqrt() * a[1].x(path, m);
                        x.max(0.0)
                    }
                    2 => {
                        let q = a[3].v;
                        let dr = (a[1].x(path, m) - a[1].x(path, 0)).clamp(-0.05, 0.05);
                        x += (q[0] + q[1] * dr) * q[3] + q[2] * q[3].sqrt() * a[2].x(path, m);
                        x.clamp(-3.0, 3.0).exp()
                    }
                    3 => {
                        let q = a[2].v;
                        let ff = a[1].x(path, m);
                        let eq = q[0]
                            + ff * (q[1] + (q[2] - q[1]) / (1.0 + (-q[3] * (ff - q[4])).exp()));
                        let gap = eq - x;
                        x += if gap > 0.0 { q[5] * gap } else { q[6] * gap };
                        x.max(0.0)
                    }
                    _ => panic!("unsupported behavioral model"),
                };
            }
            vec![out]
        })
        .collect();
    assemble(rows, &[t])
}

pub(crate) fn swaption(a: &[View<'_>]) -> Vec<Vec<f64>> {
    let n = a[2].shape[0];
    let out = (0..n)
        .into_par_iter()
        .map(|i| {
            // terms: tenor index, expiry month, semiannual periods, sign, strike
            let mut value = 0.0;
            for p in 0..a[0].shape[0] {
                let s = a[0].z(p, a[2].x(i, 0) as usize, a[2].x(i, 1) as usize);
                let d = 1.0 / (1.0 + 0.5 * s.max(0.0));
                let mut f = d;
                let mut ann = 0.0;
                for _ in 0..a[2].x(i, 2) as usize {
                    ann += 0.5 * f;
                    f *= d;
                }
                value += (a[2].x(i, 3) * (s - a[2].x(i, 4))).max(0.0)
                    * ann
                    * a[1].x(p, a[2].x(i, 1) as usize);
            }
            value / a[3].at(0)
        })
        .collect();
    vec![out]
}

#[allow(clippy::needless_range_loop)] // Month indexes address multiple output rows.
pub(crate) fn program(a: &[View<'_>]) -> Vec<Vec<f64>> {
    // reference paths, monthly originations, balance factors, spread, floating, side
    let horizon = a[1].v.len();
    let term = a[2].v.len();
    let p = a[0].shape[0];
    let mut out = vec![vec![0.0; horizon]; 3];
    for h in 0..horizon {
        let n = a[1].at(h);
        if n <= 0.0 {
            continue;
        }
        let y = (0..p)
            .map(|p| (a[0].x(p, h) + a[3].at(0)).max(0.0))
            .sum::<f64>()
            / p as f64;
        for m in h..(h + term).min(horizon) {
            let float = a[4].at(0) != 0.0;
            let rate = if float {
                (0..p)
                    .map(|p| (a[0].x(p, m) + a[3].at(0)).max(0.0))
                    .sum::<f64>()
                    / p as f64
            } else {
                y
            };
            let bal = n * a[2].at(m - h);
            out[0][m] += bal * rate / 12.0;
            out[1][m] += bal;
            let rem = (h + term - m) as f64 / 12.0;
            let dur = if float {
                1.0 / 12.0
            } else {
                ((1.0 - (1.0 + y).powf(-rem)) / y.max(1e-6) / (1.0 + y)).min(rem)
            };
            out[2][m] += a[5].at(0) * bal * dur * 1e-4;
        }
    }
    out
}

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

#[allow(clippy::needless_range_loop)] // Month indexes address asset/liability rows.
fn money_market(a: &[View<'_>]) -> Vec<Vec<f64>> {
    let h = a[4].ix(0);
    let p = a[0].shape[0];
    let mut out = vec![vec![0.0; h]; 2];
    for m in 0..h {
        let mean = (0..p).map(|p| a[0].x(p, m)).sum::<f64>() / p as f64;
        for s in 0..a[1].v.len() {
            let k = if a[3].at(s) > 0.0 { 0 } else { 1 };
            out[k][m] += a[1].at(s) * (mean + a[2].at(s)).max(0.0) / 12.0;
        }
    }
    out
}

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

#[no_mangle]
pub extern "C" fn portfolio_quant_abi_version() -> u32 {
    8
}

fn market_paths(a: &[View<'_>], mortgage: bool) -> Vec<Vec<f64>> {
    let context = crate::market::MarketContext::new(
        a[0].v,
        a[1].v,
        a[2].v,
        a[3].v,
        a[4].v,
        a[4].shape,
        [a[5].at(0), a[5].at(1), a[5].at(2)],
    )
    .expect("invalid market context");
    if mortgage {
        let model = crate::market::MortgageInputs {
            vol_points: a[6].v,
            cc_beta: a[7].v,
            cc_lambda: a[8].at(0),
            ps: [a[9].at(0), a[9].at(1), a[9].at(2), a[9].at(3)],
            eps_ps: a[10].v,
            eps_h: a[11].v,
            hpi: [a[12].at(0), a[12].at(1), a[12].at(2)],
            incentive_lag: a[13].ix(0),
        };
        let out = context.mortgage(&model).expect("invalid mortgage paths");
        vec![
            out.rates.df,
            out.rates.swaps,
            out.rates.short,
            out.mtg,
            out.hpi,
            out.yoy,
        ]
    } else {
        let out = context.rates().expect("invalid rate paths");
        vec![out.df, out.swaps, out.short]
    }
}

fn interactive(a: &[View<'_>]) -> Vec<Vec<f64>> {
    let h = a[0].shape[2];
    let mut values = vec![0.; 11 * h];
    let mut path = vec![0.; 3 * h];
    let mut scalars = vec![0.; 7];
    for i in 0..a[1].v.len() {
        for (j, value) in values[..10 * h].iter_mut().enumerate() {
            *value += a[1].at(i) * a[0].v[i * 10 * h + j];
        }
    }
    for m in 0..h {
        values[10 * h + m] = values[8 * h + m] - values[9 * h + m];
    }
    scalars[0] = values[..h].iter().sum();
    scalars[1] = values[2 * h];
    if !a[2].v.is_empty() {
        let b = a[2].v;
        for m in 0..h {
            path[m] = -(b[1] + values[2 * h + m]) * 200. / b[0] * 100.;
            path[h + m] = (b[3] + (b[4] + values[3 * h + m]).min(b[3] * b[10] / (1. - b[10])))
                / (b[5] + values[4 * h + m]).max(1e-9)
                * 100.;
            path[2 * h + m] =
                (b[6] + values[5 * h + m]) / (b[7] + values[6 * h + m]).max(1e-9) * 100.;
        }
        scalars[2] = path[0];
        scalars[3] = (b[1] + values[2 * h]) * 1e4 / (b[2] + values[8 * h]);
        scalars[4] = path[h];
        scalars[5] = path[2 * h];
        scalars[6] = (b[8] + scalars[0] * b[11] * (1. - b[12])) / (b[9] + values[8 * h - 1]) * 100.;
    }
    vec![values, path, scalars]
}

fn validate(op: u32, a: &[View<'_>]) {
    let counts = [
        0, 7, 17, 14, 25, 27, 19, 20, 9, 5, 26, 0, 4, 6, 4, 6, 5, 7, 4, 6, 14, 5, 1, 2, 4, 6, 1, 2,
        25, 27, 1, 3,
    ];
    assert!((1..=31).contains(&op));
    if op != 11 {
        assert_eq!(a.len(), counts[op as usize]);
    }
    let same = |indices: &[usize]| {
        for &i in indices {
            assert_eq!(a[i].shape, a[indices[0]].shape);
        }
    };
    match op {
        1 => {
            assert!(a[3].shape.iter().all(|&n| n > 0));
        }
        2 | 3 => {
            let off = if op == 2 { 3 } else { 2 };
            same(&(0..off).collect::<Vec<_>>());
            assert!(a[0].shape[0] > 0 && a[0].shape[1] > 0 && a[0].shape[2] == 1);
            assert!(!a[off].v.is_empty());
            assert_eq!(a[off].ix(0), 0);
            assert_eq!(a[off].ix(a[off].v.len() - 1), a[off + 1].v.len());
            same(&((off + 1)..if op == 2 { 12 } else { 9 }).collect::<Vec<_>>());
            for i in if op == 2 { 12..17 } else { 9..13 } {
                assert_eq!(a[i].v.len() + 1, a[off].v.len());
            }
        }
        4 | 5 | 10 => {
            same(&[0, 1, 2, 3]);
            let start = if op == 10 { 15 } else { 13 };
            same(&(start..start + 9).collect::<Vec<_>>());
            if op == 10 {
                assert_eq!(a[4].v.len(), a[0].shape[0]);
                assert_eq!(a[24].v.len(), a[start].v.len());
            }
            if op == 5 {
                same(&[24, 25]);
                assert_eq!(a[24].shape[..2], [a[13].v.len(), a[0].shape[0]]);
                assert!(a[22].ix(0) > 0);
            }
        }
        6 | 7 => {
            same(&[0, 1, 2, 3]);
            same(&[6, 7, 8, 9, 10, 11, 12, 13, 16]);
            if op == 7 {
                assert_eq!(a[19].shape[..2], [a[6].v.len(), a[0].shape[0]]);
                assert!(a[17].ix(0) > 0);
            }
        }
        8 | 9 => {
            assert!(!a[0].v.is_empty());
            assert_eq!(a[3].v.len() + 1, a[0].v.len());
        }
        11 => {
            assert!(!a.is_empty());
            let mode = a[0].ix(0);
            assert!(mode <= 3);
            assert_eq!(a.len(), if mode == 0 { 5 } else { 4 });
            if mode == 0 {
                assert_eq!(a[1].shape[1], 4);
                assert_eq!(a[2].shape, [6, a[1].shape[2], 1]);
                assert_eq!(a[3].v.len(), 11);
            } else {
                assert_eq!(a[1].shape[2], 1);
            }
            if mode == 2 {
                same(&[1, 2]);
            }
        }
        12 => {
            assert_eq!(a[2].shape[1..], [5, 1]);
            assert_eq!(a[1].shape, [a[0].shape[0], a[0].shape[2], 1]);
            assert_eq!(a[3].at(0), a[0].shape[0] as f64);
        }
        13 => {
            assert!(a[0].shape[0] > 0 && a[0].shape[1] >= a[1].v.len());
        }
        14 => {
            assert_eq!(a[1].v.len(), a[0].shape[0]);
            assert!(a[2].ix(0) <= a[0].shape[1]);
        }
        15 => {
            same(&[1, 2, 3]);
            assert!(!a[0].v.is_empty());
            assert_eq!(a[0].ix(0), 0);
            assert_eq!(a[0].ix(a[0].v.len() - 1), a[3].v.len());
            for w in a[0].v.windows(2) {
                assert!(w[0] <= w[1]);
            }
        }
        16 => {
            same(&[1, 2, 3]);
            assert!(a[0].shape[0] > 0 && a[4].ix(0) <= a[0].shape[1]);
        }
        17 => {
            assert_eq!(a[0].shape[1..], [4, 1]);
            assert_eq!(a[1].v.len(), 4);
            assert_eq!(a[3].v.len(), a[2].v.len() + 1);
            assert_eq!(a[4].shape[0], a[2].v.len());
        }
        18 => {
            assert_eq!(a[0].v.len(), a[1].v.len());
        }
        19 | 20 => {
            assert_eq!(a[0].v.len(), a[1].v.len());
            assert_eq!(a[2].v.len(), 4);
            assert_eq!(a[3].shape[1..], [a[4].shape[2], 1]);
            assert_eq!(a[5].v.len(), 3);
            if op == 20 {
                assert_eq!(a[6].shape, [6, 2, 1]);
                assert_eq!(a[7].v.len(), 11);
                assert_eq!(a[9].v.len(), 4);
                assert_eq!(a[12].v.len(), 3);
                for i in [10, 11] {
                    assert_eq!(a[i].shape, [a[4].shape[0], a[4].shape[1], 1]);
                }
            }
        }
        21 => {
            same(&[0, 1, 2]);
            assert_eq!(a[0].shape[2], 1);
            assert_eq!(a[3].v.len(), 6);
            assert_eq!(a[4].v.len(), 11);
        }
        22 => {
            assert_eq!(a[0].shape[1..], [11, 1]);
        }
        23 => {
            assert_eq!(a[0].shape[1..], [1, 1]);
        }
        24 => {}
        25 => {
            assert_eq!(a[0].shape[1..], [3, 1]);
            assert_eq!(a[1].shape[1..], [1, 1]);
            assert_eq!(a[2].shape[1..], [1, 1]);
            assert_eq!(a[3].shape[0], a[1].v.len());
            assert_eq!(a[3].shape[2], 1);
            assert_eq!(a[5].v.len(), 2);
        }
        26 => assert_eq!(a[0].shape[1..], [2, 1]),
        27 => {
            assert_eq!(a[0].shape[1..], [1, 1]);
            assert!(a[0]
                .v
                .iter()
                .all(|&x| x >= 0. && x <= u32::MAX as f64 && x.fract() == 0.));
            assert_eq!(a[1].v.len(), 3);
        }
        28 | 29 => {
            assert_eq!(a[2].shape[1..], [3, 1]);
            assert_eq!(a[3].shape[1..], [11, 1]);
            assert_eq!(a[5].shape[1..], [13, 1]);
            assert_eq!(a[8].v.len(), 17);
            assert_eq!(a[16].v.len(), 2);
            assert!(a[7]
                .v
                .iter()
                .all(|&v| v >= 0. && v <= u32::MAX as f64 && v.fract() == 0.));
        }
        30 => {
            assert_eq!(a[0].v.len(), 1);
            assert!(a[0].at(0) == 0. || a[0].at(0) == 1.);
        }
        31 => {
            assert_eq!(a[0].shape[1], 10);
            assert!((1..=360).contains(&a[0].shape[2]));
            assert_eq!(a[0].shape[0], a[1].v.len());
            assert!(a[1].v.iter().all(|v| v.is_finite() && *v >= 0.));
            assert!(a[0].v.iter().all(|v| v.is_finite()));
            assert!(a[2].v.is_empty() || a[2].v.len() == 13);
            if !a[2].v.is_empty() {
                assert!(a[2].v.iter().all(|v| v.is_finite()));
                assert!(a[2].at(0) > 0.);
                assert!((0.0..1.0).contains(&a[2].at(10)));
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) fn oas(a: &[View<'_>], solve: bool) -> Vec<Vec<f64>> {
    // CSR offsets, exact times, discounted sums, target/spread, paths,
    // tolerance, max_iter, lower, upper.
    let n = a[0].v.len() - 1;
    assert_eq!(a[1].v.len(), a[2].v.len());
    assert_eq!(a[0].ix(0), 0);
    assert_eq!(a[0].ix(n), a[1].v.len());
    assert!(a[4].at(0) > 0.0);
    // Flat result buffers avoid two/three heap allocations per contract on
    // every fixed-OAS bump. Each row retains the exact sequential reduction.
    let calculate = |s| {
        let (start, end) = (a[0].ix(s), a[0].ix(s + 1));
        assert!(end > start);
        let price = |o: f64| {
            let (mut px, mut deriv) = (0.0, 0.0);
            for j in start..end {
                let e = a[2].at(j) * (-o * a[1].at(j)).exp();
                px += e;
                deriv -= e * a[1].at(j);
            }
            (px / a[4].at(0), deriv / a[4].at(0))
        };
        if !solve {
            return [price(a[3].at(s)).0, 0.];
        }
        let (mut lo, mut hi, mut o) = (a[7].at(0), a[8].at(0), 0.0);
        for _ in 0..a[6].ix(0) {
            let (px, dpx) = price(o);
            let err = px - a[3].at(s);
            if err.abs() < a[5].at(0) {
                break;
            }
            if err > 0.0 {
                lo = lo.max(o);
            }
            if err < 0.0 {
                hi = hi.min(o);
            }
            let cand = o + if dpx.abs() > 1e-12 { -err / dpx } else { 0.0 };
            o = if cand <= lo || cand >= hi || !cand.is_finite() {
                (lo + hi) * 0.5
            } else {
                cand
            };
        }
        let px = price(o).0;
        assert!(
            px.is_finite() && (px - a[3].at(s)).abs() <= a[5].at(0),
            "OAS did not converge"
        );
        [o, px]
    };
    let mut output = vec![vec![0.; n]; if solve { 2 } else { 1 }];
    if solve {
        let (spread, prices) = output.split_at_mut(1);
        spread[0]
            .par_iter_mut()
            .zip(prices[0].par_iter_mut())
            .enumerate()
            .for_each(|(s, (o, p))| {
                let values = calculate(s);
                *o = values[0];
                *p = values[1];
            });
    } else {
        output[0]
            .par_iter_mut()
            .enumerate()
            .for_each(|(s, p)| *p = calculate(s)[0]);
    }
    output
}

/// Status 0 success, 1 invalid arrays/model/nonconvergence, 2 nonfinite output,
/// 3 pool error. No output buffer is modified on failure.
///
/// # Safety
/// Descriptors and their aligned data must be live for the declared sizes.
/// Inputs must not overlap outputs; outputs must be exclusively owned and not
/// overlap each other. No pointer is retained after the synchronous call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_quant_call(
    op: u32,
    inputs: *const Buffer,
    ni: usize,
    outputs: *const Buffer,
    no: usize,
    threads: usize,
) -> i32 {
    if inputs.is_null() || outputs.is_null() || ni > 64 || no > 16 || threads == 0 || threads > 256
    {
        return 1;
    }
    let result = std::panic::catch_unwind(|| {
        // SAFETY: caller supplies live descriptor arrays, per ABI contract.
        let input = unsafe { slice::from_raw_parts(inputs, ni) };
        let output = unsafe { slice::from_raw_parts(outputs, no) };
        let mut views = Vec::with_capacity(ni);
        for b in input {
            if b.data.is_null()
                || b.len > isize::MAX as usize / 8
                || b.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b)) != Some(b.len)
            {
                return Err(1);
            }
            // SAFETY: validated descriptor length; ownership required by caller.
            let v = unsafe { slice::from_raw_parts(b.data, b.len) };
            if v.iter().any(|v| v.is_nan()) {
                return Err(1);
            }
            views.push(View { v, shape: b.shape });
        }
        validate(op, &views);
        let compute = || match op {
            1 => lmm(&views),
            2 => corporate(&views, false),
            3 => corporate(&views, true),
            4 => mortgage(&views, false),
            5 => mortgage(&views, true),
            6 => deposits(&views, false),
            7 => deposits(&views, true),
            8 => oas(&views, true),
            9 => oas(&views, false),
            10 => mortgage_batch(&views),
            11 => behavioral(&views),
            12 => swaption(&views),
            13 => program(&views),
            14 => income(&views),
            15 => accrual(&views),
            16 => money_market(&views),
            17 => volatility(&views),
            18 => vec![crate::market::bootstrap(
                views[0].v,
                views[1].v,
                views[2].ix(0),
                views[3].at(0),
            )
            .expect("invalid curve")],
            19 | 20 => market_paths(&views, op == 20),
            22 => {
                let fit = crate::calibration::fit_current_coupon(views[0].v)
                    .expect("invalid current coupon fit");
                vec![fit.beta, vec![fit.lambda, fit.r_squared]]
            }
            23 => vec![
                crate::calibration::fit_ps_spread(views[0].v, views[1].at(0))
                    .expect("invalid spread fit")
                    .to_vec(),
            ],
            24 => vec![crate::calibration::factor_loadings(
                views[0].ix(0),
                views[1].ix(0),
                views[2].at(0),
                views[3].at(0),
            )
            .expect("invalid factor loadings")],
            25 | 26 => {
                let fit = if op == 25 {
                    crate::calibration::fit_abcd(
                        views[0].v,
                        views[1].v,
                        views[2].v,
                        views[3].v,
                        views[4].v,
                        [views[5].at(0), views[5].at(1)],
                    )
                } else {
                    crate::calibration::fit_deposits(views[0].v)
                }
                .expect("invalid nonlinear fit");
                vec![
                    fit.x,
                    vec![fit.rmse, fit.evaluations as f64, fit.status as f64],
                ]
            }
            27 => {
                let seed: Vec<_> = views[0].v.iter().map(|&v| v as u32).collect();
                let out = crate::random::SharedDraws::new(
                    &seed,
                    [views[1].ix(0), views[1].ix(1), views[1].ix(2)],
                )
                .expect("invalid shared draws");
                vec![out.rates, out.spread, out.hpi]
            }
            28 | 29 => {
                use crate::mortgage_risk::{MortgageRiskRequest, PrepayData, RiskConfig};
                let a = &views;
                let c = &a[8];
                let seed: Vec<_> = a[7].v.iter().map(|&v| v as u32).collect();
                let request = MortgageRiskRequest {
                    tenors: a[0].v,
                    swap_rates: a[1].v,
                    vol_quotes: a[2].v,
                    cc_history: a[3].v,
                    ps_history: a[4].v,
                    book: a[5].v,
                    original_hpi: a[6].v,
                    seed: &seed,
                    fixed_oas: a[9].v,
                    config: RiskConfig {
                        base_paths: c.ix(0),
                        sensitivity_paths: c.ix(1),
                        months: c.ix(2),
                        forwards: c.ix(3),
                        factors: c.ix(4),
                        dt: c.at(5),
                        tenor: c.at(6),
                        shift: c.at(7),
                        float32_paths: c.at(8) != 0.,
                        curve_bump: c.at(9),
                        vol_bump: c.at(10),
                        hpi: [c.at(11), c.at(12), c.at(13)],
                        incentive_lag: c.ix(14),
                        ps_spot: c.at(15),
                        rational_sigmoid: c.at(16) != 0.,
                    },
                    prepay: PrepayData {
                        month_of_year: a[10].v,
                        seasonality: a[11].v,
                        parameters: a[12].v,
                        ltv_knots: a[13].v,
                        ltv_coefficients: a[14].v,
                        smm_table: a[15].v,
                        smm_scale: a[16].at(0),
                        burnout_table: a[17].v,
                        burnout_scale: a[16].at(1),
                        cc_vol_points: a[18].v,
                        fico_x: a[19].v,
                        fico_y: a[20].v,
                        size_x: a[21].v,
                        size_y: a[22].v,
                        state_multipliers: a[23].v,
                        channel_multipliers: a[24].v,
                    },
                };
                if op == 28 {
                    let out = request.run().expect("invalid mortgage risk lifecycle");
                    vec![out.oas, out.price, out.dv01, out.sensitivities]
                } else {
                    let horizons: Vec<_> = (0..a[25].v.len()).map(|i| a[25].ix(i)).collect();
                    let out = request
                        .run_stress(&horizons, a[26].v)
                        .expect("invalid mortgage stress lifecycle");
                    vec![
                        out.base_value,
                        out.base_price,
                        out.shock_value,
                        out.pnl,
                        out.aggregate_base,
                        out.aggregate_pnl,
                        out.forward_dv01,
                        out.oas,
                    ]
                }
            }
            21 => {
                let a = &views;
                let shock = crate::market::ParallelShock {
                    horizon: a[3].ix(0),
                    shock_bp: a[3].at(1),
                    dt: a[3].at(2),
                    incentive_lag: a[3].ix(3),
                    cc_lambda: a[3].at(4),
                    hpi_beta: a[3].at(5),
                    cc_rate_beta_sum: a[4].v[..4].iter().sum(),
                };
                let out = shock
                    .apply(a[0].v, a[1].v, a[2].v, [a[0].shape[0], a[0].shape[1]])
                    .expect("invalid parallel shock");
                vec![out.mtg, out.hpi, out.yoy, out.df]
            }
            30 => vec![crate::cache::statistics(views[0].at(0) != 0.).to_vec()],
            31 => interactive(&views),
            _ => panic!("unsupported operation"),
        };
        // These operations contain no parallel work. A worker handoff costs more
        // than the coefficient reduction and worsens interactive tail latency.
        let result = if matches!(op, 30 | 31) {
            compute()
        } else {
            super::pool(threads)?.install(compute)
        };
        if result.len() != no {
            return Err(1);
        }
        for (values, out) in result.iter().zip(output) {
            if out.data.is_null()
                || values.len() != out.len
                || out.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b)) != Some(out.len)
            {
                return Err(1);
            }
            if values.iter().any(|x| !x.is_finite()) {
                return Err(2);
            }
        }
        for (values, out) in result.iter().zip(output) {
            // SAFETY: all lengths validated before the first write; caller owns output.
            unsafe {
                std::ptr::copy_nonoverlapping(values.as_ptr(), out.data, values.len());
            }
        }
        Ok(())
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(code)) => code,
        Err(_) => 1,
    }
}
