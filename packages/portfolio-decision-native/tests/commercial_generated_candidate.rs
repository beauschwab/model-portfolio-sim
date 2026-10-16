use portfolio_decision_native::commercial_candidate_journal::{
    run_generated_stream_started, GENERATED_SCHEMA,
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Instant,
};
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn request() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/commercial-generated-candidate-journal-1.json"
    ))
    .unwrap()
}
#[test]
fn one_hundred_twenty_eight_generated_physical_owners_enter_one_streamed_sim() {
    let r = request();
    let output = Buffer::default();
    let e = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":120000,"request":r});
    run_generated_stream_started(
        &serde_json::to_vec(&e).unwrap(),
        Box::new(output.clone()),
        Instant::now(),
    )
    .unwrap();
    let data = output.0.lock().unwrap();
    let rows = data
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice::<Value>(b).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows[0]["workflow"]["source_receipt"]["physical_paths"], 256);
    assert_eq!(
        rows[0]["workflow"]["source_receipt"]["maximum_live_full_physical_owners"],
        1
    );
    assert_eq!(
        rows.last().unwrap()["record"],
        "commercial_generated_complete"
    );
    assert!(rows
        .iter()
        .any(|r| r == &json!({"complete":true,"protocol":6})));
    let acquired = rows
        .iter()
        .filter(|r| r["table"] == "commercial_source_lineage")
        .flat_map(|r| r["rows"].as_array().unwrap())
        .filter(|r| r["native_atomic_group"]["phase"] == "reviewed_opening_snapshot")
        .count();
    assert_eq!(acquired, 256);
    let closings = rows
        .iter()
        .filter(|r| r["table"] == "commercial_closing")
        .flat_map(|r| r["rows"].as_array().unwrap())
        .count();
    assert_eq!(closings, 256);
}
#[test]
fn generated_journal_requires_explicit_route_and_original_deadline() {
    let mut r = request();
    r["schema"] = json!("commercial-candidate-journal-1");
    let e = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":1,"request":r});
    let started = Instant::now() - std::time::Duration::from_secs(1);
    assert!(run_generated_stream_started(
        &serde_json::to_vec(&e).unwrap(),
        Box::new(Buffer::default()),
        started
    )
    .is_err());
}
#[test]
fn generated_purchase_offsets_preserve_pre_purchase_cash_and_actual_origin() {
    fn shift_dates(value: &mut Value) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    if (key == "ordinal" || key.ends_with("_ordinal")) && value.is_i64() {
                        *value = json!(value.as_i64().unwrap() + 5);
                    } else if key.ends_with("_ordinals") {
                        for day in value.as_array_mut().unwrap() {
                            *day = json!(day.as_i64().unwrap() + 5);
                        }
                    } else {
                        shift_dates(value);
                    }
                }
            }
            Value::Array(values) => values.iter_mut().for_each(shift_dates),
            _ => {}
        }
    }
    let mut raw = request();
    shift_dates(&mut raw["commercial"]);
    raw["reporting_end_ordinal"] = json!(raw["reporting_end_ordinal"].as_i64().unwrap() + 5);
    raw["commercial"]["reporting_ordinals"] = json!([
        raw["as_of_ordinal"],
        raw["as_of_ordinal"].as_i64().unwrap() + 5,
        raw["reporting_end_ordinal"]
    ]);
    let output = Buffer::default();
    let envelope = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":120000,"request":raw});
    run_generated_stream_started(
        &serde_json::to_vec(&envelope).unwrap(),
        Box::new(output.clone()),
        Instant::now(),
    )
    .unwrap();
    let bytes = output.0.lock().unwrap();
    let rows = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice::<Value>(b).unwrap())
        .collect::<Vec<_>>();
    for row in rows
        .iter()
        .filter(|r| r["table"] == "commercial_source_lineage")
        .flat_map(|r| r["rows"].as_array().unwrap())
    {
        assert_eq!(row["source_origin_day"], 5);
        assert!(row["day"].as_i64().unwrap() >= 5);
        if row["native_atomic_group"]["phase"] == "reviewed_opening_snapshot" {
            assert_eq!(row["day"], 5);
        }
    }
    for row in rows
        .iter()
        .filter(|r| r["table"] == "path")
        .flat_map(|r| r["rows"].as_array().unwrap())
        .filter(|r| r["day"].as_i64().unwrap() < 5)
    {
        assert_eq!(row["cash"], 1000.);
    }
}
#[test]
fn genuine_generated_default_recovery_paths_survive_zero_allocation() {
    for face in [50., 0.] {
        let mut r = request();
        r["allocation"][0]["face"] = json!(face);
        let law = &mut r["commercial"]["units"][0]["graph"]["positions"][0]["generated_policy"]
            ["current_physical"][1];
        law["source"]["credit"]["intervals"][0]["hazards_per_year"][0][3] = json!(100.);
        law["source"]["credit"]["intervals"][0]["loss_given_default"] = json!([0.4, 0.4, 0.4]);
        for tail in law["source"]["credit"]["intervals"][0]["complete_recovery_tails"]
            .as_array_mut()
            .unwrap()
        {
            tail[0]["lag_years"] = json!(2. / 365.);
        }
        let output = Buffer::default();
        let e = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":120000,"request":r});
        run_generated_stream_started(
            &serde_json::to_vec(&e).unwrap(),
            Box::new(output.clone()),
            Instant::now(),
        )
        .unwrap();
        let bytes = output.0.lock().unwrap();
        let rows = bytes
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .map(|b| serde_json::from_slice::<Value>(b).unwrap())
            .collect::<Vec<_>>();
        let lineage = rows
            .iter()
            .filter(|r| r["table"] == "commercial_source_lineage")
            .flat_map(|r| r["rows"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert!(lineage
            .iter()
            .any(|r| r["native_atomic_group"]["next"]["in_default"] == true));
        assert!(lineage.iter().any(
            |r| r["native_atomic_group"]["phase"] == "default_actual_cash"
                && r["native_atomic_group"]["cash"].as_f64().unwrap() > 0.
        ));
        assert!(lineage
            .iter()
            .all(|r| r["allocation_factor"] == face / 100.));
        if face == 0. {
            assert!(lineage.iter().all(|r| r["transaction_number"].is_null()));
            for path in rows
                .iter()
                .filter(|r| r["table"] == "path")
                .flat_map(|r| r["rows"].as_array().unwrap())
            {
                assert_eq!(path["cash"], 1000.);
            }
        }
    }
}
#[test]
fn genuine_generated_exercise_keeps_original_stopped_purchase_authority() {
    let mut r = request();
    let p = &mut r["commercial"]["units"][0]["graph"]["positions"][0];
    p["original_signed_cash_quote"] = json!(80.);
    let generated = &mut p["generated_policy"];
    for role in [
        "pricing_training",
        "original_physical",
        "current_physical",
        "current_pricing",
    ] {
        let laws = if generated[role].is_array() {
            generated[role]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .collect::<Vec<_>>()
        } else {
            vec![&mut generated[role]]
        };
        for law in laws {
            law["policy"]["exercise_dates"][0]["timing"] = json!("before_scheduled_cash");
            law["policy"]["exercise_dates"][0]["call_clean_price_factor"] = json!(0.8);
            law["source"]["source"]["acquisition"]["cost"] = json!(80.);
        }
    }
    let output = Buffer::default();
    let envelope = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":120000,"request":r});
    run_generated_stream_started(
        &serde_json::to_vec(&envelope).unwrap(),
        Box::new(output.clone()),
        Instant::now(),
    )
    .unwrap();
    let bytes = output.0.lock().unwrap();
    let rows = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice::<Value>(b).unwrap())
        .collect::<Vec<_>>();
    let exits = rows
        .iter()
        .filter(|r| r["table"] == "commercial_source_lineage")
        .flat_map(|r| r["rows"].as_array().unwrap())
        .filter(|r| r["native_atomic_group"]["phase"] == "full_live_exit")
        .count();
    assert_eq!(exits, 256);
    for row in rows
        .iter()
        .filter(|r| r["table"] == "commercial_candidate_original_acquisition_proofs")
        .flat_map(|r| r["rows"].as_array().unwrap())
    {
        assert_eq!(row["native_json"]["original_stopped_cash_quote"], 80.);
    }
}
#[derive(Clone, Default)]
struct Counter(Arc<std::sync::atomic::AtomicUsize>);
impl Write for Counter {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0
            .fetch_add(b.len(), std::sync::atomic::Ordering::Relaxed);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn generated_daily_output_exceeds_old_buffer_without_an_enlarged_json_envelope() {
    let mut r = request();
    // Explicit longer daily reporting calendar; complete one-day financial
    // laws/coupon/default/recovery tails remain exactly the original source.
    r["specification"]["horizon_days"] = json!(100);
    let output = Counter::default();
    let e = json!({"op":"analyze_commercial_generated_candidate_journal","schema":GENERATED_SCHEMA,"execution_timeout_ms":120000,"request":r});
    run_generated_stream_started(
        &serde_json::to_vec(&e).unwrap(),
        Box::new(output.clone()),
        Instant::now(),
    )
    .unwrap();
    let bytes = output.0.load(std::sync::atomic::Ordering::Relaxed);
    println!("genuine generated journal bytes={bytes}");
    assert!(bytes > 32 << 20, "genuine output bytes={bytes}");
}
fn compare_json(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            if a.is_f64() || b.is_f64() {
                assert!(
                    (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 1e-9,
                    "{a} != {b}"
                );
            } else {
                assert_eq!(a, b);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare_json(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare_json(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn compact_and_borrowed_sources_preserve_atomic_groups_signed_rights_and_ordered_sim() {
    use portfolio_decision_native::commercial_candidate_journal as candidate;
    use portfolio_ledger_native::{
        commercial_intent_owner as ledger, daily, owners::commercial::compact::CompactSource,
    };
    use portfolio_risk_native::advanced_graph::Role;
    let mut raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/commercial-candidate-journal-1.json"
    ))
    .unwrap();
    let negative = -0.1;
    let cost = 100. + 100. * negative * 30. / 365.;
    let position = &mut raw["commercial"]["units"][0]["graph"]["positions"][0];
    position["original_signed_cash_quote"] = json!(cost);
    for key in ["original_pricing", "original_physical"] {
        for history in position
            .get_mut(key)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            history["source"]["source"]["acquisition"]["cost"] = json!(cost);
            history["source"]["credit"]["draw_stream"] = json!({"kind":"supplied","stream_id":"default","stream_revision":"1","paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-100_f64*2./365.).exp(),"destination_uniform":0.5}]}]});
            for role in ["original", "current"] {
                for p in history["source"]["source"][role]["periods"]
                    .as_array_mut()
                    .unwrap()
                {
                    p["coupon"]["annual_rate"] = json!(negative);
                }
            }
        }
    }
    for set in position["histories"].as_array_mut().unwrap() {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            for history in set[role].as_array_mut().unwrap() {
                let source = &mut history["source"];
                source["source"]["acquisition"]["cost"] = json!(cost);
                for which in ["original", "current"] {
                    for p in source["source"][which]["periods"].as_array_mut().unwrap() {
                        p["coupon"]["annual_rate"] = json!(negative);
                    }
                }
                source["credit"]["draw_stream"] = json!({"kind":"supplied","stream_id":"default","stream_revision":"1","paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-100_f64*2./365.).exp(),"destination_uniform":0.5}]}]});
                let interval = &mut source["credit"]["intervals"][0];
                interval["hazards_per_year"][0][3] = json!(100.);
                interval["loss_given_default"] = json!([0.4, 0.4, 0.4]);
                interval["interest_recovery_fraction"] = json!([1., 1., 1.]);
            }
        }
    }
    let request: candidate::Request = serde_json::from_value(raw).unwrap();
    let result = candidate::evaluate(&request).unwrap();
    let sources = result.preparation.native_preparation.source().native_graphs[0]
        .native_sources
        .iter()
        .filter(|s| s.role == Role::Physical)
        .collect::<Vec<_>>();
    let spec: portfolio_ledger_native::types::Spec =
        serde_json::from_value(result.preparation.resolved_specification.clone()).unwrap();
    let mut compact = Vec::new();
    let mut saw_negative = false;
    let mut saw_default = false;
    for source in &sources {
        let full = source.physical_accounting_source().unwrap();
        let packed = CompactSource::from_native(full.accounting(), source.path_key).unwrap();
        assert!(CompactSource::from_native_bounded(full.accounting(), source.path_key, 0).is_err());
        assert!(CompactSource::from_native(full.accounting(), source.path_key + 1).is_err());
        for (i, g) in full.accounting().groups().iter().enumerate() {
            assert_eq!(
                serde_json::to_vec(g).unwrap(),
                serde_json::to_vec(&*ledger::AccountingSource::group(&packed, i)).unwrap()
            );
            saw_negative |= g.next.lots.iter().any(|l| l.recognized_remaining < 0.);
            saw_default |= g.next.in_default;
        }
        compact.push(packed);
    }
    assert!(saw_negative && saw_default);
    let run = |packed: bool| {
        let bindings = sources
            .iter()
            .enumerate()
            .map(|(i, s)| ledger::Binding {
                source_class: ledger::SourceClass::CommercialLoan,
                scenario: &request
                    .run_mapping
                    .iter()
                    .find(|m| m.source_scenario_id == s.scenario_id)
                    .unwrap()
                    .ledger_scenario_id,
                position_id: &request.owners[0].position_id,
                path_key: s.path_key,
                origin_day: 0,
                source_weight: s.weight,
                allocated_face: request.allocation[0].face,
                native_unit_face: request.commercial.units[0].unit_face,
                accounting: if packed {
                    &compact[i] as &dyn ledger::AccountingSource
                } else {
                    s.physical_accounting_source().unwrap().accounting()
                },
            })
            .collect();
        let compiled = ledger::Compiled::new(&spec, bindings).unwrap();
        let output = Buffer::default();
        daily::run_with_commercial(&spec, None, &compiled, Box::new(output.clone())).unwrap();
        let result = output
            .0
            .lock()
            .unwrap()
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .map(|b| serde_json::from_slice::<Value>(b).unwrap())
            .collect::<Vec<_>>();
        result
    };
    let borrowed = run(false);
    let packed = run(true);
    let decode = |stream: Vec<Value>| {
        let mut dictionary = Vec::<String>::new();
        let mut tables = std::collections::BTreeMap::<String, Vec<Value>>::new();
        for row in stream {
            if let Some(table) = row["table"].as_str() {
                if table == "journal" {
                    dictionary.extend(
                        serde_json::from_value::<Vec<String>>(row["dictionary"].clone()).unwrap(),
                    );
                    let columns = row["columns"].as_object().unwrap();
                    for index in 0..columns["value"].as_array().unwrap().len() {
                        let mut result = serde_json::Map::new();
                        for (key, values) in columns {
                            let value = if matches!(
                                key.as_str(),
                                "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                            ) {
                                json!(dictionary[values[index].as_u64().unwrap() as usize])
                            } else {
                                values[index].clone()
                            };
                            result.insert(key.clone(), value);
                        }
                        tables
                            .entry(table.into())
                            .or_default()
                            .push(Value::Object(result));
                    }
                } else {
                    tables
                        .entry(table.into())
                        .or_default()
                        .extend(row["rows"].as_array().unwrap().iter().cloned());
                }
            } else if row["complete"] == true {
                tables.entry("completion".into()).or_default().push(row);
            }
        }
        json!(tables)
    };
    compare_json(&decode(borrowed), &decode(packed));
}
