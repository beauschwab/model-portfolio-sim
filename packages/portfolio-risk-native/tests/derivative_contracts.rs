use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::floating_rate::{
    Calculation, IndexDescriptor, IndexKind, MarginTreatment, RateBounds,
};
use portfolio_risk_native::derivative_contract::{csa::*, options::*, swaps::*, Identity};
use portfolio_risk_native::term_enhancements::indexed::*;
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn day(d: u32) -> i32 {
    ordinal_from_ymd(2024, 1, d).unwrap()
}
fn utc(d: i32) -> i64 {
    i64::from(d - 719163) * 86400 + 9 * 3600
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
fn swap(asof: i32, start: i32, end: i32, rate: f64, side: Side, path: &str) -> SwapRequest {
    let coupon = Coupon::Indexed {
        index: IndexDescriptor {
            index_id: "term-1m".into(),
            kind: IndexKind::Term,
            tenor_months: Some(1),
            average_calendar_days: None,
        },
        calendar: Box::new(id("calendar")),
        complete_calendar_mapping: true,
        calculation: Calculation::TermSimple,
        margin_treatment: MarginTreatment::Simple,
        margin: 0.,
        index_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        all_in_bounds: RateBounds {
            floor: None,
            cap: None,
        },
        reset_cutoff_unix_seconds: Some(utc(start)),
        lookback_business_days: 0,
        observation_shift: false,
        observations: vec![IndexObservation {
            interest_start_ordinal: start,
            interest_end_ordinal: end,
            observation_start_ordinal: start,
            observation_end_ordinal: end,
            fixing_ordinal: start,
            selection_cutoff_unix_seconds: utc(start),
            rate,
            source: RateSource::ConditionalProjection {
                model: id("index-model"),
                path: id("index-path"),
                information_unix_seconds: utc(asof.min(start)),
                measure: CashflowMeasure::Pricing,
            },
        }],
    };
    let mut points = vec![DiscountPoint {
        ordinal: asof,
        discount_factor: 1.,
    }];
    if end > asof {
        points.push(DiscountPoint {
            ordinal: end,
            discount_factor: 1.,
        });
    }
    SwapRequest {
        contract: id("swap-contract"),
        index_basis: id("index-basis"),
        rate_path: id("index-path"),
        position_id: "swap".into(),
        scenario_id: path.into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        asof_ordinal: asof,
        valuation_cutoff_unix_seconds: utc(asof),
        tail_end_ordinal: end.max(asof),
        payment_timing: PaymentTiming::AfterScheduledPayments,
        side,
        notional: 360.,
        fixed_rate: 0.05,
        fixed_periods: vec![FixedPeriod {
            period_id: "f1".into(),
            start_ordinal: start,
            end_ordinal: end,
            payment_ordinal: end,
            day_count: ActualDayCount::Act360,
        }],
        floating_periods: vec![ContractPeriod {
            period_id: "l1".into(),
            start_ordinal: start,
            end_ordinal: end,
            payment_ordinal: end,
            day_count: ActualDayCount::Act360,
            coupon,
            fee: 0.,
        }],
        discount_curve: DiscountCurve {
            source: id("discount"),
            collateral_discount_policy: id("discount-policy"),
            currency: "USD".into(),
            asof_ordinal: asof,
            information_unix_seconds: utc(asof),
            basis: "collateralized_clean".into(),
            points,
        },
    }
}
fn option() -> SwaptionRequest {
    let mut u = swap(day(3), day(3), day(13), 0.1, Side::Payer, "option");
    u.tail_end_ordinal = day(13);
    SwaptionRequest {
        contract: id("option-contract"),
        exercise_policy: id("european-positive-value"),
        conditional_model: id("tree-model"),
        crn: id("shared-crn"),
        position_id: "option".into(),
        scenario_id: "option".into(),
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Pricing,
        asof_ordinal: day(1),
        expiry_ordinal: day(3),
        settlement_ordinal: day(3),
        tail_end_ordinal: day(13),
        side: OptionSide::Long,
        settlement: Settlement::CollateralizedCashPrice,
        states: vec![
            TerminalState {
                state_id: "up".into(),
                probability: 0.5,
                discount_to_expiry: 0.99,
                discount_source: id("tree-discount"),
                underlying: u.clone(),
                delivered_position_id: "delivered-swap".into(),
                netting_set_id: "ns".into(),
            },
            TerminalState {
                state_id: "down".into(),
                probability: 0.5,
                discount_to_expiry: 0.99,
                discount_source: id("tree-discount"),
                underlying: {
                    let mut d = u;
                    if let Coupon::Indexed { observations, .. } = &mut d.floating_periods[0].coupon
                    {
                        observations[0].rate = 0.02;
                    }
                    d
                },
                delivered_position_id: "delivered-swap".into(),
                netting_set_id: "ns".into(),
            },
        ],
    }
}
fn node(d: u32, available: bool) -> Node {
    Node {
        ordinal: day(d),
        valuation_cutoff_unix_seconds: utc(day(d)),
        payment_timing: PaymentTiming::AfterScheduledPayments,
        trades: vec![Trade::Swap {
            request: Box::new(swap(day(d), day(1), day(11), 0.1, Side::Payer, "p")),
        }],
        discount_from_asof: 1.,
        discount_source: id("discount"),
        collateral_rate: 0.,
        collateral_rate_source: id("collateral-rate"),
        funding_spread: 0.,
        funding_source: id("funding"),
        settlement_available: available,
    }
}
fn csa() -> CsaRequest {
    CsaRequest {
        asof_ordinal: day(1),
        measure: CashflowMeasure::Pricing,
        conditional_model: id("scenario-model"),
        crn: id("shared-crn"),
        policy: Policy {
            identity: id("csa"),
            entity: "bank".into(),
            counterparty: "cp".into(),
            currency: "USD".into(),
            monetary_unit: "currency_units".into(),
            receive_threshold: 0.,
            post_threshold: 0.,
            minimum_transfer: 0.,
            settlement_lag_calendar_days: 0,
            opening_vm: 0.,
            opening_received_im: 0.,
            opening_posted_im: 0.,
            opening_source: id("opening"),
            received_vm_reusable_for_funding: false,
            received_im_segregated: true,
            collateral_interest_day_count: ActualDayCount::Act360,
            tail_return_ordinal: day(12),
            initial_margin_model: None,
        },
        mapping: vec![Mapping {
            position_id: "swap".into(),
            contract: id("swap-contract"),
            entity: "bank".into(),
            counterparty: "cp".into(),
            currency: "USD".into(),
            netting_set_id: "ns".into(),
            legal_review: id("legal"),
            enforceable_netting: true,
        }],
        paths: vec![Path {
            path_id: "p".into(),
            source: id("path"),
            probability: 1.,
            nodes: vec![node(1, true), node(3, true), node(11, true), node(12, true)],
            default: None,
        }],
        own_credit: None,
    }
}
#[test]
fn regular_swap_hand_cash_par_receiver_and_negative_rates() {
    let mut r = swap(day(1), day(1), day(11), 0.1, Side::Payer, "p");
    r.discount_curve.points[1].discount_factor = 0.95;
    let result = value_swap(&r).unwrap();
    close(result.fixed_leg_pv, 0.475);
    close(result.floating_leg_pv, 0.95);
    close(result.clean_value, 0.475);
    close(result.par_rate.unwrap(), 0.1);
    close(
        result
            .schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        0.,
    );
    r.fixed_rate = 0.1;
    close(value_swap(&r).unwrap().clean_value, 0.);
    r.fixed_rate = 0.05;
    r.side = Side::Receiver;
    close(value_swap(&r).unwrap().clean_value, -0.475);
    if let Coupon::Indexed { observations, .. } = &mut r.floating_periods[0].coupon {
        observations[0].rate = -0.02;
    }
    close(value_swap(&r).unwrap().floating_leg_pv, -0.19);
}
#[test]
fn irregular_daycount_payment_timing_and_future_information() {
    let mut r = swap(day(11), day(1), day(11), 0.1, Side::Payer, "p");
    close(value_swap(&r).unwrap().clean_value, 0.);
    r.payment_timing = PaymentTiming::BeforeScheduledPayments;
    close(value_swap(&r).unwrap().clean_value, 0.5);
    let mut r = swap(day(1), day(1), day(11), 0.1, Side::Payer, "p");
    if let Coupon::Indexed { observations, .. } = &mut r.floating_periods[0].coupon {
        if let RateSource::ConditionalProjection {
            information_unix_seconds,
            ..
        } = &mut observations[0].source
        {
            *information_unix_seconds = utc(day(2));
        }
    }
    assert!(value_swap(&r).unwrap_err().contains("future conditional"));
}
#[test]
fn european_two_state_hand_value_delayed_cash_and_physical_no_duplicate() {
    let mut r = option();
    let out = value_european(&r).unwrap();
    close(out.clean_value, 0.2475);
    close(out.states[0].intrinsic_value, 0.5);
    close(out.states[1].intrinsic_value, 0.);
    r.settlement_ordinal = day(4);
    for s in &mut r.states {
        s.underlying.discount_curve.points.insert(
            1,
            DiscountPoint {
                ordinal: day(4),
                discount_factor: 0.98,
            },
        );
    }
    let delayed = value_european(&r).unwrap();
    close(delayed.clean_value, 0.2475);
    close(delayed.states[0].settlement_cash, 0.5 / 0.98);
    r.settlement_ordinal = r.expiry_ordinal;
    r.settlement = Settlement::PhysicalDelivery;
    let physical = value_european(&r).unwrap();
    close(physical.clean_value, 0.2475);
    assert!(physical
        .schedule
        .events
        .iter()
        .all(|e| e.position_id == "delivered-swap" && e.payment_ordinal == day(13)));
    close(
        physical
            .schedule
            .events
            .iter()
            .map(|e| e.components.cash_total().unwrap())
            .sum(),
        0.25,
    );
    assert!(physical.states.iter().all(|s| s.settlement_cash == 0.));
    r.side = OptionSide::Short;
    close(value_european(&r).unwrap().clean_value, -0.2475);
}
#[test]
fn option_admission_no_contract_change_no_foresight_no_measure_relabel() {
    let mut r = option();
    r.states[1].underlying.notional = 361.;
    assert!(value_european(&r).is_err());
    let mut r = option();
    if let Coupon::Indexed { observations, .. } =
        &mut r.states[0].underlying.floating_periods[0].coupon
    {
        if let RateSource::ConditionalProjection {
            information_unix_seconds,
            ..
        } = &mut observations[0].source
        {
            *information_unix_seconds = utc(day(4));
        }
    }
    assert!(value_european(&r).is_err());
    let mut r = option();
    r.measure = CashflowMeasure::Physical;
    assert!(value_european(&r).is_err());
}
#[test]
fn vm_zero_lag_conservation_and_clean_separation() {
    let r = csa();
    let result = evaluate(&r).unwrap();
    close(result.clean_value, 0.5);
    close(result.cva.unwrap(), 0.);
    close(result.fva.unwrap(), 0.);
    let p = &result.paths[0];
    close(p.movements.iter().map(|m| m.entity_cash).sum(), 0.);
    close(p.movements.iter().map(|m| m.vm_delta).sum(), 0.);
    close(p.residual_vm, 0.);
    assert_eq!(p.calls[0].settled_ordinal, Some(day(1)));
}
#[test]
fn lag_outage_no_duplicate_pending_calls() {
    let mut r = csa();
    r.policy.settlement_lag_calendar_days = 2;
    r.policy.tail_return_ordinal = day(14);
    r.paths[0].nodes = vec![
        node(1, true),
        node(3, false),
        node(4, true),
        node(11, true),
        node(14, true),
    ];
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert_eq!(p.calls[0].due_ordinal, day(3));
    assert_eq!(p.calls[0].settled_ordinal, Some(day(4)));
    assert_eq!(p.calls.len(), 2);
    close(p.movements.iter().map(|m| m.entity_cash).sum(), 0.);
}
#[test]
fn default_received_posted_segregated_waterfall_hand_conservation() {
    let mut r = csa();
    r.policy.opening_vm = 0.2;
    r.policy.opening_received_im = 0.1;
    r.policy.opening_posted_im = 0.2;
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(1),
        closeout_ordinal: day(3),
        legal_review: id("default-legal"),
        replacement_value_policy: id("replacement-value"),
        counterparty_recovery: 0.5,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 0.25,
        posted_collateral_setoff: true,
        received_im_available_on_default: true,
        mpor: None,
    });
    let out = evaluate(&r).unwrap();
    let c = &out.paths[0].closeouts[0];
    close(c.vm_applied, 0.2);
    close(c.received_im_applied, 0.1);
    close(c.unsecured_claim, 0.2);
    close(c.counterparty_loss, 0.1);
    close(c.posted_return_loss, 0.15);
    close(c.entity_cash, 0.25);
    close(out.cva.unwrap(), 0.25);
    close(
        r.policy.opening_vm - r.policy.opening_posted_im + c.entity_cash,
        out.clean_value - c.counterparty_loss - c.posted_return_loss,
    );
    close(
        out.paths[0]
            .movements
            .iter()
            .map(|m| m.received_im_delta)
            .sum::<f64>()
            + 0.1,
        0.,
    );
}
#[test]
fn posted_collateral_setoff_negative_closeout_and_physical_loss_not_cva() {
    let mut r = csa();
    for n in &mut r.paths[0].nodes {
        if let Trade::Swap { request } = &mut n.trades[0] {
            request.side = Side::Receiver;
        }
    }
    r.measure = CashflowMeasure::Physical;
    r.policy.opening_vm = -0.2;
    r.policy.opening_posted_im = 0.1;
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(1),
        closeout_ordinal: day(3),
        legal_review: id("legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.5,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: None,
    });
    let out = evaluate(&r).unwrap();
    assert!(out.cva.is_none());
    let c = &out.paths[0].closeouts[0];
    close(c.entity_cash, -0.2);
    close(-0.2 - 0.1 + c.entity_cash, -0.5);
    close(out.paths[0].residual_posted_im, 0.);
}
#[test]
fn collateral_negative_interest_and_supplied_funding_cost() {
    let mut r = csa();
    r.policy.opening_vm = -0.2;
    r.policy.opening_posted_im = 0.1;
    r.policy.receive_threshold = 10.;
    r.policy.post_threshold = 10.;
    r.policy.minimum_transfer = 1.;
    for n in &mut r.paths[0].nodes {
        n.collateral_rate = -0.36;
        n.funding_spread = 0.72;
    }
    let out = evaluate(&r).unwrap();
    close(
        out.paths[0].discounted_funding_cost,
        0.3 * 0.72 * 11. / 360.,
    );
    let interest = out.paths[0]
        .movements
        .iter()
        .filter(|m| m.kind == "collateral_interest")
        .map(|m| m.entity_cash)
        .sum::<f64>();
    close(interest, -0.3 * 0.36 * 11. / 360.);
    close(out.fva.unwrap(), out.paths[0].discounted_funding_cost);
}
#[test]
fn legal_no_netting_gross_margin_and_input_domain() {
    let mut r = csa();
    let mut other = r.mapping[0].clone();
    other.position_id = "other".into();
    r.mapping[0].enforceable_netting = false;
    other.enforceable_netting = false;
    r.mapping.push(other);
    for n in &mut r.paths[0].nodes {
        let mut second = match &n.trades[0] {
            Trade::Swap { request } => (**request).clone(),
            _ => unreachable!(),
        };
        second.position_id = "other".into();
        second.side = Side::Receiver;
        n.trades.push(Trade::Swap {
            request: Box::new(second),
        });
    }
    let out = evaluate(&r).unwrap();
    close(out.clean_value, 0.);
    let margin = &out.paths[0].movements;
    close(
        margin
            .iter()
            .filter(|m| m.ordinal == day(1) && m.kind == "variation_margin")
            .map(|m| m.entity_cash.abs())
            .sum(),
        1.,
    );
    r.mapping[1].currency = "EUR".into();
    assert!(evaluate(&r).is_err());
}
#[test]
fn reject_missing_curve_tail_and_interim_unpaid_payment() {
    let mut r = swap(day(1), day(1), day(11), 0.1, Side::Payer, "p");
    r.discount_curve.points.pop();
    assert!(value_swap(&r).is_err());
    let mut r = csa();
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(3),
        closeout_ordinal: day(12),
        legal_review: id("legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: None,
    });
    assert!(evaluate(&r).unwrap_err().contains("unpaid"));
    r.paths[0].default = None;
    r.paths[0].nodes.last_mut().unwrap().settlement_available = false;
    assert!(evaluate(&r).is_err());
}
#[test]
fn european_physical_handoff_native_exposure_no_duplicate_cash() {
    let mut r = csa();
    r.policy.tail_return_ordinal = day(14);
    r.mapping[0].position_id = "option".into();
    r.mapping[0].contract = id("option-contract");
    let mut live = option();
    live.scenario_id = "p".into();
    live.settlement = Settlement::PhysicalDelivery;
    for state in &mut live.states {
        state.underlying.scenario_id = "p".into();
    }
    let mut terminal = live.clone();
    terminal.asof_ordinal = day(3);
    terminal.states.truncate(1);
    terminal.states[0].probability = 1.;
    terminal.states[0].discount_to_expiry = 1.;
    r.paths[0].nodes = vec![node(1, true), node(3, true), node(13, true), node(14, true)];
    r.paths[0].nodes[0].trades = vec![Trade::EuropeanSwaption {
        request: Box::new(live),
    }];
    for node in &mut r.paths[0].nodes[1..] {
        let mut delivered = swap(node.ordinal, day(3), day(13), 0.1, Side::Payer, "p");
        delivered.position_id = "delivered-swap".into();
        node.trades = vec![Trade::ExpiredEuropean {
            terminal: Box::new(terminal.clone()),
            election: id("expiry-election"),
            delivered_underlying: Some(Box::new(delivered)),
            settlement_discount: None,
        }];
    }
    let out = evaluate(&r).unwrap();
    close(out.clean_value, 0.2475);
    close(out.paths[0].exposures[1].clean_value, 0.5);
    close(out.paths[0].exposures[2].clean_value, 0.);
    close(
        out.paths[0].movements.iter().map(|m| m.entity_cash).sum(),
        0.,
    );
    if let Trade::ExpiredEuropean {
        delivered_underlying: Some(delivered),
        ..
    } = &mut r.paths[0].nodes[1].trades[0]
    {
        delivered.fixed_rate = 0.06;
    }
    assert!(evaluate(&r).unwrap_err().contains("changed underlying"));
}
#[test]
fn cash_option_handoff_before_after_settlement_and_immutable_election() {
    let mut r = csa();
    r.policy.tail_return_ordinal = day(14);
    r.mapping[0].position_id = "option".into();
    r.mapping[0].contract = id("option-contract");
    let mut live = option();
    live.scenario_id = "p".into();
    live.settlement_ordinal = day(4);
    for state in &mut live.states {
        state.underlying.scenario_id = "p".into();
        state.underlying.discount_curve.points.insert(
            1,
            DiscountPoint {
                ordinal: day(4),
                discount_factor: 0.98,
            },
        );
    }
    let mut terminal = live.clone();
    terminal.asof_ordinal = day(3);
    terminal.states.truncate(1);
    terminal.states[0].probability = 1.;
    terminal.states[0].discount_to_expiry = 1.;
    r.paths[0].nodes = vec![node(1, true), node(3, true), node(4, true), node(14, true)];
    r.paths[0].nodes[0].trades = vec![Trade::EuropeanSwaption {
        request: Box::new(live),
    }];
    for n in &mut r.paths[0].nodes[1..] {
        n.trades = vec![Trade::ExpiredEuropean {
            terminal: Box::new(terminal.clone()),
            election: id("election"),
            delivered_underlying: None,
            settlement_discount: if n.ordinal == day(3) {
                Some(Box::new(DiscountCurve {
                    source: id("settlement-discount"),
                    collateral_discount_policy: id("discount-policy"),
                    currency: "USD".into(),
                    asof_ordinal: day(3),
                    information_unix_seconds: utc(day(3)),
                    basis: "collateralized_clean".into(),
                    points: vec![
                        DiscountPoint {
                            ordinal: day(3),
                            discount_factor: 1.,
                        },
                        DiscountPoint {
                            ordinal: day(4),
                            discount_factor: 0.98,
                        },
                    ],
                }))
            } else {
                None
            },
        }];
    }
    let out = evaluate(&r).unwrap();
    close(out.paths[0].exposures[1].clean_value, 0.5);
    close(out.paths[0].exposures[2].clean_value, 0.);
    if let Trade::ExpiredEuropean { terminal, .. } = &mut r.paths[0].nodes[2].trades[0] {
        terminal.states[0].underlying.fixed_rate = 0.09;
    }
    assert!(evaluate(&r).is_err());
}
#[test]
fn closeout_rights_extinguished_and_zero_contract() {
    let mut r = csa();
    for n in &mut r.paths[0].nodes {
        if let Trade::Swap { request } = &mut n.trades[0] {
            request.notional = 0.;
        }
    }
    let out = evaluate(&r).unwrap();
    close(out.clean_value, 0.);
    assert!(out.paths[0].calls.is_empty());
    let mut r = csa();
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(1),
        closeout_ordinal: day(3),
        legal_review: id("legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.5,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: None,
    });
    let out = evaluate(&r).unwrap();
    assert!(out.paths[0]
        .exposures
        .iter()
        .filter(|e| e.ordinal > day(3))
        .all(|e| e.extinguished_after_closeout
            && e.clean_value == 0.
            && e.unsecured_receivable == 0.));
}
#[test]
fn unequal_fixed_float_frequencies_hand_weighted_pv_and_intraday_curve_rejection() {
    let mut r = swap(day(1), day(1), day(21), 0.1, Side::Payer, "p");
    let other = swap(day(1), day(6), day(21), 0.2, Side::Payer, "p");
    r.floating_periods[0] = swap(day(1), day(1), day(6), 0.1, Side::Payer, "p")
        .floating_periods
        .remove(0);
    r.floating_periods[0].period_id = "float-first".into();
    r.floating_periods.push(other.floating_periods[0].clone());
    r.discount_curve.points = vec![
        DiscountPoint {
            ordinal: day(1),
            discount_factor: 1.,
        },
        DiscountPoint {
            ordinal: day(6),
            discount_factor: 0.99,
        },
        DiscountPoint {
            ordinal: day(21),
            discount_factor: 0.96,
        },
    ];
    let result = value_swap(&r).unwrap();
    close(result.fixed_leg_pv, 0.96);
    close(result.floating_leg_pv, 0.495 + 2.88);
    close(result.clean_value, 2.415);
    close(result.par_rate.unwrap(), 3.375 / 19.2);
    r.discount_curve.information_unix_seconds += 1;
    assert!(value_swap(&r)
        .unwrap_err()
        .contains("exact valuation cutoff"));
}
#[test]
fn json_examples_public_dispatch_and_model_version_admission() {
    use portfolio_risk_native::derivative_contract::{
        evaluate as evaluate_derivative, DerivativeRequest, DerivativeResult,
    };
    for (source, expected) in [
        (
            include_str!("../../../examples/models/derivative-contract-1.json"),
            0.5,
        ),
        (
            include_str!("../../../examples/models/derivative-option-1.json"),
            0.2475,
        ),
        (
            include_str!("../../../examples/models/derivative-netting-1.json"),
            0.5,
        ),
    ] {
        let envelope: serde_json::Value = serde_json::from_str(source).unwrap();
        let mut request: DerivativeRequest =
            serde_json::from_value(envelope["request"].clone()).unwrap();
        let value = match evaluate_derivative(&request).unwrap() {
            DerivativeResult::Swap(r) => r.clean_value,
            DerivativeResult::EuropeanSwaption(r) => r.clean_value,
            DerivativeResult::Netting(r) => r.clean_value,
        };
        close(value, expected);
        request.model_version = "older".into();
        assert!(evaluate_derivative(&request).is_err());
    }
}
fn mpor_policy(measure: CashflowMeasure, position: &str, date: i32) -> MporPolicy {
    MporPolicy {
        identity: id("reviewed-mpor"),
        claim_measure: measure,
        carry_policy: UnpaidCarry::NominalNoInterest,
        exercise_rights_policy: ExerciseRightsPolicy::PreserveContractualEuropeanToCloseout,
        physical_index_paths: Vec::new(),
        settlement_instructions: vec![
            ObligationInstruction {
                position_id: position.into(),
                event_id: "fixed:f1".into(),
                payment_ordinal: date,
                disposition: ObligationDisposition::RetainToCloseout,
            },
            ObligationInstruction {
                position_id: position.into(),
                event_id: "floating:l1".into(),
                payment_ordinal: date,
                disposition: ObligationDisposition::RetainToCloseout,
            },
        ],
    }
}
fn mpor_swap() -> CsaRequest {
    let mut r = csa();
    r.policy.receive_threshold = 10.;
    r.policy.post_threshold = 10.;
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(3),
        closeout_ordinal: day(12),
        legal_review: id("mpor-legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.4,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: Some(mpor_policy(CashflowMeasure::Pricing, "swap", day(11))),
    });
    r
}
#[test]
fn mpor_native_signed_coupons_survive_maturity_and_closeout_once() {
    let r = mpor_swap();
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    let c = &p.closeouts[0];
    close(c.clean_replacement_value, 0.);
    close(c.unpaid_obligation_claim, 0.5);
    close(c.total_closeout_claim, 0.5);
    close(c.entity_cash, 0.2);
    close(c.counterparty_loss, 0.3);
    close(out.cva.unwrap(), 0.3);
    assert_eq!(p.obligations.len(), 2);
    close(p.obligations[0].amount, -0.5);
    close(p.obligations[1].amount, 1.);
    assert!(p
        .obligations
        .iter()
        .all(|o| o.included_in_closeout && o.settled_ordinal.is_none()));
    let due = p.exposures.iter().find(|e| e.ordinal == day(11)).unwrap();
    close(due.clean_value, 0.);
    close(due.total_closeout_claim, 0.5);
    close(
        p.movements
            .iter()
            .filter(|m| m.kind == "mpor_contractual_settlement")
            .map(|m| m.entity_cash)
            .sum(),
        0.,
    );
}
#[test]
fn mpor_settled_coupon_removed_from_unpaid_and_not_paid_twice() {
    let mut r = mpor_swap();
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .settlement_instructions[0]
        .disposition = ObligationDisposition::SettleOnNode {
        ordinal: day(12),
        evidence: id("actual-delayed-coupon-payment"),
    };
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    let c = &p.closeouts[0];
    close(c.unpaid_obligation_claim, 1.);
    close(c.counterparty_loss, 0.6);
    close(c.entity_cash, 0.4);
    close(
        p.movements
            .iter()
            .filter(|m| m.kind == "mpor_contractual_settlement")
            .map(|m| m.entity_cash)
            .sum(),
        -0.5,
    );
    assert_eq!(p.obligations[0].settled_ordinal, Some(day(12)));
    assert!(!p.obligations[0].included_in_closeout);
    close(-0.5 + c.entity_cash, 0.5 - c.counterparty_loss);
}
#[test]
fn mpor_same_day_before_clean_partition_and_default_after_payment() {
    let mut r = mpor_swap();
    r.paths[0].default.as_mut().unwrap().closeout_ordinal = day(11);
    r.paths[0].nodes[2].payment_timing = PaymentTiming::BeforeScheduledPayments;
    if let Trade::Swap { request } = &mut r.paths[0].nodes[2].trades[0] {
        request.payment_timing = PaymentTiming::BeforeScheduledPayments;
    }
    let out = evaluate(&r).unwrap();
    let due = &out.paths[0].exposures[2];
    close(due.clean_value, 0.5);
    close(due.replacement_after_due_obligations, 0.);
    close(due.unpaid_obligation_claim, 0.5);
    close(due.total_closeout_claim, 0.5);
    close(out.paths[0].closeouts[0].entity_cash, 0.2);
    let mut r = mpor_swap();
    let d = r.paths[0].default.as_mut().unwrap();
    d.default_ordinal = day(11);
    d.mpor.as_mut().unwrap().settlement_instructions.clear();
    let out = evaluate(&r).unwrap();
    assert!(out.paths[0].obligations.is_empty());
    close(out.paths[0].closeouts[0].total_closeout_claim, 0.);
}
#[test]
fn mpor_physical_fixings_not_pricing_projection_and_signed_cash() {
    let mut r = mpor_swap();
    r.measure = CashflowMeasure::Physical;
    let policy = r.paths[0].default.as_mut().unwrap().mpor.as_mut().unwrap();
    policy.claim_measure = CashflowMeasure::Physical;
    let mut observations = match &r.paths[0].nodes[2].trades[0] {
        Trade::Swap { request } => match &request.floating_periods[0].coupon {
            Coupon::Indexed { observations, .. } => observations.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    observations[0].rate = 0.08;
    observations[0].source = RateSource::Observed {
        fixing: id("observed-contract-reset"),
        publication_unix_seconds: utc(day(1)) - 3600,
    };
    assert!(evaluate(&r).unwrap_err().contains("Physical MPOR cash"));
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .physical_index_paths = vec![PhysicalIndexPath {
        position_id: "swap".into(),
        period_id: "l1".into(),
        path: id("observed-physical-path"),
        observations,
    }];
    let out = evaluate(&r).unwrap();
    assert!(out.cva.is_none());
    let c = &out.paths[0].closeouts[0];
    close(c.unpaid_obligation_claim, 0.3);
    close(c.entity_cash, 0.12);
    close(c.counterparty_loss, 0.18);
    assert!(matches!(
        out.paths[0].obligations[1].selected_rate_sources[0],
        RateSource::Observed { .. }
    ));
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .physical_index_paths[0]
        .observations[0]
        .rate = -0.02;
    let out = evaluate(&r).unwrap();
    close(out.paths[0].closeouts[0].total_closeout_claim, -0.7);
    close(out.paths[0].closeouts[0].entity_cash, -0.7);
    close(out.paths[0].closeouts[0].counterparty_loss, 0.);
}
#[test]
fn mpor_complete_event_coverage_no_amount_override_or_hidden_date() {
    let mut r = mpor_swap();
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .settlement_instructions
        .pop();
    assert!(evaluate(&r).unwrap_err().contains("missing native-derived"));
    let mut r = mpor_swap();
    r.paths[0].nodes.remove(2);
    assert!(evaluate(&r).unwrap_err().contains("due-date node"));
    let mut r = mpor_swap();
    let policy = r.paths[0].default.as_mut().unwrap().mpor.as_mut().unwrap();
    policy
        .settlement_instructions
        .push(policy.settlement_instructions[0].clone());
    assert!(evaluate(&r).unwrap_err().contains("duplicate MPOR"));
    let payload = serde_json::json!({"position_id":"swap","event_id":"fixed:f1","payment_ordinal":day(11),"amount":123.,"disposition":{"kind":"retain_to_closeout"}});
    assert!(serde_json::from_value::<ObligationInstruction>(payload).is_err());
}

fn mpor_option(physical: bool) -> CsaRequest {
    let mut r = csa();
    r.policy.receive_threshold = 10.;
    r.policy.post_threshold = 10.;
    r.policy.tail_return_ordinal = day(14);
    r.mapping[0].position_id = "option".into();
    r.mapping[0].contract = id("option-contract");
    let mut live = option();
    live.scenario_id = "p".into();
    if physical {
        live.settlement = Settlement::PhysicalDelivery;
    } else {
        live.settlement_ordinal = day(4);
    }
    for state in &mut live.states {
        state.underlying.scenario_id = "p".into();
        if !physical {
            state.underlying.discount_curve.points.insert(
                1,
                DiscountPoint {
                    ordinal: day(4),
                    discount_factor: 0.98,
                },
            );
        }
    }
    let mut terminal = live.clone();
    terminal.asof_ordinal = day(3);
    terminal.states.truncate(1);
    terminal.states[0].probability = 1.;
    terminal.states[0].discount_to_expiry = 1.;
    let dates = if physical {
        vec![1, 3, 13, 14]
    } else {
        vec![1, 3, 4, 14]
    };
    r.paths[0].nodes = dates.into_iter().map(|d| node(d, true)).collect();
    r.paths[0].nodes[0].trades = vec![Trade::EuropeanSwaption {
        request: Box::new(live),
    }];
    for n in &mut r.paths[0].nodes[1..] {
        let mut delivered = swap(n.ordinal, day(3), day(13), 0.1, Side::Payer, "p");
        delivered.position_id = "delivered-swap".into();
        let curve = if !physical && n.ordinal < day(4) {
            Some(Box::new(DiscountCurve {
                source: id("expiry-settlement-curve"),
                collateral_discount_policy: id("discount-policy"),
                currency: "USD".into(),
                asof_ordinal: n.ordinal,
                information_unix_seconds: n.valuation_cutoff_unix_seconds,
                basis: "collateralized_clean".into(),
                points: vec![
                    DiscountPoint {
                        ordinal: n.ordinal,
                        discount_factor: 1.,
                    },
                    DiscountPoint {
                        ordinal: day(4),
                        discount_factor: 0.98,
                    },
                ],
            }))
        } else {
            None
        };
        n.trades = vec![Trade::ExpiredEuropean {
            terminal: Box::new(terminal.clone()),
            election: id("reviewed-expiry-election"),
            delivered_underlying: if physical {
                Some(Box::new(delivered))
            } else {
                None
            },
            settlement_discount: curve,
        }];
    }
    let policy = if physical {
        mpor_policy(CashflowMeasure::Pricing, "option", day(13))
    } else {
        let mut p = mpor_policy(CashflowMeasure::Pricing, "option", day(4));
        p.settlement_instructions = vec![ObligationInstruction {
            position_id: "option".into(),
            event_id: "option_cash:up".into(),
            payment_ordinal: day(4),
            disposition: ObligationDisposition::RetainToCloseout,
        }];
        p
    };
    r.paths[0].default = Some(DefaultPolicy {
        default_ordinal: day(1),
        closeout_ordinal: day(14),
        legal_review: id("mpor-legal"),
        replacement_value_policy: id("replacement"),
        counterparty_recovery: 0.4,
        posted_vm_return_recovery: 1.,
        posted_im_return_recovery: 1.,
        posted_collateral_setoff: true,
        received_im_available_on_default: false,
        mpor: Some(policy),
    });
    r
}
#[test]
fn mpor_european_physical_exercise_then_unpaid_coupon_preserves_rights_once() {
    let r = mpor_option(true);
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert_eq!(p.mpor_exercises.len(), 1);
    assert!(p.mpor_exercises[0].exercised);
    assert_eq!(
        p.mpor_exercises[0].delivered_position_id.as_deref(),
        Some("delivered-swap")
    );
    assert_eq!(p.obligations.len(), 2);
    assert!(p.obligations.iter().all(|o| o.position_id == "option"
        && o.delivered_position_id.as_deref() == Some("delivered-swap")
        && o.origin_election.is_some()));
    close(p.closeouts[0].unpaid_obligation_claim, 0.5);
    close(p.closeouts[0].clean_replacement_value, 0.);
    close(p.closeouts[0].entity_cash, 0.2);
    close(p.closeouts[0].counterparty_loss, 0.3);
    assert!(!p
        .obligations
        .iter()
        .any(|o| o.event_id.starts_with("option_cash")));
    assert!(!p
        .movements
        .iter()
        .any(|m| m.kind == "mpor_contractual_settlement"));
}
#[test]
fn mpor_european_delayed_cash_settlement_captured_without_duplicate_delivery() {
    let r = mpor_option(false);
    let out = evaluate(&r).unwrap();
    let p = &out.paths[0];
    assert_eq!(p.mpor_exercises.len(), 1);
    assert_eq!(p.obligations.len(), 1);
    close(p.obligations[0].amount, 0.5 / 0.98);
    close(p.closeouts[0].unpaid_obligation_claim, 0.5 / 0.98);
    close(p.closeouts[0].entity_cash, 0.4 * 0.5 / 0.98);
    assert!(p.obligations[0].delivered_position_id.is_none());
    assert!(!p.obligations[0].settlement_price_sources.is_empty());
}
#[test]
fn mpor_settlement_availability_measure_and_fixing_admission_fail_closed() {
    let mut r = mpor_swap();
    r.paths[0].nodes[2].settlement_available = false;
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .settlement_instructions[0]
        .disposition = ObligationDisposition::SettleOnNode {
        ordinal: day(11),
        evidence: id("settlement-evidence"),
    };
    assert!(evaluate(&r).unwrap_err().contains("available admitted"));
    let mut r = mpor_swap();
    r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .claim_measure = CashflowMeasure::Physical;
    assert!(evaluate(&r).unwrap_err().contains("claim measure"));
    let mut r = mpor_swap();
    let instruction = &mut r.paths[0]
        .default
        .as_mut()
        .unwrap()
        .mpor
        .as_mut()
        .unwrap()
        .settlement_instructions[0];
    instruction.disposition = ObligationDisposition::SettleOnNode {
        ordinal: day(13),
        evidence: id("too-late"),
    };
    assert!(evaluate(&r).is_err());
    let mut r = mpor_swap();
    r.measure = CashflowMeasure::Physical;
    let observations = match &r.paths[0].nodes[2].trades[0] {
        Trade::Swap { request } => match &request.floating_periods[0].coupon {
            Coupon::Indexed { observations, .. } => observations.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let p = r.paths[0].default.as_mut().unwrap().mpor.as_mut().unwrap();
    p.claim_measure = CashflowMeasure::Physical;
    p.physical_index_paths = vec![PhysicalIndexPath {
        position_id: "swap".into(),
        period_id: "l1".into(),
        path: id("physical-observed"),
        observations,
    }];
    p.physical_index_paths[0].observations[0].source = RateSource::Observed {
        fixing: id("late-fixing"),
        publication_unix_seconds: utc(day(12)),
    };
    assert!(evaluate(&r).is_err());
}
#[test]
fn reviewed_mpor_requires_explicit_v2_selection_and_preserves_v1_saved_inputs() {
    use portfolio_risk_native::derivative_contract::{
        evaluate as dispatch, DerivativeRequest, DerivativeResult, Operation, MODEL_VERSION,
        MPOR_MODEL_VERSION, SCHEMA,
    };
    let mut request = DerivativeRequest {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        operation: Operation::Netting {
            request: Box::new(mpor_swap()),
        },
    };
    assert!(dispatch(&request)
        .unwrap_err()
        .contains("explicit derivative-contracts-2"));
    request.model_version = MPOR_MODEL_VERSION.into();
    let DerivativeResult::Netting(out) = dispatch(&request).unwrap() else {
        panic!("netting result")
    };
    close(out.paths[0].closeouts[0].unpaid_obligation_claim, 0.5);
    let old = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../examples/models/derivative-netting-1.json"
    ))
    .unwrap();
    let saved: DerivativeRequest = serde_json::from_value(old["request"].clone()).unwrap();
    assert_eq!(saved.model_version, MODEL_VERSION);
    assert!(dispatch(&saved).is_ok());
}
#[test]
fn mpor_v2_json_envelope_native_dispatch_hand_result() {
    use portfolio_risk_native::derivative_contract::{
        evaluate as dispatch, DerivativeRequest, DerivativeResult, MPOR_MODEL_VERSION,
    };
    let envelope: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/derivative-mpor-2.json"
    ))
    .unwrap();
    let request: DerivativeRequest = serde_json::from_value(envelope["request"].clone()).unwrap();
    assert_eq!(request.model_version, MPOR_MODEL_VERSION);
    let DerivativeResult::Netting(result) = dispatch(&request).unwrap() else {
        panic!("netting result")
    };
    close(result.clean_value, 0.5);
    close(result.cva.unwrap(), 0.3);
    close(result.paths[0].closeouts[0].unpaid_obligation_claim, 0.5);
    close(result.paths[0].closeouts[0].entity_cash, 0.2);
}

#[test]
fn option_scope_is_european_only_and_bermudan_or_american_requests_fail() {
    use portfolio_risk_native::derivative_contract::{
        evaluate as evaluate_derivative, DerivativeRequest,
    };
    // Issue #20 admitted option scope: one European expiry. Bermudan and American
    // structures have no schema; any attempt to express them fails before valuation.
    let envelope: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/models/derivative-option-1.json"
    ))
    .unwrap();
    let base = envelope["request"].clone();
    let decode = |v: serde_json::Value| serde_json::from_value::<DerivativeRequest>(v);
    let request = decode(base.clone()).unwrap();
    assert!(evaluate_derivative(&request).is_ok());
    for kind in ["bermudan_swaption", "american_swaption", "callable_swap"] {
        let mut v = base.clone();
        v["operation"]["kind"] = kind.into();
        assert!(decode(v).is_err(), "{kind} must not be admitted");
    }
    let expiry = base["operation"]["request"]["expiry_ordinal"]
        .as_i64()
        .unwrap();
    for (field, value) in [
        (
            "exercise_ordinals",
            serde_json::json!([expiry - 30, expiry]),
        ),
        ("exercise_style", serde_json::json!("bermudan")),
        ("exercise_style", serde_json::json!("american")),
        (
            "exercise_window_end_ordinal",
            serde_json::json!(expiry + 30),
        ),
    ] {
        let mut v = base.clone();
        v["operation"]["request"][field] = value;
        assert!(decode(v).is_err(), "{field} must not be admitted");
    }
}

fn own_credit(points: &[(u32, f64)], recovery: f64) -> Option<OwnCredit> {
    Some(OwnCredit {
        identity: id("own-credit"),
        recovery,
        survival: points
            .iter()
            .map(|&(d, s)| SurvivalPoint {
                ordinal: day(d),
                survival_probability: s,
            })
            .collect(),
    })
}

#[test]
fn own_credit_dva_memo_is_opt_in_and_reconciles_to_uncollateralized_payables() {
    let mut r = csa();
    // Uncollateralized receiver swap: the entity owes the counterparty, so DVA is positive.
    r.policy.post_threshold = 1e9;
    r.policy.receive_threshold = 1e9;
    for n in &mut r.paths[0].nodes {
        let Trade::Swap { request } = &mut n.trades[0] else {
            unreachable!()
        };
        **request = swap(n.ordinal, day(1), day(11), 0.1, Side::Receiver, "p");
    }
    let plain = evaluate(&r).unwrap();
    let bytes = serde_json::to_value(&plain).unwrap();
    for key in ["dva", "own_credit_policy", "clean_less_cva_fva_plus_dva"] {
        assert!(
            bytes.get(key).is_none(),
            "{key} must be absent without own credit"
        );
    }
    assert!(bytes["paths"][0]
        .get("discounted_own_default_benefit")
        .is_none());
    assert!(!plain.disclosure.contains("Own-credit DVA memo"));

    r.own_credit = own_credit(&[(1, 1.), (3, 0.99), (11, 0.95), (12, 0.94)], 0.4);
    let with = evaluate(&r).unwrap();
    // Economic clean value, CVA and FVA are untouched by the memo.
    close(with.clean_value, plain.clean_value);
    close(with.cva.unwrap(), plain.cva.unwrap());
    close(with.fva.unwrap(), plain.fva.unwrap());
    let survival = [1., 0.99, 0.95, 0.94];
    let exposures = &with.paths[0].exposures;
    assert_eq!(exposures.len(), 4);
    let expected: f64 = (1..4)
        .map(|k| {
            let e = &exposures[k];
            let payable = (-e.total_closeout_claim - (-e.vm).max(0.) - e.posted_im).max(0.);
            (survival[k - 1] - survival[k]) * 0.6 * payable
        })
        .sum();
    assert!(expected > 0.);
    close(with.dva.unwrap(), expected);
    close(
        with.paths[0].discounted_own_default_benefit.unwrap(),
        expected,
    );
    close(
        with.clean_less_cva_fva_plus_dva.unwrap(),
        with.clean_value - with.cva.unwrap() - with.fva.unwrap() + expected,
    );
    assert!(with.disclosure.starts_with("Own-credit DVA memo"));

    // A payer swap that the entity never owes has zero DVA.
    let mut payer = csa();
    payer.policy.post_threshold = 1e9;
    payer.policy.receive_threshold = 1e9;
    payer.own_credit = own_credit(&[(1, 1.), (3, 0.99), (11, 0.95), (12, 0.94)], 0.4);
    let payer = evaluate(&payer).unwrap();
    assert!(payer.paths[0]
        .exposures
        .iter()
        .all(|e| e.total_closeout_claim >= 0.));
    close(payer.dva.unwrap(), 0.);
}

#[test]
fn own_credit_dva_refuses_physical_measure_gaps_and_increasing_survival() {
    let bad = [
        own_credit(&[(1, 1.), (3, 0.99), (12, 0.94)], 0.4),
        own_credit(&[(1, 1.), (3, 0.99), (11, 0.995), (12, 0.94)], 0.4),
        own_credit(&[(1, 0.99), (3, 0.98), (11, 0.95), (12, 0.94)], 0.4),
        own_credit(&[(1, 1.), (3, 0.99), (11, 0.95), (12, 0.94)], 1.2),
        own_credit(&[(3, 1.), (11, 0.95), (12, 0.94)], 0.4),
    ];
    for own in bad {
        let mut r = csa();
        r.own_credit = own;
        assert!(evaluate(&r).is_err());
    }
    let mut r = csa();
    r.measure = CashflowMeasure::Physical;
    r.own_credit = own_credit(&[(1, 1.), (3, 0.99), (11, 0.95), (12, 0.94)], 0.4);
    assert!(evaluate(&r).is_err());
}

fn im_model(minimum_transfer: f64) -> Option<InitialMarginModel> {
    Some(InitialMarginModel {
        identity: id("standardized-schedule"),
        schedule: vec![
            ScheduleBand {
                max_remaining_years: Some(2.),
                rate: 0.01,
            },
            ScheduleBand {
                max_remaining_years: Some(5.),
                rate: 0.02,
            },
            ScheduleBand {
                max_remaining_years: None,
                rate: 0.04,
            },
        ],
        minimum_transfer,
    })
}

#[test]
fn standardized_initial_margin_is_two_way_dynamic_and_returned_at_the_tail() {
    let plain = evaluate(&csa()).unwrap();
    assert!(plain.paths[0]
        .movements
        .iter()
        .all(|m| m.kind != "initial_margin"));
    let bytes = serde_json::to_value(csa()).unwrap();
    assert!(bytes["policy"].get("initial_margin_model").is_none());

    let mut r = csa();
    r.policy.initial_margin_model = im_model(0.);
    let result = evaluate(&r).unwrap();
    let p = &result.paths[0];
    let im: Vec<_> = p
        .movements
        .iter()
        .filter(|m| m.kind == "initial_margin")
        .collect();
    // The 360 swap has under two years left: gross = 360 * 1%. The entity is in the money, so
    // it receives (0.4 + 0.6 * 1) * gross and posts 0.4 * gross (its own NGR is zero).
    let first = im[0];
    assert_eq!(first.ordinal, day(1));
    close(first.received_im_delta, 3.6);
    close(first.posted_im_delta, 0.4 * 3.6);
    close(first.entity_cash, -0.4 * 3.6);
    close(first.custodian_cash, 3.6);
    let e = &p.exposures[0];
    close(e.received_im, 3.6);
    close(e.posted_im, 1.44);
    // Everything posted and received comes back at the tail.
    close(p.residual_received_im, 0.);
    close(p.residual_posted_im, 0.);
    let custody: f64 = p
        .movements
        .iter()
        .filter(|m| m.kind == "initial_margin" || m.kind == "tail_return")
        .map(|m| m.custodian_cash)
        .sum();
    close(custody, 0.);
    // Clean value is unchanged by margin.
    close(result.clean_value, plain.clean_value);

    // A minimum transfer above the requirement suppresses the call.
    let mut r = csa();
    r.policy.initial_margin_model = im_model(10.);
    let quiet = evaluate(&r).unwrap();
    assert!(quiet.paths[0]
        .movements
        .iter()
        .all(|m| m.kind != "initial_margin"));
}

#[test]
fn standardized_initial_margin_refuses_unreviewed_schedules_and_options() {
    for model in [
        Some(InitialMarginModel {
            identity: id("x"),
            schedule: vec![],
            minimum_transfer: 0.,
        }),
        Some(InitialMarginModel {
            identity: id("x"),
            schedule: vec![ScheduleBand {
                max_remaining_years: Some(2.),
                rate: 0.01,
            }],
            minimum_transfer: 0.,
        }),
        Some(InitialMarginModel {
            identity: id("x"),
            schedule: vec![
                ScheduleBand {
                    max_remaining_years: Some(5.),
                    rate: 0.01,
                },
                ScheduleBand {
                    max_remaining_years: Some(2.),
                    rate: 0.02,
                },
                ScheduleBand {
                    max_remaining_years: None,
                    rate: 0.04,
                },
            ],
            minimum_transfer: 0.,
        }),
        Some(InitialMarginModel {
            identity: id("x"),
            schedule: vec![ScheduleBand {
                max_remaining_years: None,
                rate: 1.5,
            }],
            minimum_transfer: 0.,
        }),
        Some(InitialMarginModel {
            identity: id("x"),
            schedule: vec![ScheduleBand {
                max_remaining_years: None,
                rate: 0.01,
            }],
            minimum_transfer: -1.,
        }),
    ] {
        let mut r = csa();
        r.policy.initial_margin_model = model;
        assert!(evaluate(&r).is_err());
    }
    let mut r = mpor_option(false);
    r.policy.initial_margin_model = im_model(0.);
    assert!(evaluate(&r).is_err(), "standardized IM covers swaps only");
}
