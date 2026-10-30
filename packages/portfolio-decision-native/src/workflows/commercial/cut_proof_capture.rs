//! Unpublished, funded capture of complete finite source records from the
//! existing native preparation. This is not a cut executor or publication permit.
use super::{candidates, cut_mixed};
use portfolio_compute_control::checkpoint;
use portfolio_ledger_native::daily;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    advanced_graph::Role, commercial_graph::ProofScope, commercial_units as units,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    io::{self, Write},
};

const MAX_RAW: usize = 2 << 20;
const MAX_WORK: usize = 32_000_000;
const MAX_BYTES: usize = 128 << 20;
const MAX_STREAM: usize = 4 << 20;
// Keep the existing source/compiled component allowances, reserve both old/new
// staging buffers plus writer workspace, and never deduct a source Admission.
const COMPONENT_RESERVE: usize = (32 << 20) + 6 * MAX_STREAM + 20 * (64 << 10);
const CAPTURE_RESERVE: usize = 2 * MAX_STREAM + (64 << 10);

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("cut proof capture overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or("cut proof capture overflow".into())
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Side {
    Completed,
    Revised,
}

#[derive(Default, Debug, PartialEq, Eq, Serialize)]
pub struct SourceCounts {
    pub original_pricing: usize,
    pub original_physical: usize,
    pub physical: usize,
    pub pricing: usize,
    pub pricing_down: usize,
    pub pricing_up: usize,
}
impl SourceCounts {
    fn total(&self) -> Result<usize, String> {
        [
            self.original_pricing,
            self.original_physical,
            self.physical,
            self.pricing,
            self.pricing_down,
            self.pricing_up,
        ]
        .into_iter()
        .try_fold(0, add)
    }
    fn observe(&mut self, scope: ProofScope, role: Role) -> Result<(), String> {
        let target = match scope {
            ProofScope::OriginalPricing => &mut self.original_pricing,
            ProofScope::OriginalPhysical => &mut self.original_physical,
            ProofScope::Current => match role {
                Role::Physical => &mut self.physical,
                Role::Pricing => &mut self.pricing,
                Role::PricingDown => &mut self.pricing_down,
                Role::PricingUp => &mut self.pricing_up,
            },
        };
        *target = add(*target, 1)?;
        Ok(())
    }
}

/// A component receipt only. It grants no ledger, publication or empirical
/// authority. Its input is the typed request; caller wire/parse ownership is not
/// certified here. Actual submitted-byte identity belongs to a future route.
#[derive(Debug, Serialize)]
pub struct CaptureReceipt {
    pub schema: &'static str,
    pub typed_request_bytes: usize,
    pub source_preparation_work_upper: usize,
    pub raw_prefix_work_upper: usize,
    pub capture_work: usize,
    pub admitted_work_upper: usize,
    pub reported_retained_bytes_upper: usize,
    pub encoded_bytes: usize,
    pub source_records: usize,
    pub counts: SourceCounts,
    pub all_selected_source_records_captured: bool,
    pub retains_typed_original_economic_owners: bool,
    pub original_quote_or_anchor_refitted: bool,
    pub cut_sim_executed: bool,
    pub cut_admission_granted: bool,
    pub complete_source_sim_codec_bounds: bool,
    pub independent_persisted_verification: bool,
    pub publication_eligible: bool,
}

struct Capture<'a> {
    output: &'a mut dyn Write,
    encoded: usize,
    records: usize,
    counts: SourceCounts,
    limit: usize,
}
impl Write for Capture<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let next = add(self.encoded, bytes.len()).map_err(io::Error::other)?;
        if next > self.limit {
            return Err(io::Error::other(
                "cut source capture exceeds4MiB staging component",
            ));
        }
        if daily::cut_census_work_spent().is_none() {
            return Err(io::Error::other(
                "cut source capture requires a remaining-work ticket",
            ));
        }
        // For valid emitted JSON, cells <= encoded bytes. Pay an upper for both
        // during this one serialization; do not count/encode/parse it again.
        daily::charge_cut_census_work(mul(bytes.len(), 2).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
        for chunk in bytes.chunks(32_768) {
            checkpoint().map_err(io::Error::other)?;
            self.output.write_all(chunk)?;
        }
        self.encoded = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        checkpoint().map_err(io::Error::other)?;
        self.output.flush()
    }
}
impl Capture<'_> {
    fn observe(
        &mut self,
        side: Side,
        source: units::UnitSourceObservation<'_>,
    ) -> Result<(), String> {
        checkpoint()?;
        daily::charge_cut_census_work(16)?;
        self.counts
            .observe(source.source.scope, source.source.role)?;
        #[derive(Serialize)]
        struct Record<'a> {
            schema: &'static str,
            side: Side,
            #[serde(flatten)]
            source: units::UnitSourceObservation<'a>,
        }
        serde_json::to_writer(
            &mut *self,
            &Record {
                schema: "commercial-cut-source-record-1",
                side,
                source,
            },
        )
        .map_err(|e| e.to_string())?;
        self.write_all(b"\n").map_err(|e| e.to_string())?;
        self.records = add(self.records, 1)?;
        Ok(())
    }
}

fn expected_counts(r: &cut_mixed::Request) -> Result<SourceCounts, String> {
    let mut n = SourceCounts::default();
    for candidate in [&r.completed_candidate, &r.revised_candidate] {
        for unit in &candidate.commercial.units {
            for position in &unit.graph.positions {
                // Exercise/acquisition receipts are separate proof families.
                // This first capture component cannot imply their completeness.
                if position.generated_policy.is_some()
                    || position.exercise.is_some()
                    || !position.original_physical.is_empty()
                {
                    return Err("cut source capture requires finite nonoptional transfer histories without original Physical acquisition proofs".into());
                }
                n.original_pricing = add(n.original_pricing, position.original_pricing.len())?;
                for h in &position.histories {
                    n.physical = add(n.physical, h.physical.len())?;
                    n.pricing = add(n.pricing, h.pricing.len())?;
                    n.pricing_down = add(n.pricing_down, h.pricing_down.len())?;
                    n.pricing_up = add(n.pricing_up, h.pricing_up.len())?;
                }
            }
        }
    }
    Ok(n)
}

/// Complete source projection records are written to caller-owned unpublished
/// staging. Failure can leave an incomplete prefix and never returns a receipt.
/// No JSON request selector, API/worker publication or Sim uses this component.
pub fn capture(r: &cut_mixed::Request, output: &mut dyn Write) -> Result<CaptureReceipt, String> {
    checkpoint()?;
    if portfolio_compute_control::active().is_none()
        || r.schema != cut_mixed::SCHEMA
        || r.preparation_receipt_policy.is_none()
    {
        return Err("explicit controlled selected cut source capture required".into());
    }
    let raw = strict_json::serialized_size(r, MAX_RAW)?;
    cut_mixed::validate_selected_authority(r)?;
    let expected = expected_counts(r)?;
    let a = candidates::admit_finite_preparation(&r.completed_candidate)?;
    let b = candidates::admit_finite_preparation(&r.revised_candidate)?;
    let source_work = add(a.work_upper, b.work_upper)?;
    let raw_work = mul(raw, 8)?;
    let initial = add(source_work, raw_work)?;
    let extra_bytes = add(mul(raw, 20)?, add(COMPONENT_RESERVE, CAPTURE_RESERVE)?)?;
    let retained = add(
        add(a.retained_bytes_upper, b.retained_bytes_upper)?,
        extra_bytes,
    )?;
    if initial > MAX_WORK || retained > MAX_BYTES || mul(raw, 40)? > 64 << 20 {
        return Err(format!("cut source capture joined32M/128MiB before generation: work={initial},bytes={retained}"));
    }
    let appended = cut_mixed::validate_raw_prefix(r)?;
    let cut = usize::try_from(
        r.cut_ordinal
            .checked_sub(r.completed_candidate.as_of_ordinal)
            .ok_or("cut source capture ordinal overflow")?,
    )
    .map_err(|_| "cut source capture precedes origin")?;
    if cut == 0
        || r.cut_ordinal >= a.full_end_ordinal
        || a.full_end_ordinal != b.full_end_ordinal
        || appended == 0
    {
        return Err("cut source capture requires a genuine positive cut, complete shared tail and future appended candidate".into());
    }
    let remaining = MAX_WORK - initial;
    daily::with_cut_census_budget(remaining, || {
        let sink = RefCell::new(Capture {
            output,
            encoded: 0,
            records: 0,
            counts: SourceCounts::default(),
            limit: MAX_STREAM,
        });
        let mut old_observer = |source: units::UnitSourceObservation<'_>| {
            sink.borrow_mut().observe(Side::Completed, source)
        };
        let mut new_observer = |source: units::UnitSourceObservation<'_>| {
            sink.borrow_mut().observe(Side::Revised, source)
        };
        let (completed, _) = candidates::with_finite_compiled_observer(
            &r.completed_candidate,
            (
                add(b.work_upper, raw_work)?,
                add(b.retained_bytes_upper, extra_bytes)?,
            ),
            &mut old_observer,
            &mut |_, _, _, _| {
                let executed =
                    daily::cut_census_work_spent().ok_or("cut source capture ticket absent")?;
                let (revised, _) = candidates::with_finite_compiled_observer(
                    &r.revised_candidate,
                    (
                        add(add(a.work_upper, raw_work)?, executed)?,
                        add(a.retained_bytes_upper, extra_bytes)?,
                    ),
                    &mut new_observer,
                    &mut |_, _, _, _| Ok(()),
                )?;
                drop(revised);
                Ok(())
            },
        )?;
        drop(completed);
        let mut sink = sink.into_inner();
        if sink.counts != expected || sink.records != expected.total()? {
            return Err("cut source capture complete raw/native role coverage differs".into());
        }
        #[derive(Serialize)]
        struct Complete<'a> {
            schema: &'static str,
            source_records: usize,
            counts: &'a SourceCounts,
            capture_complete: bool,
            cut_sim_executed: bool,
            publication_eligible: bool,
        }
        let complete = Complete {
            schema: "commercial-cut-source-capture-complete-1",
            source_records: sink.records,
            counts: &sink.counts,
            capture_complete: true,
            cut_sim_executed: false,
            publication_eligible: false,
        };
        // A separate fixed footer does not contain self-referential byte/work
        // counters. The returned receipt measures the completed footer as well.
        // Footer is fixed fields/counters with a bounded <1KiB representation;
        // its one additional allocation/serialization has a separately paid cap.
        daily::charge_cut_census_work(2048)?;
        let footer = strict_json::encode(&complete, 1024)?;
        sink.write_all(&footer).map_err(|e| e.to_string())?;
        sink.write_all(b"\n").map_err(|e| e.to_string())?;
        sink.flush().map_err(|e| e.to_string())?;
        checkpoint()?;
        let capture_work =
            daily::cut_census_work_spent().ok_or("cut source capture final ticket absent")?;
        Ok(CaptureReceipt {
            schema: "commercial-cut-source-capture-receipt-1",
            typed_request_bytes: raw,
            source_preparation_work_upper: source_work,
            raw_prefix_work_upper: raw_work,
            capture_work,
            admitted_work_upper: add(initial, capture_work)?,
            reported_retained_bytes_upper: retained,
            encoded_bytes: sink.encoded,
            source_records: sink.records,
            counts: sink.counts,
            all_selected_source_records_captured: true,
            retains_typed_original_economic_owners: false,
            original_quote_or_anchor_refitted: false,
            cut_sim_executed: false,
            cut_admission_granted: false,
            complete_source_sim_codec_bounds: false,
            independent_persisted_verification: false,
            publication_eligible: false,
        })
    })
}

#[cfg(test)]
#[path = "cut_proof_capture_tests.rs"]
mod tests;
