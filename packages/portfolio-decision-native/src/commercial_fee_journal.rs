//! Joint loan and separately owned fee money in one existing daily Sim.
use crate::{commercial_holding_journal::decode_selected, commercial_ledger as loan};
use portfolio_compute_control::checkpoint;
use portfolio_model_core::strict_json;
use portfolio_risk_native::{commercial_credit_compiler as owner, commercial_fee_source as fee};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};
pub const SCHEMA: &str = "commercial-fee-journal-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub loan: loan::Request,
    pub fees: Vec<fee::Policy>,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    raw_source: Value,
    loan_receipt: loan::Receipt,
    fee_sources: Vec<Vec<fee::BoundPath>>,
    tables: BTreeMap<String, Vec<Value>>,
    completion: Value,
    reporting_horizon_days: usize,
    admitted_work_upper: usize,
    retained_bytes_upper: usize,
    native_credit_event_binding: bool,
    shared_daily_sim_adoption: bool,
    loan_carrying_value_changed: bool,
    pricing_unit_ftp_adoption: bool,
    empirical_validation: bool,
    accounting_approval: bool,
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    checkpoint()?;
    if r.schema != SCHEMA || r.fees.len() != r.loan.commercial.len() {
        return Err("exact fee/commercial source coverage required".into());
    }
    strict_json::serialized_size(r, 2 << 20)?;
    let base = portfolio_ledger_native::spec::normalize(r.loan.spec.clone(), 512, 8_000_000)?;
    let report = base.horizon_days;
    let mut current = r.loan.clone();
    let mut horizon = report;
    let (mut fee_work, mut fee_bytes, mut rows) = (0usize, 0usize, 0usize);
    for (p, raw) in r.fees.iter().zip(&r.loan.commercial) {
        let (w, b) = fee::admit(p, raw)?;
        fee_work = fee_work.checked_add(w).ok_or("fee work overflow")?;
        fee_bytes = fee_bytes.checked_add(b).ok_or("fee byte overflow")?;
        let days = p
            .complete_end_ordinal
            .checked_sub(raw.source.current.asof_ordinal)
            .ok_or("fee calendar overflow")?;
        if !(1..=1080).contains(&days) {
            return Err("fee complete calendar domain".into());
        }
        horizon = horizon.max(days as usize);
        let recovery = [&p.default, &p.prepayment, &p.exercise, &p.maturity]
            .iter()
            .map(|x| x.recoveries.len())
            .max()
            .unwrap_or(0);
        rows = rows
            .checked_add(
                (p.events.len() + p.fees.len() * (3 + recovery)) * raw.credit.paths.len() * 8,
            )
            .ok_or("fee ledger rows overflow")?;
    }
    current.spec["horizon_days"] = json!(horizon);
    let prepared = loan::admit(&current)?;
    // Loan recoveries can extend beyond both reporting and fee cash tails.
    // Charge fee daily reconciliation over the final joined Sim calendar.
    horizon = prepared.complete_horizon();
    let work = prepared
        .work_upper
        .checked_add(fee_work)
        .and_then(|v| v.checked_add(rows * (horizon + 1) * 16))
        .ok_or("fee joined work overflow")?;
    let bytes = prepared
        .retained_bytes_upper
        .checked_add(fee_bytes)
        .and_then(|v| v.checked_add(rows * 4096))
        .and_then(|v| v.checked_add(32 << 20))
        .ok_or("fee joined bytes overflow")?;
    if work > 8_000_000 || bytes > 128 << 20 {
        return Err(
            "joined loan/fee/source/journal admission exceeds8M/128MiB before generation".into(),
        );
    }
    let sources = current
        .commercial
        .iter()
        .map(owner::evaluate)
        .collect::<Result<Vec<_>, _>>()?;
    let fees = r
        .fees
        .iter()
        .zip(&sources)
        .map(|(p, s)| fee::bind(p, s))
        .collect::<Result<Vec<_>, _>>()?;
    let (receipt, stream) = loan::evaluate_prepared_with_fees(prepared, sources, &fees)?;
    let (tables, completion) = decode_selected(&stream, 32 << 20, 7)?;
    let result = ResultData {
        schema: SCHEMA,
        raw_source: serde_json::to_value(r).map_err(|e| e.to_string())?,
        loan_receipt: receipt,
        fee_sources: fees,
        tables,
        completion,
        reporting_horizon_days: report,
        admitted_work_upper: work,
        retained_bytes_upper: bytes,
        native_credit_event_binding: true,
        shared_daily_sim_adoption: true,
        loan_carrying_value_changed: false,
        pricing_unit_ftp_adoption: false,
        empirical_validation: false,
        accounting_approval: false,
    };
    strict_json::serialized_size(&result, 32 << 20)?;
    checkpoint()?;
    Ok(result)
}
pub fn run_started(bytes: &[u8], started: Instant) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Request,
    }
    let value = strict_json::parse_checked(bytes, 2 << 20, checkpoint)?;
    let raw = value
        .get("request")
        .cloned()
        .ok_or("fee raw source absent")?;
    let e: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if e.op != "analyze_commercial_fee_journal" || e.schema != SCHEMA {
        return Err("explicit fee journal operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(e.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(e.execution_timeout_ms, started)?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut result = evaluate(&e.request)?;
        result.raw_source = raw;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> Request {
        serde_json::from_str(include_str!(
            "../../../examples/models/commercial-fee-journal-1.json"
        ))
        .unwrap()
    }
    #[test]
    fn native_single_sim_retains_separate_fee_tail() {
        let v = serde_json::to_value(evaluate(&raw()).unwrap()).unwrap();
        assert_eq!(v["completion"], json!({"complete":true,"protocol":7}));
        assert_eq!(v["reporting_horizon_days"], 35);
        assert_eq!(v["loan_carrying_value_changed"], false);
        assert_eq!(
            v["fee_sources"][0][0]["source"]["reporting"][0]["state"]["fee_receivable"],
            5.
        );
        assert_eq!(
            v["fee_sources"][0][0]["source"]["closing"][0]["state"]["fee_receivable"],
            0.
        );
    }
    #[test]
    fn invalid_original_fee_contract_fails_before_generation() {
        let mut r = raw();
        r.fees[0].events[0].amount = 6.;
        assert!(evaluate(&r)
            .unwrap_err()
            .contains("separate reviewed right"));
        let mut r = raw();
        r.fees[0].fees[0].prepaid_refunds_allowed = false;
        r.fees[0].default.refund_prepaid_fraction = 1.;
        assert!(evaluate(&r).unwrap_err().contains("contradicts"));
    }
    #[test]
    fn unmatched_and_foreign_original_owner_reject() {
        let mut r = raw();
        r.fees.pop();
        assert!(evaluate(&r).is_err());
        let mut r = raw();
        r.fees[0].fees[0].tranche_id = "secondary".into();
        assert!(evaluate(&r).unwrap_err().contains("original tranche"));
    }
}
