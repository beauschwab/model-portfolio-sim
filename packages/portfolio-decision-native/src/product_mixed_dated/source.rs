use super::admission::{add, bounded, mul};
use super::*;
use portfolio_ledger_native::{dated_ledger::CompiledDatedLedger, dated_store::*};
use portfolio_model_core::cashflow::{
    AccrualPeriod, CashflowAdjustment, CashflowComponents, CashflowMeasure, DatedCashflowEvent,
    DatedSchedule, EventProvenance, ScenarioSelector, DATED_CASHFLOW_SCHEMA,
};
use portfolio_risk_native::{product_candidate_source, product_strategy};
use serde_json::json;
pub(super) fn prepare(r: &Request) -> Result<Prepared, String> {
    prepare_with_reservation(r, None, 0, 0)
}
pub(super) fn prepare_with_reservation(
    r: &Request,
    full_floor: Option<i32>,
    reserved_work: usize,
    reserved_bytes: usize,
) -> Result<Prepared, String> {
    let (combined, original_backbook) = admission::validate(r)?;
    let asof = r.backbook.dated.as_of_ordinal;
    let report = *r
        .candidate
        .product
        .reporting_ordinals
        .last()
        .ok_or("mixed reporting grid absent")?;
    let native = product_strategy::prepare(&r.candidate.product)?;
    let context = crate::product_candidate_journal::Request {
        schema: crate::product_candidate_journal::SCHEMA.into(),
        product: r.candidate.product.clone(),
        allocation: r.candidate.allocation.clone(),
        as_of_ordinal: asof,
        reporting_end_ordinal: report,
        owners: r.candidate.owners.clone(),
        policy: r.candidate.policy.clone(),
        run_mapping: r.candidate.run_mapping.clone(),
        specification: r.combined_specification.clone(),
        verified_opening_identity: r.opening_identity.clone(),
    };
    let candidate_work =
        crate::product_candidate_journal::audit_source_capacity(&context, &native)?;
    let (_, source_rows, candidate_full) =
        product_candidate_source::admitted_work(&native, asof, report)?;
    let mappings: std::collections::BTreeSet<_> = r
        .candidate
        .run_mapping
        .iter()
        .map(|m| (m.ledger_scenario_id.as_str(), m.severity.to_bits()))
        .collect();
    let runs: std::collections::BTreeSet<_> = original_backbook
        .store
        .runs()
        .iter()
        .map(|m| (m.scenario_id.as_ref(), m.severity.to_bits()))
        .collect();
    if mappings != runs || r.candidate.run_mapping.len() != runs.len() {
        return Err("mixed candidate/backbook require exact scenario/run/severity mapping".into());
    }
    let mut full = candidate_full.max(full_floor.unwrap_or(candidate_full));
    let mut declared = Vec::new();
    for source in &r.backbook.dated.sources {
        let end = asof
            .checked_add(
                i32::try_from(source.schedule.horizon_days)
                    .map_err(|_| "mixed full tail overflow")?,
            )
            .ok_or("mixed full tail overflow")?;
        full = full.max(end);
        declared.push(json!({"source_id":source.source_id,"source_revision":source.source_revision,"original_container_end_ordinal":end}));
    }
    let days = usize::try_from(full - asof).map_err(|_| "mixed reversed complete tail")?;
    if days > 1080 {
        return Err("mixed complete tails exceed 1080 days; no truncation supported".into());
    }
    let mut backbook_full = r.backbook.clone();
    backbook_full.specification["horizon_days"] = json!(days);
    backbook_full.dated.reporting_horizon_days = days as u32;
    for source in &mut backbook_full.dated.sources {
        source.schedule.horizon_days = days as u32;
    }
    let backbook_compiled =
        CompiledDatedLedger::compile(backbook_full.clone(), MAX_POSITIONS, MAX_WORK)?;
    admission::retirement(&backbook_compiled)?;
    // Aggregate before constructing daily native capture, acquisition lists or
    // generated dated schedules. Exact escaped identifiers and source records are
    // counted in addition to fixed numeric/JSON/container overhead.
    let (candidate_bytes, backbook_bytes, inventory_bytes) =
        capacity(r, &native, &backbook_compiled, &combined, days)?;
    let total_bytes = add(
        add(add(candidate_bytes, backbook_bytes)?, inventory_bytes)?,
        mul(
            add(
                bounded(r, 32 * 1024 * 1024)?,
                bounded(&native, 32 * 1024 * 1024)?,
            )?,
            4,
        )?,
    )?;
    let store_indexing = mul(
        add(
            original_backbook.store.events().len(),
            original_backbook.store.segments().len(),
        )?,
        runs.len(),
    )?;
    let path_count = native
        .source()
        .units
        .iter()
        .try_fold(0usize, |n, u| add(n, u.income.len()))?;
    let candidate_inventory = mul(mul(r.candidate.product.units.len(), days)?, runs.len())?;
    let indexed = add(
        add(
            add(
                mul(
                    add(
                        add(
                            backbook_compiled.store.events().len(),
                            backbook_compiled.store.segments().len(),
                        )?,
                        mul(source_rows, 2)?,
                    )?,
                    runs.len(),
                )?,
                mul(combined.positions.len(), 2)?,
            )?,
            source_rows,
        )?,
        mul(path_count, 16)?,
    )?;
    let compiled_upper = add(
        add(
            add(backbook_compiled.work_units, candidate_inventory)?,
            mul(source_rows, 3)?,
        )?,
        indexed,
    )?;
    let work = add(
        add(add(candidate_work, compiled_upper)?, store_indexing)?,
        mul(source_rows, 24)?,
    )?;
    let work = add(work, reserved_work)?;
    if add(total_bytes, reserved_bytes)? > MAX_BYTES || work > MAX_WORK {
        return Err(
            "mixed aggregate source/journal/capacity admission exceeded before generation".into(),
        );
    }
    let capture =
        product_candidate_source::capture(&native, &r.candidate.allocation, asof, report)?;
    let mut dated = backbook_full;
    dated.specification = r.combined_specification.clone();
    dated.specification["horizon_days"] = json!(days);
    let original_sources: std::collections::BTreeSet<_> = dated
        .dated
        .owners
        .iter()
        .map(|o| (o.source_id.clone(), o.source_revision.clone()))
        .collect();
    let mut acquisitions = acquisition::Request {
        schema: acquisition::SCHEMA.into(),
        policy: acquisition::Identity {
            id: r.policy.identity.id.clone(),
            revision: r.policy.identity.revision.clone(),
        },
        events: Vec::new(),
        right_ownership: "physical_path_net_rights-1".into(),
        reporting_ordinal: Some(report),
    };
    for (ui, owner) in r.candidate.owners.iter().enumerate() {
        if original_sources.contains(&(owner.source.id.clone(), owner.source.revision.clone())) {
            return Err("mixed candidate/backbook source identities overlap".into());
        }
        let u = &r.candidate.product.units[ui];
        let side = if matches!(
            u.purchase_graph,
            portfolio_risk_native::product_units::GraphRequest::Deposit(_)
        ) {
            -1.
        } else {
            1.
        };
        dated.dated.owners.push(OwnerMapping {
            position_id: owner.position_id.clone(),
            account_id: owner.account_id.clone(),
            currency: r.candidate.product.currency.clone(),
            direction: if side > 0. {
                Direction::Receipt
            } else {
                Direction::Payment
            },
            ownership: Ownership::FullyScenarioGenerated,
            source_id: owner.source.id.clone(),
            source_revision: owner.source.revision.clone(),
        });
        let mut events = Vec::new();
        let mut coverage = Vec::new();
        for path in capture.paths.iter().filter(|p| p.unit_index == ui) {
            portfolio_compute_control::checkpoint()?;
            let mapping = r
                .candidate
                .run_mapping
                .iter()
                .find(|m| m.source_scenario_id == path.scenario_id)
                .ok_or("mixed candidate scenario absent")?;
            let run = dated
                .dated
                .runs
                .iter()
                .find(|m| {
                    m.scenario_id == mapping.ledger_scenario_id && m.severity == mapping.severity
                })
                .ok_or("mixed dated run absent")?;
            let selector = ScenarioSelector::Named {
                scenario_id: run.run_id.clone(),
            };
            if !coverage
                .iter()
                .any(|c: &PositionCoverage| c.run == selector)
            {
                coverage.push(PositionCoverage {
                    position_id: owner.position_id.clone(),
                    run: selector.clone(),
                    status: CoverageStatus::CompleteEventSet {},
                });
            }
            let factor = path.allocation_factor * path.source_weight;
            let signed = side * factor;
            let mut event_ids = Vec::new();
            if path.daily_income.rows.len() != path.daily_income_request.events.len() {
                return Err("mixed income source row/event shape mismatch".into());
            }
            for (row, cash) in path
                .daily_income
                .rows
                .iter()
                .zip(&path.daily_income_request.events)
            {
                portfolio_compute_control::checkpoint()?;
                if row.payment_ordinal != cash.payment_ordinal || row.event_id != cash.event_id {
                    return Err("mixed income source row/event identity mismatch".into());
                }
                let id = format!(
                    "candidate:{ui}:{}:{}",
                    path.native_source_index, row.payment_ordinal
                );
                event_ids.push(id.clone());
                let interval = AccrualPeriod {
                    start_ordinal: row.payment_ordinal - 1,
                    end_ordinal: row.payment_ordinal,
                };
                let components = CashflowComponents {
                    principal: cash.principal_cash * signed,
                    cash_interest: cash.interest_cash * signed,
                    accrued_interest: row.contractual_interest_earned * signed,
                    book_amortization: row.basis_amortization * signed,
                    fees: 0.,
                };
                dated.dated.accrual_plans.push(AccrualPlanInput {
                    position_id: owner.position_id.clone(),
                    event_id: id.clone(),
                    run: selector.clone(),
                    segments: vec![RecognitionSegment {
                        start_ordinal: interval.start_ordinal,
                        end_ordinal: interval.end_ordinal,
                        accrued_interest: components.accrued_interest,
                        book_amortization: components.book_amortization,
                    }],
                });
                events.push(DatedCashflowEvent {
                    position_id: owner.position_id.clone(),
                    event_id: id,
                    scenario: selector.clone(),
                    payment_ordinal: row.payment_ordinal,
                    day_offset: (row.payment_ordinal - asof) as u32,
                    accrual_period: Some(interval),
                    currency: r.candidate.product.currency.clone(),
                    components,
                    measure: CashflowMeasure::Physical,
                    provenance: EventProvenance::Modeled {
                        model_id: "product-candidate-source-1".into(),
                        model_version: "product-candidate-source-1".into(),
                        input_revision: r.candidate.product.identity.input_revision.clone(),
                    },
                    adjustment: CashflowAdjustment::None {},
                });
            }
            let coupon_sign = product_candidate_source::contractual_interest_sign(path)?;
            acquisitions.events.push(acquisition::Event {
                event_id: format!("purchase:{ui}:{}", path.native_source_index),
                run_id: run.run_id.clone(),
                position_id: owner.position_id.clone(),
                account_id: owner.account_id.clone(),
                currency: r.candidate.product.currency.clone(),
                source: acquisition::Identity {
                    id: owner.source.id.clone(),
                    revision: owner.source.revision.clone(),
                },
                path_id: path.path_id.clone(),
                source_weight: path.source_weight,
                allocation_factor: path.allocation_factor,
                principal: path.unit_face * factor,
                cost: path.daily_income_request.opening_gross_carrying_value * factor,
                ordinal: path.purchase_ordinal,
                original_yield_anchor: acquisition::Identity {
                    id: u.income_policy.original_yield_anchor.id.clone(),
                    revision: u.income_policy.original_yield_anchor.revision.clone(),
                },
                measure: CashflowMeasure::Physical,
                recognition_event_ids: event_ids,
                coupon_sign,
            });
        }
        // The shared schedule contract requires chronological rows across all
        // named runs. Stable date sorting retains physical source/path order.
        events.sort_by_key(|event| event.payment_ordinal);
        dated.dated.sources.push(ScheduleSource {
            source_id: owner.source.id.clone(),
            source_revision: owner.source.revision.clone(),
            schedule: DatedSchedule {
                schema: DATED_CASHFLOW_SCHEMA.into(),
                as_of_ordinal: asof,
                horizon_days: days as u32,
                scenarios: dated.dated.runs.iter().map(|r| r.run_id.clone()).collect(),
                events,
            },
            coverage,
        });
    }
    let compiled = CompiledDatedLedger::compile_with_acquisitions(
        dated.clone(),
        &acquisitions,
        MAX_POSITIONS,
        MAX_WORK,
    )?;
    if compiled.work_units > compiled_upper {
        return Err("mixed generated work exceeds admitted immutable bound".into());
    }
    let admitted_work = work;
    let result = Prepared {
        raw_request: serde_json::to_value(r).map_err(|e| e.to_string())?,
        native_preparation: native,
        candidate_capture: capture,
        combined_dated_request: dated,
        acquisitions,
        backbook_source_ends: declared,
        reporting_end_ordinal: report,
        full_end_ordinal: full,
        admitted_work,
    };
    bounded(&result, MAX_BYTES / 4)?;
    Ok(result)
}
fn capacity(
    r: &Request,
    p: &ProductPrepared,
    b: &CompiledDatedLedger,
    spec: &portfolio_ledger_native::types::Spec,
    days: usize,
) -> Result<(usize, usize, usize), String> {
    let mut candidates = 0usize;
    for (i, u) in p.source().units.iter().enumerate() {
        for income in &u.income {
            portfolio_compute_control::checkpoint()?;
            let raw = &income.native_income;
            let rows = raw["native_income_result"]["rows"]
                .as_array()
                .ok_or("mixed retained income absent")?;
            let end = rows
                .last()
                .and_then(|x| x["payment_ordinal"].as_i64())
                .ok_or("mixed income full tail missing")?;
            let d = usize::try_from(end - r.candidate.product.units[i].purchase_ordinal as i64)
                .map_err(|_| "mixed capacity reversed")?;
            let source = u.complete_purchase_result["native_sources"]
                .as_array()
                .and_then(|x| {
                    x.iter().find(|x| {
                        x["role"] == "physical"
                            && x["scenario_id"] == income.scenario_id
                            && x["path_id"] == income.path_id
                    })
                })
                .ok_or("mixed capacity source absent")?;
            let cash_ids: Vec<_> = source["projection"]["cashflows"]
                .as_array()
                .ok_or("mixed capacity cash absent")?
                .iter()
                .map(|e| e["event_id"].clone())
                .collect();
            let deposit = matches!(
                r.candidate.product.units[i].purchase_graph,
                portfolio_risk_native::product_units::GraphRequest::Deposit(_)
            );
            let origins: Vec<_> = source["projection"]
                [if deposit { "interest" } else { "recognition" }]
            .as_array()
            .ok_or("mixed capacity recognition absent")?
            .iter()
            .map(|e| {
                if deposit {
                    json!(format!(
                        "deposit:{}:{}:{}",
                        e["segment_id"].as_str().unwrap_or(""),
                        e["start_ordinal"],
                        e["end_ordinal"]
                    ))
                } else {
                    e["origin_id"].clone()
                }
            })
            .collect();
            let metadata = json!([
                r.candidate.owners[i],
                r.candidate.policy,
                r.policy,
                r.candidate.run_mapping,
                u.unit_id,
                income.scenario_id,
                income.path_id,
                source["run_id"],
                origins,
                cash_ids,
                raw["raw_income_request"]["contract"],
                raw["raw_income_request"]["policy"],
                raw["raw_income_request"]["original_yield_anchor"]
            ]);
            let width = add(mul(bounded(&metadata, MAX_BYTES)?, 4)?, 8192)?;
            candidates = add(
                candidates,
                mul(add(add(mul(d, 24)?, mul(days, 2)?)?, 32)?, width)?,
            )?;
        }
    }
    let mut backbook = 0usize;
    for e in b.store.events() {
        portfolio_compute_control::checkpoint()?;
        let mut count = 8usize;
        for seg in &b.store.segments()[e.recognition_range.0..e.recognition_range.1] {
            count = add(
                count,
                usize::try_from(
                    (seg.end_ordinal.min(b.store.reporting_end_ordinal())
                        - seg.start_ordinal.max(b.store.as_of_ordinal()))
                    .max(0),
                )
                .map_err(|_| "mixed segment reversed")?,
            )?;
        }
        let metadata = json!([
            e,
            b.store.owners()[e.owner_index],
            b.store.runs(),
            r.backbook.recognition_policy
        ]);
        let width = add(mul(bounded(&metadata, MAX_BYTES)?, 4)?, 4096)?;
        backbook = add(
            backbook,
            mul(
                mul(
                    mul(count, 12)?,
                    if e.run_index.is_none() {
                        b.store.runs().len()
                    } else {
                        1
                    },
                )?,
                width,
            )?,
        )?;
    }
    let mut inventory = 0usize;
    for pos in &spec.positions {
        let width = add(
            mul(bounded(&json!([pos, b.store.runs()]), MAX_BYTES)?, 4)?,
            2048,
        )?;
        inventory = add(
            inventory,
            mul(mul(add(days, 8)?, b.store.runs().len())?, width)?,
        )?;
    }
    for account in &spec.accounts {
        let width = add(
            mul(bounded(&json!([account, b.store.runs()]), MAX_BYTES)?, 4)?,
            8192,
        )?;
        inventory = add(
            inventory,
            mul(mul(add(days, 8)?, b.store.runs().len())?, width)?,
        )?;
    }
    Ok((candidates, backbook, inventory))
}
