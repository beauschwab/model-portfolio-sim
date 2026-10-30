//! Observer proofs borrow the already compiled complete finite source pass.
use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::{commercial_graph as graph, commercial_units as units};
use serde::Serialize;
use serde_json::{json, Value};

fn graph_request() -> graph::Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-product-graph-1.json"
    ))
    .unwrap()
}

fn unit_request(fees: bool) -> units::Request {
    serde_json::from_str(if fees {
        include_str!("../../../examples/models/commercial-fee-units-1.json")
    } else {
        include_str!("../../../examples/models/commercial-units-1.json")
    })
    .unwrap()
}

fn encoded(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

fn observe(r: &graph::Request) -> (graph::ResultData, Vec<Value>) {
    let mut observations = Vec::new();
    let mut observer = |source: graph::SourceObservation<'_>| {
        observations.push(serde_json::to_value(source).unwrap());
        Ok(())
    };
    let out = graph::analyze_with_source_observer(r, &mut observer).unwrap();
    (out, observations)
}

#[test]
fn complete_graph_proofs_and_legacy_bytes_match_with_original_current_label_overlap() {
    let mut r = graph_request();
    r.original_scenario = r.scenarios[0].clone();
    let (observed, proofs) = observe(&r);
    assert_eq!(encoded(&observed), encoded(&graph::analyze(&r).unwrap()));
    assert_eq!(proofs.len(), 5);
    assert_eq!(proofs[0]["scope"], "original_pricing");
    assert_eq!(proofs[0]["scenario_index"], Value::Null);
    assert_eq!(proofs[0]["role"], "pricing");
    assert_eq!(proofs[0]["proof"]["scenario_id"], "baseline");
    let expected_roles = ["physical", "pricing", "pricing_down", "pricing_up"];
    for (index, (proof, source)) in proofs[1..].iter().zip(&observed.native_sources).enumerate() {
        assert_eq!(proof["scope"], "current");
        assert_eq!(proof["position_index"], 0);
        assert_eq!(proof["scenario_index"], 0);
        assert_eq!(proof["history_index"], index);
        assert_eq!(proof["role"], expected_roles[index]);
        assert_eq!(proof["proof"], serde_json::to_value(source).unwrap());
    }
    assert_eq!(proofs[0]["proof"]["role"], proofs[2]["proof"]["role"]);
    assert_eq!(
        proofs[0]["proof"]["scenario_id"],
        proofs[2]["proof"]["scenario_id"]
    );
    assert_ne!(proofs[0]["scope"], proofs[2]["scope"]);
}

#[test]
fn scenario_indexes_follow_raw_scenarios_and_current_history_indexes_restart() {
    let mut r = graph_request();
    let mut second = r.scenarios[0].clone();
    second.scenario_id = "second".into();
    second.run_id = "second-run".into();
    r.scenarios.push(second);
    let mut histories = r.positions[0].histories[0].clone();
    histories.scenario_id = "second".into();
    for h in histories
        .physical
        .iter_mut()
        .chain(&mut histories.pricing)
        .chain(&mut histories.pricing_down)
        .chain(&mut histories.pricing_up)
    {
        h.source.source.current.scenario_id = "second".into();
    }
    r.positions[0].histories.push(histories);
    let (observed, proofs) = observe(&r);
    assert_eq!(encoded(&observed), encoded(&graph::analyze(&r).unwrap()));
    assert_eq!(proofs.len(), 9);
    for (index, proof) in proofs[1..].iter().enumerate() {
        assert_eq!(proof["scenario_index"], index / 4);
        assert_eq!(proof["history_index"], index % 4);
    }
}

fn add_zero_weight(histories: &mut Vec<graph::History>) {
    let mut extra = histories[0].clone();
    extra.weight = 0.;
    extra.path_id = "zero-path".into();
    extra.source.credit.paths[0].path_id = extra.path_id.clone();
    extra.source.credit.paths[0].path_key = 18;
    let mut stream = serde_json::to_value(&extra.source.credit.draw_stream).unwrap();
    if let Some(paths) = stream.get_mut("paths").and_then(Value::as_array_mut) {
        for path in paths {
            path["path_key"] = json!(18);
        }
    }
    extra.source.credit.draw_stream = serde_json::from_value(stream).unwrap();
    histories.push(extra);
}

#[test]
fn zero_weight_histories_are_complete_unweighted_proofs_not_pruned() {
    let mut r = graph_request();
    add_zero_weight(&mut r.positions[0].original_pricing);
    let hs = &mut r.positions[0].histories[0];
    for histories in [
        &mut hs.physical,
        &mut hs.pricing,
        &mut hs.pricing_down,
        &mut hs.pricing_up,
    ] {
        add_zero_weight(histories);
    }
    let (observed, proofs) = observe(&r);
    assert_eq!(encoded(&observed), encoded(&graph::analyze(&r).unwrap()));
    assert_eq!(proofs.len(), 10);
    assert_eq!(
        proofs.iter().filter(|p| p["proof"]["weight"] == 0.).count(),
        5
    );
    for proof in proofs.iter().filter(|p| p["proof"]["weight"] == 0.) {
        assert_eq!(
            proof["proof"]["source_request"]["credit"]["paths"][0]["path_key"],
            18
        );
        assert!(!proof["proof"]["cash"].as_array().unwrap().is_empty());
        assert!(proof["proof"]["projection"].is_object());
    }
}

fn acquired_exercise() -> units::Request {
    let mut r = unit_request(false);
    let p = &mut r.units[0].graph.positions[0];
    let day = r.reporting_ordinals[0] + 10;
    p.exercise = Some(serde_json::from_value(json!({
        "schema":"commercial-native-exercise-1","identity":{"id":"observer-call","revision":"1"},
        "information_policy":"native_full_observed_prefix-1",
        "exercise_dates":[{"ordinal":day,"timing":"after_scheduled_cash", "call_clean_price_factor":1.,
            "put_clean_price_factor":null,"call_fee":0.,"put_fee":0.}],"simultaneous_priority":"issuer"
    })).unwrap());
    let quote = 100. + 100. * 0.1 * 10. / 365.;
    p.original_signed_cash_quote = quote;
    for h in p
        .original_pricing
        .iter_mut()
        .chain(p.histories.iter_mut().flat_map(|hs| {
            hs.physical
                .iter_mut()
                .chain(&mut hs.pricing)
                .chain(&mut hs.pricing_down)
                .chain(&mut hs.pricing_up)
        }))
    {
        h.source.source.acquisition.cost = quote;
    }
    p.original_physical = p.histories[0].physical.clone();
    for h in &mut p.original_physical {
        h.source.source.current = h.source.source.original.clone();
    }
    r
}

#[test]
fn acquired_original_physical_proofs_are_post_anchor_and_match_retained_sources() {
    let r = acquired_exercise();
    let (observed, proofs) = observe(&r.units[0].graph);
    assert_eq!(
        encoded(&observed),
        encoded(&graph::analyze(&r.units[0].graph).unwrap())
    );
    assert_eq!(proofs[0]["scope"], "original_pricing");
    assert_eq!(proofs[1]["scope"], "original_physical");
    assert_eq!(proofs[1]["scenario_index"], Value::Null);
    assert_eq!(proofs[1]["history_index"], 0);
    assert_eq!(proofs[1]["role"], "physical");
    assert_eq!(
        proofs[1]["proof"],
        serde_json::to_value(&observed.original_physical_sources[0]).unwrap()
    );
    assert!(!observed.original_acquisition_proofs.is_empty());
}

#[test]
fn unit_observer_keeps_exact_coefficients_complete_fees_and_borrowed_identity() {
    for r in [unit_request(false), unit_request(true), acquired_exercise()] {
        let mut proofs = Vec::new();
        let mut observer = |source: units::UnitSourceObservation<'_>| {
            assert_eq!(source.unit_id, r.units[source.unit_index].unit_id);
            proofs.push(serde_json::to_value(source).unwrap());
            Ok(())
        };
        let observed = units::prepare_with_source_observer(&r, &mut observer).unwrap();
        assert_eq!(encoded(&observed), encoded(&units::prepare(&r).unwrap()));
        assert!(proofs
            .iter()
            .all(|p| p["unit_index"] == 0 && p["unit_id"] == "commercial"));
        if r.units[0].graph.positions[0].fees.is_some() {
            assert!(proofs
                .iter()
                .all(|p| p["proof"]["fee_projection"].is_object()));
            assert_eq!(proofs[0]["scope"], "original_pricing");
            assert!(!observed.source().fee_income_values.is_empty());
        }
    }
}

#[test]
fn multiple_position_and_unit_indexes_bind_raw_order_without_changing_preparation() {
    fn rename(value: &mut Value) {
        match value {
            Value::String(text) if text == "commercial" => *text = "second".into(),
            Value::Array(items) => items.iter_mut().for_each(rename),
            Value::Object(items) => items.values_mut().for_each(rename),
            _ => {}
        }
    }
    let mut r = unit_request(false);
    let mut second = serde_json::to_value(&r.units[0]).unwrap();
    rename(&mut second);
    r.units.push(serde_json::from_value(second).unwrap());
    let mut proofs = Vec::new();
    let mut observer = |source: units::UnitSourceObservation<'_>| {
        assert_eq!(source.unit_id, r.units[source.unit_index].unit_id);
        proofs.push(serde_json::to_value(source).unwrap());
        Ok(())
    };
    let observed = units::prepare_with_source_observer(&r, &mut observer).unwrap();
    assert_eq!(encoded(&observed), encoded(&units::prepare(&r).unwrap()));
    assert_eq!(proofs.len(), 10);
    for (index, proof) in proofs.iter().enumerate() {
        assert_eq!(proof["unit_index"], index / 5);
        assert_eq!(proof["position_index"], 0);
        assert_eq!(proof["proof"]["position_id"], r.units[index / 5].unit_id);
    }

    let mut g = r.units[0].graph.clone();
    g.positions.push(r.units[1].graph.positions[0].clone());
    let (observed, proofs) = observe(&g);
    assert_eq!(encoded(&observed), encoded(&graph::analyze(&g).unwrap()));
    assert_eq!(proofs.len(), 10);
    for (index, proof) in proofs.iter().enumerate() {
        assert_eq!(proof["position_index"], index / 5);
        assert_eq!(
            proof["proof"]["position_id"],
            g.positions[index / 5].position_id
        );
    }
}

#[test]
fn invalid_original_loan_and_fee_quotes_emit_no_original_proof() {
    for fee in [false, true] {
        let mut r = unit_request(fee);
        let p = &mut r.units[0].graph.positions[0];
        if let Some(fees) = p.fees.as_mut() {
            fees.original_signed_fee_quote += 1.;
        } else {
            p.original_signed_cash_quote += 1.;
        }
        let mut calls = 0;
        let mut observer = |_: graph::SourceObservation<'_>| {
            calls += 1;
            Ok(())
        };
        assert!(graph::analyze_with_source_observer(&r.units[0].graph, &mut observer).is_err());
        assert_eq!(calls, 0);
    }
}

#[test]
fn callback_failure_and_cancellation_stop_without_successful_output() {
    let r = graph_request();
    let mut calls = 0;
    let mut fail = |_: graph::SourceObservation<'_>| {
        calls += 1;
        Err("observer write failed".into())
    };
    assert_eq!(
        graph::analyze_with_source_observer(&r, &mut fail)
            .err()
            .unwrap(),
        "observer write failed"
    );
    assert_eq!(calls, 1);

    let mut calls = 0;
    let mut fail_current = |source: graph::SourceObservation<'_>| {
        calls += 1;
        if source.scope == graph::ProofScope::Current {
            Err("current observer write failed".into())
        } else {
            Ok(())
        }
    };
    assert_eq!(
        graph::analyze_with_source_observer(&r, &mut fail_current)
            .err()
            .unwrap(),
        "current observer write failed"
    );
    assert_eq!(calls, 2);

    let control = Control::new(10_000).unwrap();
    let trigger = control.clone();
    let mut calls = 0;
    let mut cancel = |_: units::UnitSourceObservation<'_>| {
        calls += 1;
        trigger.cancel();
        Ok(())
    };
    let result = scope(Some(control), || {
        units::prepare_with_source_observer(&unit_request(false), &mut cancel).map(|_| ())
    });
    assert_eq!(result.unwrap_err(), "COMPUTE_CANCELLED");
    assert_eq!(calls, 1);
}

#[test]
fn generated_observer_requests_reject_before_audit_or_any_finite_unit_generation() {
    let generated: graph::Request = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-generated-graph-1.json"
    ))
    .unwrap();
    let mut invalid = generated.clone();
    invalid.positions[0].original_signed_cash_quote = -1.;
    let mut calls = 0;
    let mut observer = |_: graph::SourceObservation<'_>| {
        calls += 1;
        Ok(())
    };
    assert!(graph::analyze_with_source_observer(&invalid, &mut observer)
        .err()
        .unwrap()
        .contains("does not support generated"));
    assert_eq!(calls, 0);
    // Ordinary preparation still admits its historical generated route.
    assert!(graph::audit_admitted_work(&generated).is_ok());

    let mut units = unit_request(false);
    let mut later = units.units[0].clone();
    later.graph = invalid;
    units.units.push(later);
    let mut calls = 0;
    let mut observer = |_: units::UnitSourceObservation<'_>| {
        calls += 1;
        Ok(())
    };
    assert!(units::prepare_with_source_observer(&units, &mut observer)
        .err()
        .unwrap()
        .contains("does not support generated"));
    assert_eq!(calls, 0);
}
