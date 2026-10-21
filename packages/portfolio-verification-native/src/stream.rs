//! The native process stream: a workflow header, bounded table batches, and one completion
//! line that must come last. Lifecycle postings are read from both the lineage rows and the
//! ordered columnar journal, so ordering and ownership are checked against two sources.
use crate::raw;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub gl: String,
    pub instrument: String,
    pub debit: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct LineageIntent {
    pub event_id: String,
    pub ordinal: i64,
    pub intent_index: u64,
    pub postings: Vec<Posting>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub number: u64,
    pub day: i64,
    pub postings: Vec<Posting>,
}
#[derive(Debug, Default)]
pub struct Scenario {
    pub account: String,
    pub plan_key: String,
    pub lineage: Vec<LineageIntent>,
    pub custody: Vec<LineageIntent>,
    /// Ordered `native_lifecycle_intent` transactions from the columnar journal.
    pub transactions: Vec<Transaction>,
    pub trial_balance: BTreeMap<(String, String), f64>,
}
#[derive(Debug)]
pub struct Stream {
    pub header: Value,
    pub scenarios: BTreeMap<String, Scenario>,
    pub lines: usize,
}

fn s<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("stream row field `{key}` missing"))
}
fn f(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("stream row field `{key}` is not finite"))
}
fn i(v: &Value, key: &str) -> Result<i64, String> {
    v.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("stream row field `{key}` missing"))
}

pub fn parse(bytes: &[u8]) -> Result<Stream, String> {
    if bytes.len() > crate::MAX_STREAM_BYTES {
        return Err("stream exceeds the verifier's byte admission".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "stream is not UTF-8")?;
    let mut header = None;
    let mut scenarios: BTreeMap<String, Scenario> = BTreeMap::new();
    let mut lineage_rows: Vec<(String, Value)> = Vec::new();
    // Journal labels are interned once per run: each journal chunk carries only the entries added
    // since the previous chunk, and its indices address the accumulated dictionary.
    let mut dictionary: Vec<String> = Vec::new();
    let mut custody_rows: Vec<(String, Value)> = Vec::new();
    let mut complete = false;
    let mut lines = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        lines += 1;
        if complete {
            return Err("stream continues after its completion line".into());
        }
        let value = raw::parse(line.as_bytes(), crate::MAX_STREAM_BYTES)?;
        if let Some(w) = value.get("workflow") {
            if header.is_some() || lines != 1 {
                return Err("the workflow header must be the first and only header line".into());
            }
            header = Some(w.clone());
            continue;
        }
        if value.get("complete").is_some() {
            if value != serde_json::json!({"complete": true, "protocol": 3}) {
                return Err("completion line differs from the dated protocol".into());
            }
            complete = true;
            continue;
        }
        let table = s(&value, "table")?;
        match table {
            "lifecycle_journal_lineage" | "lifecycle_custody_journal" => {
                for row in value
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("rows missing")?
                {
                    let scenario = s(row, "scenario")?.to_string();
                    let entry = scenarios.entry(scenario.clone()).or_default();
                    let account = s(row, "account")?;
                    let key = s(row, "plan_key")?;
                    if entry.account.is_empty() {
                        entry.account = account.into();
                        entry.plan_key = key.into();
                    } else if entry.account != account || entry.plan_key != key {
                        return Err(
                            "lifecycle rows of one scenario name different accounts or plans"
                                .into(),
                        );
                    }
                    if table == "lifecycle_journal_lineage" {
                        lineage_rows.push((scenario, row.clone()));
                    } else {
                        custody_rows.push((scenario, row.clone()));
                    }
                }
            }
            "trial_balance" => {
                for row in value
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("rows missing")?
                {
                    let instrument = s(row, "instrument_id")?;
                    let gl = s(row, "gl_account")?;
                    if instrument.starts_with(crate::NAMESPACE) || gl == "cash" {
                        let scenario = scenarios
                            .entry(s(row, "scenario")?.to_string())
                            .or_default();
                        if scenario
                            .trial_balance
                            .insert((gl.to_string(), instrument.to_string()), f(row, "balance")?)
                            .is_some()
                        {
                            return Err("duplicate trial balance row".into());
                        }
                    }
                }
            }
            "journal" => {
                for entry in value
                    .get("dictionary")
                    .and_then(Value::as_array)
                    .ok_or("journal dictionary missing")?
                {
                    dictionary.push(
                        entry
                            .as_str()
                            .ok_or("journal dictionary entry is not text")?
                            .to_string(),
                    );
                }
                let columns = value.get("columns").ok_or("journal columns missing")?;
                let col = |name: &str| -> Result<&Vec<Value>, String> {
                    columns
                        .get(name)
                        .and_then(Value::as_array)
                        .ok_or_else(|| format!("journal column `{name}` missing"))
                };
                let label = |v: &Value| -> Result<String, String> {
                    let index = v.as_u64().ok_or("journal label index")? as usize;
                    dictionary
                        .get(index)
                        .cloned()
                        .ok_or_else(|| "journal dictionary index out of range".to_string())
                };
                let (tx, sc, day, ev, gl, id, val) = (
                    col("transaction_number")?,
                    col("scenario")?,
                    col("day")?,
                    col("event")?,
                    col("gl_account")?,
                    col("instrument_id")?,
                    col("value")?,
                );
                let n = tx.len();
                if [sc.len(), day.len(), ev.len(), gl.len(), id.len(), val.len()]
                    .iter()
                    .any(|&m| m != n)
                {
                    return Err("journal columns differ in length".into());
                }
                for k in 0..n {
                    if label(&ev[k])? != "native_lifecycle_intent" {
                        continue;
                    }
                    let scenario = scenarios.entry(label(&sc[k])?).or_default();
                    let number = tx[k].as_u64().ok_or("transaction number")?;
                    let day_k = day[k].as_i64().ok_or("journal day")?;
                    let posting = Posting {
                        gl: label(&gl[k])?,
                        instrument: label(&id[k])?,
                        debit: val[k]
                            .as_f64()
                            .filter(|x| x.is_finite())
                            .ok_or("journal value")?,
                    };
                    match scenario.transactions.last_mut() {
                        Some(t) if t.number == number => {
                            if t.day != day_k {
                                return Err("journal transaction spans days".into());
                            }
                            t.postings.push(posting);
                        }
                        Some(t) if t.number > number => {
                            return Err("journal transactions are not in order".into())
                        }
                        _ => scenario.transactions.push(Transaction {
                            number,
                            day: day_k,
                            postings: vec![posting],
                        }),
                    }
                }
            }
            _ => {}
        }
    }
    if !complete {
        return Err("stream lacks its completion line".into());
    }
    let header = header.ok_or("stream lacks its workflow header")?;
    for (rows, custody) in [(lineage_rows, false), (custody_rows, true)] {
        for (scenario, row) in rows {
            let entry = scenarios.get_mut(&scenario).expect("scenario entry");
            let target = if custody {
                &mut entry.custody
            } else {
                &mut entry.lineage
            };
            let intent_index = row
                .get("source_intent_index")
                .and_then(Value::as_u64)
                .ok_or("intent index")?;
            let posting_index = row
                .get("source_posting_index")
                .and_then(Value::as_u64)
                .ok_or("posting index")?;
            let posting = Posting {
                gl: s(&row, "gl_account")?.into(),
                instrument: s(&row, "instrument_id")?.into(),
                debit: f(&row, "debit_amount")?,
            };
            let event_id = s(&row, "source_event_id")?.to_string();
            let ordinal = i(&row, "ordinal")?;
            match target.last_mut() {
                Some(t) if t.intent_index == intent_index => {
                    if t.event_id != event_id
                        || t.ordinal != ordinal
                        || t.postings.len() as u64 != posting_index
                    {
                        return Err(
                            "lifecycle lineage rows are out of order or inconsistent".into()
                        );
                    }
                    t.postings.push(posting);
                }
                Some(t) if t.intent_index > intent_index => {
                    return Err("lifecycle lineage intents are not in order".into())
                }
                _ => {
                    if posting_index != 0 {
                        return Err(
                            "lifecycle lineage intent does not start at its first posting".into(),
                        );
                    }
                    target.push(LineageIntent {
                        event_id,
                        ordinal,
                        intent_index,
                        postings: vec![posting],
                    });
                }
            }
        }
    }
    Ok(Stream {
        header,
        scenarios,
        lines,
    })
}
