use portfolio_risk_native::term_protocol::respond;
use serde_json::{json, Value};

fn request() -> Value {
    serde_json::from_str(include_str!(
        "../../../examples/models/observed-deposit-calibration-1.json"
    ))
    .unwrap()
}

#[test]
fn public_dispatch_returns_behavior_fit_with_holdout_identity() {
    let raw = request();
    let envelope = json!({"schema":"observed-deposit-calibration-1","threads":1,"request":raw});
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope).unwrap());
    assert!(ok, "{}", String::from_utf8_lossy(&bytes));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["result"]["schema"], "observed-deposit-calibration-1");
    assert_eq!(value["result"]["adoption_eligible"], false);
    assert_eq!(value["result"]["training_split_id"], "train-through-2021");
}

#[test]
fn malformed_destination_is_rejected_before_output() {
    let mut raw = request();
    raw["spells"][0]["ending"]["destination"] = json!("unknown");
    let envelope = json!({"schema":"observed-deposit-calibration-1","threads":1,"request":raw});
    let (bytes, ok) = respond(&serde_json::to_vec(&envelope).unwrap());
    assert!(!ok);
    assert!(String::from_utf8_lossy(&bytes).contains("deposit migration must target"));
}

fn delayed_control(cancel: bool) {
    use portfolio_compute_control::{scope, Control};
    use portfolio_risk_native::with_compute_threads;
    use std::{sync::mpsc, thread, time::Duration};
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = thread::spawn(move || {
        with_compute_threads(1, move || {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(())
        })
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let control = Control::new(if cancel { 1000 } else { 100 }).unwrap();
    let delayed = control.clone();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        if cancel {
            delayed.cancel();
        }
        release_tx.send(()).unwrap();
    });
    let envelope =
        json!({"schema":"observed-deposit-calibration-1","threads":1,"request":request()});
    let mut actual = None;
    let outer = scope(Some(control), || {
        actual = Some(respond(&serde_json::to_vec(&envelope).unwrap()));
        Ok(())
    });
    release.join().unwrap();
    blocker.join().unwrap().unwrap();
    let reason = if cancel {
        "COMPUTE_CANCELLED"
    } else {
        "COMPUTE_DEADLINE_EXCEEDED"
    };
    assert_eq!(outer.unwrap_err(), reason);
    let (bytes, ok) = actual.expect("dispatcher must execute inside caller scope");
    assert!(
        !ok,
        "outer scope cannot hide uncontrolled successful computation"
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["error"], reason);
}

#[test]
fn delayed_caller_cancellation_reaches_real_native_dispatch() {
    delayed_control(true);
}

#[test]
fn delayed_caller_deadline_reaches_real_native_dispatch() {
    delayed_control(false);
}
