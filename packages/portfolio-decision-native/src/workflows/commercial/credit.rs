//! Explicit complete physical commercial sources and one existing daily journal.
//! Pricing/strategy adoption and independently verified publication are separate.
use portfolio_ledger_native::{commercial_intent_owner as ledger, daily, spec};
use portfolio_model_core::strict_json;
use portfolio_risk_native::commercial_credit_compiler as owner;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub const SCHEMA: &str = "commercial-credit-ledger-1";
pub const MAX_RAW_BYTES: usize = 1 << 20;
const MAX_OUTPUT_BYTES: usize = 32 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participation {
    pub scenario: String,
    pub position_id: String,
    pub path_key: u64,
    pub source_weight: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub position_id: String,
    pub allocated_face: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub spec: Value,
    pub commercial: Vec<owner::Request>,
    pub participations: Vec<Participation>,
    pub allocations: Vec<Allocation>,
}
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let mut out = self
            .0
            .lock()
            .map_err(|_| io::Error::other("buffer poisoned"))?;
        if out
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_OUTPUT_BYTES)
        {
            return Err(io::Error::other("commercial stream exceeds 32MiB"));
        }
        out.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)
    }
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("commercial coordinator upper overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("commercial coordinator upper overflow".into())
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: &'static str,
    pub original_request: Value,
    pub resolved_specification: Value,
    pub reporting_horizon_days: usize,
    pub sources: Vec<owner::ResultData>,
    pub admitted_work_upper: usize,
    pub retained_bytes_upper: usize,
    pub protocol: u32,
    pub mixed_ledger_adoption: bool,
    pub shared_daily_sim_adoption: bool,
    pub pricing_credit_adoption: bool,
    pub independent_persisted_verification: bool,
    pub empirical_validation: bool,
    pub scope: &'static str,
}

/// Native-only reservation, bound to the same immutable raw request.
pub(crate) struct Prepared<'a> {
    request: &'a Request,
    specification: portfolio_ledger_native::types::Spec,
    reporting_horizon_days: usize,
    weights: BTreeMap<(&'a str, &'a str, u64), f64>,
    allocations: BTreeMap<&'a str, f64>,
    reservations: Vec<(usize, usize)>,
    pub(crate) work_upper: usize,
    pub(crate) retained_bytes_upper: usize,
    pub(crate) ledger_bytes_upper: usize,
}
impl Prepared<'_> {
    pub(crate) fn complete_horizon(&self) -> usize {
        self.specification.horizon_days
    }
}

/// Preflight all source/credit/coupon/ledger work before generating either source.
pub(crate) fn admit(r: &Request) -> Result<Prepared<'_>, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA || r.commercial.is_empty() || r.commercial.len() > 512 {
        return Err("explicit bounded commercial credit ledger schema required".into());
    }
    let raw_bytes = strict_json::serialized_size(r, MAX_RAW_BYTES)?;
    let mut s = spec::normalize(r.spec.clone(), 512, 8_000_000)?;
    let reporting_horizon_days = s.horizon_days;
    // Admit every source before materializing a schedule, credit path or ledger.
    // The reporting cutoff never deletes a contractual or recovery cash tail.
    let admissions = r
        .commercial
        .iter()
        .map(owner::admit)
        .collect::<Result<Vec<_>, _>>()?;
    for a in &admissions {
        let elapsed = a.full_credit_end_year * 365.;
        let rounded = elapsed.round();
        let mut days = if (rounded / 365.).to_bits() == a.full_credit_end_year.to_bits() {
            rounded
        } else {
            elapsed.ceil()
        };
        if a.full_credit_end_year > days / 365. {
            days += 1.;
        }
        if !days.is_finite() || !(1. ..=1080.).contains(&days) {
            return Err("complete commercial recovery calendar exceeds 1080 days".into());
        }
        s.horizon_days = s.horizon_days.max(days as usize);
    }
    spec::validate(&s, 512, 8_000_000)?;
    if !s.netting_sets.is_empty()
        || !s.policies.is_empty()
        || !s.cashflows.is_empty()
        || s.accounts.iter().any(|a| {
            a.annual_fees != 0.
                || a.annual_costs != 0.
                || a.annual_dividends != 0.
                || a.tax_rate != 0.
                || a.capital_deductions != 0.
        })
    {
        return Err("commercial publication requires exclusive native commercial owners and cash accounts; generic owners, policies, pooled fees/taxes/netting need independent mixed replay".into());
    }
    if !s.reverse_severities.is_empty() {
        return Err(
            "commercial reverse scenarios require explicit complete source coverage".into(),
        );
    }
    let scenarios: BTreeSet<_> = std::iter::once("baseline")
        .chain(s.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let mut allocations = BTreeMap::new();
    for a in &r.allocations {
        if !a.allocated_face.is_finite()
            || !(0. ..=1e15).contains(&a.allocated_face)
            || allocations
                .insert(a.position_id.as_str(), a.allocated_face)
                .is_some()
        {
            return Err("unique bounded commercial allocation required".into());
        }
    }
    let mut weights = BTreeMap::new();
    for p in &r.participations {
        if !p.source_weight.is_finite()
            || !(0. ..=1.).contains(&p.source_weight)
            || weights
                .insert(
                    (p.scenario.as_str(), p.position_id.as_str(), p.path_key),
                    p.source_weight,
                )
                .is_some()
        {
            return Err("explicit unique bounded commercial path weights required".into());
        }
    }
    let mut used_weights = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut anchors = BTreeMap::<&str, Vec<u8>>::new();
    let mut streams = BTreeMap::<&str, Vec<u8>>::new();
    let mut calendar = None;
    let mut reservations = Vec::new();
    let mut total_work = mul(raw_bytes, 8)?;
    let mut total_bytes = mul(raw_bytes, 20)?;
    for (raw, admitted) in r.commercial.iter().zip(&admissions) {
        portfolio_compute_control::checkpoint()?;
        if raw.credit.instrument_kind == owner::InstrumentKind::IssuerCreditDebt {
            return Err(
                "issuer debt requires the explicit AC security candidate journal owner".into(),
            );
        }
        let current = &raw.source.current;
        let scenario = current.scenario_id.as_str();
        let position = current.position_id.as_str();
        if !scenarios.contains(scenario)
            || !names.insert((scenario, position))
            || !allocations.contains_key(position)
        {
            return Err("duplicate/unbound commercial position or actual run".into());
        }
        if calendar.is_some_and(|d| d != current.asof_ordinal) {
            return Err("all commercial sources require the same actual opening date".into());
        }
        calendar = Some(current.asof_ordinal);
        let pi = s
            .positions
            .iter()
            .find(|p| p.id.as_str() == position)
            .ok_or("commercial position is absent from specification")?;
        let account = s
            .accounts
            .iter()
            .find(|a| a.id.as_str() == pi.account.as_str())
            .ok_or("commercial account mapping absent")?;
        if account.currency != current.currency {
            return Err("commercial source and journal account currency differ".into());
        }
        let anchor = strict_json::encode(
            &json!({"source":raw.source.original,
            "acquisition":raw.source.acquisition}),
            MAX_RAW_BYTES,
        )?;
        if anchors.get(position).is_some_and(|old| *old != anchor) {
            return Err(
                "original source/cost/yield must remain identical across actual scenarios".into(),
            );
        }
        anchors.entry(position).or_insert(anchor);
        let stream = strict_json::encode(
            &json!({"draw_stream":raw.credit.draw_stream,
            "paths":raw.credit.paths}),
            MAX_RAW_BYTES,
        )?;
        if streams.get(position).is_some_and(|old| *old != stream) {
            return Err(
                "commercial scenarios must retain shared CRN and complete path identities".into(),
            );
        }
        streams.entry(position).or_insert(stream);
        let mut probability = 0.;
        for path in &raw.credit.paths {
            let key = (scenario, position, path.path_key);
            probability += *weights
                .get(&key)
                .ok_or("every actual commercial path needs an explicit weight")?;
            used_weights.insert(key);
        }
        if (probability - 1.).abs() > 1e-12 {
            return Err("complete per-position/run commercial probability required".into());
        }
        let reserved = ledger::admit_upper(
            admitted.carry_points_upper,
            admitted.lots_per_path_upper,
            s.horizon_days,
        )?;
        total_work = add(total_work, add(admitted.joined_work_upper, reserved.0)?)?;
        total_bytes = add(total_bytes, add(admitted.joined_bytes_upper, reserved.1)?)?;
        reservations.push(reserved);
    }
    if used_weights.len() != weights.len()
        || anchors.len() != allocations.len()
        || s.positions.len() != anchors.len()
        || s.positions
            .iter()
            .any(|p| !anchors.contains_key(p.id.as_str()))
        || allocations.keys().any(|position| {
            scenarios
                .iter()
                .any(|scenario| !names.contains(&(*scenario, *position)))
        })
    {
        return Err(
            "unused allocation/participation or missing commercial scenario coverage".into(),
        );
    }
    let base_work = mul(
        mul(
            s.horizon_days,
            s.positions.len() + s.netting_sets.len() + s.accounts.len() + s.policies.len(),
        )?,
        scenarios.len(),
    )?;
    total_work = add(total_work, base_work)?;
    if total_work > 8_000_000 || total_bytes > 128 << 20 {
        return Err(
            "complete commercial all-run preflight exceeds 8M/128MiB before any source generator"
                .into(),
        );
    }
    let ledger_bytes_upper = reservations
        .iter()
        .try_fold(0usize, |n, (_, bytes)| add(n, *bytes))?;
    Ok(Prepared {
        request: r,
        specification: s,
        reporting_horizon_days,
        weights,
        allocations,
        reservations,
        work_upper: total_work,
        retained_bytes_upper: total_bytes,
        ledger_bytes_upper,
    })
}

pub fn evaluate(r: &Request) -> Result<(Receipt, Vec<u8>), String> {
    let prepared = admit(r)?;
    let sources = r
        .commercial
        .iter()
        .map(owner::evaluate)
        .collect::<Result<Vec<_>, _>>()?;
    evaluate_prepared(prepared, sources)
}

/// Reuses complete sealed native owners; never accepts caller journal/checkpoint money.
pub(crate) fn evaluate_prepared(
    p: Prepared<'_>,
    sources: Vec<owner::ResultData>,
) -> Result<(Receipt, Vec<u8>), String> {
    evaluate_prepared_with_fees(p, sources, &[])
}
pub(crate) fn evaluate_prepared_with_fees(
    p: Prepared<'_>,
    sources: Vec<owner::ResultData>,
    fees: &[Vec<portfolio_risk_native::commercial_fee_source::BoundPath>],
) -> Result<(Receipt, Vec<u8>), String> {
    let Prepared {
        request: r,
        specification: s,
        reporting_horizon_days,
        weights,
        allocations,
        reservations,
        work_upper: total_work,
        retained_bytes_upper: total_bytes,
        ..
    } = p;
    if sources.len() != r.commercial.len() {
        return Err("native commercial owner coverage differs".into());
    }
    for (raw, source) in r.commercial.iter().zip(&sources) {
        portfolio_compute_control::checkpoint()?;
        if strict_json::encode(raw, MAX_RAW_BYTES)?
            != strict_json::encode(source.raw_request(), MAX_RAW_BYTES)?
        {
            return Err("native commercial owner is bound to a different raw source".into());
        }
    }
    let mut bindings = Vec::new();
    for (raw, source) in r.commercial.iter().zip(&sources) {
        for p in &raw.credit.paths {
            bindings.push(ledger::Binding {
                source_class: ledger::SourceClass::CommercialLoan,
                origin_day: 0,
                scenario: &raw.source.current.scenario_id,
                position_id: &raw.source.current.position_id,
                path_key: p.path_key,
                source_weight: weights[&(
                    raw.source.current.scenario_id.as_str(),
                    raw.source.current.position_id.as_str(),
                    p.path_key,
                )],
                allocated_face: allocations[raw.source.current.position_id.as_str()],
                native_unit_face: source.current_source().raw().opening_principal,
                accounting: source.accounting(),
            });
        }
    }
    let mut compiled = ledger::Compiled::new(&s, bindings)?;
    let upper = reservations.iter().try_fold((0usize, 0usize), |a, b| {
        Ok::<_, String>((add(a.0, b.0)?, add(a.1, b.1)?))
    })?;
    if compiled.work_upper > upper.0 || compiled.retained_bytes_upper > upper.1 {
        return Err("actual commercial ledger exceeded structural reservation".into());
    }
    if !fees.is_empty() {
        if fees.len() != sources.len() {
            return Err("fee loan source coverage differs".into());
        }
        let mut fee_bindings = Vec::new();
        let mut parent = 0;
        for (source, paths) in sources.iter().zip(fees) {
            if paths.len() != source.raw_request().credit.paths.len() {
                return Err("fee native path coverage differs".into());
            }
            for (path, fee) in source.raw_request().credit.paths.iter().zip(paths) {
                if path.path_key != fee.path_key() {
                    return Err("fee parent path identity differs".into());
                }
                fee_bindings.push(ledger::FeeBinding {
                    loan_binding: parent,
                    source: fee.source(),
                    native_years: fee.native_years(),
                });
                parent += 1;
            }
        }
        compiled.attach_fees(&s, fee_bindings)?;
    }
    let out = Buffer::default();
    daily::run_with_commercial(&s, None, &compiled, Box::new(out.clone()))?;
    let stream = std::mem::take(&mut *out.0.lock().map_err(|_| "commercial buffer poisoned")?);
    let receipt = Receipt {schema:SCHEMA,original_request:serde_json::to_value(r).map_err(|e|e.to_string())?,
        resolved_specification:serde_json::to_value(&s).map_err(|e|e.to_string())?,reporting_horizon_days,
        sources,admitted_work_upper:total_work,retained_bytes_upper:total_bytes,protocol:if fees.is_empty(){ledger::PROTOCOL}else{portfolio_ledger_native::commercial_fee_owner::PROTOCOL},
        mixed_ledger_adoption:false,shared_daily_sim_adoption:true,pricing_credit_adoption:false,independent_persisted_verification:false,
        empirical_validation:false,scope:"exclusive native commercial owners and cash accounts in one existing daily Sim; complete physical contractual/credit/recovery clock, reviewed original-yield carrying amounts and gross legal/recognized coupon lots; no arbitrary mixed owners, pricing/solver/saved-book, automatic capital/FTP, empirical or independently verified publication adoption"};
    portfolio_compute_control::checkpoint()?;
    Ok((receipt, stream))
}

/// Raw-byte entrypoint inherits the dispatcher's original deadline and parent cancellation.
/// Complete private preparation and journal execution finish before any output is emitted.
pub fn run_started(
    bytes: &[u8],
    output: &mut dyn Write,
    started: std::time::Instant,
) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        request: Request,
        execution_timeout_ms: u64,
    }
    portfolio_compute_control::checkpoint()?;
    let value =
        strict_json::parse_checked(bytes, MAX_RAW_BYTES, portfolio_compute_control::checkpoint)?;
    let original_request = value
        .get("request")
        .cloned()
        .ok_or("commercial raw request absent")?;
    let envelope: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(
            envelope.execution_timeout_ms,
            started,
        )?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let (mut receipt, stream) = evaluate(&envelope.request)?;
        receipt.original_request = original_request.clone();
        let header = strict_json::encode(
            &json!({"workflow":{
            "schema":"commercial-ledger-preparation-1","model_schema":SCHEMA,
            "prepared_request":original_request,"source_receipt":receipt}}),
            MAX_OUTPUT_BYTES,
        )?;
        if add(add(header.len(), 1)?, stream.len())? > MAX_OUTPUT_BYTES {
            return Err("complete commercial header/stream exceeds32MiB".into());
        }
        portfolio_compute_control::checkpoint()?;
        output
            .write_all(&header)
            .and_then(|_| output.write_all(b"\n"))
            .map_err(|e| e.to_string())?;
        for chunk in stream.chunks(32768) {
            portfolio_compute_control::checkpoint()?;
            output.write_all(chunk).map_err(|e| e.to_string())?;
        }
        output.flush().map_err(|e| e.to_string())?;
        portfolio_compute_control::checkpoint()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(r: &Request, allocation: f64, mut output: Box<dyn Write + Send>) -> Result<(), String> {
        let mut owned = r.clone();
        for a in &mut owned.allocations {
            a.allocated_face = allocation;
        }
        let (_, stream) = evaluate(&owned)?;
        output.write_all(&stream).map_err(|e| e.to_string())
    }
    fn request() -> Request {
        serde_json::from_value(
            portfolio_model_core::strict_json::parse(
                include_bytes!("../../../../../examples/models/commercial-credit-ledger-1.json"),
                1 << 20,
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn bytes(r: &Request, allocation: f64) -> Result<Vec<u8>, String> {
        let out = Buffer::default();
        run(r, allocation, Box::new(out.clone()))?;
        let b = out.0.lock().unwrap().clone();
        Ok(b)
    }
    fn rows(data: &[u8], table: &str) -> Vec<Value> {
        std::str::from_utf8(data)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|v| v["table"] == table)
            .flat_map(|v| v["rows"].as_array().unwrap().clone())
            .collect()
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    #[test]
    fn actual_delayed_source_uses_one_sim_cash_and_original_yield() {
        let out = bytes(&request(), 100.).unwrap();
        let path = rows(&out, "path");
        let last = path.iter().find(|r| r["day"] == 35).unwrap();
        close(last["cash"].as_f64().unwrap(), 1000.);
        close(last["equity"].as_f64().unwrap(), 1000.);
        let lineage = rows(&out, "commercial_source_lineage");
        assert!(lineage
            .iter()
            .any(|r| r["native_atomic_group"]["phase"] == "contractual_cash"));
        assert_eq!(
            lineage
                .iter()
                .filter(|r| r["native_atomic_group"]["phase"] == "reviewed_opening_snapshot")
                .count(),
            2
        );
    }
    #[test]
    fn early_native_cash_preserves_signed_p0_rights_and_half_allocation() {
        let mut r = request();
        for raw in &mut r.commercial {
            raw.source.original.periods[0].payment_ordinal = 738896;
            raw.source.current.periods[0].payment_ordinal = 738896;
        }
        let out = bytes(&r, 50.).unwrap();
        let line = rows(&out, "commercial_source_lineage");
        let cash = line
            .iter()
            .find(|r| r["native_atomic_group"]["phase"] == "contractual_cash")
            .unwrap();
        assert_eq!(cash["allocation_factor"], 0.5);
        assert_eq!(cash["native_atomic_group"]["next"]["principal"], 0.);
        assert!(
            cash["native_atomic_group"]["next"]["legal_interest"]
                .as_f64()
                .unwrap()
                < 0.
        );
        close(
            rows(&out, "path").last().unwrap()["cash"].as_f64().unwrap(),
            1000.,
        );
    }
    #[test]
    fn empty_owner_nonempty_legacy_full_stream_is_exact_historical_bytes() {
        let mut r = request();
        r.spec["positions"] = serde_json::json!([{"id":"old-loan","account":"bank","kind":"loan","balance":100.,"rate":0.05,"maturity_day":30}]);
        r.spec["accounts"][0]["equity"] = serde_json::json!(1100.);
        let s = spec::normalize(r.spec, 60000, 90_000_000).unwrap();
        let c = ledger::Compiled::new(&s, vec![]).unwrap();
        let a = Buffer::default();
        daily::run_with_commercial(&s, None, &c, Box::new(a.clone())).unwrap();
        let b = Buffer::default();
        daily::run(s, Box::new(b.clone())).unwrap();
        assert_eq!(*a.0.lock().unwrap(), *b.0.lock().unwrap());
    }
    #[test]
    fn reporting_cutoff_retains_full_tail_and_generic_cash_refuses() {
        let mut r = request();
        r.spec["horizon_days"] = serde_json::json!(30);
        let (receipt, data) = evaluate(&r).unwrap();
        assert_eq!(receipt.reporting_horizon_days, 30);
        assert_eq!(receipt.resolved_specification["horizon_days"], 35);
        assert!(rows(&data, "path").iter().any(|v| v["day"] == 35));
        let mut r = request();
        r.spec["cashflows"] =
            serde_json::json!([{"position":"commercial","day":10,"principal":1.,"interest":0.}]);
        assert!(bytes(&r, 100.).is_err());
    }
    #[test]
    fn recovery_calendar_retains_cash_after_source_tail_without_changing_report() {
        let mut r = request();
        // Complete daily reconciliation is admitted for each scenario. The
        // only reachable jump is to absorbing Default, so one jump is the
        // complete bound for this actual-recovery control.
        for source in &mut r.commercial {
            source.credit.max_jumps_per_path = 1;
            source.credit.draw_stream = portfolio_model_core::credit_path::DrawStream::Supplied {
                stream_id: "late-recovery-test".into(),
                stream_revision: "1".into(),
                paths: vec![portfolio_model_core::credit_path::SuppliedPathDraws {
                    path_key: 17,
                    pairs: vec![portfolio_model_core::credit_path::UniformPair {
                        waiting_uniform: (-100_f64 * 25. / 365.).exp(),
                        destination_uniform: 0.5,
                    }],
                }],
            };
            for interval in &mut source.credit.intervals {
                interval.hazards_per_year[0][3] = 100.;
                for tail in &mut interval.complete_recovery_tails {
                    tail[0].lag_years = 20. / 365.;
                }
            }
        }
        let (receipt, stream) = evaluate(&r).unwrap();
        assert_eq!(receipt.reporting_horizon_days, 35);
        assert_eq!(receipt.resolved_specification["horizon_days"], 55);
        assert!(rows(&stream, "path").iter().any(|v| v["day"] == 55));
        assert_eq!(receipt.original_request["spec"]["horizon_days"], 35);
        assert!(rows(&stream, "commercial_source_lineage").iter().any(|v| {
            v["day"].as_u64().is_some_and(|day| day > 35)
                && v["native_atomic_group"]["phase"] == "default_actual_cash"
        }));
    }
    #[test]
    fn opening_acquisition_uses_actual_day_zero_aggregate() {
        let (_, stream) = evaluate(&request()).unwrap();
        let rows = rows(&stream, "ledger");
        assert!(rows
            .iter()
            .any(|v| v["day"] == 0 && v["cash"].as_f64().is_some_and(|x| x < 0.)));
    }
    #[test]
    fn cancelled_parent_refuses_before_any_stream() {
        let out = Buffer::default();
        let c = portfolio_compute_control::Control::new(30000).unwrap();
        c.cancel();
        assert!(portfolio_compute_control::scope(Some(c), || run(
            &request(),
            100.,
            Box::new(out.clone())
        ))
        .unwrap_err()
        .contains("CANCELLED"));
        assert!(out.0.lock().unwrap().is_empty());
    }
    #[test]
    fn explicit_unequal_paths_and_zero_allocation_keep_full_native_lineage() {
        let mut r = request();
        r.participations.clear();
        for raw in &mut r.commercial {
            raw.credit.paths.push(owner::PathIdentity {
                path_id: "second-borrower".into(),
                path_key: 18,
                initial_live_state: portfolio_model_core::credit::CreditState::Watch,
            });
            r.participations.push(Participation {
                scenario: raw.source.current.scenario_id.clone(),
                position_id: raw.source.current.position_id.clone(),
                path_key: 17,
                source_weight: 0.25,
            });
            r.participations.push(Participation {
                scenario: raw.source.current.scenario_id.clone(),
                position_id: raw.source.current.position_id.clone(),
                path_key: 18,
                source_weight: 0.75,
            });
        }
        let full = bytes(&r, 50.).unwrap();
        close(
            rows(&full, "path").last().unwrap()["cash"]
                .as_f64()
                .unwrap(),
            1000.,
        );
        let zero = bytes(&r, 0.).unwrap();
        let lineage = rows(&zero, "commercial_source_lineage");
        assert_eq!(
            lineage
                .iter()
                .filter(|r| r["native_atomic_group"]["phase"] == "reviewed_opening_snapshot")
                .count(),
            4
        );
        assert!(lineage.iter().all(|r| r["allocation_factor"] == 0.));
        assert!(lineage
            .iter()
            .any(|r| r["native_atomic_group"]["phase"] == "contractual_cash"));
    }
    #[test]
    fn default_retains_gross_claims_without_expected_loss_journal_plug() {
        use portfolio_model_core::credit_path::{DrawStream, SuppliedPathDraws, UniformPair};
        let mut r = request();
        for raw in &mut r.commercial {
            raw.credit.intervals[0].hazards_per_year[0][3] = 1.;
            raw.credit.intervals[0].loss_given_default = [0.4; 3];
            raw.credit.draw_stream = DrawStream::Supplied {
                stream_id: "actual-default".into(),
                stream_revision: "1".into(),
                paths: vec![SuppliedPathDraws {
                    path_key: 17,
                    pairs: vec![UniformPair {
                        waiting_uniform: (-5_f64 / 365.).exp(),
                        destination_uniform: 0.5,
                    }],
                }],
            };
        }
        let data = bytes(&r, 100.).unwrap();
        let line = rows(&data, "commercial_source_lineage");
        assert!(line
            .iter()
            .any(|r| r["native_atomic_group"]["phase"] == "default_gross_transfer"));
        let ending = line
            .iter()
            .find(|r| {
                r["scenario"] == "baseline"
                    && r["native_atomic_group"]["phase"] == "closing_claims_retained"
            })
            .unwrap();
        close(
            ending["native_atomic_group"]["next"]["principal"]
                .as_f64()
                .unwrap(),
            40.,
        );
        assert!(
            ending["native_atomic_group"]["next"]["legal_interest"]
                .as_f64()
                .unwrap()
                > 0.
        );
        close(
            rows(&data, "path")
                .iter()
                .find(|r| r["day"] == 35 && r["scenario"] == "baseline")
                .unwrap()["equity"]
                .as_f64()
                .unwrap(),
            1000.,
        );
    }
    #[test]
    fn original_anchor_and_shared_draws_cannot_change_between_scenarios() {
        let mut r = request();
        r.commercial[1]
            .source
            .acquisition
            .original_yield_anchor
            .revision = "changed".into();
        assert!(evaluate(&r)
            .unwrap_err()
            .contains("original source/cost/yield"));
        let mut r = request();
        r.commercial[1].credit.draw_stream =
            portfolio_model_core::credit_path::DrawStream::CounterSplitmix64V1 {
                stream_id: "changed".into(),
                stream_revision: "1".into(),
                seed: 7,
            };
        assert!(evaluate(&r).unwrap_err().contains("shared CRN"));
    }
    #[test]
    fn partial_scenarios_and_unused_weights_refuse() {
        let mut r = request();
        r.commercial.pop();
        assert!(evaluate(&r)
            .unwrap_err()
            .contains("unused allocation/participation"));
        let mut r = request();
        r.participations[0].source_weight = 0.5;
        assert!(evaluate(&r).unwrap_err().contains("probability"));
        let mut r = request();
        r.allocations.push(Allocation {
            position_id: "absent".into(),
            allocated_face: 0.,
        });
        assert!(evaluate(&r)
            .unwrap_err()
            .contains("unused allocation/participation"));
    }
    #[test]
    fn commercial_acquisition_enforces_actual_pooled_cash_floor() {
        let mut r = request();
        r.spec["accounts"][0]["cash_floor"] = json!(999.);
        assert!(evaluate(&r).unwrap_err().contains("cash floor"));
    }
    #[test]
    fn generic_policy_cannot_acquire_or_sell_source_owned_commercial_position() {
        let mut r = request();
        r.spec["policies"] = json!([{"id":"duplicate-sale","account":"bank","kind":"asset_sale",
            "trigger_cash":900.,"limit":1.,"position":"commercial"}]);
        assert!(evaluate(&r).is_err());
    }
    #[test]
    fn raw_dispatch_preserves_complete_original_input_and_protocol() {
        let r = request();
        let raw = serde_json::to_value(&r).unwrap();
        let envelope = strict_json::encode(
            &json!({"request":raw,"execution_timeout_ms":30000}),
            MAX_RAW_BYTES,
        )
        .unwrap();
        let mut out = vec![];
        run_started(&envelope, &mut out, std::time::Instant::now()).unwrap();
        let lines: Vec<Value> = std::str::from_utf8(&out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            lines[0]["workflow"]["source_receipt"]["original_request"],
            raw
        );
        assert_eq!(
            lines.last().unwrap(),
            &json!({"complete":true,"protocol":6})
        );
        assert_eq!(
            lines[0]["workflow"]["source_receipt"]["independent_persisted_verification"],
            false
        );
    }
    #[test]
    fn duplicate_input_expired_dispatcher_and_cancelled_parent_emit_nothing() {
        let mut output = vec![];
        assert!(run_started(br#"{"request":{"schema":"commercial-credit-ledger-1","schema":"commercial-credit-ledger-1"},"execution_timeout_ms":30000}"#,
            &mut output,std::time::Instant::now()).unwrap_err().contains("duplicate object key"));
        assert!(output.is_empty());
        let envelope = strict_json::encode(
            &json!({"request":request(),"execution_timeout_ms":1}),
            MAX_RAW_BYTES,
        )
        .unwrap();
        assert!(run_started(
            &envelope,
            &mut output,
            std::time::Instant::now() - std::time::Duration::from_secs(1)
        )
        .unwrap_err()
        .contains("DEADLINE"));
        assert!(output.is_empty());
        let parent = portfolio_compute_control::Control::new(30000).unwrap();
        parent.cancel();
        assert!(
            portfolio_compute_control::scope(Some(parent), || run_started(
                &envelope,
                &mut output,
                std::time::Instant::now()
            ))
            .unwrap_err()
            .contains("CANCELLED")
        );
        assert!(output.is_empty());
    }
}
