//! Artifact versioned named-curve graph using real indexed/swap owners and graph cache.
use crate::{
    derivative_contract::swaps::{value_swap, SwapResult},
    graph::cache::{Cache, Key},
    market_indexed_adapter as adapter, market_product_source as bridge,
    term_enhancements::indexed::{build_path, ContractPathResult},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
pub const SCHEMA: &str = "market-indexed-graph-1";
pub const MAX_JOINED_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub product: String,
    pub source: adapter::Request,
}
#[derive(Default, Clone, Serialize)]
pub struct Counts {
    pub computed: usize,
    pub reused: usize,
}
#[derive(Clone, Serialize)]
pub struct Case {
    pub role: String,
    pub source: bridge::Identity,
    pub indexed_request: crate::products::term::contracts::indexed::ContractPathRequest,
    pub indexed_result: ContractPathResult,
    pub swap_request: crate::products::derivatives::contract::swaps::SwapRequest,
    pub swap_result: SwapResult,
    pub original_oas: f64,
    pub spread_adjusted_swap_pv: f64,
    pub indexed_pv: f64,
    pub selected_pv: f64,
}
#[derive(Serialize)]
pub struct Risk {
    pub curve_role: String,
    pub selected_rate_gradient_per_bp: f64,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: String,
    pub original_anchor: Request,
    pub current_request: Request,
    pub cases: Vec<Case>,
    pub risk: Vec<Risk>,
    pub counts: BTreeMap<String, Counts>,
    pub cache: serde_json::Value,
    pub admitted_joined_bytes: usize,
    pub admitted_work: usize,
    pub pricing_only: bool,
    pub pairing_policy: String,
    pub stochastic_crn: bool,
    pub physical_accounting: bool,
    pub production_adoption: bool,
    pub saved_library_adoption: bool,
    pub capital_ftp_eligible: bool,
}
pub struct IndexedPosition {
    original: Request,
    base: Arc<Key>,
    original_raw: usize,
    shared_curves: bool,
}
pub struct Graph {
    position: IndexedPosition,
    cache: Cache,
    limit: usize,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn key(r: &Request) -> Result<Arc<Key>, String> {
    let mut base = r.clone();
    base.source.bridge.current_market = base.source.bridge.original.market.clone();
    Key::new("market-indexed-original", base, vec![])
}
pub(crate) fn preflight(
    r: &Request,
    limit: usize,
    original_raw: usize,
) -> Result<(usize, usize), String> {
    check()?;
    if r.schema != SCHEMA
        || !matches!(r.product.as_str(), "indexed_loan" | "vanilla_swap")
        || limit > 16 * 1024 * 1024
    {
        return Err("graph version/product/cache admission".into());
    }
    if r.source.bridge.contract.periods.is_empty()
        || r.source.bridge.contract.periods.len() > 512
        || [
            &r.source.bridge.original.market,
            &r.source.bridge.current_market,
        ]
        .iter()
        .any(|m| {
            m.curves.is_empty()
                || m.curves.len() > 16
                || m.curves
                    .iter()
                    .any(|c| c.times_years.len() > 4096 || c.discount_factors.len() > 4096)
        })
    {
        return Err("graph early source cardinality admission".into());
    }
    let b = bridge::validate(&r.source.bridge)?;
    let a = adapter::admission(&r.source)?;
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw > bridge::MAX_INPUT_BYTES {
        return Err("graph typed raw byte admission".into());
    }
    let raw = raw.max(original_raw);
    // Charge complete actual product output plus pinned transient projection/current/original
    // materialization and configured cache capacity. No aggregate RSS claim.
    let bytes = a
        .retained_bytes
        .checked_mul(4)
        .and_then(|n| n.checked_add(b.retained_bytes.checked_mul(3)?))
        .and_then(|n| n.checked_add(raw.checked_mul(16)?))
        .and_then(|n| n.checked_add(limit))
        .and_then(|n| n.checked_add(262144))
        .ok_or("graph capacity overflow")?;
    if bytes > MAX_JOINED_BYTES {
        return Err("graph joined retained/transient capacity".into());
    }
    let work = a
        .numerical_work
        .checked_add(b.admitted_work)
        .and_then(|n| n.checked_add(raw.checked_mul(64)?))
        .ok_or("graph joined work overflow")?;
    if work > 60_000_000 {
        return Err("graph joined work admission".into());
    }
    check()?;
    Ok((bytes, work))
}
fn node<T: Clone + Serialize + Send + Sync + 'static>(
    cache: &mut Cache,
    k: Arc<Key>,
    stage: &str,
    counts: &mut BTreeMap<String, Counts>,
    compute: impl FnOnce() -> Result<T, String>,
) -> Result<Arc<T>, String> {
    check()?;
    if let Some(v) = cache.get(&k) {
        counts.entry(stage.into()).or_default().reused += 1;
        return Ok(v);
    }
    let result = compute();
    check()?;
    let v = Arc::new(result?);
    let size = serde_json::to_vec(v.as_ref())
        .map_err(|e| e.to_string())?
        .len()
        .checked_mul(4)
        .ok_or("node bytes overflow")?;
    cache.put(k, v.clone(), size);
    counts.entry(stage.into()).or_default().computed += 1;
    Ok(v)
}
impl Graph {
    pub fn new(r: &Request, cache_bytes: usize, cache_entries: usize) -> Result<Self, String> {
        let position = IndexedPosition::new(r, cache_bytes)?;
        if cache_entries > 4096 {
            return Err("graph entry admission".into());
        }
        Ok(Self {
            position,
            cache: Cache::new(cache_bytes, cache_entries)?,
            limit: cache_bytes,
        })
    }
    pub fn info(&self) -> serde_json::Value {
        serde_json::json!({"owner": SCHEMA, "cache":self.cache.info()})
    }
    pub fn clear(&mut self) {
        self.cache.clear()
    }
    pub fn price(&mut self, r: &Request) -> Result<ResultData, String> {
        self.position.price(&mut self.cache, self.limit, r)
    }
}
impl IndexedPosition {
    pub(crate) fn new(r: &Request, limit: usize) -> Result<Self, String> {
        preflight(r, limit, 0)?;
        Ok(Self {
            original: r.clone(),
            base: key(r)?,
            original_raw: serde_json::to_vec(r).map_err(|e| e.to_string())?.len(),
            shared_curves: false,
        })
    }
    pub(crate) fn new_shared(r: &Request) -> Result<Self, String> {
        let mut position = Self::new(r, 0)?;
        position.shared_curves = true;
        Ok(position)
    }
    pub(crate) fn price(
        &self,
        cache: &mut Cache,
        limit: usize,
        r: &Request,
    ) -> Result<ResultData, String> {
        let (admitted, work) = preflight(r, limit, self.original_raw)?;
        if key(r)?.as_ref() != self.base.as_ref() {
            return Err(
                "BASE_ANCHOR_CHANGED: explicit original source/OAS/contract rebuild required"
                    .into(),
            );
        }
        let mut counts = BTreeMap::new();
        let source = &r.source;
        let projection_key = Key::new(
            "market-indexed-projection",
            &source.bridge,
            vec![self.base.clone()],
        )?;
        let projections = node(cache, projection_key, "projection", &mut counts, || {
            bridge::evaluate_with_check(&source.bridge, || check().is_err())
        })?;
        // Every Arc result stays pinned until all risk cases and output are complete,
        // so cache eviction/admission refusal cannot change economics.
        let mut cases = Vec::with_capacity(8);
        let mut pinned_indexed = Vec::new();
        let mut pinned_swap = Vec::new();
        let mut roles = vec![
            (&source.bridge.original.market, &projections.original),
            (&source.bridge.current_market, &projections.current),
        ];
        for risk in &projections.risks {
            roles.push((&source.bridge.current_market, &risk.down));
            roles.push((&source.bridge.current_market, &risk.up));
        }
        for (market, projection) in roles {
            check()?;
            let mut canonical = projection.clone();
            canonical.scenario_id = "paired-pricing-index-slice-1".into();
            let requests = adapter::requests(source, market, &canonical, &mut || check().is_err());
            check()?;
            let (indexed_request, swap_request) = requests?;
            let index = market
                .curves
                .iter()
                .find(|c| c.curve_id == source.bridge.binding.index_curve_id)
                .ok_or("index binding")?;
            let collateral = market
                .curves
                .iter()
                .find(|c| c.curve_id == source.bridge.binding.collateral_curve_id)
                .ok_or("collateral binding")?;
            let index_parent = Key::new(
                "named-index",
                (
                    &market.identity,
                    market.available_unix_seconds,
                    market.information_unix_seconds,
                    index,
                    &source.bridge.binding,
                ),
                if self.shared_curves {
                    vec![]
                } else {
                    vec![self.base.clone()]
                },
            )?;
            let cash_key = Key::new("indexed-cash", &indexed_request, vec![index_parent])?;
            let indexed = node(cache, cash_key.clone(), "indexed-cash", &mut counts, || {
                build_path(&indexed_request)
            })?;
            let collateral_parent = Key::new(
                "named-collateral",
                (
                    &market.identity,
                    market.available_unix_seconds,
                    market.information_unix_seconds,
                    collateral,
                    &source.collateral_policy,
                ),
                if self.shared_curves {
                    vec![]
                } else {
                    vec![self.base.clone()]
                },
            )?;
            let mark_key = Key::new(
                "swap-clean",
                &swap_request,
                vec![cash_key.clone(), collateral_parent.clone()],
            )?;
            let swap = node(cache, mark_key.clone(), "swap-clean", &mut counts, || {
                value_swap(&swap_request)
            })?;
            let oas = source.bridge.original.locked_oas;
            let spread_key = Key::new("swap-original-oas", oas, vec![mark_key, self.base.clone()])?;
            let spread = node(cache, spread_key, "swap-original-oas", &mut counts, || {
                let mut pv = 0.;
                for e in &swap.schedule.events {
                    check()?;
                    let t =
                        (e.payment_ordinal - source.bridge.identity.as_of_ordinal) as f64 / 365.;
                    pv += e.components.cash_total()?
                        * swap_request.discount_curve.at(e.payment_ordinal)?
                        * (-oas * t).exp();
                }
                if !pv.is_finite() {
                    return Err("graph PV finite domain".into());
                }
                Ok(pv)
            })?;
            if (*spread - projection.pv).abs() > 1e-9 + 1e-12 * projection.pv.abs() {
                return Err("graph actual cash/PV reconciliation".into());
            }
            let collateral_bump = if projection.scenario_id == "collateral_down"
                || projection.scenario_id == "parallel_down"
            {
                -source.bridge.risk_bump
            } else if projection.scenario_id == "collateral_up"
                || projection.scenario_id == "parallel_up"
            {
                source.bridge.risk_bump
            } else {
                0.
            };
            let loan_key = Key::new(
                "indexed-loan-pv",
                (oas, collateral_bump),
                vec![cash_key, collateral_parent, self.base.clone()],
            )?;
            let loan = node(cache, loan_key, "indexed-loan-pv", &mut counts, || {
                let native = crate::products::term::graph::NativeSource {
                    position_id: indexed_request.position_id.clone(),
                    scenario_id: indexed_request.scenario_id.clone(),
                    run_id: "explicit-pricing-source".into(),
                    path_id: indexed_request.path.id.clone(),
                    weight: 1.,
                    role: crate::products::installment::graph::Role::Pricing,
                    product: crate::products::term::graph::ProductHistory::IndexedTerm {
                        request: indexed_request.clone(),
                    },
                    projection: crate::products::term::graph::SourceProjection::IndexedTerm {
                        result: (*indexed).clone(),
                    },
                };
                crate::products::term::graph::pricing::price(
                    &native,
                    collateral,
                    source.bridge.identity.as_of_ordinal,
                    oas,
                    collateral_bump,
                )
            })?;
            let selected = if r.product == "indexed_loan" {
                *loan
            } else {
                *spread
            };
            cases.push(Case {
                role: projection.scenario_id.clone(),
                source: market.identity.clone(),
                indexed_request,
                indexed_result: (*indexed).clone(),
                swap_request,
                swap_result: (*swap).clone(),
                original_oas: oas,
                spread_adjusted_swap_pv: *spread,
                indexed_pv: *loan,
                selected_pv: selected,
            });
            pinned_indexed.push(indexed);
            pinned_swap.push(swap);
        }
        let mut risk = Vec::with_capacity(3);
        for (i, role) in ["collateral", "index", "parallel"].iter().enumerate() {
            let down = &cases[2 + 2 * i];
            let up = &cases[3 + 2 * i];
            risk.push(Risk {
                curve_role: (*role).into(),
                selected_rate_gradient_per_bp: (up.selected_pv - down.selected_pv)
                    / (2. * source.bridge.risk_bump)
                    * 0.0001,
            });
        }
        check()?;
        let result = ResultData {
            schema: SCHEMA.into(),
            original_anchor: self.original.clone(),
            current_request: r.clone(),
            cases,
            risk,
            counts,
            cache: cache.info(),
            admitted_joined_bytes: admitted,
            admitted_work: work,
            pricing_only: true,
            pairing_policy: "supplied_deterministic_paired_curve_bumps-1".into(),
            stochastic_crn: false,
            physical_accounting: false,
            production_adoption: false,
            saved_library_adoption: false,
            capital_ftp_eligible: false,
        };
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            > adapter::MAX_RETAINED_BYTES
        {
            return Err("graph output capacity".into());
        }
        check()?;
        Ok(result)
    }
}
