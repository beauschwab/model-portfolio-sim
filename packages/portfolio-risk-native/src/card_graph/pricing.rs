use super::{admission::money, NativeSource};
use crate::{
    advanced_graph::{OasTarget, Role},
    model_market::curves::SuppliedCurve,
};
pub(super) fn slice<'a>(
    paths: &'a [NativeSource],
    position: &str,
    scenario: &str,
    role: Role,
) -> Vec<&'a NativeSource> {
    paths
        .iter()
        .filter(|p| p.position_id == position && p.scenario_id == scenario && p.role == role)
        .collect()
}
fn price(
    path: &NativeSource,
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    let mut correction = 0.;
    for event in path
        .projection
        .cashflows
        .iter()
        .filter(|e| e.ordinal > asof)
    {
        let years = (event.ordinal - asof) as f64 / 365.;
        let cash = money(event.principal + event.interest + event.fees)?;
        let discounted = money(
            cash * curve.value_validated(years)?.discount_factor * (-(oas + bump) * years).exp(),
        )?;
        let adjusted = discounted - correction;
        let next = money(total + adjusted)?;
        correction = (next - total) - adjusted;
        total = next;
    }
    money(total)
}
pub(super) fn weighted_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    for path in paths {
        total = money(total + path.weight * price(path, curve, asof, oas, bump)?)?;
    }
    Ok(total)
}
fn log_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
) -> Result<f64, String> {
    let mut total: Option<f64> = None;
    for path in paths.iter().filter(|p| p.weight > 0.) {
        for e in path
            .projection
            .cashflows
            .iter()
            .filter(|e| e.ordinal > asof)
        {
            let cash = money(e.principal + e.interest + e.fees)?;
            if cash <= 0. {
                continue;
            }
            let t = (e.ordinal - asof) as f64 / 365.;
            let term =
                path.weight.ln() + cash.ln() + curve.value_validated(t)?.discount_factor.ln()
                    - oas * t;
            total = Some(match total {
                None => term,
                Some(old) => {
                    let high = old.max(term);
                    high + ((old - high).exp() + (term - high).exp()).ln()
                }
            });
        }
    }
    total.ok_or("card calibration requires positive future Q rights".into())
}
pub(super) fn solve(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    target: &OasTarget,
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
    let mut positive = false;
    for p in paths {
        for e in p.projection.cashflows.iter().filter(|e| e.ordinal > asof) {
            let cash = money(e.principal + e.interest + e.fees)?;
            if cash < 0. {
                return Err("signed Q cash requires explicit locked OAS; positive target solver unsupported".into());
            }
            positive |= p.weight > 0. && cash > 0.;
        }
    }
    if !positive {
        return Err("card positive target solver needs positive future Q rights".into());
    }
    let (mut lo, mut hi) = (-5., 5.);
    let target = target_cash_price.ln();
    if log_price(paths, curve, asof, lo)? < target || log_price(paths, curve, asof, hi)? > target {
        return Err("card target outside admitted OAS bracket".into());
    }
    for _ in 0..100 {
        portfolio_compute_control::checkpoint()?;
        let mid = (lo + hi) / 2.;
        if log_price(paths, curve, asof, mid)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
