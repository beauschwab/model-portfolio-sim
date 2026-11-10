use portfolio_model_core::modeled_product_funding as funding;
use serde_json::{json, Value};

// A conveyed earned right/obligation of six is purchased for five. Historical
// service earnings are not buyer income; only the one-unit adjustment releases.
fn request(sign: f64, allocated: f64) -> funding::Request {
    let review = json!({"id":"reviewed-conveyed-fee","revision":"1"});
    let right = if sign > 0. { 18 } else { 19 };
    let adjustment = if sign > 0. { 24 } else { 25 };
    let channels = funding::SECONDARY_FEE_LANES.iter().enumerate().map(|(i,name)|
        json!({"channel":name,"coefficient":if i==right {sign as i8}else if i==adjustment {-sign as i8}else{0},"review":review}))
        .collect::<Vec<_>>();
    let states = (0..=1).map(|day| {
        let amounts = funding::SECONDARY_FEE_LANES.iter().enumerate().map(|(i,name)|
            json!({"channel":name,"amount":if day==0 && i==right {6.}else if day==0 && i==adjustment {1.}else{0.}}))
            .collect::<Vec<_>>();
        let origins: Vec<Value> = if day==0 {vec![
            json!({"origin_id":"conveyed-right","source_event_id":"transfer","channel":funding::SECONDARY_FEE_LANES[right],"amount":6.}),
            json!({"origin_id":"buyer-adjustment","source_event_id":"purchase","channel":funding::SECONDARY_FEE_LANES[adjustment],"amount":1.})
        ]} else {vec![]};
        json!({"ordinal":740000+day,"source_checkpoint_id":format!("state-{day}"),"channels":amounts,"origins":origins})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({
        "schema":funding::SECONDARY_FEE_SCHEMA,"measure":"physical","source":review,"original_yield_anchor":review,
        "path_id":"buyer-path","entity":"bank","currency_unit":"USD","scenario_id":"base","side":"asset",
        "start_ordinal":740000,"full_end_ordinal":740001,"source_weight":0.5,"unit_face":100.,"allocated_face":allocated,
        "opening_funding_balance":sign*5.,
        "policy":{"review":review,"accrual_basis":"ACT/365F","averaging":"opening_daily",
            "negative_funding":if sign>0.{"reject_negative"}else{"research_signed"},
            "principal_zero_rights":"explicit_reviewed_all_channels","termination":"complete_gross_rights_and_commitment",
            "tail":"no_synthetic_cash_or_tail","channels":channels},
        "states":states,
        "cash":[{"event_id":"conveyed-cash","ordinal":740001,"kind":"source_atomic_cash","bank_cash":sign*6.,"funding_change_sequence":2,"exclusion_review":null}],
        "changes":[
            {"sequence":1,"ordinal":740001,"source_event_id":"buyer-adjustment-release","kind":"reviewed_basis_reallocation","amount":sign,"cash_event_id":null,"review":review},
            {"sequence":2,"ordinal":740001,"source_event_id":"right-settlement","kind":"cash_assignment","amount":-sign*6.,"cash_event_id":"conveyed-cash","review":review}],
        "earned":[{"event_id":"buyer-income","ordinal":740001,"channel":"fee_cost_release","bank_earnings":sign,"review":review}],
        "resets":[{"ordinal":740000,"reference_years":0.1,"liquidity_years":0.1,"review":review}]
    })).unwrap()
}

#[test]
fn purchased_asset_and_liability_contra_channels_reconcile_once_at_every_scale() {
    for sign in [-1., 1.] {
        for allocated in [0., 50., 100.] {
            let raw = request(sign, allocated);
            let result = funding::prepare_secondary_fee_with_check(&raw, &mut || Ok(())).unwrap();
            let scale = 0.5 * allocated / 100.;
            assert_eq!(
                result.schema,
                "modeled-product-ftp-preparation-proof-result-4"
            );
            assert_eq!(
                result.days[0].scaled_average_funding_balance,
                sign * 5. * scale
            );
            assert_eq!(result.days[0].external_earnings, sign * scale);
            assert_eq!(result.days[0].bank_cash, sign * 6. * scale);
            assert_eq!(result.days[0].original_eir, 0.);
            assert!(result
                .closing_unweighted_channels
                .iter()
                .all(|x| x.amount == 0.));
            assert!(!result.source_reconstructed && !result.ftp_bridge_adoption);
        }
    }
}

#[test]
fn conveyed_adjustments_require_explicit_schema_and_complete_gross_provenance() {
    let raw = request(1., 100.);
    assert!(funding::prepare_fee_with_check(&raw, &mut || Ok(())).is_err());
    let mut old = raw.clone();
    old.schema = funding::FEE_SCHEMA.into();
    assert!(funding::prepare_fee_with_check(&old, &mut || Ok(())).is_err());
    for zero in [false, true] {
        let mut bad = raw.clone();
        if zero {
            bad.allocated_face = 0.;
        }
        bad.states[0].origins.pop();
        assert!(funding::prepare_secondary_fee_with_check(&bad, &mut || Ok(())).is_err());
        let mut bad = raw.clone();
        bad.states[0].channels[24].amount = -1.;
        assert!(funding::prepare_secondary_fee_with_check(&bad, &mut || Ok(())).is_err());
    }
}

#[test]
fn purchased_fee_funding_obeys_original_deadline_and_reviewed_sign() {
    let mut raw = request(-1., 100.);
    raw.policy.negative_funding = "reject_negative".into();
    assert!(funding::prepare_secondary_fee_with_check(&raw, &mut || Ok(())).is_err());
    let mut calls = 0;
    let error = funding::prepare_secondary_fee_with_check(&request(1., 100.), &mut || {
        calls += 1;
        if calls == 3 {
            Err("COMPUTE_DEADLINE_EXCEEDED".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(error, "COMPUTE_DEADLINE_EXCEEDED");
}
