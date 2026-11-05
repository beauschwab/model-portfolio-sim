//! Exhaustive input-layout census for the supplied fixed-source cut proof.
//! Counts actual retained capacities and exact requested Clone payloads, including
//! nonserialized defaults. This does not bound parsing, generated financial owners,
//! allocator metadata, or the complete coordinator, and grants no admission.
use super::{
    book, carry, path, source_income, term, ConditionInterval, Conditions, PathIdentity, Request,
};
use crate::products::commercial::factor_source::segments::FactorPolicy;
use portfolio_model_core::credit_accounting;
use serde::Serialize;
use std::mem::{needs_drop, size_of};

#[path = "credit_graph_capacity.rs"]
mod graph_capacity;
pub use graph_capacity::{finite_graph_raw_layout, finite_units_raw_layout};

type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b).ok_or("fixed raw layout overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b).ok_or("fixed raw layout overflow".into())
}

#[derive(Default, Debug, Clone, Copy)]
struct Heap {
    retained: usize,
    cloned: usize,
}
impl Heap {
    fn add(self, other: Self) -> R<Self> {
        Ok(Self {
            retained: add(self.retained, other.retained)?,
            cloned: add(self.cloned, other.cloned)?,
        })
    }
}
trait Layout {
    fn heap(&self) -> R<Heap>;
}
impl Layout for String {
    fn heap(&self) -> R<Heap> {
        Ok(Heap {
            retained: self.capacity(),
            cloned: self.len(),
        })
    }
}
impl<T: Layout> Layout for Vec<T> {
    fn heap(&self) -> R<Heap> {
        let mut result = Heap {
            retained: mul(self.capacity(), size_of::<T>())?,
            cloned: mul(self.len(), size_of::<T>())?,
        };
        for value in self {
            portfolio_compute_control::checkpoint()?;
            result = result.add(value.heap()?)?;
        }
        Ok(result)
    }
}
impl<T: Layout, const N: usize> Layout for [T; N] {
    fn heap(&self) -> R<Heap> {
        self.iter()
            .try_fold(Heap::default(), |h, v| h.add(v.heap()?))
    }
}
fn inline<T>(_: &T) -> R<()> {
    if needs_drop::<T>() {
        Err("fixed raw inline field acquired owned storage".into())
    } else {
        Ok(())
    }
}
impl Layout for f64 {
    fn heap(&self) -> R<Heap> {
        Ok(Heap::default())
    }
}
impl Layout for i32 {
    fn heap(&self) -> R<Heap> {
        Ok(Heap::default())
    }
}

// All fields must be named. Adding a DTO field is a compile error here; changing
// an inline field to an owning type rejects until its storage is inventoried.
macro_rules! fields {
    ($ty:path, [$($owned:ident),*], [$($scalar:ident),*]) => {
        const _: () = assert!(size_of::<$ty>() <= 8192);
        impl Layout for $ty {
            fn heap(&self) -> R<Heap> {
                let Self { $($owned,)* $($scalar,)* } = self;
                $(inline($scalar)?;)*
                let result = Heap::default();
                $(let result = result.add($owned.heap()?)?;)*
                Ok(result)
            }
        }
    };
}
const _: () = {
    assert!(size_of::<path::DrawStream>() <= 8192);
    assert!(size_of::<term::indexed::Coupon>() <= 8192);
    assert!(size_of::<term::indexed::PrincipalPlan>() <= 8192);
    assert!(size_of::<source_income::Request>() <= 8192);
    assert!(size_of::<FactorPolicy>() <= 8192);
    assert!(size_of::<Request>() <= 8192);
};
fields!(term::Identity, [id, revision], []);
fields!(credit_accounting::Identity, [id, revision], []);
fields!(
    source_income::Acquisition,
    [identity, original_yield_anchor, income_review],
    [cost, original_annual_effective_rate]
);
fields!(
    source_income::Policy,
    [identity, recognition, reset_income, fees],
    []
);
fields!(
    source_income::ProspectiveRateReview,
    [review, period_id],
    [reset_ordinal, annual_effective_rate]
);
fields!(
    ConditionInterval,
    [complete_recovery_tails],
    [
        end_year,
        hazards_per_year,
        loss_given_default,
        interest_recovery_fraction,
        credit_conversion_factor
    ]
);
fields!(PathIdentity, [path_id], [path_key, initial_live_state]);
fields!(
    Conditions,
    [identity, intervals, paths, draw_stream],
    [
        max_jumps_per_path,
        opening_undrawn_commitment,
        instrument_kind
    ]
);
fields!(path::RecoveryWeight, [], [lag_years, weight]);
fields!(
    path::UniformPair,
    [],
    [waiting_uniform, destination_uniform]
);
fields!(path::SuppliedPathDraws, [pairs], [path_key]);
fields!(
    book::Policy,
    [
        identity,
        recognition,
        cash_application,
        unrecognized_cash,
        exit_disposition,
        default_disposition
    ],
    []
);
fields!(
    book::RecognitionReview,
    [review],
    [interval_index, by_origin]
);
fields!(
    carry::RecognitionRate,
    [review],
    [interval_index, annual_effective_rates]
);
fields!(
    term::indexed::ContractPeriod,
    [period_id, coupon],
    [start_ordinal, end_ordinal, payment_ordinal, day_count, fee]
);
fields!(
    term::indexed::ContractPathRequest,
    [
        schema,
        model_version,
        contract,
        assumption,
        path,
        position_id,
        scenario_id,
        currency,
        monetary_unit,
        principal_plan,
        periods
    ],
    [
        direction,
        measure,
        asof_ordinal,
        reporting_end_ordinal,
        tail_end_ordinal,
        observed_publication_cutoff_unix_seconds,
        opening_principal
    ]
);

impl Layout for path::DrawStream {
    fn heap(&self) -> R<Heap> {
        match self {
            Self::Supplied {
                stream_id,
                stream_revision,
                paths,
            } => stream_id
                .heap()?
                .add(stream_revision.heap()?)?
                .add(paths.heap()?),
            Self::CounterSplitmix64V1 {
                stream_id,
                stream_revision,
                seed,
            } => {
                inline(seed)?;
                stream_id.heap()?.add(stream_revision.heap()?)
            }
        }
    }
}
impl Layout for term::indexed::Coupon {
    fn heap(&self) -> R<Heap> {
        match self {
            Self::Fixed { annual_rate } => {
                inline(annual_rate)?;
                Ok(Heap::default())
            }
            Self::Indexed { .. } => Err("fixed raw layout requires supplied fixed coupons".into()),
        }
    }
}
impl Layout for term::indexed::PrincipalPlan {
    fn heap(&self) -> R<Heap> {
        match self {
            Self::Supplied { principal_payments } => principal_payments.heap(),
            Self::DatedSuppliedPeriodOpening { .. } | Self::RetainedPayment { .. } => {
                Err("fixed raw layout requires supplied principal payments".into())
            }
        }
    }
}
fn absent<T>(value: &Option<T>) -> R<()> {
    if value.is_some() {
        Err("fixed raw layout requires separate advanced-owner inventory".into())
    } else {
        Ok(())
    }
}
impl Layout for source_income::Request {
    fn heap(&self) -> R<Heap> {
        let Self {
            schema,
            original,
            current,
            acquisition,
            transfer,
            policy,
            current_accounting_rate_reviews,
        } = self;
        absent(transfer)?;
        schema
            .heap()?
            .add(original.heap()?)?
            .add(current.heap()?)?
            .add(acquisition.heap()?)?
            .add(policy.heap()?)?
            .add(current_accounting_rate_reviews.heap()?)
    }
}
impl Layout for FactorPolicy {
    fn heap(&self) -> R<Heap> {
        let Self {
            review,
            clock,
            partial_calendar_day,
            overnight,
            observation_mapping,
            full_checkpoints,
            terminal_obligations,
        } = self;
        absent(terminal_obligations)?;
        review
            .heap()?
            .add(clock.heap()?)?
            .add(partial_calendar_day.heap()?)?
            .add(overnight.heap()?)?
            .add(observation_mapping.heap()?)?
            .add(full_checkpoints.heap()?)
    }
}
impl Layout for Request {
    fn heap(&self) -> R<Heap> {
        let Self {
            schema,
            source,
            factor_policy,
            credit,
            accounting,
            accounting_rates,
            prepayment_terms,
            maturity_basis_policy,
            performing_draw_tranches,
            reviewed_credit_actions,
            counted_work,
        } = self;
        absent(prepayment_terms)?;
        absent(maturity_basis_policy)?;
        absent(reviewed_credit_actions)?;
        absent(counted_work)?;
        if !performing_draw_tranches.is_empty() {
            return Err("fixed raw layout requires separate drawn-source inventory".into());
        }
        // An empty Vec can still own capacity. Never infer zero heap from the
        // skipped serialized field, even though its Clone requests zero bytes.
        let empty_draws = Heap {
            retained: mul(
                performing_draw_tranches.capacity(),
                size_of::<super::PerformingDrawTranche>(),
            )?,
            cloned: 0,
        };
        schema
            .heap()?
            .add(source.heap()?)?
            .add(factor_policy.heap()?)?
            .add(credit.heap()?)?
            .add(accounting.heap()?)?
            .add(accounting_rates.heap()?)?
            .add(empty_draws)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RawLayout {
    pub inline_bytes: usize,
    pub retained_heap_bytes: usize,
    pub cloned_heap_bytes: usize,
    pub retained_with_one_clone_bytes: usize,
}
/// Retained input storage and requested clone storage, excluding the inline
/// header. These share the exhaustive fixed-source DTO inventory above.
pub(crate) fn fixed_contract_heap(r: &term::indexed::ContractPathRequest) -> R<(usize, usize)> {
    let h = r.heap()?;
    Ok((h.retained, h.cloned))
}
pub(crate) fn fixed_policy_heap(r: &FactorPolicy) -> R<(usize, usize)> {
    let h = r.heap()?;
    Ok((h.retained, h.cloned))
}
/// Read-only layout of one already typed fixed request. This is not a preparse
/// admission or a census of its later financial source/carry/accounting owners.
pub fn finite_raw_layout(request: &Request) -> R<RawLayout> {
    portfolio_compute_control::checkpoint()?;
    let heap = request.heap()?;
    let inline_bytes = size_of::<Request>();
    Ok(RawLayout {
        inline_bytes,
        retained_heap_bytes: heap.retained,
        cloned_heap_bytes: heap.cloned,
        retained_with_one_clone_bytes: add(
            mul(inline_bytes, 2)?,
            add(heap.retained, heap.cloned)?,
        )?,
    })
}

fn nested_heap<T>(total: usize) -> R<usize> {
    total
        .checked_sub(size_of::<T>())
        .ok_or("fixed nested owner storage underflow".into())
}

#[derive(Debug, Serialize)]
pub struct FixedConstructionCapacity {
    pub preparation: FixedPreparationCapacity,
    pub original_factors: super::super::factor_source::owner::FixedCapacity,
    pub current_factors: super::super::factor_source::owner::FixedCapacity,
    pub carry_bytes_upper: usize,
    pub book_bytes_upper: usize,
    pub metadata_bytes_upper: usize,
    pub total_bytes_upper: usize,
}
#[derive(Debug, Serialize)]
pub struct FixedPreparationCapacity {
    pub intervals_upper: usize,
    pub boundaries_upper: usize,
    pub paths: usize,
    pub retained_bytes_upper: usize,
    pub construction_bytes_upper: usize,
}
fn vector<T>(count: usize) -> R<usize> {
    if count == 0 {
        return Ok(0);
    }
    mul(add(mul(count, 3)?, 8)?, size_of::<T>())
}
/// Prospective metadata expansion, before `prepare` constructs its daily clock.
/// Counts the complete recovery clock, repeated recovery vectors and reviews;
/// no financial source, cut checkpoint or serialized native receipt is trusted.
pub fn fixed_preparation_capacity(r: &Request) -> R<FixedPreparationCapacity> {
    let raw = finite_raw_layout(r)?;
    let full = super::full_credit_end_year(r)?;
    let days = (full * 365.).ceil();
    if !days.is_finite()
        || !(1. ..=4096.).contains(&days)
        || r.credit.intervals.is_empty()
        || r.credit.intervals.len() > 4096
        || r.accounting_rates.len() != r.credit.intervals.len()
        || r.accounting.recognition.len() != r.credit.intervals.len()
    {
        return Err("fixed preparation requires bounded complete clock and reviews".into());
    }
    // The sort/dedup can only remove cuts; `prepared` enforces <=4096 after
    // dedup. Reserve the larger pre-dedup vector independently.
    let cuts = add(add(r.credit.intervals.len(), days as usize)?, 1)?;
    let intervals_upper = cuts.min(4096);
    let boundaries_upper = r.source.current.periods.len();
    let paths = r.credit.paths.len();
    let recovery_heap = r.credit.intervals.iter().try_fold(0, |upper, i| {
        Ok::<_, String>(upper.max(i.complete_recovery_tails.heap()?.cloned))
    })?;
    let rate_heap = r
        .accounting_rates
        .iter()
        .try_fold(0, |upper, v| Ok::<_, String>(upper.max(v.heap()?.cloned)))?;
    let review_heap = r
        .accounting
        .recognition
        .iter()
        .try_fold(0, |upper, v| Ok::<_, String>(upper.max(v.heap()?.cloned)))?;
    let boundary_labels = r.source.current.periods.iter().try_fold(0, |sum, p| {
        add(sum, add(p.period_id.len(), r.accounting.identity.id.len())?)
    })?;
    let path_labels = r
        .credit
        .paths
        .iter()
        .try_fold(0, |sum, p| add(sum, p.path_id.len()))?;
    let source = &r.source.current;
    let labels = [
        carry::SCHEMA.len(),
        path::SCHEMA.len(),
        source.monetary_unit.len(),
        source.contract.id.len(),
        source.contract.revision.len(),
        r.credit.identity.id.len(),
        r.accounting.identity.id.len(),
        "cash_reduces_gross_no_proportional_basis_disposal-1".len(),
        "stop_income_transfer_actual_gross_retain_nominal_claims-1".len(),
    ]
    .into_iter()
    .try_fold(0, add)?;
    let retained_bytes_upper = [
        size_of::<carry::Request>(),
        size_of::<book::Policy>(),
        labels,
        vector::<path::PathInterval>(intervals_upper)?,
        mul(intervals_upper, recovery_heap)?,
        vector::<carry::RecognitionRate>(intervals_upper)?,
        mul(intervals_upper, rate_heap)?,
        // Policy clone followed by clear()/push(): count both old capacity
        // and a fresh expanded vector, including transient growth overlap.
        r.accounting.heap()?.cloned,
        vector::<book::RecognitionReview>(intervals_upper)?,
        mul(intervals_upper, review_heap)?,
        mul(boundaries_upper, size_of::<path::PathBoundary>())?,
        boundary_labels,
        mul(paths, size_of::<path::PathOpening>())?,
        path_labels,
        mul(
            paths,
            add(
                size_of::<carry::OriginalAnchor>(),
                r.source.acquisition.original_yield_anchor.id.len(),
            )?,
        )?,
        r.credit.draw_stream.heap()?.cloned,
    ]
    .into_iter()
    .try_fold(0, add)?;
    // Admission serializes original/current inputs into bounded temporary
    // buffers. Their JSON lengths can be computed without allocation; Vec
    // growth includes old/new allocations. Validation trees contain borrowed
    // keys only, at most one node per path/boundary/period/review. Reserve a
    // 4096-byte node per entry (larger than the pinned hash/BTree layouts).
    let encoded = portfolio_model_core::strict_json::serialized_size(r, 1 << 20)?;
    let entries = [
        paths,
        boundaries_upper,
        r.source.original.periods.len(),
        intervals_upper,
        r.accounting.recognition.len(),
        16,
    ]
    .into_iter()
    .try_fold(0, add)?;
    let construction_bytes_upper = [
        raw.retained_heap_bytes,
        retained_bytes_upper,
        vector::<f64>(cuts)?,
        mul(cuts, size_of::<f64>())?,
        vector::<f64>(intervals_upper)?,
        mul(boundaries_upper, size_of::<path::PathBoundary>())?,
        vector::<u8>(encoded)?,
        mul(entries, 4096)?,
    ]
    .into_iter()
    .try_fold(0, add)?;
    if construction_bytes_upper > 128 << 20 {
        return Err("fixed source preparation exceeds128MiB before expansion".into());
    }
    Ok(FixedPreparationCapacity {
        intervals_upper,
        boundaries_upper,
        paths,
        retained_bytes_upper,
        construction_bytes_upper,
    })
}
/// Supplied fixed-source construction including raw preparation. Includes
/// the borrowed input so parents can conservatively sum source phases. It is
/// not a bound on parsing or graph/Sim/codec owners.
pub fn fixed_construction_capacity(
    r: &Request,
    measure: portfolio_model_core::credit::CreditMeasure,
) -> R<FixedConstructionCapacity> {
    let raw = finite_raw_layout(r)?;
    let preparation = fixed_preparation_capacity(r)?;
    let physical = measure == portfolio_model_core::credit::CreditMeasure::Physical;
    let prepared = super::prepare(
        r,
        0,
        0,
        measure,
        &[],
        if physical {
            super::AccountingOwner::ReviewedBook
        } else {
            super::AccountingOwner::EconomicCarry
        },
    )?;
    let prepared_bytes = add(
        add(
            size_of::<carry::Request>(),
            prepared.credit.fixed_heap_capacity()?.0,
        )?,
        add(
            size_of::<book::Policy>(),
            prepared.policy.fixed_heap_capacity()?.0,
        )?,
    )?;
    if prepared.credit.raw_credit_path.intervals.len() > preparation.intervals_upper
        || prepared.credit.raw_credit_path.boundaries.len() > preparation.boundaries_upper
        || prepared_bytes > preparation.retained_bytes_upper
    {
        return Err("fixed prepared source exceeds prospective metadata inventory".into());
    }
    let original_factors =
        super::NativeFactors::fixed_capacity(&r.source.original, &r.factor_policy)?;
    let current_factors =
        super::NativeFactors::fixed_capacity(&r.source.current, &r.factor_policy)?;
    let periods = r.source.current.periods.len();
    let carry_bytes_upper = prepared.credit.fixed_construction_capacity(periods)?;
    let book_bytes_upper = if physical {
        prepared.policy.fixed_construction_capacity(
            prepared.admission.carry_points_upper,
            r.credit.paths.len(),
            periods,
        )?
    } else {
        0
    };
    // Result raw clone, live prepared request/policy and SourceAdapter vectors
    // coexist. Both complete factor phases and both core construction scratch
    // inventories are added, even where their maxima cannot occur together.
    let metadata_bytes_upper = [
        raw.retained_with_one_clone_bytes,
        size_of::<super::ResultData>().max(size_of::<super::EconomicResult>()),
        prepared.credit.fixed_heap_capacity()?.0,
        prepared.policy.fixed_heap_capacity()?.0,
        size_of::<super::SourceAdapter<'_>>(),
        mul(add(mul(r.credit.paths.len(), 3)?, 8)?, size_of::<u64>())?,
        mul(
            add(mul(prepared.credit.raw_credit_path.intervals.len(), 3)?, 8)?,
            size_of::<f64>(),
        )?,
    ]
    .into_iter()
    .try_fold(0, add)?;
    let total_bytes_upper = [
        preparation.construction_bytes_upper,
        original_factors.construction_bytes_upper,
        current_factors.construction_bytes_upper,
        carry_bytes_upper,
        book_bytes_upper,
        metadata_bytes_upper,
    ]
    .into_iter()
    .try_fold(0, add)?;
    Ok(FixedConstructionCapacity {
        preparation,
        original_factors,
        current_factors,
        carry_bytes_upper,
        book_bytes_upper,
        metadata_bytes_upper,
        total_bytes_upper,
    })
}

// A new adapter field must be assigned to the ordinary source reservation.
const _: fn(&super::SourceAdapter<'_>) = |super::SourceAdapter {
                                              selected_finite_tail: _,
                                              source: _,
                                              path_keys: _,
                                              stops: _,
                                              prepayment_terms: _,
                                              maturity_basis_policy: _,
                                              draw_sources: _,
                                              active_draws: _,
                                              accounting: _,
                                              complete_end_year: _,
                                              interval_starts: _,
                                              transfer: _,
                                          }| {};
fn empty_capacity<T>(v: &Vec<T>) -> R<usize> {
    if !v.is_empty() {
        return Err("fixed source requires separate stopped/drawn owner inventory".into());
    }
    mul(v.capacity(), size_of::<T>())
}
impl super::ResultData {
    /// Every retained field of an ordinary fixed Physical owner. Full raw,
    /// prepared credit, carry, account snapshots and both source clocks remain
    /// included; shared parent inline headers are counted exactly once.
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            raw_request,
            admission,
            selected_accounting_owner,
            original_source,
            current_source,
            original_cash_quote,
            secondary_acquisition,
            prepared_credit_request,
            carry,
            accounting,
            original_anchor_refitted,
            prospective_accounting_rate_policy,
            mixed_ledger_adoption,
            empirical_adoption,
            accounting_approval,
            native_stops,
            draw_sources,
        } = self;
        inline(schema)?;
        inline(admission)?;
        inline(selected_accounting_owner)?;
        inline(original_cash_quote)?;
        inline(original_anchor_refitted)?;
        inline(prospective_accounting_rate_policy)?;
        inline(mixed_ledger_adoption)?;
        inline(empirical_adoption)?;
        inline(accounting_approval)?;
        absent(secondary_acquisition)?;
        [
            size_of::<Self>(),
            raw_request.heap()?.retained,
            nested_heap::<super::NativeFactors>(original_source.fixed_retained_capacity()?)?,
            nested_heap::<super::NativeFactors>(current_source.fixed_retained_capacity()?)?,
            prepared_credit_request.fixed_heap_capacity()?.0,
            nested_heap::<carry::ResultData>(carry.fixed_retained_capacity()?)?,
            nested_heap::<book::ResultData>(accounting.fixed_retained_capacity()?)?,
            empty_capacity(native_stops)?,
            empty_capacity(draw_sources)?,
        ]
        .into_iter()
        .try_fold(0, add)
    }
}
impl super::EconomicResult {
    /// Pricing owns its native carry and factors but no Physical coupon book.
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            secondary_acquisition,
            original_source,
            current_source,
            carry,
            admission,
            selected_accounting_owner,
            physical_journal_eligible,
            native_stops,
            draw_sources,
        } = self;
        absent(secondary_acquisition)?;
        inline(admission)?;
        inline(selected_accounting_owner)?;
        inline(physical_journal_eligible)?;
        [
            size_of::<Self>(),
            nested_heap::<super::NativeFactors>(original_source.fixed_retained_capacity()?)?,
            nested_heap::<super::NativeFactors>(current_source.fixed_retained_capacity()?)?,
            nested_heap::<carry::ResultData>(carry.fixed_retained_capacity()?)?,
            empty_capacity(native_stops)?,
            empty_capacity(draw_sources)?,
        ]
        .into_iter()
        .try_fold(0, add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sources() -> Vec<Request> {
        let raw: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let mut sources = Vec::new();
        for side in ["completed_candidate", "revised_candidate"] {
            for unit in raw[side]["commercial"]["units"].as_array().unwrap() {
                for position in unit["graph"]["positions"].as_array().unwrap() {
                    for history in position["original_pricing"].as_array().unwrap() {
                        sources.push(serde_json::from_value(history["source"].clone()).unwrap());
                    }
                    for history in position["histories"].as_array().unwrap() {
                        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
                            for path in history[role].as_array().unwrap() {
                                sources
                                    .push(serde_json::from_value(path["source"].clone()).unwrap());
                            }
                        }
                    }
                }
            }
        }
        sources
    }
    #[test]
    fn full_physical_and_pricing_owner_storage_keeps_all_original_sources() {
        let sources = sources();
        assert_eq!(sources.len(), 27);
        for source in sources {
            if source.source.current.measure
                == portfolio_model_core::cashflow::CashflowMeasure::Physical
            {
                let c = fixed_construction_capacity(
                    &source,
                    portfolio_model_core::credit::CreditMeasure::Physical,
                )
                .unwrap();
                let owner = super::super::evaluate(&source).unwrap();
                let bytes = owner.fixed_retained_capacity().unwrap();
                assert!(bytes <= owner.admission().joined_bytes_upper);
                assert!(bytes > finite_raw_layout(&source).unwrap().retained_heap_bytes);
                assert!(bytes <= c.total_bytes_upper);
                assert!(
                    c.total_bytes_upper <= owner.admission().joined_bytes_upper,
                    "{c:?}"
                );
            } else {
                let owner = super::super::evaluate_economic_with_measure(
                    &source,
                    portfolio_model_core::credit::CreditMeasure::Pricing,
                )
                .unwrap();
                let bytes = owner.fixed_retained_capacity().unwrap();
                let c = fixed_construction_capacity(
                    &source,
                    portfolio_model_core::credit::CreditMeasure::Pricing,
                )
                .unwrap();
                assert!(bytes <= owner.admission().joined_bytes_upper);
                assert!(bytes > owner.current_source().fixed_retained_capacity().unwrap());
                assert!(bytes <= c.total_bytes_upper);
                assert!(
                    c.total_bytes_upper <= owner.admission().joined_bytes_upper,
                    "{c:?}"
                );
            }
        }
    }
    #[test]
    fn all_original_and_current_sources_have_exact_clone_payloads() {
        let sources = sources();
        assert_eq!(sources.len(), 27);
        for source in sources {
            let original = finite_raw_layout(&source).unwrap();
            let cloned = finite_raw_layout(&source.clone()).unwrap();
            assert_eq!(original.cloned_heap_bytes, cloned.retained_heap_bytes);
            assert_eq!(original.cloned_heap_bytes, cloned.cloned_heap_bytes);
            assert!(original.retained_heap_bytes >= original.cloned_heap_bytes);
        }
    }
    #[test]
    fn skipped_empty_draw_storage_and_spare_string_capacity_are_not_erased() {
        let mut source = sources().remove(0);
        let before = finite_raw_layout(&source).unwrap();
        source.performing_draw_tranches.reserve_exact(7);
        let capacity = source.performing_draw_tranches.capacity();
        source.schema.reserve_exact(1000);
        let after = finite_raw_layout(&source).unwrap();
        let delta = capacity * size_of::<super::super::PerformingDrawTranche>()
            + source.schema.capacity()
            - source.schema.len();
        assert_eq!(
            after.retained_heap_bytes - before.retained_heap_bytes,
            delta
        );
        assert_eq!(after.cloned_heap_bytes, before.cloned_heap_bytes);
    }
    #[test]
    fn advanced_owner_is_rejected_instead_of_skipped() {
        let mut source = sources().remove(0);
        source.prepayment_terms = Some(super::super::PrepaymentTerms {
            review: source.credit.identity.clone(),
            principal_premium_fraction: 0.,
        });
        assert!(finite_raw_layout(&source)
            .unwrap_err()
            .contains("advanced-owner"));
    }
}
