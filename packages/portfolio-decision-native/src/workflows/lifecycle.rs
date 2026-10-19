//! Decision preflight for `liquidity-lifecycle-1` (stage A, issue #43). Admission runs before
//! any financial generation: missing or duplicate owners, unknown fields, overallocated shared
//! resources and cyclic settlement dependencies refuse with a typed code. Acquisition and unwind
//! coordination (stages B and C) build on an admitted plan only.
use portfolio_model_core::lifecycle::{self, Admitted, Refusal};
use serde_json::{json, Value};

pub fn preflight(bytes: &[u8]) -> Result<Admitted, Refusal> {
    lifecycle::admit(bytes)
}

/// Process envelope: an admitted summary, or the refusal code and detail.
pub fn preflight_json(bytes: &[u8]) -> Value {
    match preflight(bytes) {
        Ok(plan) => {
            let r = &plan.request;
            json!({"ok": true, "schema": r.schema, "lots": r.lots.len(), "actions": r.actions.len(),
                   "obligations": r.obligations.len(), "formal_relationships": r.formal_relationships.len()})
        }
        Err(refusal) => json!({"ok": false, "code": refusal.code, "detail": refusal.detail}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str =
        include_str!("../../../portfolio-model-core/tests/fixtures/lifecycle/unwind-bridge.json");

    #[test]
    fn preflight_admits_the_worked_unwind_and_refuses_overallocation_before_generation() {
        let ok = preflight_json(FIXTURE.as_bytes());
        assert_eq!(
            (
                ok["ok"].as_bool(),
                ok["lots"].as_u64(),
                ok["obligations"].as_u64()
            ),
            (Some(true), Some(3), Some(9))
        );
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        v["funding_allocations"][0]["amount"] = json!(200.0);
        let refused = preflight_json(&serde_json::to_vec(&v).unwrap());
        assert_eq!(refused["code"], "OVERALLOCATION");
        let duplicate = FIXTURE.replacen(
            "\"currency\": \"USD\",",
            "\"currency\": \"USD\", \"currency\": \"EUR\",",
            1,
        );
        assert_eq!(
            preflight_json(duplicate.as_bytes())["code"],
            "MALFORMED_INPUT"
        );
    }
}
