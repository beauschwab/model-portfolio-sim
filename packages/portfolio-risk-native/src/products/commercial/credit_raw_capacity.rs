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

// All fields must be named. Adding a DTO field is a compile error here; changing
// an inline field to an owning type rejects until its storage is inventoried.
macro_rules! fields {
    ($ty:path, [$($owned:ident),*], [$($scalar:ident),*]) => {
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
