//! Explicit native acquisition events: zero opening claims become live claims
//! before recognition and settlement. No default draw or financing is inferred.
use crate::{dated_ledger::CompiledDatedLedger, dated_store::DatedStore, types::Spec};
use portfolio_model_core::cashflow::{validate_ordinal, CashflowMeasure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "dated-acquisition-1";
pub const MAX_EVENTS: usize = 4096;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub event_id: String,
    pub run_id: String,
    pub position_id: String,
    pub account_id: String,
    pub currency: String,
    pub source: Identity,
    pub path_id: String,
    pub source_weight: f64,
    pub allocation_factor: f64,
    pub principal: f64,
    pub cost: f64,
    pub ordinal: i32,
    pub original_yield_anchor: Identity,
    pub measure: CashflowMeasure,
    pub recognition_event_ids: Vec<String>,
    pub coupon_sign: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub policy: Identity,
    pub events: Vec<Event>,
    pub right_ownership: String,
    #[serde(default)]
    pub reporting_ordinal: Option<i32>,
}
#[derive(Debug)]
pub struct Stored {
    pub input: Event,
    pub position_index: usize,
    pub owner_index: usize,
    pub run_index: usize,
}
#[derive(Debug)]
pub struct Compiled {
    pub policy: Identity,
    pub events: Vec<Stored>,
    by_day: BTreeMap<(usize, u32), Vec<usize>>,
    groups: BTreeMap<(usize, usize), Vec<usize>>,
    pub event_pools: BTreeMap<(usize, usize), usize>,
    pub pool_events: Vec<Vec<usize>>,
    pub reporting_day: Option<u32>,
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("acquisition bounded explicit identity required".into())
    } else {
        Ok(())
    }
}
fn money(x: f64) -> Result<(), String> {
    if x.is_finite() && x.abs() <= 1e15 {
        Ok(())
    } else {
        Err("acquisition finite monetary domain required".into())
    }
}
impl Compiled {
    pub fn build(r: &Request, spec: &Spec, store: &DatedStore) -> Result<Self, String> {
        if r.schema != SCHEMA
            || r.events.is_empty()
            || r.events.len() > MAX_EVENTS
            || r.right_ownership != "physical_path_net_rights-1"
        {
            return Err("acquisition schema/event bounds mismatch".into());
        }
        label(&r.policy.id)?;
        label(&r.policy.revision)?;
        let reporting_day = r
            .reporting_ordinal
            .map(|ordinal| -> Result<u32, String> {
                validate_ordinal(ordinal)?;
                if ordinal < store.as_of_ordinal() || ordinal > store.reporting_end_ordinal() {
                    return Err("acquisition reporting date outside complete replay".into());
                }
                Ok((ordinal - store.as_of_ordinal()) as u32)
            })
            .transpose()?;
        let mut events = Vec::with_capacity(r.events.len());
        let mut by_day = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut runs = BTreeMap::new();
        let mut positions = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut dated_events = BTreeMap::new();
        for (i, run) in store.runs().iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            runs.insert(run.run_id.as_ref(), i);
        }
        for (i, p) in spec.positions.iter().enumerate() {
            if i.is_multiple_of(256) {
                portfolio_compute_control::checkpoint()?;
            }
            positions.insert(p.id.as_str(), i);
        }
        for (i, o) in store.owners().iter().enumerate() {
            if i.is_multiple_of(256) {
                portfolio_compute_control::checkpoint()?;
            }
            owners.insert(o.position_id.as_ref(), i);
        }
        for (i, e) in store.events().iter().enumerate() {
            if i.is_multiple_of(256) {
                portfolio_compute_control::checkpoint()?;
            }
            dated_events.insert((e.run_index, e.owner_index, e.event_id.as_ref()), i);
        }
        let mut groups = BTreeMap::new();
        let mut event_pools = BTreeMap::new();
        let mut pool_events = Vec::with_capacity(r.events.len());
        for e in &r.events {
            portfolio_compute_control::checkpoint()?;
            for x in [
                &e.event_id,
                &e.run_id,
                &e.position_id,
                &e.account_id,
                &e.currency,
                &e.source.id,
                &e.source.revision,
                &e.path_id,
                &e.original_yield_anchor.id,
                &e.original_yield_anchor.revision,
            ] {
                label(x)?;
            }
            for x in [e.principal, e.cost, e.source_weight, e.allocation_factor] {
                money(x)?;
            }
            validate_ordinal(e.ordinal)?;
            if e.measure != CashflowMeasure::Physical
                || e.principal < 0.
                || e.cost < 0.
                || !(0. ..=1.).contains(&e.source_weight)
                || e.allocation_factor < 0.
                || !matches!(e.coupon_sign, 1.0 | -1.0)
                || (e.principal > 0. && e.cost == 0.)
                || ((e.source_weight == 0. || e.allocation_factor == 0.)
                    && (e.principal != 0. || e.cost != 0.))
                || !ids.insert((&e.run_id, &e.event_id))
            {
                return Err("acquisition physical/weight/cost/identity domain mismatch".into());
            }
            let run_index = runs
                .get(e.run_id.as_str())
                .copied()
                .ok_or("acquisition run absent from dated source")?;
            let position_index = positions
                .get(e.position_id.as_str())
                .copied()
                .ok_or("acquisition position absent")?;
            let owner_index = owners
                .get(e.position_id.as_str())
                .copied()
                .ok_or("acquisition requires explicit dated ownership")?;
            let p = &spec.positions[position_index];
            let owner = &store.owners()[owner_index];
            if p.balance != 0.
                || p.opening_accrued != 0.
                || p.book_adjustment != 0.
                || p.account.as_str() != e.account_id
                || owner.currency.as_ref() != e.currency
                || owner.source_id.as_ref() != e.source.id
                || owner.source_revision.as_ref() != e.source.revision
                || e.ordinal < store.as_of_ordinal()
                || e.ordinal > store.reporting_end_ordinal()
            {
                return Err("acquisition zero opening/account/source/date mismatch".into());
            }
            let day = (e.ordinal - store.as_of_ordinal()) as u32;
            if e.recognition_event_ids.is_empty()
                || e.recognition_event_ids.len() > store.events().len()
            {
                return Err("acquisition physical path recognition coverage required".into());
            }
            let mut own_events = Vec::with_capacity(e.recognition_event_ids.len());
            for id in &e.recognition_event_ids {
                portfolio_compute_control::checkpoint()?;
                label(id)?;
                let index = dated_events
                    .get(&(Some(run_index), owner_index, id.as_str()))
                    .or_else(|| dated_events.get(&(None, owner_index, id.as_str())))
                    .copied()
                    .ok_or("acquisition recognition event absent from source")?;
                if store.events()[index].measure != CashflowMeasure::Physical
                    || event_pools
                        .insert((run_index, index), events.len())
                        .is_some()
                {
                    return Err("acquisition physical recognition overlap/measure mismatch".into());
                }
                let direction = match owner.direction {
                    crate::replay::stores::dated::Direction::Receipt => 1.,
                    crate::replay::stores::dated::Direction::Payment => -1.,
                };
                let components = &store.events()[index].components;
                if (e.source_weight == 0. || e.allocation_factor == 0.)
                    && [
                        components.principal,
                        components.accrued_interest,
                        components.cash_interest,
                        components.book_amortization,
                        components.fees,
                    ]
                    .iter()
                    .any(|amount| *amount != 0.)
                {
                    return Err("acquisition zero participation has nonzero path money".into());
                }
                if [components.accrued_interest, components.cash_interest]
                    .iter()
                    .any(|amount| amount * direction * e.coupon_sign < 0.)
                {
                    return Err("acquisition contractual coupon direction mismatch".into());
                }
                own_events.push(index);
            }
            own_events.sort_unstable();
            pool_events.push(own_events);
            groups
                .entry((run_index, owner_index))
                .or_insert_with(Vec::new)
                .push(events.len());
            by_day
                .entry((run_index, day))
                .or_insert_with(Vec::new)
                .push(events.len());
            events.push(Stored {
                input: e.clone(),
                position_index,
                owner_index,
                run_index,
            });
        }
        Ok(Self {
            policy: r.policy.clone(),
            events,
            by_day,
            groups,
            event_pools,
            pool_events,
            reporting_day,
        })
    }
    pub fn on(&self, run: usize, day: u32) -> impl Iterator<Item = &Stored> {
        self.by_day
            .get(&(run, day))
            .into_iter()
            .flatten()
            .map(|i| &self.events[*i])
    }
    pub fn principal_for(&self, run: usize, owner: usize) -> f64 {
        self.groups
            .get(&(run, owner))
            .into_iter()
            .flatten()
            .map(|i| self.events[*i].input.principal)
            .sum()
    }
    pub fn validate_chronology(&self, c: &CompiledDatedLedger) -> Result<(), String> {
        let mut scheduled: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (i, event) in c.store.events().iter().enumerate() {
            if i.is_multiple_of(256) {
                portfolio_compute_control::checkpoint()?;
            }
            for ri in 0..c.store.runs().len() {
                if event.run_index.is_none_or(|r| r == ri)
                    && self.groups.contains_key(&(ri, event.owner_index))
                {
                    scheduled
                        .entry((ri, event.owner_index))
                        .or_default()
                        .push(i);
                }
            }
        }
        for (&(ri, oi), indices) in &self.groups {
            portfolio_compute_control::checkpoint()?;
            let pi = c.owner_positions[oi];
            let acquisitions: Vec<_> = indices.iter().map(|i| &self.events[*i]).collect();
            let date = acquisitions[0].input.ordinal;
            if acquisitions.iter().any(|e| e.input.ordinal != date) {
                return Err("acquisition owner requires one exact purchase date per run".into());
            }
            let mut principal = c.spec.positions[pi].balance;
            let mut actions: Vec<(i32, u8, usize, f64)> = acquisitions
                .iter()
                .enumerate()
                .map(|(i, e)| (e.input.ordinal, 0, i, e.input.principal))
                .collect();
            for index in scheduled.get(&(ri, oi)).into_iter().flatten() {
                portfolio_compute_control::checkpoint()?;
                let event = &c.store.events()[*index];
                if !self.event_pools.contains_key(&(ri, *index)) {
                    return Err(
                        "acquisition complete physical path recognition coverage required".into(),
                    );
                }
                if event.payment_ordinal < date
                    || c.store.segments()[event.recognition_range.0..event.recognition_range.1]
                        .iter()
                        .any(|s| s.start_ordinal < date)
                {
                    return Err(
                        "acquisition recognition/settlement precedes actual purchase".into(),
                    );
                }
                actions.push((
                    event.payment_ordinal,
                    1,
                    event.index,
                    -event.components.principal.abs(),
                ));
            }
            actions.sort_by_key(|a| (a.0, a.1, a.2));
            for (_, _, _, delta) in actions {
                portfolio_compute_control::checkpoint()?;
                let next = principal + delta;
                money(next)?;
                if next < 0. && next.abs() > 1e-9 + 1e-12 * principal.abs() {
                    return Err(
                        "acquisition chronological settlement exceeds live principal".into(),
                    );
                }
                principal = next;
            }
            if principal.abs()
                > 1e-9
                    + 1e-12
                        * acquisitions
                            .iter()
                            .map(|e| e.input.principal)
                            .sum::<f64>()
                            .abs()
            {
                return Err("acquisition complete principal tail remains unowned".into());
            }
        }
        Ok(())
    }
}
