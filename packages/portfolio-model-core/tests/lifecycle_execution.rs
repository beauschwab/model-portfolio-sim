//! Stage C hand fixtures (issue #45): the plan's worked unwind replayed by the execution engine.
//!
//! Opening: cash 0, bond carrying 100 pledged to a 95 repo, swap liability 4, posted VM 2 and
//! posted IM 3, equity 6. Day 1: a 95 bridge draw funds the repo repayment, the released bond
//! sells for 100, the bridge is repaid and the swap is terminated at its native value of -4.
//! Day 2: VM and IM return. Cash 0 -> 1 -> 6 with no period gain or loss.
use portfolio_model_core::lifecycle::acquisition::{
    self, AcquisitionRequest, EventStatus, ResolvedHedge, MARGIN_REFRESH_REQUIRED,
};
use portfolio_model_core::lifecycle::journal::Kind;
use portfolio_model_core::lifecycle::{self, *};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle/execution-unwind.json")).unwrap()
}
fn hedge(lot: &str, ordinal: i32, clean: f64, due: f64) -> ResolvedHedge {
    ResolvedHedge {
        lot_id: lot.into(),
        ordinal,
        clean_value: clean,
        due_cash: due,
        pricing_source: Revision {
            id: "swap-pricer".into(),
            revision: "r1".into(),
        },
    }
}
fn run_with(
    v: &Value,
    hedges: &[ResolvedHedge],
) -> Result<acquisition::AcquisitionJournal, Refusal> {
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap())?;
    let acq: AcquisitionRequest = serde_json::from_value(v["execution"].clone())
        .map_err(|e| Refusal::new(MALFORMED_INPUT, e.to_string()))?;
    acquisition::replay(&plan, &acq, hedges, "a")
}
fn run(v: &Value) -> Result<acquisition::AcquisitionJournal, Refusal> {
    run_with(v, &[hedge("swap-lot", 738887, -4., 0.)])
}
fn code(v: &Value) -> &'static str {
    run(v).err().map(|r| r.code).unwrap_or("ADMITTED")
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
fn stock(day: &journal::Closing, kind: Kind) -> f64 {
    day.financial
        .iter()
        .chain(&day.custody)
        .find(|(k, _)| *k == kind)
        .map_or(0., |(_, v)| *v)
}
fn status(j: &acquisition::AcquisitionJournal, id: &str) -> EventStatus {
    j.receipt
        .events
        .iter()
        .find(|e| e.event_id == id)
        .unwrap_or_else(|| panic!("{id}"))
        .status
}
fn cash_path(j: &acquisition::AcquisitionJournal) -> Vec<f64> {
    j.daily.iter().map(|d| d.cash).collect()
}
fn equity_identity(j: &acquisition::AcquisitionJournal) {
    // Assets less liabilities equal the owner's opening equity share plus period income.
    for d in &j.daily {
        let assets: f64 = d
            .financial
            .iter()
            .filter(|(k, _)| !k.liability() && !k.pnl() && *k != Kind::OpeningEquity)
            .map(|(_, v)| v)
            .sum();
        let liabilities: f64 = d
            .financial
            .iter()
            .filter(|(k, _)| k.liability())
            .map(|(_, v)| -v)
            .sum();
        let income: f64 = d
            .financial
            .iter()
            .filter(|(k, _)| k.pnl())
            .map(|(_, v)| -v)
            .sum();
        near(
            assets - liabilities,
            -stock(d, Kind::OpeningEquity) + income,
        );
    }
}

#[test]
fn worked_unwind_needs_the_bridge_and_reaches_cash_six_without_gain_or_loss() {
    let j = run(&raw()).unwrap();
    assert_eq!(cash_path(&j), vec![0., 1., 6., 6., 6.]);
    equity_identity(&j);
    let ids: Vec<&str> = j.intents.iter().map(|i| i.event_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "draw",
            "repay-repo",
            "sell-bond",
            "terminate-swap",
            "terminate_settle:swap-lot",
            "repay-bridge",
            "vm_return:swap-lot",
            "im_return:swap-csa:738887"
        ]
    );
    assert!(j
        .receipt
        .events
        .iter()
        .all(|e| e.status == EventStatus::Settled));
    let last = j.daily.last().unwrap();
    for kind in [
        Kind::AssetPrincipal,
        Kind::FundingPrincipal,
        Kind::DerivativeLiability,
        Kind::PostedVm,
        Kind::PostedIm,
        Kind::UnpaidPayable,
    ] {
        near(stock(last, kind), 0.);
    }
    near(j.receipt.realized, 0.);
    assert!(!j.receipt.open_claims);
    let facts: Vec<(&str, f64, f64)> = j
        .receipt
        .accounting_facts
        .iter()
        .map(|f| (f.kind, f.consideration, f.realized))
        .collect();
    assert_eq!(facts, [("sale", 100., 0.), ("termination", -4., 0.)]);
    assert_eq!(
        j.receipt.accounting_facts[1].economic_allocation_ids,
        ["bond-hedge"]
    );
    assert!(j.receipt.accounting_facts[1]
        .formal_relationship_ids
        .is_empty());
    assert!(!j.receipt.formal_hedge_accounting);
}

#[test]
fn without_the_bridge_the_repayment_fails_before_the_pledged_bond_can_be_delivered() {
    let mut v = raw();
    v["execution"]["facility_draws"] = json!([]);
    v["execution"]["facility_repayments"] = json!([]);
    v["execution"]["repayments"][0]["depends_on"] = json!([]);
    let refused = run(&v).unwrap_err();
    assert_eq!(refused.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
    assert!(refused.detail.contains("repay-repo"));
    // Selling the pledged bond first is not possible either.
    let mut v = raw();
    v["execution"]["sales"][0]["depends_on"] = json!([]);
    v["execution"]["repayments"][0]["repayment_ordinal"] = json!(738888);
    v["execution"]["facility_draws"][0]["ordinal"] = json!(738888);
    v["execution"]["facility_repayments"][0]["ordinal"] = json!(738888);
    assert_eq!(code(&v), INVENTORY_UNAVAILABLE);
}

#[test]
fn vm_is_returned_or_applied_exactly_once() {
    let mut v = raw();
    v["plan"]["margin_agreements"][0]["vm_at_termination"] = json!("apply_against_consideration");
    let j = run(&v).unwrap();
    // Termination pays 4 less the 2 posted VM, and no VM return follows.
    assert_eq!(cash_path(&j), vec![0., 3., 6., 6., 6.]);
    assert!(j
        .intents
        .iter()
        .all(|i| !i.event_id.starts_with("vm_return")));
    near(stock(j.daily.last().unwrap(), Kind::PostedVm), 0.);
    equity_identity(&j);
}

#[test]
fn a_failed_sale_leaves_settled_financing_and_hedge_legs_as_they_are() {
    let mut v = raw();
    v["execution"]["sales"][0]["state"] = json!("failed");
    v["execution"]["terminations"][0]["depends_on"] = json!(["sell-bond"]);
    let j = run(&v).unwrap();
    assert_eq!(status(&j, "draw"), EventStatus::Settled);
    assert_eq!(status(&j, "repay-repo"), EventStatus::Settled);
    assert_eq!(status(&j, "sell-bond"), EventStatus::Failed);
    for blocked in [
        "repay-bridge",
        "terminate-swap",
        "terminate_settle:swap-lot",
        "vm_return:swap-lot",
        "im_return:swap-csa:738887",
    ] {
        assert_eq!(status(&j, blocked), EventStatus::Blocked, "{blocked}");
    }
    let last = j.daily.last().unwrap();
    near(last.cash, 0.);
    near(stock(last, Kind::FundingPrincipal), -95.); // the bridge debt is real
    near(stock(last, Kind::AssetPrincipal), 100.); // the bond is still owned and released
    assert_eq!(last.inventory[0].available, 100.);
    near(stock(last, Kind::DerivativeLiability), -4.);
    near(stock(last, Kind::PostedVm), 2.);
    assert!(j.receipt.open_claims || stock(last, Kind::PostedIm) == 3.);
    equity_identity(&j);
}

#[test]
fn execution_quote_and_same_day_coupon_are_realized_truthfully() {
    let mut v = raw();
    v["execution"]["terminations"][0]["consideration"] = json!({"kind": "execution_quote", "amount": -4.5, "quote": {"id": "dealer-unwind", "revision": "r1"}});
    let j = run(&v).unwrap();
    near(j.receipt.realized, -0.5);
    near(j.daily.last().unwrap().cash, 5.5);
    near(stock(j.daily.last().unwrap(), Kind::RealizedGainLoss), 0.5);
    equity_identity(&j);
    // A coupon due on the termination date joins the claim; a negative amount is a receipt.
    let j = run_with(&raw(), &[hedge("swap-lot", 738887, -4., -0.5)]).unwrap();
    near(j.daily.last().unwrap().cash, 5.5);
    near(stock(j.daily.last().unwrap(), Kind::InterestExpense), 0.5);
    let j = run_with(&raw(), &[hedge("swap-lot", 738887, -4., 0.5)]).unwrap();
    near(j.daily.last().unwrap().cash, 6.5);
    equity_identity(&j);
}

#[test]
fn termination_claim_stays_unpaid_until_its_settlement_date() {
    let mut v = raw();
    v["execution"]["terminations"][0]["settlement_ordinal"] = json!(738889);
    let j = run(&v).unwrap();
    near(stock(&j.daily[1], Kind::UnpaidPayable), -4.);
    near(stock(&j.daily[1], Kind::DerivativeLiability), 0.);
    near(j.daily[1].cash, 5.);
    near(stock(&j.daily[3], Kind::UnpaidPayable), 0.);
    assert_eq!(cash_path(&j), vec![0., 5., 5., 1., 6.]);
    equity_identity(&j);
}

#[test]
fn a_reporting_cutoff_before_collateral_return_keeps_the_claims_open() {
    let mut v = raw();
    v["execution"]["horizon_end_ordinal"] = json!(738887);
    let j = run(&v).unwrap();
    assert!(j.receipt.open_claims);
    let last = j.daily.last().unwrap();
    near(stock(last, Kind::PostedVm), 2.);
    near(stock(last, Kind::PostedIm), 3.);
    near(last.cash, 1.);
}

#[test]
fn an_offset_is_a_second_contract_and_the_original_survives() {
    let mut v = raw();
    v["execution"]["terminations"] = json!([]);
    v["execution"]["hedge_entries"] = json!([{"lot_id": "offset-lot", "agreement_id": "swap-csa", "entry_ordinal": 738887, "upfront_consideration": 0.0,
        "variation_margin": {"receive_threshold": 0.0, "post_threshold": 0.0, "minimum_transfer": 0.5}, "offsets_lot_id": "swap-lot"}]);
    v["execution"]["initial_margin"][0]["portfolio_lot_ids"] = json!(["offset-lot", "swap-lot"]);
    v["execution"]["initial_margin"][0]["posted_requirement"] = json!(3.0);
    let j = run_with(&v, &[hedge("offset-lot", 738887, 0., 0.)]).unwrap();
    let last = j.daily.last().unwrap();
    near(stock(last, Kind::DerivativeLiability), -4.);
    near(stock(last, Kind::PostedVm), 2.);
    near(stock(last, Kind::PostedIm), 6.);
    assert!(j
        .receipt
        .accounting_facts
        .iter()
        .all(|f| f.kind != "termination"));
    // Removing the offsetting trade later is a population change needing its own requirement.
    v["execution"]["initial_margin"] = json!([]);
    assert_eq!(
        run_with(&v, &[hedge("offset-lot", 738887, 0., 0.)])
            .unwrap_err()
            .code,
        MARGIN_REFRESH_REQUIRED
    );
}

#[test]
fn a_partial_sale_keeps_the_surviving_lot_on_its_original_anchors() {
    let mut v = raw();
    v["execution"]["sales"][0]["quantity"] = json!(40.0);
    v["execution"]["facility_repayments"][0]["amount"] = json!(36.0);
    let j = run(&v).unwrap();
    let last = j.daily.last().unwrap();
    near(stock(last, Kind::AssetPrincipal), 60.);
    near(stock(last, Kind::FundingPrincipal), -59.); // 95 bridge drawn, 36 repaid after the 4 termination
    assert_eq!(last.inventory[0].owned, 60.);
    let fact = &j.receipt.accounting_facts[0];
    assert_eq!(
        (
            fact.kind,
            fact.quantity,
            fact.retained_quantity,
            fact.removed_carrying
        ),
        ("partial_sale", 40., 60., 40.)
    );
    equity_identity(&j);
}

#[test]
fn dependencies_must_be_known_and_earlier_and_formal_treatment_is_refused() {
    let mut v = raw();
    v["execution"]["facility_draws"][0]["depends_on"] = json!(["sell-bond"]);
    assert_eq!(code(&v), SETTLEMENT_ORDER);
    let mut v = raw();
    v["execution"]["sales"][0]["depends_on"] = json!(["nothing"]);
    assert_eq!(code(&v), LIFECYCLE_MAPPING_REQUIRED);
    let mut v = raw();
    v["plan"]["actions"][0]["accounting_treatment"] = json!("formal");
    assert_eq!(code(&v), FORMAL_OWNER_REQUIRED);
    let mut v = raw();
    v["execution"]["terminations"][0]["consideration"] =
        json!({"kind": "execution_quote", "amount": -4.0});
    assert!(serde_json::from_value::<AcquisitionRequest>(v["execution"].clone()).is_err());
    let plan = lifecycle::admit(&serde_json::to_vec(&raw()["plan"]).unwrap()).unwrap();
    let acq: AcquisitionRequest = serde_json::from_value(raw()["execution"].clone()).unwrap();
    assert_eq!(
        acquisition::replay(&plan, &acq, &[], "a").unwrap_err().code,
        LIFECYCLE_MAPPING_REQUIRED
    );
}
