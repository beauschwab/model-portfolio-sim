//! Genuine native completed-prefix -> moved Sim -> future candidate journal.
//! Caller JSON can select raw contracts, never a cut monetary checkpoint.
use super::{candidate_holdings, candidates};
#[path = "cut_receipts.rs"]
mod receipts;
use portfolio_compute_control::checkpoint;
use portfolio_ledger_native::daily;
use portfolio_model_core::strict_json;
pub use receipts::{Policy as PreparationReceiptPolicy, Preparation as PreparationReceipt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

pub const SCHEMA: &str = "commercial-cut-mixed-journal-1";
const MAX_RAW: usize = 2 << 20;
const MAX_OUTPUT: usize = 32 << 20;
const MAX_COMPONENT: usize = 4 << 20;
const MAX_WORK: usize = 32_000_000;
const MAX_BYTES: usize = 128 << 20;
const CUT_WORK: usize = 8_000_000;
const CUT_BYTES: usize = 32 << 20;
const VALUE_PHASE_BYTES: usize = 64 << 20;
const VALUE_ENCODED_UPPER: usize = VALUE_PHASE_BYTES / 20;
// At most two fixed sixteen-table descriptor sets: no raw IDs, snapshots or
// monetary groups are retained in these descriptors. Enforce this independent
// selected component cap and reserve its decoded capacity before generation.
const DESCRIPTOR_JSON_LIMIT: usize = 64 << 10;
const DESCRIPTOR_MEMORY_LIMIT: usize = DESCRIPTOR_JSON_LIMIT * 20;
const SCOPE: &str = "one genuine native completed Sim prefix moves its complete live state into a distinct resumed Sim; original source clocks, gross rights, policy counters, original loan and fee anchors retained; new future commercial AC owners append zero state and acquire once; no deserialized monetary checkpoint, observed bank opening, native prior publication, arbitrary mixed owners or empirical/accounting approval";
type Tables = BTreeMap<String, Vec<Value>>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub cut_ordinal: i32,
    pub completed_candidate: candidates::Request,
    pub revised_candidate: candidates::Request,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparation_receipt_policy: Option<PreparationReceiptPolicy>,
}
#[derive(Serialize)]
pub struct SelectedAdmission {
    schema: &'static str,
    initial_source_work_upper: usize,
    final_joined_work_upper: usize,
    owned_wire_bytes: usize,
    owned_wire_capacity: usize,
    typed_raw_bytes: usize,
    source_owner_retained_bytes_upper: usize,
    retention_phases: receipts::Retention,
    completed_preparation_work_upper: usize,
    revised_preparation_work_upper: usize,
    completed_receipt_bytes: usize,
    revised_receipt_bytes: usize,
    descriptor_retained_bytes_upper: usize,
    cut_ledger_census: daily::CommercialCutAdmission,
    work_components: SelectedWork,
    final_result_shape: daily::CutJsonShape,
    output_inventory: daily::CommercialCutInventory,
}
#[derive(Default, Serialize)]
struct SelectedWork {
    source_preparation: usize,
    raw_prefix: usize,
    preparation_receipt_scans: usize,
    structural_census: usize,
    independent_cut_guard: usize,
    repeated_specification_scans: usize,
    prepared_specification_census: usize,
    all_native_stream_passes: usize,
    visible_stream_passes: usize,
    decoded_table_passes: usize,
    final_result_passes: usize,
    original_raw_census: usize,
    descriptor_scans: usize,
}
impl SelectedWork {
    fn total(&self) -> Result<usize, String> {
        [
            self.source_preparation,
            self.raw_prefix,
            self.preparation_receipt_scans,
            self.structural_census,
            self.independent_cut_guard,
            self.repeated_specification_scans,
            self.prepared_specification_census,
            self.all_native_stream_passes,
            self.visible_stream_passes,
            self.decoded_table_passes,
            self.final_result_passes,
            self.original_raw_census,
            self.descriptor_scans,
        ]
        .into_iter()
        .try_fold(0usize, add)
    }
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Value,
    pub origin_asof_ordinal: i32,
    pub cut_ordinal: i32,
    pub cut_day: usize,
    pub full_end_ordinal: i32,
    pub completed_preparation: PreparationReceipt,
    pub revised_preparation: PreparationReceipt,
    pub cut_tables: Tables,
    pub producer_completion: Value,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub cut_component_work_reservation: usize,
    pub cut_component_bytes_reservation: usize,
    pub component_output_limit_bytes: usize,
    pub appended_candidates: usize,
    pub completed_prefix_daily_sim_executed_once: bool,
    pub resumed_future_daily_sim_executed_once: bool,
    pub complete_origin_source_owners_retained: bool,
    pub cut_started_mixed_sim_adoption: bool,
    pub original_credit_clock_restarted: bool,
    pub original_loan_anchor_refitted: bool,
    pub original_fee_anchor_refitted: bool,
    pub surviving_second_acquisition_posted: bool,
    pub prior_publication_verified: bool,
    pub opening_identity_verified: bool,
    pub general_mixed_book_adoption: bool,
    pub empirical_validation: bool,
    pub scope: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_admission: Option<SelectedAdmission>,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("native cut admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("native cut admission overflow".into())
}
/// The surviving complete source cannot be replaced by a cut-started contract.
/// Appended sources have future acquisition dates and no observed source groups.
pub fn validate_raw_prefix(r: &Request) -> Result<usize, String> {
    candidate_holdings::raw_prefix_borrowed(
        &r.completed_candidate,
        &r.revised_candidate,
        r.cut_ordinal,
        true,
    )?;
    Ok(r.revised_candidate.commercial.units.len() - r.completed_candidate.commercial.units.len())
}
#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let mut out = self
            .0
            .lock()
            .map_err(|_| io::Error::other("native cut stream poisoned"))?;
        if out
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_COMPONENT)
        {
            return Err(io::Error::other("native cut stream exceeds4MiB component"));
        }
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        checkpoint().map_err(io::Error::other)
    }
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    if r.preparation_receipt_policy.is_some() {
        return Err("selected native cut receipt policy requires owned wire admission".into());
    }
    evaluate_internal(r, None, None)
}

pub(super) fn validate_selected_authority(r: &Request) -> Result<(), String> {
    for candidate in [&r.completed_candidate, &r.revised_candidate] {
        for unit in &candidate.commercial.units {
            for position in &unit.graph.positions {
                if position.generated_policy.is_some() {
                    return Err("selected cut receipts require finite transfer owners".into());
                }
                let sources = position
                    .original_pricing
                    .iter()
                    .chain(&position.original_physical)
                    .chain(position.histories.iter().flat_map(|h| {
                        h.physical
                            .iter()
                            .chain(&h.pricing)
                            .chain(&h.pricing_down)
                            .chain(&h.pricing_up)
                    }));
                for history in sources {
                    if history
                        .source
                        .source
                        .transfer
                        .as_ref()
                        .is_none_or(|t| t.source_work_policy.is_none())
                    {
                        return Err("selected cut receipts require genuine counted finite transfer authority on every source".into());
                    }
                }
            }
        }
    }
    Ok(())
}

fn evaluate_internal(
    r: &Request,
    original: Option<&Value>,
    owned_wire: Option<(usize, usize)>,
) -> Result<ResultData, String> {
    checkpoint()?;
    if r.schema != SCHEMA || portfolio_compute_control::active().is_none() {
        return Err("explicit controlled native cut mixed journal required".into());
    }
    let raw = strict_json::serialized_size(r, MAX_RAW)?;
    let selected = r.preparation_receipt_policy.is_some();
    if selected {
        validate_selected_authority(r)?;
    }
    let a = candidates::admit_finite_preparation(&r.completed_candidate)?;
    let b = candidates::admit_finite_preparation(&r.revised_candidate)?;
    let origin = r.completed_candidate.as_of_ordinal;
    let cut = usize::try_from(
        r.cut_ordinal
            .checked_sub(origin)
            .ok_or("native cut ordinal overflow")?,
    )
    .map_err(|_| "native cut precedes origin")?;
    if cut == 0 || r.cut_ordinal >= a.full_end_ordinal || a.full_end_ordinal != b.full_end_ordinal {
        return Err("native cut first tier requires a positive cut before the same complete joint horizon; differing tails require union admission".into());
    }
    // The entire checked cut component covers JSON-prefix scans as well as
    // numeric posting work. It is additional to both original source bounds.
    // The Value/transport phase coexists with both prepared native owners.
    let extra_work = if selected {
        // Source construction is admitted now. Receipt/table/Sim work receives
        // a second, complete structural admission before either Sim starts.
        mul(
            raw.max(owned_wire.ok_or("selected cut owned wire absent")?.0),
            8,
        )?
    } else {
        add(CUT_WORK, add(mul(raw, 8)?, MAX_COMPONENT / 8)?)?
    };
    let historical_extra_bytes = add(
        CUT_BYTES,
        add(VALUE_PHASE_BYTES, add(MAX_COMPONENT, mul(raw, 4)?)?)?,
    )?;
    let work = add(add(a.work_upper, b.work_upper)?, extra_work)?;
    let source_bytes = add(a.retained_bytes_upper, b.retained_bytes_upper)?;
    let phases = if selected {
        let (wire_bytes, wire_capacity) = owned_wire.ok_or("selected cut owned wire absent")?;
        Some(
            receipts::retention(raw, wire_bytes, wire_capacity, source_bytes)?
                .with_descriptors(DESCRIPTOR_MEMORY_LIMIT)?,
        )
    } else {
        None
    };
    let bytes = match phases {
        Some(p) => p.peak(),
        None => add(source_bytes, historical_extra_bytes)?,
    };
    let extra_bytes = match phases {
        Some(p) => p
            .source_owner_phase_bytes
            .checked_sub(source_bytes)
            .ok_or("native cut phase source reservation differs")?,
        None => historical_extra_bytes,
    };
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err(format!(
            "native cut joined32M/128MiB before dual graph generation: work={work},bytes={bytes}"
        ));
    }
    if mul(raw, 40)? > VALUE_PHASE_BYTES {
        return Err("native cut raw comparison Values exceed reserved64MiB phase".into());
    }
    let appended = validate_raw_prefix(r)?;
    let output = Buffer(Arc::default());
    let mut selected_admission = None;
    let (completed, revised) = if selected {
        let (completed, revised, admission) = selected_sources(
            r,
            original,
            [
                (
                    add(b.work_upper, extra_work)?,
                    add(b.retained_bytes_upper, extra_bytes)?,
                ),
                (
                    add(a.work_upper, extra_work)?,
                    add(a.retained_bytes_upper, extra_bytes)?,
                ),
            ],
            cut,
            &output,
            work,
            raw,
            source_bytes,
            phases.ok_or("selected cut phases absent")?,
            owned_wire.ok_or("selected cut wire absent")?,
        )?;
        selected_admission = Some(admission);
        (completed, revised)
    } else {
        let mut revised = None;
        let (completed, _) = candidates::with_finite_compiled(
            &r.completed_candidate,
            (
                add(b.work_upper, extra_work)?,
                add(b.retained_bytes_upper, extra_bytes)?,
            ),
            &mut |old_spec, _, old_owner| {
                let (prepared, _) = candidates::with_finite_compiled(
                    &r.revised_candidate,
                    (
                        add(a.work_upper, extra_work)?,
                        add(a.retained_bytes_upper, extra_bytes)?,
                    ),
                    &mut |new_spec, _, new_owner| {
                        daily::run_commercial_cut(
                            old_spec,
                            new_spec,
                            old_owner,
                            new_owner,
                            cut,
                            Box::new(output.clone()),
                        )
                    },
                )?;
                strict_json::serialized_size(&prepared, MAX_COMPONENT)?;
                revised = Some(PreparationReceipt::retain(
                    prepared,
                    original.map(|v| &v["revised_candidate"]),
                    selected,
                    MAX_COMPONENT,
                )?);
                Ok(())
            },
        )?;
        strict_json::serialized_size(&completed, MAX_COMPONENT)?;
        let completed = PreparationReceipt::retain(
            completed,
            original.map(|v| &v["completed_candidate"]),
            selected,
            MAX_COMPONENT,
        )?;
        (
            completed,
            revised.ok_or("revised native cut preparation absent")?,
        )
    };
    let data = std::mem::take(&mut *output.0.lock().map_err(|_| "native cut stream poisoned")?);
    if let Some(meta) = &selected_admission {
        if data.len() > meta.output_inventory.visible_stream.encoded_bytes_upper {
            return Err(
                "selected cut emitted stream exceeds pre-Sim structural byte inventory".into(),
            );
        }
    }
    let (tables, completion) = candidates::decode_stream_bounded(&data, 8, VALUE_PHASE_BYTES)?;
    drop(data);
    if let Some(meta) = &selected_admission {
        meta.output_inventory.verify_tables(&tables)?;
    } else {
        strict_json::serialized_size(&tables, MAX_COMPONENT)?;
    }
    let final_work = selected_admission
        .as_ref()
        .map_or(work, |m| m.final_joined_work_upper);
    let final_bytes = selected_admission
        .as_ref()
        .map_or(bytes, |m| m.retention_phases.peak());
    let raw_source = if selected {
        Value::Null
    } else {
        serde_json::to_value(r).map_err(|e| e.to_string())?
    };
    let result = ResultData {
        schema: SCHEMA,
        raw_source,
        origin_asof_ordinal: origin,
        cut_ordinal: r.cut_ordinal,
        cut_day: cut,
        full_end_ordinal: a.full_end_ordinal,
        completed_preparation: completed,
        revised_preparation: revised,
        cut_tables: tables,
        producer_completion: completion,
        admitted_work_upper: work,
        retained_bytes_upper: bytes,
        cut_component_work_reservation: selected_admission
            .as_ref()
            .map_or(CUT_WORK, |m| m.work_components.independent_cut_guard),
        cut_component_bytes_reservation: CUT_BYTES,
        component_output_limit_bytes: MAX_COMPONENT,
        appended_candidates: appended,
        completed_prefix_daily_sim_executed_once: true,
        resumed_future_daily_sim_executed_once: true,
        complete_origin_source_owners_retained: true,
        cut_started_mixed_sim_adoption: true,
        original_credit_clock_restarted: false,
        original_loan_anchor_refitted: false,
        original_fee_anchor_refitted: false,
        surviving_second_acquisition_posted: false,
        prior_publication_verified: false,
        opening_identity_verified: false,
        general_mixed_book_adoption: false,
        empirical_validation: false,
        scope: SCOPE,
        selected_admission,
    };
    let mut result = result;
    result.admitted_work_upper = final_work;
    result.retained_bytes_upper = final_bytes;
    if !selected {
        strict_json::serialized_size(&result, MAX_OUTPUT)?;
        strict_json::serialized_size(&result, VALUE_ENCODED_UPPER)?;
    }
    checkpoint()?;
    Ok(result)
}

fn encoded_shape(bytes: usize) -> daily::CutJsonShape {
    // Embedded RawValue is parsed into its COMPLETE child cells in the final
    // phase. Every cell and every encoded string byte is bounded by its text.
    daily::CutJsonShape {
        encoded_bytes_upper: bytes,
        cells_upper: bytes,
        string_bytes_upper: bytes,
    }
}
fn result_shape(
    raw: daily::CutJsonShape,
    meta: &SelectedAdmission,
) -> Result<daily::CutJsonShape, String> {
    let n = daily::CutJsonShape::number();
    let flag = daily::CutJsonShape::boolean();
    let mut fields = vec![
        ("schema", daily::cut_json_shape(SCHEMA)?),
        ("raw_source", raw),
        (
            "completed_preparation",
            encoded_shape(meta.completed_receipt_bytes),
        ),
        (
            "revised_preparation",
            encoded_shape(meta.revised_receipt_bytes),
        ),
        ("cut_tables", meta.output_inventory.decoded_tables),
        (
            "producer_completion",
            daily::cut_json_shape(
                &serde_json::json!({"complete":true,"protocol":8,"cut_day":0,"native_cut_state_moved":true,"original_calendar_retained":true,"new_sim_started_at_cut":true,"prior_publication_verified":false}),
            )?,
        ),
        ("scope", daily::cut_json_shape(SCOPE)?),
        ("selected_admission", daily::cut_json_shape(meta)?),
    ];
    fields.extend("origin_asof_ordinal cut_ordinal cut_day full_end_ordinal admitted_work_upper retained_bytes_upper cut_component_work_reservation cut_component_bytes_reservation component_output_limit_bytes appended_candidates".split_whitespace().map(|k|(k,n)));
    fields.extend("completed_prefix_daily_sim_executed_once resumed_future_daily_sim_executed_once complete_origin_source_owners_retained cut_started_mixed_sim_adoption original_credit_clock_restarted original_loan_anchor_refitted original_fee_anchor_refitted surviving_second_acquisition_posted prior_publication_verified opening_identity_verified general_mixed_book_adoption empirical_validation".split_whitespace().map(|k|(k,flag)));
    daily::cut_json_object(&fields)
}

#[allow(clippy::too_many_arguments)]
fn selected_sources(
    r: &Request,
    original: Option<&Value>,
    reservations: [(usize, usize); 2],
    cut: usize,
    output: &Buffer,
    initial_work: usize,
    raw_bytes: usize,
    source_bytes: usize,
    retention: receipts::Retention,
    wire: (usize, usize),
) -> Result<(PreparationReceipt, PreparationReceipt, SelectedAdmission), String> {
    let original = original.ok_or("selected cut exact original raw absent")?;
    let mut first = None;
    let mut second = None;
    let mut admission = None;
    let (completed, _) = candidates::with_finite_compiled_receipt(
        &r.completed_candidate,
        reservations[0],
        &mut |old_spec, _, old_owner, view| {
            let limit = MAX_COMPONENT.min(
                MAX_WORK
                    .checked_sub(initial_work)
                    .ok_or("selected cut source work exceeds joined cap")?
                    / 2,
            );
            let (receipt, na) = PreparationReceipt::encode_view(
                view,
                Some(&original["completed_candidate"]),
                limit,
            )?;
            first = Some(receipt);
            let already_executed = mul(na, 2)?;
            let mut revised_reservation = reservations[1];
            revised_reservation.0 = add(revised_reservation.0, already_executed)?;
            let (revised, _) = candidates::with_finite_compiled_receipt(
                &r.revised_candidate,
                revised_reservation,
                &mut |new_spec, _, new_owner, view| {
                    let second_work = add(initial_work, already_executed)?;
                    let limit = MAX_COMPONENT.min(
                        MAX_WORK
                            .checked_sub(second_work)
                            .ok_or("selected cut receipt work exceeds joined cap")?
                            / 2,
                    );
                    let (receipt, nb) = PreparationReceipt::encode_view(
                        view,
                        Some(&original["revised_candidate"]),
                        limit,
                    )?;
                    second = Some(receipt);
                    // Reserve each sizing/structural pass from the remaining
                    // root ticket before it traverses cells/bytes. No Sim exists.
                    let remaining = MAX_WORK
                        .checked_sub(add(second_work, mul(nb, 2)?)?)
                        .ok_or("selected cut receipts exhaust root work before census")?;
                    let meta = daily::with_cut_census_budget(remaining, || {
                        let spec_started = daily::cut_census_work_spent().unwrap_or(0);
                        let (_, old_spec_bytes) = daily::measure_cut_json_shape(old_spec)?;
                        let (_, new_spec_bytes) = daily::measure_cut_json_shape(new_spec)?;
                        let spec_census =
                            daily::cut_census_work_spent().unwrap_or(0) - spec_started;
                        daily::charge_cut_census_work(mul(
                            add(old_spec_bytes, new_spec_bytes)?,
                            8,
                        )?)?;
                        let ledger =
                            daily::commercial_cut_census(old_spec, new_spec, old_owner, new_owner)?;
                        let inventory = daily::commercial_cut_inventory(
                            old_spec, new_spec, old_owner, new_owner, cut,
                        )?;
                        if inventory.visible_stream.encoded_bytes_upper > MAX_COMPONENT
                            || inventory.decoded_tables.encoded_bytes_upper > MAX_COMPONENT
                        {
                            return Err(format!("selected cut structural output exceeds4MiB before Sim: stream={},tables={},source_work={initial_work},receipt_scan_work={}",inventory.visible_stream.encoded_bytes_upper,inventory.decoded_tables.encoded_bytes_upper,mul(add(na,nb)?,2)?));
                        }
                        let (raw_shape, _) = daily::measure_cut_json_shape(original)?;
                        let descriptor_started = daily::cut_census_work_spent().unwrap_or(0);
                        let mut meta = SelectedAdmission {
                            schema: "commercial-cut-encoded-admission-1",
                            initial_source_work_upper: initial_work,
                            final_joined_work_upper: 0,
                            owned_wire_bytes: wire.0,
                            owned_wire_capacity: wire.1,
                            typed_raw_bytes: raw_bytes,
                            source_owner_retained_bytes_upper: source_bytes,
                            retention_phases: retention
                                .with_sim_batch(inventory.stream_batch_retained_bytes_upper)?,
                            completed_preparation_work_upper: initial_work,
                            revised_preparation_work_upper: second_work,
                            completed_receipt_bytes: na,
                            revised_receipt_bytes: nb,
                            descriptor_retained_bytes_upper: 0,
                            cut_ledger_census: ledger,
                            work_components: SelectedWork::default(),
                            final_result_shape: daily::CutJsonShape::default(),
                            output_inventory: inventory,
                        };
                        // Counters are numeric tokens with fixed32-byte upper
                        // widths: updating their values cannot change this shape.
                        let descriptors = daily::cut_json_shape(&meta)?;
                        if descriptors.encoded_bytes_upper > DESCRIPTOR_JSON_LIMIT {
                            return Err(
                            "selected cut descriptors exceed reserved64KiB component before Sim"
                                .into(),
                        );
                        }
                        meta.descriptor_retained_bytes_upper = DESCRIPTOR_MEMORY_LIMIT;
                        meta.final_result_shape = result_shape(raw_shape, &meta)?;
                        let w = &mut meta.work_components;
                        w.raw_prefix = mul(raw_bytes.max(wire.0), 8)?;
                        w.source_preparation = initial_work
                            .checked_sub(w.raw_prefix)
                            .ok_or("selected cut source census differs")?;
                        w.preparation_receipt_scans = mul(add(na, nb)?, 2)?;
                        w.structural_census = meta.output_inventory.census_work_upper;
                        w.independent_cut_guard = ledger.work_upper;
                        w.repeated_specification_scans = mul(ledger.specification_bytes, 8)?;
                        w.prepared_specification_census = spec_census;
                        w.all_native_stream_passes =
                            mul(meta.output_inventory.all_stream.scan_work()?, 4)?;
                        w.visible_stream_passes =
                            mul(meta.output_inventory.visible_stream.scan_work()?, 2)?;
                        w.decoded_table_passes =
                            mul(meta.output_inventory.decoded_tables.scan_work()?, 2)?;
                        w.final_result_passes = mul(meta.final_result_shape.scan_work()?, 3)?;
                        w.original_raw_census = raw_shape.scan_work()?;
                        w.descriptor_scans = daily::cut_census_work_spent()
                            .unwrap_or(0)
                            .checked_sub(descriptor_started)
                            .ok_or("cut descriptor ticket state differs")?;
                        meta.final_joined_work_upper = w.total()?;
                        let peak = meta.retention_phases.peak();
                        if meta.final_result_shape.encoded_bytes_upper > VALUE_ENCODED_UPPER
                            || meta.final_joined_work_upper > MAX_WORK
                            || peak > MAX_BYTES
                            || meta.cut_ledger_census.work_upper > CUT_WORK
                            || meta.cut_ledger_census.retained_bytes_upper > CUT_BYTES
                        {
                            daily::charge_cut_census_work(mul(descriptors.scan_work()?, 3)?)?;
                            return Err(format!(
                                "selected cut pre-Sim structural admission refused: {}",
                                serde_json::to_string(&meta).map_err(|e| e.to_string())?
                            ));
                        }
                        Ok(meta)
                    })?;
                    let all_frame_bytes_upper =
                        meta.output_inventory.all_stream.encoded_bytes_upper;
                    admission = Some(meta);
                    daily::run_commercial_cut_with_inventory(
                        old_spec,
                        new_spec,
                        old_owner,
                        new_owner,
                        cut,
                        Box::new(output.clone()),
                        all_frame_bytes_upper,
                    )
                },
            )?;
            // The encoded receipt retains every native financial field. These
            // distinct native graph owners die before any output table parse.
            drop(revised);
            Ok(())
        },
    )?;
    drop(completed);
    Ok((
        first.ok_or("completed encoded native receipt absent")?,
        second.ok_or("revised encoded native receipt absent")?,
        admission.ok_or("selected cut pre-Sim admission absent")?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    op: String,
    schema: String,
    execution_timeout_ms: u64,
    request: Request,
}
fn parse_envelope(
    bytes: &[u8],
    started: Instant,
) -> Result<(Envelope, Value, portfolio_compute_control::Control), String> {
    // This covers only the first strict tree and parser workspace. Original
    // request cloning, typed DTOs and complete source/Sim phases are separate.
    let (raw, _) = portfolio_model_core::transport::value_capacity::parse_checked(
        bytes,
        MAX_RAW,
        VALUE_PHASE_BYTES,
        checkpoint,
    )?;
    let original = raw["request"].clone();
    let envelope: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_commercial_cut_mixed_journal" || envelope.schema != SCHEMA {
        return Err("explicit native cut mixed journal operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(
            envelope.execution_timeout_ms,
            started,
        )?,
    };
    Ok((envelope, original, control))
}

pub fn run_envelope_started(bytes: &[u8], started: Instant) -> Result<Value, String> {
    let (envelope, original, control) = parse_envelope(bytes, started)?;
    if envelope.request.preparation_receipt_policy.is_some() {
        return Err("selected native cut receipts require the owned wire entrypoint".into());
    }
    execute_envelope(envelope, original, control, None)
}

/// The dispatcher drops its peek Value and transfers its input Vec here. Only
/// this owned entrypoint can promise those outer owners are gone before Sim.
pub fn run_owned_envelope_started(bytes: Vec<u8>, started: Instant) -> Result<Value, String> {
    let owned_wire = (bytes.len(), bytes.capacity());
    let (envelope, original, control) = parse_envelope(&bytes, started)?;
    if envelope.request.preparation_receipt_policy.is_none() {
        return Err("owned cut wire requires the explicit native receipt policy".into());
    }
    drop(bytes);
    execute_envelope(envelope, original, control, Some(owned_wire))
}

/// Preserve the original control through borrowed stdout serialization too.
/// The library Value entrypoint remains useful for explicitly contained tests.
pub fn run_owned_cli_started(
    bytes: Vec<u8>,
    started: Instant,
    output: &mut dyn Write,
) -> Result<(), String> {
    run_cli_internal(bytes, started, output, true)
}

/// Early original-wire CLI dispatch. The borrowed selector allocates no peek
/// Value; strict parsing below owns duplicate/shape validation. Both historical
/// receipt policies retain their existing financial admission and output schema.
pub fn run_cli_started(
    bytes: Vec<u8>,
    started: Instant,
    output: &mut dyn Write,
) -> Result<(), String> {
    run_cli_internal(bytes, started, output, false)
}

fn run_cli_internal(
    bytes: Vec<u8>,
    started: Instant,
    output: &mut dyn Write,
    require_selected: bool,
) -> Result<(), String> {
    // Classify only after bounded strict parsing. The fixed route consumes this
    // tree directly into typed input and drops the wire before source creation.
    // It never owns an original Value clone or a final expanded Value result.
    if !require_selected {
        let (raw, _) = portfolio_model_core::transport::value_capacity::parse_checked(
            &bytes,
            MAX_RAW,
            VALUE_PHASE_BYTES,
            checkpoint,
        )?;
        if super::cut_fixed_intake::supports(&raw) {
            let input = super::cut_fixed_intake::derive(&bytes, bytes.capacity(), &raw)?;
            let envelope: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
            if envelope.op != "analyze_commercial_cut_mixed_journal" || envelope.schema != SCHEMA {
                return Err("explicit fixed native cut operation required".into());
            }
            let control = match portfolio_compute_control::active() {
                Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
                None => portfolio_compute_control::Control::from_started(
                    envelope.execution_timeout_ms,
                    started,
                )?,
            };
            drop(bytes);
            return portfolio_compute_control::scope(Some(control), || {
                struct Success<'a> {
                    output: &'a mut dyn Write,
                    started: bool,
                }
                impl Write for Success<'_> {
                    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                        checkpoint().map_err(io::Error::other)?;
                        if !self.started {
                            self.output.write_all(b"{\"ok\":")?;
                            self.started = true;
                        }
                        self.output.write(b)
                    }
                    fn flush(&mut self) -> io::Result<()> {
                        checkpoint().map_err(io::Error::other)?;
                        self.output.flush()
                    }
                }
                let mut success = Success {
                    output,
                    started: false,
                };
                super::cut_fixed_packet::write_packet(
                    &envelope.request,
                    input.typed_bytes_upper,
                    &mut success,
                    None,
                )?;
                checkpoint()?;
                success
                    .output
                    .write_all(b"}\n")
                    .map_err(|e| e.to_string())?;
                success.output.flush().map_err(|e| e.to_string())
            });
        }
        drop(raw);
    }
    let owned_wire = (bytes.len(), bytes.capacity());
    let (envelope, original, control) = parse_envelope(&bytes, started)?;
    let selected = envelope.request.preparation_receipt_policy.is_some();
    if require_selected && !selected {
        return Err("owned cut wire requires the explicit native receipt policy".into());
    }
    drop(bytes);
    let stdout_control = control.clone();
    let value = execute_envelope(envelope, original, control, selected.then_some(owned_wire))?;
    portfolio_compute_control::scope(Some(stdout_control), || {
        #[derive(Serialize)]
        struct Success<'a> {
            ok: &'a Value,
        }
        struct Checked<'a>(&'a mut dyn Write);
        impl Write for Checked<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                checkpoint().map_err(io::Error::other)?;
                self.0.write(bytes)
            }
            fn flush(&mut self) -> io::Result<()> {
                checkpoint().map_err(io::Error::other)?;
                self.0.flush()
            }
        }
        let mut output = Checked(output);
        serde_json::to_writer(&mut output, &Success { ok: &value }).map_err(|e| e.to_string())?;
        output.write_all(b"\n").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())
    })
}

fn execute_envelope(
    envelope: Envelope,
    original: Value,
    control: portfolio_compute_control::Control,
    owned_wire: Option<(usize, usize)>,
) -> Result<Value, String> {
    portfolio_compute_control::scope(Some(control), move || {
        let selected = owned_wire.is_some();
        let mut result = if selected {
            evaluate_internal(&envelope.request, Some(&original), owned_wire)?
        } else {
            evaluate(&envelope.request)?
        };
        if !selected {
            result
                .completed_preparation
                .replace_original(original["completed_candidate"].clone())?;
            result
                .revised_preparation
                .replace_original(original["revised_candidate"].clone())?;
        }
        result.raw_source = original;
        if !selected {
            strict_json::serialized_size(&result.completed_preparation, MAX_COMPONENT)?;
            strict_json::serialized_size(&result.revised_preparation, MAX_COMPONENT)?;
            strict_json::serialized_size(&result, MAX_OUTPUT)?;
            strict_json::serialized_size(&result, VALUE_ENCODED_UPPER)?;
        }
        checkpoint()?;
        if !selected {
            return serde_json::to_value(result).map_err(|e| e.to_string());
        }
        let limit = result
            .selected_admission
            .as_ref()
            .ok_or("selected cut final inventory absent")?
            .final_result_shape
            .encoded_bytes_upper
            .min(VALUE_ENCODED_UPPER);
        let encoded = strict_json::encode(&result, limit)?;
        drop(result);
        drop(envelope);
        // Input Vec, dispatcher peek, typed requests, graphs, tables, original
        // Value and RawValue receipts are all gone. This is a disjoint phase.
        strict_json::parse_checked(&encoded, VALUE_ENCODED_UPPER, checkpoint)
    })
}
