use super::*;
use crate::model_market::{curves::CurveRole, label, range};
use std::collections::HashSet;

pub(super) fn checkpoint(check: &mut impl FnMut() -> bool) -> Result<(), String> {
    if check() {
        Err("COMPUTE_CANCELLED".into())
    } else {
        Ok(())
    }
}
pub(super) fn fraction(start: i32, end: i32, count: &str) -> f64 {
    (end - start) as f64 / if count == "actual360" { 360. } else { 365. }
}
fn ordinal(o: i32) -> Result<(), String> {
    if !(1..=3_652_059).contains(&o) {
        Err("ordinal outside supported calendar".into())
    } else {
        Ok(())
    }
}
fn utc_day(o: i32) -> i64 {
    (i64::from(o) - 719_163) * 86_400
}
fn identity(i: &Identity) -> Result<(), String> {
    for x in [
        &i.source_id,
        &i.source_revision,
        &i.input_revision,
        &i.currency,
    ] {
        label(x, 256)?;
    }
    if i.model_version != SCHEMA {
        return Err("source identity model version mismatch".into());
    }
    ordinal(i.as_of_ordinal)
}
fn market(r: &Request, m: &Market, check: &mut impl FnMut() -> bool) -> Result<usize, String> {
    checkpoint(check)?;
    identity(&m.identity)?;
    if m.identity.currency != r.identity.currency
        || m.identity.as_of_ordinal != r.identity.as_of_ordinal
    {
        return Err("market source currency/asof mismatch".into());
    }
    let day = utc_day(m.identity.as_of_ordinal);
    if m.available_unix_seconds < utc_day(1)
        || m.available_unix_seconds > m.information_unix_seconds
        || m.information_unix_seconds < day
        || m.information_unix_seconds >= day + 86_400
    {
        return Err("source unavailable at declared asof information time".into());
    }
    if m.curves.is_empty() || m.curves.len() > 16 {
        return Err("curve count admission".into());
    }
    let mut names = HashSet::new();
    let mut nodes = 0usize;
    for c in &m.curves {
        checkpoint(check)?;
        c.validate()?;
        if c.currency != r.identity.currency || !names.insert(&c.curve_id) {
            return Err("duplicate curve or currency mismatch".into());
        }
        nodes = nodes
            .checked_add(c.times_years.len())
            .ok_or("node work overflow")?;
    }
    let c = m
        .curves
        .iter()
        .find(|c| c.curve_id == r.binding.collateral_curve_id)
        .ok_or("missing named collateral curve")?;
    let i = m
        .curves
        .iter()
        .find(|c| c.curve_id == r.binding.index_curve_id)
        .ok_or("missing named index curve")?;
    if !matches!(&c.role, CurveRole::Collateral { collateral_index } if *collateral_index == r.binding.collateral_index)
        || !matches!(&i.role, CurveRole::Index { index_id } if *index_id == r.binding.index_id)
    {
        return Err("named collateral/index role binding mismatch".into());
    }
    for p in &r.contract.periods {
        checkpoint(check)?;
        if m.information_unix_seconds > p.reset_cutoff_unix_seconds {
            return Err("source unavailable at contractual reset cutoff".into());
        }
        // Reject unsupported tails before projection allocation, using native lookup.
        for (curve, o) in [
            (i, p.start_ordinal),
            (i, p.end_ordinal),
            (c, p.payment_ordinal),
        ] {
            curve.value_validated((o - r.identity.as_of_ordinal) as f64 / 365.)?;
        }
    }
    Ok(nodes)
}

pub(super) fn admit(r: &Request, check: &mut impl FnMut() -> bool) -> Result<Admission, String> {
    checkpoint(check)?;
    if r.schema != SCHEMA
        || r.model_version != SCHEMA
        || r.measure != "pricing"
        || r.projection_policy != "deterministic_asof_simple_forward-1"
        || r.curve_time_policy != "actual365_fixed_from_asof-1"
    {
        return Err("unsupported version/measure/projection/time policy".into());
    }
    identity(&r.identity)?;
    for x in [
        &r.binding.index_id,
        &r.binding.index_curve_id,
        &r.binding.collateral_curve_id,
        &r.binding.collateral_index,
        &r.binding.calendar_identity,
        &r.contract.contract_id,
        &r.contract.contract_revision,
        &r.original.review_identity,
    ] {
        label(x, 256)?;
    }
    if !r.binding.complete_supplied_date_mapping
        || r.binding.collateral_curve_id == r.binding.index_curve_id
    {
        return Err("explicit complete distinct index/collateral mapping required".into());
    }
    if r.contract.monetary_unit != "actual_currency"
        || !matches!(
            r.contract.direction.as_str(),
            "payer_fixed" | "receiver_fixed"
        )
    {
        return Err("unsupported monetary unit/direction".into());
    }
    range(r.contract.notional, 1e-12, 1e12, "notional")?;
    range(r.contract.fixed_rate, -10., 10., "fixed rate")?;
    range(r.contract.margin, -10., 10., "margin")?;
    range(r.original.locked_oas, -1., 1., "locked original OAS")?;
    range(r.risk_bump, 1e-6, 0.05, "risk bump")?;
    for c in [&r.contract.fixed_day_count, &r.contract.index_day_count] {
        if !matches!(c.as_str(), "actual360" | "actual365_fixed") {
            return Err("unsupported contractual day count".into());
        }
    }
    if r.contract.periods.is_empty() || r.contract.periods.len() > 512 {
        return Err("period count admission".into());
    }
    let mut names = HashSet::new();
    let mut previous = None;
    for p in &r.contract.periods {
        checkpoint(check)?;
        label(&p.period_id, 256)?;
        for o in [
            p.reset_ordinal,
            p.start_ordinal,
            p.end_ordinal,
            p.payment_ordinal,
        ] {
            ordinal(o)?;
        }
        if !names.insert(&p.period_id)
            || p.reset_ordinal < r.identity.as_of_ordinal
            || p.reset_ordinal > p.start_ordinal
            || p.start_ordinal >= p.end_ordinal
            || p.payment_ordinal < p.end_ordinal
            || p.payment_ordinal - r.identity.as_of_ordinal > 36_500
            || previous.is_some_and(|o| o != p.start_ordinal)
        {
            return Err("dated period order/complete contiguous tail domain".into());
        }
        previous = Some(p.end_ordinal);
        if p.reset_cutoff_unix_seconds < utc_day(p.reset_ordinal)
            || p.reset_cutoff_unix_seconds >= utc_day(p.reset_ordinal) + 86_400
        {
            return Err("exact reset cutoff must belong to reset calendar date".into());
        }
    }
    ordinal(r.contract.maturity_ordinal)?;
    if previous != Some(r.contract.maturity_ordinal) {
        return Err("complete final accrual end must equal declared contractual maturity".into());
    }
    let nodes = market(r, &r.original.market, check)?
        .checked_add(market(r, &r.current_market, check)?)
        .ok_or("node work overflow")?;
    // String byte lengths use serde escaping, not raw UTF-8 labels. No source row vectors allocated yet.
    let input = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if input > MAX_INPUT_BYTES {
        return Err("input byte admission exceeded".into());
    }
    let rows = r
        .contract
        .periods
        .len()
        .checked_mul(8)
        .ok_or("projection row overflow")?;
    let work = nodes
        .checked_mul(4)
        .and_then(|v| v.checked_add(rows.checked_mul(64)?))
        .ok_or("work admission overflow")?;
    // Each repeated row contains only a subset of input string leaves. Charging complete input per row
    // plus 4096 bounded scalar/JSON overhead conservatively covers every escaped metadata occurrence.
    let retained = rows
        .checked_mul(input.checked_add(4096).ok_or("retained overflow")?)
        .and_then(|v| v.checked_add(input))
        .and_then(|v| v.checked_add(16_384))
        .ok_or("retained admission overflow")?;
    if work > MAX_WORK || retained > MAX_RETAINED_BYTES {
        return Err("aggregate work/retained byte admission exceeded".into());
    }
    checkpoint(check)?;
    Ok(Admission {
        input_bytes: input,
        retained_bytes: retained,
        admitted_work: work,
        projection_rows: rows,
    })
}
