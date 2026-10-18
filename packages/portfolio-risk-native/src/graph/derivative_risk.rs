//! `saved-derivative-risk-1`: saved raw swap trades as cached graph nodes with paired curve risk.
//!
//! Each position is a complete Pricing `SwapRequest` from a saved derivative source. Risk comes
//! from deterministic paired bumps of the supplied curves: the collateral discount curve shifts by
//! a continuously compounded zero-rate bump on ACT/365F, and every conditionally projected index
//! observation either shifts additively by the same bump at its fixing date, or is re-projected
//! as the simple forward of its named index curve bumped the same way (an explicit policy). Observed fixings never move.
//! Key-rate bumps use triangular weights on reviewed knot dates that sum to one, so key-rate
//! risk adds up to the parallel bump for linear trades. Hedged-item risk reuses the hedge owner's
//! benchmark valuation on the first instrument's bumped curve. Designation never changes a trade's
//! economic value or risk; this graph has no accounting output.
use crate::graph::cache::{Cache, Key};
use crate::products::derivatives::contract::{
    amount, identity, near, swaps, swaps::DiscountCurve, swaps::DiscountPoint, swaps::SwapRequest,
    text, Identity,
};
use crate::products::derivatives::hedge_accounting::{
    self as hedge, HedgedItem, ItemDirection, ItemKind,
};
use crate::products::term::contracts::indexed::{Coupon, RateSource};
use portfolio_model_core::cashflow::{actual_year_fraction, ActualDayCount};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub const SCHEMA: &str = "saved-derivative-risk-1";
const MAX_WORK: usize = 20_000_000;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    /// The saved derivative source these trades were read from (provenance and owner anchor).
    pub source: Identity,
    pub positions: Vec<Position>,
    #[serde(default)]
    pub relationships: Vec<RelationshipInput>,
    pub bumps: BumpPolicy,
    /// How projected fixings respond to a bump; there is no default.
    pub projection_policy: ProjectionPolicy,
    /// Required, and only admitted, under `named_projection_curves`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projection_curves: Vec<ProjectionCurve>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionPolicy {
    /// Supplied conditional projections shift additively by the bump at their fixing date.
    AdditiveProjectedObservations,
    /// Every projected fixing is the simple forward of its named index curve over the
    /// observation interest period; the base forward must reproduce the supplied fixing.
    NamedProjectionCurves,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionCurve {
    pub index_id: String,
    pub source: Identity,
    pub day_count: ActualDayCount,
    pub points: Vec<DiscountPoint>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub position_id: String,
    pub netting_set_id: String,
    pub swap: SwapRequest,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BumpPolicy {
    pub identity: Identity,
    pub bump_bp: f64,
    pub key_rate_ordinals: Vec<i32>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipInput {
    pub relationship_id: String,
    pub instrument_position_ids: Vec<String>,
    #[serde(default)]
    pub hedged_item: Option<ItemInput>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInput {
    pub item: HedgedItem,
    pub benchmark_rate: f64,
    pub hedged_notional: f64,
    #[serde(default)]
    pub hedged_term_end_ordinal: Option<i32>,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Counts {
    pub computed: usize,
    pub reused: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Risk {
    pub clean_value: f64,
    pub dv01: f64,
    pub key_rate_dv01: Vec<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PositionRisk {
    pub position_id: String,
    pub netting_set_id: String,
    pub relationship_id: Option<String>,
    #[serde(flatten)]
    pub risk: Risk,
}
#[derive(Clone, Debug, Serialize)]
pub struct NettingSetRisk {
    pub netting_set_id: String,
    #[serde(flatten)]
    pub risk: Risk,
}
#[derive(Clone, Debug, Serialize)]
pub struct RelationshipRisk {
    pub relationship_id: String,
    pub instrument: Risk,
    pub hedged_item: Option<Risk>,
    pub net_dv01: f64,
    pub net_key_rate_dv01: Vec<f64>,
    /// Minus instrument DV01 over hedged-item DV01: one means a full economic rate offset.
    pub dv01_offset_ratio: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub source: Identity,
    pub bump_policy: Identity,
    pub bump_bp: f64,
    pub key_rate_ordinals: Vec<i32>,
    pub positions: Vec<PositionRisk>,
    pub netting_sets: Vec<NettingSetRisk>,
    pub relationships: Vec<RelationshipRisk>,
    pub total: Risk,
    pub projected_observations_bumped: usize,
    pub counts: BTreeMap<String, Counts>,
    pub cache: serde_json::Value,
    pub admitted_work: usize,
    pub pairing_policy: String,
    pub pricing_only: bool,
    pub observed_fixings_bumped: bool,
    pub accounting_designation_output: bool,
    pub capital_ftp_eligible: bool,
    pub disclosure: String,
}

#[derive(Clone, Copy)]
enum Scenario {
    Base,
    Parallel(f64),
    Key(usize, f64),
}

fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}

/// Triangular key-rate weight of knot `k` at date `t`; weights over all knots sum to one.
fn key_weight(knots: &[i32], k: usize, t: i32) -> f64 {
    let n = knots.len();
    if n == 1 {
        return 1.;
    }
    let x = f64::from(t);
    let at = |i: usize| f64::from(knots[i]);
    if k == 0 && t <= knots[0] || k == n - 1 && t >= knots[n - 1] {
        return 1.;
    }
    if k > 0 && t > knots[k - 1] && t <= knots[k] {
        return (x - at(k - 1)) / (at(k) - at(k - 1));
    }
    if k + 1 < n && t > knots[k] && t < knots[k + 1] {
        return (at(k + 1) - x) / (at(k + 1) - at(k));
    }
    0.
}
fn shift(policy: &BumpPolicy, scenario: Scenario, t: i32) -> f64 {
    match scenario {
        Scenario::Base => 0.,
        Scenario::Parallel(sign) => sign * policy.bump_bp * 1e-4,
        Scenario::Key(k, sign) => {
            sign * policy.bump_bp * 1e-4 * key_weight(&policy.key_rate_ordinals, k, t)
        }
    }
}
fn bump_curve(curve: &DiscountCurve, policy: &BumpPolicy, scenario: Scenario) -> DiscountCurve {
    let mut c = curve.clone();
    for p in &mut c.points {
        let years = f64::from(p.ordinal - curve.asof_ordinal) / 365.;
        p.discount_factor *= (-shift(policy, scenario, p.ordinal) * years).exp();
    }
    c
}
fn point(points: &[DiscountPoint], t: i32) -> Result<f64, String> {
    points
        .binary_search_by_key(&t, |p| p.ordinal)
        .map(|i| points[i].discount_factor)
        .map_err(|_| "projection curve lacks an exact observation date; no interpolation".into())
}
/// Simple forward of a projection curve over an observation interest period.
fn forward(c: &ProjectionCurve, start: i32, end: i32) -> Result<f64, String> {
    let tau = actual_year_fraction(start, end, c.day_count)?;
    amount((point(&c.points, start)? / point(&c.points, end)? - 1.) / tau)
}
fn bump_projection(
    c: &ProjectionCurve,
    asof: i32,
    policy: &BumpPolicy,
    scenario: Scenario,
) -> ProjectionCurve {
    let mut out = c.clone();
    for p in &mut out.points {
        let years = f64::from(p.ordinal - asof) / 365.;
        p.discount_factor *= (-shift(policy, scenario, p.ordinal) * years).exp();
    }
    out
}
fn bump_swap(
    swap: &SwapRequest,
    r: &Request,
    scenario: Scenario,
) -> Result<(SwapRequest, usize), String> {
    let policy = &r.bumps;
    let mut s = swap.clone();
    s.discount_curve = bump_curve(&swap.discount_curve, policy, scenario);
    let mut projected = 0;
    for p in &mut s.floating_periods {
        if let Coupon::Indexed {
            index,
            observations,
            ..
        } = &mut p.coupon
        {
            let curve = match r.projection_policy {
                ProjectionPolicy::AdditiveProjectedObservations => None,
                ProjectionPolicy::NamedProjectionCurves => Some(bump_projection(
                    r.projection_curves
                        .iter()
                        .find(|c| c.index_id == index.index_id)
                        .ok_or("projected index has no named projection curve")?,
                    swap.asof_ordinal,
                    policy,
                    scenario,
                )),
            };
            for o in observations {
                if matches!(o.source, RateSource::ConditionalProjection { .. }) {
                    o.rate = match &curve {
                        None => o.rate + shift(policy, scenario, o.fixing_ordinal),
                        Some(c) => forward(c, o.interest_start_ordinal, o.interest_end_ordinal)?,
                    };
                    projected += 1;
                }
            }
        }
    }
    Ok((s, projected))
}
fn scenarios(policy: &BumpPolicy) -> Vec<Scenario> {
    let mut out = vec![
        Scenario::Base,
        Scenario::Parallel(-1.),
        Scenario::Parallel(1.),
    ];
    for k in 0..policy.key_rate_ordinals.len() {
        out.push(Scenario::Key(k, -1.));
        out.push(Scenario::Key(k, 1.));
    }
    out
}
/// Paired central differences per basis point: positive when value rises as rates fall.
fn risk(values: &[f64], policy: &BumpPolicy) -> Result<Risk, String> {
    let d = |down: f64, up: f64| amount((down - up) / (2. * policy.bump_bp));
    let mut key = Vec::with_capacity(policy.key_rate_ordinals.len());
    for k in 0..policy.key_rate_ordinals.len() {
        key.push(d(values[3 + 2 * k], values[4 + 2 * k])?);
    }
    Ok(Risk {
        clean_value: values[0],
        dv01: d(values[1], values[2])?,
        key_rate_dv01: key,
    })
}
fn add(a: &mut [f64], b: &[f64]) -> Result<(), String> {
    for (x, y) in a.iter_mut().zip(b) {
        *x = amount(*x + y)?;
    }
    Ok(())
}

struct Admitted {
    work: usize,
    owners: HashMap<String, String>,
}
fn admit(r: &Request) -> Result<Admitted, String> {
    check()?;
    let mut bytes = 0;
    identity(&r.source, &mut bytes)?;
    identity(&r.bumps.identity, &mut bytes)?;
    let k = &r.bumps.key_rate_ordinals;
    if r.schema != SCHEMA
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.relationships.len() > 64
        || !r.bumps.bump_bp.is_finite()
        || r.bumps.bump_bp <= 0.
        || r.bumps.bump_bp > 100.
        || k.len() > 32
        || k.windows(2).any(|w| w[1] <= w[0])
    {
        return Err("saved derivative risk requires the schema, 1-256 positions, at most 64 relationships, a bump in (0, 100] bp and at most 32 increasing key-rate dates".into());
    }
    let first = &r.positions[0].swap;
    if k.first().is_some_and(|d| *d <= first.asof_ordinal) {
        return Err("key-rate dates must follow the valuation date".into());
    }
    match r.projection_policy {
        ProjectionPolicy::AdditiveProjectedObservations => {
            if !r.projection_curves.is_empty() {
                return Err(
                    "projection curves are admitted only under named_projection_curves".into(),
                );
            }
        }
        ProjectionPolicy::NamedProjectionCurves => {
            let mut names = HashSet::new();
            if r.projection_curves.is_empty() || r.projection_curves.len() > 16 {
                return Err("named projection policy requires 1-16 projection curves".into());
            }
            for c in &r.projection_curves {
                text(&c.index_id, &mut bytes)?;
                identity(&c.source, &mut bytes)?;
                if !names.insert(c.index_id.as_str())
                    || c.points.is_empty()
                    || c.points.len() > 8192
                    || c.points.windows(2).any(|w| w[1].ordinal <= w[0].ordinal)
                    || c.points
                        .iter()
                        .any(|p| !p.discount_factor.is_finite() || p.discount_factor <= 0.)
                {
                    return Err(
                        "projection curves need unique index IDs and increasing positive points"
                            .into(),
                    );
                }
            }
            // The base forward of each named curve must reproduce every supplied projected fixing.
            for p in &r.positions {
                let (base, _) = bump_swap(&p.swap, r, Scenario::Base)?;
                for (a, b) in p.swap.floating_periods.iter().zip(&base.floating_periods) {
                    if let (
                        Coupon::Indexed {
                            observations: x, ..
                        },
                        Coupon::Indexed {
                            observations: y, ..
                        },
                    ) = (&a.coupon, &b.coupon)
                    {
                        if x.iter().zip(y).any(|(u, v)| !near(u.rate, v.rate)) {
                            return Err("named projection curve does not reproduce a supplied projected fixing".into());
                        }
                    }
                }
            }
        }
    }
    let n = scenarios(&r.bumps).len();
    let mut ids = HashSet::new();
    let mut work = 0usize;
    for p in &r.positions {
        text(&p.position_id, &mut bytes)?;
        text(&p.netting_set_id, &mut bytes)?;
        if !ids.insert(p.position_id.as_str())
            || p.swap.position_id != p.position_id
            || p.swap.asof_ordinal != first.asof_ordinal
            || p.swap.valuation_cutoff_unix_seconds != first.valuation_cutoff_unix_seconds
            || p.swap.currency != first.currency
        {
            return Err("positions need unique IDs matching their swap, one valuation date and one currency".into());
        }
        work = work
            .checked_add(swaps::work(&p.swap).saturating_mul(n))
            .ok_or("work overflow")?;
    }
    let mut owners = HashMap::new();
    let mut relationship_ids = HashSet::new();
    for rel in &r.relationships {
        text(&rel.relationship_id, &mut bytes)?;
        if !relationship_ids.insert(rel.relationship_id.as_str())
            || rel.instrument_position_ids.is_empty()
        {
            return Err("relationships need unique IDs and at least one instrument".into());
        }
        for id in &rel.instrument_position_ids {
            if !ids.contains(id.as_str())
                || owners
                    .insert(id.clone(), rel.relationship_id.clone())
                    .is_some()
            {
                return Err(
                    "each instrument must be a listed position in at most one relationship".into(),
                );
            }
        }
        if let Some(x) = &rel.hedged_item {
            let end = x
                .hedged_term_end_ordinal
                .unwrap_or(x.item.principal_ordinal);
            if x.item.kind != ItemKind::FixedRateInstrument
                || x.item.portfolio.is_some()
                || !x.benchmark_rate.is_finite()
                || !x.hedged_notional.is_finite()
                || x.hedged_notional <= 0.
                || x.hedged_notional > x.item.notional
                || end > x.item.principal_ordinal
                || end <= first.asof_ordinal
            {
                return Err("hedged-item risk admits a fixed-rate instrument with a finite benchmark rate, a hedged notional within the item and a term end after valuation".into());
            }
            work = work
                .checked_add(x.item.periods.len().saturating_mul(n))
                .ok_or("work overflow")?;
        }
    }
    if work > MAX_WORK {
        return Err("saved derivative risk work exceeds 20000000".into());
    }
    check()?;
    Ok(Admitted { work, owners })
}

fn node(
    cache: &mut Cache,
    key: Arc<Key>,
    stage: &str,
    counts: &mut BTreeMap<String, Counts>,
    compute: impl FnOnce() -> Result<f64, String>,
) -> Result<Arc<f64>, String> {
    check()?;
    if let Some(v) = cache.get::<f64>(&key) {
        counts.entry(stage.into()).or_default().reused += 1;
        return Ok(v);
    }
    let v = Arc::new(compute()?);
    cache.put(key, v.clone(), 64);
    counts.entry(stage.into()).or_default().computed += 1;
    Ok(v)
}

pub struct Graph {
    source: Identity,
    cache: Cache,
}
impl Graph {
    pub fn new(r: &Request, cache_bytes: usize, cache_entries: usize) -> Result<Self, String> {
        admit(r)?;
        if cache_bytes > 16 * 1024 * 1024 || cache_entries > 65536 {
            return Err("saved derivative risk cache admission".into());
        }
        Ok(Self {
            source: r.source.clone(),
            cache: Cache::new(cache_bytes, cache_entries)?,
        })
    }
    pub fn info(&self) -> serde_json::Value {
        serde_json::json!({"owner": SCHEMA, "cache": self.cache.info()})
    }
    pub fn clear(&mut self) {
        self.cache.clear()
    }
    pub fn price(&mut self, r: &Request) -> Result<ResultData, String> {
        let admitted = admit(r)?;
        if r.source != self.source {
            return Err(
                "BASE_SOURCE_CHANGED: a different saved derivative source needs a new graph".into(),
            );
        }
        let cases = scenarios(&r.bumps);
        let mut counts = BTreeMap::new();
        let mut projected = 0;
        // Values are copied out of the cache immediately, so eviction cannot change economics.
        let mut values: HashMap<&str, Vec<f64>> = HashMap::new();
        for p in &r.positions {
            let mut row = Vec::with_capacity(cases.len());
            for &s in &cases {
                let (swap, n) = bump_swap(&p.swap, r, s)?;
                if matches!(s, Scenario::Base) {
                    projected += n;
                }
                let key = Key::new("saved-derivative-swap-clean", &swap, vec![])?;
                row.push(*node(
                    &mut self.cache,
                    key,
                    "swap-clean",
                    &mut counts,
                    || swaps::value_swap(&swap).map(|v| v.clean_value),
                )?);
            }
            values.insert(p.position_id.as_str(), row);
        }
        let mut positions = Vec::with_capacity(r.positions.len());
        let mut sets: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut total = vec![0.; cases.len()];
        for p in &r.positions {
            let row = &values[p.position_id.as_str()];
            add(&mut total, row)?;
            add(
                sets.entry(p.netting_set_id.as_str())
                    .or_insert_with(|| vec![0.; cases.len()]),
                row,
            )?;
            positions.push(PositionRisk {
                position_id: p.position_id.clone(),
                netting_set_id: p.netting_set_id.clone(),
                relationship_id: admitted.owners.get(&p.position_id).cloned(),
                risk: risk(row, &r.bumps)?,
            });
        }
        let mut netting_sets = Vec::with_capacity(sets.len());
        for (id, row) in &sets {
            netting_sets.push(NettingSetRisk {
                netting_set_id: (*id).into(),
                risk: risk(row, &r.bumps)?,
            });
        }
        let by_id: HashMap<&str, &Position> = r
            .positions
            .iter()
            .map(|p| (p.position_id.as_str(), p))
            .collect();
        let mut relationships = Vec::with_capacity(r.relationships.len());
        for rel in &r.relationships {
            let mut instrument = vec![0.; cases.len()];
            for id in &rel.instrument_position_ids {
                add(&mut instrument, &values[id.as_str()])?;
            }
            let instrument = risk(&instrument, &r.bumps)?;
            let item = match &rel.hedged_item {
                None => None,
                Some(x) => {
                    let anchor = &by_id[rel.instrument_position_ids[0].as_str()].swap;
                    let sign = if x.item.direction == ItemDirection::Asset {
                        1.
                    } else {
                        -1.
                    };
                    let end = x
                        .hedged_term_end_ordinal
                        .unwrap_or(x.item.principal_ordinal);
                    let mut row = Vec::with_capacity(cases.len());
                    for &s in &cases {
                        let curve = bump_curve(&anchor.discount_curve, &r.bumps, s);
                        let key = Key::new(
                            "saved-hedged-item-benchmark",
                            (
                                &x.item,
                                x.benchmark_rate,
                                x.hedged_notional,
                                end,
                                anchor.payment_timing,
                                &curve,
                            ),
                            vec![],
                        )?;
                        row.push(*node(
                            &mut self.cache,
                            key,
                            "hedged-item-benchmark",
                            &mut counts,
                            || {
                                let unit = hedge::benchmark_unit_clean(
                                    &x.item,
                                    x.benchmark_rate,
                                    &curve,
                                    anchor.asof_ordinal,
                                    anchor.payment_timing,
                                    end,
                                )?;
                                amount(sign * x.hedged_notional * unit)
                            },
                        )?);
                    }
                    Some(risk(&row, &r.bumps)?)
                }
            };
            let mut net_key = instrument.key_rate_dv01.clone();
            let net_dv01 = match &item {
                Some(i) => {
                    add(&mut net_key, &i.key_rate_dv01)?;
                    amount(instrument.dv01 + i.dv01)?
                }
                None => instrument.dv01,
            };
            let ratio = item
                .as_ref()
                .filter(|i| i.dv01 != 0.)
                .map(|i| -instrument.dv01 / i.dv01);
            relationships.push(RelationshipRisk {
                relationship_id: rel.relationship_id.clone(),
                instrument,
                hedged_item: item,
                net_dv01,
                net_key_rate_dv01: net_key,
                dv01_offset_ratio: ratio,
            });
        }
        let result = ResultData {
            schema: SCHEMA.into(),
            source: r.source.clone(),
            bump_policy: r.bumps.identity.clone(),
            bump_bp: r.bumps.bump_bp,
            key_rate_ordinals: r.bumps.key_rate_ordinals.clone(),
            positions,
            netting_sets,
            relationships,
            total: risk(&total, &r.bumps)?,
            projected_observations_bumped: projected,
            counts,
            cache: self.cache.info(),
            admitted_work: admitted.work,
            pairing_policy: match r.projection_policy {
                ProjectionPolicy::AdditiveProjectedObservations => {
                    "deterministic_paired_curve_bumps-1:additive_projected_observations"
                }
                ProjectionPolicy::NamedProjectionCurves => {
                    "deterministic_paired_curve_bumps-1:named_projection_curves"
                }
            }
            .into(),
            pricing_only: true,
            observed_fixings_bumped: false,
            accounting_designation_output: false,
            capital_ftp_eligible: false,
            disclosure: "Clean values of complete saved Pricing swaps with central paired curve bumps: collateral discount factors shift by a continuously compounded ACT/365F zero-rate bump and conditionally projected index observations either shift additively at their fixing date or are re-projected from the bumped named index curve, per the explicit projection policy; observed fixings never move. Key-rate bumps use triangular weights on the reviewed knot dates. Hedged-item risk is the hedge owner's benchmark value on the first instrument's bumped curve. No curve recalibration, convexity adjustment, XVA, accounting designation, capital or FTP output.".into(),
        };
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_OUTPUT_BYTES
        {
            return Err("saved derivative risk output exceeds 32 MiB".into());
        }
        check()?;
        Ok(result)
    }
}
