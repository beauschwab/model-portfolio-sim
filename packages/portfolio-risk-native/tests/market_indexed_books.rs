use portfolio_risk_native::{
    market_indexed_books::{Portfolio, Request},
    term_protocol::respond,
};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!("fixtures/market_indexed_books.json")).unwrap()
}
fn typed(r: &Value) -> Request {
    serde_json::from_value(r.clone()).unwrap()
}
fn loan_pv(face: f64, margin: f64) -> f64 {
    let fs = [1., 0.97, 0.94];
    let ds = [0.98, 0.95];
    let mut pv = 0.;
    for i in 0..2 {
        let cash = face * (fs[i] / fs[i + 1] - 1. + margin * 0.5) + if i == 1 { face } else { 0. };
        pv += cash * ds[i] * (-0.015 * (180 * (i + 1)) as f64 / 365.).exp();
    }
    pv
}
#[test]
fn native_mixed_reduction_hand_quote_and_warm_shared_cache() {
    let r = raw();
    let mut owner = Portfolio::new(&typed(&r), 8 * 1024 * 1024, 1024).unwrap();
    let cold = owner.price(&typed(&r)).unwrap();
    let warm = owner.price(&typed(&r)).unwrap();
    assert_eq!(cold["positions"], warm["positions"]);
    assert_eq!(cold["totals"], warm["totals"]);
    let rows = cold["positions"]["loans"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["loan1", "fixed1", "loan2"]
    );
    for (i, face, margin) in [(0, 100., 0.), (2, 200., 0.01)] {
        assert!((rows[i]["market_value"].as_f64().unwrap() - loan_pv(face, margin)).abs() < 1e-9);
    }
    assert!(
        (cold["scope_net_value"].as_f64().unwrap()
            - rows
                .iter()
                .map(|v| v["market_value"].as_f64().unwrap())
                .sum::<f64>())
        .abs()
            < 1e-9
    );
    for src in warm["market_indexed"]["sources"].as_array().unwrap() {
        for c in src["counts"].as_object().unwrap().values() {
            assert_eq!(c["computed"], 0);
        }
    }
    assert_eq!(cold["market_indexed"]["physical_accounting"], false);
}
#[test]
fn invalid_warm_quotes_rows_parents_and_unsupported_metrics_fail() {
    let r = raw();
    let mut p = Portfolio::new(&typed(&r), 8 * 1024 * 1024, 1024).unwrap();
    p.price(&typed(&r)).unwrap();
    for which in [
        "quote",
        "original",
        "row",
        "oas",
        "analytics",
        "spread",
        "order",
        "currency",
        "cap",
        "extras",
    ] {
        let mut bad = r.clone();
        match which {
            "quote" => {
                bad["bindings"][0]["current_row"]["price"] = json!(-1.);
                bad["legacy"]["current"]["terms"][0]["deck"]["contracts"][0]["price"] = json!(-1.);
            }
            "original" => {
                bad["bindings"][0]["original_row"]["price"] = json!(99.);
                bad["legacy"]["base"]["terms"][0]["deck"]["contracts"][0]["price"] = json!(99.);
            }
            "row" => bad["bindings"][0]["current_row"]["notional"] = json!(1.),
            "oas" => {
                bad["bindings"][0]["graph"]["source"]["bridge"]["original"]["locked_oas"] =
                    json!(0.016)
            }
            "analytics" => bad["legacy"]["include_analytics"] = json!(true),
            "spread" => bad["legacy"]["spread_shift"] = json!(0.01),
            "order" => bad["bindings"].as_array_mut().unwrap().reverse(),
            "cap" => bad["bindings"][0]["graph"]["source"]["bridge"]["current_market"]["curves"][1]["discount_factors"][1]=json!(0.001),
            "extras" => bad["balance_sheet_extras"]=json!({"mm":[]}),
            _ => {
                bad["bindings"][0]["graph"]["source"]["bridge"]["current_market"]["curves"][0]
                    ["currency"] = json!("EUR")
            }
        }
        assert!(p.price(&typed(&bad)).is_err(), "{which}");
    }
    let mut changed = r.clone();
    changed["bindings"][0]["current_row"]["price"] = json!(101.);
    changed["legacy"]["current"]["terms"][0]["deck"]["contracts"][0]["price"] = json!(101.);
    assert!(p.price(&typed(&changed)).is_ok());
}
#[test]
fn conflicting_reviewed_original_quote_is_never_fitted() {
    let mut r = raw();
    r["bindings"][0]["original_row"]["price"] = json!(100.);
    r["bindings"][0]["current_row"]["price"] = json!(100.);
    for role in ["base", "current"] {
        r["legacy"][role]["terms"][0]["deck"]["contracts"][0]["price"] = json!(100.);
    }
    let mut owner = Portfolio::new(&typed(&r), 0, 0).unwrap();
    assert!(owner
        .price(&typed(&r))
        .unwrap_err()
        .contains("quote conflicts"));
}
#[test]
fn eviction_and_collateral_edits_preserve_cash_original_oas_and_values() {
    let r = raw();
    let mut warm = Portfolio::new(&typed(&r), 8 * 1024 * 1024, 1024).unwrap();
    let mut zero = Portfolio::new(&typed(&r), 0, 0).unwrap();
    let a = warm.price(&typed(&r)).unwrap();
    let b = zero.price(&typed(&r)).unwrap();
    assert_eq!(a["positions"], b["positions"]);
    let mut changed = r.clone();
    for binding in changed["bindings"].as_array_mut().unwrap() {
        binding["graph"]["source"]["bridge"]["current_market"]["curves"][0]["discount_factors"]
            [1] = json!(0.979);
    }
    let c = warm.price(&typed(&changed)).unwrap();
    let d = zero.price(&typed(&changed)).unwrap();
    assert_eq!(c["positions"], d["positions"]);
    for src in c["market_indexed"]["sources"].as_array().unwrap() {
        assert_eq!(src["counts"]["indexed-cash"]["computed"], 0);
        for case in src["cases"].as_array().unwrap() {
            assert_eq!(case["original_oas"], 0.015);
        }
    }
}
#[test]
fn pre_cancel_and_aggregate_capacity_reject_before_finance() {
    let control = portfolio_compute_control::Control::new(1000).unwrap();
    control.cancel();
    let r = typed(&raw());
    let error =
        portfolio_compute_control::scope(Some(control), || Portfolio::new(&r, 0, 0).map(|_| ()))
            .unwrap_err();
    assert!(error.contains("CANCELLED"));
    let mut bad = raw();
    bad["legacy"]["base"]["market"]["config"]["paths"] = json!(9);
    assert!(Portfolio::new(&typed(&bad), 0, 0).is_err());
    let mut full = raw();
    let template = full["bindings"][0].clone();
    let mut bindings = Vec::new();
    let mut ids = Vec::new();
    let mut rows = Vec::new();
    for i in 0..16 {
        let id = format!("loan-{i}");
        let mut binding = template.clone();
        binding["position_id"] = json!(id);
        binding["graph"]["source"]["bridge"]["contract"]["contract_id"] = json!(id);
        ids.push(id);
        rows.push(binding["original_row"].clone());
        bindings.push(binding);
    }
    full["bindings"] = json!(bindings);
    for role in ["base", "current"] {
        full["legacy"][role]["terms"][0]["ids"] = json!(ids);
        full["legacy"][role]["terms"][0]["deck"]["contracts"] = json!(rows);
    }
    let error = Portfolio::new(&typed(&full), 16 * 1024 * 1024, 4096)
        .err()
        .unwrap();
    assert!(error.contains("aggregate"), "{error}");
}
#[test]
fn actual_registry_one_owner_wrong_owner_and_drop() {
    let r = raw();
    let call = |request: Value| {
        let (bytes, ok) = respond(
            &serde_json::to_vec(&json!({"schema":"graph-1","threads":2,"request":request}))
                .unwrap(),
        );
        (serde_json::from_slice::<Value>(&bytes).unwrap(), ok)
    };
    let (created, ok) = call(
        json!({"op":"create_market_indexed_books","max_bytes":8388608,"max_entries":1024,"request":r}),
    );
    assert!(ok, "{created}");
    let h = created["result"]["handle"].as_u64().unwrap();
    let (info, ok) = call(json!({"op":"info","handle":h}));
    assert!(ok);
    assert_eq!(info["result"]["owner"], "market-indexed-books-1");
    let (_, ok) = call(json!({"op":"price_market_indexed_books","handle":h,"request":r}));
    assert!(ok);
    let (_, ok) =
        call(json!({"op":"price_market_indexed","handle":h,"request":r["bindings"][0]["graph"]}));
    assert!(!ok);
    let (_, ok) = call(json!({"op":"drop","handle":h}));
    assert!(ok);
    let (_, ok) = call(json!({"op":"info","handle":h}));
    assert!(!ok);
}
#[test]
fn complete_curve_identity_is_interned_once_across_native_positions() {
    let mut r = raw();
    for role in ["base", "current"] {
        let book = &mut r["legacy"][role]["terms"][0];
        book["ids"] = json!(["loan1", "loan2"]);
        let contracts = book["deck"]["contracts"].as_array_mut().unwrap();
        contracts.remove(1);
    }
    let mut pooled = Portfolio::new(&typed(&r), 8 * 1024 * 1024, 1024).unwrap();
    let result = pooled.price(&typed(&r)).unwrap();
    let mut standalone_identities = 0;
    for b in r["bindings"].as_array().unwrap() {
        let g: portfolio_risk_native::market_indexed_graph::Request =
            serde_json::from_value(b["graph"].clone()).unwrap();
        let mut owner =
            portfolio_risk_native::market_indexed_graph::Graph::new(&g, 8 * 1024 * 1024, 1024)
                .unwrap();
        let value = owner.price(&g).unwrap();
        standalone_identities += value.cache["identity_entries"].as_u64().unwrap();
        let found = result["positions"]["loans"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == b["position_id"])
            .unwrap();
        assert!(
            (found["market_value"].as_f64().unwrap() - value.cases[1].selected_pv).abs() < 1e-9
        );
    }
    assert!(result["cache"]["identity_entries"].as_u64().unwrap() < standalone_identities);
}
