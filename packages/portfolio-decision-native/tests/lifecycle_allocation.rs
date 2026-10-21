//! Stage H (issue #50): prepared allocation over reviewed packages of the worked unwind. The
//! selected candidate is replayed exactly and its stream verifies independently; formal packages
//! are ineligible, never approximated; exhaustion returns no validated plan with the violation;
//! merging packages is structural and reproduces the original request.
use portfolio_decision_native::workflows::lifecycle::allocation::{merge, run};
use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};

const UNWIND: &str = include_str!("fixtures/lifecycle-unwind.json");
const KINDS: [&str; 6] = [
    "facility_draws",
    "repayments",
    "sales",
    "facility_repayments",
    "terminations",
    "initial_margin",
];

struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn unwind() -> Value {
    serde_json::from_str(UNWIND).unwrap()
}
/// The hold baseline: the same book, no actions, runoff.
fn base() -> Value {
    let mut v = unwind();
    v["plan"]["actions"] = json!([]);
    for a in v["acquisitions"].as_array_mut().unwrap() {
        for k in KINDS {
            a[k] = json!([]);
        }
        a["business_plan"] = json!({"mode": "runoff"});
    }
    v
}
fn priced(price: f64) -> Value {
    let mut v = unwind();
    for a in v["acquisitions"].as_array_mut().unwrap() {
        a["sales"][0]["clean_price_per_100"] = json!(price);
    }
    v
}
fn package(id: &str, group: &str, request: Value) -> Value {
    json!({"package_id": id, "group": group, "review": {"id": format!("{id}-review"), "revision": "r1"}, "request": request})
}
fn allocate(
    packages: Vec<Value>,
    floor: f64,
    objective: &str,
    refinements: u32,
) -> (Value, Vec<u8>) {
    let req = json!({"schema": "liquidity-allocation-1", "base": base(), "packages": packages,
        "constraints": {"minimum_usable_cash": floor, "objective": objective, "max_refinements": refinements}});
    let out = Arc::new(Mutex::new(Vec::new()));
    run(
        &serde_json::to_vec(&req).unwrap(),
        Box::new(Capture(out.clone())),
    )
    .unwrap();
    let bytes = out.lock().unwrap().clone();
    let cut = bytes.iter().position(|b| *b == b'\n').unwrap();
    let head: Value = serde_json::from_slice(&bytes[..cut]).unwrap();
    (head["allocation"].clone(), bytes[cut + 1..].to_vec())
}

#[test]
fn the_selected_package_is_replayed_exactly_and_verifies_independently() {
    let mut formal = unwind();
    formal["plan"]["actions"][0]["accounting_treatment"] = json!("formal");
    let (r, stream) = allocate(
        vec![
            package("at-par", "bond", unwind()),
            package("above-par", "bond", priced(101.)),
            package("formal", "bond", formal),
        ],
        0.,
        "worst_case_total_earnings",
        4,
    );
    assert_eq!(r["selected"], json!(["above-par"]));
    assert_eq!(r["exact_replay_passed"], true);
    let formal = r["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["package_id"] == "formal")
        .unwrap();
    assert_eq!(formal["eligible"], false);
    assert!(
        formal["refusal"]
            .as_str()
            .unwrap()
            .contains("FORMAL_OWNER_REQUIRED"),
        "{formal}"
    );
    let above = r["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["package_id"] == "above-par")
        .unwrap();
    assert_eq!(above["coefficients"]["baseline"]["trading_pnl"], 1.0);
    let attempt = &r["attempts"][0];
    assert_eq!(attempt["estimate"]["baseline"]["trading_pnl"], 1.0);
    assert_eq!(attempt["exact"]["baseline"]["trading_pnl"], 1.0);
    // The candidate's own stream verifies against the merged request it replayed.
    let combined = serde_json::to_vec(&r["combined_request"]).unwrap();
    let receipt = portfolio_verification_native::verify(&combined, &stream).unwrap();
    assert!(receipt.verified);
}

#[test]
fn an_unreachable_floor_returns_no_plan_with_the_binding_scenario() {
    let (r, stream) = allocate(
        vec![
            package("at-par", "bond", unwind()),
            package("above-par", "bond", priced(101.)),
        ],
        7.,
        "worst_case_external_nii",
        8,
    );
    assert_eq!(r["outcome"], "estimate_infeasible");
    assert_eq!(r["exact_replay_passed"], false);
    assert!(r["selected"].is_null() && r["combined_request"].is_null() && stream.is_empty());
    let v = &r["violation"];
    assert_eq!(
        (v["measure"].as_str(), v["limit"].as_f64()),
        (Some("usable_cash_bound"), Some(7.0))
    );
    assert!(v["scenario_id"].is_string() && v["value"].as_f64().unwrap() < 7.);
}

#[test]
fn a_failed_candidate_is_cut_exactly_and_the_next_one_is_validated() {
    // Two groups whose packages cannot be combined (their books differ): the pair is estimated
    // best, its exact replay refuses, the exact selection is cut and the best single package wins.
    let mut other = priced(102.);
    other["plan"]["actions"][0]["action_id"] = json!("unwind-other");
    other["plan"]["settlement_policy"]["revision"] = json!("r2");
    let packages = || {
        vec![
            package("above-par", "bond", priced(101.)),
            package("other-book", "desk", other.clone()),
        ]
    };
    let (r, stream) = allocate(packages(), 0., "worst_case_total_earnings", 4);
    let attempts = r["attempts"].as_array().unwrap();
    assert_eq!(attempts[0]["selection"], json!(["above-par", "other-book"]));
    assert_eq!(attempts[0]["violation"]["measure"], "exact_replay");
    assert!(attempts[0]["violation"]["detail"]
        .as_str()
        .unwrap()
        .contains("cannot be combined"));
    assert_eq!(r["outcome"], "validated");
    assert_eq!(r["selected"], json!(["other-book"]));
    let combined = serde_json::to_vec(&r["combined_request"]).unwrap();
    assert!(
        portfolio_verification_native::verify(&combined, &stream)
            .unwrap()
            .verified
    );
    // With no refinement allowed the same request exhausts and validates nothing.
    let (r, stream) = allocate(packages(), 0., "worst_case_total_earnings", 0);
    assert_eq!(r["outcome"], "exhausted");
    assert!(r["selected"].is_null() && stream.is_empty());
    assert_eq!(r["violation"]["measure"], "exact_replay");
}

#[test]
fn merging_packages_is_structural_and_reproduces_the_original_unwind() {
    // Split the worked unwind into a bond package and a swap package.
    let mut bond = unwind();
    let mut swap = unwind();
    bond["plan"]["actions"][0]["lots"] = json!([{"lot_id": "bond-lot", "quantity": 100.0}]);
    swap["plan"]["actions"][0]["action_id"] = json!("unwind-swap");
    swap["plan"]["actions"][0]["lots"] = json!([{"lot_id": "swap-lot", "quantity": 100.0}]);
    for (b, s) in bond["acquisitions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(swap["acquisitions"].as_array_mut().unwrap())
    {
        b["terminations"] = json!([]);
        b["initial_margin"] = json!([]);
        for k in [
            "facility_draws",
            "repayments",
            "sales",
            "facility_repayments",
        ] {
            s[k] = json!([]);
        }
    }
    let merged = merge(&base(), &[&bond, &swap]).unwrap();
    let original = unwind();
    assert_eq!(merged["acquisitions"], original["acquisitions"]);
    assert_eq!(merged["plan"]["actions"].as_array().unwrap().len(), 2);
    assert_eq!(merged["dated"], original["dated"]);
    // Packages over different books cannot be combined.
    let mut other = swap.clone();
    other["plan"]["accounts"][0]["cash"] = json!(1.0);
    assert!(merge(&base(), &[&bond, &other])
        .unwrap_err()
        .contains("cannot be combined"));
    // Holding everything is the baseline itself.
    assert_eq!(merge(&base(), &[]).unwrap(), base());
}

#[test]
fn holding_the_book_is_a_validated_plan_when_it_is_best() {
    let (r, stream) = allocate(
        vec![package("below-par", "bond", priced(99.))],
        0.,
        "worst_case_total_earnings",
        2,
    );
    assert_eq!(r["outcome"], "validated");
    assert_eq!(r["selected"], json!([]));
    assert_eq!(r["combined_request"], base());
    let combined = serde_json::to_vec(&r["combined_request"]).unwrap();
    assert!(
        portfolio_verification_native::verify(&combined, &stream)
            .unwrap()
            .verified
    );
}
