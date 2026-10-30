//! Actual-purchase commercial allocations reuse sealed graph financial owners
//! in the existing daily Sim. No prepared money, stop or journal input is admitted.
use crate::workflows::candidates::{Identity, Owner, RunMapping};
use portfolio_ledger_native::{commercial_intent_owner as ledger, daily, spec};
use portfolio_model_core::{cashflow::validate_ordinal, strict_json};
use portfolio_risk_native::{
    advanced_graph::Role, commercial_credit_compiler as source, commercial_units as units,
    product_strategy::Allocation,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};
pub const SCHEMA: &str = "commercial-candidate-journal-1";
pub const GENERATED_SCHEMA: &str = "commercial-generated-candidate-journal-1";
pub const GENERATED_FEE_SCHEMA: &str = "commercial-generated-candidate-fee-journal-1";
#[path = "generated_candidates.rs"]
mod generated_stream;
pub use generated_stream::run_generated_stream_started;
const MAX_BYTES: usize = 32 << 20;
const MAX_WORK: usize = 32_000_000;
/// Retain the exact submitted fields; serialize the typed source only when the
/// caller has no original receipt. Both candidate preparation routes use this.
fn raw_request_receipt<T: Serialize>(original: Option<&Value>, typed: &T) -> Result<Value, String> {
    match original {
        Some(original) => Ok(original.clone()),
        None => serde_json::to_value(typed).map_err(|error| error.to_string()),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub commercial: units::Request,
    pub allocation: Vec<Allocation>,
    pub as_of_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub owners: Vec<Owner>,
    pub run_mapping: Vec<RunMapping>,
    pub specification: Value,
    pub verified_opening_identity: Identity,
}
#[derive(Serialize)]
pub struct Preparation {
    pub raw_request: Value,
    pub native_preparation: units::Prepared,
    pub allocation_replay: Value,
    pub resolved_specification: Value,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
}
/// Complete native receipt borrowed during the callback, before Sim starts.
/// This mirrors Preparation's serialized fields without cloning its graphs.
#[derive(Clone, Copy, Serialize)]
pub(super) struct PreparationView<'a> {
    pub raw_request: &'a Value,
    pub native_preparation: &'a units::Prepared,
    pub allocation_replay: &'a Value,
    pub resolved_specification: &'a Value,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub preparation: Preparation,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub producer_completion: Value,
    pub reporting_end_ordinal: i32,
    pub full_end_ordinal: i32,
    pub shared_daily_sim_adoption: bool,
    pub graph_exercise_source_adoption: bool,
    pub independent_persisted_verification: bool,
    pub opening_identity_verified: bool,
    pub mixed_ledger_adoption: bool,
    pub empirical_validation: bool,
    pub scope: &'static str,
}
#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>, usize);
impl Default for Buffer {
    fn default() -> Self {
        Self(Arc::default(), MAX_BYTES)
    }
}
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let mut result = self
            .0
            .lock()
            .map_err(|_| io::Error::other("candidate buffer poisoned"))?;
        if result
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.1)
        {
            return Err(io::Error::other("commercial candidate stream exceeds32MiB"));
        }
        result.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("commercial candidate upper overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("commercial candidate upper overflow".into())
}
fn days(year: f64) -> Result<usize, String> {
    let elapsed = year * 365.;
    let rounded = elapsed.round();
    let mut result = if (rounded / 365.).to_bits() == year.to_bits() {
        rounded
    } else {
        elapsed.ceil()
    };
    if year > result / 365. {
        result += 1.;
    }
    if !result.is_finite() || !(0. ..=1080.).contains(&result) {
        return Err("commercial candidate complete tail exceeds1080days".into());
    }
    Ok(result as usize)
}
#[path = "candidate_preparation_capacity.rs"]
mod preparation_capacity;
#[derive(Clone, Copy)]
enum PreparationMode {
    Execute,
    AdmitOrdinary,
    AdmitBorrowed,
}

type FiniteConsumer<'consumer> = dyn for<'source> FnMut(
        &'source portfolio_ledger_native::types::Spec,
        &'source units::Prepared,
        &ledger::Compiled<'source>,
        &PreparationView<'source>,
    ) -> Result<(), String>
    + 'consumer;

fn prepare_and_run(
    r: &Request,
    generated: bool,
    original: Option<&Value>,
    output: Box<dyn Write + Send>,
    bridge_reservation: (usize, usize),
    mode: PreparationMode,
    consumer: Option<&mut FiniteConsumer<'_>>,
) -> Result<(Option<Preparation>, i32, usize, Admission), String> {
    prepare_and_run_observed(
        r,
        generated,
        original,
        output,
        bridge_reservation,
        mode,
        consumer,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_and_run_observed(
    r: &Request,
    generated: bool,
    original: Option<&Value>,
    mut output: Box<dyn Write + Send>,
    bridge_reservation: (usize, usize),
    mode: PreparationMode,
    consumer: Option<&mut FiniteConsumer<'_>>,
    observer: Option<&mut units::SourceObserver<'_>>,
) -> Result<(Option<Preparation>, i32, usize, Admission), String> {
    portfolio_compute_control::checkpoint()?;
    if observer.is_some()
        && (generated || consumer.is_none() || !matches!(mode, PreparationMode::Execute))
    {
        return Err(
            "source capture requires an explicit finite borrowed preparation consumer".into(),
        );
    }
    if generated && consumer.is_some() {
        return Err(
            "finite native preparation callback cannot borrow compact generated owners".into(),
        );
    }
    validate_ordinal(r.as_of_ordinal)?;
    validate_ordinal(r.reporting_end_ordinal)?;
    let generated_fees = generated && r.schema == GENERATED_FEE_SCHEMA;
    if !(if generated {
        r.schema == GENERATED_SCHEMA || generated_fees
    } else {
        r.schema == SCHEMA
    }) || r.commercial.reporting_ordinals.first() != Some(&r.as_of_ordinal)
        || r.commercial.reporting_ordinals.last() != Some(&r.reporting_end_ordinal)
    {
        return Err("commercial candidate actual reporting identity differs".into());
    }
    for v in [
        &r.verified_opening_identity.id,
        &r.verified_opening_identity.revision,
    ] {
        if v.is_empty() || v.len() > 200 {
            return Err("explicit bounded opening receipt identity required".into());
        }
    }
    let raw_bytes = strict_json::serialized_size(r, if generated { 1 << 20 } else { MAX_BYTES })?;
    for unit in &r.commercial.units {
        for position in &unit.graph.positions {
            if position.fees.is_some() && generated {
                return Err("commercial candidate fee acquisition/rights require explicit same-Sim adoption".into());
            }
            if position.generated_policy.is_some() && !generated {
                return Err("generated commercial candidates require bounded streaming native Sim adoption; compact generated coefficient recipes are not borrowed journal owners".into());
            }
            if generated
                && position
                    .generated_policy
                    .as_ref()
                    .is_some_and(|g| g.fee_acquisition.is_some())
                    != generated_fees
            {
                return Err("generated fee ownership requires the explicitly versioned fee stream schema on every unit".into());
            }
            if generated && position.generated_policy.is_none() {
                return Err(
                    "explicit generated journal requires generated native policy owners".into(),
                );
            }
            if position.credit_identity
                == portfolio_risk_native::commercial_graph::CreditIdentity::IssuerCreditDebt
            {
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
                if sources.into_iter().any(|h| {
                    h.source.credit.opening_undrawn_commitment != 0.
                        || !h.source.performing_draw_tranches.is_empty()
                }) {
                    return Err(
                        "issuer AC security cannot inherit commercial facility/draw ownership"
                            .into(),
                    );
                }
            }
        }
    }
    let preparation_only = consumer.is_some() || matches!(mode, PreparationMode::AdmitBorrowed);
    let mut work = add(mul(raw_bytes, 8)?, units::audit_source_work(&r.commercial)?)?;
    let mut bytes = mul(raw_bytes, 20)?;
    let mut s = spec::normalize(r.specification.clone(), 512, 8_000_000)?;
    let reporting_days = usize::try_from(r.reporting_end_ordinal - r.as_of_ordinal)
        .map_err(|_| "commercial candidate reporting before opening")?;
    // The existing Sim admits a minimum 30-day calendar. Reporting retains the
    // requested actual date; extending the empty calendar never retimes cash.
    if ((!generated && s.horizon_days != reporting_days.max(30))
        || (generated && (s.horizon_days < reporting_days.max(30) || s.horizon_days > 1080)))
        || !s.reverse_severities.is_empty()
        || !s.netting_sets.is_empty()
        || !s.policies.is_empty()
        || !s.cashflows.is_empty()
        || s.accounts.iter().any(|a| {
            a.currency != r.commercial.currency
                || a.annual_fees != 0.
                || a.annual_costs != 0.
                || a.annual_dividends != 0.
                || a.tax_rate != 0.
                || a.capital_deductions != 0.
        })
    {
        return Err("commercial candidate requires exclusive native owners and reviewed cash accounts without generic overlays".into());
    }
    if r.owners.len() != r.commercial.units.len()
        || r.allocation.len() != r.commercial.units.len()
        || s.positions.len() != r.owners.len()
    {
        return Err("complete candidate allocation/owner/placeholder coverage required".into());
    }
    let ledger_runs: BTreeSet<_> = std::iter::once("baseline")
        .chain(s.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let mut runs = BTreeMap::new();
    let mut used_runs = BTreeSet::new();
    for m in &r.run_mapping {
        if !m.severity.is_finite()
            || !ledger_runs.contains(m.ledger_scenario_id.as_str())
            || !used_runs.insert(m.ledger_scenario_id.as_str())
            || runs.insert(m.source_scenario_id.as_str(), m).is_some()
        {
            return Err("explicit unique commercial source/run mapping required".into());
        }
    }
    if used_runs != ledger_runs {
        return Err("every actual journal run needs native commercial sources".into());
    }
    let mut owners = BTreeMap::new();
    let mut positions = BTreeSet::new();
    let mut reservations = Vec::new();
    let mut generated_owner_bytes = 0usize;
    let mut generated_retained_bytes = 0usize;
    let mut fee_groups_upper = 0usize;
    let mut preparation_groups = 0usize;
    for ((u, a), o) in r.commercial.units.iter().zip(&r.allocation).zip(&r.owners) {
        if a.unit_id != u.unit_id
            || o.unit_id != u.unit_id
            || !a.face.is_finite()
            || !(0. ..=1e15).contains(&a.face)
            || o.pre_purchase_principal != 0.
            || o.pre_purchase_accrued != 0.
            || o.pre_purchase_basis != 0.
            || o.accounting_category != u.accounting_category
            || o.ownership != "exclusive_native_commercial_graph-1"
            || o.accounting_category != "AC"
            || o.ledger_classification != "ac"
            || o.source.id.is_empty()
            || o.source.revision.is_empty()
            || owners.insert(o.unit_id.as_str(), o).is_some()
            || !positions.insert(o.position_id.as_str())
        {
            return Err("candidate ownership/allocation identity differs; native acquired owners require AC/ac".into());
        }
        let position = s
            .positions
            .iter()
            .find(|p| p.id.as_str() == o.position_id)
            .ok_or("commercial candidate placeholder absent")?;
        if position.account.as_str() != o.account_id
            || position.classification.as_str() != o.ledger_classification
            || position.kind.as_str()
                != if u.graph.positions[0].credit_identity
                    == portfolio_risk_native::commercial_graph::CreditIdentity::IssuerCreditDebt
                {
                    "security"
                } else {
                    "loan"
                }
        {
            return Err("commercial candidate account/classification mapping differs".into());
        }
        let offset = usize::try_from(u.graph.as_of_ordinal - r.as_of_ordinal)
            .map_err(|_| "commercial candidate purchase precedes opening")?;
        if u.graph.scenarios.len() != runs.len() {
            return Err("commercial candidate scenario mapping coverage differs".into());
        }
        for scene in &u.graph.scenarios {
            let m = runs
                .get(scene.scenario_id.as_str())
                .ok_or("commercial candidate source run absent")?;
            if m.severity.to_bits() != scene.severity.to_bits() {
                return Err("commercial source run severity changed".into());
            }
        }
        if generated {
            let position = &u.graph.positions[0];
            let g = position
                .generated_policy
                .as_ref()
                .ok_or("generated policy absent")?;
            let graph_upper =
                portfolio_risk_native::commercial_generated_graph::admit(&u.graph, position, g)?;
            bytes = add(bytes, graph_upper.retained_bytes_upper)?;
            generated_retained_bytes = add(
                generated_retained_bytes,
                portfolio_risk_native::commercial_generated_graph::physical_journal_retained_upper(
                    g,
                )?,
            )?;
            for law in &g.current_physical {
                let admitted = portfolio_risk_native::commercial_generated_policy::current_physical_source_capacity(g,law)?;
                s.horizon_days = s
                    .horizon_days
                    .max(add(offset, days(admitted.full_credit_end_year)?)?);
                // At most one expanded genuine Physical owner is live. Its
                // structural phase cap coexists with retained compact journal.
                generated_owner_bytes = generated_owner_bytes.max(admitted.retained_bytes_upper);
                work = add(work, mul(admitted.work_upper, law.evaluation.paths)?)?;
            }
        }
        for set in &u.graph.positions[0].histories {
            for h in &set.physical {
                if u.graph.positions[0].credit_identity
                    == portfolio_risk_native::commercial_graph::CreditIdentity::IssuerCreditDebt
                    && (h.source.credit.opening_undrawn_commitment != 0.
                        || !h.source.performing_draw_tranches.is_empty())
                {
                    return Err(
                        "issuer AC security cannot inherit commercial facility/draw ownership"
                            .into(),
                    );
                }
                let admitted = source::admit(&h.source)?;
                s.horizon_days = s
                    .horizon_days
                    .max(add(offset, days(admitted.full_credit_end_year)?)?);
                reservations.push((admitted, offset));
            }
        }
        if !generated {
            if let Some(fees) = &u.graph.positions[0].fees {
                for set in &fees.histories {
                    for history in &set.physical {
                        let tail = usize::try_from(
                            history.policy.complete_end_ordinal - u.graph.as_of_ordinal,
                        )
                        .map_err(|_| "candidate fee complete tail precedes acquisition")?;
                        s.horizon_days = s.horizon_days.max(add(offset, tail)?);
                        fee_groups_upper = add(
                            fee_groups_upper,
                            add(
                                portfolio_risk_native::commercial_fee_source::event_bound(
                                    &history.policy,
                                ),
                                add(fees.allocations.len(), history.acquisition_releases.len())?,
                            )?,
                        )?;
                    }
                }
            }
        }
    }
    spec::validate(&s, 512, 8_000_000)?;
    let mut reserved = (0usize, 0usize);
    for (admitted, _) in &reservations {
        let upper = ledger::admit_upper(
            add(admitted.carry_points_upper, 8)?,
            add(admitted.lots_per_path_upper, 1)?,
            s.horizon_days,
        )?;
        if !preparation_only {
            work = add(work, upper.0)?;
            bytes = add(bytes, add(admitted.joined_bytes_upper, upper.1)?)?;
        }
        preparation_groups = add(
            preparation_groups,
            add(admitted.accounting_groups_upper, 8)?,
        )?;
        reserved = (add(reserved.0, upper.0)?, add(reserved.1, upper.1)?);
    }
    if fee_groups_upper > 0 {
        // At most eight postings plus group/snapshot rows; unchanged joined
        // ledger admission is charged before graph/source generation.
        let rows = mul(fee_groups_upper, 10)?;
        let fee_work = mul(mul(rows, add(s.horizon_days, 1)?)?, 16)?;
        let fee_bytes = mul(rows, 4096)?;
        if !preparation_only {
            work = add(work, fee_work)?;
            bytes = add(bytes, fee_bytes)?;
        }
        reserved = (add(reserved.0, fee_work)?, add(reserved.1, fee_bytes)?);
    }
    if preparation_only {
        let graph_bytes = r.commercial.units.iter().try_fold(0usize, |total, unit| {
            add(
                total,
                portfolio_risk_native::commercial_graph::audit_capacity(&unit.graph)?
                    .retained_bytes_upper,
            )
        })?;
        let cells = mul(
            mul(r.commercial.units.len(), ledger_runs.len())?,
            r.commercial.reporting_ordinals.len(),
        )?;
        let capacity = preparation_capacity::census(preparation_capacity::Input {
            groups: add(preparation_groups, fee_groups_upper)?,
            bindings: add(reservations.len(), fee_groups_upper)?,
            positions: s.positions.len(),
            accounts: s.accounts.len(),
            scenario_label_bytes: ledger_runs.iter().map(|s| s.len()).max().unwrap_or(0),
            coefficient_cells: mul(cells, 11)?,
            graph_bytes,
        })?;
        work = add(work, capacity.work)?;
        bytes = add(bytes, capacity.bytes)?;
    } else {
        work = add(
            work,
            mul(
                mul(s.horizon_days, s.positions.len() + s.accounts.len())?,
                ledger_runs.len(),
            )?,
        )?;
    }
    if generated {
        // Metadata/Q ownership is released before the compact accounting phase.
        // Both phases retain raw input; only the P recipe/proof phase coexists
        // with one expanded full owner and the complete compact journal.
        bytes = bytes.max(add(
            mul(raw_bytes, 20)?,
            add(
                generated_retained_bytes,
                add(generated_owner_bytes, 32 << 20)?,
            )?,
        )?);
        work = add(work, 8_000_000)?;
    }
    work = add(work, bridge_reservation.0)?;
    bytes = add(bytes, bridge_reservation.1)?;
    if work > if generated { 4_000_000_000 } else { MAX_WORK } || bytes > 128 << 20 {
        return Err(format!("commercial candidate joined {}/128MiB preflight exceeds bound before graph generation: work={work},bytes={bytes},physical_retained={generated_retained_bytes},one_owner={generated_owner_bytes}",if generated {"4B"}else{"32M"}));
    }
    let admission = Admission {
        work_upper: work,
        retained_bytes_upper: bytes,
        full_end_ordinal: r
            .as_of_ordinal
            .checked_add(i32::try_from(s.horizon_days).map_err(|_| "candidate ordinal overflow")?)
            .ok_or("candidate ordinal overflow")?,
        output_bytes_upper: MAX_BYTES,
    };
    if !matches!(mode, PreparationMode::Execute) {
        return Ok((None, admission.full_end_ordinal, 0, admission));
    }
    let prepared = match observer {
        Some(observer) => units::prepare_with_source_observer(&r.commercial, observer)?,
        None => units::prepare(&r.commercial)?,
    };
    let allocation_replay = units::evaluate(&prepared, &r.allocation)?;
    if generated {
        return generated_stream::execute(
            r,
            original,
            prepared,
            allocation_replay,
            s,
            work,
            bytes,
            output,
        )
        .map(|(p, end, paths)| (p, end, paths, admission));
    }
    let physical_paths = 0;
    let mut bindings = Vec::new();
    for (ui, graph) in prepared.source().native_graphs.iter().enumerate() {
        let u = &r.commercial.units[ui];
        let o = owners[u.unit_id.as_str()];
        let origin_day = usize::try_from(u.graph.as_of_ordinal - r.as_of_ordinal)
            .map_err(|_| "commercial candidate opening offset")?;
        for native in graph
            .native_sources
            .iter()
            .filter(|s| s.role == Role::Physical)
        {
            let accounting: &dyn ledger::AccountingSource = native
                .physical_accounting_source()
                .ok_or("sealed native Physical graph financial owner absent")?
                .accounting();
            let m = runs[native.scenario_id.as_str()];
            bindings.push(ledger::Binding {
                scenario: &m.ledger_scenario_id,
                position_id: &o.position_id,
                path_key: native.path_key,
                source_weight: native.weight,
                allocated_face: r.allocation[ui].face,
                native_unit_face: u.unit_face,
                accounting,
                origin_day,
                source_class: if u.graph.positions[0].credit_identity
                    == portfolio_risk_native::commercial_graph::CreditIdentity::IssuerCreditDebt
                {
                    ledger::SourceClass::IssuerAcSecurity
                } else {
                    ledger::SourceClass::CommercialLoan
                },
            });
        }
    }
    let mut compiled = ledger::Compiled::new(&s, bindings)?;
    if !generated && fee_groups_upper > 0 {
        let mut fee_bindings = Vec::new();
        let mut parent = 0usize;
        for graph in &prepared.source().native_graphs {
            for native in graph
                .native_sources
                .iter()
                .filter(|s| s.role == Role::Physical)
            {
                if let Some(fees) = &native.fee_projection {
                    fee_bindings.push(ledger::FeeBinding {
                        loan_binding: parent,
                        native_position_id: &native.position_id,
                        source: fees.bound_path.source(),
                        native_years: fees.bound_path.native_years(),
                    });
                }
                parent += 1;
            }
        }
        compiled.attach_fees(&s, fee_bindings)?;
    }
    if !generated
        && (compiled.work_upper > reserved.0 || compiled.retained_bytes_upper > reserved.1)
    {
        return Err(
            "commercial candidate financial owner exceeds reserved structural bound".into(),
        );
    }
    let full_end = r
        .as_of_ordinal
        .checked_add(i32::try_from(s.horizon_days).map_err(|_| "candidate ordinal overflow")?)
        .ok_or("candidate ordinal overflow")?;
    let raw_request = raw_request_receipt(original, r)?;
    let resolved_specification = serde_json::to_value(&s).map_err(|e| e.to_string())?;
    if generated {
        generated_stream::preparation(
            &mut output,
            &raw_request,
            &prepared,
            &allocation_replay,
            &resolved_specification,
            work,
            bytes,
            full_end,
            physical_paths,
        )?;
    }
    if let Some(consume) = consumer {
        consume(
            &s,
            &prepared,
            &compiled,
            &PreparationView {
                raw_request: &raw_request,
                native_preparation: &prepared,
                allocation_replay: &allocation_replay,
                resolved_specification: &resolved_specification,
                admitted_work_upper: work,
                retained_bytes_upper: bytes,
            },
        )?;
    } else {
        daily::run_with_commercial(&s, None, &compiled, output)?;
    }
    drop(compiled);
    portfolio_compute_control::checkpoint()?;
    Ok((
        Some(Preparation {
            raw_request,
            native_preparation: prepared,
            allocation_replay,
            resolved_specification,
            admitted_work_upper: work,
            retained_bytes_upper: bytes,
        }),
        full_end,
        physical_paths,
        admission,
    ))
}

pub(super) struct Admission {
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
    pub full_end_ordinal: i32,
    pub output_bytes_upper: usize,
}
/// Exact ordinary admission, with no graph, stopped owner or Sim generation.
pub(super) fn admit_finite(r: &Request) -> Result<Admission, String> {
    let (_, _, _, admission) = prepare_and_run(
        r,
        false,
        None,
        Box::new(std::io::sink()),
        (0, 0),
        PreparationMode::AdmitOrdinary,
        None,
    )?;
    Ok(admission)
}

/// Preparation-only admission is selected only by the native cut callback.
/// Original source/graph work is unchanged; full daily runtime is separately
/// charged by the cut component before either source can be generated.
pub(super) fn admit_finite_preparation(r: &Request) -> Result<Admission, String> {
    let (_, _, _, admission) = prepare_and_run(
        r,
        false,
        None,
        Box::new(std::io::sink()),
        (0, 0),
        PreparationMode::AdmitBorrowed,
        None,
    )?;
    Ok(admission)
}

/// Native-only borrow scope. Prepared source and Compiled remain genuine and
/// live for the callback; no financial checkpoint can be supplied or escaped.
pub(super) fn with_finite_compiled(
    r: &Request,
    reservation: (usize, usize),
    consumer: &mut dyn for<'source> FnMut(
        &'source portfolio_ledger_native::types::Spec,
        &'source units::Prepared,
        &ledger::Compiled<'source>,
    ) -> Result<(), String>,
) -> Result<(Preparation, Admission), String> {
    with_finite_compiled_receipt(r, reservation, &mut |spec, prepared, compiled, _| {
        consumer(spec, prepared, compiled)
    })
}

pub(super) fn with_finite_compiled_receipt(
    r: &Request,
    reservation: (usize, usize),
    consumer: &mut FiniteConsumer<'_>,
) -> Result<(Preparation, Admission), String> {
    let (prepared, _, _, admission) = prepare_and_run(
        r,
        false,
        None,
        Box::new(std::io::sink()),
        reservation,
        PreparationMode::Execute,
        Some(consumer),
    )?;
    Ok((
        prepared.ok_or("finite native preparation absent")?,
        admission,
    ))
}

/// Observe source proofs during the very same finite preparation. Callback
/// serialization has its own caller-funded bounded work/output contract; this
/// function never deducts source work or enlarges the source admission.
pub(super) fn with_finite_compiled_observer(
    r: &Request,
    reservation: (usize, usize),
    observer: &mut units::SourceObserver<'_>,
    consumer: &mut FiniteConsumer<'_>,
) -> Result<(Preparation, Admission), String> {
    let (prepared, _, _, admission) = prepare_and_run_observed(
        r,
        false,
        None,
        Box::new(std::io::sink()),
        reservation,
        PreparationMode::Execute,
        Some(consumer),
        Some(observer),
    )?;
    Ok((
        prepared.ok_or("finite observed native preparation absent")?,
        admission,
    ))
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_reserved(r, (0, 0))
}
/// A native downstream owner reserves its coexisting bridge before any graph
/// generation. Public callers cannot supply financial owners or bypass bounds.
pub(super) fn evaluate_reserved(
    r: &Request,
    reservation: (usize, usize),
) -> Result<ResultData, String> {
    evaluate_reserved_bounded(r, reservation, MAX_BYTES)
}
/// A stricter native downstream component may refuse a smaller output tier;
/// it cannot enlarge the ordinary candidate source, stream or result caps.
pub(super) fn evaluate_reserved_bounded(
    r: &Request,
    reservation: (usize, usize),
    output_limit: usize,
) -> Result<ResultData, String> {
    if output_limit == 0 || output_limit > MAX_BYTES {
        return Err("candidate bounded result tier exceeds ordinary output capacity".into());
    }
    let output = Buffer(Arc::default(), output_limit);
    let (preparation, full_end, _, _) = prepare_and_run(
        r,
        false,
        None,
        Box::new(output.clone()),
        reservation,
        PreparationMode::Execute,
        None,
    )?;
    let preparation = preparation.ok_or("finite candidate preparation absent")?;
    let data = std::mem::take(&mut *output.0.lock().map_err(|_| "candidate buffer poisoned")?);
    let protocol = if r
        .commercial
        .units
        .iter()
        .any(|u| u.graph.positions.iter().any(|p| p.fees.is_some()))
    {
        7
    } else {
        ledger::PROTOCOL
    };
    let (tables, completion) = decode_stream(&data, protocol)?;
    let result=ResultData{schema:SCHEMA,preparation,tables,producer_completion:completion,reporting_end_ordinal:r.reporting_end_ordinal,full_end_ordinal:full_end,shared_daily_sim_adoption:true,graph_exercise_source_adoption:true,independent_persisted_verification:false,opening_identity_verified:false,mixed_ledger_adoption:false,empirical_validation:false,scope:"actual purchased commercial candidates from sealed native unit/conditional-exercise Physical sources; original Pricing OAS and separate stopped Physical acquisition/EIR anchors retained; complete daily source/default/coupon/recovery tails in one existing Sim; no caller monetary owner or generic mixed book; supplied opening receipt identity is not verified opening evidence"};
    strict_json::serialized_size(&result, output_limit)?;
    portfolio_compute_control::checkpoint()?;
    Ok(result)
}
/// Admit the complete columnar-to-row Value expansion before materialization.
/// The ordinary decoder and its historical outputs remain unchanged.
pub(super) fn decode_stream_bounded(
    data: &[u8],
    protocol: u32,
    value_bytes: usize,
) -> Result<(BTreeMap<String, Vec<Value>>, Value), String> {
    // Parsed source rows and their ordinary cloned rows can coexist. Forty
    // times encoded bytes bounds both Values under the retained JSON policy.
    let mut upper = mul(data.len(), 40)?;
    if upper > value_bytes {
        return Err("native cut decoded Value phase exceeds64MiB".into());
    }
    let mut dictionary = Vec::<String>::new();
    for line in data.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        portfolio_compute_control::checkpoint()?;
        let row = strict_json::parse(line, MAX_BYTES)?;
        if row.get("table").and_then(Value::as_str) != Some("journal") {
            continue;
        }
        let labels = row["dictionary"]
            .as_array()
            .ok_or("candidate journal dictionary absent")?;
        for label in labels {
            dictionary.push(
                label
                    .as_str()
                    .ok_or("candidate journal dictionary label differs")?
                    .to_string(),
            );
        }
        let columns = row["columns"]
            .as_object()
            .ok_or("candidate journal columns absent")?;
        let count = columns
            .get("value")
            .and_then(Value::as_array)
            .ok_or("candidate journal values absent")?
            .len();
        if columns.len() != 8
            || columns
                .values()
                .any(|v| v.as_array().is_none_or(|v| v.len() != count))
        {
            return Err("candidate journal column shape differs".into());
        }
        for i in 0..count {
            let mut encoded = 2usize;
            for (key, values) in columns {
                encoded = add(encoded, add(mul(key.len(), 6)?, 8)?)?;
                let value = &values[i];
                let amount = if matches!(
                    key.as_str(),
                    "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                ) {
                    let index = usize::try_from(
                        value
                            .as_u64()
                            .ok_or("candidate journal dictionary index differs")?,
                    )
                    .map_err(|_| "candidate dictionary overflow")?;
                    let label = dictionary
                        .get(index)
                        .ok_or("candidate journal dictionary index absent")?;
                    add(mul(label.len(), 6)?, 2)?
                } else {
                    64
                };
                encoded = add(encoded, amount)?;
            }
            upper = add(upper, mul(encoded, 20)?)?;
            if upper > value_bytes {
                return Err("native cut decoded dictionary expansion exceeds64MiB".into());
            }
        }
    }
    decode_stream(data, protocol)
}

pub(super) fn decode_stream(
    data: &[u8],
    protocol: u32,
) -> Result<(BTreeMap<String, Vec<Value>>, Value), String> {
    let mut tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut completion = None;
    let mut dictionary = Vec::<String>::new();
    for line in data.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        portfolio_compute_control::checkpoint()?;
        let row = strict_json::parse(line, MAX_BYTES)?;
        if let Some(table) = row.get("table").and_then(Value::as_str) {
            if table == "journal" {
                dictionary.extend(
                    serde_json::from_value::<Vec<String>>(row["dictionary"].clone())
                        .map_err(|e| e.to_string())?,
                );
                let columns = row["columns"]
                    .as_object()
                    .ok_or("candidate journal columns absent")?;
                let count = columns["value"]
                    .as_array()
                    .ok_or("candidate journal values absent")?
                    .len();
                if columns.len() != 8
                    || columns
                        .values()
                        .any(|v| v.as_array().is_none_or(|v| v.len() != count))
                {
                    return Err("candidate journal column shape differs".into());
                }
                for i in 0..count {
                    let mut decoded = serde_json::Map::new();
                    for (key, values) in columns {
                        let value = &values[i];
                        if matches!(
                            key.as_str(),
                            "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                        ) {
                            let index = usize::try_from(
                                value
                                    .as_u64()
                                    .ok_or("candidate journal dictionary index differs")?,
                            )
                            .map_err(|_| "candidate dictionary overflow")?;
                            decoded.insert(
                                key.clone(),
                                json!(dictionary
                                    .get(index)
                                    .ok_or("candidate journal dictionary index absent")?),
                            );
                        } else {
                            decoded.insert(key.clone(), value.clone());
                        }
                    }
                    tables
                        .entry(table.into())
                        .or_default()
                        .push(Value::Object(decoded));
                }
            } else {
                let rows = row["rows"]
                    .as_array()
                    .ok_or("candidate stream rows absent")?;
                tables
                    .entry(table.into())
                    .or_default()
                    .extend(rows.iter().cloned());
            }
        } else if row.get("complete") == Some(&Value::Bool(true))
            && row.get("protocol") == Some(&json!(protocol))
            && completion.is_none()
        {
            completion = Some(row);
        } else {
            return Err("candidate stream completion/record differs".into());
        }
    }
    let completion = completion.ok_or("candidate native journal did not complete")?;
    Ok((tables, completion))
}

pub fn run_envelope_started(bytes: &[u8], started: Instant) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Request,
    }
    let raw = strict_json::parse_checked(bytes, MAX_BYTES, portfolio_compute_control::checkpoint)?;
    let original = raw["request"].clone();
    let envelope: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_commercial_candidate_journal" || envelope.schema != SCHEMA {
        return Err("explicit commercial candidate operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(
            envelope.execution_timeout_ms,
            started,
        )?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut result = evaluate(&envelope.request)?;
        result.preparation.raw_request = original;
        portfolio_compute_control::checkpoint()?;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod raw_receipt_tests {
    use super::*;
    use std::cell::Cell;

    struct Observed<'a, T> {
        value: &'a T,
        calls: &'a Cell<usize>,
    }
    impl<T: Serialize> Serialize for Observed<'_, T> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.calls.set(self.calls.get() + 1);
            self.value.serialize(serializer)
        }
    }

    #[test]
    fn real_finite_and_generated_receipts_serialize_only_the_missing_original() {
        for bytes in [
            include_bytes!("../../../../../examples/models/commercial-candidate-journal-1.json")
                .as_slice(),
            include_bytes!(
                "../../../../../examples/models/commercial-generated-candidate-journal-1.json"
            )
            .as_slice(),
            include_bytes!(
                "../../../../../examples/models/commercial-generated-candidate-fee-journal-1.json"
            )
            .as_slice(),
        ] {
            let typed: Request = serde_json::from_slice(bytes).unwrap();
            let original: Value = serde_json::from_slice(bytes).unwrap();
            let calls = Cell::new(0);
            let observed = Observed {
                value: &typed,
                calls: &calls,
            };
            let retained = raw_request_receipt(Some(&original), &observed).unwrap();
            assert_eq!(calls.get(), 0);
            assert_eq!(
                serde_json::to_vec(&retained).unwrap(),
                serde_json::to_vec(&original).unwrap()
            );

            let fallback = raw_request_receipt(None, &observed).unwrap();
            assert_eq!(calls.get(), 1);
            assert_eq!(
                serde_json::to_vec(&fallback).unwrap(),
                serde_json::to_vec(&serde_json::to_value(&typed).unwrap()).unwrap()
            );
        }
    }

    #[test]
    fn supplied_receipt_preserves_all_scalar_categories_and_skips_fallible_fallback() {
        struct Rejected;
        impl Serialize for Rejected {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("fallback serialization invoked"))
            }
        }
        let original = json!({"signed":-0.0,"integer":u64::MAX,"missing":null,
            "paths":["baseline","stress"],"zero":0,"enabled":false});
        let actual = raw_request_receipt(Some(&original), &Rejected).unwrap();
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&original).unwrap()
        );
        assert_eq!(
            actual["signed"].as_f64().unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(actual["integer"].as_u64(), Some(u64::MAX));
        assert!(raw_request_receipt(None, &Rejected)
            .unwrap_err()
            .contains("fallback serialization invoked"));
    }
}

#[cfg(test)]
mod cut_decoder_tests {
    use super::*;
    fn stream() -> Vec<u8> {
        let row = json!({"table":"journal","dictionary":["baseline","bank","opening","cash",""],
            "columns":{"transaction_number":[1],"scenario":[0],"day":[3],"account":[1],"event":[2],"gl_account":[3],"instrument_id":[4],"value":[2.0]}});
        format!("{}\n{}\n", row, json!({"complete":true,"protocol":8})).into_bytes()
    }
    #[test]
    fn bounded_decoder_preserves_every_ordinary_decoded_cell() {
        let data = stream();
        let expected = decode_stream(&data, 8).unwrap();
        let actual = decode_stream_bounded(&data, 8, 1 << 20).unwrap();
        assert_eq!(expected, actual);
        assert_eq!(actual.0["journal"][0]["value"], 2.0);
    }
    #[test]
    fn bounded_decoder_reserves_dictionary_expansion_before_cloning_rows() {
        let data = stream();
        let error = decode_stream_bounded(&data, 8, data.len() * 40).unwrap_err();
        assert!(error.contains("dictionary expansion"));
    }
    #[test]
    fn bounded_decoder_rejects_incomplete_or_malformed_native_stream() {
        assert!(decode_stream_bounded(
            b"{\"table\":\"journal\",\"dictionary\":[],\"columns\":{}}",
            8,
            1 << 20
        )
        .is_err());
        assert!(
            decode_stream_bounded(b"{\"complete\":true,\"protocol\":7}\n", 8, 1 << 20).is_err()
        );
    }
}
