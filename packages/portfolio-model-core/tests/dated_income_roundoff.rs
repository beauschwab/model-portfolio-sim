use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_model_core::dated_income::{self, DatedIncomeRequest, Identity, IncomeEvent};

fn identity(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "reviewed-v1".into(),
    }
}
fn mortgage() -> DatedIncomeRequest {
    let start = 738916;
    // Explicit performing mortgage investor rights: ten-day earned intervals,
    // remitted two days later, positive frozen original EIR of 2%.
    let coupons = [
        0.09999999999999999,
        0.06609999999999999,
        0.03216609999999999,
    ];
    let events = (1..=32)
        .map(|day| {
            let (principal, interest) = match day {
                12 => (33.9, coupons[0]),
                22 => (33.9339, coupons[1]),
                32 => (32.16609999999999, coupons[2]),
                _ => (0., 0.),
            };
            let earned = if day <= 30 {
                coupons[((day - 1) / 10) as usize] / 10.
            } else {
                0.
            };
            IncomeEvent {
                event_id: format!("mortgage-{day}"),
                source: identity("investor-cash"),
                payment_ordinal: start + day,
                contractual_interest_earned: earned,
                principal_cash: principal,
                interest_cash: interest,
                integral_fee_cash: 0.,
                nonintegral_fee_cash: 0.,
                yield_change: None,
            }
        })
        .collect();
    DatedIncomeRequest {
        schema: dated_income::SCHEMA.into(),
        contract: identity("mortgage"),
        original_yield_anchor: identity("original-yield"),
        policy: identity("reviewed-income"),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Physical,
        as_of_ordinal: start,
        full_contract_end_ordinal: start + 32,
        opening_principal: 100.,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: 100.07972856995012,
        original_annual_effective_rate: 0.02,
        opening_annual_effective_rate: 0.02,
        events,
    }
}
#[test]
fn positive_eir_mortgage_retains_tiny_accrued_right_and_offsetting_basis() {
    let request = mortgage();
    let result = dated_income::evaluate(&request).unwrap();
    let last = result.rows.last().unwrap();
    let bound = 64.
        * f64::EPSILON
        * (last.opening_gross_carrying_value.abs()
            + last.cash_received.abs()
            + last.effective_interest.abs());
    let unclamped = last.opening_gross_carrying_value
        + (last.effective_interest
            - request.events.last().unwrap().principal_cash
            - request.events.last().unwrap().interest_cash);
    assert!(unclamped < 0. && unclamped.abs() <= bound);
    assert_eq!(result.remaining_principal, 0.);
    assert_eq!(result.remaining_gross_carrying_value, 0.);
    assert_ne!(result.remaining_accrued_interest, 0.);
    assert!(result.remaining_accrued_interest.abs() <= bound);
    assert_eq!(
        last.closing_book_adjustment,
        -result.remaining_accrued_interest
    );
    // No cash, interest, or contractual-right plug is introduced.
    for (event, row) in request.events.iter().zip(&result.rows) {
        assert_eq!(
            row.cash_received,
            event.principal_cash + event.interest_cash
        );
        assert_eq!(
            row.contractual_interest_earned,
            event.contractual_interest_earned
        );
        assert_eq!(row.modification_gain_loss, 0.);
    }
}
#[test]
fn genuine_accrued_rights_cannot_enable_negative_carry_clamp() {
    for accrued in [-1e-6, 1e-6] {
        let mut request = mortgage();
        request.opening_accrued_interest = accrued;
        assert!(dated_income::evaluate(&request)
            .unwrap_err()
            .contains("negative gross carrying"));
    }
}
#[test]
fn materially_negative_carry_is_not_a_roundoff_adjustment() {
    let mut request = mortgage();
    let last = request.events.last_mut().unwrap();
    last.interest_cash += 1e-8;
    last.contractual_interest_earned += 1e-8;
    assert!(dated_income::evaluate(&request)
        .unwrap_err()
        .contains("negative gross carrying"));
}
