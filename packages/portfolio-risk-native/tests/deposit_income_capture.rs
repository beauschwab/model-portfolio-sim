use portfolio_risk_native::{
    advanced_graph::{CaptureOwner, CapturePolicy},
    deposit_graph::{DepositGraph, Request},
    deposit_graph_capture::income::{capture_original_income, IncomeAnchor},
};
use serde_json::Value;
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/deposit-graph-income-ledger-1.json"
    ))
    .unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn capture(
    v: Value,
) -> Result<portfolio_risk_native::deposit_graph_capture::income::Capture, String> {
    let graph = DepositGraph::new(&serde_json::from_value::<Request>(v["graph"].clone()).unwrap())?;
    let owners: Vec<CaptureOwner> = serde_json::from_value(v["owners"].clone()).unwrap();
    let policy: CapturePolicy = serde_json::from_value(v["capture_policy"].clone()).unwrap();
    let anchors: Vec<IncomeAnchor> = serde_json::from_value(v["income_anchors"].clone()).unwrap();
    capture_original_income(&graph, &owners, &policy, &anchors)
}
#[test]
fn original_yield_premium_daily_expense_and_exact_terminal_basis() {
    let v = raw();
    let opening = 1031_f64 * 1.1_f64.powf(-31. / 365.);
    let out = capture(v).unwrap();
    near(out.scheduled.openings[0].gross_carrying_value, opening);
    for source in &out.income_sources {
        near(
            source.result.rows[0].effective_interest,
            opening * (1.1_f64.powf(1. / 365.) - 1.),
        );
        near(
            source
                .result
                .rows
                .iter()
                .map(|r| r.effective_interest)
                .sum(),
            1031. - opening,
        );
        near(source.result.remaining_gross_carrying_value, 0.);
        near(
            source.result.rows.last().unwrap().closing_book_adjustment,
            0.,
        );
        assert_eq!(
            source.result.original_yield_anchor.id,
            "original-book-yield"
        );
        assert!(source
            .result
            .rows
            .iter()
            .all(|r| r.yield_review.is_none() && r.modification_gain_loss == 0.));
    }
    let base=out.scheduled.sources[0].schedule.events.iter().filter(|e|matches!(&e.scenario,portfolio_model_core::cashflow::ScenarioSelector::Named{scenario_id} if scenario_id=="baseline")).collect::<Vec<_>>();
    near(base.iter().map(|e| e.components.cash_interest).sum(), -31.);
    near(base.iter().map(|e| e.components.principal).sum(), -1000.);
    near(
        base.iter().map(|e| e.components.book_amortization).sum(),
        opening - 1000.,
    );
    near(
        base.iter()
            .map(|e| e.components.accrued_interest + e.components.book_amortization)
            .sum(),
        -(1031. - opening),
    );
    assert_eq!(out.cash_origins.len(), 6); // zero-cash basis origins are separate
}
#[test]
fn discounted_original_quote_has_same_complete_cash_and_reversed_basis() {
    let mut v = raw();
    let carry = 1031_f64 * 2_f64.powf(-31. / 365.);
    v["income_anchors"][0]["original_annual_effective_rate"] = 1.into();
    v["income_anchors"][0]["original_gross_carrying_value"] = carry.into();
    v["income_anchors"][0]["purchase_price_per100"] = (carry / 10.).into();
    let out = capture(v).unwrap();
    assert!(carry < 1000.);
    for source in out.income_sources {
        near(source.result.remaining_gross_carrying_value, 0.);
        near(
            source
                .result
                .rows
                .iter()
                .map(|r| r.effective_interest)
                .sum(),
            1031. - carry,
        );
    }
}
#[test]
fn anchor_and_expectation_changes_reject_without_catchup_or_plug() {
    for case in [
        "seasoned", "yield", "quote", "carrying", "basis", "terminal", "missing", "scenario",
        "inflow", "negative",
    ] {
        let mut v = raw();
        let a = &mut v["income_anchors"][0];
        match case {
            "seasoned" => a["purchase_ordinal"] = 738915.into(),
            "yield" => a["original_annual_effective_rate"] = (-0.01).into(),
            "quote" => a["purchase_price_per100"] = 100.into(),
            "carrying" => a["original_gross_carrying_value"] = 1000.into(),
            "basis" => a["basis_revision_policy"] = "silently_catchup".into(),
            "terminal" => a["terminal_basis_policy"] = "plug".into(),
            "missing" => v["income_anchors"] = serde_json::json!([]),
            "scenario" => {
                v["graph"]["positions"][0]["histories"][1]["physical"][0]["request"]["segments"]
                    [0]["repricing"]["target"]["intercept"] = 0.2.into()
            }
            "inflow" => {
                v["graph"]["positions"][0]["histories"][0]["physical"][0]["request"]["drivers"][1]
                    ["external_inflows"][0] = 1.into()
            }
            _ => {
                v["graph"]["positions"][0]["histories"][0]["physical"][0]["request"]["segments"]
                    [0]["repricing"]["floor"] = (-0.01).into()
            }
        }
        assert!(capture(v).is_err(), "{case}");
    }
}
#[test]
fn original_supplied_competing_runoff_cash_is_not_proportionally_adjusted_again() {
    let mut v = raw();
    let k = 0.2_f64 / 365.;
    let exit = -(-k).exp_m1();
    let mut carry = 0.;
    for day in 0..31 {
        let b = 1000. * (-k * f64::from(day)).exp();
        let accrued = f64::from(day) * b * 0.001;
        carry += (b + accrued) * exit * 1.1_f64.powf(-f64::from(day) / 365.);
    }
    carry += 1031. * (-k * 31.).exp() * 1.1_f64.powf(-31. / 365.);
    v["income_anchors"][0]["original_gross_carrying_value"] = carry.into();
    v["income_anchors"][0]["purchase_price_per100"] = (carry / 10.).into();
    for h in v["graph"]["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
    {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            h[role][0]["request"]["segments"][0]["retention"]["withdrawal_per_year"] = 0.2.into();
        }
    }
    let out = capture(v).unwrap();
    for s in out.income_sources {
        near(s.result.remaining_principal, 0.);
        near(s.result.remaining_gross_carrying_value, 0.);
    }
    assert!(out
        .cash_origins
        .iter()
        .any(|o| o.ordinal == 738916 && o.kind == "withdrawal"));
}

#[test]
fn exact_zero_income_keeps_all_daily_origin_identities() {
    let mut v = raw();
    v["income_anchors"][0]["original_annual_effective_rate"] = 0.into();
    v["income_anchors"][0]["original_gross_carrying_value"] = 1000.into();
    v["income_anchors"][0]["purchase_price_per100"] = 100.into();
    for h in v["graph"]["positions"][0]["histories"]
        .as_array_mut()
        .unwrap()
    {
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            h[role][0]["request"]["segments"][0]["opening_paid_rate"] = 0.into();
            h[role][0]["request"]["segments"][0]["repricing"]["target"]["intercept"] = 0.into();
        }
    }
    let out = capture(v).unwrap();
    assert_eq!(out.scheduled.accrual_plans.len(), 31 * 3);
    assert!(out
        .scheduled
        .accrual_plans
        .iter()
        .all(|p| p.segments[0].book_amortization == 0.));
}
