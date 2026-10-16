//! Raw-policy proof and actual varying-market observation controls.
use super::*;

fn request() -> Request {
    let graph: crate::commercial_graph::Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-generated-fee-graph-1.json"
    ))
    .unwrap();
    graph.positions[0]
        .generated_policy
        .as_deref()
        .unwrap()
        .pricing_training
        .clone()
}

#[test]
fn fixed_fee_observation_proof_requires_complete_zero_credit_and_original_headers() {
    let r = request();
    assert!(r.annual_covariance.iter().any(|x| *x > 0.));
    assert!(r.source.credit.intervals.iter().all(|p| p
        .hazards_per_year
        .iter()
        .flatten()
        .all(|h| *h == 0.)));
    assert!(fees::observed_features_are_path_constant(&r, true));
    assert!(!fees::observed_features_are_path_constant(&r, false));
    let mut other = r.clone();
    other.nonintegral_fees = None;
    assert!(!fees::observed_features_are_path_constant(&other, true));
    let mut other = r.clone();
    let secondary: owner::Request = serde_json::from_slice(include_bytes!(
        "../../../../../../examples/models/commercial-secondary-acquisition-1.json"
    ))
    .unwrap();
    assert!(secondary.source.transfer.is_some());
    other.source.source.transfer = secondary.source.transfer;
    assert!(!fees::observed_features_are_path_constant(&other, true));
    let mut other = r.clone();
    other.source.source.current.principal_plan = term::indexed::PrincipalPlan::RetainedPayment {
        original_payment: 1.,
        terminal: term::indexed::TerminalPrincipal::ResidualPayoff {},
    };
    assert!(!fees::observed_features_are_path_constant(&other, true));
    let mut other = r.clone();
    other.source.reviewed_credit_actions = Some(
        portfolio_model_core::commercial_coupon_accounting::reviews::ReviewedActions {
            policy: portfolio_model_core::credit_accounting::AccountingPolicy {
                identity: r.source.accounting.identity.clone(),
                accrued: portfolio_model_core::credit_accounting::AccruedPolicy::ReverseIncomeAtDefault,
                basis: portfolio_model_core::credit_accounting::BasisPolicy::TransferUntilReviewedChargeoff,
                allowance_shortfall: portfolio_model_core::credit_accounting::AllowanceShortfall::Reject,
                excess_cash_recovery: portfolio_model_core::credit_accounting::ExcessCashRecovery::CreditAllowance,
            },
            allowance_reviews: Vec::new(),
            chargeoffs: Vec::new(),
            legal_waivers: Vec::new(),
        },
    );
    assert!(!fees::observed_features_are_path_constant(&other, true));
}

#[test]
fn every_raw_role_rechecks_off_origin_hazards_and_unsupported_principal_and_draws() {
    let r = request();
    for hazard in [f64::from_bits(1), 0.01, -0.01, f64::NAN] {
        let mut other = r.clone();
        other
            .source
            .credit
            .intervals
            .last_mut()
            .unwrap()
            .hazards_per_year[2][3] = hazard;
        let facts = numerical_bound::inspect(&other).unwrap();
        assert!(!facts.all_credit_hazards_zero);
        assert!(!fees::observed_features_are_path_constant(
            &other,
            facts.all_credit_hazards_zero
        ));
        assert_eq!(
            regression_numerical_columns_upper(&other).unwrap(),
            regression_columns_upper(&other).unwrap()
        );
        assert!(fees::observed_features_are_path_constant(
            &r,
            numerical_bound::inspect(&r)
                .unwrap()
                .all_credit_hazards_zero
        ));
    }
    let mut other = r.clone();
    other.source.source.current.principal_plan =
        term::indexed::PrincipalPlan::DatedSuppliedPeriodOpening {
            principal_events: Vec::new(),
        };
    assert!(!fees::observed_features_are_path_constant(&other, true));
    other = r.clone();
    other
        .source
        .performing_draw_tranches
        .push(owner::PerformingDrawTranche {
            review: r.identity.clone(),
            draw_ordinal: r.source.source.current.asof_ordinal + 1,
            eligible_origins: [true, false, false],
            source: r.source.source.clone(),
        });
    assert!(!fees::observed_features_are_path_constant(&other, true));
}

#[test]
fn actual_varying_market_paths_retain_all_constant_fee_features_in_the_fit() {
    let r = request();
    let tape = generate_tape(&r, &r.training).unwrap();
    let paths = collect(&r, &r.training, &tape).unwrap();
    assert_eq!(paths.len(), 128);
    let width = paths[0].points[0].features.len();
    let fee_count = fees::feature_count(&r).unwrap();
    assert_eq!(fee_count, 22);
    let fee_start = width - fee_count;
    assert!(paths
        .iter()
        .any(|p| p.points[0].features[0] != paths[0].points[0].features[0]));
    for path in &paths {
        assert_eq!(path.points[0].features.len(), width);
        assert_eq!(
            path.points[0].features[fee_start..],
            paths[0].points[0].features[fee_start..]
        );
        assert!(path.points[0].fee_payoffs.is_some());
    }
    let trained = fit_paths(&r, &paths).unwrap();
    for regression in &trained.regressions {
        assert_eq!(regression.means.len(), width);
        assert!(regression.active_features.iter().all(|i| *i < fee_start));
        assert_eq!(
            regression.means[fee_start..],
            paths[0].points[0].features[fee_start..]
        );
    }
}

#[test]
fn signed_prepaid_and_refunded_fee_blocks_are_constant_at_each_observation_clock() {
    use portfolio_model_core::nonintegral_fee::{Action, Reason};
    for sign in [-1., 1.] {
        for timing in [
            ExerciseTiming::BeforeScheduledCash,
            ExerciseTiming::AfterScheduledCash,
        ] {
            let mut r = request();
            let at = r.policy.exercise_dates[0].ordinal;
            r.policy.exercise_dates[0].timing = timing;
            let mut second = r.policy.exercise_dates[0].clone();
            second.ordinal = at + 1;
            second.timing = ExerciseTiming::BeforeScheduledCash;
            r.policy.exercise_dates.push(second);
            r.training.market_seed = vec![19];
            let law = r.nonintegral_fees.as_mut().unwrap();
            // This declared control prepays service. Its reviewed exit policy
            // must dispose of that prepaid stock even before service is earned.
            law.policy.exercise.refund_prepaid_fraction = 1.;
            law.policy.maturity.refund_prepaid_fraction = 1.;
            let service = &mut law.policy.fees[1];
            service.entitlement *= sign;
            service.earned_refunds_allowed = true;
            for event in &mut law.policy.events {
                if event.fee_id == service.fee_id {
                    event.amount *= sign;
                    event.ordinal = if event.action == Action::Cash {
                        at - 1
                    } else {
                        at
                    };
                    if event.action == Action::Cash {
                        event.sequence = 2;
                    }
                    if event.action == Action::Earn {
                        event.amount = 2. * sign;
                        event.sequence = 5;
                    }
                }
            }
            let mut early_earning = law.policy.events[2].clone();
            early_earning.event_id = "origin-partial-earning".into();
            early_earning.ordinal = at - 1;
            early_earning.sequence = 3;
            early_earning.amount = sign;
            let mut refund = early_earning.clone();
            refund.event_id = "reviewed-earned-refund".into();
            refund.sequence = 4;
            refund.action = Action::RefundEarned;
            refund.reason = Reason::OtherReviewed;
            refund.amount = 0.5 * sign;
            law.policy.events.push(early_earning);
            law.policy.events.push(refund);
            law.policy.events.sort_by_key(|e| (e.ordinal, e.sequence));
            assert!(fees::observed_features_are_path_constant(&r, true));
            let tape = generate_tape(&r, &r.training).unwrap();
            let paths = collect(&r, &r.training, &tape).unwrap();
            let width = paths[0].points[0].features.len();
            let fee_start = width - fees::feature_count(&r).unwrap();
            for index in 0..2 {
                for path in &paths {
                    assert_eq!(path.points[index].features.len(), width);
                    assert_eq!(
                        path.points[index].features[fee_start..],
                        paths[0].points[index].features[fee_start..]
                    );
                    assert!(path.points[index].fee_payoffs.is_some());
                }
            }
            if matches!(timing, ExerciseTiming::BeforeScheduledCash) {
                assert_ne!(
                    paths[0].points[0].features[fee_start..],
                    paths[0].points[1].features[fee_start..]
                );
            }
            let trained = fit_paths(&r, &paths).unwrap();
            assert_eq!(trained.regressions.len(), 2);
            for regression in &trained.regressions {
                assert_eq!(regression.means.len(), width);
                assert!(regression.active_features.iter().all(|i| *i < fee_start));
            }
        }
    }
}
