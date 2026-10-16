//! Sealed raw-law metadata census, independent of sampled features/outcomes.
use super::*;

pub(super) const SCAN_PASSES: usize = 8;
pub(super) const HEADER_BYTES: usize = 128;
pub(super) struct StructuralFeatures {
    pub active_features_upper: usize,
    pub full_features_upper: usize,
    pub all_credit_hazards_zero: bool,
    pub scan_work_upper: usize,
    #[cfg(test)]
    pub measured_work: usize,
}
struct Scan {
    used: usize,
    upper: usize,
}
impl Scan {
    fn debit(&mut self, n: usize) -> Result<(), String> {
        self.used = self
            .used
            .checked_add(n)
            .filter(|v| *v <= self.upper)
            .ok_or("structural numerical metadata scan exceeds ticket")?;
        Ok(())
    }
}
#[cfg(test)]
thread_local! { static CALLS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)}; }
#[cfg(test)]
pub(super) fn calls() -> usize {
    CALLS.get()
}

pub(super) fn inspect(r: &Request) -> Result<StructuralFeatures, String> {
    #[cfg(test)]
    CALLS.set(CALLS.get() + 1);
    let draws = r.source.performing_draw_tranches.len();
    // The header census is itself bounded/debited before reading nested Vec
    // lengths. It reads no source identities, strings, observations or rates.
    let mut headers = Scan {
        used: 0,
        upper: draws.checked_add(1).ok_or("structural headers overflow")?,
    };
    headers.debit(1)?;
    let mut periods = r.source.source.current.periods.len();
    for draw in &r.source.performing_draw_tranches {
        check()?;
        headers.debit(1)?;
        periods = periods
            .checked_add(draw.source.current.periods.len())
            .ok_or("structural period headers overflow")?;
    }
    let upper = 32usize
        .checked_add(
            r.source
                .credit
                .intervals
                .len()
                .checked_mul(15)
                .ok_or("structural credit scan overflow")?,
        )
        .and_then(|v| v.checked_add(r.annual_covariance.len()))
        .and_then(|v| v.checked_add(periods.checked_mul(2)?))
        .and_then(|v| v.checked_add(draws.checked_mul(8)?))
        .ok_or("structural metadata scan overflow")?;
    let mut scan = Scan {
        used: headers.used,
        upper,
    };
    scan.debit(
        draws
            .checked_mul(2)
            .and_then(|v| v.checked_add(1))
            .and_then(|v| v.checked_add(periods))
            .ok_or("structural feature scan overflow")?,
    )?;
    let full = sources::feature_bound(r)?;
    let mut all_credit_hazards_zero = true;
    'hazards: for interval in &r.source.credit.intervals {
        check()?;
        for row in &interval.hazards_per_year {
            for hazard in row {
                scan.debit(1)?;
                if *hazard != 0. {
                    all_credit_hazards_zero = false;
                    break 'hazards;
                }
            }
        }
    }
    let mut covariance_zero = true;
    for covariance in &r.annual_covariance {
        scan.debit(1)?;
        if *covariance != 0. {
            covariance_zero = false;
            break;
        }
    }
    let mut observations = 0usize;
    for period in &r.source.source.current.periods {
        check()?;
        scan.debit(1)?;
        if let Coupon::Indexed {
            observations: values,
            ..
        } = &period.coupon
        {
            observations = observations
                .checked_add(values.len())
                .ok_or("structural observation headers overflow")?;
        }
    }
    let active = if draws > 0 {
        full
    } else if all_credit_hazards_zero && covariance_zero {
        0
    } else if all_credit_hazards_zero
        && matches!(
            &r.source.source.current.principal_plan,
            term::indexed::PrincipalPlan::Supplied { .. }
        )
    {
        r.drivers
            .len()
            .checked_add(if r.hazard_loadings.is_empty() {
                0
            } else {
                r.drivers.len()
            })
            .and_then(|v| v.checked_add(observations))
            .and_then(|v| v.checked_add(1))
            .and_then(|v| v.checked_add(r.source.source.current.periods.len()))
            .ok_or("structural varying feature upper overflow")?
    } else {
        full
    };
    Ok(StructuralFeatures {
        active_features_upper: active,
        full_features_upper: full,
        all_credit_hazards_zero,
        scan_work_upper: upper,
        #[cfg(test)]
        measured_work: scan.used,
    })
}
