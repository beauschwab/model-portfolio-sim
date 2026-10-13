//! Bounded immutable event/accrual schedules for the dated daily route.
//! This store never changes cashflows for default, sale, prepay, or new draws.
//! Scheduled recognition and rights are inputs to a separately owned coordinator,
//! not proof that scheduled expected amounts are actual accounting recognition.

use portfolio_model_core::cashflow::{
    validate_ordinal, CashflowAdjustment, CashflowComponents, CashflowMeasure, DatedSchedule,
    EventProvenance, ScenarioSelector, MAX_EVENTS, MAX_HORIZON_DAYS, MAX_POSITIONS, MAX_SCENARIOS,
};
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub const DATED_STORE_SCHEMA: &str = "dated-store-1";
pub const MAX_STORE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_REPORTING_DAYS: u32 = 1080;
pub const MAX_ACCRUAL_SEGMENTS: usize = 250_000;
const MAX_COVERAGE_ROWS: usize = 250_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    ContractualDatedNativeState,
    FullyScenarioGenerated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Receipt,
    Payment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunIdentity {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub input_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerMapping {
    pub position_id: String,
    pub account_id: String,
    pub currency: String,
    pub direction: Direction,
    pub ownership: Ownership,
    /// Schedule-container identity, distinct from per-event raw/model provenance.
    pub source_id: String,
    pub source_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CoverageStatus {
    CompleteEventSet {},
    NoEvents { rationale: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionCoverage {
    pub position_id: String,
    pub run: ScenarioSelector,
    pub status: CoverageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSource {
    pub source_id: String,
    pub source_revision: String,
    pub schedule: DatedSchedule,
    pub coverage: Vec<PositionCoverage>,
}

/// Recognition amount distributed uniformly within an EXPLICIT supplied actual
/// calendar interval. Distinct intervals can encode daily compounding, varying
/// balances, ACT/ACT year splits or producer-generated state-aware recognition.
/// No distribution rule is inferred from a period's total amount.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionSegment {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub accrued_interest: f64,
    pub book_amortization: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccrualPlanInput {
    pub position_id: String,
    pub event_id: String,
    /// Must match the event selector exactly, not silently cover extra runs.
    pub run: ScenarioSelector,
    pub segments: Vec<RecognitionSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatedStoreRequest {
    pub schema: String,
    pub as_of_ordinal: i32,
    pub reporting_horizon_days: u32,
    pub runs: Vec<RunIdentity>,
    pub owners: Vec<OwnerMapping>,
    pub sources: Vec<ScheduleSource>,
    pub accrual_plans: Vec<AccrualPlanInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredProvenance {
    Contractual {
        source_id: Arc<str>,
        source_revision: Arc<str>,
    },
    Modeled {
        model_id: Arc<str>,
        model_version: Arc<str>,
        input_revision: Arc<str>,
    },
    Assumed {
        assumption_id: Arc<str>,
        rationale: Arc<str>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredOwner {
    pub position_id: Arc<str>,
    pub account_id: Arc<str>,
    pub currency: Arc<str>,
    pub direction: Direction,
    pub ownership: Ownership,
    pub source_id: Arc<str>,
    pub source_revision: Arc<str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredRun {
    pub run_id: Arc<str>,
    pub scenario_id: Arc<str>,
    pub severity: f64,
    pub input_revision: Arc<str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredEvent {
    pub index: usize,
    pub owner_index: usize,
    pub event_id: Arc<str>,
    /// None means explicitly common to all admitted runs.
    pub run_index: Option<usize>,
    pub payment_ordinal: i32,
    pub day_offset: u32,
    pub full_contract_end_ordinal: i32,
    pub accrual_period: Option<portfolio_model_core::cashflow::AccrualPeriod>,
    pub components: CashflowComponents,
    pub measure: CashflowMeasure,
    pub provenance: Arc<StoredProvenance>,
    /// Range into the retained explicit recognition-segment vector.
    pub recognition_range: (usize, usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct RecognitionDelta {
    pub accrued_interest: f64,
    pub book_amortization: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    BeforeEvents,
    AfterEvents,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ScheduledRights {
    pub earned_interest_to_date: f64,
    pub book_amortization_to_date: f64,
    pub cash_interest_settled: f64,
    /// Positive = signed asset right, negative = signed liability obligation.
    /// Early cash receipt can produce deferred income; early payment prepaid
    /// expense. This right persists independently of principal settlement.
    pub signed_interest_right: f64,
    pub cash_settled: bool,
    pub accrual_complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccrualBoundaryKind {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AccrualBoundary {
    pub event_index: usize,
    pub segment_index: usize,
    pub kind: AccrualBoundaryKind,
}

#[derive(Debug)]
pub struct DatedStore {
    as_of_ordinal: i32,
    reporting_horizon_days: u32,
    runs: Vec<StoredRun>,
    owners: Vec<StoredOwner>,
    events: Vec<StoredEvent>,
    segments: Vec<RecognitionSegment>,
    settlements: BTreeMap<u32, Vec<usize>>,
    accrual_boundaries: BTreeMap<i32, Vec<AccrualBoundary>>,
    // Retaining the dictionary makes shared ownership/memory accounting explicit.
    strings: Vec<Arc<str>>,
    provenances: Vec<Arc<StoredProvenance>>,
    metadata_bytes: usize,
}

#[derive(Default)]
struct Interner {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, usize>,
    bytes: usize,
}
impl Interner {
    fn text(&mut self, value: &str) -> Arc<str> {
        if let Some(&i) = self.lookup.get(value) {
            return self.strings[i].clone();
        }
        let text: Arc<str> = value.into();
        self.lookup.insert(text.clone(), self.strings.len());
        self.strings.push(text.clone());
        self.bytes += value.len();
        text
    }
    fn provenance(&mut self, value: &EventProvenance) -> StoredProvenance {
        match value {
            EventProvenance::Contractual {
                source_id,
                source_revision,
            } => StoredProvenance::Contractual {
                source_id: self.text(source_id),
                source_revision: self.text(source_revision),
            },
            EventProvenance::Modeled {
                model_id,
                model_version,
                input_revision,
            } => StoredProvenance::Modeled {
                model_id: self.text(model_id),
                model_version: self.text(model_version),
                input_revision: self.text(input_revision),
            },
            EventProvenance::Assumed {
                assumption_id,
                rationale,
            } => StoredProvenance::Assumed {
                assumption_id: self.text(assumption_id),
                rationale: self.text(rationale),
            },
        }
    }
}

type EventKey = (usize, String, Option<usize>);
type CoverageKey = (usize, Option<usize>);

impl DatedStore {
    pub fn build(request: DatedStoreRequest) -> Result<Self, String> {
        validate_request(&request)?;
        let mut intern = Interner::default();
        let runs: Vec<_> = request
            .runs
            .iter()
            .map(|r| StoredRun {
                run_id: intern.text(&r.run_id),
                scenario_id: intern.text(&r.scenario_id),
                severity: r.severity,
                input_revision: intern.text(&r.input_revision),
            })
            .collect();
        let run_index: HashMap<_, _> = runs
            .iter()
            .enumerate()
            .map(|(i, r)| (r.run_id.to_string(), i))
            .collect();
        let owners: Vec<_> = request
            .owners
            .iter()
            .map(|p| StoredOwner {
                position_id: intern.text(&p.position_id),
                account_id: intern.text(&p.account_id),
                currency: intern.text(&p.currency),
                direction: p.direction,
                ownership: p.ownership,
                source_id: intern.text(&p.source_id),
                source_revision: intern.text(&p.source_revision),
            })
            .collect();
        let owner_index: HashMap<_, _> = owners
            .iter()
            .enumerate()
            .map(|(i, p)| (p.position_id.to_string(), i))
            .collect();
        let declared_runs: HashSet<_> = run_index.keys().map(String::as_str).collect();
        let mut paired = Vec::new();
        let mut coverage = HashMap::<CoverageKey, CoverageStatus>::new();
        let mut count_by_owner_run = HashMap::<CoverageKey, usize>::new();
        let mut provenance_lookup = HashMap::<StoredProvenance, usize>::new();
        let mut provenances = Vec::<Arc<StoredProvenance>>::new();
        for source in request.sources {
            portfolio_compute_control::checkpoint()?;
            if source.schedule.as_of_ordinal != request.as_of_ordinal
                || source
                    .schedule
                    .scenarios
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>()
                    != declared_runs
            {
                return Err("dated sources must share valuation date and exactly cover explicit run identities".into());
            }
            for row in source.coverage {
                let i = *owner_index
                    .get(&row.position_id)
                    .ok_or("coverage names unknown position")?;
                validate_source_owner(&source.source_id, &source.source_revision, &owners[i])?;
                let run = resolve_selector(&row.run, &run_index)?;
                if coverage.insert((i, run), row.status).is_some() {
                    return Err("duplicate dated position/run coverage".into());
                }
            }
            let full_end = source.schedule.as_of_ordinal + source.schedule.horizon_days as i32;
            for event in source.schedule.events {
                let i = *owner_index
                    .get(&event.position_id)
                    .ok_or("dated event names unknown owner")?;
                let owner = &owners[i];
                validate_source_owner(&source.source_id, &source.source_revision, owner)?;
                if event.currency != owner.currency.as_ref() {
                    return Err("dated event/owner currency mismatch".into());
                }
                if event.measure == CashflowMeasure::Pricing {
                    return Err("pricing-measure events cannot enter physical GL".into());
                }
                if owner.ownership == Ownership::ContractualDatedNativeState
                    && event.measure != CashflowMeasure::Contractual
                {
                    return Err(
                        "contractual native-state ownership requires contractual source events"
                            .into(),
                    );
                }
                if !matches!(event.adjustment, CashflowAdjustment::None {}) {
                    return Err(
                        "dated native-owned events cannot carry proportional survival overlays"
                            .into(),
                    );
                }
                let principal_sign = if owner.direction == Direction::Receipt {
                    1.0
                } else {
                    -1.0
                };
                if principal_sign * event.components.principal < 0.0 {
                    return Err("principal polarity conflicts with explicit owner direction".into());
                }
                let run = resolve_selector(&event.scenario, &run_index)?;
                *count_by_owner_run.entry((i, run)).or_default() += 1;
                let value = intern.provenance(&event.provenance);
                let provenance = if let Some(&j) = provenance_lookup.get(&value) {
                    provenances[j].clone()
                } else {
                    let shared = Arc::new(value.clone());
                    provenance_lookup.insert(value, provenances.len());
                    provenances.push(shared.clone());
                    shared
                };
                paired.push(StoredEvent {
                    index: 0,
                    owner_index: i,
                    event_id: intern.text(&event.event_id),
                    run_index: run,
                    payment_ordinal: event.payment_ordinal,
                    day_offset: event.day_offset,
                    full_contract_end_ordinal: full_end,
                    accrual_period: event.accrual_period,
                    components: event.components,
                    measure: event.measure,
                    provenance,
                    recognition_range: (0, 0),
                });
            }
        }
        let named_coverage_owners: HashSet<_> = coverage
            .keys()
            .filter_map(|&(owner, run)| run.map(|_| owner))
            .collect();
        for i in 0..owners.len() {
            let common = coverage.get(&(i, None));
            if common.is_some() && named_coverage_owners.contains(&i) {
                return Err("all-run and named position coverage declarations overlap".into());
            }
            for run in 0..runs.len() {
                let status = common
                    .or_else(|| coverage.get(&(i, Some(run))))
                    .ok_or("missing explicit dated position/run coverage")?;
                let count = count_by_owner_run.get(&(i, None)).copied().unwrap_or(0)
                    + count_by_owner_run
                        .get(&(i, Some(run)))
                        .copied()
                        .unwrap_or(0);
                match status {
                    CoverageStatus::CompleteEventSet {} if count == 0 => {
                        return Err(
                            "complete event coverage has no events; declare no_events explicitly"
                                .into(),
                        )
                    }
                    CoverageStatus::NoEvents { .. } if count != 0 => {
                        return Err("no_events declaration conflicts with retained schedule".into())
                    }
                    _ => {}
                }
            }
        }
        paired.sort_by_key(|e| e.payment_ordinal);
        let mut lookup = HashMap::<EventKey, usize>::new();
        let mut identities = HashMap::<(usize, Arc<str>), (bool, HashSet<usize>)>::new();
        for (i, event) in paired.iter_mut().enumerate() {
            if i.is_multiple_of(1024) {
                portfolio_compute_control::checkpoint()?;
            }
            event.index = i;
            let identity = identities
                .entry((event.owner_index, event.event_id.clone()))
                .or_default();
            match event.run_index {
                None if identity.0 || !identity.1.is_empty() => {
                    return Err("duplicate event identity or all/named event overlap".into())
                }
                None => identity.0 = true,
                Some(run) if identity.0 || !identity.1.insert(run) => {
                    return Err("duplicate event identity or all/named event overlap".into())
                }
                Some(_) => {}
            }
            lookup.insert(
                (
                    event.owner_index,
                    event.event_id.to_string(),
                    event.run_index,
                ),
                i,
            );
        }
        let mut segments = Vec::new();
        let mut seen_plans = HashSet::new();
        for plan in request.accrual_plans {
            portfolio_compute_control::checkpoint()?;
            let owner = *owner_index
                .get(&plan.position_id)
                .ok_or("accrual plan names unknown owner")?;
            let run = resolve_selector(&plan.run, &run_index)?;
            let i = *lookup
                .get(&(owner, plan.event_id.clone(), run))
                .ok_or("accrual plan has no matching exact event/run identity")?;
            if !seen_plans.insert(i) {
                return Err("duplicate accrual plan for event identity".into());
            }
            let event = &mut paired[i];
            validate_plan(event, &plan.segments)?;
            let start = segments.len();
            segments.extend(plan.segments);
            event.recognition_range = (start, segments.len());
        }
        for event in &paired {
            if (event.components.accrued_interest != 0.0
                || event.components.book_amortization != 0.0)
                && !seen_plans.contains(&event.index)
            {
                return Err(
                    "noncash period amount requires explicit scheduled accrual segments".into(),
                );
            }
        }
        let mut settlements = BTreeMap::<u32, Vec<usize>>::new();
        let mut boundaries = BTreeMap::<i32, Vec<AccrualBoundary>>::new();
        for event in &paired {
            settlements
                .entry(event.day_offset)
                .or_default()
                .push(event.index);
            for (j, segment) in segments
                .iter()
                .enumerate()
                .take(event.recognition_range.1)
                .skip(event.recognition_range.0)
            {
                boundaries
                    .entry(segment.start_ordinal)
                    .or_default()
                    .push(AccrualBoundary {
                        event_index: event.index,
                        segment_index: j,
                        kind: AccrualBoundaryKind::Start,
                    });
                boundaries
                    .entry(segment.end_ordinal)
                    .or_default()
                    .push(AccrualBoundary {
                        event_index: event.index,
                        segment_index: j,
                        kind: AccrualBoundaryKind::End,
                    });
            }
        }
        // End old segments before starting new ones at the same boundary.
        for rows in boundaries.values_mut() {
            rows.sort_by_key(|r| {
                if r.kind == AccrualBoundaryKind::End {
                    0
                } else {
                    1
                }
            });
        }
        let store = Self {
            as_of_ordinal: request.as_of_ordinal,
            reporting_horizon_days: request.reporting_horizon_days,
            runs,
            owners,
            events: paired,
            segments,
            settlements,
            accrual_boundaries: boundaries,
            strings: intern.strings,
            provenances,
            metadata_bytes: intern.bytes,
        };
        if store.retained_capacity_bytes() > MAX_STORE_BYTES {
            return Err("dated store retained capacity exceeds 128 MiB admission".into());
        }
        Ok(store)
    }

    pub fn as_of_ordinal(&self) -> i32 {
        self.as_of_ordinal
    }
    pub fn reporting_horizon_days(&self) -> u32 {
        self.reporting_horizon_days
    }
    pub fn reporting_end_ordinal(&self) -> i32 {
        self.as_of_ordinal + self.reporting_horizon_days as i32
    }
    pub fn runs(&self) -> &[StoredRun] {
        &self.runs
    }
    pub fn owners(&self) -> &[StoredOwner] {
        &self.owners
    }
    pub fn events(&self) -> &[StoredEvent] {
        &self.events
    }
    pub fn segments(&self) -> &[RecognitionSegment] {
        &self.segments
    }
    pub fn interned_string_count(&self) -> usize {
        self.strings.len()
    }
    pub fn provenance_count(&self) -> usize {
        self.provenances.len()
    }

    pub fn retained_capacity_bytes(&self) -> usize {
        self.metadata_bytes+self.strings.capacity()*std::mem::size_of::<Arc<str>>()
            // Arc allocations include two reference counters and alignment.
            +self.strings.len()*2*std::mem::size_of::<usize>()
            +self.provenances.capacity()*std::mem::size_of::<Arc<StoredProvenance>>()
            +self.provenances.len()*(std::mem::size_of::<StoredProvenance>()+2*std::mem::size_of::<usize>())
            +self.runs.capacity()*std::mem::size_of::<StoredRun>()
            +self.owners.capacity()*std::mem::size_of::<StoredOwner>()
            +self.events.capacity()*std::mem::size_of::<StoredEvent>()
            +self.segments.capacity()*std::mem::size_of::<RecognitionSegment>()
            +self.settlements.values().map(|v|v.capacity()*std::mem::size_of::<usize>()).sum::<usize>()
            +self.accrual_boundaries.values().map(|v|v.capacity()*std::mem::size_of::<AccrualBoundary>()).sum::<usize>()
            // Conservative B-tree node overhead charge; not allocator RSS.
            +(self.settlements.len()+self.accrual_boundaries.len())*128
    }

    fn run_index(&self, run_id: &str) -> Result<usize, String> {
        self.runs
            .iter()
            .position(|r| r.run_id.as_ref() == run_id)
            .ok_or("unknown dated run; no scenario/severity fallback".into())
    }
    fn event_for_run(&self, run: usize, index: usize) -> Result<&StoredEvent, String> {
        let event = self.events.get(index).ok_or("unknown dated event index")?;
        if event.run_index.is_some_and(|i| i != run) {
            return Err("dated event is not admitted for requested run".into());
        }
        Ok(event)
    }
    pub fn settlements_on(
        &self,
        run_id: &str,
        day: u32,
    ) -> Result<impl Iterator<Item = &StoredEvent>, String> {
        let run = self.run_index(run_id)?;
        if day > self.reporting_horizon_days {
            return Err("settlement query exceeds reporting horizon; use full tail access".into());
        }
        Ok(self
            .settlements
            .get(&day)
            .into_iter()
            .flatten()
            .filter_map(move |&i| {
                let event = &self.events[i];
                if event.run_index.is_none_or(|r| r == run) {
                    Some(event)
                } else {
                    None
                }
            }))
    }
    pub fn tail_events(&self, run_id: &str) -> Result<impl Iterator<Item = &StoredEvent>, String> {
        let run = self.run_index(run_id)?;
        Ok(self.events.iter().filter(move |e| {
            e.day_offset > self.reporting_horizon_days && e.run_index.is_none_or(|r| r == run)
        }))
    }
    pub fn pending_accrual_events(
        &self,
        run_id: &str,
    ) -> Result<impl Iterator<Item = &StoredEvent>, String> {
        let run = self.run_index(run_id)?;
        let end = self.reporting_end_ordinal();
        Ok(self.events.iter().filter(move |e| {
            e.accrual_period.is_some_and(|p| p.end_ordinal > end)
                && e.recognition_range.0 < e.recognition_range.1
                && e.run_index.is_none_or(|r| r == run)
        }))
    }
    pub fn opening_accrual_events(
        &self,
        run_id: &str,
    ) -> Result<impl Iterator<Item = &StoredEvent>, String> {
        let run = self.run_index(run_id)?;
        let asof = self.as_of_ordinal;
        Ok(self.events.iter().filter(move |e| {
            e.accrual_period
                .is_some_and(|p| p.start_ordinal < asof && p.end_ordinal > asof)
                && e.recognition_range.0 < e.recognition_range.1
                && e.run_index.is_none_or(|r| r == run)
        }))
    }
    pub fn accrual_boundaries_on(
        &self,
        run_id: &str,
        ordinal: i32,
    ) -> Result<impl Iterator<Item = &AccrualBoundary>, String> {
        let run = self.run_index(run_id)?;
        validate_ordinal(ordinal)?;
        Ok(self
            .accrual_boundaries
            .get(&ordinal)
            .into_iter()
            .flatten()
            .filter(move |r| {
                self.events[r.event_index]
                    .run_index
                    .is_none_or(|i| i == run)
            }))
    }
    /// Scheduled signed recognition between exact boundaries, clamped only to
    /// supplied segment coverage. This is not survival-adjusted actual NII.
    pub fn accrual_delta(
        &self,
        run_id: &str,
        event_index: usize,
        start: i32,
        end: i32,
    ) -> Result<RecognitionDelta, String> {
        let run = self.run_index(run_id)?;
        let event = self.event_for_run(run, event_index)?;
        validate_ordinal(start)?;
        validate_ordinal(end)?;
        if end < start {
            return Err("dated recognition interval is reversed".into());
        }
        let values = &self.segments[event.recognition_range.0..event.recognition_range.1];
        let first = values.partition_point(|s| s.end_ordinal <= start);
        let last = values.partition_point(|s| s.start_ordinal < end);
        let mut interest = StableSum::default();
        let mut amort = StableSum::default();
        for segment in &values[first..last.max(first)] {
            let a = segment.start_ordinal.max(start);
            let b = segment.end_ordinal.min(end);
            if a >= b {
                continue;
            }
            // Cumulative endpoints ensure adjacent interval partitions telescope
            // and a complete segment returns exactly its supplied amount.
            let fraction = |date: i32| {
                f64::from(date - segment.start_ordinal)
                    / f64::from(segment.end_ordinal - segment.start_ordinal)
            };
            let ia = segment.accrued_interest * fraction(a);
            let ib = segment.accrued_interest * fraction(b);
            let aa = segment.book_amortization * fraction(a);
            let ab = segment.book_amortization * fraction(b);
            interest.add(ib - ia);
            amort.add(ab - aa);
        }
        let result = RecognitionDelta {
            accrued_interest: interest.value(),
            book_amortization: amort.value(),
        };
        amount(result.accrued_interest)?;
        amount(result.book_amortization)?;
        Ok(result)
    }
    pub fn rights_at(
        &self,
        run_id: &str,
        event_index: usize,
        ordinal: i32,
        boundary: Boundary,
    ) -> Result<ScheduledRights, String> {
        let run = self.run_index(run_id)?;
        let event = self.event_for_run(run, event_index)?;
        validate_ordinal(ordinal)?;
        let earned = if let Some(period) = event.accrual_period {
            self.accrual_delta(
                run_id,
                event_index,
                period.start_ordinal,
                ordinal.max(period.start_ordinal),
            )?
        } else {
            RecognitionDelta::default()
        };
        let settled = event.payment_ordinal < ordinal
            || (event.payment_ordinal == ordinal && boundary == Boundary::AfterEvents);
        let cash = if settled {
            event.components.cash_interest
        } else {
            0.0
        };
        let right = earned.accrued_interest - cash;
        amount(right)?;
        Ok(ScheduledRights {
            earned_interest_to_date: earned.accrued_interest,
            book_amortization_to_date: earned.book_amortization,
            cash_interest_settled: cash,
            signed_interest_right: right,
            cash_settled: settled,
            accrual_complete: event
                .accrual_period
                .is_none_or(|p| ordinal >= p.end_ordinal),
        })
    }
}

#[derive(Default)]
struct StableSum {
    total: f64,
    correction: f64,
}
impl StableSum {
    fn add(&mut self, value: f64) {
        let next = self.total + value;
        self.correction += if self.total.abs() >= value.abs() {
            (self.total - next) + value
        } else {
            (value - next) + self.total
        };
        self.total = next;
    }
    fn value(&self) -> f64 {
        self.total + self.correction
    }
}

fn text(value: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err("dated identities must be bounded nonempty text".into());
    }
    Ok(())
}
fn amount(value: f64) -> Result<(), String> {
    if !value.is_finite() || value.abs() > MAX_MONETARY_AMOUNT {
        return Err("dated recognition amount exceeds finite monetary admission".into());
    }
    Ok(())
}
fn resolve_selector(
    selector: &ScenarioSelector,
    runs: &HashMap<String, usize>,
) -> Result<Option<usize>, String> {
    match selector {
        ScenarioSelector::All {} => Ok(None),
        ScenarioSelector::Named { scenario_id } => Ok(Some(
            *runs
                .get(scenario_id)
                .ok_or("dated selector names unknown run")?,
        )),
    }
}
fn validate_source_owner(source: &str, revision: &str, owner: &StoredOwner) -> Result<(), String> {
    if owner.source_id.as_ref() != source || owner.source_revision.as_ref() != revision {
        return Err("position is bound to a different dated source/revision".into());
    }
    Ok(())
}
fn validate_plan(event: &StoredEvent, segments: &[RecognitionSegment]) -> Result<(), String> {
    let period = event
        .accrual_period
        .ok_or("scheduled recognition requires explicit accrual endpoints")?;
    if segments.is_empty() {
        return Err("explicit accrual plan must contain recognition segments".into());
    }
    let mut cursor = period.start_ordinal;
    let mut interest = StableSum::default();
    let mut amort = StableSum::default();
    for segment in segments {
        validate_ordinal(segment.start_ordinal)?;
        validate_ordinal(segment.end_ordinal)?;
        amount(segment.accrued_interest)?;
        amount(segment.book_amortization)?;
        if segment.start_ordinal != cursor
            || segment.end_ordinal <= cursor
            || segment.end_ordinal > period.end_ordinal
        {
            return Err("explicit accrual segments must exactly partition the event period".into());
        }
        cursor = segment.end_ordinal;
        interest.add(segment.accrued_interest);
        amort.add(segment.book_amortization);
    }
    if cursor != period.end_ordinal {
        return Err("explicit accrual plan omits part of the event period".into());
    }
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()));
    if !close(interest.value(), event.components.accrued_interest)
        || !close(amort.value(), event.components.book_amortization)
    {
        return Err("scheduled recognition segments do not reconcile period components".into());
    }
    Ok(())
}

fn validate_request(request: &DatedStoreRequest) -> Result<(), String> {
    if request.schema != DATED_STORE_SCHEMA {
        return Err("unsupported dated store schema".into());
    }
    validate_ordinal(request.as_of_ordinal)?;
    if request.reporting_horizon_days > MAX_REPORTING_DAYS
        || i64::from(request.as_of_ordinal) + i64::from(request.reporting_horizon_days)
            > i64::from(portfolio_model_core::cashflow::MAX_ORDINAL)
    {
        return Err("invalid bounded dated reporting horizon".into());
    }
    if request.runs.is_empty()
        || request.runs.len() > MAX_SCENARIOS
        || request.owners.len() > MAX_POSITIONS
        || request.sources.len() > MAX_POSITIONS
        || request.accrual_plans.len() > MAX_EVENTS
    {
        return Err("dated run/position/source/plan admission exceeded".into());
    }
    let mut bytes = request.schema.len();
    let mut admit = |length: usize| -> Result<(), String> {
        bytes = bytes.checked_add(length).ok_or("dated metadata overflow")?;
        if bytes > MAX_DATED_METADATA_BYTES {
            return Err("dated input metadata exceeds 32 MiB admission".into());
        }
        Ok(())
    };
    let mut runs = HashSet::new();
    let mut cases = HashSet::new();
    for run in &request.runs {
        text(&run.run_id, 128)?;
        text(&run.scenario_id, 128)?;
        text(&run.input_revision, 256)?;
        if !run.severity.is_finite()
            || !(0.0..=100.0).contains(&run.severity)
            || !runs.insert(run.run_id.as_str())
            || !cases.insert((&run.scenario_id, run.severity.to_bits()))
        {
            return Err("invalid/duplicate dated run or scenario/severity identity".into());
        }
        admit(run.run_id.len() + run.scenario_id.len() + run.input_revision.len())?;
    }
    let mut owners = HashSet::new();
    for owner in &request.owners {
        for label in [
            &owner.position_id,
            &owner.account_id,
            &owner.source_id,
            &owner.source_revision,
        ] {
            text(label, 256)?;
            admit(label.len())?;
        }
        if owner.currency.len() != 3
            || !owner.currency.bytes().all(|c| c.is_ascii_uppercase())
            || !owners.insert(&owner.position_id)
        {
            return Err("invalid/duplicate dated position or currency".into());
        }
        admit(owner.currency.len())?;
    }
    let mut sources = HashSet::new();
    let mut events = 0usize;
    let mut coverage = 0usize;
    for source in &request.sources {
        portfolio_compute_control::checkpoint()?;
        text(&source.source_id, 256)?;
        text(&source.source_revision, 256)?;
        if !sources.insert(&source.source_id) {
            return Err("duplicate dated source identity (including alternative revisions)".into());
        }
        admit(source.source_id.len() + source.source_revision.len())?;
        source.schedule.validate()?;
        if source.schedule.horizon_days < request.reporting_horizon_days {
            return Err(
                "full contractual schedule horizon cannot end before reporting horizon".into(),
            );
        }
        events = events
            .checked_add(source.schedule.events.len())
            .ok_or("dated event count overflow")?;
        coverage = coverage
            .checked_add(source.coverage.len())
            .ok_or("dated coverage count overflow")?;
        if events > MAX_EVENTS || coverage > MAX_COVERAGE_ROWS {
            return Err("dated total event/coverage admission exceeded".into());
        }
        admit(source.schedule.schema.len())?;
        for run in &source.schedule.scenarios {
            admit(run.len())?;
        }
        for event in &source.schedule.events {
            admit(event.position_id.len() + event.event_id.len() + event.currency.len())?;
            if let ScenarioSelector::Named { scenario_id } = &event.scenario {
                admit(scenario_id.len())?;
            }
            match &event.provenance {
                EventProvenance::Contractual {
                    source_id,
                    source_revision,
                } => admit(source_id.len() + source_revision.len())?,
                EventProvenance::Modeled {
                    model_id,
                    model_version,
                    input_revision,
                } => admit(model_id.len() + model_version.len() + input_revision.len())?,
                EventProvenance::Assumed {
                    assumption_id,
                    rationale,
                } => admit(assumption_id.len() + rationale.len())?,
            }
        }
        for row in &source.coverage {
            text(&row.position_id, 256)?;
            admit(row.position_id.len())?;
            if let ScenarioSelector::Named { scenario_id } = &row.run {
                text(scenario_id, 128)?;
                admit(scenario_id.len())?;
            }
            if let CoverageStatus::NoEvents { rationale } = &row.status {
                text(rationale, 1024)?;
                admit(rationale.len())?;
            }
        }
    }
    for owner in &request.owners {
        if !sources.contains(&owner.source_id) {
            return Err("owner references missing dated source".into());
        }
    }
    let mut count = 0usize;
    for plan in &request.accrual_plans {
        text(&plan.position_id, 256)?;
        text(&plan.event_id, 256)?;
        admit(plan.position_id.len() + plan.event_id.len())?;
        if let ScenarioSelector::Named { scenario_id } = &plan.run {
            text(scenario_id, 128)?;
            admit(scenario_id.len())?;
        }
        count = count
            .checked_add(plan.segments.len())
            .ok_or("dated segment count overflow")?;
        if count > MAX_ACCRUAL_SEGMENTS {
            return Err("dated recognition segment admission exceeded".into());
        }
    }
    if events
        .checked_mul(request.runs.len())
        .is_none_or(|work| work > MAX_EVENTS * MAX_SCENARIOS)
    {
        return Err("dated event/run work admission exceeded".into());
    }
    // Request dates are bounded by core; this also documents that recognition
    // plans cannot introduce another unbounded historical date domain.
    for source in &request.sources {
        for event in &source.schedule.events {
            if let Some(period) = event.accrual_period {
                if request.as_of_ordinal - period.start_ordinal > MAX_HORIZON_DAYS as i32 {
                    return Err("dated accrual history exceeds 100-year domain".into());
                }
            }
        }
    }
    Ok(())
}
