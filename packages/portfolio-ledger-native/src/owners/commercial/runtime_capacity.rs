//! Native finite runtime containers, independently of economic source storage.
//! Every source group is counted, including zero allocations and full tails.
//! Snapshot payloads are a separate exhaustive typed census at the caller.
use super::{Compiled, Runtime, Stock, Tracking};
use crate::owners::commercial::fees::FeeTracking;
use std::mem::size_of;

// Adding an owned runtime field must revisit this capacity implementation.
const _: fn(&Runtime) = |Runtime { paths: _, fees: _ }| {};
const _: fn(&Tracking) = |Tracking {
                              stocks: _,
                              legal: _,
                              snapshot: _,
                              legal_principal: _,
                          }| {};
const _: fn(&FeeTracking) = |FeeTracking {
                                 states: _,
                                 stocks: _,
                             }| {};
const _: fn(&portfolio_model_core::nonintegral_fee::State) =
    |portfolio_model_core::nonintegral_fee::State {
         acquired_fee_asset_valuation_adjustment: _,
         acquired_fee_liability_valuation_adjustment: _,
         transferred_net_stock: _,
         acquired_stock_selected: _,
         acquired_cost: _,
         released_acquired_cost: _,
         acquired_fee_asset: _,
         acquired_fee_liability: _,
         earned: _,
         refunded_earned: _,
         cancelled_unearned: _,
         waived_earned: _,
         recovered_waived: _,
         net_service_cash: _,
         fee_receivable: _,
         fee_payable: _,
         deferred_fee_income: _,
         prepaid_fee_expense: _,
         unearned_entitlement: _,
     }| {};

type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("commercial runtime capacity overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("commercial runtime capacity overflow".into())
}
fn vector<T>(count: usize) -> R<usize> {
    if count == 0 {
        return Ok(0);
    }
    // Pinned Vec doubling, including old/new overlap and minimum allocation.
    mul(add(mul(count, 3)?, 8)?, size_of::<T>())
}
fn tree<K, V>(count: usize) -> R<usize> {
    // Pinned BTree nodes hold <=11 key/value pairs. One whole internal node
    // per entry also covers root/leaf sparsity, pointers and split overlap.
    // Count payload separately; keys and values move rather than deep-clone
    // during a split. 128 bytes covers parent/length/edge bookkeeping.
    mul(count, add(mul(11, size_of::<(K, V)>())?, 128)?)
}

pub(crate) struct Capacity {
    /// Full loan/fee runtime, excluding separately counted Snapshot payload.
    pub runtime_bytes_upper: usize,
    /// All line, value, and snapshot-reference vectors used by the runtime.
    pub temporary_bytes_upper: usize,
    pub census_work: usize,
}

pub(crate) fn finite_runtime_capacity(
    source: &Compiled<'_>,
    mut charge: impl FnMut(usize) -> R<()>,
) -> R<Capacity> {
    let mut work = 0;
    let mut tick = |n| {
        charge(n)?;
        work = add(work, n)?;
        Ok::<_, String>(())
    };
    tick(1)?;
    if size_of::<usize>() != 8 || size_of::<String>() != 24 {
        return Err("commercial runtime census requires validated 64-bit layout".into());
    }
    let mut bytes = add(
        size_of::<Runtime>(),
        add(
            vector::<Tracking>(source.bindings.len())?,
            vector::<FeeTracking>(source.fees.len())?,
        )?,
    )?;
    let mut values = 0;
    let mut maximum_lines = 0;
    for binding in &source.bindings {
        tick(1)?;
        if binding.accounting.compact_bytes().is_some() {
            return Err("finite runtime census requires borrowed complete groups".into());
        }
        let (mut stocks, mut legal, mut payload) = (0, 0, 0);
        for index in 0..binding.accounting.group_count() {
            tick(1)?;
            let group = binding.accounting.group(index);
            stocks = add(stocks, group.postings.len())?;
            legal = add(legal, group.next.lots.len())?;
            for posting in &group.postings {
                tick(1)?;
                payload = add(payload, posting.coupon_id.as_ref().map_or(0, String::len))?;
            }
            for lot in &group.next.lots {
                tick(1)?;
                payload = add(payload, lot.coupon_id.len())?;
            }
            maximum_lines = maximum_lines.max(add(
                mul(add(group.postings.len(), group.next.lots.len())?, 2)?,
                2,
            )?);
        }
        bytes = add(
            bytes,
            add(
                payload,
                add(
                    vector::<(Stock, f64)>(stocks)?,
                    tree::<(String, usize), f64>(legal)?,
                )?,
            )?,
        )?;
        values = add(values, add(mul(add(stocks, legal)?, 2)?, 2)?)?;
    }
    for fee in &source.fees {
        tick(1)?;
        if fee.source.compact_bytes().is_some() {
            return Err("finite fee runtime census requires borrowed complete groups".into());
        }
        let (mut stocks, mut states, mut payload) = (0, 0, 0);
        for index in 0..fee.source.group_count() {
            tick(1)?;
            let group = fee.source.group(index);
            // Every posting can add a stock key; each group can add a state.
            // Repeated fee IDs are deliberately counted again rather than
            // relying on a hash/deduplication scan or zero monetary pruning.
            stocks = add(stocks, group.postings.len())?;
            states = add(states, 1)?;
            payload = add(
                payload,
                mul(add(group.postings.len(), 1)?, group.event.fee_id.len())?,
            )?;
            maximum_lines = maximum_lines.max(group.postings.len());
        }
        bytes = add(
            bytes,
            add(
                payload,
                add(
                    tree::<(String, &str), f64>(stocks)?,
                    tree::<String, portfolio_model_core::nonintegral_fee::State>(states)?,
                )?,
            )?,
        )?;
        values = add(values, mul(states, 8)?)?;
    }
    // Each call returns one vector; reserving all families together dominates
    // their staggered use. A staged replacement retains at most two runtime
    // copies; the caller reserves the original plus two additional copies.
    let temporary = add(
        vector::<super::Line>(maximum_lines)?,
        add(
            mul(vector::<(usize, &str, f64)>(values)?, 2)?,
            vector::<(usize, &super::Snapshot)>(source.bindings.len())?,
        )?,
    )?;
    Ok(Capacity {
        runtime_bytes_upper: bytes,
        temporary_bytes_upper: temporary,
        census_work: work,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_tree_nodes_cover_each_container_slot_without_json_assumptions() {
        assert_eq!(size_of::<usize>(), 8);
        assert_eq!(size_of::<String>(), 24);
        assert_eq!(tree::<(String, usize), f64>(1).unwrap(), 568);
        assert_eq!(
            tree::<String, portfolio_model_core::nonintegral_fee::State>(0).unwrap(),
            0
        );
        assert!(
            tree::<String, portfolio_model_core::nonintegral_fee::State>(1).unwrap()
                > size_of::<portfolio_model_core::nonintegral_fee::State>() * 11
        );
    }
    #[test]
    fn all_growth_dimensions_check_overflow_before_allocating() {
        assert_eq!(vector::<Tracking>(0).unwrap(), 0);
        assert!(vector::<Tracking>(usize::MAX).is_err());
        assert!(tree::<(String, usize), f64>(usize::MAX).is_err());
        assert!(add(usize::MAX, 1).is_err());
    }
}
