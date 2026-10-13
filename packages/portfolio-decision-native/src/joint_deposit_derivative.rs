//! Explicit raw joint owner. One actual opening, one existing daily Sim, no monetary union.
use crate::saved_derivative_mixed::{self as shared, Owner};
use portfolio_compute_control::{checkpoint, scope, Control};
use portfolio_ledger_native::{
    deposit_source_portfolio as deposit, derivative_intent_owner as owner,
};
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub const SCHEMA: &str = "saved-derivative-deposit-portfolio-mixed-1";
const MAX_INPUT: usize = 32 * 1024 * 1024;
const MAX_STREAM: usize = 32 * 1024 * 1024;
const MAX_RETAINED: usize = 128 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    identity: portfolio_risk_native::derivative_contract::Identity,
    ownership: String,
    phase: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    portfolio: deposit::Request,
    derivative_owners: Vec<Owner>,
    policy: Policy,
}
mod admission;
pub use admission::Admission;
use admission::{add, derivative_upper, mul};
#[derive(Clone)]
struct Output(Arc<Mutex<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        checkpoint().map_err(io::Error::other)?;
        let mut b = self
            .0
            .lock()
            .map_err(|_| io::Error::other("joint output lock"))?;
        if b.len()
            .checked_add(data.len())
            .is_none_or(|n| n > MAX_STREAM)
        {
            return Err(io::Error::other("joint32MiB output admission"));
        }
        b.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        checkpoint().map_err(io::Error::other)
    }
}
#[cfg(test)]
thread_local! { static GENERATOR_CALLS: std::cell::Cell<[usize;2]> = const { std::cell::Cell::new([0,0]) }; }
fn produce(bytes: &[u8]) -> Result<Produced, String> {
    checkpoint()?;
    let raw_joint_arena_upper = mul(bytes.len(), 16)?;
    if raw_joint_arena_upper > MAX_RETAINED {
        return Err("joint original byte arena exceeds128MiB before parsing".into());
    }
    let original = strict_json::parse(bytes, MAX_INPUT)?;
    let r: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if r.schema != SCHEMA
        || r.policy.identity.id.is_empty()
        || r.policy.identity.revision.is_empty()
        || r.policy.ownership != "exclusive_native_deposit_portfolio_and_derivative-1"
        || r.policy.phase != "dated_cash_then_deposit_atom_then_derivative_native_events-1"
    {
        return Err("explicit joint schema/reviewed ownership/phase required".into());
    }
    let (raw_deposit, raw_derivative) = (&original["portfolio"], &original["derivative_owners"]);
    deposit::parse(&serde_json::to_vec(raw_deposit).map_err(|e| e.to_string())?)?;
    shared::raw_domain::validate_owners(&json!({"owners":raw_derivative}))?;

    // Raw reviewed gross anchors are provisional admission only. Native projection
    // subsequently verifies them, and Compiled compares generated gross stocks.
    let opening = shared::opening_anchors(&r.derivative_owners)?;
    let da = deposit::audit_with_native_opening(&r.portfolio, &opening)?;
    let spec = portfolio_ledger_native::spec::normalize_with_native_opening(
        r.portfolio.specification.clone(),
        256,
        owner::MAX_WORK,
        &opening,
    )?;
    let asof = r.portfolio.positions[0].paths[0]
        .source
        .continuation
        .original
        .as_of_ordinal;
    let end = asof
        .checked_add(spec.horizon_days as i32)
        .ok_or("joint date overflow")?;
    let expected = r
        .portfolio
        .runs
        .iter()
        .map(|r| r.scenario_id.clone())
        .collect();
    shared::validate_owners_for_context(&r.derivative_owners, &spec, asof, end, &expected)?;
    let mixed_days = spec
        .horizon_days
        .checked_add(1)
        .ok_or("joint mixed days overflow")?;
    let (dw, db, count) = derivative_upper(&r, mixed_days)?;
    let staged = 2 * MAX_STREAM;
    let work = add(mul(bytes.len(), 8)?, add(da.work_units, dw)?)?;
    let retained = add(
        raw_joint_arena_upper,
        add(da.reserved_bytes, add(db, staged)?)?,
    )?;
    if work > owner::MAX_WORK || retained > MAX_RETAINED {
        return Err(format!(
            "joint pre-generation8M/128MiB reservation exceeded: work={work}, retained={retained}"
        ));
    }
    let admission = Admission {
        deposit: da,
        derivative_work_upper: dw,
        derivative_retained_upper: db,
        joined_work_upper: work,
        joined_retained_upper: retained,
        stream_and_header_staging: staged,
        derivative_sources: count,
        raw_joint_arena_upper,
    };
    #[cfg(test)]
    GENERATOR_CALLS.with(|c| {
        let mut n = c.get();
        n[0] += 1;
        c.set(n)
    });
    let portfolio = deposit::prepare_with_native_opening(r.portfolio, &opening)?;
    #[cfg(test)]
    GENERATOR_CALLS.with(|c| {
        let mut n = c.get();
        n[1] += 1;
        c.set(n)
    });
    let results = shared::project_sources(&r.derivative_owners, portfolio.dated())?;
    let native_wire = results.iter().try_fold(0, |n, r| {
        add(n, strict_json::serialized_size(r, MAX_STREAM)?)
    })?;
    if native_wire > db {
        return Err("joint native sources exceeded structural reservation".into());
    }
    let mut sources = Vec::new();
    let mut i = 0;
    for o in &r.derivative_owners {
        for run in &o.runs {
            sources.push(shared::source(run, &results[i], &o.account_id)?);
            i += 1;
        }
    }
    let compiled = owner::Compiled::new(portfolio.dated(), sources)?;
    if compiled.work > add(admission.deposit.work_units, dw)? || compiled.capacity > db {
        return Err("joint mapped derivative usage exceeds reservation".into());
    }
    let output = Output(Arc::new(Mutex::new(Vec::with_capacity(MAX_STREAM))));
    portfolio_ledger_native::daily::run_dated_with_source_portfolio_and_derivatives(
        &portfolio,
        &compiled,
        Box::new(output.clone()),
    )?;
    let derivative_work_used = compiled.work;
    let derivative_capacity_used = compiled.capacity;
    drop(compiled);
    let stream = String::from_utf8(
        Arc::try_unwrap(output.0)
            .map_err(|_| "joint output remains borrowed")?
            .into_inner()
            .map_err(|_| "joint output lock")?,
    )
    .map_err(|e| e.to_string())?;
    checkpoint()?;
    Ok(Produced {
        original,
        portfolio,
        results,
        stream,
        admission,
        derivative_work_used,
        derivative_capacity_used,
        derivative_output_bytes: native_wire,
    })
}
struct Produced {
    original: Value,
    portfolio: deposit::Prepared,
    results: Vec<portfolio_risk_native::saved_derivative_source::ResultData>,
    stream: String,
    admission: Admission,
    derivative_work_used: usize,
    derivative_capacity_used: usize,
    derivative_output_bytes: usize,
}
#[derive(Serialize)]
struct Receipt<'a> {
    schema: &'static str,
    original_request: &'a Value,
    raw_portfolio: &'a Value,
    raw_derivative_owners: &'a Value,
    portfolio_prepared_request: &'a portfolio_ledger_native::dated_ledger::DatedLedgerRequest,
    portfolio_usage: &'a deposit::Usage,
    portfolio_sources: PortfolioSources<'a>,
    native_sources: &'a [portfolio_risk_native::saved_derivative_source::ResultData],
    #[serde(skip_serializing_if = "Option::is_none")]
    native_stream: Option<&'a str>,
    admission: &'a Admission,
    derivative_work_used: usize,
    derivative_capacity_used: usize,
    derivative_output_bytes: usize,
    same_sim_native_replay: bool,
    empirical_validation: bool,
    capital_ftp_eligible: bool,
    public_adoption: bool,
}
#[derive(Serialize)]
struct PortfolioSources<'a> {
    owners: &'a [deposit::OwnedPosition],
    cash_origins: &'a [deposit::CashOrigin],
}
impl Produced {
    fn receipt(&self) -> Receipt<'_> {
        let (owners, cash_origins) = self.portfolio.source_receipts();
        Receipt {
            schema: SCHEMA,
            original_request: &self.original,
            raw_portfolio: &self.original["portfolio"],
            raw_derivative_owners: &self.original["derivative_owners"],
            portfolio_prepared_request: self.portfolio.prepared_request(),
            portfolio_usage: self.portfolio.usage(),
            portfolio_sources: PortfolioSources {
                owners,
                cash_origins,
            },
            native_sources: &self.results,
            native_stream: Some(&self.stream),
            admission: &self.admission,
            derivative_work_used: self.derivative_work_used,
            derivative_capacity_used: self.derivative_capacity_used,
            derivative_output_bytes: self.derivative_output_bytes,
            same_sim_native_replay: true,
            empirical_validation: false,
            capital_ftp_eligible: false,
            public_adoption: false,
        }
    }
}
pub fn execute(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let produced = produce(bytes)?;
    bounded_frame(&produced.receipt())
}
// One exact-sized encoded arena, separately charged from the retained stream.
fn bounded_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    checkpoint()?;
    let size = strict_json::serialized_size(value, MAX_STREAM)?;
    let mut encoded = Vec::with_capacity(size);
    serde_json::to_writer(&mut encoded, value).map_err(|e| e.to_string())?;
    if encoded.len() != size {
        return Err("joint frame count differs".into());
    }
    checkpoint()?;
    Ok(encoded)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    request: Value,
    execution_timeout_ms: u64,
}
pub fn run(bytes: &[u8], output: &mut dyn Write) -> Result<(), String> {
    run_started(bytes, output, std::time::Instant::now())
}
/// Selected dispatcher supplies its original entry clock, including routing decode.
pub fn run_started(
    bytes: &[u8],
    output: &mut dyn Write,
    parse_started: std::time::Instant,
) -> Result<(), String> {
    checkpoint()?;
    let parent = portfolio_compute_control::active();
    if mul(bytes.len(), 16)? > MAX_RETAINED {
        return Err("joint envelope arena exceeds128MiB before parsing".into());
    }
    let v = strict_json::parse_checked(bytes, MAX_INPUT, checkpoint)?;
    let e: Envelope = serde_json::from_value(v).map_err(|e| e.to_string())?;
    let control = match parent {
        Some(p) => p.child_started(e.execution_timeout_ms, parse_started)?,
        None => Control::from_started(e.execution_timeout_ms, parse_started)?,
    };
    scope(Some(control), || {
        let produced = produce(&strict_json::encode(&e.request, MAX_INPUT)?)?;
        let mut receipt = produced.receipt();
        receipt.native_stream = None;
        #[derive(Serialize)]
        struct Header<'a> {
            workflow: Workflow<'a>,
        }
        #[derive(Serialize)]
        struct Workflow<'a> {
            schema: &'static str,
            model_schema: &'static str,
            raw_joint_request: &'a Value,
            portfolio_prepared_request:
                &'a portfolio_ledger_native::dated_ledger::DatedLedgerRequest,
            source_receipt: Receipt<'a>,
        }
        let header = bounded_frame(&Header {
            workflow: Workflow {
                schema: "model-ledger-preparation-1",
                model_schema: SCHEMA,
                raw_joint_request: &produced.original,
                portfolio_prepared_request: produced.portfolio.prepared_request(),
                source_receipt: receipt,
            },
        })?;
        let stream = &produced.stream;
        if add(add(header.len(), 1)?, stream.len())? > MAX_STREAM {
            return Err("joint complete public stream exceeds32MiB".into());
        }
        checkpoint()?;
        output
            .write_all(&header)
            .and_then(|_| output.write_all(b"\n"))
            .map_err(|e| e.to_string())?;
        for c in stream.as_bytes().chunks(32768) {
            checkpoint()?;
            output.write_all(c).map_err(|e| e.to_string())?;
            checkpoint()?;
        }
        output.flush().map_err(|e| e.to_string())?;
        checkpoint()
    })
}
#[cfg(test)]
mod tests;
