use super::{admission::money, KnownProduct, NativeSource};
use crate::{
    advanced_graph::{OasTarget, Role},
    model_market::curves::SuppliedCurve,
};
use portfolio_model_core::floating_rate::Direction;
/// Graph2-only bounded lookup, preserving original path order and price math.
pub(super) struct SourceIndex<'a>(
    std::collections::BTreeMap<(&'a str, &'a str, u8), Vec<&'a NativeSource>>,
);
fn role_key(r: Role) -> u8 {
    match r {
        Role::Physical => 0,
        Role::Pricing => 1,
        Role::PricingDown => 2,
        Role::PricingUp => 3,
    }
}
impl<'a> SourceIndex<'a> {
    pub(super) fn new(paths: &'a [NativeSource]) -> Result<Self, String> {
        let mut map = std::collections::BTreeMap::new();
        for p in paths {
            portfolio_compute_control::checkpoint()?;
            map.entry((
                p.position_id.as_str(),
                p.scenario_id.as_str(),
                role_key(p.role),
            ))
            .or_insert_with(Vec::new)
            .push(p);
        }
        Ok(Self(map))
    }
    pub(super) fn slice(
        &self,
        position: &str,
        scenario: &str,
        role: Role,
    ) -> Vec<&'a NativeSource> {
        self.0
            .get(&(position, scenario, role_key(role)))
            .cloned()
            .unwrap_or_default()
    }
}
pub(super) fn direction(p: &KnownProduct) -> f64 {
    let d = match p {
        KnownProduct::IndexedTerm { contract } => contract.direction,
        KnownProduct::CdElection { contract } => contract.direction,
        KnownProduct::CdSequence { contract } => contract.direction,
    };
    if d == Direction::Receipt {
        1.
    } else {
        -1.
    }
}
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
pub(crate) fn price(
    p: &NativeSource,
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut sum = 0.;
    let mut correction = 0.;
    for e in p.projection.events().filter(|e| e.payment_ordinal > asof) {
        let c = &e.components;
        let cash = money(c.principal + c.cash_interest + c.fees)?;
        let t = (e.payment_ordinal - asof) as f64 / 365.;
        let value =
            money(cash * curve.value_validated(t)?.discount_factor * (-(oas + bump) * t).exp())?;
        let adjusted = value - correction;
        let next = money(sum + adjusted)?;
        correction = (next - sum) - adjusted;
        sum = next;
    }
    Ok(sum)
}
pub(super) fn weighted_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    bump: f64,
) -> Result<f64, String> {
    let mut sum = 0.;
    for p in paths {
        sum = money(sum + p.weight * price(p, curve, asof, oas, bump)?)?;
    }
    Ok(sum)
}
fn log_price(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    oas: f64,
    d: f64,
) -> Result<f64, String> {
    let mut sum: Option<f64> = None;
    for p in paths.iter().filter(|p| p.weight > 0.) {
        for e in p.projection.events().filter(|e| e.payment_ordinal > asof) {
            let c = &e.components;
            let cash = money(d * (c.principal + c.cash_interest + c.fees))?;
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
    sum.ok_or("no positive directional term calibration rights".into())
}
pub(super) fn solve(
    paths: &[&NativeSource],
    curve: &SuppliedCurve,
    asof: i32,
    target: &OasTarget,
    d: f64,
) -> Result<f64, String> {
    #[cfg(test)]
    super::evidence::SOLVES.with(|n| n.set(n.get() + 1));
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
        for e in p.projection.events().filter(|e| e.payment_ordinal > asof) {
            let c = &e.components;
            let cash = money(d * (c.principal + c.cash_interest + c.fees))?;
            if cash < 0. {
                return Err(
                    "signed directional term/renewal Q cash requires explicit locked OAS".into(),
                );
            }
            positive |= p.weight > 0. && cash > 0.;
        }
    }
    if !positive {
        return Err("term target needs positive directional future Q rights".into());
    }
    let (mut lo, mut hi) = (-5., 5.);
    let target = target_cash_price.ln();
    if log_price(paths, curve, asof, lo, d)? < target
        || log_price(paths, curve, asof, hi, d)? > target
    {
        return Err("term target outside admitted OAS bracket".into());
    }
    for _ in 0..100 {
        portfolio_compute_control::checkpoint()?;
        let mid = (lo + hi) / 2.;
        if log_price(paths, curve, asof, mid, d)? > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo + hi) / 2.)
}
