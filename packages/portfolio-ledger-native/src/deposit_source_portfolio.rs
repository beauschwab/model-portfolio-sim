//! Version2 exclusive multi-deposit sources into the existing single daily Sim.
use crate::{
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest, RecognitionPolicy},
    dated_store::*,
};
use portfolio_model_core::{cashflow::*, deposit_source_income_v2 as source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "deposit-performing-portfolio-ledger-2";
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub(crate) use crate::deposit_portfolio_money::gross;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathRequest {
    pub run_id: String,
    pub source: source::Request,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionRequest {
    pub position_id: String,
    pub account_id: String,
    pub paths: Vec<PathRequest>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub runs: Vec<Run>,
    pub positions: Vec<PositionRequest>,
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
    pub(crate) atom_events: Vec<Vec<usize>>,
}
#[derive(Debug, Serialize)]
pub struct OwnedPosition {
    pub(crate) position_index: usize,
    pub(crate) account_index: usize,
    pub(crate) paths: Vec<Path>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub source_work: usize,
    pub source_retained: usize,
    pub source_output: usize,
    pub ledger_work: usize,
    pub ledger_retained: usize,
    pub stream_reserved: usize,
    pub raw_bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct Usage {
    pub source_retained_used: usize,
    pub source_output_bytes: usize,
    pub source_charged_work: usize,
    pub dated_store_capacity: usize,
}
pub struct Prepared {
    pub(crate) dated: CompiledDatedLedger,
    pub(crate) prepared_request: DatedLedgerRequest,
    pub(crate) owners: Vec<OwnedPosition>,
    pub(crate) raw_request: Request,
    pub(crate) admission: Admission,
    pub(crate) cash_origins: Vec<CashOrigin>,
    pub(crate) usage: Usage,
}
#[derive(Debug, Serialize)]
pub struct CashOrigin {
    pub position_id: String,
    pub run_id: String,
    pub event_id: String,
    pub native_event_id: String,
    pub native_kind: String,
    pub full_native_accrued_settlement: bool,
    pub allocations: Vec<crate::deposit_source_principal::EarnedAllocation>,
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    source::add(a, b)
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    source::mul(a, b)
}
pub fn parse(b: &[u8]) -> Result<(Request, Value), String> {
    let v = crate::deposit_source_principal_json::value(b)?;
    if !v["recognition_policy"]["fees"].is_string() {
        return Err("fees string required".into());
    }
    for p in v["positions"].as_array().ok_or("positions required")? {
        for s in p["paths"].as_array().ok_or("complete paths required")? {
            source::parse(&serde_json::to_vec(&s["source"]).map_err(|e| e.to_string())?)?;
        }
    }
    let r = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok((r, v))
}
pub fn audit(r: &Request) -> Result<Admission, String> {
    audit_with_native_opening(r, &[])
}
/// Additive joined-owner admission. The coordinator must derive these anchors from raw native sources.
pub fn audit_with_native_opening(
    r: &Request,
    native_opening: &[crate::spec::NativeOpening],
) -> Result<Admission, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.runs.len() < 2
        || r.runs.len() > 8
        || r.interest_rights_policy != "native_earned_counted_fifo_cash_origin-2"
    {
        return Err("explicit bounded portfolio2 owner/run policy required".into());
    }
    if r.positions.capacity() > 256
        || r.runs.capacity() > 8
        || r.positions.iter().any(|p| p.paths.capacity() > 8)
    {
        return Err("typed owner/run arena exceeds bounded capacity".into());
    }
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw > 1_048_576 {
        return Err("portfolio raw exceeds1MiB".into());
    }
    let spec = if native_opening.is_empty() {
        crate::spec::normalize(r.specification.clone(), 256, MAX_WORK)?
    } else {
        crate::spec::normalize_with_native_opening(
            r.specification.clone(),
            256,
            MAX_WORK,
            native_opening,
        )?
    };
    if spec.positions.len() != r.positions.len()
        || !spec.cashflows.is_empty()
        || !spec.policies.is_empty()
        || !spec.netting_sets.is_empty()
        || !spec.reverse_severities.is_empty()
    {
        return Err("all positions need exclusive source ownership; generic flows/policies/reverse unsupported".into());
    }
    for a in &spec.accounts {
        if a.annual_fees != 0.
            || a.annual_costs != 0.
            || a.annual_dividends != 0.
            || a.tax_rate != 0.
        {
            return Err("account extras require separate reviewed ownership".into());
        }
    }
    let required: BTreeSet<_> = std::iter::once("baseline")
        .chain(spec.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let mut run_ids = BTreeSet::new();
    let mut scenarios = BTreeSet::new();
    for run in &r.runs {
        if !text(&run.run_id)
            || !text(&run.scenario_id)
            || run.severity.to_bits() != 1f64.to_bits()
            || !run_ids.insert(run.run_id.as_str())
            || !scenarios.insert(run.scenario_id.as_str())
        {
            return Err("duplicate/ambiguous physical runs or severity".into());
        }
    }
    if required != scenarios {
        return Err("exact complete baseline/named coverage required".into());
    }
    let positions: BTreeMap<_, _> = spec
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    let accounts: BTreeMap<_, _> = spec
        .accounts
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.as_str(), i))
        .collect();
    let mut seen = BTreeSet::new();
    let mut source_work = 0;
    let mut source_retained = 0;
    let mut source_output = 0;
    let mut ledger_retained = 0;
    let mut dates = None;
    for owner in &r.positions {
        portfolio_compute_control::checkpoint()?;
        if !text(&owner.position_id)
            || !text(&owner.account_id)
            || !seen.insert(owner.position_id.as_str())
        {
            return Err("duplicate exclusive position owner".into());
        }
        let &pi = positions
            .get(owner.position_id.as_str())
            .ok_or("unknown owned position")?;
        let &ai = accounts
            .get(owner.account_id.as_str())
            .ok_or("unknown account")?;
        let p = &spec.positions[pi];
        let a = &spec.accounts[ai];
        if p.account.as_str() != owner.account_id
            || p.kind.as_str() != "deposit"
            || p.classification.as_str() != "ac"
            || p.start_day != 0
            || p.opening_accrued != 0.
            || p.rate != 0.
            || p.annual_pd != 0.
            || p.allowance != 0.
            || p.commitment != 0.
            || p.monthly_runoff != 0.
            || p.floating_beta != 0.
            || p.migration_rate != 0.
            || p.draw_fraction != 0.
            || p.encumbered_fraction != 0.
            || !p.collateral_position.is_empty()
            || p.eligible_fraction != 0.
        {
            return Err("owned performing deposit generic overlays unsupported".into());
        }
        if a.annual_fees != 0.
            || a.annual_costs != 0.
            || a.annual_dividends != 0.
            || a.tax_rate != 0.
        {
            return Err("account extras require separate reviewed ownership".into());
        }
        let map: BTreeSet<_> = owner.paths.iter().map(|p| p.run_id.as_str()).collect();
        if map != run_ids || map.len() != owner.paths.len() {
            return Err(
                "each owner must declare every run once, including zero/unused sources".into(),
            );
        }
        let original = owner.paths.first().ok_or("no paths")?;
        let anchor = serde_json::to_vec(&(
            &original.source.continuation.original,
            &original.source.continuation.anchor,
            &original.source.continuation.contract,
        ))
        .map_err(|e| e.to_string())?;
        for path in &owner.paths {
            let plan = source::audit(&path.source, &mut portfolio_compute_control::checkpoint)?;
            let c = &path.source.continuation;
            let current_dates = (c.original.as_of_ordinal, c.original.horizon_days);
            if dates.is_some_and(|v| v != current_dates) {
                return Err("same explicit purchase/full horizon grid required".into());
            }
            dates = Some(current_dates);
            if c.current.identity.run_id != path.run_id
                || a.currency != c.original.identity.currency
                || a.entity != c.original.identity.entity_id
                || c.contract.id != owner.position_id
                || p.balance.to_bits() != c.anchor.original_principal.to_bits()
                || p.book_adjustment.to_bits()
                    != (c.anchor.original_gross_carrying_value - c.anchor.original_principal)
                        .to_bits()
                || anchor
                    != serde_json::to_vec(&(&c.original, &c.anchor, &c.contract))
                        .map_err(|e| e.to_string())?
            {
                return Err("original acquisition/contract/currency/face anchors mismatch".into());
            }
            source_work = add(source_work, plan.work_units)?;
            source_retained = add(source_retained, plan.retained_reserved)?;
            source_output = add(source_output, plan.output_reserved)?;
            // Counted derived events/plans, indexes, immutable IDs and temporary FIFO arena.
            ledger_retained = add(ledger_retained, mul(plan.horizon_days, 8192)?)?;
        }
        for v in [
            p.balance,
            p.book_adjustment,
            p.balance + p.book_adjustment,
            a.cash,
            a.equity,
        ] {
            crate::deposit_source_principal::money(v)?;
        }
    }
    gross(
        spec.positions
            .iter()
            .flat_map(|p| [p.balance, p.book_adjustment, p.opening_accrued])
            .chain(spec.accounts.iter().flat_map(|a| [a.cash, a.equity])),
    )?;
    let (_, days) = dates.ok_or("empty source grid")?;
    if spec.horizon_days != days as usize {
        return Err("spec must retain complete horizon".into());
    }
    let run_days = mul(r.runs.len(), add(days as usize, 1)?)?;
    let owned_days = mul(run_days, r.positions.len())?;
    let ledger_work = add(
        mul(owned_days, 2048)?,
        mul(run_days, mul(spec.accounts.len(), 256)?)?,
    )?;
    let stream_reserved = add(
        mul(owned_days, 12_288)?,
        mul(add(spec.positions.len(), spec.accounts.len())?, 16_384)?,
    )?;
    let work = add(
        source_work,
        add(source_output, add(ledger_work, mul(raw, 8)?)?)?,
    )?;
    let bytes = add(
        source_retained,
        add(
            source_output,
            add(ledger_retained, add(stream_reserved, mul(raw, 12)?)?)?,
        )?,
    )?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err("joined portfolio pre-generation8M/32MiB exceeded".into());
    }
    Ok(Admission {
        work_units: work,
        reserved_bytes: bytes,
        source_work,
        source_retained,
        source_output,
        ledger_work,
        ledger_retained,
        stream_reserved,
        raw_bytes: raw,
    })
}
pub fn prepare(r: Request) -> Result<Prepared, String> {
    prepare_with_native_opening(r, &[])
}
/// The empty anchor path preserves the historical source2 implementation exactly.
pub fn prepare_with_native_opening(
    r: Request,
    native_opening: &[crate::spec::NativeOpening],
) -> Result<Prepared, String> {
    let admission = audit_with_native_opening(&r, native_opening)?;
    let spec = if native_opening.is_empty() {
        crate::spec::normalize(r.specification.clone(), 256, MAX_WORK)?
    } else {
        crate::spec::normalize_with_native_opening(
            r.specification.clone(),
            256,
            MAX_WORK,
            native_opening,
        )?
    };
    let asof = r.positions[0].paths[0]
        .source
        .continuation
        .original
        .as_of_ordinal;
    let days = spec.horizon_days as u32;
    let mut events = Vec::new();
    let mut plans = Vec::new();
    let mut schedules = Vec::new();
    let mut mappings = Vec::new();
    let mut owners = Vec::new();
    let mut origins = Vec::new();
    let account_index: BTreeMap<_, _> = spec
        .accounts
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.as_str(), i))
        .collect();
    let raw_position: BTreeMap<_, _> = r
        .positions
        .iter()
        .map(|p| (p.position_id.as_str(), p))
        .collect();
    for (position_index, p) in spec.positions.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let owner = raw_position[p.id.as_str()];
        let ai = account_index[owner.account_id.as_str()];
        let account = &spec.accounts[ai];
        let mut paths = Vec::new();
        let mut coverage = Vec::new();
        let event_start = events.len();
        for (path_index, run) in r.runs.iter().enumerate() {
            let raw = owner
                .paths
                .iter()
                .find(|p| p.run_id == run.run_id)
                .ok_or("path missing")?;
            let native = source::evaluate(&raw.source, portfolio_compute_control::checkpoint)?;
            let current = &native.native.current;
            let captured = crate::deposit_fifo_derivation::compile_counted(
                &current.source,
                asof + days as i32,
                &mut portfolio_compute_control::checkpoint,
            )?;
            let selector = ScenarioSelector::Named {
                scenario_id: run.run_id.clone(),
            };
            coverage.push(PositionCoverage {
                position_id: p.id.to_string(),
                run: selector.clone(),
                status: CoverageStatus::CompleteEventSet {},
            });
            let settlement_index: BTreeMap<_, _> = captured
                .rights
                .settlements
                .iter()
                .map(|a| (a.native_cash_index, a))
                .collect();
            for range in &captured.cash {
                portfolio_compute_control::checkpoint()?;
                let c = &current.source.cashflows[range.native_cash_index];
                let id = format!(
                    "cash:{position_index}:{path_index}:{}",
                    range.native_cash_index
                );
                let segments = captured.segments[range.segment_start..range.segment_end].to_vec();
                let earned = segments.iter().map(|s| s.accrued_interest).sum();
                let accrual_period =
                    segments
                        .first()
                        .zip(segments.last())
                        .map(|(a, b)| AccrualPeriod {
                            start_ordinal: a.start_ordinal,
                            end_ordinal: b.end_ordinal,
                        });
                events.push(DatedCashflowEvent {
                    position_id: p.id.to_string(),
                    event_id: id.clone(),
                    scenario: selector.clone(),
                    payment_ordinal: c.ordinal,
                    day_offset: (c.ordinal - asof) as u32,
                    accrual_period,
                    currency: account.currency.clone(),
                    components: CashflowComponents {
                        principal: c.principal,
                        cash_interest: c.cash_interest,
                        accrued_interest: earned,
                        book_amortization: 0.,
                        fees: 0.,
                    },
                    measure: CashflowMeasure::Physical,
                    provenance: EventProvenance::Modeled {
                        model_id: "deposit_source_counted_portfolio".into(),
                        model_version: source::SCHEMA.into(),
                        input_revision: current.source.identity.source_revision.clone(),
                    },
                    adjustment: CashflowAdjustment::None {},
                });
                if !segments.is_empty() {
                    plans.push(AccrualPlanInput {
                        position_id: p.id.to_string(),
                        event_id: id.clone(),
                        run: selector.clone(),
                        segments,
                    });
                }
                let allocations = settlement_index
                    .get(&range.native_cash_index)
                    .map(|a| {
                        captured.rights.allocations[a.allocation_start..a.allocation_end]
                            .iter()
                            .map(|v| crate::deposit_source_principal::EarnedAllocation {
                                native_interest_index: v.native_interest_index,
                                amount: v.amount,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                origins.push(CashOrigin {
                    position_id: p.id.to_string(),
                    run_id: run.run_id.clone(),
                    event_id: id,
                    native_event_id: c.event_id.clone(),
                    native_kind: c.kind.clone(),
                    full_native_accrued_settlement: matches!(
                        c.kind.as_str(),
                        "interest_payment" | "contractual_maturity"
                    ),
                    allocations,
                });
            }
            for (i, row) in current.income.rows.iter().enumerate() {
                portfolio_compute_control::checkpoint()?;
                let e = &current.income_request.events[i];
                let id = format!("book:{position_index}:{path_index}:{i}");
                events.push(DatedCashflowEvent {
                    position_id: p.id.to_string(),
                    event_id: id.clone(),
                    scenario: selector.clone(),
                    payment_ordinal: e.payment_ordinal,
                    day_offset: (e.payment_ordinal - asof) as u32,
                    accrual_period: Some(AccrualPeriod {
                        start_ordinal: e.payment_ordinal - 1,
                        end_ordinal: e.payment_ordinal,
                    }),
                    currency: account.currency.clone(),
                    components: CashflowComponents {
                        principal: 0.,
                        cash_interest: 0.,
                        accrued_interest: 0.,
                        book_amortization: -row.basis_amortization,
                        fees: 0.,
                    },
                    measure: CashflowMeasure::Physical,
                    provenance: EventProvenance::Modeled {
                        model_id: "deposit_source_counted_portfolio".into(),
                        model_version: source::SCHEMA.into(),
                        input_revision: current.source.identity.source_revision.clone(),
                    },
                    adjustment: CashflowAdjustment::None {},
                });
                plans.push(AccrualPlanInput {
                    position_id: p.id.to_string(),
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
            paths.push(Path {
                run_id: run.run_id.clone(),
                scenario_id: run.scenario_id.clone(),
                severity: run.severity,
                native,
                atom_events: Vec::new(),
            });
        }
        let anchor = &owner.paths[0].source.continuation;
        let source_id = format!("owned:{}", p.id);
        let source_revision = anchor.contract.revision.clone();
        let mut owned_events = events.split_off(event_start);
        owned_events.sort_by_key(|e| e.payment_ordinal);
        schedules.push(ScheduleSource {
            source_id: source_id.clone(),
            source_revision: source_revision.clone(),
            schedule: DatedSchedule {
                schema: DATED_CASHFLOW_SCHEMA.into(),
                as_of_ordinal: asof,
                horizon_days: days,
                scenarios: r.runs.iter().map(|s| s.run_id.clone()).collect(),
                events: owned_events,
            },
            coverage,
        });
        mappings.push(OwnerMapping {
            position_id: p.id.to_string(),
            account_id: account.id.clone(),
            currency: account.currency.clone(),
            direction: Direction::Payment,
            ownership: Ownership::FullyScenarioGenerated,
            source_id,
            source_revision,
        });
        owners.push(OwnedPosition {
            position_index,
            account_index: ai,
            paths,
        });
    }
    let req = DatedLedgerRequest {
        schema: crate::dated_ledger::SCHEMA.into(),
        monetary_unit: "currency_units".into(),
        recognition_policy: r.recognition_policy.clone(),
        specification: r.specification.clone(),
        dated: DatedStoreRequest {
            schema: DATED_STORE_SCHEMA.into(),
            as_of_ordinal: asof,
            reporting_horizon_days: days,
            runs: r
                .runs
                .iter()
                .map(|s| RunIdentity {
                    run_id: s.run_id.clone(),
                    scenario_id: s.scenario_id.clone(),
                    severity: s.severity,
                    input_revision: "explicit-owner-source-revisions".into(),
                })
                .collect(),
            owners: mappings,
            sources: schedules,
            accrual_plans: plans,
        },
    };
    let dated = if native_opening.is_empty() {
        CompiledDatedLedger::compile(req.clone(), 256, MAX_WORK)?
    } else {
        CompiledDatedLedger::compile_with_native_opening(
            req.clone(),
            256,
            MAX_WORK,
            native_opening,
        )?
    };
    let by_event: BTreeMap<_, _> = dated
        .store
        .events()
        .iter()
        .map(|e| (e.event_id.as_ref(), e.index))
        .collect();
    let by_native: BTreeMap<_, _> = origins
        .iter()
        .map(|c| {
            (
                (
                    c.position_id.as_str(),
                    c.run_id.as_str(),
                    c.native_event_id.as_str(),
                ),
                by_event[c.event_id.as_str()],
            )
        })
        .collect();
    for owner in &mut owners {
        for path in &mut owner.paths {
            let id = spec.positions[owner.position_index].id.as_str();
            path.atom_events = path
                .native
                .native
                .current
                .atoms
                .iter()
                .map(|a| {
                    a.cash_event_ids
                        .iter()
                        .map(|c| {
                            by_native
                                .get(&(id, path.run_id.as_str(), c.as_str()))
                                .copied()
                                .ok_or_else(|| "atom cash event lineage absent".to_string())
                        })
                        .collect()
                })
                .collect::<Result<_, _>>()?;
        }
    }
    for run in dated.store.runs() {
        for e in dated.store.events().iter().filter(|e| {
            e.run_index
                .is_none_or(|i| dated.store.runs()[i].run_id == run.run_id)
        }) {
            portfolio_compute_control::checkpoint()?;
            let rights = dated.store.rights_at(
                &run.run_id,
                e.index,
                asof + days as i32,
                Boundary::AfterEvents,
            )?;
            if !crate::deposit_source_principal::near(rights.signed_interest_right, 0.)
                || !rights.cash_settled
                || !rights.accrual_complete
            {
                return Err("individual unweighted event/gross tail remains open".into());
            }
        }
    }
    portfolio_compute_control::checkpoint()?;
    let usage = Usage {
        source_retained_used: owners
            .iter()
            .flat_map(|o| &o.paths)
            .try_fold(0, |n, p| add(n, p.native.retained_used))?,
        source_output_bytes: owners
            .iter()
            .flat_map(|o| &o.paths)
            .try_fold(0, |n, p| add(n, p.native.output_bytes))?,
        source_charged_work: owners
            .iter()
            .flat_map(|o| &o.paths)
            .try_fold(0, |n, p| add(n, p.native.charged_work_used))?,
        dated_store_capacity: dated.store.retained_capacity_bytes(),
    };
    if usage.source_retained_used > admission.source_retained
        || usage.source_output_bytes > admission.source_output
        || usage.source_charged_work > admission.source_work
        || usage.dated_store_capacity > admission.ledger_retained
    {
        return Err("actual observed source/store arena exceeds reservation".into());
    }
    Ok(Prepared {
        dated,
        prepared_request: req,
        owners,
        raw_request: r,
        admission,
        cash_origins: origins,
        usage,
    })
}
impl Prepared {
    /// Immutable source-owned schedule and opening authority for a typed joined coordinator.
    pub fn dated(&self) -> &CompiledDatedLedger {
        &self.dated
    }
    pub fn request(&self) -> &Request {
        &self.raw_request
    }
    pub fn usage(&self) -> &Usage {
        &self.usage
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn prepared_request(&self) -> &DatedLedgerRequest {
        &self.prepared_request
    }
    /// Borrowed sealed native source receipts for a joint coordinator; no caller-supplied states.
    pub fn source_receipts(&self) -> (&[OwnedPosition], &[CashOrigin]) {
        (&self.owners, &self.cash_origins)
    }
    pub(crate) fn path(
        &self,
        index: usize,
        scenario: &str,
        severity: f64,
    ) -> Result<&Path, String> {
        self.owners[index]
            .paths
            .iter()
            .find(|p| p.scenario_id == scenario && p.severity.to_bits() == severity.to_bits())
            .ok_or_else(|| "explicit per-owner run missing".into())
    }
}
#[derive(Serialize)]
struct Receipt<'a> {
    raw_request: &'a Value,
    owners: &'a [OwnedPosition],
    cash_origins: &'a [CashOrigin],
    admission: &'a Admission,
    usage: &'a Usage,
    accounting_standard_approval: bool,
    empirical_validation: bool,
    installed_adoption: bool,
    saved_book_adoption: bool,
    capital_ftp_eligible: bool,
}
#[derive(Serialize)]
struct Header<'a> {
    schema: &'static str,
    model_schema: &'static str,
    prepared_request: &'a DatedLedgerRequest,
    source_receipt: Receipt<'a>,
}
#[derive(Serialize)]
struct Workflow<'a> {
    workflow: Header<'a>,
}
pub fn run_public(
    raw: &Value,
    p: &Prepared,
    output: Box<dyn std::io::Write + Send>,
) -> Result<(), String> {
    portfolio_compute_control::checkpoint()?;
    let (decoded, _) = parse(&serde_json::to_vec(raw).map_err(|e| e.to_string())?)?;
    if serde_json::to_vec(&decoded).map_err(|e| e.to_string())?
        != serde_json::to_vec(&p.raw_request).map_err(|e| e.to_string())?
    {
        return Err("immutable raw/DTO source mismatch".into());
    }
    let mut output = Bounded {
        inner: output,
        bytes: 0,
        // The second source-output allowance in joined work covers complete
        // borrowed header + Sim serialization, not only the header.
        cap: p.admission.source_output.min(MAX_BYTES),
    };
    serde_json::to_writer(
        &mut output,
        &Workflow {
            workflow: Header {
                schema: "deposit-performing-portfolio-workflow-2",
                model_schema: SCHEMA,
                prepared_request: &p.prepared_request,
                source_receipt: Receipt {
                    raw_request: raw,
                    owners: &p.owners,
                    cash_origins: &p.cash_origins,
                    admission: &p.admission,
                    usage: &p.usage,
                    accounting_standard_approval: false,
                    empirical_validation: false,
                    installed_adoption: false,
                    saved_book_adoption: false,
                    capital_ftp_eligible: false,
                },
            },
        },
    )
    .map_err(|e| e.to_string())?;
    use std::io::Write;
    output.write_all(b"\n").map_err(|e| e.to_string())?;
    crate::daily::run_dated_with_source_portfolio(p, Box::new(output))?;
    portfolio_compute_control::checkpoint()
}
struct Bounded {
    inner: Box<dyn std::io::Write + Send>,
    bytes: usize,
    cap: usize,
}
impl std::io::Write for Bounded {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        if self.bytes.checked_add(b.len()).is_none_or(|v| v > self.cap) {
            return Err(std::io::Error::other(
                "complete portfolio serialization exceeds charged work/output allowance",
            ));
        }
        let n = self.inner.write(b)?;
        self.bytes += n;
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        self.inner.flush()?;
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)
    }
}

/// Selected version2 branch: retain duplicate-preserving original envelope bytes.
pub fn parse_envelope(bytes: &[u8]) -> Result<(Request, u64, Value), String> {
    if bytes.len() > 1_048_576 {
        return Err("portfolio raw envelope1MiB bound".into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        request: Request,
        execution_timeout_ms: u64,
    }
    let e: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if e.execution_timeout_ms == 0 || e.execution_timeout_ms > 1_800_000 {
        return Err("explicit bounded execution timeout required".into());
    }
    let value = crate::deposit_source_principal_json::value(bytes)?;
    let raw = value
        .get("request")
        .ok_or("raw portfolio request required")?
        .clone();
    let (selected, _) = parse(&serde_json::to_vec(&raw).map_err(|e| e.to_string())?)?;
    if serde_json::to_vec(&selected).map_err(|e| e.to_string())?
        != serde_json::to_vec(&e.request).map_err(|e| e.to_string())?
    {
        return Err("selected portfolio raw/DTO identity differs".into());
    }
    Ok((e.request, e.execution_timeout_ms, raw))
}
