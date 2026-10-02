//! Transactional raw-book pricing coordinator. The actor invokes Rust pricing
//! directly, then constructs/solves/replays the C++ HiGHS model before staging.
use crate::{library, Record, Session};
#[path = "owned_workflow.rs"]
mod workflow;
use portfolio_risk_native::{decision_inputs, incremental, unit_lifecycle};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Market {
    name: String,
    swap_rates: Vec<f64>,
    vol_quotes: Vec<f64>,
    spread: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    graph: incremental::PriceRequest,
    units: unit_lifecycle::UnitRequest,
    markets: Vec<Market>,
    threads: usize,
    max_bytes: usize,
    max_entries: usize,
}
#[derive(Clone)]
struct State {
    base: Value,
    library: unit_lifecycle::UnitLibrary,
    templates: Vec<unit_lifecycle::NamedTemplate>,
}
pub struct Owner {
    input: Input,
    handle: u64,
    index: HashMap<String, (String, usize)>,
    states: Vec<State>,
    pending: Option<Vec<State>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = incremental::Request::Drop {
            handle: self.handle,
        }
        .run();
    }
}
fn number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| "nonfinite native decision result".into())
}
fn contributions(result: &Value) -> Result<BTreeMap<String, Vec<f64>>, String> {
    let mut out = BTreeMap::new();
    for (book, rows) in result["positions"]
        .as_object()
        .ok_or("missing priced positions")?
    {
        let sign = if book == "mbs" || book == "loans" {
            1.
        } else {
            -1.
        };
        for row in rows.as_array().ok_or("invalid priced positions")? {
            let id = row["id"].as_str().ok_or("missing priced ID")?;
            let mv = number(&row["market_value"])?;
            out.insert(
                format!("{book}:{id}"),
                vec![
                    sign * mv,
                    sign * number(&row["dv01"])?,
                    number(&row["nii_total"])?,
                    if book == "mbs" && !id.starts_with("HL") {
                        mv * 0.85
                    } else {
                        0.
                    },
                    if sign > 0. { mv } else { 0. },
                ],
            );
        }
    }
    Ok(out)
}
fn market_graph(graph: &mut incremental::PriceRequest, market: &Market) {
    graph.current.market.swap_rates = market.swap_rates.clone();
    graph.current.market.vol_quotes = market.vol_quotes.clone();
    if let Some(b) = graph.current.mbs.as_mut() {
        b.request.swap_rates = market.swap_rates.clone();
        b.request.vol_quotes = market.vol_quotes.clone();
    }
    if let Some(a) = graph.auxiliary.as_mut() {
        a.books.market.swap_rates = market.swap_rates.clone();
        a.books.market.vol_quotes = market.vol_quotes.clone();
    }
    graph.spread_shift = market.spread;
}
fn market_units(units: &mut unit_lifecycle::UnitRequest, market: &Market) {
    units.market.swap_rates = market.swap_rates.clone();
    units.market.vol_quotes = market.vol_quotes.clone();
    units.mortgage.swap_rates = market.swap_rates.clone();
    units.mortgage.vol_quotes = market.vol_quotes.clone();
}
fn vectors(lib: &unit_lifecycle::UnitLibrary) -> Vec<Vec<Vec<f64>>> {
    lib.units
        .iter()
        .map(|u| {
            let v = lib
                .vectors
                .iter()
                .find(|v| v.template == u.template && v.purchase_m == u.h)
                .expect("validated unit coefficients");
            v.values
                .chunks_exact(lib.horizon)
                .map(|r| r.to_vec())
                .collect()
        })
        .collect()
}
fn snapshot(states: &[State]) -> Value {
    json!({"bases":states.iter().map(|s|&s.base).collect::<Vec<_>>(),
        "libraries":states.iter().map(|s|json!({"data":s.library,"templates":s.templates})).collect::<Vec<_>>()})
}
fn delta(base: &mut Value, d: &[f64]) -> Result<(), String> {
    fn add(v: &mut Value, d: f64) -> Result<(), String> {
        *v = json!(number(v)? + d);
        Ok(())
    }
    add(&mut base["nii_total_$"], d[2])?;
    add(&mut base["eve"]["eve_$"], d[0])?;
    add(&mut base["eve"]["dv01_net_$"], d[1])?;
    add(&mut base["eve"]["mv_assets_$"], d[4])?;
    add(&mut base["lcr"]["hqla_l2a_uncapped_$"], d[3])?;
    let end = base["capital"]["cet1_path"]
        .as_array_mut()
        .and_then(|a| a.last_mut())
        .ok_or("missing capital path")?;
    add(&mut end["cet1_$"], d[2] * 0.43 * (1. - 0.45))
}
impl Owner {
    pub fn new(request: &mut Value) -> Result<(Self, Session, Value), String> {
        portfolio_compute_control::checkpoint()?;
        let raw = request.get_mut("input").ok_or("missing input")?.take();
        let input: Input = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        if input.markets.is_empty()
            || input.markets.len() > 13
            || input.units.horizon > 120
            || input.units.horizon != input.graph.base.horizon
            || !input.graph.include_analytics
            || input.graph.include_key_rates
        {
            return Err("invalid owned decision scenario grid".into());
        }
        let mut names = std::collections::HashSet::new();
        for market in &input.markets {
            if market.name.is_empty() || !names.insert(&market.name) || !market.spread.is_finite() {
                return Err("invalid decision market".into());
            }
        }
        let mut index = HashMap::new();
        let k = input
            .graph
            .kpis
            .as_ref()
            .ok_or("decision requires KPI inputs")?;
        for (book, rows) in &k.books {
            if !["mbs", "loans", "debt", "deposits", "cds"].contains(&book.as_str()) {
                continue;
            }
            for (i, row) in rows.iter().enumerate() {
                if index
                    .insert(format!("{book}:{}", row.id), (book.clone(), i))
                    .is_some()
                {
                    return Err("duplicate position key".into());
                }
            }
        }
        if index.len() > 200_000 {
            return Err("position limit exceeded".into());
        }
        let handle = incremental::Request::Create {
            max_bytes: input.max_bytes,
            max_entries: input.max_entries,
        }
        .run()?["handle"]
            .as_u64()
            .ok_or("missing graph handle")?;
        let mut owner = Self {
            input,
            handle,
            index,
            states: vec![],
            pending: None,
        };
        let mut records: BTreeMap<String, Vec<Vec<f64>>> = BTreeMap::new();
        let mut scenarios = vec![];
        for market in &owner.input.markets {
            portfolio_compute_control::checkpoint()?;
            let mut graph = owner.input.graph.clone();
            market_graph(&mut graph, market);
            let result = owner.price(graph)?;
            let mut base = result["kpis"].clone();
            base["nii_total_$"] = result["nii"]["total"].clone();
            for (key, metric) in contributions(&result)? {
                records.entry(key).or_default().push(metric);
            }
            let mut units = owner.input.units.clone();
            market_units(&mut units, market);
            let lib =
                portfolio_risk_native::with_compute_threads(owner.input.threads, || units.run())?;
            scenarios.push(json!({"base":library::base(&base)?,"vectors":vectors(&lib)}));
            owner.states.push(State {
                base,
                library: lib,
                templates: units.templates,
            });
        }
        if records.len() != owner.index.len()
            || records.keys().any(|key| !owner.index.contains_key(key))
        {
            return Err("KPI and pricing positions disagree".into());
        }
        let payload = json!({"units":owner.states[0].library.units,"scenarios":scenarios,"constraints":request["constraints"],
            "records":records.into_iter().map(|(key,metrics)|Record {key,metrics}).collect::<Vec<_>>()});
        let session = Session::new(&payload)?;
        let snapshots = snapshot(&owner.states);
        Ok((owner, session, snapshots))
    }
    fn price(&self, request: incremental::PriceRequest) -> Result<Value, String> {
        portfolio_risk_native::with_compute_threads(self.input.threads, || {
            incremental::Request::Price {
                handle: self.handle,
                request: Box::new(request),
            }
            .run()
        })
    }
    pub fn update(&mut self, session: &mut Session, request: Value) -> Result<Value, String> {
        let start = Instant::now();
        let edits: HashMap<String, Value> = crate::decode(&request, "edits")?;
        for (key, patch) in &edits {
            let (book, _) = self.index.get(key).ok_or("unknown instrument")?;
            decision_inputs::validate_patch(book, patch)?;
        }
        let templates: HashMap<String, Value> = crate::decode(&request, "templates")?;
        for patch in templates.values() {
            for (field, v) in patch.as_object().ok_or("invalid template patch")? {
                if field != "spread_bp"
                    || !v.is_null()
                        && v.as_f64()
                            .is_none_or(|x| !x.is_finite() || !(-1000. ..=2000.).contains(&x))
                {
                    return Err("template spread_bp must be finite in [-1000, 2000]".into());
                }
            }
        }
        let version = session.version;
        let mut plan_request = request;
        plan_request["op"] = json!("plan");
        let plan = session.call(plan_request)?;
        let token = plan["token"].clone();
        let result = self.stage(session, &plan, start);
        if result.is_err() {
            self.pending = None;
            session.call(json!({"op":"abort","version":version,"token":token}))?;
        }
        result
    }
    fn stage(
        &mut self,
        session: &mut Session,
        plan: &Value,
        start: Instant,
    ) -> Result<Value, String> {
        let dirty = plan["dirty"].as_array().ok_or("invalid native plan")?;
        let mut selected = decision_inputs::Selection::new();
        let mut patches = BTreeMap::new();
        let mut records: BTreeMap<String, Vec<Vec<f64>>> = BTreeMap::new();
        for item in dirty {
            let key = item["key"].as_str().ok_or("invalid dirty key")?;
            let (book, row) = self.index.get(key).ok_or("unknown dirty key")?;
            selected.entry(book.clone()).or_default().push(*row);
            patches.insert(key.to_string(), item["patch"].clone());
        }
        // Stable saved-row order preserves deterministic aggregation.
        for rows in selected.values_mut() {
            rows.sort_unstable();
        }
        let changes = plan["templates"]
            .as_object()
            .ok_or("invalid template plan")?;
        let changed = !selected.is_empty() || !changes.is_empty();
        let mut states = if changed { self.states.clone() } else { vec![] };
        let mut graphs = vec![];
        let mut columns: BTreeMap<String, Vec<Vec<Vec<Vec<f64>>>>> =
            changes.keys().map(|n| (n.clone(), vec![])).collect();
        for (si, market) in self.input.markets.iter().enumerate() {
            if !selected.is_empty() {
                let mut graph = decision_inputs::select(&self.input.graph, &selected)?;
                decision_inputs::apply(&mut graph.current, &patches)?;
                market_graph(&mut graph, market);
                let result = self.price(graph)?;
                graphs.push(result["graph"].clone());
                for (key, metric) in contributions(&result)? {
                    let old = &session.records[&key].metrics[si];
                    let d: Vec<_> = metric.iter().zip(old).map(|(a, b)| a - b).collect();
                    delta(&mut states[si].base, &d)?;
                    records.entry(key).or_default().push(metric);
                }
            }
            if !changes.is_empty() {
                let mut request = self.input.units.clone();
                market_units(&mut request, market);
                request.templates.retain(|t| changes.contains_key(&t.name));
                for t in &mut request.templates {
                    if let Some(x) = changes[&t.name]["spread_bp"].as_f64() {
                        t.template.spread_bp = x;
                    }
                }
                let part = portfolio_risk_native::with_compute_threads(self.input.threads, || {
                    request.run()
                })?;
                let full = &mut states[si].library;
                let h = full.horizon;
                for (j, u) in part.units.iter().enumerate() {
                    let i = full
                        .units
                        .iter()
                        .position(|v| v.template == u.template && v.h == u.h)
                        .ok_or("unit merge grid mismatch")?;
                    for (a, b) in [
                        (&mut full.nii, &part.nii),
                        (&mut full.runoff, &part.runoff),
                        (&mut full.balance, &part.balance),
                        (&mut full.cash_interest, &part.cash_interest),
                    ] {
                        a[i * h..(i + 1) * h].copy_from_slice(&b[j * h..(j + 1) * h]);
                    }
                    full.dv01[i] = part.dv01[j];
                }
                for updated in &part.vectors {
                    let existing = full
                        .vectors
                        .iter_mut()
                        .find(|v| {
                            v.template == updated.template && v.purchase_m == updated.purchase_m
                        })
                        .ok_or("coefficient merge grid mismatch")?;
                    *existing = updated.clone();
                }
                let v = vectors(full);
                for (name, scenario_columns) in &mut columns {
                    scenario_columns.push(
                        full.units
                            .iter()
                            .enumerate()
                            .filter(|(_, u)| &u.template == name)
                            .map(|(i, _)| v[i].clone())
                            .collect(),
                    );
                }
                for updated in request.templates {
                    let entry = states[si]
                        .templates
                        .iter_mut()
                        .find(|t| t.name == updated.name)
                        .ok_or("missing template")?;
                    *entry = updated;
                }
            }
        }
        portfolio_compute_control::checkpoint()?;
        let priced = start.elapsed().as_secs_f64() * 1000.;
        let mut candidate=session.call(json!({"op":"stage","version":session.version,"token":plan["token"],
            "records":records.into_iter().map(|(key,metrics)|Record {key,metrics}).collect::<Vec<_>>(),"columns":columns}))?;
        portfolio_compute_control::checkpoint()?;
        if changed {
            for (state, native) in states.iter().zip(
                candidate["bases"]
                    .as_array()
                    .ok_or("missing native bases")?,
            ) {
                let expected =
                    serde_json::to_value(library::base(&state.base)?).map_err(|e| e.to_string())?;
                for (key, v) in expected.as_object().ok_or("invalid base")? {
                    let (a, b) = (number(v)?, number(&native[key])?);
                    if (a - b).abs() > 1e-5 + 1e-10 * b.abs() {
                        return Err("native base-delta replay failed".into());
                    }
                }
            }
            candidate["snapshot"] = snapshot(&states);
            self.pending = Some(states);
        }
        candidate["constraints"] = plan["constraints"].clone();
        candidate["work"] = json!({"positions_repriced":dirty.len(),"templates_rebuilt":changes.len(),"total_positions":self.index.len(),"pricing_graph":graphs});
        candidate["timings_ms"] = json!({"pricing_and_coefficients":priced,"native_solve":start.elapsed().as_secs_f64()*1000.-priced});
        candidate["execution"] = json!({"orchestration":"rust","pricing":"rust","solver":"HiGHS (C++)","publication":"external guard"});
        Ok(candidate)
    }
    pub fn publish(&mut self) {
        if let Some(states) = self.pending.take() {
            self.states = states;
        }
    }
    pub fn abort(&mut self) {
        self.pending = None;
    }
}

/// Standalone raw-book scenario graph, unit construction and transactional solves.
/// This returns candidate results; durable publication remains the caller's job.
fn execute(request: &mut Value) -> Result<(Owner, Session, Vec<Value>), String> {
    let steps = request
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![json!({})]);
    if steps.is_empty() || steps.len() > 128 {
        return Err("owned run requires 1..128 steps".into());
    }
    let (mut owner, mut session, _) = Owner::new(request)?;
    let mut results = vec![];
    for step in steps {
        let mut message = step;
        message["op"] = json!("update_owned");
        message["version"] = json!(session.version);
        if message.get("edits").is_none() {
            message["edits"] = json!({});
        }
        if message.get("templates").is_none() {
            message["templates"] = json!({});
        }
        let mut result = owner.update(&mut session, message)?;
        session.call(json!({"op":"publish","version":session.version,"token":result["token"]}))?;
        owner.publish();
        result
            .as_object_mut()
            .ok_or("invalid native result")?
            .remove("snapshot");
        results.push(result);
    }
    Ok((owner, session, results))
}

fn control(request: &Value) -> Result<portfolio_compute_control::Control, String> {
    let timeout = match request.get("timeout_ms") {
        None => 3_600_000,
        Some(value) => value.as_u64().ok_or("timeout_ms must be an integer")?,
    };
    portfolio_compute_control::Control::new(timeout)
}
pub fn run(mut request: Value) -> Result<Value, String> {
    let control = control(&request)?;
    portfolio_compute_control::scope(Some(control), || {
        let (owner, session, results) = execute(&mut request)?;
        Ok(
            json!({"results":results,"snapshot":snapshot(&owner.states),"version":session.version,
            "execution":{"orchestration":"rust","pricing":"rust","solver":"HiGHS (C++)","durable_publication":false}}),
        )
    })
}

pub fn run_streamed(
    mut request: Value,
    output: Box<dyn std::io::Write + Send>,
) -> Result<(), String> {
    let control = control(&request)?;
    portfolio_compute_control::scope(Some(control), || {
        if request["mode"] == "saved" {
            return workflow::saved(
                request
                    .get_mut("input")
                    .ok_or("missing saved input")?
                    .take(),
                output,
            );
        }
        let (owner, session, results) = execute(&mut request)?;
        workflow::run(
            &owner,
            &session,
            request.get("ledger").ok_or("missing ledger request")?,
            results,
            output,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadlines_do_not_silently_accept_invalid_types() {
        for value in [
            json!(null),
            json!(true),
            json!(-1),
            json!(1.5),
            json!("100"),
            json!(0),
            json!(3_600_001),
        ] {
            assert!(control(&json!({"timeout_ms":value})).is_err());
        }
        assert!(control(&json!({})).is_ok());
        assert!(control(&json!({"timeout_ms":100})).is_ok());
    }
}
