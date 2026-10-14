//! Opt-in inclusive wall-time attribution. Absent from ordinary release builds.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
type Totals = BTreeMap<&'static str, (u64, f64)>;
static TOTALS: OnceLock<Mutex<Totals>> = OnceLock::new();
pub struct Span {
    name: &'static str,
    start: Instant,
}
impl Span {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            start: Instant::now(),
        }
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let mut totals = TOTALS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let v = totals.entry(self.name).or_default();
        v.0 += 1;
        v.1 += elapsed;
    }
}
pub fn report() -> serde_json::Value {
    let totals = TOTALS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    serde_json::json!({"inclusive_seconds_and_calls": totals.iter().map(|(name,(calls,seconds))| (*name,serde_json::json!({"calls":calls,"seconds":seconds}))).collect::<BTreeMap<_,_>>()})
}

/// Caller releases the owned diagnostic string with portfolio_term_free.
#[no_mangle]
pub extern "C" fn portfolio_compute_profile() -> *mut std::ffi::c_char {
    std::ffi::CString::new(report().to_string())
        .expect("JSON has no NUL")
        .into_raw()
}
