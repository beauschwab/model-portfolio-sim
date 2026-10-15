use super::*;
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/fixtures/joint-deposit-derivative.json"
    ))
    .unwrap()
}
fn controlled(f: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    scope(Some(Control::new(30000).unwrap()), f)
}
#[test]
fn raw_joint_one_pool_complete_sources() {
    controlled(|| {
        let r = raw();
        let encoded = execute(&serde_json::to_vec(&r).unwrap())?;
        let result: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(result["original_request"], r);
        assert_eq!(result["raw_portfolio"], r["portfolio"]);
        assert_eq!(result["raw_derivative_owners"], r["derivative_owners"]);
        assert_eq!(
            result["portfolio_sources"]["owners"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(result["native_sources"].as_array().unwrap().len(), 2);
        let stream = result["native_stream"].as_str().unwrap();
        assert_eq!(
            stream.lines().last(),
            Some("{\"complete\":true,\"protocol\":3}")
        );
        assert!(stream.contains("derivative_journal_lineage"));
        assert!(stream.contains("deposit_principal_lineage"));
        assert!(
            result["admission"]["joined_retained_upper"]
                .as_u64()
                .unwrap()
                <= MAX_RETAINED as u64
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn missing_run_and_opening_plug_refuse_before_header() {
    for change in 0..2 {
        let mut r = raw();
        if change == 0 {
            r["derivative_owners"][0]["runs"]
                .as_array_mut()
                .unwrap()
                .pop();
        } else {
            r["portfolio"]["specification"]["accounts"][0]["equity"] = json!(999.);
        }
        let bytes = serde_json::to_vec(&json!({"request":r,"execution_timeout_ms":30000})).unwrap();
        let mut output = Vec::new();
        assert!(run(&bytes, &mut output).is_err());
        assert!(output.is_empty());
    }
}
#[test]
fn invalid_default_and_ownership_refuse() {
    controlled(|| {
        let mut r = raw();
        r["derivative_owners"][0]["runs"][0]["saved"]["physical"]["derivative"]["operation"]
            ["request"]["paths"][0]["default"] = json!({});
        assert!(execute(&serde_json::to_vec(&r).unwrap()).is_err());
        let mut r = raw();
        r["policy"]["ownership"] = json!("concat_standalone");
        assert!(execute(&serde_json::to_vec(&r).unwrap())
            .unwrap_err()
            .contains("ownership"));
        Ok(())
    })
    .unwrap();
}
#[test]
fn cancellation_and_duplicate_no_output() {
    let control = Control::new(30000).unwrap();
    control.cancel();
    assert!(scope(Some(control), || execute(
        &serde_json::to_vec(&raw()).unwrap()
    ))
    .is_err());
    let mut output = Vec::new();
    let bytes=br#"{"request":{"schema":"saved-derivative-deposit-portfolio-mixed-1","schema":"x"},"execution_timeout_ms":30000}"#;
    assert!(run(bytes, &mut output).unwrap_err().contains("duplicate"));
    assert!(output.is_empty());
}
#[test]
fn public_flush_cancellation_rejects_after_native_completion() {
    struct Cancel;
    impl Write for Cancel {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            portfolio_compute_control::active().unwrap().cancel();
            Ok(())
        }
    }
    let b = serde_json::to_vec(&json!({"request":raw(),"execution_timeout_ms":30000})).unwrap();
    assert!(run(&b, &mut Cancel)
        .unwrap_err()
        .contains("COMPUTE_CANCELLED"));
}
#[test]
fn inherited_cancelled_or_expired_parent_never_parses_or_emits() {
    let cancelled = Control::new(30000).unwrap();
    cancelled.cancel();
    let expired = Control::from_started(
        1,
        std::time::Instant::now() - std::time::Duration::from_secs(2),
    )
    .unwrap();
    let bytes = b"not JSON";
    for control in [cancelled, expired] {
        let mut out = Vec::new();
        let e = scope(Some(control), || run(bytes, &mut out)).unwrap_err();
        assert!(e.starts_with("COMPUTE_"));
        assert!(out.is_empty());
    }
}

#[test]
fn dispatcher_entry_elapsed_budget_cannot_be_renewed_by_decode() {
    let bytes = serde_json::to_vec(&json!({"request":raw(),"execution_timeout_ms":1})).unwrap();
    let mut out = Vec::new();
    let original_entry = std::time::Instant::now() - std::time::Duration::from_secs(2);
    assert!(run_started(&bytes, &mut out, original_entry)
        .unwrap_err()
        .contains("COMPUTE_DEADLINE_EXCEEDED"));
    assert!(out.is_empty());
}

#[test]
fn actual_work_boundary_refusal_calls_no_financial_generator() {
    controlled(|| {
        let compact = serde_json::to_vec(&raw()).unwrap();
        let r: Request = serde_json::from_slice(&compact).unwrap();
        let opening = shared::opening_anchors(&r.derivative_owners)?;
        let da = deposit::audit_with_native_opening(&r.portfolio, &opening)?;
        let (dw, _, _) = derivative_upper(&r, 41)?;
        let initial = add(mul(compact.len(), 8)?, add(da.work_units, dw)?)?;
        assert!(initial < owner::MAX_WORK);
        // Legal JSON whitespace changes neither typed contracts nor monetary inputs.
        // execute charges actual raw byte parsing work, crossing the REAL8M limit.
        let pad = (owner::MAX_WORK - initial) / 8 + 1;
        let mut bytes = compact;
        bytes.resize(bytes.len() + pad, b' ');
        GENERATOR_CALLS.with(|c| c.set([0, 0]));
        let e = execute(&bytes).unwrap_err();
        assert!(
            e.contains("joint pre-generation8M/128MiB reservation exceeded"),
            "{e}"
        );
        assert_eq!(GENERATOR_CALLS.with(|c| c.get()), [0, 0]);
        Ok(())
    })
    .unwrap();
}

#[test]
fn public_original_value_work_refusal_calls_no_financial_generator() {
    controlled(|| {
        let mut r = raw();
        r["policy"]["identity"]["revision"] = json!("x".repeat(300000));
        let bytes = serde_json::to_vec(&json!({"request":r,"execution_timeout_ms":30000})).unwrap();
        GENERATOR_CALLS.with(|c| c.set([0, 0]));
        shared::SOURCE_GENERATOR_CALLS.with(|c| c.set(0));
        let mut output = Vec::new();
        let e = run(&bytes, &mut output).unwrap_err();
        assert!(
            e.contains("joint pre-generation8M/128MiB reservation exceeded"),
            "{e}"
        );
        assert_eq!(GENERATOR_CALLS.with(|c| c.get()), [0, 0]);
        assert_eq!(shared::SOURCE_GENERATOR_CALLS.with(|c| c.get()), 0);
        assert!(output.is_empty());
        Ok(())
    })
    .unwrap();
}
