use super::*;
use crate::market::analysis::curves::CurveRole;
use portfolio_model_core::{
    cashflow::validate_ordinal, consumer_card::CardAction, limits::MAX_MONETARY_AMOUNT,
};
use serde::Serialize;
use std::{collections::HashSet, io::Write};

fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err("card graph identity requires bounded nonblank text".into());
    }
    Ok(())
}
pub(super) fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        return Err("card graph numeric admission exceeded".into());
    }
    Ok(())
}
pub(super) fn money(v: f64) -> Result<f64, String> {
    number(v, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
    Ok(v)
}
struct ByteCount {
    size: usize,
    limit: usize,
}
impl Write for ByteCount {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(data.len());
        if self.size > self.limit {
            return Err(std::io::Error::other("card graph byte admission exceeded"));
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut out = ByteCount { size: 0, limit };
    serde_json::to_writer(&mut out, v).map_err(|e| e.to_string())?;
    Ok(out.size)
}
fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    count(v, MAX_IDENTITY_BYTES)?;
    serde_json::to_vec(v).map_err(|e| e.to_string())
}
fn paths(v: &[History]) -> Result<(), String> {
    if v.is_empty() || v.len() > 128 {
        return Err("card graph path admission exceeded".into());
    }
    let mut ids = HashSet::new();
    let mut sum = 0.;
    for p in v {
        label(&p.path_id)?;
        label(&p.assumption_id)?;
        number(p.weight, 0., 1.)?;
        if !ids.insert(&p.path_id) || p.steps.len() > 4096 {
            return Err("duplicate card path or step admission exceeded".into());
        }
        sum += p.weight;
    }
    if (sum - 1.).abs() > 1e-12 {
        return Err("card history probabilities must sum to one; no normalization fallback".into());
    }
    Ok(())
}
pub(super) fn validate_with_work(r: &Request) -> Result<(usize, usize), String> {
    if r.schema != SCHEMA || r.identity.randomness_domain != "supplied_paired_histories-1" {
        return Err("card graph requires explicit version and supplied paired histories".into());
    }
    for v in [
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.base_scenario_id,
    ] {
        label(v)?;
    }
    validate_ordinal(r.as_of_ordinal)?;
    number(r.bump_bp, 0.001, 1000.)?;
    if r.currency.len() != 3 || !r.currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err("card graph requires uppercase currency".into());
    }
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("card reporting boundaries required")?;
    if !(1..=120).contains(&h)
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
    {
        return Err("card graph dimensions exceed admission".into());
    }
    for &date in &r.reporting_ordinals {
        validate_ordinal(date)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("card reporting dates must start at asof and increase".into());
    }
    let mut ids = HashSet::new();
    let mut run_ids = HashSet::new();
    for s in &r.scenarios {
        label(&s.scenario_id)?;
        label(&s.run_id)?;
        number(s.severity, 0., 100.)?;
        if !ids.insert(&s.scenario_id)
            || !run_ids.insert(&s.run_id)
            || s.collateral_curve.currency != r.currency
            || !matches!(s.collateral_curve.role, CurveRole::Collateral { .. })
        {
            return Err("card scenario/curve/currency mismatch".into());
        }
        s.collateral_curve.validate()?;
    }
    let base = r
        .scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing card base scenario")?;
    let mut positions = HashSet::new();
    let mut runs = 0usize;
    let mut rows = 0usize;
    let mut work = 0usize;
    for p in &r.positions {
        label(&p.position_id)?;
        if !positions.insert(&p.position_id) {
            return Err("duplicate card position".into());
        }
        let c = &p.contract;
        if c.horizon_days > 36600 {
            return Err("card horizon admission exceeded".into());
        }
        let end = r
            .as_of_ordinal
            .checked_add(c.horizon_days as i32)
            .ok_or("card horizon overflow")?;
        validate_ordinal(end)?;
        if c.schema != "consumer-card-1"
            || c.identity.model_version != "consumer-card-1"
            || c.identity.position_id != p.position_id
            || c.identity.currency != r.currency
            || c.identity.measure != CashflowMeasure::Contractual
            || c.as_of_ordinal != r.as_of_ordinal
            || !c.steps.is_empty()
            || p.histories.len() != r.scenarios.len()
            || r.reporting_ordinals[h] > end
        {
            return Err("card known contract/measure/date/history coverage mismatch".into());
        }
        let mut reference = None;
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            if set.scenario_id != s.scenario_id {
                return Err("card histories must match explicit scenario order".into());
            }
            for (role, history) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                paths(history)?;
                runs = runs.checked_add(history.len()).ok_or("card run overflow")?;
                for path in history {
                    if path.steps.len() + c.cycles.len() > 4096
                        || path.steps.windows(2).any(|s| s[0].ordinal > s[1].ordinal)
                        || path
                            .steps
                            .iter()
                            .any(|s| s.ordinal < r.as_of_ordinal || s.ordinal > end)
                    {
                        return Err("card steps/cycles/date admission exceeded".into());
                    }
                    // Core stores at most one lot per opening class and draw/fee action.
                    // Charge every lot/day, every possible recognition row, statement and
                    // action allocation/sort BEFORE any financial projection allocates.
                    let lots = 3 + path
                        .steps
                        .iter()
                        .filter(|s| {
                            matches!(s.action, CardAction::Draw { .. } | CardAction::Fee { .. })
                        })
                        .count();
                    let days = c.horizon_days as usize + 1;
                    let daily = days
                        .checked_mul(lots + c.cycles.len())
                        .ok_or("card daily overflow")?;
                    if daily > 250_000 {
                        return Err("card daily lot/cycle work admission exceeded".into());
                    }
                    let cash = path.steps.len() + 1;
                    let count =
                        days + days * lots + c.cycles.len() * lots + c.cycles.len() + 2 * cash + 1;
                    rows = rows.checked_add(count).ok_or("card rows overflow")?;
                    let sort_passes = 6 + (usize::BITS - lots.leading_zeros()) as usize;
                    let actions = path.steps.len() * lots * sort_passes;
                    let passes = if role == Role::Physical {
                        h + 3
                    } else if role == Role::Pricing {
                        104
                    } else {
                        4
                    };
                    let reporting = if role == Role::Physical {
                        count * passes
                    } else {
                        count + cash * passes
                    };
                    work = work
                        .checked_add(daily + actions + reporting)
                        .ok_or("card work overflow")?;
                }
            }
            for history in [&set.pricing, &set.pricing_down, &set.pricing_up] {
                let pairing: Vec<_> = history.iter().map(|p| (&p.path_id, p.weight)).collect();
                if let Some(ref known) = reference {
                    if known != &pairing {
                        return Err("card Q histories require identical ordered path identities/weights across scenarios/bumps".into());
                    }
                } else {
                    reference = Some(pairing);
                }
            }
        }
        match &p.oas_target {
            OasTarget::Locked {
                base_oas,
                source_id,
                source_revision,
            } => {
                number(*base_oas, -5., 5.)?;
                label(source_id)?;
                label(source_revision)?;
            }
            OasTarget::CalibratePositiveCash {
                target_cash_price,
                source_id,
                source_revision,
            } => {
                number(*target_cash_price, 1e-12, MAX_MONETARY_AMOUNT)?;
                label(source_id)?;
                label(source_revision)?;
            }
        }
    }
    if runs > MAX_RUNS || rows > MAX_ROWS || work > MAX_WORK {
        return Err("card graph aggregate run/row/work admission exceeded".into());
    }
    count(r, MAX_IDENTITY_BYTES)?;
    for position in &r.positions {
        for (scenario, set) in r.scenarios.iter().zip(&position.histories) {
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for path in paths {
                    let mut raw = position.contract.clone();
                    raw.identity.measure = if role == Role::Physical {
                        CashflowMeasure::Physical
                    } else {
                        CashflowMeasure::Pricing
                    };
                    raw.identity.assumption_id = path.assumption_id.clone();
                    raw.identity.run_id = scenario.run_id.clone();
                    raw.steps = path.steps.clone();
                    consumer_card::validate_request(&raw)?;
                }
            }
        }
    }
    Ok((base, work))
}
pub(super) fn validate(r: &Request) -> Result<usize, String> {
    validate_with_work(r).map(|(base, _)| base)
}
pub(super) fn base_key(r: &Request, base: usize) -> Result<Vec<u8>, String> {
    let positions: Vec<_> = r
        .positions
        .iter()
        .map(|p| {
            (
                &p.position_id,
                &p.oas_target,
                &p.contract,
                &p.histories[base].pricing,
            )
        })
        .collect();
    bytes(&(
        &r.schema,
        &r.identity.model_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        r.as_of_ordinal,
        &r.currency,
        &r.base_scenario_id,
        r.terminal_policy,
        r.cash_timing,
        &r.scenarios[base],
        positions,
    ))
}
pub(super) fn retained(r: &Request, result: &ResultData, key: usize) -> Result<(), String> {
    let input = count(r, MAX_IDENTITY_BYTES)?;
    let output = count(result, MAX_RETAINED_BYTES)?;
    if input
        .saturating_add(output)
        .saturating_mul(4)
        .saturating_add(key)
        > MAX_RETAINED_BYTES
    {
        return Err("card graph retained byte admission exceeded".into());
    }
    Ok(())
}
