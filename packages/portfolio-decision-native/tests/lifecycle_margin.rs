//! Stage I (issue #51) through the native coordinator and the daily Sim: the worked unwind with an
//! offsetting receiver swap under the research IM method. Sensitivities are native; the requirement
//! follows the live legal portfolio, and the independent verifier recomputes both.
use portfolio_decision_native::workflows::lifecycle::acquisition::run;
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

const UNWIND: &str = include_str!("fixtures/lifecycle-unwind.json");

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}

/// The unwind with `offset-lot` held from the opening (a receiver on the same terms, carried at
/// 0) and the CSA switched to `research-sensitivity-im-1`, recomputed on the termination date.
fn research(terminate: bool) -> Value {
    let mut v: Value = serde_json::from_str(UNWIND).unwrap();
    for l in v["plan"]["lots"].as_array_mut().unwrap() {
        if l["lot_id"] == "offset-lot" {
            l["acquisition_ordinal"] = json!(738800);
        }
    }
    let a = &mut v["plan"]["margin_agreements"][0];
    a["initial_margin"] = json!("research_sensitivity");
    a["margin_method"] = json!({"method": "research-sensitivity-im-1", "revision": "r1",
        "bucket_upper_days": [365, 3650], "risk_weights": [5000.0, 5000.0],
        "correlations": [[1.0, 0.3], [0.3, 1.0]], "two_way": false,
        "review": {"id": "im-method-review", "revision": "r1"}});
    for acq in v["acquisitions"].as_array_mut().unwrap() {
        acq["initial_margin"] = json!([]);
        acq["dynamic_margin"] = json!([{"agreement_id": "swap-csa", "ordinal": 738887}]);
        if !terminate {
            acq["terminations"][0]["state"] = json!("failed");
        }
    }
    let mut offset = v["hedge_contracts"][0].clone();
    offset["lot_id"] = json!("offset-lot");
    offset["swap"]["position_id"] = json!("swap-offset");
    offset["swap"]["side"] = json!("receiver");
    v["hedge_contracts"].as_array_mut().unwrap().push(offset);
    v
}

fn execute(v: &Value) -> Result<(Value, Vec<u8>), String> {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let receipt = run(
        &serde_json::to_vec(v).unwrap(),
        Box::new(Capture(bytes.clone())),
    )?;
    let stream = bytes.lock().unwrap().clone();
    Ok((serde_json::to_value(receipt).unwrap(), stream))
}

#[test]
fn removing_the_offset_raises_im_and_both_runs_verify_independently() {
    // Receiver: +(5% - 1%) x 3600 x 10/360 = +4 at DF 1, bumped by exp(-1e-4 x 10/365).
    let s = 4. * ((-0.0001_f64 * 10. / 365.).exp() - 1.);
    let mut requirements = Vec::new();
    for terminate in [true, false] {
        let v = research(terminate);
        let (receipt, stream) = execute(&v).unwrap();
        let verified =
            portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream)
                .unwrap();
        assert!(verified.verified && verified.independent_equations);
        assert_eq!(receipt["margin_sensitivities"].as_array().unwrap().len(), 2);
        for j in receipt["journals"].as_array().unwrap() {
            let call = &j["receipt"]["margin_calls"][0];
            let requirement = call["requirement"].as_f64().unwrap();
            if terminate {
                assert_eq!(call["live_lot_ids"], json!(["offset-lot"]));
                near(call["sensitivities"][0].as_f64().unwrap(), s);
                near(call["sensitivities"][1].as_f64().unwrap(), 0.);
                near(requirement, 5000. * s.abs());
            } else {
                // Payer and receiver on the same terms net exactly: the opening IM of 3 returns.
                assert_eq!(call["live_lot_ids"], json!(["swap-lot", "offset-lot"]));
                near(requirement, 0.);
            }
            near(call["posted_change"].as_f64().unwrap(), requirement - 3.);
            near(j["receipt"]["posted_im"].as_f64().unwrap(), requirement);
            requirements.push(requirement);
        }
        // A tampered requirement (a leading digit added to it) does not verify.
        let text = String::from_utf8(stream).unwrap();
        assert!(text.contains("\"requirement\":"));
        let tampered = text.replacen("\"requirement\":", "\"requirement\":1", 1);
        assert!(portfolio_verification_native::verify(
            &serde_json::to_vec(&v).unwrap(),
            tampered.as_bytes()
        )
        .is_err());
    }
    assert!(requirements[0] > requirements[2]);
}

#[test]
fn research_im_refuses_without_a_recomputation_or_a_live_sensitivity() {
    let mut v = research(true);
    for acq in v["acquisitions"].as_array_mut().unwrap() {
        acq["dynamic_margin"] = json!([]);
    }
    let err = execute(&v).unwrap_err();
    assert!(err.contains("MARGIN_REFRESH_REQUIRED"), "{err}");
    let mut v = research(true);
    v["hedge_contracts"].as_array_mut().unwrap().pop();
    let err = execute(&v).unwrap_err();
    assert!(
        err.contains("MARGIN_REFRESH_REQUIRED") && err.contains("offset-lot"),
        "{err}"
    );
}

#[test]
fn the_large_tier_is_explicit() {
    // An unknown tier refuses; the explicit tier runs the same economics and records itself.
    let mut v = research(true);
    v["scale_tier"] = json!("unbounded");
    let err = execute(&v).unwrap_err();
    assert!(err.contains("UNSUPPORTED_POLICY"), "{err}");
    v["scale_tier"] = json!("acceptance_60000");
    let (tiered, _) = execute(&v).unwrap();
    let (plain, _) = execute(&research(true)).unwrap();
    assert_eq!(tiered["scale_tier"], "acceptance_60000");
    assert!(plain.get("scale_tier").is_none());
    assert_eq!(tiered["journals"], plain["journals"]);
    // Without the tier a request above 8 MiB refuses before any financial work.
    let mut big = research(true);
    big["plan"]["note"] = json!("x".repeat(9 * 1024 * 1024));
    let err = execute(&big).unwrap_err();
    assert!(err.contains("BOUND_EXCEEDED"), "{err}");
}
