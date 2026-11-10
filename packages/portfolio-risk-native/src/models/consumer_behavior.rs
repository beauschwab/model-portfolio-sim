//! Standalone analytical consumer fit/projection coordinator. No saved-book adoption.
use portfolio_model_core::consumer_behavior as behavior;
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = behavior::SCHEMA;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub fit: behavior::Request,
    pub projection: Option<behavior::Projection>,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub fitted: behavior::ResultData,
    pub projection: Option<behavior::ProjectionResult>,
    pub adoption_eligible: bool,
    pub empirical_accuracy_validated: bool,
    pub reserved_response_bytes: usize,
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    let mut check = portfolio_compute_control::checkpoint;
    check()?;
    let raw_bytes = portfolio_model_core::strict_json::serialized_size(r, 8 << 20)?;
    // A conservative response budget policy, enforced again on actual serialized
    // output. This is not an exact size estimate or a bound on aggregate RSS.
    let events = r
        .projection
        .as_ref()
        .map(behavior::projection_event_upper)
        .transpose()?
        .unwrap_or(0);
    let bytes = raw_bytes
        .checked_mul(12)
        .and_then(|n| n.checked_add(4 << 20))
        .and_then(|n| events.checked_mul(4096).and_then(|e| n.checked_add(e)))
        .ok_or("consumer behaviour joined response reservation overflow")?;
    if bytes > 32 << 20 {
        return Err("consumer behaviour joined response admission".into());
    }
    let fitted = behavior::fit_checked(&r.fit, &mut check)?;
    let projection = r
        .projection
        .as_ref()
        .map(|p| fitted.project_paths_checked(p, &mut check))
        .transpose()?;
    check()?;
    let result = ResultData {
        schema: SCHEMA,
        raw_source: r.clone(),
        fitted,
        projection,
        adoption_eligible: false,
        empirical_accuracy_validated: false,
        reserved_response_bytes: bytes,
    };
    portfolio_model_core::strict_json::serialized_size(&result, bytes)?;
    check()?;
    Ok(result)
}
