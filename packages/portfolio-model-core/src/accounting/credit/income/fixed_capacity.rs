//! Exhaustive requested storage for ordinary, untransferred coupon carry.
//! Advanced draw/review/contingent owners require their own inventory.
use super::*;
use crate::models::credit::paths as path;
use std::mem::{needs_drop, size_of};

pub(crate) type R<T> = Result<T, String>;
pub(crate) fn sum(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("fixed carry storage overflow".into())
}
pub(crate) fn product(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("fixed carry storage overflow".into())
}
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Heap {
    pub retained: usize,
    pub cloned: usize,
}
impl Heap {
    pub fn add(self, b: Self) -> R<Self> {
        Ok(Self {
            retained: sum(self.retained, b.retained)?,
            cloned: sum(self.cloned, b.cloned)?,
        })
    }
}
pub(crate) trait Storage {
    fn heap(&self) -> R<Heap>;
}
impl Storage for String {
    fn heap(&self) -> R<Heap> {
        Ok(Heap {
            retained: self.capacity(),
            cloned: self.len(),
        })
    }
}
impl<T: Storage> Storage for Vec<T> {
    fn heap(&self) -> R<Heap> {
        let mut h = Heap {
            retained: product(self.capacity(), size_of::<T>())?,
            cloned: product(self.len(), size_of::<T>())?,
        };
        for value in self {
            h = h.add(value.heap()?)?;
        }
        Ok(h)
    }
}
impl<T: Storage, const N: usize> Storage for [T; N] {
    fn heap(&self) -> R<Heap> {
        self.iter()
            .try_fold(Heap::default(), |h, v| h.add(v.heap()?))
    }
}
impl<T: Storage> Storage for Option<T> {
    fn heap(&self) -> R<Heap> {
        self.as_ref().map_or(Ok(Heap::default()), Storage::heap)
    }
}
pub(crate) fn inline<T>(_: &T) -> R<()> {
    if needs_drop::<T>() {
        Err("fixed carry scalar acquired owned storage".into())
    } else {
        Ok(())
    }
}
macro_rules! fields {
    ($ty:path, [$($owned:ident),*], [$($scalar:ident),*]) => {
        impl Storage for $ty {
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
pub(crate) use fields;
impl Storage for usize {
    fn heap(&self) -> R<Heap> {
        Ok(Heap::default())
    }
}

fields!(path::RecoveryWeight, [], [lag_years, weight]);
fields!(
    path::PathInterval,
    [complete_recovery_tails],
    [
        end_year,
        hazards_per_year,
        accrual_rates,
        loss_given_default,
        credit_conversion_factor,
        interest_recovery_fraction
    ]
);
fields!(path::PathBoundary, [event_id, policy_id], [year, action]);
fields!(
    path::PathOpening,
    [path_id],
    [
        path_key,
        initial_live_state,
        principal,
        undrawn_commitment,
        accrued_interest,
        book_adjustment
    ]
);
fields!(
    path::UniformPair,
    [],
    [waiting_uniform, destination_uniform]
);
fields!(path::SuppliedPathDraws, [pairs], [path_key]);
impl Storage for path::DrawStream {
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
fields!(
    path::CreditPathRequest,
    [
        schema,
        monetary_unit,
        source_id,
        source_revision,
        model_policy_id,
        intervals,
        boundaries,
        draw_stream,
        paths
    ],
    [measure, origin_time, max_jumps_per_path]
);
fields!(
    OriginalAnchor,
    [original_yield_anchor],
    [path_key, acquisition_cost, original_annual_effective_rate]
);
fields!(
    RecognitionRate,
    [review],
    [interval_index, annual_effective_rates]
);
fields!(
    Request,
    [
        schema,
        raw_credit_path,
        original_anchors,
        recognition_rates,
        policy_review,
        partial_payment_policy,
        default_policy
    ],
    []
);
fields!(
    path::PathEvent,
    [event_id, policy_id, kind],
    [
        year,
        origin,
        destination,
        cash,
        principal,
        interest,
        book_adjustment_recognition
    ]
);
fields!(path::PathRecovery, [], [year, principal, interest]);
fields!(
    path::PathDefault,
    [recoveries],
    [
        year,
        origin,
        principal,
        accrued_interest,
        book_adjustment,
        contingent_draw,
        economic_principal_loss
    ]
);
fields!(
    path::CreditPathResult,
    [path_id, events, default],
    [
        path_key,
        outcome,
        closing_principal,
        closing_undrawn_commitment,
        closing_accrued_interest,
        closing_book_adjustment,
        earned_interest,
        earned_interest_by_origin,
        cash_received,
        cash_funded,
        book_adjustment_recognized,
        random_pairs_used
    ]
);
fields!(
    path::CreditPathsResult,
    [
        schema,
        monetary_unit,
        source_id,
        source_revision,
        model_policy_id,
        clock,
        stream_id,
        stream_revision,
        paths
    ],
    [measure, origin_time, work_units]
);
fields!(
    path::LiveSojourn,
    [],
    [
        path_key,
        start_year,
        end_year,
        origin,
        principal,
        undrawn_commitment,
        opening_accrued_interest,
        book_adjustment,
        accrual_rate,
        earned_interest
    ]
);
fields!(path::CreditPathTraceResult, [result, sojourns], []);
fields!(
    CanonicalCouponLegalBalance,
    [coupon_id],
    [
        native_audit_index,
        native_cash_index,
        opening_entitlement_principal,
        cumulative_factor,
        contractual_earned,
        actual_interest_collected,
        legal_remaining
    ]
);
fields!(
    DrawTrancheCarry,
    [tranche_id],
    [
        original_annual_effective_rate,
        nominal_draw,
        acquisition_cost,
        gross_carry
    ]
);
fields!(
    CarryPoint,
    [
        canonical_legal_coupons,
        draw_tranches,
        pending_coupon_obligation_indices
    ],
    [
        path_key,
        year,
        kind,
        origin_index,
        principal,
        contractual_accrued_interest,
        book_adjustment,
        gross_carry,
        accounting_rate,
        effective_interest,
        cash,
        basis_realization,
        undrawn_commitment
    ]
);
fields!(
    RecoveryReceipt,
    [],
    [
        path_key,
        default_year,
        recovery_index,
        year,
        principal,
        interest,
        applied_in_horizon
    ]
);
fields!(
    NativeCouponEntitlement,
    [],
    [
        native_audit_index,
        native_cash_index,
        opening_entitlement_principal
    ]
);
fields!(
    EarnedOriginContribution,
    [
        coupon_id,
        source_event_id,
        source_id,
        source_revision,
        native_entitlement
    ],
    [
        path_key,
        interval_index,
        origin_index,
        start_year,
        end_year,
        captured_segment_index,
        delta_factor,
        actual_principal,
        contractual_earned
    ]
);
fields!(
    NativeContractualCash,
    [coupon_id, source_event_id, tranche_id],
    [native_cash_index, principal, interest, fee]
);
fields!(SourceCashReceipt, [cash], [path_key, year]);
fields!(
    NativeCouponRecovery,
    [coupon_id, source_event_id],
    [
        path_key,
        default_year,
        recovery_index,
        year,
        native_audit_index,
        earning_origin_index,
        interest,
        applied_in_horizon
    ]
);

impl Request {
    /// Ordinary fixed-coupon construction, excluding the borrowed input and
    /// native factor provider. The caller must establish no transfers, stops,
    /// draws or pending fixings. Every possible jump and full recovery tail is
    /// retained. This estimate is not by itself joined source admission.
    pub fn fixed_construction_capacity(&self, periods: usize) -> R<usize> {
        if periods == 0 || periods > 4096 {
            return Err("fixed carry requires bounded complete coupon periods".into());
        }
        let raw = &self.raw_credit_path;
        let paths = raw.paths.len();
        let sojourns = sum(
            sum(raw.intervals.len(), raw.boundaries.len())?,
            sum(raw.max_jumps_per_path, 1)?,
        )?;
        let events = sum(raw.boundaries.len(), raw.max_jumps_per_path)?;
        let points = product(paths, sum(sojourns, sum(events, 68)?)?)?;
        let lots = product(periods, 3)?;
        let contributions = product(product(paths, sojourns)?, periods)?;
        let coupon_recoveries = product(product(paths, lots)?, 32)?;
        let source_cash = product(paths, periods)?;
        let recovery_rows = product(paths, 32)?;
        // All copied identities are validated at <=256 bytes. Vec slots
        // include old/new overlap; nested cloned snapshots request len.
        let canonical = product(periods, sum(size_of::<CanonicalCouponLegalBalance>(), 256)?)?;
        let point_storage = sum(vector::<CarryPoint>(points)?, product(points, canonical)?)?;
        let contribution_storage = sum(
            vector::<EarnedOriginContribution>(contributions)?,
            product(contributions, 4 * 256)?,
        )?;
        let cash_storage = sum(
            vector::<SourceCashReceipt>(source_cash)?,
            product(source_cash, 2 * 256)?,
        )?;
        let coupon_storage = sum(
            vector::<NativeCouponRecovery>(coupon_recoveries)?,
            product(coupon_recoveries, 2 * 256)?,
        )?;
        let path_event_storage = sum(
            vector::<path::PathEvent>(events)?,
            product(events, 3 * 256)?,
        )?;
        let path_storage = sum(
            vector::<path::CreditPathResult>(paths)?,
            product(
                paths,
                sum(
                    256,
                    sum(path_event_storage, vector::<path::PathRecovery>(32)?)?,
                )?,
            )?,
        )?;
        // Mutable canonical balances, factor atoms, default lots, cloned label
        // sets and credit validation tables coexist with retained result rows.
        let atoms = sum(
            vector::<(String, String, String, String, usize, f64)>(periods)?,
            product(periods, 4 * 256)?,
        )?;
        let default_lots = sum(
            vector::<crate::products::commercial::coupon_accounting::CouponLot>(lots)?,
            product(lots, 4 * 256)?,
        )?;
        let factor_query = sum(
            vector::<NativeCumulativeCouponFactor>(periods)?,
            product(periods, 256)?,
        )?;
        let mutable_canonical = sum(
            vector::<CanonicalCouponLegalBalance>(periods)?,
            product(periods, 256)?,
        )?;
        let label_tree = product(
            periods,
            sum(sum(product(11, size_of::<String>())?, 128)?, 256)?,
        )?;
        let validation = product(sum(sum(paths, raw.boundaries.len())?, 1)?, 4096)?;
        [
            size_of::<ResultData>(),
            size_of::<Owner<'_>>(),
            self.heap()?.cloned,
            10 * 256,
            point_storage,
            contribution_storage,
            cash_storage,
            coupon_storage,
            path_storage,
            vector::<path::LiveSojourn>(product(paths, sojourns)?)?,
            vector::<RecoveryReceipt>(recovery_rows)?,
            atoms,
            default_lots,
            factor_query,
            mutable_canonical,
            label_tree,
            validation,
        ]
        .into_iter()
        .try_fold(0, sum)
    }

    /// Exact retained and requested-clone heap of the prepared core input.
    /// Inline storage is already inside its parent's typed header.
    pub fn fixed_heap_capacity(&self) -> R<(usize, usize)> {
        let h = self.heap()?;
        Ok((h.retained, h.cloned))
    }
}
/// Pinned Vec doubling, including prior allocation and minimum capacity.
/// Nested String and vector payloads are charged separately by the caller.
pub(crate) fn vector<T>(count: usize) -> R<usize> {
    if count == 0 {
        return Ok(0);
    }
    product(sum(product(count, 3)?, 8)?, size_of::<T>())
}
impl ResultData {
    /// Includes every private result field and nested owned vector/string.
    /// This is a post-construction check, not raw admission or an RSS bound.
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            raw_request,
            admission,
            credit,
            carry_points,
            complete_recovery_receipts,
            native_factor_provider_selected,
            earned_origin_contributions,
            native_source_cash,
            native_coupon_recoveries,
            complete_coupon_settlement_ownership,
            original_anchor_refitted,
            default_interest_stopped,
            corporate_source_binding,
            mixed_ledger_adoption,
            accounting_approval,
            tail_census,
        } = self;
        let Admission {
            raw_bytes,
            row_upper,
            work_upper,
            retained_bytes_upper,
        } = admission;
        inline(&(
            schema,
            *raw_bytes,
            *row_upper,
            *work_upper,
            *retained_bytes_upper,
            *native_factor_provider_selected,
            *complete_coupon_settlement_ownership,
            *original_anchor_refitted,
            *default_interest_stopped,
            *corporate_source_binding,
            *mixed_ledger_adoption,
            *accounting_approval,
        ))?;
        if tail_census.is_some() {
            return Err("selected carry tail requires separate capacity inventory".into());
        }
        let heap = raw_request
            .heap()?
            .add(credit.heap()?)?
            .add(carry_points.heap()?)?
            .add(complete_recovery_receipts.heap()?)?
            .add(earned_origin_contributions.heap()?)?
            .add(native_source_cash.heap()?)?
            .add(native_coupon_recoveries.heap()?)?;
        sum(size_of::<Self>(), heap.retained)
    }
}
