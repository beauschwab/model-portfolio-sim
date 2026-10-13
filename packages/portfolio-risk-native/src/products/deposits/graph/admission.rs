use super::*;
use crate::market::analysis::curves::CurveRole;
use portfolio_model_core::{
    cashflow::{validate_ordinal, CashflowMeasure},
    deposit_state::TerminalPolicy,
    limits::MAX_MONETARY_AMOUNT,
};
use serde::Serialize;
use std::{collections::HashSet, io::Write};
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        Err("deposit graph bounded identity required".into())
    } else {
        Ok(())
    }
}
pub(super) fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        Err("deposit graph numeric admission exceeded".into())
    } else {
        Ok(())
    }
}
pub(super) fn money(v: f64) -> Result<f64, String> {
    number(v, -MAX_MONETARY_AMOUNT, MAX_MONETARY_AMOUNT)?;
    Ok(v)
}
struct Counter {
    size: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
        self.size = self.size.saturating_add(b.len());
        if self.size > self.limit {
            return Err(std::io::Error::other(
                "deposit graph byte admission exceeded",
            ));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut c = Counter { size: 0, limit };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.size)
}
fn opening(h: &History) -> Result<Vec<u8>, String> {
    let r = &h.request;
    let segments: Vec<_> = r
        .segments
        .iter()
        .map(|s| {
            (
                &s.segment_id,
                s.opening_balance,
                s.opening_accrued_interest,
                s.opening_paid_rate,
                &s.characteristics,
                s.maturity_ordinal,
            )
        })
        .collect();
    serde_json::to_vec(&(
        &r.schema,
        &r.identity.entity_id,
        &r.identity.cohort_id,
        &r.identity.currency,
        &r.identity.source_revision,
        &r.identity.model_version,
        r.as_of_ordinal,
        r.horizon_days,
        r.day_count,
        segments,
    ))
    .map_err(|e| e.to_string())
}
pub(super) fn validate(r: &Request) -> Result<usize, String> {
    validate_with_work(r).map(|v| v.0)
}
pub(super) fn validate_with_work(r: &Request) -> Result<(usize, usize), String> {
    if r.schema != SCHEMA || r.identity.randomness_domain != "supplied_paired_histories-1" {
        return Err("deposit graph supplied paired history version required".into());
    }
    for s in [
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.base_scenario_id,
    ] {
        label(s)?;
    }
    validate_ordinal(r.as_of_ordinal)?;
    number(r.bump_bp, 0.001, 1000.)?;
    if r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || !(2..=121).contains(&r.reporting_ordinals.len())
    {
        return Err("deposit graph dimensions/currency exceed admission".into());
    }
    for &d in &r.reporting_ordinals {
        validate_ordinal(d)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|d| d[0] >= d[1])
    {
        return Err("deposit reporting dates must start asof and increase".into());
    }
    let mut ids = HashSet::new();
    let mut run_ids = HashSet::new();
    for s in &r.scenarios {
        label(&s.scenario_id)?;
        label(&s.run_id)?;
        number(s.severity, 0., 100.)?;
        s.collateral_curve.validate()?;
        if !ids.insert(&s.scenario_id)
            || !run_ids.insert(&s.run_id)
            || s.collateral_curve.currency != r.currency
            || !matches!(s.collateral_curve.role, CurveRole::Collateral { .. })
        {
            return Err("deposit scenario/curve scope mismatch".into());
        }
    }
    let base = r
        .scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("deposit base scenario missing")?;
    let mut positions = HashSet::new();
    let (mut runs, mut rows, mut work) = (0usize, 0usize, 0usize);
    for p in &r.positions {
        label(&p.position_id)?;
        if !positions.insert(&p.position_id) || p.histories.len() != r.scenarios.len() {
            return Err("deposit position/scenario coverage mismatch".into());
        }
        let mut known = None;
        let mut pairing = None;
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            if s.scenario_id != set.scenario_id {
                return Err("deposit history scenario ordering mismatch".into());
            }
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                if paths.is_empty() || paths.len() > 128 {
                    return Err("deposit history path admission exceeded".into());
                }
                let mut path_ids = HashSet::new();
                let mut weight = 0.;
                for h in paths {
                    portfolio_compute_control::checkpoint()?;
                    label(&h.path_id)?;
                    number(h.weight, 0., 1.)?;
                    weight += h.weight;
                    if !path_ids.insert(&h.path_id) {
                        return Err("duplicate deposit path".into());
                    }
                    let c = &h.request;
                    portfolio_model_core::deposit_state::validate_declared_request(c)?;
                    let n = c.segments.len();
                    let d = c.horizon_days as usize;
                    if n == 0
                        || n > 16
                        || d == 0
                        || d > 36600
                        || c.migrations.len() > n * n
                        || c.drivers.len() > d + 367
                        || c.segments
                            .iter()
                            .any(|s| s.interest_payment_ordinals.len() > d)
                        || c.schema != "deposit-state-1"
                        || c.identity.model_version != "deposit-state-1"
                        || c.identity.currency != r.currency
                        || c.identity.run_id != s.run_id
                        || c.as_of_ordinal != r.as_of_ordinal
                        || c.identity.measure
                            != if role == Role::Physical {
                                CashflowMeasure::Physical
                            } else {
                                CashflowMeasure::Pricing
                            }
                        || matches!(c.terminal, TerminalPolicy::FranchiseValueOnly { .. })
                    {
                        return Err("deposit supplied measure/state/calendar scope mismatch".into());
                    }
                    let end = r
                        .as_of_ordinal
                        .checked_add(c.horizon_days as i32)
                        .ok_or("deposit horizon overflow")?;
                    validate_ordinal(end)?;
                    if *r.reporting_ordinals.last().unwrap() > end {
                        return Err("deposit reporting exceeds complete tail".into());
                    }
                    let state = opening(h)?;
                    if let Some(ref original) = known {
                        if original != &state {
                            return Err("deposit histories must retain identical original opening/characteristics/maturity state".into());
                        }
                    } else {
                        known = Some(state);
                    }
                    let daily = (d + 1)
                        .checked_mul(n * n + 6 * n)
                        .ok_or("deposit daily work overflow")?;
                    if daily > 250000 {
                        return Err("deposit single projection work admission exceeded".into());
                    }
                    let generated = (d + 1) + (2 * n * d) + (n * n * d) + (2 * n * (d + 1));
                    rows = rows.checked_add(generated).ok_or("deposit rows overflow")?;
                    let passes = if role == Role::Physical {
                        r.reporting_ordinals.len() + 3
                    } else if role == Role::Pricing {
                        105
                    } else {
                        4
                    };
                    work = work
                        .checked_add(daily + generated * passes)
                        .ok_or("deposit graph work overflow")?;
                    runs += 1;
                }
                if (weight - 1.).abs() > 1e-12 {
                    return Err(
                        "deposit history weights must sum to one without normalization".into(),
                    );
                }
                if role != Role::Physical {
                    let pairs: Vec<_> = paths.iter().map(|h| (&h.path_id, h.weight)).collect();
                    if let Some(ref old) = pairing {
                        if old != &pairs {
                            return Err("deposit Q path identities/weights must remain paired across scenarios and bumps".into());
                        }
                    } else {
                        pairing = Some(pairs);
                    }
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
        return Err("deposit graph aggregate run/row/work admission exceeded".into());
    }
    count(r, MAX_IDENTITY_BYTES)?;
    Ok((base, work))
}
pub(super) fn base_key(r: &Request, base: usize) -> Result<Vec<u8>, String> {
    let positions: Vec<_> = r
        .positions
        .iter()
        .map(|p| (&p.position_id, &p.oas_target, &p.histories[base].pricing))
        .collect();
    let v = (
        &r.schema,
        &r.identity.model_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        r.as_of_ordinal,
        &r.currency,
        &r.base_scenario_id,
        r.tail_policy,
        r.cash_policy,
        &r.scenarios[base],
        positions,
    );
    count(&v, MAX_IDENTITY_BYTES)?;
    serde_json::to_vec(&v).map_err(|e| e.to_string())
}
pub(super) fn retained(r: &Request, out: &ResultData, key: usize) -> Result<(), String> {
    if count(r, MAX_IDENTITY_BYTES)?
        .saturating_add(count(out, MAX_RETAINED_BYTES)?)
        .saturating_mul(4)
        .saturating_add(key)
        > MAX_RETAINED_BYTES
    {
        Err("deposit graph retained byte admission exceeded".into())
    } else {
        Ok(())
    }
}
