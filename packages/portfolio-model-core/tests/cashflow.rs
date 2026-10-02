use portfolio_model_core::cashflow::*;
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};

fn event(day: u32, identity: &str, scenario: ScenarioSelector) -> DatedCashflowEvent {
    let as_of = ordinal_from_ymd(2024, 1, 31).unwrap();
    DatedCashflowEvent {
        position_id: "loan-1".into(),
        event_id: identity.into(),
        scenario,
        payment_ordinal: as_of + day as i32,
        day_offset: day,
        accrual_period: Some(AccrualPeriod {
            start_ordinal: as_of,
            end_ordinal: as_of + day as i32,
        }),
        currency: "USD".into(),
        components: CashflowComponents {
            principal: 100.0,
            cash_interest: 5.0,
            accrued_interest: 3.0,
            book_amortization: -2.0,
            fees: 1.0,
        },
        measure: CashflowMeasure::Contractual,
        provenance: EventProvenance::Contractual {
            source_id: "contract-1".into(),
            source_revision: "revision-7".into(),
        },
        adjustment: CashflowAdjustment::None {},
    }
}

fn schedule(events: Vec<DatedCashflowEvent>) -> DatedSchedule {
    DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: ordinal_from_ymd(2024, 1, 31).unwrap(),
        horizon_days: 366,
        scenarios: vec!["base".into(), "stress".into()],
        events,
    }
}

#[test]
fn gregorian_ordinals_and_leap_century_rules_are_hand_checked() {
    assert_eq!(ordinal_from_ymd(1, 1, 1).unwrap(), 1);
    assert_eq!(ordinal_from_ymd(1970, 1, 1).unwrap(), 719163);
    assert_eq!(ordinal_from_ymd(2024, 2, 29).unwrap(), 738945);
    assert_eq!(ordinal_from_ymd(9999, 12, 31).unwrap(), MAX_ORDINAL);
    assert_eq!(ymd_from_ordinal(738945).unwrap(), (2024, 2, 29));
    assert_eq!(ymd_from_ordinal(MAX_ORDINAL).unwrap(), (9999, 12, 31));
    assert!(ordinal_from_ymd(1900, 2, 29).is_err());
    assert!(ordinal_from_ymd(2000, 2, 29).is_ok());
    for bad in [0, -1, MAX_ORDINAL + 1] {
        assert!(ymd_from_ordinal(bad).is_err());
    }
    assert!(ordinal_from_ymd(2024, 13, 1).is_err());
}

#[test]
fn calendar_months_preserve_actual_february_and_explicit_roll_policy() {
    let jan31 = ordinal_from_ymd(2024, 1, 31).unwrap();
    assert_eq!(
        ymd_from_ordinal(add_calendar_months(jan31, 1, MonthRoll::PreserveEndOfMonth).unwrap())
            .unwrap(),
        (2024, 2, 29)
    );
    assert_eq!(
        ymd_from_ordinal(add_calendar_months(jan31, 2, MonthRoll::PreserveEndOfMonth).unwrap())
            .unwrap(),
        (2024, 3, 31)
    );
    assert_eq!(
        add_calendar_months(jan31, 1, MonthRoll::ClampDay).unwrap() - jan31,
        29
    );
    assert!(add_calendar_months(jan31, 1, MonthRoll::RejectInvalidDay).is_err());
    let feb29 = ordinal_from_ymd(2024, 2, 29).unwrap();
    assert_eq!(
        ymd_from_ordinal(add_calendar_months(feb29, 1, MonthRoll::ClampDay).unwrap()).unwrap(),
        (2024, 3, 29)
    );
    assert_eq!(
        ymd_from_ordinal(add_calendar_months(feb29, 1, MonthRoll::PreserveEndOfMonth).unwrap())
            .unwrap(),
        (2024, 3, 31)
    );
    assert!(add_calendar_months(1, -1, MonthRoll::ClampDay).is_err());
    assert!(add_calendar_months(MAX_ORDINAL, i32::MAX, MonthRoll::ClampDay).is_err());
}

#[test]
fn actual_accrual_matches_independent_calendar_counts() {
    let jan31 = ordinal_from_ymd(2024, 1, 31).unwrap();
    let feb29 = ordinal_from_ymd(2024, 2, 29).unwrap();
    assert_eq!(
        actual_year_fraction(jan31, feb29, ActualDayCount::Act360).unwrap(),
        29.0 / 360.0
    );
    assert_eq!(
        actual_year_fraction(jan31, feb29, ActualDayCount::Act365Fixed).unwrap(),
        29.0 / 365.0
    );
    assert_eq!(
        actual_year_fraction(jan31, feb29, ActualDayCount::ActActIsda).unwrap(),
        29.0 / 366.0
    );
    let start = ordinal_from_ymd(2023, 12, 31).unwrap();
    let end = ordinal_from_ymd(2024, 3, 1).unwrap();
    assert!(
        (actual_year_fraction(start, end, ActualDayCount::ActActIsda).unwrap()
            - (1.0 / 365.0 + 60.0 / 366.0))
            .abs()
            < 1e-15
    );
    assert_eq!(
        actual_year_fraction(start, start, ActualDayCount::ActActIsda).unwrap(),
        0.0
    );
    assert!(actual_year_fraction(end, start, ActualDayCount::Act360).is_err());
    // $1000, 6% coupon, 29 actual days, ACT/360 = $4.833333... .
    let coupon =
        1000.0 * 0.06 * actual_year_fraction(jan31, feb29, ActualDayCount::Act360).unwrap();
    assert!((coupon - 29.0 / 6.0).abs() < 1e-14);
}

#[test]
fn weekend_compounding_uses_single_three_day_simple_factor() {
    // Friday 2024-03-01 through Tuesday 2024-03-05: Fri rate applies 3
    // calendar days, Monday rate 1. This is not three Friday compound terms.
    let start = ordinal_from_ymd(2024, 3, 1).unwrap();
    let intervals = [
        OvernightRateInterval {
            start_ordinal: start,
            end_ordinal: start + 3,
            rate: 0.05,
        },
        OvernightRateInterval {
            start_ordinal: start + 3,
            end_ordinal: start + 4,
            rate: 0.06,
        },
    ];
    let factor = overnight_compounded_factor(start, start + 4, &intervals).unwrap();
    assert!((factor - (1.0 + 0.05 * 3.0 / 360.0) * (1.0 + 0.06 / 360.0)).abs() < 1e-15);
    let daily_weekend = (1.0_f64 + 0.05 / 360.0).powi(3) * (1.0 + 0.06 / 360.0);
    assert!((factor - daily_weekend).abs() > 1e-8);
    assert_eq!(overnight_compounded_factor(start, start, &[]).unwrap(), 1.0);
    let zero = [OvernightRateInterval {
        start_ordinal: start,
        end_ordinal: start + 4,
        rate: 0.0,
    }];
    assert_eq!(
        overnight_compounded_factor(start, start + 4, &zero).unwrap(),
        1.0
    );
}

#[test]
fn overnight_intervals_reject_gaps_overlaps_unknown_rates_and_invalid_factors() {
    let start = 738945;
    let mut values = [OvernightRateInterval {
        start_ordinal: start,
        end_ordinal: start + 1,
        rate: 0.05,
    }];
    assert!(overnight_compounded_factor(start, start + 2, &values).is_err());
    values[0].start_ordinal += 1;
    assert!(overnight_compounded_factor(start, start + 2, &values).is_err());
    values[0].start_ordinal = start;
    values[0].rate = f64::NAN;
    assert!(overnight_compounded_factor(start, start + 1, &values).is_err());
    values[0].rate = -360.0;
    assert!(overnight_compounded_factor(start, start + 1, &values).is_err());
    values[0].rate = -0.01;
    assert!(overnight_compounded_factor(start, start + 1, &values).unwrap() < 1.0);
}

#[test]
fn scenario_resolution_preserves_dates_components_and_order() {
    let common = event(29, "feb-payment", ScenarioSelector::All {});
    let base = event(
        60,
        "march-payment",
        ScenarioSelector::Named {
            scenario_id: "base".into(),
        },
    );
    let mut stress = event(
        60,
        "march-payment",
        ScenarioSelector::Named {
            scenario_id: "stress".into(),
        },
    );
    stress.components.principal = 80.0;
    let schedule = schedule(vec![common.clone(), base.clone(), stress.clone()]);
    schedule.validate().unwrap();
    assert_eq!(
        schedule.for_scenario("base").unwrap(),
        vec![common.clone(), base]
    );
    assert_eq!(
        schedule.for_scenario("stress").unwrap(),
        vec![common, stress]
    );
    assert!(schedule.for_scenario("unknown").is_err());
    assert_eq!(schedule.events[0].components.cash_total().unwrap(), 106.0);
}

#[test]
fn duplicate_event_identity_and_common_specific_overlap_fail_in_both_orders() {
    let common = event(29, "payment", ScenarioSelector::All {});
    let named = event(
        29,
        "payment",
        ScenarioSelector::Named {
            scenario_id: "base".into(),
        },
    );
    for events in [
        vec![common.clone(), common.clone()],
        vec![named.clone(), named.clone()],
        vec![common.clone(), named.clone()],
        vec![named, common],
    ] {
        assert!(schedule(events).validate().is_err());
    }
    // Independent same-day events are not collapsed.
    schedule(vec![
        event(29, "coupon", ScenarioSelector::All {}),
        event(29, "fee", ScenarioSelector::All {}),
    ])
    .validate()
    .unwrap();
}

#[test]
fn schedule_rejects_mismatched_dates_chronology_horizons_and_domains() {
    let e = event(29, "payment", ScenarioSelector::All {});
    let mut bad = schedule(vec![e.clone()]);
    bad.events[0].day_offset = 30;
    assert!(bad.validate().is_err());
    bad = schedule(vec![
        event(60, "march", ScenarioSelector::All {}),
        e.clone(),
    ]);
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.horizon_days = 28;
    assert!(bad.validate().is_err());
    bad.horizon_days = MAX_HORIZON_DAYS + 1;
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.events[0].payment_ordinal = 0;
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.events[0].components.accrued_interest = f64::INFINITY;
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.events[0].currency = "usd".into();
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.events[0].position_id = "".into();
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.events[0].accrual_period.as_mut().unwrap().end_ordinal =
        bad.as_of_ordinal + bad.horizon_days as i32 + 1;
    assert!(bad.validate().is_err());
    bad = schedule(vec![e.clone()]);
    bad.as_of_ordinal = MAX_ORDINAL;
    assert!(bad.validate().is_err());
    bad = schedule(vec![e]);
    bad.schema = "dated-cashflow-99".into();
    assert!(bad.validate().is_err());
}

#[test]
fn preceding_settlement_may_occur_before_contractual_accrual_end() {
    let mut s = schedule(vec![event(29, "payment", ScenarioSelector::All {})]);
    let period = s.events[0].accrual_period.as_mut().unwrap();
    period.end_ordinal += 2;
    s.validate().unwrap();
    assert!(s.events[0].accrual_period.unwrap().end_ordinal > s.events[0].payment_ordinal);
    s.events[0].accrual_period.as_mut().unwrap().start_ordinal =
        s.events[0].accrual_period.unwrap().end_ordinal + 1;
    assert!(s.validate().is_err());
}

#[test]
fn undeclared_scenarios_provenance_and_limits_fail_closed() {
    let mut s = schedule(vec![event(
        29,
        "payment",
        ScenarioSelector::Named {
            scenario_id: "missing".into(),
        },
    )]);
    assert!(s.validate().is_err());
    s.scenarios.push("missing".into());
    s.validate().unwrap();
    s.scenarios.push("missing".into());
    assert!(s.validate().is_err());
    s = schedule(vec![event(29, "payment", ScenarioSelector::All {})]);
    s.events[0].measure = CashflowMeasure::Physical;
    assert!(s.validate().is_err());
    s.events[0].provenance = EventProvenance::Modeled {
        model_id: "mortgage-1".into(),
        model_version: "1".into(),
        input_revision: "immutable-input".into(),
    };
    s.validate().unwrap();
    s.events[0].provenance = EventProvenance::Assumed {
        assumption_id: "input-1".into(),
        rationale: "".into(),
    };
    assert!(s.validate().is_err());
    s = schedule(vec![]);
    s.scenarios.clear();
    assert!(s.validate().is_err());
    s.scenarios = (0..=MAX_SCENARIOS).map(|i| i.to_string()).collect();
    assert!(s.validate().is_err());
    s = schedule(vec![]);
    s.events = (0..=MAX_EVENTS)
        .map(|i| event(29, &i.to_string(), ScenarioSelector::All {}))
        .collect();
    assert!(s.validate().is_err());
}

#[test]
fn proportional_overlay_is_explicit_and_zero_shock_is_exact_identity() {
    let e = event(29, "payment", ScenarioSelector::All {});
    let unchanged = e.proportionally_adjust(1.0, "").unwrap();
    assert_eq!(
        serde_json::to_vec(&e).unwrap(),
        serde_json::to_vec(&unchanged).unwrap()
    );
    let changed = e
        .proportionally_adjust(0.8, "balance overlay pending joint product regeneration")
        .unwrap();
    assert_eq!(changed.components.principal, 80.0);
    assert_eq!(changed.components.cash_interest, 4.0);
    assert_eq!(changed.payment_ordinal, e.payment_ordinal);
    assert!(matches!(
        changed.adjustment,
        CashflowAdjustment::ProportionalSurvivingPrincipal {
            surviving_fraction: 0.8,
            ..
        }
    ));
    schedule(vec![changed.clone()]).validate().unwrap();
    assert!(changed
        .proportionally_adjust(0.5, "another overlay")
        .is_err());
    assert!(e.proportionally_adjust(1.01, "invalid balance").is_err());
    assert!(e
        .proportionally_adjust(f64::NAN, "invalid balance")
        .is_err());
    assert!(e.proportionally_adjust(0.8, "").is_err());
    assert_eq!(
        e.proportionally_adjust(0.0, "fully extinguished balance")
            .unwrap()
            .components
            .cash_total()
            .unwrap(),
        0.0
    );
}

#[test]
fn strict_json_contract_roundtrips_and_rejects_unknown_fields() {
    let s = schedule(vec![event(29, "payment", ScenarioSelector::All {})]);
    let bytes = serde_json::to_vec(&s).unwrap();
    let restored: DatedSchedule = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, s);
    restored.validate().unwrap();
    let mut json = serde_json::to_value(&s).unwrap();
    json["events"][0]["hidden_month_offset"] = 1.into();
    assert!(serde_json::from_value::<DatedSchedule>(json).is_err());
    let mut json = serde_json::to_value(&s).unwrap();
    json["events"][0]["scenario"]["implicit_override"] = true.into();
    assert!(serde_json::from_value::<DatedSchedule>(json).is_err());
}

#[test]
fn numeric_overflow_and_position_admission_are_validated() {
    let mut s = schedule(vec![event(29, "payment", ScenarioSelector::All {})]);
    s.events[0].components.principal = f64::MAX;
    s.events[0].components.cash_interest = f64::MAX;
    assert!(s.validate().is_err());
    let mut s = schedule(vec![]);
    s.events = (0..=MAX_POSITIONS)
        .map(|i| {
            let mut e = event(29, "payment", ScenarioSelector::All {});
            e.position_id = i.to_string();
            e
        })
        .collect();
    assert!(s.validate().is_err());
}

#[test]
fn signed_currency_amounts_and_total_metadata_have_explicit_bounds() {
    let mut s = schedule(vec![event(29, "payment", ScenarioSelector::All {})]);
    s.events[0].components = CashflowComponents {
        principal: -MAX_MONETARY_AMOUNT,
        cash_interest: 0.0,
        accrued_interest: 0.0,
        book_amortization: 0.0,
        fees: 0.0,
    };
    s.validate().unwrap();
    s.events[0].components.principal = -MAX_MONETARY_AMOUNT - 1.0;
    assert!(s.validate().is_err());
    s.events[0].components.principal = MAX_MONETARY_AMOUNT;
    s.events[0].components.cash_interest = 1.0;
    assert!(s.validate().is_err());
    s.events[0].components.cash_interest = 0.0;
    s.events[0].components.book_amortization = MAX_MONETARY_AMOUNT + 1.0;
    assert!(s.validate().is_err());
    // Every individual string is valid and the row count is admitted, but
    // combined supplied metadata is greater than 32 MiB.
    let rationale = "a".repeat(1024);
    let count = MAX_DATED_METADATA_BYTES / rationale.len() + 1;
    s = schedule(
        (0..count)
            .map(|i| {
                let mut e = event(29, &i.to_string(), ScenarioSelector::All {});
                e.provenance = EventProvenance::Assumed {
                    assumption_id: "source".into(),
                    rationale: rationale.clone(),
                };
                e
            })
            .collect(),
    );
    assert!(s.events.len() < MAX_EVENTS);
    assert!(s.validate().unwrap_err().contains("metadata"));
}
