use portfolio_model_core::consumer_behavior::{Fitted, Product};
use portfolio_risk_native::models::consumer_behavior::{evaluate, Request};

fn request() -> Request {
    serde_json::from_str(include_str!(
        "../../../examples/models/consumer-behaviour-spec-1.json"
    ))
    .unwrap()
}

#[test]
fn three_product_panels_fit_and_project_complete_shared_paths() {
    for product in [Product::Auto, Product::Personal, Product::CreditCard] {
        let mut r = request();
        r.fit.specification.product = product;
        if product == Product::CreditCard {
            r.fit.specification.default_dpd = 180;
        }
        let out = evaluate(&r).unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= out.reserved_response_bytes);
        assert!(!out.adoption_eligible && !out.empirical_accuracy_validated);
        let Fitted::Exact { result } = &out.fitted.fitted else {
            panic!()
        };
        assert!(result.model.numerical_eligible);
        assert!(!result.model.adoption_eligible);
        let projected = out.projection.unwrap();
        let default = projected.paths.paths[0].default.as_ref().unwrap();
        assert!((default.year - 0.6).abs() < 1e-8);
        assert!((default.economic_principal_loss - 44.).abs() < 1e-12);
        assert!(default.recoveries[0].year > 2.);
    }
}
#[test]
fn public_entry_retains_original_clock_and_strict_nested_fields() {
    use portfolio_risk_native::term_protocol::{respond, respond_started};
    let envelope = serde_json::json!({"schema":"consumer-behaviour-spec-1","threads":1,"execution_timeout_ms":30000,"request":request()});
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let (output, ok) = respond(&bytes);
    assert!(ok, "{}", String::from_utf8_lossy(&output));
    let duplicate = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("\"watch_dpd\":30", "\"watch_dpd\":30,\"watch_dpd\":60");
    let (output, ok) = respond(duplicate.as_bytes());
    assert!(!ok && String::from_utf8_lossy(&output).contains("duplicate"));
    let past = std::time::Instant::now() - std::time::Duration::from_secs(31);
    let (output, ok) = respond_started(&bytes, past);
    assert!(!ok && String::from_utf8_lossy(&output).contains("DEADLINE"));
}
#[test]
fn cancellation_precedes_fit_and_projection() {
    let control = portfolio_compute_control::Control::new(30000).unwrap();
    control.cancel();
    let error =
        portfolio_compute_control::scope(Some(control), || evaluate(&request())).unwrap_err();
    assert!(error.contains("CANCELLED"));
}

#[test]
fn joined_admission_rejects_before_running_an_invalid_fit() {
    let mut r = request();
    r.fit.specification.columns.clear(); // Would fail mapping if fitting began.
    let projection = r.projection.as_mut().unwrap();
    projection.path_source.paths = vec![projection.path_source.paths[0].clone(); 128];
    projection.path_source.max_jumps_per_path = 128;
    let error = evaluate(&r).unwrap_err();
    assert!(error.contains("complete path output admission"), "{error}");
}
