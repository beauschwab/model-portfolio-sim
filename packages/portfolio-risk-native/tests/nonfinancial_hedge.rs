//! nonfinancial-hedge-1: component cash flow hedges of forecasted purchases and sales, with
//! ASU 2025-09 eligibility, basis risk, missed forecasts and consumption, by hand values.
use portfolio_risk_native::products::derivatives::nonfinancial_hedge::{evaluate, Request, Result};
use serde_json::{json, Value};

type Edit = Box<dyn Fn(&mut Value)>;

fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/nonfinancial-hedge.json")).unwrap()
}
fn run(v: &Value) -> std::result::Result<Result, String> {
    let r: Request = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    evaluate(&r)
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 + 1e-12 * b.abs(), "{a} vs {b}");
}
fn delivery<'a>(
    r: &'a Result,
    id: &str,
) -> &'a portfolio_risk_native::products::derivatives::nonfinancial_hedge::DeliveryClosing {
    r.deliveries.iter().find(|d| d.delivery_id == id).unwrap()
}
fn pnl(r: &Result, account: &str) -> f64 {
    r.journal
        .iter()
        .filter(|p| p.account == account)
        .map(|p| p.credit - p.debit)
        .sum()
}

#[test]
fn matched_component_hedges_defer_to_delivery_and_consumption() {
    let r = run(&raw()).unwrap();
    // Same index and quantities: the hypothetical equals the swap, so every period passes at one.
    assert!(r.periods.iter().all(|p| p.assessment_passed));
    for p in &r.periods {
        if let Some(x) = p.cumulative_ratio {
            close(x, 1.);
        }
    }
    // Purchase legs settle at 100 * (1080 - 1000) and 100 * (1020 - 1000); OCI by delivery.
    close(delivery(&r, "mar").oci_recognized, 8000.);
    close(delivery(&r, "apr").oci_recognized, 2000.);
    // Consumption: 40% of March in April, the rest of March and all of April in June.
    close(pnl(&r, "pnl:cost_of_sales_hedge_reclassification"), 10000.);
    let april: f64 = r
        .journal
        .iter()
        .filter(|p| p.ordinal == 738977 && p.account == "pnl:cost_of_sales_hedge_reclassification")
        .map(|p| p.credit - p.debit)
        .sum();
    close(april, 3200.);
    // Sale of 50 t: the pay-index swap settles 50 * (1000 - 990), released to revenue at delivery.
    close(delivery(&r, "may").oci_recognized, 500.);
    close(pnl(&r, "pnl:revenue_hedge_reclassification"), 500.);
    // Everything deferred has been released, the swaps have closed in cash, and the books balance.
    for d in &r.deliveries {
        close(d.oci_balance, 0.);
    }
    let cash: f64 = r
        .journal
        .iter()
        .filter(|p| p.account == "cash")
        .map(|p| p.debit - p.credit)
        .sum();
    close(cash, 8000. + 2000. + 500.);
    close(r.closing.iter().map(|b| b.debit_balance).sum(), 0.);
    assert_eq!(
        r.relationships[0].component_kind,
        "clearly_and_closely_related"
    );
    assert!(r.relationships[0].asu_2025_09_component && !r.relationships[1].asu_2025_09_component);
    assert!(r.relationships.iter().all(|x| x.status == "completed"));
    assert!(!r.accounting_standard_approval && !r.purchase_and_inventory_cash_included);
}

#[test]
fn basis_risk_is_measured_and_a_failed_assessment_goes_to_earnings() {
    // Hedge the Gulf component with a European-index swap: close prices pass the band.
    let mut v = raw();
    v["instruments"][0]["index_id"] = json!("pe-europe");
    let r = run(&v).unwrap();
    let p: Vec<_> = r
        .periods
        .iter()
        .filter(|p| p.relationship_id == "cfh-purchases")
        .collect();
    assert!(p.iter().all(|x| x.assessment_passed));
    assert!(p
        .iter()
        .any(|x| x.cumulative_ratio.is_some_and(|y| (y - 1.).abs() > 1e-6)));
    // A narrow band fails: the period change goes to earnings and the hedge is discontinued.
    v["policy"]["assessment_lower_bound"] = json!(0.99);
    v["policy"]["assessment_upper_bound"] = json!(1.01);
    let r = run(&v).unwrap();
    let failed = r.periods.iter().find(|p| !p.assessment_passed).unwrap();
    close(failed.earnings, failed.instrument_change);
    assert!(pnl(&r, "pnl:commodity_swap_mark") != 0.);
    assert_eq!(r.relationships[0].status, "discontinued");
}

#[test]
fn a_forecast_probable_not_to_occur_releases_its_oci_at_once() {
    let mut v = raw();
    v["events"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["delivery_id"] != json!("apr"));
    v["events"].as_array_mut().unwrap().push(json!({
        "event_id": "apr-cancelled", "ordinal": 738946, "kind": "forecast_status",
        "review": {"id": "forecast-review", "revision": "r1"},
        "exposure_id": "pe-purchases", "delivery_id": "apr", "status": "probable_not_to_occur"
    }));
    let r = run(&v).unwrap();
    let apr = delivery(&r, "apr");
    assert_eq!(apr.status, "probable_not_to_occur");
    close(apr.oci_balance, 0.);
    // OCI built through 1 March is released; later April-leg changes go to earnings.
    close(
        pnl(&r, "pnl:missed_forecast_reclassification"),
        apr.oci_recognized,
    );
    assert!(apr.oci_recognized > 0. && pnl(&r, "pnl:commodity_swap_mark") != 0.);
}

#[test]
fn eligibility_and_inputs_are_checked_before_replay() {
    let cases: Vec<Edit> = vec![
        // A clearly-and-closely-related component needs the entity's ASU 2025-09 adoption.
        Box::new(|r| {
            r["policy"]
                .as_object_mut()
                .unwrap()
                .remove("asu_2025_09_adoption");
        }),
        Box::new(|r| r["policy"]["asu_2025_09_adoption"]["adoption_ordinal"] = json!(738946)),
        Box::new(|r| {
            r["exposures"][0]["component"] = json!({"kind": "subcomponent", "index_id": "pe-gulf",
                "parent_index_id": "pe-gulf", "contract": {"id": "c", "revision": "r1"},
                "evidence": {"id": "e", "revision": "r1"}})
        }),
        Box::new(|r| r["instruments"][0]["side"] = json!("pay_index")),
        Box::new(|r| r["relationships"][0]["legs"][0]["delivery_id"] = json!("apr")),
        Box::new(|r| r["instruments"][0]["legs"][0]["quantity"] = json!(150.)),
        Box::new(|r| r["relationships"][1]["designation_ordinal"] = json!(739038)),
        Box::new(|r| r["events"][0]["fraction"] = json!(0.)),
        Box::new(|r| r["events"][0]["ordinal"] = json!(738917)),
        Box::new(|r| r["dates"][1]["indices"][0]["forwards"] = json!([])),
        Box::new(|r| r["unknown"] = json!(1)),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut v = raw();
        change(&mut v);
        assert!(run(&v).is_err(), "case {i} must be refused");
    }
    // A contractually specified component needs no adoption.
    let mut v = raw();
    v["policy"]
        .as_object_mut()
        .unwrap()
        .remove("asu_2025_09_adoption");
    v["exposures"][0]["component"] = json!({"kind": "contractually_specified", "index_id": "pe-gulf",
        "contract": {"id": "purchase-formula", "revision": "r1"}});
    assert!(run(&v).is_ok());
}
