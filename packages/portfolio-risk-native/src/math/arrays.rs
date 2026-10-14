//! Borrowed numeric array views and row assembly, independent of financial models.

pub(crate) struct View<'a> {
    pub(crate) v: &'a [f64],
    pub(crate) shape: [usize; 3],
}
impl View<'_> {
    pub(crate) fn at(&self, i: usize) -> f64 {
        self.v[i]
    }
    pub(crate) fn ix(&self, i: usize) -> usize {
        let x = self.v[i];
        assert!(x >= 0.0 && x.fract() == 0.0 && x < usize::MAX as f64);
        x as usize
    }
    pub(crate) fn x(&self, p: usize, m: usize) -> f64 {
        assert!(p < self.shape[0] && m < self.shape[1] && self.shape[2] == 1);
        self.v[p * self.shape[1] + m]
    }
    pub(crate) fn z(&self, s: usize, p: usize, h: usize) -> f64 {
        assert!(s < self.shape[0] && p < self.shape[1] && h < self.shape[2]);
        self.v[(s * self.shape[1] + p) * self.shape[2] + h]
    }
}

pub(crate) fn assemble(rows: Vec<Vec<Vec<f64>>>, widths: &[usize]) -> Vec<Vec<f64>> {
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
