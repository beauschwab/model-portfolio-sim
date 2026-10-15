use super::*;
use portfolio_ledger_native::{
    daily,
    dated_ledger::{classify_right, path_rights, CompiledDatedLedger},
    dated_store::{Boundary, Direction},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Collector {
    line: Vec<u8>,
    dictionary: Vec<String>,
    journal: Vec<Journal>,
    tables: BTreeMap<String, Vec<Value>>,
    complete: bool,
    protocol: Option<u32>,
    bytes: usize,
}
impl Collector {
    fn message(&mut self) -> Result<(), String> {
        let value: Value = serde_json::from_slice(&self.line).map_err(|e| e.to_string())?;
        self.line.clear();
        if value["complete"] == true {
            if self.complete || value["protocol"] != 3 {
                return Err("mixed completion protocol mismatch".into());
            }
            self.complete = true;
            self.protocol = Some(3);
            return Ok(());
        }
        if self.complete {
            return Err("mixed messages after completion".into());
        }
        let table = value["table"]
            .as_str()
            .ok_or("mixed native table missing")?;
        if table == "journal" {
            self.dictionary.extend(
                serde_json::from_value::<Vec<String>>(value["dictionary"].clone())
                    .map_err(|e| e.to_string())?,
            );
            let columns = &value["columns"];
            let n = columns["value"]
                .as_array()
                .ok_or("mixed journal values missing")?
                .len();
            for key in [
                "transaction_number",
                "scenario",
                "day",
                "account",
                "event",
                "gl_account",
                "instrument_id",
            ] {
                if columns[key].as_array().is_none_or(|v| v.len() != n) {
                    return Err("mixed native journal column shape mismatch".into());
                }
            }
            for i in 0..n {
                if i.is_multiple_of(256) {
                    portfolio_compute_control::checkpoint()?;
                }
                let index = |key: &str| -> Result<usize, String> {
                    columns[key][i]
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| "mixed native journal integer missing".into())
                };
                let text = |key: &str| -> Result<String, String> {
                    self.dictionary
                        .get(index(key)?)
                        .cloned()
                        .ok_or_else(|| "mixed journal dictionary index missing".into())
                };
                self.journal.push(Journal {
                    transaction_number: index("transaction_number")?,
                    scenario: text("scenario")?,
                    day: index("day")?,
                    account: text("account")?,
                    event: text("event")?,
                    gl_account: text("gl_account")?,
                    instrument_id: text("instrument_id")?,
                    value: columns["value"][i]
                        .as_f64()
                        .ok_or("mixed journal monetary value missing")?,
                });
            }
        } else if [
            "trial_balance",
            "closing_statements",
            "path",
            "dated_rights",
            "event_lineage",
            "acquisition_lineage",
            "exposures",
            "ledger",
            "consumer_daily",
            "consumer_lineage",
            "consumer_acquisition_lineage",
            "product_daily",
            "product_lineage",
            "product_acquisition_lineage",
            "product_rights_daily",
            "product_rights_lineage",
            "product_rights_acquisition_lineage",
        ]
        .contains(&table)
        {
            self.tables.entry(table.into()).or_default().extend(
                value["rows"]
                    .as_array()
                    .ok_or("mixed native table rows absent")?
                    .iter()
                    .cloned(),
            );
        }
        Ok(())
    }
    fn rows(&mut self, key: &str) -> Vec<Value> {
        self.tables.remove(key).unwrap_or_default()
    }
}
#[derive(Clone)]
struct Capture(Arc<Mutex<Collector>>);
impl Write for Capture {
    fn write(&mut self, mut data: &[u8]) -> io::Result<usize> {
        let n = data.len();
        let mut c = self
            .0
            .lock()
            .map_err(|_| io::Error::other("mixed collector poisoned"))?;
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        c.bytes = c
            .bytes
            .checked_add(n)
            .ok_or_else(|| io::Error::other("mixed collector byte overflow"))?;
        if c.bytes > MAX_BYTES / 4 {
            return Err(io::Error::other(
                "mixed native output byte admission exceeded",
            ));
        }
        while !data.is_empty() {
            if let Some(end) = data.iter().position(|b| *b == b'\n') {
                c.line.extend_from_slice(&data[..end]);
                c.message().map_err(io::Error::other)?;
                data = &data[end + 1..];
            } else {
                c.line.extend_from_slice(data);
                break;
            }
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn evaluate(p: Prepared) -> Result<ResultData, String> {
    Ok(evaluate_inner(p, None)?.0)
}
pub(super) fn evaluate_with_consumers(
    p: Prepared,
    request: portfolio_ledger_native::consumer_intent_owner::Request,
) -> Result<(ResultData, ConsumerTables), String> {
    evaluate_inner(p, Some(OwnerRequest::Consumer(request)))
}
enum OwnerRequest {
    Consumer(portfolio_ledger_native::consumer_intent_owner::Request),
    Product(portfolio_ledger_native::product_event_owner::Request),
}
pub(super) fn evaluate_with_products(
    p: Prepared,
    request: portfolio_ledger_native::product_event_owner::Request,
) -> Result<(ResultData, ConsumerTables), String> {
    evaluate_inner(p, Some(OwnerRequest::Product(request)))
}
fn evaluate_inner(
    p: Prepared,
    consumers: Option<OwnerRequest>,
) -> Result<(ResultData, ConsumerTables), String> {
    let compiled = CompiledDatedLedger::compile_with_acquisitions(
        p.combined_dated_request.clone(),
        &p.acquisitions,
        MAX_POSITIONS,
        MAX_WORK,
    )?;
    let reporting_rights = reporting_rights(&compiled, p.reporting_end_ordinal)?;
    let collector = Arc::new(Mutex::new(Collector::default()));
    let (consumer_sources, daily_table, lineage_table, acquisition_table) = match consumers {
        Some(OwnerRequest::Consumer(request)) => {
            let owners = portfolio_ledger_native::consumer_intent_owner::Compiled::compile(
                request, &compiled,
            )?;
            let sources = serde_json::to_value(&owners.sources)
                .map_err(|e| e.to_string())?
                .as_array()
                .ok_or("consumer receipt shape")?
                .clone();
            daily::run_dated_with_consumers(
                compiled,
                owners,
                Box::new(Capture(collector.clone())),
            )?;
            (
                sources,
                "consumer_daily",
                "consumer_lineage",
                "consumer_acquisition_lineage",
            )
        }
        Some(OwnerRequest::Product(request)) => {
            let owners = portfolio_ledger_native::product_event_owner::Compiled::compile(
                request, &compiled,
            )?;
            let sources = serde_json::to_value(&owners.sources)
                .map_err(|e| e.to_string())?
                .as_array()
                .ok_or("product receipt shape")?
                .clone();
            daily::run_dated_with_products(compiled, owners, Box::new(Capture(collector.clone())))?;
            (
                sources,
                "product_daily",
                "product_lineage",
                "product_acquisition_lineage",
            )
        }
        None => {
            daily::run_dated(compiled, Box::new(Capture(collector.clone())))?;
            (
                Vec::new(),
                "consumer_daily",
                "consumer_lineage",
                "consumer_acquisition_lineage",
            )
        }
    };
    let mut c = Arc::try_unwrap(collector)
        .map_err(|_| "mixed collector still retained")?
        .into_inner()
        .map_err(|_| "mixed collector poisoned")?;
    if !c.complete || !c.line.is_empty() {
        return Err("mixed native full-tail output incomplete".into());
    }
    let trial_balance = c
        .rows("trial_balance")
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let daily_balance = c.rows("path");
    let day = (p.reporting_end_ordinal - p.combined_dated_request.dated.as_of_ordinal) as u64;
    let reported_daily_balance = daily_balance
        .iter()
        .filter(|v| v["day"].as_u64() == Some(day))
        .cloned()
        .collect();
    let journal = std::mem::take(&mut c.journal);
    let result=ResultData{schema:SCHEMA,protocol:c.protocol.ok_or("mixed protocol absent")?,complete:c.complete,journal,trial_balance,closing_statements:c.rows("closing_statements"),daily_balance,dated_rights:c.rows("dated_rights"),event_lineage:c.rows("event_lineage"),acquisition_lineage:c.rows("acquisition_lineage"),exposures:c.rows("exposures"),ledger:c.rows("ledger"),reporting_rights,reported_daily_balance,reporting_end_ordinal:p.reporting_end_ordinal,full_end_ordinal:p.full_end_ordinal,preparation:p,independent_persisted_verification:false,dynamic_mixed_acceptance:false,
  scope:"single pooled native full-tail expected dated backbook/candidate GL with reviewed original opening and candidate EIR anchors; supplied dated backbook recognition, separate physical expectations, end-of-day cash constraints; no observed-settlement, accounting-standard, empirical, joint-credit/stochastic or independently persisted verification claim"};
    admission::bounded(&result, MAX_BYTES / 4)?;
    Ok((
        result,
        ConsumerTables {
            rights_daily: c.rows("product_rights_daily"),
            rights_lineage: c.rows("product_rights_lineage"),
            rights_acquisitions: c.rows("product_rights_acquisition_lineage"),
            daily: c.rows(daily_table),
            lineage: c.rows(lineage_table),
            acquisitions: c.rows(acquisition_table),
            sources: consumer_sources,
        },
    ))
}
fn reporting_rights(c: &CompiledDatedLedger, ordinal: i32) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let acquisitions = c
        .acquisitions
        .as_ref()
        .ok_or("mixed acquisition ownership absent")?;
    for (ri, run) in c.store.runs().iter().enumerate() {
        for event in c.store.events().iter().filter(|e| {
            e.run_index.is_none_or(|r| r == ri)
                && !acquisitions.event_pools.contains_key(&(ri, e.index))
        }) {
            portfolio_compute_control::checkpoint()?;
            let owner = &c.store.owners()[event.owner_index];
            let rights =
                c.store
                    .rights_at(&run.run_id, event.index, ordinal, Boundary::AfterEvents)?;
            out.push(json!({"scenario":run.scenario_id,"run_id":run.run_id,"position_id":owner.position_id,"account":owner.account_id,"source_id":owner.source_id,"source_revision":owner.source_revision,"event_id":event.event_id,"ordinal":ordinal,"right_ownership":"original_event_rights-1","signed_interest_right":rights.signed_interest_right,"gross_rights":classify_right(rights.signed_interest_right,rights.cash_settled),"earned_interest":rights.earned_interest_to_date,"cash_interest_settled":rights.cash_interest_settled,"recognized_book_amortization":rights.book_amortization_to_date}));
        }
        for (i, acquisition) in acquisitions
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.run_index == ri)
        {
            portfolio_compute_control::checkpoint()?;
            let owner = &c.store.owners()[acquisition.owner_index];
            let mut earned = 0.;
            let mut paid = 0.;
            let mut book = 0.;
            let mut ids = BTreeSet::new();
            for &event_index in &acquisitions.pool_events[i] {
                portfolio_compute_control::checkpoint()?;
                let event = &c.store.events()[event_index];
                let r =
                    c.store
                        .rights_at(&run.run_id, event.index, ordinal, Boundary::AfterEvents)?;
                earned += r.earned_interest_to_date;
                paid += r.cash_interest_settled;
                book += r.book_amortization_to_date;
                ids.insert(event.event_id.to_string());
            }
            let direction = if acquisition.input.coupon_sign > 0. {
                owner.direction
            } else {
                match owner.direction {
                    Direction::Receipt => Direction::Payment,
                    Direction::Payment => Direction::Receipt,
                }
            };
            out.push(json!({"scenario":run.scenario_id,"run_id":run.run_id,"position_id":owner.position_id,"account":owner.account_id,"source_id":owner.source_id,"source_revision":owner.source_revision,"event_id":acquisition.input.event_id,"path_id":acquisition.input.path_id,"ordinal":ordinal,"right_ownership":"physical_path_net_rights-1","coupon_sign":acquisition.input.coupon_sign,"signed_interest_right":earned-paid,"gross_rights":path_rights(direction,earned-paid),"earned_interest":earned,"cash_interest_settled":paid,"recognized_book_amortization":book,"native_event_ids":ids}));
        }
    }
    Ok(out)
}
