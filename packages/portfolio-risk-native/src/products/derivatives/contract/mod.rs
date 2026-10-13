//! Versioned clean derivative contracts and separately scoped collateral/XVA.
pub mod csa;
pub mod options;
pub mod swaps;
pub use crate::products::term::contracts::Identity;
pub(crate) use crate::products::term::contracts::{
    amount, domain, identity, near, nonnegative, text,
};
use serde::{Deserialize, Serialize};
pub const SCHEMA: &str = "derivative-contract-1";
pub const MODEL_VERSION: &str = "derivative-contracts-1";
pub const MPOR_MODEL_VERSION: &str = "derivative-contracts-2";
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Swap {
        request: Box<swaps::SwapRequest>,
    },
    EuropeanSwaption {
        request: Box<options::SwaptionRequest>,
    },
    Netting {
        request: Box<csa::CsaRequest>,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DerivativeRequest {
    pub schema: String,
    pub model_version: String,
    pub operation: Operation,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum DerivativeResult {
    Swap(swaps::SwapResult),
    EuropeanSwaption(options::SwaptionResult),
    Netting(csa::CsaResult),
}
pub fn evaluate(r: &DerivativeRequest) -> Result<DerivativeResult, String> {
    if r.schema != SCHEMA
        || ![MODEL_VERSION, MPOR_MODEL_VERSION].contains(&r.model_version.as_str())
    {
        return Err("derivative schema/model mismatch; explicit rebuild required".into());
    }
    if r.model_version == MODEL_VERSION
        && matches!(&r.operation,Operation::Netting{request} if request.paths.iter().any(|p|p.default.as_ref().is_some_and(|d|d.mpor.is_some())))
    {
        return Err(
            "MPOR obligation policy requires explicit derivative-contracts-2 selection".into(),
        );
    }
    match &r.operation {
        Operation::Swap { request } => swaps::value_swap(request).map(DerivativeResult::Swap),
        Operation::EuropeanSwaption { request } => {
            options::value_european(request).map(DerivativeResult::EuropeanSwaption)
        }
        Operation::Netting { request } => csa::evaluate(request).map(DerivativeResult::Netting),
    }
}
