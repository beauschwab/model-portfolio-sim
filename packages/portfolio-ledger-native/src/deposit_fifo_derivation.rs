//! Prospective linear event-plan derivation; no daily/owner/public schema changes.
use crate::dated_store::RecognitionSegment;
use portfolio_model_core::{deposit_fifo_capacity as fifo, deposit_state::DepositStateResult};
use serde::Serialize;
use std::mem::size_of;
#[derive(Debug, Clone, Serialize)]
pub struct CashPlanRange {
    pub native_cash_index: usize,
    pub segment_start: usize,
    pub segment_end: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct PlanArena {
    pub rights: fifo::CompactRights,
    pub segments: Vec<RecognitionSegment>,
    pub cash: Vec<CashPlanRange>,
    pub segment_bound: usize,
    pub capacity_reserved: usize,
    pub capacity_used: usize,
    pub work_reserved: usize,
    pub work_used: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or_else(|| "plan count overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or_else(|| "plan count overflow".into())
}
pub fn compile(
    s: &DepositStateResult,
    full_end: i32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<PlanArena, String> {
    compile_mode(s, full_end, check, true)
}
pub fn compile_counted(
    s: &DepositStateResult,
    full_end: i32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<PlanArena, String> {
    compile_mode(s, full_end, check, false)
}
fn compile_mode(
    s: &DepositStateResult,
    full_end: i32,
    check: &mut impl FnMut() -> Result<(), String>,
    count_output: bool,
) -> Result<PlanArena, String> {
    check()?;
    let mut ids = 0;
    for c in &s.cashflows {
        check()?;
        ids = add(ids, c.event_id.len())?;
    }
    let mut base = fifo::cardinality_plan(s.interest.len(), s.cashflows.len(), ids)?;
    if !count_output {
        base.work_reserved = add(mul(base.allocation_bound, 32)?, ids)?;
        base.joined_reserved = base.retained_reserved;
    }
    let segment_bound = mul(base.allocation_bound, 2)?;
    let capacity_reserved = add(
        base.joined_reserved,
        add(
            mul(segment_bound, size_of::<RecognitionSegment>())?,
            mul(s.cashflows.len(), size_of::<CashPlanRange>())?,
        )?,
    )?;
    let work_reserved = add(
        base.work_reserved,
        add(mul(base.allocation_bound, 16)?, mul(s.cashflows.len(), 16)?)?,
    )?;
    if capacity_reserved > fifo::MAX_BYTES || work_reserved > fifo::MAX_WORK {
        return Err("joined FIFO/plan reservation exceeded".into());
    }
    let rights = if count_output {
        fifo::allocate(s, full_end, check)?
    } else {
        fifo::allocate_without_output_count(s, full_end, check)?
    };
    let mut segments = Vec::new();
    let mut cash = Vec::new();
    segments
        .try_reserve_exact(segment_bound)
        .map_err(|_| "segment reserve failed")?;
    cash.try_reserve_exact(s.cashflows.len())
        .map_err(|_| "cash range reserve failed")?;
    let mut range_cursor = 0;
    let mut allocation_visits = 0;
    for (cash_index, c) in s.cashflows.iter().enumerate() {
        check()?;
        if c.ordinal > full_end {
            return Err("complete source cash tail exceeds plan end".into());
        }
        let start = segments.len();
        if c.cash_interest != 0. {
            let r = rights
                .settlements
                .get(range_cursor)
                .ok_or("missing cash range")?;
            if r.native_cash_index != cash_index {
                return Err("cash range lineage mismatch".into());
            }
            let mut prior_end = None;
            for a in &rights.allocations[r.allocation_start..r.allocation_end] {
                check()?;
                allocation_visits = add(allocation_visits, 1)?;
                let origin = &s.interest[a.native_interest_index];
                if let Some(end) = prior_end {
                    if end < origin.start_ordinal {
                        segments.push(RecognitionSegment {
                            start_ordinal: end,
                            end_ordinal: origin.start_ordinal,
                            accrued_interest: 0.,
                            book_amortization: 0.,
                        });
                    }
                }
                segments.push(RecognitionSegment {
                    start_ordinal: origin.start_ordinal,
                    end_ordinal: origin.end_ordinal,
                    accrued_interest: -a.amount,
                    book_amortization: 0.,
                });
                prior_end = Some(origin.end_ordinal);
            }
            range_cursor += 1;
        }
        if segments.len() > segment_bound {
            return Err("actual segment count exceeded reservation".into());
        }
        cash.push(CashPlanRange {
            native_cash_index: cash_index,
            segment_start: start,
            segment_end: segments.len(),
        });
    }
    let capacity_used = add(
        rights.retained_used,
        add(
            mul(segments.capacity(), size_of::<RecognitionSegment>())?,
            mul(cash.capacity(), size_of::<CashPlanRange>())?,
        )?,
    )?;
    let work_used = add(
        rights.work_used,
        add(mul(allocation_visits, 16)?, mul(cash.len(), 16)?)?,
    )?;
    if capacity_used > capacity_reserved || work_used > work_reserved {
        return Err("actual plan work/capacity exceeded reservation".into());
    }
    Ok(PlanArena {
        rights,
        segments,
        cash,
        segment_bound,
        capacity_reserved,
        capacity_used,
        work_reserved,
        work_used,
    })
}
