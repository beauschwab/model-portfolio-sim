//! Stage G (issue #49): business-plan modes on the worked unwind. The policy funds payments from
//! the state at that moment only; later outcomes cannot change earlier decisions, failed legs
//! change later ones, facility access and renewal are bounded and explicit, and monthly reports
//! reconcile to the dated events while exposing intra-month minimum cash.
use portfolio_model_core::lifecycle::acquisition::{self, AcquisitionRequest, ResolvedHedge};
use portfolio_model_core::lifecycle::{self, *};
use serde_json::{json, Value};

const ASOF: i32 = 738886;

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle/execution-unwind.json")).unwrap()
}
fn hedges() -> Vec<ResolvedHedge> {
    vec![ResolvedHedge {
        lot_id: "swap-lot".into(),
        ordinal: 738887,
        clean_value: -4.,
        due_cash: 0.,
        pricing_source: Revision {
            id: "swap-pricer".into(),
            revision: "r1".into(),
        },
    }]
}
fn run(v: &Value) -> Result<acquisition::AcquisitionJournal, Refusal> {
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap())?;
    let acq: AcquisitionRequest = serde_json::from_value(v["execution"].clone())
        .map_err(|e| Refusal::new(MALFORMED_INPUT, e.to_string()))?;
    acquisition::replay(&plan, &acq, &hedges(), "a")
}
/// The worked unwind with the bridge left to the funding policy instead of a schedule.
fn policy(repay_surplus: bool) -> Value {
    let mut v = raw();
    let e = &mut v["execution"];
    e["facility_draws"] = json!([]);
    e["facility_repayments"] = json!([]);
    e["repayments"][0]["depends_on"] = json!([]);
    e["business_plan"] = json!({"mode": "policy", "funding_policy": {
        "policy": {"id": "treasury-funding", "revision": "r1"},
        "minimum_usable_cash": 0.0,
        "facilities": [{"facility_id": "bridge", "access": [{"from_ordinal": ASOF, "to_ordinal": ASOF + 400}]}],
        "repay_surplus": repay_surplus,
    }});
    v
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
fn kinds(j: &acquisition::AcquisitionJournal) -> Vec<(i32, &'static str, f64)> {
    j.receipt
        .policy_decisions
        .iter()
        .map(|d| (d.ordinal, d.kind, d.amount))
        .collect()
}

#[test]
fn the_policy_funds_the_worked_unwind_from_settled_state_only() {
    let j = run(&policy(true)).unwrap();
    let d = &j.receipt.policy_decisions;
    assert_eq!(
        kinds(&j),
        [(738887, "draw", 95.), (738887, "surplus_repayment", 95.)]
    );
    assert_eq!(d[0].trigger, "repay-repo");
    near(d[0].usable_cash_before, 0.);
    near(d[0].need, 95.);
    assert_eq!(d[1].trigger, "end_of_day");
    near(j.receipt.closing_cash, 6.);
    // The draw is a step of its own, settled before the repayment it funds.
    let ids: Vec<&str> = j
        .receipt
        .steps
        .iter()
        .map(|s| s.event_id.as_str())
        .collect();
    let draw = ids
        .iter()
        .position(|i| i.starts_with("policy_draw:bridge"))
        .unwrap();
    assert_eq!(ids[draw + 1], "repay-repo");
    near(j.receipt.steps[draw + 1].cash_movement, -95.);
    assert_eq!(j.receipt.events.len(), j.receipt.steps.len());
    assert_eq!(
        j.receipt.business_plan_mode,
        Some(lifecycle::policy::PlanMode::Policy)
    );
    // The schedule-mode stream is unchanged: no business plan, no new receipt fields.
    let schedule = run(&raw()).unwrap();
    assert!(schedule.receipt.policy_decisions.is_empty() && schedule.receipt.monthly.is_none());
    let text = serde_json::to_string(&schedule.receipt).unwrap();
    assert!(
        !text.contains("policy_decisions")
            && !text.contains("monthly")
            && !text.contains("business_plan_mode")
    );
}

#[test]
fn later_outcomes_cannot_change_earlier_decisions_and_failed_legs_change_later_ones() {
    // A later realized settlement (VM and IM returning two days later) leaves day-2 decisions alone.
    let base = run(&policy(true)).unwrap();
    let mut later = policy(true);
    later["plan"]["margin_agreements"][0]["settlement_lag_days"] = json!(3);
    later["execution"]["horizon_end_ordinal"] = json!(ASOF + 10);
    let later = run(&later).unwrap();
    let day2 = |j: &acquisition::AcquisitionJournal| -> Vec<(String, f64, f64)> {
        j.receipt
            .policy_decisions
            .iter()
            .filter(|d| d.ordinal <= 738887)
            .map(|d| (d.decision_id.clone(), d.amount, d.usable_cash_before))
            .collect()
    };
    assert_eq!(day2(&base), day2(&later));

    // A failed sale leaves no cash for the termination: the policy draws again and repays nothing.
    let mut failed = policy(true);
    failed["plan"]["facilities"][0]["commitment"] = json!(120.0);
    failed["execution"]["sales"][0]["state"] = json!("failed");
    let failed = run(&failed).unwrap();
    let k = kinds(&failed);
    assert_eq!(k[0], (738887, "draw", 95.));
    assert_eq!(k[1], (738887, "draw", 4.));
    assert!(failed.receipt.policy_decisions[1]
        .trigger
        .starts_with("terminate"));
    assert!(!k
        .iter()
        .any(|x| x.0 == 738887 && x.1 == "surplus_repayment"));
}

#[test]
fn draws_need_declared_access_and_no_cash_plug_is_invented() {
    let mut v = policy(true);
    v["execution"]["business_plan"]["funding_policy"]["facilities"][0]["access"] =
        json!([{"from_ordinal": ASOF + 3, "to_ordinal": ASOF + 4}]);
    let r = run(&v).unwrap_err();
    assert_eq!(r.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
    assert!(r.detail.contains("repay-repo"), "{}", r.detail);
}

fn renewing(windows: Value, max_generations: u32, horizon_days: i32) -> Value {
    let mut v = policy(false);
    v["plan"]["facilities"][0]["expiry_ordinal"] = json!(ASOF + 10);
    v["execution"]["horizon_end_ordinal"] = json!(ASOF + horizon_days);
    v["execution"]["business_plan"]["funding_policy"]["facilities"][0]["renewal"] =
        json!({"max_generations": max_generations, "tenor_days": 10, "windows": windows});
    v
}

#[test]
fn renewal_is_an_explicit_bounded_series_and_never_perpetual() {
    // Two permitted renewals, then the generation bound: the balance stays a liability, unpaid
    // at the horizon, with no payoff synthesized.
    let all = json!([{"from_ordinal": ASOF, "to_ordinal": ASOF + 60}]);
    let j = run(&renewing(all.clone(), 3, 25)).unwrap();
    assert_eq!(
        kinds(&j),
        [
            (738887, "draw", 95.),
            (ASOF + 10, "renewal", 95.),
            (ASOF + 20, "renewal", 95.)
        ]
    );
    assert_eq!(j.receipt.policy_decisions[2].generation, 3);
    let replacements: Vec<&str> = j
        .receipt
        .accounting_facts
        .iter()
        .filter(|f| f.kind == "funding_replacement")
        .map(|f| f.lot_id.as_str())
        .collect();
    assert_eq!(replacements, ["bridge~g2", "bridge~g3"]);
    let last = j.daily.last().unwrap();
    near(
        last.financial
            .iter()
            .find(|(k, _)| *k == journal::Kind::FundingPrincipal)
            .unwrap()
            .1,
        -95.,
    );
    // At the bound the third expiry is repaid from usable cash.
    let j = run(&renewing(all, 3, 35)).unwrap();
    assert_eq!(
        kinds(&j).last().unwrap(),
        &(ASOF + 30, "maturity_repayment", 95.)
    );
    near(j.receipt.closing_cash, 6.);
    // Without renewal or enough usable cash the maturity is refused: no perpetual funding.
    let mut short = renewing(
        json!([{"from_ordinal": ASOF + 12, "to_ordinal": ASOF + 13}]),
        3,
        15,
    );
    short["plan"]["facilities"][0]["commitment"] = json!(120.0);
    short["execution"]["sales"][0]["state"] = json!("failed");
    let r = run(&short).unwrap_err();
    assert_eq!(r.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
    assert!(
        r.detail.contains("policy_maturity_repayment:bridge"),
        "{}",
        r.detail
    );
    // No renewal window: the expiring balance is repaid from cash at expiry.
    let j = run(&renewing(
        json!([{"from_ordinal": ASOF + 12, "to_ordinal": ASOF + 13}]),
        3,
        15,
    ))
    .unwrap();
    assert_eq!(
        kinds(&j),
        [
            (738887, "draw", 95.),
            (ASOF + 10, "maturity_repayment", 95.)
        ]
    );
    near(j.receipt.closing_cash, 6.);
}

#[test]
fn runoff_executes_no_actions_and_policies_need_the_policy_mode() {
    let mut v = raw();
    v["execution"]["business_plan"] = json!({"mode": "runoff"});
    assert_eq!(run(&v).unwrap_err().code, MALFORMED_INPUT);
    let mut v = raw();
    for k in [
        "facility_draws",
        "repayments",
        "sales",
        "facility_repayments",
        "terminations",
        "initial_margin",
    ] {
        v["execution"][k] = json!([]);
    }
    v["execution"]["business_plan"] = json!({"mode": "runoff"});
    let j = run(&v).unwrap();
    near(j.receipt.closing_cash, j.receipt.opening_cash);
    assert!(j.receipt.monthly.as_ref().is_some_and(|m| !m.is_empty()));
    let mut v = raw();
    v["execution"]["business_plan"] = json!({"mode": "schedule", "funding_policy": policy(true)["execution"]["business_plan"]["funding_policy"]});
    assert_eq!(run(&v).unwrap_err().code, MALFORMED_INPUT);
    // A policy facility cannot also be drawn by schedule.
    let mut v = policy(true);
    v["execution"]["facility_draws"] = raw()["execution"]["facility_draws"].clone();
    assert_eq!(run(&v).unwrap_err().code, UNSUPPORTED_POLICY);
}

#[test]
fn monthly_reports_reconcile_and_keep_the_intra_month_minimum() {
    let all = json!([{"from_ordinal": ASOF, "to_ordinal": ASOF + 60}]);
    let j = run(&renewing(all, 3, 45)).unwrap_or_else(|r| panic!("{r}"));
    let months = j.receipt.monthly.as_ref().unwrap();
    assert_eq!(
        months.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(),
        ["2024-01", "2024-02"]
    );
    let net: f64 = months.iter().map(|m| m.net_cash).sum();
    near(net, j.receipt.settled_cash_movements);
    near(months[0].opening_cash, 0.);
    near(months[1].opening_cash, months[0].closing_cash);
    // Usable cash fell to 0 after the repo repayment although January closes positive.
    near(months[0].minimum_usable_cash, 0.);
    assert!(months[0].closing_usable_cash > 0.);
    assert_eq!(months[0].policy_decisions, 4);
    near(months[0].closing_facility_drawn, 0.);
    assert!(months
        .iter()
        .all(|m| m.minimum_usable_cash <= m.closing_usable_cash));
}
