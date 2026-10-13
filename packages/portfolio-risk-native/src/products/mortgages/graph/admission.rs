use super::*;
use crate::market::analysis::curves::CurveRole;
use portfolio_model_core::{cashflow::validate_ordinal, limits::MAX_MONETARY_AMOUNT};
use serde::Serialize;
use std::{collections::HashSet, io::Write};
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        Err("mortgage graph identity requires bounded nonblank text".into())
    } else {
        Ok(())
    }
}
pub(super) fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        Err("mortgage graph numeric admission exceeded".into())
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
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(data.len());
        if self.size > self.limit {
            return Err(std::io::Error::other(
                "mortgage graph byte admission exceeded",
            ));
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut out = Counter { size: 0, limit };
    serde_json::to_writer(&mut out, v).map_err(|e| e.to_string())?;
    Ok(out.size)
}
fn paths(paths: &[History]) -> Result<(), String> {
    if paths.is_empty() || paths.len() > 128 {
        return Err("mortgage graph path admission exceeded".into());
    }
    let mut ids = HashSet::new();
    let mut sum = 0.;
    for p in paths {
        label(&p.path_id)?;
        label(&p.assumption_id)?;
        number(p.weight, 0., 1.)?;
        if !ids.insert(&p.path_id)
            || p.path.is_empty()
            || p.path.len() > 4096
            || p.arm_resets.len() > 4096
        {
            return Err("mortgage duplicate path or path/reset admission exceeded".into());
        }
        sum += p.weight;
    }
    if (sum - 1.).abs() > 1e-12 {
        return Err("mortgage path weights must sum to one; no normalization fallback".into());
    }
    Ok(())
}
pub(super) fn validate(r: &Request) -> Result<usize, String> {
    validate_with_work(r).map(|(base, _)| base)
}
pub(super) fn validate_with_work(r: &Request) -> Result<(usize, usize), String> {
    if r.schema != SCHEMA || r.identity.randomness_domain != "supplied_paired_histories-1" {
        return Err(
            "mortgage graph requires explicit version and supplied paired histories".into(),
        );
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
        return Err("mortgage graph requires uppercase currency".into());
    }
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("mortgage reporting boundaries required")?;
    if !(1..=120).contains(&h)
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
    {
        return Err("mortgage graph dimensions exceed admission".into());
    }
    count(r, MAX_IDENTITY_BYTES)?;
    for &date in &r.reporting_ordinals {
        validate_ordinal(date)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("mortgage reporting dates must begin at asof and increase".into());
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
            return Err("mortgage scenario/curve/currency mismatch".into());
        }
        s.collateral_curve.validate()?;
    }
    let base = r
        .scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing mortgage base scenario")?;
    let mut positions = HashSet::new();
    let (mut runs, mut rows, mut work) = (0usize, 0usize, 0usize);
    for p in &r.positions {
        label(&p.position_id)?;
        if !positions.insert(&p.position_id) {
            return Err("duplicate mortgage position".into());
        }
        let c = &p.contract;
        if c.horizon_days == 0
            || c.horizon_days > 36600
            || c.payment_dates.is_empty()
            || c.payment_dates.len() > 4096
        {
            return Err("mortgage horizon/payment calendar admission exceeded".into());
        }
        if c.complete_recovery_tails.iter().any(|tail| tail.len() > 32) {
            return Err("mortgage recovery tail exceeds native point admission".into());
        }
        let end = r
            .as_of_ordinal
            .checked_add(c.horizon_days as i32)
            .ok_or("mortgage horizon overflow")?;
        validate_ordinal(end)?;
        if c.schema != "mortgage-state-1"
            || c.identity.model_version != "mortgage-state-1"
            || c.monetary_unit != "currency_units"
            || c.identity.position_id != p.position_id
            || c.identity.currency != r.currency
            || c.identity.measure != CashflowMeasure::Contractual
            || c.as_of_ordinal != r.as_of_ordinal
            || !c.path.is_empty()
            || p.histories.len() != r.scenarios.len()
            || r.reporting_ordinals[h] > end
        {
            return Err("mortgage known contract/measure/date/history mismatch".into());
        }
        let final_due = *c.payment_dates.last().unwrap();
        if final_due > end {
            return Err("mortgage graph rejects truncated contractual calendar".into());
        }
        let agency = matches!(c.ownership, MortgageOwnership::Agency { .. });
        if let MortgageOwnership::Agency {
            default_buyout_lag_days,
            ..
        } = c.ownership
        {
            if default_buyout_lag_days > 36600
                || final_due
                    .checked_add(default_buyout_lag_days as i32)
                    .is_none_or(|date| date > end)
            {
                return Err("mortgage agency horizon must include full possible investor buyout recognition tail".into());
            }
        }
        if let MortgageRate::Arm { resets, .. } = &c.rate {
            if !resets.is_empty() {
                return Err(
                    "mortgage known ARM template requires empty measure-specific resets".into(),
                );
            }
        }
        let mut reference = None;
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            if set.scenario_id != s.scenario_id {
                return Err("mortgage histories must match explicit scenario order".into());
            }
            for (role, histories) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                paths(histories)?;
                runs = runs
                    .checked_add(histories.len())
                    .ok_or("mortgage run overflow")?;
                if runs > MAX_RUNS {
                    return Err("mortgage graph aggregate run admission exceeded".into());
                }
                for path in histories {
                    if path.path[0].start_ordinal != r.as_of_ordinal
                        || path
                            .path
                            .windows(2)
                            .any(|v| v[0].start_ordinal >= v[1].start_ordinal)
                        || path
                            .path
                            .iter()
                            .any(|v| v.start_ordinal < r.as_of_ordinal || v.start_ordinal > end)
                        || path
                            .arm_resets
                            .iter()
                            .any(|v| v.ordinal <= r.as_of_ordinal || v.ordinal > end)
                        || path
                            .arm_resets
                            .windows(2)
                            .any(|v| v[0].ordinal >= v[1].ordinal)
                        || matches!(c.rate, MortgageRate::Fixed) && !path.arm_resets.is_empty()
                    {
                        return Err(
                            "mortgage path/reset dates and fixed/ARM ownership mismatch".into()
                        );
                    }
                    let boundaries =
                        c.payment_dates.len() + path.path.len() + path.arm_resets.len() + 1;
                    if boundaries > 4096 {
                        return Err("mortgage combined boundary admission exceeded".into());
                    }
                    let has_default = path
                        .path
                        .iter()
                        .any(|v| v.credit_hazards_per_year.iter().any(|row| row[3] != 0.));
                    let claims = if has_default { 3 * boundaries } else { 0 };
                    if claims > 4096 {
                        return Err("mortgage worst-case default claim admission exceeded".into());
                    }
                    let recovery = claims
                        * c.complete_recovery_tails
                            .iter()
                            .map(|t| t.len())
                            .max()
                            .unwrap_or(0);
                    let default_coupons = if agency {
                        claims * (c.payment_dates.len() + 1)
                    } else {
                        0
                    };
                    let entitlement =
                        3 * c.payment_dates.len() + 2 * boundaries + recovery + default_coupons;
                    let count = 10 * boundaries
                        + path.arm_resets.len()
                        + claims
                        + recovery
                        + 3 * entitlement
                        + default_coupons;
                    rows = rows.checked_add(count).ok_or("mortgage row overflow")?;
                    let passes = if role == Role::Physical {
                        h + 2
                    } else if role == Role::Pricing {
                        104
                    } else {
                        4
                    };
                    // Shared operator admits <=128 Poisson terms and <=11 composition
                    // levels. Two operators per interval; charge bounded evaluations,
                    // interval/default scans, full tails and every valuation/report pass.
                    let source_work = boundaries * (256 + 44 + claims) + recovery + default_coupons;
                    let report_work = if role == Role::Physical {
                        count * passes
                    } else {
                        count + entitlement * passes
                    };
                    work = work
                        .checked_add(source_work + report_work)
                        .ok_or("mortgage work overflow")?;
                    if rows > MAX_ROWS || work > MAX_WORK {
                        return Err("mortgage graph aggregate row/work admission exceeded".into());
                    }
                }
            }
            for history in [&set.pricing, &set.pricing_down, &set.pricing_up] {
                let pairing: Vec<_> = history.iter().map(|p| (&p.path_id, p.weight)).collect();
                if let Some(ref old) = reference {
                    if old != &pairing {
                        return Err("mortgage Q histories require identical ordered path identities/weights across scenarios/bumps".into());
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
    work = work
        .checked_add(4 * runs * r.positions.len() * r.scenarios.len())
        .ok_or("mortgage source selection work overflow")?;
    if runs > MAX_RUNS || rows > MAX_ROWS || work > MAX_WORK {
        return Err("mortgage graph aggregate run/row/work admission exceeded".into());
    }
    count(r, MAX_IDENTITY_BYTES)?;
    // Admit the complete aggregate budget before constructing any role request.
    // Reuse core domain/fixing validation without generating a mortgage path.
    for position in &r.positions {
        for (scenario, set) in r.scenarios.iter().zip(&position.histories) {
            for (role, histories) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                for history in histories {
                    portfolio_compute_control::checkpoint()?;
                    mortgage_state::validate_request(&history_request(
                        position, scenario, role, history,
                    ))?;
                }
            }
        }
    }
    Ok((base, work))
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
    let key = (
        &r.schema,
        &r.identity.model_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        r.as_of_ordinal,
        &r.currency,
        &r.base_scenario_id,
        r.terminal_policy,
        &r.scenarios[base],
        positions,
    );
    count(&key, MAX_IDENTITY_BYTES)?;
    serde_json::to_vec(&key).map_err(|e| e.to_string())
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
        return Err("mortgage graph retained byte admission exceeded".into());
    }
    Ok(())
}
