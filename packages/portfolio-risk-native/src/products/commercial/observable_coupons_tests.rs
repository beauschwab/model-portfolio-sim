//! Actual observable source, credit-state and original-yield controls.
use super::tests::request;
use super::*;
use crate::term_enhancements::indexed;
use serde_json::json;

fn observable_request(calculation: &str, margin_treatment: &str) -> Request {
    let mut raw = request(5);
    let asof = raw.source.current.asof_ordinal;
    let observations: Vec<_> = (0..3).map(|day| {
            let publication = (i64::from(asof + day + 1) - 719163) * 86400 + 8 * 3600;
            json!({"interest_start_ordinal":asof+day,"interest_end_ordinal":asof+day+1,
                "observation_start_ordinal":asof+day,"observation_end_ordinal":asof+day+1,
                "fixing_ordinal":asof+day,"selection_cutoff_unix_seconds":publication,"rate":0.04+f64::from(day)*0.01,
                "source":{"kind":"conditional_published_projection","model":{"id":"synthetic-publication-model","revision":"1"},"path":{"id":"base","revision":"1"},"measure":"physical","state_information_unix_seconds":(i64::from(asof+day)-719163)*86400,"publication_unix_seconds":publication}})
        }).collect();
    for source in [&mut raw.source.original, &mut raw.source.current] {
        source.periods[0].end_ordinal = asof + 3;
        source.tail_end_ordinal = asof + 5;
        source.reporting_end_ordinal = asof + 5;
        source.observed_publication_cutoff_unix_seconds = (i64::from(asof) - 719163) * 86400;
        source.periods[0].coupon = serde_json::from_value(json!({"kind":"indexed",
                "index":{"index_id":"synthetic-overnight","kind":"overnight","tenor_months":null,"average_calendar_days":null},
                "calendar":{"id":"reviewed-daily-mapping","revision":"1"},"complete_calendar_mapping":true,
                "calculation":calculation,"margin_treatment":margin_treatment,"margin":0.01,
                "index_bounds":{"floor":null,"cap":null},"all_in_bounds":{"floor":null,"cap":null},
                "reset_cutoff_unix_seconds":null,"lookback_business_days":0,"observation_shift":false,
                "observations":observations})).unwrap();
    }
    let interest = if calculation == "daily_compounded" {
        let product = [0.04_f64, 0.05, 0.06].iter().fold(1., |product, rate| {
            product
                * (1.
                    + (rate
                        + if margin_treatment == "compounded" {
                            0.01
                        } else {
                            0.
                        })
                        / 365.)
        });
        100. * (product - 1.
            + if margin_treatment == "simple" {
                0.01 * 3. / 365.
            } else {
                0.
            })
    } else {
        100. * (0.05 + 0.06 + 0.07) / 365.
    };
    raw.source.acquisition.cost = 100. + interest;
    raw.credit.intervals[0].end_year = 5. / 365.;
    raw.factor_policy.overnight = "observable_index_prefix_simple_margin-1".into();
    raw.factor_policy.observation_mapping =
        "selected_observation_weight_over_interest_daycount-1".into();
    raw
}

#[test]
fn observable_daily_native_credit_earns_only_known_index_and_separate_simple_margin() {
    let raw = observable_request("daily_simple", "simple");
    let result = evaluate(&raw).unwrap();
    let points = result.carry().carry_points();
    let before = points
        .iter()
        .find(|p| p.kind == "live_sojourn" && p.year == 1. / 365.)
        .unwrap();
    assert!((before.contractual_accrued_interest - 100. * 0.01 / 365.).abs() < 1e-12);
    let information_year = (86400. + 8. * 3600.) / 31_536_000.;
    let at = points
        .iter()
        .find(|p| p.kind == "live_sojourn" && p.year == information_year)
        .unwrap();
    let hand = 100. * (0.04 / 365. + 0.01 * information_year);
    assert!((at.contractual_accrued_interest - hand).abs() < 1e-12);
    let closing = &result.accounting().closing()[0];
    assert_eq!(closing.legal_interest, 0.);
    assert_eq!(
        result.original_source.source().schedule.events[0]
            .components
            .cash_interest,
        result.current_source().source().schedule.events[0]
            .components
            .cash_interest
    );
    assert!(
        (result.carry().native_source_cash()[0].cash.interest - 100. * 0.18 / 365.).abs() < 1e-12
    );
    assert_eq!(
        result
            .raw_request
            .source
            .acquisition
            .original_annual_effective_rate,
        0.
    );
}

#[test]
fn observable_compound_margins_match_independent_product_and_full_cash() {
    for treatment in ["simple", "compounded"] {
        let raw = observable_request("daily_compounded", treatment);
        let result = evaluate(&raw).unwrap();
        let first = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "live_sojourn" && p.year == 1. / 365.)
            .unwrap();
        let expected = if treatment == "simple" {
            100. * 0.01 / 365.
        } else {
            0.
        };
        assert!((first.contractual_accrued_interest - expected).abs() < 1e-12);
        let at = result
            .carry()
            .carry_points()
            .iter()
            .find(|p| p.kind == "live_sojourn" && p.year == 4. / 365.)
            .unwrap();
        let full = result.current_source().source().audit[0].interest;
        assert!((at.contractual_accrued_interest - full).abs() < 1e-12);
    }
}

#[test]
fn observable_future_rate_edit_cannot_change_earlier_legal_or_recognized_prefix() {
    let raw = observable_request("daily_compounded", "simple");
    let baseline = evaluate(&raw).unwrap();
    let mut changed = raw.clone();
    let term::indexed::Coupon::Indexed { observations, .. } =
        &mut changed.source.current.periods[0].coupon
    else {
        unreachable!()
    };
    observations[2].rate = 0.9;
    let revised = evaluate(&changed).unwrap();
    let cutoff = 2. / 365.;
    let select = |r: &ResultData| {
        r.carry()
            .carry_points()
            .iter()
            .filter(|p| p.year <= cutoff)
            .map(|p| serde_json::to_vec(p).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(select(&baseline), select(&revised));
    assert_eq!(
        baseline.raw_request.source.acquisition.cost,
        revised.raw_request.source.acquisition.cost
    );
}

#[test]
fn observable_nonaccrual_preserves_legal_known_prefix_without_recognized_coupon() {
    let mut raw = observable_request("daily_simple", "simple");
    raw.accounting.recognition[0].by_origin = [book::Recognition::LegalOnlyIncomeStopped; 3];
    let result = evaluate(&raw).unwrap();
    let point = result
        .accounting()
        .groups()
        .iter()
        .find(|g| g.phase == "live_sojourn" && g.year == 2. / 365.)
        .unwrap();
    assert!(point.next.legal_interest > 0.);
    assert_eq!(point.next.recognized_interest, 0.);
}

#[test]
fn observable_unknown_exit_rights_refuse_instead_of_waiver_or_lookahead() {
    for destination in [3, 4] {
        let mut raw = observable_request("daily_simple", "simple");
        let hazard = 10.;
        raw.credit.intervals[0].hazards_per_year[0][destination] = hazard;
        raw.credit.draw_stream = serde_json::from_value(json!({"kind":"supplied","stream_id":"terminal-observation","stream_revision":"1",
                "paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-hazard*0.5/365_f64).exp(),"destination_uniform":0.5}]}]})).unwrap();
        let error = evaluate(&raw).unwrap_err();
        assert!(
            error.contains("unquantified pre-exit coupon obligation"),
            "{error}"
        );
    }
    let mut raw = observable_request("daily_simple", "simple");
    raw.factor_policy.overnight = "pre_observation_accumulator_partial_weight-1".into();
    raw.factor_policy.observation_mapping = "explicit_equal_calendar_span-1".into();
    assert!(evaluate(&raw)
        .unwrap_err()
        .contains("observable native factor policy"));
}

fn enable_terminal(raw: &mut Request) {
    raw.factor_policy.terminal_obligations=Some(serde_json::from_value(json!({"review":{"id":"explicit-contingent-rights","revision":"1"},
        "live_exit_recognition":"recognize_known_served_right_at_information-1","live_exit_settlement":"retain_original_coupon_settlement_after_information-1",
        "default_recognition":"new_known_default_right_is_legal_only-1","default_recovery":"original_signed_interest_fraction_and_tails_after_information-1"})).unwrap());
}
fn terminal_at(raw: &mut Request, destination: usize, day: f64) {
    let hazard = 10.;
    raw.credit.intervals[0].hazards_per_year[0][destination] = hazard;
    raw.credit.draw_stream=serde_json::from_value(json!({"kind":"supplied","stream_id":"pending-terminal","stream_revision":"1",
        "paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-hazard*day/365_f64).exp(),"destination_uniform":0.5}]}]})).unwrap();
}
#[test]
fn observable_prepay_keeps_unquantified_right_then_original_later_settlement() {
    let mut raw = observable_request("daily_simple", "simple");
    enable_terminal(&mut raw);
    terminal_at(&mut raw, 4, 0.5);
    let result = evaluate(&raw).unwrap();
    let exit = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "full_live_exit")
        .unwrap();
    assert!((exit.cash - (100. + 100. * 0.01 * 0.5 / 365.)).abs() < 1e-12);
    let fixes: Vec<_> = result
        .carry()
        .carry_points()
        .iter()
        .filter(|p| p.kind == "pending_coupon_fixing")
        .collect();
    assert_eq!(fixes.len(), 1);
    assert_eq!(fixes[0].year, (86400. + 8. * 3600.) / 31_536_000.);
    assert!((fixes[0].contractual_accrued_interest - 100. * 0.04 * 0.5 / 365.).abs() < 1e-12);
    let cash = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "pending_coupon_cash")
        .unwrap();
    assert_eq!(cash.year, 5. / 365.);
    assert!((cash.cash - 100. * 0.04 * 0.5 / 365.).abs() < 1e-12);
    assert_eq!(result.accounting().closing()[0].principal, 0.);
    assert!(result.accounting().closing()[0].legal_interest.abs() < 1e-12);
    assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
}
#[test]
fn observable_default_retains_pending_legal_right_without_default_income_or_lgd_substitution() {
    let mut raw = observable_request("daily_simple", "simple");
    enable_terminal(&mut raw);
    terminal_at(&mut raw, 3, 0.5);
    raw.credit.intervals[0].loss_given_default = [0.4; 3];
    raw.credit.intervals[0].interest_recovery_fraction = [0.25; 3];
    let result = evaluate(&raw).unwrap();
    let path = &result.carry().credit().result.paths[0];
    assert!(
        (path.default.as_ref().unwrap().accrued_interest - 100. * 0.01 * 0.5 / 365.).abs() < 1e-12
    );
    let fix = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "pending_coupon_fixing")
        .unwrap();
    assert_eq!(fix.effective_interest, 0.);
    let cash = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "pending_coupon_cash")
        .unwrap();
    assert_eq!(cash.year, fix.year);
    assert!((cash.cash - 100. * 0.04 * 0.5 / 365. * 0.25).abs() < 1e-12);
    assert!(
        (result.accounting().closing()[0].legal_interest - 100. * 0.05 * 0.5 / 365. * 0.75).abs()
            < 1e-12
    );
    assert_eq!(result.accounting().closing()[0].principal, 40.);
}

#[test]
fn observable_exercise_before_publication_keeps_original_coupon_cash_tail() {
    let mut raw = observable_request("daily_compounded", "simple");
    enable_terminal(&mut raw);
    let stop = NativeStop::from_compiled_policy(
        17,
        "native-early-call".into(),
        raw.source.current.asof_ordinal + 1,
        NativeStopAction::Call,
        1.,
        0.,
        term::Identity {
            id: "call-review".into(),
            revision: "1".into(),
        },
    )
    .unwrap();
    let result = evaluate_with_stops(&raw, &[stop]).unwrap();
    let exit = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "full_live_exit")
        .unwrap();
    assert_eq!(exit.year, 1. / 365.);
    assert!(!exit.pending_coupon_obligation_indices.is_empty());
    let fix = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "pending_coupon_fixing")
        .unwrap();
    assert!(fix.pending_coupon_obligation_indices.is_empty());
    let cash = result
        .carry()
        .carry_points()
        .iter()
        .find(|p| p.kind == "pending_coupon_cash")
        .unwrap();
    assert_eq!(cash.year, 5. / 365.);
    assert!((cash.cash - 100. * 0.04 / 365.).abs() < 1e-12);
    assert_eq!(result.accounting().closing()[0].gross_carry, 0.);
}
#[test]
fn observable_signed_pending_default_claims_use_reviewed_interest_fraction_and_ordered_recoveries()
{
    let mut raw = observable_request("daily_simple", "simple");
    enable_terminal(&mut raw);
    terminal_at(&mut raw, 3, 0.5);
    for source in [&mut raw.source.original, &mut raw.source.current] {
        let term::indexed::Coupon::Indexed { observations, .. } = &mut source.periods[0].coupon
        else {
            unreachable!()
        };
        for observation in observations {
            observation.rate = -0.04;
        }
    }
    raw.source.acquisition.cost = 100. + 100. * (-0.03) * 3. / 365.;
    raw.credit.intervals[0].loss_given_default = [0.4; 3];
    raw.credit.intervals[0].interest_recovery_fraction = [0.25; 3];
    raw.credit.intervals[0].complete_recovery_tails = std::array::from_fn(|_| {
        vec![
            path::RecoveryWeight {
                lag_years: 0.25 / 365.,
                weight: 0.4,
            },
            path::RecoveryWeight {
                lag_years: 4. / 365.,
                weight: 0.6,
            },
        ]
    });
    let result = evaluate(&raw).unwrap();
    let pending: Vec<_> = result
        .carry()
        .carry_points()
        .iter()
        .filter(|p| p.kind == "pending_coupon_cash")
        .collect();
    assert_eq!(pending.len(), 2);
    assert!(pending[0].cash < 0.);
    assert!(pending[0].year < pending[1].year);
    assert!(result.accounting().closing()[0].legal_interest < 0.);
    let groups = result.accounting().groups();
    assert!(groups.windows(2).all(|pair| pair[0].year <= pair[1].year));
    assert!(groups
        .iter()
        .filter(|g| g.phase == "pending_coupon_fixing")
        .all(|g| g.effective_interest == 0.));
}

#[test]
fn observable_pending_default_reviews_retain_fragment_waivers_and_voluntary_cash_once() {
    let mut raw = observable_request("daily_simple", "simple");
    enable_terminal(&mut raw);
    terminal_at(&mut raw, 3, 0.5);
    raw.credit.intervals[0].loss_given_default = [0.4; 3];
    raw.credit.intervals[0].interest_recovery_fraction = [0.25; 3];
    raw.credit.intervals[0].complete_recovery_tails = std::array::from_fn(|_| {
        vec![path::RecoveryWeight {
            lag_years: 4. / 365.,
            weight: 1.,
        }]
    });
    let plain = evaluate(&raw).unwrap();
    let at = raw.source.current.asof_ordinal;
    let publication = (i64::from(at + 1) - 719163) * 86400 + 8 * 3600;
    let pending_id = format!("commercial-interest:17:pending-coupon:0:{publication}:0");
    let delta = 100. * 0.04 * 0.5 / 365.;
    raw.reviewed_credit_actions = Some(serde_json::from_value(serde_json::json!({
        "policy":{"identity":{"id":"reviewed-pending-default","revision":"1"},"accrued":"transfer_until_reviewed_reversal","basis":"transfer_until_reviewed_chargeoff","allowance_shortfall":"expense_at_reviewed_chargeoff","excess_cash_recovery":"recognize_in_earnings"},
        "allowance_reviews":[{"path_key":17,"review":{"review":{"id":"provision","revision":"1"},"recognition_ordinal":at+1,"phase":"before_credit","target_amount":50.}}],
        "chargeoffs":[{"path_key":17,"review":{"review":{"id":"chargeoff","revision":"1"},"claim_id":"commercial-default:17","recognition_ordinal":at+2,"principal_amount":20.,"book_adjustment_amount":0.,"accrued_interest_reversal":0.}}],
        "legal_waivers":[{"path_key":17,"review":{"id":"pending-waiver","revision":"1"},"recognition_ordinal":at+2,"claim_id":pending_id,"principal_amount":0.,"accrued_interest_amount":delta*0.5,"future_cash_disposition":"retain_supplied_voluntary_recovery_cash-1"}]
    })).unwrap());
    let result = evaluate(&raw).unwrap();
    assert_eq!(
        serde_json::to_value(result.carry().credit()).unwrap(),
        serde_json::to_value(plain.carry().credit()).unwrap()
    );
    let before = result
        .accounting()
        .groups()
        .iter()
        .find(|g| g.phase == "reviewed_allowance_target")
        .unwrap();
    assert!(!before
        .next
        .lots
        .iter()
        .any(|l| l.coupon_id.starts_with("pending-coupon:")));
    let fixing = result
        .accounting()
        .groups()
        .iter()
        .find(|g| g.phase == "pending_coupon_fixing")
        .unwrap();
    assert_eq!(fixing.effective_interest, 0.);
    let next = &result.accounting().closing()[0];
    let late = next
        .lots
        .iter()
        .find(|l| l.coupon_id.starts_with("pending-coupon:"))
        .unwrap();
    assert!((late.legal_remaining - delta * 0.25).abs() < 1e-12);
    assert_eq!(late.recognized_remaining, 0.);
    assert_eq!(next.principal, 20.);
    let reviews = next.reviewed_credit_state.as_ref().unwrap();
    assert_eq!(reviews.allowance, 30.);
    assert_eq!(reviews.legal_waivers.len(), 1);
    let cash = result
        .accounting()
        .groups()
        .iter()
        .find(|g| g.phase == "pending_coupon_cash")
        .unwrap();
    assert!((cash.year - 4.5 / 365.).abs() < 1e-15);
    assert!((cash.cash - delta * 0.25).abs() < 1e-12);
    assert!(cash.postings.iter().any(|p| p.posting.account
        == portfolio_model_core::credit_accounting::AccountCode::RecoveryIncome));
    let bad = raw.reviewed_credit_actions.as_mut().unwrap();
    bad.legal_waivers[0].recognition_ordinal = at + 1;
    assert!(evaluate(&raw)
        .unwrap_err()
        .contains("unknown native coupon claim"));
}

#[test]
fn counted_observable_terminal_default_and_exit_meter_every_query_family() {
    for destination in [3, 4] {
        let mut raw = observable_request("daily_compounded", "simple");
        enable_terminal(&mut raw);
        terminal_at(&mut raw, destination, 0.5);
        let expected = evaluate(&raw).unwrap();
        crate::commercial_generated_policy::bind_counted_source(&mut raw).unwrap();
        let result = evaluate(&raw).unwrap();
        let source = result.current_source();
        let operations = source.counted_operations().unwrap();
        assert!(
            operations.observations > 0
                && operations.earned_calls > 0
                && operations.canonical_calls > 0
                && operations.pending_calls > 0
                && operations.terminal_calls == 1
        );
        assert!(operations.binding > source.segments().len());
        assert!(
            operations.work().unwrap()
                <= source
                    .admission()
                    .counted_work
                    .as_ref()
                    .unwrap()
                    .upper
                    .work()
                    .unwrap()
        );
        assert_eq!(
            portfolio_model_core::strict_json::encode(&result.accounting().groups(), 1 << 20)
                .unwrap(),
            portfolio_model_core::strict_json::encode(&expected.accounting().groups(), 1 << 20)
                .unwrap()
        );
    }
}

#[test]
fn counted_cash_inventory_covers_signed_multi_draw_default_fragments_and_each_tail() {
    for sign in [-1., 1.] {
        let mut raw = observable_request("daily_compounded", "simple");
        enable_terminal(&mut raw);
        let at = raw.source.current.asof_ordinal;
        for contract in [&mut raw.source.original, &mut raw.source.current] {
            contract.tail_end_ordinal = at + 8;
            contract.reporting_end_ordinal = at + 8;
            contract.periods[0].payment_ordinal = at + 8;
            let indexed::Coupon::Indexed {
                observations,
                margin,
                ..
            } = &mut contract.periods[0].coupon
            else {
                unreachable!()
            };
            *margin *= sign;
            for (index, obs) in observations.iter_mut().enumerate() {
                obs.rate *= sign;
                let publication = (i64::from(at + 5) - 719163) * 86400 + index as i64 * 3600;
                obs.selection_cutoff_unix_seconds = publication;
                let indexed::RateSource::ConditionalPublishedProjection {
                    publication_unix_seconds,
                    ..
                } = &mut obs.source
                else {
                    unreachable!()
                };
                *publication_unix_seconds = publication;
            }
        }
        let product = [0.04, 0.05, 0.06]
            .iter()
            .fold(1., |p, r| p * (1. + sign * r / 365.));
        raw.source.acquisition.cost = 100. * (product + sign * 0.01 * 3. / 365.);
        raw.credit.intervals[0].end_year = 8. / 365.;
        raw.credit.opening_undrawn_commitment = 40.;
        raw.credit.max_jumps_per_path = 1;
        for interval in &mut raw.credit.intervals {
            for tails in &mut interval.complete_recovery_tails {
                *tails = (0..3)
                    .map(|i| path::RecoveryWeight {
                        lag_years: f64::from(i) / 365.,
                        weight: 1. / 3.,
                    })
                    .collect();
            }
        }
        for index in 0..2 {
            let mut source = raw.source.clone();
            for contract in [&mut source.original, &mut source.current] {
                contract.contract.id = format!("metered-draw-{index}");
                contract.asof_ordinal = at + index + 1;
                contract.opening_principal = 20.;
                contract.principal_plan = indexed::PrincipalPlan::Supplied {
                    principal_payments: vec![20.],
                };
                contract.periods[0].period_id = format!("metered-draw-coupon-{index}");
                contract.periods[0].start_ordinal = at + index + 1;
                contract.periods[0].end_ordinal = at + 4 + index;
                contract.periods[0].payment_ordinal = at + 7 + index;
                contract.periods[0].coupon = indexed::Coupon::Fixed { annual_rate: 0. };
            }
            source.acquisition.cost = 20.;
            source.acquisition.original_annual_effective_rate = 0.;
            raw.performing_draw_tranches.push(PerformingDrawTranche {
                review: term::Identity {
                    id: format!("draw-review-{index}"),
                    revision: "1".into(),
                },
                draw_ordinal: at + index + 1,
                eligible_origins: [true, false, false],
                source,
            });
        }
        let zero_hazard_bound =
            crate::commercial_stochastic_exercise::cash_event_bound(&raw).unwrap();
        terminal_at(&mut raw, 3, 2.5);
        crate::commercial_generated_policy::bind_counted_source(&mut raw).unwrap();
        assert_eq!(
            zero_hazard_bound,
            crate::commercial_stochastic_exercise::cash_event_bound(&raw).unwrap()
        );
        let actual = evaluate(&raw).unwrap();
        let cash = crate::commercial_cash::economic_cash(actual.carry()).unwrap();
        assert_eq!(cash.iter().filter(|c| c.kind == "draw").count(), 2);
        assert_eq!(
            cash.iter()
                .filter(|c| c.kind == "pending_coupon_cash")
                .count(),
            3 * 3
        );
        assert!(cash.len() <= zero_hazard_bound);
    }
}
#[test]
fn counted_cash_inventory_covers_before_and_after_scheduled_exercise() {
    for timing in [
        NativeStopTiming::BeforeScheduledCash,
        NativeStopTiming::AfterScheduledCash,
    ] {
        let mut raw = observable_request("daily_simple", "simple");
        enable_terminal(&mut raw);
        crate::commercial_generated_policy::bind_counted_source(&mut raw).unwrap();
        let stop = NativeStop::from_compiled_policy_with_timing(
            17,
            "metered-timing-call".into(),
            raw.source.current.asof_ordinal + 1,
            NativeStopAction::Call,
            1.,
            0.,
            term::Identity {
                id: "metered-call-policy".into(),
                revision: "1".into(),
            },
            timing,
            None,
        )
        .unwrap();
        let result = evaluate_with_stops(&raw, &[stop]).unwrap();
        let cash = crate::commercial_cash::economic_cash(result.carry()).unwrap();
        assert!(cash.iter().any(|c| c.kind == "pending_coupon_cash"));
        assert!(
            cash.len() <= crate::commercial_stochastic_exercise::cash_event_bound(&raw).unwrap()
        );
    }
}
