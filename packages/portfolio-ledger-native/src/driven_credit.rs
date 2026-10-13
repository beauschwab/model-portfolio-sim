//! Explicit frozen driver projection into realized physical credit ledger contracts.
//! Caller policy intervals contain no hazards. Preparation emits no financial rows.
use crate::credit_path_replay::{
    CompiledPathLedger, PathLedgerBatch, PathLedgerRequest, PathLedgerRun, PositionAccount,
};
use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_accounting::Identity;
use portfolio_model_core::credit_calibration::ObservationTime;
use portfolio_model_core::credit_covariate::{
    self, CovariateProjectionRequest, ProjectedCreditHazards, ProjectionUse,
};
use portfolio_model_core::credit_path::{
    self, CreditPathRequest, DrawStream, PathBoundary, PathInterval, PathOpening, RecoveryWeight,
};
use portfolio_model_core::credit_path_accounting::{
    self, DayMapping, PathAccountingRequest, PathBinding,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub const SCHEMA: &str = "driven-credit-ledger-1";
pub const MAX_BATCHES: usize = 4096;
pub const MAX_DRIVER_INTERVALS: usize = 50_000;
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PREPARATION_WORK: usize = 20_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UsePolicy {
    Adoption {},
    ResearchAnalyticalOnly { review: Identity, rationale: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyInterval {
    pub end_time: ObservationTime,
    pub accrual_rates: [f64; 3],
    pub loss_given_default: [f64; 3],
    pub credit_conversion_factor: [f64; 3],
    pub interest_recovery_fraction: [f64; 3],
    pub complete_recovery_tails: [Vec<RecoveryWeight>; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathTemplate {
    pub schema: String,
    pub measure: CreditMeasure,
    pub monetary_unit: String,
    pub source_id: String,
    pub source_revision: String,
    pub model_policy_id: String,
    pub origin_time: ObservationTime,
    pub max_jumps_per_path: usize,
    pub intervals: Vec<PolicyInterval>,
    pub boundaries: Vec<PathBoundary>,
    pub draw_stream: DrawStream,
    pub paths: Vec<PathOpening>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingTemplate {
    pub schema: String,
    pub path: PathTemplate,
    pub day_mapping: DayMapping,
    pub reporting_end_ordinal: i32,
    pub timestamp_tolerance_seconds: f64,
    pub bindings: Vec<PathBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrivenBatch {
    pub batch_id: String,
    pub account_id: String,
    pub projection: CovariateProjectionRequest,
    pub template: AccountingTemplate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrivenRun {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub batches: Vec<DrivenBatch>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub use_policy: UsePolicy,
    pub monetary_unit: String,
    pub as_of_ordinal: i32,
    pub specification: Value,
    pub timeout_ms: u64,
    pub runs: Vec<DrivenRun>,
}
#[derive(Debug, Serialize)]
pub struct ProjectionReceipt {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub batch_id: String,
    pub account_id: String,
    pub position_id: String,
    pub path_key: u64,
    pub projection: ProjectedCreditHazards,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: String,
    pub raw_source: Request,
    pub projections: Vec<ProjectionReceipt>,
    pub research_only: bool,
    pub adoption_output: bool,
    pub empirical_validation: bool,
    pub regulatory_compliance: bool,
    pub preparation_work_bound: usize,
    pub projection_work_units: usize,
    pub ledger_admitted_work: usize,
}
#[derive(Debug, Serialize)]
pub struct Prepared {
    pub schema: String,
    pub ledger: PathLedgerRequest,
    pub receipt: Receipt,
}

struct SizeCounter {
    count: usize,
    limit: usize,
    deadline: Instant,
}
impl Write for SizeCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        if Instant::now() >= self.deadline {
            return Err(io::Error::other("COMPUTE_DEADLINE_EXCEEDED"));
        }
        self.count = self
            .count
            .checked_add(buf.len())
            .ok_or_else(|| io::Error::other("driven credit input size overflow"))?;
        if self.count > self.limit {
            return Err(io::Error::other(
                "driven credit serialized size admission exceeded",
            ));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn size<T: Serialize>(
    value: &T,
    limit: usize,
    start: Instant,
    timeout: Duration,
) -> Result<usize, String> {
    let mut counter = SizeCounter {
        count: 0,
        limit,
        deadline: start + timeout,
    };
    serde_json::to_writer(&mut counter, value).map_err(|e| e.to_string())?;
    Ok(counter.count)
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("driven credit reviewed metadata admission exceeded".into())
    } else {
        Ok(())
    }
}
fn check(start: Instant, timeout: Duration) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    if start.elapsed() >= timeout {
        Err("COMPUTE_DEADLINE_EXCEEDED".into())
    } else {
        Ok(())
    }
}
pub fn prepare(request: &Request) -> Result<Prepared, String> {
    prepare_with_limits(
        request,
        crate::credit_path_replay::MAX_POSITIONS,
        crate::credit_path_replay::MAX_WORK,
    )
}
pub fn prepare_with_limits(
    request: &Request,
    max_positions: usize,
    max_work: usize,
) -> Result<Prepared, String> {
    let start = Instant::now();
    if request.schema != SCHEMA
        || request.monetary_unit != "currency_units"
        || !(1..=3_600_000).contains(&request.timeout_ms)
        || request.runs.is_empty()
        || request.runs.len() > MAX_BATCHES
        || max_positions > crate::credit_path_replay::MAX_POSITIONS
        || max_work > crate::credit_path_replay::MAX_WORK
    {
        return Err("unsupported driven credit schema/units/deadline".into());
    }
    let timeout = Duration::from_millis(request.timeout_ms);
    check(start, timeout)?;
    let research = matches!(request.use_policy, UsePolicy::ResearchAnalyticalOnly { .. });
    if let UsePolicy::ResearchAnalyticalOnly { review, rationale } = &request.use_policy {
        text(&review.id)?;
        text(&review.revision)?;
        text(rationale)?;
    }
    // Reject aggregate dimensions before projecting, cloning or building any schedules.
    let mut batches = 0usize;
    let mut intervals = 0usize;
    for run in &request.runs {
        check(start, timeout)?;
        if run.batches.is_empty() {
            return Err("driven credit run batches required".into());
        }
        batches = batches
            .checked_add(run.batches.len())
            .ok_or("driven credit batch overflow")?;
        if batches > MAX_BATCHES {
            return Err("driven credit batch admission exceeded".into());
        }
        for batch in &run.batches {
            intervals = intervals
                .checked_add(batch.projection.intervals.len())
                .ok_or("driven credit interval overflow")?;
            if intervals > MAX_DRIVER_INTERVALS {
                return Err("driven credit driver interval admission exceeded".into());
            }
        }
    }
    let bytes = size(request, MAX_INPUT_BYTES, start, timeout)?;
    let work_bound = bytes
        .checked_add(
            intervals
                .checked_mul(256)
                .ok_or("projection work overflow")?,
        )
        .ok_or("projection work overflow")?;
    if work_bound > MAX_PREPARATION_WORK {
        return Err("driven credit preparation work admission exceeded".into());
    }
    check(start, timeout)?;
    let mut runs = Vec::with_capacity(request.runs.len());
    let mut receipts = Vec::with_capacity(batches);
    let mut crn = BTreeMap::<String, Vec<u8>>::new();
    let mut projection_work = 0usize;
    for run in &request.runs {
        let mut prepared_batches = Vec::with_capacity(run.batches.len());
        for batch in &run.batches {
            check(start, timeout)?;
            let template = &batch.template;
            let path = &template.path;
            let projection = &batch.projection;
            if template.schema != credit_path_accounting::SCHEMA
                || path.schema != credit_path::SCHEMA
                || path.measure != CreditMeasure::Physical
                || path.monetary_unit != request.monetary_unit
                || projection.measure != CreditMeasure::Physical
                || path.paths.len() != 1
                || template.bindings.len() != 1
                || path.origin_time != projection.origin_time
                || path.origin_time.ordinal != request.as_of_ordinal
                || path.origin_time.seconds_of_day != 0
                || projection.end_time.ordinal != template.reporting_end_ordinal
                || projection.end_time.seconds_of_day != 0
                || projection.scenario_id != run.scenario_id
                || !(1..=3_600_000).contains(&projection.timeout_ms)
                || path.intervals.len() != projection.intervals.len()
                || path.intervals.is_empty()
                || path.model_policy_id != projection.model.model_policy.id
            {
                return Err(
                    "driven credit exact physical template/projection calendar coverage required"
                        .into(),
                );
            }
            let opening = &path.paths[0];
            let binding = &template.bindings[0];
            if projection.position_id != opening.path_id
                || projection.position_id != binding.position_id
                || opening.path_key != binding.path_key
            {
                return Err("driven credit position/path/binding identity mismatch".into());
            }
            if research != matches!(projection.use_policy, ProjectionUse::AnalyticalOnly) {
                return Err(
                    "driven credit projection use must match declared execution policy".into(),
                );
            }
            let frozen_crn =
                serde_json::to_vec(&(&path.draw_stream, opening)).map_err(|e| e.to_string())?;
            if let Some(previous) = crn.get(&opening.path_id) {
                if previous != &frozen_crn {
                    return Err("driven credit scenario CRN/opening identity mismatch".into());
                }
            } else {
                crn.insert(opening.path_id.clone(), frozen_crn);
            }
            for (policy, driver) in path.intervals.iter().zip(&projection.intervals) {
                if policy.end_time != driver.end {
                    return Err("driven credit interval policy/driver end mismatch".into());
                }
            }
            let remaining = timeout.saturating_sub(start.elapsed()).as_millis();
            if remaining == 0 {
                return Err("COMPUTE_DEADLINE_EXCEEDED".into());
            }
            let mut bounded_projection = projection.clone();
            bounded_projection.timeout_ms =
                projection.timeout_ms.min(remaining.min(3_600_000) as u64);
            let projected = credit_covariate::project(&bounded_projection)?;
            if projected.adoption_output == research || projected.analytical_only != research {
                return Err("driven credit projection eligibility mismatch".into());
            }
            projection_work = projection_work
                .checked_add(projected.work_units)
                .ok_or("projection work overflow")?;
            if projection_work > MAX_PREPARATION_WORK {
                return Err("driven credit projection work admission exceeded".into());
            }
            let resolved = path
                .intervals
                .iter()
                .zip(&projected.intervals)
                .map(|(p, h)| PathInterval {
                    end_year: h.end_year,
                    hazards_per_year: h.hazards_per_year,
                    accrual_rates: p.accrual_rates,
                    loss_given_default: p.loss_given_default,
                    credit_conversion_factor: p.credit_conversion_factor,
                    interest_recovery_fraction: p.interest_recovery_fraction,
                    complete_recovery_tails: p.complete_recovery_tails.clone(),
                })
                .collect();
            prepared_batches.push(PathLedgerBatch {
                batch_id: batch.batch_id.clone(),
                account_by_position: vec![PositionAccount {
                    position_id: opening.path_id.clone(),
                    account_id: batch.account_id.clone(),
                }],
                request: PathAccountingRequest {
                    schema: template.schema.clone(),
                    path: CreditPathRequest {
                        schema: path.schema.clone(),
                        measure: path.measure,
                        monetary_unit: path.monetary_unit.clone(),
                        source_id: path.source_id.clone(),
                        source_revision: path.source_revision.clone(),
                        model_policy_id: path.model_policy_id.clone(),
                        origin_time: path.origin_time,
                        max_jumps_per_path: path.max_jumps_per_path,
                        intervals: resolved,
                        boundaries: path.boundaries.clone(),
                        draw_stream: path.draw_stream.clone(),
                        paths: path.paths.clone(),
                    },
                    day_mapping: template.day_mapping,
                    reporting_end_ordinal: template.reporting_end_ordinal,
                    timestamp_tolerance_seconds: template.timestamp_tolerance_seconds,
                    bindings: template.bindings.clone(),
                },
            });
            receipts.push(ProjectionReceipt {
                run_id: run.run_id.clone(),
                scenario_id: run.scenario_id.clone(),
                severity: run.severity,
                batch_id: batch.batch_id.clone(),
                account_id: batch.account_id.clone(),
                position_id: opening.path_id.clone(),
                path_key: opening.path_key,
                projection: projected,
            });
        }
        runs.push(PathLedgerRun {
            run_id: run.run_id.clone(),
            scenario_id: run.scenario_id.clone(),
            severity: run.severity,
            batches: prepared_batches,
        });
    }
    let ledger = PathLedgerRequest {
        schema: crate::credit_path_replay::SCHEMA.into(),
        monetary_unit: request.monetary_unit.clone(),
        as_of_ordinal: request.as_of_ordinal,
        specification: request.specification.clone(),
        runs,
    };
    check(start, timeout)?;
    size(&ledger, MAX_INPUT_BYTES, start, timeout)?;
    // Compilation proves complete raw-spec/run/anchor/ownership admission; financial
    // batch evaluation still happens once during the staged protocol-5 execution.
    let validated = CompiledPathLedger::compile_with_limits(
        serde_json::from_value(serde_json::to_value(&ledger).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        max_positions,
        max_work,
    )?;
    let ledger_admitted_work = validated.admitted_work();
    drop(validated);
    check(start, timeout)?;
    Ok(Prepared {
        schema: SCHEMA.into(),
        ledger,
        receipt: Receipt {
            schema: SCHEMA.into(),
            raw_source: request.clone(),
            projections: receipts,
            research_only: research,
            adoption_output: !research,
            empirical_validation: false,
            regulatory_compliance: false,
            preparation_work_bound: work_bound,
            projection_work_units: projection_work,
            ledger_admitted_work,
        },
    })
}
