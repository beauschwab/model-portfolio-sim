use portfolio_model_core::{
    cashflow::CashflowMeasure,
    floating_rate::{IndexDescriptor, IndexKind, RevisionIdentity, SuppliedFixing},
    mortgage_state::*,
};
use portfolio_risk_native::{
    advanced_graph::{OasTarget, Role},
    mortgage_graph::*,
};
fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/mortgage-product-graph-1.json"
    ))
    .unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-9, "{a} != {b}");
}
fn id(value: &str) -> RevisionIdentity {
    RevisionIdentity {
        id: value.into(),
        revision: "v1".into(),
    }
}
fn source(result: &ResultData, role: Role) -> &MortgageResult {
    &result
        .native_sources
        .iter()
        .find(|s| s.role == role)
        .unwrap()
        .projection
}
fn all_physical(metric: &Metrics, field: fn(&PhysicalAmounts) -> f64) -> f64 {
    metric.physical_periods.iter().map(field).sum::<f64>() + field(&metric.physical_tail)
}
fn add_scenario(r: &mut Request, name: &str, rate: f64) {
    let mut scenario = r.scenarios[0].clone();
    scenario.scenario_id = name.into();
    scenario.run_id = format!("{name}-run");
    scenario.collateral_curve.discount_factors = scenario
        .collateral_curve
        .times_years
        .iter()
        .map(|t| (-rate * t).exp())
        .collect();
    r.scenarios.push(scenario);
    let mut set = r.positions[0].histories[0].clone();
    set.scenario_id = name.into();
    r.positions[0].histories.push(set);
}
fn arm(r: &mut Request, recast: PaymentRecast) {
    let c = &mut r.positions[0].contract;
    c.rate = MortgageRate::Arm {
        terms: ArmTerms {
            index: IndexDescriptor {
                index_id: "supplied-index".into(),
                kind: IndexKind::Term,
                tenor_months: Some(1),
                average_calendar_days: None,
            },
            reference_calendar: id("supplied-calendar"),
            margin: 0.01,
            original_note_rate: c.current_note_rate,
            initial_up_cap: 0.02,
            initial_down_cap: 0.015,
            subsequent_up_cap: 0.01,
            subsequent_down_cap: 0.01,
            lifetime_up_cap: 0.03,
            lifetime_down_cap: 0.025,
            note_floor: 0.02,
            prior_reset_count: 0,
            payment_recast: recast,
        },
        resets: vec![],
    };
    let reset = ArmReset {
        ordinal: c.as_of_ordinal + 10,
        input: MortgageIndexInput::Projected {
            fixing: SuppliedFixing {
                fixing_id: "explicit-reset".into(),
                revision: "v1".into(),
                index_id: "supplied-index".into(),
                value_ordinal: c.as_of_ordinal + 10,
                publication_unix_seconds: c.publication_cutoff_unix_seconds - 1,
                rate: 0.10,
            },
            projection_source: id("supplied-index-path"),
            available_unix_seconds: c.publication_cutoff_unix_seconds - 1,
            reset_cutoff_unix_seconds: c.publication_cutoff_unix_seconds + 864000,
        },
    };
    for set in &mut r.positions[0].histories {
        for histories in [
            &mut set.physical,
            &mut set.pricing,
            &mut set.pricing_down,
            &mut set.pricing_up,
        ] {
            histories[0].arm_resets = vec![reset.clone()];
        }
    }
}
fn agency(r: &mut Request) {
    let c = &mut r.positions[0].contract;
    c.horizon_days = 50;
    c.ownership = MortgageOwnership::Agency {
        issuer: "supplied-guarantor".into(),
        guarantee: id("supplied-guarantee"),
        remittance_lag_days: 2,
        default_buyout_lag_days: 20,
        servicing_and_guarantee_spread: 0.01,
        default_coupon_policy: DefaultCouponPolicy::FrozenAtDefault,
    };
}

#[test]
fn independent_actual_payment_price_and_lagged_risk_fixture() {
    let r = request();
    let result = analyze(&r).unwrap();
    let m = &result.metrics[0];
    let final_cash = 32.1661 * (1. + 0.0365 * 10. / 365.);
    close(m.pv, 68. + final_cash);
    let b = r.bump_bp / 10000.;
    let dv01 = (34. * (b * 12. / 365.).sinh()
        + 34. * (b * 22. / 365.).sinh()
        + final_cash * (b * 32. / 365.).sinh())
        / r.bump_bp;
    close(m.supplied_history_dv01, dv01);
    close(all_physical(m, |a| a.investor_principal_cash), 100.);
    close(
        all_physical(m, |a| a.gross_recognized_investor_interest),
        0.1982661,
    );
    close(m.physical_periods[0].principal_entitlement, 33.9);
    close(m.physical_periods[0].investor_principal_cash, 0.);
    close(m.physical_tail.investor_principal_cash, 66.1);
    let q = source(&result, Role::Pricing);
    assert_eq!(q.identity.measure, CashflowMeasure::Pricing);
    assert_eq!(
        source(&result, Role::Physical).identity.measure,
        CashflowMeasure::Physical
    );
    assert_eq!(q.cashflows.last().unwrap().ordinal, r.as_of_ordinal + 32);
    close(q.points.last().unwrap().contractual_principal, 0.);
}

#[test]
fn physical_competing_refinance_is_not_implicitly_pricing_measure() {
    let mut r = request();
    r.positions[0].contract.refinance.incentive_slope = 10.;
    let set = &mut r.positions[0].histories[0];
    for histories in [&mut set.pricing, &mut set.pricing_down, &mut set.pricing_up] {
        histories[0].path[0].refinancing_market_rate = 0.1;
    }
    set.physical[0].path[0].refinancing_market_rate = 0.;
    set.physical[0].path[0].turnover_hazard_per_year = 0.2;
    let result = analyze(&r).unwrap();
    let p = source(&result, Role::Physical);
    let h = 0.2 + 10. * (0.0365 - 0.005) * (-0.02_f64).exp();
    let s = (-h * 10. / 365.).exp();
    close(p.points[0].turnover_hazard_per_year, 0.2);
    close(
        p.borrower_cashflows
            .iter()
            .filter(|e| e.ordinal == r.as_of_ordinal + 10 && e.kind == "turnover")
            .map(|e| e.principal)
            .sum(),
        100. * 0.2 / h * (1. - s),
    );
    close(
        result.metrics[0].physical_periods[0].gross_recognized_investor_interest,
        100. * 0.0365 * (1. - s) / h,
    );
    // Q remains its supplied zero-turnover/high-refi-rate path.
    close(result.metrics[0].pv, 100.1982661);
    close(
        all_physical(&result.metrics[0], |a| a.investor_principal_cash),
        100.,
    );
}

#[test]
fn fixed_base_oas_is_original_through_plus_minus_200bp_and_revised_physical_paths() {
    let mut r = request();
    add_scenario(&mut r, "up200", 0.02);
    add_scenario(&mut r, "down200", -0.02);
    r.positions[0].oas_target = OasTarget::CalibratePositiveCash {
        target_cash_price: 100.1982661,
        source_id: "controlled-quote".into(),
        source_revision: "v1".into(),
    };
    let graph = MortgageGraph::new(&r).unwrap();
    close(graph.base_anchor().oas[0].base_oas, 0.);
    let cash = [(34., 12.), (34., 22.), (32.1982661, 32.)];
    for (index, rate) in [(1, 0.02), (2, -0.02)] {
        close(
            graph.snapshot().metrics[index].pv,
            cash.iter()
                .map(|(c, d)| c * (-rate * d / 365_f64).exp())
                .sum(),
        );
    }
    let mut changed = r.clone();
    changed.identity.input_revision = "revised".into();
    changed.positions[0].histories[1].physical[0].path[0].turnover_hazard_per_year = 1.;
    let fresh = graph.repriced(&changed).unwrap();
    close(
        fresh.snapshot().metrics[1].pv,
        graph.snapshot().metrics[1].pv,
    );
    assert_eq!(
        serde_json::to_value(fresh.base_anchor()).unwrap(),
        serde_json::to_value(graph.base_anchor()).unwrap()
    );
    assert_eq!(graph.request().identity.input_revision, "mortgage-input-v1");
    assert_eq!(fresh.request().identity.input_revision, "revised");
    changed.positions[0].histories[0].pricing[0].path[0].turnover_hazard_per_year = 0.1;
    assert!(graph
        .reprice(&changed)
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}

#[test]
fn explicit_q_bump_histories_enter_risk_with_native_discount_bumps() {
    let mut r = request();
    r.positions[0].histories[0].pricing_down[0].path[0].turnover_hazard_per_year = 1.;
    let result = analyze(&r).unwrap();
    let qdown = source(&result, Role::PricingDown);
    let qup = source(&result, Role::PricingUp);
    let bumped = |projection: &MortgageResult, rate: f64| {
        projection
            .cashflows
            .iter()
            .map(|e| {
                (e.principal + e.interest)
                    * (-rate * (e.ordinal - r.as_of_ordinal) as f64 / 365.).exp()
            })
            .sum::<f64>()
    };
    close(
        result.metrics[0].supplied_history_dv01,
        (bumped(qdown, -0.0025) - bumped(qup, 0.0025)) / 50.,
    );
}

#[test]
fn principal_only_whole_loan_defaults_and_recovery_tail_conserve_without_terminal_plug() {
    let mut r = request();
    r.positions[0].contract.current_note_rate = 0.;
    r.positions[0].histories[0].physical[0].path[0].credit_hazards_per_year[0][3] = 2.;
    let result = analyze(&r).unwrap();
    let p = source(&result, Role::Physical);
    let m = &result.metrics[0];
    let s = (-2_f64 * 10. / 365.).exp();
    let default = 100. * (1. - s) + 66. * s * (1. - s) + 32. * s * s * (1. - s);
    close(
        p.defaults.iter().map(|d| d.borrower_principal).sum(),
        default,
    );
    close(
        all_physical(m, |a| a.borrower_principal_loss),
        0.4 * default,
    );
    close(
        all_physical(m, |a| a.whole_loan_recovery_principal_cash),
        0.6 * default,
    );
    close(
        all_physical(m, |a| a.investor_principal_cash) + 0.4 * default,
        100.,
    );
    assert!(p.cashflows.last().unwrap().ordinal > r.as_of_ordinal + 30);
    close(m.pv, 100.); // Zero-credit Q is explicit and differs from P.
}

#[test]
fn whole_loan_default_accrued_and_incomplete_surviving_rights_reject_entire_graph() {
    let mut r = request();
    r.positions[0].histories[0].physical[0].path[0].credit_hazards_per_year[0][3] = 2.;
    assert!(analyze(&r)
        .err()
        .unwrap()
        .contains("collection or disposal"));
    let mut r = request();
    r.positions[0].contract.opening_probability = [0., 0., 1.];
    r.positions[0].contract.opening_borrower_principal = [0., 0., 100.];
    r.positions[0].contract.opening_investor_principal = [0., 0., 100.];
    assert!(analyze(&r)
        .err()
        .unwrap()
        .contains("complete contractual/live investor rights"));
}

#[test]
fn agency_guarantee_advances_keep_unpaid_borrower_rights_separate() {
    let mut r = request();
    agency(&mut r);
    r.positions[0].contract.opening_probability = [0., 0., 1.];
    r.positions[0].contract.opening_borrower_principal = [0., 0., 100.];
    r.positions[0].contract.opening_investor_principal = [0., 0., 100.];
    let result = analyze(&r).unwrap();
    let p = source(&result, Role::Physical);
    let m = &result.metrics[0];
    let coupon = 0.0265;
    let interest = (100. + 66.1 + 32.1661) * coupon * 10. / 365.;
    close(m.physical_totals.guarantee_principal_advance, 100.);
    close(m.physical_totals.guarantee_interest_advance, interest);
    close(all_physical(m, |a| a.borrower_principal_cash), 0.);
    close(all_physical(m, |a| a.investor_principal_cash), 100.);
    close(m.pv, 100. + interest);
    close(p.points.last().unwrap().borrower_principal[2], 100.);
    assert!(p.points.last().unwrap().borrower_interest[2] > 0.);
    close(p.points.last().unwrap().investor_principal.iter().sum(), 0.);
}

#[test]
fn agency_default_buyout_and_frozen_coupon_do_not_add_borrower_recovery_twice() {
    let mut r = request();
    agency(&mut r);
    r.positions[0].histories[0].physical[0].path[0].credit_hazards_per_year[0][3] = 2.;
    let result = analyze(&r).unwrap();
    let p = source(&result, Role::Physical);
    let first = &p.defaults[0];
    let t = 10. / 365.;
    let s = (-2_f64 * t).exp();
    close(first.investor_principal, 100. * (1. - s));
    close(
        first.investor_accrued_interest,
        100. * 0.0265 * ((1. - s) / 2. - t * s),
    );
    assert_eq!(
        first.investor_buyout_ordinal,
        Some(first.default_ordinal + 20)
    );
    close(
        all_physical(&result.metrics[0], |a| a.investor_principal_cash),
        100.,
    );
    close(
        all_physical(&result.metrics[0], |a| a.whole_loan_recovery_principal_cash),
        0.,
    );
    assert!(all_physical(&result.metrics[0], |a| a.borrower_recovery_principal_cash) > 0.);
    assert!(all_physical(&result.metrics[0], |a| a.agency_buyout_principal) > 0.);
    assert!(p
        .cashflows
        .iter()
        .all(|e| e.kind != "whole_loan_default_recovery"));
}

#[test]
fn arm_cap_reset_and_original_payment_are_retained_by_measure() {
    let mut r = request();
    arm(&mut r, PaymentRecast::RetainOriginal);
    let result = analyze(&r).unwrap();
    let q = source(&result, Role::Pricing);
    close(q.resets[0].new_rate, 0.0565);
    close(q.resets[0].new_payment, 34.);
    assert_eq!(q.resets[0].fixing_id, "explicit-reset");
    assert!(q.resets[0].projected);
    let first_interest = 0.1;
    let second_interest = 66.1 * 0.0565 * 10. / 365.;
    let remaining = 66.1 - (34. - second_interest);
    let final_interest = remaining * 0.0565 * 10. / 365.;
    close(
        result.metrics[0].pv,
        100. + first_interest + second_interest + final_interest,
    );
    close(q.original_payment, 34.);
    assert!(!q.calendar_mapping_verified);
}

#[test]
fn arm_recast_uses_complete_actual_remaining_contractual_schedule() {
    let mut r = request();
    arm(&mut r, PaymentRecast::ReamortizeRemaining);
    let result = analyze(&r).unwrap();
    let q = source(&result, Role::Pricing);
    let g = 1. + 0.0565 * 10. / 365.;
    let payment = 66.1 * g * g / (1. + g);
    close(q.resets[0].new_payment, payment);
    close(result.metrics[0].pv, 34. + 2. * payment);
    close(q.original_payment, 34.);
}

#[test]
fn zero_change_and_weighted_paired_histories_are_exact_repeatable() {
    let mut r = request();
    add_scenario(&mut r, "identity", 0.);
    for set in &mut r.positions[0].histories {
        for paths in [
            &mut set.physical,
            &mut set.pricing,
            &mut set.pricing_down,
            &mut set.pricing_up,
        ] {
            let mut second = paths[0].clone();
            paths[0].weight = 0.7;
            second.path_id = "paired-1".into();
            second.weight = 0.3;
            paths.push(second);
        }
    }
    let result = analyze(&r).unwrap();
    close(result.metrics[0].pv, 100.1982661);
    assert_eq!(
        serde_json::to_value(&result.metrics[0].physical_periods).unwrap(),
        serde_json::to_value(&result.metrics[1].physical_periods).unwrap()
    );
    assert_eq!(result.metrics[0].pv, result.metrics[1].pv);
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::to_value(analyze(&r).unwrap()).unwrap()
    );
}

#[test]
fn domains_tail_measure_dates_and_pairing_reject_before_simplified_fallback() {
    for case in 0..9 {
        let mut r = request();
        match case {
            0 => r.positions[0].contract.horizon_days = 20,
            1 => r.positions[0].contract.identity.measure = CashflowMeasure::Physical,
            2 => r.positions[0].histories[0].pricing_up[0].path_id = "unpaired".into(),
            3 => r.positions[0].histories[0].physical[0].weight = 0.9,
            4 => r.positions[0].histories[0].physical[0].path[0].start_ordinal += 1,
            5 => {
                agency(&mut r);
                r.positions[0].contract.horizon_days = 30;
            }
            6 => {
                arm(&mut r, PaymentRecast::RetainOriginal);
                if let MortgageIndexInput::Projected {
                    available_unix_seconds,
                    ..
                } = &mut r.positions[0].histories[0].pricing[0].arm_resets[0].input
                {
                    *available_unix_seconds += 1_000_000;
                }
            }
            7 => r.positions[0].contract.horizon_days = u32::MAX,
            _ => r.identity.randomness_domain = "stochastic-crn-claim".into(),
        }
        assert!(analyze(&r).is_err(), "case{case}");
    }
}

#[test]
fn aggregate_work_and_curve_tail_are_admitted_without_hidden_flat_extension() {
    let mut r = request();
    r.scenarios[0].collateral_curve.times_years = vec![0., 0.01];
    r.scenarios[0].collateral_curve.discount_factors = vec![1., 1.];
    assert!(analyze(&r).is_err());
    let mut r = request();
    let template = r.positions[0].clone();
    r.positions = (0..256)
        .map(|i| {
            let mut p = template.clone();
            p.position_id = format!("loan{i}");
            p.contract.identity.position_id = p.position_id.clone();
            let set = &mut p.histories[0];
            for paths in [
                &mut set.physical,
                &mut set.pricing,
                &mut set.pricing_down,
                &mut set.pricing_up,
            ] {
                let h = paths[0].clone();
                *paths = (0..128)
                    .map(|j| {
                        let mut v = h.clone();
                        v.path_id = format!("paired{j}");
                        v.weight = 1. / 128.;
                        v
                    })
                    .collect();
            }
            p
        })
        .collect();
    assert!(analyze(&r).err().unwrap().contains("admission"));
}

#[test]
fn audit_work_is_validated_without_path_generation_or_target_solving() {
    let mut r = request();
    let work = audit_admitted_work(&r).unwrap();
    assert!(work > 0 && work <= MAX_WORK);
    // Admission returns work even when a financial calibration would fail;
    // it does not attempt an OAS root while admitting a cohort workload.
    r.positions[0].oas_target = OasTarget::CalibratePositiveCash {
        target_cash_price: 1e15,
        source_id: "unattainable-quote".into(),
        source_revision: "v1".into(),
    };
    assert_eq!(audit_admitted_work(&r).unwrap(), work);
    assert!(analyze(&r).is_err());
    r.positions[0].histories[0].pricing_up[0].weight = 0.5;
    assert!(audit_admitted_work(&r).is_err());
}

#[test]
fn shared_preflight_has_same_invalid_contract_and_fixing_domains_as_projection() {
    for case in 0..2 {
        let mut r = request();
        if case == 0 {
            r.positions[0].contract.original_payment = 0.;
        } else {
            arm(&mut r, PaymentRecast::RetainOriginal);
            if let MortgageIndexInput::Projected {
                available_unix_seconds,
                ..
            } = &mut r.positions[0].histories[0].pricing[0].arm_resets[0].input
            {
                *available_unix_seconds += 1_000_000;
            }
        }
        let mut raw = r.positions[0].contract.clone();
        raw.identity.measure = CashflowMeasure::Pricing;
        raw.path = r.positions[0].histories[0].pricing[0].path.clone();
        if let MortgageRate::Arm { resets, .. } = &mut raw.rate {
            *resets = r.positions[0].histories[0].pricing[0].arm_resets.clone();
        }
        let preflight = validate_request(&raw).err().unwrap();
        assert_eq!(project(&raw).err().unwrap(), preflight);
        assert_eq!(audit_admitted_work(&r).err().unwrap(), preflight);
    }
}

#[test]
fn zero_investor_cash_still_retains_later_issuer_default_loss_tail() {
    let mut r = request();
    agency(&mut r);
    let contract = &mut r.positions[0].contract;
    contract.opening_probability = [0., 0., 1.];
    contract.opening_borrower_principal = [0., 0., 100.];
    contract.opening_investor_principal = [0.; 3];
    contract.loss_given_default = [1.; 3];
    contract.complete_recovery_tails = [vec![], vec![], vec![]];
    r.positions[0].histories[0].physical[0].path[0].credit_hazards_per_year[2][3] = 2.;
    let result = analyze(&r).unwrap();
    close(result.metrics[0].pv, 0.);
    close(
        all_physical(&result.metrics[0], |a| a.borrower_principal_loss),
        100. * (1. - (-2_f64 * 50. / 365.).exp()),
    );
    assert!(result.metrics[0].physical_tail.borrower_principal_loss > 0.);
    assert!(source(&result, Role::Physical).cashflows.is_empty());
}
