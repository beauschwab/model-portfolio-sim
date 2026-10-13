//! Real dispatcher gates: caller controls must reach the Rayon-owned tree.
use portfolio_compute_control::{scope, Control};
use portfolio_risk_native::{term_protocol::respond, with_compute_threads};
use serde_json::{json, Value};
use std::{sync::mpsc, thread, time::Duration};

fn delayed(cancel: bool) {
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
    let fixture: Value =
        serde_json::from_str(include_str!("../../../examples/models/term-exercise.json")).unwrap();
    let input = serde_json::to_vec(&json!({
        "schema":"conditional-exercise-1", "threads":1, "request":fixture["request"]
    }))
    .unwrap();
    let control = Control::new(if cancel { 1000 } else { 100 }).unwrap();
    let trigger = control.clone();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        if cancel {
            trigger.cancel();
        }
        release_tx.send(()).unwrap();
    });
    let mut actual = None;
    let outer = scope(Some(control), || {
        actual = Some(respond(&input));
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
    let (response, ok) = actual.unwrap();
    assert!(
        !ok,
        "outer cancellation cannot conceal a successful uncontrolled calculation"
    );
    let value: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(value["error"], reason);
}

#[test]
fn cancellation_crosses_real_rayon_dispatch() {
    delayed(true);
}
#[test]
fn deadline_crosses_real_rayon_dispatch() {
    delayed(false);
}
