//! Explicit receipt lifetimes. Encoded receipts contain the complete native
//! preparation, but no longer retain its financial graph owners.
use super::candidates;
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Policy {
    #[serde(rename = "encode_native_preparations_before_value_decode-1")]
    EncodeBeforeDecode,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Preparation {
    Native(Box<candidates::Preparation>),
    Encoded(Box<RawValue>),
}
impl Preparation {
    pub(super) fn encode_view(
        view: &candidates::PreparationView<'_>,
        original: Option<&Value>,
        maximum: usize,
    ) -> Result<(Self, usize), String> {
        let view = candidates::PreparationView {
            raw_request: original.unwrap_or(view.raw_request),
            ..*view
        };
        checkpoint()?;
        let bytes = strict_json::encode(&view, maximum)?;
        let count = bytes.len();
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let receipt = RawValue::from_string(text).map_err(|e| e.to_string())?;
        checkpoint()?;
        Ok((Self::Encoded(receipt), count))
    }

    pub(super) fn retain(
        mut native: candidates::Preparation,
        original: Option<&Value>,
        selected: bool,
        maximum: usize,
    ) -> Result<Self, String> {
        checkpoint()?;
        if let Some(original) = original {
            native.raw_request = original.clone();
        }
        if !selected {
            return Ok(Self::Native(Box::new(native)));
        }
        let bytes = strict_json::encode(&native, maximum)?;
        // Drop every graph, financial source and coefficient owner before the
        // caller can start table decoding. RawValue owns only this JSON text.
        drop(native);
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let receipt = RawValue::from_string(text).map_err(|e| e.to_string())?;
        checkpoint()?;
        Ok(Self::Encoded(receipt))
    }

    pub(super) fn replace_original(&mut self, original: Value) -> Result<(), String> {
        match self {
            Self::Native(native) => {
                native.raw_request = original;
                Ok(())
            }
            Self::Encoded(_) => Err("encoded native cut receipt is already immutable".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Retention {
    pub owned_wire_parse_phase_bytes: usize,
    pub source_owner_phase_bytes: usize,
    pub raw_validation_phase_bytes: usize,
    pub decoded_tables_phase_bytes: usize,
    pub final_value_phase_bytes: usize,
}
impl Retention {
    pub(super) fn with_sim_batch(mut self, bytes: usize) -> Result<Self, String> {
        self.source_owner_phase_bytes = add(self.source_owner_phase_bytes, bytes)?;
        Ok(self)
    }
    pub(super) fn with_descriptors(mut self, bytes: usize) -> Result<Self, String> {
        self.source_owner_phase_bytes = add(self.source_owner_phase_bytes, bytes)?;
        self.decoded_tables_phase_bytes = add(self.decoded_tables_phase_bytes, bytes)?;
        Ok(self)
    }
    pub(super) fn peak(self) -> usize {
        self.source_owner_phase_bytes
            .max(self.owned_wire_parse_phase_bytes)
            .max(self.raw_validation_phase_bytes)
            .max(self.decoded_tables_phase_bytes)
            .max(self.final_value_phase_bytes)
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("native cut receipt capacity overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("native cut receipt capacity overflow".into())
}
pub(super) fn retention(
    raw: usize,
    wire_bytes: usize,
    wire_capacity: usize,
    source_bytes: usize,
) -> Result<Retention, String> {
    const COMPONENT: usize = 4 << 20;
    const VALUE: usize = 64 << 20;
    // The typed request and exact submitted Value coexist until result encoding.
    let inputs = mul(add(raw, wire_bytes)?, 20)?;
    // Vec growth can retain twice the encoded bound. Include two immutable
    // receipt texts, the stream Vec and an encoding Vec plus RawValue boxing.
    // This is a component bound, not a claim about allocator/global RSS.
    let buffers = mul(COMPONENT, 6)?;
    Ok(Retention {
        owned_wire_parse_phase_bytes: add(
            wire_capacity,
            add(mul(wire_bytes, 40)?, mul(raw, 20)?)?,
        )?,
        source_owner_phase_bytes: add(source_bytes, add(inputs, add(32 << 20, buffers)?)?)?,
        raw_validation_phase_bytes: add(inputs, VALUE)?,
        decoded_tables_phase_bytes: add(inputs, add(VALUE, buffers)?)?,
        // The selected envelope drops all input/typed result owners first.
        final_value_phase_bytes: add(VALUE, mul(VALUE / 20, 2)?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phases_sum_coexisting_owners_and_only_max_disjoint_lifetimes() {
        let r = retention(100, 100, 256, 15 << 20).unwrap();
        assert_eq!(
            r.source_owner_phase_bytes,
            (15 + 32 + 24) * (1 << 20) + 4000
        );
        assert_eq!(r.decoded_tables_phase_bytes, (64 + 24) * (1 << 20) + 4000);
        assert_eq!(r.raw_validation_phase_bytes, (64 << 20) + 4000);
        assert_eq!(r.owned_wire_parse_phase_bytes, 6256);
        assert_eq!(r.peak(), r.decoded_tables_phase_bytes);
        assert_eq!(
            retention(100, 100, 256, 35 << 20).unwrap().peak(),
            (35 + 32 + 24) * (1 << 20) + 4000
        );
    }
    #[test]
    fn impossible_phase_sizes_refuse_before_allocating() {
        assert!(retention(usize::MAX, 0, 0, 0).is_err());
        assert!(retention(0, 0, 0, usize::MAX).is_err());
        assert!(retention(0, usize::MAX, 0, 0).is_err());
        assert!(retention(1, 1, usize::MAX, 0).is_err());
    }
    #[test]
    fn raw_receipt_embeds_identical_json_instead_of_a_quoted_json_string() {
        let raw = RawValue::from_string("{\"cash\":-0.0,\"day\":2}".into()).unwrap();
        let encoded = serde_json::to_string(&Preparation::Encoded(raw)).unwrap();
        assert_eq!(encoded, "{\"cash\":-0.0,\"day\":2}");
    }
}
