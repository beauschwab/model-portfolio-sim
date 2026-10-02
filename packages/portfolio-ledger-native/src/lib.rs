//! Pure, ordered daily GL reduction. No pricing, policy or persistence semantics.
//! One call per day (or closing replay); no retained pointers or global state.

pub mod daily;
pub mod flow_store;
pub mod spec;
#[path = "bin/types.rs"]
pub mod types;

/// Normalize and validate a raw daily specification. The returned buffer is
/// owned by this library and must be released with portfolio_balance_free.
///
/// # Safety
/// Input must reference `length` live bytes during this call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_balance_validate(
    input: *const u8,
    length: usize,
) -> *mut std::ffi::c_char {
    let result = std::panic::catch_unwind(|| -> Result<serde_json::Value, String> {
        if input.is_null() || length > 100_000_000 {
            return Err("null or oversized specification".into());
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            raw: serde_json::Value,
            max_positions: usize,
            max_work: usize,
        }
        let request: Request = serde_json::from_slice(std::slice::from_raw_parts(input, length))
            .map_err(|e| e.to_string())?;
        serde_json::to_value(spec::normalize(
            request.raw,
            request.max_positions,
            request.max_work,
        )?)
        .map_err(|e| e.to_string())
    })
    .unwrap_or_else(|_| Err("native specification validation failed".into()));
    let response = match result {
        Ok(v) => serde_json::json!({"ok":v}),
        Err(e) => serde_json::json!({"error":e}),
    };
    std::ffi::CString::new(response.to_string())
        .unwrap()
        .into_raw()
}
/// # Safety
/// Pointer must be a live, unreleased result of portfolio_balance_validate.
#[no_mangle]
pub unsafe extern "C" fn portfolio_balance_free(pointer: *mut std::ffi::c_char) {
    if !pointer.is_null() {
        drop(std::ffi::CString::from_raw(pointer));
    }
}

const INVALID: i32 = 1;
const UNBALANCED: i32 = 2;
const MISMATCH: i32 = 3;
const OVERFLOW: i32 = 4;
const PANIC: i32 = 5;
const MAX_KEYS: usize = 2_000_000;
const MAX_LINES: usize = 20_000_000;

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

fn apply(
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

#[no_mangle]
pub extern "C" fn portfolio_ledger_abi_version() -> u32 {
    1
}

/// Status: 0 success, 1 invalid input, 2 unbalanced transaction, 3 checkpoint
/// mismatch, 4 overflow, 5 panic. All failures leave output unchanged.
///
/// # Safety
/// Caller supplies live aligned buffers: previous/expected/output n, offsets
/// transactions+1, keys/values lines. Buffers must not overlap; inputs remain
/// immutable and output exclusively writable until return. Pointers (including
/// empty buffers) must be nonnull. Length caps are not pointer validity checks.
/// No pointer is retained. Output is copied only after all validation succeeds.
#[no_mangle]
pub unsafe extern "C" fn portfolio_ledger_apply(
    n: usize,
    transactions: usize,
    lines: usize,
    previous: *const f64,
    offsets: *const i64,
    keys: *const i64,
    values: *const f64,
    expected: *const f64,
    output: *mut f64,
) -> i32 {
    if n > MAX_KEYS
        || lines > MAX_LINES
        || transactions > lines
        || previous.is_null()
        || offsets.is_null()
        || keys.is_null()
        || values.is_null()
        || expected.is_null()
        || output.is_null()
        || !(previous as usize).is_multiple_of(8)
        || !(offsets as usize).is_multiple_of(8)
        || !(keys as usize).is_multiple_of(8)
        || !(values as usize).is_multiple_of(8)
        || !(expected as usize).is_multiple_of(8)
        || !(output as usize).is_multiple_of(8)
    {
        return INVALID;
    }
    std::panic::catch_unwind(|| {
        apply(
            std::slice::from_raw_parts(previous, n),
            std::slice::from_raw_parts(offsets, transactions + 1),
            std::slice::from_raw_parts(keys, lines),
            std::slice::from_raw_parts(values, lines),
            std::slice::from_raw_parts(expected, n),
            std::slice::from_raw_parts_mut(output, n),
        )
    })
    .map_or(PANIC, |r| r.err().unwrap_or(0))
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
