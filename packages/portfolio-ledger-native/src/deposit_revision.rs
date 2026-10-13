//! Unapplied single-liability purchase-time original-EIR revision compiler.
use crate::dated_ledger::{CompiledDatedLedger, DatedLedgerRequest, RecognitionPolicy};
use crate::dated_store::*;
use portfolio_model_core::{
    cashflow::*,
    deposit_revised_income::{self as source, Request as SourceRequest, ResultData},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
pub const SCHEMA: &str = "deposit-revised-income-ledger-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub position_id: String,
    pub account_id: String,
    pub source: SourceRequest,
    pub recognition_policy: RecognitionPolicy,
    pub interest_rights_policy: String,
    pub specification: Value,
}
#[derive(Debug, Clone, Serialize)]
pub struct Revision {
    pub run_index: usize,
    pub position_index: usize,
    pub account_index: usize,
    pub original_basis: f64,
    pub revised_basis: f64,
    pub amount: f64,
    pub review: portfolio_model_core::dated_income::Identity,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub native_source_work: usize,
    pub ledger_work: usize,
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
    pub dated: CompiledDatedLedger,
    pub prepared_request: DatedLedgerRequest,
    pub revisions: Vec<Revision>,
    pub native: ResultData,
    pub raw_request: Request,
    pub admission: Admission,
    pub cash_origins: Vec<CashOrigin>,
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn money(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > source::MAX_MONEY {
        Err("revised deposit ledger money exceeds 10000".into())
    } else {
        Ok(())
    }
}
pub fn prepare(r: Request) -> Result<Prepared, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || !text(&r.position_id)
        || !text(&r.account_id)
        || r.interest_rights_policy != "native_earned_fifo_cash_origin-1"
    {
        return Err("explicit bounded revision owner/schema required".into());
    }
    let source_admission = source::audit_request(&r.source)?;
    let spec = crate::spec::normalize(r.specification.clone(), 1, source::MAX_WORK)?;
    if spec.positions.len() != 1
        || spec.accounts.len() != 1
        || !spec.reverse_severities.is_empty()
        || !spec.cashflows.is_empty()
        || !spec.policies.is_empty()
        || !spec.netting_sets.is_empty()
        || spec.horizon_days != r.source.original.horizon_days as usize
    {
        return Err(
            "revision ledger requires one isolated liability, full tail and ordinary explicit runs"
                .into(),
        );
    }
    let p = &spec.positions[0];
    let a = &spec.accounts[0];
    let anchor = &r.source.anchor;
    if p.id.as_str() != r.position_id
        || p.account.as_str() != r.account_id
        || a.id != r.account_id
        || a.currency != r.source.original.identity.currency
        || a.entity != r.source.original.identity.entity_id
        || p.kind.as_str() != "deposit"
        || p.classification.as_str() != "ac"
        || p.opening_accrued != 0.
        || !near(p.balance, anchor.original_principal)
        || !near(
            p.book_adjustment,
            anchor.original_gross_carrying_value - anchor.original_principal,
        )
        || p.start_day != 0
        || p.rate != 0.
        || p.annual_pd != 0.
        || p.allowance != 0.
        || p.commitment != 0.
        || p.monthly_runoff != 0.
        || p.floating_beta != 0.
        || p.migration_rate != 0.
        || p.draw_fraction != 0.
        || a.annual_fees != 0.
        || a.annual_costs != 0.
        || a.annual_dividends != 0.
        || a.tax_rate != 0.
    {
        return Err(
            "revision owner must retain original cost/opening with no legacy financial overlay"
                .into(),
        );
    }
    for x in [
        p.balance,
        p.book_adjustment,
        p.opening_accrued,
        a.cash,
        a.equity,
    ] {
        money(x)?;
    }
    let required: BTreeSet<_> = std::iter::once("baseline")
        .chain(spec.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let supplied: BTreeSet<_> = r
        .source
        .runs
        .iter()
        .map(|s| s.scenario_id.as_str())
        .collect();
    if required != supplied {
        return Err("revision source must cover exactly baseline plus all named scenarios".into());
    }
    let ledger_reserve = r
        .source
        .runs
        .len()
        .checked_mul(spec.horizon_days.checked_add(1).ok_or("days overflow")?)
        .and_then(|n| n.checked_mul(65536))
        .ok_or("ledger reservation overflow")?;
    let reserved_bytes = source_admission
        .reserved_bytes
        .checked_add(ledger_reserve)
        .ok_or("joined bytes overflow")?;
    let replay_reserve = r
        .source
        .runs
        .len()
        .checked_mul(spec.horizon_days.checked_add(1).ok_or("days overflow")?)
        .and_then(|n| n.checked_mul(8192))
        .ok_or("replay work overflow")?;
    let work_units = source_admission
        .work_units
        .checked_add(replay_reserve)
        .and_then(|n| {
            n.checked_add(
                r.source
                    .runs
                    .len()
                    .checked_mul(spec.horizon_days.checked_add(1)?)?
                    .checked_mul(16)?,
            )
        })
        .ok_or("joined work overflow")?;
    if reserved_bytes > source::MAX_BYTES || work_units > source::MAX_WORK {
        return Err("joined revision source/replay reservation exceeds 8M/32MiB".into());
    }
    // All sources/runs are admitted before generating any physical history.
    let native = source::evaluate_with_check(&r.source, portfolio_compute_control::checkpoint)?;
    let asof = r.source.original.as_of_ordinal;
    let mut events = Vec::new();
    let mut plans = Vec::new();
    let mut coverage = Vec::new();
    let mut runs = Vec::new();
    let mut cash_origins = Vec::new();
    for (path_index, path) in native.paths.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let selector = ScenarioSelector::Named {
            scenario_id: path.run_id.clone(),
        };
        runs.push(RunIdentity {
            run_id: path.run_id.clone(),
            scenario_id: path.scenario_id.clone(),
            severity: path.severity,
            input_revision: path.source.identity.source_revision.clone(),
        });
        coverage.push(PositionCoverage {
            position_id: r.position_id.clone(),
            run: selector.clone(),
            status: CoverageStatus::CompleteEventSet {},
        });
        let mut remaining: Vec<_> = path.source.interest.iter().map(|x| x.amount).collect();
        for (cash_index, cash) in path.source.cashflows.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            let id = format!("cash:{path_index}:{cash_index}");
            let mut needed = -cash.cash_interest;
            let mut segments = Vec::new();
            let mut allocations = Vec::new();
            let full = matches!(
                cash.kind.as_str(),
                "interest_payment" | "contractual_maturity"
            );
            for (i, origin) in path.source.interest.iter().enumerate() {
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
                    model_id: "deposit_state_original_yield_revision".into(),
                    model_version: source::SCHEMA.into(),
                    input_revision: path.source.identity.source_revision.clone(),
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
        for (i, row) in path.income.rows.iter().enumerate().skip(1) {
            let e = &path.income_request.events[i];
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
                    model_id: "deposit_state_original_yield_revision".into(),
                    model_version: source::SCHEMA.into(),
                    input_revision: path.source.identity.source_revision.clone(),
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
    let source_id = r.source.contract.id.clone();
    let source_revision = r.source.contract.revision.clone();
    let dated_request = DatedLedgerRequest {
        schema: crate::dated_ledger::SCHEMA.into(),
        monetary_unit: "currency_units".into(),
        recognition_policy: r.recognition_policy.clone(),
        specification: r.specification.clone(),
        dated: DatedStoreRequest {
            schema: DATED_STORE_SCHEMA.into(),
            as_of_ordinal: asof,
            reporting_horizon_days: r.source.original.horizon_days,
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
                    horizon_days: r.source.original.horizon_days,
                    scenarios: native.paths.iter().map(|p| p.run_id.clone()).collect(),
                    events,
                },
                coverage,
            }],
            accrual_plans: plans,
        },
    };
    let dated = CompiledDatedLedger::compile(dated_request.clone(), 1, source::MAX_WORK)?;
    for run in dated.store.runs() {
        portfolio_compute_control::checkpoint()?;
        for event in dated.store.events().iter().filter(|e| {
            e.run_index
                .is_none_or(|index| dated.store.runs()[index].run_id == run.run_id)
        }) {
            let rights = dated.store.rights_at(
                &run.run_id,
                event.index,
                source_admission.full_end_ordinal,
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
        > r.source
            .runs
            .len()
            .checked_mul(spec.horizon_days.checked_add(1).ok_or("days overflow")?)
            .and_then(|n| n.checked_mul(16))
            .ok_or("dated reserve overflow")?
    {
        return Err("dated recognition work exceeded preflight reservation".into());
    }
    let actual_work = source_admission
        .work_units
        .checked_add(replay_reserve)
        .and_then(|n| n.checked_add(dated.work_units))
        .ok_or("actual joined work overflow")?;
    if actual_work > source::MAX_WORK {
        return Err("actual joined revision work exceeds 8M".into());
    }
    let mut revisions = Vec::with_capacity(native.paths.len());
    for path in &native.paths {
        revisions.push(Revision {
            run_index: dated.run_index(&path.scenario_id, path.severity)?,
            position_index: 0,
            account_index: 0,
            original_basis: anchor.original_gross_carrying_value - anchor.original_principal,
            revised_basis: path.revised_carrying_value - anchor.original_principal,
            amount: path.basis_revision,
            review: r.source.policy.review.clone(),
        });
    }
    let admission = Admission {
        work_units: actual_work,
        reserved_bytes,
        native_source_work: source_admission.work_units,
        ledger_work: dated
            .work_units
            .checked_add(replay_reserve)
            .ok_or("ledger work overflow")?,
    };
    portfolio_compute_control::checkpoint()?;
    Ok(Prepared {
        dated,
        prepared_request: dated_request,
        revisions,
        native,
        raw_request: r,
        admission,
        cash_origins,
    })
}
pub fn run(p: Prepared, output: Box<dyn std::io::Write + Send>) -> Result<(), String> {
    crate::daily::run_dated_with_deposit_revision(&p.dated, &p.revisions, output)
}
pub fn parse_request(bytes: &[u8]) -> Result<(Request, Value), String> {
    if bytes.len() > 1024 * 1024 {
        return Err("revised deposit raw input exceeds1MiB".into());
    }
    crate::deposit_revision_json::parse(bytes)
}

pub fn parse_envelope(bytes: &[u8]) -> Result<(Request, u64, Value), String> {
    if bytes.len() > 1024 * 1024 {
        return Err("revised deposit raw envelope exceeds1MiB".into());
    }
    crate::deposit_revision_json::envelope(bytes)
}

struct LimitedOutput {
    output: Box<dyn std::io::Write + Send>,
    bytes: usize,
}
impl std::io::Write for LimitedOutput {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        let next = self.bytes.checked_add(data.len()).ok_or_else(|| std::io::Error::other("output count overflow"))?;
        if next > source::MAX_BYTES {
            return Err(std::io::Error::other("joined source/stream output32MiB cap"));
        }
        let written = self.output.write(data)?;
        self.bytes = self.bytes.checked_add(written).ok_or_else(|| std::io::Error::other("output count overflow"))?;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> { self.output.flush() }
}

/// The existing modeled-source wire; all financial amounts remain native source owned.
pub fn run_public(p: Prepared, raw: Value, output: Box<dyn std::io::Write + Send>) -> Result<(), String> {
    use std::io::Write;
    let (typed, _) = parse_request(&serde_json::to_vec(&raw).map_err(|e|e.to_string())?)?;
    if serde_json::to_vec(&typed).map_err(|e|e.to_string())? != serde_json::to_vec(&p.raw_request).map_err(|e|e.to_string())? {
        return Err("original revised deposit raw request differs from prepared source".into());
    }
    drop(typed);
    let header = serde_json::json!({"workflow": {
        "schema": "model-ledger-preparation-1", "model_schema": SCHEMA,
        "prepared_request": p.prepared_request,
        "source_receipt": {
            "raw_request": raw,
            "original_source":p.native.original_source, "original_pv":p.native.original_pv,
            "paths":p.native.paths, "revisions":p.revisions, "admission":p.admission,
            "cash_origins":p.cash_origins, "interest_rights_policy":p.raw_request.interest_rights_policy,
            "original_anchors":p.raw_request.source.anchor, "revision_policy":p.raw_request.source.policy,
            "accounting_standard_approval":false, "installed_adoption":false, "capital_ftp_eligible":false
        }
    }});
    let bytes = serde_json::to_vec(&header).map_err(|e|e.to_string())?;
    if bytes.len() > p.admission.reserved_bytes { return Err("actual retained header exceeds reservation".into()); }
    portfolio_compute_control::checkpoint()?;
    let mut output = LimitedOutput { output, bytes:0 };
    output.write_all(&bytes).map_err(|e|e.to_string())?;
    output.write_all(b"\n").map_err(|e|e.to_string())?;
    output.flush().map_err(|e|e.to_string())?;
    drop(header);
    drop(bytes);
    run(p, Box::new(output))
}
