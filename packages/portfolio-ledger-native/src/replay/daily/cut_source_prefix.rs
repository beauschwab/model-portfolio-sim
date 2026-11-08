//! Private, injective comparison encoding. Public financial proofs stay named.
//! Exhaustive destructuring makes an added source field a compile-time change.
use portfolio_model_core::commercial_coupon_accounting::{AtomicGroup, Snapshot};
use portfolio_model_core::nonintegral_fee::{Event, Group, State};
use serde::{Serialize, Serializer};

pub(super) struct LoanGroup<'a>(pub &'a AtomicGroup);
struct LoanSnapshot<'a>(&'a Snapshot);

impl Serialize for LoanGroup<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let AtomicGroup {
            path_key,
            year,
            phase,
            policy,
            source_event_id,
            effective_interest,
            basis_realization,
            cash,
            postings,
            next,
        } = self.0;
        (
            path_key,
            year,
            phase,
            policy,
            source_event_id,
            effective_interest,
            basis_realization,
            cash,
            postings,
            LoanSnapshot(next),
        )
            .serialize(serializer)
    }
}

impl Serialize for LoanSnapshot<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Snapshot {
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
        } = self.0;
        (
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
        )
            .serialize(serializer)
    }
}

pub(super) struct FeeGroup<'a>(pub &'a Group);
struct FeeEvent<'a>(&'a Event);
struct FeeState<'a>(&'a State);

impl Serialize for FeeGroup<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Group {
            event,
            policy,
            postings,
            next,
        } = self.0;
        (FeeEvent(event), policy, postings, FeeState(next)).serialize(serializer)
    }
}

impl Serialize for FeeEvent<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Event {
            event_id,
            fee_id,
            source,
            review,
            ordinal,
            sequence,
            action,
            reason,
            amount,
        } = self.0;
        (
            event_id, fee_id, source, review, ordinal, sequence, action, reason, amount,
        )
            .serialize(serializer)
    }
}

impl Serialize for FeeState<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let State {
            acquired_fee_asset_valuation_adjustment,
            acquired_fee_liability_valuation_adjustment,
            transferred_net_stock,
            acquired_stock_selected,
            acquired_cost,
            released_acquired_cost,
            acquired_fee_asset,
            acquired_fee_liability,
            earned,
            refunded_earned,
            cancelled_unearned,
            waived_earned,
            recovered_waived,
            net_service_cash,
            fee_receivable,
            fee_payable,
            deferred_fee_income,
            prepaid_fee_expense,
            unearned_entitlement,
        } = self.0;
        // Include even zero-valued optional public fields. Fixed slots prevent
        // two different gross owners from collapsing to the same evidence.
        (
            acquired_stock_selected,
            [
                acquired_fee_asset_valuation_adjustment,
                acquired_fee_liability_valuation_adjustment,
                transferred_net_stock,
                acquired_cost,
                released_acquired_cost,
                acquired_fee_asset,
                acquired_fee_liability,
                earned,
                refunded_earned,
                cancelled_unearned,
                waived_earned,
                recovered_waived,
                net_service_cash,
                fee_receivable,
                fee_payable,
                deferred_fee_income,
                prepaid_fee_expense,
                unearned_entitlement,
            ],
        )
            .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fee_comparison_keeps_every_zero_gross_state_slot_distinct() {
        let original = State::default();
        let empty = serde_json::to_vec(&FeeState(&original)).unwrap();
        let mut encodings = std::collections::HashSet::new();
        macro_rules! changed {
            ($($field:ident),+ $(,)?) => { $(
                let mut s = original.clone(); s.$field = 1.;
                let encoded = serde_json::to_vec(&FeeState(&s)).unwrap();
                assert_ne!(empty, encoded);
                assert!(encodings.insert(encoded), stringify!($field));
            )+ };
        }
        changed!(
            acquired_fee_asset_valuation_adjustment,
            acquired_fee_liability_valuation_adjustment,
            transferred_net_stock,
            acquired_cost,
            released_acquired_cost,
            acquired_fee_asset,
            acquired_fee_liability,
            earned,
            refunded_earned,
            cancelled_unearned,
            waived_earned,
            recovered_waived,
            net_service_cash,
            fee_receivable,
            fee_payable,
            deferred_fee_income,
            prepaid_fee_expense,
            unearned_entitlement
        );
        let mut selected = original;
        selected.acquired_stock_selected = true;
        let encoded = serde_json::to_vec(&FeeState(&selected)).unwrap();
        assert_ne!(empty, encoded);
        assert!(encodings.insert(encoded));
        assert_eq!(encodings.len(), 19);
    }
}
