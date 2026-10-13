//! Native supplied deposit graph -> explicit scheduled dated-ledger source.
//! This boundary admits no implicit accounting policy or default event pruning.
#[path = "deposit_ledger/income.rs"]
pub mod income;
use portfolio_ledger_native::{
    daily,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest, RecognitionPolicy},
    dated_store::{
        AccrualPlanInput, CoverageStatus, DatedStoreRequest, Direction, OwnerMapping, Ownership,
        PositionCoverage, RecognitionSegment, RunIdentity, ScheduleSource, DATED_STORE_SCHEMA,
    },
};
use portfolio_risk_native::{
    advanced_graph::{CaptureOwner, CapturePolicy, ScheduledCapture},
    deposit_graph::{DepositGraph, Request as GraphRequest, ResultData},
    deposit_graph_capture::{self, CashOrigin},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, io::Write};

pub const SCHEMA: &str = "deposit-graph-dated-ledger-1";
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_POSITIONS: usize = 60_000;
pub const MAX_WORK: usize = 90_000_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunMapping {
    pub run_id: String,
    pub ledger_scenario_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub graph: GraphRequest,
    pub owners: Vec<CaptureOwner>,
    pub capture_policy: CapturePolicy,
    pub run_mapping: Vec<RunMapping>,
    pub monetary_unit: String,
    pub recognition_policy: RecognitionPolicy,
    pub specification: Value,
}

#[derive(Serialize)]
pub struct SourceReceipt {
    pub schema: String,
    /// Complete typed input, including supplied policies, ledger and P/Q histories.
    /// Canonical serialization is deterministic; publication owns the hash.
    pub raw_request: Request,
    pub graph: ResultData,
    /// Includes each recognition origin ID omitted from the numeric store layout.
    pub capture: ScheduledCapture,
    pub cash_origins: Vec<CashOrigin>,
    pub normalization_policy: String,
    pub scope: String,
}

#[derive(Serialize)]
pub struct Prepared {
    pub schema: String,
    pub dated_request: DatedLedgerRequest,
    pub receipt: SourceReceipt,
}

struct Count {
    size: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(value.len());
        if self.size > self.limit {
            return Err(std::io::Error::other(
                "graph dated ledger byte admission exceeded",
            ));
        }
        Ok(value.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded<T: Serialize>(value: &T, limit: usize) -> Result<(), String> {
    serde_json::to_writer(&mut Count { size: 0, limit }, value).map_err(|e| e.to_string())
}
fn label(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        Err("graph ledger mapping identity must be bounded nonblank text".into())
    } else {
        Ok(())
    }
}

fn runs(request: &Request) -> Result<Vec<RunIdentity>, String> {
    if request.run_mapping.len() != request.graph.scenarios.len() || request.run_mapping.len() > 13
    {
        return Err("graph ledger requires complete explicit run mapping".into());
    }
    let mut mapping = HashMap::new();
    for row in &request.run_mapping {
        label(&row.run_id)?;
        label(&row.ledger_scenario_id)?;
        if mapping
            .insert(row.run_id.as_str(), row.ledger_scenario_id.as_str())
            .is_some()
        {
            return Err("duplicate graph ledger run mapping".into());
        }
    }
    request
        .graph
        .scenarios
        .iter()
        .map(|scenario| {
            let &ledger_scenario = mapping
                .get(scenario.run_id.as_str())
                .ok_or("graph ledger run mapping contains unknown or missing run")?;
            let base = scenario.scenario_id == request.graph.base_scenario_id;
            if (ledger_scenario == "baseline") != base || (base && scenario.severity != 1.) {
                return Err(
                    "graph base must map to ledger baseline severity one exclusively".into(),
                );
            }
            Ok(RunIdentity {
                run_id: scenario.run_id.clone(),
                scenario_id: ledger_scenario.to_owned(),
                severity: scenario.severity,
                input_revision: request.graph.identity.input_revision.clone(),
            })
        })
        .collect()
}

fn anchor_number(
    row: &mut serde_json::Map<String, Value>,
    key: &str,
    amount: f64,
) -> Result<(), String> {
    if let Some(value) = row.get(key) {
        if value.as_f64() != Some(amount) {
            return Err(format!(
                "explicit ledger {key} conflicts with native deposit opening"
            ));
        }
    } else {
        row.insert(key.to_owned(), json!(amount));
    }
    Ok(())
}

fn anchored_specification(request: &Request, capture: &ScheduledCapture) -> Result<Value, String> {
    let mut raw = request.specification.clone();
    let positions = raw
        .get_mut("positions")
        .and_then(Value::as_array_mut)
        .ok_or("graph ledger requires explicit raw position mappings")?;
    if positions.len() > MAX_POSITIONS {
        return Err("graph ledger position admission exceeded".into());
    }
    let mut ids = HashMap::new();
    for (index, row) in positions.iter().enumerate() {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("missing ledger position ID")?;
        if ids.insert(id.to_owned(), index).is_some() {
            return Err("duplicate ledger position ID".into());
        }
    }
    let owners: HashMap<_, _> = capture
        .owners
        .iter()
        .map(|o| (o.position_id.as_str(), o))
        .collect();
    for opening in &capture.openings {
        portfolio_compute_control::checkpoint()?;
        let index = *ids
            .get(&opening.position_id)
            .ok_or("graph owner missing ledger position")?;
        let row = positions[index]
            .as_object_mut()
            .ok_or("ledger position must be an object")?;
        let owner = owners[opening.position_id.as_str()];
        if row.get("kind").and_then(Value::as_str) != Some("deposit")
            || row.get("account").and_then(Value::as_str) != Some(owner.account_id.as_str())
        {
            return Err("deposit graph ledger owner must name its explicit deposit/account".into());
        }
        anchor_number(row, "balance", opening.principal)?;
        anchor_number(row, "opening_accrued", opening.accrued_interest)?;
        anchor_number(
            row,
            "book_adjustment",
            opening.gross_carrying_value - opening.principal,
        )?;
        if let Some(value) = row.get("source_id") {
            if value.as_str() != Some(owner.source_id.as_str()) {
                return Err("explicit ledger source_id conflicts with graph source owner".into());
            }
        } else {
            row.insert("source_id".into(), json!(owner.source_id));
        }
    }
    Ok(raw)
}

fn compile(request: &DatedLedgerRequest) -> Result<CompiledDatedLedger, String> {
    // The existing request is not Clone; clone its owned fields without a JSON
    // deserialize/re-normalization transformation or independent financial math.
    CompiledDatedLedger::compile(
        DatedLedgerRequest {
            schema: request.schema.clone(),
            monetary_unit: request.monetary_unit.clone(),
            recognition_policy: request.recognition_policy.clone(),
            specification: request.specification.clone(),
            dated: request.dated.clone(),
        },
        MAX_POSITIONS,
        MAX_WORK,
    )
}

/// Prepare the complete native source and validate it through the actual daily
/// compiler. This does not publish or establish independent economic verification.
pub fn prepare(request: &Request) -> Result<Prepared, String> {
    portfolio_compute_control::checkpoint()?;
    bounded(request, MAX_INPUT_BYTES)?;
    if request.schema != SCHEMA || request.monetary_unit != "currency_units" {
        return Err("graph ledger requires explicit schema and currency units".into());
    }
    let execution_runs = runs(request)?;
    let graph = DepositGraph::new(&request.graph)?;
    let captured =
        deposit_graph_capture::capture(&graph, &request.owners, &request.capture_policy)?;
    let capture = captured.scheduled;
    let specification = anchored_specification(request, &capture)?;
    let reporting_days = request
        .graph
        .reporting_ordinals
        .last()
        .copied()
        .ok_or("missing graph reporting dates")?
        - request.graph.as_of_ordinal;
    if reporting_days <= 0 {
        return Err("graph ledger requires a positive actual-date reporting horizon".into());
    }
    let owners = capture
        .owners
        .iter()
        .map(|owner| OwnerMapping {
            position_id: owner.position_id.clone(),
            account_id: owner.account_id.clone(),
            currency: request.graph.currency.clone(),
            direction: Direction::Payment,
            ownership: Ownership::FullyScenarioGenerated,
            source_id: owner.source_id.clone(),
            source_revision: owner.source_revision.clone(),
        })
        .collect();
    let sources = capture.sources.iter().map(|source| ScheduleSource {
        source_id: source.source_id.clone(),
        source_revision: source.source_revision.clone(),
        schedule: source.schedule.clone(),
        coverage: source.coverage.iter().map(|row| PositionCoverage {
            position_id: row.position_id.clone(),
            run: row.run.clone(),
            status: match &row.status {
                portfolio_risk_native::advanced_graph::CaptureCoverageStatus::CompleteEventSet {} => CoverageStatus::CompleteEventSet {},
                portfolio_risk_native::advanced_graph::CaptureCoverageStatus::NoEvents { rationale } => CoverageStatus::NoEvents { rationale: rationale.clone() },
            },
        }).collect(),
    }).collect();
    let accrual_plans = capture
        .accrual_plans
        .iter()
        .map(|plan| AccrualPlanInput {
            position_id: plan.position_id.clone(),
            event_id: plan.event_id.clone(),
            run: plan.run.clone(),
            segments: plan
                .segments
                .iter()
                .map(|segment| RecognitionSegment {
                    start_ordinal: segment.start_ordinal,
                    end_ordinal: segment.end_ordinal,
                    accrued_interest: segment.accrued_interest,
                    book_amortization: segment.book_amortization,
                })
                .collect(),
        })
        .collect();
    let prepared = Prepared {
        schema: SCHEMA.into(),
        dated_request: DatedLedgerRequest {
            schema: portfolio_ledger_native::dated_ledger::SCHEMA.into(),
            monetary_unit: request.monetary_unit.clone(),
            recognition_policy: request.recognition_policy.clone(),
            specification,
            dated: DatedStoreRequest {
                schema: DATED_STORE_SCHEMA.into(),
                as_of_ordinal: request.graph.as_of_ordinal,
                reporting_horizon_days: reporting_days as u32,
                runs: execution_runs,
                owners, sources, accrual_plans,
            },
        },
        receipt: SourceReceipt {
            schema: "deposit-graph-dated-ledger-source-1".into(),
            raw_request: request.clone(),
            graph: graph.snapshot().clone(),
            capture,
            cash_origins: captured.cash_origins,
            normalization_policy: "native_par_deposit_opening_anchors-1".into(),
            scope: "native physical deposit runoff sources; journal replay and raw source economic verification are separate publication gates".into(),
        },
    };
    // Conservative serialized retained charge, in addition to native graph/store
    // component limits. This is not an aggregate process RSS bound.
    bounded(&prepared, MAX_RETAINED_BYTES / 4)?;
    compile(&prepared.dated_request)?;
    Ok(prepared)
}

/// Execute the same fully admitted source through the native streamed daily
/// runner. The caller persists the returned receipt and independently replays
/// journals before immutable publication. No manifest is created here.
pub fn run(request: &Request, output: Box<dyn Write + Send>) -> Result<SourceReceipt, String> {
    let prepared = prepare(request)?;
    let compiled = compile(&prepared.dated_request)?;
    daily::run_dated(compiled, output)?;
    Ok(prepared.receipt)
}
