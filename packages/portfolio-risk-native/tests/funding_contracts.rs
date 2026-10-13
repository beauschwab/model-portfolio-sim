use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_risk_native::funding_contract::*;
use portfolio_risk_native::term_enhancements::{
    indexed::{ContractPeriod, Coupon},
    Identity,
};
fn d(n: u32) -> i32 {
    ordinal_from_ymd(2026, 1, n).unwrap()
}
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn contract(name: &str, start: i32, end: i32, p: f64, rate: f64) -> FundingContract {
    FundingContract {
        contract: id(name),
        position_id: name.into(),
        entity_id: "bank".into(),
        counterparty_id: "counterparty".into(),
        currency: "USD".into(),
        direction: FundingDirection::Lending,
        funding_kind: FundingKind::Interbank,
        status: FundingStatus::New,
        principal: p,
        start_ordinal: start,
        maturity_ordinal: end,
        rate_path: id("fixed-path"),
        origination_fee: 0.,
        interest_periods: vec![ContractPeriod {
            period_id: "coupon".into(),
            start_ordinal: start,
            end_ordinal: end,
            payment_ordinal: end,
            day_count: ActualDayCount::Act360,
            coupon: Coupon::Fixed { annual_rate: rate },
            fee: 0.,
        }],
        collateral: None,
        exit: Exit::Maturity { rollover: None },
    }
}
fn request() -> FundingRequest {
    FundingRequest {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        assumption: id("funding-policy"),
        scenario_id: "physical-scenario".into(),
        measure: CashflowMeasure::Physical,
        monetary_unit: "currency_units".into(),
        currency: "USD".into(),
        asof_ordinal: d(1),
        reporting_end_ordinal: d(3),
        tail_end_ordinal: d(21),
        observed_publication_cutoff_unix_seconds: 1767225600,
        collateral_assets: vec![],
        contracts: vec![contract("fund:old", d(1), d(11), 100., 0.36)],
    }
}
fn snapshot(name: &str, date: i32, price: f64, haircut: f64) -> CollateralSnapshot {
    CollateralSnapshot {
        source: id(name),
        ordinal: date,
        values: vec![CollateralValue {
            asset_id: "Treasury:A".into(),
            market_price: price,
            eligible: true,
            haircut,
        }],
    }
}
fn asset(name: &str) -> CollateralAsset {
    CollateralAsset {
        asset_id: name.into(),
        owner_entity_id: "counterparty".into(),
        currency: "USD".into(),
        asset_class: "Treasury".into(),
        ownership: Ownership::Owned,
        ownership_source: id("owned-inventory"),
        quantity: 200.,
        already_encumbered_quantity: 0.,
    }
}
fn repo() -> FundingRequest {
    let mut r = request();
    r.collateral_assets = vec![asset("Treasury:A")];
    r.contracts[0].funding_kind = FundingKind::Repo;
    r.contracts[0].collateral = Some(Box::new(CollateralAgreement {
        policy: id("repo-collateral"),
        eligible_asset_classes: vec!["Treasury".into()],
        include_accrued_interest: true,
        initial_accrued_interest_claim: 0.,
        initial_accrued_right_source: id("initial-accrued"),
        minimum_transfer_value: 0.,
        margin_breach: MarginBreach::Reject,
        initial_snapshot: snapshot("initial", d(1), 1., 0.1),
        initial_pledges: vec![Allocation {
            asset_id: "Treasury:A".into(),
            quantity: 112.,
        }],
        margin_steps: vec![],
    }));
    r
}
fn margin(date: i32, price: f64, h: f64, qty: f64) -> MarginStep {
    MarginStep {
        ordinal: date,
        management_access: Access::Available,
        action_source: id("reviewed-margin"),
        accrued_interest_claim: 0.2,
        accrued_right_source: id("actual-accrual-state"),
        snapshot: snapshot(&format!("margin:{date}"), date, price, h),
        transfers: if qty != 0. {
            vec![Allocation {
                asset_id: "Treasury:A".into(),
                quantity: qty,
            }]
        } else {
            vec![]
        },
    }
}
fn default_policy() -> DefaultPolicy {
    DefaultPolicy {
        policy: id("counterparty-default"),
        ordinal: d(5),
        accrued_interest_claim: 0.4,
        accrued_right_source: id("actual-default-accrual"),
        economic_principal_loss: 60.,
        economic_interest_loss: 0.2,
        recovery_source: id("reviewed-recovery-tail"),
        complete_principal_recoveries: vec![DatedAmount {
            ordinal: d(20),
            amount: 40.,
        }],
        complete_interest_recoveries: vec![DatedAmount {
            ordinal: d(21),
            amount: 0.2,
        }],
        collateral_realization: None,
    }
}
#[test]
fn actual_maturity_lending_borrowing_fee_and_signed_interest_conserve() {
    let mut r = request();
    r.contracts[0].origination_fee = 2.;
    let x = evaluate(&r).unwrap();
    close(x.schedule.events[0].components.principal, -100.);
    close(x.schedule.events[1].components.principal, 100.);
    close(x.schedule.events[1].components.cash_interest, 1.);
    close(x.contracts[0].live_principal_after_reporting, 100.);
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.cash_total().unwrap())
            .sum(),
        3.,
    );
    r.contracts[0].direction = FundingDirection::Borrowing;
    let y = evaluate(&r).unwrap();
    for (a, b) in x.schedule.events.iter().zip(&y.schedule.events) {
        close(
            a.components.cash_total().unwrap(),
            -b.components.cash_total().unwrap(),
        );
    }
    r.contracts[0].direction = FundingDirection::Lending;
    r.contracts[0].origination_fee = 0.;
    r.contracts[0].interest_periods[0].coupon = Coupon::Fixed { annual_rate: -0.36 };
    close(
        evaluate(&r).unwrap().schedule.events[1]
            .components
            .cash_interest,
        -1.,
    );
    r.contracts[0].interest_periods[0].coupon = Coupon::Fixed { annual_rate: 0. };
    close(
        evaluate(&r).unwrap().schedule.events[1]
            .components
            .cash_interest,
        0.,
    );
}
#[test]
fn partial_roll_replacement_spread_is_new_principal_and_outage_keeps_old_obligation() {
    let mut r = request();
    r.reporting_end_ordinal = d(11);
    let new = contract("fund:new", d(11), d(21), 60., 0.72);
    r.contracts[0].exit = Exit::Maturity {
        rollover: Some(Box::new(Rollover {
            election_source: id("partial-roll"),
            market_access: Access::Available,
            requested_principal: 60.,
            replacement: Some(Box::new(new)),
        })),
    };
    let x = evaluate(&r).unwrap();
    assert_eq!(x.contracts.len(), 2);
    close(x.contracts[0].live_principal_after_reporting, 0.);
    close(x.contracts[1].live_principal_after_reporting, 60.);
    close(x.rollover[0].executed_principal, 60.);
    close(
        x.schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal == d(11))
            .map(|e| e.components.cash_total().unwrap())
            .sum(),
        41.,
    );
    close(
        x.schedule.events.last().unwrap().components.cash_interest,
        1.2,
    );
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        0.,
    );
    if let Exit::Maturity {
        rollover: Some(ref mut roll),
    } = r.contracts[0].exit
    {
        roll.market_access = Access::Unavailable;
        roll.replacement = None;
    }
    let outage = evaluate(&r).unwrap();
    assert_eq!(outage.contracts.len(), 1);
    close(outage.rollover[0].market_access_shortfall, 60.);
    close(outage.schedule.events[1].components.principal, 100.);
    close(outage.schedule.events[1].components.cash_interest, 1.);
    r.contracts[0].direction = FundingDirection::Borrowing;
    close(
        evaluate(&r).unwrap().schedule.events[1]
            .components
            .cash_total()
            .unwrap(),
        -101.,
    );
}
#[test]
fn opening_state_does_not_readvance_and_retains_remaining_coupon_anchor() {
    let mut r = request();
    r.asof_ordinal = d(5);
    r.reporting_end_ordinal = d(5);
    r.observed_publication_cutoff_unix_seconds = 1767571200;
    r.contracts[0].status = FundingStatus::Opening;
    let x = evaluate(&r).unwrap();
    assert_eq!(x.schedule.events.len(), 1);
    close(x.contracts[0].opening_principal, 100.);
    close(x.contracts[0].new_advance_principal, 0.);
    close(x.schedule.events[0].components.cash_interest, 1.);
    r.contracts[0].start_ordinal = d(1) - 30;
    r.contracts[0].interest_periods[0].start_ordinal = d(1);
    evaluate(&r).unwrap();
    r.contracts[0].origination_fee = 1.;
    assert!(evaluate(&r).is_err());
}
#[test]
fn haircut_change_margin_deliver_release_and_report_inventory_are_non_cash() {
    let mut r = repo();
    let a = r.contracts[0].collateral.as_mut().unwrap();
    a.margin_steps = vec![margin(d(3), 0.9, 0.2, 40.), margin(d(4), 1., 0.1, -40.)];
    let x = evaluate(&r).unwrap();
    assert_eq!(x.schedule.events.len(), 2);
    close(x.margin_calls[0].eligible_value_before, 112. * 0.9 * 0.8);
    close(
        x.margin_calls[0].margin_call_value,
        100.2 - 112. * 0.9 * 0.8,
    );
    close(x.margin_calls[0].eligible_value_after, 152. * 0.9 * 0.8);
    close(x.margin_calls[0].unmet_margin_value, 0.);
    close(x.inventory[0].encumbered_quantity_after_reporting, 152.);
    close(x.inventory[0].encumbered_quantity_at_tail_end, 0.);
    close(x.inventory[0].disposed_quantity, 0.);
    close(x.inventory[0].remaining_quantity, 200.);
    close(
        x.collateral_movements
            .iter()
            .map(|m| m.quantity_delta)
            .sum(),
        0.,
    );
    close(x.contracts[0].cash_principal_settled, 100.);
    let y = evaluate(&r).unwrap();
    assert_eq!(
        serde_json::to_string(&x).unwrap(),
        serde_json::to_string(&y).unwrap()
    );
}
#[test]
fn collateral_substitution_and_overlapping_claims_are_exclusive() {
    let mut r = repo();
    r.collateral_assets.push(asset("Treasury:B"));
    let a = r.contracts[0].collateral.as_mut().unwrap();
    let mut s = margin(d(3), 1., 0.1, -112.);
    s.snapshot.values.push(CollateralValue {
        asset_id: "Treasury:B".into(),
        market_price: 1.,
        eligible: true,
        haircut: 0.1,
    });
    s.transfers.push(Allocation {
        asset_id: "Treasury:B".into(),
        quantity: 112.,
    });
    a.margin_steps = vec![s];
    let x = evaluate(&r).unwrap();
    close(x.inventory[0].encumbered_quantity_after_reporting, 0.);
    close(x.inventory[1].encumbered_quantity_after_reporting, 112.);
    r = repo();
    let mut second = r.contracts[0].clone();
    second.position_id = "repo:overlap".into();
    second.contract = id("second-repo");
    r.contracts.push(second);
    assert!(evaluate(&r).unwrap_err().contains("encumbered"));
    // At the old maturity, collateral is released before a separately supplied new pledge.
    r = repo();
    let mut second = r.contracts[0].clone();
    second.position_id = "repo:later".into();
    second.contract = id("later-repo");
    second.start_ordinal = d(11);
    second.maturity_ordinal = d(21);
    second.interest_periods[0].start_ordinal = d(11);
    second.interest_periods[0].end_ordinal = d(21);
    second.interest_periods[0].payment_ordinal = d(21);
    second.collateral.as_mut().unwrap().initial_snapshot =
        snapshot("later-initial", d(11), 1., 0.1);
    r.contracts.push(second);
    evaluate(&r).unwrap();
    r.collateral_assets[0].ownership = Ownership::ReceivedCollateral;
    assert!(evaluate(&r).is_err());
}
#[test]
fn management_outage_reports_pending_call_without_deleting_principal_or_release() {
    let mut r = repo();
    let a = r.contracts[0].collateral.as_mut().unwrap();
    a.margin_breach = MarginBreach::Report;
    let mut step = margin(d(3), 0.9, 0.2, 40.);
    step.management_access = Access::Unavailable;
    a.margin_steps = vec![step];
    let x = evaluate(&r).unwrap();
    assert!(x.margin_calls[0].executed_transfers.is_empty());
    close(x.margin_calls[0].requested_transfers[0].quantity, 40.);
    close(
        x.margin_calls[0].unmet_margin_value,
        100.2 - 112. * 0.9 * 0.8,
    );
    close(x.inventory[0].encumbered_quantity_after_reporting, 112.);
    close(x.schedule.events[1].components.principal, 100.);
    assert_eq!(
        x.collateral_movements.last().unwrap().disposition,
        "maturity-release"
    );
    r.contracts[0].collateral.as_mut().unwrap().margin_breach = MarginBreach::Reject;
    assert!(evaluate(&r).is_err());
}
#[test]
fn unsecured_default_has_separate_claim_loss_and_complete_delayed_recovery() {
    let mut r = request();
    r.reporting_end_ordinal = d(5);
    r.contracts[0].exit = Exit::CounterpartyDefault {
        policy: Box::new(default_policy()),
    };
    let x = evaluate(&r).unwrap();
    close(x.contracts[0].cash_principal_settled, 40.);
    close(x.contracts[0].economic_principal_loss, 60.);
    close(x.contracts[0].economic_interest_loss, 0.2);
    close(x.contracts[0].live_principal_after_reporting, 0.);
    close(x.contracts[0].gross_default_claim_after_reporting, 100.);
    close(
        x.contracts[0].expected_principal_recoveries_after_reporting,
        40.,
    );
    assert_eq!(x.schedule.events.last().unwrap().payment_ordinal, d(21));
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        -60.,
    );
    if let Exit::CounterpartyDefault { policy } = &mut r.contracts[0].exit {
        policy.complete_principal_recoveries[0].amount = 39.;
    }
    assert!(evaluate(&r).is_err());
}
fn default_repo() -> FundingRequest {
    let mut r = repo();
    r.reporting_end_ordinal = d(5);
    let mut p = default_policy();
    p.economic_principal_loss = 50.;
    p.economic_interest_loss = 0.4;
    p.complete_principal_recoveries.clear();
    p.complete_interest_recoveries.clear();
    p.collateral_realization = Some(Box::new(CollateralRealization {
        policy: id("liquidation-policy"),
        ordinal: d(6),
        snapshot: snapshot("liquidation-mark", d(6), 0.5, 0.9),
        sale_discount: 0.1,
        sale_cost: 0.4,
        waterfall: Waterfall::PrincipalThenInterest,
        surplus_return_ordinal: d(6),
    }));
    r.contracts[0].exit = Exit::CounterpartyDefault {
        policy: Box::new(p),
    };
    r
}
#[test]
fn collateral_liquidation_applies_sale_cost_once_no_second_haircut_and_no_duplicate_cash() {
    let r = default_repo();
    let x = evaluate(&r).unwrap();
    let a = &x.collateral_realizations[0];
    close(a.collateral_market_value, 56.);
    close(a.gross_sale_proceeds, 50.4);
    close(a.net_sale_proceeds, 50.);
    close(a.principal_recovery, 50.);
    assert!(a.cash_already_in_funding_schedule);
    assert_eq!(x.schedule.events.len(), 2);
    close(x.schedule.events[1].components.principal, 50.);
    close(x.schedule.events[1].components.cash_interest, 0.);
    close(x.inventory[0].disposed_quantity, 112.);
    close(x.inventory[0].remaining_quantity, 88.);
    close(
        x.contracts[0].expected_principal_recoveries_after_reporting,
        50.,
    );
    // Inventory sold on default is not a released asset available for another repo.
    assert_eq!(
        x.collateral_movements.last().unwrap().disposition,
        "liquidated-not-released-for-reuse"
    );
}
#[test]
fn liquidation_waterfall_surplus_and_signed_borrowing_domain_are_explicit() {
    let mut r = default_repo();
    if let Exit::CounterpartyDefault { policy } = &mut r.contracts[0].exit {
        let real = policy.collateral_realization.as_mut().unwrap();
        real.waterfall = Waterfall::InterestThenPrincipal;
        policy.economic_principal_loss = 50.4;
        policy.economic_interest_loss = 0.;
    }
    let x = evaluate(&r).unwrap();
    close(x.collateral_realizations[0].principal_recovery, 49.6);
    close(x.collateral_realizations[0].interest_recovery, 0.4);
    if let Exit::CounterpartyDefault { policy } = &mut r.contracts[0].exit {
        policy.economic_principal_loss = 0.;
        policy.economic_interest_loss = 0.;
        let real = policy.collateral_realization.as_mut().unwrap();
        real.snapshot.values[0].market_price = 1.2;
        real.sale_discount = 0.;
        real.sale_cost = 4.4;
    }
    let x = evaluate(&r).unwrap();
    close(x.collateral_realizations[0].net_sale_proceeds, 130.);
    close(x.collateral_realizations[0].surplus_returned_to_owner, 29.6);
    close(x.schedule.events[1].components.cash_total().unwrap(), 100.4);
    r.contracts[0].direction = FundingDirection::Borrowing;
    assert!(evaluate(&r).is_err());
}
#[test]
fn repo_borrowing_pledges_own_inventory_and_outside_encumbrance_is_not_released() {
    let mut r = repo();
    r.contracts[0].direction = FundingDirection::Borrowing;
    assert!(evaluate(&r).is_err());
    r.collateral_assets[0].owner_entity_id = "bank".into();
    r.collateral_assets[0].already_encumbered_quantity = 80.;
    let x = evaluate(&r).unwrap();
    close(x.inventory[0].outside_encumbered_quantity, 80.);
    close(x.schedule.events[0].components.principal, 100.);
    close(x.schedule.events[1].components.principal, -100.);
    r.collateral_assets[0].already_encumbered_quantity = 100.;
    assert!(evaluate(&r).is_err());
}
#[test]
fn missing_collateral_haircut_dates_currency_recovery_and_work_fail_closed() {
    let mut r = repo();
    r.contracts[0].collateral = None;
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts[0]
        .collateral
        .as_mut()
        .unwrap()
        .initial_snapshot
        .values[0]
        .haircut = 1.;
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts[0]
        .collateral
        .as_mut()
        .unwrap()
        .initial_snapshot
        .values[0]
        .eligible = false;
    assert!(evaluate(&r).is_err());
    r = repo();
    r.collateral_assets[0].currency = "EUR".into();
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts[0].maturity_ordinal = d(22);
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts[0]
        .collateral
        .as_mut()
        .unwrap()
        .initial_snapshot
        .ordinal = d(2);
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts[0].principal = f64::NAN;
    assert!(evaluate(&r).is_err());
    r = repo();
    r.contracts = vec![r.contracts[0].clone(); MAX_CONTRACTS + 1];
    assert!(evaluate(&r).is_err());
    r = repo();
    r.collateral_assets = vec![r.collateral_assets[0].clone(); MAX_ASSETS + 1];
    assert!(evaluate(&r).is_err());
    r = default_repo();
    r.measure = CashflowMeasure::Pricing;
    assert!(evaluate(&r).is_err());
    r = default_repo();
    if let Exit::CounterpartyDefault { policy } = &mut r.contracts[0].exit {
        policy
            .collateral_realization
            .as_mut()
            .unwrap()
            .surplus_return_ordinal = d(7);
    }
    assert!(evaluate(&r).is_err());
}

#[test]
fn checked_in_repo_rollover_example_has_independent_cash_and_margin_reconciliation() {
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/funding-contract-1.json"
    ))
    .unwrap();
    let r: FundingRequest = serde_json::from_value(raw["request"].clone()).unwrap();
    let x = evaluate(&r).unwrap();
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        0.,
    );
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.cash_total().unwrap())
            .sum(),
        100. * 0.36 * 10. / 360. + 60. * 0.72 * 10. / 360.,
    );
    close(
        x.schedule
            .events
            .iter()
            .filter(|e| e.payment_ordinal == d(11))
            .map(|e| e.components.cash_total().unwrap())
            .sum(),
        41.,
    );
    close(
        x.margin_calls[0].margin_call_value,
        100.2 - 112. * 0.9 * 0.8,
    );
    close(x.inventory[0].encumbered_quantity_after_reporting, 152.);
    close(x.inventory[0].encumbered_quantity_at_tail_end, 0.);
}
