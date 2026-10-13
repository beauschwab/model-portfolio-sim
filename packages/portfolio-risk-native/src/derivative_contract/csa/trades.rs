//! Clean valuation and explicit European-rights handoff; no duplicate option/swap exposure.
use super::*;
pub(super) fn position(t: &Trade) -> &str {
    match t {
        Trade::Swap { request } => &request.position_id,
        Trade::EuropeanSwaption { request } => &request.position_id,
        Trade::ExpiredEuropean { terminal, .. } => &terminal.position_id,
    }
}
pub(super) fn contractual(t: &Trade) -> &Identity {
    match t {
        Trade::Swap { request } => &request.contract,
        Trade::EuropeanSwaption { request } => &request.contract,
        Trade::ExpiredEuropean { terminal, .. } => &terminal.contract,
    }
}
fn option_key(r: &options::SwaptionRequest) -> Result<String, String> {
    serde_json::to_string(&(
        &r.contract,
        &r.exercise_policy,
        &r.currency,
        r.expiry_ordinal,
        r.settlement_ordinal,
        r.tail_end_ordinal,
        r.side,
        r.settlement,
        r.states
            .first()
            .map(|s| swaps::contract_key(&s.underlying))
            .transpose()?,
    ))
    .map_err(|e| e.to_string())
}
pub(super) fn trade_key(t: &Trade) -> Result<String, String> {
    match t {
        Trade::Swap { request } => swaps::contract_key(request),
        Trade::EuropeanSwaption { request } => option_key(request),
        Trade::ExpiredEuropean { terminal, .. } => option_key(terminal),
    }
}
pub(super) fn trade_work(t: &Trade) -> usize {
    match t {
        Trade::Swap { request } => swaps::work(request),
        Trade::EuropeanSwaption { request } => request
            .states
            .iter()
            .fold(0usize, |w, s| w.saturating_add(swaps::work(&s.underlying))),
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => terminal
            .states
            .iter()
            .fold(0usize, |w, s| w.saturating_add(swaps::work(&s.underlying)))
            .saturating_add(delivered_underlying.as_ref().map_or(0, |u| swaps::work(u))),
    }
}
fn swap_value(r: &swaps::SwapRequest, n: &Node, currency: &str, path: &str) -> Result<f64, String> {
    if r.asof_ordinal != n.ordinal
        || r.valuation_cutoff_unix_seconds != n.valuation_cutoff_unix_seconds
        || r.payment_timing != n.payment_timing
        || r.currency != currency
        || r.scenario_id != path
    {
        return Err("CSA raw swap date/time/coupon order/currency/path mismatch".into());
    }
    Ok(swaps::value_swap(r)?.clean_value)
}
pub(super) fn priced(
    t: &Trade,
    n: &Node,
    currency: &str,
    path: &str,
    bucket: &str,
) -> Result<f64, String> {
    match t {
        Trade::Swap { request } => swap_value(request, n, currency, path),
        Trade::EuropeanSwaption { request: r } => {
            if r.asof_ordinal != n.ordinal
                || r.currency != currency
                || r.scenario_id != path
                || n.ordinal >= r.expiry_ordinal
            {
                return Err(
                    "CSA live European nodes require matching path/currency and pre-expiry anchor"
                        .into(),
                );
            }
            for s in &r.states {
                if s.netting_set_id != bucket {
                    return Err("option delivery legal netting mapping mismatch".into());
                }
            }
            Ok(options::value_european(r)?.clean_value)
        }
        Trade::ExpiredEuropean {
            terminal: r,
            election,
            delivered_underlying,
            settlement_discount,
        } => {
            let mut bytes = 0;
            identity(election, &mut bytes)?;
            if n.ordinal < r.expiry_ordinal
                || r.asof_ordinal != r.expiry_ordinal
                || r.states.len() != 1
                || r.states[0].probability != 1.
                || r.states[0].discount_to_expiry != 1.
                || r.currency != currency
                || r.scenario_id != path
                || r.states[0].netting_set_id != bucket
            {
                return Err("expiry handoff requires one complete supplied expiry state with exact legal mapping".into());
            }
            let result = options::value_european(r)?;
            let state = &result.states[0];
            if r.settlement == options::Settlement::CollateralizedCashPrice {
                if delivered_underlying.is_some() {
                    return Err("cash-settled option cannot deliver an underlying".into());
                }
                if swaps::active(r.settlement_ordinal, n.ordinal, n.payment_timing) {
                    let curve=settlement_discount.as_ref().ok_or("outstanding option settlement needs exact current collateral discount curve")?;
                    curve.validate_at(n.ordinal, currency, n.valuation_cutoff_unix_seconds)?;
                    let sign = if r.side == options::OptionSide::Long {
                        1.
                    } else {
                        -1.
                    };
                    amount(sign * state.settlement_cash * curve.at(r.settlement_ordinal)?)
                } else {
                    Ok(0.)
                }
            } else {
                if settlement_discount.is_some() {
                    return Err("physical option cannot add a cash payoff discount route".into());
                }
                if !state.exercised {
                    if delivered_underlying.is_some() {
                        return Err("unexercised option cannot deliver a swap".into());
                    }
                    return Ok(0.);
                }
                let delivered = delivered_underlying
                    .as_ref()
                    .ok_or("exercised physical option requires mapped raw future swap")?;
                let mut expected = r.states[0].underlying.clone();
                if r.side == options::OptionSide::Short {
                    expected.side = if expected.side == swaps::Side::Payer {
                        swaps::Side::Receiver
                    } else {
                        swaps::Side::Payer
                    };
                }
                if delivered.position_id != r.states[0].delivered_position_id
                    || swaps::contract_key(delivered)? != swaps::contract_key(&expected)?
                {
                    return Err(
                        "physical delivery changed underlying economics or mapped identity".into(),
                    );
                }
                swap_value(delivered, n, currency, path)
            }
        }
    }
}
pub(super) fn complete_tail(t: &Trade, tail: i32) -> Result<(), String> {
    let last = match t {
        Trade::Swap { request } => request
            .fixed_periods
            .iter()
            .map(|p| p.payment_ordinal)
            .chain(request.floating_periods.iter().map(|p| p.payment_ordinal))
            .max()
            .unwrap_or(tail),
        Trade::EuropeanSwaption { request } => request.tail_end_ordinal,
        Trade::ExpiredEuropean { terminal, .. } => terminal.tail_end_ordinal,
    };
    if last > tail {
        return Err("CSA tail must retain complete trade maturity and settlement rights".into());
    }
    Ok(())
}
pub(super) fn no_interim_payment(t: &Trade, start: i32, end: i32) -> Result<(), String> {
    let pays = match t {
        Trade::Swap { request: r } => r
            .fixed_periods
            .iter()
            .map(|p| p.payment_ordinal)
            .chain(r.floating_periods.iter().map(|p| p.payment_ordinal))
            .collect::<Vec<_>>(),
        Trade::EuropeanSwaption { request: r } => vec![r.expiry_ordinal, r.settlement_ordinal],
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => {
            let mut dates = vec![terminal.expiry_ordinal, terminal.settlement_ordinal];
            if let Some(u) = delivered_underlying {
                dates.extend(u.fixed_periods.iter().map(|p| p.payment_ordinal));
                dates.extend(u.floating_periods.iter().map(|p| p.payment_ordinal));
            }
            dates
        }
    };
    if pays.iter().any(|d| *d > start && *d < end) {
        return Err("closeout interval containing a trade payment/exercise requires supplied unpaid obligations; unsupported in this version".into());
    }
    Ok(())
}
