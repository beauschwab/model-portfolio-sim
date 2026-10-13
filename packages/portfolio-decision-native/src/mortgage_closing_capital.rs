//! Artifact proposal: authoritative native raw replay precedes closing capital.
use portfolio_ledger_native::{dated_ledger::CompiledDatedLedger, mortgage_pending_owner as owner};
use portfolio_model_core::{cashflow, credit_accounting::Identity};
use portfolio_risk_native::{treasury, treasury_bridge};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::{Arc, Mutex},
};

pub const PENDING_GLS: &[&str] = treasury_bridge::MORTGAGE_PENDING_ASSET_GLS;
type Key = (String, String, String, String);
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_BRIDGE_WORK: usize = 2_000_000;

fn hash(v: &impl serde::Serialize) -> Result<String, String> {
    fn encode(value: &Value, out: &mut Sha256) {
        fn length(out: &mut Sha256, n: usize) {
            out.update((n as u64).to_be_bytes());
        }
        match value {
            Value::Null => out.update(b"n"),
            Value::Bool(v) => out.update(if *v { b"t" } else { b"f" }),
            Value::Number(v) if v.is_f64() => {
                out.update(b"d");
                out.update(
                    v.as_f64()
                        .expect("finite typed float")
                        .to_bits()
                        .to_be_bytes(),
                );
            }
            Value::Number(v) => {
                out.update(b"i");
                let text = v.to_string();
                length(out, text.len());
                out.update(text.as_bytes());
            }
            Value::String(v) => {
                out.update(b"s");
                length(out, v.len());
                out.update(v.as_bytes());
            }
            Value::Array(v) => {
                out.update(b"a");
                length(out, v.len());
                for item in v {
                    encode(item, out);
                }
            }
            Value::Object(v) => {
                out.update(b"o");
                length(out, v.len());
                let sorted: BTreeMap<_, _> = v.iter().collect();
                for (key, item) in sorted {
                    encode(&Value::String(key.clone()), out);
                    encode(item, out);
                }
            }
        }
    }
    let value = serde_json::to_value(v).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    encode(&value, &mut digest);
    Ok(format!("{:x}", digest.finalize()))
}
fn integer(v: &Value) -> Result<i64, String> {
    v.as_i64().ok_or("exact integer source field".into())
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or("finite source amount".into())
}
fn closed(v: &Value) -> Result<(), String> {
    for ch in v["slots"]
        .as_array()
        .filter(|x| x.len() == 7)
        .ok_or("seven closing slots required")?
    {
        for field in ["principal", "recognized_interest", "basis"] {
            owner::near(number(&ch[field])?, 0.)?;
        }
    }
    for field in ["borrower_principal", "borrower_contract_interest"] {
        for n in v[field]
            .as_array()
            .filter(|x| x.len() == 7)
            .ok_or("seven borrower closing slots required")?
        {
            owner::near(number(n)?, 0.)?;
        }
    }
    for claim in v["claims"]
        .as_array()
        .ok_or("complete closing claims required")?
    {
        for field in ["principal", "recognized_interest", "basis"] {
            owner::near(number(&claim["book"][field])?, 0.)?;
        }
        for field in ["economic_principal", "economic_contract_interest"] {
            owner::near(number(&claim[field])?, 0.)?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Collector {
    bytes: usize,
    work: usize,
    first: bool,
    buffer: Vec<u8>,
    dictionary: Vec<String>,
    balances: BTreeMap<Key, f64>,
    trial: Vec<Value>,
    closing: Vec<Value>,
    mortgage_closing: Vec<Value>,
    current: Option<(String, i64, i64, String, String)>,
    sequences: BTreeMap<String, (i64, i64)>,
    transaction_sum: f64,
    transaction_abs: f64,
    complete: bool,
    journal_rows: usize,
}
impl Collector {
    fn charge(&mut self, n: usize) -> Result<(), String> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|x| *x <= MAX_BRIDGE_WORK)
            .ok_or("closing bridge2M work bound")?;
        portfolio_compute_control::checkpoint()
    }
    fn balanced(&self) -> Result<(), String> {
        if self.transaction_sum.abs() > 1e-9 * self.transaction_abs.max(1.) {
            Err("closing unbalanced ordered journal".into())
        } else {
            Ok(())
        }
    }
    fn line(&mut self) -> Result<(), String> {
        if !self.first {
            self.first = true;
            self.buffer.clear();
            return Ok(());
        }
        if self.complete {
            return Err("output after actual producer completion".into());
        }
        let v: Value = serde_json::from_slice(&self.buffer).map_err(|e| e.to_string())?;
        self.buffer.clear();
        self.charge(1)?;
        let Some(table) = v["table"].as_str() else {
            if v != json!({"complete":true,"protocol":3}) || !v["protocol"].is_u64() {
                return Err("actual exact completion required".into());
            }
            self.balanced()?;
            self.complete = true;
            return Ok(());
        };
        if table == "journal" {
            for s in v["dictionary"]
                .as_array()
                .ok_or("journal dictionary missing")?
            {
                self.dictionary
                    .push(s.as_str().ok_or("journal dictionary label")?.into());
            }
            let columns = v["columns"].as_object().ok_or("journal columns missing")?;
            let length = columns["value"]
                .as_array()
                .ok_or("journal values missing")?
                .len();
            for i in 0..length {
                self.charge(16)?;
                let text = |field: &str| -> Result<String, String> {
                    let index = integer(&columns[field][i])?;
                    self.dictionary
                        .get(usize::try_from(index).map_err(|e| e.to_string())?)
                        .cloned()
                        .ok_or("journal dictionary reference".into())
                };
                let scenario = text("scenario")?;
                let account = text("account")?;
                let event = text("event")?;
                let gl = text("gl_account")?;
                let instrument = text("instrument_id")?;
                let tx = integer(&columns["transaction_number"][i])?;
                let day = integer(&columns["day"][i])?;
                let identity = (scenario.clone(), tx, day, account.clone(), event);
                if self.current.as_ref() != Some(&identity) {
                    self.balanced()?;
                    let previous = self.sequences.entry(scenario.clone()).or_insert((0, 0));
                    if tx != previous.0 + 1 || day < previous.1 {
                        return Err("closing ordered transaction sequence".into());
                    }
                    *previous = (tx, day);
                    self.current = Some(identity);
                    self.transaction_sum = 0.;
                    self.transaction_abs = 0.;
                }
                let amount = number(&columns["value"][i])?;
                self.transaction_sum += amount;
                self.transaction_abs += amount.abs();
                self.journal_rows += 1;
                *self
                    .balances
                    .entry((scenario, account, gl, instrument))
                    .or_default() += amount;
            }
        } else if ["trial_balance", "closing_statements", "mortgage_closing"].contains(&table) {
            let rows = v["rows"].as_array().ok_or("closing table rows missing")?;
            self.charge(rows.len().checked_mul(32).ok_or("closing work overflow")?)?;
            match table {
                "trial_balance" => self.trial.extend(rows.iter().cloned()),
                "closing_statements" => self.closing.extend(rows.iter().cloned()),
                _ => self.mortgage_closing.extend(rows.iter().cloned()),
            }
        }
        if self.balances.len() > 10000
            || self.trial.len() > 10000
            || self.mortgage_closing.len() > 4096
        {
            return Err("closing retained key bound".into());
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Sink(Arc<Mutex<Collector>>);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut c = self.0.lock().map_err(|e| io::Error::other(e.to_string()))?;
        c.charge(bytes.len()).map_err(io::Error::other)?;
        c.bytes = c
            .bytes
            .checked_add(bytes.len())
            .filter(|x| *x <= MAX_BYTES)
            .ok_or_else(|| io::Error::other("closing32MiB stream bound"))?;
        for &b in bytes {
            if b == b'\n' {
                c.line().map_err(io::Error::other)?;
            } else if c.first {
                c.buffer.push(b);
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn run_owned(
    t: treasury::Request,
    policies: Vec<treasury_bridge::LedgerPolicy>,
    multiplier: f64,
    raw: owner::Request,
    review: Identity,
    exposure_policy: treasury_bridge::ExposurePolicy,
) -> Result<Value, String> {
    exposure_policy.validate()?;
    // Supplied policy domains refuse before any source projection/replay.
    treasury_bridge::validate_reviewed_mortgage_inputs(
        &t,
        &policies,
        multiplier,
        &exposure_policy,
    )?;
    for value in [&review.id, &review.revision] {
        if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
            return Err("reviewed closing policy identity required".into());
        }
    }
    if !t.positions.is_empty() || !t.curves.is_empty() || !t.strategy_units.is_empty() {
        return Err("closing capital cannot prepare FTP or strategy positions".into());
    }
    if policies.len() > 16
        || policies
            .iter()
            .any(|p| p.gl_risk_weights.len() + p.instrument_risk_weights.len() > 128)
    {
        return Err("closing reviewed policy/map admission".into());
    }
    let dated = CompiledDatedLedger::compile(raw.dated.clone(), 512, owner::MAX_WORK)?;
    let end = raw
        .dated
        .dated
        .as_of_ordinal
        .checked_add(dated.spec.horizon_days as i32)
        .ok_or("closing clock overflow")?;
    let (y, m, d) = cashflow::ymd_from_ordinal(end)?;
    if t.as_of != format!("{y:04}-{m:02}-{d:02}") {
        return Err("capital date must equal authoritative full tail".into());
    }
    let accounts: BTreeMap<_, _> = dated
        .spec
        .accounts
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    for p in &policies {
        let a = accounts
            .get(p.account.as_str())
            .ok_or("missing authoritative capital account")?;
        if p.capital.currency != a.currency
            || p.capital.entity != a.entity
            || p.capital.scenario != p.scenario
        {
            return Err("capital account entity/currency/scenario identity mismatch".into());
        }
    }
    // Admission precedes source generation; the bridge has a separate2M ceiling.
    let audit = owner::audit(&raw, &dated)?;
    let mut audits = BTreeMap::new();
    for s in &raw.sources {
        let a = portfolio_model_core::mortgage_pending_accounting::audit(
            &s.request,
            &mut portfolio_compute_control::checkpoint,
        )?;
        audits.insert(s.source_id.as_str(), a);
    }
    let policy_work = policies.len() * 13 * 64
        + policies
            .iter()
            .map(|p| (p.gl_risk_weights.len() + p.instrument_risk_weights.len()) * 16)
            .sum::<usize>()
        + 64;
    let mut upper = audit
        .max_gl_keys
        .checked_mul(64)
        .and_then(|x| x.checked_add(131072))
        .ok_or("closing extra work overflow")?;
    for o in &raw.owners {
        for run in &o.runs {
            for path in &run.paths {
                // <=32 source lines per compound action,16 operations per journal line,
                // plus closure and trial scans. Source admission charges all source work.
                let n = audits
                    .get(path.source_id.as_str())
                    .ok_or("closing source reference")?
                    .max_groups;
                upper = upper
                    .checked_add(n.checked_mul(1024).ok_or("closing group overflow")?)
                    .ok_or("closing extra work overflow")?;
            }
        }
    }
    if upper > MAX_BRIDGE_WORK {
        return Err("closing bridge pre-generation work admission".into());
    }
    let source_hash = hash(&raw)?;
    let raw_clone = serde_json::to_value(&raw).map_err(|e| e.to_string())?;
    let expected_bindings: BTreeSet<_> = raw
        .owners
        .iter()
        .flat_map(|o| {
            o.runs.iter().flat_map(move |r| {
                r.paths.iter().map(move |p| {
                    (
                        r.scenario_id.clone(),
                        o.position_id.clone(),
                        p.path_id.clone(),
                        p.source_id.clone(),
                    )
                })
            })
        })
        .collect();
    let prepared = owner::prepare(raw)?;
    let source_closings: Vec<_> = prepared
        .owner
        .sources()
        .iter()
        .map(|s| json!({"source_id":s.source_id,"closing":s.native.closing}))
        .collect();
    for source in &source_closings {
        closed(&source["closing"])?;
    }
    let structural_work_bound = upper;
    let upper = MAX_BRIDGE_WORK; // Explicit complete bridge reservation, including byte decoding.
    let c = Arc::new(Mutex::new(Collector::default()));
    c.lock().map_err(|e| e.to_string())?.charge(policy_work)?;
    owner::run(&prepared, Box::new(Sink(c.clone())))?;
    let mut c = c.lock().map_err(|e| e.to_string())?;
    if !c.complete
        || !c.buffer.is_empty()
        || c.trial.len() != c.balances.len()
        || c.mortgage_closing.len() != audit.bindings
    {
        return Err("incomplete authoritative tail/journal coverage".into());
    }
    let mut seen_bindings = BTreeSet::new();
    for closing in &c.mortgage_closing {
        closed(&closing["unweighted_book"])?;
        let k = (
            closing["scenario"]
                .as_str()
                .ok_or("closing scenario")?
                .to_owned(),
            closing["position_id"]
                .as_str()
                .ok_or("closing position")?
                .to_owned(),
            closing["path_id"]
                .as_str()
                .ok_or("closing path")?
                .to_owned(),
            closing["registry_source_id"]
                .as_str()
                .ok_or("closing source")?
                .to_owned(),
        );
        if !seen_bindings.insert(k) {
            return Err("duplicate authoritative source closing".into());
        }
    }
    if seen_bindings != expected_bindings {
        return Err("incomplete authoritative source closing identities".into());
    }

    let mut rows = Vec::new();
    let mut seen_trial = BTreeSet::new();
    for row in &c.trial {
        let key = (
            row["scenario"].as_str().ok_or("trial scenario")?.into(),
            row["account"].as_str().ok_or("trial account")?.into(),
            row["gl_account"].as_str().ok_or("trial GL")?.into(),
            row["instrument_id"]
                .as_str()
                .ok_or("trial instrument")?
                .into(),
        );
        if !seen_trial.insert(key.clone()) {
            return Err("duplicate authoritative trial key".into());
        }
        owner::near(
            number(&row["balance"])?,
            *c.balances.get(&key).ok_or("journal/trial key mismatch")?,
        )?;
        if PENDING_GLS.contains(&key.2.as_str()) {
            owner::near(number(&row["balance"])?, 0.)?;
        }
        rows.push(serde_json::from_value(row.clone()).map_err(|e| e.to_string())?);
    }
    // Existing 64-per-key preflight covers closing/trial plus grouped policy scan.
    let trial_work = c.trial.len() * 64;
    c.charge(policy_work + trial_work)?;
    let mut out = treasury_bridge::prepare_reviewed_mortgage_exposure(
        t,
        rows,
        policies,
        multiplier,
        &exposure_policy,
    )?;
    let bridge = out["capital_bridge"]
        .as_array()
        .ok_or("capital bridge missing")?;
    if bridge.len() != c.closing.len() {
        return Err("native closing scope mismatch".into());
    }
    let mut closing_scopes = BTreeSet::new();
    for closing in &c.closing {
        if !closing_scopes.insert((
            closing["scenario"].clone().to_string(),
            closing["account"].clone().to_string(),
        )) {
            return Err("duplicate authoritative closing scope".into());
        }
        let capital = bridge
            .iter()
            .find(|r| r["scenario"] == closing["scenario"] && r["account"] == closing["account"])
            .ok_or("closing capital scope missing")?;
        for (a, b) in [
            ("assets", "assets"),
            ("liabilities", "liabilities"),
            ("book_equity", "equity"),
        ] {
            owner::near(number(&capital[a])?, number(&closing[b])? * multiplier)?;
        }
    }
    if c.work > upper {
        return Err("closing work exceeded pre-generation bound".into());
    }
    out["mortgage_closing_source"] = json!({"schema":"mortgage-closing-capital-source-1","raw_mixed_sha256":source_hash,"raw_mixed_hash_policy":"typed-json-binary64-identity-1","raw_mixed":raw_clone,"review":review,"full_end_ordinal":end,"source_closings":source_closings,"native_raw_replayed":true,"native_ordered_journal_replayed":true,"journal_rows":c.journal_rows,"source_work_units":audit.work_units,"bridge_work_units":c.work,"bridge_work_bound":upper,"bridge_structural_work_bound":structural_work_bound,"stream_decode_bytes_charged":c.bytes,"bridge_work_limit":MAX_BRIDGE_WORK,"closing_capital_eligible":true,"capital_ftp_eligible":false,"interim_capital_eligible":false,"accounting_standard_approval":false,"empirical_validation":false,"production_adoption":false,"regulatory_credit_rwa_approval":false});
    Ok(out)
}

pub const SCHEMA: &str = "mortgage-closing-capital-1";
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub treasury: treasury::Request,
    pub policies: Vec<treasury_bridge::LedgerPolicy>,
    pub amount_multiplier: f64,
    pub raw_mixed: owner::Request,
    pub closing_review: Identity,
    pub exposure_policy: treasury_bridge::ExposurePolicy,
}
pub fn parse(bytes: &[u8]) -> Result<Request, String> {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("capital request exceeds2MiB".into());
    }
    let raw = strict_json::unique(bytes)?;
    let request: Request = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    if request.schema != SCHEMA {
        return Err("explicit mortgage closing schema required".into());
    }
    Ok(request)
}
pub fn run(request: Request) -> Result<Value, String> {
    if request.schema != SCHEMA {
        return Err("explicit mortgage closing schema required".into());
    }
    portfolio_compute_control::checkpoint()?;
    run_owned(
        request.treasury,
        request.policies,
        request.amount_multiplier,
        request.raw_mixed,
        request.closing_review,
        request.exposure_policy,
    )
}

/// New selected operation re-parses original bytes. The caller's generic peek
/// never supplies the financial body. Historical CLI input admission is 128MiB;
/// this body limit is enforced after that allocation, not an RSS guarantee.
pub fn run_envelope(bytes: &[u8]) -> Result<Value, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Box<serde_json::value::RawValue>,
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_mortgage_closing_capital" || envelope.schema != SCHEMA {
        return Err("explicit mortgage closing envelope required".into());
    }
    if !(1..=1_800_000).contains(&envelope.execution_timeout_ms) {
        return Err("bounded original capital timeout required".into());
    }
    let control = portfolio_compute_control::Control::new(envelope.execution_timeout_ms)?;
    portfolio_compute_control::scope(Some(control), || {
        portfolio_compute_control::checkpoint()?;
        let request = parse(envelope.request.get().as_bytes())?;
        run(request)
    })
}
#[path = "mortgage_closing_capital/strict_json.rs"]
mod strict_json;
