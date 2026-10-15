//! Proposed performing purchase-to-revision source-owned deposit principal seam.
//! Artifact-only: no accounting, saved-book, capital, or FTP permission.
use crate::replay::dated::{CompiledDatedLedger, DatedLedgerRequest, RecognitionPolicy};
use crate::replay::stores::dated::*;
use portfolio_model_core::cashflow::*;
use portfolio_model_core::deposit_source_owner as source;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
pub const SCHEMA: &str = "deposit-performing-income-ledger-1";
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub source: source::Request,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub position_id: String,
    pub account_id: String,
    pub runs: Vec<RunRequest>,
    pub recognition_policy: RecognitionPolicy,
    pub interest_rights_policy: String,
    pub specification: Value,
}
#[derive(Debug, Serialize)]
pub struct Path {
    pub(crate) run_id: String,
    pub(crate) scenario_id: String,
    pub(crate) severity: f64,
    pub(crate) native: source::ResultData,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub native_source_work: usize,
    pub ledger_work: usize,
    pub derivation_work: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct EarnedAllocation {
    pub native_interest_index: usize,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CashOrigin {
    pub run_id: String,
    pub event_id: String,
    pub native_event_id: String,
    pub native_kind: String,
    pub full_native_accrued_settlement: bool,
    pub allocations: Vec<EarnedAllocation>,
}
pub struct Prepared {
    pub(crate) dated: CompiledDatedLedger,
    pub(crate) prepared_request: DatedLedgerRequest,
    pub(crate) paths: Vec<Path>,
    pub(crate) raw_request: Request,
    pub(crate) admission: Admission,
    pub(crate) cash_origins: Vec<CashOrigin>,
}
pub fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
pub fn money(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > 10_000. {
        Err("deposit continuation ledger money cap10000".into())
    } else {
        Ok(())
    }
}
pub fn source_parse(b: &[u8]) -> Result<source::Request, String> {
    source::parse(b)
}
pub fn parse_request(b: &[u8]) -> Result<(Request, Value), String> {
    crate::owners::deposits::source_principal_json::parse(b)
}
pub fn audit(r: &Request) -> Result<Admission, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || !text(&r.position_id)
        || !text(&r.account_id)
        || r.interest_rights_policy != "native_earned_fifo_cash_origin-1"
        || r.runs.len() < 2
        || r.runs.len() > 8
    {
        return Err("explicit bounded source principal ownership and runs required".into());
    }
    let bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if bytes > 1_048_576 {
        return Err("raw request exceeds1MiB".into());
    }
    let c = &r.runs[0].source.continuation;
    let d = c.original.horizon_days as usize;
    let original =
        serde_json::to_vec(&(&c.original, &c.anchor, &c.contract)).map_err(|e| e.to_string())?;
    let mut ids = HashSet::new();
    let mut scenarios = HashSet::new();
    let mut source_work = 0usize;
    let mut reserved = bytes.checked_mul(12).ok_or("raw reserve overflow")?;
    for run in &r.runs {
        portfolio_compute_control::checkpoint()?;
        let a = source::audit(&run.source)?;
        let cc = &run.source.continuation;
        if !text(&run.run_id)
            || !text(&run.scenario_id)
            || run.severity.to_bits() != 1f64.to_bits()
            || !ids.insert(&run.run_id)
            || !scenarios.insert(&run.scenario_id)
            || cc.original.horizon_days as usize != d
            || serde_json::to_vec(&(&cc.original, &cc.anchor, &cc.contract))
                .map_err(|e| e.to_string())?
                != original
        {
            return Err("run identity/immutable original anchor mismatch".into());
        }
        source_work = source_work
            .checked_add(a.work_units)
            .ok_or("source work overflow")?;
        reserved = reserved
            .checked_add(a.reserved_bytes)
            .ok_or("source bytes overflow")?;
    }
    let run_days = r
        .runs
        .len()
        .checked_mul(d.checked_add(1).ok_or("days overflow")?)
        .ok_or("run days overflow")?;
    let ledger_work = run_days
        .checked_mul(8192 + 16)
        .ok_or("ledger work overflow")?;
    let derivation_work = r
        .runs
        .len()
        .checked_mul(
            d.checked_mul(
                d.checked_mul(3)
                    .and_then(|v| v.checked_add(2))
                    .ok_or("slots overflow")?,
            )
            .ok_or("slots overflow")?,
        )
        .and_then(|v| v.checked_mul(64))
        .ok_or("derivation work overflow")?;
    reserved = reserved
        .checked_add(run_days.checked_mul(65536).ok_or("ledger bytes overflow")?)
        .ok_or("bytes overflow")?;
    let work = source_work
        .checked_add(ledger_work)
        .and_then(|v| v.checked_add(derivation_work))
        .and_then(|v| v.checked_add(bytes.checked_mul(8)?))
        .ok_or("joined work overflow")?;
    if work > MAX_WORK || reserved > MAX_BYTES {
        return Err("joined pre-generation work8M/bytes32MiB exceeded".into());
    }
    // The supplied actual opening remains the sole account/subledger opening.
    let spec = crate::validation::specification::normalize(r.specification.clone(), 1, MAX_WORK)?;
    if spec.positions.len() != 1
        || spec.accounts.len() != 1
        || !spec.cashflows.is_empty()
        || !spec.policies.is_empty()
        || !spec.netting_sets.is_empty()
        || !spec.reverse_severities.is_empty()
        || spec.horizon_days != d
    {
        return Err("one isolated complete liability/ordinary runs only".into());
    }
    let p = &spec.positions[0];
    let a = &spec.accounts[0];
    if p.id.as_str() != r.position_id
        || p.account.as_str() != r.account_id
        || a.id != r.account_id
        || p.kind.as_str() != "deposit"
        || p.classification.as_str() != "ac"
        || a.currency != c.original.identity.currency
        || a.entity != c.original.identity.entity_id
        || p.opening_accrued != 0.
        || p.start_day != 0
        || p.balance.to_bits() != c.anchor.original_principal.to_bits()
        || p.book_adjustment.to_bits()
            != (c.anchor.original_gross_carrying_value - c.anchor.original_principal).to_bits()
        || p.rate != 0.
        || p.annual_pd != 0.
        || p.allowance != 0.
        || p.commitment != 0.
        || p.monthly_runoff != 0.
        || p.floating_beta != 0.
        || p.migration_rate != 0.
        || p.draw_fraction != 0.
        || p.encumbered_fraction != 0.
        || a.annual_fees != 0.
        || a.annual_costs != 0.
        || a.annual_dividends != 0.
        || a.tax_rate != 0.
    {
        return Err("original purchase opening must match; generic overlays unsupported".into());
    }
    for x in [
        p.balance,
        p.book_adjustment,
        p.balance + p.book_adjustment,
        a.cash,
        a.equity,
    ] {
        money(x)?
    }
    let required: BTreeSet<_> = std::iter::once("baseline")
        .chain(spec.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let supplied: BTreeSet<_> = r.runs.iter().map(|s| s.scenario_id.as_str()).collect();
    if required != supplied {
        return Err("exact baseline plus named run coverage required".into());
    }
    Ok(Admission {
        work_units: work,
        reserved_bytes: reserved,
        native_source_work: source_work,
        ledger_work,
        derivation_work,
    })
}
pub fn prepare(r: Request) -> Result<Prepared, String> {
    let admission = audit(&r)?;
    let anchor_raw = r.runs[0].source.continuation.clone();
    let asof = anchor_raw.original.as_of_ordinal;
    let days = anchor_raw.original.horizon_days;
    let spec = crate::validation::specification::normalize(r.specification.clone(), 1, MAX_WORK)?;
    let a = &spec.accounts[0];
    let mut paths = Vec::with_capacity(r.runs.len());
    for run in &r.runs {
        portfolio_compute_control::checkpoint()?;
        let native =
            source::evaluate_with_check(&run.source, portfolio_compute_control::checkpoint)?;
        paths.push(Path {
            run_id: run.run_id.clone(),
            scenario_id: run.scenario_id.clone(),
            severity: run.severity,
            native,
        });
    }
    let mut events = Vec::new();
    let mut plans = Vec::new();
    let mut coverage = Vec::new();
    let mut runs = Vec::new();
    let mut cash_origins = Vec::new();
    for (path_index, path) in paths.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let selector = ScenarioSelector::Named {
            scenario_id: path.run_id.clone(),
        };
        runs.push(RunIdentity {
            run_id: path.run_id.clone(),
            scenario_id: path.scenario_id.clone(),
            severity: path.severity,
            input_revision: path.native.current.source.identity.source_revision.clone(),
        });
        coverage.push(PositionCoverage {
            position_id: r.position_id.clone(),
            run: selector.clone(),
            status: CoverageStatus::CompleteEventSet {},
        });
        let mut remaining: Vec<_> = path
            .native
            .current
            .source
            .interest
            .iter()
            .map(|x| x.amount)
            .collect();
        for (cash_index, cash) in path.native.current.source.cashflows.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            let id = format!("cash:{path_index}:{cash_index}");
            let mut needed = -cash.cash_interest;
            let mut segments = Vec::new();
            let mut allocations = Vec::new();
            let full = matches!(
                cash.kind.as_str(),
                "interest_payment" | "contractual_maturity"
            );
            for (i, origin) in path.native.current.source.interest.iter().enumerate() {
                if needed <= 0. && !full {
                    break;
                }
                if remaining[i] <= 0. {
                    continue;
                }
                if origin.end_ordinal > cash.ordinal {
                    if near(needed, 0.) {
                        break;
                    }
                    return Err("deposit cash cannot borrow future earned interest rights".into());
                }
                let allocated = if full {
                    remaining[i]
                } else {
                    needed.min(remaining[i])
                };
                if allocated > 0. {
                    if let Some(prior) = segments.last() {
                        let prior: &RecognitionSegment = prior;
                        if prior.end_ordinal < origin.start_ordinal {
                            segments.push(RecognitionSegment {
                                start_ordinal: prior.end_ordinal,
                                end_ordinal: origin.start_ordinal,
                                accrued_interest: 0.,
                                book_amortization: 0.,
                            });
                        }
                    }
                    segments.push(RecognitionSegment {
                        start_ordinal: origin.start_ordinal,
                        end_ordinal: origin.end_ordinal,
                        accrued_interest: -allocated,
                        book_amortization: 0.,
                    });
                    allocations.push(EarnedAllocation {
                        native_interest_index: i,
                        amount: allocated,
                    });
                }
                remaining[i] -= allocated;
                needed -= allocated;
            }
            if !near(needed, 0.) {
                return Err("deposit native cash lacks complete earned-origin ownership".into());
            }
            let earned: f64 = segments.iter().map(|s| s.accrued_interest).sum();
            let accrual_period =
                segments
                    .first()
                    .zip(segments.last())
                    .map(|(first, last)| AccrualPeriod {
                        start_ordinal: first.start_ordinal,
                        end_ordinal: last.end_ordinal,
                    });
            events.push(DatedCashflowEvent {
                position_id: r.position_id.clone(),
                event_id: id.clone(),
                scenario: selector.clone(),
                payment_ordinal: cash.ordinal,
                day_offset: (cash.ordinal - asof) as u32,
                accrual_period,
                currency: a.currency.clone(),
                components: CashflowComponents {
                    principal: cash.principal,
                    cash_interest: cash.cash_interest,
                    accrued_interest: earned,
                    book_amortization: 0.,
                    fees: 0.,
                },
                measure: CashflowMeasure::Physical,
                provenance: EventProvenance::Modeled {
                    model_id: "deposit_source_principal_continuation".into(),
                    model_version: source::SCHEMA.into(),
                    input_revision: path.native.current.source.identity.source_revision.clone(),
                },
                adjustment: CashflowAdjustment::None {},
            });
            if !segments.is_empty() {
                plans.push(AccrualPlanInput {
                    position_id: r.position_id.clone(),
                    event_id: id.clone(),
                    run: selector.clone(),
                    segments,
                });
            }
            cash_origins.push(CashOrigin {
                run_id: path.run_id.clone(),
                event_id: id,
                native_event_id: cash.event_id.clone(),
                native_kind: cash.kind.clone(),
                full_native_accrued_settlement: full,
                allocations,
            });
        }
        if !near(remaining.iter().map(|x| x.abs()).sum(), 0.) {
            return Err("deposit gross earned rights remain beyond full source tail".into());
        }
        // Effective-yield basis recognition is a distinct noncash origin, never
        // a same-day coupon cash event. Existing native earned lots above remain
        // bound to the actual cash event which extinguishes their gross right.
        for (i, row) in path.native.current.income.rows.iter().enumerate() {
            let e = &path.native.current.income_request.events[i];
            let id = format!("book:{path_index}:{i}");
            events.push(DatedCashflowEvent {
                position_id: r.position_id.clone(),
                event_id: id.clone(),
                scenario: selector.clone(),
                payment_ordinal: e.payment_ordinal,
                day_offset: (e.payment_ordinal - asof) as u32,
                accrual_period: Some(AccrualPeriod {
                    start_ordinal: e.payment_ordinal - 1,
                    end_ordinal: e.payment_ordinal,
                }),
                currency: a.currency.clone(),
                components: CashflowComponents {
                    principal: 0.,
                    cash_interest: 0.,
                    accrued_interest: 0.,
                    book_amortization: -row.basis_amortization,
                    fees: 0.,
                },
                measure: CashflowMeasure::Physical,
                provenance: EventProvenance::Modeled {
                    model_id: "deposit_source_principal_continuation".into(),
                    model_version: source::SCHEMA.into(),
                    input_revision: path.native.current.source.identity.source_revision.clone(),
                },
                adjustment: CashflowAdjustment::None {},
            });
            plans.push(AccrualPlanInput {
                position_id: r.position_id.clone(),
                event_id: id,
                run: selector.clone(),
                segments: vec![RecognitionSegment {
                    start_ordinal: e.payment_ordinal - 1,
                    end_ordinal: e.payment_ordinal,
                    accrued_interest: 0.,
                    book_amortization: -row.basis_amortization,
                }],
            });
        }
    }
    // Stable equal-day path order is retained by the stable sort.
    events.sort_by_key(|e| e.payment_ordinal);
    let source_id = anchor_raw.contract.id.clone();
    let source_revision = anchor_raw.contract.revision.clone();
    let dated_request = DatedLedgerRequest {
        schema: crate::replay::dated::SCHEMA.into(),
        monetary_unit: "currency_units".into(),
        recognition_policy: r.recognition_policy.clone(),
        specification: r.specification.clone(),
        dated: DatedStoreRequest {
            schema: DATED_STORE_SCHEMA.into(),
            as_of_ordinal: asof,
            reporting_horizon_days: days,
            runs,
            owners: vec![OwnerMapping {
                position_id: r.position_id.clone(),
                account_id: r.account_id.clone(),
                currency: a.currency.clone(),
                direction: Direction::Payment,
                ownership: Ownership::FullyScenarioGenerated,
                source_id: source_id.clone(),
                source_revision: source_revision.clone(),
            }],
            sources: vec![ScheduleSource {
                source_id,
                source_revision,
                schedule: DatedSchedule {
                    schema: DATED_CASHFLOW_SCHEMA.into(),
                    as_of_ordinal: asof,
                    horizon_days: days,
                    scenarios: paths.iter().map(|p| p.run_id.clone()).collect(),
                    events,
                },
                coverage,
            }],
            accrual_plans: plans,
        },
    };
    let dated = CompiledDatedLedger::compile(dated_request.clone(), 1, MAX_WORK)?;
    for run in dated.store.runs() {
        portfolio_compute_control::checkpoint()?;
        for event in dated.store.events().iter().filter(|e| {
            e.run_index
                .is_none_or(|index| dated.store.runs()[index].run_id == run.run_id)
        }) {
            let rights = dated.store.rights_at(
                &run.run_id,
                event.index,
                asof + days as i32,
                Boundary::AfterEvents,
            )?;
            if !near(rights.signed_interest_right, 0.)
                || !rights.cash_settled
                || !rights.accrual_complete
            {
                return Err("deposit full-tail gross event right remains open".into());
            }
        }
    }
    if dated.work_units
        > r.runs
            .len()
            .checked_mul((days as usize).checked_add(1).ok_or("days overflow")?)
            .and_then(|n| n.checked_mul(16))
            .ok_or("dated work overflow")?
    {
        return Err("dated work exceeds preflight".into());
    }
    portfolio_compute_control::checkpoint()?;
    Ok(Prepared {
        dated,
        prepared_request: dated_request,
        paths,
        raw_request: r,
        admission,
        cash_origins,
    })
}
impl Prepared {
    pub fn path(&self, scenario: &str, severity: f64) -> Result<&Path, String> {
        self.paths
            .iter()
            .find(|p| p.scenario_id == scenario && p.severity.to_bits() == severity.to_bits())
            .ok_or("missing explicit source run".into())
    }
}
struct LimitedOutput {
    output: Box<dyn std::io::Write + Send>,
    bytes: usize,
}
impl std::io::Write for LimitedOutput {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        if self
            .bytes
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("output overflow"))?
            > MAX_BYTES
        {
            return Err(std::io::Error::other("source/header/stream32MiB cap"));
        }
        let n = self.output.write(b)?;
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| std::io::Error::other("output overflow"))?;
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        self.output.flush()?;
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)
    }
}
pub fn run_public(
    p: Prepared,
    raw: Value,
    output: Box<dyn std::io::Write + Send>,
) -> Result<(), String> {
    use std::io::Write;
    let (typed, _) = parse_request(&serde_json::to_vec(&raw).map_err(|e| e.to_string())?)?;
    if serde_json::to_vec(&typed).map_err(|e| e.to_string())?
        != serde_json::to_vec(&p.raw_request).map_err(|e| e.to_string())?
    {
        return Err("raw source changed after preparation".into());
    }
    let header = serde_json::json!({"workflow":{"schema":"model-ledger-preparation-1","model_schema":SCHEMA,
        "prepared_request":p.prepared_request,"source_receipt":{"raw_request":raw,"paths":p.paths,"admission":p.admission,
        "cash_origins":p.cash_origins,"phase_policy":"opening_recognition_actual_cash_source_atom_reclass_then_revision-1",
        "accounting_standard_approval":false,"installed_adoption":false,"saved_book_adoption":false,"capital_ftp_eligible":false}}});
    let bytes = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    if bytes.len() > p.admission.reserved_bytes {
        return Err("actual header exceeds reservation".into());
    }
    portfolio_compute_control::checkpoint()?;
    let mut out = LimitedOutput { output, bytes: 0 };
    out.write_all(&bytes).map_err(|e| e.to_string())?;
    out.write_all(b"\n").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    drop(header);
    drop(bytes);
    crate::replay::daily::run_dated_with_source_principal(&p, Box::new(out))?;
    portfolio_compute_control::checkpoint()
}

/// Selected public branch only: strict original envelope bytes plus exact raw Value.
pub fn parse_envelope(bytes: &[u8]) -> Result<(Request, u64, Value), String> {
    if bytes.len() > 1_048_576 {
        return Err("performing deposit raw envelope1MiB bound".into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        request: Request,
        execution_timeout_ms: u64,
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.execution_timeout_ms == 0 || envelope.execution_timeout_ms > 1_800_000 {
        return Err("explicit bounded execution timeout required".into());
    }
    let value = crate::owners::deposits::source_principal_json::value(bytes)?;
    let raw = value.get("request").ok_or("raw request required")?.clone();
    let (selected, _) = parse_request(&serde_json::to_vec(&raw).map_err(|e| e.to_string())?)?;
    // `Envelope` rejects duplicate typed fields; the Value visitor rejects every
    // remaining duplicate/alias before this comparison is permitted.
    if serde_json::to_vec(&selected).map_err(|e| e.to_string())?
        != serde_json::to_vec(&envelope.request).map_err(|e| e.to_string())?
    {
        return Err("selected request identity differs".into());
    }
    Ok((envelope.request, envelope.execution_timeout_ms, raw))
}
