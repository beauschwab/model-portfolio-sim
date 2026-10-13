//! Staged realized physical path journals. Structural compilation precedes all output;
//! each bounded core batch is financially validated before its rows are emitted.
use crate::types::Spec;
use portfolio_model_core::cashflow::validate_ordinal;
use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_path::{BoundaryAction, ACT365_SECONDS_PER_YEAR};
use portfolio_model_core::credit_path_accounting::{self as core, PathAccountingRequest};
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufWriter, Write};

pub const SCHEMA: &str = "credit-path-ledger-1";
pub const PROTOCOL: u32 = 5;
pub const MAX_WORK: usize = 90_000_000;
pub const MAX_POSITIONS: usize = 60_000;
pub const MAX_ACCOUNT_DAYS: usize = 250_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionAccount {
    pub position_id: String,
    pub account_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathLedgerBatch {
    pub batch_id: String,
    pub account_by_position: Vec<PositionAccount>,
    pub request: PathAccountingRequest,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathLedgerRun {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub batches: Vec<PathLedgerBatch>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathLedgerRequest {
    pub schema: String,
    pub monetary_unit: String,
    pub as_of_ordinal: i32,
    pub specification: Value,
    pub runs: Vec<PathLedgerRun>,
}
#[derive(Debug)]
pub struct CompiledPathLedger {
    spec: Spec,
    as_of: i32,
    end: i32,
    runs: Vec<PathLedgerRun>,
    admitted_work: usize,
    max_work: usize,
}
fn bits(x: f64) -> u64 {
    if x == 0.0 {
        0.0f64.to_bits()
    } else {
        x.to_bits()
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8_f64.max(128.0 * f64::EPSILON * a.abs().max(b.abs()))
}
fn money(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("path ledger currency amount exceeds admission".into())
    } else {
        Ok(())
    }
}
fn add_work(work: &mut usize, n: usize, limit: usize) -> Result<(), String> {
    *work = work.checked_add(n).ok_or("path ledger work overflow")?;
    if *work > limit {
        Err("path ledger work admission exceeded".into())
    } else {
        Ok(())
    }
}
fn text(s: &str, bytes: &mut usize) -> Result<(), String> {
    *bytes = bytes
        .checked_add(s.len())
        .ok_or("path ledger metadata overflow")?;
    if s.is_empty()
        || s.len() > 256
        || s.chars().any(char::is_control)
        || *bytes > MAX_DATED_METADATA_BYTES
    {
        Err("path ledger metadata admission exceeded".into())
    } else {
        Ok(())
    }
}
fn metadata(value: &Value, bytes: &mut usize) -> Result<(), String> {
    match value {
        Value::String(s) => text(s, bytes),
        Value::Array(a) => {
            for v in a {
                metadata(v, bytes)?;
            }
            Ok(())
        }
        Value::Object(o) => {
            for v in o.values() {
                metadata(v, bytes)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
impl CompiledPathLedger {
    pub fn compile(request: PathLedgerRequest) -> Result<Self, String> {
        Self::compile_with_limits(request, MAX_POSITIONS, MAX_WORK)
    }
    pub fn normalized_spec(&self) -> &Spec {
        &self.spec
    }
    pub fn admitted_work(&self) -> usize {
        self.admitted_work
    }
    pub fn compile_with_limits(
        request: PathLedgerRequest,
        max_positions: usize,
        max_work: usize,
    ) -> Result<Self, String> {
        if max_positions > MAX_POSITIONS
            || max_work > MAX_WORK
            || request.schema != SCHEMA
            || request.monetary_unit != "currency_units"
        {
            return Err("unsupported path ledger schema/units/admission tier".into());
        }
        validate_ordinal(request.as_of_ordinal)?;
        let spec = crate::spec::normalize_signed_credit_path(
            request.specification,
            max_positions,
            max_work,
        )?;
        let end = request
            .as_of_ordinal
            .checked_add(spec.horizon_days as i32)
            .ok_or("path ledger clock overflow")?;
        validate_ordinal(end)?;
        if spec.positions.is_empty()
            || !spec.cashflows.is_empty()
            || !spec.netting_sets.is_empty()
            || !spec.policies.is_empty()
            || spec.accounts.iter().any(|a| {
                a.annual_fees != 0.0
                    || a.annual_costs != 0.0
                    || a.tax_rate != 0.0
                    || a.annual_dividends != 0.0
            })
        {
            return Err("path ledger requires exclusive owned loans without legacy cashflow/earnings policies".into());
        }
        for p in &spec.positions {
            if p.kind != "loan"
                || p.start_day != 0
                || p.annual_pd != 0.0
                || p.migration_rate != 0.0
                || p.draw_fraction != 0.0
                || p.monthly_runoff != 0.0
                || p.floating_beta != 0.0
                || p.encumbered_fraction != 0.0
                || !p.collateral_position.is_empty()
                || p.eligible_fraction != 0.0
                || matches!(p.classification.as_str(), "afs" | "trading")
            {
                return Err(
                    "path owned positions cannot overlap legacy financial ownership".into(),
                );
            }
        }
        let expected: BTreeSet<_> = std::iter::once(("baseline".to_string(), bits(1.0)))
            .chain(spec.scenarios.iter().flat_map(|s| {
                std::iter::once((s.name.clone(), bits(1.0))).chain(
                    spec.reverse_severities
                        .iter()
                        .map(|&v| (s.name.clone(), bits(v))),
                )
            }))
            .collect();
        if request.runs.len() != expected.len() {
            return Err("path ledger requires complete baseline/scenario/reverse coverage".into());
        }
        let accounts: HashMap<_, _> = spec.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
        let positions: HashMap<_, _> = spec.positions.iter().map(|p| (p.id.as_str(), p)).collect();
        let account_days = spec
            .accounts
            .len()
            .checked_mul(spec.horizon_days)
            .ok_or("account day overflow")?;
        if account_days > MAX_ACCOUNT_DAYS {
            return Err("path ledger account-day retention admission exceeded".into());
        }
        let mut work = 0;
        add_work(
            &mut work,
            account_days
                .checked_mul(expected.len())
                .ok_or("run work overflow")?,
            max_work,
        )?;
        let mut bytes = 0;
        let mut run_ids = BTreeSet::new();
        let mut actual = BTreeSet::new();
        for r in &request.runs {
            portfolio_compute_control::checkpoint()?;
            text(&r.run_id, &mut bytes)?;
            text(&r.scenario_id, &mut bytes)?;
            if !r.severity.is_finite()
                || !run_ids.insert(r.run_id.as_str())
                || !expected.contains(&(r.scenario_id.clone(), bits(r.severity)))
                || !actual.insert((r.scenario_id.clone(), bits(r.severity)))
                || r.batches.is_empty()
                || r.batches.len() > MAX_POSITIONS
            {
                return Err("path ledger run identity/severity coverage is incompatible".into());
            }
            let mut seen = BTreeSet::new();
            let mut claims = BTreeSet::new();
            let mut batch_ids = BTreeSet::new();
            for batch in &r.batches {
                text(&batch.batch_id, &mut bytes)?;
                if !batch_ids.insert(batch.batch_id.as_str()) {
                    return Err("duplicate path batch ID".into());
                }
                let q = &batch.request;
                let n = q.bindings.len();
                if q.schema != core::SCHEMA
                    || q.path.measure != CreditMeasure::Physical
                    || q.path.monetary_unit != "currency_units"
                    || q.path.origin_time.ordinal != request.as_of_ordinal
                    || q.path.origin_time.seconds_of_day != 0
                    || q.reporting_end_ordinal != end
                    || n == 0
                    || n > core::MAX_PATHS
                    || q.path.paths.len() != n
                    || batch.account_by_position.len() != n
                    || n.checked_mul(spec.horizon_days)
                        .is_none_or(|v| v > core::MAX_ROWS)
                    || q.path.intervals.len() > 4096
                    || q.path.boundaries.len() > 4096
                {
                    return Err(
                        "path batch measure/clock/coverage/retention admission exceeded".into(),
                    );
                }
                let horizon = q
                    .path
                    .intervals
                    .last()
                    .ok_or("path intervals required")?
                    .end_year;
                if !near(
                    horizon * ACT365_SECONDS_PER_YEAR,
                    spec.horizon_days as f64 * 86400.0,
                ) {
                    return Err("path batch term horizon differs from ledger horizon".into());
                }
                // Only one bounded batch metadata tree exists transiently during compilation.
                metadata(
                    &serde_json::to_value(batch).map_err(|e| e.to_string())?,
                    &mut bytes,
                )?;
                add_work(&mut work, n * spec.horizon_days, max_work)?;
                add_work(
                    &mut work,
                    n.checked_mul(q.path.boundaries.len() + q.path.intervals.len())
                        .ok_or("term work overflow")?,
                    max_work,
                )?;
                for t in &q.path.intervals {
                    for tail in &t.complete_recovery_tails {
                        if tail.len() > 32 {
                            return Err("complete path recovery tail admission exceeded".into());
                        }
                        add_work(
                            &mut work,
                            tail.len().checked_mul(n).ok_or("tail work overflow")?,
                            max_work,
                        )?;
                    }
                }
                let mappings: HashMap<_, _> = batch
                    .account_by_position
                    .iter()
                    .map(|m| (m.position_id.as_str(), m.account_id.as_str()))
                    .collect();
                let paths: HashMap<_, _> = q.path.paths.iter().map(|p| (p.path_key, p)).collect();
                if mappings.len() != n || paths.len() != n {
                    return Err("duplicate path/account mapping".into());
                }
                let mut binding_keys = BTreeSet::new();
                for b in &q.bindings {
                    if !binding_keys.insert(b.path_key) {
                        return Err("duplicate binding path key".into());
                    }
                    let p = positions
                        .get(b.position_id.as_str())
                        .ok_or("unknown path loan anchor")?;
                    let a_id = *mappings
                        .get(b.position_id.as_str())
                        .ok_or("missing path account mapping")?;
                    let a = accounts.get(a_id).ok_or("unknown path account")?;
                    let opening = paths
                        .get(&b.path_key)
                        .ok_or("missing realized path opening")?;
                    if !seen.insert(b.position_id.as_str())
                        || !claims.insert(b.claim_id.as_str())
                        || p.account != a_id
                        || b.currency != a.currency
                        || b.economic_source.id != q.path.source_id
                        || b.economic_source.revision != q.path.source_revision
                        || !near(opening.principal, p.balance)
                        || !near(opening.undrawn_commitment, p.commitment)
                        || !near(opening.accrued_interest, p.opening_accrued)
                        || !near(opening.book_adjustment, p.book_adjustment)
                        || !near(b.opening_allowance, p.allowance)
                    {
                        return Err(
                            "path opening/account/source/claim anchor reconciliation failed".into(),
                        );
                    }
                    let maturities = q
                        .path
                        .boundaries
                        .iter()
                        .filter(|e| matches!(e.action, BoundaryAction::Maturity {}))
                        .collect::<Vec<_>>();
                    // A batch boundary applies to every path in that batch. Heterogeneous maturities require separate batches.
                    if maturities.len() > 1
                        || maturities
                            .iter()
                            .any(|e| !near(e.year * 365.0, p.maturity_day as f64))
                        || (p.maturity_day > 0
                            && p.maturity_day <= spec.horizon_days
                            && maturities.len() != 1)
                    {
                        return Err(
                            "explicit path maturity must match every in-report contractual anchor"
                                .into(),
                        );
                    }
                }
            }
            if seen.len() != positions.len() {
                return Err(
                    "path ledger requires exactly one realized owner for every position per run"
                        .into(),
                );
            }
        }
        Ok(Self {
            spec,
            as_of: request.as_of_ordinal,
            end,
            runs: request.runs,
            admitted_work: work,
            max_work,
        })
    }
}
pub fn validate_json(input: &str, max_positions: usize, max_work: usize) -> Result<String, String> {
    let request: PathLedgerRequest = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let compiled = CompiledPathLedger::compile_with_limits(request, max_positions, max_work)?;
    serde_json::to_string(compiled.normalized_spec()).map_err(|e| e.to_string())
}
fn name(code: usize) -> &'static str {
    [
        "cash",
        "asset_principal",
        "credit_default_principal",
        "accrued_interest",
        "credit_default_accrued_interest",
        "book_adjustment",
        "credit_default_book_adjustment",
        "allowance",
        "interest_income",
        "credit_loss_expense",
        "basis_recognition_expense",
        "recovery_income",
        "equity_opening",
    ][code]
}
fn instrument(code: usize, position: &str, claim: Option<&str>) -> String {
    match code {
        0 => "@cash".into(),
        2 | 4 | 6 => claim.unwrap_or(position).into(),
        12 => "@equity".into(),
        _ => position.into(),
    }
}
struct Sink<W: Write> {
    out: BufWriter<W>,
    rows: BTreeMap<&'static str, Vec<Value>>,
    count: usize,
    transaction: usize,
}
impl<W: Write> Sink<W> {
    fn emit(&mut self, table: &'static str, row: Value) -> Result<(), String> {
        self.rows.entry(table).or_default().push(row);
        self.count += 1;
        if self.count >= 1024 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        for (table, rows) in std::mem::take(&mut self.rows) {
            serde_json::to_writer(&mut self.out, &json!({"table":table,"rows":rows}))
                .map_err(|e| e.to_string())?;
            self.out.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        self.count = 0;
        Ok(())
    }
}
type Gl = BTreeMap<(usize, usize, String), f64>;
#[allow(clippy::too_many_arguments)]
fn post<W: Write>(
    sink: &mut Sink<W>,
    gl: &mut Gl,
    scope: &PathLedgerRun,
    spec: &Spec,
    ai: usize,
    day: usize,
    ordinal: i32,
    key: &str,
    event: &str,
    lines: &[(usize, String, f64)],
    daily: &mut [[f64; 12]],
) -> Result<Option<usize>, String> {
    let sum = lines.iter().map(|x| x.2).sum::<f64>();
    let scale = lines.iter().map(|x| x.2.abs()).sum::<f64>();
    if sum.abs() > 1e-8_f64.max(128.0 * f64::EPSILON * scale) {
        return Err("path ledger transaction unbalanced".into());
    }
    if lines.iter().all(|line| line.2 == 0.0) {
        return Ok(None);
    }
    sink.transaction += 1;
    let transaction = sink.transaction;
    let a = &spec.accounts[ai];
    for (code, instrument, value) in lines {
        money(*value)?;
        if *value == 0.0 {
            continue;
        }
        let balance = gl.entry((ai, *code, instrument.clone())).or_default();
        *balance += value;
        money(*balance)?;
        if day > 0 {
            daily[(day - 1) * spec.accounts.len() + ai][*code] += value;
        }
        sink.emit("journal",json!({"transaction_number":transaction,"transaction_key":key,"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"day":day,"ordinal":ordinal,"account":a.id,"entity":a.entity,"currency":a.currency,"event":event,"gl_account":name(*code),"instrument_id":instrument,"value":value}))?;
    }
    Ok(Some(transaction))
}
/// Stream bounded financially validated batches. Staged rows are not a publication;
/// completion is withheld on any later source/account/cancellation/write failure.
pub fn run<W: Write>(compiled: CompiledPathLedger, output: W) -> Result<(), String> {
    let mut sink = Sink {
        out: BufWriter::new(output),
        rows: BTreeMap::new(),
        count: 0,
        transaction: 0,
    };
    let mut work = compiled.admitted_work;
    for scope in &compiled.runs {
        portfolio_compute_control::checkpoint()?;
        let spec = &compiled.spec;
        let ac = spec.accounts.len();
        let mut gl = Gl::new();
        let mut deltas = vec![[0.0; 12]; ac * spec.horizon_days];
        let mut sources = vec![[0.0; 8]; ac * spec.horizon_days];
        let mut openings = vec![[0.0; 13]; ac];
        let accounts: HashMap<_, _> = spec
            .accounts
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.as_str(), i))
            .collect();
        let mut account_positions = vec![Vec::new(); ac];
        for p in &spec.positions {
            account_positions[accounts[p.account.as_str()]].push(p);
        }
        for (ai, a) in spec.accounts.iter().enumerate() {
            let mut lines = vec![
                (0, "@cash".into(), a.cash),
                (12, "@equity".into(), -a.equity),
            ];
            for p in &account_positions[ai] {
                lines.extend([
                    (1, p.id.to_string(), p.balance),
                    (3, p.id.to_string(), p.opening_accrued),
                    (5, p.id.to_string(), p.book_adjustment),
                    (7, p.id.to_string(), -p.allowance),
                ]);
            }
            for (code, _, v) in &lines {
                openings[ai][*code] += v;
            }
            let key = serde_json::to_string(&("opening", &scope.run_id, &a.id))
                .map_err(|e| e.to_string())?;
            post(
                &mut sink,
                &mut gl,
                scope,
                spec,
                ai,
                0,
                compiled.as_of,
                &key,
                "opening",
                &lines,
                &mut deltas,
            )?;
        }
        for batch in &scope.batches {
            portfolio_compute_control::checkpoint()?;
            // No rows for this financial batch are emitted until all core gates succeed.
            let result = core::evaluate(&batch.request)?;
            add_work(&mut work, result.work_units, compiled.max_work)?;
            let mapping: HashMap<_, _> = batch
                .account_by_position
                .iter()
                .map(|m| {
                    (
                        m.position_id.as_str(),
                        *accounts
                            .get(m.account_id.as_str())
                            .expect("compiled account mapping"),
                    )
                })
                .collect();
            for path in result.paths {
                portfolio_compute_control::checkpoint()?;
                let ai = mapping[path.binding.position_id.as_str()];
                let a = &spec.accounts[ai];
                let position = &path.binding.position_id;
                sink.emit("credit_path_sources",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"batch_id":batch.batch_id,"account":a.id,"entity":a.entity,"currency":a.currency,"position":position,"path_key":path.binding.path_key,"claim_id":path.binding.claim_id,"source_id":result.source_id,"source_revision":result.source_revision,"measure":"physical","monetary_unit":"currency_units","binding":path.binding,"economic":path.economic}))?;
                for (index, timed) in path.intents.iter().enumerate() {
                    if index.is_multiple_of(256) {
                        portfolio_compute_control::checkpoint()?;
                    }
                    let j = &timed.intent;
                    let day = (j.recognition_ordinal - compiled.as_of) as usize;
                    if day == 0 || day > spec.horizon_days {
                        return Err("trusted core journal clock outside ledger".into());
                    }
                    let key = serde_json::to_string(&(
                        "path",
                        &scope.run_id,
                        &batch.batch_id,
                        position,
                        index,
                    ))
                    .map_err(|e| e.to_string())?;
                    let lines = j
                        .postings
                        .iter()
                        .map(|p| {
                            let code = p.account.numeric_code() as usize - 1;
                            (
                                code,
                                instrument(code, position, j.claim_id.as_deref()),
                                p.amount,
                            )
                        })
                        .collect::<Vec<_>>();
                    let number = post(
                        &mut sink,
                        &mut gl,
                        scope,
                        spec,
                        ai,
                        day,
                        j.recognition_ordinal,
                        &key,
                        &j.event,
                        &lines,
                        &mut deltas,
                    )?;
                    sink.emit("credit_path_lineage",json!({"transaction_number":number,"transaction_key":key,"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"batch_id":batch.batch_id,"account":a.id,"entity":a.entity,"currency":a.currency,"position":position,"path_key":path.binding.path_key,"day":day,"ordinal":j.recognition_ordinal,"event":j.event,"claim_id":j.claim_id,"source_id":j.source.id,"source_revision":j.source.revision,"policy_id":j.policy.id,"policy_revision":j.policy.revision,"exact_event":timed.exact_event,"accrual":timed.accrual,"event_ids":timed.event_ids,"contractual_policy_id":timed.contractual_policy_id,"timing":timed.timing}))?;
                }
                for row in &path.daily {
                    let day = (row.recognition_ordinal - compiled.as_of) as usize;
                    let values = [
                        row.live.iter().map(|s| s.principal).sum(),
                        row.default_principal,
                        row.live.iter().map(|s| s.accrued_interest).sum(),
                        row.default_accrued_interest,
                        row.live.iter().map(|s| s.book_adjustment).sum(),
                        row.default_book_adjustment,
                        -row.allowance,
                    ];
                    for (value, total) in values.iter().zip(&mut sources[(day - 1) * ac + ai]) {
                        *total += value;
                        money(*total)?;
                    }
                    let payable = -row
                        .live
                        .iter()
                        .map(|s| s.accrued_interest.min(0.0))
                        .sum::<f64>()
                        - row.default_accrued_interest.min(0.0);
                    sources[(day - 1) * ac + ai][7] += payable;
                    money(sources[(day - 1) * ac + ai][7])?;
                    sink.emit("credit_path_daily",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"batch_id":batch.batch_id,"account":a.id,"entity":a.entity,"currency":a.currency,"position":position,"path_key":path.binding.path_key,"claim_id":path.binding.claim_id,"day":day,"ordinal":row.recognition_ordinal,"state":row}))?;
                }
                for recovery in &path.complete_recoveries {
                    sink.emit("credit_path_recoveries",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"batch_id":batch.batch_id,"account":a.id,"entity":a.entity,"currency":a.currency,"position":position,"path_key":path.binding.path_key,"claim_id":path.binding.claim_id,"source_id":path.binding.recovery_schedule_review.id,"source_revision":path.binding.recovery_schedule_review.revision,"recovery":recovery}))?;
                }
                for claim in &path.closing.claims {
                    sink.emit("credit_path_claims",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"account":a.id,"entity":a.entity,"currency":a.currency,"position":position,"path_key":path.binding.path_key,"ordinal":compiled.end,"claim":claim}))?;
                }
            }
        }
        let mut balances = openings;
        let mut errors = vec![0.0_f64; ac];
        for day in 1..=spec.horizon_days {
            portfolio_compute_control::checkpoint()?;
            for (ai, a) in spec.accounts.iter().enumerate() {
                let row = (day - 1) * ac + ai;
                for (v, d) in balances[ai].iter_mut().take(12).zip(deltas[row]) {
                    *v += d;
                    money(*v)?;
                }
                for (code, value) in sources[row].iter().take(7).enumerate() {
                    let actual = balances[ai][code + 1];
                    errors[ai] = errors[ai].max((actual - value).abs());
                    if !near(actual, *value) {
                        return Err("path ledger daily GL/source reconciliation failed".into());
                    }
                }
                let b = &balances[ai];
                let cash = b[0];
                let net_assets = b[..8].iter().sum::<f64>();
                let liabilities = sources[row][7];
                let assets = net_assets + liabilities;
                let profit = -b[8..12].iter().sum::<f64>();
                let equity = a.equity + profit;
                errors[ai] = errors[ai].max((net_assets - equity).abs());
                if !near(net_assets, equity) {
                    return Err("path ledger daily net asset/equity reconciliation failed".into());
                }
                sink.emit("daily_balance",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"day":day,"ordinal":compiled.as_of+day as i32,"account":a.id,"entity":a.entity,"currency":a.currency,"cash":cash,"assets":assets,"liabilities":liabilities,"net_assets":net_assets,"equity":equity,"allowance":-b[7],"reconciliation_error":errors[ai]}))?;
            }
        }
        let mut final_totals = vec![[0.0; 13]; ac];
        for ((ai, code, instrument), balance) in &gl {
            final_totals[*ai][*code] += balance;
            let a = &spec.accounts[*ai];
            sink.emit("trial_balance",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"account":a.id,"entity":a.entity,"currency":a.currency,"gl_account":name(*code),"instrument_id":instrument,"balance":balance}))?;
        }
        for (ai, a) in spec.accounts.iter().enumerate() {
            for (actual, expected) in final_totals[ai].iter().zip(balances[ai]) {
                if !near(*actual, expected) {
                    return Err("path final instrument/daily account replay differs".into());
                }
            }
            let b = &balances[ai];
            let net_assets = b[..8].iter().sum::<f64>();
            let liabilities = sources[(spec.horizon_days - 1) * ac + ai][7];
            sink.emit("summary",json!({"run_id":scope.run_id,"scenario":scope.scenario_id,"severity":scope.severity,"account":a.id,"entity":a.entity,"currency":a.currency,"final_cash":b[0],"final_assets":net_assets+liabilities,"final_liabilities":liabilities,"final_net_assets":net_assets,"final_equity":a.equity-b[8..12].iter().sum::<f64>(),"max_reconciliation_error":errors[ai]}))?;
        }
    }
    sink.flush()?;
    portfolio_compute_control::checkpoint()?;
    serde_json::to_writer(&mut sink.out, &json!({"complete":true,"protocol":PROTOCOL}))
        .map_err(|e| e.to_string())?;
    sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
    sink.out.flush().map_err(|e| e.to_string())
}
