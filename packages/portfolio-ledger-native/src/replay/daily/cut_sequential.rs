//! Native component for disjoint completed/revised source lifetimes. It grants
//! no raw coordinator or publication authority. Financial state is moved only;
//! JSON bytes are immutable comparison evidence, never a monetary checkpoint.
use super::*;
use crate::owners::commercial::intents::{Compiled, Scheduled};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use serde::{ser::SerializeSeq, ser::SerializeStruct, Serialize};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
#[path = "cut_detached_state.rs"]
mod detached;
use detached::DetachedState;

const MAX_PROOF: usize = 4 << 20;
const MAX_WORK: usize = 8_000_000;
const MAX_BYTES: usize = 32 << 20;

fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b).ok_or("sequential cut overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b).ok_or("sequential cut overflow".into())
}

struct Bindings<'a>(&'a Compiled<'a>, usize);
impl Serialize for Bindings<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.1))?;
        for b in &self.0.bindings[..self.1] {
            checkpoint().map_err(serde::ser::Error::custom)?;
            seq.serialize_element(&(
                b.scenario,
                b.position_id,
                b.path_key,
                b.origin_day,
                b.source_class.identity(),
                b.source_weight,
                b.allocated_face,
                b.native_unit_face,
            ))?;
        }
        seq.end()
    }
}
struct Fees<'a>(&'a Compiled<'a>, usize);
impl Serialize for Fees<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.1))?;
        for f in &self.0.fees[..self.1] {
            checkpoint().map_err(serde::ser::Error::custom)?;
            seq.serialize_element(&(f.loan_binding, f.native_position_id))?;
        }
        seq.end()
    }
}
struct Events<'a> {
    spec: &'a Spec,
    source: &'a Compiled<'a>,
    cut: usize,
}
impl Serialize for Events<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(None)?;
        for scene in
            std::iter::once("baseline").chain(self.spec.scenarios.iter().map(|s| s.name.as_str()))
        {
            for day in 0..=self.cut {
                checkpoint().map_err(serde::ser::Error::custom)?;
                for event in self.source.ordered_events(scene, day) {
                    checkpoint().map_err(serde::ser::Error::custom)?;
                    match event {
                        Scheduled::Loan(b, i) => seq.serialize_element(&(
                            scene,
                            day,
                            "loan",
                            b,
                            i,
                            &*self.source.bindings[b].accounting.group(i),
                        ))?,
                        Scheduled::Fee(b, i) => seq.serialize_element(&(
                            scene,
                            day,
                            "fee",
                            b,
                            i,
                            self.source.fees[b].native_years[i],
                            &*self.source.fees[b].source.group(i),
                        ))?,
                    }
                }
            }
        }
        seq.end()
    }
}
struct PrefixView<'a> {
    spec: &'a Spec,
    source: &'a Compiled<'a>,
    cut: usize,
    positions: usize,
    bindings: usize,
    fees: usize,
}
impl Serialize for PrefixView<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("CompleteObservedPrefix", 5)?;
        out.serialize_field(
            "specification",
            &cut::revision_spec(self.spec, &self.spec.positions[..self.positions]),
        )?;
        out.serialize_field(
            "position_owners",
            &self.source.position_owners[..self.positions],
        )?;
        out.serialize_field("bindings", &Bindings(self.source, self.bindings))?;
        out.serialize_field("fees", &Fees(self.source, self.fees))?;
        out.serialize_field(
            "ordered_events",
            &Events {
                spec: self.spec,
                source: self.source,
                cut: self.cut,
            },
        )?;
        out.end()
    }
}

struct ProofWriter {
    data: Vec<u8>,
    size: usize,
}

struct ProofSizer {
    size: usize,
}
impl Write for ProofSizer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .size
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("sequential proof size overflow"))?;
        if next > MAX_PROOF {
            return Err(io::Error::other("sequential proof exceeds4MiB"));
        }
        self.size = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn funded_proof<T: Serialize>(
    output: &mut dyn Write,
    value: &T,
) -> R<portfolio_model_core::transport::prepaid_json::WriteWork> {
    portfolio_model_core::transport::prepaid_json::write_with_output_charge(output, value, |work| {
        checkpoint()?;
        super::cut_shape::charge(work)
    })
}
impl Write for ProofWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        if self
            .data
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.size)
        {
            return Err(io::Error::other(
                "sequential prefix serialization changed after sizing",
            ));
        }
        self.data.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn exclusive(spec: &Spec, source: &Compiled<'_>) -> R<()> {
    crate::validation::specification::validate(spec, 60000, 90_000_000)?;
    if !spec.reverse_severities.is_empty()
        || !spec.netting_sets.is_empty()
        || !spec.policies.is_empty()
        || !spec.cashflows.is_empty()
        || source.position_owners.len() != spec.positions.len()
        || source.position_owners.iter().any(|owned| !owned)
        || source
            .bindings
            .iter()
            .any(|b| b.accounting.compact_bytes().is_some())
        || source
            .fees
            .iter()
            .any(|f| f.source.compact_bytes().is_some())
        || spec.accounts.iter().any(|a| {
            a.annual_fees != 0.
                || a.annual_costs != 0.
                || a.annual_dividends != 0.
                || a.tax_rate != 0.
                || a.capital_deductions != 0.
        })
    {
        return Err(
            "sequential cut requires exclusive finite commercial owners without generic overlays"
                .into(),
        );
    }
    Ok(())
}
fn lookup_work(spec: &Spec, source: &Compiled<'_>, cut: usize) -> R<usize> {
    let (a, b) = source.event_index_inventory()?;
    let lookup = add(
        add(
            a.scenario_utf8_comparison_upper_per_lookup,
            b.scenario_utf8_comparison_upper_per_lookup,
        )?,
        add(
            add(
                a.day_comparison_upper_per_lookup,
                b.day_comparison_upper_per_lookup,
            )?,
            8,
        )?,
    )?;
    add(
        mul(mul(add(spec.scenarios.len(), 1)?, add(cut, 1)?)?, lookup)?,
        mul(add(a.entries, b.entries)?, 8)?,
    )
}
fn state_reservation(spec: &Spec, source: &Compiled<'_>, raw: usize) -> R<(usize, usize)> {
    // Retain the existing conservative owner/spec/position conventions for
    // EACH detached run. This is a component reservation, not process RSS.
    let per_run = add(
        mul(source.retained_bytes_upper, 2)?,
        add(mul(spec.positions.len(), 4096)?, mul(raw, 20)?)?,
    )?;
    let conventional = add(
        mul(add(spec.scenarios.len(), 1)?, per_run)?,
        mul(
            add(spec.scenarios.len(), 1)?,
            std::mem::size_of::<DetachedState>(),
        )?,
    )?;
    // Complete reviewed claim/tail payload is not bounded by a posting-row
    // count. Visit every source snapshot, including after the report date and
    // at zero allocation. Each run retains at most one snapshot per binding;
    // an active staged clone and replacement are separately reserved below.
    let (mut snapshots, mut scan_work) = (0usize, 0usize);
    for b in &source.bindings {
        let mut maximum = 0;
        for index in 0..b.accounting.group_count() {
            checkpoint()?;
            let group = b.accounting.group(index);
            let layout = super::cut_prefix::commercial_snapshot_clone_layout(&group.next)?;
            maximum = maximum.max(add(layout.inline_bytes, layout.heap_bytes)?);
            scan_work = add(scan_work, add(layout.census_work, 1)?)?;
            if scan_work > MAX_WORK {
                return Err("sequential snapshot census exceeds8M".into());
            }
        }
        snapshots = add(snapshots, maximum)?;
    }
    Ok((
        add(conventional, mul(snapshots, add(spec.scenarios.len(), 3)?)?)?,
        scan_work,
    ))
}
fn guard(work: usize, bytes: usize) -> R<()> {
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err("sequential cut component exceeds8M/32MiB".into());
    }
    Ok(())
}

/// Opaque movement of genuine owned Sim fields, with no remaining borrow of
/// the completed graph or source. Cannot be cloned, serialized or deserialized.
/// The raw coordinator must separately prove joint admission before capture.
pub struct SequentialCommercialCut {
    original: Spec,
    cut: usize,
    bindings: usize,
    fees: usize,
    proof: Vec<u8>,
    states: Vec<DetachedState>,
    sink: Sink,
    enabled: Arc<AtomicBool>,
    prefix_work: usize,
    prefix_state_bytes: usize,
    prefix_row_vector_bytes: usize,
}

#[derive(Serialize)]
pub struct SequentialCutReceipt {
    pub protocol: u32,
    pub component_work_upper: usize,
    pub component_retained_bytes_upper: usize,
    pub complete_observed_prefix_bytes: usize,
    pub runs_moved: usize,
    pub completed_source_borrow_retained: bool,
    pub raw_coordinator_admission_granted: bool,
    pub independent_persisted_verification: bool,
}

impl SequentialCommercialCut {
    pub fn capture(
        spec: &Spec,
        source: &Compiled<'_>,
        cut: usize,
        output: Box<dyn Write + Send>,
    ) -> R<Self> {
        checkpoint()?;
        exclusive(spec, source)?;
        if cut == 0 || cut >= spec.horizon_days {
            return Err(
                "sequential cut requires a positive cut before the complete horizon".into(),
            );
        }
        let view = PrefixView {
            spec,
            source,
            cut,
            positions: spec.positions.len(),
            bindings: source.bindings.len(),
            fees: source.fees.len(),
        };
        let mut sizer = ProofSizer { size: 0 };
        let sizing = funded_proof(&mut sizer, &view)?;
        let size = sizer.size;
        let raw = strict_json::serialized_size(spec, 2 << 20)?;
        let (prefix_state_bytes, snapshot_scan_work) = state_reservation(spec, source, raw)?;
        let inventory =
            super::cut_inventory::commercial_sequential_cut_inventory(spec, source, cut)?;
        let execution_work = add(
            mul(source.work_upper, add(spec.scenarios.len(), 1)?)?,
            add(mul(raw, 8)?, mul(lookup_work(spec, source, cut)?, 2)?)?,
        )?;
        let prefix_work = add(
            execution_work,
            add(snapshot_scan_work, mul(sizing.total_work, 2)?)?,
        )?;
        let prefix_work = add(
            prefix_work,
            add(
                inventory.census_work_upper,
                add(
                    inventory.all_stream.scan_work()?,
                    mul(inventory.all_stream.encoded_bytes_upper, 2)?,
                )?,
            )?,
        )?;
        guard(
            prefix_work,
            add(
                add(prefix_state_bytes, size)?,
                inventory.stream_batch_retained_bytes_upper,
            )?,
        )?;
        let prefix_row_vector_bytes = inventory.stream_batch_memory.row_vector_bytes_upper;
        drop(inventory);
        let mut data = Vec::new();
        data.try_reserve_exact(size).map_err(|e| e.to_string())?;
        if data.capacity() != size {
            return Err("sequential proof capacity exceeds exact reservation".into());
        }
        let mut writer = ProofWriter { data, size };
        let encoded = funded_proof(&mut writer, &view)?;
        if writer.data.len() != size || encoded != sizing {
            return Err("sequential prefix serialization changed after sizing".into());
        }
        super::cut_shape::charge(execution_work)?;
        let enabled = Arc::new(AtomicBool::new(false));
        let mut sink = Sink::new(Box::new(cut::WindowWriter {
            enabled: enabled.clone(),
            output,
            structural_bytes: None,
        }));
        sink.prepaid_frames = true;
        let mut states = Vec::with_capacity(spec.scenarios.len() + 1);
        let baseline = cut::baseline();
        for scenario in std::iter::once(&baseline).chain(spec.scenarios.iter()) {
            checkpoint()?;
            let mut sim = Sim::new(
                spec,
                scenario.clone(),
                1.,
                true,
                &mut sink,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(source),
            )?;
            sim.opening()?;
            sim.run_range(1, cut)?;
            sim.sink.flush()?;
            states.push(DetachedState::detach(sim)?);
        }
        // None of the muted dictionaries reached the receiver. The first
        // visible run publishes their union; later runs continue that cursor.
        sink.sent = 0;
        Ok(Self {
            original: spec.clone(),
            cut,
            bindings: source.bindings.len(),
            fees: source.fees.len(),
            proof: writer.data,
            states,
            sink,
            enabled,
            prefix_work,
            prefix_state_bytes,
            prefix_row_vector_bytes,
        })
    }

    pub fn resume(mut self, revised: &Spec, source: &Compiled<'_>) -> R<SequentialCutReceipt> {
        checkpoint()?;
        exclusive(revised, source)?;
        if revised.positions.len() < self.original.positions.len()
            || source.bindings.len() < self.bindings
            || source.fees.len() < self.fees
        {
            return Err("sequential cut cannot remove original ownership".into());
        }
        cut::validate_new_positions(&self.original, revised, source)?;
        if source.bindings[self.bindings..]
            .iter()
            .any(|b| b.origin_day <= self.cut)
            || source.fees[self.fees..]
                .iter()
                .any(|f| f.loan_binding < self.bindings)
        {
            return Err(
                "sequential cut new ownership predates cut or overlaps an old fee owner".into(),
            );
        }
        let raw = strict_json::serialized_size(revised, 2 << 20)?;
        let (future_state_bytes, snapshot_scan_work) = state_reservation(revised, source, raw)?;
        let inventory =
            super::cut_inventory::commercial_sequential_cut_inventory(revised, source, self.cut)?;
        let execution_work = add(
            mul(source.work_upper, add(revised.scenarios.len(), 1)?)?,
            add(
                mul(raw, 8)?,
                add(
                    lookup_work(revised, source, self.cut)?,
                    mul(add(source.bindings.len(), revised.positions.len())?, 128)?,
                )?,
            )?,
        )?;
        let work = add(self.prefix_work, add(execution_work, snapshot_scan_work)?)?;
        let work = add(
            work,
            add(
                inventory.census_work_upper,
                add(
                    inventory.all_stream.scan_work()?,
                    mul(inventory.all_stream.encoded_bytes_upper, 2)?,
                )?,
            )?,
        )?;
        let bytes = add(
            self.prefix_state_bytes,
            add(future_state_bytes, self.proof.capacity())?,
        )?;
        // Muted prefix Values and journal columns were flushed/dropped before
        // detachment. Ordinary-row Vecs retain their high-water allocations.
        // Exact observed-prefix equality below proves that every live prefix
        // dictionary label also belongs to the complete revised inventory.
        let bytes = add(
            bytes,
            add(
                inventory.stream_batch_retained_bytes_upper,
                self.prefix_row_vector_bytes,
            )?,
        )?;
        drop(inventory);
        guard(work, bytes)?;
        let view = PrefixView {
            spec: revised,
            source,
            cut: self.cut,
            positions: self.original.positions.len(),
            bindings: self.bindings,
            fees: self.fees,
        };
        let (equal, comparison) = strict_json::serialized_matches_counted(
            &self.proof,
            &view,
            self.proof.len(),
            |work| {
                checkpoint()?;
                super::cut_shape::charge(work)
            },
        )?;
        let work = add(work, comparison.total_work)?;
        guard(work, bytes)?;
        if !equal {
            return Err("sequential cut immutable specification, binding or complete observed group differs".into());
        }
        super::cut_shape::charge(execution_work)?;
        let proof_size = self.proof.len();
        drop(self.proof);
        let runs = self.states.len();
        self.enabled.store(true, Ordering::Relaxed);
        for state in self.states {
            checkpoint()?;
            let sim = state.attach(revised, source, &mut self.sink);
            let mut resumed = cut::begin_resumed_sim(sim, self.cut, None)?;
            resumed.run_range(self.cut + 1, revised.horizon_days)?;
            resumed.finish()?;
            resumed.sink.flush()?;
        }
        for row in self.sink.reports.attribution(&revised.scenarios) {
            self.sink.emit("attribution", row)?;
        }
        self.sink.flush()?;
        checkpoint()?;
        write_frame(
            &mut self.sink.out,
            true,
            &json!({"complete":true,"protocol":9,
            "cut_day":self.cut,"native_cut_state_moved":true,"original_calendar_retained":true,
            "new_sim_started_at_cut":true,"prior_publication_verified":false,
            "sequential_source_owners":true}),
        )?;
        super::cut_shape::charge(2)?;
        self.sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
        Ok(SequentialCutReceipt {
            protocol: 9,
            component_work_upper: work,
            component_retained_bytes_upper: bytes,
            complete_observed_prefix_bytes: proof_size,
            runs_moved: runs,
            completed_source_borrow_retained: false,
            raw_coordinator_admission_granted: false,
            independent_persisted_verification: false,
        })
    }
}
