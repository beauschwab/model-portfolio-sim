use portfolio_risk_native::conventions::Date;
use portfolio_risk_native::dated_term::{DatedTermRequest, DATED_TERM_SCHEMA, MAX_TERM_BATCH};
use serde_json::{json, Value};

fn date(year: i32, month: i32, day: i32) -> i32 {
    Date::new(year, month, day).unwrap().number()
}

fn request_json() -> Value {
    json!({"schema": DATED_TERM_SCHEMA, "product": "corporate", "asof": date(2024,1,31),
        "horizon_days": 366, "calendar": {"name":"NONE"}, "bdc":"NONE",
        "accrual_dates":"unadjusted_scheduled_dates", "scenarios":["base","zero_shock"],
        "positions":[{"position_id":"asset-1", "currency":"USD", "source_id":"terms-1", "source_revision":"r1",
            "accrual_anchor_ordinal":date(2024,1,31), "direction":"receipt",
            "contract":{"maturity":date(2024,3,31), "freq_months":1, "daycount":"ACT/360",
                "coupon":0.06, "notional":1000.0, "price":100.0}}]})
}

fn parse(value: Value) -> DatedTermRequest {
    serde_json::from_value(value).unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "actual={actual:.15}, expected={expected:.15}"
    );
}

#[test]
fn leap_monthend_coupon_cashflows_have_exact_hand_dates_and_components() {
    let result = parse(request_json()).build().unwrap();
    assert_eq!(result.periods.len(), 2);
    let expected = [(date(2024, 2, 29), 29.0), (date(2024, 3, 31), 31.0)];
    for (i, &(payment, days)) in expected.iter().enumerate() {
        let event = &result.schedule.events[i];
        assert_eq!(event.payment_ordinal, payment);
        assert_eq!(event.day_offset, if i == 0 { 29 } else { 60 });
        close(event.components.cash_interest, 1000.0 * 0.06 * days / 360.0);
        assert_eq!(
            event.components.cash_interest,
            event.components.accrued_interest
        );
        assert_eq!(
            event.components.principal,
            if i == 0 { 0.0 } else { 1000.0 }
        );
        assert_eq!(event.components.book_amortization, 0.0);
        assert_eq!(event.components.fees, 0.0);
        assert_eq!(result.periods[i].scheduled_end_ordinal, payment);
    }
    result.schedule.validate().unwrap();
}

#[test]
fn modified_following_good_friday_preserves_unadjusted_accrual_end() {
    let mut value = request_json();
    value["calendar"] = json!({"name":"US"});
    value["bdc"] = "MF".into();
    value["asof"] = date(2024, 2, 29).into();
    value["positions"][0]["accrual_anchor_ordinal"] = date(2024, 2, 29).into();
    // Easter Sunday March31 follows Good Friday March29. MF pays March28.
    let result = parse(value).build().unwrap();
    assert_eq!(result.periods.len(), 1);
    let audit = &result.periods[0];
    assert_eq!(audit.scheduled_end_ordinal, date(2024, 3, 31));
    assert_eq!(audit.accrual_end_ordinal, date(2024, 3, 31));
    assert_eq!(audit.payment_ordinal, date(2024, 3, 28));
    assert_eq!(audit.settlement_lag_days, -3);
    close(audit.accrual_year_fraction, 31.0 / 360.0);
    close(
        result.schedule.events[0].components.cash_interest,
        1000.0 * 0.06 * 31.0 / 360.0,
    );
    assert!(
        result.schedule.events[0]
            .accrual_period
            .unwrap()
            .end_ordinal
            > result.schedule.events[0].payment_ordinal
    );
    result.schedule.validate().unwrap();
}

#[test]
fn adjusted_accrual_policy_changes_interest_explicitly_and_retains_original_dates() {
    let mut value = request_json();
    value["calendar"] = json!({"name":"US"});
    value["bdc"] = "F".into();
    value["asof"] = date(2026, 3, 3).into();
    value["positions"][0]["accrual_anchor_ordinal"] = date(2026, 3, 3).into();
    value["positions"][0]["contract"]["maturity"] = date(2026, 4, 3).into();
    let unadjusted = parse(value.clone()).build().unwrap();
    value["accrual_dates"] = "adjusted_payment_dates".into();
    let adjusted = parse(value).build().unwrap();
    // Good Friday April3 -> Monday April6; 31 vs34 actual accrual days.
    assert_eq!(adjusted.periods[0].payment_ordinal, date(2026, 4, 6));
    assert_eq!(adjusted.periods[0].scheduled_end_ordinal, date(2026, 4, 3));
    close(unadjusted.periods[0].accrual_year_fraction, 31.0 / 360.0);
    close(adjusted.periods[0].accrual_year_fraction, 34.0 / 360.0);
    close(
        adjusted.schedule.events[0].components.cash_interest
            - unadjusted.schedule.events[0].components.cash_interest,
        0.5,
    );
}

#[test]
fn current_coupon_anchor_before_asof_does_not_lose_prevaluation_accrual() {
    let mut value = request_json();
    value["asof"] = date(2024, 2, 15).into();
    let result = parse(value.clone()).build().unwrap();
    assert_eq!(result.schedule.events[0].day_offset, 14);
    assert_eq!(
        result.schedule.events[0]
            .accrual_period
            .unwrap()
            .start_ordinal,
        date(2024, 1, 31)
    );
    close(
        result.schedule.events[0].components.cash_interest,
        1000.0 * 0.06 * 29.0 / 360.0,
    );
    value["asof"] = date(2024, 3, 1).into();
    assert!(parse(value)
        .build()
        .unwrap_err_string()
        .contains("historical"));
}

// Avoid requiring Debug on transport structs whose nested raw Contract does
// not implement it; retain the concrete error for rejection assertions.
trait ErrorText {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorText for Result<T, String> {
    fn unwrap_err_string(self) -> String {
        match self {
            Ok(_) => panic!("expected rejection"),
            Err(error) => error,
        }
    }
}

#[test]
fn unequal_stub_level_payments_match_independent_discounted_annuity() {
    let mut value = request_json();
    value["asof"] = date(2024, 1, 1).into();
    value["positions"][0]["accrual_anchor_ordinal"] = date(2024, 1, 1).into();
    value["positions"][0]["contract"]["maturity"] = date(2024, 4, 17).into();
    value["positions"][0]["contract"]["amort_type"] = "level_payment".into();
    value["positions"][0]["contract"]["coupon"] = 0.12.into();
    let result = parse(value).build().unwrap();
    let hand_dates = [
        date(2024, 1, 1),
        date(2024, 1, 17),
        date(2024, 2, 17),
        date(2024, 3, 17),
        date(2024, 4, 17),
    ];
    let taus: Vec<f64> = hand_dates
        .windows(2)
        .map(|pair| f64::from(pair[1] - pair[0]) / 360.0)
        .collect();
    let (mut factor, mut annuity) = (1.0, 0.0);
    for tau in &taus {
        factor *= 1.0 + 0.12 * tau;
        annuity += 1.0 / factor;
    }
    let payment = 1000.0 / annuity;
    let mut remaining = 1000.0;
    for (i, event) in result.schedule.events.iter().enumerate() {
        assert_eq!(event.payment_ordinal, hand_dates[i + 1]);
        let interest = remaining * 0.12 * taus[i];
        close(event.components.cash_interest, interest);
        close(event.components.principal, payment - interest);
        close(event.components.cash_total().unwrap(), payment);
        remaining -= payment - interest;
    }
    close(remaining, 0.0);
    close(
        result
            .schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        1000.0,
    );
}

#[test]
fn unadjusted_level_payment_uses_unadjusted_taus_not_adjusted_deck() {
    let mut value = request_json();
    value["calendar"] = json!({"name":"US"});
    value["bdc"] = "MF".into();
    value["positions"][0]["contract"]["amort_type"] = "level_payment".into();
    let result = parse(value).build().unwrap();
    assert_eq!(result.periods[1].accrual_end_ordinal, date(2024, 3, 31));
    assert_eq!(result.periods[1].payment_ordinal, date(2024, 3, 28));
    close(
        result.schedule.events[0].components.cash_total().unwrap(),
        result.schedule.events[1].components.cash_total().unwrap(),
    );
    close(result.periods[1].accrual_year_fraction, 31.0 / 360.0);
}

#[test]
fn equal_principal_and_cd_bullet_keep_their_distinct_contractual_semantics() {
    let mut value = request_json();
    value["positions"][0]["contract"]["amort_type"] = "annuity".into();
    let result = parse(value).build().unwrap();
    assert_eq!(result.schedule.events[0].components.principal, 500.0);
    assert_eq!(result.schedule.events[1].components.principal, 500.0);
    close(
        result.schedule.events[1].components.cash_interest,
        500.0 * 0.06 * 31.0 / 360.0,
    );
    let mut value = request_json();
    value["product"] = "cd".into();
    value["positions"][0]["direction"] = "payment".into();
    value["positions"][0]["contract"]["ew_mult"] = 0.into();
    let result = parse(value).build().unwrap();
    assert_eq!(result.schedule.events[0].components.principal, 0.0);
    assert_eq!(result.schedule.events[1].components.principal, -1000.0);
    close(
        result.schedule.events[1].components.cash_interest,
        -1000.0 * 0.06 * 31.0 / 360.0,
    );
}

#[test]
fn single_payment_cd_accrual_has_no_implied_coupon_frequency_or_reinvestment() {
    let mut value = request_json();
    value["product"] = "cd".into();
    value["positions"][0]["contract"]["freq_months"] = Value::Null;
    value["positions"][0]["contract"]["channel"] = "brokered".into();
    let result = parse(value).build().unwrap();
    assert_eq!(result.periods.len(), 1);
    assert_eq!(result.schedule.events[0].components.principal, 1000.0);
    close(
        result.schedule.events[0].components.cash_interest,
        1000.0 * 0.06 * 60.0 / 360.0,
    );
}

#[test]
fn zero_change_and_inert_price_are_exact_serialized_identities() {
    let value = request_json();
    let result = parse(value.clone()).build().unwrap();
    assert_eq!(
        result.schedule.for_scenario("base").unwrap(),
        result.schedule.for_scenario("zero_shock").unwrap()
    );
    let repeated = parse(value.clone()).build().unwrap();
    assert_eq!(
        serde_json::to_vec(&result).unwrap(),
        serde_json::to_vec(&repeated).unwrap()
    );
    let mut changed = value;
    changed["positions"][0]["contract"]["price"] = 123.4.into();
    assert_eq!(
        serde_json::to_vec(&result).unwrap(),
        serde_json::to_vec(&parse(changed).build().unwrap()).unwrap()
    );
}

#[test]
fn equal_date_output_stably_preserves_input_position_order() {
    let mut value = request_json();
    let mut second = value["positions"][0].clone();
    second["position_id"] = "asset-2".into();
    value["positions"].as_array_mut().unwrap().push(second);
    let result = parse(value).build().unwrap();
    assert_eq!(
        result
            .schedule
            .events
            .iter()
            .map(|e| e.position_id.as_str())
            .collect::<Vec<_>>(),
        vec!["asset-1", "asset-2", "asset-1", "asset-2"]
    );
    for (event, audit) in result.schedule.events.iter().zip(&result.periods) {
        assert_eq!(event.position_id, audit.position_id);
        assert_eq!(event.event_id, audit.event_id);
    }
}

#[test]
fn optional_floating_sinking_and_active_deposit_behavior_fail_explicitly() {
    for mutation in [
        json!({"is_float":true}),
        json!({"call_schedule":[[date(2024,2,29),1.0]]}),
        json!({"put_schedule":[[date(2024,2,29),1.0]]}),
        json!({"sink_schedule":[]}),
        json!({"amort_type":"sink"}),
    ] {
        let mut value = request_json();
        for (key, entry) in mutation.as_object().unwrap() {
            value["positions"][0]["contract"][key] = entry.clone();
        }
        assert!(parse(value).build().is_err());
    }
    let mut value = request_json();
    value["product"] = "cd".into();
    assert!(parse(value.clone())
        .build()
        .unwrap_err_string()
        .contains("withdrawal"));
    value["positions"][0]["contract"]["ew_mult"] = 0.into();
    value["positions"][0]["contract"]["amort_type"] = "annuity".into();
    assert!(parse(value).build().is_err());
}

#[test]
fn horizons_calendars_sources_and_batch_domains_fail_before_publication() {
    let mut value = request_json();
    value["horizon_days"] = 59.into();
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    value["calendar"] = json!({"name":"misspelled"});
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    value["positions"][0]["source_revision"] = "".into();
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    value["positions"][0]["accrual_anchor_ordinal"] = date(2024, 2, 1).into();
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    value["positions"][0]["contract"]["notional"] = 1e15_f64.into();
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    let position = value["positions"][0].clone();
    value["positions"] = vec![position; MAX_TERM_BATCH + 1].into();
    assert!(parse(value).build().is_err());
    let mut value = request_json();
    let position = value["positions"][0].clone();
    value["positions"].as_array_mut().unwrap().push(position);
    assert!(parse(value).build().is_err());
    // US holiday-following settlement must be admitted independently from
    // unadjusted accrual: original April3 fits horizon, adjusted April6 does not.
    let mut value = request_json();
    value["calendar"] = json!({"name":"US"});
    value["bdc"] = "F".into();
    value["asof"] = date(2026, 3, 3).into();
    value["horizon_days"] = 31.into();
    value["positions"][0]["accrual_anchor_ordinal"] = date(2026, 3, 3).into();
    value["positions"][0]["contract"]["maturity"] = date(2026, 4, 3).into();
    assert!(parse(value)
        .build()
        .unwrap_err_string()
        .contains("no clipping"));
}

#[test]
fn unknown_json_semantics_are_rejected_and_zero_notional_is_valid() {
    let mut value = request_json();
    value["positions"][0]["contract"]["notional"] = 0.into();
    let result = parse(value).build().unwrap();
    assert!(result
        .schedule
        .events
        .iter()
        .all(|e| e.components.cash_total().unwrap() == 0.0));
    let mut value = request_json();
    value["implicit_calendar_override"] = true.into();
    assert!(serde_json::from_value::<DatedTermRequest>(value).is_err());
    let mut value = request_json();
    value["positions"][0]["credit_adjustment"] = 0.1.into();
    assert!(serde_json::from_value::<DatedTermRequest>(value).is_err());
}

#[test]
fn all_existing_daycounts_keep_their_explicit_native_meanings() {
    for (basis, expected) in [
        ("ACT/360", 366.0 / 360.0),
        ("ACT/365F", 366.0 / 365.0),
        ("ACT/ACT", 1.0),
        ("30/360", 1.0),
    ] {
        let mut value = request_json();
        value["asof"] = date(2024, 1, 1).into();
        value["positions"][0]["accrual_anchor_ordinal"] = date(2024, 1, 1).into();
        value["positions"][0]["contract"]["maturity"] = date(2025, 1, 1).into();
        value["positions"][0]["contract"]["freq_months"] = 12.into();
        value["positions"][0]["contract"]["daycount"] = basis.into();
        let result = parse(value).build().unwrap();
        close(result.periods[0].accrual_year_fraction, expected);
        close(
            result.schedule.events[0].components.cash_interest,
            1000.0 * 0.06 * expected,
        );
    }
}

#[test]
fn explicit_holidays_and_combined_event_audit_metadata_are_bounded() {
    let mut value = request_json();
    value["bdc"] = "F".into();
    value["calendar"] = json!({"name":"WEEKEND","extra_holidays":[date(2024,2,29)]});
    let result = parse(value.clone()).build().unwrap();
    assert_eq!(result.periods[0].payment_ordinal, date(2024, 3, 1));
    assert_eq!(result.periods[0].accrual_end_ordinal, date(2024, 2, 29));
    value["calendar"]["extra_holidays"] = vec![date(2024, 2, 29); 1025].into();
    assert!(parse(value).build().is_err());

    // Per-field lengths, per-position periods and batch size are individually
    // admitted. Repeated result source/audit text still exceeds the 32MiB cap.
    let mut value = request_json();
    value["horizon_days"] = 36600.into();
    value["positions"][0]["contract"]["maturity"] = date(2104, 1, 31).into();
    value["positions"][0]["source_id"] = "s".repeat(256).into();
    value["positions"][0]["source_revision"] = "r".repeat(256).into();
    let template = value["positions"][0].clone();
    value["positions"] = (0..40)
        .map(|i| {
            let mut p = template.clone();
            p["position_id"] = format!("{i:04}{}", "p".repeat(252)).into();
            p
        })
        .collect::<Vec<_>>()
        .into();
    assert!(parse(value)
        .build()
        .unwrap_err_string()
        .contains("metadata"));
}
