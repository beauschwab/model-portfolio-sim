use super::*;
use crate::{
    advanced_graph::{OasTarget, Role},
    funding_contract::{Exit, FundingContract, FundingRequest, FundingResult},
    model_market::curves::CurveRole,
    term_enhancements::indexed::{Coupon, RateSource},
};
use portfolio_model_core::{
    cashflow::{validate_ordinal, CashflowMeasure},
    limits::MAX_MONETARY_AMOUNT,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
};
fn label(v: &str) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err("funding graph identity requires bounded nonblank text".into());
    }
    Ok(())
}
fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        return Err("funding graph numeric admission exceeded".into());
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
    fn write(&mut self, v: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(v.len());
        if self.size > self.limit {
            return Err(std::io::Error::other(
                "funding graph byte admission exceeded",
            ));
        }
        Ok(v.len())
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
fn canonical<T: Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}
fn without_reporting(r: &FundingRequest) -> Result<Value, String> {
    let mut v = canonical(r)?;
    v.as_object_mut()
        .ok_or("funding object missing")?
        .remove("reporting_end_ordinal");
    Ok(v)
}
/// Compare known opening/contract content, allowing ONLY explicitly modeled
/// future margin/election and conditional projected rate source/rate slots.
fn opening_key(r: &FundingRequest) -> Result<Value, String> {
    let mut v = without_reporting(r)?;
    let top = v.as_object_mut().ok_or("funding object missing")?;
    for name in ["measure", "scenario_id", "assumption"] {
        top.remove(name);
    }
    for c in top
        .get_mut("contracts")
        .and_then(Value::as_array_mut)
        .ok_or("known contracts missing")?
    {
        let map = c.as_object_mut().ok_or("known contract missing")?;
        map.remove("exit");
        map.remove("rate_path");
        if let Some(a) = map.get_mut("collateral").and_then(Value::as_object_mut) {
            a.remove("margin_steps");
        }
        for p in map
            .get_mut("interest_periods")
            .and_then(Value::as_array_mut)
            .ok_or("known periods missing")?
        {
            if let Some(obs) = p["coupon"]
                .get_mut("observations")
                .and_then(Value::as_array_mut)
            {
                for o in obs {
                    if o["source"]["kind"] == "conditional_projection" {
                        let fields = o.as_object_mut().ok_or("observation object missing")?;
                        fields.remove("rate");
                        fields.insert("source".into(), json!({"kind":"conditional_projection"}));
                    }
                }
            }
        }
    }
    Ok(v)
}
fn contracts(r: &FundingRequest) -> Result<Vec<&FundingContract>, String> {
    if r.contracts.is_empty() || r.contracts.len() > 256 || r.collateral_assets.len() > 4096 {
        return Err("funding graph book admission exceeded".into());
    }
    let mut all = Vec::new();
    for c in &r.contracts {
        all.push(c);
        if let Exit::Maturity {
            rollover: Some(roll),
        } = &c.exit
        {
            if let Some(next) = &roll.replacement {
                if !matches!(next.exit, Exit::Maturity { rollover: None }) {
                    return Err("funding graph rejects recursive replacement".into());
                }
                all.push(next);
            }
        }
    }
    if all.len() > 256 {
        return Err("funding graph old plus replacement contract admission exceeded".into());
    }
    Ok(all)
}
#[derive(Default)]
struct Budget {
    runs: usize,
    rows: usize,
    work: usize,
}
fn preflight(r: &FundingRequest, h: usize, role: Role, b: &mut Budget) -> Result<(), String> {
    let all = contracts(r)?;
    let mut rows = r.collateral_assets.len();
    let mut work = 0usize;
    let mut cash = 0usize;
    let mut actions = 0usize;
    for c in all {
        if c.interest_periods.is_empty() || c.interest_periods.len() > 4096 {
            return Err("funding graph coupon admission exceeded".into());
        }
        rows += c.interest_periods.len() + 1;
        cash += c.interest_periods.len() + 1;
        actions += 2;
        for p in &c.interest_periods {
            if let Coupon::Indexed { observations, .. } = &p.coupon {
                if observations.len() > 4096 {
                    return Err("funding graph observation admission exceeded".into());
                }
                rows += observations.len();
                work += observations.len() * 8;
            }
        }
        if let Some(a) = &c.collateral {
            if a.margin_steps.len() > 4096
                || a.initial_pledges.len() > 4096
                || a.eligible_asset_classes.len() > 256
                || a.initial_snapshot.values.len() > 4096
            {
                return Err("funding graph collateral admission exceeded".into());
            }
            rows += 2 * a.initial_pledges.len() + a.initial_snapshot.values.len();
            work += a.initial_snapshot.values.len();
            actions += a.margin_steps.len();
            for m in &a.margin_steps {
                if m.transfers.len() > 4096 || m.snapshot.values.len() > 4096 {
                    return Err("funding graph margin admission exceeded".into());
                }
                rows += 1 + 4 * m.transfers.len() + m.snapshot.values.len();
                work += m.snapshot.values.len() + m.transfers.len() * 4;
            }
            // Every held/delivered asset may be marked, validated, sorted and
            // finally returned/disposed; charge registry scans conservatively.
            let registry = r.collateral_assets.len();
            work += (2 * a.margin_steps.len() + 4)
                * registry
                * (6 + (usize::BITS - registry.max(1).leading_zeros()) as usize);
        }
        match &c.exit {
            Exit::Maturity { rollover: Some(_) } => {
                rows += 1;
            }
            Exit::CounterpartyDefault { policy: d } => {
                if d.complete_principal_recoveries.len() > 4096
                    || d.complete_interest_recoveries.len() > 4096
                {
                    return Err("funding graph recovery tail admission exceeded".into());
                }
                cash += d.complete_principal_recoveries.len()
                    + d.complete_interest_recoveries.len()
                    + 1;
                rows += d.complete_principal_recoveries.len()
                    + d.complete_interest_recoveries.len()
                    + 1;
                if let Some(real) = &d.collateral_realization {
                    if real.snapshot.values.len() > 4096 {
                        return Err("funding graph sale snapshot admission exceeded".into());
                    }
                    rows += real.snapshot.values.len() + 1;
                    work += real.snapshot.values.len() * 8;
                }
            }
            _ => {}
        }
    }
    let passes = if role == Role::Physical {
        h + 3
    } else if role == Role::Pricing {
        104
    } else {
        4
    };
    rows += cash;
    work += rows
        + actions * (2 + (usize::BITS - actions.max(1).leading_zeros()) as usize)
        + if role == Role::Physical {
            // Per-period securities are accumulated in an ordered asset/owner/
            // disposition map. Charge bounded logarithmic insertion as well
            // as all monetary/report scans, not just cash output count.
            rows * passes * 20
        } else {
            cash * passes
        };
    b.rows = b
        .rows
        .checked_add(rows)
        .ok_or("funding graph rows overflow")?;
    b.work = b
        .work
        .checked_add(work)
        .ok_or("funding graph work overflow")?;
    if b.rows > MAX_ROWS || b.work > MAX_WORK {
        return Err("funding graph aggregate row/work admission exceeded before projection".into());
    }
    Ok(())
}
fn immutable(
    map: &mut HashMap<(String, String, String), Value>,
    kind: &str,
    id: &crate::term_enhancements::Identity,
    value: Value,
) -> Result<(), String> {
    let key = (kind.into(), id.id.clone(), id.revision.clone());
    if let Some(old) = map.get(&key) {
        if old != &value {
            return Err("immutable funding source identity reused with different content".into());
        }
    } else {
        map.insert(key, value);
    }
    Ok(())
}
fn sources(
    r: &FundingRequest,
    map: &mut HashMap<(String, String, String), Value>,
) -> Result<(), String> {
    let mut metadata = 0usize;
    crate::term_enhancements::identity(&r.assumption, &mut metadata)?;
    crate::term_enhancements::text(&r.scenario_id, &mut metadata)?;
    for c in contracts(r)? {
        crate::term_enhancements::identity(&c.rate_path, &mut metadata)?;
        let mut snapshots = Vec::new();
        if let Some(a) = &c.collateral {
            snapshots.push(&a.initial_snapshot);
            snapshots.extend(a.margin_steps.iter().map(|s| &s.snapshot));
        }
        if let Exit::CounterpartyDefault { policy: d } = &c.exit {
            if let Some(real) = &d.collateral_realization {
                snapshots.push(&real.snapshot);
            }
        }
        for s in snapshots {
            immutable(map, "collateral_snapshot", &s.source, canonical(s)?)?;
        }
        for p in &c.interest_periods {
            if let Coupon::Indexed {
                index,
                observations,
                ..
            } = &p.coupon
            {
                for o in observations {
                    money(o.rate)?;
                    if let RateSource::ConditionalProjection {
                        model,
                        path,
                        information_unix_seconds,
                        measure,
                    } = &o.source
                    {
                        crate::term_enhancements::identity(model, &mut metadata)?;
                        crate::term_enhancements::identity(path, &mut metadata)?;
                        if !(0..=253_402_300_799).contains(information_unix_seconds)
                            || *measure == CashflowMeasure::Contractual
                        {
                            return Err(
                                "invalid known/supplied conditional funding rate metadata".into()
                            );
                        }
                    }
                    if let RateSource::Observed {
                        fixing,
                        publication_unix_seconds,
                    } = &o.source
                    {
                        immutable(
                            map,
                            "observed_fixing",
                            fixing,
                            json!([
                                index.index_id,
                                o.fixing_ordinal,
                                o.rate,
                                publication_unix_seconds
                            ]),
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate(r: &Request) -> Result<usize, String> {
    if r.schema != SCHEMA || r.identity.randomness_domain != "supplied_paired_histories-1" {
        return Err("funding graph requires explicit version and supplied paired histories".into());
    }
    for v in [
        &r.book_id,
        &r.currency,
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
        return Err("funding graph currency admission exceeded".into());
    }
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("funding report boundaries required")?;
    if !(1..=120).contains(&h)
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || r.histories.len() != r.scenarios.len()
    {
        return Err("funding graph dimensions exceed admission".into());
    }
    for &d in &r.reporting_ordinals {
        validate_ordinal(d)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("funding reporting dates must start at asof and increase".into());
    }
    if r.known.schema != funding_contract::SCHEMA
        || r.known.model_version != funding_contract::MODEL_VERSION
        || r.known.measure != CashflowMeasure::Contractual
        || r.known.monetary_unit != "currency_units"
        || r.known.currency != r.currency
        || r.known.asof_ordinal != r.as_of_ordinal
        || r.known.reporting_end_ordinal != r.reporting_ordinals[h]
        || r.known.tail_end_ordinal < r.reporting_ordinals[h]
        || (r.known.tail_end_ordinal - r.as_of_ordinal) as i64 > 36600
    {
        return Err("funding known contract version/measure/date/unit mismatch".into());
    }
    validate_ordinal(r.known.tail_end_ordinal)?;
    for c in contracts(&r.known)? {
        if !matches!(c.exit, Exit::Maturity { rollover: None })
            || c.collateral
                .as_ref()
                .is_some_and(|a| !a.margin_steps.is_empty())
        {
            return Err(
                "funding known template requires maturity/no-election and no future margin actions"
                    .into(),
            );
        }
    }
    let mut ids = HashSet::new();
    let mut runs = HashSet::new();
    let mut budget = Budget::default();
    let mut pairing = None;
    for (s, set) in r.scenarios.iter().zip(&r.histories) {
        label(&s.scenario_id)?;
        label(&s.run_id)?;
        number(s.severity, 0., 100.)?;
        if !ids.insert(&s.scenario_id)
            || !runs.insert(&s.run_id)
            || s.collateral_curve.currency != r.currency
            || !matches!(s.collateral_curve.role, CurveRole::Collateral { .. })
            || set.scenario_id != s.scenario_id
        {
            return Err("funding scenario/history/curve identity mismatch".into());
        }
        s.collateral_curve.validate()?;
        for (role, paths) in [
            (Role::Physical, &set.physical),
            (Role::Pricing, &set.pricing),
            (Role::PricingDown, &set.pricing_down),
            (Role::PricingUp, &set.pricing_up),
        ] {
            if paths.is_empty() || paths.len() > 128 {
                return Err("funding graph path admission exceeded".into());
            }
            let mut path_ids = HashSet::new();
            let mut sum = 0.;
            for path in paths {
                label(&path.path_id)?;
                number(path.weight, 0., 1.)?;
                if !path_ids.insert(&path.path_id)
                    || path.request.scenario_id != s.run_id
                    || path.request.measure
                        != if role == Role::Physical {
                            CashflowMeasure::Physical
                        } else {
                            CashflowMeasure::Pricing
                        }
                    || path.request.reporting_end_ordinal != r.reporting_ordinals[h]
                {
                    return Err("funding path identity/run/measure/report mismatch".into());
                }
                budget.runs += 1;
                sum += path.weight;
                preflight(&path.request, h, role, &mut budget)?;
            }
            if (sum - 1.).abs() > 1e-12 {
                return Err(
                    "funding probabilities must sum to one; no normalization fallback".into(),
                );
            }
            if role != Role::Physical {
                let pairs: Vec<_> = paths.iter().map(|p| (&p.path_id, p.weight)).collect();
                if let Some(ref old) = pairing {
                    if old != &pairs {
                        return Err("funding Q histories require identical ordered path IDs/weights across scenarios/bumps".into());
                    }
                } else {
                    pairing = Some(pairs);
                }
            }
        }
    }
    if budget.runs > MAX_RUNS {
        return Err("funding graph run admission exceeded".into());
    }
    match &r.oas_target {
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
    count(r, MAX_IDENTITY_BYTES)?;
    let known = opening_key(&r.known)?;
    let mut content = HashMap::new();
    sources(&r.known, &mut content)?;
    for set in &r.histories {
        for paths in [
            &set.physical,
            &set.pricing,
            &set.pricing_down,
            &set.pricing_up,
        ] {
            for p in paths {
                if opening_key(&p.request)? != known {
                    return Err(
                        "funding history changes known opening/static contract terms".into(),
                    );
                }
                sources(&p.request, &mut content)?;
            }
        }
    }
    r.scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing funding base scenario".into())
}
pub(super) fn base_key(r: &Request, base: usize) -> Result<Vec<u8>, String> {
    let q: Result<Vec<_>, String> = r.histories[base]
        .pricing
        .iter()
        .map(|p| Ok((&p.path_id, p.weight, without_reporting(&p.request)?)))
        .collect();
    bytes(&(
        &r.schema,
        &r.book_id,
        &r.identity.model_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
        &r.identity.randomness_domain,
        r.as_of_ordinal,
        &r.currency,
        r.cash_timing,
        r.tail_policy,
        &r.base_scenario_id,
        &r.scenarios[base],
        without_reporting(&r.known)?,
        &r.oas_target,
        q?,
    ))
}
pub(super) fn source_rows(r: &FundingResult) -> usize {
    r.schedule.events.len()
        + r.contracts.len()
        + r.contracts
            .iter()
            .map(|c| {
                c.coupon_audit.len()
                    + c.coupon_audit
                        .iter()
                        .map(|a| a.selected_rate_sources.len())
                        .sum::<usize>()
            })
            .sum::<usize>()
        + r.collateral_movements.len()
        + r.margin_calls.len()
        + r.margin_calls
            .iter()
            .map(|m| m.requested_transfers.len() + m.executed_transfers.len())
            .sum::<usize>()
        + r.collateral_realizations.len()
        + r.rollover.len()
        + r.inventory.len()
}
pub(super) fn retained(r: &Request, out: &ResultData, key: usize) -> Result<(), String> {
    let input = count(r, MAX_IDENTITY_BYTES)?;
    let output = count(out, MAX_RETAINED_BYTES)?;
    if input
        .saturating_add(output)
        .saturating_mul(4)
        .saturating_add(key)
        > MAX_RETAINED_BYTES
    {
        return Err("funding graph retained byte admission exceeded".into());
    }
    Ok(())
}
