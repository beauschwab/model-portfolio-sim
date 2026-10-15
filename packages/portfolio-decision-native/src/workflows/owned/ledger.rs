//! Bounded accounting capture -> explicit typed ledger mapping -> daily replay.
use super::{Owner, Session};
use portfolio_ledger_native::{
    flow_store::FlowStore,
    types::{Cashflow, Position},
};
use portfolio_risk_native::{
    accounting_lifecycle::AccountingRequest, decision_inputs, ledger_mapping::MappingRequest,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedInput {
    books: AccountingRequest,
    units: Option<portfolio_risk_native::unit_lifecycle::UnitRequest>,
    ledger: Input,
    allocation: Vec<Value>,
    threads: usize,
    materialize_specification: bool,
}

pub(super) fn saved(raw: Value, mut output: Box<dyn Write + Send>) -> Result<(), String> {
    let request: SavedInput = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    request.books.validate()?;
    let input = &request.ledger;
    let mut spec = portfolio_ledger_native::spec::parse_raw(input.specification.clone())?;
    if !spec.positions.is_empty()
        || !spec.cashflows.is_empty()
        || spec.horizon_days != request.books.horizon * 30
    {
        return Err(
            "saved-book specification must match product horizon and omit positions/cashflows"
                .into(),
        );
    }
    if request.books.hedges.as_ref().is_some_and(|h| !h.is_empty()) {
        return Err("saved hedge trades require a trade-to-netting-set cashflow adapter".into());
    }
    let mut flows = FlowStore::new(spec.horizon_days)?;
    let mut seen = HashSet::new();
    let patches = BTreeMap::new();
    // Preserve the reference accounting/posting order, including MM before CDs.
    for key in ["mbs", "loans", "debt", "deposits", "mm", "cds"] {
        let n = match key {
            "mbs" => request.books.mbs.as_ref().map_or(0, |b| b.ids.len()),
            "deposits" => request.books.deposits.as_ref().map_or(0, |b| b.ids.len()),
            "mm" => request.books.mm.as_ref().map_or(0, Vec::len),
            _ => request
                .books
                .terms
                .iter()
                .find(|b| b.key == key)
                .map_or(0, |b| b.ids.len()),
        };
        for first in (0..n).step_by(256) {
            let end = (first + 256).min(n);
            let selection = if key == "mm" {
                BTreeMap::new()
            } else {
                BTreeMap::from([(key.to_string(), (first..end).collect())])
            };
            let mut base = decision_inputs::accounting(&request.books, &selection);
            if key == "mm" {
                base.mm = Some(request.books.mm.as_ref().unwrap()[first..end].to_vec());
            }
            capture(
                request.threads,
                None,
                base,
                &patches,
                input,
                &mut spec.positions,
                &mut flows,
                &mut seen,
            )?;
        }
    }
    if seen.len() != input.position_mapping.len() {
        return Err("saved-book mapping contains extra identities".into());
    }
    if !request.allocation.is_empty() {
        let units = request
            .units
            .ok_or("allocation requires raw unit templates")?;
        if units.horizon != request.books.horizon {
            return Err("candidate horizon mismatch".into());
        }
        let lib = portfolio_risk_native::with_compute_threads(request.threads, || units.run())?;
        let rows = |v: &Vec<f64>| {
            v.chunks_exact(lib.horizon)
                .map(|v| v.to_vec())
                .collect::<Vec<_>>()
        };
        let units: Vec<_> = lib
            .units
            .iter()
            .map(|u| json!({"template":u.template,"h":u.h,"side":u.side}))
            .collect();
        let mapping:MappingRequest=serde_json::from_value(json!({"specification":{},"mapping":input.template_mapping,"amount_scale":input.amount_scale,
            "openings":null,"flows":null,"allocation":request.allocation,"library":{"units":units,"horizon":lib.horizon,
            "runoff":rows(&lib.runoff),"cash_interest":rows(&lib.cash_interest),"nii":rows(&lib.nii)}})).map_err(|e|e.to_string())?;
        append(mapping.run()?, &mut spec.positions, &mut flows)?;
    }
    portfolio_ledger_native::spec::validate(&spec, 62000, 120_000_000)?;
    flows.validate(&spec)?;
    let mut header = json!({"schema":"saved-ledger-1","positions":spec.positions.len(),
        "cashflow_rows":flows.count(),"cashflow_capacity_bytes":flows.bytes(),"orchestration":"rust","durable_publication":false});
    if request.materialize_specification {
        if flows.count() > 250000 {
            return Err("materialized saved-book cashflow budget exceeded".into());
        }
        let mut replay = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
        replay["version"] = json!("balance-stress-2");
        replay["cashflows"] = json!((1..=spec.horizon_days)
            .flat_map(|d| flows.day(d, &spec.positions))
            .collect::<Vec<_>>());
        header["specification"] = replay;
    }
    serde_json::to_writer(&mut output, &json!({"workflow":header})).map_err(|e| e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    portfolio_ledger_native::daily::run_with_flows(spec, Some(flows), output)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    specification: Value,
    position_mapping: BTreeMap<String, Value>,
    template_mapping: BTreeMap<String, Value>,
    amount_scale: f64,
    include_candidate: bool,
}
fn append(
    mapped: Value,
    positions: &mut Vec<Position>,
    flows: &mut FlowStore,
) -> Result<(), String> {
    let rows: Vec<Position> =
        serde_json::from_value(mapped["positions"].clone()).map_err(|e| e.to_string())?;
    if positions.len() + rows.len() > 62000 {
        return Err("workflow position admission exceeded".into());
    }
    let index: HashMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.to_string(), positions.len() + i))
        .collect();
    let cashflows: Vec<Cashflow> =
        serde_json::from_value(mapped["cashflows"].clone()).map_err(|e| e.to_string())?;
    for f in cashflows {
        let i = *index
            .get(&f.position)
            .ok_or("captured cashflow identity mismatch")?;
        flows.push(
            i,
            f.day,
            [
                f.principal,
                f.cash_interest,
                f.accrual_interest,
                f.book_amortization,
            ],
        )?;
    }
    positions.extend(rows);
    Ok(())
}
#[allow(clippy::too_many_arguments)] // Explicit immutable execution and capture destinations.
fn capture(
    threads: usize,
    market: Option<&super::Market>,
    mut base: AccountingRequest,
    patches: &BTreeMap<String, Value>,
    input: &Input,
    positions: &mut Vec<Position>,
    flows: &mut FlowStore,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    base.capture_anchor = true;
    let anchor = portfolio_risk_native::with_compute_threads(threads, || base.run())?;
    let mut current = base;
    current.capture_anchor = false;
    current.capture_cashflows = true;
    current.anchors = anchor.anchors;
    current.deposit_initial_rate = anchor.deposit_initial_rate;
    decision_inputs::apply(&mut current, patches)?;
    // Daily overlays start from the chosen first market scenario; yield anchors
    // always come from the immutable base contracts and base market.
    if let Some(market) = market {
        current.market.swap_rates = market.swap_rates.clone();
        current.market.vol_quotes = market.vol_quotes.clone();
        if let Some(b) = &mut current.mbs {
            b.request.swap_rates = market.swap_rates.clone();
            b.request.vol_quotes = market.vol_quotes.clone();
        }
    }
    let result = portfolio_risk_native::with_compute_threads(threads, || current.run())?;
    let mut mapping = BTreeMap::new();
    for row in &result.instrument_openings {
        let key = format!("{}:{}", row.book, row.id);
        let meta = input
            .position_mapping
            .get(&key)
            .ok_or("incomplete saved-book mapping")?;
        if !seen.insert(key.clone()) {
            return Err("duplicate mapped opening".into());
        }
        mapping.insert(key, meta);
    }
    let request:MappingRequest=serde_json::from_value(json!({"specification":{},"mapping":mapping,"amount_scale":input.amount_scale,
        "openings":result.instrument_openings,"flows":result.instrument_cashflows,"allocation":null,"library":null})).map_err(|e|e.to_string())?;
    append(request.run()?, positions, flows)
}
pub(super) fn run(
    owner: &Owner,
    session: &Session,
    raw: &Value,
    results: Vec<Value>,
    mut output: Box<dyn Write + Send>,
) -> Result<(), String> {
    let input: Input = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    if !input.amount_scale.is_finite() || input.amount_scale <= 0. {
        return Err("invalid amount_scale".into());
    }
    let mut spec = portfolio_ledger_native::spec::parse_raw(input.specification.clone())?;
    if !spec.positions.is_empty() || !spec.cashflows.is_empty() {
        return Err("saved-book specification cannot supply positions or cashflows".into());
    }
    if spec.horizon_days != owner.input.graph.base.horizon * 30 {
        return Err("daily horizon must cover the complete monthly product horizon".into());
    }
    if owner.input.graph.auxiliary.as_ref().is_some_and(|a| {
        a.books.hedges.as_ref().is_some_and(|h| !h.is_empty())
            || a.swaptions.as_ref().is_some_and(|s| !s.is_empty())
    }) {
        return Err("saved hedge trades require a trade-to-netting-set cashflow adapter".into());
    }
    let patches: BTreeMap<_, _> = session
        .patches
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (book, row) in owner.index.values() {
        groups.entry(book.clone()).or_default().push(*row);
    }
    let mut flows = FlowStore::new(spec.horizon_days)?;
    let mut seen = HashSet::new();
    for key in ["mbs", "loans", "debt", "deposits", "mm", "cds"] {
        if key == "mm" {
            if let Some(aux) = &owner.input.graph.auxiliary {
                if let Some(mm) = &aux.books.mm {
                    for chunk in mm.chunks(256) {
                        let mut book = decision_inputs::accounting(&aux.books, &BTreeMap::new());
                        book.mm = Some(chunk.to_vec());
                        capture(
                            owner.input.threads,
                            Some(&owner.input.markets[0]),
                            book,
                            &patches,
                            &input,
                            &mut spec.positions,
                            &mut flows,
                            &mut seen,
                        )?;
                    }
                }
            }
            continue;
        }
        let Some(rows) = groups.get_mut(key) else {
            continue;
        };
        rows.sort_unstable();
        for chunk in rows.chunks(256) {
            let selected = BTreeMap::from([(key.to_string(), chunk.to_vec())]);
            let graph = decision_inputs::select(&owner.input.graph, &selected)?;
            capture(
                owner.input.threads,
                Some(&owner.input.markets[0]),
                graph.base,
                &patches,
                &input,
                &mut spec.positions,
                &mut flows,
                &mut seen,
            )?;
        }
    }
    if seen.len() != input.position_mapping.len() {
        return Err("saved-book mapping contains extra identities".into());
    }
    if input.include_candidate {
        let result = results.last().ok_or("missing candidate")?;
        if result["feasible"] != true {
            return Err("cannot replay an infeasible candidate".into());
        }
        let lib = &owner.states[0].library;
        let rows = |v: &Vec<f64>| {
            v.chunks_exact(lib.horizon)
                .map(|x| x.to_vec())
                .collect::<Vec<_>>()
        };
        let units: Vec<_> = lib
            .units
            .iter()
            .map(|u| json!({"template":u.template,"h":u.h,"side":u.side}))
            .collect();
        let request:MappingRequest=serde_json::from_value(json!({"specification":{},"mapping":input.template_mapping,"amount_scale":input.amount_scale,
            "openings":null,"flows":null,"allocation":result["allocation"],"library":{"units":units,"horizon":lib.horizon,
                "runoff":rows(&lib.runoff),"cash_interest":rows(&lib.cash_interest),"nii":rows(&lib.nii)}})).map_err(|e|e.to_string())?;
        append(request.run()?, &mut spec.positions, &mut flows)?;
    }
    portfolio_ledger_native::spec::validate(&spec, 62000, 120_000_000)?;
    flows.validate(&spec)?;
    serde_json::to_writer(&mut output,&json!({"workflow":{"schema":"owned-ledger-1","version":session.version,
        "results":results,"positions":spec.positions.len(),"cashflow_rows":flows.count(),"cashflow_capacity_bytes":flows.bytes(),
        "pricing_scenario":owner.input.markets[0].name,"orchestration":"rust","solver":"HiGHS (C++)",
        "timing":"30-day reporting months; base product expectations plus explicit daily overlays","durable_publication":false}})).map_err(|e|e.to_string())?;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    portfolio_ledger_native::daily::run_with_flows(spec, Some(flows), output)
}
