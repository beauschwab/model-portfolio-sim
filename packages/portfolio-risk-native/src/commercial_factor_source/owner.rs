//! Checked source compiler owns all factor descriptors; external input owns none.
use crate::{
    commercial_factor_source::segments::{FactorPolicy, FactorSegment},
    term_enhancements::indexed::{self, ContractPathRequest, ContractPathResult, Coupon},
};
use portfolio_model_core::cashflow::CashflowMeasure;
use serde::Serialize;
use std::cell::Cell;
use std::collections::BTreeMap;

#[cfg(test)]
thread_local! { pub(crate) static SOURCE_CALLS: Cell<usize> = const { Cell::new(0) }; }

#[derive(Debug, Serialize)]
pub struct Admission {
    pub segment_bound: usize,
    pub query_bound: usize,
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
    pub typed_source_structural_upper: usize,
    pub calendar_year_scan_bound: usize,
}

#[cfg(test)]
mod observability_tests {
    use super::*;
    fn fixture() -> (ContractPathRequest, FactorPolicy) {
        let raw: crate::commercial_stochastic_exercise::Request = serde_json::from_slice(
            include_bytes!("../../../../examples/models/commercial-generated-exercise-1.json"),
        )
        .unwrap();
        (
            raw.source.source.current.clone(),
            raw.source.factor_policy.clone(),
        )
    }
    fn opening_indexed_fixture() -> (ContractPathRequest, FactorPolicy) {
        let (mut raw, policy) = fixture();
        let mut period = raw
            .periods
            .iter()
            .find(|p| matches!(p.coupon, Coupon::Indexed { .. }))
            .unwrap()
            .clone();
        let shift = period.start_ordinal - raw.asof_ordinal;
        period.start_ordinal -= shift;
        period.end_ordinal -= shift;
        period.payment_ordinal -= shift;
        let Coupon::Indexed {
            observations,
            reset_cutoff_unix_seconds,
            ..
        } = &mut period.coupon
        else {
            unreachable!()
        };
        if let Some(cutoff) = reset_cutoff_unix_seconds {
            *cutoff -= i64::from(shift) * 86400;
        }
        for o in observations {
            o.interest_start_ordinal -= shift;
            o.interest_end_ordinal -= shift;
            o.observation_start_ordinal -= shift;
            o.observation_end_ordinal -= shift;
            o.fixing_ordinal -= shift;
            o.selection_cutoff_unix_seconds -= i64::from(shift) * 86400;
            if let indexed::RateSource::ConditionalProjection {
                information_unix_seconds,
                ..
            } = &mut o.source
            {
                *information_unix_seconds -= i64::from(shift) * 86400;
            }
        }
        raw.periods = vec![period];
        raw.principal_plan = indexed::PrincipalPlan::Supplied {
            principal_payments: vec![raw.opening_principal],
        };
        (raw, policy)
    }
    #[test]
    fn indexed_actual_cash_before_future_fixing_refuses_before_generation() {
        let (mut raw, policy) = fixture();
        let period = raw
            .periods
            .iter_mut()
            .find(|p| matches!(p.coupon, Coupon::Indexed { .. }))
            .unwrap();
        period.payment_ordinal = period.start_ordinal - 1;
        SOURCE_CALLS.with(|n| n.set(0));
        let error = NativeFactors::compile_for_measure(raw, policy, 0, CashflowMeasure::Pricing)
            .unwrap_err();
        assert!(error.contains("before all fixings"));
        SOURCE_CALLS.with(|n| assert_eq!(n.get(), 0));
    }
    #[test]
    fn fixed_early_cash_cannot_reveal_future_retained_payment_balance() {
        let (mut raw, policy) = fixture();
        let mut last = raw.periods.last().unwrap().clone();
        last.period_id = "future-fixed-retained-balance".into();
        last.start_ordinal = last.end_ordinal;
        last.end_ordinal = raw.tail_end_ordinal;
        last.payment_ordinal = raw.asof_ordinal + 1;
        last.coupon = Coupon::Fixed { annual_rate: 0. };
        raw.periods.push(last);
        raw.principal_plan = indexed::PrincipalPlan::RetainedPayment {
            original_payment: 20.,
            terminal: indexed::TerminalPrincipal::ResidualPayoff {},
        };
        indexed::build_path(&raw).unwrap();
        SOURCE_CALLS.with(|n| n.set(0));
        let error = NativeFactors::compile_for_measure(raw, policy, 0, CashflowMeasure::Pricing)
            .unwrap_err();
        assert!(error.contains("inherited indexed entitlement availability"));
        SOURCE_CALLS.with(|n| assert_eq!(n.get(), 0));
    }
    #[test]
    fn delayed_observed_publication_cannot_fund_partial_earned_state() {
        let (mut raw, policy) = opening_indexed_fixture();
        let period = raw
            .periods
            .iter_mut()
            .find(|p| matches!(p.coupon, Coupon::Indexed { .. }))
            .unwrap();
        let Coupon::Indexed {
            observations,
            reset_cutoff_unix_seconds,
            ..
        } = &mut period.coupon
        else {
            unreachable!()
        };
        let observation = &mut observations[0];
        let publication = (i64::from(observation.interest_start_ordinal) - 719163) * 86_400 + 3600;
        observation.source = indexed::RateSource::Observed {
            fixing: crate::term_enhancements::Identity {
                id: "delayed-real-publication".into(),
                revision: "1".into(),
            },
            publication_unix_seconds: publication,
        };
        observation.selection_cutoff_unix_seconds = publication;
        *reset_cutoff_unix_seconds = Some(publication);
        raw.observed_publication_cutoff_unix_seconds = publication;
        // Broad supplied indexed arithmetic accepts this same-day term reset.
        indexed::build_path(&raw).unwrap();
        SOURCE_CALLS.with(|n| n.set(0));
        let error = NativeFactors::compile_for_measure(raw, policy, 0, CashflowMeasure::Pricing)
            .unwrap_err();
        assert!(error.contains("interest-segment start midnight"));
        SOURCE_CALLS.with(|n| assert_eq!(n.get(), 0));
    }
    #[test]
    fn already_published_lookback_fixing_preserves_native_factor_and_actual_cash() {
        let (mut raw, policy) = opening_indexed_fixture();
        let period = raw
            .periods
            .iter_mut()
            .find(|p| matches!(p.coupon, Coupon::Indexed { .. }))
            .unwrap();
        let Coupon::Indexed { observations, .. } = &mut period.coupon else {
            unreachable!()
        };
        let observation = &mut observations[0];
        observation.fixing_ordinal = observation.interest_start_ordinal - 2;
        let publication = (i64::from(observation.interest_start_ordinal - 1) - 719163) * 86_400;
        observation.source = indexed::RateSource::Observed {
            fixing: crate::term_enhancements::Identity {
                id: "published-lookback".into(),
                revision: "1".into(),
            },
            publication_unix_seconds: publication,
        };
        raw.observed_publication_cutoff_unix_seconds = publication;
        let expected = indexed::build_path(&raw).unwrap();
        let native =
            NativeFactors::compile_for_measure(raw, policy, 1, CashflowMeasure::Pricing).unwrap();
        assert_eq!(
            serde_json::to_vec(native.source()).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        assert!(!native
            .earned(
                0.,
                native
                    .raw()
                    .periods
                    .last()
                    .unwrap()
                    .end_ordinal
                    .checked_sub(native.raw().asof_ordinal)
                    .unwrap() as f64
                    / 365.
            )
            .unwrap()
            .is_empty());
    }
}

#[derive(Debug, Serialize)]
pub struct FactorAtom<'a> {
    pub segment_index: usize,
    pub native_source_event_id: &'a str,
    pub period_id: &'a str,
    pub delta_factor: f64,
    pub start_factor: f64,
    pub end_factor: f64,
    pub source_cash_index: usize,
    pub native_audit_index: usize,
}

#[derive(Debug)]
pub struct NativeFactors {
    raw: ContractPathRequest,
    source: ContractPathResult,
    policy: FactorPolicy,
    segments: Vec<FactorSegment>,
    cash_indices: Vec<usize>,
    admission: Admission,
    queries_used: Cell<usize>,
}

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "factor bound overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "factor bound overflow".into())
}

fn structural(value: &serde_json::Value) -> Result<usize, String> {
    portfolio_compute_control::checkpoint()?;
    match value {
        serde_json::Value::Null => Ok(4),
        serde_json::Value::Bool(_) => Ok(5),
        serde_json::Value::Number(_) => Ok(32),
        serde_json::Value::String(s) => serde_json::to_vec(s)
            .map(|v| v.len())
            .map_err(|e| e.to_string()),
        serde_json::Value::Array(items) => {
            let mut n = add(2, items.len().saturating_sub(1))?;
            for v in items {
                n = add(n, structural(v)?)?;
            }
            Ok(n)
        }
        serde_json::Value::Object(items) => {
            let mut n = add(2, items.len().saturating_sub(1))?;
            for (k, v) in items {
                n = add(n, serde_json::to_vec(k).map_err(|e| e.to_string())?.len())?;
                n = add(n, add(1, structural(v)?)?)?;
            }
            Ok(n)
        }
    }
}

impl NativeFactors {
    pub fn admit(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
    ) -> Result<Admission, String> {
        Self::admit_for_measure(raw, policy, queries, CashflowMeasure::Physical)
    }
    /// Native economic graph entrypoint: requires an already explicitly tagged
    /// source. This never converts physical intensities or fixings to pricing.
    pub fn admit_for_measure(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
    ) -> Result<Admission, String> {
        portfolio_compute_control::checkpoint()?;
        policy.validate()?;
        if raw.measure != measure
            || queries > 4096
            || raw.periods.is_empty()
            || raw.periods.len() > 4096
        {
            return Err("bounded explicitly measure-matched native factor source required".into());
        }
        let encoded = portfolio_model_core::strict_json::serialized_size(&raw, 1024 * 1024)?;
        let r = structural(&serde_json::to_value(raw).map_err(|e| e.to_string())?)?;
        if encoded > 1024 * 1024 {
            return Err("typed factor source exceeds 1 MiB".into());
        }
        let mut s = 0usize;
        let midnight = |ordinal: i32| (i64::from(ordinal) - 719163) * 86_400;
        let retained_payment = matches!(
            raw.principal_plan,
            indexed::PrincipalPlan::RetainedPayment { .. }
        );
        let mut entitlement_known = midnight(raw.asof_ordinal);
        for p in &raw.periods {
            portfolio_compute_control::checkpoint()?;
            // Seasoned opening-right ownership is not invented by this compiler.
            if p.start_ordinal < raw.asof_ordinal {
                return Err(
                    "seasoned coupon prefix requires separate native opening-right owner".into(),
                );
            }
            if retained_payment && midnight(p.payment_ordinal) < entitlement_known {
                return Err("native retained-payment actual cash precedes inherited indexed entitlement availability".into());
            }
            match &p.coupon {
                Coupon::Fixed { .. } => s = add(s, 1)?,
                Coupon::Indexed {
                    observations,
                    observation_shift,
                    reset_cutoff_unix_seconds,
                    ..
                } => {
                    for o in observations {
                        // This continuous actual-path owner has midnight dated
                        // settlement/segment boundaries. The broad supplied
                        // indexed analysis contract remains separately usable.
                        let availability = match &o.source {
                            indexed::RateSource::Observed {
                                publication_unix_seconds,
                                ..
                            } => *publication_unix_seconds,
                            indexed::RateSource::ConditionalProjection {
                                information_unix_seconds,
                                ..
                            } => *information_unix_seconds,
                        };
                        if o.fixing_ordinal > p.payment_ordinal
                            || availability > midnight(p.payment_ordinal)
                            || o.selection_cutoff_unix_seconds > midnight(p.payment_ordinal)
                        {
                            return Err("native actual cash cannot settle indexed coupon before all fixings, source availability and selection cutoffs".into());
                        }
                        if availability > midnight(o.interest_start_ordinal)
                            || o.selection_cutoff_unix_seconds > midnight(o.interest_start_ordinal)
                            || reset_cutoff_unix_seconds
                                .is_some_and(|cutoff| cutoff > midnight(p.start_ordinal))
                        {
                            return Err("native continuous factor requires rate selected and available by interest-segment start midnight; intraday contingent-rate modeling is unsupported".into());
                        }
                        if retained_payment {
                            entitlement_known = entitlement_known
                                .max(availability)
                                .max(o.selection_cutoff_unix_seconds)
                                .max(midnight(o.fixing_ordinal));
                            if let Some(reset) = reset_cutoff_unix_seconds {
                                entitlement_known = entitlement_known.max(*reset);
                            }
                        }
                        let interest_span = o
                            .interest_end_ordinal
                            .checked_sub(o.interest_start_ordinal)
                            .ok_or("interest calendar overflow")?;
                        let observation_span = o
                            .observation_end_ordinal
                            .checked_sub(o.observation_start_ordinal)
                            .ok_or("observation calendar overflow")?;
                        if *observation_shift && interest_span != observation_span {
                            return Err(
                                "continuous shifted factor requires explicit equal calendar spans"
                                    .into(),
                            );
                        }
                    }
                    s = add(s, observations.len())?;
                }
            }
        }
        if s == 0 || s > 4096 {
            return Err("native segment component bound exceeded before source generation".into());
        }
        let qs = mul(queries, s)?;
        portfolio_model_core::cashflow::validate_ordinal(raw.asof_ordinal)?;
        portfolio_model_core::cashflow::validate_ordinal(raw.tail_end_ordinal)?;
        let days = raw
            .tail_end_ordinal
            .checked_sub(raw.asof_ordinal)
            .ok_or("source calendar overflow")?;
        if !(0..=36_600).contains(&days) {
            return Err("source complete calendar exceeds native domain".into());
        }
        // Across disjoint actual source segments, ACT/ACT can additionally split
        // Gregorian years. Charge both endpoint evaluations conservatively; do
        // not pretend one calendar call is a constant-cost long-span operation.
        let year_scan = add((days as usize).div_ceil(365), 2)?;
        let work = add(
            add(add(250_000, mul(16, r)?)?, mul(128, s)?)?,
            mul(256, add(qs, mul(queries, year_scan)?)?)?,
        )?;
        let bytes = add(
            mul(16, r)?,
            mul(4096, add(add(s, raw.periods.len())?, qs)?)?,
        )?;
        if work > 8_000_000 || bytes > 32 * 1024 * 1024 {
            return Err(
                "joined source/factor preflight work or capacity exceeded before generation".into(),
            );
        }
        let admission = Admission {
            segment_bound: s,
            query_bound: queries,
            work_upper: work,
            retained_bytes_upper: bytes,
            typed_source_structural_upper: r,
            calendar_year_scan_bound: year_scan,
        };
        Ok(admission)
    }
    pub fn compile(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
    ) -> Result<Self, String> {
        Self::compile_for_measure(raw, policy, queries, CashflowMeasure::Physical)
    }
    pub fn compile_for_measure(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
    ) -> Result<Self, String> {
        let admission = Self::admit_for_measure(&raw, &policy, queries, measure)?;
        let s = admission.segment_bound;
        let mut segments = Vec::new();
        segments.try_reserve_exact(s).map_err(|e| e.to_string())?;
        #[cfg(test)]
        SOURCE_CALLS.with(|n| n.set(n.get() + 1));
        let source = indexed::build_path_with_segments(&raw, &mut |segment| {
            portfolio_compute_control::checkpoint()?;
            if segments.len() == s {
                return Err("native segment count exceeds admitted source bound".into());
            }
            segments.push(segment);
            Ok(())
        })?;
        if segments.len() != s {
            return Err("native complete segment coverage mismatch".into());
        }
        let mut cash_by_id = BTreeMap::new();
        for (index, event) in source.schedule.events.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            if cash_by_id.insert(event.event_id.as_str(), index).is_some() {
                return Err("duplicate native coupon cash origin".into());
            }
        }
        let mut cash_indices = Vec::with_capacity(s);
        let audit_by_id: BTreeMap<_, _> = source
            .audit
            .iter()
            .enumerate()
            .map(|(i, a)| (a.period_id.as_str(), i))
            .collect();
        for segment in &mut segments {
            let index = *audit_by_id
                .get(segment.period_id())
                .ok_or("native coupon audit origin missing")?;
            segment.bind_audit_index(index);
            cash_indices.push(
                *cash_by_id
                    .get(segment.period_id())
                    .ok_or("native coupon-to-cash origin is missing")?,
            );
        }
        Ok(Self {
            raw,
            source,
            policy,
            segments,
            cash_indices,
            admission,
            queries_used: Cell::new(0),
        })
    }

    pub fn raw(&self) -> &ContractPathRequest {
        &self.raw
    }
    pub fn source(&self) -> &ContractPathResult {
        &self.source
    }
    pub fn policy(&self) -> &FactorPolicy {
        &self.policy
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn segments(&self) -> &[FactorSegment] {
        &self.segments
    }
    /// Cumulative source authority in contractual coupon order, independent of
    /// credit-origin partitioning and previously rounded monetary increments.
    pub fn cumulative_coupon_factors(&self, year: f64) -> Result<Vec<(usize, usize, f64)>, String> {
        portfolio_compute_control::checkpoint()?;
        if !year.is_finite() || year < 0. {
            return Err("invalid cumulative coupon source clock".into());
        }
        let mut result = Vec::new();
        for (audit_index, _) in self.source.audit.iter().enumerate() {
            let mut factor = 0.;
            let mut cash_index = None;
            for (index, segment) in self
                .segments
                .iter()
                .enumerate()
                .filter(|(_, s)| s.audit_index() == audit_index)
            {
                portfolio_compute_control::checkpoint()?;
                cash_index = Some(self.cash_indices[index]);
                let start = f64::from(segment.start_ordinal() - self.raw.asof_ordinal) / 365.;
                let end = f64::from(segment.end_ordinal() - self.raw.asof_ordinal) / 365.;
                if year <= start {
                    break;
                }
                let time = year.min(end);
                let (day, fraction) = self.clock(segment, time, start, end)?;
                factor = segment.cumulative(day, fraction)?;
                if year < end {
                    break;
                }
            }
            result.push((
                audit_index,
                cash_index.ok_or("canonical source coupon has no cash origin")?,
                factor,
            ));
        }
        Ok(result)
    }

    /// Factor-only sojourn query; the consuming Core owner supplies actual P.
    pub fn earned(&self, start_year: f64, end_year: f64) -> Result<Vec<FactorAtom<'_>>, String> {
        portfolio_compute_control::checkpoint()?;
        if self.queries_used.get() >= self.admission.query_bound {
            return Err("factor query count exceeds retained admission".into());
        }
        self.queries_used.set(self.queries_used.get() + 1);
        let full = f64::from(self.raw.tail_end_ordinal - self.raw.asof_ordinal) / 365.;
        if !start_year.is_finite()
            || !end_year.is_finite()
            || start_year < 0.
            || end_year < start_year
            || end_year > full
        {
            return Err("factor query outside native complete ACT365 source clock".into());
        }
        let mut atoms = Vec::new();
        atoms
            .try_reserve_exact(self.segments.len())
            .map_err(|e| e.to_string())?;
        for (index, segment) in self.segments.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            let start = f64::from(segment.start_ordinal() - self.raw.asof_ordinal) / 365.;
            let end = f64::from(segment.end_ordinal() - self.raw.asof_ordinal) / 365.;
            let lo = start_year.max(start);
            let hi = end_year.min(end);
            if hi <= lo {
                continue;
            }
            let (lo_day, lo_fraction) = self.clock(segment, lo, start, end)?;
            let (hi_day, hi_fraction) = self.clock(segment, hi, start, end)?;
            let before = segment.cumulative(lo_day, lo_fraction)?;
            let after = segment.cumulative(hi_day, hi_fraction)?;
            let delta = after - before;
            if !delta.is_finite() {
                return Err("factor delta overflow".into());
            }
            atoms.push(FactorAtom {
                segment_index: index,
                native_source_event_id: segment.period_id(),
                period_id: segment.period_id(),
                delta_factor: delta,
                start_factor: before,
                end_factor: after,
                source_cash_index: self.cash_indices[index],
                native_audit_index: segment.audit_index(),
            });
        }
        Ok(atoms)
    }

    fn clock(
        &self,
        segment: &FactorSegment,
        time: f64,
        start: f64,
        end: f64,
    ) -> Result<(i32, f64), String> {
        // Equality with the native source's declared elapsed clock is exact,
        // not a near comparison or time/balance clamp.
        if time == start {
            return Ok((segment.start_ordinal(), 0.));
        }
        if time == end {
            return Ok((segment.end_ordinal(), 0.));
        }
        let days = time * 365.;
        let integer_day = days.round();
        if time == integer_day / 365. {
            let ordinal = self
                .raw
                .asof_ordinal
                .checked_add(integer_day as i32)
                .ok_or("query calendar overflow")?;
            return Ok((ordinal, 0.));
        }
        let complete = days.floor();
        let fraction = days - complete;
        let ordinal = self
            .raw
            .asof_ordinal
            .checked_add(complete as i32)
            .ok_or("query calendar overflow")?;
        Ok((ordinal, fraction))
    }
}
