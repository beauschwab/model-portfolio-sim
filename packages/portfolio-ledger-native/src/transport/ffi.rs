//! Bounded C buffer and raw JSON validation entrypoints.
use crate::journal::reduction::{apply, INVALID, MAX_KEYS, MAX_LINES, PANIC};
use crate::{credit_path_replay, credit_replay, dated_ledger, spec};
/// Normalize and validate a raw daily specification. The returned buffer is
/// owned by this library and must be released with portfolio_balance_free.
///
/// # Safety
/// Input must reference `length` live bytes during this call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_balance_validate(
    input: *const u8,
    length: usize,
) -> *mut std::ffi::c_char {
    let result = std::panic::catch_unwind(|| -> Result<serde_json::Value, String> {
        if input.is_null() || length > 100_000_000 {
            return Err("null or oversized specification".into());
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            raw: serde_json::Value,
            max_positions: usize,
            max_work: usize,
        }
        let request: Request = serde_json::from_slice(std::slice::from_raw_parts(input, length))
            .map_err(|e| e.to_string())?;
        if request
            .raw
            .get("schema")
            .and_then(serde_json::Value::as_str)
            == Some(credit_path_replay::SCHEMA)
        {
            let raw = serde_json::from_value(request.raw).map_err(|e| e.to_string())?;
            let compiled = credit_path_replay::CompiledPathLedger::compile_with_limits(
                raw,
                request.max_positions,
                request.max_work,
            )?;
            return serde_json::to_value(compiled.normalized_spec()).map_err(|e| e.to_string());
        }
        if request
            .raw
            .get("schema")
            .and_then(serde_json::Value::as_str)
            == Some(credit_replay::SCHEMA)
        {
            let raw = serde_json::from_value(request.raw).map_err(|e| e.to_string())?;
            let compiled = credit_replay::CompiledCreditReplay::compile_with_limits(
                raw,
                request.max_positions,
                request.max_work,
            )?;
            return serde_json::to_value(compiled.normalized_spec()).map_err(|e| e.to_string());
        }
        if request
            .raw
            .get("schema")
            .and_then(serde_json::Value::as_str)
            == Some(dated_ledger::SCHEMA)
        {
            let raw = serde_json::from_value(request.raw).map_err(|e| e.to_string())?;
            let source = dated_ledger::CompiledDatedLedger::compile(
                raw,
                request.max_positions,
                request.max_work,
            )?;
            return serde_json::to_value(source.spec).map_err(|e| e.to_string());
        }
        serde_json::to_value(spec::normalize(
            request.raw,
            request.max_positions,
            request.max_work,
        )?)
        .map_err(|e| e.to_string())
    })
    .unwrap_or_else(|_| Err("native specification validation failed".into()));
    let response = match result {
        Ok(v) => serde_json::json!({"ok":v}),
        Err(e) => serde_json::json!({"error":e}),
    };
    std::ffi::CString::new(response.to_string())
        .unwrap()
        .into_raw()
}
/// # Safety
/// Pointer must be a live, unreleased result of portfolio_balance_validate.
#[no_mangle]
pub unsafe extern "C" fn portfolio_balance_free(pointer: *mut std::ffi::c_char) {
    if !pointer.is_null() {
        drop(std::ffi::CString::from_raw(pointer));
    }
}

#[no_mangle]
pub extern "C" fn portfolio_ledger_abi_version() -> u32 {
    1
}

/// Status: 0 success, 1 invalid input, 2 unbalanced transaction, 3 checkpoint
/// mismatch, 4 overflow, 5 panic. All failures leave output unchanged.
///
/// # Safety
/// Caller supplies live aligned buffers: previous/expected/output n, offsets
/// transactions+1, keys/values lines. Buffers must not overlap; inputs remain
/// immutable and output exclusively writable until return. Pointers (including
/// empty buffers) must be nonnull. Length caps are not pointer validity checks.
/// No pointer is retained. Output is copied only after all validation succeeds.
#[no_mangle]
pub unsafe extern "C" fn portfolio_ledger_apply(
    n: usize,
    transactions: usize,
    lines: usize,
    previous: *const f64,
    offsets: *const i64,
    keys: *const i64,
    values: *const f64,
    expected: *const f64,
    output: *mut f64,
) -> i32 {
    if n > MAX_KEYS
        || lines > MAX_LINES
        || transactions > lines
        || previous.is_null()
        || offsets.is_null()
        || keys.is_null()
        || values.is_null()
        || expected.is_null()
        || output.is_null()
        || !(previous as usize).is_multiple_of(8)
        || !(offsets as usize).is_multiple_of(8)
        || !(keys as usize).is_multiple_of(8)
        || !(values as usize).is_multiple_of(8)
        || !(expected as usize).is_multiple_of(8)
        || !(output as usize).is_multiple_of(8)
    {
        return INVALID;
    }
    std::panic::catch_unwind(|| {
        apply(
            std::slice::from_raw_parts(previous, n),
            std::slice::from_raw_parts(offsets, transactions + 1),
            std::slice::from_raw_parts(keys, lines),
            std::slice::from_raw_parts(values, lines),
            std::slice::from_raw_parts(expected, n),
            std::slice::from_raw_parts_mut(output, n),
        )
    })
    .map_or(PANIC, |r| r.err().unwrap_or(0))
}
