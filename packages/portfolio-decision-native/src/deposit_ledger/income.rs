//! Purchase-date original effective-yield deposit source into reviewed dated GL.
use super::{
    anchored_specification, bounded, compile, runs, RunMapping, MAX_INPUT_BYTES, MAX_RETAINED_BYTES,
};
use portfolio_ledger_native::{
    daily,
    dated_ledger::{DatedLedgerRequest, RecognitionPolicy},
    dated_store::{
        AccrualPlanInput, CoverageStatus, DatedStoreRequest, Direction, OwnerMapping, Ownership,
        PositionCoverage, RecognitionSegment, ScheduleSource, DATED_STORE_SCHEMA,
    },
};
use portfolio_risk_native::{
    advanced_graph::{CaptureCoverageStatus, CaptureOwner, CapturePolicy, ScheduledCapture},
    deposit_graph::{DepositGraph, Request as GraphRequest, ResultData},
    deposit_graph_capture::{
        income::{self, IncomeAnchor, IncomeSource},
        CashOrigin,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
pub const SCHEMA: &str = "deposit-graph-income-ledger-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub graph: GraphRequest,
    pub owners: Vec<CaptureOwner>,
    pub capture_policy: CapturePolicy,
    pub income_anchors: Vec<IncomeAnchor>,
    pub run_mapping: Vec<RunMapping>,
    pub monetary_unit: String,
    pub recognition_policy: RecognitionPolicy,
    pub specification: Value,
}
#[derive(Serialize)]
pub struct SourceReceipt {
    pub schema: String,
    pub raw_request: Request,
    pub graph: ResultData,
    pub capture: ScheduledCapture,
    pub cash_origins: Vec<CashOrigin>,
    pub income_sources: Vec<IncomeSource>,
    pub normalization_policy: String,
    pub scope: String,
}
#[derive(Serialize)]
pub struct Prepared {
    pub schema: String,
    pub dated_request: DatedLedgerRequest,
    pub receipt: SourceReceipt,
}
pub fn prepare(request: &Request) -> Result<Prepared, String> {
    portfolio_compute_control::checkpoint()?;
    bounded(request, MAX_INPUT_BYTES)?;
    portfolio_compute_control::checkpoint()?;
    if request.schema != SCHEMA || request.monetary_unit != "currency_units" {
        return Err(
            "versioned original income deposit request with explicit units required".into(),
        );
    }
    // Reuse immutable mapping/admission helpers. This transport-shaped value does
    // not call the par source generator or change the supplied capture policy.
    let common = super::Request {
        schema: super::SCHEMA.into(),
        graph: request.graph.clone(),
        owners: request.owners.clone(),
        capture_policy: request.capture_policy.clone(),
        run_mapping: request.run_mapping.clone(),
        monetary_unit: request.monetary_unit.clone(),
        recognition_policy: request.recognition_policy.clone(),
        specification: request.specification.clone(),
    };
    let execution_runs = runs(&common)?;
    let graph = DepositGraph::new(&request.graph)?;
    let captured = income::capture_original_income(
        &graph,
        &request.owners,
        &request.capture_policy,
        &request.income_anchors,
    )?;
    let specification = anchored_specification(&common, &captured.scheduled)?;
    let reporting_days = request
        .graph
        .reporting_ordinals
        .last()
        .copied()
        .ok_or("missing reporting dates")?
        - request.graph.as_of_ordinal;
    if reporting_days <= 0 {
        return Err("positive actual reporting horizon required".into());
    }
    let owners = captured
        .scheduled
        .owners
        .iter()
        .map(|o| OwnerMapping {
            position_id: o.position_id.clone(),
            account_id: o.account_id.clone(),
            currency: request.graph.currency.clone(),
            direction: Direction::Payment,
            ownership: Ownership::FullyScenarioGenerated,
            source_id: o.source_id.clone(),
            source_revision: o.source_revision.clone(),
        })
        .collect();
    let sources = captured
        .scheduled
        .sources
        .iter()
        .map(|s| ScheduleSource {
            source_id: s.source_id.clone(),
            source_revision: s.source_revision.clone(),
            schedule: s.schedule.clone(),
            coverage: s
                .coverage
                .iter()
                .map(|c| PositionCoverage {
                    position_id: c.position_id.clone(),
                    run: c.run.clone(),
                    status: match &c.status {
                        CaptureCoverageStatus::CompleteEventSet {} => {
                            CoverageStatus::CompleteEventSet {}
                        }
                        CaptureCoverageStatus::NoEvents { rationale } => CoverageStatus::NoEvents {
                            rationale: rationale.clone(),
                        },
                    },
                })
                .collect(),
        })
        .collect();
    let accrual_plans = captured
        .scheduled
        .accrual_plans
        .iter()
        .map(|p| AccrualPlanInput {
            position_id: p.position_id.clone(),
            event_id: p.event_id.clone(),
            run: p.run.clone(),
            segments: p
                .segments
                .iter()
                .map(|s| RecognitionSegment {
                    start_ordinal: s.start_ordinal,
                    end_ordinal: s.end_ordinal,
                    accrued_interest: s.accrued_interest,
                    book_amortization: s.book_amortization,
                })
                .collect(),
        })
        .collect();
    let prepared=Prepared{schema:SCHEMA.into(),dated_request:DatedLedgerRequest{schema:portfolio_ledger_native::dated_ledger::SCHEMA.into(),monetary_unit:request.monetary_unit.clone(),recognition_policy:request.recognition_policy.clone(),specification,dated:DatedStoreRequest{schema:DATED_STORE_SCHEMA.into(),as_of_ordinal:request.graph.as_of_ordinal,reporting_horizon_days:reporting_days as u32,runs:execution_runs,owners,sources,accrual_plans}},receipt:SourceReceipt{schema:"deposit-graph-income-ledger-source-1".into(),raw_request:request.clone(),graph:graph.snapshot().clone(),capture:captured.scheduled,cash_origins:captured.cash_origins,income_sources:captured.income_sources,normalization_policy:"native_original_eir_deposit_opening_anchors-1".into(),scope:"purchase-date original yield and unchanged original physical expected cash, full-tail basis closure; independent economic source and journal replay are separate publication gates".into()}};
    bounded(&prepared, MAX_RETAINED_BYTES / 4)?;
    portfolio_compute_control::checkpoint()?;
    compile(&prepared.dated_request)?;
    Ok(prepared)
}
pub fn run(request: &Request, output: Box<dyn Write + Send>) -> Result<SourceReceipt, String> {
    let prepared = prepare(request)?;
    let compiled = compile(&prepared.dated_request)?;
    daily::run_dated(compiled, output)?;
    Ok(prepared.receipt)
}
