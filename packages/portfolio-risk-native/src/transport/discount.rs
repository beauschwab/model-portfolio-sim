//! Checked C entrypoints for prepared-cashflow discount reduction.
use crate::pricing::discount::price;

#[no_mangle]
pub extern "C" fn portfolio_risk_abi_version() -> u32 {
    1
}

/// Status: 0 success, 1 invalid dimensions/inputs, 2 nonfinite price, 3 panic/pool error.
///
/// # Safety
/// Caller owns aligned, live, nonoverlapping buffers for the declared lengths:
/// offsets n+1; times/values m; oas/output n. Input storage must remain immutable
/// until return. Output must be exclusively writable. No pointer is retained.
#[no_mangle]
pub unsafe extern "C" fn portfolio_risk_price(
    n: usize,
    m: usize,
    offsets: *const i64,
    times: *const f64,
    values: *const f64,
    oas: *const f64,
    paths: usize,
    threads: usize,
    output: *mut f64,
) -> i32 {
    if n == 0 {
        return if m == 0 && paths > 0 && threads > 0 && threads <= 256 {
            0
        } else {
            1
        };
    }
    if n >= (isize::MAX as usize / 8)
        || m > (isize::MAX as usize / 8)
        || offsets.is_null()
        || times.is_null()
        || values.is_null()
        || oas.is_null()
        || output.is_null()
    {
        return 1;
    }
    // SAFETY: buffer lifetime/size/non-aliasing are the caller's ABI contract.
    // The Python adapter supplies checked immutable arrays and fresh output.
    std::panic::catch_unwind(|| {
        price(
            std::slice::from_raw_parts(offsets, n + 1),
            std::slice::from_raw_parts(times, m),
            std::slice::from_raw_parts(values, m),
            std::slice::from_raw_parts(oas, n),
            paths,
            threads,
            std::slice::from_raw_parts_mut(output, n),
        )
    })
    .map_or(3, |result| result.err().unwrap_or(0))
}
