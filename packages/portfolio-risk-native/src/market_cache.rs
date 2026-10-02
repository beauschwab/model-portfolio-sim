//! Shared immutable abcd calibration stage for raw product lifecycles.
use crate::cache::{resolve, Key};
use crate::calibration::fit_abcd as fit_abcd_uncached;

pub(crate) fn fit_abcd(
    quotes: &[f64],
    forwards: &[f64],
    dfs: &[f64],
    loadings: &[f64],
    initial: &[f64],
    grid: [f64; 2],
) -> Result<crate::least_squares::Fit, String> {
    let key = Key::new(1)
        .floats(quotes)
        .floats(forwards)
        .floats(dfs)
        .floats(loadings)
        .floats(initial)
        .floats(&grid);
    Ok((*resolve(key, || {
        let fit = fit_abcd_uncached(quotes, forwards, dfs, loadings, initial, grid)?;
        let bytes = fit.x.capacity() * 8 + std::mem::size_of_val(&fit);
        Ok((fit, bytes))
    })?)
    .clone())
}
