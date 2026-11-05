//! Checked source compiler owns all factor descriptors; external input owns none.
use crate::products::commercial::acquisition::counted::Authority as FiniteAuthority;
use crate::products::term::contracts::indexed::work::{
    self, ContractWorkSummary, GeneratedCatalogAuthority, Op, WorkMeter,
};
use crate::{
    commercial_factor_source::segments::{FactorPolicy, FactorSegment},
    term_enhancements::indexed::{self, ContractPathRequest, ContractPathResult, Coupon},
};
use portfolio_model_core::cashflow::CashflowMeasure;
use serde::Serialize;
use std::cell::Cell;
use std::collections::BTreeMap;

#[path = "fixed_capacity.rs"]
mod fixed_capacity;
pub use fixed_capacity::FixedCapacity;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) counted_work: Option<ContractWorkSummary>,
}

#[cfg(test)]
mod observability_tests {
    use super::*;
    fn fixture() -> (ContractPathRequest, FactorPolicy) {
        let raw: crate::products::commercial::stochastic_exercise::Request =
            serde_json::from_slice(include_bytes!(
                "../../../../../../examples/models/commercial-generated-exercise-1.json"
            ))
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
            fixing: crate::products::term::contracts::Identity {
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
            fixing: crate::products::term::contracts::Identity {
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
    meter: Option<WorkMeter>,
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

#[derive(Default, Clone, Copy)]
struct SourceContext<'a> {
    transferred: bool,
    generated: Option<&'a GeneratedCatalogAuthority>,
    finite: Option<&'a FiniteAuthority<'a>>,
    basis: Option<&'a indexed::principal::AcquiredBasis>,
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
        Self::admit_for_measure_with_work(raw, policy, queries, measure, None)
    }
    pub(crate) fn admit_for_measure_with_work(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        authority: Option<&GeneratedCatalogAuthority>,
    ) -> Result<Admission, String> {
        Self::admit_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                generated: authority,
                ..SourceContext::default()
            },
            None,
        )
    }
    pub(crate) fn admit_transfer(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
    ) -> Result<Admission, String> {
        Self::admit_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                transferred: true,
                ..SourceContext::default()
            },
            None,
        )
    }
    pub(crate) fn admit_finite_transfer(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        finite: &FiniteAuthority<'_>,
        transferred: bool,
    ) -> Result<Admission, String> {
        Self::admit_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                transferred,
                finite: Some(finite),
                ..SourceContext::default()
            },
            None,
        )
    }
    fn admit_owned(
        raw: &ContractPathRequest,
        policy: &FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        context: SourceContext<'_>,
        measured_verification: Option<&Cell<usize>>,
    ) -> Result<Admission, String> {
        let SourceContext {
            transferred,
            generated: authority,
            finite,
            ..
        } = context;
        if finite.is_some() && authority.is_some() {
            return Err("mixed generated/finite source authority".into());
        }
        if let Some(finite) = finite {
            let scanned = finite.verify(raw, transferred)?;
            if let Some(measured) = measured_verification {
                measured.set(scanned);
            }
        }
        if transferred && authority.is_some() {
            return Err("generated counted transfer source ownership is unsupported".into());
        }
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
        indexed::principal::validate(raw)?;
        let mut s = 0usize;
        let mut observation_year_scan = 0usize;
        let midnight = |ordinal: i32| (i64::from(ordinal) - 719163) * 86_400;
        let retained_payment = matches!(
            raw.principal_plan,
            indexed::PrincipalPlan::RetainedPayment { .. }
        );
        let mut entitlement_known = midnight(raw.asof_ordinal);
        for p in &raw.periods {
            portfolio_compute_control::checkpoint()?;
            // Seasoned opening-right ownership is not invented by this compiler.
            if p.start_ordinal < raw.asof_ordinal && !transferred {
                return Err(
                    "seasoned coupon prefix requires separate native opening-right owner".into(),
                );
            }
            if retained_payment
                && policy.observable()
                && midnight(p.start_ordinal) < entitlement_known
            {
                return Err("observable retained-payment next opening principal needs its original coupon information before service starts; contingent principal owner required".into());
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
                            indexed::RateSource::ConditionalPublishedProjection {
                                publication_unix_seconds,
                                ..
                            } => {
                                if !policy.observable() {
                                    return Err("published projection requires explicit observable native factor policy".into());
                                }
                                *publication_unix_seconds
                            }
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
                        if !policy.observable()
                            && (availability > midnight(o.interest_start_ordinal)
                                || o.selection_cutoff_unix_seconds
                                    > midnight(o.interest_start_ordinal)
                                || reset_cutoff_unix_seconds
                                    .is_some_and(|cutoff| cutoff > midnight(p.start_ordinal)))
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
                        if policy.observable() {
                            let span = usize::try_from(observation_span)
                                .map_err(|_| "invalid observable source calendar span")?;
                            observation_year_scan =
                                add(observation_year_scan, add(span.div_ceil(365), 2)?)?;
                        }
                        if !policy.observable()
                            && *observation_shift
                            && interest_span != observation_span
                        {
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
        let year_scan = add(
            add((days as usize).div_ceil(365), 2)?,
            observation_year_scan,
        )?;
        let contingent_work = if policy.terminal_obligations.is_some() {
            mul(256, mul(s, add(s, observation_year_scan)?)?)?
        } else {
            0
        };
        let legacy_work = add(
            add(contingent_work, indexed::principal::work_upper(raw)?)?,
            add(
                add(add(250_000, mul(16, r)?)?, mul(128, s)?)?,
                mul(256, add(qs, mul(queries, year_scan)?)?)?,
            )?,
        )?;
        let contingent_bytes = if policy.terminal_obligations.is_some() {
            mul(1024, mul(s, s)?)?
        } else {
            0
        };
        let counted_work = if let Some(finite) = finite {
            Some(finite.summary(
                raw,
                queries,
                policy.observable(),
                policy.terminal_obligations.is_some(),
            )?)
        } else {
            authority
                .map(|_| {
                    ContractWorkSummary::census(
                        raw,
                        queries,
                        policy.observable(),
                        policy.terminal_obligations.is_some(),
                    )
                })
                .transpose()?
        };
        let work = if let Some(summary) = &counted_work {
            summary.upper.work()?
        } else {
            legacy_work
        };
        let verification_bytes = finite
            .map(FiniteAuthority::verification_bytes)
            .transpose()?
            .unwrap_or(0);
        let bytes = add(
            verification_bytes,
            add(
                counted_work
                    .as_ref()
                    .map_or(0, ContractWorkSummary::retained_bytes),
                add(
                    add(
                        contingent_bytes,
                        mul(indexed::principal::events(&raw.principal_plan).len(), 8192)?,
                    )?,
                    add(
                        mul(16, r)?,
                        mul(4096, add(add(s, raw.periods.len())?, qs)?)?,
                    )?,
                )?,
            )?,
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
            counted_work,
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
        Self::compile_for_measure_with_work(raw, policy, queries, measure, None)
    }
    pub(crate) fn compile_for_measure_with_work(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        authority: Option<&GeneratedCatalogAuthority>,
    ) -> Result<Self, String> {
        Self::compile_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                generated: authority,
                ..SourceContext::default()
            },
        )
    }
    pub(crate) fn compile_transfer(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
    ) -> Result<Self, String> {
        Self::compile_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                transferred: true,
                ..SourceContext::default()
            },
        )
    }
    pub(crate) fn compile_transfer_with_basis(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        basis: &indexed::principal::AcquiredBasis,
    ) -> Result<Self, String> {
        Self::compile_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                transferred: true,
                basis: Some(basis),
                ..SourceContext::default()
            },
        )
    }
    pub(crate) fn compile_finite_transfer(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        finite: &FiniteAuthority<'_>,
        basis: Option<&indexed::principal::AcquiredBasis>,
    ) -> Result<Self, String> {
        Self::compile_owned(
            raw,
            policy,
            queries,
            measure,
            SourceContext {
                transferred: true,
                finite: Some(finite),
                basis,
                ..SourceContext::default()
            },
        )
    }
    fn compile_owned(
        raw: ContractPathRequest,
        policy: FactorPolicy,
        queries: usize,
        measure: CashflowMeasure,
        context: SourceContext<'_>,
    ) -> Result<Self, String> {
        let SourceContext {
            transferred,
            finite,
            basis,
            ..
        } = context;
        let verified_bytes = Cell::new(0);
        let admission = Self::admit_owned(
            &raw,
            &policy,
            queries,
            measure,
            context,
            Some(&verified_bytes),
        )?;
        let meter = admission.counted_work.as_ref().map(WorkMeter::new);
        work::bump(meter.as_ref(), Op::InputBytes, verified_bytes.get())?;
        let fixed_capacity = if !transferred
            && basis.is_none()
            && finite.is_none()
            && meter.is_none()
            && policy.terminal_obligations.is_none()
            && matches!(raw.principal_plan, indexed::PrincipalPlan::Supplied { .. })
            && raw
                .periods
                .iter()
                .all(|p| matches!(p.coupon, Coupon::Fixed { .. }))
        {
            let capacity = Self::fixed_capacity(&raw, &policy)?;
            if capacity.construction_bytes_upper > admission.retained_bytes_upper {
                return Err("fixed factor construction exceeds original source reservation".into());
            }
            Some(capacity)
        } else {
            None
        };
        let s = admission.segment_bound;
        let mut segments = Vec::new();
        segments.try_reserve_exact(s).map_err(|e| e.to_string())?;
        #[cfg(test)]
        SOURCE_CALLS.with(|n| n.set(n.get() + 1));
        let mut capture = |segment| {
            portfolio_compute_control::checkpoint()?;
            if segments.len() == s {
                return Err("native segment count exceeds admitted source bound".into());
            }
            work::bump(meter.as_ref(), Op::Segment, 1)?;
            segments.push(segment);
            Ok(())
        };
        let source = if let Some(finite) = finite {
            indexed::build_transfer_path_counted(
                &raw,
                finite.information_asof(),
                meter.as_ref().ok_or("finite source work ledger absent")?,
                Some(&mut capture),
                basis,
            )?
        } else if let Some(basis) = basis {
            indexed::build_transfer_path_with_basis(&raw, &mut capture, basis)?
        } else if transferred {
            indexed::build_transfer_path_with_segments(&raw, &mut capture, raw.asof_ordinal)?
        } else if let Some(meter) = meter.as_ref() {
            indexed::build_path_with_counted_segments(&raw, &mut capture, meter)?
        } else {
            indexed::build_path_with_segments(&raw, &mut capture)?
        };
        if meter.is_some() {
            work::bump(
                meter.as_ref(),
                Op::OutputBytes,
                portfolio_model_core::strict_json::serialized_size(&source, 32 << 20)?,
            )?;
            work::bump(
                meter.as_ref(),
                Op::OutputBytes,
                portfolio_model_core::strict_json::serialized_size(
                    &segments.iter().map(|s| s.evidence()).collect::<Vec<_>>(),
                    32 << 20,
                )?,
            )?;
        }
        if segments.len() != s {
            return Err("native complete segment coverage mismatch".into());
        }
        let mut cash_by_id = BTreeMap::new();
        for (index, event) in source.schedule.events.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            work::bump(meter.as_ref(), Op::Binding, source.schedule.events.len())?;
            if cash_by_id.insert(event.event_id.as_str(), index).is_some() {
                return Err("duplicate native coupon cash origin".into());
            }
        }
        let mut cash_indices = Vec::with_capacity(s);
        work::bump(
            meter.as_ref(),
            Op::Binding,
            mul(raw.periods.len(), raw.periods.len())?,
        )?;
        let audit_by_id: BTreeMap<_, _> = source
            .audit
            .iter()
            .enumerate()
            .map(|(i, a)| (a.period_id.as_str(), i))
            .collect();
        for segment in &mut segments {
            work::bump(
                meter.as_ref(),
                Op::Binding,
                add(raw.periods.len(), source.schedule.events.len())?,
            )?;
            let index = *audit_by_id
                .get(segment.period_id())
                .ok_or("native coupon audit origin missing")?;
            if policy.observable() {
                let period = &raw.periods[index];
                if let Coupon::Indexed {
                    observations,
                    reset_cutoff_unix_seconds,
                    ..
                } = &period.coupon
                {
                    let observation = observations
                        .iter()
                        .find(|o| {
                            if let Some(meter) = meter.as_ref() {
                                meter.tick(Op::Binding, 1);
                            }
                            o.interest_start_ordinal == segment.start_ordinal()
                                && o.interest_end_ordinal == segment.end_ordinal()
                        })
                        .ok_or("native observable observation binding missing")?;
                    let availability = match observation.source {
                        indexed::RateSource::Observed {
                            publication_unix_seconds,
                            ..
                        } => publication_unix_seconds,
                        indexed::RateSource::ConditionalProjection {
                            information_unix_seconds,
                            ..
                        } => information_unix_seconds,
                        indexed::RateSource::ConditionalPublishedProjection {
                            publication_unix_seconds,
                            ..
                        } => publication_unix_seconds,
                    };
                    let information = availability
                        .max(observation.selection_cutoff_unix_seconds)
                        .max((i64::from(observation.fixing_ordinal) - 719163) * 86400)
                        .max(reset_cutoff_unix_seconds.unwrap_or(0));
                    segment.bind_observable_information(information, raw.asof_ordinal);
                }
            }
            segment.bind_audit_index(index);
            cash_indices.push(
                *cash_by_id
                    .get(segment.period_id())
                    .ok_or("native coupon-to-cash origin is missing")?,
            );
        }
        if let Some(meter) = meter.as_ref() {
            meter.check()?;
        }
        let result = Self {
            raw,
            source,
            policy,
            segments,
            cash_indices,
            admission,
            queries_used: Cell::new(0),
            meter,
        };
        if let Some(capacity) = fixed_capacity {
            if result.fixed_retained_capacity()? > capacity.retained_bytes_upper {
                return Err(
                    "fixed factor retention exceeds raw-derived construction inventory".into(),
                );
            }
        }
        Ok(result)
    }

    pub(crate) fn count_terminal_scan(&self) -> Result<(), String> {
        work::bump(self.meter.as_ref(), Op::TerminalCall, 1)?;
        work::bump(
            self.meter.as_ref(),
            Op::TerminalStep,
            add(
                mul(self.segments.len(), self.segments.len())?,
                mul(self.source.audit.len(), self.segments.len())?,
            )?,
        )
    }
    #[cfg(test)]
    pub(crate) fn counted_operations(&self) -> Option<work::OperationLedger> {
        let measured = self.meter.as_ref().map(WorkMeter::observed);
        if let (Some(summary), Some(measured)) = (&self.admission.counted_work, measured) {
            work::export_receipt(&self.raw, summary, measured);
        }
        measured
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
    /// Reuse the exact immutable binding established while compiling this
    /// source; independent principal rows cannot change coupon cash ownership.
    pub(crate) fn cash_index_for_segment(&self, index: usize) -> Result<usize, String> {
        self.cash_indices
            .get(index)
            .copied()
            .ok_or_else(|| "unbound native coupon cash index".into())
    }
    /// Cumulative source authority in contractual coupon order, independent of
    /// credit-origin partitioning and previously rounded monetary increments.
    pub fn cumulative_coupon_factors(&self, year: f64) -> Result<Vec<(usize, usize, f64)>, String> {
        portfolio_compute_control::checkpoint()?;
        if !year.is_finite() || year < 0. {
            return Err("invalid cumulative coupon source clock".into());
        }
        if self.policy.observable() {
            return self.observable_coupon_factors(year);
        }
        work::bump(self.meter.as_ref(), Op::CanonicalCall, 1)?;
        let mut result = Vec::new();
        for (audit_index, _) in self.source.audit.iter().enumerate() {
            let mut factor = 0.;
            let mut cash_index = None;
            for (index, segment) in self.segments.iter().enumerate() {
                work::bump(self.meter.as_ref(), Op::CanonicalStep, 1)?;
                if segment.audit_index() != audit_index {
                    continue;
                }
                portfolio_compute_control::checkpoint()?;
                cash_index = Some(self.cash_indices[index]);
                let start = f64::from(segment.start_ordinal() - self.raw.asof_ordinal) / 365.;
                let end = f64::from(segment.end_ordinal() - self.raw.asof_ordinal) / 365.;
                if year <= start {
                    break;
                }
                let time = year.min(end);
                let (day, fraction) = self.clock(segment, time, start, end)?;
                factor = segment.cumulative_metered(day, fraction, self.meter.as_ref())?;
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

    /// Observable cumulative factor, rebuilt only from selected available
    /// observations and known simple margin, in native contractual order.
    fn observable_coupon_factors(&self, year: f64) -> Result<Vec<(usize, usize, f64)>, String> {
        self.observable_frozen_coupon_factors(year, year)
    }
    pub(crate) fn observable_frozen_coupon_factors(
        &self,
        served_year: f64,
        information_year: f64,
    ) -> Result<Vec<(usize, usize, f64)>, String> {
        work::bump(self.meter.as_ref(), Op::CanonicalCall, 1)?;
        let (ordinal, fraction) = self.absolute_clock(served_year)?;
        let mut result = Vec::with_capacity(self.source.audit.len());
        let mut state = portfolio_model_core::coupon_factor_state::CouponFactorState::new();
        let mut active = None;
        for (index, segment) in self.segments.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            work::bump(self.meter.as_ref(), Op::CanonicalStep, 1)?;
            if active != Some(segment.audit_index()) {
                state = portfolio_model_core::coupon_factor_state::CouponFactorState::new();
                active = Some(segment.audit_index());
            }
            segment.observe(
                &mut state,
                ordinal,
                fraction,
                information_year,
                self.meter.as_ref(),
            )?;
            let value = segment.observed_factor(state);
            if !value.is_finite() {
                return Err("observable native factor overflow".into());
            }
            if self
                .segments
                .get(index + 1)
                .is_none_or(|next| next.audit_index() != segment.audit_index())
            {
                result.push((segment.audit_index(), self.cash_indices[index], value));
            }
        }
        Ok(result)
    }

    pub(crate) fn pending_indices(
        &self,
        served_year: f64,
        information_year: f64,
    ) -> Result<Vec<usize>, String> {
        if !self.policy.observable() {
            return Ok(Vec::new());
        }
        work::bump(self.meter.as_ref(), Op::PendingCall, 1)?;
        work::bump(self.meter.as_ref(), Op::PendingStep, self.segments.len())?;
        let (day, fraction) = self.absolute_clock(served_year)?;
        Ok(self
            .segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.pending_at(day, fraction, information_year))
            .map(|(i, _)| i)
            .collect())
    }

    pub fn pending_obligations(
        &self,
        year: f64,
    ) -> Result<Vec<super::segments::PendingCouponObligation<'_>>, String> {
        if !self.policy.observable() {
            return Ok(Vec::new());
        }
        work::bump(self.meter.as_ref(), Op::PendingCall, 1)?;
        work::bump(self.meter.as_ref(), Op::PendingStep, self.segments.len())?;
        let (ordinal, fraction) = self.absolute_clock(year)?;
        let mut result = Vec::new();
        for segment in &self.segments {
            portfolio_compute_control::checkpoint()?;
            if let Some(obligation) = segment.pending_obligation(ordinal, fraction, year) {
                result.push(obligation);
            }
        }
        Ok(result)
    }

    pub(crate) fn validate_terminal(&self, year: f64) -> Result<(), String> {
        if !self.policy.observable() || self.policy.terminal_obligations.is_some() {
            return Ok(());
        }
        if !self.pending_obligations(year)?.is_empty() {
            return Err("observable terminal event retains an unquantified pre-exit coupon obligation; reviewed contingent-right settlement owner required, no waiver or future fixing substitution".into());
        }
        Ok(())
    }

    fn absolute_clock(&self, year: f64) -> Result<(i32, f64), String> {
        if !year.is_finite() || !(0. ..=100.).contains(&year) {
            return Err("invalid observable source clock".into());
        }
        let days = year * 365.;
        let round = days.round();
        if year == round / 365. {
            return Ok((self.raw.asof_ordinal + round as i32, 0.));
        }
        let whole = days.floor();
        Ok((self.raw.asof_ordinal + whole as i32, days - whole))
    }

    /// Factor-only sojourn query; the consuming Core owner supplies actual P.
    pub fn earned(&self, start_year: f64, end_year: f64) -> Result<Vec<FactorAtom<'_>>, String> {
        portfolio_compute_control::checkpoint()?;
        work::bump(self.meter.as_ref(), Op::EarnedCall, 1)?;
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
        if self.policy.observable() {
            let before = self.observable_coupon_factors(start_year)?;
            let after = self.observable_coupon_factors(end_year)?;
            let mut atoms = Vec::with_capacity(before.len());
            let mut segment_index = 0;
            for ((audit, cash, start_factor), (_, _, end_factor)) in before.into_iter().zip(after) {
                work::bump(self.meter.as_ref(), Op::EarnedStep, 1)?;
                while self
                    .segments
                    .get(segment_index)
                    .is_some_and(|s| s.audit_index() != audit)
                {
                    portfolio_compute_control::checkpoint()?;
                    work::bump(self.meter.as_ref(), Op::EarnedStep, 1)?;
                    segment_index += 1;
                }
                if segment_index == self.segments.len() {
                    return Err("observable native coupon origin missing".into());
                }
                if start_factor == end_factor {
                    continue;
                }
                let segment = &self.segments[segment_index];
                atoms.push(FactorAtom {
                    segment_index,
                    native_source_event_id: segment.period_id(),
                    period_id: segment.period_id(),
                    delta_factor: end_factor - start_factor,
                    start_factor,
                    end_factor,
                    source_cash_index: cash,
                    native_audit_index: audit,
                });
            }
            return Ok(atoms);
        }
        let mut atoms = Vec::new();
        atoms
            .try_reserve_exact(self.segments.len())
            .map_err(|e| e.to_string())?;
        for (index, segment) in self.segments.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            work::bump(self.meter.as_ref(), Op::EarnedStep, 1)?;
            let start = f64::from(segment.start_ordinal() - self.raw.asof_ordinal) / 365.;
            let end = f64::from(segment.end_ordinal() - self.raw.asof_ordinal) / 365.;
            let lo = start_year.max(start);
            let hi = end_year.min(end);
            if hi <= lo {
                continue;
            }
            let (lo_day, lo_fraction) = self.clock(segment, lo, start, end)?;
            let (hi_day, hi_fraction) = self.clock(segment, hi, start, end)?;
            let before = segment.cumulative_metered(lo_day, lo_fraction, self.meter.as_ref())?;
            let after = segment.cumulative_metered(hi_day, hi_fraction, self.meter.as_ref())?;
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
