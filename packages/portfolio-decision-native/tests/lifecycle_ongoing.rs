//! Ongoing contractual economics, independently replayed through the shared daily journal.
use portfolio_decision_native::workflows::lifecycle::acquisition::run;
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/lifecycle-ongoing.json")).unwrap()
}
fn execute(v: &Value) -> Result<(Value, Vec<u8>), String> {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let receipt = run(
        &serde_json::to_vec(v).unwrap(),
        Box::new(Capture(bytes.clone())),
    )?;
    let stream = bytes.lock().unwrap().clone();
    Ok((serde_json::to_value(receipt).unwrap(), stream))
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[test]
fn held_swap_and_facility_interest_reconcile_and_verify() {
    let v = fixture();
    let (r, stream) = execute(&v).unwrap();
    portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
    for j in r["journals"].as_array().unwrap() {
        let receipt = &j["receipt"];
        assert_eq!(
            receipt["open_claims"], true,
            "matured swap still has unreturned VM and IM"
        );
        near(receipt["closing_cash"].as_f64().unwrap(), 995.7);
        near(
            receipt["gross_derivative_liabilities"].as_f64().unwrap(),
            0.,
        );
        let month = &receipt["monthly"][0];
        near(month["interest_expense"].as_f64().unwrap(), 0.3);
        near(month["derivative_income"].as_f64().unwrap(), -4.);
        near(month["derivative_mark_pnl"].as_f64().unwrap(), 4.);
    }
    // Mutating the reviewed rate must fail even when the journal itself remains balanced.
    let mut changed = v.clone();
    changed["acquisitions"][0]["facility_interest"][0]["annual_rate"] = json!(0.18);
    assert!(
        portfolio_verification_native::verify(&serde_json::to_vec(&changed).unwrap(), &stream)
            .is_err()
    );
}
#[test]
fn omitted_coupon_and_changed_contract_refuse() {
    let mut v = fixture();
    v["acquisitions"][0]["derivative_updates"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(execute(&v).unwrap_err().contains("every coupon"));
    let mut v = fixture();
    v["hedge_contracts"][1]["swap"]["fixed_rate"] = json!(0.06);
    assert!(execute(&v).unwrap_err().contains("immutable"));
}

#[test]
fn negative_rates_and_unpaid_accrual_keep_gross_rights() {
    for rate in [-0.36, 0.36] {
        let mut v = fixture();
        for a in v["acquisitions"].as_array_mut().unwrap() {
            a["facility_interest"][0]["annual_rate"] = json!(rate);
            a["facility_interest"][0]["payment_ordinals"] = json!([]);
        }
        let (r, stream) = execute(&v).unwrap();
        portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
        for j in r["journals"].as_array().unwrap() {
            assert_eq!(j["receipt"]["open_claims"], true);
            near(j["receipt"]["closing_cash"].as_f64().unwrap(), 996.);
            near(
                j["receipt"]["monthly"][0]["interest_expense"]
                    .as_f64()
                    .unwrap(),
                rate / 1.2,
            );
        }
    }
}

#[test]
fn day_count_and_same_day_repayment_use_overnight_principal() {
    for (repayment, basis, expense) in [
        (738887, "act360", 0.),
        (738890, "act365_fixed", 108. / 365.),
    ] {
        let mut v = fixture();
        for a in v["acquisitions"].as_array_mut().unwrap() {
            a["facility_repayments"][0]["ordinal"] = json!(repayment);
            a["facility_interest"][0]["day_count"] = json!(basis);
        }
        let (r, stream) = execute(&v).unwrap();
        portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
        for j in r["journals"].as_array().unwrap() {
            near(
                j["receipt"]["monthly"][0]["interest_expense"]
                    .as_f64()
                    .unwrap(),
                expense,
            );
            near(
                j["receipt"]["closing_cash"].as_f64().unwrap(),
                996. - expense,
            );
        }
    }
}

#[test]
fn variation_margin_uses_current_marks_and_settles_after_the_lag() {
    let mut v = fixture();
    for a in v["acquisitions"].as_array_mut().unwrap() {
        a["variation_margin_reviews"] = json!([{"agreement_id":"swap-csa","ordinal":738891,"terms":{"receive_threshold":0.,"post_threshold":0.5,"minimum_transfer":0.}}]);
    }
    let (r, stream) = execute(&v).unwrap();
    portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
    for j in r["journals"].as_array().unwrap() {
        near(j["receipt"]["posted_vm"].as_f64().unwrap(), 1.5);
        near(j["receipt"]["closing_cash"].as_f64().unwrap(), 996.2);
        let step = j["receipt"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["event_id"] == "vm_rebalance:swap-csa:738891")
            .unwrap();
        assert_eq!(step["ordinal"], 738892);
    }
}

#[test]
fn termination_coupon_is_owned_once_and_failed_termination_keeps_later_cash() {
    for failed in [false, true] {
        let mut v = fixture();
        for a in v["acquisitions"].as_array_mut().unwrap() {
            a["terminations"] = json!([{"lot_id":"swap-lot","agreement_id":"swap-csa","effective_ordinal":738897,"settlement_ordinal":738898,"consideration":{"kind":"model_clean_value"},"fees":0.,"consent":{"id":"termination","revision":"r1"},"state":if failed {"failed"} else {"pending"}}]);
            a["initial_margin"] = json!([{"agreement_id":"swap-csa","portfolio_lot_ids":[],"scenario_id":a["scenario_id"],"ordinal":738897,"posted_requirement":0.,"received_requirement":0.,"review":{"id":"im","revision":"r1"}}]);
            // The planned termination binds its empty-scope requirement. On failure its
            // dependent return is blocked, so the existing posted IM remains held.
        }
        let (r, stream) = execute(&v).unwrap();
        portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
        for j in r["journals"].as_array().unwrap() {
            near(
                j["receipt"]["monthly"][0]["derivative_income"]
                    .as_f64()
                    .unwrap(),
                -4.,
            );
            near(
                j["receipt"]["closing_cash"].as_f64().unwrap(),
                if failed { 995.7 } else { 1000.7 },
            );
        }
    }
}

#[test]
fn funding_policy_finances_coupon_and_interest_without_lookahead() {
    let mut v = fixture();
    for path in ["plan", "dated"] {
        let a = if path == "plan" {
            &mut v[path]["accounts"][0]
        } else {
            &mut v[path]["specification"]["accounts"][0]
        };
        a["cash"] = json!(0.);
        a["equity"] = json!(106.);
    }
    for a in v["acquisitions"].as_array_mut().unwrap() {
        a["facility_draws"] = json!([]);
        a["facility_repayments"] = json!([]);
        a["facility_interest"][0]["payment_ordinals"] = json!([738898]);
        a["business_plan"] = json!({"mode":"policy","funding_policy":{
            "policy":{"id":"treasury","revision":"r1"},"minimum_usable_cash":0.,
            "facilities":[{"facility_id":"bridge","access":[{"from_ordinal":738886,"to_ordinal":738900}]}],"repay_surplus":false
        }});
    }
    let (r, stream) = execute(&v).unwrap();
    portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
    for j in r["journals"].as_array().unwrap() {
        let receipt = &j["receipt"];
        near(receipt["closing_cash"].as_f64().unwrap(), 0.);
        let decisions = receipt["policy_decisions"].as_array().unwrap();
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0]["ordinal"], 738897);
        assert_eq!(decisions[0]["trigger"], "derivative_update:swap-lot:738897");
        near(decisions[0]["amount"].as_f64().unwrap(), 4.);
        assert_eq!(decisions[1]["ordinal"], 738898);
        near(decisions[1]["amount"].as_f64().unwrap(), 0.004);
    }
}

#[test]
fn allocation_retains_hold_economics_and_refuses_missing_baseline_owners() {
    use portfolio_decision_native::workflows::lifecycle::allocation;
    let candidate = fixture();
    let mut base = candidate.clone();
    for a in base["acquisitions"].as_array_mut().unwrap() {
        a["facility_draws"] = json!([]);
        a["facility_repayments"] = json!([]);
        a["business_plan"] = json!({"mode":"runoff"});
    }
    let mut request = json!({"schema":"liquidity-allocation-1","base":base,"packages":[{
        "package_id":"draw","group":"funding","review":{"id":"review","revision":"r1"},"request":candidate
    }],"constraints":{"minimum_usable_cash":0.,"objective":"worst_case_total_earnings","max_refinements":2}});
    let out = Arc::new(Mutex::new(Vec::new()));
    allocation::run(
        &serde_json::to_vec(&request).unwrap(),
        Box::new(Capture(out.clone())),
    )
    .unwrap();
    let bytes = out.lock().unwrap();
    let first = bytes.split(|b| *b == b'\n').next().unwrap();
    let head: Value = serde_json::from_slice(first).unwrap();
    let receipt = &head["allocation"];
    near(
        receipt["base"]["baseline"]["external_nii"]
            .as_f64()
            .unwrap(),
        -3.7, // 0.3 of dated backbook income and -4 of swap coupon income.
    );
    near(
        receipt["base"]["baseline"]["trading_pnl"].as_f64().unwrap(),
        4.,
    );
    let coefficient = &receipt["packages"][0]["coefficients"]["baseline"];
    near(coefficient["external_nii"].as_f64().unwrap(), 0.);
    near(coefficient["trading_pnl"].as_f64().unwrap(), 0.);
    near(coefficient["funding_cost"].as_f64().unwrap(), 0.3);
    let mut changed = request.clone();
    for contract in changed["packages"][0]["request"]["hedge_contracts"]
        .as_array_mut()
        .unwrap()
    {
        contract["swap"]["fixed_rate"] = json!(0.);
    }
    let error =
        allocation::run(&serde_json::to_vec(&changed).unwrap(), Box::new(Vec::new())).unwrap_err();
    assert!(error.contains("raw hedge sources"), "{error}");
    request["base"]["acquisitions"][0]["derivative_updates"] = json!([]);
    let error =
        allocation::run(&serde_json::to_vec(&request).unwrap(), Box::new(Vec::new())).unwrap_err();
    assert!(error.contains("ongoing"), "{error}");
}

#[test]
fn segregated_vm_return_releases_cash_and_only_new_posted_margin_needs_funding() {
    let mut v = fixture();
    v["plan"]["accounts"][0]["cash"] = json!(2.);
    v["plan"]["accounts"][0]["equity"] = json!(104.);
    v["dated"]["specification"]["accounts"][0]["cash"] = json!(2.);
    v["dated"]["specification"]["accounts"][0]["equity"] = json!(104.);
    v["plan"]["margin_agreements"][0]["opening_collateral"]["posted_vm"] = json!(0.);
    v["plan"]["margin_agreements"][0]["opening_collateral"]["received_vm"] = json!(2.);
    for a in v["acquisitions"].as_array_mut().unwrap() {
        a["facility_draws"] = json!([]);
        a["facility_repayments"] = json!([]);
        a["variation_margin_reviews"] = json!([{"agreement_id":"swap-csa","ordinal":738891,"terms":{"receive_threshold":0.,"post_threshold":0.,"minimum_transfer":0.}}]);
        a["business_plan"] = json!({"mode":"policy","funding_policy":{
            "policy":{"id":"treasury","revision":"r1"},"minimum_usable_cash":0.,
            "facilities":[{"facility_id":"bridge","access":[{"from_ordinal":738886,"to_ordinal":738916}]}],"repay_surplus":false
        }});
    }
    let (r, stream) = execute(&v).unwrap();
    portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
    for j in r["journals"].as_array().unwrap() {
        let receipt = &j["receipt"];
        near(
            receipt["policy_decisions"][0]["amount"].as_f64().unwrap(),
            2.,
        );
        assert_eq!(
            receipt["policy_decisions"][0]["trigger"],
            "vm_rebalance:swap-csa:738891"
        );
        near(receipt["received_vm"].as_f64().unwrap(), 0.);
        near(receipt["posted_vm"].as_f64().unwrap(), 2.);
        near(receipt["closing_cash"].as_f64().unwrap(), 0.);
    }
    let mut overlap = v.clone();
    let mut overflow = v.clone();
    overflow["acquisitions"][0]["variation_margin_reviews"][0]["ordinal"] = json!(i64::MAX);
    assert!(portfolio_verification_native::verify(
        &serde_json::to_vec(&overflow).unwrap(),
        &stream
    )
    .is_err());
    let mut second = overlap["acquisitions"][0]["variation_margin_reviews"][0].clone();
    second["ordinal"] = json!(738892);
    overlap["acquisitions"][0]["variation_margin_reviews"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert!(execute(&overlap)
        .unwrap_err()
        .contains("overlapping pending VM"));
    v["acquisitions"][0]["derivative_updates"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert!(execute(&v).unwrap_err().contains("current native value"));
}

#[test]
fn vm_reviews_refuse_pending_termination_collateral_returns() {
    let v = fixture();
    let (_, stream) = execute(&v).unwrap();
    for day in [738891, 738892, 738893] {
        let mut changed = v.clone();
        for a in changed["acquisitions"].as_array_mut().unwrap() {
            a["terminations"] = json!([{"lot_id":"swap-lot","agreement_id":"swap-csa","effective_ordinal":738891,"settlement_ordinal":738892,"consideration":{"kind":"model_clean_value"},"fees":0.,"consent":{"id":"termination","revision":"r1"}}]);
            a["variation_margin_reviews"] = json!([{"agreement_id":"swap-csa","ordinal":day,"terms":{"receive_threshold":0.,"post_threshold":0.,"minimum_transfer":0.}}]);
        }
        assert!(execute(&changed)
            .unwrap_err()
            .contains("termination collateral"));
        let error =
            portfolio_verification_native::verify(&serde_json::to_vec(&changed).unwrap(), &stream)
                .unwrap_err();
        assert!(error.contains("termination collateral"), "{error}");
    }
    // An earlier entry's unsettled VM is another owner of the agreement's collateral.
    let mut pending_entry = v.clone();
    for a in pending_entry["acquisitions"].as_array_mut().unwrap() {
        a["hedge_entries"] = json!([{"lot_id":"swap-lot","agreement_id":"swap-csa","entry_ordinal":738887,"upfront_consideration":-4.,"variation_margin":{"receive_threshold":0.,"post_threshold":0.,"minimum_transfer":0.}}]);
        a["variation_margin_reviews"] = json!([{"agreement_id":"swap-csa","ordinal":738888,"terms":{"receive_threshold":0.,"post_threshold":0.,"minimum_transfer":0.}}]);
    }
    assert!(execute(&pending_entry)
        .unwrap_err()
        .contains("overlapping pending VM"));
    let error = portfolio_verification_native::verify(
        &serde_json::to_vec(&pending_entry).unwrap(),
        &stream,
    )
    .unwrap_err();
    assert!(error.contains("pending settlement overlap"), "{error}");
}

#[test]
fn zero_market_value_does_not_complete_a_live_contract() {
    use portfolio_model_core::lifecycle::{
        self,
        acquisition::{self, AcquisitionRequest, ResolvedHedge},
        Revision,
    };
    let v = fixture();
    let plan = lifecycle::admit(&serde_json::to_vec(&v["plan"]).unwrap()).unwrap();
    let acq: AcquisitionRequest = serde_json::from_value(v["acquisitions"][0].clone()).unwrap();
    for (tail, claims) in [(738897, true), (738916, false)] {
        let values: Vec<_> = acq
            .derivative_updates
            .iter()
            .map(|u| ResolvedHedge {
                lot_id: u.lot_id.clone(),
                ordinal: u.ordinal,
                tail_end_ordinal: tail,
                clean_value: 0.,
                due_cash: 0.,
                pricing_source: Revision {
                    id: "independent-test-value".into(),
                    revision: "r1".into(),
                },
            })
            .collect();
        let journal =
            acquisition::replay(&plan, &acq, &values, &plan.request.accounts[0].account_id)
                .unwrap();
        assert_eq!(journal.receipt.open_claims, claims);
    }
}

#[test]
fn independent_accrual_horizon_refuses_before_event_expansion() {
    let v = fixture();
    let (_, stream) = execute(&v).unwrap();
    for end in [738886 + 367, i64::MAX, i64::MIN] {
        let mut changed = v.clone();
        changed["acquisitions"][0]["horizon_end_ordinal"] = json!(end);
        let error =
            portfolio_verification_native::verify(&serde_json::to_vec(&changed).unwrap(), &stream)
                .unwrap_err();
        assert!(error.contains("execution horizon"), "{error}");
    }
}

#[test]
fn independent_swap_equation_applies_index_then_all_in_bounds() {
    let (_, uncapped) = execute(&fixture()).unwrap();
    for (index, margin, all_in, income) in [
        (
            json!({"floor":null,"cap":null}),
            0.,
            json!({"floor":null,"cap":0.005}),
            -4.5,
        ),
        (
            json!({"floor":0.02,"cap":null}),
            0.01,
            json!({"floor":null,"cap":0.025}),
            -2.5,
        ),
        (
            json!({"floor":null,"cap":0.005}),
            -0.02,
            json!({"floor":0.,"cap":null}),
            -5.,
        ),
    ] {
        let mut v = fixture();
        for contract in v["hedge_contracts"].as_array_mut().unwrap() {
            for period in contract["swap"]["floating_periods"].as_array_mut().unwrap() {
                period["coupon"]["index_bounds"] = index.clone();
                period["coupon"]["margin"] = json!(margin);
                period["coupon"]["all_in_bounds"] = all_in.clone();
            }
        }
        let (r, stream) = execute(&v).unwrap();
        portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &stream).unwrap();
        assert!(
            portfolio_verification_native::verify(&serde_json::to_vec(&v).unwrap(), &uncapped)
                .is_err()
        );
        near(
            r["journals"][0]["receipt"]["monthly"][0]["derivative_income"]
                .as_f64()
                .unwrap(),
            income,
        );
    }
}
