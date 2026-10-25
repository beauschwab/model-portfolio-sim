//! Stage I (issue #51): research-method IM on the live legal portfolio. Offsetting swaps net;
//! removing an offset raises the remaining requirement; a population change needs a recompute on
//! that date and a sensitivity for every live lot; received IM is segregated custody only.
use portfolio_model_core::lifecycle::acquisition::{
    self, AcquisitionRequest, ResolvedHedge, Sensitivity, MARGIN_REFRESH_REQUIRED,
};
use portfolio_model_core::lifecycle::{self, *};
use serde_json::{json, Value};

const DAY: i32 = 738887;

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle/execution-unwind.json")).unwrap()
}
/// The worked unwind with a second, offsetting receiver swap in the same CSA under the method.
fn offset() -> Value {
    let mut v = raw();
    let mut second = v["plan"]["lots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["lot_id"] == "swap-lot")
        .unwrap()
        .clone();
    second["lot_id"] = json!("swap-2");
    second["position_id"] = json!("swap-2");
    second["original_cost"] = json!(0.0);
    v["plan"]["lots"].as_array_mut().unwrap().push(second);
    let a = &mut v["plan"]["margin_agreements"][0];
    a["lot_ids"].as_array_mut().unwrap().push(json!("swap-2"));
    a["initial_margin"] = json!("research_sensitivity");
    a["margin_method"] = json!({"method": "research-sensitivity-im-1", "revision": "r1",
        "bucket_upper_days": [365, 3650], "risk_weights": [10.0, 10.0],
        "correlations": [[1.0, 0.5], [0.5, 1.0]], "two_way": false,
        "review": {"id": "im-method-review", "revision": "r1"}});
    v["execution"]["initial_margin"] = json!([]);
    v["execution"]["dynamic_margin"] = json!([{"agreement_id": "swap-csa", "ordinal": DAY}]);
    v
}
fn sensitivities() -> Vec<Sensitivity> {
    vec![
        Sensitivity {
            lot_id: "swap-lot".into(),
            ordinal: DAY,
            buckets: vec![-0.05, -0.03],
        },
        Sensitivity {
            lot_id: "swap-2".into(),
            ordinal: DAY,
            buckets: vec![0.05, 0.03],
        },
    ]
}
fn run_with(v: &Value, s: &[Sensitivity]) -> Result<acquisition::AcquisitionJournal, Refusal> {
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap())?;
    let acq: AcquisitionRequest = serde_json::from_value(v["execution"].clone())
        .map_err(|e| Refusal::new(MALFORMED_INPUT, e.to_string()))?;
    let hedges = vec![ResolvedHedge {
        lot_id: "swap-lot".into(),
        ordinal: DAY,
        clean_value: -4.,
        due_cash: 0.,
        pricing_source: Revision {
            id: "swap-pricer".into(),
            revision: "r1".into(),
        },
    }];
    acquisition::replay_with_margin(&plan, &acq, &hedges, s, "a")
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}

#[test]
fn removing_an_offset_raises_the_remaining_requirement() {
    // The swap is terminated: only the receiver remains live and its risk is unhedged.
    let j = run_with(&offset(), &sensitivities()).unwrap();
    let call = &j.receipt.margin_calls[0];
    assert_eq!(call.live_lot_ids, ["swap-2"]);
    near(call.requirement, 0.7); // sqrt(0.5^2 + 0.3^2 + 2*0.5*0.5*0.3)
    near(call.posted_change, 0.7 - 3.0); // the opening 3.0 posted IM is reset to the requirement
    near(j.receipt.posted_im, 0.7);
    // The termination fails: both swaps stay live, offset, and the requirement is zero.
    let mut kept = offset();
    kept["execution"]["terminations"][0]["state"] = json!("failed");
    let j = run_with(&kept, &sensitivities()).unwrap();
    let call = &j.receipt.margin_calls[0];
    assert_eq!(call.live_lot_ids, ["swap-lot", "swap-2"]);
    near(call.requirement, 0.);
    assert!(j.receipt.margin_calls[0].requirement < 0.7);
}

#[test]
fn population_changes_need_a_recompute_and_every_live_sensitivity() {
    let mut v = offset();
    v["execution"]["dynamic_margin"] = json!([]);
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        MARGIN_REFRESH_REQUIRED
    );
    let r = run_with(&offset(), &sensitivities()[..1]).unwrap_err();
    assert_eq!(r.code, MARGIN_REFRESH_REQUIRED);
    assert!(r.detail.contains("swap-2"), "{}", r.detail);
    // Supplied requirements cannot be mixed into a research-method agreement.
    let mut v = offset();
    v["execution"]["initial_margin"] = raw()["execution"]["initial_margin"].clone();
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        UNSUPPORTED_POLICY
    );
}

#[test]
fn methods_are_explicit_and_two_way_im_is_segregated() {
    let mut v = offset();
    v["plan"]["margin_agreements"][0]["margin_method"]["method"] = json!("isda-simm");
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        UNSUPPORTED_POLICY
    );
    let mut v = offset();
    v["plan"]["margin_agreements"][0]["margin_method"]["correlations"] =
        json!([[1.0, 0.5], [0.4, 1.0]]);
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        MALFORMED_INPUT
    );
    let mut v = offset();
    v["plan"]["margin_agreements"][0]["initial_margin"] = json!("supplied_requirements");
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        MALFORMED_INPUT
    );
    let mut v = offset();
    v["plan"]["margin_agreements"][0]["margin_method"]["two_way"] = json!(true);
    v["plan"]["margin_agreements"][0]["segregation"] = json!("reusable");
    assert_eq!(
        run_with(&v, &sensitivities()).unwrap_err().code,
        UNSUPPORTED_POLICY
    );
    let mut v = offset();
    v["plan"]["margin_agreements"][0]["margin_method"]["two_way"] = json!(true);
    let j = run_with(&v, &sensitivities()).unwrap();
    near(j.receipt.margin_calls[0].received_change, 0.7);
    near(j.receipt.received_im_custody, 0.7);
}
