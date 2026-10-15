//! Structural upper for every mapped opening/intent/closing and complete mixed-day scan.
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("derivative replay admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("derivative replay admission overflow".into())
}
use portfolio_compute_control::checkpoint;
use portfolio_risk_native::{
    derivative_accounting::DerivativeAccountingRequest,
    derivative_contract::{csa::Trade, options::SwaptionRequest, swaps::SwapRequest, Operation},
};
use std::collections::BTreeSet;
fn swap_slots(r: &SwapRequest) -> Result<usize, String> {
    add(r.fixed_periods.len(), r.floating_periods.len())
}
fn option_slots(r: &SwaptionRequest) -> Result<usize, String> {
    r.states.iter().try_fold(add(1, r.states.len())?, |n, s| {
        add(n, swap_slots(&s.underlying)?)
    })
}
fn cash_slots(t: &Trade) -> Result<usize, String> {
    checkpoint()?;
    match t {
        Trade::Swap { request } => swap_slots(request),
        Trade::EuropeanSwaption { request } => option_slots(request),
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => add(
            option_slots(terminal)?,
            delivered_underlying
                .as_ref()
                .map_or(Ok(0), |u| swap_slots(u))?,
        ),
    }
}
pub(crate) fn replay_upper(
    r: &DerivativeAccountingRequest,
    mixed_days: usize,
) -> Result<usize, String> {
    let Operation::Netting { request: csa } = &r.derivative.operation else {
        return Err("joint raw netting source required".into());
    };
    let path = csa.paths.first().ok_or("joint Physical path missing")?;
    let buckets = csa
        .mapping
        .iter()
        .map(|m| {
            if m.enforceable_netting {
                (true, m.netting_set_id.as_str())
            } else {
                (false, m.position_id.as_str())
            }
        })
        .collect::<BTreeSet<_>>()
        .len();
    let mut trades = 0;
    let mut rights = 0;
    for node in &path.nodes {
        checkpoint()?;
        trades = add(trades, node.trades.len())?;
        for t in &node.trades {
            rights = add(rights, cash_slots(t)?)?;
        }
    }
    let t = r.opening.trades.len();
    // Opening7+2/trade. Each node/trade: two three-line marks +four-line delivery.
    // Whole path: <=nodes*buckets calls, each settles once (including catchup),
    // so VM/custody<=6*N*B, interest<=7*N*B, tail<=6*B, total<=20*N*B.
    // Default closeout6 financial+2 custody/bucket+2/trade; each potential right:
    // due<=3, MPOR settlement<=2, default unpaid retirement<=1 (all charged).
    let lines = add(
        add(7, mul(t, 2)?)?,
        add(
            mul(trades, 10)?,
            add(
                mul(mul(path.nodes.len(), buckets)?, 20)?,
                add(mul(buckets, 8)?, add(mul(t, 2)?, mul(rights, 6)?)?)?,
            )?,
        )?,
    )?;
    // All16 AccountCode cells can survive; no netting or zero-value reduction of work.
    // Source/intents validation visits lines once. Existing owner visits all historical
    // gross keys through EVERY mixed day, including after shorter derivative tails.
    add(
        mul(lines, add(1, mul(mixed_days, 6)?)?)?,
        mul(path.nodes.len(), 16)?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_replay_factor_charges_long_tail_and_checked_overflow() {
        let raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/joint-deposit-derivative.json"
        ))
        .unwrap();
        let r: DerivativeAccountingRequest = serde_json::from_value(
            raw["derivative_owners"][0]["runs"][0]["saved"]["physical"].clone(),
        )
        .unwrap();
        let short = replay_upper(&r, 41).unwrap();
        let long = replay_upper(&r, 1081).unwrap();
        assert!(long > short * 20);
        assert!(replay_upper(&r, usize::MAX).is_err());
    }
}
