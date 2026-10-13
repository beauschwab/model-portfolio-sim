use portfolio_model_core::cashflow::{ordinal_from_ymd, ActualDayCount, CashflowMeasure};
use portfolio_model_core::floating_rate::Direction;
use portfolio_risk_native::term_enhancements::{cd::*, Identity, MODEL_VERSION, SCHEMA};
fn d(n: u32) -> i32 {
    ordinal_from_ymd(2026, 1, n).unwrap()
}
fn id(s: &str) -> Identity {
    Identity {
        id: s.into(),
        revision: "r1".into(),
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn request() -> CdElectionRequest {
    CdElectionRequest {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        measure: CashflowMeasure::Physical,
        currency: "USD".into(),
        monetary_unit: "currency_units".into(),
        direction: Direction::Receipt,
        scenario_id: "elected".into(),
        election_source: id("explicit-election"),
        policy: CdPolicy {
            identity: id("contract-penalty-policy"),
            channel: Channel::Retail,
            early_withdrawal_permitted: true,
            partial_withdrawal_permitted: true,
            brokered_contract_allows_withdrawal: false,
            earliest_withdrawal_ordinal: d(1),
            minimum_remaining_principal: 0.,
            penalty: PenaltyRule::InterestDays {
                annual_penalty_rate: 0.05,
                days: 90,
                denominator: 365,
            },
            penalty_limit: PenaltyLimit::PrincipalAndAccruedInterest,
            penalty_cap: None,
            penalty_waivers: vec![],
            maturity_grace_days: 2,
            grace_annual_rate: 0.1825,
            renewal_permitted: true,
            switching_permitted: true,
        },
        opening: CdState {
            contract: id("cd-original"),
            position_id: "cd:old".into(),
            principal: 100.,
            accrued_interest: 0.,
            last_accrual_ordinal: d(1),
            maturity_ordinal: d(11),
            annual_rate: 0.365,
            day_count: ActualDayCount::Act365Fixed,
        },
        election_ordinal: d(6),
        reporting_end_ordinal: d(7),
        tail_end_ordinal: d(31),
        election: Election::Withdraw { principal: 40. },
    }
}
fn new_contract(name: &str, maturity: i32, rate: f64) -> RenewalContract {
    RenewalContract {
        contract: id(name),
        position_id: name.into(),
        maturity_ordinal: maturity,
        annual_rate: rate,
        day_count: ActualDayCount::Act365Fixed,
    }
}
fn maturity(r: &mut CdElectionRequest, p: [f64; 3], interest: [f64; 3]) {
    r.election = Election::Maturity {
        cash_out_principal: p[0],
        renew_principal: p[1],
        switch_principal: p[2],
        cash_out_interest: interest[0],
        renew_interest: interest[1],
        switch_interest: interest[2],
        renewal: if p[1] + interest[1] != 0. {
            Some(Box::new(new_contract("cd:renew", d(21), 0.1)))
        } else {
            None
        },
        switching: if p[2] + interest[2] != 0. {
            Some(Box::new(new_contract("cd:switch", d(31), 0.2)))
        } else {
            None
        },
    };
}
#[test]
fn partial_withdrawal_separates_gross_face_penalty_and_cash_with_complete_tail() {
    let r = request();
    let x = step_cd(&r).unwrap();
    let penalty = 40. * 0.05 * 90. / 365.;
    close(x.earned_interest_delta, 0.5);
    close(x.gross_principal_removed, 40.);
    close(x.penalty, penalty);
    close(x.penalty_from_accrued_interest, 0.2);
    close(x.penalty_from_principal, penalty - 0.2);
    close(x.principal_cash_returned, 40. - (penalty - 0.2));
    close(x.interest_cash_paid, 0.);
    close(x.net_external_cash, 40.2 - penalty);
    close(x.closing.principal, 60.);
    close(x.closing.accrued_interest, 0.3);
    close(x.settlement_schedule.events[0].components.principal, 40.);
    close(x.settlement_schedule.events[0].components.fees, -penalty);
    close(x.remaining_schedule.events[0].components.principal, 60.);
    close(x.remaining_schedule.events[0].components.cash_interest, 0.6);
    assert_eq!(x.remaining_schedule.events[0].payment_ordinal, d(11));
    assert!(x.remaining_schedule.events[0].payment_ordinal > r.reporting_end_ordinal);
    close(
        x.gross_principal_removed + x.closing.principal,
        r.opening.principal,
    );
}
#[test]
fn penalty_bounds_zero_penalty_waivers_and_full_withdrawal() {
    let mut r = request();
    r.policy.penalty_limit = PenaltyLimit::AccruedInterestOnly;
    let x = step_cd(&r).unwrap();
    close(x.penalty, 0.2);
    close(x.principal_cash_returned, 40.);
    close(x.penalty_from_principal, 0.);
    r.policy.penalty_limit = PenaltyLimit::PrincipalAndAccruedInterest;
    r.policy.penalty_cap = Some(0.1);
    close(step_cd(&r).unwrap().penalty, 0.1);
    r.policy.penalty_cap = None;
    r.policy.penalty_waivers = vec![WaiverWindow {
        start_ordinal: d(6),
        end_ordinal: d(6),
    }];
    close(step_cd(&r).unwrap().penalty, 0.);
    r.policy.penalty_waivers.clear();
    r.policy.penalty = PenaltyRule::PrincipalFraction { fraction: 2. };
    r.election = Election::Withdraw { principal: 100. };
    let all = step_cd(&r).unwrap();
    close(all.penalty, 100.5);
    close(all.net_external_cash, 0.);
    close(all.closing.principal, 0.);
    assert!(all.remaining_schedule.events.is_empty());
    r.policy.penalty = PenaltyRule::None {};
    close(step_cd(&r).unwrap().net_external_cash, 100.5);
}
#[test]
fn negative_coupon_is_a_signed_interest_right_and_does_not_make_negative_penalty() {
    let mut r = request();
    r.opening.annual_rate = -0.365;
    r.policy.penalty_limit = PenaltyLimit::AccruedInterestOnly;
    r.election = Election::Withdraw { principal: 100. };
    let x = step_cd(&r).unwrap();
    close(x.earned_interest_delta, -0.5);
    close(x.interest_cash_paid, -0.5);
    close(x.penalty, 0.);
    close(x.net_external_cash, 99.5);
    r.direction = Direction::Payment;
    close(step_cd(&r).unwrap().net_external_cash, -99.5);
}
#[test]
fn channel_partial_lockout_minimum_and_grace_restrictions_are_explicit() {
    let mut r = request();
    r.policy.channel = Channel::Brokered;
    assert!(step_cd(&r).is_err());
    r.policy.brokered_contract_allows_withdrawal = true;
    step_cd(&r).unwrap();
    r = request();
    r.policy.partial_withdrawal_permitted = false;
    assert!(step_cd(&r).is_err());
    r.election = Election::Withdraw { principal: 100. };
    step_cd(&r).unwrap();
    r = request();
    r.policy.earliest_withdrawal_ordinal = d(7);
    assert!(step_cd(&r).is_err());
    r.policy.earliest_withdrawal_ordinal = d(6);
    step_cd(&r).unwrap();
    r = request();
    r.policy.minimum_remaining_principal = 61.;
    assert!(step_cd(&r).is_err());
    r.policy.minimum_remaining_principal = 60.;
    step_cd(&r).unwrap();
    r = request();
    r.election_ordinal = d(11);
    r.reporting_end_ordinal = d(11);
    assert!(step_cd(&r).is_err());
}
#[test]
fn exact_maturity_cashout_renewal_switch_and_grace_conserve_net_funding() {
    let mut r = request();
    r.election_ordinal = d(13);
    r.reporting_end_ordinal = d(13);
    // Original interest 1, explicit two-day grace interest .1.
    maturity(&mut r, [20., 50., 30.], [0.22, 0.55, 0.33]);
    let x = step_cd(&r).unwrap();
    close(x.earned_interest_delta, 1.1);
    close(x.net_external_cash, 20.22);
    close(x.closing.principal, 0.);
    close(x.closing.accrued_interest, 0.);
    close(x.new_contracts[0].principal, 50.55);
    close(x.new_contracts[1].principal, 30.33);
    close(
        x.new_contracts.iter().map(|s| s.principal).sum::<f64>() + x.net_external_cash,
        101.1,
    );
    assert_eq!(x.settlement_schedule.events.len(), 3);
    assert_eq!(x.remaining_schedule.events.len(), 2);
    close(
        x.remaining_schedule.events[0].components.cash_interest,
        50.55 * 0.1 * 8. / 365.,
    );
    assert_eq!(x.remaining_schedule.events[1].payment_ordinal, d(31));
    r.direction = Direction::Payment;
    close(step_cd(&r).unwrap().net_external_cash, -20.22);
    r.election_ordinal = d(14);
    r.reporting_end_ordinal = d(14);
    assert!(step_cd(&r).is_err());
}
#[test]
fn final_date_cashout_and_zero_rate_do_not_infer_renewal() {
    let mut r = request();
    r.election_ordinal = d(11);
    r.reporting_end_ordinal = d(11);
    r.opening.annual_rate = 0.;
    maturity(&mut r, [100., 0., 0.], [0., 0., 0.]);
    let x = step_cd(&r).unwrap();
    close(x.net_external_cash, 100.);
    assert!(x.new_contracts.is_empty());
    assert!(x.remaining_schedule.events.is_empty());
    assert_eq!(x.settlement_schedule.events[0].payment_ordinal, d(11));
}
#[test]
fn invalid_elections_policy_ids_units_tails_and_nonfinite_inputs_fail_closed() {
    let mut r = request();
    r.election_ordinal = d(11);
    r.reporting_end_ordinal = d(11);
    maturity(&mut r, [20., 50., 29.], [0.2, 0.5, 0.3]);
    assert!(step_cd(&r).is_err());
    maturity(&mut r, [20., 50., 30.], [0.2, 0.5, 0.2]);
    assert!(step_cd(&r).is_err());
    maturity(&mut r, [20., 50., 30.], [0.2, 0.5, 0.3]);
    r.policy.renewal_permitted = false;
    assert!(step_cd(&r).is_err());
    r.policy.renewal_permitted = true;
    r.tail_end_ordinal = d(20);
    assert!(step_cd(&r).is_err());
    r = request();
    r.measure = CashflowMeasure::Pricing;
    assert!(step_cd(&r).is_err());
    r = request();
    r.monetary_unit = "millions".into();
    assert!(step_cd(&r).is_err());
    r = request();
    r.opening.annual_rate = f64::NAN;
    assert!(step_cd(&r).is_err());
    r = request();
    r.election = Election::Withdraw { principal: 101. };
    assert!(step_cd(&r).is_err());
    r = request();
    r.policy.identity.id.clear();
    assert!(step_cd(&r).is_err());
    r = request();
    r.policy.penalty = PenaltyRule::InterestDays {
        annual_penalty_rate: 0.05,
        days: 90,
        denominator: 366,
    };
    assert!(step_cd(&r).is_err());
}
