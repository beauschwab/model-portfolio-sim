//! Unapplied opt-in portfolio owner; actual indexed/legacy pricers share one cache.
use crate::{
    graph_cache::{Cache, Key},
    incremental::PriceRequest,
    market_indexed_graph::{self as indexed, IndexedPosition},
    term_deck::{Amortization, Contract, Product},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub const SCHEMA: &str = "market-indexed-books-1";
const BYTES: usize = 128 * 1024 * 1024;
const OUTPUT: usize = 32 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteReview {
    pub id: String,
    pub revision: String,
    pub policy: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub book: String,
    pub position_id: String,
    pub original_row: Contract,
    pub current_row: Contract,
    pub quote_review: QuoteReview,
    pub graph: indexed::Request,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub identity: crate::market_product_source::Identity,
    pub balance_sheet_extras: Option<Value>,
    pub legacy: PriceRequest,
    pub bindings: Option<Vec<Binding>>,
}
pub struct Portfolio {
    original: Request,
    anchor: Arc<Key>,
    positions: Vec<IndexedPosition>,
    cache: Cache,
    limit: usize,
    original_raw: usize,
}
fn checkpoint() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn text(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.contains('\0')
}
fn exact(a: impl Serialize, b: impl Serialize) -> Result<bool, String> {
    Ok(serde_json::to_vec(&a).map_err(|e| e.to_string())?
        == serde_json::to_vec(&b).map_err(|e| e.to_string())?)
}
fn anchor(r: &Request) -> Result<Arc<Key>, String> {
    let mut base = r.clone();
    base.legacy.current = base.legacy.base.clone();
    if let Some(bindings) = &mut base.bindings {
        for b in bindings {
            b.current_row = b.original_row.clone();
            b.graph.source.bridge.current_market = b.graph.source.bridge.original.market.clone();
        }
    }
    Key::new("market-indexed-books-original", base, vec![])
}
fn matching_rows(r: &Request) -> Result<(), String> {
    let l = &r.legacy;
    if r.balance_sheet_extras
        .as_ref()
        .is_some_and(|v| v.as_object().is_none_or(|m| !m.is_empty()))
    {
        return Err("opt-in price-only scope excludes balance sheet extras".into());
    }
    if !text(&r.identity.source_id)
        || !text(&r.identity.source_revision)
        || !text(&r.identity.input_revision)
        || !text(&r.identity.currency)
        || r.identity.model_version != SCHEMA
        || r.identity.as_of_ordinal != l.base.asof
    {
        return Err("portfolio source/measure/currency/asof identity".into());
    }
    l.base.validate()?;
    l.current.validate()?;
    if l.include_analytics
        || l.include_key_rates
        || l.kpis.is_some()
        || l.auxiliary.is_some()
        || l.spread_shift != 0.
        || l.overrides
            .values()
            .flat_map(|v| v.values())
            .any(|v| *v != 0.)
    {
        return Err("opt-in pricing only: analytics/key rates/spread edits unsupported".into());
    }
    if l.base.asof != l.current.asof
        || l.base.horizon != l.current.horizon
        || l.base.terms.len() != l.current.terms.len()
    {
        return Err("portfolio grids/book scope mismatch".into());
    }
    for a in [&l.base, &l.current] {
        if a.mbs.is_some()
            || a.deposits.is_some()
            || a.mm.is_some()
            || a.hedges.is_some()
            || !a.anchors.is_empty()
            || a.forecast.is_some()
            || a.draws.is_some()
            || a.terms
                .iter()
                .any(|b| b.book_yields.is_some() || !matches!(b.key.as_str(), "loans" | "debt"))
        {
            return Err(
                "initial opt-in scope: loans/debt price-only without accounting owners".into(),
            );
        }
    }
    let mut all = BTreeSet::new();
    for (a, b) in l.base.terms.iter().zip(&l.current.terms) {
        if a.key != b.key || a.ids != b.ids {
            return Err("portfolio ordered IDs mismatch".into());
        }
        for id in &a.ids {
            if !text(id) || !all.insert((a.key.clone(), id.clone())) {
                return Err("portfolio duplicate/invalid position".into());
            }
        }
    }
    let bindings = r.bindings.as_ref().ok_or("missing opt-in bindings")?;
    let mut seen = BTreeSet::new();
    let mut prior_index = None;
    for binding in bindings {
        checkpoint()?;
        if binding.book != "loans"
            || !seen.insert(binding.position_id.clone())
            || !text(&binding.quote_review.id)
            || !text(&binding.quote_review.revision)
            || binding.quote_review.policy != "original_native_pv_quote_reconciliation-1"
        {
            return Err("binding book/unique ID/quote review policy".into());
        }
        let i = l
            .base
            .terms
            .iter()
            .position(|b| b.key == binding.book)
            .ok_or("binding book missing")?;
        let a = &l.base.terms[i];
        let b = &l.current.terms[i];
        let j = a
            .ids
            .iter()
            .position(|id| id == &binding.position_id)
            .ok_or("binding position missing")?;
        if prior_index.is_some_and(|old| j <= old) {
            return Err("binding order must follow raw loan rows".into());
        }
        prior_index = Some(j);
        if a.deck.product != Product::Corporate
            || !exact(&binding.original_row, &a.deck.contracts[j])?
            || !exact(&binding.current_row, &b.deck.contracts[j])?
        {
            return Err("complete typed raw row binding mismatch".into());
        }
        let mut current = binding.current_row.clone();
        current.price = binding.original_row.price;
        if !exact(&current, &binding.original_row)? {
            return Err(
                "current contract edits require original rebuild; quote-only row edit supported"
                    .into(),
            );
        }
        let c = &binding.original_row;
        let bridge = &binding.graph.source.bridge;
        let dc = serde_json::to_value(c.daycount).map_err(|e| e.to_string())?;
        let expected_dc = if bridge.contract.index_day_count == "actual360" {
            "ACT/360"
        } else {
            "ACT/365F"
        };
        if binding.graph.product != "indexed_loan"
            || bridge.contract.contract_id != binding.position_id
            || c.maturity != bridge.contract.maturity_ordinal
            || c.notional != bridge.contract.notional
            || bridge.identity.as_of_ordinal != l.base.asof
            || c.coupon != bridge.contract.margin
            || bridge.identity.currency != r.identity.currency
            || dc != expected_dc
            || !c.is_float
            || c.amort_type != Amortization::Bullet
            || c.freq_months != Some(binding.graph.source.index_tenor_months as i32)
            || c.call_schedule.is_some()
            || c.put_schedule.is_some()
            || c.sink_schedule.is_some()
            || c.cap != 10.
            || c.floor != -10.
            || c.penalty_months != 0.
            || !c.channel.is_empty()
            || c.ew_mult != 1.
            || c.notional <= 0.
            || !c.price.is_finite()
            || c.price <= 0.
            || !binding.current_row.price.is_finite()
            || binding.current_row.price <= 0.
        {
            return Err(
                "unsupported bound bullet asset terms/actual-date source/amount/quote".into(),
            );
        }
    }
    Ok(())
}
fn admit(r: &Request, limit: usize, original_raw: usize) -> Result<(usize, usize), String> {
    checkpoint()?;
    if r.schema != SCHEMA || limit > 16 * 1024 * 1024 {
        return Err("portfolio schema/cache admission".into());
    }
    let bindings = r.bindings.as_ref().ok_or("missing opt-in bindings")?;
    if bindings.is_empty() || bindings.len() > 16 {
        return Err("portfolio binding count admission".into());
    }
    if r.legacy.kpis.is_some()
        || r.legacy.auxiliary.is_some()
        || r.legacy.overrides.len() > 2
        || r.legacy
            .overrides
            .iter()
            .any(|(k, v)| !text(k) || v.len() > 64 || v.keys().any(|id| !text(id)))
    {
        return Err("portfolio early ancillary/override admission".into());
    }
    let mut rows = 0usize;
    let mut max_cells = 0usize;
    for a in [&r.legacy.base, &r.legacy.current] {
        checkpoint()?;
        if a.terms.len() > 2
            || a.mbs.is_some()
            || a.deposits.is_some()
            || a.mm.is_some()
            || a.hedges.is_some()
            || a.forecast.is_some()
            || a.draws.is_some()
            || !a.anchors.is_empty()
            || a.market.tenors.len() > 16
            || a.market.swap_rates.len() > 16
            || a.market.vol_quotes.len() > 48
            || a.market.seed.len() > 32
            || a.withdrawal_parameters.len() > 5
            || a.terms.iter().any(|book| {
                book.ids.len() > 64
                    || book.ids.len() != book.deck.contracts.len()
                    || book.book_yields.is_some()
                    || !text(&book.key)
                    || !text(&book.deck.calendar.name)
                    || book.deck.calendar.extra_holidays.len() > 4096
                    || book.ids.iter().any(|id| !text(id))
                    || book.deck.contracts.iter().any(|c| {
                        c.channel.len() > 256
                            || c.call_schedule.as_ref().is_some_and(|v| v.len() > 64)
                            || c.put_schedule.as_ref().is_some_and(|v| v.len() > 64)
                            || c.sink_schedule.as_ref().is_some_and(|v| v.len() > 64)
                    })
            })
        {
            return Err("portfolio early legacy cardinality/ownership admission".into());
        }
        let c = &a.market.config;
        if c.paths == 0
            || c.paths > 8
            || c.months == 0
            || c.months > 360
            || c.forwards == 0
            || c.forwards > 512
            || c.factors == 0
            || c.factors > 4
        {
            return Err("opt-in legacy grid admission".into());
        }
        let n = a.terms.iter().try_fold(0usize, |v, b| {
            v.checked_add(b.ids.len()).ok_or("row overflow")
        })?;
        if n > 64 {
            return Err("opt-in total row admission".into());
        }
        rows = rows.max(n);
        max_cells = max_cells.max(
            c.paths
                .checked_mul(c.months)
                .and_then(|v| v.checked_mul(c.forwards))
                .ok_or("grid overflow")?,
        );
    }
    let mut indexed_bytes = 0usize;
    let mut indexed_work = 0usize;
    for b in bindings {
        checkpoint()?;
        if [&b.original_row, &b.current_row].iter().any(|c| {
            c.channel.len() > 256
                || c.call_schedule.is_some()
                || c.put_schedule.is_some()
                || c.sink_schedule.is_some()
        }) {
            return Err("bound early unsupported option/cardinality admission".into());
        }
        let (cost, ops) = indexed::preflight(&b.graph, 0, 0)?;
        indexed_bytes = indexed_bytes
            .checked_add(cost)
            .ok_or("joined bytes overflow")?;
        indexed_work = indexed_work
            .checked_add(ops)
            .ok_or("joined work overflow")?;
    }
    matching_rows(r)?;
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw > 2 * 1024 * 1024 {
        return Err("portfolio raw input admission".into());
    }
    let legacy_work = max_cells
        .checked_mul(rows.max(1))
        .and_then(|v| v.checked_mul(128))
        .ok_or("legacy joined work overflow")?;
    let bytes = raw
        .max(original_raw)
        .checked_mul(32)
        .and_then(|v| v.checked_add(limit))
        .and_then(|v| v.checked_add(legacy_work.checked_mul(4)?))
        .and_then(|v| v.checked_add(262144))
        .and_then(|v| v.checked_add(indexed_bytes))
        .ok_or("portfolio capacity overflow")?;
    let work = legacy_work
        .checked_add(
            raw.max(original_raw)
                .checked_mul(64)
                .ok_or("raw work overflow")?,
        )
        .and_then(|v| v.checked_add(indexed_work))
        .ok_or("portfolio work overflow")?;
    if bytes > BYTES || work > 60_000_000 {
        return Err("portfolio aggregate retained/transient/work admission".into());
    }
    Ok((bytes, work))
}
fn unbound(r: &Request) -> Result<PriceRequest, String> {
    let mut out = r.legacy.clone();
    let ids: BTreeSet<_> = r
        .bindings
        .as_ref()
        .ok_or("bindings")?
        .iter()
        .map(|b| b.position_id.as_str())
        .collect();
    if let Some(overrides) = out.overrides.get_mut("loans") {
        overrides.retain(|id, _| !ids.contains(id.as_str()));
    }
    for a in [&mut out.base, &mut out.current] {
        for book in &mut a.terms {
            if book.key == "loans" {
                let retained: Vec<_> = book
                    .ids
                    .iter()
                    .zip(&book.deck.contracts)
                    .filter(|(id, _)| !ids.contains(id.as_str()))
                    .map(|(id, c)| (id.clone(), c.clone()))
                    .collect();
                book.ids = retained.iter().map(|(id, _)| id.clone()).collect();
                book.deck.contracts = retained.into_iter().map(|(_, c)| c).collect();
            }
        }
    }
    Ok(out)
}
impl Portfolio {
    pub fn new(r: &Request, limit: usize, entries: usize) -> Result<Self, String> {
        if entries > 4096 {
            return Err("portfolio cache entry admission".into());
        }
        admit(r, limit, 0)?;
        let positions = r
            .bindings
            .as_ref()
            .ok_or("bindings")?
            .iter()
            .map(|b| IndexedPosition::new_shared(&b.graph))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            original: r.clone(),
            anchor: anchor(r)?,
            positions,
            cache: Cache::new(limit, entries)?,
            limit,
            original_raw: serde_json::to_vec(r).map_err(|e| e.to_string())?.len(),
        })
    }
    pub fn info(&self) -> Value {
        json!({"owner":SCHEMA,"cache":self.cache.info()})
    }
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    pub fn price(&mut self, r: &Request) -> Result<Value, String> {
        let (bytes, work) = admit(r, self.limit, self.original_raw)?;
        if anchor(r)?.as_ref() != self.anchor.as_ref() {
            return Err(
                "BASE_ANCHOR_CHANGED: original portfolio/quote/source/OAS rebuild required".into(),
            );
        }
        // One native cache and native aggregation. Bound rows never enter quarterly Flow.
        let mut out = unbound(r)?.run(&mut self.cache)?;
        let mut source_results = Vec::new();
        let mut bound_rows = BTreeMap::new();
        for (b, position) in r
            .bindings
            .as_ref()
            .ok_or("bindings")?
            .iter()
            .zip(&self.positions)
        {
            checkpoint()?;
            let result = position.price(&mut self.cache, 0, &b.graph)?;
            // This first DTO has no contractual cap/floor engine. It may own a
            // row's default bounds only when every admitted paired coupon stays
            // within them; never silently omit a binding legacy contract option.
            for case in &result.cases {
                for period in &case.indexed_request.periods {
                    checkpoint()?;
                    if let crate::products::term::contracts::indexed::Coupon::Indexed {
                        margin,
                        observations,
                        ..
                    } = &period.coupon
                    {
                        if observations.iter().any(|o| {
                            o.rate + margin < b.original_row.floor
                                || o.rate + margin > b.original_row.cap
                        }) {
                            return Err(
                                "bound contractual caps/floors need an explicit supported owner"
                                    .into(),
                            );
                        }
                    } else {
                        return Err("indexed owner coupon mismatch".into());
                    }
                }
            }
            let original_pv = result.cases[0].selected_pv;
            let target = b.original_row.price * b.original_row.notional / 100.;
            if (original_pv - target).abs() > 1e-9 + 1e-12 * target.abs() {
                return Err(
                    "reviewed original quote conflicts with supplied-OAS original native PV".into(),
                );
            }
            let pv = result.cases[1].selected_pv;
            let oas = b.graph.source.bridge.original.locked_oas;
            bound_rows.insert(b.position_id.clone(),json!({"id":b.position_id,"base_oas_bp":oas*1e4,"applied_oas_bp":oas*1e4,"model_price":100.*pv/b.current_row.notional,"notional":b.current_row.notional,"market_value":pv}));
            source_results.push(result);
        }
        let mut totals = BTreeMap::new();
        for book in &r.legacy.current.terms {
            let legacy = out["positions"][&book.key]
                .as_array()
                .ok_or("legacy rows")?;
            let lookup: BTreeMap<_, _> = legacy
                .iter()
                .map(|v| (v["id"].as_str().unwrap_or(""), v))
                .collect();
            let mut rows = Vec::new();
            let mut total = 0.;
            for id in &book.ids {
                checkpoint()?;
                let row = if book.key == "loans" {
                    bound_rows
                        .get(id)
                        .or_else(|| lookup.get(id.as_str()).copied())
                } else {
                    lookup.get(id.as_str()).copied()
                }
                .ok_or("position owner missing")?
                .clone();
                total += row["market_value"].as_f64().ok_or("native monetary row")?;
                rows.push(row);
            }
            if !total.is_finite() {
                return Err("portfolio total finite domain".into());
            }
            totals.insert(book.key.clone(), total);
            out["positions"][&book.key] = json!(rows);
        }
        let mut net = 0.;
        for (book, total) in &totals {
            net += total * if book == "loans" { 1. } else { -1. };
        }
        out["totals"] = json!(totals);
        out["scope_net_value"] = json!(net);
        out["cache"] = self.cache.info();
        out["graph_scope"] =
            json!("legacy_unbound_only; explicit indexed counts retained per native source");
        out["market_indexed"] = json!({"schema":SCHEMA,"original_request":self.original,"current_request":r,"sources":source_results,"admitted_work":work,"admitted_bytes":bytes,"pricing_only":true,"production_adoption":false,"saved_library_adoption":false,"physical_accounting":false,"joint_dynamics":false,"observed_fixings":false,"calendar_mapping_verified":false,"empirical_validation":false,"accounting_adoption":false,"capital_ftp_eligible":false});
        if serde_json::to_vec(&out).map_err(|e| e.to_string())?.len() > OUTPUT {
            return Err("portfolio output admission".into());
        }
        checkpoint()?;
        Ok(out)
    }
}
// None remains an exact historical call: no new owner, schema gate or limits.
#[cfg(test)]
fn run_optional(r: Request, cache: &mut Cache) -> Result<Value, String> {
    if r.bindings.is_none() {
        r.legacy.run(cache)
    } else {
        Err("opt-in request requires portfolio owner".into())
    }
}
#[cfg(test)]
mod compatibility {
    use super::*;
    #[test]
    fn absent_branch_nonempty_legacy_serialized_identity() {
        let mut value: Value =
            serde_json::from_str(include_str!("fixtures/market_indexed_books.json")).unwrap();
        value["bindings"] = Value::Null;
        let r: Request = serde_json::from_value(value).unwrap();
        let mut a = Cache::new(8 * 1024 * 1024, 1024).unwrap();
        let mut b = Cache::new(8 * 1024 * 1024, 1024).unwrap();
        for _ in 0..2 {
            let old = r.legacy.clone().run(&mut a).unwrap();
            let new = run_optional(r.clone(), &mut b).unwrap();
            assert_eq!(
                serde_json::to_vec(&old).unwrap(),
                serde_json::to_vec(&new).unwrap()
            );
        }
    }
}
