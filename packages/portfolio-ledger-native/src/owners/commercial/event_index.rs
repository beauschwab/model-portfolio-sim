//! Borrowed scenario identities and immutable event order. No financial group,
//! receipt or monetary state is copied into this index.
use super::intents::Scheduled;
use portfolio_compute_control::checkpoint;
use serde::Serialize;
use std::{cmp::Ordering, iter::FusedIterator};

type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("commercial event index overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("commercial event index overflow".into())
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    day: usize,
    year: f64,
    event: Scheduled,
}
fn event_key(event: Scheduled) -> (u8, usize, usize) {
    match event {
        Scheduled::Loan(binding, group) => (0, binding, group),
        Scheduled::Fee(binding, group) => (1, binding, group),
    }
}
fn within_day(a: &Entry, b: &Entry) -> Ordering {
    a.year
        .total_cmp(&b.year)
        .then(event_key(a.event).cmp(&event_key(b.event)))
}
fn ordered(a: &Entry, b: &Entry) -> Ordering {
    a.day.cmp(&b.day).then_with(|| within_day(a, b))
}

fn boundary(rows: &[Entry], day: usize, inclusive: bool, mut comparison: impl FnMut()) -> usize {
    let (mut left, mut right) = (0, rows.len());
    while left < right {
        let middle = left + (right - left) / 2;
        comparison();
        let before = if inclusive {
            rows[middle].day <= day
        } else {
            rows[middle].day < day
        };
        if before {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    left
}

fn search_upper(entries: usize) -> usize {
    (usize::BITS - entries.leading_zeros()) as usize
}

#[derive(Default)]
struct SortWork {
    comparisons: usize,
    copies: usize,
    peak_vector_capacity_bytes: usize,
}

/// An explicit bottom-up merge sort gives a portable comparison/move bound.
/// Scratch contains only Copy metadata, never source groups or financial state.
fn sort(rows: &mut Vec<Entry>) -> R<SortWork> {
    let mut work = SortWork::default();
    if rows.len() < 2 {
        return Ok(work);
    }
    checkpoint()?;
    let mut scratch = rows.clone();
    work.copies = rows.len();
    work.peak_vector_capacity_bytes = mul(
        add(rows.capacity(), scratch.capacity())?,
        std::mem::size_of::<Entry>(),
    )?;
    let mut width = 1usize;
    while width < rows.len() {
        let step = mul(width, 2)?;
        for start in (0..rows.len()).step_by(step) {
            checkpoint()?;
            let middle = start.saturating_add(width).min(rows.len());
            let end = start.saturating_add(step).min(rows.len());
            let (mut left, mut right) = (start, middle);
            for (offset, slot) in scratch[start..end].iter_mut().enumerate() {
                if offset % 256 == 0 {
                    checkpoint()?;
                }
                let from_left = if left == middle {
                    false
                } else if right == end {
                    true
                } else {
                    work.comparisons = add(work.comparisons, 1)?;
                    ordered(&rows[left], &rows[right]) != Ordering::Greater
                };
                *slot = if from_left {
                    let row = rows[left];
                    left += 1;
                    row
                } else {
                    let row = rows[right];
                    right += 1;
                    row
                };
                work.copies = add(work.copies, 1)?;
            }
        }
        std::mem::swap(rows, &mut scratch);
        width = step;
    }
    checkpoint()?;
    Ok(work)
}

/// Closed arithmetic bounds for this metadata sort, separate from all source,
/// allocation, Compiled runtime, Sim and output codec admission.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct EventSortUpper {
    pub entries: usize,
    pub passes: usize,
    pub comparison_upper: usize,
    pub metadata_copy_upper: usize,
}
pub fn sort_upper(entries: usize) -> R<EventSortUpper> {
    if entries < 2 {
        return Ok(EventSortUpper {
            entries,
            ..Default::default()
        });
    }
    let passes = (usize::BITS - (entries - 1).leading_zeros()) as usize;
    Ok(EventSortUpper {
        entries,
        passes,
        comparison_upper: mul(entries, passes)?,
        metadata_copy_upper: mul(entries, add(passes, 1)?)?,
    })
}

struct Timeline<'a> {
    scenario: &'a str,
    rows: Vec<Entry>,
}

#[derive(Default)]
pub(super) struct Index<'a> {
    timelines: Vec<Timeline<'a>>,
    work: SortWork,
    sealed: bool,
}

/// Observations about the actual metadata index only. These do not authorize a
/// root admission discount or certify a private allocator/global RSS bound.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct EventIndexInventory {
    pub scenario_timelines: usize,
    pub entries: usize,
    pub largest_timeline_entries: usize,
    pub sort_comparisons: usize,
    pub sort_comparison_upper: usize,
    pub sort_metadata_copies: usize,
    pub sort_metadata_copy_upper: usize,
    pub observed_largest_sort_vectors_capacity_bytes: usize,
    pub observed_retained_metadata_capacity_bytes: usize,
    pub owned_scenario_string_bytes: usize,
    pub daily_schedule_vector_allocations: usize,
    pub daily_sort_comparisons: usize,
    pub scenario_comparison_upper_per_lookup: usize,
    pub scenario_utf8_comparison_upper_per_lookup: usize,
    pub day_comparison_upper_per_lookup: usize,
    pub complete_source_sim_codec_bounds: bool,
}
impl<'a> Index<'a> {
    pub(super) fn register(&mut self, scenario: &'a str) -> R<usize> {
        if self.sealed {
            return Err("commercial event index already sealed".into());
        }
        if let Some(index) = self.timelines.iter().position(|t| t.scenario == scenario) {
            return Ok(index);
        }
        let index = self.timelines.len();
        self.timelines.push(Timeline {
            scenario,
            rows: Vec::new(),
        });
        Ok(index)
    }
    pub(super) fn push(
        &mut self,
        timeline: usize,
        day: usize,
        year: f64,
        event: Scheduled,
    ) -> R<()> {
        if self.sealed {
            return Err("commercial event index already sealed".into());
        }
        if !year.is_finite() {
            return Err("commercial event index nonfinite year".into());
        }
        self.timelines
            .get_mut(timeline)
            .ok_or("commercial event timeline absent")?
            .rows
            .push(Entry { day, year, event });
        Ok(())
    }
    pub(super) fn seal(&mut self) -> R<()> {
        if self.sealed {
            return Err("commercial event index already sealed".into());
        }
        for timeline in &mut self.timelines {
            let work = sort(&mut timeline.rows)?;
            self.work.comparisons = add(self.work.comparisons, work.comparisons)?;
            self.work.copies = add(self.work.copies, work.copies)?;
            self.work.peak_vector_capacity_bytes = self
                .work
                .peak_vector_capacity_bytes
                .max(work.peak_vector_capacity_bytes);
        }
        self.sealed = true;
        Ok(())
    }
    fn at(&self, scenario: &str, day: usize) -> &[Entry] {
        debug_assert!(self.sealed || self.timelines.is_empty());
        let Some(t) = self.timelines.iter().find(|t| t.scenario == scenario) else {
            return &[];
        };
        let start = boundary(&t.rows, day, false, || {});
        let end = boundary(&t.rows, day, true, || {});
        &t.rows[start..end]
    }
    pub(super) fn inventory(&self) -> R<EventIndexInventory> {
        let mut out = EventIndexInventory {
            scenario_timelines: self.timelines.len(),
            sort_comparisons: self.work.comparisons,
            sort_metadata_copies: self.work.copies,
            observed_largest_sort_vectors_capacity_bytes: self.work.peak_vector_capacity_bytes,
            observed_retained_metadata_capacity_bytes: mul(
                self.timelines.capacity(),
                std::mem::size_of::<Timeline<'_>>(),
            )?,
            ..Default::default()
        };
        for timeline in &self.timelines {
            out.scenario_utf8_comparison_upper_per_lookup = add(
                out.scenario_utf8_comparison_upper_per_lookup,
                add(1, timeline.scenario.len())?,
            )?;
            out.entries = add(out.entries, timeline.rows.len())?;
            out.largest_timeline_entries = out.largest_timeline_entries.max(timeline.rows.len());
            let upper = sort_upper(timeline.rows.len())?;
            out.sort_comparison_upper = add(out.sort_comparison_upper, upper.comparison_upper)?;
            out.sort_metadata_copy_upper =
                add(out.sort_metadata_copy_upper, upper.metadata_copy_upper)?;
            out.observed_retained_metadata_capacity_bytes = add(
                out.observed_retained_metadata_capacity_bytes,
                mul(timeline.rows.capacity(), std::mem::size_of::<Entry>())?,
            )?;
        }
        if out.sort_comparisons > out.sort_comparison_upper
            || out.sort_metadata_copies > out.sort_metadata_copy_upper
        {
            return Err("commercial metadata sort exceeded its construction bound".into());
        }
        out.scenario_comparison_upper_per_lookup = out.scenario_timelines;
        out.day_comparison_upper_per_lookup = mul(2, search_upper(out.largest_timeline_entries))?;
        Ok(out)
    }
}

/// Merge two immutable, already ordered daily slices. Every source event is
/// returned once, with no daily allocation, sort or source group projection.
pub(crate) struct OrderedEvents<'a> {
    loans: &'a [Entry],
    fees: &'a [Entry],
}
impl<'a> OrderedEvents<'a> {
    pub(super) fn new(
        loans: &'a Index<'_>,
        fees: &'a Index<'_>,
        scenario: &str,
        day: usize,
    ) -> Self {
        Self {
            loans: loans.at(scenario, day),
            fees: fees.at(scenario, day),
        }
    }
}
impl Iterator for OrderedEvents<'_> {
    type Item = Scheduled;
    fn next(&mut self) -> Option<Self::Item> {
        let loan_first = match (self.loans.first(), self.fees.first()) {
            (None, None) => return None,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => within_day(a, b) != Ordering::Greater,
        };
        let rows = if loan_first {
            &mut self.loans
        } else {
            &mut self.fees
        };
        let first = rows[0].event;
        *rows = &rows[1..];
        Some(first)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Entry is non-ZST. Each Vec allocation is <=isize::MAX bytes, so the
        // combined element count of these two borrowed slices cannot overflow.
        let n = self.loans.len() + self.fees.len();
        (n, Some(n))
    }
}
impl ExactSizeIterator for OrderedEvents<'_> {}
impl FusedIterator for OrderedEvents<'_> {}

#[cfg(test)]
#[path = "event_index_tests.rs"]
mod tests;
