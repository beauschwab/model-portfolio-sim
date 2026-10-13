//! Additive per-call execution capability. No financial inputs or cached deadlines.
use portfolio_compute_control::Control;
use std::collections::HashMap;
use std::ffi::{c_char, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Separate bounded execution registry: at most 256 small token/flag entries.
/// 128 KiB conservative retained allocation allowance, independent of graph budgets.
const MAX_TOKENS: usize = 256;
const MAX_REGISTRY_BYTES: usize = 128 * 1024;
struct Token {
    cancelled: Arc<AtomicBool>,
    phase: AtomicU8,
}
static TOKENS: OnceLock<Mutex<HashMap<u64, Arc<Token>>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
fn registry() -> &'static Mutex<HashMap<u64, Arc<Token>>> {
    TOKENS.get_or_init(|| Mutex::new(HashMap::with_capacity(MAX_TOKENS)))
}
fn entry(id: u64) -> Result<Arc<Token>, String> {
    registry()
        .lock()
        .map_err(|_| "graph runtime registry poisoned")?
        .get(&id)
        .cloned()
        .ok_or_else(|| "GRAPH_CONTROL_UNKNOWN_TOKEN".into())
}
pub fn create_token() -> Result<u64, String> {
    let mut tokens = registry()
        .lock()
        .map_err(|_| "graph runtime registry poisoned")?;
    // Conservative allocation accounting: table capacity plus token/Arc flags,
    // fourfold allocator/bucket slack, and fixed registry metadata. No raw inputs.
    let table = tokens
        .capacity()
        .checked_add(1)
        .and_then(|n| n.checked_mul(std::mem::size_of::<(u64, Arc<Token>)>() + 1))
        .and_then(|n| n.checked_mul(4))
        .ok_or("GRAPH_CONTROL_REGISTRY_BYTES")?;
    let entries = MAX_TOKENS
        .checked_mul(
            std::mem::size_of::<Token>()
                + std::mem::size_of::<AtomicBool>()
                + 4 * std::mem::size_of::<usize>(),
        )
        .and_then(|n| n.checked_mul(4))
        .ok_or("GRAPH_CONTROL_REGISTRY_BYTES")?;
    if table
        .checked_add(entries)
        .and_then(|n| n.checked_add(4096))
        .is_none_or(|n| n > MAX_REGISTRY_BYTES)
    {
        return Err("GRAPH_CONTROL_REGISTRY_BYTES".into());
    }
    if tokens.len() >= MAX_TOKENS {
        return Err("GRAPH_CONTROL_TOKEN_LIMIT".into());
    }
    let id = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |x| x.checked_add(1))
        .map_err(|_| "graph runtime token exhausted")?;
    tokens.insert(
        id,
        Arc::new(Token {
            cancelled: Arc::new(AtomicBool::new(false)),
            phase: AtomicU8::new(0),
        }),
    );
    Ok(id)
}
pub fn signal(id: u64) -> Result<(), String> {
    let token = entry(id)?;
    if token.phase.load(Ordering::Acquire) == 2 {
        return Err("GRAPH_CONTROL_FINISHED".into());
    }
    token.cancelled.store(true, Ordering::Release);
    Ok(())
}
pub fn release(id: u64) -> Result<(), String> {
    let mut tokens = registry()
        .lock()
        .map_err(|_| "graph runtime registry poisoned")?;
    let token = tokens.get(&id).ok_or("GRAPH_CONTROL_UNKNOWN_TOKEN")?;
    if token.phase.load(Ordering::Acquire) == 1 {
        return Err("GRAPH_CONTROL_RUNNING".into());
    }
    tokens.remove(&id);
    Ok(())
}
fn error_bytes(error: String) -> Vec<u8> {
    #[derive(serde::Serialize)]
    struct Failure {
        ok: bool,
        error: String,
    }
    serde_json::to_vec(&Failure { ok: false, error }).expect("error JSON")
}
fn call<T>(
    input: &[u8],
    id: u64,
    timeout_ms: u64,
    handoff: impl FnOnce(Vec<u8>) -> Result<T, String>,
) -> Result<T, String> {
    entry(id).and_then(|token| {
        // Claim and release atomically with respect to registry removal.
        {
            let tokens = registry()
                .lock()
                .map_err(|_| "graph runtime registry poisoned")?;
            if !tokens.get(&id).is_some_and(|x| Arc::ptr_eq(x, &token)) {
                return Err("GRAPH_CONTROL_UNKNOWN_TOKEN".into());
            }
            token
                .phase
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| "GRAPH_CONTROL_TOKEN_REUSED")?;
        }
        struct Finish(Arc<Token>);
        impl Drop for Finish {
            fn drop(&mut self) {
                self.0.phase.store(2, Ordering::Release)
            }
        }
        let _finish = Finish(token.clone());
        let control = Control::with_cancel_flag(timeout_ms, token.cancelled.clone())?;
        let mut created = None;
        let output = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let bytes = crate::term_protocol::respond_controlled_graph(
                input,
                control.clone(),
                &mut created,
            )?;
            control.check()?;
            let result = handoff(bytes)?;
            control.check()?;
            Ok::<_, String>(result)
        }))
        .unwrap_or_else(|_| Err("native controlled graph request failed validation".into()));
        if output.is_err() {
            if let Some(handle) = created {
                crate::incremental::retire_unpublished(handle)?;
            }
        }
        output
    })
}
pub fn respond(input: &[u8], id: u64, timeout_ms: u64) -> (Vec<u8>, bool) {
    let result = call(input, id, timeout_ms, Ok);
    match result {
        Ok(bytes) => (bytes, true),
        Err(error) => (error_bytes(error), false),
    }
}
#[no_mangle]
pub extern "C" fn portfolio_graph_runtime_version() -> u32 {
    1
}
#[no_mangle]
pub extern "C" fn portfolio_graph_cancel_new() -> u64 {
    create_token().unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn portfolio_graph_cancel_signal(id: u64) -> i32 {
    if signal(id).is_ok() {
        1
    } else {
        0
    }
}
#[no_mangle]
pub extern "C" fn portfolio_graph_cancel_release(id: u64) -> i32 {
    if release(id).is_ok() {
        1
    } else {
        0
    }
}
/// Controlled call; result is freed with the unchanged portfolio_term_free.
/// # Safety
/// input must reference length live bytes for this synchronous invocation.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_graph_controlled(
    input: *const u8,
    length: usize,
    id: u64,
    timeout_ms: u64,
) -> *mut c_char {
    let result = if input.is_null() || length > crate::term_protocol::MAX_INPUT {
        // Even malformed input consumes a valid per-call token.
        call(&[], id, timeout_ms, |bytes| {
            CString::new(bytes).map_err(|e| e.to_string())
        })
    } else {
        // SAFETY: caller provides the bounded live range specified above.
        call(
            unsafe { std::slice::from_raw_parts(input, length) },
            id,
            timeout_ms,
            |bytes| CString::new(bytes).map_err(|e| e.to_string()),
        )
    };
    result
        .unwrap_or_else(|error| CString::new(error_bytes(error)).expect("JSON contains no NUL"))
        .into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::time::Duration;
    static SERIAL: Mutex<()> = Mutex::new(());
    fn envelope(request: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({"schema":"graph-1","threads":2,"request":request})).unwrap()
    }
    fn create() -> Vec<u8> {
        envelope(
            json!({"op":"create_market_indexed_books","max_bytes":8388608,"max_entries":1024,"request":serde_json::from_str::<Value>(include_str!("../tests/fixtures/market_indexed_books.json")).unwrap()}),
        )
    }
    fn uncontrolled(input: &[u8]) -> Value {
        let (bytes, ok) = crate::term_protocol::respond(input);
        assert!(ok, "{}", String::from_utf8_lossy(&bytes));
        serde_json::from_slice(&bytes).unwrap()
    }
    fn handle() -> u64 {
        uncontrolled(&create())["result"]["handle"]
            .as_u64()
            .unwrap()
    }
    fn drop_owner(h: u64) {
        uncontrolled(&envelope(json!({"op":"drop","handle":h})));
    }
    fn controlled(input: &[u8], ms: u64) -> (Vec<u8>, bool) {
        let token = create_token().unwrap();
        let result = respond(input, token, ms);
        release(token).unwrap();
        result
    }
    #[test]
    fn additive_c_abi_returns_and_frees_response_once() {
        let _serial = SERIAL.lock().unwrap();
        assert_eq!(portfolio_graph_runtime_version(), 1);
        let id = portfolio_graph_cancel_new();
        assert_ne!(id, 0);
        let input = create();
        let pointer =
            unsafe { portfolio_term_graph_controlled(input.as_ptr(), input.len(), id, 10000) };
        let value: Value = unsafe {
            serde_json::from_slice(std::ffi::CStr::from_ptr(pointer).to_bytes()).unwrap()
        };
        assert_eq!(value["ok"], true);
        let h = value["result"]["handle"].as_u64().unwrap();
        unsafe { crate::term_protocol::portfolio_term_free(pointer) };
        assert_eq!(portfolio_graph_cancel_release(id), 1);
        drop_owner(h);
    }
    #[test]
    fn bounded_fresh_finished_registry_and_reuse() {
        let _serial = SERIAL.lock().unwrap();
        let ids: Vec<_> = (0..MAX_TOKENS).map(|_| create_token().unwrap()).collect();
        assert_eq!(create_token().unwrap_err(), "GRAPH_CONTROL_TOKEN_LIMIT");
        let id = ids[0];
        assert!(!respond(b"{}", id, 1000).1);
        assert_eq!(signal(id).unwrap_err(), "GRAPH_CONTROL_FINISHED");
        assert!(String::from_utf8(respond(b"{}", id, 1000).0)
            .unwrap()
            .contains("TOKEN_REUSED"));
        for id in ids {
            release(id).unwrap()
        }
        assert_eq!(signal(id).unwrap_err(), "GRAPH_CONTROL_UNKNOWN_TOKEN");
    }
    #[test]
    fn precancel_and_invalid_deadline_consume_no_owner() {
        let _serial = SERIAL.lock().unwrap();
        let id = create_token().unwrap();
        signal(id).unwrap();
        let (bytes, ok) = respond(&create(), id, 1000);
        assert!(!ok);
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("COMPUTE_CANCELLED"));
        release(id).unwrap();
        let (bytes, ok) = controlled(&create(), 0);
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().contains("timeout_ms"));
    }
    #[test]
    fn unchanged_info_bytes_and_fresh_warm_deadline() {
        let _serial = SERIAL.lock().unwrap();
        let h = handle();
        let info = envelope(json!({"op":"info","handle":h}));
        let old = crate::term_protocol::respond(&info);
        let new = controlled(&info, 1000);
        assert_eq!(old, new);
        std::thread::sleep(Duration::from_millis(3));
        assert_eq!(old, controlled(&info, 1000));
        drop_owner(h);
    }
    #[test]
    fn nonempty_cold_warm_financial_response_bytes_are_unchanged() {
        let _serial = SERIAL.lock().unwrap();
        let a = handle();
        let b = handle();
        let raw: Value =
            serde_json::from_str(include_str!("../tests/fixtures/market_indexed_books.json"))
                .unwrap();
        for _ in 0..2 {
            let old = crate::term_protocol::respond(&envelope(
                json!({"op":"price_market_indexed_books","handle":a,"request":raw}),
            ));
            let new = controlled(
                &envelope(json!({"op":"price_market_indexed_books","handle":b,"request":raw})),
                10000,
            );
            assert_eq!(old, new);
        }
        let mut bad = raw;
        bad["bindings"][0]["current_row"]["price"] = json!(0);
        let old = crate::term_protocol::respond(&envelope(
            json!({"op":"price_market_indexed_books","handle":a,"request":bad}),
        ));
        let new = controlled(
            &envelope(json!({"op":"price_market_indexed_books","handle":b,"request":bad})),
            10000,
        );
        assert!(!old.1);
        assert_eq!(old, new);
        drop_owner(a);
        drop_owner(b);
    }
    #[test]
    fn wrong_owner_and_legacy_operations_fail_closed() {
        let _serial = SERIAL.lock().unwrap();
        let old = uncontrolled(&envelope(
            json!({"op":"create","max_bytes":1000,"max_entries":1}),
        ))["result"]["handle"]
            .as_u64()
            .unwrap();
        let (bytes, ok) = controlled(&envelope(json!({"op":"info","handle":old})), 1000);
        assert!(!ok);
        assert!(String::from_utf8(bytes).unwrap().contains("OWNER_MISMATCH"));
        drop_owner(old);
        let h = handle();
        let (bytes, ok) = controlled(&envelope(json!({"op":"drop","handle":h})), 1000);
        assert!(!ok);
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("EXPLICIT_INDEXED_OPERATION"));
        drop_owner(h);
    }
    #[test]
    fn actual_owner_lock_wait_is_cancellable_and_deadlined() {
        let _serial = SERIAL.lock().unwrap();
        let h = handle();
        for cancelled in [true, false] {
            let id = create_token().unwrap();
            let info = envelope(json!({"op":"info","handle":h}));
            crate::incremental::with_test_indexed_owner_lock(h, || {
                let task = std::thread::spawn(move || {
                    respond(&info, id, if cancelled { 1000 } else { 10 })
                });
                while entry(id).unwrap().phase.load(Ordering::Acquire) == 0 {
                    std::thread::yield_now()
                }
                assert_eq!(release(id).unwrap_err(), "GRAPH_CONTROL_RUNNING");
                if cancelled {
                    signal(id).unwrap()
                }
                let (bytes, ok) = task.join().unwrap();
                assert!(!ok);
                assert!(String::from_utf8(bytes).unwrap().contains(if cancelled {
                    "COMPUTE_CANCELLED"
                } else {
                    "COMPUTE_DEADLINE_EXCEEDED"
                }));
            });
            release(id).unwrap();
        }
        drop_owner(h);
    }
    #[test]
    fn late_cancel_retires_unpublished_created_handle() {
        let _serial = SERIAL.lock().unwrap();
        let id = create_token().unwrap();
        let mut h = 0;
        let error = call(&create(), id, 10000, |bytes| {
            h = serde_json::from_slice::<Value>(&bytes).unwrap()["result"]["handle"]
                .as_u64()
                .unwrap();
            signal(id)?;
            Ok(bytes)
        })
        .unwrap_err();
        assert!(error.contains("COMPUTE_CANCELLED"));
        release(id).unwrap();
        let (bytes, ok) = crate::term_protocol::respond(&envelope(json!({"op":"info","handle":h})));
        assert!(!ok);
        assert!(String::from_utf8(bytes)
            .unwrap()
            .contains("unknown graph cache handle"));
    }
    #[test]
    fn late_handoff_failure_and_deadline_retire_new_owner() {
        let _serial = SERIAL.lock().unwrap();
        for timeout in [false, true] {
            let id = create_token().unwrap();
            let mut h = 0;
            let error = call(&create(), id, 1000, |bytes| {
                h = serde_json::from_slice::<Value>(&bytes).unwrap()["result"]["handle"]
                    .as_u64()
                    .unwrap();
                if timeout {
                    std::thread::sleep(Duration::from_millis(1010));
                    Ok(bytes)
                } else {
                    Err("broken handoff".into())
                }
            })
            .unwrap_err();
            assert!(error.contains(if timeout {
                "COMPUTE_DEADLINE_EXCEEDED"
            } else {
                "broken handoff"
            }));
            release(id).unwrap();
            assert!(!crate::term_protocol::respond(&envelope(json!({"op":"info","handle":h}))).1);
        }
    }
    #[test]
    fn independent_parallel_tokens_do_not_cross_cancel() {
        let _serial = SERIAL.lock().unwrap();
        let h = handle();
        let a = create_token().unwrap();
        let b = create_token().unwrap();
        signal(a).unwrap();
        let info = envelope(json!({"op":"info","handle":h}));
        let other = info.clone();
        let ta = std::thread::spawn(move || respond(&info, a, 1000));
        let tb = std::thread::spawn(move || respond(&other, b, 1000));
        assert!(!ta.join().unwrap().1);
        assert!(tb.join().unwrap().1);
        release(a).unwrap();
        release(b).unwrap();
        drop_owner(h);
    }
}
