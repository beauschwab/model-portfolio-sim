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
    /// Total stream bytes, including line breaks.
    pub bytes: u64,
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

/// The whole stream in memory, within the default 256 MiB bound.
pub fn parse(bytes: &[u8]) -> Result<Stream, String> {
    if bytes.len() > crate::MAX_STREAM_BYTES {
        return Err("stream exceeds the verifier's byte admission".into());
    }
    let mut p = Parser::new(crate::MAX_STREAM_BYTES as u64);
    p.bytes(bytes)?;
    p.finish()
}

/// Incremental parse of the stream in any split of its bytes. Only lifecycle rows, lifecycle
/// trial balance rows and lifecycle journal transactions are retained, so memory follows the
/// lifecycle activity and one bounded line, not the size of the backbook journal.
pub struct Parser {
    max: u64,
    total: u64,
    partial: Vec<u8>,
    header: Option<Value>,
    scenarios: BTreeMap<String, Scenario>,
    lineage_rows: Vec<(String, Value)>,
    custody_rows: Vec<(String, Value)>,
    // Journal labels are interned once per run: each journal chunk carries only the entries added
    // since the previous chunk, and its indices address the accumulated dictionary.
    dictionary: Vec<String>,
    complete: bool,
    lines: usize,
}

impl Parser {
    pub fn new(max_bytes: u64) -> Self {
        Parser {
            max: max_bytes,
            total: 0,
            partial: Vec::new(),
            header: None,
            scenarios: BTreeMap::new(),
            lineage_rows: Vec::new(),
            custody_rows: Vec::new(),
            dictionary: Vec::new(),
            complete: false,
            lines: 0,
        }
    }

    /// Feeds stream bytes; complete lines are parsed as they arrive.
    pub fn bytes(&mut self, mut data: &[u8]) -> Result<(), String> {
        self.total += data.len() as u64;
        if self.total > self.max {
            return Err("stream exceeds the verifier's byte admission".into());
        }
        while let Some(p) = data.iter().position(|b| *b == b'\n') {
            if self.partial.is_empty() {
                self.line(&data[..p])?;
            } else {
                let mut line = std::mem::take(&mut self.partial);
                line.extend_from_slice(&data[..p]);
                self.line(&line)?;
            }
            data = &data[p + 1..];
        }
        if self.partial.len() + data.len() > crate::MAX_LINE_BYTES {
            return Err("a stream line exceeds the verifier's line bound".into());
        }
        self.partial.extend_from_slice(data);
        Ok(())
    }

    fn line(&mut self, raw_line: &[u8]) -> Result<(), String> {
        let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        let text = std::str::from_utf8(raw_line).map_err(|_| "stream is not UTF-8")?;
        if text.trim().is_empty() {
            return Ok(());
        }
        self.lines += 1;
        let lines = self.lines;
        if self.complete {
            return Err("stream continues after its completion line".into());
        }
        let header = &mut self.header;
        let scenarios = &mut self.scenarios;
        let lineage_rows = &mut self.lineage_rows;
        let custody_rows = &mut self.custody_rows;
        let dictionary = &mut self.dictionary;
        let value = raw::parse(text.as_bytes(), crate::MAX_LINE_BYTES)?;
        if let Some(w) = value.get("workflow") {
            if header.is_some() || lines != 1 {
                return Err("the workflow header must be the first and only header line".into());
            }
            *header = Some(w.clone());
            return Ok(());
        }
        if value.get("complete").is_some() {
            if value != serde_json::json!({"complete": true, "protocol": 3}) {
                return Err("completion line differs from the dated protocol".into());
            }
            self.complete = true;
            return Ok(());
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
        Ok(())
    }

    /// Completes the parse: the final line, the completion line and the ordered lineage.
    pub fn finish(mut self) -> Result<Stream, String> {
        if !self.partial.is_empty() {
            let line = std::mem::take(&mut self.partial);
            self.line(&line)?;
        }
        let Parser {
            total,
            header,
            mut scenarios,
            lineage_rows,
            custody_rows,
            complete,
            lines,
            ..
        } = self;
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
                                "lifecycle lineage intent does not start at its first posting"
                                    .into(),
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
            bytes: total,
        })
    }
}
