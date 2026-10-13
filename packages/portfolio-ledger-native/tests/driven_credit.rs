use portfolio_ledger_native::credit_path_replay::{self, CompiledPathLedger};
use portfolio_ledger_native::driven_credit::{self, Request, UsePolicy};
use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_calibration::DataKind;
use portfolio_model_core::credit_covariate::{self, CovariateFitRequest, ProjectionUse};
use serde_json::{json, Value};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/driven-credit-ledger-1.json"
    ))
    .unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn synthetic_macro_hazards_crn_exact_events_and_native_journal_close_by_hand() {
    let q = request();
    let raw = serde_json::to_vec(&q).unwrap();
    let prepared = driven_credit::prepare(&q).unwrap();
    assert_eq!(serde_json::to_vec(&q).unwrap(), raw);
    assert_eq!(
        serde_json::to_vec(&prepared.receipt.raw_source).unwrap(),
        raw
    );
    assert!(prepared.receipt.research_only);
    assert!(!prepared.receipt.adoption_output);
    assert!(!prepared.receipt.empirical_validation);
    assert!(!prepared.receipt.regulatory_compliance);
    assert_eq!(prepared.receipt.projections.len(), 2);
    assert!(prepared.receipt.ledger_admitted_work > 0);
    for (i, run) in prepared.ledger.runs.iter().enumerate() {
        let p = &run.batches[0].request.path;
        close(
            p.intervals[0].hazards_per_year[0][3],
            if i == 0 { 0.25 } else { 0.5 },
        );
        assert_eq!(
            serde_json::to_vec(&p.draw_stream).unwrap(),
            serde_json::to_vec(&q.runs[i].batches[0].template.path.draw_stream).unwrap()
        );
        assert_eq!(p.paths[0].path_key, 17);
        let economics = portfolio_model_core::credit_path::evaluate(p).unwrap();
        close(
            economics.paths[0].default.as_ref().unwrap().year,
            if i == 0 { 0.5 / 365. } else { 0.25 / 365. },
        );
        assert_eq!(economics.paths[0].random_pairs_used, 1);
    }
    let mut output = Vec::new();
    credit_path_replay::run(
        CompiledPathLedger::compile(prepared.ledger).unwrap(),
        &mut output,
    )
    .unwrap();
    let records = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records.last().unwrap(),
        &json!({"complete":true,"protocol":5})
    );
    let summaries = records
        .iter()
        .filter(|r| r["table"] == "summary")
        .flat_map(|r| r["rows"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2);
    for (i, row) in summaries.iter().enumerate() {
        let expected = if i == 0 { 66.05 } else { 66.025 };
        close(row["final_cash"].as_f64().unwrap(), expected);
        close(row["final_assets"].as_f64().unwrap(), expected);
        close(row["final_equity"].as_f64().unwrap(), expected);
        close(row["final_liabilities"].as_f64().unwrap(), 0.);
    }
}

#[test]
fn adoption_requires_native_eligible_model_not_synthetic_flag_relabel() {
    let mut q = request();
    q.use_policy = UsePolicy::Adoption {};
    for r in &mut q.runs {
        r.batches[0].projection.use_policy = ProjectionUse::Adoption;
    }
    assert!(driven_credit::prepare(&q)
        .unwrap_err()
        .contains("ineligible"));
    // Observed is a test observation contract, not evidence that fixture history is bank data.
    let mut fit: CovariateFitRequest = serde_json::from_str(include_str!(
        "../../../examples/models/credit-covariate-1.json"
    ))
    .unwrap();
    fit.data_kind = DataKind::Observed;
    let model = credit_covariate::fit(&fit).unwrap().model;
    assert!(model.adoption_eligible);
    for r in &mut q.runs {
        r.batches[0].projection.model = model.clone();
    }
    let prepared = driven_credit::prepare(&q).unwrap();
    assert!(prepared.receipt.adoption_output);
    assert!(!prepared.receipt.research_only);
    assert!(!prepared.receipt.empirical_validation);
}

#[test]
fn hazard_clock_crosses_driver_boundary_without_redrawing_or_rounding() {
    let mut q = request();
    for (i, run) in q.runs.iter_mut().enumerate() {
        let b = &mut run.batches[0];
        let mut second = b.projection.intervals[0].clone();
        let boundary = portfolio_model_core::credit_calibration::ObservationTime {
            ordinal: q.as_of_ordinal + 1,
            seconds_of_day: 0,
        };
        b.projection.intervals[0].end = boundary;
        second.start = boundary;
        second.interval_id = "second-driver-interval".into();
        second.covariates[0] = if i == 0 { 1.0 } else { 0.0 };
        b.projection.intervals.push(second);
        let second_policy = b.template.path.intervals[0].clone();
        b.template.path.intervals[0].end_time = boundary;
        b.template.path.intervals.push(second_policy);
        if let portfolio_model_core::credit_path::DrawStream::Supplied { paths, .. } =
            &mut b.template.path.draw_stream
        {
            paths[0].pairs[0].waiting_uniform = (-0.001_f64).exp();
        }
    }
    let out = driven_credit::prepare(&q).unwrap();
    for (i, run) in out.ledger.runs.iter().enumerate() {
        let economics =
            portfolio_model_core::credit_path::evaluate(&run.batches[0].request.path).unwrap();
        let expected = if i == 0 {
            1.0 / 365.0 + (0.001 - 0.25 / 365.0) / 0.5
        } else {
            0.001 / 0.5
        };
        close(economics.paths[0].default.as_ref().unwrap().year, expected);
        close(economics.paths[0].earned_interest, 100.0 * 0.365 * expected);
        assert_eq!(economics.paths[0].random_pairs_used, 1);
    }
}

#[test]
fn templates_cannot_supply_or_override_hazards_or_extra_policy_fields() {
    let mut raw = serde_json::to_value(request()).unwrap();
    raw["runs"][0]["batches"][0]["template"]["path"]["intervals"][0]["hazards_per_year"] =
        json!(([[0; 5]; 3]));
    assert!(serde_json::from_value::<Request>(raw).is_err());
    let mut raw = serde_json::to_value(request()).unwrap();
    raw["use_policy"]["empirical_validation"] = json!(true);
    assert!(serde_json::from_value::<Request>(raw).is_err());
}

#[test]
fn strict_matching_calendars_scope_and_policy_modes_reject() {
    for failure in 0..9 {
        let mut q = request();
        let b = &mut q.runs[0].batches[0];
        match failure {
            0 => b.template.path.intervals[0].end_time.ordinal -= 1,
            1 => b.projection.origin_time.seconds_of_day = 1,
            2 => b.projection.end_time.ordinal -= 1,
            3 => b.projection.scenario_id = "wrong".into(),
            4 => b.projection.position_id = "wrong".into(),
            5 => b.template.path.measure = CreditMeasure::Pricing,
            6 => b.projection.use_policy = ProjectionUse::Adoption,
            7 => b.projection.intervals[0].available_time.ordinal += 1,
            _ => b.template.path.model_policy_id = "other-policy".into(),
        }
        assert!(
            driven_credit::prepare(&q).is_err(),
            "failure {failure} admitted"
        );
    }
}

#[test]
fn all_scenario_paths_preserve_supplied_or_counter_stream_and_opening() {
    for change in 0..3 {
        let mut q = request();
        let b = &mut q.runs[1].batches[0];
        match change {
            0 => b.template.path.paths[0].path_key = 18,
            1 => {
                b.template.path.paths[0].initial_live_state =
                    portfolio_model_core::credit::CreditState::Watch
            }
            _ => {
                if let portfolio_model_core::credit_path::DrawStream::Supplied {
                    stream_revision,
                    ..
                } = &mut b.template.path.draw_stream
                {
                    *stream_revision = "2".into();
                }
            }
        }
        assert!(driven_credit::prepare(&q).is_err());
    }
    let mut q = request();
    for r in &mut q.runs {
        r.batches[0].template.path.draw_stream =
            portfolio_model_core::credit_path::DrawStream::CounterSplitmix64V1 {
                stream_id: "common-counter".into(),
                stream_revision: "1".into(),
                seed: 17,
            };
    }
    let prepared = driven_credit::prepare(&q).unwrap();
    assert!(matches!(
        prepared.ledger.runs[0].batches[0].request.path.draw_stream,
        portfolio_model_core::credit_path::DrawStream::CounterSplitmix64V1 { seed: 17, .. }
    ));
}

#[test]
fn source_receipt_retains_driver_model_and_crn_identities_without_input_mutation() {
    let q = request();
    let a = driven_credit::prepare(&q).unwrap();
    let b = driven_credit::prepare(&q).unwrap();
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
    let receipt = &a.receipt.projections[0];
    assert_eq!(
        receipt.projection.coefficient_identity_sha256,
        q.runs[0].batches[0]
            .projection
            .model
            .coefficient_identity_sha256
    );
    assert_eq!(
        receipt.projection.intervals[0].source.id,
        "synthetic-driver-path"
    );
    assert_eq!(receipt.position_id, "loan");
    assert_eq!(receipt.path_key, 17);
    let mut q = q;
    q.runs[0].batches[0].projection.model.rows[0] =
        q.runs[0].batches[0].projection.model.rows[1].clone();
    assert!(driven_credit::prepare(&q)
        .unwrap_err()
        .contains("hash mismatch"));
}

#[test]
fn compilation_checks_opening_full_run_coverage_and_caller_admission() {
    let mut q = request();
    q.specification["reverse_severities"] = json!([0.5]);
    assert!(driven_credit::prepare(&q).unwrap_err().contains("coverage"));
    let mut q = request();
    q.specification["accounts"][0]["equity"] = json!(999.);
    assert!(driven_credit::prepare(&q).is_err());
    assert!(driven_credit::prepare_with_limits(&request(), 0, 90_000_000).is_err());
    assert!(driven_credit::prepare_with_limits(&request(), 2_000, 1).is_err());
}

#[test]
fn bounded_deadline_run_count_and_cooperative_cancellation() {
    let mut q = request();
    q.timeout_ms = 0;
    assert!(driven_credit::prepare(&q).is_err());
    let mut q = request();
    q.runs[0].batches[0].projection.timeout_ms = 3_600_001;
    assert!(driven_credit::prepare(&q).is_err());
    let mut q = request();
    q.runs[0].batches.clear();
    q.runs = vec![q.runs[0].clone(); driven_credit::MAX_BATCHES + 1];
    assert!(driven_credit::prepare(&q).is_err());
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    let error = portfolio_compute_control::scope(Some(c), || driven_credit::prepare(&request()))
        .unwrap_err();
    assert_eq!(error, "COMPUTE_CANCELLED");
}
