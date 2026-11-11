use portfolio_model_core::consumer_card::*;
use portfolio_model_core::consumer_contract::ConsumerState;

fn request() -> CardRequest {
    let mut r: CardRequest = serde_json::from_str(include_str!(
        "../../../examples/models/consumer-card-1.json"
    ))
    .unwrap();
    r.horizon_days = 20;
    r.opening_principal = [0.; 3];
    r.opening_billed_interest = [0.; 3];
    r.opening_unbilled_interest = [0.; 3];
    r.opening_unrecognized_billed_interest = [0.; 3];
    r.opening_fees = [0.; 3];
    r.opening_contingent_purchase_interest = 0.;
    r.opening_statement_remaining = [0.; 3];
    r.opening_minimum_remaining = 0.;
    r.opening_due_ordinal = None;
    r.cycles.clear();
    r.steps.clear();
    r
}

#[test]
fn zero_open_line_is_live_until_explicit_closure_cancels_commitment_once() {
    let r = request();
    let ordinary = simulate(&r).unwrap();
    assert_eq!(
        ordinary.points.last().unwrap().state,
        ConsumerState::Performing
    );
    assert!(ordinary.points.last().unwrap().undrawn > 0.);
    let day = r.as_of_ordinal + 4;
    let out = simulate_with_zero_balance_closure(&r, day).unwrap();
    assert!(out
        .points
        .iter()
        .filter(|p| p.ordinal < day)
        .all(|p| p.state == ConsumerState::Performing));
    assert!(out
        .points
        .iter()
        .filter(|p| p.ordinal >= day)
        .all(|p| p.state == ConsumerState::PaidOff && p.limit == 0. && p.undrawn == 0.));
    assert_eq!(out.points.len(), ordinary.points.len());
    assert!(out.cashflows.is_empty() && out.recognition.is_empty());
}

#[test]
fn closure_follows_supplied_same_day_payment_without_creating_payoff_cash() {
    let mut r = request();
    r.initial_purchase_grace = true;
    r.opening_principal[0] = 100.;
    let day = r.as_of_ordinal + 4;
    r.steps.push(CardStep {
        ordinal: day,
        action: CardAction::Payment { amount: 100. },
    });
    let out = simulate_with_zero_balance_closure(&r, day).unwrap();
    assert_eq!(out.collected_principal, 100.);
    assert_eq!(out.cashflows.len(), 1);
    assert_eq!(out.closing_grace_contingent_interest, 0.);
    assert_eq!(out.points.last().unwrap().state, ConsumerState::PaidOff);
}

#[test]
fn every_gross_right_blocks_closure_even_when_tiny() {
    for kind in 0..6 {
        let mut r = request();
        match kind {
            0 => r.opening_principal[0] = 1e-100,
            1 => r.opening_billed_interest[0] = 1e-100,
            2 => r.opening_unbilled_interest[0] = 1e-100,
            3 => {
                r.opening_billed_interest[0] = 1e-100;
                r.opening_unrecognized_billed_interest[0] = 1e-100;
            }
            4 => r.opening_fees[0] = 1e-100,
            _ => {
                r.opening_principal[0] = 1.;
                r.opening_contingent_purchase_interest = 1e-100;
            }
        }
        assert!(simulate_with_zero_balance_closure(&r, r.as_of_ordinal)
            .unwrap_err()
            .contains("zero gross rights"));
    }
}

#[test]
fn closure_rejects_outside_clock_future_actions_and_default() {
    let mut r = request();
    assert!(simulate_with_zero_balance_closure(&r, r.as_of_ordinal - 1).is_err());
    assert!(simulate_with_zero_balance_closure(&r, r.as_of_ordinal + 21).is_err());
    r.steps.push(CardStep {
        ordinal: r.as_of_ordinal + 2,
        action: CardAction::SetLimit { amount: 100. },
    });
    assert!(simulate_with_zero_balance_closure(&r, r.as_of_ordinal + 1)
        .unwrap_err()
        .contains("after zero-balance closure"));
    r.steps[0].action = CardAction::Default {};
    assert!(simulate_with_zero_balance_closure(&r, r.as_of_ordinal + 2).is_err());
}
