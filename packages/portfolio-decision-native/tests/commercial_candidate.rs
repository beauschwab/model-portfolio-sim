use portfolio_decision_native::commercial_candidate_journal::{self as candidate, Request};
use serde_json::{json, Value};
fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-candidate-journal-1.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
fn paths(result: &candidate::ResultData) -> &[Value] {
    &result.tables["path"]
}
fn final_cash(result: &candidate::ResultData) -> f64 {
    paths(result)
        .iter()
        .rfind(|r| r["scenario"] == "baseline")
        .unwrap()["cash"]
        .as_f64()
        .unwrap()
}
fn source_rows(r: &mut Value) -> Vec<&mut Value> {
    let p = &mut r["commercial"]["units"][0]["graph"]["positions"][0];
    let obj = p.as_object_mut().unwrap();
    let mut result = Vec::new();
    for (key, values) in obj.iter_mut() {
        if key == "original_pricing" || key == "original_physical" {
            for h in values.as_array_mut().unwrap() {
                result.push(&mut h["source"]);
            }
        } else if key == "histories" {
            for set in values.as_array_mut().unwrap() {
                for (role, hs) in set.as_object_mut().unwrap() {
                    if matches!(
                        role.as_str(),
                        "physical" | "pricing" | "pricing_down" | "pricing_up"
                    ) {
                        for h in hs.as_array_mut().unwrap() {
                            result.push(&mut h["source"]);
                        }
                    }
                }
            }
        }
    }
    result
}
#[test]
fn candidate_allocations_post_one_acquisition_and_complete_cash_in_same_sim() {
    let r = request();
    let result = candidate::evaluate(&r).unwrap();
    assert!(result.shared_daily_sim_adoption);
    assert!(result.graph_exercise_source_adoption);
    assert!(!result.opening_identity_verified);
    assert!(!result.mixed_ledger_adoption);
    near(final_cash(&result), 1000.);
    let source = &result.preparation.native_preparation.source().native_graphs[0].native_sources;
    assert_eq!(
        source
            .iter()
            .filter(|s| s.physical_accounting_source().is_some())
            .count(),
        2
    );
    let lineage = &result.tables["commercial_source_lineage"];
    assert_eq!(
        lineage
            .iter()
            .filter(|r| r["native_atomic_group"]["phase"] == "reviewed_opening_snapshot")
            .count(),
        2
    );
    assert!(lineage.iter().all(|r| r["allocation_factor"] == 0.5));
    assert_eq!(
        result.producer_completion,
        json!({"complete":true,"protocol":6})
    );
}
#[test]
fn future_acquisition_keeps_cash_and_principal_zero_until_actual_purchase() {
    let mut v = serde_json::to_value(request()).unwrap();
    let asof = v["as_of_ordinal"].as_i64().unwrap();
    v["commercial"]["reporting_ordinals"] = json!([asof, asof + 5, asof + 15]);
    v["commercial"]["units"][0]["graph"]["as_of_ordinal"] = json!(asof + 5);
    v["commercial"]["units"][0]["graph"]["reporting_ordinals"] = json!([asof + 5, asof + 15]);
    for owner in source_rows(&mut v) {
        for role in ["original", "current"] {
            let source = &mut owner["source"][role];
            source["asof_ordinal"] = json!(asof + 5);
            source["reporting_end_ordinal"] = json!(asof + 15);
            source["tail_end_ordinal"] = json!(asof + 40);
            for period in source["periods"].as_array_mut().unwrap() {
                for key in ["start_ordinal", "end_ordinal", "payment_ordinal"] {
                    period[key] = json!(period[key].as_i64().unwrap() + 5);
                }
            }
        }
    }
    let r: Request = serde_json::from_value(v).unwrap();
    let result = candidate::evaluate(&r).unwrap();
    assert_eq!(result.full_end_ordinal, asof as i32 + 40);
    for row in paths(&result)
        .iter()
        .filter(|r| r["day"].as_u64().unwrap() < 5)
    {
        near(row["cash"].as_f64().unwrap(), 1000.);
    }
    let lineage = &result.tables["commercial_source_lineage"];
    let opening = lineage
        .iter()
        .find(|r| r["native_atomic_group"]["phase"] == "reviewed_opening_snapshot")
        .unwrap();
    assert_eq!(opening["day"], 5);
    assert_eq!(opening["native_atomic_group"]["year"], 0.);
    near(final_cash(&result), 1000.);
}
#[test]
fn acquired_call_uses_native_q_policy_and_original_stopped_physical_yield() {
    let mut v = serde_json::to_value(request()).unwrap();
    let rate = 0.03_f64;
    let cost = (100. + 100. * 0.1 * 10. / 365.) * (-rate.ln_1p() * 10. / 365.).exp();
    let p = &mut v["commercial"]["units"][0]["graph"]["positions"][0];
    p["exercise"] = json!({"schema":"commercial-native-exercise-1","identity":{"id":"call","revision":"1"},"information_policy":"native_full_observed_prefix-1","exercise_dates":[{"ordinal":738896,"timing":"after_scheduled_cash","call_clean_price_factor":1.,"put_clean_price_factor":null,"call_fee":0.,"put_fee":0.}],"simultaneous_priority":"issuer"});
    p["original_signed_cash_quote"] = json!(cost);
    p["oas_target"]["base_oas"] = json!(rate.ln_1p());
    p["original_physical"] = p["histories"][0]["physical"].clone();
    for owner in source_rows(&mut v) {
        owner["source"]["acquisition"]["cost"] = json!(cost);
        owner["source"]["acquisition"]["original_annual_effective_rate"] = json!(rate);
        for review in owner["accounting_rates"].as_array_mut().unwrap() {
            review["annual_effective_rates"] = json!([rate, rate, rate]);
        }
    }
    let h = &mut v["commercial"]["units"][0]["graph"]["positions"][0]["original_physical"][0]
        ["source"]["source"];
    h["current"] = h["original"].clone();
    let r: Request = serde_json::from_value(v).unwrap();
    let result = candidate::evaluate(&r).unwrap();
    let g = &result.preparation.native_preparation.source().native_graphs[0];
    assert_eq!(g.original_oas[0].base_oas.to_bits(), rate.ln_1p().to_bits());
    assert_eq!(g.original_acquisition_proofs.len(), 1);
    assert!(result.tables["commercial_source_lineage"]
        .iter()
        .any(|r| r["native_atomic_group"]["phase"] == "full_live_exit"));
    near(
        final_cash(&result),
        1000. + 0.5 * (100. + 100. * 0.1 * 10. / 365. - cost),
    );
}
#[test]
fn zero_participation_keeps_complete_source_and_wrong_mapping_rejects() {
    let mut r = request();
    r.allocation[0].face = 0.;
    let result = candidate::evaluate(&r).unwrap();
    near(final_cash(&result), 1000.);
    assert!(!result.tables["commercial_source_lineage"].is_empty());
    r.owners[0].pre_purchase_basis = 1.;
    assert!(candidate::evaluate(&r).is_err());
    let mut r = request();
    r.run_mapping[0].severity = -0.;
    assert!(candidate::evaluate(&r).is_err());
    let mut r = request();
    r.specification["horizon_days"] = json!(14);
    assert!(candidate::evaluate(&r).is_err());
    let mut r = request();
    r.commercial.units[0].accounting_category = "HTM".into();
    r.owners[0].accounting_category = "HTM".into();
    r.owners[0].ledger_classification = "htm".into();
    assert!(candidate::evaluate(&r).is_err());
}
#[test]
fn default_and_delayed_recovery_preserve_principal_and_original_quote_without_fake_cash() {
    let mut raw = serde_json::to_value(request()).unwrap();
    let stream = json!({"kind":"supplied","stream_id":"controlled-default","stream_revision":"1","paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-100_f64*2./365.).exp(),"destination_uniform":0.5}]}]});
    for source in source_rows(&mut raw) {
        source["credit"]["draw_stream"] = stream.clone();
    }
    let hs = &mut raw["commercial"]["units"][0]["graph"]["positions"][0]["histories"][0];
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        let interval = &mut hs[role][0]["source"]["credit"]["intervals"][0];
        interval["hazards_per_year"][0][3] = json!(100.);
        interval["loss_given_default"] = json!([0.4, 0.4, 0.4]);
        interval["interest_recovery_fraction"] = json!([1., 1., 1.]);
        for tail in interval["complete_recovery_tails"].as_array_mut().unwrap() {
            tail[0]["lag_years"] = json!(40. / 365.);
        }
    }
    let r: Request = serde_json::from_value(raw).unwrap();
    let result = candidate::evaluate(&r).unwrap();
    let quote = r.commercial.units[0].graph.positions[0].original_signed_cash_quote;
    near(
        final_cash(&result),
        1000. - 0.5 * quote + 0.5 * (60. + 100. * 0.1 * 2. / 365.),
    );
    assert_eq!(result.full_end_ordinal, 738886 + 75);
    assert!(result.tables["commercial_source_lineage"]
        .iter()
        .any(|r| r["native_atomic_group"]["phase"] == "default_gross_transfer"));
}
#[test]
fn raw_native_entry_rejects_duplicates_and_original_deadline_before_graph_generation() {
    let envelope = json!({"op":"analyze_commercial_candidate_journal","schema":candidate::SCHEMA,"execution_timeout_ms":10000,"request":request()});
    let raw = serde_json::to_string(&envelope)
        .unwrap()
        .replace("\"face\":50.0", "\"face\":50.0,\"face\":51.0");
    assert!(
        candidate::run_envelope_started(raw.as_bytes(), std::time::Instant::now())
            .unwrap_err()
            .contains("duplicate")
    );
    let mut envelope = envelope;
    envelope["execution_timeout_ms"] = json!(1);
    let started = std::time::Instant::now() - std::time::Duration::from_millis(10);
    assert_eq!(
        candidate::run_envelope_started(&serde_json::to_vec(&envelope).unwrap(), started)
            .unwrap_err(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
}

#[test]
fn issuer_ac_security_retains_own_original_eir_default_claim_and_typed_journal_owner() {
    let mut raw = serde_json::to_value(request()).unwrap();
    let annual = 0.03_f64;
    let cash = 100. + 100. * 0.1 * 30. / 365.;
    let cost = cash * (-annual.ln_1p() * 35. / 365.).exp();
    raw["specification"]["positions"][0]["kind"] = json!("security");
    let p = &mut raw["commercial"]["units"][0]["graph"]["positions"][0];
    p["credit_identity"] = json!("issuer_credit_debt");
    p["original_signed_cash_quote"] = json!(cost);
    p["oas_target"]["base_oas"] = json!(annual.ln_1p());
    for source in source_rows(&mut raw) {
        source["credit"]["instrument_kind"] = json!("issuer_credit_debt");
        source["source"]["acquisition"]["cost"] = json!(cost);
        source["source"]["acquisition"]["original_annual_effective_rate"] = json!(annual);
        for review in source["accounting_rates"].as_array_mut().unwrap() {
            review["annual_effective_rates"] = json!([annual, annual, annual]);
        }
        source["credit"]["draw_stream"] = json!({"kind":"supplied","stream_id":"controlled-default","stream_revision":"1","paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-100_f64*2./365.).exp(),"destination_uniform":0.5}]}]});
    }
    for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
        let i = &mut raw["commercial"]["units"][0]["graph"]["positions"][0]["histories"][0][role]
            [0]["source"]["credit"]["intervals"][0];
        i["hazards_per_year"][0][3] = json!(100.);
        i["loss_given_default"] = json!([0.4, 0.4, 0.4]);
        i["interest_recovery_fraction"] = json!([1., 1., 1.]);
    }
    let result = candidate::evaluate(&serde_json::from_value(raw.clone()).unwrap()).unwrap();
    near(
        final_cash(&result),
        1000. - 0.5 * cost + 0.5 * (60. + 100. * 0.1 * 2. / 365.),
    );
    assert!(result.tables["commercial_source_lineage"]
        .iter()
        .all(|r| r["source_class"] == "issuer_ac_security-1"));
    assert!(result.tables["commercial_source_lineage"]
        .iter()
        .any(|r| r["native_atomic_group"]["effective_interest"]
            .as_f64()
            .is_some_and(|v| v > 0.)));
    for class in ["afs", "htm"] {
        let mut bad = raw.clone();
        bad["specification"]["positions"][0]["classification"] = json!(class);
        bad["owners"][0]["ledger_classification"] = json!(class);
        assert!(candidate::evaluate(&serde_json::from_value(bad).unwrap()).is_err());
    }
    raw["commercial"]["units"][0]["graph"]["positions"][0]["histories"][0]["physical"][0]
        ["source"]["credit"]["opening_undrawn_commitment"] = json!(20.);
    assert!(candidate::evaluate(&serde_json::from_value(raw).unwrap())
        .err()
        .unwrap()
        .contains("facility/draw"));
}
