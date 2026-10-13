use portfolio_risk_native::aged_units::{cache::Cache, prepare, AgedUnitsRequest};

fn request() -> AgedUnitsRequest {
    let mut r: AgedUnitsRequest =
        serde_json::from_str(include_str!("../../../examples/models/aged-units-1.json")).unwrap();
    let mut t = r.templates[0].clone();
    t.template_id = "second".into();
    let mut p = r.purchases[0].clone();
    p.template_id = t.template_id.clone();
    r.templates.push(t);
    r.purchases.push(p);
    r
}
fn exact(cache: &Cache, r: &AgedUnitsRequest) {
    assert_eq!(
        serde_json::to_vec(cache.snapshot()).unwrap(),
        serde_json::to_vec(&prepare(r).unwrap()).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(cache.request()).unwrap(),
        serde_json::to_vec(r).unwrap()
    );
}
#[test]
fn exact_full_input_hit_has_no_pricing_and_snapshot_is_immutable() {
    let r = request();
    let mut cache = Cache::new(&r).unwrap();
    let clone = cache.clone();
    assert!(std::ptr::eq(cache.request(), clone.request()));
    assert!(std::ptr::eq(cache.snapshot(), clone.snapshot()));
    exact(&cache, &r);
    let work = cache.update(&r).unwrap();
    assert!(work.unchanged_request);
    assert_eq!(work.pricing_work, 0);
    assert_eq!(work.templates_rebuilt, 0);
    assert!(work.retained_bytes > 0);
}
#[test]
fn quote_original_yield_and_weights_rebuild_only_complete_dirty_template() {
    let mut r = request();
    let mut cache = Cache::new(&r).unwrap();
    r.purchases[0].quote_price_per_unit = 0.9;
    r.purchases[0].original_annual_effective_yield = (1. / 0.9_f64).powf(365. / 366.) - 1.;
    r.templates[0].weights.rwa = 2.;
    let work = cache.update(&r).unwrap();
    assert_eq!(work.templates_rebuilt, 1);
    assert_eq!(work.units_rebuilt, 1);
    assert!(!work.full_rebuild && !work.unchanged_request);
    exact(&cache, &r);
}
#[test]
fn term_change_rebuilds_native_full_tail_union_and_removes_old_settlements() {
    let mut r = request();
    let mut cache = Cache::new(&r).unwrap();
    r.templates[0].term.term_months = 6;
    r.templates[0].term.freq_months = 6;
    let work = cache.update(&r).unwrap();
    assert_eq!(work.templates_rebuilt, 1);
    assert_eq!(cache.snapshot().funding.ordinals.len(), 4);
    exact(&cache, &r);
}
#[test]
fn purchase_grid_and_canonical_order_merge_scenarios_events_and_diagnostics() {
    let mut r = request();
    r.scenarios.push(r.scenarios[0].clone());
    r.scenarios[1].scenario_id = "second-scenario".into();
    r.scenarios[1].collateral_curve.curve_id = "other-collateral".into();
    let mut cache = Cache::new(&r).unwrap();
    let mut p = r.purchases[0].clone();
    p.purchase_ordinal = r.reporting_ordinals[1];
    r.purchases.push(p);
    r.purchases.swap(0, 1); // Different template global order; each date sequence valid.
    r.templates.reverse();
    let work = cache.update(&r).unwrap();
    assert_eq!(work.templates_rebuilt, 1);
    assert_eq!(work.units_rebuilt, 2);
    exact(&cache, &r);
}
#[test]
fn input_grid_revision_updates_source_without_label_only_identity() {
    let mut r = request();
    let mut cache = Cache::new(&r).unwrap();
    r.identity.input_revision = "actual-source-v2".into();
    r.identity.grid_revision = "actual-grid-v2".into();
    let work = cache.update(&r).unwrap();
    assert_eq!(work.pricing_work, 0);
    assert!(!work.unchanged_request);
    assert!(cache
        .snapshot()
        .funding
        .purchase_costs
        .iter()
        .all(|p| p.contract_source.revision == "actual-source-v2"));
    exact(&cache, &r);
    // Economics change even if all revision labels are intentionally unchanged.
    r.templates[1].weights.asf = 0.25;
    assert_eq!(cache.update(&r).unwrap().templates_rebuilt, 1);
    exact(&cache, &r);
}
#[test]
fn shared_curve_model_crn_and_calendar_changes_rebuild_all() {
    let mut r = request();
    let mut cache = Cache::new(&r).unwrap();
    for change in 0..4 {
        match change {
            0 => r.scenarios[0].collateral_curve.discount_factors[1] *= 0.999,
            1 => r.identity.model_revision = "model-v2".into(),
            2 => r.identity.crn_id = "crn-v2".into(),
            _ => r.templates[0].term.calendar.name = "US".into(),
        }
        let work = cache.update(&r).unwrap();
        assert!(work.full_rebuild);
        assert_eq!(work.templates_rebuilt, 2);
        exact(&cache, &r);
    }
}
#[test]
fn failed_update_and_staged_clone_preserve_committed_request_and_result() {
    let r = request();
    let mut cache = Cache::new(&r).unwrap();
    let mut staged = cache.clone();
    let mut bad = r.clone();
    bad.purchases[0].original_annual_effective_yield = 0.3;
    assert!(cache.update(&bad).is_err());
    exact(&cache, &r);
    bad = r.clone();
    bad.templates[1].weights.rwa = 3.;
    staged.update(&bad).unwrap();
    exact(&staged, &bad);
    exact(&cache, &r);
}
#[test]
fn template_removal_needs_no_pricing_and_reindexes_all_funding_events() {
    let mut r = request();
    let mut cache = Cache::new(&r).unwrap();
    r.templates.remove(0);
    r.purchases.remove(0);
    let work = cache.update(&r).unwrap();
    assert_eq!(work.templates_rebuilt, 0);
    assert!(cache
        .snapshot()
        .funding
        .cash_events
        .iter()
        .all(|e| e.unit_index == 0));
    exact(&cache, &r);
}
#[test]
fn validation_rows_merge_in_global_input_order_and_rebuild_only_dirty_source() {
    let mut r = request();
    let start = r.reporting_ordinals[0];
    let middle = r.reporting_ordinals[1];
    r.reporting_ordinals = vec![start, middle, middle + 31, middle + 61];
    for i in 0..2 {
        let mut p = r.purchases[i].clone();
        p.purchase_ordinal = middle + 31;
        r.purchases.push(p);
    }
    for i in [1, 0] {
        let mut p = r.purchases[i].clone();
        p.purchase_ordinal = middle;
        r.validation_purchases.push(p);
    }
    let mut cache = Cache::new(&r).unwrap();
    exact(&cache, &r);
    r.validation_purchases[0].quote_price_per_unit = 0.9;
    // Feb29 2024 -> clamped Feb28 2025 is independently 365 days.
    r.validation_purchases[0].original_annual_effective_yield = 1. / 0.9 - 1.;
    let work = cache.update(&r).unwrap();
    assert_eq!(work.templates_rebuilt, 1);
    assert_eq!(work.units_rebuilt, 2);
    exact(&cache, &r);
}
#[test]
fn aggregate_pricing_admission_is_preserved_across_template_partitions() {
    let mut r = request();
    r.scenarios[0].collateral_curve.right_tail =
        portfolio_risk_native::model_market::curves::CurveTail::FlatForward;
    let start = r.reporting_ordinals[0];
    r.reporting_ordinals = (0..=120).map(|i| start + i * 30).collect();
    let template = r.templates[0].clone();
    let purchase = r.purchases[0].clone();
    r.templates.clear();
    r.purchases.clear();
    for i in 0..16 {
        let mut t = template.clone();
        t.template_id = format!("long-monthly-{i}");
        t.term.term_months = 1190;
        t.term.freq_months = 1;
        let mut p = purchase.clone();
        p.template_id = t.template_id.clone();
        r.templates.push(t);
        r.purchases.push(p);
    }
    let error = Cache::new(&r).err().unwrap();
    assert!(error.contains("work admission"), "{error}");
}
#[test]
fn retained_byte_admission_rejects_duplicated_piece_markets_and_preserves_old_cache() {
    let initial = request();
    let mut cache = Cache::new(&initial).unwrap();
    let mut r = initial.clone();
    let mut curve = r.scenarios[0].clone();
    curve.collateral_curve.times_years = (0..4096).map(|i| i as f64 / 4095. * 100.).collect();
    curve.collateral_curve.discount_factors = curve
        .collateral_curve
        .times_years
        .iter()
        .map(|t| (-0.05 * t).exp())
        .collect();
    r.scenarios = (0..13)
        .map(|i| {
            let mut s = curve.clone();
            s.scenario_id = format!("scenario-{i}");
            s.collateral_curve.curve_id = format!("curve-{i}");
            s
        })
        .collect();
    r.base_scenario_id = "scenario-0".into();
    let template = r.templates[0].clone();
    let purchase = r.purchases[0].clone();
    r.templates.clear();
    r.purchases.clear();
    for i in 0..32 {
        let mut t = template.clone();
        t.template_id = format!("market-copy-{i}");
        let mut p = purchase.clone();
        p.template_id = t.template_id.clone();
        r.templates.push(t);
        r.purchases.push(p);
    }
    assert!(cache.update(&r).err().unwrap().contains("byte admission"));
    exact(&cache, &initial);
}
