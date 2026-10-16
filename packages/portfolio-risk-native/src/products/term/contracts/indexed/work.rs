//! Checked algorithmic work ledger, independent of monetary coupon arithmetic.
//! Only a native generated catalog may create the source authority. No numeric
//! allowance is deserializable. Omitted contracts retain their historical bound.
use super::{ContractPathRequest, Coupon};
use portfolio_model_core::{
    cashflow::{ymd_from_ordinal, ActualDayCount},
    strict_json,
};
use serde::Serialize;
use std::{cell::Cell, sync::Arc};
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "counted source work overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "counted source work overflow".into())
}
#[derive(Debug, Clone)]
pub(crate) struct GeneratedCatalogAuthority {
    binding: Arc<[u8]>,
    minimum_bytes: usize,
}
impl GeneratedCatalogAuthority {
    pub(crate) fn bind<T: Serialize>(raw: &T) -> Result<Self, String> {
        let v = serde_json::to_value(raw).map_err(|e| e.to_string())?;
        Ok(Self {
            binding: strict_json::encode(raw, 1 << 20)?.into(),
            minimum_bytes: minimum_json_bytes(&v)?,
        })
    }
    pub(crate) fn verify<T: Serialize>(&self, raw: &T) -> Result<(), String> {
        if self.binding.as_ref() != strict_json::encode(raw, 1 << 20)? {
            return Err("stale counted catalog source/clock/path authority".into());
        }
        Ok(())
    }
    pub(crate) fn shared_body_lower_with(&self, other: &Self) -> usize {
        if Arc::ptr_eq(&self.binding, &other.binding) {
            self.minimum_bytes
        } else {
            0
        }
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.binding.len() + 64 + std::mem::size_of::<Self>()
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub(crate) struct OperationLedger {
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub periods: usize,
    pub observations: usize,
    pub segments: usize,
    pub calendar: usize,
    pub ordering: usize,
    pub binding: usize,
    pub validation: usize,
    pub earned_calls: usize,
    pub earned_steps: usize,
    pub canonical_calls: usize,
    pub canonical_steps: usize,
    pub pending_calls: usize,
    pub pending_steps: usize,
    pub terminal_calls: usize,
    pub terminal_steps: usize,
}
#[derive(Clone, Copy)]
pub(crate) enum Op {
    InputBytes,
    OutputBytes,
    Period,
    Observation,
    Segment,
    Calendar,
    Ordering,
    Binding,
    Validation,
    EarnedCall,
    EarnedStep,
    CanonicalCall,
    CanonicalStep,
    PendingCall,
    PendingStep,
    TerminalCall,
    TerminalStep,
}
impl OperationLedger {
    fn cell(&mut self, op: Op) -> &mut usize {
        match op {
            Op::InputBytes => &mut self.input_bytes,
            Op::OutputBytes => &mut self.output_bytes,
            Op::Period => &mut self.periods,
            Op::Observation => &mut self.observations,
            Op::Segment => &mut self.segments,
            Op::Calendar => &mut self.calendar,
            Op::Ordering => &mut self.ordering,
            Op::Binding => &mut self.binding,
            Op::Validation => &mut self.validation,
            Op::EarnedCall => &mut self.earned_calls,
            Op::EarnedStep => &mut self.earned_steps,
            Op::CanonicalCall => &mut self.canonical_calls,
            Op::CanonicalStep => &mut self.canonical_steps,
            Op::PendingCall => &mut self.pending_calls,
            Op::PendingStep => &mut self.pending_steps,
            Op::TerminalCall => &mut self.terminal_calls,
            Op::TerminalStep => &mut self.terminal_steps,
        }
    }
    pub(crate) fn work(&self) -> Result<usize, String> {
        let mut steps = 0;
        for v in [
            self.periods,
            self.observations,
            self.segments,
            self.ordering,
            self.binding,
            self.validation,
            self.earned_steps,
            self.canonical_steps,
            self.pending_steps,
            self.terminal_steps,
        ] {
            steps = add(steps, v)?
        }
        add(
            add(mul(16, self.input_bytes)?, self.output_bytes)?,
            add(mul(128, steps)?, mul(256, self.calendar)?)?,
        )
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct FactorUseBound {
    pub earned: usize,
    pub canonical: usize,
    pub pending: usize,
    pub terminal: usize,
}
impl FactorUseBound {
    pub(crate) fn new(
        queries: usize,
        segments: usize,
        observable: bool,
        terminal: bool,
    ) -> Result<Self, String> {
        let selected = terminal && queries > 0;
        Ok(Self {
            earned: queries,
            canonical: add(
                mul(queries, if observable { 3 } else { 1 })?,
                if selected { add(segments, 1)? } else { 0 },
            )?,
            pending: if observable && queries > 0 {
                add(add(mul(queries, 2)?, 68)?, mul(segments, 66)?)?
            } else {
                0
            },
            terminal: usize::from(selected),
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContractWorkSummary {
    pub policy: &'static str,
    pub use_bound: FactorUseBound,
    pub upper: OperationLedger,
}
// All immutable escaped UTF8 bytes are counted; numeric cells are <=32 bytes.
fn minimum_json_bytes(v: &serde_json::Value) -> Result<usize, String> {
    match v {
        serde_json::Value::String(_) => Ok(2),
        serde_json::Value::Number(_) => Ok(1),
        serde_json::Value::Bool(_) => Ok(4),
        serde_json::Value::Null => Ok(4),
        serde_json::Value::Array(v) => {
            let mut n = 2 + v.len().saturating_sub(1);
            for x in v {
                n = add(n, minimum_json_bytes(x)?)?
            }
            Ok(n)
        }
        serde_json::Value::Object(v) => {
            let mut n = 2 + v.len().saturating_sub(1);
            for (k, x) in v {
                n = add(
                    n,
                    add(
                        serde_json::to_vec(k).map_err(|e| e.to_string())?.len() + 1,
                        minimum_json_bytes(x)?,
                    )?,
                )?
            }
            Ok(n)
        }
    }
}
fn structural(v: &serde_json::Value) -> Result<usize, String> {
    match v {
        serde_json::Value::Number(_) => Ok(32),
        serde_json::Value::Array(a) => {
            let mut n = 2 + a.len();
            for v in a {
                n = add(n, structural(v)?)?
            }
            Ok(n)
        }
        serde_json::Value::Object(a) => {
            let mut n = 2 + 2 * a.len();
            for (k, v) in a {
                n = add(
                    n,
                    add(
                        serde_json::to_vec(k).map_err(|e| e.to_string())?.len(),
                        structural(v)?,
                    )?,
                )?
            }
            Ok(n)
        }
        _ => Ok(serde_json::to_vec(v).map_err(|e| e.to_string())?.len()),
    }
}
fn identity_upper(v: &mut serde_json::Value) {
    if let Some(o) = v.as_object_mut() {
        for k in ["id", "revision"] {
            if let Some(v) = o.get_mut(k) {
                *v = serde_json::Value::String("\\".repeat(128));
            }
        }
    }
}
fn expand_metadata(v: &mut serde_json::Value) {
    identity_upper(&mut v["path"]);
    if let Some(ps) = v["periods"].as_array_mut() {
        for p in ps {
            if let Some(os) = p["coupon"]
                .get_mut("observations")
                .and_then(serde_json::Value::as_array_mut)
            {
                for o in os {
                    for k in ["model", "path"] {
                        if let Some(v) = o["source"].get_mut(k) {
                            identity_upper(v)
                        }
                    }
                }
            }
        }
    }
}
fn metadata(raw: &ContractPathRequest, expanded: bool) -> Result<usize, String> {
    let mut v = serde_json::to_value(raw).map_err(|e| e.to_string())?;
    if expanded {
        expand_metadata(&mut v)
    }
    structural(&v)
}
pub(crate) fn request_bytes<T: Serialize>(raw: &T) -> Result<usize, String> {
    let mut v = serde_json::to_value(raw).map_err(|e| e.to_string())?;
    for role in ["original", "current"] {
        expand_metadata(&mut v["source"][role]);
    }
    if let Some(ds) = v["performing_draw_tranches"].as_array_mut() {
        for d in ds {
            for role in ["original", "current"] {
                expand_metadata(&mut d["source"][role]);
            }
        }
    }
    if let Some(ps) = v["credit"]["paths"].as_array_mut() {
        for p in ps {
            p["path_id"] = serde_json::Value::String("generated-511".into());
        }
    }
    for name in ["stream_id", "stream_revision"] {
        if let Some(v) = v["credit"]["draw_stream"].get_mut(name) {
            *v = serde_json::Value::String("\\".repeat(128));
        }
    }
    structural(&v)
}
pub(crate) fn core_request_bytes<T: Serialize>(raw: &T) -> Result<usize, String> {
    fn expand(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::String(s) => {
                if s.len() <= 256 {
                    *s = "\\".repeat(256);
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    expand(v)
                }
            }
            serde_json::Value::Object(o) => {
                for v in o.values_mut() {
                    expand(v)
                }
            }
            _ => (),
        }
    }
    let mut v = serde_json::to_value(raw).map_err(|e| e.to_string())?;
    expand(&mut v);
    structural(&v)
}
fn scan(start: i32, end: i32, basis: ActualDayCount) -> Result<usize, String> {
    let n = end.checked_sub(start).ok_or("counted calendar overflow")?;
    if !(0..=36600).contains(&n) {
        return Err("counted calendar source domain exceeded".into());
    }
    Ok(if matches!(basis, ActualDayCount::ActActIsda) {
        (n as usize).div_ceil(365) + 2
    } else {
        1
    })
}
impl ContractWorkSummary {
    pub(crate) fn census(
        raw: &ContractPathRequest,
        queries: usize,
        observable: bool,
        terminal: bool,
    ) -> Result<Self, String> {
        let p = raw.periods.len();
        let mut s = 0;
        let mut o = 0;
        let mut binding = 0;
        let mut years = 0;
        let mut query_years = 0;
        for period in &raw.periods {
            portfolio_compute_control::checkpoint()?;
            years = add(
                years,
                scan(period.start_ordinal, period.end_ordinal, period.day_count)?,
            )?;
            match &period.coupon {
                Coupon::Fixed { .. } => {
                    s = add(s, 1)?;
                    query_years = add(
                        query_years,
                        add(
                            scan(period.start_ordinal, period.end_ordinal, period.day_count)?,
                            1,
                        )?,
                    )?;
                }
                Coupon::Indexed {
                    observations,
                    observation_shift,
                    ..
                } => {
                    o = add(o, observations.len())?;
                    s = add(s, observations.len())?;
                    if observable {
                        binding = add(binding, mul(observations.len(), observations.len())?)?
                    }
                    for obs in observations {
                        let interest = scan(
                            obs.interest_start_ordinal,
                            obs.interest_end_ordinal,
                            period.day_count,
                        )?;
                        let observation = scan(
                            obs.observation_start_ordinal,
                            obs.observation_end_ordinal,
                            period.day_count,
                        )?;
                        let evaluation = if *observation_shift && observable {
                            add(add(mul(2, interest)?, mul(2, observation)?)?, 2)?
                        } else if *observation_shift {
                            add(add(interest, observation)?, 2)?
                        } else {
                            add(interest, 1)?
                        };
                        query_years = add(query_years, evaluation)?;
                        years = add(
                            years,
                            add(
                                scan(
                                    obs.interest_start_ordinal,
                                    obs.interest_end_ordinal,
                                    period.day_count,
                                )?,
                                scan(
                                    obs.observation_start_ordinal,
                                    obs.observation_end_ordinal,
                                    period.day_count,
                                )?,
                            )?,
                        )?;
                    }
                }
            }
        }
        let uses = FactorUseBound::new(queries, s, observable, terminal)?;
        let bytes = metadata(raw, true)?;
        let upper = OperationLedger {
            input_bytes: bytes,
            output_bytes: add(mul(bytes, 8)?, mul(8192, add(add(p, s)?, 1)?)?)?,
            periods: p,
            observations: o,
            segments: s,
            calendar: add(
                years,
                mul(
                    add(
                        if observable { 0 } else { mul(2, uses.earned)? },
                        uses.canonical,
                    )?,
                    query_years,
                )?,
            )?,
            ordering: mul(p, p)?,
            binding: add(add(mul(2, mul(p, p)?)?, mul(2, mul(p, s)?)?)?, binding)?,
            validation: add(mul(4, mul(p, p)?)?, 32)?,
            earned_calls: uses.earned,
            earned_steps: mul(uses.earned, add(mul(3, s)?, p)?)?,
            canonical_calls: uses.canonical,
            canonical_steps: mul(uses.canonical, add(mul(p, s)?, add(s, p)?)?)?,
            pending_calls: uses.pending,
            pending_steps: mul(uses.pending, s)?,
            terminal_calls: uses.terminal,
            terminal_steps: mul(uses.terminal, add(mul(s, s)?, mul(p, s)?)?)?,
        };
        upper.work()?;
        Ok(Self {
            policy: "counted_native_contract_factor_work-1",
            use_bound: uses,
            upper,
        })
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + std::mem::size_of::<WorkMeter>()
    }
}
#[derive(Debug)]
pub(crate) struct WorkMeter {
    upper: OperationLedger,
    observed: Cell<OperationLedger>,
    failed: Cell<bool>,
}
impl WorkMeter {
    pub(crate) fn new(summary: &ContractWorkSummary) -> Self {
        Self {
            upper: summary.upper,
            observed: Cell::new(OperationLedger::default()),
            failed: Cell::new(false),
        }
    }
    pub(crate) fn tick(&self, op: Op, n: usize) {
        let mut measured = self.observed.get();
        let mut upper = self.upper;
        let limit = *upper.cell(op);
        let cell = measured.cell(op);
        if let Some(v) = cell.checked_add(n) {
            *cell = v;
            if v > limit {
                self.failed.set(true)
            }
        } else {
            self.failed.set(true)
        }
        self.observed.set(measured);
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.failed.get() {
            return Err(format!("counted native source operation ledger exceeds preflight: observed={:?},upper={:?}",self.observed.get(),self.upper));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn observed(&self) -> OperationLedger {
        self.observed.get()
    }
}
pub(crate) fn bump(meter: Option<&WorkMeter>, op: Op, n: usize) -> Result<(), String> {
    if let Some(m) = meter {
        m.tick(op, n);
        m.check()?
    }
    Ok(())
}
pub(crate) fn calendar(
    meter: Option<&WorkMeter>,
    start: i32,
    end: i32,
    basis: ActualDayCount,
) -> Result<(), String> {
    if let Some(m) = meter {
        let steps = if matches!(basis, ActualDayCount::ActActIsda) && end > start {
            let first = ymd_from_ordinal(start)?.0;
            let last = ymd_from_ordinal(end - 1)?.0;
            (last - first + 1) as usize
        } else {
            1
        };
        bump(Some(m), Op::Calendar, steps)?
    }
    Ok(())
}
pub(crate) fn builder_start(
    meter: Option<&WorkMeter>,
    raw: &ContractPathRequest,
) -> Result<(), String> {
    if let Some(m) = meter {
        bump(Some(m), Op::InputBytes, metadata(raw, false)?)?;
        let p = raw.periods.len();
        bump(Some(m), Op::Validation, add(mul(4, mul(p, p)?)?, 32)?)?
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn export_receipt(
    raw: &ContractPathRequest,
    summary: &ContractWorkSummary,
    measured: OperationLedger,
) {
    if let Ok(path) = std::env::var("PORTFOLIO_COUNTED_WORK_RECEIPT") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({"raw":raw,"summary":summary,"measured":measured}),
        )
        .unwrap();
        file.write_all(b"\n").unwrap();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_counted_operation_family_fails_closed_at_its_exact_limit() {
        for op in [
            Op::InputBytes,
            Op::OutputBytes,
            Op::Period,
            Op::Observation,
            Op::Segment,
            Op::Calendar,
            Op::Ordering,
            Op::Binding,
            Op::Validation,
            Op::EarnedCall,
            Op::EarnedStep,
            Op::CanonicalCall,
            Op::CanonicalStep,
            Op::PendingCall,
            Op::PendingStep,
            Op::TerminalCall,
            Op::TerminalStep,
        ] {
            let mut upper = OperationLedger::default();
            *upper.cell(op) = 1;
            let summary = ContractWorkSummary {
                policy: "counted_native_contract_factor_work-1",
                use_bound: FactorUseBound::new(0, 1, false, false).unwrap(),
                upper,
            };
            let meter = WorkMeter::new(&summary);
            bump(Some(&meter), op, 1).unwrap();
            assert!(bump(Some(&meter), op, 1).is_err());
            assert!(meter.check().is_err());
        }
    }
    #[test]
    fn counted_json_scans_bound_escaped_utf8_strings_and_numeric_extremes() {
        let raw = serde_json::json!({"identity":"\\\"\u{e9}\u{96ea}","numbers":[f64::MAX,f64::MIN_POSITIVE,-0.0],"empty":[],"nested":{"x":true}});
        assert!(structural(&raw).unwrap() >= strict_json::serialized_size(&raw, 1 << 20).unwrap());
        assert!(
            minimum_json_bytes(&raw).unwrap()
                <= strict_json::serialized_size(&raw, 1 << 20).unwrap()
        );
    }
}
