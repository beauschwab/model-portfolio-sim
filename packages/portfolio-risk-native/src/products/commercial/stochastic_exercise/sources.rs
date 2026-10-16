//! Canonical source inventory for generated Pricing and Physical draws.
use super::*;
use crate::products::term::contracts::indexed::{ContractPathRequest, ContractPeriod};

pub(super) fn contracts(
    raw: &owner::Request,
) -> impl Iterator<Item = (Option<&str>, &ContractPathRequest)> {
    std::iter::once((None, &raw.source.current)).chain(
        raw.performing_draw_tranches
            .iter()
            .map(|d| (Some(d.review.id.as_str()), &d.source.current)),
    )
}
pub(super) fn loading_contract<'a>(
    raw: &'a owner::Request,
    loading: &CouponLoading,
) -> Result<&'a ContractPathRequest, String> {
    match &loading.draw_tranche_id {
        None => Ok(&raw.source.current),
        Some(id) => raw
            .performing_draw_tranches
            .iter()
            .find(|d| d.review.id == *id)
            .map(|d| &d.source.current)
            .ok_or_else(|| "generated coupon draw tranche absent".into()),
    }
}
pub(super) fn loading_period<'a>(
    raw: &'a owner::Request,
    loading: &CouponLoading,
) -> Result<&'a ContractPeriod, String> {
    loading_contract(raw, loading)?
        .periods
        .iter()
        .find(|p| p.period_id == loading.period_id)
        .ok_or_else(|| "generated coupon loading period absent".into())
}
pub(super) fn loading_period_mut<'a>(
    raw: &'a mut owner::Request,
    loading: &CouponLoading,
) -> Result<&'a mut ContractPeriod, String> {
    let contract = match &loading.draw_tranche_id {
        None => &mut raw.source.current,
        Some(id) => {
            &mut raw
                .performing_draw_tranches
                .iter_mut()
                .find(|d| d.review.id == *id)
                .ok_or("generated coupon draw tranche absent")?
                .source
                .current
        }
    };
    contract
        .periods
        .iter_mut()
        .find(|p| p.period_id == loading.period_id)
        .ok_or_else(|| "generated coupon loading period absent".into())
}
pub(crate) fn feature_bound(r: &Request) -> Result<usize, String> {
    let mut features = 2 * r.drivers.len() + 4;
    for (tranche, contract) in contracts(&r.source) {
        if tranche.is_some() {
            features = features
                .checked_add(2)
                .ok_or("generated feature count overflow")?;
        }
        for period in &contract.periods {
            let observations = match &period.coupon {
                Coupon::Indexed { observations, .. } => observations.len(),
                _ => 0,
            };
            features = features
                .checked_add(observations + 2)
                .ok_or("generated feature count overflow")?;
        }
    }
    Ok(features)
}
pub(super) fn append_observed_features(
    raw: &owner::Request,
    point: &portfolio_model_core::credit_path_income::CarryPoint,
    date: &term::exercise::ExerciseDate,
    opening: f64,
    features: &mut Vec<f64>,
) -> Result<(), String> {
    // Keep the legacy main-coupon ordering byte-for-byte when no draws exist.
    // Inventory shape is contractual and equal across Q/P; latent path recipe
    // identities, future activation and future reviewed actions are never keys.
    for (tranche, contract) in contracts(raw) {
        check()?;
        let acquired =
            tranche.and_then(|id| point.draw_tranches.iter().find(|d| d.tranche_id == id));
        let observed = tranche.is_none() || acquired.is_some();
        if tranche.is_some() {
            features.extend([
                if observed { 1. } else { 0. },
                acquired.map_or(0., |d| d.nominal_draw / opening),
            ]);
        }
        for period in &contract.periods {
            if let Coupon::Indexed { observations, .. } = &period.coupon {
                for observation in observations {
                    features.push(
                        if observed
                            && observation.fixing_ordinal <= date.ordinal
                            && observation.selection_cutoff_unix_seconds <= utc(date.ordinal)
                        {
                            observation.rate
                        } else {
                            0.
                        },
                    );
                }
            }
        }
        for period in &contract.periods {
            let coupon = point.canonical_legal_coupons.iter().find(|p| {
                observed
                    && p.coupon_id == period.period_id
                    && exercise::coupon_entitlement_observed(period, date.ordinal, date.timing)
            });
            features.extend(coupon.map_or([0., 0.], |p| {
                [
                    p.opening_entitlement_principal / opening,
                    p.legal_remaining / opening,
                ]
            }));
        }
    }
    Ok(())
}

/// Complete extracted economic cash inventory, including supplied recovery tails.
/// Four residual events cover terminal/default transfer, maturity/exit and the
/// generic conversion funding event. No hazard value removes a legal cash tail.
pub(crate) fn cash_event_bound(
    raw: &crate::commercial_credit_compiler::Request,
) -> Result<usize, String> {
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or_else(|| "native cash inventory overflow".to_string())
    };
    let mut n = raw.source.current.periods.len();
    let mut segments = 0usize;
    for contract in std::iter::once(&raw.source.current).chain(
        raw.performing_draw_tranches
            .iter()
            .map(|d| &d.source.current),
    ) {
        for period in &contract.periods {
            segments = add(
                segments,
                match &period.coupon {
                    Coupon::Fixed { .. } => 1,
                    Coupon::Indexed { observations, .. } => observations.len(),
                },
            )?;
        }
    }
    for draw in &raw.performing_draw_tranches {
        n = add(n, add(draw.source.current.periods.len(), 1)?)?;
    }
    let tail = raw
        .credit
        .intervals
        .iter()
        .flat_map(|p| p.complete_recovery_tails.iter())
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    n = add(n, add(tail, 4)?)?;
    if raw.factor_policy.terminal_obligations.is_some() {
        n = add(
            n,
            segments
                .checked_mul(tail + 1)
                .ok_or("native contingent cash inventory overflow")?,
        )?;
    }
    Ok(n)
}
