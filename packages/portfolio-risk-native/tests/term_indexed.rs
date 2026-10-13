use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::floating_rate::{
    Calculation, Direction, IndexDescriptor, IndexKind, MarginTreatment, RateBounds,
};
use portfolio_risk_native::term_enhancements::{indexed::*, Identity, MODEL_VERSION, SCHEMA};

fn day(y: i32, m: u32, d: u32) -> i32 {
    ordinal_from_ymd(y, m, d).unwrap()
}
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn utc(day: i32, hour: i64) -> i64 {
    i64::from(day - 719163) * 86400 + hour * 3600
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn period(n: &str, start: i32, end: i32, rate: f64) -> ContractPeriod {
    ContractPeriod {
        period_id: n.into(),
        start_ordinal: start,
        end_ordinal: end,
        payment_ordinal: end,
        day_count: ActualDayCount::Act360,
        coupon: Coupon::Fixed { annual_rate: rate },
        fee: 0.,
    }
}
fn request() -> ContractPathRequest {
    let start = day(2024, 2, 1);
    let end = day(2024, 4, 1);
    ContractPathRequest {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        contract: id("loan"),
        assumption: id("contract-policy"),
        path: id("base-path:7"),
        position_id: "loan:7".into(),
        scenario_id: "base".into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        direction: Direction::Receipt,
        measure: CashflowMeasure::Contractual,
        asof_ordinal: start,
        reporting_end_ordinal: day(2024, 3, 1),
        tail_end_ordinal: end,
        observed_publication_cutoff_unix_seconds: utc(start, 23),
        opening_principal: 100.,
        principal_plan: PrincipalPlan::Supplied {
            principal_payments: vec![20., 80.],
        },
        periods: vec![
            period("feb", start, day(2024, 3, 1), 0.12),
            period("mar", day(2024, 3, 1), end, 0.12),
        ],
    }
}
fn index_coupon(
    request: &ContractPathRequest,
    period: &ContractPeriod,
    tenor: u32,
    rate: f64,
) -> Coupon {
    let reset = utc(period.start_ordinal, 9);
    Coupon::Indexed {
        index: IndexDescriptor {
            index_id: format!("explicit-index-{tenor}m"),
            kind: IndexKind::Term,
            tenor_months: Some(tenor),
            average_calendar_days: None,
        },
        calendar: Box::new(id("calendar")),
        complete_calendar_mapping: true,
        calculation: Calculation::TermSimple,
        margin_treatment: MarginTreatment::Simple,
        margin: 0.01,
        index_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        all_in_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        reset_cutoff_unix_seconds: Some(reset),
        lookback_business_days: 0,
        observation_shift: false,
        observations: vec![IndexObservation {
            interest_start_ordinal: period.start_ordinal,
            interest_end_ordinal: period.end_ordinal,
            observation_start_ordinal: period.start_ordinal,
            observation_end_ordinal: period.end_ordinal,
            fixing_ordinal: period.start_ordinal,
            selection_cutoff_unix_seconds: reset,
            rate,
            source: RateSource::ConditionalProjection {
                model: id("conditional-forward"),
                path: request.path.clone(),
                information_unix_seconds: reset,
                measure: CashflowMeasure::Pricing,
            },
        }],
    }
}

#[test]
fn supplied_balloon_leap_periods_fees_and_full_tail_conserve() {
    let mut r = request();
    r.periods[0].fee = 2.;
    let x = build_path(&r).unwrap();
    close(x.audit[0].interest, 100. * 0.12 * 29. / 360.);
    close(x.audit[1].interest, 80. * 0.12 * 31. / 360.);
    close(x.principal_after_reporting, 80.);
    close(
        x.schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        100.,
    );
    assert_eq!(x.schedule.events[1].payment_ordinal, r.tail_end_ordinal);
    close(x.schedule.events[0].components.fees, 2.);
    r.direction = Direction::Payment;
    let y = build_path(&r).unwrap();
    for (a, b) in x.schedule.events.iter().zip(&y.schedule.events) {
        close(
            a.components.cash_total().unwrap(),
            -b.components.cash_total().unwrap(),
        );
    }
}
#[test]
fn retained_original_payment_and_explicit_balloon_follow_hand_recurrence() {
    let mut r = request();
    r.principal_plan = PrincipalPlan::RetainedPayment {
        original_payment: 30.,
        terminal: TerminalPrincipal::ResidualPayoff {},
    };
    let interest1 = 100. * 0.12 * 29. / 360.;
    let remaining = 100. - (30. - interest1);
    let interest2 = remaining * 0.12 * 31. / 360.;
    let balloon = remaining - (30. - interest2);
    let x = build_path(&r).unwrap();
    close(x.audit[0].closing_principal, remaining);
    close(x.audit[1].interest, interest2);
    close(x.audit[1].terminal_balloon, balloon);
    close(x.audit[1].principal_paid, remaining);
    r.principal_plan = PrincipalPlan::RetainedPayment {
        original_payment: 30.,
        terminal: TerminalPrincipal::Balloon { amount: balloon },
    };
    build_path(&r).unwrap();
    r.principal_plan = PrincipalPlan::RetainedPayment {
        original_payment: 30.,
        terminal: TerminalPrincipal::Balloon {
            amount: balloon + 1.,
        },
    };
    assert!(build_path(&r).is_err());
    r.principal_plan = PrincipalPlan::RetainedPayment {
        original_payment: 0.01,
        terminal: TerminalPrincipal::ResidualPayoff {},
    };
    assert!(build_path(&r).is_err());
}
#[test]
fn explicit_one_and_six_month_indices_select_independent_conditional_rates() {
    let mut r = request();
    r.measure = CashflowMeasure::Pricing;
    r.periods[0].coupon = index_coupon(&r, &r.periods[0], 1, 0.02);
    r.periods[1].coupon = index_coupon(&r, &r.periods[1], 6, 0.05);
    let x = build_path(&r).unwrap();
    close(x.audit[0].interest, 100. * 0.03 * 29. / 360.);
    close(x.audit[1].interest, 80. * 0.06 * 31. / 360.);
    assert!(!x.audit[0].calendar_mapping_verified);
    assert!(matches!(
        x.audit[1].selected_rate_sources[0],
        RateSource::ConditionalProjection { .. }
    ));
    r.measure = CashflowMeasure::Physical;
    assert!(build_path(&r).is_err());
}
#[test]
fn observed_publication_and_projected_reset_use_exact_intraday_information() {
    let mut r = request();
    r.measure = CashflowMeasure::Pricing;
    r.periods[0].coupon = index_coupon(&r, &r.periods[0], 1, 0.02);
    if let Coupon::Indexed {
        reset_cutoff_unix_seconds,
        observations,
        ..
    } = &mut r.periods[0].coupon
    {
        observations[0].source = RateSource::Observed {
            fixing: id("published"),
            publication_unix_seconds: utc(observations[0].fixing_ordinal, 8),
        };
        *reset_cutoff_unix_seconds = Some(utc(observations[0].fixing_ordinal, 7));
    }
    assert!(build_path(&r).unwrap_err().contains("lookahead"));
    if let Coupon::Indexed {
        reset_cutoff_unix_seconds,
        observations,
        ..
    } = &mut r.periods[0].coupon
    {
        *reset_cutoff_unix_seconds = Some(utc(observations[0].fixing_ordinal, 9));
    }
    build_path(&r).unwrap();
    r.observed_publication_cutoff_unix_seconds = utc(r.asof_ordinal, 7);
    assert!(build_path(&r).is_err());
}
#[test]
fn compounded_weekend_shift_simple_margin_bounds_and_nearzero_are_handcomputed() {
    let start = day(2026, 1, 2);
    let end = day(2026, 1, 6);
    let mut r = request();
    r.asof_ordinal = start;
    r.reporting_end_ordinal = start;
    r.tail_end_ordinal = end;
    r.measure = CashflowMeasure::Pricing;
    r.periods = vec![period("weekend", start, end, 0.)];
    r.principal_plan = PrincipalPlan::Supplied {
        principal_payments: vec![100.],
    };
    let observation = |a, b, oa, ob, rate| IndexObservation {
        interest_start_ordinal: a,
        interest_end_ordinal: b,
        observation_start_ordinal: oa,
        observation_end_ordinal: ob,
        fixing_ordinal: oa,
        selection_cutoff_unix_seconds: utc(oa, 0),
        rate,
        source: RateSource::ConditionalProjection {
            model: id("overnight-model"),
            path: r.path.clone(),
            information_unix_seconds: utc(oa, 0),
            measure: CashflowMeasure::Pricing,
        },
    };
    r.periods[0].coupon = Coupon::Indexed {
        index: IndexDescriptor {
            index_id: "overnight".into(),
            kind: IndexKind::Overnight,
            tenor_months: None,
            average_calendar_days: None,
        },
        calendar: Box::new(id("supplied-calendar")),
        complete_calendar_mapping: true,
        calculation: Calculation::DailyCompounded,
        margin_treatment: MarginTreatment::Simple,
        margin: 0.01,
        index_bounds: RateBounds {
            floor: Some(0.),
            cap: Some(0.05),
        },
        all_in_bounds: RateBounds {
            floor: Some(0.02),
            cap: Some(0.055),
        },
        reset_cutoff_unix_seconds: None,
        lookback_business_days: 1,
        observation_shift: true,
        observations: vec![
            observation(start, start + 3, start - 1, start, 0.06),
            observation(start + 3, end, start, start + 3, -0.02),
        ],
    };
    // Bound rates: 0.055 and 0.02 all-in; simple margin excluded
    // from product, observed shifted weights 1 and 3, margin weights 3+1.
    let expected = 100. * ((1. + 0.045 / 360.) * (1. + 0.01 * 3. / 360.) - 1. + 0.01 * 4. / 360.);
    close(build_path(&r).unwrap().audit[0].interest, expected);
    if let Coupon::Indexed {
        margin,
        index_bounds,
        all_in_bounds,
        observations,
        observation_shift,
        ..
    } = &mut r.periods[0].coupon
    {
        *margin = 0.;
        *index_bounds = RateBounds {
            floor: None,
            cap: None,
        };
        *all_in_bounds = *index_bounds;
        *observation_shift = false;
        for o in observations {
            o.rate = 1e-14;
        }
    }
    let value = build_path(&r).unwrap().audit[0].interest;
    assert!((value - 100. * 1e-14 * 4. / 360.).abs() < 1e-26);
    // Exact overnight information time, not merely a date comparison.
    if let Coupon::Indexed { observations, .. } = &mut r.periods[0].coupon {
        let late_information = observations[0].selection_cutoff_unix_seconds + 1;
        if let RateSource::ConditionalProjection {
            information_unix_seconds,
            ..
        } = &mut observations[0].source
        {
            *information_unix_seconds = late_information;
        }
    }
    assert!(build_path(&r).unwrap_err().contains("selection cutoff"));
}
#[test]
fn zero_negative_coupon_early_settlement_and_admission_fail_closed() {
    let mut r = request();
    r.periods[0].coupon = Coupon::Fixed { annual_rate: 0. };
    r.periods[1].coupon = Coupon::Fixed { annual_rate: -0.01 };
    r.periods[1].payment_ordinal = r.periods[1].end_ordinal - 1;
    let x = build_path(&r).unwrap();
    close(x.audit[0].interest, 0.);
    assert!(x.audit[1].interest < 0.);
    r.tail_end_ordinal -= 2;
    assert!(build_path(&r).is_err());
    r = request();
    r.model_version = "legacy".into();
    assert!(build_path(&r).is_err());
    r = request();
    r.opening_principal = f64::NAN;
    assert!(build_path(&r).is_err());
    r = request();
    r.periods[1].period_id = r.periods[0].period_id.clone();
    assert!(build_path(&r).is_err());
    r = request();
    r.periods = vec![r.periods[0].clone(); 4097];
    assert!(build_path(&r).is_err());
    r = request();
    r.principal_plan = PrincipalPlan::Supplied {
        principal_payments: vec![20., 79.],
    };
    assert!(build_path(&r).is_err());
    r = request();
    r.observed_publication_cutoff_unix_seconds = utc(r.asof_ordinal + 1, 0);
    assert!(build_path(&r).unwrap_err().contains("valuation date"));
}
