//! Bounded Physical macro replay through the existing daily monetary owners.
use super::{macro_scenarios as macros, product_paths};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

pub const SCHEMA: &str = "consumer-macro-products-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub product_id: String,
    pub mappings: Vec<macros::Mapping>,
    /// One path per owner: scenario-dependent covariates cannot be broadcast across paths.
    pub template: product_paths::Request,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub timeout_ms: u64,
    pub macro_source: macros::Request,
    pub products: Vec<Product>,
}
#[derive(Debug, Serialize)]
pub struct Case {
    pub product_id: String,
    pub scenario_id: String,
    pub path_key: u64,
    pub result: product_paths::ResultData,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_source: Request,
    pub macro_result: macros::ResultData,
    pub cases: Vec<Case>,
    pub prospective_product_rows: usize,
    pub reserved_response_bytes: usize,
    pub adoption_eligible: bool,
    pub empirical_accuracy_validated: bool,
    pub pricing_measure_permission: bool,
}

pub fn evaluate(
    r: &Request,
    external: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let started = Instant::now();
    let mut check = || {
        external()?;
        if started.elapsed() >= Duration::from_millis(r.timeout_ms) {
            Err("consumer macro products original deadline exceeded".into())
        } else {
            Ok(())
        }
    };
    check()?;
    let raw_bytes = crate::strict_json::serialized_size(r, 8 << 20)?;
    if r.schema != SCHEMA
        || !(1..=120000).contains(&r.timeout_ms)
        || r.products.is_empty()
        || r.products.len() > 3
        || r.macro_source.scenarios.len() != 4
    {
        return Err("consumer macro products schema/work admission".into());
    }
    // Reserve all four cases before fitting or generating any product cashflows.
    // At most twelve existing bounded fits are repeated from immutable training inputs.
    let mut names = HashSet::new();
    let mut rows = 0usize;
    let mut bytes = raw_bytes
        .checked_add(8 << 20)
        .ok_or("consumer macro bytes overflow")?;
    for product in &r.products {
        check()?;
        if product.product_id.trim().is_empty()
            || product.product_id.len() > 256
            || product.product_id.chars().any(char::is_control)
            || !names.insert(&product.product_id)
            || product.template.products.len() != 1
        {
            return Err("consumer macro products unique owner/single path admission".into());
        }
        let a = product_paths::admit(&product.template)?;
        rows = rows
            .checked_add(
                a.product_rows
                    .checked_mul(4)
                    .ok_or("consumer macro rows overflow")?,
            )
            .ok_or("consumer macro rows overflow")?;
        bytes = bytes
            .checked_add(
                a.response_bytes
                    .checked_mul(4)
                    .ok_or("consumer macro bytes overflow")?,
            )
            .ok_or("consumer macro bytes overflow")?;
        if rows > 100000 || bytes > 96 << 20 {
            return Err("consumer macro products aggregate work/byte admission".into());
        }
    }
    let macro_result = macros::evaluate(&r.macro_source, &mut check)?;
    // Bind every case before generating cash: an invalid late owner cannot leave partial output.
    let mut requests = Vec::with_capacity(r.products.len() * 4);
    for scenario in &r.macro_source.scenarios {
        for product in &r.products {
            check()?;
            let mut q = product.template.clone();
            let path_key = q.projection.path_source.paths[0].path_key;
            q.projection.intervals = macro_result.project_intervals(
                &q.fit.specification,
                &q.projection,
                &scenario.scenario_id,
                path_key,
                &product.mappings,
                &mut check,
            )?;
            q.projection.scenario_id = scenario.scenario_id.clone();
            // Source hash strings can enlarge the request, so repeat admission on the exact input.
            let a = product_paths::admit(&q)?;
            let old = product_paths::admit(&product.template)?;
            bytes = bytes
                .checked_add(a.response_bytes.saturating_sub(old.response_bytes))
                .ok_or("consumer macro bytes overflow")?;
            if bytes > 96 << 20 {
                return Err("consumer macro compiled byte admission".into());
            }
            requests.push((
                product.product_id.clone(),
                scenario.scenario_id.clone(),
                path_key,
                q,
            ));
        }
    }
    let mut cases = Vec::with_capacity(requests.len());
    for (product_id, scenario_id, path_key, q) in requests {
        check()?;
        let result = product_paths::evaluate_checked(&q, &mut check)?;
        cases.push(Case {
            product_id,
            scenario_id,
            path_key,
            result,
        });
    }
    let result = ResultData {
        schema: SCHEMA,
        raw_source: r.clone(),
        macro_result,
        cases,
        prospective_product_rows: rows,
        reserved_response_bytes: bytes,
        adoption_eligible: false,
        empirical_accuracy_validated: false,
        pricing_measure_permission: false,
    };
    crate::strict_json::serialized_size(&result, bytes)?;
    check()?;
    Ok(result)
}
