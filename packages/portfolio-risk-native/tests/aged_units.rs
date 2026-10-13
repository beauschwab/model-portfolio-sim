use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_risk_native::advanced_product::*;
use portfolio_risk_native::aged_units::*;
use portfolio_risk_native::conventions::{Bdc, CalendarSpec, DayCount};
use portfolio_risk_native::dated_term::AccrualDates;
use portfolio_risk_native::model_market::curves::*;
use portfolio_risk_native::term_deck::{Amortization, Product};
fn day(y: i32, m: u32, d: u32) -> i32 {
    ordinal_from_ymd(y, m, d).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn curve(rate: f64) -> SuppliedCurve {
    SuppliedCurve {
        curve_id: format!("collateral-{rate}"),
        source_id: "synthetic".into(),
        source_revision: "v1".into(),
        currency: "USD".into(),
        role: CurveRole::Collateral {
            collateral_index: "SOFR".into(),
        },
        times_years: vec![0., 1., 2., 5.],
        discount_factors: vec![1., (-rate).exp(), (-2. * rate).exp(), (-5. * rate).exp()],
        right_tail: CurveTail::Reject,
    }
}
fn term() -> FixedTermDefinition {
    FixedTermDefinition {
        product: Product::Corporate,
        term_months: 12,
        freq_months: 12,
        daycount: DayCount::Act365,
        coupon: 0.,
        amortization: Amortization::Bullet,
        calendar: CalendarSpec {
            name: "NONE".into(),
            extra_holidays: vec![],
        },
        bdc: Bdc::None,
        accrual_dates: AccrualDates::UnadjustedScheduledDates,
    }
}
fn purchase(date: i32) -> AgedPurchase {
    AgedPurchase {
        template_id: "fixed".into(),
        purchase_ordinal: date,
        quote_price_per_unit: 1.,
        original_annual_effective_yield: 0.,
        original_yield_anchor: SourceIdentity {
            id: "original-yield".into(),
            revision: format!("purchase-{date}"),
        },
    }
}
fn request() -> AgedUnitsRequest {
    AgedUnitsRequest {
        schema: SCHEMA.into(),
        identity: AgedIdentity {
            model_revision: "fixed-slice-v1".into(),
            grid_revision: "actual-dates-v1".into(),
            crn_id: "deterministic-no-draws".into(),
            input_revision: "v1".into(),
        },
        valuation_policy: ValuationPolicy::ContractualDiscounting {},
        curve_as_of_ordinal: day(2024, 1, 31),
        currency: "USD".into(),
        reporting_ordinals: vec![
            day(2024, 1, 31),
            day(2024, 2, 29),
            day(2024, 7, 31),
            day(2025, 1, 31),
            day(2025, 7, 31),
        ],
        base_scenario_id: "base".into(),
        scenarios: vec![
            AgedScenario {
                scenario_id: "base".into(),
                collateral_curve: curve(0.05),
            },
            AgedScenario {
                scenario_id: "up100".into(),
                collateral_curve: curve(0.06),
            },
        ],
        templates: vec![AgedTemplate {
            template_id: "fixed".into(),
            side: 1.,
            accounting_category: "ac".into(),
            capital_policy_revision: "supplied-v1".into(),
            weights: Weights {
                hqla_l2a: 0.2,
                outflow30: 0.,
                asf: 0.,
                rsf: 0.8,
                rwa: 1.,
            },
            term: term(),
        }],
        purchases: vec![purchase(day(2024, 1, 31)), purchase(day(2024, 7, 31))],
        validation_purchases: vec![],
        bump_bp: 25.,
        interpolation_absolute_tolerance: 0.01,
        interpolation_relative_tolerance: 0.01,
    }
}
fn value(o: &AgedUnitsResult, s: usize, n: usize, j: usize, m: usize) -> f64 {
    o.values[(s * o.units.len() + n) * 10 * o.horizon + j * o.horizon + m]
}
#[test]
fn actual_purchase_dates_change_maturity_and_preserve_full_tail() {
    let r = request();
    let o = prepare(&r).unwrap();
    assert_eq!(o.shape, [2, 2, 10, 4]);
    assert_eq!(o.units[0].full_contract_end_ordinal, day(2025, 1, 31));
    assert_eq!(o.units[1].full_contract_end_ordinal, day(2025, 7, 31));
    assert_eq!(o.units[1].h, 2);
    near(value(&o, 0, 1, 1, 0), 0.);
    near(value(&o, 0, 1, 1, 2), 1.);
    assert_eq!(
        o.units[0].original_yield_anchor.revision,
        format!("purchase-{}", day(2024, 1, 31))
    );
}
#[test]
fn aged_dv01_uses_remaining_time_even_with_unchanged_balance() {
    let r = request();
    let o = prepare(&r).unwrap();
    let t0 = (day(2025, 1, 31) - day(2024, 1, 31)) as f64 / 365.;
    let t1 = (day(2025, 1, 31) - day(2024, 2, 29)) as f64 / 365.;
    near(value(&o, 0, 0, 2, 0), (0.0025 * t0).sinh() / 25.);
    near(value(&o, 0, 0, 2, 1), (0.0025 * t1).sinh() / 25.);
    near(value(&o, 0, 0, 1, 0), 1.);
    near(value(&o, 0, 0, 1, 1), 1.);
    assert!(value(&o, 0, 0, 2, 1) < value(&o, 0, 0, 2, 0));
    near(value(&o, 0, 0, 2, 3), 0.);
    near(value(&o, 0, 0, 1, 3), 0.);
}
#[test]
fn base_oas_is_frozen_at_purchase_for_every_age_and_scenario() {
    let r = request();
    let o = prepare(&r).unwrap();
    near(o.units[0].base_oas, -0.05);
    let remaining = (day(2025, 1, 31) - day(2024, 2, 29)) as f64 / 365.;
    near(
        value(&o, 1, 0, 2, 1),
        (-0.01 * remaining).exp() * (0.0025 * remaining).sinh() / 25.,
    );
    assert_eq!(o.units[0].opening_states[0].gross_carrying_value, 1.);
    for m in 0..4 {
        near(value(&o, 0, 0, 0, m), 0.);
        near(value(&o, 1, 0, 0, m), 0.);
    }
}
#[test]
fn native_book_replay_recognizes_effective_income_and_cash_once() {
    let mut r = request();
    r.templates[0].term.coupon = 0.1;
    for p in &mut r.purchases {
        let maturity = if p.purchase_ordinal == day(2024, 1, 31) {
            day(2025, 1, 31)
        } else {
            day(2025, 7, 31)
        };
        let t = (maturity - p.purchase_ordinal) as f64 / 365.;
        p.original_annual_effective_yield = (1. + 0.1 * t).powf(1. / t) - 1.;
    }
    let o = prepare(&r).unwrap();
    let y = r.purchases[0].original_annual_effective_yield;
    near(value(&o, 0, 0, 0, 0), (1. + y).powf(29. / 365.) - 1.);
    near(
        (0..4).map(|m| value(&o, 0, 0, 0, m)).sum(),
        0.1 * 366. / 365.,
    );
    near(o.units[0].opening_states[3].principal, 0.);
    near(o.units[0].opening_states[3].gross_carrying_value, 0.);
}
#[test]
fn equal_principal_replays_actual_retirements_and_risk_not_proxy() {
    let mut r = request();
    r.templates[0].term.freq_months = 6;
    r.templates[0].term.amortization = Amortization::Annuity;
    let o = prepare(&r).unwrap();
    near(value(&o, 0, 0, 1, 2), 0.5);
    let t = (day(2025, 1, 31) - day(2024, 7, 31)) as f64 / 365.;
    near(value(&o, 0, 0, 2, 2), 0.5 * (0.0025 * t).sinh() / 25.);
    assert!((value(&o, 0, 0, 2, 0) * 0.5 - value(&o, 0, 0, 2, 2)).abs() > 1e-6);
}
#[test]
fn cd_magnitudes_have_explicit_liability_sign_and_no_automatic_withdrawal() {
    let mut r = request();
    r.templates[0].term.product = Product::Cd;
    r.templates[0].term.freq_months = 0;
    r.templates[0].side = -1.;
    r.templates[0].accounting_category = "liability".into();
    let o = prepare(&r).unwrap();
    assert!(value(&o, 0, 0, 2, 0) < 0.);
    near(value(&o, 0, 0, 8, 0), 0.);
    near(value(&o, 0, 0, 9, 0), 1.);
    near(value(&o, 0, 0, 5, 0), 0.);
    near(o.units[0].opening_states[3].cash_paid_to_date, 1.);
}
#[test]
fn preceding_early_payment_retires_cash_without_invented_accrued_opening() {
    let mut r = request();
    r.reporting_ordinals = vec![
        day(2024, 7, 31),
        day(2024, 8, 30),
        day(2024, 8, 31),
        day(2024, 9, 30),
    ];
    r.curve_as_of_ordinal = day(2024, 7, 31);
    r.templates[0].term.term_months = 1;
    r.templates[0].term.bdc = Bdc::Preceding;
    r.templates[0].term.calendar.name = "WEEKEND".into();
    r.templates[0].term.coupon = 0.1;
    r.purchases = vec![purchase(day(2024, 7, 31))];
    r.purchases[0].original_annual_effective_yield =
        (1.0_f64 + 0.1 * 31. / 365.).powf(365. / 30.) - 1.;
    let o = prepare(&r).unwrap();
    near(o.units[0].opening_states[1].principal, 0.);
    near(o.units[0].opening_states[1].gross_carrying_value, 0.);
    assert_eq!(o.units[0].full_contract_end_ordinal, day(2024, 8, 31));
    near(value(&o, 0, 0, 2, 1), 0.);
    near(value(&o, 0, 0, 0, 1), 0.);
}
#[test]
fn off_grid_reports_real_native_replay_error_and_no_flat_tail() {
    let mut r = request();
    r.validation_purchases = vec![purchase(day(2024, 2, 29))];
    r.interpolation_absolute_tolerance = 0.;
    r.interpolation_relative_tolerance = 0.;
    let o = prepare(&r).unwrap();
    assert_eq!(o.interpolation_diagnostics.len(), 20);
    let d = o
        .interpolation_diagnostics
        .iter()
        .find(|d| d.scenario_id == "base" && d.metric == "balance")
        .unwrap();
    assert!(d.max_absolute_error > 0.);
    assert!(!d.within_supplied_tolerance);
    r.validation_purchases[0].purchase_ordinal = day(2025, 1, 31);
    assert!(prepare(&r).is_err());
}
#[test]
fn native_full_replay_at_grid_is_exact_and_scenario_order_is_explicit() {
    let r = request();
    let a = prepare(&r).unwrap();
    let b = prepare(&r).unwrap();
    assert_eq!(a.values, b.values);
    let mut reversed = r.clone();
    reversed.scenarios.reverse();
    let c = prepare(&reversed).unwrap();
    assert_eq!(c.scenario_ids, ["up100", "base"]);
    assert_eq!(&a.values[..80], &c.values[80..]);
}
#[test]
fn source_yield_coverage_measure_and_domains_fail_closed() {
    let r = request();
    let mut bad = r.clone();
    bad.purchases[0].original_annual_effective_yield = 0.1;
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.purchases.push(bad.purchases[0].clone());
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.identity.crn_id = "".into();
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.scenarios[1].scenario_id = "base".into();
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.scenarios[0].collateral_curve.role = CurveRole::Index {
        index_id: "SOFR".into(),
    };
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.reporting_ordinals.swap(1, 2);
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.purchases[0].purchase_ordinal += 1;
    assert!(prepare(&bad).is_err());
    let mut bad = r.clone();
    bad.bump_bp = 0.;
    assert!(prepare(&bad).is_err());
    let mut json = serde_json::to_value(&r).unwrap();
    json["pricing_measure"] = "physical".into();
    assert!(serde_json::from_value::<AgedUnitsRequest>(json).is_err());
}
#[test]
fn unsupported_sink_behavior_direction_and_unbracketed_quote_are_rejected() {
    let mut r = request();
    r.templates[0].term.amortization = Amortization::Sink;
    assert!(prepare(&r).is_err());
    let mut r = request();
    r.templates[0].side = -1.;
    assert!(prepare(&r).is_err());
    let p = project_fixed(
        &term(),
        day(2024, 1, 31),
        "x",
        "USD",
        &SourceIdentity {
            id: "x".into(),
            revision: "v1".into(),
        },
    )
    .unwrap();
    assert!(solve_purchase_oas(&p, day(2024, 1, 31), &curve(0.05), 1e15).is_err());
    assert!(price_remaining(&p, day(2024, 1, 31), day(2024, 1, 30), &curve(0.05), 0., 0.).is_err());
}
#[test]
fn cooperative_cancellation_prevents_prepared_result() {
    let c = portfolio_compute_control::Control::new(1000).unwrap();
    c.cancel();
    let result = portfolio_compute_control::scope(Some(c), || prepare(&request()));
    assert_eq!(result.err().unwrap(), "COMPUTE_CANCELLED");
}
#[test]
fn level_payment_fixed_contract_has_actual_calendar_installments() {
    let mut r = request();
    r.templates[0].term.term_months = 4;
    r.templates[0].term.freq_months = 1;
    r.templates[0].term.amortization = Amortization::LevelPayment;
    r.reporting_ordinals = vec![day(2024, 1, 31), day(2024, 3, 31), day(2024, 5, 31)];
    r.purchases = vec![purchase(day(2024, 1, 31))];
    let o = prepare(&r).unwrap();
    near(value(&o, 0, 0, 1, 1), 0.5);
    let a = (day(2024, 4, 30) - day(2024, 3, 31)) as f64 / 365.;
    let b = (day(2024, 5, 31) - day(2024, 3, 31)) as f64 / 365.;
    near(
        value(&o, 0, 0, 2, 1),
        0.25 * ((0.0025 * a).sinh() + (0.0025 * b).sinh()) / 25.,
    );
    near(o.units[0].opening_states[2].principal, 0.);
}
#[test]
fn following_late_cash_retains_pending_right_at_original_maturity() {
    let mut r = request();
    r.curve_as_of_ordinal = day(2024, 2, 29);
    r.reporting_ordinals = vec![
        day(2024, 2, 29),
        day(2024, 3, 31),
        day(2024, 4, 1),
        day(2024, 4, 30),
    ];
    r.templates[0].term.term_months = 1;
    r.templates[0].term.bdc = Bdc::Following;
    r.templates[0].term.calendar.name = "WEEKEND".into();
    // Month clamp gives March29, so choose March31 original maturity by purchasing Jan31 + 2months.
    r.curve_as_of_ordinal = day(2024, 1, 31);
    r.reporting_ordinals[0] = day(2024, 1, 31);
    r.templates[0].term.term_months = 2;
    r.purchases = vec![purchase(day(2024, 1, 31))];
    let o = prepare(&r).unwrap();
    near(value(&o, 0, 0, 1, 1), 1.);
    near(value(&o, 0, 0, 1, 2), 0.);
    assert_eq!(o.units[0].full_contract_end_ordinal, day(2024, 4, 1));
    near(value(&o, 0, 0, 2, 1), (0.0025_f64 / 365.).sinh() / 25.);
}
#[test]
fn negative_yield_single_cd_cash_remains_explicit_contractual_valuation() {
    let mut r = request();
    r.templates[0].term.product = Product::Cd;
    r.templates[0].term.freq_months = 0;
    r.templates[0].term.coupon = -0.01;
    r.templates[0].side = -1.;
    for p in &mut r.purchases {
        let maturity = if p.purchase_ordinal == day(2024, 1, 31) {
            day(2025, 1, 31)
        } else {
            day(2025, 7, 31)
        };
        let t = (maturity - p.purchase_ordinal) as f64 / 365.;
        p.original_annual_effective_yield = (1. - 0.01 * t).powf(1. / t) - 1.;
    }
    let o = prepare(&r).unwrap();
    near(
        (0..4).map(|m| value(&o, 0, 0, 0, m)).sum(),
        0.01 * 366. / 365.,
    );
}
#[test]
fn persisted_aged_example_has_closed_form_first_and_reaged_dv01() {
    let r: AgedUnitsRequest =
        serde_json::from_str(include_str!("../../../examples/models/aged-units-1.json")).unwrap();
    let o = prepare(&r).unwrap();
    assert_eq!(o.shape, [1, 1, 10, 2]);
    near(o.units[0].base_oas, -0.05);
    near(
        value(&o, 0, 0, 2, 0),
        (0.0025_f64 * 366. / 365.).sinh() / 25.,
    );
    near(
        value(&o, 0, 0, 2, 1),
        (0.0025_f64 * 337. / 365.).sinh() / 25.,
    );
    near(value(&o, 0, 0, 0, 0), 0.);
    assert_eq!(o.curve_scenarios[0].collateral_curve.source_revision, "v1");
}
fn funding(o: &AgedUnitsResult, scenario: usize, unit: usize, date: i32) -> f64 {
    let f = &o.funding;
    let at = f.ordinals.iter().position(|d| *d == date).unwrap();
    f.demand[(scenario * f.shape[1] + unit) * f.shape[2] + at]
}
#[test]
fn discount_and_premium_purchase_cash_use_quotes_without_par_gains() {
    for quote in [0.9, 1.1] {
        let mut r = request();
        r.purchases.truncate(1);
        r.purchases[0].quote_price_per_unit = quote;
        r.purchases[0].original_annual_effective_yield = (1.0_f64 / quote).powf(365. / 366.) - 1.;
        let o = prepare(&r).unwrap();
        near(o.funding.purchase_costs[0].bank_cash, -quote);
        near(funding(&o, 0, 0, day(2024, 1, 31)), quote);
        near(funding(&o, 0, 0, day(2024, 2, 29)), quote);
        near(funding(&o, 0, 0, day(2025, 1, 31)), quote - 1.);
        near((0..4).map(|m| value(&o, 0, 0, 0, m)).sum(), 1. - quote);
        near(
            o.funding.cash_events.iter().map(|e| e.bank_cash).sum(),
            1. - quote,
        );
        near(o.units[0].opening_states[0].gross_carrying_value, quote);
    }
}
#[test]
fn contractual_coupon_frees_cash_only_when_settled_not_when_earned() {
    let mut r = request();
    r.purchases.truncate(1);
    r.templates[0].term.coupon = 0.1;
    r.purchases[0].original_annual_effective_yield =
        (1.0_f64 + 0.1 * 366. / 365.).powf(365. / 366.) - 1.;
    let o = prepare(&r).unwrap();
    assert!(value(&o, 0, 0, 0, 0) > 0.);
    near(funding(&o, 0, 0, day(2024, 2, 29)), 1.);
    near(funding(&o, 0, 0, day(2025, 1, 31)), -0.1 * 366. / 365.);
    let e = o
        .funding
        .cash_events
        .iter()
        .find(|e| e.kind == "contractual_settlement")
        .unwrap();
    near(e.principal_cash, 1.);
    near(e.interest_cash, 0.1 * 366. / 365.);
    near(e.purchase_cash, 0.);
}
#[test]
fn complete_funding_tail_exposes_short_cd_maturity_after_reporting() {
    let mut r = request();
    r.purchases.truncate(1);
    r.reporting_ordinals = vec![day(2024, 1, 31), day(2024, 2, 29)];
    let mut cd = r.templates[0].clone();
    cd.template_id = "cd".into();
    cd.side = -1.;
    cd.term.product = Product::Cd;
    cd.term.freq_months = 0;
    cd.term.term_months = 6;
    r.templates.push(cd);
    let mut p = purchase(day(2024, 1, 31));
    p.template_id = "cd".into();
    r.purchases.push(p);
    let o = prepare(&r).unwrap();
    assert!(o.funding.ordinals.contains(&day(2024, 7, 31)));
    near(
        funding(&o, 0, 0, day(2024, 1, 31)) + funding(&o, 0, 1, day(2024, 1, 31)),
        0.,
    );
    near(
        funding(&o, 0, 0, day(2024, 7, 31)) + funding(&o, 0, 1, day(2024, 7, 31)),
        1.,
    );
    near(
        funding(&o, 0, 0, day(2025, 1, 31)) + funding(&o, 0, 1, day(2025, 1, 31)),
        0.,
    );
}
#[test]
fn matched_asset_cd_and_future_funding_have_actual_date_cash_conservation() {
    let mut r = request();
    r.purchases.truncate(1);
    let mut cd = r.templates[0].clone();
    cd.template_id = "cd".into();
    cd.side = -1.;
    cd.term.product = Product::Cd;
    cd.term.freq_months = 0;
    r.templates.push(cd);
    let mut p = purchase(day(2024, 1, 31));
    p.template_id = "cd".into();
    r.purchases.push(p);
    let o = prepare(&r).unwrap();
    for &date in &o.funding.ordinals {
        near(funding(&o, 0, 0, date) + funding(&o, 0, 1, date), 0.);
    }
    r.purchases[1].purchase_ordinal = day(2024, 2, 29);
    let o = prepare(&r).unwrap();
    near(
        funding(&o, 0, 0, day(2024, 1, 31)) + funding(&o, 0, 1, day(2024, 1, 31)),
        1.,
    );
    near(
        funding(&o, 0, 0, day(2024, 2, 29)) + funding(&o, 0, 1, day(2024, 2, 29)),
        0.,
    );
}
#[test]
fn scheduled_principal_cash_funds_actual_later_purchase_and_never_duplicates_income() {
    let mut r = request();
    r.templates[0].term.term_months = 6;
    for p in &mut r.purchases {
        let days = if p.purchase_ordinal == day(2024, 1, 31) {
            182.
        } else {
            184.
        };
        p.quote_price_per_unit = 0.9;
        p.original_annual_effective_yield = (1.0_f64 / 0.9).powf(365. / days) - 1.;
    }
    let o = prepare(&r).unwrap();
    near(
        funding(&o, 0, 0, day(2024, 1, 31)) + funding(&o, 0, 1, day(2024, 1, 31)),
        0.9,
    );
    near(
        funding(&o, 0, 0, day(2024, 7, 31)) + funding(&o, 0, 1, day(2024, 7, 31)),
        0.8,
    );
    near(
        funding(&o, 0, 0, day(2025, 1, 31)) + funding(&o, 0, 1, day(2025, 1, 31)),
        -0.2,
    );
    near(
        (0..2)
            .flat_map(|n| (0..4).map(move |m| (n, m)))
            .map(|(n, m)| value(&o, 0, n, 0, m))
            .sum(),
        0.2,
    );
    near(o.funding.cash_events.iter().map(|e| e.bank_cash).sum(), 0.2);
}
#[test]
fn large_quote_cannot_mask_original_yield_missing_terminal_cash() {
    let mut r = request();
    r.purchases.truncate(1);
    r.purchases[0].quote_price_per_unit = 1e15;
    r.purchases[0].original_annual_effective_yield = -0.99;
    assert!(prepare(&r).is_err());
}
#[test]
fn partial_amortization_funds_later_origination_with_explicit_remaining_cost() {
    let mut r = request();
    r.templates[0].term.term_months = 4;
    r.templates[0].term.freq_months = 2;
    r.templates[0].term.amortization = Amortization::Annuity;
    r.reporting_ordinals = vec![
        day(2024, 1, 31),
        day(2024, 3, 31),
        day(2024, 5, 31),
        day(2024, 7, 31),
        day(2024, 9, 30),
    ];
    r.purchases = vec![purchase(day(2024, 1, 31)), purchase(day(2024, 3, 31))];
    r.purchases[1].quote_price_per_unit = 0.75;
    // .5*(z+z^2)=.75 on two exact 61-day future periods.
    let z = (7.0_f64.sqrt() - 1.) / 2.;
    r.purchases[1].original_annual_effective_yield = z.powf(-365. / 61.) - 1.;
    let o = prepare(&r).unwrap();
    near(
        funding(&o, 0, 0, day(2024, 1, 31)) + funding(&o, 0, 1, day(2024, 1, 31)),
        1.,
    );
    near(
        funding(&o, 0, 0, day(2024, 3, 31)) + funding(&o, 0, 1, day(2024, 3, 31)),
        1.25,
    );
    near(
        funding(&o, 0, 0, day(2024, 5, 31)) + funding(&o, 0, 1, day(2024, 5, 31)),
        0.25,
    );
    near(
        funding(&o, 0, 0, day(2024, 7, 31)) + funding(&o, 0, 1, day(2024, 7, 31)),
        -0.25,
    );
    near(
        o.funding.cash_events.iter().map(|e| e.bank_cash).sum(),
        0.25,
    );
    near(
        (0..2)
            .flat_map(|n| (0..4).map(move |m| (n, m)))
            .map(|(n, m)| value(&o, 0, n, 0, m))
            .sum(),
        0.25,
    );
}
