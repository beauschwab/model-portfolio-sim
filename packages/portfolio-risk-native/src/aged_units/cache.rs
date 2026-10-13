//! Transactional preparation reuse. Canonical full typed input, never label-only identity.
use super::{funding, AgedUnitsRequest, AgedUnitsResult, MAX_PRICING_WORK};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_STAGED_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Serialize, Default, Debug)]
pub struct CacheWork {
    pub templates_rebuilt: usize,
    pub units_rebuilt: usize,
    pub full_rebuild: bool,
    pub unchanged_request: bool,
    pub pricing_work: usize,
    pub retained_bytes: usize,
}
#[derive(Clone)]
struct Piece {
    key: Vec<u8>,
    result: AgedUnitsResult,
    pricing_work: usize,
}
#[derive(Clone)]
pub struct Cache {
    request: Arc<AgedUnitsRequest>,
    canonical: Arc<Vec<u8>>,
    shared: Arc<Vec<u8>>,
    pieces: HashMap<String, Arc<Piece>>,
    result: Arc<AgedUnitsResult>,
    retained_bytes: usize,
}

struct Count(usize);
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(b.len());
        if self.0 > MAX_RETAINED_BYTES {
            return Err(std::io::Error::other("aged cache byte admission exceeded"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn serialized_size<T: Serialize>(v: &T) -> Result<usize, String> {
    let mut count = Count(0);
    serde_json::to_writer(&mut count, v).map_err(|e| e.to_string())?;
    Ok(count.0)
}
fn canonical<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    serialized_size(v)?;
    serde_json::to_vec(v).map_err(|e| e.to_string())
}
fn shared_key(r: &AgedUnitsRequest) -> Result<Vec<u8>, String> {
    canonical(&(
        &r.schema,
        &r.identity.model_revision,
        &r.identity.crn_id,
        &r.valuation_policy,
        r.curve_as_of_ordinal,
        &r.currency,
        &r.reporting_ordinals,
        &r.base_scenario_id,
        &r.scenarios,
        r.bump_bp,
        r.interpolation_absolute_tolerance,
        r.interpolation_relative_tolerance,
    ))
}
fn calendar_changed(a: &AgedUnitsRequest, b: &AgedUnitsRequest) -> Result<bool, String> {
    for t in &b.templates {
        if let Some(old) = a.templates.iter().find(|x| x.template_id == t.template_id) {
            if canonical(&(&old.term.calendar, &old.term.bdc, &old.term.accrual_dates))?
                != canonical(&(&t.term.calendar, &t.term.bdc, &t.term.accrual_dates))?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn request_for_template(r: &AgedUnitsRequest, id: &str) -> AgedUnitsRequest {
    let mut one = r.clone();
    one.templates.retain(|t| t.template_id == id);
    one.purchases.retain(|p| p.template_id == id);
    one.validation_purchases.retain(|p| p.template_id == id);
    one
}
fn piece_key(r: &AgedUnitsRequest) -> Result<Vec<u8>, String> {
    canonical(&(&r.templates, &r.purchases, &r.validation_purchases))
}
fn result_bytes(r: &AgedUnitsResult) -> Result<usize, String> {
    // Serialized bytes charge strings/metadata plus an additional conservative
    // native allocation charge (numeric buffers can serialize as single-byte 0).
    let numeric = (r.values.capacity() + r.funding.demand.capacity()) * 8;
    let events = r.funding.cash_events.capacity() * std::mem::size_of::<funding::CashEvent>();
    let units = r.units.capacity() * std::mem::size_of::<super::AgedUnit>();
    let states: usize = r
        .units
        .iter()
        .map(|u| {
            u.opening_states.capacity()
                * std::mem::size_of::<crate::advanced_product::ReplayedBookState>()
        })
        .sum();
    Ok(serialized_size(r)?
        .saturating_mul(4)
        .saturating_add(numeric)
        .saturating_add(events)
        .saturating_add(units)
        .saturating_add(states))
}

impl Cache {
    pub fn new(r: &AgedUnitsRequest) -> Result<Self, String> {
        Self::build(r, None).map(|(cache, _)| cache)
    }
    pub fn request(&self) -> &AgedUnitsRequest {
        &self.request
    }
    pub fn snapshot(&self) -> &AgedUnitsResult {
        &self.result
    }
    pub fn update(&mut self, r: &AgedUnitsRequest) -> Result<CacheWork, String> {
        portfolio_compute_control::checkpoint()?;
        super::validate(r)?;
        if canonical(r)?.as_slice() == self.canonical.as_slice() {
            return Ok(CacheWork {
                unchanged_request: true,
                retained_bytes: self.retained_bytes,
                ..CacheWork::default()
            });
        }
        let (next, work) = Self::build(r, Some(self))?;
        portfolio_compute_control::checkpoint()?;
        *self = next;
        Ok(work)
    }
    fn build(r: &AgedUnitsRequest, old: Option<&Self>) -> Result<(Self, CacheWork), String> {
        super::validate(r)?;
        let canonical = canonical(r)?;
        let shared = shared_key(r)?;
        let full = if let Some(old) = old {
            shared != *old.shared || calendar_changed(&old.request, r)?
        } else {
            true
        };
        let mut pieces = HashMap::new();
        let mut work = CacheWork {
            full_rebuild: full,
            ..CacheWork::default()
        };
        let mut admission_work = 0usize;
        let mut bytes = serialized_size(r)?
            .saturating_mul(4)
            .saturating_add(canonical.capacity())
            .saturating_add(shared.capacity());
        for t in &r.templates {
            portfolio_compute_control::checkpoint()?;
            let one = request_for_template(r, &t.template_id);
            let key = piece_key(&one)?;
            let hit = old
                .filter(|_| !full)
                .and_then(|old| old.pieces.get(&t.template_id))
                .filter(|piece| piece.key == key);
            let piece = if let Some(hit) = hit {
                Arc::clone(hit)
            } else {
                let (result, pricing_work) =
                    super::prepare_with_work(&one, MAX_PRICING_WORK - admission_work)?;
                work.templates_rebuilt += 1;
                work.units_rebuilt += result.units.len();
                work.pricing_work += pricing_work;
                Arc::new(Piece {
                    key,
                    result,
                    pricing_work,
                })
            };
            admission_work = admission_work
                .checked_add(piece.pricing_work)
                .ok_or("aged cache pricing work overflow")?;
            if admission_work > MAX_PRICING_WORK {
                return Err("aggregate aged cache pricing work admission exceeded".into());
            }
            bytes = bytes
                .saturating_add(result_bytes(&piece.result)?)
                .saturating_add(piece.key.capacity());
            if bytes > MAX_RETAINED_BYTES
                || bytes.saturating_add(old.map_or(0, |o| o.retained_bytes)) > MAX_STAGED_BYTES
            {
                return Err("aged cache retained/staged byte admission exceeded".into());
            }
            pieces.insert(t.template_id.clone(), piece);
        }
        let result = merge(r, &pieces)?;
        bytes = bytes.saturating_add(result_bytes(&result)?);
        if bytes > MAX_RETAINED_BYTES
            || bytes.saturating_add(old.map_or(0, |o| o.retained_bytes)) > MAX_STAGED_BYTES
        {
            return Err("aged cache retained/staged byte admission exceeded".into());
        }
        work.retained_bytes = bytes;
        Ok((
            Self {
                request: Arc::new(r.clone()),
                canonical: Arc::new(canonical),
                shared: Arc::new(shared),
                pieces,
                result: Arc::new(result),
                retained_bytes: bytes,
            },
            work,
        ))
    }
}

fn merge(
    r: &AgedUnitsRequest,
    pieces: &HashMap<String, Arc<Piece>>,
) -> Result<AgedUnitsResult, String> {
    let h = r.reporting_ordinals.len() - 1;
    let n = r.purchases.len();
    let s = r.scenarios.len();
    let first = &pieces[&r.templates[0].template_id].result;
    let mut values = vec![0.; s * n * 10 * h];
    let mut units = Vec::with_capacity(n);
    let mut plans = Vec::with_capacity(n);
    for (i, purchase) in r.purchases.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let piece = &pieces[&purchase.template_id].result;
        let j = piece
            .units
            .iter()
            .position(|u| u.purchase_ordinal == purchase.purchase_ordinal)
            .ok_or("cached purchase identity absent")?;
        for scenario in 0..s {
            values[(scenario * n + i) * 10 * h..(scenario * n + i + 1) * 10 * h].copy_from_slice(
                &piece.values[(scenario * piece.units.len() + j) * 10 * h
                    ..(scenario * piece.units.len() + j + 1) * 10 * h],
            );
        }
        units.push(piece.units[j].clone());
        let mut cost = piece.funding.purchase_costs[j].clone();
        cost.unit_index = i;
        cost.contract_source.revision = r.identity.input_revision.clone();
        let mut events: Vec<_> = piece
            .funding
            .cash_events
            .iter()
            .filter(|e| e.unit_index == j)
            .cloned()
            .collect();
        for event in &mut events {
            event.unit_index = i;
        }
        plans.push(funding::UnitCashPlan {
            purchase: cost,
            events,
        });
    }
    let mut diagnostics = Vec::new();
    for purchase in &r.validation_purchases {
        diagnostics.extend(
            pieces[&purchase.template_id]
                .result
                .interpolation_diagnostics
                .iter()
                .filter(|d| d.purchase_ordinal == purchase.purchase_ordinal)
                .cloned(),
        );
    }
    let funding = funding::compile(
        plans,
        r.scenarios.iter().map(|x| x.scenario_id.clone()).collect(),
        &r.reporting_ordinals,
        r.identity.input_revision.clone(),
    )?;
    Ok(AgedUnitsResult {
        schema: r.schema.clone(),
        identity: r.identity.clone(),
        native_projection_model: first.native_projection_model.clone(),
        curve_as_of_ordinal: r.curve_as_of_ordinal,
        currency: r.currency.clone(),
        bump_bp: r.bump_bp,
        curve_scenarios: r.scenarios.clone(),
        template_definitions: r.templates.clone(),
        funding,
        valuation_policy: r.valuation_policy.clone(),
        scenario_ids: r.scenarios.iter().map(|x| x.scenario_id.clone()).collect(),
        base_scenario_id: r.base_scenario_id.clone(),
        reporting_ordinals: r.reporting_ordinals.clone(),
        units,
        horizon: h,
        shape: [s, n, 10, h],
        values,
        interpolation_diagnostics: diagnostics,
        income_policy: first.income_policy.clone(),
        scope: first.scope.clone(),
    })
}
