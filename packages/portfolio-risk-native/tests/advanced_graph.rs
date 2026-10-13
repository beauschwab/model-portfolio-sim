use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_model_core::consumer_installment::{
    InstallmentAction, InstallmentProduct, InstallmentStep,
};
use portfolio_risk_native::advanced_graph::*;

fn request() -> Request {
    let mut contract: portfolio_model_core::consumer_installment::InstallmentRequest =
        serde_json::from_str(include_str!(
            "../../../examples/models/consumer-installment-1.json"
        ))
        .unwrap();
    contract.identity.measure = CashflowMeasure::Contractual;
    let curve: portfolio_risk_native::model_market::curves::SuppliedCurve = serde_json::from_value(
        serde_json::json!({"curve_id":"collateral","source_id":"supplied-synthetic","source_revision":"v1",
            "currency":"USD","role":{"kind":"collateral","collateral_index":"SOFR"},
            "times_years":[0.,1.,2.],"discount_factors":[1.,(-0.05_f64).exp(),(-0.1_f64).exp()],"right_tail":"reject"})).unwrap();
    let history = History {
        path_id: "paired-0".into(),
        weight: 1.,
        assumption_id: "supplied-actions-v1".into(),
        steps: vec![],
    };
    Request {
        schema: SCHEMA.into(),
        identity: GraphIdentity {
            model_revision: "consumer-supplied-v1".into(),
            input_revision: "graph-input-v1".into(),
            paired_history_source: "explicit-controlled-actions".into(),
            paired_history_revision: "paired-v1".into(),
            randomness_domain: "supplied_paired_histories-1".into(),
        },
        as_of_ordinal: contract.as_of_ordinal,
        currency: "USD".into(),
        reporting_ordinals: vec![
            contract.as_of_ordinal,
            contract.payment_dates[0],
            contract.payment_dates[1],
        ],
        base_scenario_id: "base".into(),
        bump_bp: 25.,
        scenarios: vec![Scenario {
            scenario_id: "base".into(),
            run_id: "base-run".into(),
            severity: 1.,
            collateral_curve: curve,
        }],
        positions: vec![Position {
            position_id: contract.identity.position_id.clone(),
            oas_target: OasTarget::Locked {
                base_oas: -0.05,
                source_id: "reviewed-oas".into(),
                source_revision: "v1".into(),
            },
            product: Product::ConsumerInstallment {
                contract,
                histories: vec![ScenarioHistories {
                    scenario_id: "base".into(),
                    physical: vec![history.clone()],
                    pricing: vec![history.clone()],
                    pricing_down: vec![history.clone()],
                    pricing_up: vec![history],
                }],
            },
        }],
    }
}
fn sets(r: &mut Request) -> &mut Vec<ScenarioHistories> {
    let Product::ConsumerInstallment { histories, .. } = &mut r.positions[0].product;
    histories
}
fn near(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-10_f64.max(b.abs() * 1e-12),
        "{a} != {b}"
    );
}
fn capture(graph: &AdvancedGraph) -> std::result::Result<ScheduledCapture, String> {
    graph.capture_scheduled(
        &[CaptureOwner {
            position_id: "personal-1".into(),
            account_id: "consumer-assets".into(),
            source_id: "native-consumer-source".into(),
            source_revision: "graph-input-v1".into(),
            ownership: "fully_scenario_generated".into(),
            direction: "receipt".into(),
        }],
        &CapturePolicy {
            policy_id: "par-note-policy".into(),
            policy_revision: "v1".into(),
            recognition_model: "consumer_simple_interest_par-1".into(),
            book_basis: "par_note_simple_interest".into(),
            fees: "reject".into(),
        },
    )
}
#[test]
fn actual_date_graph_has_independent_payment_interest_price_and_risk() {
    let r = request();
    let graph = AdvancedGraph::new(&r).unwrap();
    let m = &graph.snapshot().metrics[0];
    // Jan31->Feb29: 1000*.365*29/365=29; payment529 retires500.
    // Feb29->Mar31: 500*.365*31/365=15.5; final payment515.5.
    near(m.pv, 1044.5);
    near(m.gross_physical_contract_interest[0], 29.);
    near(m.gross_physical_contract_interest[1], 15.5);
    assert_eq!(m.physical_principal_cash, vec![500., 500.]);
    near(
        m.supplied_history_dv01,
        (529. * (0.0025_f64 * 29. / 365.).sinh() + 515.5 * (0.0025_f64 * 60. / 365.).sinh()) / 25.,
    );
    assert!(graph
        .snapshot()
        .native_sources
        .iter()
        .all(|s| s.projection.identity.measure
            == if s.role == Role::Physical {
                CashflowMeasure::Physical
            } else {
                CashflowMeasure::Pricing
            }));
}
#[test]
fn physical_payoff_is_not_implicitly_pricing_measure_and_bump_histories_are_explicit() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payoff {},
    });
    sets(&mut r)[0].pricing_down[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payoff {},
    });
    let graph = AdvancedGraph::new(&r).unwrap();
    let m = &graph.snapshot().metrics[0];
    near(m.pv, 1044.5);
    near(m.gross_physical_contract_interest.iter().sum(), 29.);
    let down = 1029. * (0.0025_f64 * 29. / 365.).exp();
    let up = 529. * (-0.0025_f64 * 29. / 365.).exp() + 515.5 * (-0.0025_f64 * 60. / 365.).exp();
    near(m.supplied_history_dv01, (down - up) / 50.);
    near(
        capture(&graph).unwrap().sources[0].schedule.events[0]
            .components
            .principal,
        1000.,
    );
}
#[test]
fn native_positive_target_calibrates_once_and_reprice_requires_exact_base_anchor() {
    let mut r = request();
    r.positions[0].oas_target = OasTarget::CalibratePositiveCash {
        target_cash_price: 1044.5,
        source_id: "observed-quote".into(),
        source_revision: "v1".into(),
    };
    let graph = AdvancedGraph::new(&r).unwrap();
    near(graph.base_anchor().oas[0].base_oas, -0.05);
    let mut second = r.scenarios[0].clone();
    second.scenario_id = "up100".into();
    second.run_id = "up100-run".into();
    second.collateral_curve.discount_factors = vec![1., (-0.06_f64).exp(), (-0.12_f64).exp()];
    r.scenarios.push(second);
    let mut history = sets(&mut r)[0].clone();
    history.scenario_id = "up100".into();
    sets(&mut r).push(history);
    let result = graph.reprice(&r).unwrap();
    near(result.base_anchor.oas[0].base_oas, -0.05);
    near(
        result.metrics[1].pv,
        529. * (-0.01_f64 * 29. / 365.).exp() + 515.5 * (-0.01_f64 * 60. / 365.).exp(),
    );
    let day = r.reporting_ordinals[1];
    sets(&mut r)[0].pricing[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payoff {},
    });
    assert!(graph
        .reprice(&r)
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}
#[test]
fn scheduled_capture_keeps_all_native_origins_cash_and_ownership() {
    let graph = AdvancedGraph::new(&request()).unwrap();
    let output = capture(&graph).unwrap();
    assert_eq!(output.sources.len(), 1);
    assert_eq!(output.sources[0].schedule.scenarios, vec!["base-run"]);
    assert_eq!(output.accrual_plans.len(), 2);
    near(output.accrual_plans[0].segments[0].accrued_interest, 29.);
    near(output.accrual_plans[1].segments[0].accrued_interest, 15.5);
    near(
        output.sources[0]
            .schedule
            .events
            .iter()
            .map(|e| e.components.principal)
            .sum(),
        1000.,
    );
    assert_eq!(output.openings[0].gross_carrying_value, 1000.);
    let mut owners = output.owners.clone();
    owners[0].source_revision = "stale".into();
    assert!(graph.capture_scheduled(&owners, &output.policy).is_err());
    assert!(graph.capture_scheduled(&[], &output.policy).is_err());
}
#[test]
fn physical_default_keeps_retirement_recovery_loss_and_rejects_entire_capture() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Default {},
    });
    let graph = AdvancedGraph::new(&r).unwrap();
    let m = &graph.snapshot().metrics[0];
    near(m.physical_default_principal[0], 1000.);
    near(m.physical_principal_loss[0], 550.);
    near(m.physical_principal_cash[1], 450.);
    near(m.gross_physical_contract_interest[0], 29.);
    assert!(capture(&graph)
        .err()
        .unwrap()
        .contains("no partial event pruning"));
    assert!(graph.snapshot().native_sources[0]
        .projection
        .default
        .is_some());
}
#[test]
fn signed_recovery_cost_prices_at_locked_oas_and_never_calibrates_implicitly() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    let Product::ConsumerInstallment { contract, .. } = &mut r.positions[0].product;
    let InstallmentProduct::Personal {
        collection_cost, ..
    } = &mut contract.product
    else {
        panic!()
    };
    *collection_cost = 600.;
    sets(&mut r)[0].pricing[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Default {},
    });
    let central = sets(&mut r)[0].pricing.clone();
    sets(&mut r)[0].pricing_down = central.clone();
    sets(&mut r)[0].pricing_up = central;
    let graph = AdvancedGraph::new(&r).unwrap();
    near(graph.snapshot().metrics[0].pv, -100.);
    r.positions[0].oas_target = OasTarget::CalibratePositiveCash {
        target_cash_price: 1.,
        source_id: "quote".into(),
        source_revision: "v1".into(),
    };
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("signed Q cash"));
}
#[test]
fn path_probabilities_and_paired_identity_domains_are_strict() {
    let mut r = request();
    sets(&mut r)[0].pricing_up[0].path_id = "different-randomness".into();
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("ordered path identity"));
    let mut r = request();
    sets(&mut r)[0].physical[0].weight = 0.8;
    assert!(AdvancedGraph::new(&r).err().unwrap().contains("sum to one"));
    let mut r = request();
    r.identity.randomness_domain = "native_stochastic_policy".into();
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("no inferred stochastic policy"));
}
#[test]
fn native_weighted_physical_paths_conserve_without_becoming_gl_realizations() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    let mut payoff = sets(&mut r)[0].physical[0].clone();
    payoff.path_id = "payoff".into();
    payoff.weight = 0.3;
    payoff.steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payoff {},
    });
    sets(&mut r)[0].physical[0].weight = 0.7;
    sets(&mut r)[0].physical.push(payoff);
    let graph = AdvancedGraph::new(&r).unwrap();
    let m = &graph.snapshot().metrics[0];
    near(m.gross_physical_contract_interest[1], 10.85);
    near(m.physical_principal_cash.iter().sum(), 1000.);
    assert!(capture(&graph)
        .err()
        .unwrap()
        .contains("one explicit physical path"));
}
#[test]
fn repriced_snapshot_preserves_anchor_and_captures_current_physical_source_only() {
    let mut r = request();
    let original = AdvancedGraph::new(&r).unwrap();
    let day = r.reporting_ordinals[1];
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payoff {},
    });
    let revised = original.repriced(&r).unwrap();
    assert_eq!(
        serde_json::to_vec(original.base_anchor()).unwrap(),
        serde_json::to_vec(revised.base_anchor()).unwrap()
    );
    near(
        capture(&original).unwrap().sources[0].schedule.events[0]
            .components
            .principal,
        500.,
    );
    near(
        capture(&revised).unwrap().sources[0].schedule.events[0]
            .components
            .principal,
        1000.,
    );
}
#[test]
fn auto_collateral_repossession_loss_is_native_and_not_a_mortgage_relabel() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    let Product::ConsumerInstallment { contract, .. } = &mut r.positions[0].product;
    contract.product = InstallmentProduct::Auto {
        new_vehicle: true,
        vintage_ordinal: 738551,
        original_ltv: 1.,
        channel: "dealer".into(),
        collateral_value_as_of: 1000.,
        annual_depreciation: 0.1,
        sale_haircut: 0.2,
        repossession_cost: 50.,
        recovery_delay_days: 20,
    };
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Default {},
    });
    let graph = AdvancedGraph::new(&r).unwrap();
    let net = 1000. * 0.9_f64.powf(49. / 365.) * 0.8 - 50.;
    near(graph.snapshot().metrics[0].physical_principal_cash[1], net);
    near(
        graph.snapshot().metrics[0].physical_principal_loss[0],
        1000. - net,
    );
    assert!(capture(&graph).is_err());
}
#[test]
fn nonaccrual_cash_basis_and_fees_are_retained_in_analysis_but_reject_capture() {
    let mut r = request();
    let early = r.reporting_ordinals[1];
    let end = r.reporting_ordinals[2];
    sets(&mut r)[0].physical[0].steps = vec![
        InstallmentStep {
            ordinal: early,
            action: InstallmentAction::SetState {
                state: portfolio_model_core::consumer_contract::ConsumerState::Nonaccrual,
            },
        },
        InstallmentStep {
            ordinal: end,
            action: InstallmentAction::Payoff {},
        },
    ];
    let graph = AdvancedGraph::new(&r).unwrap();
    near(
        graph.snapshot().metrics[0]
            .gross_physical_contract_interest
            .iter()
            .sum(),
        60.,
    );
    assert!(capture(&graph).is_err());
    let mut r = request();
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: early,
        action: InstallmentAction::Fee { amount: 100. },
    });
    let graph = AdvancedGraph::new(&r).unwrap();
    near(
        graph.snapshot().metrics[0].physical_fee_cash.iter().sum(),
        100.,
    );
    assert!(capture(&graph).is_err());
}
#[test]
fn terminal_truncation_dates_and_measure_missing_coverage_reject() {
    let mut r = request();
    let end = r.reporting_ordinals[2];
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: end,
        action: InstallmentAction::Payment { amount: 500. },
    });
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("complete terminal obligations"));
    let mut r = request();
    let Product::ConsumerInstallment { contract, .. } = &mut r.positions[0].product;
    contract.horizon_days = u32::MAX;
    assert!(AdvancedGraph::new(&r).err().unwrap().contains("horizon"));
    let mut r = request();
    let Product::ConsumerInstallment { contract, .. } = &mut r.positions[0].product;
    contract.identity.measure = CashflowMeasure::Physical;
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("known contract/measure"));
    let mut r = request();
    sets(&mut r)[0].pricing_up.clear();
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("path admission"));
}
#[test]
fn capture_zero_event_runs_have_explicit_coverage_and_no_fabricated_settlement() {
    let mut r = request();
    let Product::ConsumerInstallment { contract, .. } = &mut r.positions[0].product;
    contract.opening_principal = 0.;
    let graph = AdvancedGraph::new(&r).unwrap();
    let output = capture(&graph).unwrap();
    assert!(output.sources[0].schedule.events.is_empty());
    assert!(matches!(
        output.sources[0].coverage[0].status,
        CaptureCoverageStatus::NoEvents { .. }
    ));
    assert!(output.accrual_plans.is_empty());
}
#[test]
fn strict_version_and_work_admission_reject_before_native_path_creation() {
    let mut r = request();
    r.identity.randomness_domain = "other".into();
    assert!(AdvancedGraph::new(&r).is_err());
    let mut r = request();
    let original = r.positions[0].clone();
    r.positions.clear();
    for i in 0..256 {
        let mut p = original.clone();
        p.position_id = format!("position-{i}");
        let Product::ConsumerInstallment {
            contract,
            histories,
        } = &mut p.product;
        contract.identity.position_id = p.position_id.clone();
        let path = histories[0].physical[0].clone();
        histories[0].physical = (0..128)
            .map(|j| {
                let mut h = path.clone();
                h.path_id = format!("path-{j}");
                h.weight = 1. / 128.;
                h
            })
            .collect();
        r.positions.push(p);
    }
    assert!(AdvancedGraph::new(&r)
        .err()
        .unwrap()
        .contains("aggregate run/row/work admission"));
}
#[test]
fn partial_interest_cash_keeps_full_fifo_origins_and_unsettled_rights() {
    let mut r = request();
    let day = r.reporting_ordinals[1];
    sets(&mut r)[0].physical[0].steps.push(InstallmentStep {
        ordinal: day,
        action: InstallmentAction::Payment { amount: 10. },
    });
    let graph = AdvancedGraph::new(&r).unwrap();
    let output = capture(&graph).unwrap();
    near(
        output.sources[0].schedule.events[0]
            .components
            .cash_interest,
        10.,
    );
    near(
        output.sources[0].schedule.events[1]
            .components
            .cash_interest,
        50.,
    );
    assert_eq!(output.accrual_plans[1].segments.len(), 2);
    near(
        output.accrual_plans[0].segments[0].accrued_interest
            + output.accrual_plans[1].segments[0].accrued_interest,
        29.,
    );
    near(output.accrual_plans[1].segments[1].accrued_interest, 31.);
    near(
        graph.snapshot().metrics[0]
            .gross_physical_contract_interest
            .iter()
            .sum(),
        60.,
    );
}
#[test]
fn persisted_synthetic_graph_example_has_independent_math() {
    let r: Request = serde_json::from_str(include_str!(
        "../../../examples/models/advanced-product-graph-1.json"
    ))
    .unwrap();
    let graph = AdvancedGraph::new(&r).unwrap();
    near(graph.snapshot().metrics[0].pv, 1044.5);
    near(
        graph.snapshot().metrics[0]
            .gross_physical_contract_interest
            .iter()
            .sum(),
        44.5,
    );
    near(
        graph.snapshot().metrics[0].supplied_history_dv01,
        (529. * (0.0025_f64 * 29. / 365.).sinh() + 515.5 * (0.0025_f64 * 60. / 365.).sinh()) / 25.,
    );
}
