//! Historical continuation shape and numerical-policy boundary.
use crate::math::checked_least_squares as shared;
use nalgebra::DMatrix;
pub(super) use shared::Checked;
#[derive(Clone, Copy, Debug)]
pub(super) struct WorkBound(shared::WorkBound);
impl WorkBound {
    pub(super) fn new(m: usize, k: usize) -> Result<Self, String> {
        if k == 0 || m > 1024 || k > m / 2 {
            return Err("checked least-squares shape outside admitted tall domain".into());
        }
        Ok(Self(shared::WorkBound::new(m, k)?))
    }
    pub(super) fn total(self) -> usize {
        self.0.total()
    }
    pub(super) fn bytes(self) -> usize {
        self.0.bytes()
    }
}
pub(super) fn solve(
    a: &DMatrix<f64>,
    y: &[f64],
    check: impl FnMut() -> Result<(), String>,
) -> Result<Checked, String> {
    WorkBound::new(a.nrows(), a.ncols())?;
    shared::solve(a, y, check)
}
