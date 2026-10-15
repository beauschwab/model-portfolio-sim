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
const MAX_BYTES: usize = 32 << 20;
const MAX_WORK: usize = 32_000_000;
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
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
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
            .is_none_or(|n| n > MAX_BYTES)
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
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    portfolio_compute_control::checkpoint()?;
    validate_ordinal(r.as_of_ordinal)?;
    validate_ordinal(r.reporting_end_ordinal)?;
    if r.schema != SCHEMA
        || r.commercial.reporting_ordinals.first() != Some(&r.as_of_ordinal)
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
    let raw_bytes = strict_json::serialized_size(r, MAX_BYTES)?;
    for unit in &r.commercial.units {
        for position in &unit.graph.positions {
            if position.generated_policy.is_some() {
                return Err("generated commercial candidates require bounded streaming native Sim adoption; compact generated coefficient recipes are not borrowed journal owners".into());
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
    let mut work = add(mul(raw_bytes, 8)?, units::audit_source_work(&r.commercial)?)?;
    let mut bytes = mul(raw_bytes, 20)?;
    let mut s = spec::normalize(r.specification.clone(), 512, 8_000_000)?;
    let reporting_days = usize::try_from(r.reporting_end_ordinal - r.as_of_ordinal)
        .map_err(|_| "commercial candidate reporting before opening")?;
    // The existing Sim admits a minimum 30-day calendar. Reporting retains the
    // requested actual date; extending the empty calendar never retimes cash.
    if s.horizon_days != reporting_days.max(30)
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
    }
    spec::validate(&s, 512, 8_000_000)?;
    let mut reserved = (0usize, 0usize);
    for (admitted, _) in &reservations {
        let upper = ledger::admit_upper(
            add(admitted.carry_points_upper, 8)?,
            add(admitted.lots_per_path_upper, 1)?,
            s.horizon_days,
        )?;
        work = add(work, upper.0)?;
        bytes = add(bytes, add(admitted.joined_bytes_upper, upper.1)?)?;
        reserved = (add(reserved.0, upper.0)?, add(reserved.1, upper.1)?);
    }
    work = add(
        work,
        mul(
            mul(s.horizon_days, s.positions.len() + s.accounts.len())?,
            ledger_runs.len(),
        )?,
    )?;
    if work > MAX_WORK || bytes > 128 << 20 {
        return Err(
            "commercial candidate joined32M/128MiB preflight exceeds bound before graph generation"
                .into(),
        );
    }
    let prepared = units::prepare(&r.commercial)?;
    let allocation_replay = units::evaluate(&prepared, &r.allocation)?;
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
            let financial = native
                .physical_accounting_source()
                .ok_or("sealed native Physical graph financial owner absent")?;
            let m = runs[native.scenario_id.as_str()];
            bindings.push(ledger::Binding {
                scenario: &m.ledger_scenario_id,
                position_id: &o.position_id,
                path_key: native.path_key,
                source_weight: native.weight,
                allocated_face: r.allocation[ui].face,
                native_unit_face: u.unit_face,
                accounting: financial.accounting(),
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
    let compiled = ledger::Compiled::new(&s, bindings)?;
    if compiled.work_upper > reserved.0 || compiled.retained_bytes_upper > reserved.1 {
        return Err(
            "commercial candidate financial owner exceeds reserved structural bound".into(),
        );
    }
    let output = Buffer::default();
    daily::run_with_commercial(&s, None, &compiled, Box::new(output.clone()))?;
    let data = std::mem::take(&mut *output.0.lock().map_err(|_| "candidate buffer poisoned")?);
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
            && row.get("protocol") == Some(&json!(ledger::PROTOCOL))
            && completion.is_none()
        {
            completion = Some(row);
        } else {
            return Err("candidate stream completion/record differs".into());
        }
    }
    let completion = completion.ok_or("candidate native journal did not complete")?;
    let full_end = r
        .as_of_ordinal
        .checked_add(i32::try_from(s.horizon_days).map_err(|_| "candidate ordinal overflow")?)
        .ok_or("candidate ordinal overflow")?;
    let result=ResultData{schema:SCHEMA,preparation:Preparation{raw_request:serde_json::to_value(r).map_err(|e|e.to_string())?,native_preparation:prepared,allocation_replay,resolved_specification:serde_json::to_value(s).map_err(|e|e.to_string())?,admitted_work_upper:work,retained_bytes_upper:bytes},tables,producer_completion:completion,reporting_end_ordinal:r.reporting_end_ordinal,full_end_ordinal:full_end,shared_daily_sim_adoption:true,graph_exercise_source_adoption:true,independent_persisted_verification:false,opening_identity_verified:false,mixed_ledger_adoption:false,empirical_validation:false,scope:"actual purchased commercial candidates from sealed native unit/conditional-exercise Physical sources; original Pricing OAS and separate stopped Physical acquisition/EIR anchors retained; complete daily source/default/coupon/recovery tails in one existing Sim; no caller monetary owner or generic mixed book; supplied opening receipt identity is not verified opening evidence"};
    strict_json::serialized_size(&result, MAX_BYTES)?;
    portfolio_compute_control::checkpoint()?;
    Ok(result)
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
