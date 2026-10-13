//! Prospective counted FIFO arena. No public horizon/domain change or projector.
use crate::deposit_continuation::{self as legacy, Allocation, Origin, Rights, Settlement};
use crate::deposit_state::DepositStateResult;
use serde::Serialize;
use std::mem::size_of;

pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub origins: usize,
    pub settlements: usize,
    pub allocation_bound: usize,
    pub visit_bound: usize,
    pub work_reserved: usize,
    pub retained_reserved: usize,
    pub serialized_reserved: usize,
    pub joined_reserved: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct CashRange {
    pub native_cash_index: usize,
    pub native_event_id: String,
    pub ordinal: i32,
    pub full_native_retirement: bool,
    pub actual_bank_cash_interest: f64,
    pub allocation_start: usize,
    pub allocation_end: usize,
    pub retained_cash_roundoff_right: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CompactRights {
    pub origins: Vec<Origin>,
    pub settlements: Vec<CashRange>,
    pub allocations: Vec<Allocation>,
    pub remaining_origin_rights: f64,
    pub retained_cash_roundoff_right: f64,
    pub total_right: f64,
    pub plan: Plan,
    pub visits: usize,
    /// Internal work counter; excluded from counted JSON to avoid a circular receipt-size definition.
    #[serde(skip)]
    pub work_used: usize,
    pub retained_used: usize,
    pub serialized_used: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or_else(|| "FIFO count overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or_else(|| "FIFO count overflow".into())
}
pub fn cardinality_plan(i: usize, c: usize, identity_bytes: usize) -> Result<Plan, String> {
    let bound = add(i, c)?;
    // Counting, creation, scanning, stable sums and capacity accounting; not source projection work.
    let serialized = add(2048, add(mul(bound, 1024)?, mul(identity_bytes, 6)?)?)?;
    let work = add(add(mul(bound, 32)?, identity_bytes)?, mul(serialized, 4)?)?;
    let retained = add(
        size_of::<CompactRights>(),
        add(
            add(
                mul(i, size_of::<Origin>())?,
                mul(c, size_of::<CashRange>())?,
            )?,
            add(mul(bound, size_of::<Allocation>())?, identity_bytes)?,
        )?,
    )?;
    let joined = add(retained, serialized)?;
    if work > MAX_WORK || joined > MAX_BYTES {
        return Err("FIFO arena pre-generation work/bytes exceeded".into());
    }
    Ok(Plan {
        origins: i,
        settlements: c,
        allocation_bound: bound,
        visit_bound: bound,
        work_reserved: work,
        retained_reserved: retained,
        serialized_reserved: serialized,
        joined_reserved: joined,
    })
}
fn reserve<T>(v: &mut Vec<T>, slots: usize) -> Result<(), String> {
    v.try_reserve_exact(slots)
        .map_err(|_| "FIFO allocation failed".into())
}
pub fn allocate(
    s: &DepositStateResult,
    through: i32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<CompactRights, String> {
    allocate_mode(s, through, check, true)
}
/// Borrowed native owner charges serialization once at its complete source boundary.
pub fn allocate_without_output_count(
    s: &DepositStateResult,
    through: i32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<CompactRights, String> {
    allocate_mode(s, through, check, false)
}
fn allocate_mode(
    s: &DepositStateResult,
    through: i32,
    check: &mut impl FnMut() -> Result<(), String>,
    count_output: bool,
) -> Result<CompactRights, String> {
    check()?;
    let mut i = 0;
    let mut c = 0;
    let mut identity_bytes = 0;
    let mut prior = None;
    for o in &s.interest {
        check()?;
        if !o.amount.is_finite()
            || o.amount < 0.
            || o.start_ordinal >= o.end_ordinal
            || prior.is_some_and(|p| p > o.end_ordinal)
        {
            return Err("FIFO requires chronological nonnegative earned origins".into());
        }
        prior = Some(o.end_ordinal);
        i = add(i, 1)?;
    }
    prior = None;
    for cash in &s.cashflows {
        check()?;
        if !cash.cash_interest.is_finite()
            || cash.cash_interest > 0.
            || prior.is_some_and(|p| p > cash.ordinal)
        {
            return Err("FIFO requires chronological bank-payment cash".into());
        }
        prior = Some(cash.ordinal);
        c = add(c, 1)?;
        identity_bytes = add(identity_bytes, cash.event_id.len())?;
    }
    let mut plan = cardinality_plan(i, c, identity_bytes)?;
    if !count_output {
        plan.work_reserved = add(mul(add(i, c)?, 32)?, identity_bytes)?;
        plan.joined_reserved = plan.retained_reserved;
        plan.serialized_reserved = 0;
    }
    let mut origins = Vec::new();
    let mut settlements = Vec::new();
    let mut allocations = Vec::new();
    reserve(&mut origins, i)?;
    reserve(&mut settlements, c)?;
    reserve(&mut allocations, plan.allocation_bound)?;
    for (index, o) in s
        .interest
        .iter()
        .enumerate()
        .filter(|(_, o)| o.end_ordinal <= through)
    {
        check()?;
        origins.push(Origin {
            native_interest_index: index,
            start_ordinal: o.start_ordinal,
            end_ordinal: o.end_ordinal,
            earned: o.amount,
            allocated: 0.,
            remaining: o.amount,
        });
    }
    let mut cursor = 0;
    let mut visits = 0;
    for (index, cash) in s
        .cashflows
        .iter()
        .enumerate()
        .filter(|(_, c)| c.ordinal <= through && c.cash_interest != 0.)
    {
        check()?;
        let full = matches!(
            cash.kind.as_str(),
            "interest_payment" | "contractual_maturity"
        );
        let actual = -cash.cash_interest;
        let mut needed = actual;
        let start = allocations.len();
        while cursor < origins.len() {
            check()?;
            visits = add(visits, 1)?;
            if visits > plan.visit_bound {
                return Err("FIFO actual work exceeded reservation".into());
            }
            let o = &mut origins[cursor];
            if o.end_ordinal > cash.ordinal {
                break;
            }
            let amount = if full {
                o.remaining
            } else {
                needed.min(o.remaining).max(0.)
            };
            if amount > 0. {
                o.remaining -= amount;
                o.allocated += amount;
                needed -= amount;
                if allocations.len() == plan.allocation_bound {
                    return Err("FIFO actual allocations exceeded reservation".into());
                }
                allocations.push(Allocation {
                    native_interest_index: o.native_interest_index,
                    amount,
                });
            }
            if o.remaining == 0. {
                cursor += 1;
            }
            if !full && needed <= 0. {
                break;
            }
        }
        let assigned: f64 = allocations[start..].iter().map(|a| a.amount).sum();
        if !legacy::near(assigned, actual) {
            return Err("cash cannot retire current eligible earned origins".into());
        }
        settlements.push(CashRange {
            native_cash_index: index,
            native_event_id: cash.event_id.clone(),
            ordinal: cash.ordinal,
            full_native_retirement: full,
            actual_bank_cash_interest: cash.cash_interest,
            allocation_start: start,
            allocation_end: allocations.len(),
            retained_cash_roundoff_right: assigned - actual,
        });
    }
    let remaining_origin_rights = origins.iter().map(|o| o.remaining).sum::<f64>();
    let retained_cash_roundoff_right = settlements
        .iter()
        .map(|s| s.retained_cash_roundoff_right)
        .sum::<f64>();
    let retained_used = add(
        size_of::<CompactRights>(),
        add(
            add(
                mul(origins.capacity(), size_of::<Origin>())?,
                mul(settlements.capacity(), size_of::<CashRange>())?,
            )?,
            add(
                mul(allocations.capacity(), size_of::<Allocation>())?,
                settlements
                    .iter()
                    .try_fold(0, |n, s| add(n, s.native_event_id.capacity()))?,
            )?,
        )?,
    )?;
    let work_used = add(mul(add(i, c)?, 16)?, add(visits, allocations.len())?)?;
    if retained_used > plan.retained_reserved || work_used > plan.work_reserved {
        return Err("FIFO actual capacity/work exceeded preflight".into());
    }
    let mut result = CompactRights {
        origins,
        settlements,
        allocations,
        remaining_origin_rights,
        retained_cash_roundoff_right,
        total_right: remaining_origin_rights + retained_cash_roundoff_right,
        plan,
        visits,
        work_used,
        retained_used,
        serialized_used: 0,
    };
    if !count_output {
        return Ok(result);
    }
    // A bounded counting writer avoids a temporary serialized whole-result buffer.
    // Updating the decimal size field converges after at most a few spelling changes.
    for _ in 0..4 {
        let mut writer = CountOutput {
            bytes: 0,
            cap: result.plan.serialized_reserved,
            check,
        };
        serde_json::to_writer(&mut writer, &result).map_err(|e| e.to_string())?;
        result.work_used = add(result.work_used, writer.bytes)?;
        if result.work_used > result.plan.work_reserved {
            return Err("FIFO serialization work exceeded preflight".into());
        }
        if writer.bytes == result.serialized_used {
            return Ok(result);
        }
        result.serialized_used = writer.bytes;
    }
    Err("FIFO counted output size failed to stabilize".into())
}
struct CountOutput<'a, F> {
    bytes: usize,
    cap: usize,
    check: &'a mut F,
}
impl<F: FnMut() -> Result<(), String>> std::io::Write for CountOutput<'_, F> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        (self.check)().map_err(std::io::Error::other)?;
        self.bytes = self
            .bytes
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("FIFO output overflow"))?;
        if self.bytes > self.cap {
            return Err(std::io::Error::other("FIFO serialized output exceeded cap"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (self.check)().map_err(std::io::Error::other)
    }
}
impl CompactRights {
    /// Test/control expansion only; its additional allocations are not arena retention.
    pub fn expand(&self) -> Rights {
        Rights {
            origins: self.origins.clone(),
            settlements: self
                .settlements
                .iter()
                .map(|s| Settlement {
                    native_cash_index: s.native_cash_index,
                    native_event_id: s.native_event_id.clone(),
                    ordinal: s.ordinal,
                    full_native_retirement: s.full_native_retirement,
                    actual_bank_cash_interest: s.actual_bank_cash_interest,
                    allocations: self.allocations[s.allocation_start..s.allocation_end].to_vec(),
                    retained_cash_roundoff_right: s.retained_cash_roundoff_right,
                })
                .collect(),
            remaining_origin_rights: self.remaining_origin_rights,
            retained_cash_roundoff_right: self.retained_cash_roundoff_right,
            total_right: self.total_right,
        }
    }
}
