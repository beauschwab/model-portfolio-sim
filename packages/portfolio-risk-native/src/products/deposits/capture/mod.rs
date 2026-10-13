//! Par deposit physical runoff -> signed actual-date liability rights.
//! The graph supplies P/Q explicitly; only one complete P path per run is captured.
pub mod income;
use crate::{
    advanced_graph::{
        CaptureCoverage, CaptureCoverageStatus, CaptureOpening, CaptureOwner, CapturePlan,
        CapturePolicy, CaptureSegment, CaptureSource, Role, ScheduledCapture,
    },
    deposit_graph::{self, DepositGraph},
};
use portfolio_model_core::{
    cashflow::{
        AccrualPeriod, CashflowAdjustment, CashflowComponents, CashflowMeasure, DatedCashflowEvent,
        DatedSchedule, EventProvenance, ScenarioSelector,
    },
    deposit_state::TerminalPolicy,
    limits::MAX_MONETARY_AMOUNT,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
};
pub const SCHEMA: &str = "deposit-graph-capture-1";
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_SEGMENTS: usize = 250_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub graph: deposit_graph::Request,
    pub owners: Vec<CaptureOwner>,
    pub policy: CapturePolicy,
}
#[derive(Clone, Serialize)]
pub struct CashOrigin {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub event_id: String,
    pub native_event_id: String,
    pub segment_id: String,
    pub kind: String,
    pub ordinal: i32,
}
#[derive(Clone, Serialize)]
pub struct Capture {
    pub scheduled: ScheduledCapture,
    pub cash_origins: Vec<CashOrigin>,
}
#[derive(Clone, Serialize)]
pub struct Prepared {
    pub schema: String,
    pub raw_request: Request,
    pub graph: deposit_graph::ResultData,
    pub capture: Capture,
}
struct Counter {
    size: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        self.size = self.size.saturating_add(b.len());
        if self.size > self.limit {
            return Err(std::io::Error::other(
                "deposit capture byte admission exceeded",
            ));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded<T: Serialize>(v: &T, limit: usize) -> Result<(), String> {
    serde_json::to_writer(&mut Counter { size: 0, limit }, v).map_err(|e| e.to_string())
}
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 120 || v.chars().any(char::is_control) {
        Err("deposit capture identity must be bounded nonblank text".into())
    } else {
        Ok(())
    }
}
fn money(v: f64) -> Result<f64, String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        Err("deposit capture monetary admission exceeded".into())
    } else {
        Ok(v)
    }
}
/// Capture an already admitted graph without projecting or calibrating it again.
pub fn capture(
    graph: &DepositGraph,
    owners: &[CaptureOwner],
    policy: &CapturePolicy,
) -> Result<Capture, String> {
    portfolio_compute_control::checkpoint()?;
    let request = graph.request();
    let result = graph.snapshot();
    label(&policy.policy_id)?;
    label(&policy.policy_revision)?;
    if policy.recognition_model != "deposit_daily_par-1"
        || policy.book_basis != "par_deposit_simple_interest"
        || policy.fees != "reject"
        || owners.len() != request.positions.len()
    {
        return Err("deposit capture requires explicit gross daily par basis/fee policy and complete owners; no EIR inferred".into());
    }
    let mut owner_ids = HashSet::new();
    let mut source_ids = HashSet::new();
    for o in owners {
        for v in [
            &o.position_id,
            &o.account_id,
            &o.source_id,
            &o.source_revision,
        ] {
            label(v)?;
        }
        if !owner_ids.insert(&o.position_id)
            || !source_ids.insert(&o.source_id)
            || o.direction != "payment"
            || o.ownership != "fully_scenario_generated"
            || o.source_revision != request.identity.input_revision
            || !request
                .positions
                .iter()
                .any(|p| p.position_id == o.position_id)
        {
            return Err(
                "deposit owner/source must be unique explicit payment liability ownership".into(),
            );
        }
    }
    let mut sources = Vec::new();
    let mut plans = Vec::new();
    let mut openings = Vec::new();
    let mut cash_origins = Vec::new();
    let mut segment_count = 0usize;
    for position in &request.positions {
        portfolio_compute_control::checkpoint()?;
        let owner = owners
            .iter()
            .find(|o| o.position_id == position.position_id)
            .ok_or("missing deposit owner")?;
        let mut events = Vec::new();
        let mut coverage = Vec::new();
        let mut horizon = None;
        let mut opening = None;
        for scenario in &request.scenarios {
            portfolio_compute_control::checkpoint()?;
            let set = position
                .histories
                .iter()
                .find(|h| h.scenario_id == scenario.scenario_id)
                .ok_or("missing deposit physical history")?;
            if set.physical.len() != 1 || set.physical[0].weight != 1. {
                return Err(
                    "deposit capture requires one explicit physical path weight one per run".into(),
                );
            }
            let raw = &set.physical[0].request;
            if !raw.migrations.is_empty()
                || raw
                    .drivers
                    .iter()
                    .any(|d| d.external_inflows.iter().any(|v| *v != 0.))
                || raw.segments.iter().any(|s| {
                    s.opening_accrued_interest != 0.
                        || s.opening_paid_rate < 0.
                        || s.repricing.floor < 0.
                })
            {
                return Err("deposit migration/inflow/opening accrued/negative-rate capture unsupported; no partial source pruning".into());
            }
            let matches: Vec<_> = result
                .native_sources
                .iter()
                .filter(|s| {
                    s.position_id == position.position_id
                        && s.scenario_id == scenario.scenario_id
                        && s.role == Role::Physical
                })
                .collect();
            if matches.len() != 1
                || matches[0].weight != 1.
                || matches[0].projection.identity.measure != CashflowMeasure::Physical
            {
                return Err("deposit physical source identity differs".into());
            }
            let source = matches[0];
            let projection = &source.projection;
            if !projection.migrations.is_empty()
                || projection.total_external_inflows != 0.
                || projection.interest.iter().any(|v| v.amount < 0.)
                || projection
                    .cashflows
                    .iter()
                    .any(|v| v.principal > 0. || v.cash_interest > 0.)
            {
                return Err("deposit source exceeds runoff/nonnegative-expense ownership".into());
            }
            let principal = money(raw.segments.iter().map(|s| s.opening_balance).sum())?;
            if opening.is_some_and(|old| old != principal)
                || horizon.is_some_and(|old| old != raw.horizon_days)
            {
                return Err("deposit capture original opening/horizon differs across runs".into());
            }
            opening = Some(principal);
            horizon = Some(raw.horizon_days);
            let selector = ScenarioSelector::Named {
                scenario_id: scenario.run_id.clone(),
            };
            coverage.push(CaptureCoverage {
                position_id: position.position_id.clone(),
                run: selector.clone(),
                status: if projection.cashflows.is_empty() {
                    CaptureCoverageStatus::NoEvents {
                        rationale: "complete native physical deposit projection has no cash events"
                            .into(),
                    }
                } else {
                    CaptureCoverageStatus::CompleteEventSet {}
                },
            });
            let indices: HashMap<_, _> = raw
                .segments
                .iter()
                .enumerate()
                .map(|(i, s)| (s.segment_id.as_str(), i))
                .collect();
            let mut remaining: Vec<_> = projection.interest.iter().map(|v| v.amount).collect();
            let mut origin_indices = vec![Vec::new(); raw.segments.len()];
            for (i, origin) in projection.interest.iter().enumerate() {
                origin_indices[*indices
                    .get(origin.segment_id.as_str())
                    .ok_or("unknown deposit interest segment")?]
                .push(i);
            }
            let mut cursor = vec![0usize; raw.segments.len()];
            for cash in &projection.cashflows {
                portfolio_compute_control::checkpoint()?;
                let si = *indices
                    .get(cash.segment_id.as_str())
                    .ok_or("unknown deposit cash segment")?;
                let event_id = format!("deposit:{si}:{}:{}", cash.kind, cash.ordinal);
                let mut needed = -cash.cash_interest;
                let mut segments = Vec::new();
                while needed > 0. && cursor[si] < origin_indices[si].len() {
                    portfolio_compute_control::checkpoint()?;
                    let oi = origin_indices[si][cursor[si]];
                    let origin = &projection.interest[oi];
                    if origin.end_ordinal > cash.ordinal
                        || origin.start_ordinal >= origin.end_ordinal
                    {
                        return Err("deposit settlement cannot borrow future accrued rights".into());
                    }
                    let allocated = needed.min(remaining[oi]);
                    if allocated > 0. {
                        segments.push(CaptureSegment {
                            origin_id: format!(
                                "deposit-interest:{si}:{}:{}",
                                origin.start_ordinal, origin.end_ordinal
                            ),
                            start_ordinal: origin.start_ordinal,
                            end_ordinal: origin.end_ordinal,
                            accrued_interest: -allocated,
                            book_amortization: 0.,
                        });
                        segment_count += 1;
                        if segment_count > MAX_SEGMENTS {
                            return Err(
                                "deposit capture recognition segments exceed admission".into()
                            );
                        }
                    }
                    remaining[oi] -= allocated;
                    needed -= allocated;
                    if remaining[oi] == 0. {
                        cursor[si] += 1;
                    }
                }
                if needed > 1e-9_f64.max(cash.cash_interest.abs() * 1e-12) {
                    return Err("deposit cash lacks independently earned origin rights".into());
                }
                if !segments.is_empty() {
                    let total: f64 = segments.iter().map(|s| s.accrued_interest).sum();
                    segments.last_mut().unwrap().accrued_interest += cash.cash_interest - total;
                }
                let period = segments
                    .first()
                    .zip(segments.last())
                    .map(|(a, b)| AccrualPeriod {
                        start_ordinal: a.start_ordinal,
                        end_ordinal: b.end_ordinal,
                    });
                if !segments.is_empty() {
                    plans.push(CapturePlan {
                        position_id: position.position_id.clone(),
                        event_id: event_id.clone(),
                        run: selector.clone(),
                        segments,
                    });
                }
                let provenance = if cash.kind == "assumed_terminal_payout" {
                    match &raw.terminal {
                        TerminalPolicy::AssumedLiquidation { rationale } => {
                            EventProvenance::Assumed {
                                assumption_id: raw.identity.assumption_id.clone(),
                                rationale: rationale.clone(),
                            }
                        }
                        _ => {
                            return Err(
                                "deposit assumed terminal payout lacks explicit assumption".into()
                            )
                        }
                    }
                } else {
                    EventProvenance::Modeled {
                        model_id: "deposit-state-1".into(),
                        model_version: "deposit-state-1".into(),
                        input_revision: request.identity.input_revision.clone(),
                    }
                };
                events.push(DatedCashflowEvent {
                    position_id: position.position_id.clone(),
                    event_id: event_id.clone(),
                    scenario: selector.clone(),
                    payment_ordinal: cash.ordinal,
                    day_offset: u32::try_from(cash.ordinal - request.as_of_ordinal)
                        .map_err(|_| "deposit cash precedes opening")?,
                    accrual_period: period,
                    currency: request.currency.clone(),
                    components: CashflowComponents {
                        principal: cash.principal,
                        cash_interest: cash.cash_interest,
                        accrued_interest: cash.cash_interest,
                        book_amortization: 0.,
                        fees: 0.,
                    },
                    measure: CashflowMeasure::Physical,
                    provenance,
                    adjustment: CashflowAdjustment::None {},
                });
                cash_origins.push(CashOrigin {
                    position_id: position.position_id.clone(),
                    scenario_id: scenario.scenario_id.clone(),
                    run_id: scenario.run_id.clone(),
                    event_id,
                    native_event_id: cash.event_id.clone(),
                    segment_id: cash.segment_id.clone(),
                    kind: cash.kind.clone(),
                    ordinal: cash.ordinal,
                });
            }
            if remaining.iter().map(|v| v.abs()).sum::<f64>()
                > 1e-9_f64.max(projection.total_interest_expense.abs() * 1e-12)
            {
                return Err("deposit closing earned rights omitted from capture".into());
            }
        }
        events.sort_by_key(|e| e.payment_ordinal);
        let schedule = DatedSchedule {
            schema: "dated-cashflow-1".into(),
            as_of_ordinal: request.as_of_ordinal,
            horizon_days: horizon.ok_or("deposit capture no horizon")?,
            scenarios: request.scenarios.iter().map(|s| s.run_id.clone()).collect(),
            events,
        };
        schedule.validate()?;
        sources.push(CaptureSource {
            source_id: owner.source_id.clone(),
            source_revision: owner.source_revision.clone(),
            schedule,
            coverage,
        });
        let principal = opening.ok_or("deposit capture no opening")?;
        openings.push(CaptureOpening {
            position_id: position.position_id.clone(),
            principal,
            accrued_interest: 0.,
            gross_carrying_value: principal,
        });
    }
    let out=Capture{scheduled:ScheduledCapture{schema:"deposit-scheduled-capture-1".into(),identity:request.identity.clone(),policy:policy.clone(),scenarios:request.scenarios.clone(),owners:owners.to_vec(),openings,sources,accrual_plans:plans,scope:"physical runoff deposit liability; signed gross daily deposit expense, par basis, explicit opening-before-day0 actions and complete tail; no EIR, migration or inflow inferred".into()},cash_origins};
    bounded(&out, MAX_RETAINED_BYTES / 4)?;
    Ok(out)
}
pub fn prepare(r: &Request) -> Result<Prepared, String> {
    bounded(r, MAX_INPUT_BYTES)?;
    if r.schema != SCHEMA {
        return Err("versioned deposit capture request required".into());
    }
    let graph = DepositGraph::new(&r.graph)?;
    let capture = capture(&graph, &r.owners, &r.policy)?;
    let out = Prepared {
        schema: SCHEMA.into(),
        raw_request: r.clone(),
        graph: graph.snapshot().clone(),
        capture,
    };
    bounded(&out, MAX_RETAINED_BYTES / 4)?;
    Ok(out)
}
