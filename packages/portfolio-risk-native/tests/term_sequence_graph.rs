#[path = "common/cd_sequence_graph.rs"]
mod common;
use common::*;
use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::{
    advanced_graph::Role,
    term_enhancements::cd_sequence,
    term_product_graph::{self as graph, SourceProjection, TermGraph},
};
use serde_json::{json, Value};
fn result(v: Value) -> graph::ResultData {
    graph::analyze(&req(v)).unwrap()
}
fn error(v: Value) -> String {
    graph::analyze(&req(v)).err().unwrap()
}

#[test]
fn empty_sequence_full_tail_and_native_risk_are_hand_priced() {
    let out = result(raw());
    let m = &out.metrics[0];
    close(m.pv, -101.);
    let t = 10. / 365.;
    close(
        m.supplied_history_dv01,
        -101. * ((0.0001_f64 * t).exp() - (-0.0001_f64 * t).exp()) / 2.,
    );
    close(m.physical_tail.principal_cash, -100.);
    close(m.physical_tail.interest_cash, -1.);
    assert_eq!(out.native_sources.len(), 4);
    assert_eq!(out.original_native_sources.as_ref().unwrap().len(), 4);
    let status = out.sequence_policy.unwrap();
    assert_eq!(status.oas_calibration_calls, 0);
    assert_eq!(status.single_election_fallbacks, 0);
    assert!(
        !status.capital_ftp_eligible
            && !status.accounting_adoption
            && !status.saved_library_cohort_adoption
    );
    for source in out.native_sources {
        let graph::ProductHistory::CdSequence { request } = source.product else {
            panic!()
        };
        let exact = cd_sequence::prepare(&request, &Control::new(5000).unwrap()).unwrap();
        let SourceProjection::CdSequence { result } = source.projection else {
            panic!()
        };
        assert_eq!(
            serde_json::to_vec(&exact).unwrap(),
            serde_json::to_vec(&result).unwrap()
        );
        assert_eq!(result.schedule.events.len(), 1);
        assert!(result.action_receipts.is_empty());
    }
}

#[test]
fn multiple_withdrawals_conserve_piecewise_accrual_and_exclude_superseded_tails() {
    let mut v = raw();
    partial(&mut v);
    let out = result(v);
    let m = &out.metrics[0];
    close(m.pv, -100.76);
    close(m.physical_tail.principal_cash, -100.);
    close(m.physical_tail.interest_cash, -0.76);
    close(m.physical_tail.cd_principal_extinguished, 60.);
    close(m.physical_tail.cd_interest_accrued_through_election, 0.68);
    let q = out
        .native_sources
        .iter()
        .find(|s| s.role == Role::Pricing)
        .unwrap();
    let SourceProjection::CdSequence { result } = &q.projection else {
        panic!()
    };
    assert_eq!(result.action_receipts.len(), 2);
    assert_eq!(result.schedule.events.len(), 3);
    for (e, (day, p, i)) in result.schedule.events.iter().zip([
        (739622, -40., -0.2),
        (739625, -20., -0.16),
        (739627, -40., -0.4),
    ]) {
        assert_eq!(e.payment_ordinal, day);
        close(e.components.principal, p);
        close(e.components.cash_interest, i);
    }
    // The native intermediate previews repay 60 and 40 again; they are not price cash.
    assert_eq!(
        result.action_receipts[0]
            .native
            .remaining_schedule
            .events
            .len(),
        1
    );
}

#[test]
fn renewal_and_subsequent_withdrawal_use_authorized_terms_and_unsuperseded_cash() {
    let mut v = raw();
    let old = policy();
    let target = json!({"contract":{"id":"renewed-contract","revision":"r1"},"position_id":"renewed","maturity_ordinal":739637,"annual_rate":0.73,"day_count":"act365_fixed"});
    let mut new = old.clone();
    new["position_id"] = json!("renewed");
    new["contract"] = target["contract"].clone();
    new["policy"]["identity"]["id"] = json!("renewed-policy");
    v["positions"][0]["known"]["contract"]["policies"] = json!([old, new]);
    v["positions"][0]["known"]["contract"]["authorized_new_contracts"] = json!([target]);
    all(&mut v, |q| {
        q["policies"] = json!([old, new]);
        q["actions"] = json!([
  {"action_id":"renew","position_id":"cd:example","ordinal":739627,"source":{"id":"renew","revision":"r1"},"election":{"kind":"maturity","cash_out_principal":40.,"renew_principal":60.,"switch_principal":0.,"cash_out_interest":1.,"renew_interest":0.,"switch_interest":0.,"renewal":target,"switching":null}},
  {"action_id":"later-withdraw","position_id":"renewed","ordinal":739632,"source":{"id":"later-withdraw","revision":"r1"},"election":{"kind":"withdraw","principal":20.}}]);
    });
    v["original_quotes"][0]["signed_cash_price"] = json!(-102.);
    let x = result(v.clone());
    close(x.metrics[0].pv, -102.);
    close(x.metrics[0].physical_tail.cd_new_contract_principal, 60.);
    let SourceProjection::CdSequence { result: q } = &x.native_sources[1].projection else {
        panic!()
    };
    assert_eq!(q.schedule.events.len(), 4);
    let expected = [(-100., -1.), (60., 0.), (-20., -0.2), (-40., -0.8)];
    for (e, (p, i)) in q.schedule.events.iter().zip(expected) {
        close(e.components.principal, p);
        close(e.components.cash_interest, i);
    }
    assert_eq!(q.schedule.events.last().unwrap().payment_ordinal, 739637);
    // Changing only a current renewal term is not authorized by the known revision.
    v["positions"][0]["histories"][0]["pricing"][0]["product"]["request"]["actions"][0]
        ["election"]["renewal"]["annual_rate"] = json!(0.74);
    v["positions"][0]["histories"][0]["pricing"][0]["product"]["request"]["actions"][0]["source"]
        ["id"] = json!("unauthorized-rate-source");
    assert!(error(v).contains("authorized renewal/switch terms changed"));
}

#[test]
fn zero_principal_after_full_withdrawal_keeps_nonzero_ieee_earned_right_without_clamp() {
    let mut v = raw();
    let p = 23841.4808604056;
    let accrued = 7.075336659527932;
    let policy = policy();
    v["positions"][0]["known"]["contract"]["opening"]["principal"] = json!(p);
    v["positions"][0]["known"]["contract"]["opening"]["accrued_interest"] = json!(accrued);
    v["positions"][0]["known"]["contract"]["opening"]["annual_rate"] = json!(0.);
    v["positions"][0]["known"]["contract"]["policies"] = json!([policy]);
    all(&mut v, |q| {
        q["opening"]["principal"] = json!(p);
        q["opening"]["accrued_interest"] = json!(accrued);
        q["opening"]["annual_rate"] = json!(0.);
        q["policies"] = json!([policy]);
        q["actions"] = json!([{"action_id":"all","position_id":"cd:example","ordinal":739622,"source":{"id":"full-withdraw","revision":"r1"},"election":{"kind":"withdraw","principal":p}}]);
    });
    v["original_quotes"][0]["signed_cash_price"] = json!(-p - accrued);
    let x = result(v);
    let SourceProjection::CdSequence { result: q } = &x.native_sources[1].projection else {
        panic!()
    };
    let residual = accrued - accrued * p / p;
    assert!(residual > 0.);
    assert_eq!(q.schedule.events.len(), 2);
    assert_eq!(q.live_before_terminal_settlement[0].principal, 0.);
    assert_eq!(
        q.live_before_terminal_settlement[0].accrued_interest,
        residual
    );
    assert_eq!(q.schedule.events[1].components.principal, -0.);
    assert_eq!(q.schedule.events[1].components.cash_interest, -residual);
}

#[test]
fn same_asof_settlement_excluded_from_quote_and_future_pv_retained_in_opening() {
    let mut v = raw();
    partial(&mut v);
    all(&mut v, |q| {
        q["actions"].as_array_mut().unwrap().truncate(1);
        q["actions"][0]["ordinal"] = json!(739617);
    });
    v["original_quotes"][0]["signed_cash_price"] = json!(-60.6);
    let x = result(v);
    close(x.metrics[0].pv, -60.6);
    close(x.metrics[0].opening_actions.principal_cash, -40.);
    close(x.metrics[0].physical_tail.principal_cash, -60.);
}

#[test]
fn zero_weight_sources_are_retained_and_invalid_zero_sources_are_not_pruned() {
    let mut v = raw();
    for current in [false, true] {
        let set = if current {
            &mut v["positions"][0]["histories"][0]
        } else {
            &mut v["original_base"]["histories"][0]["histories"]
        };
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            let mut extra = set[role][0].clone();
            extra["path_id"] = json!("zero");
            extra["weight"] = json!(0.);
            set[role].as_array_mut().unwrap().push(extra);
        }
    }
    let x = result(v.clone());
    assert_eq!(x.native_sources.len(), 8);
    assert_eq!(x.original_native_sources.unwrap().len(), 8);
    close(x.metrics[0].pv, -101.);
    v["positions"][0]["histories"][0]["physical"][1]["product"]["request"]["opening"]
        ["accrued_interest"] = json!(-101.);
    assert!(error(v).contains("known terms"));
}

#[test]
fn original_quote_binds_only_original_cash_current_behavior_and_curve_reprice_without_refit() {
    let mut v = raw();
    let g = TermGraph::new(&req(v.clone())).unwrap();
    let p = policy();
    v["positions"][0]["known"]["contract"]["policies"] = json!([p]);
    // A known-policy change is immutable; build a common authorization universe first.
    let mut anchored = raw();
    partial(&mut anchored);
    let g2 = TermGraph::new(&req(anchored.clone())).unwrap();
    for s in anchored["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
    {
        roles(s, &|q| {
            q["actions"].as_array_mut().unwrap().truncate(1);
            q["actions"][0]["source"]["id"] = json!("current-first-50");
            q["actions"][0]["election"]["principal"] = json!(50.);
        });
    }
    anchored["scenarios"][0]["collateral_curve"]["discount_factors"][1] = json!((-0.07_f64).exp());
    anchored["scenarios"][0]["collateral_curve"]["source_revision"] = json!("current-r2");
    let x = g2.reprice(&req(anchored)).unwrap();
    let expected = -50.25 * (-0.02_f64 * 5. / 365.).exp() - 50.5 * (-0.02_f64 * 10. / 365.).exp();
    close(x.metrics[0].pv, expected);
    assert_eq!(
        serde_json::to_vec(g2.base_anchor()).unwrap(),
        serde_json::to_vec(&x.base_anchor).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(&g2.snapshot().original_native_sources).unwrap(),
        serde_json::to_vec(&x.original_native_sources).unwrap()
    );
    // An empty current history may use the same immutable original source and OAS.
    let mut current = raw();
    current["scenarios"][0]["collateral_curve"]["discount_factors"][1] = json!((-0.15_f64).exp());
    close(
        g.reprice(&req(current)).unwrap().metrics[0].pv,
        -101. * (-0.10_f64 * 10. / 365.).exp(),
    );
}

#[test]
fn quote_original_content_locked_oas_and_signed_zero_are_not_mutable_on_warm_reprice() {
    let v = raw();
    let g = TermGraph::new(&req(v.clone())).unwrap();
    for path in [
        "quote",
        "oas",
        "original_curve",
        "original_terminal",
        "known",
    ] {
        let mut c = v.clone();
        match path {
            "quote" => c["original_quotes"][0]["signed_cash_price"] = json!(-100.),
            "oas" => c["positions"][0]["oas_target"]["base_oas"] = json!(-0.03),
            "original_curve" => {
                c["original_base"]["scenario"]["collateral_curve"]["source_revision"] =
                    json!("edited")
            }
            "original_terminal" => {
                c["original_base"]["histories"][0]["histories"]["pricing"][0]["product"]
                    ["request"]["terminal_projection_source"]["revision"] = json!("edited")
            }
            _ => {
                c["positions"][0]["known"]["contract"]["opening"]["annual_rate"] = json!(0.36);
                all(&mut c, |q| q["opening"]["annual_rate"] = json!(0.36));
            }
        }
        assert!(
            g.reprice(&req(c))
                .err()
                .unwrap()
                .contains("BASE_ANCHOR_CHANGED"),
            "{path}"
        );
    }
    let mut bad = v.clone();
    bad["original_quotes"][0]["signed_cash_price"] = json!(-100.);
    assert!(error(bad).contains("quote conflicts"));
    let mut c = v;
    c["original_base"]["scenario"]["severity"] = json!(-0.0);
    assert!(g
        .reprice(&req(c))
        .err()
        .unwrap()
        .contains("BASE_ANCHOR_CHANGED"));
}

#[test]
fn signed_coupon_and_zero_principal_earned_right_survive_reporting_cutoff() {
    for (p, a, rate, expected) in [
        (100., -0.2, -0.365, -98.8),
        (0., 2., 0., -2.),
        (100., -1., 0.365, -100.),
    ] {
        let mut v = raw();
        v["positions"][0]["known"]["contract"]["opening"]["principal"] = json!(p);
        v["positions"][0]["known"]["contract"]["opening"]["accrued_interest"] = json!(a);
        v["positions"][0]["known"]["contract"]["opening"]["annual_rate"] = json!(rate);
        all(&mut v, |q| {
            q["opening"]["principal"] = json!(p);
            q["opening"]["accrued_interest"] = json!(a);
            q["opening"]["annual_rate"] = json!(rate);
        });
        v["original_quotes"][0]["signed_cash_price"] = json!(expected);
        let x = result(v);
        close(x.metrics[0].pv, expected);
        close(x.metrics[0].physical_tail.principal_cash, -p);
        close(x.metrics[0].physical_tail.interest_cash, expected + p);
    }
}

#[test]
fn pair_order_weight_measure_terms_and_full_tail_domain_are_validated_on_every_call() {
    let v = raw();
    let g = TermGraph::new(&req(v.clone())).unwrap();
    for mutation in ["id", "weight", "measure", "currency", "tail", "source"] {
        let mut c = v.clone();
        let q = &mut c["positions"][0]["histories"][0]["pricing_up"][0];
        match mutation {
            "id" => q["path_id"] = json!("other"),
            "weight" => q["weight"] = json!(0.5),
            "measure" => q["product"]["request"]["measure"] = json!("physical"),
            "currency" => q["product"]["request"]["currency"] = json!("EUR"),
            "tail" => q["product"]["request"]["tail_end_ordinal"] = json!(739626),
            _ => q["product"]["request"]["terminal_projection_source"]["id"] = json!(""),
        }
        assert!(g.reprice(&req(c)).is_err(), "{mutation}");
    }
    let mut v = raw();
    v["positions"][0]["oas_target"] = json!({"kind":"calibrate_positive_cash","target_cash_price":101.,"source_id":"review","source_revision":"r1"});
    assert!(error(v).contains("no calibration"));
    let mut v = raw();
    v["schema"] = json!("term-product-graph-1");
    assert!(error(v).contains("explicit term-product-graph-2"));
}

#[test]
fn multi_owner_signed_reduction_retains_every_source() {
    let mut v = raw();
    let mut p = v["positions"][0].clone();
    p["position_id"] = json!("asset-cd");
    p["known"]["contract"]["opening"]["position_id"] = json!("asset-cd");
    p["known"]["contract"]["opening"]["contract"]["id"] = json!("asset-original");
    p["known"]["contract"]["direction"] = json!("receipt");
    for s in p["histories"].as_array_mut().unwrap() {
        roles(s, &|q| {
            q["direction"] = json!("receipt");
            q["opening"]["position_id"] = json!("asset-cd");
            q["opening"]["contract"]["id"] = json!("asset-original");
        });
    }
    let h = p["histories"][0].clone();
    v["positions"].as_array_mut().unwrap().push(p);
    v["original_base"]["histories"]
        .as_array_mut()
        .unwrap()
        .push(json!({"position_id":"asset-cd","histories":h}));
    v["original_quotes"].as_array_mut().unwrap().push(json!({"position_id":"asset-cd","signed_cash_price":101.,"source":{"id":"asset-quote","revision":"r1"}}));
    let x = result(v);
    assert_eq!(x.native_sources.len(), 8);
    close(x.metrics[0].pv + x.metrics[1].pv, 0.);
}

#[test]
fn active_cancellation_and_deadline_return_no_graph() {
    let r = req(raw());
    let c = Control::new(1000).unwrap();
    c.cancel();
    assert_eq!(
        scope(Some(c), || TermGraph::new(&r).map(|_| ())).unwrap_err(),
        "COMPUTE_CANCELLED"
    );
    let c = Control::new(1).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert_eq!(
        scope(Some(c), || TermGraph::new(&r).map(|_| ())).unwrap_err(),
        "COMPUTE_DEADLINE_EXCEEDED"
    );
}
