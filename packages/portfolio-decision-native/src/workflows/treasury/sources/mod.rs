//! Research native source composition. Reviewed funding coefficients confer no cash authority.
mod admission;
mod preparation;
pub use admission::{
    admit, charge_states, conversion_work, path_capacity, preparation_bound, preparation_limit,
};
mod deposit_channels;
mod deposit_continuation_channels;
mod deposit_portfolio_channels;
use deposit_channels::deposit_path;
use deposit_continuation_channels::deposit_continuation_path;
use deposit_portfolio_channels::{deposit_portfolio_path, portfolio_source, rebuild_portfolio};
use portfolio_ledger_native::deposit_source_portfolio as portfolio;
use portfolio_ledger_native::deposit_source_principal as continuation;
mod product_channels;
use product_channels::product_path;
mod mortgage_channels;
use crate::workflows::mixed::product_credit as product;
use mortgage_channels::mortgage_path;
use portfolio_ledger_native::{deposit_revision as deposit, mortgage_pending_owner as mortgage};
use portfolio_model_core::modeled_product_funding as ftp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub const SCHEMA: &str = "modeled-product-ftp-source-proof-1";
pub const MAX_SOURCE_WORK: usize = 48_000_000;
pub const MAX_CAPACITY: usize = 256 * 1024 * 1024;
pub const MAX_OUTPUT: usize = 32 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SourceInput {
    Mortgage(Box<mortgage::Request>),
    Product(Box<product::Request>),
    Deposit(Box<deposit::Request>),
    DepositContinuation(Box<continuation::Request>),
    DepositPortfolio(Box<portfolio::Request>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    pub source_id: String,
    pub scope: String,
    pub scope_review: ftp::Identity,
    pub input: SourceInput,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub source_id: String,
    pub owner_index: usize,
    pub run_index: usize,
    pub path_index: usize,
    pub entity: String,
    pub currency: String,
    pub policy: ftp::Policy,
    /// Explicit analysis convention. Cash is retained, never presumed to fund selected channels.
    pub cash_assignment: String,
    pub cash_exclusion_review: ftp::Identity,
    pub channel_change_review: ftp::Identity,
    /// A complete reviewed map for source P&L labels, excluding mandatory original-EIR ownership.
    pub other_income: BTreeMap<String, String>,
    pub income_review: ftp::Identity,
    pub resets: Vec<ftp::Reset>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub sources: Vec<SourceEntry>,
    pub reviews: Vec<Review>,
}
#[derive(Serialize)]
pub struct SourceReceipt {
    pub source_id: String,
    pub raw_native_result: Value,
    pub work_bound: usize,
    pub capacity_bound: usize,
}
#[derive(Serialize)]
pub struct PreparedPath {
    pub source_id: String,
    pub owner_index: usize,
    pub run_index: usize,
    pub path_index: usize,
    pub position_id: String,
    pub account_id: String,
    pub ledger_run_id: String,
    pub opening_earnings: Vec<ftp::Amount>,
    pub opening_source_cash: f64,
    pub preparation: ftp::ResultData,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Value,
    pub sources: Vec<SourceReceipt>,
    pub paths: Vec<PreparedPath>,
    pub source_work_bound: usize,
    pub retained_capacity_bound: usize,
    pub preparation_work: usize,
    pub preparation_work_bound: usize,
    pub joined_work_bound: usize,
    pub normalized_raw_byte_charge: usize,
    pub native_sources_rebuilt: bool,
    pub source_journals_retained: bool,
    pub independent_persisted_verification: bool,
    pub ftp_bridge_adoption: bool,
    pub capital_ftp_eligible: bool,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
}
fn fail<T>(s: &str) -> Result<T, String> {
    Err(s.into())
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("aggregate admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("aggregate admission overflow".into())
}
/// Bounds the actual unchanged primitive formula from admitted raw source shapes.
/// Cash/P&L events and origin inventories are charged even for zero participation.
fn text(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        fail("bounded source/review identity required")
    } else {
        Ok(())
    }
}
fn id(x: &ftp::Identity) -> Result<(), String> {
    text(&x.id)?;
    text(&x.revision)
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or("native finite monetary field required".into())
}
fn num(v: &Value, k: &str) -> Result<f64, String> {
    number(&v[k])
}
fn array<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>, String> {
    v[k].as_array().ok_or(format!("native {k} array required"))
}
fn day(v: &Value, k: &str) -> Result<i32, String> {
    v[k].as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or(format!("native {k} integer required"))
}

/// Complete source-stage cost ceilings, all charged before the first generator.
#[derive(Clone)]
struct Sink(Arc<Mutex<Vec<u8>>>);
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let mut x = self
            .0
            .lock()
            .map_err(|_| io::Error::other("sink poisoned"))?;
        if x.len().checked_add(b.len()).is_none_or(|n| n > MAX_OUTPUT) {
            return Err(io::Error::other("native source output bound"));
        }
        x.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn rebuild(s: &SourceInput) -> Result<Value, String> {
    match s {
        SourceInput::Mortgage(r) => {
            serde_json::to_value(mortgage::evaluate((**r).clone())?).map_err(|e| e.to_string())
        }
        SourceInput::Product(r) => {
            serde_json::to_value(product::evaluate(r)?).map_err(|e| e.to_string())
        }
        SourceInput::Deposit(r) => {
            let p = deposit::prepare((**r).clone())?;
            let x = Arc::new(Mutex::new(Vec::new()));
            deposit::run_public(
                p,
                serde_json::to_value(r).map_err(|e| e.to_string())?,
                Box::new(Sink(x.clone())),
            )?;
            let bytes = x.lock().map_err(|_| "sink poisoned")?;
            let frames = bytes
                .split(|b| *b == b'\n')
                .filter(|b| !b.is_empty())
                .map(serde_json::from_slice::<Value>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let header = &frames[0]["workflow"];
            Ok(
                json!({"native":header["source_receipt"],"prepared_request":header["prepared_request"],"ledger_messages":frames}),
            )
        }
        SourceInput::DepositPortfolio(r) => rebuild_portfolio(r),
        SourceInput::DepositContinuation(r) => {
            let p = continuation::prepare((**r).clone())?;
            let x = Arc::new(Mutex::new(Vec::new()));
            continuation::run_public(
                p,
                serde_json::to_value(r).map_err(|e| e.to_string())?,
                Box::new(Sink(x.clone())),
            )?;
            let bytes = x.lock().map_err(|_| "sink poisoned")?;
            let frames = bytes
                .split(|b| *b == b'\n')
                .filter(|b| !b.is_empty())
                .map(serde_json::from_slice::<Value>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let header = &frames[0]["workflow"];
            Ok(
                json!({"native":header["source_receipt"],"prepared_request":header["prepared_request"],"ledger_messages":frames}),
            )
        }
    }
}
struct PathData {
    source: ftp::Identity,
    anchor: ftp::Identity,
    path: String,
    scenario: String,
    currency: String,
    side: String,
    start: i32,
    end: i32,
    weight: f64,
    face: f64,
    allocated: f64,
    states: Vec<ftp::State>,
    cash: Vec<(i32, String, f64)>,
    earned: Vec<(i32, String, String, f64)>,
}
fn origin(state: &mut ftp::State, origin: &str, index: usize, amount: f64) {
    state.channels[index].amount += amount;
    // Signed zero and exact native residuals retained; no clipping or normalization.
    state.origins.push(ftp::Origin {
        origin_id: format!("{origin}:{}", ftp::LANES[index]),
        source_event_id: state.source_checkpoint_id.clone(),
        channel: ftp::LANES[index].into(),
        amount,
    });
}
fn state(ordinal: i32, event: String) -> ftp::State {
    ftp::State {
        ordinal,
        source_checkpoint_id: event,
        channels: ftp::LANES
            .iter()
            .map(|s| ftp::Amount {
                channel: (*s).into(),
                amount: 0.,
            })
            .collect(),
        origins: Vec::new(),
    }
}
fn right(s: &mut ftp::State, key: &str, a: f64, receivable: usize, payable: usize) {
    origin(s, key, if a < 0. { payable } else { receivable }, a.abs())
}
fn shared(s: &mut ftp::State, core: &Value) -> Result<(), String> {
    for (i, p) in array(core, "live")?.iter().enumerate() {
        let k = format!("live:{i}");
        origin(s, &k, 0, num(p, "principal")?);
        right(s, &k, num(p, "accrued_interest")?, 4, 5);
        origin(s, &k, 8, num(p, "book_adjustment")?);
        origin(s, &k, 16, num(p, "undrawn")?);
    }
    origin(s, "allowance", 17, num(core, "allowance")?);
    for p in array(core, "claims")? {
        let k = p["claim_id"].as_str().ok_or("claim identity required")?;
        origin(s, k, 3, num(p, "outstanding_principal")?);
        right(s, k, num(p, "outstanding_accrued_interest")?, 6, 7);
        origin(s, k, 10, num(p, "outstanding_book_adjustment")?);
        origin(
            s,
            k,
            14,
            array(p, "remaining_recoveries")?
                .iter()
                .map(|r| num(r, "amount"))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .sum(),
        );
    }
    Ok(())
}
fn identity(v: &Value) -> Result<ftp::Identity, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}
fn path_keys(raw: &SourceInput) -> Vec<(usize, usize, usize)> {
    match raw {
        SourceInput::Mortgage(r) => r
            .owners
            .iter()
            .enumerate()
            .flat_map(|(oi, o)| {
                o.runs.iter().enumerate().flat_map(move |(ri, run)| {
                    run.paths
                        .iter()
                        .enumerate()
                        .map(move |(pi, _)| (oi, ri, pi))
                })
            })
            .collect(),
        SourceInput::Product(r) => r
            .product_owners
            .iter()
            .enumerate()
            .flat_map(|(oi, o)| {
                o.runs.iter().enumerate().flat_map(move |(ri, run)| {
                    run.paths
                        .iter()
                        .enumerate()
                        .map(move |(pi, _)| (oi, ri, pi))
                })
            })
            .collect(),
        SourceInput::Deposit(r) => (0..r.source.runs.len()).map(|ri| (0, ri, 0)).collect(),
        SourceInput::DepositContinuation(r) => (0..r.runs.len()).map(|ri| (0, ri, 0)).collect(),
        SourceInput::DepositPortfolio(r) => r
            .positions
            .iter()
            .enumerate()
            .flat_map(|(oi, _)| (0..r.runs.len()).map(move |ri| (oi, ri, 0)))
            .collect(),
    }
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let costs = admit(r)?;
    let mut sources = Vec::new();
    let mut paths = Vec::new();
    let mut preparation_work = 0;
    for (s, &(work, bytes)) in r.sources.iter().zip(&costs) {
        portfolio_compute_control::checkpoint()?;
        let native = rebuild(&s.input)?;
        for (oi, ri, pi) in path_keys(&s.input) {
            let v = r
                .reviews
                .iter()
                .find(|v| {
                    v.source_id == s.source_id
                        && (v.owner_index, v.run_index, v.path_index) == (oi, ri, pi)
                })
                .ok_or("complete source/path review")?;
            let d = match &s.input {
                SourceInput::Mortgage(raw) => mortgage_path(raw, &native, v)?,
                SourceInput::Product(raw) => product_path(raw, &native, v)?,
                SourceInput::Deposit(raw) => deposit_path(raw, &native, v)?,
                SourceInput::DepositContinuation(raw) => {
                    deposit_continuation_path(raw, &native, v)?
                }
                SourceInput::DepositPortfolio(raw) => deposit_portfolio_path(raw, &native, v)?,
            };
            let (preparation, opening_earnings, opening_source_cash) =
                preparation::prepare_path_in_tier(
                    d,
                    v,
                    matches!(s.input, SourceInput::DepositPortfolio(_)),
                )?;
            preparation_work = add(preparation_work, preparation.admitted_work)?;
            if preparation_work > preparation_limit(r) {
                return fail("aggregate FTP preparation work exceeded");
            }
            let (position_id, account_id, ledger_run_id) = match &s.input {
                SourceInput::Mortgage(r) => {
                    let o = &r.owners[v.owner_index];
                    (
                        o.position_id.clone(),
                        o.account_id.clone(),
                        o.runs[v.run_index].ledger_run_id.clone(),
                    )
                }
                SourceInput::Product(r) => {
                    let o = &r.product_owners[v.owner_index];
                    (
                        o.position_id.clone(),
                        o.account_id.clone(),
                        o.runs[v.run_index].ledger_run_id.clone(),
                    )
                }
                SourceInput::Deposit(r) => (
                    r.position_id.clone(),
                    r.account_id.clone(),
                    r.source.runs[v.run_index].run_id.clone(),
                ),
                SourceInput::DepositContinuation(r) => (
                    r.position_id.clone(),
                    r.account_id.clone(),
                    r.runs[v.run_index].run_id.clone(),
                ),
                SourceInput::DepositPortfolio(r) => (
                    r.positions[oi].position_id.clone(),
                    r.positions[oi].account_id.clone(),
                    r.runs[ri].run_id.clone(),
                ),
            };
            paths.push(PreparedPath {
                position_id,
                account_id,
                ledger_run_id,
                source_id: s.source_id.clone(),
                owner_index: v.owner_index,
                run_index: v.run_index,
                path_index: v.path_index,
                opening_earnings,
                opening_source_cash,
                preparation,
            });
        }
        sources.push(SourceReceipt {
            source_id: s.source_id.clone(),
            raw_native_result: native,
            work_bound: work,
            capacity_bound: bytes,
        });
    }
    let result = ResultData {
        schema: SCHEMA,
        raw_request: serde_json::to_value(r).map_err(|e| e.to_string())?,
        sources,
        paths,
        source_work_bound: add(
            costs.iter().try_fold(0, |n, c| add(n, c.0))?,
            conversion_work(r)?,
        )?,
        retained_capacity_bound: add(
            add(
                add(
                    costs.iter().try_fold(0, |n, c| add(n, c.1))?,
                    path_capacity(r)?,
                )?,
                2 * MAX_OUTPUT,
            )?,
            mul(serde_json::to_vec(r).map_err(|e| e.to_string())?.len(), 8)?,
        )?,
        preparation_work,
        normalized_raw_byte_charge: serde_json::to_vec(r).map_err(|e| e.to_string())?.len(),
        preparation_work_bound: preparation_bound(r)?,
        joined_work_bound: add(
            add(
                costs.iter().try_fold(0, |n, c| add(n, c.0))?,
                conversion_work(r)?,
            )?,
            preparation_bound(r)?,
        )?,
        native_sources_rebuilt: true,
        source_journals_retained: true,
        independent_persisted_verification: false,
        ftp_bridge_adoption: false,
        capital_ftp_eligible: false,
        accounting_standard_approval: false,
        empirical_validation: false,
    };
    let x = Arc::new(Mutex::new(Vec::new()));
    serde_json::to_writer(Sink(x), &result).map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()?;
    Ok(result)
}
