//! Independent acquisition admission and pooled actual-date conservation fixtures.
use portfolio_ledger_native::{
    acquisition,
    dated_ledger::{CompiledDatedLedger, DatedLedgerRequest},
};
use serde_json::{json, Value};

fn mixed_raw() -> Value {
    let candidate: Value = serde_json::from_str(include_str!(
        "../../../examples/models/product-candidate-journal-1.json"
    ))
    .unwrap();
    let mut backbook: Value =
        serde_json::from_str(include_str!("../../../examples/models/dated-ledger-1.json")).unwrap();
    let start = candidate["as_of_ordinal"].as_i64().unwrap();
    let report = candidate["reporting_end_ordinal"].as_i64().unwrap();
    backbook["specification"] = candidate["specification"].clone();
    backbook["specification"]["accounts"][0]["equity"] = json!(1020.);
    backbook["specification"]["positions"] = json!([{"id":"backbook","account":"bank","kind":"loan","classification":"ac","balance":20.,"rate":0.,"maturity_day":0}]);
    backbook["dated"]["as_of_ordinal"] = json!(start);
    backbook["dated"]["runs"][1]["scenario_id"] = json!("stress");
    backbook["dated"]["reporting_horizon_days"] = json!(report - start);
    backbook["dated"]["owners"][0]["position_id"] = json!("backbook");
    backbook["dated"]["owners"][0]["account_id"] = json!("bank");
    let source = &mut backbook["dated"]["sources"][0];
    source["coverage"][0]["position_id"] = json!("backbook");
    source["schedule"]["as_of_ordinal"] = json!(start);
    source["schedule"]["horizon_days"] = json!(75);
    let event = &mut source["schedule"]["events"][0];
    event["position_id"] = json!("backbook");
    event["payment_ordinal"] = json!(start + 75);
    event["day_offset"] = json!(75);
    event["accrual_period"] = json!({"start_ordinal":start,"end_ordinal":start+75});
    event["components"]["principal"] = json!(20.);
    event["components"]["cash_interest"] = json!(0.75);
    event["components"]["accrued_interest"] = json!(0.75);
    backbook["dated"]["accrual_plans"][0]["position_id"] = json!("backbook");
    backbook["dated"]["accrual_plans"][0]["segments"][0] = json!({"start_ordinal":start,"end_ordinal":start+75,"accrued_interest":0.75,"book_amortization":0.});
    let mut combined = backbook["specification"].clone();
    combined["positions"].as_array_mut().unwrap().extend(
        candidate["specification"]["positions"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    json!({"schema":"product-mixed-dated-1","candidate":{"product":candidate["product"],"allocation":candidate["allocation"],"owners":candidate["owners"],"policy":candidate["policy"],"run_mapping":candidate["run_mapping"]},"backbook":backbook,"combined_specification":combined,"policy":{"identity":{"id":"reviewed-mixed","revision":"r1"},"expectation":"separately_supplied_physical_expected_paths-1","phase":"opening_acquisition_recognition_settlement-1","backbook_basis":"reviewed_supplied_dated_recognition-1","tail":"retain_complete-1","fees":"reject","defaults":"reject","marks":"reject"},"opening_identity":{"id":"actual-opening","revision":"r1"}})
}

fn mixed_prepare(
    raw: Value,
) -> Result<portfolio_decision_native::product_mixed_dated::Prepared, String> {
    portfolio_decision_native::product_mixed_dated::prepare(&serde_json::from_value(raw).unwrap())
}

#[test]
fn mixed_actual_opening_source_run_and_reverse_ownership_fail_closed() {
    for case in 0..6 {
        let mut raw = mixed_raw();
        match case {
            0 => raw["combined_specification"]["accounts"][0]["cash"] = json!(1001.),
            1 => raw["combined_specification"]["positions"][1]["balance"] = json!(1.),
            2 => raw["candidate"]["owners"][0]["source"] = json!({"id":"source","revision":"r1"}),
            3 => raw["candidate"]["run_mapping"][1]["ledger_scenario_id"] = json!("missing"),
            4 => {
                raw["combined_specification"]["reverse_severities"] = json!([1.]);
                raw["backbook"]["specification"]["reverse_severities"] = json!([1.]);
            }
            _ => raw["candidate"]["owners"][0]["position_id"] = json!("backbook"),
        }
        assert!(mixed_prepare(raw).is_err(), "case {case}");
    }
}

#[test]
fn mixed_opposite_backbook_rights_cannot_cancel_complete_tail_ownership() {
    let mut raw = mixed_raw();
    raw["backbook"]["dated"]["sources"][0]["schedule"]["events"][0]["components"]
        ["cash_interest"] = json!(0.);
    raw["backbook"]["dated"]["sources"][0]["schedule"]["events"][0]["components"]
        ["accrued_interest"] = json!(1.);
    raw["backbook"]["dated"]["accrual_plans"][0]["segments"][0]["accrued_interest"] = json!(1.);
    let mut negative = raw["backbook"]["dated"]["sources"][0]["schedule"]["events"][0].clone();
    negative["event_id"] = json!("opposing-right");
    negative["components"]["principal"] = json!(0.);
    negative["components"]["accrued_interest"] = json!(-1.);
    raw["backbook"]["dated"]["sources"][0]["schedule"]["events"]
        .as_array_mut()
        .unwrap()
        .push(negative);
    let mut plan = raw["backbook"]["dated"]["accrual_plans"][0].clone();
    plan["event_id"] = json!("opposing-right");
    plan["segments"][0]["accrued_interest"] = json!(-1.);
    raw["backbook"]["dated"]["accrual_plans"]
        .as_array_mut()
        .unwrap()
        .push(plan);
    assert!(mixed_prepare(raw).is_err());
}

#[test]
fn mixed_report_does_not_retire_or_truncate_backbook_tail() {
    let raw = mixed_raw();
    let start = raw["backbook"]["dated"]["as_of_ordinal"].as_i64().unwrap() as i32;
    let out = mixed_prepare(raw.clone()).unwrap();
    assert_eq!(out.reporting_end_ordinal, start + 60);
    assert_eq!(out.full_end_ordinal, start + 75);
    assert_eq!(out.combined_dated_request.dated.reporting_horizon_days, 75);
    let typed: portfolio_decision_native::product_mixed_dated::Request =
        serde_json::from_value(raw).unwrap();
    assert_eq!(out.raw_request, serde_json::to_value(typed).unwrap());
    assert_eq!(
        out.combined_dated_request.dated.sources[0].schedule.events[0].payment_ordinal,
        start + 75
    );
}

#[test]
fn mixed_complete_cash_has_one_actual_opening_and_no_standalone_equity() {
    let raw = mixed_raw();
    let cost: f64 = raw["candidate"]["allocation"]
        .as_array()
        .unwrap()
        .iter()
        .zip(raw["candidate"]["product"]["units"].as_array().unwrap())
        .map(|(a, u)| {
            a["face"].as_f64().unwrap() * u["quote_price_per_face_unit"].as_f64().unwrap()
        })
        .sum();
    let result = portfolio_decision_native::product_mixed_dated::evaluate(
        &serde_json::from_value(raw).unwrap(),
    )
    .unwrap();
    for scenario in ["baseline", "stress"] {
        let cash: f64 = result
            .trial_balance
            .iter()
            .filter(|r| r.scenario == scenario && r.gl_account == "cash")
            .map(|r| r.balance)
            .sum();
        // Independent raw principal/coupons: candidates104.45, backbook20.75.
        assert!((cash - (1000. - cost + 104.45 + 20.75)).abs() < 1e-9);
        let opening_cash: f64 = result
            .journal
            .iter()
            .filter(|r| {
                r.scenario == scenario
                    && r.day == 0
                    && r.event == "opening"
                    && r.gl_account == "cash"
            })
            .map(|r| r.value)
            .sum();
        assert_eq!(opening_cash, 1000.);
        let opening_equity: f64 = result
            .journal
            .iter()
            .filter(|r| {
                r.scenario == scenario
                    && r.day == 0
                    && r.event == "opening"
                    && r.gl_account == "opening_equity"
            })
            .map(|r| r.value)
            .sum();
        assert_eq!(opening_equity, -1020.);
    }
    assert!(result.complete);
    assert!(!result.independent_persisted_verification && !result.dynamic_mixed_acceptance);
}

#[test]
fn mixed_cancellation_or_output_error_never_returns_success() {
    let request =
        serde_json::from_value::<portfolio_decision_native::product_mixed_dated::Request>(
            mixed_raw(),
        )
        .unwrap();
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let mut output = Vec::new();
    let error = portfolio_compute_control::scope(Some(control), || {
        portfolio_decision_native::product_mixed_dated::run(&request, &mut output)
    })
    .err()
    .unwrap();
    assert!(error.contains("COMPUTE_CANCELLED"));
    assert!(output.is_empty());
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "independent fixture sink failure",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(portfolio_decision_native::product_mixed_dated::run(&request, Broken).is_err());
}

fn dated() -> Value {
    let mut raw: Value =
        serde_json::from_str(include_str!("../../../examples/models/dated-ledger-1.json")).unwrap();
    let start = raw["dated"]["as_of_ordinal"].as_i64().unwrap();
    raw["specification"]["accounts"][0]["cash"] = json!(1000.);
    raw["specification"]["accounts"][0]["equity"] = json!(1000.);
    raw["specification"]["positions"][0]["balance"] = json!(0.);
    raw["dated"]["owners"][0]["ownership"] = json!("fully_scenario_generated");
    raw["dated"]["sources"][0]["schedule"]["events"][0]["measure"] = json!("physical");
    raw["dated"]["sources"][0]["schedule"]["events"][0]["provenance"] = json!({"kind":"modeled","model_id":"independent-acquisition-fixture","model_version":"1","input_revision":"r1"});
    raw["dated"]["sources"][0]["schedule"]["events"][0]["accrual_period"]["start_ordinal"] =
        json!(start + 10);
    raw["dated"]["sources"][0]["schedule"]["events"][0]["components"]["accrued_interest"] =
        json!(0.2);
    raw["dated"]["sources"][0]["schedule"]["events"][0]["components"]["cash_interest"] = json!(0.2);
    raw["dated"]["accrual_plans"][0]["segments"][0]["start_ordinal"] = json!(start + 10);
    raw["dated"]["accrual_plans"][0]["segments"][0]["accrued_interest"] = json!(0.2);
    raw
}
fn acquisitions(raw: &Value) -> Value {
    let start = raw["dated"]["as_of_ordinal"].as_i64().unwrap();
    let events: Vec<_> = raw["dated"]["runs"].as_array().unwrap().iter().map(|run| json!({
        "event_id":"purchase","run_id":run["run_id"],"position_id":"p","account_id":"a","currency":"USD",
        "source":{"id":"source","revision":"r1"},"path_id":"p1","source_weight":1.,"allocation_factor":1.,
        "principal":100.,"cost":100.,"ordinal":start+10,"original_yield_anchor":{"id":"original-yield","revision":"r1"},"measure":"physical","recognition_event_ids":["coupon"],"coupon_sign":1.
    })).collect();
    json!({"schema":"dated-acquisition-1","policy":{"id":"reviewed-acquisition","revision":"r1"},"right_ownership":"physical_path_net_rights-1","events":events})
}
fn compile(raw: Value, acquisitions: Value) -> Result<CompiledDatedLedger, String> {
    CompiledDatedLedger::compile_with_acquisitions(
        serde_json::from_value::<DatedLedgerRequest>(raw).unwrap(),
        &serde_json::from_value::<acquisition::Request>(acquisitions).unwrap(),
        60000,
        90_000_000,
    )
}
#[test]
fn purchase_precedes_all_recognition_and_settlement_even_when_total_face_is_available() {
    for early_cash in [false, true] {
        let mut raw = dated();
        let acquisitions = acquisitions(&raw);
        let start = raw["dated"]["as_of_ordinal"].as_i64().unwrap();
        if early_cash {
            let event = &mut raw["dated"]["sources"][0]["schedule"]["events"][0];
            event["payment_ordinal"] = json!(start + 9);
            event["day_offset"] = json!(9);
        } else {
            raw["dated"]["sources"][0]["schedule"]["events"][0]["accrual_period"]
                ["start_ordinal"] = json!(start + 9);
            raw["dated"]["accrual_plans"][0]["segments"][0]["start_ordinal"] = json!(start + 9);
        }
        assert!(compile(raw, acquisitions)
            .err()
            .unwrap()
            .contains("precedes actual purchase"));
    }
}
#[test]
fn acquisition_measure_opening_provenance_coverage_and_identity_are_admitted_explicitly() {
    for case in 0..9 {
        let mut raw = dated();
        let mut acquisitions = acquisitions(&raw);
        match case {
            0 => acquisitions["events"][0]["measure"] = json!("pricing"),
            1 => acquisitions["events"][0]["source"]["revision"] = json!("changed"),
            2 => acquisitions["events"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            3 => {
                let duplicate = acquisitions["events"][0].clone();
                acquisitions["events"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            4 => {
                raw["specification"]["positions"][0]["balance"] = json!(1.);
                raw["specification"]["accounts"][0]["equity"] = json!(1001.);
            }
            5 => acquisitions["events"][0]["run_id"] = json!("uncovered"),
            6 => acquisitions["events"][0]["account_id"] = json!("unowned"),
            7 => acquisitions["events"][0]["coupon_sign"] = json!(0.),
            _ => acquisitions["events"][0]["coupon_sign"] = json!(-1.),
        }
        assert!(compile(raw, acquisitions).is_err(), "case {case}");
    }
}
#[test]
fn a_second_purchase_date_or_missing_terminal_principal_has_no_implicit_owner() {
    let mut raw = dated();
    let mut empty = raw["dated"]["sources"][0]["schedule"]["events"][0].clone();
    empty["event_id"] = json!("second-zero-event");
    empty["components"]["principal"] = json!(0.);
    empty["components"]["cash_interest"] = json!(0.);
    empty["components"]["accrued_interest"] = json!(0.);
    raw["dated"]["sources"][0]["schedule"]["events"]
        .as_array_mut()
        .unwrap()
        .push(empty);
    let mut plan = raw["dated"]["accrual_plans"][0].clone();
    plan["event_id"] = json!("second-zero-event");
    plan["segments"][0]["accrued_interest"] = json!(0.);
    raw["dated"]["accrual_plans"]
        .as_array_mut()
        .unwrap()
        .push(plan);
    let mut acquisitions = acquisitions(&raw);
    let mut extra = acquisitions["events"][0].clone();
    extra["event_id"] = json!("second-purchase");
    extra["ordinal"] = json!(extra["ordinal"].as_i64().unwrap() + 1);
    extra["principal"] = json!(0.);
    extra["cost"] = json!(0.);
    extra["recognition_event_ids"] = json!(["second-zero-event"]);
    acquisitions["events"].as_array_mut().unwrap().push(extra);
    assert!(compile(raw.clone(), acquisitions)
        .err()
        .unwrap()
        .contains("one exact purchase date"));
    let mut missing = dated();
    missing["dated"]["sources"][0]["schedule"]["events"][0]["components"]["principal"] = json!(99.);
    assert!(compile(missing, acquisitions_for(&dated()))
        .err()
        .unwrap()
        .contains("principal tail remains unowned"));
}
fn acquisitions_for(raw: &Value) -> Value {
    acquisitions(raw)
}

#[test]
fn cancellation_during_admission_returns_no_compiled_state() {
    let raw = dated();
    let acquisitions = acquisitions(&raw);
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let error = portfolio_compute_control::scope(Some(control), || compile(raw, acquisitions))
        .err()
        .unwrap();
    assert_eq!(error, "COMPUTE_CANCELLED");
}

#[test]
fn sparse_mapping_retains_explicit_empty_owners_and_acquisition_amounts_once() {
    let mut raw = dated();
    // Only p is acquired. These explicit, unrelated zero-opening owners must not
    // be assigned its acquisition, source history or rights.
    for i in 0..255 {
        let id = format!("idle-{i}");
        let mut position = raw["specification"]["positions"][0].clone();
        position["id"] = json!(id);
        raw["specification"]["positions"]
            .as_array_mut()
            .unwrap()
            .push(position);
        let mut owner = raw["dated"]["owners"][0].clone();
        owner["position_id"] = json!(id);
        raw["dated"]["owners"].as_array_mut().unwrap().push(owner);
        raw["dated"]["sources"][0]["coverage"].as_array_mut().unwrap().push(json!({
            "position_id":id,"run":{"kind":"all"},
            "status":{"kind":"no_events","rationale":"explicit zero opening without acquisitions"}
        }));
    }
    let mut acq = acquisitions(&raw);
    // Amounts have already been weighted by the source producer. Metadata must
    // never multiply them a second time while preparing the ledger store.
    for event in acq["events"].as_array_mut().unwrap() {
        event["source_weight"] = json!(0.25);
        event["allocation_factor"] = json!(0.4);
        event["cost"] = json!(101.);
    }
    let compiled = compile(raw, acq).unwrap();
    assert_eq!(compiled.store.owners().len(), 256);
    let acq = compiled.acquisitions.as_ref().unwrap();
    assert_eq!(acq.events.len(), 2);
    for run in 0..2 {
        let events: Vec<_> = acq.on(run, 10).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].input.principal, 100.);
        assert_eq!(events[0].input.cost, 101.);
        assert_eq!(events[0].input.source_weight, 0.25);
        assert_eq!(events[0].input.allocation_factor, 0.4);
        assert_eq!(events[0].position_index, 0);
    }
}

#[test]
fn impossible_weight_amount_and_acquisition_capacity_are_rejected() {
    for case in 0..4 {
        let raw = dated();
        let mut acq = acquisitions(&raw);
        match case {
            0 => acq["events"][0]["source_weight"] = json!(0.),
            1 => acq["events"][0]["cost"] = json!(1e15 + 1.),
            2 => acq["events"][0]["allocation_factor"] = json!(-1.),
            _ => {
                let event = acq["events"][0].clone();
                acq["events"] = json!(vec![event; acquisition::MAX_EVENTS + 1]);
            }
        }
        assert!(compile(raw, acq).is_err(), "case {case}");
    }
}
