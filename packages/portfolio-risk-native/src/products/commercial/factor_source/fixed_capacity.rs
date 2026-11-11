//! Requested heap bounds for ordinary supplied fixed factors. Source admission
//! and its temporary JSON census precede this phase; caller/graph copies, credit
//! carry, allocator metadata and process RSS are separate reservations.
use super::{
    add, mul, ContractPathRequest, ContractPathResult, FactorPolicy, FactorSegment, NativeFactors,
};
use crate::products::commercial::credit_compiler::{fixed_contract_heap, fixed_policy_heap};
use crate::products::term::contracts::{self as term, indexed};
use portfolio_model_core::cashflow::{
    CashflowAdjustment, DatedCashflowEvent, DatedSchedule, EventProvenance, ScenarioSelector,
    DATED_CASHFLOW_SCHEMA,
};
use serde::Serialize;
use std::collections::HashSet;
use std::mem::size_of;

type R<T> = Result<T, String>;
fn sum(values: impl IntoIterator<Item = usize>) -> R<usize> {
    values.into_iter().try_fold(0, add)
}
fn copy_only<T: Copy>(_: T) {}
// Pinned hashbrown table layout: <=7/8 occupancy, power-of-two buckets,
// one control byte per bucket and sixteen trailing control bytes. Include the
// old table during growth rather than treating final capacity as peak storage.
fn hash_growth(entries: usize, slot_bytes: usize) -> R<usize> {
    if entries == 0 {
        return Ok(0);
    }
    let required = add(mul(entries, 8)?, 6)? / 7;
    let buckets = required
        .max(4)
        .checked_next_power_of_two()
        .ok_or("fixed hash capacity overflow")?;
    let current = add(mul(buckets, add(slot_bytes, 1)?)?, 16)?;
    let old = add(mul(buckets / 2, add(slot_bytes, 1)?)?, 16)?;
    add(current, old)
}
fn identity_heap(v: &term::Identity) -> R<usize> {
    let term::Identity { id, revision } = v;
    add(id.capacity(), revision.capacity())
}
fn identity_clone(v: &term::Identity) -> R<usize> {
    add(v.id.len(), v.revision.len())
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FixedCapacity {
    pub retained_bytes_upper: usize,
    pub construction_bytes_upper: usize,
}

impl NativeFactors {
    /// Derive all fixed-owner retained allocations before generating cash. The
    /// raw and policy values are moved, so spare input capacity is retained and
    /// cannot be replaced by their serialized length or requested Clone size.
    pub fn fixed_capacity(raw: &ContractPathRequest, policy: &FactorPolicy) -> R<FixedCapacity> {
        portfolio_compute_control::checkpoint()?;
        if size_of::<usize>() != 8
            || raw.periods.is_empty()
            || raw.periods.len() > term::MAX_PERIODS
        {
            return Err("fixed factor capacity requires bounded periods and 64-bit layout".into());
        }
        let input = add(fixed_contract_heap(raw)?.0, fixed_policy_heap(policy)?.0)?;
        let n = raw.periods.len();
        // Vec::with_capacity(n) or try_reserve_exact(n), with exactly one item
        // per period: events, audits, captured segments and cash indices. Fixed
        // coupons own no selected rate-source Vec entries or pending fixings.
        let vectors = mul(
            n,
            sum([
                size_of::<DatedCashflowEvent>(),
                size_of::<indexed::PeriodAudit>(),
                size_of::<FactorSegment>(),
                size_of::<usize>(),
            ])?,
        )?;
        let labels = raw.periods.iter().try_fold(0, |total, p| {
            portfolio_compute_control::checkpoint()?;
            // Event/audit/segment each own one copy of the period identity.
            add(total, mul(p.period_id.len(), 3)?)
        })?;
        let event_labels = mul(
            n,
            sum([
                raw.position_id.len(),
                raw.scenario_id.len(),
                raw.currency.len(),
                raw.assumption.id.len(),
                raw.assumption.revision.len(),
                term::MODEL_VERSION.len(),
            ])?,
        )?;
        let result_labels = sum([
            term::SCHEMA.len(),
            term::MODEL_VERSION.len(),
            identity_clone(&raw.contract)?,
            identity_clone(&raw.assumption)?,
            identity_clone(&raw.path)?,
            DATED_CASHFLOW_SCHEMA.len(),
            size_of::<String>(),
            raw.scenario_id.len(),
            indexed::PATH_DISCLOSURE.len(),
        ])?;
        let retained_bytes_upper = sum([
            size_of::<Self>(),
            input,
            vectors,
            labels,
            event_labels,
            result_labels,
        ])?;
        // Pinned Rust BTree nodes: <=n nodes per tree, each with eleven
        // borrowed str/index entries and <=128 bytes of parent/child metadata.
        // Two factor binding maps coexist. Conservatively retain all source
        // validation tables too, although they are destroyed before binding.
        // Contract validation owns a borrowed-period-id set. Dated validation
        // owns positions, scenarios, and event identities with a singleton
        // scenario set per event. The generated source has one position and
        // one scenario; no hash table owns cloned text.
        // Stable sort may additionally request at most n event slots. Its
        // scratch holds moved headers, not independently cloned String heaps.
        let node = add(mul(11, size_of::<(&str, usize)>())?, 128)?;
        let scratch = sum([
            mul(mul(n, 2)?, node)?,
            hash_growth(n, size_of::<&str>())?,
            mul(2, hash_growth(1, size_of::<&str>())?)?,
            hash_growth(n, size_of::<((&str, &str), (bool, HashSet<&str>))>())?,
            mul(n, hash_growth(1, size_of::<&str>())?)?,
            mul(n, size_of::<DatedCashflowEvent>())?,
        ])?;
        Ok(FixedCapacity {
            retained_bytes_upper,
            construction_bytes_upper: add(retained_bytes_upper, scratch)?,
        })
    }

    /// Complete actual retained owner inventory, used after construction as a
    /// guard against capacity changes. It does not substitute for the bound
    /// above and does not grant raw coordinator/publication authority.
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            raw,
            source,
            policy,
            segments,
            cash_indices,
            admission,
            queries_used,
            meter,
        } = self;
        let super::Admission {
            segment_bound,
            query_bound,
            work_upper,
            retained_bytes_upper,
            typed_source_structural_upper,
            calendar_year_scan_bound,
            counted_work,
        } = admission;
        copy_only((
            *segment_bound,
            *query_bound,
            *work_upper,
            *retained_bytes_upper,
            *typed_source_structural_upper,
            *calendar_year_scan_bound,
            queries_used.get(),
        ));
        if counted_work.is_some() || meter.is_some() {
            return Err("fixed factor inventory requires ordinary source ownership".into());
        }
        let segment_labels = segments
            .iter()
            .try_fold(0, |total, s| add(total, s.fixed_heap_capacity()?))?;
        sum([
            size_of::<Self>(),
            fixed_contract_heap(raw)?.0,
            fixed_policy_heap(policy)?.0,
            result_heap(source)?,
            mul(segments.capacity(), size_of::<FactorSegment>())?,
            segment_labels,
            mul(cash_indices.capacity(), size_of::<usize>())?,
        ])
    }
}

fn result_heap(v: &ContractPathResult) -> R<usize> {
    let ContractPathResult {
        schema,
        model_version,
        contract,
        assumption,
        path,
        schedule,
        audit,
        principal_after_reporting,
        disclosure,
    } = v;
    copy_only(*principal_after_reporting);
    let audits = audit.iter().try_fold(0, |total, row| {
        let indexed::PeriodAudit {
            period_id,
            opening_principal,
            interest,
            principal_paid,
            closing_principal,
            terminal_balloon,
            calendar_mapping_verified,
            selected_rate_sources,
        } = row;
        copy_only((
            *opening_principal,
            *interest,
            *principal_paid,
            *closing_principal,
            *terminal_balloon,
            *calendar_mapping_verified,
        ));
        if !selected_rate_sources.is_empty() {
            return Err("fixed factor audit retained indexed rates".into());
        }
        // Empty vectors can still retain spare capacity.
        add(
            total,
            add(
                period_id.capacity(),
                mul(
                    selected_rate_sources.capacity(),
                    size_of::<indexed::RateSource>(),
                )?,
            )?,
        )
    })?;
    sum([
        schema.capacity(),
        model_version.capacity(),
        identity_heap(contract)?,
        identity_heap(assumption)?,
        identity_heap(path)?,
        schedule_heap(schedule)?,
        mul(audit.capacity(), size_of::<indexed::PeriodAudit>())?,
        audits,
        disclosure.capacity(),
    ])
}

fn schedule_heap(v: &DatedSchedule) -> R<usize> {
    let DatedSchedule {
        schema,
        as_of_ordinal,
        horizon_days,
        scenarios,
        events,
    } = v;
    copy_only((*as_of_ordinal, *horizon_days));
    let labels = scenarios.iter().try_fold(0, |a, s| add(a, s.capacity()))?;
    let event_heap = events.iter().try_fold(0, |a, e| add(a, event_heap(e)?))?;
    sum([
        schema.capacity(),
        mul(scenarios.capacity(), size_of::<String>())?,
        labels,
        mul(events.capacity(), size_of::<DatedCashflowEvent>())?,
        event_heap,
    ])
}

fn event_heap(v: &DatedCashflowEvent) -> R<usize> {
    let DatedCashflowEvent {
        position_id,
        event_id,
        scenario,
        payment_ordinal,
        day_offset,
        accrual_period,
        currency,
        components,
        measure,
        provenance,
        adjustment,
    } = v;
    copy_only((
        *payment_ordinal,
        *day_offset,
        *accrual_period,
        *components,
        *measure,
    ));
    let ScenarioSelector::Named { scenario_id } = scenario else {
        return Err("fixed factor requires named event scenarios".into());
    };
    let EventProvenance::Modeled {
        model_id,
        model_version,
        input_revision,
    } = provenance
    else {
        return Err("fixed factor requires native modeled provenance".into());
    };
    if !matches!(adjustment, CashflowAdjustment::None {}) {
        return Err("fixed factor cannot own an adjusted cashflow".into());
    }
    sum([
        position_id.capacity(),
        event_id.capacity(),
        scenario_id.capacity(),
        currency.capacity(),
        model_id.capacity(),
        model_version.capacity(),
        input_revision.capacity(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use portfolio_model_core::cashflow::CashflowMeasure;

    fn sources() -> Vec<crate::commercial_credit_compiler::Request> {
        let raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let mut result = Vec::new();
        for side in ["completed_candidate", "revised_candidate"] {
            for unit in raw[side]["commercial"]["units"].as_array().unwrap() {
                for p in unit["graph"]["positions"].as_array().unwrap() {
                    for h in p["original_pricing"].as_array().unwrap() {
                        result.push(serde_json::from_value(h["source"].clone()).unwrap());
                    }
                    for hs in p["histories"].as_array().unwrap() {
                        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                            for h in hs[role].as_array().unwrap() {
                                result.push(serde_json::from_value(h["source"].clone()).unwrap());
                            }
                        }
                    }
                }
            }
        }
        result
    }

    #[test]
    fn all_original_factor_owners_match_raw_derived_retained_storage() {
        let sources = sources();
        assert_eq!(sources.len(), 27);
        for source in sources {
            for raw in [source.source.original, source.source.current] {
                let policy = source.factor_policy.clone();
                let capacity = NativeFactors::fixed_capacity(&raw, &policy).unwrap();
                let measure = raw.measure;
                let owner = NativeFactors::compile_for_measure(raw, policy, 0, measure).unwrap();
                assert_eq!(
                    owner.fixed_retained_capacity().unwrap(),
                    capacity.retained_bytes_upper
                );
                assert!(capacity.construction_bytes_upper <= owner.admission.retained_bytes_upper);
            }
        }
    }

    #[test]
    fn spare_inputs_are_retained_and_advanced_sources_refuse() {
        let mut source = sources().remove(0);
        let before =
            NativeFactors::fixed_capacity(&source.source.current, &source.factor_policy).unwrap();
        source.source.current.periods.reserve_exact(19);
        source.factor_policy.review.id.reserve_exact(8192);
        let after =
            NativeFactors::fixed_capacity(&source.source.current, &source.factor_policy).unwrap();
        assert!(after.retained_bytes_upper > before.retained_bytes_upper + 8192);
        let owner = NativeFactors::compile_for_measure(
            source.source.current.clone(),
            source.factor_policy.clone(),
            0,
            CashflowMeasure::Pricing,
        )
        .unwrap();
        // Clone requests only the logical length, and must not inherit spare capacity.
        assert_eq!(
            owner.fixed_retained_capacity().unwrap(),
            before.retained_bytes_upper
        );
        source.source.current.principal_plan = indexed::PrincipalPlan::RetainedPayment {
            original_payment: 100.0,
            terminal: indexed::TerminalPrincipal::Balloon { amount: 0.0 },
        };
        assert!(
            NativeFactors::fixed_capacity(&source.source.current, &source.factor_policy).is_err()
        );
    }
}
