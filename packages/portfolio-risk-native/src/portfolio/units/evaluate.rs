use crate::math::arrays::View;

pub(crate) fn interactive(a: &[View<'_>]) -> Vec<Vec<f64>> {
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
