//! Scoped new-request shape admission. Historical SuppliedCurve parsing is unchanged.
use crate::market::analysis::curves::{CurveRole, CurveTail, SuppliedCurve};
use serde::{Deserialize, Deserializer};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCurve {
    curve_id: String,
    source_id: String,
    source_revision: String,
    currency: String,
    role: CurveRole,
    times_years: Vec<f64>,
    discount_factors: Vec<f64>,
    right_tail: String,
}
pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<SuppliedCurve>, D::Error> {
    Vec::<RawCurve>::deserialize(d)?
        .into_iter()
        .map(|raw| {
            let right_tail = match raw.right_tail.as_str() {
                "reject" => CurveTail::Reject,
                "flat_zero" => CurveTail::FlatZero,
                "flat_forward" => CurveTail::FlatForward,
                _ => {
                    return Err(serde::de::Error::custom(
                        "unsupported curve tail enum string",
                    ))
                }
            };
            Ok(SuppliedCurve {
                curve_id: raw.curve_id,
                source_id: raw.source_id,
                source_revision: raw.source_revision,
                currency: raw.currency,
                role: raw.role,
                times_years: raw.times_years,
                discount_factors: raw.discount_factors,
                right_tail,
            })
        })
        .collect()
}
