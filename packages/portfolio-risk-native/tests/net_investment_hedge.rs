//! net-investment-hedge-1: translation, forward and debt net-investment hedges, the ASU 2025-09
//! dual hedge and disposal, with hand-computed values from the example.
use portfolio_risk_native::products::derivatives::net_investment::{evaluate, Request, Result};
use serde_json::{json, Value};

type Edit = Box<dyn Fn(&mut Value)>;

fn raw(name: &str) -> Value {
    let text = match name {
        "base" => include_str!("fixtures/net-investment-hedge.json"),
        _ => include_str!("fixtures/net-investment-dual.json"),
    };
    serde_json::from_str(text).unwrap()
}
fn run(v: &Value) -> std::result::Result<Result, String> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    evaluate(&r)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} vs {b}");
}
fn balance(r: &Result, account: &str) -> f64 {
    r.closing
        .iter()
        .find(|b| b.account == account)
        .map_or(0., |b| b.debit_balance)
}

#[test]
fn translation_and_debt_hedge_match_hand_values() {
    let r = run(&raw("base")).unwrap();
    let adj: Vec<f64> = r
        .translation
        .iter()
        .skip(1)
        .map(|t| t.translation_adjustment)
        .collect();
    for (a, b) in adj.iter().zip([-20.2, 40.8, -30.6, 15.52]) {
        close(*a, b);
    }
    // Debt EUR 700 hedging EUR 600: CTA is 600/700 of the spot gain until the 20% disposal
    // leaves EUR 776 of net assets against EUR 1,000 designated (scale 0.776).
    let debt: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "nih-debt")
        .collect();
    for (p, cta) in debt.iter().zip([12., -24., 18., -9.312]) {
        close(p.cta, cta);
        close(p.cta + p.excess_earnings, p.included_change);
    }
    close(debt[3].hedged_effective, 465.6);
    // The forward's spot component is EUR 400 times the spot change; the 0.776 scale applies in May.
    let fwd: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "nih-fwd")
        .collect();
    for (p, inc) in fwd.iter().zip([8., -16., 12., -8.]) {
        close(p.included_change, inc);
    }
    close(fwd[3].designated_fraction, 0.776);
    assert!(r.periods.iter().all(|p| p.assessment_passed == Some(true)));
}

#[test]
fn amortization_disposal_release_and_balanced_tails() {
    let raw = raw("base");
    let r = run(&raw).unwrap();
    let k = raw["instruments"][0]["forward_rate"].as_f64().unwrap();
    // Amortized forward points: the whole excluded change to maturity, 400 * (K - S0), lands in
    // earnings (scaled by the designated fraction in each period).
    let fwd: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "nih-fwd")
        .collect();
    let total = 400. * (k - 1.10);
    let days = [31., 29., 31., 30.];
    let phi = [1., 1., 1., 0.776];
    for i in 0..4 {
        close(fwd[i].excluded_earnings, phi[i] * total * days[i] / 121.);
    }
    // Settlement at maturity is 400 * (K - S_T) in cash, and the forward account closes.
    close(balance(&r, "fx_forward"), 0.);
    let settle: f64 = r
        .journal
        .iter()
        .filter(|p| p.event == "fx_forward_settlement" && p.account == "cash")
        .map(|p| p.debit - p.credit)
        .sum();
    close(settle, 400. * (k - 1.11));
    // The 20% disposal releases 20% of translation CTA (15.0 after April) and of hedge CTA.
    let release: Vec<_> = r
        .journal
        .iter()
        .filter(|p| p.event == "cta_reclassification")
        .collect();
    let translation_release = release
        .iter()
        .find(|p| p.account == "cta_translation")
        .unwrap();
    close(translation_release.debit - translation_release.credit, 3.0);
    let gain = r
        .journal
        .iter()
        .find(|p| p.event == "disposal" && p.account == "pnl:disposal_gain_loss")
        .unwrap();
    close(gain.debit - gain.credit, 0.2 * 1057.3 - 230.);
    // Every transaction balances, and so does the closing trial balance.
    let mut by_tx = std::collections::BTreeMap::<&str, f64>::new();
    for p in &r.journal {
        *by_tx.entry(&p.transaction_id).or_default() += p.debit - p.credit;
    }
    assert!(by_tx.values().all(|v| v.abs() < 1e-9));
    close(r.closing.iter().map(|b| b.debit_balance).sum(), 0.);
    close(balance(&r, "net_investment_foreign_operation"), 776. * 1.11);
    assert!(!r.accounting_standard_approval && !r.debt_interest_included && !r.dual_hedge_applied);
}

#[test]
fn dual_hedge_keeps_basis_remeasurement_out_of_the_net_investment_hedge() {
    let r = run(&raw("dual")).unwrap();
    assert!(r.dual_hedge_applied);
    let periods: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "nih-dual")
        .collect();
    assert_eq!(periods.len(), 3);
    let spots = [1.10, 1.08, 1.12, 1.09];
    for (i, p) in periods.iter().enumerate() {
        // While the fair value hedge is active only the EUR 100 principal is in the hedge.
        close(p.included_change, -100. * (spots[i + 1] - spots[i]));
        close(p.cta, p.included_change);
    }
    // The basis exists after the first month and its spot remeasurement goes to earnings.
    assert!(periods[1].basis_remeasurement != 0. || periods[2].basis_remeasurement != 0.);
    let earnings: f64 = r
        .journal
        .iter()
        .filter(|p| p.account == "pnl:fx_basis_remeasurement")
        .map(|p| p.credit - p.debit)
        .sum();
    close(
        earnings,
        periods.iter().map(|p| p.basis_remeasurement).sum(),
    );
}

#[test]
fn after_the_fair_value_hedge_ends_its_basis_joins_the_net_investment_hedge() {
    let mut v = raw("dual");
    let hedge = &mut v["relationships"][0]["dual_fair_value_hedge"]["request"];
    let feb15 = 738931;
    hedge["events"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["ordinal"].as_i64().unwrap() < feb15);
    hedge["events"].as_array_mut().unwrap().push(json!({
        "event_id": "dedesignate-fvh-1", "relationship_id": "fvh-1", "ordinal": feb15, "sequence": 0,
        "kind": "discontinue", "review": {"id": "fvh-dedesignation", "revision": "r1"},
        "reason": "dedesignation"
    }));
    let r = run(&v).unwrap();
    let p: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "nih-dual")
        .collect();
    // March onward the basis is inside the hedge: no basis earnings, included change carries it.
    assert_eq!(p[2].basis_remeasurement, 0.);
    assert!((p[2].included_change - (-100. * (1.09 - 1.12))).abs() > 1e-9);
    close(p[2].cta + p[2].excess_earnings, p[2].included_change);
}

#[test]
fn invalid_requests_fail_before_replay() {
    let cases: Vec<Edit> = vec![
        Box::new(|r| r["foreign_currency"] = json!("USD")),
        Box::new(|r| r["dates"][2]["ordinal"] = json!(738886)),
        Box::new(|r| r["dates"][1]["spot"] = json!(0.)),
        Box::new(|r| r["relationships"][0]["excluded_component"] = Value::Null),
        Box::new(|r| r["relationships"][1]["method"] = json!("forward")),
        Box::new(|r| r["relationships"][1]["instrument_position_id"] = json!("fwd-1")),
        Box::new(|r| r["instruments"][0]["side"] = json!("buy_foreign")),
        Box::new(|r| r["instruments"][0]["maturity_ordinal"] = json!(738900)),
        Box::new(|r| {
            r["dates"][1]
                .as_object_mut()
                .unwrap()
                .remove("foreign_discount");
        }),
        Box::new(|r| r["events"][0]["fraction"] = json!(1.5)),
        Box::new(|r| r["events"][0]["kind"] = json!("dedesignate")),
        Box::new(|r| r["foreign_operation"]["movements"][0]["ordinal"] = json!(738900)),
        Box::new(|r| r["policy"]["assessment_lower_bound"] = json!(1.2)),
        Box::new(|r| {
            r["relationships"][1]["dual_fair_value_hedge"] =
                json!({"relationship_id": "x", "request": {}})
        }),
        Box::new(|r| r["unknown"] = json!(1)),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut v = raw("base");
        change(&mut v);
        assert!(run(&v).is_err(), "case {i} must be refused");
    }
    let mut v = raw("dual");
    v["policy"]
        .as_object_mut()
        .unwrap()
        .remove("dual_hedge_adoption");
    assert!(run(&v).is_err(), "a dual hedge needs reviewed adoption");
    let mut v = raw("dual");
    v["relationships"][0]["dual_fair_value_hedge"]["request"]["currency"] = json!("USD");
    assert!(
        run(&v).is_err(),
        "the fair value hedge must run in the foreign currency"
    );
}
