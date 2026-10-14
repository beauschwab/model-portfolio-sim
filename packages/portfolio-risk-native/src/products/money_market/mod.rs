use crate::math::arrays::View;

#[allow(clippy::needless_range_loop)] // Month indexes address asset/liability rows.
pub(crate) fn money_market(a: &[View<'_>]) -> Vec<Vec<f64>> {
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
