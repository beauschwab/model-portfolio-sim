//! Independent frozen pre-refactor clone/JSON raw-prefix oracle (2026-10-29).
//! Frozen baseline: 43ec50877a9dd31e0356f809290048f3b2e04c3a.
//! This module validates raw comparison semantics, not admission or publication.
use super::{candidate_holdings, candidates, cut_mixed};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{
    commercial_continuation as continuation, commercial_fee_source::adoption as fee,
    commercial_graph as graph, term_enhancements::indexed,
};
use serde::Serialize;
use serde_json::Value;
const MAX_OUTPUT: usize = 32 << 20;
const MAX_RAW: usize = 2 << 20;

fn oracle_same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    checkpoint()?;
    if strict_json::encode(a, MAX_OUTPUT)? != strict_json::encode(b, MAX_OUTPUT)? {
        return Err(
            "candidate holding immutable original anchor or observed prefix changed".into(),
        );
    }
    Ok(())
}
fn oracle_loan_prefix(a: &graph::History, b: &graph::History, cut: i32) -> Result<(), String> {
    oracle_same(&(&a.path_id, a.weight), &(&b.path_id, b.weight))?;
    if cut <= a.source.source.current.asof_ordinal {
        return oracle_same(&a.source, &b.source);
    }
    frozen_continuation_prefix(&continuation::Request {
        schema: continuation::SCHEMA.into(),
        cut_ordinal: cut,
        completed_source: a.source.clone(),
        revised_source: b.source.clone(),
    })?;
    Ok(())
}
fn oracle_fee_prefix(a: &fee::History, b: &fee::History, cut: i32) -> Result<(), String> {
    oracle_same(&a.path_id, &b.path_id)?;
    let mut policy = b.policy.clone();
    policy.events = a.policy.events.clone();
    oracle_same(&a.policy, &policy)?;
    oracle_same(
        &a.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.policy
            .events
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )?;
    oracle_same(
        &a.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
        &b.acquisition_releases
            .iter()
            .filter(|e| e.ordinal <= cut)
            .collect::<Vec<_>>(),
    )
}
/// Retain all original Q/P proofs, model/contract identities, costs, quotes,
/// original yields/spreads and allocation authorities. Only future histories
/// may change; their observed source prefix is checked before generation.
fn oracle_raw_prefix(r: &candidate_holdings::Request) -> Result<(), String> {
    let a = &r.completed_candidate;
    let b = &r.revised_candidate;
    if a.commercial.units.len() != b.commercial.units.len() {
        return Err("candidate holding unit coverage changed".into());
    }
    let mut normalized = b.commercial.clone();
    for ((old, new), folded) in a
        .commercial
        .units
        .iter()
        .zip(&b.commercial.units)
        .zip(&mut normalized.units)
    {
        if old.graph.positions.len() != new.graph.positions.len() {
            return Err("candidate holding position coverage changed".into());
        }
        for ((p, q), folded) in old
            .graph
            .positions
            .iter()
            .zip(&new.graph.positions)
            .zip(&mut folded.graph.positions)
        {
            if p.histories.len() != q.histories.len() {
                return Err("candidate holding scenario coverage changed".into());
            }
            for (x, y) in p.histories.iter().zip(&q.histories) {
                oracle_same(&x.scenario_id, &y.scenario_id)?;
                for (xs, ys) in [
                    (&x.physical, &y.physical),
                    (&x.pricing, &y.pricing),
                    (&x.pricing_down, &y.pricing_down),
                    (&x.pricing_up, &y.pricing_up),
                ] {
                    if xs.len() != ys.len() {
                        return Err("candidate holding path coverage changed".into());
                    }
                    for (x, y) in xs.iter().zip(ys) {
                        oracle_loan_prefix(x, y, r.cut_ordinal)?;
                    }
                }
            }
            folded.histories = p.histories.clone();
            match (&p.fees, &q.fees, &mut folded.fees) {
                (None, None, None) => {}
                (Some(x), Some(y), Some(z)) => {
                    if x.histories.len() != y.histories.len() {
                        return Err("candidate holding fee scenario coverage changed".into());
                    }
                    for (x, y) in x.histories.iter().zip(&y.histories) {
                        oracle_same(&x.scenario_id, &y.scenario_id)?;
                        for (xs, ys) in [
                            (&x.physical, &y.physical),
                            (&x.pricing, &y.pricing),
                            (&x.pricing_down, &y.pricing_down),
                            (&x.pricing_up, &y.pricing_up),
                        ] {
                            if xs.len() != ys.len() {
                                return Err("candidate holding fee path coverage changed".into());
                            }
                            for (x, y) in xs.iter().zip(ys) {
                                oracle_fee_prefix(x, y, r.cut_ordinal)?;
                            }
                        }
                    }
                    z.histories = x.histories.clone();
                }
                _ => return Err("candidate holding fee ownership changed".into()),
            }
        }
    }
    oracle_same(&a.commercial, &normalized)?;
    let mut av = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(b).map_err(|e| e.to_string())?;
    av.as_object_mut()
        .ok_or("candidate holding request object")?
        .remove("commercial");
    bv.as_object_mut()
        .ok_or("candidate holding request object")?
        .remove("commercial");
    oracle_same(&av, &bv)
}

fn oracle_cut_same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    checkpoint()?;
    if strict_json::encode(a, MAX_RAW)? != strict_json::encode(b, MAX_RAW)? {
        return Err(
            "native cut immutable owner/original anchor or observed raw prefix differs".into(),
        );
    }
    Ok(())
}
/// The surviving complete source cannot be replaced by a cut-started contract.
/// Appended sources have actual future acquisition dates and no observed groups.
fn oracle_cut_prefix(r: &cut_mixed::Request) -> Result<usize, String> {
    let a = &r.completed_candidate;
    let b = &r.revised_candidate;
    let n = a.commercial.units.len();
    if b.commercial.units.len() < n || b.allocation.len() < n || b.owners.len() < n {
        return Err("native cut cannot remove surviving units/allocation/owners".into());
    }
    oracle_cut_same(&a.as_of_ordinal, &b.as_of_ordinal)?;
    oracle_cut_same(
        &a.commercial
            .reporting_ordinals
            .iter()
            .filter(|d| **d <= r.cut_ordinal)
            .collect::<Vec<_>>(),
        &b.commercial
            .reporting_ordinals
            .iter()
            .filter(|d| **d <= r.cut_ordinal)
            .collect::<Vec<_>>(),
    )?;
    let mut folded: candidates::Request =
        serde_json::from_value(serde_json::to_value(b).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    folded.commercial.units.truncate(n);
    folded.allocation.truncate(n);
    folded.owners.truncate(n);
    folded.reporting_end_ordinal = a.reporting_end_ordinal;
    folded.commercial.reporting_ordinals = a.commercial.reporting_ordinals.clone();
    for (old, new) in a.commercial.units.iter().zip(&mut folded.commercial.units) {
        oracle_cut_same(
            &old.graph
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= r.cut_ordinal)
                .collect::<Vec<_>>(),
            &new.graph
                .reporting_ordinals
                .iter()
                .filter(|d| **d <= r.cut_ordinal)
                .collect::<Vec<_>>(),
        )?;
        new.graph.reporting_ordinals = old.graph.reporting_ordinals.clone();
    }
    let positions = folded
        .specification
        .get_mut("positions")
        .and_then(Value::as_array_mut)
        .ok_or("native cut placeholder array absent")?;
    if positions.len() < n {
        return Err("native cut surviving placeholder coverage differs".into());
    }
    positions.truncate(n);
    // The ordinary candidate route normalizes its reporting calendar to the
    // complete source tail. Both exact admissions prove the same full horizon.
    if let Some(days) = a.specification.get("horizon_days") {
        folded.specification["horizon_days"] = days.clone();
    }
    let completed: candidates::Request =
        serde_json::from_value(serde_json::to_value(a).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    oracle_raw_prefix(&candidate_holdings::Request {
        schema: candidate_holdings::SCHEMA.into(),
        cut_ordinal: r.cut_ordinal,
        completed_candidate: completed,
        revised_candidate: folded,
    })?;
    for unit in b.commercial.units.iter().skip(n) {
        if unit.graph.as_of_ordinal <= r.cut_ordinal {
            return Err("native cut appended candidate acquisition must follow cut".into());
        }
    }
    Ok(b.commercial.units.len() - n)
}

// Frozen from commit 43ec50877a9dd31e0356f809290048f3b2e04c3a.
fn frozen_source_same<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    checkpoint()?;
    Ok(strict_json::encode(a, 32 << 20)? == strict_json::encode(b, 32 << 20)?)
}
fn frozen_source_require_same<T: Serialize>(a: &T, b: &T) -> Result<(), String> {
    if frozen_source_same(a, b)? {
        Ok(())
    } else {
        Err("commercial completed prefix or original authority changed".into())
    }
}

fn frozen_contract_prefix(
    a: &indexed::ContractPathRequest,
    b: &indexed::ContractPathRequest,
    cut: i32,
) -> Result<(), String> {
    let mut av = serde_json::to_value(a).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(b).map_err(|e| e.to_string())?;
    for v in [&mut av, &mut bv] {
        v.as_object_mut()
            .ok_or("commercial source object required")?
            .remove("periods");
    }
    frozen_source_require_same(&av, &bv)?;
    if a.periods.len() != b.periods.len() {
        return Err("commercial continuation contractual period coverage changed".into());
    }
    for (old, new) in a.periods.iter().zip(&b.periods) {
        checkpoint()?;
        // Dates, entitlement identities and principal plan are retained even for
        // future periods. Only future coupon/fee assumptions can be revised here.
        frozen_source_require_same(
            &(
                old.period_id.as_str(),
                old.start_ordinal,
                old.end_ordinal,
                old.payment_ordinal,
                old.day_count,
            ),
            &(
                new.period_id.as_str(),
                new.start_ordinal,
                new.end_ordinal,
                new.payment_ordinal,
                new.day_count,
            ),
        )?;
        if old.start_ordinal < cut || old.payment_ordinal <= cut {
            frozen_source_require_same(old, new)?;
        }
        let available = |p: &indexed::ContractPeriod| match &p.coupon {
            indexed::Coupon::Indexed { observations, .. } => observations
                .iter()
                .filter(|o| o.fixing_ordinal <= cut)
                .cloned()
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        frozen_source_require_same(&available(old), &available(new))?;
    }
    Ok(())
}

fn frozen_source_raw_prefix(r: &continuation::Request, cut: f64) -> Result<(), String> {
    let a = &r.completed_source;
    let b = &r.revised_source;
    frozen_source_require_same(&a.source.original, &b.source.original)?;
    frozen_source_require_same(&a.source.acquisition, &b.source.acquisition)?;
    frozen_source_require_same(&a.source.transfer, &b.source.transfer)?;
    frozen_source_require_same(&a.source.policy, &b.source.policy)?;
    frozen_source_require_same(&a.factor_policy, &b.factor_policy)?;
    frozen_source_require_same(&a.performing_draw_tranches, &b.performing_draw_tranches)?;
    frozen_source_require_same(&a.prepayment_terms, &b.prepayment_terms)?;
    frozen_source_require_same(&a.maturity_basis_policy, &b.maturity_basis_policy)?;
    frozen_source_require_same(&a.reviewed_credit_actions, &b.reviewed_credit_actions)?;
    frozen_contract_prefix(&a.source.current, &b.source.current, r.cut_ordinal)?;
    let mut previous = 0.;
    if a.credit.intervals.len() != b.credit.intervals.len() {
        return Err("commercial continuation credit interval coverage changed".into());
    }
    for (i, (old, new)) in a
        .credit
        .intervals
        .iter()
        .zip(&b.credit.intervals)
        .enumerate()
    {
        frozen_source_require_same(&old.end_year, &new.end_year)?;
        if previous < cut {
            frozen_source_require_same(old, new)?;
            frozen_source_require_same(
                &a.accounting
                    .recognition
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
                &b.accounting
                    .recognition
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
            )?;
            frozen_source_require_same(
                &a.accounting_rates
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
                &b.accounting_rates
                    .iter()
                    .filter(|p| p.interval_index == i)
                    .collect::<Vec<_>>(),
            )?;
        }
        previous = old.end_year;
    }
    let mut av = serde_json::to_value(&a.credit).map_err(|e| e.to_string())?;
    let mut bv = serde_json::to_value(&b.credit).map_err(|e| e.to_string())?;
    for v in [&mut av, &mut bv] {
        v.as_object_mut()
            .ok_or("credit object required")?
            .remove("intervals");
    }
    frozen_source_require_same(&av, &bv)?;
    // Accounting identity/application/disposition are retained. Prospective
    // recognition/rate changes are permitted only in wholly future intervals.
    let mut ap = a.accounting.clone();
    let mut bp = b.accounting.clone();
    ap.recognition.clear();
    bp.recognition.clear();
    frozen_source_require_same(&ap, &bp)?;
    let past: Vec<_> = a
        .source
        .current_accounting_rate_reviews
        .iter()
        .filter(|p| p.reset_ordinal < r.cut_ordinal)
        .collect();
    let now: Vec<_> = b
        .source
        .current_accounting_rate_reviews
        .iter()
        .filter(|p| p.reset_ordinal < r.cut_ordinal)
        .collect();
    frozen_source_require_same(&past, &now)
}

fn frozen_continuation_prefix(r: &continuation::Request) -> Result<f64, String> {
    checkpoint()?;
    if r.schema != continuation::SCHEMA {
        return Err("explicit commercial holding continuation schema required".into());
    }
    let origin = r.completed_source.source.current.asof_ordinal;
    let elapsed = r
        .cut_ordinal
        .checked_sub(origin)
        .ok_or("commercial cut calendar overflow")?;
    if elapsed <= 0 {
        return Err("commercial holding cut must follow original acquisition".into());
    }
    let cut = f64::from(elapsed) / 365.;
    frozen_source_raw_prefix(r, cut)?;
    Ok(cut)
}

const HOLDING: &str =
    include_str!("../../../../../examples/models/commercial-candidate-holding-journal-1.json");
const PUBLIC_CUT: &str =
    include_str!("../../../../../examples/models/commercial-cut-mixed-journal-1.json");
const SHORT_CUT: &str = include_str!(
    "../../../../portfolio-risk/tests/fixtures/commercial-short-secondary-cut-refusal.json"
);

#[derive(Clone, Copy, Debug)]
enum Mode {
    Holding,
    Cut,
}

fn compare(value: &Value, mode: Mode, label: &str) -> Option<bool> {
    match mode {
        Mode::Holding => {
            let request: candidate_holdings::Request =
                serde_json::from_value(value.clone()).ok()?;
            let expected = oracle_raw_prefix(&request);
            let actual = candidate_holdings::raw_prefix_borrowed(
                &request.completed_candidate,
                &request.revised_candidate,
                request.cut_ordinal,
                false,
            );
            assert_eq!(actual, expected, "borrowed holding mismatch at {label}");
            assert_eq!(
                candidate_holdings::raw_prefix(&request),
                expected,
                "holding wrapper mismatch at {label}"
            );
            Some(expected.is_ok())
        }
        Mode::Cut => {
            let request: cut_mixed::Request = serde_json::from_value(value.clone()).ok()?;
            let expected = oracle_cut_prefix(&request);
            let actual = cut_mixed::validate_raw_prefix(&request);
            assert_eq!(actual, expected, "cut wrapper mismatch at {label}");
            Some(expected.is_ok())
        }
    }
}

// Retain one concrete address for each declared JSON shape, while preserving
// named physical/pricing/down/up/original/current ownership separately. Array
// index normalization only avoids repeating identical declarations in every
// scenario; explicit controls below additionally visit every actual source.
fn collect_shapes(
    value: &Value,
    address: &str,
    shape: &str,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    out.entry(shape.into()).or_insert_with(|| address.into());
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_shapes(
                    child,
                    &format!("{address}/{escaped}"),
                    &format!("{shape}/{escaped}"),
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_shapes(
                    child,
                    &format!("{address}/{index}"),
                    &format!("{shape}/*"),
                    out,
                );
            }
        }
        _ => {}
    }
}

fn scalar_mutation(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(format!("{text}-changed"))),
        Value::Bool(flag) => Some(Value::Bool(!flag)),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                integer.checked_add(1).map(Value::from)
            } else if let Some(integer) = number.as_u64() {
                integer.checked_sub(1).map(Value::from)
            } else {
                number.as_f64().map(|x| Value::from(x + 0.125))
            }
        }
        Value::Null => Some(Value::Bool(true)),
        _ => None,
    }
}

fn mutation_matrix(source: &str, mode: Mode) {
    let baseline: Value = serde_json::from_str(source).unwrap();
    assert_eq!(compare(&baseline, mode, "unmodified fixture"), Some(true));
    let mut shapes = std::collections::BTreeMap::new();
    collect_shapes(
        &baseline["revised_candidate"],
        "/revised_candidate",
        "",
        &mut shapes,
    );
    let mut executed = 0;
    let mut accepted = 0;
    let mut refused = 0;
    for (shape, address) in &shapes {
        let original = baseline.pointer(address).unwrap();
        let mut alternatives = Vec::new();
        if let Some(value) = scalar_mutation(original) {
            alternatives.push(("scalar", value));
        }
        match original {
            Value::Array(items) => {
                if !items.is_empty() {
                    let mut removed = items.clone();
                    removed.remove(0);
                    alternatives.push(("remove-array-first", Value::Array(removed)));
                    let mut duplicated = items.clone();
                    duplicated.push(items[0].clone());
                    alternatives.push(("duplicate-array-first", Value::Array(duplicated)));
                }
                if items.len() > 1 {
                    let mut reversed = items.clone();
                    reversed.reverse();
                    alternatives.push(("reverse-array", Value::Array(reversed)));
                }
            }
            Value::Object(fields) => {
                // Optional fields/defaults and free-form ledger objects must be
                // compared as their deserialized DTO, just as before refactoring.
                for key in fields.keys() {
                    let mut removed = fields.clone();
                    removed.remove(key);
                    alternatives.push(("remove-object-field", Value::Object(removed)));
                }
                alternatives.push(("optional-null", Value::Null));
            }
            _ => {}
        }
        for (kind, alternative) in alternatives {
            let mut changed = baseline.clone();
            *changed.pointer_mut(address).unwrap() = alternative;
            if let Some(ok) = compare(&changed, mode, &format!("{shape}: {kind}")) {
                executed += 1;
                if ok {
                    accepted += 1;
                } else {
                    refused += 1;
                }
            }
        }
    }
    assert!(
        executed > 400,
        "insufficient typed mutation coverage: {executed}"
    );
    assert!(refused > 100, "missing immutable refusals: {refused}");
    assert!(
        accepted > 0,
        "missing allowed future/default normalization cases"
    );
    eprintln!(
        "{mode:?}: {} declaration shapes, {executed} supported DTO mutations, \
               {accepted} accepted and {refused} refused",
        shapes.len()
    );
}

#[test]
fn borrowed_holding_matches_frozen_clone_oracle_for_every_present_declaration() {
    mutation_matrix(HOLDING, Mode::Holding);
}

#[test]
fn borrowed_public_cut_matches_frozen_folding_oracle_for_every_present_declaration() {
    mutation_matrix(PUBLIC_CUT, Mode::Cut);
}

#[test]
fn borrowed_secondary_cut_matches_frozen_folding_oracle_for_every_present_declaration() {
    mutation_matrix(SHORT_CUT, Mode::Cut);
}

// Visit every actual loan/fee source instead of selecting one declaration
// shape. This includes original quotes, all P/Q bump paths, zero weights,
// scenario order and any default/prepay policy in the fixture.
fn mutate_all_named(value: &mut Value, name: &str, update: fn(&mut Value)) -> usize {
    let mut count = 0;
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                if key == name {
                    update(child);
                    count += 1;
                }
                count += mutate_all_named(child, name, update);
            }
        }
        Value::Array(items) => {
            for child in items {
                count += mutate_all_named(child, name, update);
            }
        }
        _ => {}
    }
    count
}

#[test]
fn borrowed_zero_weight_owners_still_retain_anchors_and_observed_prefixes() {
    for (source, mode) in [(HOLDING, Mode::Holding), (SHORT_CUT, Mode::Cut)] {
        let mut base: Value = serde_json::from_str(source).unwrap();
        let count = mutate_all_named(&mut base, "weight", |x| *x = Value::from(0.0));
        assert!(count >= 8);
        assert_eq!(compare(&base, mode, "all source weights zero"), Some(true));
        for field in [
            "annual_rate",
            "original_signed_cash_quote",
            "original_signed_fee_quote",
            "original_effective_yield",
            "original_discount_spread",
            "amount",
        ] {
            let mut changed = base.clone();
            let count = mutate_all_named(&mut changed["revised_candidate"], field, |x| {
                if let Some(new) = scalar_mutation(x) {
                    *x = new;
                }
            });
            if count > 0 {
                assert_eq!(
                    compare(&changed, mode, field),
                    Some(false),
                    "zero participation cannot waive {field}"
                );
            }
        }
    }
}

#[test]
fn borrowed_cut_boundaries_match_frozen_source_and_fee_clocks() {
    for (source, mode) in [(HOLDING, Mode::Holding), (SHORT_CUT, Mode::Cut)] {
        let base: Value = serde_json::from_str(source).unwrap();
        let origin = base["completed_candidate"]["as_of_ordinal"]
            .as_i64()
            .unwrap();
        let cut = base["cut_ordinal"].as_i64().unwrap();
        for date in [origin - 1, origin, origin + 1, cut, cut + 1, cut + 30] {
            let mut changed = base.clone();
            changed["cut_ordinal"] = Value::from(date);
            assert!(compare(&changed, mode, &format!("cut {date}")).is_some());
        }
    }
}

#[test]
fn borrowed_appended_acquisition_rejects_at_cut_and_retains_future_authority() {
    for source in [PUBLIC_CUT, SHORT_CUT] {
        let base: Value = serde_json::from_str(source).unwrap();
        let n = base["completed_candidate"]["commercial"]["units"]
            .as_array()
            .unwrap()
            .len();
        let cut = base["cut_ordinal"].as_i64().unwrap();
        let units = base["revised_candidate"]["commercial"]["units"]
            .as_array()
            .unwrap();
        assert!(units.len() > n);
        for date in [cut - 1, cut, cut + 1] {
            let mut changed = base.clone();
            changed["revised_candidate"]["commercial"]["units"][n]["graph"]["as_of_ordinal"] =
                Value::from(date);
            assert_eq!(
                compare(&changed, Mode::Cut, "appended acquisition boundary"),
                Some(date > cut)
            );
        }
    }
}

#[test]
fn borrowed_and_frozen_comparisons_obey_active_cancellation() {
    let holding: candidate_holdings::Request = serde_json::from_str(HOLDING).unwrap();
    let cut: cut_mixed::Request = serde_json::from_str(SHORT_CUT).unwrap();
    let control = portfolio_compute_control::Control::new(30_000).unwrap();
    let mut results = None;
    let outer = portfolio_compute_control::scope(Some(control.clone()), || {
        control.cancel();
        results = Some((
            oracle_raw_prefix(&holding),
            candidate_holdings::raw_prefix_borrowed(
                &holding.completed_candidate,
                &holding.revised_candidate,
                holding.cut_ordinal,
                false,
            ),
            oracle_cut_prefix(&cut),
            cut_mixed::validate_raw_prefix(&cut),
        ));
        Ok(())
    });
    assert_eq!(outer.unwrap_err(), "COMPUTE_CANCELLED");
    let (old_holding, new_holding, old_cut, new_cut) = results.unwrap();
    assert_eq!(new_holding, old_holding);
    assert_eq!(new_cut, old_cut);
    assert_eq!(new_holding.unwrap_err(), "COMPUTE_CANCELLED");
    assert_eq!(new_cut.unwrap_err(), "COMPUTE_CANCELLED");
}

#[test]
fn borrowed_future_fee_cash_and_release_changes_preserve_observed_events() {
    let baseline: Value = serde_json::from_str(HOLDING).unwrap();
    let cut = baseline["cut_ordinal"].as_i64().unwrap();
    let mut policy_cases = 0;
    let mut release_cases = 0;
    let mut covered_lanes = std::collections::BTreeSet::new();
    let units = baseline["revised_candidate"]["commercial"]["units"]
        .as_array()
        .unwrap();
    for (u, unit) in units.iter().enumerate() {
        for (p, position) in unit["graph"]["positions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let Some(histories) = position["fees"]["histories"].as_array() else {
                continue;
            };
            for (s, scenario) in histories.iter().enumerate() {
                for lane in ["physical", "pricing", "pricing_down", "pricing_up"] {
                    for (h, history) in scenario[lane].as_array().unwrap().iter().enumerate() {
                        let prefix = format!("/revised_candidate/commercial/units/{u}/graph/positions/{p}/fees/histories/{s}/{lane}/{h}");
                        for suffix in ["policy/events", "acquisition_releases"] {
                            let rows: &[Value] = match history.pointer(&format!("/{suffix}")) {
                                Some(Value::Array(rows)) => rows,
                                // Pricing histories intentionally omit the empty
                                // default acquisition-release list; only the
                                // physical owner carries acquired-cost releases.
                                None if suffix == "acquisition_releases" => &[],
                                _ => {
                                    panic!("required fee-event array missing at {prefix}/{suffix}")
                                }
                            };
                            for (index, row) in rows.iter().enumerate() {
                                if row["ordinal"].as_i64().unwrap() <= cut {
                                    continue;
                                }
                                let mut changed = baseline.clone();
                                let address = format!("{prefix}/{suffix}/{index}/amount");
                                let amount = changed.pointer_mut(&address).unwrap();
                                *amount = scalar_mutation(amount).unwrap();
                                assert_eq!(compare(&changed, Mode::Holding, &address), Some(true));
                                let ordinal = format!("{prefix}/{suffix}/{index}/ordinal");
                                *changed.pointer_mut(&ordinal).unwrap() = Value::from(cut);
                                assert_eq!(compare(&changed, Mode::Holding, &ordinal), Some(false));
                                if suffix == "policy/events" {
                                    policy_cases += 1;
                                    covered_lanes.insert(lane);
                                } else {
                                    release_cases += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        policy_cases >= 8,
        "all four fee lanes in both scenarios must be exercised"
    );
    assert_eq!(covered_lanes.len(), 4);
    assert!(
        release_cases >= 2,
        "both physical acquired-cost release clocks must be exercised"
    );
}

#[test]
fn borrowed_comparison_preserves_typed_defaults_and_json_number_representations() {
    for (source, mode) in [(HOLDING, Mode::Holding), (SHORT_CUT, Mode::Cut)] {
        let baseline: Value = serde_json::from_str(source).unwrap();
        // Typed f64 fields normalize an integer JSON literal through serde.
        let mut integer = baseline.clone();
        integer["revised_candidate"]["owners"][0]["pre_purchase_principal"] = Value::from(0);
        assert_eq!(
            compare(&integer, mode, "typed integer to f64 normalization"),
            Some(true)
        );
        // Signed zero remains significant under the old canonical encoder.
        let mut signed_zero = baseline.clone();
        signed_zero["revised_candidate"]["owners"][0]["pre_purchase_principal"] = Value::from(-0.0);
        assert_eq!(
            compare(&signed_zero, mode, "typed signed zero"),
            Some(false)
        );
        // The free-form specification is a Value, so its numeric representation
        // must be treated exactly as by the frozen JSON round trip.
        for number in [Value::from(0), Value::from(0.0), Value::from(-0.0)] {
            let mut changed = baseline.clone();
            changed["revised_candidate"]["specification"]["positions"][0]["balance"] = number;
            assert!(compare(&changed, mode, "free-form numeric representation").is_some());
        }
    }
}
