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
