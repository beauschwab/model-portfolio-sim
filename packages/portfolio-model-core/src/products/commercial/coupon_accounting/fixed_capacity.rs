//! Complete ordinary coupon-book storage; reviewed claims require their own
//! inventory. No serialized size is used to infer skipped vector capacity.
use super::{
    AtomicGroup, CouponLot, OwnedPosting, Policy, RecognitionReview, ResultData, Snapshot,
};
use crate::accounting::credit::expected::{Identity, PostingIntent};
use crate::accounting::credit::income::fixed_capacity::{
    fields, inline, product, sum, vector, Heap, Storage, R,
};
use std::mem::size_of;

fields!(Identity, [id, revision], []);
fields!(RecognitionReview, [review], [interval_index, by_origin]);
fields!(
    Policy,
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
    CouponLot,
    [coupon_id, source_event_id, source_id, source_revision],
    [
        native_audit_index,
        native_cash_index,
        earning_origin_index,
        contractual_earned,
        legal_cash_collected,
        legal_remaining,
        recognized_earned,
        recognized_cash_applied,
        recognized_remaining,
        default_claim
    ]
);
fields!(PostingIntent, [], [account, amount]);
fields!(OwnedPosting, [coupon_id, posting], [earning_origin_index]);
fields!(
    AtomicGroup,
    [policy, source_event_id, postings, next],
    [
        path_key,
        year,
        phase,
        effective_interest,
        basis_realization,
        cash
    ]
);
impl Storage for Snapshot {
    fn heap(&self) -> R<Heap> {
        let Self {
            path_key,
            year,
            principal,
            recognized_interest,
            legal_interest,
            native_legal_reduction_difference,
            book_adjustment,
            gross_carry,
            in_default,
            reviewed_credit_state,
            lots,
            pending_coupon_obligation_indices,
            canonical_legal_coupons,
        } = self;
        inline(&(
            *path_key,
            *year,
            *principal,
            *recognized_interest,
            *legal_interest,
            *native_legal_reduction_difference,
            *book_adjustment,
            *gross_carry,
            *in_default,
        ))?;
        if reviewed_credit_state.is_some() {
            return Err("reviewed credit snapshot requires separate capacity inventory".into());
        }
        lots.heap()?
            .add(pending_coupon_obligation_indices.heap()?)?
            .add(canonical_legal_coupons.heap()?)
    }
}
impl Policy {
    /// Ordinary book construction excluding borrowed carry: <=points groups,
    /// <=paths closing snapshots and <=3*periods lots. Each group has <=2
    /// postings per lot plus seven principal/basis/cash/income postings.
    /// The caller must reject reviewed claims and stopped/transferred sources.
    pub fn fixed_construction_capacity(
        &self,
        points: usize,
        paths: usize,
        periods: usize,
    ) -> R<usize> {
        if periods == 0 || periods > 4096 || points < paths {
            return Err("invalid complete fixed coupon-book dimensions".into());
        }
        let lots = product(periods, 3)?;
        let canonical = product(
            periods,
            sum(
                size_of::<crate::accounting::credit::income::CanonicalCouponLegalBalance>(),
                256,
            )?,
        )?;
        let snapshot = sum(
            product(lots, sum(size_of::<CouponLot>(), 4 * 256)?)?,
            canonical,
        )?;
        let lines = sum(product(lots, 2)?, 7)?;
        let postings = sum(vector::<OwnedPosting>(lines)?, product(lines, 256)?)?;
        let group_payload = sum(snapshot, sum(postings, 3 * 256)?)?;
        // Previous/next/post-clone, the independent mutable lots vector and
        // FIFO scratch coexist. Per-path coverage stores only borrowed labels.
        let mutable_lots = sum(vector::<CouponLot>(lots)?, product(lots, 4 * 256)?)?;
        let coverage = product(
            paths,
            sum(
                product(lots, sum(product(11, size_of::<(&str, usize)>())?, 128)?)?,
                sum(
                    product(
                        11,
                        size_of::<(u64, std::collections::BTreeSet<(&str, usize)>)>(),
                    )?,
                    128,
                )?,
            )?,
        )?;
        [
            size_of::<ResultData>(),
            self.heap()?.cloned,
            vector::<AtomicGroup>(points)?,
            product(points, group_payload)?,
            vector::<Snapshot>(paths)?,
            product(paths, snapshot)?,
            product(3, sum(size_of::<Snapshot>(), snapshot)?)?,
            mutable_lots,
            vector::<usize>(lots)?,
            coverage,
        ]
        .into_iter()
        .try_fold(0, sum)
    }

    pub fn fixed_heap_capacity(&self) -> R<(usize, usize)> {
        let h = self.heap()?;
        Ok((h.retained, h.cloned))
    }
}
impl ResultData {
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            policy,
            groups,
            closing,
            complete_native_recovery_tail_retained,
            claims_fully_closed,
            actual_risk_source_binding,
            mixed_ledger_adoption,
            accounting_approval,
        } = self;
        inline(&(
            *schema,
            *complete_native_recovery_tail_retained,
            *claims_fully_closed,
            *actual_risk_source_binding,
            *mixed_ledger_adoption,
            *accounting_approval,
        ))?;
        let h = policy.heap()?.add(groups.heap()?)?.add(closing.heap()?)?;
        sum(size_of::<Self>(), h.retained)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unused_snapshot_vectors_keep_their_allocated_slots() {
        let mut s = Snapshot {
            path_key: 1,
            year: 0.,
            principal: 0.,
            recognized_interest: 0.,
            legal_interest: 0.,
            native_legal_reduction_difference: 0.,
            book_adjustment: 0.,
            gross_carry: 0.,
            in_default: false,
            reviewed_credit_state: None,
            lots: Vec::new(),
            pending_coupon_obligation_indices: Vec::new(),
            canonical_legal_coupons: Vec::new(),
        };
        s.lots.reserve_exact(4);
        s.pending_coupon_obligation_indices.reserve_exact(3);
        s.canonical_legal_coupons.reserve_exact(2);
        let h = s.heap().unwrap();
        assert_eq!(h.cloned, 0);
        assert_eq!(
            h.retained,
            s.lots.capacity() * size_of::<CouponLot>()
                + s.pending_coupon_obligation_indices.capacity() * size_of::<usize>()
                + s.canonical_legal_coupons.capacity()
                    * size_of::<crate::accounting::credit::income::CanonicalCouponLegalBalance>()
        );
    }
}
