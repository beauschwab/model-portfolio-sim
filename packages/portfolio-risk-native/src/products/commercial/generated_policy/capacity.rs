//! Structural retained recipe capacity for the explicitly generated tier.
//! These recipes contain raw typed input, never full financial carry snapshots.
use super::{add, mul, owner, strict_json};
use crate::products::{
    commercial::{acquisition::Transfer, source_income},
    term::contracts::indexed::{ContractPathRequest, Coupon, PrincipalPlan},
};
use std::mem::{size_of, size_of_val};

fn vector<T>(v: &[T]) -> Result<usize, String> {
    // Generated recipes use cloned DTO vectors. Reserve twice their exact
    // element count for Vec capacity and allocator size-class rounding.
    mul(mul(v.len(), size_of::<T>())?, 2)
}
fn contract(s: &ContractPathRequest) -> Result<usize, String> {
    let mut bytes = vector(&s.periods)?;
    if let PrincipalPlan::Supplied { principal_payments } = &s.principal_plan {
        bytes = add(bytes, vector(principal_payments)?)?;
    }
    for period in &s.periods {
        if let Coupon::Indexed {
            calendar,
            observations,
            ..
        } = &period.coupon
        {
            bytes = add(bytes, mul(size_of_val(&**calendar), 2)?)?;
            bytes = add(bytes, vector(observations)?)?;
            // Native generation replaces future RateSource model identities.
            // Both model/path identities are admitted bounded metadata. This
            // also covers generated original-source copying across P roles.
            bytes = add(bytes, mul(observations.len(), 2048)?)?;
        }
    }
    Ok(bytes)
}
const RUNTIME_HEADER_BYTES: usize = 8192;
fn transfer_pointer_bytes() -> usize {
    size_of::<Option<Box<Transfer>>>()
}
fn runtime_header_upper(draws: usize) -> Result<usize, String> {
    if draws > 32 {
        return Err("performing draw tranche bound32 exceeded".into());
    }
    // One main slot plus the twice-capacity draw Vec slots. These slots exist
    // even when transfer is absent; a selected Box body is charged separately.
    let transfer_slots = add(1, mul(2, draws)?)?;
    let pointers = add(
        size_of::<
            Option<Box<crate::products::term::contracts::indexed::work::GeneratedCatalogAuthority>>,
        >(),
        mul(transfer_slots, transfer_pointer_bytes())?,
    )?;
    // Exact inline stop, <=256-byte event id, four <=128-byte review strings,
    // six replaced global identity strings, <=13-byte generated path label,
    // String over-allocation and one allocator/header cell. Per-observation
    // model/path growth has its separate 2048-byte reserve in contract().
    add(
        add(pointers, size_of::<owner::NativeStop>())?,
        2 * (256 + 4 * 128 + 6 * 128 + 13) + 64,
    )
}
fn transfer_allocations(source: &source_income::Request) -> Result<usize, String> {
    let Some(transfer) = &source.transfer else {
        return Ok(0);
    };
    // Twice the concrete Box body includes its inline origin and policy DTOs
    // and allocator rounding. All String bytes are already in twice-encoded
    // raw input. Nested Vec/Box allocations are independent of that encoding.
    add(
        mul(size_of::<Transfer>(), 2)?,
        add(
            contract(&transfer.contract_origin)?,
            vector(&transfer.coupon_allocations)?,
        )?,
    )
}
pub(super) fn raw_recipe_upper(raw: &owner::Request) -> Result<usize, String> {
    // Keep the historical inline base; both new runtime slots are explicitly
    // reserved in the existing header cell below, not silently left uncharged.
    if runtime_header_upper(raw.performing_draw_tranches.len())? > RUNTIME_HEADER_BYTES {
        return Err("generated recipe runtime header exceeds its retained cell".into());
    }
    // Encoding contains every owned string (including escaped content) and
    // keys. Twice this length reserves cloned String capacities/allocations;
    // all inline structs, Vec elements and Box allocations are counted below.
    let historical_inline = size_of::<owner::Request>()
        .checked_sub(size_of::<
            Option<Box<crate::products::term::contracts::indexed::work::GeneratedCatalogAuthority>>,
        >())
        .and_then(|n| n.checked_sub(transfer_pointer_bytes()))
        .ok_or("generated recipe inline layout differs")?;
    let mut bytes = add(
        historical_inline,
        mul(strict_json::serialized_size(raw, 1 << 20)?, 2)?,
    )?;
    bytes = add(
        bytes,
        raw.counted_work.as_ref().map_or(0, |a| a.retained_bytes()),
    )?;
    bytes = add(bytes, contract(&raw.source.original)?)?;
    bytes = add(bytes, contract(&raw.source.current)?)?;
    bytes = add(bytes, vector(&raw.source.current_accounting_rate_reviews)?)?;
    bytes = add(bytes, vector(&raw.credit.intervals)?)?;
    for interval in &raw.credit.intervals {
        for tail in &interval.complete_recovery_tails {
            bytes = add(bytes, vector(tail)?)?;
        }
    }
    bytes = add(bytes, vector(&raw.credit.paths)?)?;
    if let portfolio_model_core::credit_path::DrawStream::Supplied { paths, .. } =
        &raw.credit.draw_stream
    {
        bytes = add(bytes, vector(paths)?)?;
        for path in paths {
            bytes = add(bytes, vector(&path.pairs)?)?;
        }
    }
    bytes = add(bytes, vector(&raw.accounting.recognition)?)?;
    bytes = add(bytes, vector(&raw.accounting_rates)?)?;
    let historical_draw_element = size_of::<owner::PerformingDrawTranche>()
        .checked_sub(transfer_pointer_bytes())
        .ok_or("generated draw inline layout differs")?;
    bytes = add(
        bytes,
        mul(
            mul(raw.performing_draw_tranches.len(), historical_draw_element)?,
            2,
        )?,
    )?;
    bytes = add(bytes, transfer_allocations(&raw.source)?)?;
    for draw in &raw.performing_draw_tranches {
        bytes = add(bytes, transfer_allocations(&draw.source)?)?;
        bytes = add(bytes, contract(&draw.source.original)?)?;
        bytes = add(bytes, contract(&draw.source.current)?)?;
        bytes = add(bytes, vector(&draw.source.current_accounting_rate_reviews)?)?;
    }
    if let Some(actions) = &raw.reviewed_credit_actions {
        bytes = add(bytes, vector(&actions.allowance_reviews)?)?;
        bytes = add(bytes, vector(&actions.chargeoffs)?)?;
        bytes = add(bytes, vector(&actions.legal_waivers)?)?;
    }

    // One retained cell covers the proven runtime slots/labels/stop above.
    // Selected authority and transfer bodies retain separate allocations.
    add(bytes, RUNTIME_HEADER_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_runtime_header_covers_both_optional_slots_and_maximum_draw_vector() {
        let empty = runtime_header_upper(0).unwrap();
        let full = runtime_header_upper(32).unwrap();
        assert_eq!(full - empty, 2 * 32 * transfer_pointer_bytes());
        assert!(full <= RUNTIME_HEADER_BYTES);
        assert!(runtime_header_upper(33).is_err());
    }
    #[test]
    fn capacity_transfer_slot_layout_keeps_historical_source_inline_base() {
        type HistoricalSource = (
            String,
            ContractPathRequest,
            ContractPathRequest,
            source_income::Acquisition,
            source_income::Policy,
            Vec<source_income::ProspectiveRateReview>,
        );
        assert_eq!(
            size_of::<source_income::Request>(),
            size_of::<HistoricalSource>() + transfer_pointer_bytes()
        );
        type HistoricalDraw = (
            crate::term_enhancements::Identity,
            i32,
            [bool; 3],
            HistoricalSource,
        );
        assert_eq!(
            size_of::<owner::PerformingDrawTranche>(),
            size_of::<HistoricalDraw>() + transfer_pointer_bytes()
        );
    }
    #[test]
    fn capacity_selected_transfer_body_and_complete_origin_allocations_are_additional() {
        let raw: owner::Request = serde_json::from_slice(include_bytes!(
            "../../../../../../examples/models/commercial-secondary-acquisition-1.json"
        ))
        .unwrap();
        let transfer = raw.source.transfer.as_ref().unwrap();
        let allocations = transfer_allocations(&raw.source).unwrap();
        assert!(
            allocations
                >= 2 * size_of::<Transfer>()
                    + 2 * transfer.coupon_allocations.len()
                        * size_of::<crate::products::commercial::acquisition::CouponAllocation>()
        );
        let mut absent = raw.clone();
        absent.source.transfer = None;
        assert!(
            raw_recipe_upper(&raw).unwrap() >= raw_recipe_upper(&absent).unwrap() + allocations
        );
        assert_eq!(transfer_allocations(&absent.source).unwrap(), 0);
        let mut expanded = raw.clone();
        let t = expanded.source.transfer.as_mut().unwrap();
        t.coupon_allocations.push(t.coupon_allocations[0].clone());
        t.contract_origin
            .periods
            .push(t.contract_origin.periods[0].clone());
        assert!(transfer_allocations(&expanded.source).unwrap() > allocations);
    }
}
