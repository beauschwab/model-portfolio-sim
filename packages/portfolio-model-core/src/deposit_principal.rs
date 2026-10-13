//! Optional native deposit allocation receipts. No economics state is supplied by callers.
use crate::deposit_state::{DepositCashEvent, DepositSegment};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrincipalAtom {
    pub allocation_index: usize,
    pub ordinal: i32,
    pub segment_id: String,
    pub kind: String,
    pub principal_before: f64,
    pub principal_after: f64,
    pub exited: Option<f64>,
    pub withdrawal_intensity: Option<f64>,
    pub closure_intensity: Option<f64>,
    pub cash_event_ids: Vec<String>,
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit(
    sink: &mut Option<&mut Vec<PrincipalAtom>>,
    ordinal: i32,
    s: &DepositSegment,
    kind: &str,
    before: f64,
    after: f64,
    exited: Option<f64>,
    w: Option<f64>,
    c: Option<f64>,
    cash: &[DepositCashEvent],
) {
    if let Some(out) = sink.as_deref_mut() {
        out.push(PrincipalAtom {
            allocation_index: out.len(),
            ordinal,
            segment_id: s.segment_id.clone(),
            kind: kind.into(),
            principal_before: before,
            principal_after: after,
            exited,
            withdrawal_intensity: w,
            closure_intensity: c,
            cash_event_ids: cash.iter().map(|v| v.event_id.clone()).collect(),
        });
    }
}
