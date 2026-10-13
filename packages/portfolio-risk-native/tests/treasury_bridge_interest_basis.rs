//! Closing-capital mapping fixtures only. These are reviewed synthetic signed
//! trial balances, not public Card2 eligibility or inferred regulatory weights.
use portfolio_risk_native::treasury_bridge::Request;
use serde_json::{json, Value};

fn request(rows: &[(&str, &str, f64)], weights: Value) -> Value {
    json!({
        "kind":"ledger", "amount_multiplier":1.,
        "treasury":{"as_of":"2026-10-02", "policy_id":"reviewed-capital-test-1",
            "capital":[], "curves":[], "positions":[]},
        "trial_balance":rows.iter().map(|(gl,id,balance)| json!({
            "scenario":"baseline", "account":"bank", "gl_account":gl,
            "instrument_id":id, "balance":balance})).collect::<Vec<_>>(),
        "policies":[{"account":"bank", "scenario":"baseline",
            "include_aoci":true, "equity_exclusions":0., "intangible_assets":0.,
            "leverage_addon":0., "leverage_deductions":0.,
            "gl_risk_weights":weights, "instrument_risk_weights":{},
            "capital":{"entity":"bank", "currency":"USD", "scenario":"baseline",
                "period":"2026-10-02", "common_equity":0., "retained_earnings":0.,
                "eligible_aoci":0., "cet1_deductions":0., "additional_tier1":0.,
                "tier2":0., "eligible_tlac":0., "eligible_ltd":0., "credit_rwa":0.,
                "market_rwa":0., "operational_rwa":0., "advanced_rwa":null,
                "average_assets":100., "total_leverage_exposure":0.,
                "tangible_common_equity":0., "tangible_assets":0., "requirements":{}}}]
    })
}

fn run(raw: Value) -> Result<Value, String> {
    serde_json::from_value::<Request>(raw)
        .map_err(|e| e.to_string())?
        .run()
}

fn near(actual: &Value, expected: f64) {
    let actual = actual.as_f64().unwrap();
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

fn rights() -> Value {
    request(
        &[
            ("cash", "", 100.),
            ("asset_principal", "loan", 50.),
            ("dated_interest_receivable", "live", 5.),
            ("live_interest_basis_adjustment", "live", -2.),
            ("credit_default_interest_receivable", "default", 4.),
            ("default_interest_basis_adjustment", "default", 1.),
            ("pnl:original_effective_interest", "live", -3.),
            ("pnl:reviewed_basis_realization", "default", -2.),
            ("opening_equity", "", -153.),
        ],
        json!({"cash":0., "asset_principal":1., "dated_interest_receivable":0.5,
            "live_interest_basis_adjustment":0.5, "credit_default_interest_receivable":1.5,
            "default_interest_basis_adjustment":1.5}),
    )
}

#[test]
fn gross_rights_and_signed_basis_feed_assets_rwa_and_separate_income_once() {
    let result = run(rights()).unwrap();
    let audit = &result["capital_bridge"][0];
    // Assets = 100 + 50 + (5 - 2) + (4 + 1) = 158.
    // Supplied RWA = 50 + .5*(5-2) + 1.5*(4+1) = 59.
    near(&audit["assets"], 158.);
    near(&audit["liabilities"], 0.);
    near(&audit["credit_rwa"], 59.);
    near(&audit["common_equity"], 153.);
    near(&audit["retained_earnings"], 5.); // original EIR 3, reviewed realization 2
    near(&audit["book_equity"], 158.);
    let prepared = &result["prepared_specification"]["capital"][0];
    near(&prepared["total_leverage_exposure"], 158.);
    near(&prepared["tangible_assets"], 158.);
    assert!(prepared["advanced_rwa"].is_null());
    assert!(prepared["requirements"].as_object().unwrap().is_empty());
    assert!(result["ftp_positions"].as_array().unwrap().is_empty());
    let contributions = result["rwa_contributions"].as_array().unwrap();
    let expected = [
        ("cash", 100., 0.),
        ("asset_principal", 50., 50.),
        ("dated_interest_receivable", 5., 2.5),
        ("live_interest_basis_adjustment", -2., -1.),
        ("credit_default_interest_receivable", 4., 6.),
        ("default_interest_basis_adjustment", 1., 1.5),
    ];
    assert_eq!(contributions.len(), expected.len());
    for (row, (gl, balance, rwa)) in contributions.iter().zip(expected) {
        assert_eq!(row["gl_account"], gl); // source reduction order is unchanged
        near(&row["balance"], balance);
        near(&row["rwa_contribution"], rwa);
    }
}

#[test]
fn principal_zero_interest_only_contra_does_not_become_liability_or_income() {
    for (gross, basis, assets, rwa) in [(1., -1., 100., 0.), (4., -1., 103., 2.25)] {
        for (right, adjustment) in [
            (
                "dated_interest_receivable",
                "live_interest_basis_adjustment",
            ),
            (
                "credit_default_interest_receivable",
                "default_interest_basis_adjustment",
            ),
        ] {
            let mut weights = json!({"cash":0.});
            weights[right] = json!(0.75);
            weights[adjustment] = json!(0.75);
            let result = run(request(
                &[
                    ("cash", "", 100.),
                    (right, "card", gross),
                    (adjustment, "card", basis),
                    ("opening_equity", "", -assets),
                ],
                weights,
            ))
            .unwrap();
            let audit = &result["capital_bridge"][0];
            near(&audit["assets"], assets);
            near(&audit["liabilities"], 0.);
            near(&audit["credit_rwa"], rwa);
            near(&audit["retained_earnings"], 0.);
            assert_eq!(result["rwa_contributions"][1]["balance"], gross);
            assert_eq!(result["rwa_contributions"][2]["balance"], basis);
        }
    }
}

#[test]
fn supplied_instrument_weight_overrides_both_signed_gl_rows() {
    let mut raw = rights();
    raw["policies"][0]["instrument_risk_weights"] = json!({"live":2.,"default":0.25});
    let result = run(raw).unwrap();
    // Different reviewed weights: 50 + 2*(5-2) + .25*(4+1).
    near(&result["capital_bridge"][0]["credit_rwa"], 57.25);
    near(&result["rwa_contributions"][3]["rwa_contribution"], -4.);
    near(&result["rwa_contributions"][5]["rwa_contribution"], 0.25);
}

#[test]
fn monetary_conversion_preserves_signed_adjustments_and_income() {
    let mut raw = rights();
    raw["amount_multiplier"] = json!(1000.);
    let result = run(raw).unwrap();
    near(&result["capital_bridge"][0]["assets"], 158000.);
    near(&result["capital_bridge"][0]["credit_rwa"], 59000.);
    near(&result["capital_bridge"][0]["retained_earnings"], 5000.);
    near(&result["rwa_contributions"][3]["balance"], -2000.);
}

#[test]
fn missing_unknown_negative_and_nonfinite_weights_refuse_without_loss_defaults() {
    for gl in [
        "live_interest_basis_adjustment",
        "default_interest_basis_adjustment",
    ] {
        let mut raw = rights();
        raw["policies"][0]["gl_risk_weights"]
            .as_object_mut()
            .unwrap()
            .remove(gl);
        assert_eq!(run(raw).unwrap_err(), "missing asset risk-weight mapping");
        for weight in [json!(-1.), Value::Null, json!(1e19)] {
            let mut raw = rights();
            raw["policies"][0]["gl_risk_weights"][gl] = weight;
            assert!(run(raw).is_err());
        }
    }
    let mut raw = rights();
    raw["policies"][0]["gl_risk_weights"]["unreviewed_interest_basis"] = json!(1.);
    assert_eq!(run(raw).unwrap_err(), "unknown asset GL risk mapping");
    let mut raw = rights();
    raw["policies"] = json!([]);
    assert_eq!(
        run(raw).unwrap_err(),
        "ledger bridge admission or nonempty capital inputs"
    );
    let mut raw = rights();
    raw["policies"][0]["scenario"] = json!("unconfigured");
    assert_eq!(run(raw).unwrap_err(), "missing ledger account policy");
}

#[test]
fn signed_basis_cannot_turn_total_rwa_negative() {
    let mut raw = rights();
    raw["policies"][0]["gl_risk_weights"]["live_interest_basis_adjustment"] = json!(100.);
    assert_eq!(run(raw).unwrap_err(), "unbalanced or invalid ledger bridge");
}

#[test]
fn original_eir_expense_and_reviewed_basis_income_remain_distinct_pnl() {
    let result = run(request(
        &[
            ("cash", "", 100.),
            ("dated_interest_receivable", "card", 1.),
            ("live_interest_basis_adjustment", "card", -0.5),
            ("pnl:original_effective_interest", "card", 0.25),
            ("pnl:reviewed_basis_realization", "card", -0.75),
            ("opening_equity", "", -100.),
        ],
        json!({"cash":0.,"dated_interest_receivable":1.,"live_interest_basis_adjustment":1.}),
    ))
    .unwrap();
    near(&result["capital_bridge"][0]["assets"], 100.5);
    near(&result["capital_bridge"][0]["credit_rwa"], 0.5);
    near(&result["capital_bridge"][0]["retained_earnings"], 0.5);
    assert_eq!(result["rwa_contributions"].as_array().unwrap().len(), 3);
}

#[test]
fn zero_adjustments_use_existing_zero_mapping_rule_without_invented_weights() {
    let result = run(request(
        &[
            ("cash", "", 100.),
            ("live_interest_basis_adjustment", "card", 0.),
            ("default_interest_basis_adjustment", "card", 0.),
            ("opening_equity", "", -100.),
        ],
        json!({"cash":0.}),
    ))
    .unwrap();
    near(&result["capital_bridge"][0]["assets"], 100.);
    near(&result["capital_bridge"][0]["credit_rwa"], 0.);
    for row in &result["rwa_contributions"].as_array().unwrap()[1..] {
        near(&row["risk_weight"], 0.);
    }
}

#[test]
fn separate_credit_protocols_do_not_implicitly_adopt_card_basis_accounts() {
    for protocol in [4, 5] {
        let mut raw = request(
            &[
                ("cash", "", 100.),
                ("live_interest_basis_adjustment", "card", 1.),
                ("equity_opening", "", -101.),
            ],
            json!({"cash":0.,"live_interest_basis_adjustment":1.}),
        );
        raw["kind"] = json!("credit_ledger");
        raw["credit_protocol"] = json!(protocol);
        assert_eq!(
            run(raw).unwrap_err(),
            "unsupported credit source GL live_interest_basis_adjustment"
        );
    }
}

#[test]
fn old_gross_right_and_deferred_income_mapping_and_order_remain_unchanged() {
    let result = run(request(
        &[
            ("cash", "", 100.),
            ("credit_default_principal", "loan", 60.),
            ("credit_default_book_adjustment", "loan", 2.),
            ("credit_default_interest_receivable", "loan", 4.),
            ("credit_default_deferred_income", "loan", -3.),
            ("allowance", "loan", -10.),
            ("pnl:consumer_credit_loss_expense", "loan", 40.),
            ("pnl:consumer_interest_income", "loan", -5.),
            ("opening_equity", "", -188.),
        ],
        json!({"cash":0.,"credit_default_principal":1.,"credit_default_book_adjustment":1.,
        "credit_default_interest_receivable":1.,"allowance":1.}),
    ))
    .unwrap();
    near(&result["capital_bridge"][0]["assets"], 156.);
    near(&result["capital_bridge"][0]["liabilities"], 3.);
    near(&result["capital_bridge"][0]["book_equity"], 153.);
    near(&result["capital_bridge"][0]["credit_rwa"], 56.);
    near(&result["capital_bridge"][0]["retained_earnings"], -35.);
    let order: Vec<_> = result["rwa_contributions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["gl_account"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "cash",
            "credit_default_principal",
            "credit_default_book_adjustment",
            "credit_default_interest_receivable",
            "allowance"
        ]
    );
}
