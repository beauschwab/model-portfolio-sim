use super::{admission::money, NativeSource, Request};
use crate::{
    advanced_graph::{OasTarget, Role},
    funding_contract::FundingDirection,
    model_market::curves::SuppliedCurve,
};
pub(super) fn direction(r: &Request) -> f64 {
    if r.known
        .contracts
        .iter()
        .all(|c| c.direction == FundingDirection::Lending)
    {
        1.
    } else if r
        .known
        .contracts
        .iter()
        .all(|c| c.direction == FundingDirection::Borrowing)
    {
        -1.
    } else {
        0.
    }
}
pub(super) fn slice<'a>(
    paths: &'a [NativeSource],
    scenario: &str,
    role: Role,
) -> Vec<&'a NativeSource> {
    paths
        .iter()
        .filter(|p| p.scenario_id == scenario && p.role == role)
        .collect()
}
fn price(
    p: &NativeSource,
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    let mut correction = 0.;
    for e in p
        .projection
        .schedule
        .events
        .iter()
        .filter(|e| e.payment_ordinal > asof)
    {
        let years = (e.payment_ordinal - asof) as f64 / 365.;
        let c = &e.components;
        let cash = money(c.principal + c.cash_interest + c.fees)?;
        let discounted = money(
            cash * curve.value_validated(years)?.discount_factor * (-(oas + bump) * years).exp(),
        )?;
        let adjusted = discounted - correction;
        let next = money(total + adjusted)?;
        correction = (next - total) - adjusted;
        total = next;
    }
    Ok(total)
}
pub(super) fn weighted_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    for p in paths {
        total = money(total + p.weight * price(p, curve, asof, oas, bump)?)?;
    }
    Ok(total)
}
fn log_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    direction: f64,
) -> Result<f64, String> {
    let mut sum: Option<f64> = None;
    for p in paths.iter().filter(|p| p.weight > 0.) {
        for e in p
            .projection
            .schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal > asof)
        {
            let c = &e.components;
            let cash = money(direction * (c.principal + c.cash_interest + c.fees))?;
            if cash <= 0. {
                continue;
            }
            let t = (e.payment_ordinal - asof) as f64 / 365.;
            let term = p.weight.ln() + cash.ln() + curve.value_validated(t)?.discount_factor.ln()
                - oas * t;
            sum = Some(match sum {
                None => term,
                Some(old) => {
                    let top = old.max(term);
                    top + ((old - top).exp() + (term - top).exp()).ln()
                }
            });
        }
    }
    sum.ok_or("no directional future funding Q cash".into())
}
pub(super) fn solve(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    target: &OasTarget,
    direction: f64,
) -> Result<f64, String> {
    if let OasTarget::Locked { base_oas, .. } = target {
        return Ok(*base_oas);
    }
    let OasTarget::CalibratePositiveCash {
        target_cash_price, ..
    } = target
    else {
        unreachable!()
    };
    if direction == 0. {
        return Err("mixed lending/borrowing funding book requires explicit locked OAS".into());
    }
    let mut positive = false;
    for p in paths {
        for e in p
            .projection
            .schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal > asof)
        {
            let c = &e.components;
            let cash = money(direction * (c.principal + c.cash_interest + c.fees))?;
            if cash < 0. {
                return Err(
                    "signed directional Q cash requires explicit locked funding OAS".into(),
                );
            }
            positive |= p.weight > 0. && cash > 0.;
        }
    }
    if !positive {
        return Err("funding target needs positive directional future Q cash".into());
    }
    let (mut lo, mut hi) = (-5., 5.);
    let target = target_cash_price.ln();
    if log_price(paths, curve, asof, lo, direction)? < target
        || log_price(paths, curve, asof, hi, direction)? > target
    {
        return Err("funding cash target outside admitted OAS bracket".into());
    }
    for _ in 0..100 {
        portfolio_compute_control::checkpoint()?;
        let mid = (lo + hi) / 2.;
        if log_price(paths, curve, asof, mid, direction)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
