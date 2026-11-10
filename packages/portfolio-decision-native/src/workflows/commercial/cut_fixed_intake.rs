//! Prospective strict-tree -> typed ordinary cut intake. Financial construction,
//! Sim and packet ownership are separate phases; this is not route authority.
use super::{candidates, cut_mixed};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::transport::value_capacity;
use serde::Serialize;
use serde_json::Value;
use std::mem::size_of;
const MAX_RAW: usize = 2 << 20;
const MAX_BYTES: usize = 128 << 20;
const HEADER: usize = 8192;
type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b).ok_or("fixed intake overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b).ok_or("fixed intake overflow".into())
}
#[derive(Default, Debug, Serialize)]
pub(super) struct TypedShape {
    objects: usize,
    arrays: usize,
    strings: usize,
    primitives: usize,
    string_bytes: usize,
    array_minimum_bytes: usize,
}
impl TypedShape {
    fn visit(&mut self, v: &Value) -> R<()> {
        checkpoint()?;
        match v {
            Value::Object(m) => {
                self.objects = add(self.objects, 1)?;
                for (k, v) in m {
                    self.string_bytes = add(self.string_bytes, k.len())?;
                    self.visit(v)?;
                }
            }
            Value::Array(a) => {
                self.arrays = add(self.arrays, 1)?;
                if !a.is_empty() {
                    let slot = if a.iter().any(Value::is_object) {
                        HEADER
                    } else {
                        size_of::<Value>()
                    };
                    self.array_minimum_bytes = add(self.array_minimum_bytes, mul(8, slot)?)?;
                }
                for v in a {
                    self.visit(v)?;
                }
            }
            Value::String(s) => {
                self.strings = add(self.strings, 1)?;
                self.string_bytes = add(self.string_bytes, s.len())?;
            }
            _ => self.primitives = add(self.primitives, 1)?,
        }
        Ok(())
    }
    fn upper(&self) -> R<usize> {
        // Each typed object fits the pinned 8192-byte header. Its containing
        // Vec gets three slots per live element for old/new growth. Arrays of
        // strings, scalar values and Vec headers are accounted independently.
        // Complete defaulted fixed DTO fields are covered by the header and
        // <=512 bytes of constant payload per object (including empty Arcs in
        // Spec). A double UTF-8 allowance covers String -> Arc conversion.
        [
            mul(self.objects, 3 * HEADER + 512)?,
            mul(self.arrays, 3 * size_of::<Value>())?,
            mul(self.strings, 3 * size_of::<Value>())?,
            mul(self.primitives, 3 * size_of::<Value>())?,
            mul(self.string_bytes, 2)?,
            self.array_minimum_bytes,
            8 * HEADER,
        ]
        .into_iter()
        .try_fold(0, add)
    }
}
#[derive(Debug, Serialize)]
pub(super) struct Capacity {
    pub typed_shape: TypedShape,
    pub typed_bytes_upper: usize,
    pub strict_value_bytes_upper: usize,
    pub input_phase_bytes_upper: usize,
}
pub(super) fn derive(bytes: &[u8], wire_capacity: usize, strict: &Value) -> R<Capacity> {
    if wire_capacity < bytes.len() || wire_capacity > 2 * MAX_RAW {
        return Err("fixed cut wire capacity exceeds existing bounded intake".into());
    }
    let json = value_capacity::estimate(bytes, MAX_RAW, checkpoint)?;
    let mut typed_shape = TypedShape::default();
    typed_shape.visit(strict)?;
    let typed_bytes_upper = typed_shape.upper()?;
    // Value is consumed into typed DTOs. Count its entire old allocation and
    // every prospective typed allocation together; no move/reuse discount.
    let input_phase_bytes_upper = [
        wire_capacity,
        json.value_tree_bytes_upper,
        json.parser_workspace_bytes_upper,
        typed_bytes_upper,
    ]
    .into_iter()
    .try_fold(0, add)?;
    if input_phase_bytes_upper > MAX_BYTES {
        return Err("fixed cut typed input exceeds128MiB before deserialization".into());
    }
    Ok(Capacity {
        typed_shape,
        typed_bytes_upper,
        strict_value_bytes_upper: json.value_tree_bytes_upper,
        input_phase_bytes_upper,
    })
}
/// The fixed inventory does not claim advanced ownership. Those selections
/// retain their historical entrypoint and receipts. Unknown/malformed fields
/// still fail strict typed deserialization, never disappear from a census.
pub(super) fn supports(envelope: &Value) -> bool {
    let raw = &envelope["request"];
    if !raw["preparation_receipt_policy"].is_null() {
        return false;
    }
    for side in ["completed_candidate", "revised_candidate"] {
        let Some(units) = raw[side]["commercial"]["units"].as_array() else {
            return false;
        };
        if units.is_empty() {
            return false;
        }
        for unit in units {
            let Some(positions) = unit["graph"]["positions"].as_array() else {
                return false;
            };
            if positions.len() != 1 {
                return false;
            }
            for p in positions {
                if ["exercise", "generated_policy"]
                    .iter()
                    .any(|k| !p[k].is_null())
                    || p["original_physical"]
                        .as_array()
                        .is_some_and(|a| !a.is_empty())
                {
                    return false;
                }
                if let Some(fees) = p.get("fees").filter(|f| !f.is_null()) {
                    if fees["original_physical"]
                        .as_array()
                        .is_some_and(|a| !a.is_empty())
                    {
                        return false;
                    }
                    let Some(original) = fees["original_pricing"].as_array() else {
                        return false;
                    };
                    let Some(sets) = fees["histories"].as_array() else {
                        return false;
                    };
                    if original
                        .iter()
                        .chain(sets.iter().flat_map(|s| {
                            ["physical", "pricing", "pricing_down", "pricing_up"]
                                .into_iter()
                                .flat_map(move |k| s[k].as_array().into_iter().flatten())
                        }))
                        .any(|h| !h["policy"]["transfer"].is_null())
                    {
                        return false;
                    }
                }
                let Some(originals) = p["original_pricing"].as_array() else {
                    return false;
                };
                let Some(sets) = p["histories"].as_array() else {
                    return false;
                };
                for h in originals.iter().chain(sets.iter().flat_map(|s| {
                    ["physical", "pricing", "pricing_down", "pricing_up"]
                        .into_iter()
                        .flat_map(move |k| s[k].as_array().into_iter().flatten())
                })) {
                    let source = &h["source"];
                    if [
                        "prepayment_terms",
                        "maturity_basis_policy",
                        "reviewed_credit_actions",
                    ]
                    .iter()
                    .any(|k| !source[k].is_null())
                        || source["performing_draw_tranches"]
                            .as_array()
                            .is_some_and(|a| !a.is_empty())
                        || !source["source"]["transfer"].is_null()
                        || !source["factor_policy"]["terminal_obligations"].is_null()
                    {
                        return false;
                    }
                    for clock in ["original", "current"] {
                        let c = &source["source"][clock];
                        let Some(periods) = c["periods"].as_array() else {
                            return false;
                        };
                        if c["principal_plan"]["kind"] != "supplied"
                            || periods.iter().any(|p| p["coupon"]["kind"] != "fixed")
                        {
                            return false;
                        }
                    }
                }
            }
        }
    }
    true
}
// Wrapper types outside the exhaustive financial raw inventory. Any growth
// beyond the prospective per-object header fails compilation.
const _: () = {
    assert!(size_of::<cut_mixed::Request>() <= HEADER);
    assert!(size_of::<candidates::Request>() <= HEADER);
    assert!(size_of::<crate::workflows::candidates::Owner>() <= HEADER);
    assert!(size_of::<crate::workflows::candidates::RunMapping>() <= HEADER);
    assert!(size_of::<portfolio_risk_native::product_strategy::Allocation>() <= HEADER);
    assert!(size_of::<portfolio_ledger_native::types::Spec>() <= HEADER);
    assert!(size_of::<portfolio_ledger_native::types::Account>() <= HEADER);
    assert!(size_of::<portfolio_ledger_native::types::Position>() <= HEADER);
    assert!(size_of::<portfolio_ledger_native::types::Scenario>() <= HEADER);
};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_original_envelope_is_admitted_before_typed_expansion() {
        let raw: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let envelope = serde_json::json!({"op":"analyze_commercial_cut_mixed_journal","schema":cut_mixed::SCHEMA,"execution_timeout_ms":180000,"request":raw});
        let bytes = serde_json::to_vec(&envelope).unwrap();
        let cap = derive(&bytes, bytes.capacity(), &envelope).unwrap();
        assert!(cap.typed_bytes_upper > 0);
        assert!(cap.input_phase_bytes_upper < 64 << 20);
        let r: cut_mixed::Request = serde_json::from_value(envelope["request"].clone()).unwrap();
        let retained = portfolio_risk_native::commercial_credit_compiler::finite_units_raw_layout(
            &r.completed_candidate.commercial,
        )
        .unwrap()
        .retained_heap_bytes
            + portfolio_risk_native::commercial_credit_compiler::finite_units_raw_layout(
                &r.revised_candidate.commercial,
            )
            .unwrap()
            .retained_heap_bytes;
        assert!(cap.typed_bytes_upper > retained);
    }
    #[test]
    fn repeated_empty_objects_are_not_free_typed_capacity() {
        let value = serde_json::json!({"payload":vec![serde_json::json!({});6000]});
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(derive(&bytes, bytes.capacity(), &value)
            .unwrap_err()
            .contains("before deserialization"));
    }
}
