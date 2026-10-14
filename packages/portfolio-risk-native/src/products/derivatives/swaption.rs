use crate::math::arrays::View;
use rayon::prelude::*;

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
