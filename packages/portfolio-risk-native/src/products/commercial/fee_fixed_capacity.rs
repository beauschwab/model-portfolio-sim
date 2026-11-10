//! Ordinary fee projections: complete raw, service, acquired-cost and cash owners.
use super::{adoption, BoundPath, ExitEvidence};
use crate::products::commercial::{cash::EconomicCash, credit_compiler as loan};
use portfolio_model_core::nonintegral_fee as fee;
use std::mem::{needs_drop, size_of};
type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b).ok_or("fixed fee capacity overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b).ok_or("fixed fee capacity overflow".into())
}
fn sum(v: impl IntoIterator<Item = usize>) -> R<usize> {
    v.into_iter().try_fold(0, add)
}
fn vector<T>(n: usize) -> R<usize> {
    if n == 0 {
        Ok(0)
    } else {
        mul(add(mul(n, 3)?, 8)?, size_of::<T>())
    }
}
fn scalar<T>(_: &T) -> R<()> {
    if needs_drop::<T>() {
        Err("fixed fee scalar acquired ownership".into())
    } else {
        Ok(())
    }
}

pub fn events(h: &adoption::History, allocations: usize) -> R<usize> {
    if h.policy.transfer.is_some() {
        return Err("fixed fee capacity requires separate transferred rights inventory".into());
    }
    sum([
        super::event_bound(&h.policy),
        allocations,
        h.acquisition_releases.len(),
    ])
}
/// Raw-dimensional portable reservation; the exhaustive construction bound must fit.
pub fn reservation(h: &adoption::History, allocations: usize) -> R<usize> {
    mul(
        add(add(events(h, allocations)?, h.policy.fees.len())?, 1)?,
        256 << 10,
    )
}
pub fn construction(h: &adoption::History, allocations: &Vec<adoption::Allocation>) -> R<usize> {
    let groups = events(h, allocations.len())?;
    let raw = loan::finite_fee_history_raw_layout(h)?;
    let costs = loan::finite_fee_allocations_raw_layout(allocations)?;
    // Before-exit prefix, complete service replay and acquired-cost replay can
    // coexist. Each core allowance includes its own input/sort/JSON workspace.
    let bytes = sum([
        mul(
            fee::fixed_capacity::construction(h.policy.fees.len(), groups)?,
            3,
        )?,
        mul(raw.retained_with_one_clone_bytes, 8)?,
        mul(costs.retained_with_one_clone_bytes, 4)?,
        size_of::<adoption::Projection>(),
        vector::<BoundPath>(1)?,
        vector::<f64>(groups)?,
        vector::<EconomicCash>(groups)?,
        mul(groups, 2 * 256)?,
        // Prefix/supplied/timeline/events staging and stable-sort scratch.
        mul(vector::<fee::Event>(groups)?, 6)?,
        mul(groups, 6 * 6 * 128)?,
        mul(vector::<(f64, u8, fee::Event)>(groups)?, 2)?,
        8192,
    ])?;
    if bytes > reservation(h, allocations.len())? {
        return Err("complete fee inventory exceeds its raw-dimensional allowance".into());
    }
    Ok(bytes)
}
impl BoundPath {
    fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            path_key,
            position_id,
            scenario,
            native_years,
            source,
            exit,
            native_credit_event_binding,
            loan_carrying_value_changed,
        } = self;
        scalar(path_key)?;
        scalar(native_credit_event_binding)?;
        scalar(loan_carrying_value_changed)?;
        let label = if let Some(ExitEvidence {
            native_event_id,
            native_year,
            reason,
            exercise_timing,
        }) = exit
        {
            scalar(native_year)?;
            scalar(reason)?;
            scalar(exercise_timing)?;
            native_event_id.capacity()
        } else {
            0
        };
        sum([
            size_of::<Self>(),
            position_id.capacity(),
            scenario.capacity(),
            mul(native_years.capacity(), size_of::<f64>())?,
            source.fixed_retained_capacity()? - size_of::<fee::ResultData>(),
            label,
        ])
    }
}
impl adoption::Projection {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            raw_fee_history,
            bound_path,
            cash,
            discount_spread,
            allocations,
            loan_original_cost_yield_unchanged,
        } = self;
        scalar(discount_spread)?;
        scalar(loan_original_cost_yield_unchanged)?;
        let labels = cash.iter().try_fold(0, |n, c| {
            let EconomicCash {
                event_id,
                kind,
                year,
                principal,
                interest,
                fees,
            } = c;
            scalar(year)?;
            scalar(principal)?;
            scalar(interest)?;
            scalar(fees)?;
            add(n, add(event_id.capacity(), kind.capacity())?)
        })?;
        sum([
            size_of::<Self>(),
            loan::finite_fee_history_raw_layout(raw_fee_history)?.retained_heap_bytes,
            bound_path.fixed_retained_capacity()? - size_of::<BoundPath>(),
            loan::finite_fee_allocations_raw_layout(allocations)?.retained_heap_bytes,
            mul(cash.capacity(), size_of::<EconomicCash>())?,
            labels,
        ])
    }
}
