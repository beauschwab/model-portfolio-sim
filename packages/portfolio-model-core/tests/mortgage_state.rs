use portfolio_model_core::{
    cashflow::CashflowMeasure,
    floating_rate::{IndexDescriptor, IndexKind, RevisionIdentity, SuppliedFixing},
    mortgage_state::*,
};
use serde_json::{json, Value};
fn request() -> MortgageRequest {
    serde_json::from_str(include_str!(
        "../../../examples/models/mortgage-state-1.json"
    ))
    .unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-9, "{a} != {b}");
}
fn zero(r: &mut MortgageRequest) {
    r.path[0].credit_hazards_per_year = [[0.; 5]; 3];
    r.path[0].turnover_hazard_per_year = 0.;
    r.refinance.incentive_slope = 0.;
}
fn id(s: &str) -> RevisionIdentity {
    RevisionIdentity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn arm(r: &mut MortgageRequest, resets: Vec<ArmReset>, recast: PaymentRecast) {
    r.rate = MortgageRate::Arm {
        terms: ArmTerms {
            index: IndexDescriptor {
                index_id: "synthetic-index".into(),
                kind: IndexKind::Term,
                tenor_months: Some(1),
                average_calendar_days: None,
            },
            reference_calendar: id("supplied-calendar"),
            margin: 0.01,
            original_note_rate: r.current_note_rate,
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
        resets,
    };
}
fn reset(r: &MortgageRequest, offset: i32, rate: f64, projected: bool) -> ArmReset {
    let fixing = SuppliedFixing {
        fixing_id: format!("fix-{offset}"),
        revision: "v1".into(),
        index_id: "synthetic-index".into(),
        value_ordinal: r.as_of_ordinal + offset,
        publication_unix_seconds: r.publication_cutoff_unix_seconds - 10,
        rate,
    };
    ArmReset {
        ordinal: r.as_of_ordinal + offset,
        input: if projected {
            MortgageIndexInput::Projected {
                fixing,
                projection_source: id("explicit-forward-path"),
                available_unix_seconds: r.publication_cutoff_unix_seconds - 1,
                reset_cutoff_unix_seconds: r.publication_cutoff_unix_seconds
                    + 86400 * offset as i64,
            }
        } else {
            MortgageIndexInput::Observed {
                fixing,
                reset_cutoff_unix_seconds: r.publication_cutoff_unix_seconds
                    + 86400 * offset as i64,
            }
        },
    }
}

#[test]
fn retained_fixed_payment_actual_dates_and_terminal_principal_match_hand_schedule() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 30;
    let result = project(&r).unwrap();
    assert_eq!(result.original_payment, 34.);
    close(result.points[0].borrower_principal[0], 66.1);
    close(result.points[1].borrower_principal[0], 32.1661);
    close(result.points[2].borrower_principal[0], 0.);
    close(result.borrower_scheduled_principal, 100.);
    close(
        result.cashflows.iter().map(|c| c.interest).sum(),
        0.1 + 0.0661 + 0.0321661,
    );
    assert_eq!(result.cashflows[0].ordinal, r.as_of_ordinal + 12);
    assert_eq!(
        result.cashflows.last().unwrap().ordinal,
        r.as_of_ordinal + 32
    );
    close(result.points.last().unwrap().matured_probability, 1.);
    close(result.points.last().unwrap().probability[6], 0.);
}

#[test]
fn refinance_turnover_and_default_compete_once_with_full_recovery_tail() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 10;
    r.payment_dates = vec![r.as_of_ordinal + 20, r.as_of_ordinal + 30];
    r.current_note_rate = 0.1;
    r.path[0].refinancing_market_rate = 0.05;
    r.refinance.incentive_threshold = 0.;
    r.refinance.incentive_slope = 6.;
    r.refinance.burnout_sensitivity = 0.;
    r.path[0].turnover_hazard_per_year = 0.2;
    r.path[0].credit_hazards_per_year[0][3] = 0.2;
    let result = project(&r).unwrap();
    let t = 10. / 365.;
    let s = (-0.7_f64 * t).exp();
    let exit = 1. - s;
    close(result.borrower_turnover_principal, 100. * 0.2 / 0.7 * exit);
    close(result.borrower_refinance_principal, 100. * 0.3 / 0.7 * exit);
    close(result.points[0].borrower_principal[0], 100. * s);
    close(
        result.defaults[0].borrower_principal,
        100. * 0.2 / 0.7 * exit,
    );
    close(
        result.defaults[0].borrower_principal_loss,
        result.defaults[0].borrower_principal * 0.4,
    );
    close(
        result.defaults[0]
            .complete_recovery_tail
            .iter()
            .map(|p| p.principal)
            .sum(),
        result.defaults[0].borrower_principal * 0.6,
    );
    assert!(result.defaults[0]
        .complete_recovery_tail
        .iter()
        .all(|p| p.ordinal > r.as_of_ordinal + 10));
    let flux = (1. - s * (1. + 0.7 * t)) / (0.7 * 0.7);
    close(
        result
            .cashflows
            .iter()
            .filter(|c| c.kind == "turnover")
            .map(|c| c.interest)
            .sum(),
        100. * 0.1 * 0.2 * flux,
    );
    close(
        result
            .cashflows
            .iter()
            .filter(|c| c.kind == "refinance")
            .map(|c| c.interest)
            .sum(),
        100. * 0.1 * 0.3 * flux,
    );
    close(result.borrower_principal_conservation_error, 0.);
    close(result.investor_principal_conservation_error, 0.);
}

#[test]
fn delinquent_wholeloan_rights_survive_then_cure_without_invented_payment() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 20;
    r.opening_probability = [0., 0., 1.];
    r.opening_borrower_principal = [0., 0., 100.];
    r.opening_investor_principal = [0., 0., 100.];
    let no_cure = project(&r).unwrap();
    close(no_cure.points[1].borrower_principal[2], 100.);
    close(no_cure.points[1].borrower_interest[2], 0.2);
    close(no_cure.points[1].investor_interest[2], 0.);
    assert!(no_cure.cashflows.is_empty());
    r.path[0].credit_hazards_per_year[2][0] = 1.;
    let cured = project(&r).unwrap();
    assert!(cured.points[0].borrower_principal[0] > 0.);
    assert!(cured.borrower_scheduled_principal > 0.);
    assert!(cured.recognition.iter().map(|r| r.interest).sum::<f64>() > 0.);
    close(cured.borrower_principal_conservation_error, 0.);
}

#[test]
fn agency_advances_do_not_reduce_unpaid_borrower_principal() {
    let mut r = request();
    zero(&mut r);
    r.opening_probability = [0., 0., 1.];
    r.opening_borrower_principal = [0., 0., 100.];
    r.opening_investor_principal = [0., 0., 100.];
    r.ownership = MortgageOwnership::Agency {
        issuer: "supplied-issuer".into(),
        guarantee: id("supplied-guarantee"),
        remittance_lag_days: 3,
        default_buyout_lag_days: 15,
        servicing_and_guarantee_spread: 0.00365,
        default_coupon_policy: DefaultCouponPolicy::FrozenAtDefault,
    };
    let result = project(&r).unwrap();
    close(result.points[0].borrower_principal[2], 100.);
    close(result.points[0].investor_principal[2], 66.1);
    close(result.guarantee_principal_advance, 67.8339);
    close(result.guarantee_interest_advance, 0.09 + 66.1 * 0.0009);
    assert!(result.borrower_cashflows.is_empty());
    assert_eq!(result.cashflows[0].ordinal, r.as_of_ordinal + 13);
    close(result.investor_principal_conservation_error, 0.);
}

#[test]
fn agency_default_buyout_and_guaranteed_coupon_are_distinct_from_borrower_recovery() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 10;
    r.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.ownership = MortgageOwnership::Agency {
        issuer: "supplied-issuer".into(),
        guarantee: id("supplied-guarantee"),
        remittance_lag_days: 3,
        default_buyout_lag_days: 15,
        servicing_and_guarantee_spread: 0.,
        default_coupon_policy: DefaultCouponPolicy::FrozenAtDefault,
    };
    let result = project(&r).unwrap();
    let d = 100. * (1. - (-0.2_f64 * 10. / 365.).exp());
    close(result.defaults[0].borrower_principal, d);
    assert_eq!(
        result.defaults[0].investor_buyout_ordinal,
        Some(r.as_of_ordinal + 25)
    );
    close(
        result
            .entitlements
            .iter()
            .filter(|e| e.kind == "agency_default_buyout")
            .map(|e| e.principal)
            .sum(),
        d,
    );
    close(
        result
            .borrower_cashflows
            .iter()
            .filter(|e| e.kind == "borrower_default_recovery")
            .map(|e| e.principal)
            .sum(),
        d * 0.6,
    );
    assert!(result
        .entitlements
        .iter()
        .any(|e| e.kind == "agency_default_guaranteed_coupon"
            && e.entitlement_ordinal == r.as_of_ordinal + 20));
    assert!(!result.cashflows.iter().any(|e| e.kind.contains("recovery")));
    assert!(result
        .cashflows
        .iter()
        .any(|e| e.kind == "agency_default_buyout" && e.ordinal == r.as_of_ordinal + 28));
}

#[test]
fn arm_initial_subsequent_lifetime_caps_and_floor_use_explicit_fixing_kind() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 30;
    let resets = vec![
        reset(&r, 10, 0.2, true),
        reset(&r, 20, 0.2, true),
        reset(&r, 25, -0.5, true),
        reset(&r, 27, -0.5, true),
        reset(&r, 28, -0.5, true),
        reset(&r, 29, -0.5, true),
        reset(&r, 30, -0.5, true),
    ];
    arm(&mut r, resets, PaymentRecast::RetainOriginal);
    let result = project(&r).unwrap();
    close(result.resets[0].new_rate, 0.0565);
    close(result.resets[1].new_rate, 0.0665);
    close(result.resets[2].new_rate, 0.0565);
    close(result.resets[3].new_rate, 0.0465);
    close(result.resets[6].new_rate, 0.02);
    assert!(result.resets.iter().all(|p| p.projected));
    assert_eq!(result.original_payment, result.resets[0].new_payment);
}

#[test]
fn arm_recast_mid_period_keeps_old_accrued_coupon_in_first_remaining_payment() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 30;
    let res = reset(&r, 5, 0.1, true);
    arm(&mut r, vec![res], PaymentRecast::ReamortizeRemaining);
    let result = project(&r).unwrap();
    let rate = 0.0565;
    let taus = [5. / 365., 10. / 365., 10. / 365.];
    let discounts = [
        1. / (1. + rate * taus[0]),
        1. / ((1. + rate * taus[0]) * (1. + rate * taus[1])),
        1. / ((1. + rate * taus[0]) * (1. + rate * taus[1]) * (1. + rate * taus[2])),
    ];
    let expected = (100. + 0.05 * discounts[0]) / discounts.iter().sum::<f64>();
    close(result.resets[0].new_payment, expected);
    close(result.original_payment, 34.);
    close(result.points.last().unwrap().borrower_principal[0], 0.);
    close(result.borrower_scheduled_principal, 100.);
}

#[test]
fn seasoning_burnout_and_measure_identity_are_retained_without_empirical_claim() {
    let mut r = request();
    zero(&mut r);
    r.origination_ordinal = r.as_of_ordinal - 10;
    r.refinance.refinance_seasoning_days = 100;
    r.refinance.turnover_seasoning_days = 100;
    r.path[0].turnover_hazard_per_year = 0.2;
    r.path[0].refinancing_market_rate = 0.;
    r.refinance.incentive_slope = 2.;
    r.refinance.incentive_threshold = 0.;
    r.refinance.burnout_sensitivity = 3.;
    r.opening_burnout_incentive_years = 0.5;
    r.identity.measure = CashflowMeasure::Pricing;
    let result = project(&r).unwrap();
    close(result.points[0].turnover_hazard_per_year, 0.02);
    close(
        result.points[0].refinance_hazard_per_year,
        2. * 0.0365 * 0.1 * (-1.5_f64).exp(),
    );
    assert_eq!(result.identity.measure, CashflowMeasure::Pricing);
    assert_eq!(result.original_payment, 34.);
    assert_eq!(result.points[0].seasoning_days, 20);
    close(
        result.points[0].burnout_incentive_years,
        0.5 + 0.0365 * 10. / 365.,
    );
}

#[test]
fn missing_model_selection_future_publication_and_invalid_tail_fail_closed() {
    let r = request();
    let mut raw = serde_json::to_value(&r).unwrap();
    raw.as_object_mut().unwrap().remove("original_payment");
    assert!(serde_json::from_value::<MortgageRequest>(raw).is_err());
    let mut bad = request();
    bad.schema = "legacy-mortgage".into();
    assert!(project(&bad).is_err());
    bad = request();
    bad.path[0].credit_hazards_per_year[0][4] = 0.1;
    assert!(project(&bad).is_err());
    bad = request();
    bad.complete_recovery_tails[0][0].weight = 0.5;
    assert!(project(&bad).is_err());
    bad = request();
    let mut res = reset(&bad, 10, 0.03, false);
    if let MortgageIndexInput::Observed { fixing, .. } = &mut res.input {
        fixing.publication_unix_seconds = bad.publication_cutoff_unix_seconds + 1;
    }
    arm(&mut bad, vec![res], PaymentRecast::RetainOriginal);
    assert!(project(&bad).is_err());
    bad = request();
    bad.identity.measure = CashflowMeasure::Contractual;
    assert!(project(&bad).is_err());
    let raw: Value = json!({"schema":"mortgage-state-1"});
    assert!(serde_json::from_value::<MortgageRequest>(raw).is_err());
}

#[test]
fn agency_default_coupon_on_default_date_is_paid_on_that_entitlement_date() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 10;
    r.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.ownership = MortgageOwnership::Agency {
        issuer: "supplied".into(),
        guarantee: id("guarantee"),
        remittance_lag_days: 2,
        default_buyout_lag_days: 15,
        servicing_and_guarantee_spread: 0.,
        default_coupon_policy: DefaultCouponPolicy::FrozenAtDefault,
    };
    let result = project(&r).unwrap();
    let due = r.as_of_ordinal + 10;
    let accrued = result.defaults[0].investor_accrued_interest;
    close(
        result
            .entitlements
            .iter()
            .filter(|e| {
                e.kind == "agency_default_guaranteed_coupon" && e.entitlement_ordinal == due
            })
            .map(|e| e.interest)
            .sum(),
        accrued,
    );
    close(
        result
            .entitlements
            .iter()
            .filter(|e| {
                e.kind == "agency_default_guaranteed_coupon" && e.entitlement_ordinal == due + 10
            })
            .map(|e| e.interest)
            .sum(),
        result.defaults[0].investor_principal * r.current_note_rate * 10. / 365.,
    );
}

#[test]
fn large_currency_scale_preserves_principal_without_renormalizing() {
    let mut r = request();
    let scale = 1e10;
    r.original_principal *= scale;
    r.original_payment *= scale;
    r.current_payment *= scale;
    r.opening_contractual_principal *= scale;
    for b in r
        .opening_borrower_principal
        .iter_mut()
        .chain(&mut r.opening_investor_principal)
    {
        *b *= scale;
    }
    let result = project(&r).unwrap();
    assert!(result.borrower_principal_conservation_error.abs() < 0.1);
    assert!(result.investor_principal_conservation_error.abs() < 0.1);
}

#[test]
fn complete_agency_coupon_tails_are_work_bounded_even_at_zero_coupon() {
    let mut r = request();
    zero(&mut r);
    r.horizon_days = 4096;
    r.payment_dates = (1..=4096).map(|d| r.as_of_ordinal + d).collect();
    r.current_note_rate = 0.;
    r.original_payment = 0.01;
    r.current_payment = 0.01;
    r.borrower_collection_fraction = [0.; 3];
    r.path[0].credit_hazards_per_year[0][3] = 0.2;
    r.ownership = MortgageOwnership::Agency {
        issuer: "supplied".into(),
        guarantee: id("guarantee"),
        remittance_lag_days: 0,
        default_buyout_lag_days: 4096,
        servicing_and_guarantee_spread: 0.,
        default_coupon_policy: DefaultCouponPolicy::FrozenAtDefault,
    };
    assert!(project(&r).unwrap_err().contains("work"));
}
