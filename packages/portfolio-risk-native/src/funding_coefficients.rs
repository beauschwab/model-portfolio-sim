//! Bounded coefficient-only reduction. Additive export; existing product ABI stays 8.
#[no_mangle]
pub extern "C" fn portfolio_funding_coefficients_version() -> u32 {
    1
}

/// # Safety
/// Caller owns aligned readable `units*dates`/`units` input buffers and writable
/// `dates` output values for this synchronous call. Buffers must not overlap.
#[no_mangle]
pub unsafe extern "C" fn portfolio_funding_coefficients_call(
    coefficients: *const f64,
    amounts: *const f64,
    units: usize,
    dates: usize,
    output: *mut f64,
) -> i32 {
    if dates == 0
        || dates > 250_000
        || units > 1024
        || units.checked_mul(dates).is_none_or(|n| n > 1_000_000)
        || output.is_null()
        || units > 0 && (coefficients.is_null() || amounts.is_null())
    {
        return 1;
    }
    let result = std::panic::catch_unwind(|| {
        let mut values = vec![0.; dates];
        if units > 0 {
            let c = std::slice::from_raw_parts(coefficients, units * dates);
            let a = std::slice::from_raw_parts(amounts, units);
            if c.iter().any(|v| !v.is_finite()) || a.iter().any(|v| !v.is_finite() || *v < 0.) {
                return None;
            }
            for (u, amount) in a.iter().enumerate() {
                for (d, value) in values.iter_mut().enumerate() {
                    *value += amount * c[u * dates + d];
                }
            }
        }
        if values.iter().any(|v| !v.is_finite()) {
            return None;
        }
        Some(values)
    });
    match result {
        Ok(Some(values)) => {
            std::ptr::copy_nonoverlapping(values.as_ptr(), output, dates);
            0
        }
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_cash_reduces_in_unit_order() {
        unsafe {
            let mut out = [0.; 3];
            assert_eq!(
                portfolio_funding_coefficients_call(
                    [0.9, 0.9, -0.1, -1., -1., 0.].as_ptr(),
                    [100., 50.].as_ptr(),
                    2,
                    3,
                    out.as_mut_ptr()
                ),
                0
            );
            assert_eq!(out, [40., 40., -10.]);
        }
    }
    #[test]
    fn failure_writes_nothing() {
        unsafe {
            let mut out = [7.; 1];
            assert_eq!(
                portfolio_funding_coefficients_call(
                    [f64::INFINITY].as_ptr(),
                    [1.].as_ptr(),
                    1,
                    1,
                    out.as_mut_ptr()
                ),
                2
            );
            assert_eq!(out, [7.]);
        }
    }
}
