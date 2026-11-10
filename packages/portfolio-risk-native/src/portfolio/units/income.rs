//! Separate noninterest income from retained coefficients; never reprice or
//! reinterpret service revenue as external net interest income.
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;

#[no_mangle]
pub extern "C" fn portfolio_income_coefficients_version() -> u32 {
    1
}

fn money(value: f64) -> Option<f64> {
    (value.is_finite() && value.abs() <= MAX_MONETARY_AMOUNT).then_some(value)
}

/// # Safety
/// Caller owns aligned readable `units*periods`, `units` and `periods` input
/// buffers, plus writable `2*periods+2` output values. Output must not overlap
/// inputs. The call is synchronous. On failure no output is written.
#[no_mangle]
pub unsafe extern "C" fn portfolio_income_coefficients_call(
    fee_coefficients: *const f64,
    amounts: *const f64,
    interest_income: *const f64,
    units: usize,
    periods: usize,
    output: *mut f64,
) -> i32 {
    if periods == 0
        || periods > 4096
        || units > 1024
        || units.checked_mul(periods).is_none_or(|n| n > 1_000_000)
        || interest_income.is_null()
        || output.is_null()
        || units > 0 && (fee_coefficients.is_null() || amounts.is_null())
    {
        return 1;
    }
    let result = std::panic::catch_unwind(|| {
        let nii = std::slice::from_raw_parts(interest_income, periods);
        if nii.iter().any(|value| money(*value).is_none()) {
            return None;
        }
        let mut values = vec![0.; 2 * periods + 2];
        if units > 0 {
            let coefficients = std::slice::from_raw_parts(fee_coefficients, units * periods);
            let allocations = std::slice::from_raw_parts(amounts, units);
            if coefficients.iter().any(|value| !value.is_finite())
                || allocations
                    .iter()
                    .any(|value| !value.is_finite() || *value < 0. || *value > MAX_MONETARY_AMOUNT)
            {
                return None;
            }
            for (unit, allocation) in allocations.iter().enumerate() {
                for period in 0..periods {
                    values[period] =
                        money(values[period] + allocation * coefficients[unit * periods + period])?;
                }
            }
        }
        for period in 0..periods {
            values[periods + period] = money(nii[period] + values[period])?;
        }
        let fees = money(values[..periods].iter().sum())?;
        let interest = money(nii.iter().sum())?;
        values[2 * periods] = fees;
        values[2 * periods + 1] = money(interest + fees)?;
        Some(values)
    });
    match result {
        Ok(Some(values)) => {
            std::ptr::copy_nonoverlapping(values.as_ptr(), output, values.len());
            0
        }
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_fees_remain_separate_and_combine_in_native_order() {
        let mut output = [0.; 6];
        unsafe {
            assert_eq!(
                portfolio_income_coefficients_call(
                    [0.5, -0.2, -0.3, 0.6].as_ptr(),
                    [100., 50.].as_ptr(),
                    [2., -1.].as_ptr(),
                    2,
                    2,
                    output.as_mut_ptr()
                ),
                0
            );
        }
        assert_eq!(output, [35., 10., 37., 9., 45., 46.]);
    }

    #[test]
    fn empty_allocation_has_zero_fees_and_retains_interest() {
        let mut output = [0.; 6];
        unsafe {
            assert_eq!(
                portfolio_income_coefficients_call(
                    std::ptr::null(),
                    std::ptr::null(),
                    [2., -1.].as_ptr(),
                    0,
                    2,
                    output.as_mut_ptr()
                ),
                0
            );
        }
        assert_eq!(output, [0., 0., 2., -1., 0., 1.]);
    }

    #[test]
    fn invalid_coefficients_allocations_and_overflow_write_nothing() {
        for (coefficient, allocation, nii) in [
            (f64::NAN, 1., 0.),
            (1., -1., 0.),
            (1e15, 2., 0.),
            (0., 0., f64::INFINITY),
        ] {
            let mut output = [7.; 4];
            unsafe {
                assert_eq!(
                    portfolio_income_coefficients_call(
                        [coefficient].as_ptr(),
                        [allocation].as_ptr(),
                        [nii].as_ptr(),
                        1,
                        1,
                        output.as_mut_ptr()
                    ),
                    2
                );
            }
            assert_eq!(output, [7.; 4]);
        }
    }
}
