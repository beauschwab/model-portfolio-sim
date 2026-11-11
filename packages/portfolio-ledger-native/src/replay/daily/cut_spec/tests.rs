use super::*;
use serde_json::{json, Value};

fn spec() -> Spec {
    serde_json::from_value(json!({
        "horizon_days": 30,
        "accounts": [{"id":"a", "entity":"e", "currency":"USD", "cash":100.0, "equity":100.0}],
        "positions": [{"id":"p", "account":"a", "kind":"loan", "balance":80.0}],
        "netting_sets": [{"id":"n", "account":"a", "counterparty":"c"}],
        "policies": [{"id":"r", "account":"a", "kind":"funding", "trigger_cash":1.0, "limit":5.0}],
        "scenarios": [{"name":"stress"}],
        "reverse_severities": [0.5],
        "cashflows": [{"position":"p", "day":3, "principal":2.0}],
        "ruleset":"research"
    }))
    .unwrap()
}

#[test]
fn specification_clone_census_matches_actual_capacity_and_shared_position_ownership() {
    use std::mem::size_of;
    let mut original = spec();
    original.accounts[0].id = "account 雪😀".into();
    original.accounts[0].id.reserve(10000);
    original.accounts.reserve(1000);
    original.positions.reserve(1000);
    original.ruleset.reserve(10000);
    let copy = original.clone();
    let mut actual = size_of::<Spec>()
        + copy.accounts.capacity() * size_of::<Account>()
        + copy.positions.capacity() * size_of::<Position>()
        + copy.netting_sets.capacity() * size_of::<NettingSet>()
        + copy.policies.capacity() * size_of::<Policy>()
        + copy.scenarios.capacity() * size_of::<Scenario>()
        + copy.reverse_severities.capacity() * size_of::<f64>()
        + copy.cashflows.capacity() * size_of::<Cashflow>()
        + copy.ruleset.capacity();
    for a in &copy.accounts {
        actual += [
            &a.id,
            &a.entity,
            &a.currency,
            &a.lcr_applicability,
            &a.nsfr_applicability,
            &a.htm_accounting,
        ]
        .iter()
        .map(|s| s.capacity())
        .sum::<usize>();
    }
    for n in &copy.netting_sets {
        actual += [&n.id, &n.account, &n.counterparty]
            .iter()
            .map(|s| s.capacity())
            .sum::<usize>();
    }
    for p in &copy.policies {
        actual += [
            &p.id,
            &p.account,
            &p.kind,
            &p.position,
            &p.destination,
            &p.sale_reason,
        ]
        .iter()
        .map(|s| s.capacity())
        .sum::<usize>();
    }
    actual += copy
        .scenarios
        .iter()
        .map(|s| s.name.capacity())
        .sum::<usize>();
    actual += copy
        .cashflows
        .iter()
        .map(|f| f.position.capacity() + f.scenario.capacity())
        .sum::<usize>();
    // SharedText clones increment their existing Arc; they allocate no payload.
    assert!(std::ptr::eq(
        original.positions[0].id.as_str(),
        copy.positions[0].id.as_str()
    ));
    let work = cut_shape::with_budget(10000, || {
        assert_eq!(specification_clone_bytes(&original)?, actual);
        assert_eq!(original.positions[0].cloned_heap()?, 0);
        ticket()
    })
    .unwrap();
    assert!(work > 100);
    assert!(cut_shape::with_budget(1, || specification_clone_bytes(&original)).is_err());
    assert!(cut_shape::spent().is_none());
}

#[test]
fn clone_census_includes_long_unescaped_payload_and_ignores_numeric_values() {
    let mut a = spec();
    let before = specification_clone_bytes(&a).unwrap();
    let old = a.accounts[0].entity.len();
    a.accounts[0].entity = "雪\\\"\n".repeat(2000);
    let delta = a.accounts[0].entity.len() - old;
    a.accounts[0].cash = -0.;
    a.positions[0].balance = f64::MAX;
    assert_eq!(specification_clone_bytes(&a).unwrap(), before + delta);
    let empty: Spec = serde_json::from_value(json!({})).unwrap();
    assert_eq!(
        specification_clone_bytes(&empty).unwrap(),
        size_of::<Spec>() + empty.ruleset.len()
    );
}
fn check_layout<T: Typed>(value: &T) {
    let native = serde_json::to_value(value).unwrap();
    let layout = value.layout_value().unwrap();
    // Encoding Values preserves number categories and zero signs, unlike f64 ==.
    assert_eq!(
        serde_json::to_vec(&native).unwrap(),
        serde_json::to_vec(&layout).unwrap()
    );
    cut_shape::with_budget(10_000_000, || {
        let before = ticket()?;
        let shape = census(value)?;
        assert_eq!(ticket()? - before, shape.census_work_upper()?);
        let oracle = cut_shape::shape(value)?;
        assert!(shape.encoded_bytes_upper >= oracle.encoded_bytes_upper);
        assert_eq!(add(shape.cells, shape.emitted_fields)?, oracle.cells_upper);
        Ok(())
    })
    .unwrap();
}
fn inventory(old: &Spec, new: &Spec) -> R<TypedCutSpecificationInventory> {
    cut_shape::with_budget(10_000_000, || {
        commercial_cut_typed_specification_inventory(old, new, 0)
    })
}

#[test]
fn complete_native_layout_defaults_and_empty_containers_match_serde_oracle() {
    let s = spec();
    check_layout(&s);
    check_layout(&view(&s, &s.positions));
    check_layout(&s.accounts[0]);
    check_layout(&s.positions[0]);
    check_layout(&s.netting_sets[0]);
    check_layout(&s.policies[0]);
    check_layout(&s.scenarios[0]);
    check_layout(&s.cashflows[0]);
    let empty: Spec = serde_json::from_value(json!({})).unwrap();
    check_layout(&empty);
    let report = inventory(&empty, &empty).unwrap();
    assert!(report.immutable_specification_verified);
    assert!(!report.compiled_ownership_verified);
    assert_eq!(report.appended_positions, 0);
    let canonical_view = serde_json::to_value(view(&s, &s.positions)).unwrap();
    assert_eq!(
        serde_json::to_vec(&canonical_view).unwrap(),
        serde_json::to_vec(&serde_json::to_value(&s).unwrap()).unwrap()
    );
}

fn leaf_paths(value: &Value, path: String, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                leaf_paths(value, format!("{path}/{key}"), out);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                leaf_paths(value, format!("{path}/{index}"), out);
            }
        }
        _ => out.push(path),
    }
}
#[test]
fn every_declared_raw_leaf_mutation_is_observed() {
    let s = spec();
    let base = serde_json::to_value(&s).unwrap();
    let mut paths = vec![];
    leaf_paths(&base, String::new(), &mut paths);
    assert!(
        paths.len() >= 110,
        "full DTO family must remain in the matrix"
    );
    for path in paths {
        let mut changed = base.clone();
        let value = changed.pointer_mut(&path).unwrap();
        *value = match value {
            Value::String(text) => Value::String(format!("{text}x")),
            Value::Bool(b) => Value::Bool(!*b),
            Value::Number(n) if n.is_f64() => json!(n.as_f64().unwrap() + 0.125),
            Value::Number(n) => json!(n.as_u64().unwrap() + 1),
            _ => panic!("unexpected leaf at {path}"),
        };
        let changed: Spec = serde_json::from_value(changed).unwrap();
        assert!(inventory(&s, &changed).is_err(), "unobserved leaf: {path}");
        check_layout(&changed);
    }
}

#[test]
fn every_float_leaf_preserves_negative_zero_and_adjacent_bits() {
    let mut base = serde_json::to_value(spec()).unwrap();
    let mut paths = vec![];
    leaf_paths(&base, String::new(), &mut paths);
    let float_paths: Vec<_> = paths
        .into_iter()
        .filter(|p| base.pointer(p).unwrap().is_f64())
        .collect();
    assert_eq!(float_paths.len(), 74);
    for path in &float_paths {
        *base.pointer_mut(path).unwrap() = json!(0.0);
    }
    let zero: Spec = serde_json::from_value(base.clone()).unwrap();
    for path in float_paths {
        for bits in [(-0.0f64).to_bits(), 1] {
            let mut changed = base.clone();
            *changed.pointer_mut(&path).unwrap() = json!(f64::from_bits(bits));
            let changed: Spec = serde_json::from_value(changed).unwrap();
            assert!(
                inventory(&zero, &changed).is_err(),
                "floating bit ignored: {path}/{bits}"
            );
        }
    }
}

#[test]
fn escaped_long_strings_and_numeric_extremes_match_layout() {
    let mut s = spec();
    s.ruleset = "\"\\\n\r\t\u{0000}\u{0001}非😀".repeat(2048);
    s.positions[0].id = s.ruleset.as_str().into();
    s.horizon_days = usize::MAX;
    s.positions[0].payment_interval_days = usize::MAX;
    s.accounts[0].cash = f64::MAX;
    s.netting_sets[0].fair_value = -f64::MAX;
    s.positions[0].rate = f64::MIN_POSITIVE;
    s.cashflows[0].cash_interest = f64::from_bits(1);
    s.scenarios[0].rate_shift = -0.0;
    check_layout(&s);
    assert!(inventory(&s, &s).unwrap().immutable_specification_verified);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad = spec();
        bad.accounts[0].cash = value;
        assert!(inventory(&bad, &bad).is_err());
        cut_shape::with_budget(10, || {
            let before = ticket()?;
            assert!(value.same(&value).is_err());
            assert_eq!(ticket()? - before, 1);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn ordered_containers_and_add_remove_are_not_normalized() {
    let s = spec();
    let base = serde_json::to_value(&s).unwrap();
    for key in [
        "accounts",
        "positions",
        "netting_sets",
        "policies",
        "scenarios",
        "reverse_severities",
        "cashflows",
    ] {
        let mut removed = base.clone();
        removed[key].as_array_mut().unwrap().clear();
        let removed: Spec = serde_json::from_value(removed).unwrap();
        assert!(inventory(&s, &removed).is_err(), "removal: {key}");
        let mut doubled = base.clone();
        let mut second = doubled[key][0].clone();
        match &mut second {
            Value::Object(fields) => {
                let field = fields.keys().next().unwrap().clone();
                if fields[&field].is_string() {
                    fields.insert(field, json!("different"));
                } else {
                    fields.insert(field, json!(1.25));
                }
            }
            _ => second = json!(0.75),
        }
        doubled[key].as_array_mut().unwrap().push(second);
        let original: Spec = serde_json::from_value(doubled.clone()).unwrap();
        doubled[key].as_array_mut().unwrap().swap(0, 1);
        let reordered: Spec = serde_json::from_value(doubled).unwrap();
        assert!(inventory(&original, &reordered).is_err(), "ordering: {key}");
    }
}

#[test]
fn appended_raw_positions_have_no_compiled_owner_or_clock_authority() {
    let s = spec();
    let mut revised = s.clone();
    let mut p = s.positions[0].clone();
    p.id = "future".into();
    p.balance = -0.0;
    p.start_day = 0; // Source origin clock is deliberately not inferred here.
    revised.positions.push(p);
    let report = inventory(&s, &revised).unwrap();
    assert_eq!(report.appended_positions, 1);
    assert!(report.immutable_specification_verified);
    assert!(!report.compiled_ownership_verified);
    for field in [
        "balance",
        "book_adjustment",
        "opening_accrued",
        "allowance",
        "commitment",
    ] {
        let mut changed = serde_json::to_value(&revised).unwrap();
        changed["positions"][1][field] = json!(1.0);
        assert!(
            inventory(&s, &serde_json::from_value(changed).unwrap()).is_err(),
            "opening field: {field}"
        );
    }
    let mut bad = revised.clone();
    bad.positions[1].classification = "afs".into();
    assert!(inventory(&s, &bad).is_err());
    bad = revised;
    bad.positions[1].collateral_position = "p".into();
    assert!(inventory(&s, &bad).is_err());
    assert!(
        cut_shape::with_budget(100_000, || commercial_cut_typed_specification_inventory(
            &s,
            &s,
            s.horizon_days
        ))
        .is_err()
    );
}

#[test]
fn real_ticket_precharges_strings_comparisons_and_restores_after_failure() {
    let s = spec();
    assert!(commercial_cut_typed_specification_inventory(&s, &s, 0).is_err());
    assert!(
        cut_shape::with_budget(1, || commercial_cut_typed_specification_inventory(
            &s, &s, 0
        ))
        .is_err()
    );
    assert!(cut_shape::spent().is_none());
    let long = "x".repeat(10_000);
    cut_shape::with_budget(2, || {
        let mut counter = Counter::default();
        assert!(long.visit(&mut counter).is_err());
        assert_eq!(ticket()?, 1); // Only string node paid; no string byte scanned.
        assert_eq!(counter.0.string_utf8_bytes, 0);
        assert!(long.same(&long).is_err());
        assert_eq!(ticket()?, 1);
        Ok(())
    })
    .unwrap();
    cut_shape::with_budget(100_000, || {
        let a = census(&s)?;
        let before = ticket()?;
        assert!(s.same(&s)?);
        assert!(ticket()? - before <= a.comparison_work_upper(a)?);
        Ok(())
    })
    .unwrap();
    let report = inventory(&s, &s).unwrap();
    assert!(report.actual_census_ticket_work > report.completed_shape.census_work_upper().unwrap());
    let boundary = TypedPrefixShape {
        cells: usize::MAX,
        ..TypedPrefixShape::default()
    };
    // MAX + zero remains representable. Exercise the two actual checked census
    // additions separately, rather than expecting MAX itself to be rejected.
    assert_eq!(boundary.census_work_upper().unwrap(), usize::MAX);
    for overflowing in [
        TypedPrefixShape {
            declared_field_visits: 1,
            ..boundary
        },
        TypedPrefixShape {
            string_utf8_bytes: 1,
            ..boundary
        },
    ] {
        assert!(overflowing.census_work_upper().is_err());
    }
    assert!(boundary.comparison_work_upper(boundary).is_err());
    let multiplication_overflow = TypedPrefixShape {
        cells: usize::MAX / 4 + 1,
        ..TypedPrefixShape::default()
    };
    assert!(multiplication_overflow
        .comparison_work_upper(TypedPrefixShape::default())
        .is_err());
}

#[test]
fn native_cancellation_and_deadline_refuse_without_ticket_leak() {
    let control = portfolio_compute_control::Control::new(10_000).unwrap();
    let s = spec();
    let result = portfolio_compute_control::scope(Some(control.clone()), || {
        cut_shape::with_budget(1_000_000, || {
            census(&s)?;
            control.cancel();
            commercial_cut_typed_specification_inventory(&s, &s, 0)
        })
    });
    assert_eq!(result.unwrap_err(), "COMPUTE_CANCELLED");
    assert!(cut_shape::spent().is_none());
    let expired = portfolio_compute_control::Control::from_started(
        1,
        std::time::Instant::now() - std::time::Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        portfolio_compute_control::scope(Some(expired), || inventory(&s, &s)).unwrap_err(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
    assert!(portfolio_compute_control::checkpoint().is_ok());
}
