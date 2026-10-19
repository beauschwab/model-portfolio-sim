//! Stage B hand fixture (issue #44): a bond bought at 99.5 clean plus 0.5 accrued and 0.25 fees,
//! financed by a 95 repo advance secured by the bond, with an off-market swap entered for -4 and
//! paid for upfront, 3 posted and 2 received IM, and a 4 variation-margin post a day later.
//!
//! Cash: 20 +95 -100.25 +4 -3 = 15.75 on the acquisition day, 11.75 after VM. Equity 19.75 =
//! 20 less the fee. Nothing is carried at par, financed twice, or credited twice.
use portfolio_model_core::lifecycle::acquisition::{
    self, AcquisitionRequest, ResolvedHedge, MARGIN_REFRESH_REQUIRED,
};
use portfolio_model_core::lifecycle::journal::Kind;
use portfolio_model_core::lifecycle::{self, *};
use serde_json::{json, Value};

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle/acquisition-bridge.json")).unwrap()
}
fn run(v: &Value, clean: f64) -> Result<acquisition::AcquisitionJournal, Refusal> {
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap())?;
    let acq: AcquisitionRequest = serde_json::from_value(v["acquisition"].clone())
        .map_err(|e| Refusal::new(MALFORMED_INPUT, e.to_string()))?;
    let hedges = vec![ResolvedHedge {
        lot_id: "swap-lot".into(),
        ordinal: 738887,
        clean_value: clean,
        due_cash: 0.,
        pricing_source: Revision {
            id: "swap-pricer".into(),
            revision: "r1".into(),
        },
    }];
    acquisition::replay(&plan, &acq, &hedges, "a")
}
fn code(v: &Value, clean: f64) -> &'static str {
    run(v, clean).err().map(|r| r.code).unwrap_or("ADMITTED")
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

#[test]
fn acquisition_cash_inventory_financing_derivative_and_collateral_reconcile() {
    let j = run(&raw(), -4.).unwrap();
    let cash: Vec<f64> = j.daily.iter().map(|d| d.cash).collect();
    assert_eq!(cash, vec![20., 15.75, 11.75, 11.75, 11.75]);
    let day1 = &j.daily[1];
    near(stock(day1, Kind::AssetPrincipal), 100.);
    near(stock(day1, Kind::BookAdjustment), -0.5);
    near(stock(day1, Kind::InterestReceivable), 0.5);
    near(stock(day1, Kind::FundingPrincipal), -95.);
    near(stock(day1, Kind::DerivativeLiability), -4.);
    near(stock(day1, Kind::PostedIm), 3.);
    near(stock(day1, Kind::CustodyAsset), 2.);
    near(stock(day1, Kind::FeeExpense), 0.25);
    assert_eq!(day1.inventory[0].pledged, 100.);
    assert_eq!(day1.inventory[0].available, 0.);
    near(stock(&j.daily[2], Kind::PostedVm), 4.);
    // Received IM is custody memo: no financial line, no cash.
    assert!(day1.financial.iter().all(|(k, _)| !k.memo()));
    // Assets less liabilities equal the owner opening cash less the expensed fee on every day.
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
        near(assets - liabilities, 20. - stock(d, Kind::FeeExpense));
    }
    let r = &j.receipt;
    assert!(
        r.cash_bridge_reconciled
            && r.inventory_conserved
            && r.collateral_conserved
            && !r.formal_hedge_accounting
    );
    near(r.closing_cash, 11.75);
    near(r.gross_derivative_liabilities, 4.);
    near(r.posted_vm, 4.);
    near(r.received_im_custody, 2.);
    let ids: Vec<&str> = j.intents.iter().map(|i| i.event_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "advance:repo-lot",
            "purchase:bond-lot",
            "entry:swap-lot",
            "im:swap-csa:738887",
            "im:swap-csa:738887:custody",
            "vm:swap-lot"
        ]
    );
    assert!(j
        .intents
        .iter()
        .all(|i| i.lines.iter().map(|l| l.debit).sum::<f64>().abs() < 1e-12));
}

#[test]
fn unavailable_funding_or_collateral_is_infeasible_without_a_plug() {
    let mut v = raw();
    v["plan"]["accounts"][0]["cash"] = json!(5.0);
    let refused = run(&v, -4.).unwrap_err();
    assert_eq!(refused.code, INSUFFICIENT_SETTLEMENT_LIQUIDITY);
    assert!(refused.detail.contains("purchase:bond-lot"));
    // The pledge must deliver an owned lot: a bond settling the day after the advance is not.
    let mut v = raw();
    v["plan"]["lots"][0]["acquisition_ordinal"] = json!(738888);
    v["acquisition"]["purchases"][0]["settlement_ordinal"] = json!(738888);
    assert_eq!(code(&v, -4.), INVENTORY_UNAVAILABLE);
    let mut v = raw();
    v["acquisition"]["financings"] = json!([]);
    v["plan"]["pledges"] = json!([]);
    v["plan"]["funding_allocations"] = json!([]);
    // Without the advance the purchase needs 100.25 of cash against 20.
    assert_eq!(code(&v, -4.), INSUFFICIENT_SETTLEMENT_LIQUIDITY);
}

#[test]
fn no_par_gain_no_repeated_advance_and_no_day_one_gain() {
    let mut v = raw();
    v["plan"]["lots"][0]["original_cost"] = json!(100.0);
    assert_eq!(code(&v, -4.), UNSUPPORTED_POLICY);
    let mut v = raw();
    v["plan"]["lots"][1]["acquisition_ordinal"] = json!(738800);
    assert_eq!(code(&v, -4.), UNSUPPORTED_POLICY);
    let mut v = raw();
    v["acquisition"]["hedge_entries"][0]["upfront_consideration"] = json!(0.0);
    assert_eq!(code(&v, -4.), UNSUPPORTED_POLICY);
    // At-market entry with no consideration is the ordinary case.
    assert_eq!(code(&v, 0.), "ADMITTED");
    let j = run(&v, 0.).unwrap();
    assert!(j.intents.iter().all(|i| !i.event_id.starts_with("vm:")));
}

#[test]
fn supplied_initial_margin_binds_the_exact_portfolio_scenario_and_date() {
    for (path, value) in [
        ("ordinal", json!(738888)),
        ("scenario_id", json!("stress")),
        ("portfolio_lot_ids", json!(["swap-lot", "other"])),
    ] {
        let mut v = raw();
        v["acquisition"]["initial_margin"][0][path] = value;
        assert_eq!(code(&v, -4.), MARGIN_REFRESH_REQUIRED, "{path}");
    }
    let mut v = raw();
    v["acquisition"]["initial_margin"] = json!([]);
    assert_eq!(code(&v, -4.), MARGIN_REFRESH_REQUIRED);
}

#[test]
fn received_vm_restricts_or_frees_usable_cash_exactly_once() {
    let mut v = raw();
    v["acquisition"]["hedge_entries"][0]["upfront_consideration"] = json!(4.0);
    let j = run(&v, 4.).unwrap();
    let day2 = &j.daily[2];
    near(stock(day2, Kind::ReceivedVm), -4.);
    near(day2.cash, 7.75 + 4.);
    near(day2.usable_cash, 7.75);
    v["plan"]["margin_agreements"][0]["segregation"] = json!("reusable");
    v["acquisition"]["initial_margin"][0]["received_requirement"] = json!(0.0);
    let j = run(&v, 4.).unwrap();
    near(j.daily[2].usable_cash, j.daily[2].cash);
}

#[test]
fn formal_treatment_needs_the_hedge_accounting_owner_and_never_falls_back() {
    let mut v = raw();
    v["plan"]["actions"][0]["accounting_treatment"] = json!("formal");
    assert_eq!(code(&v, -4.), FORMAL_OWNER_REQUIRED);
}

#[test]
fn hedge_values_are_native_derived_and_strict_input_is_enforced() {
    let v = raw();
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap()).unwrap();
    let acq: AcquisitionRequest = serde_json::from_value(v["acquisition"].clone()).unwrap();
    assert_eq!(
        acquisition::replay(&plan, &acq, &[], "a").unwrap_err().code,
        LIFECYCLE_MAPPING_REQUIRED
    );
    let mut v = raw();
    v["acquisition"]["hedge_entries"][0]["clean_value"] = json!(-4.0);
    assert!(serde_json::from_value::<AcquisitionRequest>(v["acquisition"].clone()).is_err());
    let mut v = raw();
    v["acquisition"]["horizon_end_ordinal"] = json!(738886 + 400);
    assert_eq!(code(&v, -4.), BOUND_EXCEEDED);
}
