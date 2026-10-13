use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::{
    market_indexed_books::{Portfolio, Request},
    term_protocol::respond,
};
use serde_json::{json, Value};
fn raw() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/market_indexed_books_actual_grid.json"
    ))
    .unwrap()
}
fn typed(v: Value) -> Request {
    serde_json::from_value(v).unwrap()
}
#[test]
fn actual_production_grid_keeps_quote_and_original_oas() {
    let r = typed(raw());
    let mut owner = Portfolio::new(&r, 8 * 1024 * 1024, 1024).unwrap();
    let initial = owner.price(&r).unwrap();
    let warm = owner.price(&r).unwrap();
    assert_eq!(initial["positions"], warm["positions"]);
    assert!(initial["market_indexed"]["admitted_work"].as_u64().unwrap() <= 60_000_000);
    assert!(
        initial["market_indexed"]["admitted_bytes"]
            .as_u64()
            .unwrap()
            <= 128 * 1024 * 1024
    );
    for binding in r.bindings.unwrap() {
        let actual = initial["positions"]["loans"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == binding.position_id)
            .unwrap();
        assert_eq!(
            actual["base_oas_bp"].as_f64().unwrap(),
            binding.graph.source.bridge.original.locked_oas * 1e4
        );
        let target = binding.original_row.price * binding.original_row.notional / 100.;
        assert!(
            (actual["market_value"].as_f64().unwrap() - target).abs()
                <= 1e-9 + 1e-12 * target.abs()
        );
    }
}
#[test]
fn actual_grid_typical_path_count_refuses_before_projection() {
    let mut r = raw();
    for which in ["base", "current"] {
        r["legacy"][which]["market"]["config"]["paths"] = json!(8);
    }
    let error = Portfolio::new(&typed(r), 8 * 1024 * 1024, 1024)
        .err()
        .unwrap();
    assert!(
        error.contains("aggregate") || error.contains("admission"),
        "{error}"
    );
}
#[test]
fn caller_control_reaches_real_dispatcher_without_success() {
    for cancelled in [true, false] {
        let control = Control::new(if cancelled { 1000 } else { 1 }).unwrap();
        let active = control.clone();
        let mut response = None;
        let error=scope(Some(control),|| {
            if cancelled {active.cancel()} else {std::thread::sleep(std::time::Duration::from_millis(2));}
            let request=json!({"schema":"graph-1","threads":2,"request":{"op":"create_market_indexed_books","max_bytes":0,"max_entries":0,"request":raw()}});
            response=Some(respond(&serde_json::to_vec(&request).unwrap()));Ok(())
        }).unwrap_err();
        assert!(error.contains(if cancelled {
            "COMPUTE_CANCELLED"
        } else {
            "COMPUTE_DEADLINE_EXCEEDED"
        }));
        let (bytes, ok) = response.unwrap();
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().contains(&error));
    }
}
