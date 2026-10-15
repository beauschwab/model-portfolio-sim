//! Bounded original-byte C request/response transport.
use crate::{commercial_intake, dispatch};
use serde_json::json;
use std::ffi::{c_char, CStr, CString};
/// Caller owns the returned string and releases it with decision_free.
///
/// # Safety
/// Input must be a valid NUL-terminated UTF-8 buffer for this call.
#[no_mangle]
pub unsafe extern "C" fn decision_request(input: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if input.is_null() {
            return Err("null input".into());
        }
        let bytes = CStr::from_ptr(input).to_bytes();
        if bytes.len() > 128 * 1024 * 1024 {
            return Err("request exceeds 128 MiB".into());
        }
        let v = commercial_intake::parse(bytes)?;
        dispatch(v)
    })
    .unwrap_or_else(|_| Err("native request failed".into()));
    let value = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e}),
    };
    CString::new(value.to_string()).unwrap().into_raw()
}
/// Pointer must be an unfreed result from decision_request.
///
/// # Safety
/// The caller must not free the pointer twice or pass another allocator's memory.
#[no_mangle]
pub unsafe extern "C" fn decision_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(CString::from_raw(pointer));
    }
}
