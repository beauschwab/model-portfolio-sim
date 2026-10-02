use portfolio_model_core::cashflow::ordinal_from_ymd;
use portfolio_model_core::floating_rate::*;
use serde_json::json;

fn date(day: u32) -> i32 {
    ordinal_from_ymd(2026, 1, day).unwrap()
}
fn timestamp(day: u32, hour: i64) -> i64 {
    i64::from(date(day) - 719_163) * 86_400 + hour * 3600
}
fn request(method: &str) -> FloatingRateRequest {
    serde_json::from_value(json!({
        "schema": "floating-rate-1", "position_id": "commercial:1", "currency": "USD",
        "monetary_unit": "currency_units", "direction": "receipt",
        "index": {"index_id": "USD-SOFR", "kind": "overnight",
            "tenor_months": null, "average_calendar_days": null},
        "reference_calendar": {"id": "US-government-securities", "revision": "2026-r1"},
        "fixing_snapshot": {"id": "supplied-sofr", "revision": "immutable-1"},
        "publication_cutoff_unix_seconds": timestamp(12, 20),
        "complete_observation_coverage": true,
        "fixings": [
            {"fixing_id": "fri", "revision": "r1", "index_id": "USD-SOFR",
                "value_ordinal": date(2), "publication_unix_seconds": timestamp(5, 13), "rate": 0.05},
            {"fixing_id": "mon", "revision": "r2", "index_id": "USD-SOFR",
                "value_ordinal": date(5), "publication_unix_seconds": timestamp(6, 13), "rate": 0.06}
        ],
        "periods": [{"period_id": "coupon-1", "start_ordinal": date(2),
            "end_ordinal": date(6), "payment_ordinal": date(6), "calculation": method,
            "margin_treatment": "simple", "day_count": "act360", "margin": 0.,
            "index_bounds": {"floor": null, "cap": null},
            "all_in_bounds": {"floor": null, "cap": null},
            "lookback_business_days": 0, "observation_shift": false,
            "principal": [{"start_ordinal": date(2), "end_ordinal": date(6), "amount": 100000.}],
            "observations": [
                {"interest_start_ordinal": date(2), "interest_end_ordinal": date(5),
                    "observation_start_ordinal": date(2), "observation_end_ordinal": date(5), "fixing_id": "fri"},
                {"interest_start_ordinal": date(5), "interest_end_ordinal": date(6),
                    "observation_start_ordinal": date(5), "observation_end_ordinal": date(6), "fixing_id": "mon"}
            ]}]
    })).unwrap()
}
fn amount(request: &FloatingRateRequest) -> f64 {
    evaluate(request).unwrap().periods[0].interest_amount
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 2e-9,
        "{actual:.15} != {expected:.15}"
    );
}

#[test]
fn weekend_simple_and_compounded_match_hand_calculations() {
    let simple = request("daily_simple");
    close(amount(&simple), 100000. * (0.05 * 3. + 0.06) / 360.);
    let compound = request("daily_compounded");
    let factor = (1. + 0.05 * 3. / 360.) * (1. + 0.06 / 360.);
    close(amount(&compound), 100000. * (factor - 1.));
    let result = evaluate(&compound).unwrap();
    assert_eq!(result.fixing_snapshot.revision, "immutable-1");
    assert_eq!(result.periods[0].fixing_references[1].revision, "r2");
    assert_eq!(result.periods[0].payment_ordinal, date(6));
}

#[test]
fn shifted_observation_weights_are_independent_of_interest_weights() {
    let mut req = request("daily_compounded");
    let p = &mut req.periods[0];
    p.start_ordinal = date(5);
    p.end_ordinal = date(7);
    p.payment_ordinal = date(7);
    p.principal[0].start_ordinal = date(5);
    p.principal[0].end_ordinal = date(7);
    p.lookback_business_days = 1;
    p.observations[0].interest_start_ordinal = date(5);
    p.observations[0].interest_end_ordinal = date(6);
    p.observations[1].interest_start_ordinal = date(6);
    p.observations[1].interest_end_ordinal = date(7);
    let unshifted = 100000. * ((1. + 0.05 / 360.) * (1. + 0.06 / 360.) - 1.);
    close(amount(&req), unshifted);
    req.periods[0].observation_shift = true;
    let shifted = 100000. * ((1. + 0.05 * 3. / 360.) * (1. + 0.06 / 360.) - 1.);
    close(amount(&req), shifted);
    assert!(amount(&req) > unshifted);
    req.periods[0].calculation = Calculation::DailySimple;
    close(amount(&req), 100000. * (0.05 * 3. + 0.06) / 360.);
    req.periods[0].margin = 0.01;
    // Observation shift changes index weights, while an uncapitalized margin
    // accrues on the actual two-day interest period, not the four-day lookback.
    close(
        amount(&req),
        100000. * (0.05 * 3. + 0.06 + 0.01 * 2.) / 360.,
    );
    req.periods[0].calculation = Calculation::DailyCompounded;
    close(amount(&req), shifted + 100000. * 0.01 * 2. / 360.);
}

#[test]
fn index_and_all_in_bounds_and_margin_compounding_are_distinct() {
    let mut req = request("daily_compounded");
    req.periods[0].margin = 0.01;
    req.periods[0].index_bounds = RateBounds {
        floor: Some(0.04),
        cap: Some(0.055),
    };
    req.periods[0].all_in_bounds = RateBounds {
        floor: Some(0.055),
        cap: Some(0.062),
    };
    // Friday index .05 + .01 = .06; Monday index capped .055 + .01,
    // then all-in capped .062. Simple margin remains outside the product.
    let simple_margin =
        100000. * ((1. + 0.05 * 3. / 360.) * (1. + 0.052 / 360.) - 1. + 0.01 * 4. / 360.);
    close(amount(&req), simple_margin);
    req.periods[0].margin_treatment = MarginTreatment::Compounded;
    let compounded_margin = 100000. * ((1. + 0.06 * 3. / 360.) * (1. + 0.062 / 360.) - 1.);
    close(amount(&req), compounded_margin);
    assert!(amount(&req) > simple_margin);
    req.periods[0].calculation = Calculation::DailySimple;
    req.periods[0].margin_treatment = MarginTreatment::Simple;
    close(amount(&req), 100000. * (0.06 * 3. + 0.062) / 360.);
    req.fixings[0].rate = -0.1;
    // Index floor .04 then all-in floor .055: floors are applied in order.
    close(amount(&req), 100000. * (0.055 * 3. + 0.062) / 360.);
}

#[test]
fn simple_interest_uses_daily_principal_but_rate_compounding_rejects_changes() {
    let mut req = request("daily_simple");
    req.periods[0].principal = vec![
        PrincipalInterval {
            start_ordinal: date(2),
            end_ordinal: date(3),
            amount: 100000.,
        },
        PrincipalInterval {
            start_ordinal: date(3),
            end_ordinal: date(6),
            amount: 50000.,
        },
    ];
    close(
        amount(&req),
        (100000. * 0.05 + 50000. * 0.05 * 2. + 50000. * 0.06) / 360.,
    );
    req.periods[0].calculation = Calculation::DailyCompounded;
    assert!(evaluate(&req)
        .err()
        .unwrap()
        .contains("constant period principal"));
    req.periods[0].calculation = Calculation::BalanceCompounded;
    assert!(evaluate(&req).is_err());
    req.periods[0].calculation = Calculation::DailySimple;
    req.periods[0].observation_shift = true;
    assert!(evaluate(&req).is_err());
}

#[test]
fn term_coupon_uses_explicit_fixing_and_tenor_and_supports_principal_changes() {
    let mut req = request("term_simple");
    req.index.kind = IndexKind::Term;
    req.index.tenor_months = Some(6);
    req.fixings.truncate(1);
    req.fixings[0].publication_unix_seconds = timestamp(2, 8);
    req.periods[0].reset_cutoff_unix_seconds = Some(timestamp(2, 9));
    req.periods[0].observations.truncate(1);
    req.periods[0].observations[0].interest_end_ordinal = date(6);
    req.periods[0].observations[0].observation_end_ordinal = ordinal_from_ymd(2026, 7, 2).unwrap();
    req.periods[0].margin = 0.01;
    req.periods[0].principal = vec![
        PrincipalInterval {
            start_ordinal: date(2),
            end_ordinal: date(4),
            amount: 100000.,
        },
        PrincipalInterval {
            start_ordinal: date(4),
            end_ordinal: date(6),
            amount: 50000.,
        },
    ];
    close(amount(&req), 0.06 * (100000. * 2. + 50000. * 2.) / 360.);
    let result = evaluate(&req).unwrap();
    assert_eq!(result.index.tenor_months, Some(6));
    close(result.periods[0].effective_annual_rate.unwrap(), 0.06);
    assert!(!result.calendar_mapping_verified);
    req.periods[0].reset_cutoff_unix_seconds = Some(timestamp(2, 7));
    assert!(evaluate(&req).is_err()); // 08:00 publication unavailable at 07:00 reset
    req.periods[0].reset_cutoff_unix_seconds = Some(timestamp(2, 9));
    assert!(evaluate(&req).is_ok());
    req.direction = Direction::Payment;
    close(amount(&req), -0.06 * (100000. * 2. + 50000. * 2.) / 360.);
    req.fixings[0].publication_unix_seconds = timestamp(5, 13);
    assert!(evaluate(&req).is_err()); // cannot observe an in-advance rate later
}

#[test]
fn payment_adjustment_is_independent_of_accrual_end() {
    let mut req = request("daily_simple");
    // A supplied preceding adjustment settles before unadjusted accrual end.
    req.periods[0].payment_ordinal = date(5);
    close(amount(&req), 100000. * (0.05 * 3. + 0.06) / 360.);
    assert_eq!(evaluate(&req).unwrap().periods[0].payment_ordinal, date(5));
}

#[test]
fn near_zero_negative_and_zero_balance_are_stable() {
    let mut req = request("daily_compounded");
    for fixing in &mut req.fixings {
        fixing.rate = 1e-14;
    }
    let expected = 100000. * (1e-14 * 4. / 360.);
    let actual = amount(&req);
    assert!(actual > 0.);
    assert!((actual - expected).abs() < 1e-26);
    for fixing in &mut req.fixings {
        fixing.rate = -0.01;
    }
    close(
        amount(&req),
        100000. * ((1. - 0.01 * 3. / 360.) * (1. - 0.01 / 360.) - 1.),
    );
    req.periods[0].principal[0].amount = 0.;
    let result = evaluate(&req).unwrap();
    assert_eq!(result.periods[0].interest_amount, 0.);
    assert!(result.periods[0].effective_annual_rate.is_none());
}

#[test]
fn unavailable_duplicate_wrong_index_and_incomplete_coverage_fail() {
    let base = request("daily_simple");
    let mut req = base.clone();
    req.publication_cutoff_unix_seconds = timestamp(6, 12);
    assert!(evaluate(&req).is_err()); // last fixing arrives at 13 UTC
    req = base.clone();
    req.fixings.push(req.fixings[0].clone());
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.fixings[0].index_id = "other-index".into();
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods[0].observations[1].fixing_id = "missing".into();
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods[0].observations[1].interest_start_ordinal = date(4);
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods[0].principal[0].end_ordinal = date(5);
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.complete_observation_coverage = false;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.fixings[0].publication_unix_seconds = timestamp(1, 13);
    assert!(evaluate(&req).is_err()); // publication cannot precede value date
}

#[test]
fn monetary_metadata_numerical_and_work_limits_are_independent() {
    let base = request("daily_compounded");
    let mut req = base.clone();
    req.periods[0].principal[0].amount = 1e15 + 1.;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.monetary_unit = "millions".into();
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.position_id = "x".repeat(1025);
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods[0].index_bounds = RateBounds {
        floor: Some(0.1),
        cap: Some(0.01),
    };
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.fixings[0].rate = -120.; // factor 1-120*3/360 = 0
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.fixings[0].rate = f64::NAN;
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods = vec![base.periods[0].clone(); MAX_PERIODS + 1];
    assert!(evaluate(&req).is_err());
    req = base.clone();
    req.periods[0].observations = vec![base.periods[0].observations[0].clone(); MAX_INTERVALS];
    assert!(evaluate(&req).err().unwrap().contains("work admission"));
}
