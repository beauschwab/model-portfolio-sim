//! A journal window from complete native acquisition replay, never a checkpoint.
use crate::commercial_ledger as ledger;
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    commercial_continuation as holding, commercial_credit_compiler as owner,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Write, time::Instant};

pub const SCHEMA: &str = "commercial-holding-journal-1";
const MAX_RAW: usize = 2 << 20;
const MAX_OUTPUT: usize = 32 << 20;
const MAX_WORK: usize = 32_000_000; // Existing joined candidate tier; inner ledger retains8M.
const MAX_RETAINED: usize = 128 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub cut_ordinal: i32,
    pub origin_ledger: ledger::Request,
    pub revised_sources: Vec<owner::Request>,
}

#[derive(Debug, Serialize)]
pub struct PrefixProof {
    raw_source: holding::Request,
    completed: owner::ResultData,
    openings: Vec<holding::Opening>,
    cut_year: f64,
    admitted_work_upper: usize,
    retained_bytes_upper: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRow {
    transaction_number: usize,
    scenario: String,
    day: usize,
    account: String,
    event: String,
    gl_account: String,
    instrument_id: String,
    value: f64,
}
#[derive(Debug, Serialize)]
struct WindowRow {
    #[serde(flatten)]
    row: JournalRow,
    origin_transaction_number: Option<usize>,
}
type Key = (String, String, String, String);
type Tables = BTreeMap<String, Vec<Value>>;

#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_source: Value,
    origin_asof_ordinal: i32,
    cut_ordinal: i32,
    cut_day: usize,
    full_end_ordinal: i32,
    source_receipt: ledger::Receipt,
    prefix_proofs: Vec<PrefixProof>,
    full_tables: Tables,
    window_tables: Tables,
    producer_completion: Value,
    admitted_work_upper: usize,
    retained_bytes_upper: usize,
    complete_origin_daily_sim_adoption: bool,
    original_credit_clock_restarted: bool,
    second_acquisition_posted: bool,
    prior_publication_verified: bool,
    cut_started_mixed_sim_adoption: bool,
    secondary_acquisition_adoption: bool,
    empirical_validation: bool,
    scope: &'static str,
}

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("holding journal admission overflow".into())
}
fn amount(value: f64) -> Result<f64, String> {
    if value.is_finite() && value.abs() <= 1e15 {
        Ok(value)
    } else {
        Err("holding journal monetary domain".into())
    }
}
fn get<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row[key]
        .as_str()
        .ok_or_else(|| format!("holding native {key} absent"))
}
fn key(row: &JournalRow) -> Key {
    (
        row.scenario.clone(),
        row.account.clone(),
        row.gl_account.clone(),
        row.instrument_id.clone(),
    )
}

/// Decode only the output of the same native Sim; no caller stream is accepted.
fn decode(data: &[u8], reserved_bytes: usize) -> Result<(Tables, Value), String> {
    decode_selected(data, reserved_bytes, 6)
}
pub(crate) fn decode_selected(
    data: &[u8],
    reserved_bytes: usize,
    protocol: u32,
) -> Result<(BTreeMap<String, Vec<Value>>, Value), String> {
    if data.len() > reserved_bytes || data.len() > MAX_OUTPUT {
        return Err("holding stream exceeds preflight reservation".into());
    }
    let mut tables = Tables::new();
    let mut dictionary = Vec::<String>::new();
    let mut completion = None;
    for line in data.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        checkpoint()?;
        if completion.is_some() {
            return Err("holding data follows native completion".into());
        }
        let row = strict_json::parse_checked(line, MAX_OUTPUT, checkpoint)?;
        if let Some(table) = row.get("table").and_then(Value::as_str) {
            if table == "journal" {
                dictionary.extend(
                    serde_json::from_value::<Vec<String>>(row["dictionary"].clone())
                        .map_err(|e| e.to_string())?,
                );
                let cols = row["columns"]
                    .as_object()
                    .ok_or("holding native journal columns absent")?;
                let count = cols
                    .get("value")
                    .and_then(Value::as_array)
                    .ok_or("holding native money absent")?
                    .len();
                if cols.len() != 8
                    || cols
                        .values()
                        .any(|v| v.as_array().is_none_or(|v| v.len() != count))
                {
                    return Err("holding native journal shape".into());
                }
                for i in 0..count {
                    checkpoint()?;
                    let mut decoded = serde_json::Map::new();
                    for (k, values) in cols {
                        let value = &values[i];
                        let v = if matches!(
                            k.as_str(),
                            "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                        ) {
                            let index =
                                usize::try_from(value.as_u64().ok_or("holding dictionary index")?)
                                    .map_err(|_| "holding dictionary overflow")?;
                            json!(dictionary
                                .get(index)
                                .ok_or("holding dictionary entry absent")?)
                        } else {
                            value.clone()
                        };
                        decoded.insert(k.clone(), v);
                    }
                    let value = Value::Object(decoded);
                    let _: JournalRow =
                        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                    tables.entry(table.into()).or_default().push(value);
                }
            } else {
                tables.entry(table.into()).or_default().extend(
                    row["rows"]
                        .as_array()
                        .ok_or("holding native table rows absent")?
                        .iter()
                        .cloned(),
                );
            }
        } else if row == json!({"complete":true,"protocol":protocol}) {
            completion = Some(row);
        } else {
            return Err("holding native completion identity".into());
        }
    }
    Ok((
        tables,
        completion.ok_or("holding complete native journal absent")?,
    ))
}

fn window(tables: &Tables, specification: &Value, cut: usize) -> Result<Tables, String> {
    let mut prefix = BTreeMap::<Key, f64>::new();
    let mut journal = Vec::new();
    let mut last = BTreeMap::<String, (usize, usize)>::new();
    for value in tables.get("journal").ok_or("holding full journal absent")? {
        checkpoint()?;
        let row: JournalRow = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        amount(row.value)?;
        if let Some(&(day, tx)) = last.get(&row.scenario) {
            if row.day < day || row.transaction_number < tx {
                return Err("holding full journal is out of order".into());
            }
        }
        last.insert(row.scenario.clone(), (row.day, row.transaction_number));
        if row.day <= cut {
            let k = key(&row);
            let v = prefix.entry(k).or_default();
            *v = amount(*v + row.value)?;
        }
        journal.push(row);
    }
    let scenarios: Vec<_> = std::iter::once("baseline")
        .chain(
            specification["scenarios"]
                .as_array()
                .ok_or("holding scenarios absent")?
                .iter()
                .map(|s| s["name"].as_str().unwrap_or("")),
        )
        .collect();
    let accounts = specification["accounts"]
        .as_array()
        .ok_or("holding accounts absent")?;
    let mut rows = Vec::<WindowRow>::new();
    let mut opening = Vec::new();
    let mut retained = Vec::new();
    let mut closing = BTreeMap::<Key, f64>::new();
    for scenario in scenarios {
        let mut sequence = 0;
        for account in accounts {
            checkpoint()?;
            let account = get(account, "id")?;
            let mut stocks = BTreeMap::<(String, String), f64>::new();
            let mut prior_pnl = 0.;
            for ((s, a, gl, id), value) in &prefix {
                if s != scenario || a != account {
                    continue;
                }
                if gl.starts_with("pnl:") {
                    prior_pnl = amount(prior_pnl + *value)?;
                } else {
                    stocks.insert((gl.clone(), id.clone()), *value);
                }
            }
            let eq = stocks
                .entry(("opening_equity".into(), String::new()))
                .or_default();
            *eq = amount(*eq + prior_pnl)?;
            retained.push(json!({"scenario":scenario,"account":account,"day":cut,"signed_prior_pnl":prior_pnl}));
            if stocks.len() == 1 && stocks.values().all(|v| *v == 0.) {
                continue;
            }
            let signed = stocks.values().sum::<f64>();
            let gross = stocks.values().map(|v| v.abs()).sum::<f64>();
            if !signed.is_finite() || signed.abs() > 1e-9 + 1e-12 * gross {
                return Err("holding derived opening is unbalanced".into());
            }
            sequence += 1;
            for ((gl, id), value) in stocks {
                opening.push(json!({"scenario":scenario,"account":account,"day":cut,"gl_account":gl,"instrument_id":id,"balance":value}));
                closing.insert(
                    (scenario.into(), account.into(), gl.clone(), id.clone()),
                    value,
                );
                rows.push(WindowRow {
                    row: JournalRow {
                        transaction_number: sequence,
                        scenario: scenario.into(),
                        day: cut,
                        account: account.into(),
                        event: "native_holding_opening".into(),
                        gl_account: gl,
                        instrument_id: id,
                        value,
                    },
                    origin_transaction_number: None,
                });
            }
        }
        let mut previous = None;
        for row in journal
            .iter()
            .filter(|r| r.scenario == scenario && r.day > cut)
        {
            checkpoint()?;
            if row.event == "reviewed_opening_snapshot" || row.event == "opening" {
                return Err("holding suffix contains a second acquisition/opening".into());
            }
            if previous != Some(row.transaction_number) {
                sequence += 1;
                previous = Some(row.transaction_number);
            }
            let mut new = row.clone();
            new.transaction_number = sequence;
            let v = closing.entry(key(row)).or_default();
            *v = amount(*v + row.value)?;
            rows.push(WindowRow {
                row: new,
                origin_transaction_number: Some(row.transaction_number),
            });
        }
    }
    let mut result = Tables::new();
    result.insert(
        "journal".into(),
        rows.into_iter()
            .map(|r| serde_json::to_value(r).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?,
    );
    result.insert("opening".into(), opening);
    result.insert("retained_prior_pnl".into(), retained);
    result.insert("trial_balance".into(),closing.into_iter().map(|((s,a,g,i),v)|json!({"scenario":s,"account":a,"gl_account":g,"instrument_id":i,"balance":v})).collect());
    for name in ["commercial_daily", "commercial_closing"] {
        result.insert(
            name.into(),
            tables
                .get(name)
                .ok_or("holding native daily/closing absent")?
                .iter()
                .filter(|r| r["day"].as_u64().is_some_and(|d| d >= cut as u64))
                .cloned()
                .collect(),
        );
    }
    // Every non-P&L closing amount must reproduce the complete-origin native GL.
    for row in tables
        .get("trial_balance")
        .ok_or("holding full trial balance absent")?
    {
        let gl = get(row, "gl_account")?;
        if gl.starts_with("pnl:") || gl == "opening_equity" {
            continue;
        }
        let target = row["balance"]
            .as_f64()
            .ok_or("holding closing amount absent")?;
        let found = result["trial_balance"]
            .iter()
            .find(|r| {
                r["scenario"] == row["scenario"]
                    && r["account"] == row["account"]
                    && r["gl_account"] == row["gl_account"]
                    && r["instrument_id"] == row["instrument_id"]
            })
            .and_then(|r| r["balance"].as_f64())
            .ok_or("holding closing stock lost")?;
        if found.to_bits() != target.to_bits() {
            return Err("holding closing native stock bits changed".into());
        }
    }
    Ok(result)
}

pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    checkpoint()?;
    if portfolio_compute_control::active().is_none()
        || r.schema != SCHEMA
        || r.origin_ledger.commercial.len() != r.revised_sources.len()
        || r.revised_sources.is_empty()
    {
        return Err("explicit controlled complete-origin holding journal required".into());
    }
    let raw_bytes = strict_json::serialized_size(r, MAX_RAW)?;
    let mut revised = r.origin_ledger.clone();
    revised.commercial = r.revised_sources.clone();
    let prepared = ledger::admit(&revised)?;
    let origin = r.origin_ledger.commercial[0].source.current.asof_ordinal;
    let cut = usize::try_from(
        r.cut_ordinal
            .checked_sub(origin)
            .ok_or("holding calendar overflow")?,
    )
    .map_err(|_| "holding cut precedes origin")?;
    if cut == 0 {
        return Err("holding cut must follow original acquisition".into());
    }
    let pairs: Vec<_> = r
        .origin_ledger
        .commercial
        .iter()
        .zip(&r.revised_sources)
        .map(|(a, b)| holding::Request {
            schema: holding::SCHEMA.into(),
            cut_ordinal: r.cut_ordinal,
            completed_source: a.clone(),
            revised_source: b.clone(),
        })
        .collect();
    let admissions = pairs
        .iter()
        .map(holding::admit)
        .collect::<Result<Vec<_>, _>>()?;
    let mut work = add(prepared.work_upper, raw_bytes * 8)?;
    let mut bytes = add(prepared.retained_bytes_upper, raw_bytes * 20)?;
    // Inner source/daily caps remain unchanged. Joined32M covers both complete
    // prefixes plus one genuine Sim; decoded full/window buffers are charged.
    for a in admissions {
        work = add(work, a.work_upper)?;
        bytes = add(bytes, a.retained_bytes_upper)?;
    }
    bytes = add(
        bytes,
        prepared
            .ledger_bytes_upper
            .checked_mul(2)
            .ok_or("holding buffer upper overflow")?,
    )?;
    if work > MAX_WORK || bytes > MAX_RETAINED {
        return Err(format!("holding joined preflight exceeds32M/128MiB before source generation: work={work}, retained_bytes={bytes}"));
    }
    let reserved = prepared.ledger_bytes_upper;
    let mut proofs = Vec::new();
    let mut sources = Vec::new();
    for pair in pairs {
        checkpoint()?;
        let result = holding::evaluate(&pair)?;
        sources.push(result.revised);
        proofs.push(PrefixProof {
            raw_source: pair,
            completed: result.completed,
            openings: result.openings,
            cut_year: result.cut_year,
            admitted_work_upper: result.admitted_work_upper,
            retained_bytes_upper: result.retained_bytes_upper,
        });
    }
    let (receipt, data) = ledger::evaluate_prepared(prepared, sources)?;
    let full = receipt.resolved_specification["horizon_days"]
        .as_u64()
        .ok_or("holding full calendar absent")?;
    let full_end = origin
        .checked_add(i32::try_from(full).map_err(|_| "holding calendar overflow")?)
        .ok_or("holding calendar overflow")?;
    let (tables, completion) = decode(&data, reserved)?;
    drop(data);
    let window = window(&tables, &receipt.resolved_specification, cut)?;
    let result=ResultData {schema:SCHEMA,raw_source:serde_json::to_value(r).map_err(|e|e.to_string())?,origin_asof_ordinal:origin,cut_ordinal:r.cut_ordinal,cut_day:cut,full_end_ordinal:full_end,
        source_receipt:receipt,prefix_proofs:proofs,full_tables:tables,window_tables:window,producer_completion:completion,admitted_work_upper:work,retained_bytes_upper:bytes,
        complete_origin_daily_sim_adoption:true,original_credit_clock_restarted:false,second_acquisition_posted:false,prior_publication_verified:false,cut_started_mixed_sim_adoption:false,secondary_acquisition_adoption:false,empirical_validation:false,
        scope:"one complete-origin native commercial daily Sim with a derived cut opening and ordered prospective journal; prior P&L rolls into opening equity, future source postings and native closing stock bits retained; no caller checkpoint, verified prior-publication opening, cut-started mixed owners, secondary conveyed rights or empirical adoption"};
    strict_json::serialized_size(&result, MAX_OUTPUT)?;
    checkpoint()?;
    Ok(result)
}

/// Original clock remains active through preparation, bounded encoding and writes.
pub fn run_cli_started(
    input: &[u8],
    mut output: impl Write,
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
    let raw = strict_json::parse_checked(input, MAX_RAW, checkpoint)?;
    let original = raw["request"].clone();
    let envelope: Envelope = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_commercial_holding_journal" || envelope.schema != SCHEMA {
        return Err("explicit holding journal operation required".into());
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
        let mut revised = original["origin_ledger"].clone();
        revised["commercial"] = original["revised_sources"].clone();
        result.source_receipt.original_request = revised;
        result.raw_source = original;
        #[derive(Serialize)]
        struct Success<'a> {
            ok: &'a ResultData,
        }
        let bytes = strict_json::encode(&Success { ok: &result }, MAX_OUTPUT)?;
        for chunk in bytes.chunks(32768) {
            checkpoint()?;
            output.write_all(chunk).map_err(|e| e.to_string())?;
        }
        output
            .write_all(b"\n")
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())?;
        checkpoint()
    })
}
