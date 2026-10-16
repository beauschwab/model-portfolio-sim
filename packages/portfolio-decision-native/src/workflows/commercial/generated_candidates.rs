//! Bounded record emission for the explicit generated candidate tier. The
//! journal is streamed directly from the existing Sim, never decoded/reencoded
//! into one enlarged buffered result.
use super::{prepare_and_run, Request, GENERATED_FEE_SCHEMA, GENERATED_SCHEMA};
use portfolio_model_core::strict_json;
use portfolio_risk_native::commercial_units;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};

fn emit(output: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    struct Record<'a, W> {
        output: &'a mut W,
        bytes: usize,
    }
    impl<W: Write> Write for Record<'_, W> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
            self.bytes = self
                .bytes
                .checked_add(b.len())
                .ok_or_else(|| io::Error::other("generated record overflow"))?;
            if self.bytes > (32 << 20) - 1 {
                return Err(io::Error::other("generated record exceeds32MiB"));
            }
            self.output.write_all(b)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.output.flush()
        }
    }
    // Record cap is enforced incrementally; no complete graph/proof JSON buffer
    // coexists with the native immutable owner it describes.
    serde_json::to_writer(&mut Record { output, bytes: 0 }, value).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()
}
#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    r: &Request,
    original: Option<&Value>,
    prepared: commercial_units::Prepared,
    allocation: Value,
    s: portfolio_ledger_native::types::Spec,
    work: usize,
    bytes: usize,
    mut output: Box<dyn Write + Send>,
) -> Result<(Option<super::Preparation>, i32, usize), String> {
    use portfolio_ledger_native::{
        commercial_intent_owner as ledger, daily,
        owners::commercial::{compact::CompactSource, compact_fee::CompactFeeSource},
    };
    use portfolio_risk_native::{advanced_graph::Role, commercial_graph::CreditIdentity};
    let full_end = r
        .as_of_ordinal
        .checked_add(i32::try_from(s.horizon_days).map_err(|_| "generated calendar overflow")?)
        .ok_or("generated calendar overflow")?;
    let paths = prepared
        .source()
        .native_graphs
        .iter()
        .map(|g| {
            g.native_sources
                .iter()
                .filter(|s| s.role == Role::Physical)
                .count()
        })
        .sum();
    let raw = original
        .cloned()
        .unwrap_or(serde_json::to_value(r).map_err(|e| e.to_string())?);
    let resolved = serde_json::to_value(&s).map_err(|e| e.to_string())?;
    preparation(
        &mut output,
        &raw,
        &prepared,
        &allocation,
        &resolved,
        work,
        bytes,
        full_end,
        paths,
    )?;
    drop(raw);
    drop(resolved);
    drop(allocation);
    // Consuming ownership makes phase disjointness concrete. All Pricing cash,
    // original public receipts, coefficients and funding buffers die BEFORE
    // the first generated Physical accounting owner is expanded.
    let graphs = prepared.into_source().native_graphs;
    let physical = graphs
        .into_iter()
        .map(|g| {
            g.native_sources
                .into_iter()
                .filter(|s| s.role == Role::Physical)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut compact = Vec::new();
    let mut compact_fees = Vec::new();
    let mut retained = 0usize;
    for native in physical.iter().flatten() {
        portfolio_compute_control::checkpoint()?;
        let full = native
            .materialize_generated_physical()?
            .ok_or("genuine generated Physical recipe absent")?;
        let journal = CompactSource::from_native_bounded(
            full.accounting(),
            native.path_key,
            (32 << 20) - retained,
        )?;
        retained = retained
            .checked_add(journal.retained_bytes())
            .ok_or("compact retained overflow")?;
        if retained > 32 << 20 {
            return Err("generated compact journal exceeds32MiB".into());
        }
        let fees = native.materialize_generated_fees(&full)?;
        let fee_journal = if let Some(fees) = fees {
            // The retained native clock lives beside the compact posting owner.
            // Reserve its complete allocation before copying either component.
            let timeline_bytes = fee_timeline_bytes(fees.bound_path.native_years().len())?;
            let remaining = (32usize << 20)
                .checked_sub(retained)
                .and_then(|v| v.checked_sub(timeline_bytes))
                .ok_or("compact fee timeline exceeds remaining32MiB")?;
            let journal =
                CompactFeeSource::from_native_bounded(fees.bound_path.source(), remaining)?;
            retained = retained
                .checked_add(journal.retained_bytes())
                .and_then(|v| v.checked_add(timeline_bytes))
                .ok_or("compact fee retained overflow")?;
            Some((journal, fees.bound_path.native_years().to_vec()))
        } else {
            None
        };
        compact.push(journal);
        compact_fees.push(fee_journal);
        // `full` drops here; there is never a vector of expanded P owners.
    }
    let mut bindings = Vec::new();
    let mut index = 0;
    for (ui, sources) in physical.iter().enumerate() {
        let u = &r.commercial.units[ui];
        let o = &r.owners[ui];
        for native in sources {
            let map = r
                .run_mapping
                .iter()
                .find(|m| m.source_scenario_id == native.scenario_id)
                .ok_or("generated run mapping absent")?;
            bindings.push(ledger::Binding {
                source_class: if u.graph.positions[0].credit_identity
                    == CreditIdentity::IssuerCreditDebt
                {
                    ledger::SourceClass::IssuerAcSecurity
                } else {
                    ledger::SourceClass::CommercialLoan
                },
                scenario: &map.ledger_scenario_id,
                position_id: &o.position_id,
                path_key: native.path_key,
                origin_day: usize::try_from(u.graph.as_of_ordinal - r.as_of_ordinal)
                    .map_err(|_| "generated purchase precedes opening")?,
                source_weight: native.weight,
                allocated_face: r.allocation[ui].face,
                native_unit_face: u.unit_face,
                accounting: &compact[index],
            });
            index += 1;
        }
    }
    let mut compiled = ledger::Compiled::new(&s, bindings)?;
    if r.schema == GENERATED_FEE_SCHEMA {
        let native_sources = physical.iter().flatten().collect::<Vec<_>>();
        let fees = compact_fees
            .iter()
            .enumerate()
            .map(|(index, source)| {
                let (source, years) = source
                    .as_ref()
                    .ok_or("generated paired compact fee owner absent")?;
                Ok(ledger::FeeBinding {
                    loan_binding: index,
                    native_position_id: &native_sources[index].position_id,
                    source,
                    native_years: years,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        compiled.attach_fees(&s, fees)?;
    }
    daily::run_with_commercial(&s, None, &compiled, output)?;
    portfolio_compute_control::checkpoint()?;
    Ok((None, full_end, paths))
}
#[derive(Serialize)]
struct Row<'a, T: Serialize> {
    unit_index: usize,
    source_index: usize,
    native_json: &'a T,
}
#[derive(Serialize)]
struct Table<'a, T: Serialize> {
    table: &'static str,
    rows: [Row<'a, T>; 1],
}
fn table(
    output: &mut impl Write,
    name: &'static str,
    unit: usize,
    index: usize,
    value: &impl Serialize,
) -> Result<(), String> {
    emit(
        output,
        &Table {
            table: name,
            rows: [Row {
                unit_index: unit,
                source_index: index,
                native_json: value,
            }],
        },
    )
}
#[derive(Serialize)]
struct Coefficients<'a> {
    schema: &'static str,
    scenario_ids: &'a [String],
    units: &'a [portfolio_risk_native::product_strategy::UnitIdentity],
    shape: [usize; 4],
    metrics: &'a [&'static str],
    values: &'a [f64],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fee_income_values: &'a Vec<f64>,
    funding: &'a commercial_units::Funding,
    admitted_work_upper: usize,
    scope: &'static str,
}
#[derive(Serialize)]
struct Graph<'a> {
    schema: &'static str,
    original_oas: &'a [portfolio_risk_native::commercial_graph::Oas],
    metrics: &'a [portfolio_risk_native::commercial_graph::Metric],
    admitted_work_upper: usize,
    shared_draws_retained: bool,
    physical_accounting_source_retained: bool,
    generated_physical_recipes_retained: Option<bool>,
    generated_oas_calibration:
        &'a Option<portfolio_risk_native::commercial_stochastic_exercise::calibration::Receipt>,
    empirical_validation: bool,
    scope: &'static str,
    exercise_policies: &'a [portfolio_risk_native::commercial_graph::ExerciseReceipt],
}
#[allow(clippy::too_many_arguments)]
pub(super) fn preparation(
    output: &mut impl Write,
    raw: &Value,
    prepared: &commercial_units::Prepared,
    allocation: &Value,
    specification: &Value,
    work: usize,
    bytes: usize,
    full_end: i32,
    physical_paths: usize,
) -> Result<(), String> {
    let mut header = json!({"workflow":{
        "schema":if raw["schema"]==GENERATED_FEE_SCHEMA {"commercial-generated-candidate-fee-preparation-1"}else{"commercial-generated-candidate-preparation-1"},"model_schema":raw["schema"],
        "prepared_request":specification,
        "source_receipt":{
            "original_request":raw,"allocation_replay":allocation,
            "admitted_work_upper":work,"retained_bytes_upper":bytes,
            "reporting_end_ordinal":raw["reporting_end_ordinal"],"full_end_ordinal":full_end,
            "physical_paths":physical_paths,"maximum_live_full_physical_owners":1,
            "shared_daily_sim_adoption":true,"graph_exercise_source_adoption":true,
            "opening_identity_verified":false,"mixed_ledger_adoption":false,"empirical_validation":false
        }
    }});
    if raw["schema"] == GENERATED_FEE_SCHEMA {
        header["workflow"]["source_receipt"]["maximum_live_full_fee_owners"] = json!(1);
    }
    emit(output, &header)?;
    let s = prepared.source();
    table(
        output,
        "commercial_candidate_units",
        0,
        0,
        &Coefficients {
            schema: s.schema,
            scenario_ids: &s.scenario_ids,
            units: &s.units,
            shape: s.shape,
            metrics: &s.metrics,
            values: &s.values,
            fee_income_values: &s.fee_income_values,
            funding: &s.funding,
            admitted_work_upper: s.admitted_work_upper,
            scope: s.scope,
        },
    )?;
    for (ui, g) in s.native_graphs.iter().enumerate() {
        table(
            output,
            "commercial_candidate_graphs",
            ui,
            0,
            &Graph {
                schema: g.schema,
                original_oas: &g.original_oas,
                metrics: &g.metrics,
                admitted_work_upper: g.admitted_work_upper,
                shared_draws_retained: g.shared_draws_retained,
                physical_accounting_source_retained: g.physical_accounting_source_retained,
                generated_physical_recipes_retained: g.generated_physical_recipes_retained,
                generated_oas_calibration: &g.generated_oas_calibration,
                empirical_validation: g.empirical_validation,
                scope: g.scope,
                exercise_policies: &g.exercise_policies,
            },
        )?;
        for (index, source) in g.native_sources.iter().enumerate() {
            table(
                output,
                "commercial_candidate_native_sources",
                ui,
                index,
                source,
            )?;
        }
        for (index, source) in g.original_physical_sources.iter().enumerate() {
            table(
                output,
                "commercial_candidate_original_sources",
                ui,
                index,
                source,
            )?;
        }
        for (index, proof) in g.original_acquisition_proofs.iter().enumerate() {
            table(
                output,
                "commercial_candidate_original_acquisition_proofs",
                ui,
                index,
                proof,
            )?;
        }
    }
    Ok(())
}
#[derive(Clone)]
struct Shared(Arc<Mutex<Box<dyn Write + Send>>>);
impl Write for Shared {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        self.0
            .lock()
            .map_err(|_| io::Error::other("generated journal output poisoned"))?
            .write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("generated journal output poisoned"))?
            .flush()
    }
}
pub fn run_generated_stream_started(
    input: &[u8],
    output: Box<dyn Write + Send>,
    started: Instant,
) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Request,
    }
    let raw = strict_json::parse_checked(input, 1 << 20, portfolio_compute_control::checkpoint)?;
    let original = raw["request"].clone();
    let e: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    let fees = e.schema == GENERATED_FEE_SCHEMA;
    if e.schema != e.request.schema
        || !(if fees {
            e.op == "analyze_commercial_generated_candidate_fee_journal"
        } else {
            e.op == "analyze_commercial_generated_candidate_journal" && e.schema == GENERATED_SCHEMA
        })
    {
        return Err("explicit generated commercial candidate operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(e.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(e.execution_timeout_ms, started)?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut output = Shared(Arc::new(Mutex::new(output)));
        let (_, full_end, paths, _) = prepare_and_run(
            &e.request,
            true,
            Some(&original),
            Box::new(output.clone()),
            (0, 0),
            super::PreparationMode::Execute,
            None,
        )?;
        let mut trailer = json!({"record":if fees {"commercial_generated_fee_complete"}else{"commercial_generated_complete"},"complete":true,"schema":e.schema,
            "protocol":if fees {7}else{6},"physical_paths":paths,"maximum_live_full_physical_owners":1,"full_end_ordinal":full_end,
            "independent_persisted_verification":false,"opening_identity_verified":false,
            "mixed_ledger_adoption":false,"empirical_validation":false});
        if fees {
            trailer["maximum_live_full_fee_owners"] = json!(1);
        }
        emit(&mut output, &trailer)?;
        output.flush().map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()
    })
}

fn fee_timeline_bytes(groups: usize) -> Result<usize, String> {
    groups
        .checked_mul(std::mem::size_of::<f64>())
        .and_then(|v| v.checked_add(std::mem::size_of::<Vec<f64>>()))
        .ok_or_else(|| "compact fee timeline capacity overflow".into())
}

#[cfg(test)]
mod fee_timeline_tests {
    #[test]
    fn clock_vector_and_header_are_reserved_before_copy_and_overflow_refuses() {
        let header = std::mem::size_of::<Vec<f64>>();
        assert_eq!(super::fee_timeline_bytes(0).unwrap(), header);
        assert_eq!(super::fee_timeline_bytes(256).unwrap(), header + 2048);
        assert!(super::fee_timeline_bytes(usize::MAX).is_err());
    }
}
