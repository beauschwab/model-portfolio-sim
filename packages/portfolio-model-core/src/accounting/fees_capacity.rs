//! Complete ordinary fee ownership. Transferred rights need a separate census.
use super::*;
use crate::accounting::credit::income::fixed_capacity::{
    fields, inline, product, sum, vector, Heap, Storage, R,
};
use std::mem::size_of;

fields!(
    Fee,
    [
        fee_id,
        source,
        classification_review,
        service,
        position_id,
        tranche_id
    ],
    [entitlement, prepaid_refunds_allowed, earned_refunds_allowed]
);
fields!(
    Event,
    [event_id, fee_id, source, review],
    [ordinal, sequence, action, reason, amount]
);
fields!(
    Request,
    [schema, currency, monetary_unit, policy, fees, events],
    [
        measure,
        asof_ordinal,
        reporting_end_ordinal,
        complete_end_ordinal
    ]
);
fields!(Posting, [], [account, amount]);
fields!(
    State,
    [],
    [
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
        unearned_entitlement
    ]
);
fields!(
    Closing,
    [fee_id, source, position_id, tranche_id, state],
    []
);
fields!(Group, [event, policy, postings, next], []);

/// Actual owned capacity and the requested payload of one Clone, excluding inline storage.
#[derive(Debug, Clone, Copy)]
pub struct RawCapacity {
    pub retained_heap_bytes: usize,
    pub cloned_heap_bytes: usize,
}
fn raw<T: Storage>(v: &T) -> R<RawCapacity> {
    let h = v.heap()?;
    Ok(RawCapacity {
        retained_heap_bytes: h.retained,
        cloned_heap_bytes: h.cloned,
    })
}
pub fn fee_raw(v: &Fee) -> R<RawCapacity> {
    raw(v)
}
pub fn event_raw(v: &Event) -> R<RawCapacity> {
    raw(v)
}
pub fn request_raw(v: &Request) -> R<RawCapacity> {
    raw(v)
}

impl ResultData {
    /// Complete retained allocation, including spare capacity and both closing snapshots.
    pub fn fixed_retained_capacity(&self) -> R<usize> {
        let Self {
            schema,
            raw_request,
            native_transfer,
            groups,
            reporting,
            closing,
            full_source_tail_replayed,
            loan_carrying_value_changed,
            native_credit_event_binding,
            mixed_ledger_adoption,
            pricing_unit_ftp_adoption,
            empirical_validation,
            accounting_approval,
        } = self;
        inline(schema)?;
        inline(full_source_tail_replayed)?;
        inline(loan_carrying_value_changed)?;
        inline(native_credit_event_binding)?;
        inline(mixed_ledger_adoption)?;
        inline(pricing_unit_ftp_adoption)?;
        inline(empirical_validation)?;
        inline(accounting_approval)?;
        if native_transfer.is_some() {
            return Err("fixed fee inventory requires separate transferred-right ownership".into());
        }
        let h = raw_request
            .heap()?
            .add(groups.heap()?)?
            .add(reporting.heap()?)?
            .add(closing.heap()?)?;
        sum(size_of::<Self>(), h.retained)
    }
}

/// Prospective complete construction for a validated ordinary fee request.
/// Native-generated labels also pass the same 128-byte identity validation.
/// Three-slot growth plus eight minimum slots includes old/new Vec overlap.
pub fn construction(fees: usize, events: usize) -> R<usize> {
    if fees == 0 || fees > MAX_FEES || events > MAX_EVENTS {
        return Err("fixed fee construction dimensions exceed existing source limits".into());
    }
    // Request: nine owned labels per fee, six per event and <=1024 fixed text.
    let request = [
        size_of::<Request>(),
        1024,
        vector::<Fee>(fees)?,
        product(fees, 9 * 128)?,
        vector::<Event>(events)?,
        product(events, 6 * 128)?,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    // Ten postings is the maximum: two action accounts plus eight gross-right deltas.
    let groups = [
        vector::<Group>(events)?,
        product(events, 8 * 128)?,
        product(events, vector::<Posting>(10)?)?,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    let snapshot = sum(vector::<Closing>(fees)?, product(fees, 5 * 128)?)?;
    let owner = [
        size_of::<ResultData>(),
        request,
        groups,
        product(snapshot, 2)?,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    // Complete result plus raw clone, original/request overlap, states/reporting,
    // borrowed-key validation maps and sort/scan workspace. No cash-value pruning.
    // The final complete JSON size check temporarily owns a growing byte Vec.
    // Reserve all keys, scalar spellings and worst-case escaped bounded labels
    // in raw input, groups and both snapshots, including old/new growth.
    let json = [product(events, 16384)?, product(fees, 32768)?, 16384]
        .into_iter()
        .try_fold(0, sum)?;
    [
        owner,
        product(request, 2)?,
        product(vector::<State>(fees)?, 2)?,
        product(json, 3)?,
        product(sum(fees, events)?, 4096)?,
    ]
    .into_iter()
    .try_fold(0, sum)
}

const _: () = {
    assert!(size_of::<Fee>() <= 8192);
    assert!(size_of::<Event>() <= 8192);
    assert!(size_of::<Request>() <= 8192);
    assert!(size_of::<ResultData>() <= 8192);
    assert!(size_of::<Group>() <= 8192);
    assert!(size_of::<Closing>() <= 8192);
};

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Request {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/models/nonintegral-fee-accounting-1.json"
        )))
        .unwrap()
    }
    #[test]
    fn complete_fee_owner_and_escaped_output_fit_prospective_bound() {
        let mut r = sample();
        // The source admits quoted and non-ASCII labels; preserve all identity links.
        r.policy.id = "quoted-\"\\-é".repeat(5);
        for f in &mut r.fees {
            f.classification_review.id = r.policy.id.clone();
        }
        let v = evaluate(&r).unwrap();
        let bound = construction(r.fees.len(), r.events.len()).unwrap();
        assert!(v.fixed_retained_capacity().unwrap() < bound);
        assert!(serde_json::to_vec(&v).unwrap().len() * 3 < bound);
        assert!(!v.accounting_approval && !v.empirical_validation);
    }
    #[test]
    fn spare_input_and_output_storage_is_counted_without_changing_clone_payload() {
        let mut r = sample();
        let before = request_raw(&r).unwrap();
        r.fees.reserve(40);
        r.events.reserve(70);
        r.fees[0].fee_id.reserve(200);
        let after = request_raw(&r).unwrap();
        assert!(after.retained_heap_bytes > before.retained_heap_bytes);
        assert_eq!(after.cloned_heap_bytes, before.cloned_heap_bytes);
        let mut v = evaluate(&r).unwrap();
        let old = v.fixed_retained_capacity().unwrap();
        v.groups.reserve(40);
        v.reporting.reserve(20);
        v.closing[0].fee_id.reserve(200);
        assert!(v.fixed_retained_capacity().unwrap() > old);
    }
    #[test]
    fn ordinary_dimensions_refuse_out_of_domain_before_construction() {
        assert!(construction(0, 1).is_err());
        assert!(construction(MAX_FEES + 1, 1).is_err());
        assert!(construction(1, MAX_EVENTS + 1).is_err());
        assert!(construction(1, 2).unwrap() > construction(1, 1).unwrap());
    }
}
