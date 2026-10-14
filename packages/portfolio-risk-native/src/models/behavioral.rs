use crate::math::arrays::assemble;
use crate::math::arrays::View;
use rayon::prelude::*;

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
