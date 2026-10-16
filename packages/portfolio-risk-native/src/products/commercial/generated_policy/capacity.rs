//! Structural retained recipe capacity for the explicitly generated tier.
//! These recipes contain raw typed input, never full financial carry snapshots.
use super::{add, mul, owner, strict_json};
use crate::products::term::contracts::indexed::{ContractPathRequest, Coupon, PrincipalPlan};
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
pub(super) fn raw_recipe_upper(raw: &owner::Request) -> Result<usize, String> {
    // Encoding contains every owned string (including escaped content) and
    // keys. Twice this length reserves cloned String capacities/allocations;
    // all inline structs, Vec elements and Box allocations are counted below.
    let mut bytes = add(
        size_of::<owner::Request>()
            - size_of::<
                Option<
                    Box<crate::products::term::contracts::indexed::work::GeneratedCatalogAuthority>,
                >,
            >(),
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
    bytes = add(bytes, vector(&raw.performing_draw_tranches)?)?;
    for draw in &raw.performing_draw_tranches {
        bytes = add(bytes, contract(&draw.source.original)?)?;
        bytes = add(bytes, contract(&draw.source.current)?)?;
        bytes = add(bytes, vector(&draw.source.current_accounting_rate_reviews)?)?;
    }
    if let Some(actions) = &raw.reviewed_credit_actions {
        bytes = add(bytes, vector(&actions.allowance_reviews)?)?;
        bytes = add(bytes, vector(&actions.chargeoffs)?)?;
        bytes = add(bytes, vector(&actions.legal_waivers)?)?;
    }

    // Generated path labels, randomness-domain identities and the one sealed
    // native stop have separate bounded capacity beyond the raw template.
    add(bytes, 8192)
}
