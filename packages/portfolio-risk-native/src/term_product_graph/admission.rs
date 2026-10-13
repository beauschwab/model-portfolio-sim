use super::*;
use crate::{
    advanced_graph::{OasTarget, Role},
    model_market::curves::CurveRole,
    term_enhancements::{
        self,
        cd::Election,
        cd_sequence,
        indexed::{Coupon, RateSource},
    },
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
        return Err("term graph requires bounded nonblank identity".into());
    }
    Ok(())
}
fn number(v: f64, lo: f64, hi: f64) -> Result<(), String> {
    if !v.is_finite() || !(lo..=hi).contains(&v) {
        return Err("term graph numeric admission exceeded".into());
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
    controlled: bool,
    next_check: usize,
}
impl Write for ByteCount {
    fn write(&mut self, v: &[u8]) -> std::io::Result<usize> {
        self.size = self.size.saturating_add(v.len());
        if self.controlled && self.size >= self.next_check {
            portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
            self.next_check = self.size.saturating_add(16_384);
        }
        if self.size > self.limit {
            return Err(std::io::Error::other("term graph byte admission exceeded"));
        }
        Ok(v.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut out = ByteCount {
        size: 0,
        limit,
        controlled: false,
        next_check: 0,
    };
    serde_json::to_writer(&mut out, v).map_err(|e| e.to_string())?;
    Ok(out.size)
}
fn count_checked<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    portfolio_compute_control::checkpoint()?;
    let mut out = ByteCount {
        size: 0,
        limit,
        controlled: true,
        next_check: 0,
    };
    serde_json::to_writer(&mut out, v).map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()?;
    Ok(out.size)
}
fn canonical<T: Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}
fn bytes<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    count(v, MAX_IDENTITY_BYTES)?;
    serde_json::to_vec(v).map_err(|e| e.to_string())
}
fn indexed_key(r: &indexed::ContractPathRequest) -> Result<Value, String> {
    let mut v = canonical(r)?;
    let fields = v.as_object_mut().ok_or("term object missing")?;
    for name in [
        "reporting_end_ordinal",
        "measure",
        "scenario_id",
        "assumption",
        "path",
    ] {
        fields.remove(name);
    }
    for p in fields
        .get_mut("periods")
        .and_then(Value::as_array_mut)
        .ok_or("term periods missing")?
    {
        if let Some(obs) = p["coupon"]
            .get_mut("observations")
            .and_then(Value::as_array_mut)
        {
            for o in obs {
                if o["source"]["kind"] == "conditional_projection" {
                    let item = o.as_object_mut().ok_or("term observation missing")?;
                    item.remove("rate");
                    item.insert("source".into(), json!({"kind":"conditional_projection"}));
                }
            }
        }
    }
    Ok(v)
}
fn base_history(p: &ProductHistory) -> Result<Value, String> {
    let mut v = canonical(p)?;
    v["request"]
        .as_object_mut()
        .ok_or("term raw request missing")?
        .remove("reporting_end_ordinal");
    Ok(v)
}
fn base_known(p: &KnownProduct) -> Result<Value, String> {
    let mut v = canonical(p)?;
    if let Some(fields) = v["contract"].as_object_mut() {
        fields.remove("reporting_end_ordinal");
    }
    Ok(v)
}
fn immutable(
    map: &mut HashMap<(String, String), Value>,
    id: &term_enhancements::Identity,
    v: Value,
) -> Result<(), String> {
    let key = (id.id.clone(), id.revision.clone());
    if let Some(old) = map.get(&key) {
        if old != &v {
            return Err("immutable term fixing identity reused with different content".into());
        }
    } else {
        map.insert(key, v);
    }
    Ok(())
}
fn index_metadata(
    r: &indexed::ContractPathRequest,
    map: &mut HashMap<(String, String), Value>,
) -> Result<usize, String> {
    if r.periods.is_empty() || r.periods.len() > 4096 {
        return Err("term graph coupon admission exceeded".into());
    }
    let mut rows = 2 * r.periods.len() + 1;
    let mut metadata = 0usize;
    for id in [&r.contract, &r.assumption, &r.path] {
        term_enhancements::identity(id, &mut metadata)?;
    }
    for p in &r.periods {
        if let Coupon::Indexed {
            index,
            observations,
            ..
        } = &p.coupon
        {
            if observations.len() > 4096 {
                return Err("term graph observation admission exceeded".into());
            }
            rows += observations.len();
            for o in observations {
                money(o.rate)?;
                match &o.source {
                    RateSource::Observed {
                        fixing,
                        publication_unix_seconds,
                    } => {
                        term_enhancements::identity(fixing, &mut metadata)?;
                        immutable(
                            map,
                            fixing,
                            json!([
                                index.index_id,
                                o.fixing_ordinal,
                                o.rate,
                                publication_unix_seconds
                            ]),
                        )?;
                    }
                    RateSource::ConditionalProjection {
                        model,
                        path,
                        information_unix_seconds,
                        measure,
                    } => {
                        term_enhancements::identity(model, &mut metadata)?;
                        term_enhancements::identity(path, &mut metadata)?;
                        if !(0..=253_402_300_799).contains(information_unix_seconds)
                            || *measure == CashflowMeasure::Contractual
                        {
                            return Err("term conditional source metadata invalid".into());
                        }
                    }
                }
            }
        }
    }
    Ok(rows)
}
fn known(
    r: &Request,
    p: &Position,
    last: i32,
    fixings: &mut HashMap<(String, String), Value>,
) -> Result<(), String> {
    match &p.known {
        KnownProduct::IndexedTerm { contract: c } => {
            if c.schema != term_enhancements::SCHEMA
                || c.model_version != term_enhancements::MODEL_VERSION
                || c.position_id != p.position_id
                || c.currency != r.currency
                || c.monetary_unit != "currency_units"
                || c.measure != CashflowMeasure::Contractual
                || c.asof_ordinal != r.as_of_ordinal
                || c.reporting_end_ordinal != last
            {
                return Err("known indexed term identity/version/measure/date mismatch".into());
            }
            term_enhancements::domain(r.as_of_ordinal, c.tail_end_ordinal)?;
            if c.tail_end_ordinal < last {
                return Err("known term tail precedes reporting".into());
            }
            index_metadata(c, fixings)?;
        }
        KnownProduct::CdSequence { contract: c } => sequence_known(r, p, c, last, fixings)?,
        KnownProduct::CdElection { contract: c } => {
            if c.model_version != term_enhancements::MODEL_VERSION
                || c.currency != r.currency
                || c.opening.position_id != p.position_id
                || c.opening.last_accrual_ordinal != r.as_of_ordinal
                || c.opening.accrued_interest < 0.
                || c.policy.penalty_waivers.len() > 256
            {
                return Err(
                    "known CD identity/version/date or negative accrued rights unsupported".into(),
                );
            }
            money(c.opening.accrued_interest)?;
            term_enhancements::domain(r.as_of_ordinal, c.tail_end_ordinal)?;
            if c.tail_end_ordinal < last {
                return Err("known CD tail precedes reporting".into());
            }
        }
    }
    Ok(())
}
struct HistoryContext<'a> {
    graph: &'a Request,
    last: i32,
    fixings: &'a mut HashMap<(String, String), Value>,
    owners: &'a HashSet<&'a str>,
    child_owners: &'a mut HashMap<String, String>,
}
fn history(
    p: &Position,
    path: &History,
    run: &str,
    role: Role,
    context: &mut HistoryContext<'_>,
) -> Result<usize, String> {
    let measure = if role == Role::Physical {
        CashflowMeasure::Physical
    } else {
        CashflowMeasure::Pricing
    };
    match (&p.known, &path.product) {
        (KnownProduct::CdSequence { contract }, ProductHistory::CdSequence { request: q }) => {
            sequence_history(p, contract, q, run, measure, context)
        }
        (KnownProduct::IndexedTerm { contract }, ProductHistory::IndexedTerm { request: q }) => {
            if q.measure != measure
                || q.scenario_id != run
                || q.reporting_end_ordinal != context.last
                || indexed_key(q)? != indexed_key(contract)?
            {
                return Err(
                    "indexed term history changes known terms or measure/run/report".into(),
                );
            }
            index_metadata(q, context.fixings)
        }
        (KnownProduct::CdElection { contract: c }, ProductHistory::CdElection { request: q }) => {
            if q.schema != term_enhancements::SCHEMA
                || q.model_version != c.model_version
                || q.measure != measure
                || q.scenario_id != run
                || q.currency != c.currency
                || q.monetary_unit != "currency_units"
                || q.direction != c.direction
                || q.tail_end_ordinal != c.tail_end_ordinal
                || q.election_ordinal < context.graph.as_of_ordinal
                || q.reporting_end_ordinal != context.last.max(q.election_ordinal)
                || canonical(&q.opening)? != canonical(&c.opening)?
                || canonical(&q.policy)? != canonical(&c.policy)?
            {
                return Err("CD history changes known terms or explicit measure/run/election/report handoff".into());
            }
            if q.policy.penalty_waivers.len() > 256 {
                return Err("term CD waiver admission exceeded".into());
            }
            if let Election::Maturity {
                renewal, switching, ..
            } = &q.election
            {
                let mut children = HashSet::new();
                for new in renewal.iter().chain(switching) {
                    label(&new.position_id)?;
                    if context.owners.contains(new.position_id.as_str())
                        || !children.insert(&new.position_id)
                    {
                        return Err(
                            "CD new contract collides with owned existing/future position".into(),
                        );
                    }
                    if let Some(owner) = context.child_owners.get(&new.position_id) {
                        if owner != &p.position_id {
                            return Err("CD future contract assigned to multiple owners".into());
                        }
                    } else {
                        context
                            .child_owners
                            .insert(new.position_id.clone(), p.position_id.clone());
                    }
                }
            }
            Ok(12 + q.policy.penalty_waivers.len())
        }
        _ => Err("term history product differs from known product".into()),
    }
}
pub(super) fn validate(r: &Request) -> Result<usize, String> {
    if !matches!(r.schema.as_str(), SCHEMA | SEQUENCE_SCHEMA)
        || r.identity.randomness_domain != "supplied_paired_histories-1"
    {
        return Err("term graph requires explicit supplied-paired model version".into());
    }
    if r.schema == SEQUENCE_SCHEMA {
        portfolio_compute_control::checkpoint()?;
        if r.original_base.is_none() || r.original_quotes.len() != r.positions.len() {
            return Err("sequence graph needs complete ordered original quote sources".into());
        }
        for (p, q) in r.positions.iter().zip(&r.original_quotes) {
            if q.position_id != p.position_id || !matches!(&p.oas_target, OasTarget::Locked { .. })
            {
                return Err("sequence graph requires ordered original quotes and locked OAS; no calibration".into());
            }
            money(q.signed_cash_price)?;
            let mut metadata = 0;
            term_enhancements::identity(&q.source, &mut metadata)?;
            if matches!(p.known, KnownProduct::CdElection { .. }) {
                return Err(
                    "graph2 requires shared native CD sequence; no single-election fallback".into(),
                );
            }
        }
    } else if r.original_base.is_some()
        || !r.original_quotes.is_empty()
        || r.positions.iter().any(|p| {
            matches!(p.known, KnownProduct::CdSequence { .. })
                || p.histories.iter().any(|set| {
                    [
                        &set.physical,
                        &set.pricing,
                        &set.pricing_down,
                        &set.pricing_up,
                    ]
                    .into_iter()
                    .any(|rows| {
                        rows.iter()
                            .any(|h| matches!(h.product, ProductHistory::CdSequence { .. }))
                    })
                })
        })
    {
        return Err(
            "explicit term-product-graph-2 selection required for sequence/quote capability".into(),
        );
    }
    for v in [
        &r.currency,
        &r.base_scenario_id,
        &r.identity.model_revision,
        &r.identity.input_revision,
        &r.identity.paired_history_source,
        &r.identity.paired_history_revision,
    ] {
        label(v)?;
    }
    validate_ordinal(r.as_of_ordinal)?;
    number(r.bump_bp, 0.001, 1000.)?;
    let h = r
        .reporting_ordinals
        .len()
        .checked_sub(1)
        .ok_or("term report boundaries required")?;
    if !(1..=120).contains(&h)
        || r.positions.is_empty()
        || r.positions.len() > 256
        || r.scenarios.is_empty()
        || r.scenarios.len() > 13
        || r.currency.len() != 3
        || !r.currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err("term graph dimensions/currency exceed admission".into());
    }
    for &d in &r.reporting_ordinals {
        validate_ordinal(d)?;
    }
    if r.reporting_ordinals[0] != r.as_of_ordinal
        || r.reporting_ordinals.windows(2).any(|v| v[0] >= v[1])
    {
        return Err("term reporting dates must start at asof and increase".into());
    }
    let mut scenarios = HashSet::new();
    let mut run_ids = HashSet::new();
    for s in &r.scenarios {
        label(&s.scenario_id)?;
        label(&s.run_id)?;
        number(s.severity, 0., 100.)?;
        if !scenarios.insert(&s.scenario_id)
            || !run_ids.insert(&s.run_id)
            || s.collateral_curve.currency != r.currency
            || !matches!(s.collateral_curve.role, CurveRole::Collateral { .. })
        {
            return Err("term scenario/curve identity mismatch".into());
        }
        s.collateral_curve.validate()?;
    }
    let owners: HashSet<_> = r.positions.iter().map(|p| p.position_id.as_str()).collect();
    if owners.len() != r.positions.len() {
        return Err("duplicate term position".into());
    }
    // Cheap bounded dimensions and streamed JSON admission precede canonical
    // full dependency comparisons and any financial schedule allocation.
    for p in &r.positions {
        if p.histories.len() != r.scenarios.len() {
            return Err("term history scenario coverage mismatch".into());
        }
        for set in &p.histories {
            for paths in [
                &set.physical,
                &set.pricing,
                &set.pricing_down,
                &set.pricing_up,
            ] {
                if paths.is_empty() || paths.len() > 128 {
                    return Err("term path admission exceeded".into());
                }
                for path in paths {
                    if let ProductHistory::CdSequence { request: q } = &path.product {
                        if q.actions.len() > cd_sequence::MAX_ACTIONS
                            || q.policies.len() > cd_sequence::MAX_LIVE
                        {
                            return Err("CD sequence graph action/policy admission exceeded".into());
                        }
                        count_checked(q, cd_sequence::MAX_RAW_BYTES)?;
                    }
                    if let ProductHistory::IndexedTerm { request: q } = &path.product {
                        if q.periods.len()>4096 || q.periods.iter().any(|p|matches!(&p.coupon,Coupon::Indexed{observations,..} if observations.len()>4096)){return Err("term large period/observation admission exceeded".into());}
                    }
                }
            }
        }
    }
    if r.schema == SEQUENCE_SCHEMA {
        count_checked(r, MAX_IDENTITY_BYTES)?;
    } else {
        count(r, MAX_IDENTITY_BYTES)?;
    }
    let mut fixings = HashMap::new();
    let mut child_owners = HashMap::new();
    let mut runs = 0usize;
    let mut rows = 0usize;
    let mut work = 0usize;
    let last = r.reporting_ordinals[h];
    for p in &r.positions {
        label(&p.position_id)?;
        known(r, p, last, &mut fixings)?;
        let mut pairing = if let Some(original) = &r.original_base {
            let owner = original
                .histories
                .iter()
                .find(|o| o.position_id == p.position_id)
                .ok_or("original owner coverage missing")?;
            Some(
                owner
                    .histories
                    .pricing
                    .iter()
                    .map(|h| (&h.path_id, h.weight))
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
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
        for (s, set) in r.scenarios.iter().zip(&p.histories) {
            if set.scenario_id != s.scenario_id {
                return Err("term history order differs from scenarios".into());
            }
            for (role, paths) in [
                (Role::Physical, &set.physical),
                (Role::Pricing, &set.pricing),
                (Role::PricingDown, &set.pricing_down),
                (Role::PricingUp, &set.pricing_up),
            ] {
                let mut ids = HashSet::new();
                let mut sum = 0.;
                for path in paths {
                    label(&path.path_id)?;
                    number(path.weight, 0., 1.)?;
                    if !ids.insert(&path.path_id) {
                        return Err("duplicate term path".into());
                    }
                    sum += path.weight;
                    runs += 1;
                    let n = history(
                        p,
                        path,
                        &s.run_id,
                        role,
                        &mut HistoryContext {
                            graph: r,
                            last,
                            fixings: &mut fixings,
                            owners: &owners,
                            child_owners: &mut child_owners,
                        },
                    )?;
                    rows = rows.checked_add(n).ok_or("term rows overflow")?;
                    let passes = if role == Role::Physical {
                        h + 12
                    } else if role == Role::Pricing {
                        112
                    } else {
                        12
                    };
                    work = work
                        .checked_add(
                            n.checked_mul(passes)
                                .ok_or("term work multiplication overflow")?,
                        )
                        .ok_or("term work overflow")?;
                    if rows > MAX_ROWS || work > MAX_WORK || runs > MAX_RUNS {
                        return Err("term graph aggregate run/row/work admission exceeded before projection".into());
                    }
                }
                if (sum - 1.).abs() > 1e-12 {
                    return Err(
                        "term probabilities must sum to one; no normalization fallback".into(),
                    );
                }
                if role != Role::Physical {
                    let pairs: Vec<_> = paths.iter().map(|p| (&p.path_id, p.weight)).collect();
                    if let Some(ref old) = pairing {
                        if if r.schema == SEQUENCE_SCHEMA {
                            !exact(old, &pairs)?
                        } else {
                            old != &pairs
                        } {
                            return Err("term Q paths require identical ordered IDs/weights across scenarios/bumps".into());
                        }
                    } else {
                        pairing = Some(pairs);
                    }
                }
            }
        }
    }
    if r.schema == SEQUENCE_SCHEMA {
        original_admission(
            r,
            last,
            &owners,
            &mut fixings,
            &mut child_owners,
            &mut rows,
            &mut work,
            &mut runs,
        )?;
        // Charge retained current/original source indexing and actual validated
        // curve nodes in addition to generation/pricing/report passes.
        let nodes = r
            .scenarios
            .iter()
            .map(|s| s.collateral_curve.times_years.len())
            .sum::<usize>()
            .checked_add(
                r.original_base
                    .as_ref()
                    .ok_or("original base missing")?
                    .scenario
                    .collateral_curve
                    .times_years
                    .len(),
            )
            .ok_or("curve node work overflow")?;
        work = work
            .checked_add(
                runs.checked_mul(16)
                    .ok_or("source indexing work overflow")?,
            )
            .and_then(|n| nodes.checked_mul(2).and_then(|x| n.checked_add(x)))
            .ok_or("index/curve work overflow")?;
        if work > MAX_WORK {
            return Err(
                "term graph joined source/index/curve work exceeded before projection".into(),
            );
        }
        let input = count_checked(r, MAX_IDENTITY_BYTES)?;
        let conservative = input
            .checked_mul(12)
            .and_then(|n| rows.checked_mul(32_768).and_then(|x| n.checked_add(x)))
            .and_then(|n| work.checked_mul(8).and_then(|x| n.checked_add(x)))
            .ok_or("sequence graph aggregate capacity overflow")?;
        if conservative > MAX_RETAINED_BYTES {
            return Err(
                "sequence graph retained+source+transient capacity exceeded before projection"
                    .into(),
            );
        }
        sequence_used_universe(r)?;
    }
    r.scenarios
        .iter()
        .position(|s| s.scenario_id == r.base_scenario_id)
        .ok_or("missing term base scenario".into())
}
pub(super) fn base_key(r: &Request, base: usize) -> Result<Vec<u8>, String> {
    if r.schema == SEQUENCE_SCHEMA {
        let known: Result<Vec<_>, String> = r
            .positions
            .iter()
            .map(|p| Ok((&p.position_id, base_known(&p.known)?, &p.oas_target)))
            .collect();
        // Complete immutable original receipt, including report/metadata spelling,
        // is pinned. A report edit cannot reuse cached original source DTOs silently.
        let original = canonical(r.original_base.as_ref().ok_or("original base missing")?)?;
        return bytes(&(
            &r.schema,
            &r.identity.model_revision,
            &r.identity.paired_history_source,
            &r.identity.paired_history_revision,
            &r.identity.randomness_domain,
            r.as_of_ordinal,
            &r.currency,
            r.cash_timing,
            known?,
            original,
            &r.original_quotes,
        ));
    }
    let positions: Result<Vec<_>, String> = r
        .positions
        .iter()
        .map(|p| {
            let q: Result<Vec<_>, String> = p.histories[base]
                .pricing
                .iter()
                .map(|h| Ok((&h.path_id, h.weight, base_history(&h.product)?)))
                .collect();
            Ok((&p.position_id, base_known(&p.known)?, &p.oas_target, q?))
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
        &r.scenarios[base],
        r.cash_timing,
        positions?,
    ))
}
pub(super) fn source_rows(p: &SourceProjection) -> usize {
    match p {
        SourceProjection::CdSequence { result: r } => {
            r.schedule.events.len()
                + r.live_before_terminal_settlement.len()
                + r.action_receipts
                    .iter()
                    .map(|a| {
                        a.native.settlement_schedule.events.len()
                            + a.native.remaining_schedule.events.len()
                            + a.native.new_contracts.len()
                            + 2
                    })
                    .sum::<usize>()
        }
        SourceProjection::IndexedTerm { result: r } => {
            r.schedule.events.len()
                + r.audit.len()
                + r.audit
                    .iter()
                    .map(|a| a.selected_rate_sources.len())
                    .sum::<usize>()
        }
        SourceProjection::CdElection { result: r } => {
            r.settlement_schedule.events.len()
                + r.remaining_schedule.events.len()
                + r.new_contracts.len()
                + 1
        }
    }
}
pub(super) fn retained(r: &Request, out: &ResultData, key: usize) -> Result<(), String> {
    let input = if r.schema == SEQUENCE_SCHEMA {
        count_checked(r, MAX_IDENTITY_BYTES)?
    } else {
        count(r, MAX_IDENTITY_BYTES)?
    };
    let output = if r.schema == SEQUENCE_SCHEMA {
        count_checked(out, MAX_RETAINED_BYTES)?
    } else {
        count(out, MAX_RETAINED_BYTES)?
    };
    if input
        .saturating_add(output)
        .saturating_mul(4)
        .saturating_add(key)
        > MAX_RETAINED_BYTES
    {
        return Err("term graph retained byte admission exceeded".into());
    }
    Ok(())
}

include!("sequence_admission.rs");
