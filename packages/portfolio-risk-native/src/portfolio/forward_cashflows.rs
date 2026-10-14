use crate::math::arrays::View;

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
