//! Actual proposed dispatcher caller-control gates. No broad CLI/envelope change.
use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::{term_protocol::respond, with_compute_threads};
use serde_json::{json, Value};
use std::{sync::mpsc, thread, time::Duration};
fn input() -> Vec<u8> {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../examples/models/market-indexed-adapter-1.json"
    ))
    .unwrap();
    serde_json::to_vec(
        &json!({"schema":"market-indexed-adapter-1","threads":1,"request":raw["request"]}),
    )
    .unwrap()
}
fn check_delayed(cancel: bool) {
    // Occupy the real shared single-thread pool so the dispatcher runs only after
    // caller control changes. This is deterministic and cannot pass merely because
    // outer scope checked cancellation after a successful unowned calculation.
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
    let bytes = input();
    let control = Control::new(if cancel { 1000 } else { 100 }).unwrap();
    let delayed = control.clone();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        if cancel {
            delayed.cancel()
        };
        release_tx.send(()).unwrap();
    });
    let mut actual = None;
    let outer = scope(Some(control), || {
        actual = Some(respond(&bytes));
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
    let (bytes, ok) =
        actual.expect("the actual dispatcher was entered before delayed control change");
    assert!(
        !ok,
        "outer control alone must not hide a successful uncontrolled native result"
    );
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["error"], reason);
}
#[test]
fn caller_cancel_is_propagated_into_real_rayon_dispatch() {
    check_delayed(true)
}
#[test]
fn caller_deadline_reason_is_preserved_inside_real_dispatch() {
    check_delayed(false)
}
