//! Exclusive owned-inventory transport and dated collateral valuation.
use super::*;

pub(super) fn credited(
    runtime: &Runtime,
    s: &CollateralSnapshot,
    registry: &[CollateralAsset],
    asset_ids: &HashMap<&str, usize>,
) -> Result<(f64, f64), String> {
    let agreement = runtime
        .contract
        .collateral
        .as_ref()
        .ok_or("collateral agreement missing")?;
    let values: HashMap<usize, &CollateralValue> = s
        .values
        .iter()
        .map(|v| (asset_ids[v.asset_id.as_str()], v))
        .collect();
    let mut raw = 0.;
    let mut credit = 0.;
    let mut held: Vec<(usize, f64)> = runtime.held.iter().map(|(&idx, &q)| (idx, q)).collect();
    held.sort_by_key(|(idx, _)| *idx);
    for (idx, q) in held {
        if q == 0. {
            continue;
        }
        let mark = values
            .get(&idx)
            .ok_or("snapshot omits held collateral asset")?;
        let value = amount(q * mark.market_price)?;
        raw = amount(raw + value)?;
        if mark.eligible
            && agreement
                .eligible_asset_classes
                .contains(&registry[idx].asset_class)
        {
            credit = amount(credit + value * (1. - mark.haircut))?;
        }
    }
    Ok((raw, credit))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn transfer(
    runtime: &mut Runtime,
    a: &Allocation,
    date: i32,
    source: &Identity,
    disposition: &str,
    snapshot: &CollateralSnapshot,
    registry: &[CollateralAsset],
    asset_ids: &HashMap<&str, usize>,
    remaining: &[f64],
    locked: &mut [f64],
    movements: &mut Vec<CollateralMovement>,
) -> Result<(), String> {
    let idx = asset_ids[a.asset_id.as_str()];
    let asset = &registry[idx];
    let contract = &runtime.contract;
    let owner = if contract.direction == FundingDirection::Borrowing {
        &contract.entity_id
    } else {
        &contract.counterparty_id
    };
    if asset.ownership != Ownership::Owned || &asset.owner_entity_id != owner {
        return Err(
            "collateral must be owned by cash borrower; received collateral reuse rejected".into(),
        );
    }
    let prior = *runtime.held.get(&idx).unwrap_or(&0.);
    let mut q = a.quantity;
    if q > 0. {
        let mark = snapshot
            .values
            .iter()
            .find(|v| v.asset_id == a.asset_id)
            .ok_or("delivery asset has no dated collateral mark")?;
        let agreement = contract.collateral.as_ref().ok_or("agreement missing")?;
        currency(&asset.currency, &contract.currency)?;
        if !mark.eligible
            || !agreement
                .eligible_asset_classes
                .contains(&asset.asset_class)
        {
            return Err(
                "delivered collateral is ineligible under explicit agreement/snapshot".into(),
            );
        }
        let free = remaining[idx] - asset.already_encumbered_quantity - locked[idx];
        if q > free && !near(q, free) {
            return Err("collateral quantity already encumbered by incompatible claim".into());
        }
        q = q.min(free.max(0.));
    } else {
        if -q > prior && !near(-q, prior) {
            return Err("collateral return exceeds this contract's pledge".into());
        }
        q = q.max(-prior);
    }
    runtime.held.insert(idx, prior + q);
    locked[idx] += q;
    if locked[idx] < 0. && near(locked[idx], 0.) {
        locked[idx] = 0.;
    }
    movements.push(CollateralMovement {
        position_id: contract.position_id.clone(),
        ordinal: date,
        asset_id: a.asset_id.clone(),
        owner_entity_id: asset.owner_entity_id.clone(),
        quantity_delta: q,
        disposition: disposition.into(),
        source: source.clone(),
    });
    Ok(())
}
