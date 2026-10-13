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
fn cash(p: f64, i: f64) -> Result<f64, String> {
    money(-(p + i))
}
pub(super) fn weighted(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut total = 0.;
    for path in paths {
        let (mut sum, mut correction) = (0., 0.);
        for e in path
            .projection
            .cashflows
            .iter()
            .filter(|e| e.ordinal > asof)
        {
            let t = (e.ordinal - asof) as f64 / 365.;
            let discounted = money(
                cash(e.principal, e.cash_interest)?
                    * curve.value_validated(t)?.discount_factor
                    * (-(oas + bump) * t).exp(),
            )?;
            let adjusted = discounted - correction;
            let next = money(sum + adjusted)?;
            correction = (next - sum) - adjusted;
            sum = next;
        }
        total = money(total + path.weight * sum)?;
    }
    Ok(total)
}
fn log_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
) -> Result<f64, String> {
    let mut value: Option<f64> = None;
    for p in paths.iter().filter(|p| p.weight > 0.) {
        for e in p.projection.cashflows.iter().filter(|e| e.ordinal > asof) {
            let c = cash(e.principal, e.cash_interest)?;
            if c <= 0. {
                continue;
            }
            let t = (e.ordinal - asof) as f64 / 365.;
            let term =
                p.weight.ln() + c.ln() + curve.value_validated(t)?.discount_factor.ln() - oas * t;
            value = Some(match value {
                None => term,
                Some(old) => {
                    let hi = old.max(term);
                    hi + ((old - hi).exp() + (term - hi).exp()).ln()
                }
            });
        }
    }
    value.ok_or("deposit positive Q liability rights required".into())
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
            let c = cash(e.principal, e.cash_interest)?;
            if c < 0. {
                return Err("signed Q bank cash requires locked deposit OAS; no monotone liability calibration inferred".into());
            }
            positive |= p.weight > 0. && c > 0.;
        }
    }
    if !positive {
        return Err("deposit positive target requires future liability rights".into());
    }
    let (mut lo, mut hi) = (-5., 5.);
    let target = target_cash_price.ln();
    if log_price(paths, curve, asof, lo)? < target || log_price(paths, curve, asof, hi)? > target {
        return Err("deposit target outside admitted OAS bracket".into());
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
