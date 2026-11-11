use portfolio_model_core::modeled_product_funding as funding;
use serde_json::{json, Value};

fn raw() -> funding::Request {
    let review = json!({"id":"reviewed-fee-funding","revision":"1"});
    let channels = funding::FEE_LANES.iter().enumerate().map(|(i, name)|
        json!({"channel":name,"coefficient":if i==18 || i==22 {1}else{0},"review":review}))
        .collect::<Vec<_>>();
    let states = (0..=1).map(|day| {
        let amounts = funding::FEE_LANES.iter().enumerate().map(|(i,name)|
            json!({"channel":name,"amount":if day==0 && i==18 {5.}else if day==0 && i==22 {3.}else{0.}}))
            .collect::<Vec<_>>();
        let origins: Vec<Value> = if day==0 {vec![
            json!({"origin_id":"service","source_event_id":"fee-right","channel":"fee_receivable","amount":5.}),
            json!({"origin_id":"acquired-basis","source_event_id":"acquisition","channel":"acquired_fee_asset","amount":3.})
        ]} else {vec![]};
        json!({"ordinal":740000+day,"source_checkpoint_id":format!("native-state-{day}"),"channels":amounts,"origins":origins})
    }).collect::<Vec<_>>();
    serde_json::from_value(json!({
        "schema":funding::FEE_SCHEMA,"measure":"physical","source":review,
        "original_yield_anchor":review,"path_id":"physical-path","entity":"bank","currency_unit":"USD",
        "scenario_id":"base","side":"asset","start_ordinal":740000,"full_end_ordinal":740001,
        "source_weight":0.25,"unit_face":100.,"allocated_face":100.,"opening_funding_balance":8.,
        "policy":{"review":review,"accrual_basis":"ACT/365F","averaging":"opening_daily",
            "negative_funding":"reject_negative","principal_zero_rights":"explicit_reviewed_all_channels",
            "termination":"complete_gross_rights_and_commitment","tail":"no_synthetic_cash_or_tail","channels":channels},
        "states":states,
        "cash":[{"event_id":"fee-cash","ordinal":740001,"kind":"source_atomic_cash","bank_cash":5.,"funding_change_sequence":1,"exclusion_review":null}],
        "changes":[
            {"sequence":1,"ordinal":740001,"source_event_id":"fee-settlement","kind":"cash_assignment","amount":-5.,"cash_event_id":"fee-cash","review":review},
            {"sequence":2,"ordinal":740001,"source_event_id":"basis-release","kind":"reviewed_basis_reallocation","amount":-3.,"cash_event_id":null,"review":review}],
        "earned":[
            {"event_id":"service-income","ordinal":740001,"channel":"service_fee","bank_earnings":5.,"review":review},
            {"event_id":"cost-release","ordinal":740001,"channel":"fee_cost_release","bank_earnings":-3.,"review":review}],
        "resets":[{"ordinal":740000,"reference_years":0.1,"liquidity_years":0.1,"review":review}]
    })).unwrap()
}

#[test]
fn explicit_fee_funding_retains_gross_rights_and_income_separation_once() {
    let result = funding::prepare_fee_with_check(&raw(), &mut || Ok(())).unwrap();
    assert_eq!(
        result.schema,
        "modeled-product-ftp-preparation-proof-result-3"
    );
    assert_eq!(result.scale, 0.25);
    assert_eq!(result.days[0].scaled_average_funding_balance, 2.);
    assert_eq!(result.days[0].original_eir, 0.);
    assert_eq!(result.days[0].external_earnings, 0.5);
    assert_eq!(result.days[0].bank_cash, 1.25);
    assert!(result
        .closing_unweighted_channels
        .iter()
        .all(|c| c.amount == 0.));
    assert!(
        !result.source_reconstructed
            && !result.ftp_bridge_adoption
            && !result.accounting_standard_approval
    );
}

#[test]
fn fee_channels_need_explicit_selection_and_all_policies() {
    let mut request = raw();
    assert!(funding::prepare(&request).is_err());
    request.policy.channels.pop();
    assert!(funding::prepare_fee_with_check(&request, &mut || Ok(())).is_err());
    let mut request = raw();
    request.policy.channels[12].coefficient = 1;
    assert!(funding::prepare_fee_with_check(&request, &mut || Ok(())).is_err());
}

#[test]
fn zero_allocation_retains_unweighted_fee_tail_validation() {
    let mut request = raw();
    request.allocated_face = 0.;
    let result = funding::prepare_fee_with_check(&request, &mut || Ok(())).unwrap();
    assert_eq!(result.days[0].external_earnings, 0.);
    request.states[1].channels[18].amount = 1.;
    request.states[1].origins.push(funding::Origin {
        origin_id: "unsettled".into(),
        source_event_id: "fee-right".into(),
        channel: "fee_receivable".into(),
        amount: 1.,
    });
    assert!(funding::prepare_fee_with_check(&request, &mut || Ok(())).is_err());
}
