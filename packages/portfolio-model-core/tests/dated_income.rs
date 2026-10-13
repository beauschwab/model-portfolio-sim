use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_model_core::dated_income::*;
fn id(name: &str) -> Identity {
    Identity {
        id: name.into(),
        revision: "reviewed-v1".into(),
    }
}
fn event(day: i32, principal: f64, coupon: f64) -> IncomeEvent {
    IncomeEvent {
        event_id: format!("event-{day}"),
        source: id("cash-source"),
        payment_ordinal: 738886 + day,
        contractual_interest_earned: coupon,
        principal_cash: principal,
        interest_cash: coupon,
        integral_fee_cash: 0.,
        nonintegral_fee_cash: 0.,
        yield_change: None,
    }
}
fn request() -> DatedIncomeRequest {
    DatedIncomeRequest {
        schema: SCHEMA.into(),
        contract: id("loan"),
        original_yield_anchor: id("original-yield"),
        policy: id("supplied-policy"),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Contractual,
        as_of_ordinal: 738886,
        full_contract_end_ordinal: 739616,
        opening_principal: 100.,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: 100.,
        original_annual_effective_rate: 0.1,
        opening_annual_effective_rate: 0.1,
        events: vec![event(365, 100., 10.)],
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
#[test]
fn full_year_equal_coupon_yield_retires_exactly() {
    let out = evaluate(&request()).unwrap();
    near(out.rows[0].effective_interest, 10.);
    near(out.remaining_gross_carrying_value, 0.);
    near(out.rows[0].basis_amortization, 0.);
}
#[test]
fn leap_and_stub_use_actual_days_not_reporting_months() {
    let mut r = request();
    r.events = vec![event(29, 0., 0.), event(365, 100., 10.)];
    let out = evaluate(&r).unwrap();
    near(
        out.rows[0].effective_interest,
        100. * (1.1_f64.powf(29. / 365.) - 1.),
    );
    near(out.remaining_gross_carrying_value, 0.);
}
#[test]
fn integral_and_nonintegral_fee_ownership_is_separate() {
    let mut r = request();
    r.events[0].principal_cash = 90.;
    r.events[0].integral_fee_cash = 10.;
    r.events[0].nonintegral_fee_cash = 3.;
    let out = evaluate(&r).unwrap();
    near(out.rows[0].cash_received, 113.);
    near(out.rows[0].recognized_nonintegral_fee_income, 3.);
    near(out.rows[0].basis_amortization, -10.);
    near(out.remaining_principal, 10.);
    near(out.remaining_gross_carrying_value, 0.);
}
#[test]
fn seasoned_anchor_is_retained_and_not_refit() {
    let mut r = request();
    r.opening_principal = 60.;
    r.opening_gross_carrying_value = 58.;
    r.events = vec![event(30, 10., 1.)];
    let out = evaluate(&r).unwrap();
    near(
        out.rows[0].effective_interest,
        58. * (1.1_f64.powf(30. / 365.) - 1.),
    );
    near(out.remaining_principal, 50.);
}
#[test]
fn reviewed_prospective_rate_changes_only_future_intervals() {
    let mut r = request();
    let mut first = event(365, 0., 10.);
    first.yield_change = Some(YieldChange::Prospective {
        review: id("reset"),
        annual_effective_rate: 0.2,
    });
    r.events = vec![first, event(730, 100., 20.)];
    let out = evaluate(&r).unwrap();
    near(out.rows[0].effective_interest, 10.);
    near(out.rows[1].effective_interest, 20.);
    near(out.remaining_gross_carrying_value, 0.);
}
#[test]
fn catchup_uses_original_anchor_and_complete_revised_source() {
    let mut r = request();
    let mut first = event(365, 0., 10.);
    first.yield_change = Some(YieldChange::CatchupOriginal {
        review: id("modification"),
        complete_future_cash: vec![FutureCash {
            payment_ordinal: 739616,
            amount: 99.,
        }],
    });
    r.events = vec![first, event(730, 90., 9.)];
    let out = evaluate(&r).unwrap();
    near(out.rows[0].modification_gain_loss, -10.);
    near(out.rows[0].closing_gross_carrying_value, 90.);
    near(out.remaining_gross_carrying_value, 0.);
}
#[test]
fn unrelated_catchup_rights_and_pricing_measure_fail() {
    let mut r = request();
    r.events[0].yield_change = Some(YieldChange::CatchupOriginal {
        review: id("m"),
        complete_future_cash: vec![FutureCash {
            payment_ordinal: 739616,
            amount: 3.,
        }],
    });
    assert!(evaluate(&r).is_err());
    r = request();
    r.measure = CashflowMeasure::Pricing;
    assert!(evaluate(&r).is_err());
}
#[test]
fn negative_yield_is_not_clipped() {
    let mut r = request();
    r.original_annual_effective_rate = -0.1;
    r.opening_annual_effective_rate = -0.1;
    r.opening_gross_carrying_value = 100. / 0.9;
    r.events = vec![event(365, 100., 0.)];
    let out = evaluate(&r).unwrap();
    near(out.rows[0].effective_interest, -100. / 9.);
    near(out.remaining_gross_carrying_value, 0.);
}
#[test]
fn invalid_dates_duplicates_units_and_overflow_fail_before_result() {
    for case in 0..5 {
        let mut r = request();
        match case {
            0 => r.events[0].payment_ordinal = r.as_of_ordinal - 1,
            1 => r.events.push(r.events[0].clone()),
            2 => r.monetary_unit = "millions".into(),
            3 => r.events[0].principal_cash = 101.,
            _ => r.opening_gross_carrying_value = f64::NAN,
        }
        assert!(evaluate(&r).is_err());
    }
}
