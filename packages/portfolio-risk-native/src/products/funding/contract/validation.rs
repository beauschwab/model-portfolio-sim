//! Date, inventory, source and bounded-work admission.
use super::*;

pub(super) fn coupon_metadata(
    p: &crate::products::term::contracts::indexed::ContractPeriod,
    bytes: &mut usize,
    total: &mut usize,
) -> Result<(), String> {
    use crate::products::term::contracts::indexed::{Coupon, RateSource};
    text(&p.period_id, bytes)?;
    if let Coupon::Indexed {
        index,
        calendar,
        observations,
        ..
    } = &p.coupon
    {
        text(&index.index_id, bytes)?;
        identity(calendar, bytes)?;
        work(observations.len(), total)?;
        for o in observations {
            match &o.source {
                RateSource::Observed { fixing, .. } => identity(fixing, bytes)?,
                RateSource::ConditionalProjection { model, path, .. } => {
                    identity(model, bytes)?;
                    identity(path, bytes)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn currency(c: &str, expected: &str) -> Result<(), String> {
    if c != expected {
        return Err("funding/collateral currency mismatch; FX conversion unsupported".into());
    }
    Ok(())
}
pub(super) fn work(n: usize, total: &mut usize) -> Result<(), String> {
    *total = total.checked_add(n).ok_or("funding work overflow")?;
    if *total > MAX_WORK {
        return Err("funding work exceeds 250000 items".into());
    }
    Ok(())
}
pub(super) fn snapshot(
    s: &CollateralSnapshot,
    date: i32,
    assets: &HashMap<&str, usize>,
    bytes: &mut usize,
    total: &mut usize,
) -> Result<(), String> {
    identity(&s.source, bytes)?;
    validate_ordinal(s.ordinal)?;
    work(s.values.len(), total)?;
    if s.ordinal != date || s.values.is_empty() || s.values.len() > MAX_ASSETS {
        return Err("collateral snapshot must be complete, bounded and exactly dated".into());
    }
    let mut ids = HashSet::new();
    for v in &s.values {
        text(&v.asset_id, bytes)?;
        nonnegative(v.market_price)?;
        if !assets.contains_key(v.asset_id.as_str())
            || !ids.insert(&v.asset_id)
            || !v.haircut.is_finite()
            || !(0.0..1.0).contains(&v.haircut)
        {
            return Err("unknown/duplicate collateral asset or unsupported haircut".into());
        }
    }
    Ok(())
}
pub(super) fn allocations(
    v: &[Allocation],
    assets: &HashMap<&str, usize>,
    signed: bool,
    bytes: &mut usize,
    total: &mut usize,
) -> Result<(), String> {
    work(v.len(), total)?;
    let mut ids = HashSet::new();
    for a in v {
        text(&a.asset_id, bytes)?;
        amount(a.quantity)?;
        if (!signed && a.quantity <= 0.)
            || a.quantity == 0.
            || !assets.contains_key(a.asset_id.as_str())
            || !ids.insert(&a.asset_id)
        {
            return Err("invalid/duplicate collateral allocation".into());
        }
    }
    Ok(())
}
pub(super) fn recoveries(
    v: &[DatedAmount],
    first: i32,
    last: i32,
    total: &mut usize,
) -> Result<f64, String> {
    if v.len() > 4096 {
        return Err("funding recovery tail exceeds 4096 points".into());
    }
    work(v.len(), total)?;
    let mut prior = None;
    let mut sum = 0.;
    for x in v {
        domain(first, x.ordinal)?;
        nonnegative(x.amount)?;
        if x.ordinal > last || prior.is_some_and(|d| d >= x.ordinal) {
            return Err("funding recovery tail must be complete, dated and ordered".into());
        }
        prior = Some(x.ordinal);
        sum = amount(sum + x.amount)?;
    }
    Ok(sum)
}
