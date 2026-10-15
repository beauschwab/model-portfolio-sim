//! Ordered, atomic journal reduction and checkpoint reconciliation.
pub(crate) const INVALID: i32 = 1;
pub(crate) const UNBALANCED: i32 = 2;
pub(crate) const MISMATCH: i32 = 3;
pub(crate) const OVERFLOW: i32 = 4;
pub(crate) const PANIC: i32 = 5;
pub(crate) const MAX_KEYS: usize = 2_000_000;
pub(crate) const MAX_LINES: usize = 20_000_000;

// Expansion summation retains low-order cancellation terms, unlike naive sum.
// Transaction checks use the same tolerance as the independent Python fsum gate.
fn accurate_sum(values: &[f64]) -> Result<f64, i32> {
    let mut partials: Vec<f64> = Vec::new();
    for &value in values {
        let mut x = value;
        let mut kept = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            if !hi.is_finite() {
                return Err(OVERFLOW);
            }
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[kept] = lo;
                kept += 1;
            }
            x = hi;
        }
        partials.truncate(kept);
        partials.push(x);
    }
    Ok(partials.iter().rev().sum())
}

pub(crate) fn apply(
    previous: &[f64],
    offsets: &[i64],
    keys: &[i64],
    values: &[f64],
    expected: &[f64],
    output: &mut [f64],
) -> Result<(), i32> {
    let n = previous.len();
    if n > MAX_KEYS
        || values.len() > MAX_LINES
        || expected.len() != n
        || output.len() != n
        || keys.len() != values.len()
        || offsets.is_empty()
        || offsets[0] != 0
        || offsets.last() != Some(&(values.len() as i64))
        || offsets.windows(2).any(|w| w[0] < 0 || w[1] <= w[0])
        || keys.iter().any(|&k| k < 0 || k as usize >= n)
        || previous
            .iter()
            .chain(values)
            .chain(expected)
            .any(|x| !x.is_finite())
    {
        return Err(INVALID);
    }
    // Work privately: failed transactions/checkpoints never partially publish.
    let mut staged = previous.to_vec();
    for range in offsets.windows(2) {
        let lo = range[0] as usize;
        let hi = range[1] as usize;
        let amounts = &values[lo..hi];
        let scale: f64 = amounts.iter().map(|x| x.abs()).sum();
        if !scale.is_finite() {
            return Err(OVERFLOW);
        }
        if accurate_sum(amounts)?.abs() > 1e-9 * scale.max(1.0) {
            return Err(UNBALANCED);
        }
        for j in lo..hi {
            let balance = &mut staged[keys[j] as usize];
            *balance += values[j];
            if !balance.is_finite() {
                return Err(OVERFLOW);
            }
        }
    }
    for (&got, &want) in staged.iter().zip(expected) {
        if (got - want).abs() > 1e-8_f64.max(1e-9 * got.abs().max(want.abs())) {
            return Err(MISMATCH);
        }
    }
    output.copy_from_slice(&staged);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_reduction_and_cancellation() {
        let mut out = [99.; 2];
        apply(
            &[0., 0.],
            &[0, 4],
            &[0, 0, 0, 1],
            &[1e16, 1., -1e16, -1.],
            &[0., -1.],
            &mut out,
        )
        .unwrap();
        assert_eq!(out, [0., -1.]); // sequential balances, accurate transaction sum
        assert_eq!(accurate_sum(&[1e16, 1., -1e16]).unwrap(), 1.);
    }

    #[test]
    fn errors_are_atomic() {
        for (keys, values, expected, code) in [
            (vec![0, 1], vec![1., -2.], vec![1., -2.], UNBALANCED),
            (vec![0, 1], vec![1., -1.], vec![2., -1.], MISMATCH),
            (vec![-1, 1], vec![1., -1.], vec![1., -1.], INVALID),
            (vec![0, 1], vec![f64::NAN, -1.], vec![1., -1.], INVALID),
            (
                vec![0, 1],
                vec![f64::MAX, -f64::MAX],
                vec![1., -1.],
                OVERFLOW,
            ),
        ] {
            let mut out = [123.; 2];
            assert_eq!(
                apply(&[0., 0.], &[0, 2], &keys, &values, &expected, &mut out),
                Err(code)
            );
            assert_eq!(out, [123.; 2]);
        }
    }

    #[test]
    fn malformed_offsets_and_empty_batch() {
        let mut out = [7.; 2];
        for offsets in [&[1, 2][..], &[0, 3, 2], &[0, 0, 2], &[0, -1, 2]] {
            assert_eq!(
                apply(&[0.; 2], offsets, &[0, 1], &[1., -1.], &[1., -1.], &mut out),
                Err(INVALID)
            );
        }
        apply(&[2., -2.], &[0], &[], &[], &[2., -2.], &mut out).unwrap();
        assert_eq!(out, [2., -2.]);
        apply(&[], &[0], &[], &[], &[], &mut []).unwrap();
    }
}
