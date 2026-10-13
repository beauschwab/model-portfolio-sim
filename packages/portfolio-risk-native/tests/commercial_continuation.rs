use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::commercial_continuation::{evaluate, Request};
use serde_json::Value;

fn raw() -> Value {
    serde_json::from_slice(include_bytes!(
        "../../../examples/models/commercial-holding-continuation-1.json"
    ))
    .unwrap()
}
fn run(v: Value) -> Result<Value, String> {
    let request: Request = serde_json::from_value(v).map_err(|e| e.to_string())?;
    scope(Some(Control::new(30000)?), || {
        serde_json::to_value(evaluate(&request)?).map_err(|e| e.to_string())
    })
}
#[test]
fn complete_origin_cut_preserves_gross_rights_original_yield_and_future_postings() {
    let r = raw();
    let v = run(r.clone()).unwrap();
    let opening = &v["openings"][0];
    assert_eq!(v["credit_clock_restarted"], false);
    assert_eq!(opening["book"]["principal"], 100.);
    assert!(opening["book"]["legal_interest"].as_f64().unwrap() > 0.4);
    assert_ne!(
        opening["book"]["gross_carry"],
        r["completed_source"]["source"]["acquisition"]["cost"]
    );
    let gs = v["revised"]["accounting"]["groups"].as_array().unwrap();
    let tail: Vec<_> = opening["subsequent_native_group_indices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &gs[i.as_u64().unwrap() as usize])
        .collect();
    assert!(tail
        .iter()
        .all(|g| g["year"].as_f64().unwrap() > 15. / 365.
            && g["phase"] != "reviewed_opening_snapshot"));
    assert_eq!(
        tail.iter()
            .map(|g| g["cash"].as_f64().unwrap())
            .sum::<f64>()
            .to_bits(),
        opening["subsequent_cash"].as_f64().unwrap().to_bits()
    );
    assert_eq!(
        v["revised"]["raw_request"]["source"]["acquisition"],
        v["completed"]["raw_request"]["source"]["acquisition"]
    );
    assert_eq!(v, run(r).unwrap());
}
#[test]
fn past_coupon_credit_stream_original_cost_and_future_dates_cannot_change() {
    for field in ["coupon", "hazard", "stream", "cost", "date"] {
        let mut r = raw();
        let q = &mut r["revised_source"];
        match field {
            "coupon" => q["source"]["current"]["periods"][0]["coupon"]["annual_rate"] = 0.2.into(),
            "hazard" => q["credit"]["intervals"][0]["hazards_per_year"][0][0] = 0.1.into(),
            "stream" => q["credit"]["draw_stream"]["seed"] = 99.into(),
            "cost" => q["source"]["acquisition"]["cost"] = 101.into(),
            _ => q["source"]["current"]["periods"][1]["payment_ordinal"] = 739000.into(),
        }
        assert!(run(r).unwrap_err().contains("prefix"));
    }
}
#[test]
fn original_control_and_complete_origin_are_required() {
    let mut r = raw();
    r["cut_ordinal"] = 738886.into();
    assert!(run(r).unwrap_err().contains("follow"));
    let r: Request = serde_json::from_value(raw()).unwrap();
    assert!(evaluate(&r).unwrap_err().contains("control"));
    let c = Control::new(30000).unwrap();
    c.cancel();
    assert!(scope(Some(c), || evaluate(&r))
        .unwrap_err()
        .contains("CANCELLED"));
}

#[test]
fn future_hazard_revision_keeps_the_original_waiting_hazard() {
    let mut r = raw();
    for name in ["completed_source", "revised_source"] {
        let credit = &mut r[name]["credit"];
        for interval in credit["intervals"].as_array_mut().unwrap() {
            interval["hazards_per_year"][0][3] = 10.0.into();
        }
        credit["draw_stream"] = serde_json::json!({"kind":"supplied","stream_id":"retained-wait","stream_revision":"1",
            "paths":[{"path_key":17,"pairs":[{"waiting_uniform":(-10.0_f64*20.0/365.0).exp(),"destination_uniform":0.5}]}]});
    }
    r["revised_source"]["credit"]["intervals"][1]["hazards_per_year"][0][3] = 20.0.into();
    let v = run(r).unwrap();
    let original = v["completed"]["carry"]["credit"]["result"]["paths"][0]["default"]["year"]
        .as_f64()
        .unwrap();
    let revised = v["revised"]["carry"]["credit"]["result"]["paths"][0]["default"]["year"]
        .as_f64()
        .unwrap();
    assert!((original - 20.0 / 365.0).abs() < 1e-14);
    assert!((revised - 17.5 / 365.0).abs() < 1e-14);
}

#[test]
fn available_fixings_and_oversized_complete_prefixes_reject() {
    let mut r = raw();
    r["revised_source"]["source"]["current"]["periods"][1]["coupon"]["observations"][0]
        ["fixing_ordinal"] = 738886.into();
    assert!(run(r).unwrap_err().contains("prefix"));
    let mut r = raw();
    for name in ["completed_source", "revised_source"] {
        r[name]["credit"]["paths"] = serde_json::json!((0..128).map(|i|serde_json::json!({"path_id":format!("p{i}"),"path_key":i,"initial_live_state":"performing"})).collect::<Vec<_>>());
    }
    let error = run(r).unwrap_err();
    assert!(
        error.contains("bounded explicitly measure-matched native factor source"),
        "{error}"
    );
}
