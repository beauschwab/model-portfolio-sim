//! Complete supplied conditional-index paths; no front-quarter proxy.
use super::*;
use portfolio_model_core::cashflow::*;
use portfolio_model_core::coupon_factor_state::CouponFactorState;
use portfolio_model_core::floating_rate::{
    Calculation, Direction, IndexDescriptor, IndexKind, MarginTreatment, RateBounds,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use crate::commercial_factor_source::segments::{FactorSegment, SegmentSink};

#[derive(Debug, Clone, Serialize)]
pub struct EarnedFactorPoint {
    pub period_id: String,
    pub ordinal: i32,
    pub cumulative_factor: f64,
    pub source: Option<RateSource>,
}

pub type EarnedFactorSink<'a> = dyn FnMut(EarnedFactorPoint) -> Result<(), String> + 'a;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RateSource {
    Observed {
        fixing: Identity,
        publication_unix_seconds: i64,
    },
    /// Information available at the simulated reset/observation, never a
    /// realized future discount ratio masquerading as an in-advance fixing.
    ConditionalProjection {
        model: Identity,
        path: Identity,
        information_unix_seconds: i64,
        measure: CashflowMeasure,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexObservation {
    pub interest_start_ordinal: i32,
    pub interest_end_ordinal: i32,
    pub observation_start_ordinal: i32,
    pub observation_end_ordinal: i32,
    pub fixing_ordinal: i32,
    /// Exact cutoff when this observation is selected. For projected overnight
    /// observations this must be on/before its simulated observation date.
    pub selection_cutoff_unix_seconds: i64,
    pub rate: f64,
    pub source: RateSource,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Coupon {
    Fixed {
        annual_rate: f64,
    },
    Indexed {
        index: IndexDescriptor,
        calendar: Box<Identity>,
        complete_calendar_mapping: bool,
        calculation: Calculation,
        margin_treatment: MarginTreatment,
        margin: f64,
        index_bounds: RateBounds,
        all_in_bounds: RateBounds,
        reset_cutoff_unix_seconds: Option<i64>,
        lookback_business_days: u32,
        observation_shift: bool,
        observations: Vec<IndexObservation>,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractPeriod {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub day_count: ActualDayCount,
    pub coupon: Coupon,
    /// Receipt-positive signed contractual fee; not implicit book amortization.
    pub fee: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrincipalPlan {
    Supplied {
        principal_payments: Vec<f64>,
    },
    /// Original payment is supplied, never recalculated on remaining tenor.
    RetainedPayment {
        original_payment: f64,
        terminal: TerminalPrincipal,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalPrincipal {
    ResidualPayoff {},
    /// Extra terminal principal above the retained final regular payment.
    Balloon {
        amount: f64,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractPathRequest {
    pub schema: String,
    pub model_version: String,
    pub contract: Identity,
    pub assumption: Identity,
    pub path: Identity,
    pub position_id: String,
    pub scenario_id: String,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub measure: CashflowMeasure,
    pub asof_ordinal: i32,
    pub reporting_end_ordinal: i32,
    /// Complete final settlement domain, independently of report horizon.
    pub tail_end_ordinal: i32,
    pub observed_publication_cutoff_unix_seconds: i64,
    pub opening_principal: f64,
    pub principal_plan: PrincipalPlan,
    pub periods: Vec<ContractPeriod>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PeriodAudit {
    pub period_id: String,
    pub opening_principal: f64,
    pub interest: f64,
    pub principal_paid: f64,
    pub closing_principal: f64,
    pub terminal_balloon: f64,
    pub calendar_mapping_verified: bool,
    pub selected_rate_sources: Vec<RateSource>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContractPathResult {
    pub schema: String,
    pub model_version: String,
    pub contract: Identity,
    pub assumption: Identity,
    pub path: Identity,
    pub schedule: DatedSchedule,
    pub audit: Vec<PeriodAudit>,
    pub principal_after_reporting: f64,
    pub disclosure: String,
}
fn bounded(bounds: RateBounds, rate: f64) -> Result<f64, String> {
    if bounds
        .floor
        .into_iter()
        .chain(bounds.cap)
        .any(|x| !x.is_finite())
        || matches!((bounds.floor,bounds.cap),(Some(a),Some(b)) if a>b)
    {
        return Err("invalid index/all-in bounds".into());
    }
    Ok(rate
        .max(bounds.floor.unwrap_or(f64::NEG_INFINITY))
        .min(bounds.cap.unwrap_or(f64::INFINITY)))
}
const MAX_TIMESTAMP: i64 = 253_402_300_799;
fn timestamp(t: i64) -> Result<(), String> {
    if !(0..=MAX_TIMESTAMP).contains(&t) {
        return Err("invalid exact UTC rate timestamp".into());
    }
    Ok(())
}
fn utc_day(t: i64) -> i32 {
    719_163 + (t / 86_400) as i32
}

fn coupon_factor(
    request: &ContractPathRequest,
    p: &ContractPeriod,
    bytes: &mut usize,
    work: &mut usize,
    capture: &mut Option<&mut EarnedFactorSink<'_>>,
    daily_fragments: bool,
    segments: &mut Option<&mut SegmentSink<'_>>,
) -> Result<(f64, Vec<RateSource>, bool), String> {
    let tau = actual_year_fraction(p.start_ordinal, p.end_ordinal, p.day_count)?;
    match &p.coupon {
        Coupon::Fixed { annual_rate } => {
            if !annual_rate.is_finite() || annual_rate * tau <= -1. {
                return Err("invalid fixed coupon factor".into());
            }
            let factor = amount(CouponFactorState::fixed_factor(*annual_rate, tau))?;
            if let Some(sink) = segments.as_deref_mut() {
                portfolio_compute_control::checkpoint()?;
                sink(FactorSegment::fixed(&p.period_id, p.start_ordinal, p.end_ordinal,
                    p.day_count, *annual_rate, factor))?;
            }
            if let Some(sink) = capture.as_deref_mut() {
                portfolio_compute_control::checkpoint()?;
                let first = if daily_fragments {
                    p.start_ordinal + 1
                } else {
                    p.end_ordinal
                };
                for day in first..=p.end_ordinal {
                    portfolio_compute_control::checkpoint()?;
                    let value = if day == p.end_ordinal {
                        factor
                    } else {
                        CouponFactorState::fixed_factor(
                            *annual_rate,
                            actual_year_fraction(p.start_ordinal, day, p.day_count)?,
                        )
                    };
                    sink(EarnedFactorPoint {
                        period_id: p.period_id.clone(),
                        ordinal: day,
                        cumulative_factor: value,
                        source: None,
                    })?;
                }
            }
            Ok((factor, vec![], true))
        }
        Coupon::Indexed {
            index,
            calendar,
            complete_calendar_mapping,
            calculation,
            margin_treatment,
            margin,
            index_bounds,
            all_in_bounds,
            reset_cutoff_unix_seconds,
            lookback_business_days,
            observation_shift,
            observations,
        } => {
            identity(calendar, bytes)?;
            text(&index.index_id, bytes)?;
            let valid_index = match index.kind {
                IndexKind::Term => {
                    matches!(index.tenor_months, Some(1..=1200))
                        && index.average_calendar_days.is_none()
                }
                IndexKind::Overnight => {
                    index.tenor_months.is_none() && index.average_calendar_days.is_none()
                }
                IndexKind::PublishedAverage => {
                    index.tenor_months.is_none()
                        && matches!(index.average_calendar_days, Some(1..=36600))
                }
            };
            *work = work
                .checked_add(observations.len())
                .ok_or("index work overflow")?;
            if !valid_index
                || !complete_calendar_mapping
                || !margin.is_finite()
                || *lookback_business_days > 366
                || observations.is_empty()
                || *work > 250_000
                || *calculation == Calculation::BalanceCompounded
                || (*calculation != Calculation::DailyCompounded
                    && *margin_treatment != MarginTreatment::Simple)
            {
                return Err("unsupported or incomplete index path/convention".into());
            }
            let term = *calculation == Calculation::TermSimple;
            if term {
                let reset = reset_cutoff_unix_seconds
                    .ok_or("term index requires exact reset UTC cutoff")?;
                timestamp(reset)?;
                if observations.len() != 1
                    || *observation_shift
                    || index.kind == IndexKind::Overnight
                    || utc_day(reset) > p.start_ordinal
                {
                    return Err("term fixing must be selected in advance for explicit tenor".into());
                }
            } else if index.kind != IndexKind::Overnight || reset_cutoff_unix_seconds.is_some() {
                return Err("daily index requires overnight observations and no term reset".into());
            }
            let mut cursor = p.start_ordinal;
            let mut prior_obs_end = None;
            let mut factor_state = CouponFactorState::new();
            let mut sources = Vec::with_capacity(observations.len());
            for o in observations {
                domain(o.interest_start_ordinal, o.interest_end_ordinal)?;
                domain(o.observation_start_ordinal, o.observation_end_ordinal)?;
                validate_ordinal(o.fixing_ordinal)?;
                timestamp(o.selection_cutoff_unix_seconds)?;
                if o.interest_start_ordinal != cursor
                    || o.interest_end_ordinal <= cursor
                    || o.interest_end_ordinal > p.end_ordinal
                    || o.observation_end_ordinal <= o.observation_start_ordinal
                    || o.observation_start_ordinal > o.interest_start_ordinal
                    || o.fixing_ordinal > o.interest_start_ordinal
                    || !o.rate.is_finite()
                    || (!term && o.fixing_ordinal != o.observation_start_ordinal)
                    || (*observation_shift
                        && prior_obs_end.is_some_and(|end| end != o.observation_start_ordinal))
                {
                    return Err(
                        "index observation dates must exactly cover interest dates and shift dates"
                            .into(),
                    );
                }
                let availability = match &o.source {
                    RateSource::Observed {
                        fixing,
                        publication_unix_seconds,
                    } => {
                        identity(fixing, bytes)?;
                        timestamp(*publication_unix_seconds)?;
                        if *publication_unix_seconds
                            > request.observed_publication_cutoff_unix_seconds
                        {
                            return Err("observed fixing unavailable at valuation cutoff".into());
                        }
                        if utc_day(*publication_unix_seconds) < o.fixing_ordinal {
                            return Err("fixing publication precedes value date".into());
                        }
                        *publication_unix_seconds
                    }
                    RateSource::ConditionalProjection {
                        model,
                        path,
                        information_unix_seconds,
                        measure,
                    } => {
                        identity(model, bytes)?;
                        identity(path, bytes)?;
                        timestamp(*information_unix_seconds)?;
                        if path != &request.path
                            || *measure != request.measure
                            || request.measure == CashflowMeasure::Contractual
                        {
                            return Err("projected rate path/measure identity mismatch".into());
                        }
                        *information_unix_seconds
                    }
                };
                if availability > o.selection_cutoff_unix_seconds {
                    return Err(
                        "rate source unavailable at exact observation selection cutoff".into(),
                    );
                }
                if term {
                    if availability > reset_cutoff_unix_seconds.unwrap() {
                        return Err("same-day or future reset lookahead".into());
                    }
                    if o.fixing_ordinal > utc_day(reset_cutoff_unix_seconds.unwrap()) {
                        return Err("fixing value date follows reset".into());
                    }
                } else if matches!(o.source, RateSource::ConditionalProjection { .. })
                    && utc_day(o.selection_cutoff_unix_seconds) > o.observation_start_ordinal
                {
                    return Err("overnight projection uses future information".into());
                }
                let selected = bounded(*index_bounds, o.rate)?;
                let raw_all_in = selected + margin;
                if !raw_all_in.is_finite() {
                    return Err("index plus margin overflow".into());
                }
                let all_in = bounded(*all_in_bounds, raw_all_in)?;
                if !all_in.is_finite() {
                    return Err("index plus margin overflow".into());
                }
                let interest_tau = actual_year_fraction(
                    o.interest_start_ordinal,
                    o.interest_end_ordinal,
                    p.day_count,
                )?;
                let weight = if *observation_shift {
                    actual_year_fraction(
                        o.observation_start_ordinal,
                        o.observation_end_ordinal,
                        p.day_count,
                    )?
                } else {
                    interest_tau
                };
                let before_observation = factor_state;
                if *calculation == Calculation::DailyCompounded {
                    let rate = if *margin_treatment == MarginTreatment::Compounded {
                        all_in
                    } else {
                        all_in - margin
                    };
                    let increment = rate * weight;
                    if !increment.is_finite() || increment <= -1. {
                        return Err("nonpositive compounded index factor".into());
                    }
                    factor_state.compounded_observation(increment, interest_tau);
                } else {
                    factor_state.simple_observation(all_in, *margin, weight, interest_tau);
                }
                cursor = o.interest_end_ordinal;
                prior_obs_end = Some(o.observation_end_ordinal);
                sources.push(o.source.clone());
                if let Some(sink) = segments.as_deref_mut() {
                    portfolio_compute_control::checkpoint()?;
                    let full_factor = if *calculation == Calculation::DailyCompounded {
                        factor_state.compounded_factor(if *margin_treatment == MarginTreatment::Simple {
                            Some(*margin)
                        } else { None })
                    } else { factor_state.simple_factor() };
                    sink(FactorSegment::indexed(&p.period_id, o.interest_start_ordinal,
                        o.interest_end_ordinal, o.observation_start_ordinal,
                        o.observation_end_ordinal, p.day_count, *observation_shift,
                        before_observation, *calculation, *margin_treatment,
                        all_in, *margin, full_factor, &o.source))?;
                }
                if let Some(sink) = capture.as_deref_mut() {
                    portfolio_compute_control::checkpoint()?;
                    let full_factor = if *calculation == Calculation::DailyCompounded {
                        factor_state.compounded_factor(
                            if *margin_treatment == MarginTreatment::Simple {
                                Some(*margin)
                            } else {
                                None
                            },
                        )
                    } else {
                        factor_state.simple_factor()
                    };
                    let first = if daily_fragments {
                        o.interest_start_ordinal + 1
                    } else {
                        o.interest_end_ordinal
                    };
                    for day in first..=o.interest_end_ordinal {
                        portfolio_compute_control::checkpoint()?;
                        let cumulative_factor = if day == o.interest_end_ordinal {
                            full_factor
                        } else {
                            let partial_tau =
                                actual_year_fraction(o.interest_start_ordinal, day, p.day_count)?;
                            let partial_weight = if *observation_shift {
                                actual_year_fraction(
                                    o.observation_start_ordinal,
                                    o.observation_start_ordinal + (day - o.interest_start_ordinal),
                                    p.day_count,
                                )?
                            } else {
                                partial_tau
                            };
                            let mut fragment = before_observation;
                            if *calculation == Calculation::DailyCompounded {
                                let rate = if *margin_treatment == MarginTreatment::Compounded {
                                    all_in
                                } else {
                                    all_in - margin
                                };
                                fragment.compounded_observation(rate * partial_weight, partial_tau);
                                fragment.compounded_factor(
                                    if *margin_treatment == MarginTreatment::Simple {
                                        Some(*margin)
                                    } else {
                                        None
                                    },
                                )
                            } else {
                                fragment.simple_observation(
                                    all_in,
                                    *margin,
                                    partial_weight,
                                    partial_tau,
                                );
                                fragment.simple_factor()
                            }
                        };
                        if !cumulative_factor.is_finite() {
                            return Err("nonfinite provisional native earned factor".into());
                        }
                        sink(EarnedFactorPoint {
                            period_id: p.period_id.clone(),
                            ordinal: day,
                            cumulative_factor,
                            source: Some(o.source.clone()),
                        })?;
                    }
                }
            }
            if cursor != p.end_ordinal {
                return Err("incomplete index period coverage".into());
            }
            let factor = if *calculation == Calculation::DailyCompounded {
                factor_state.compounded_factor(if *margin_treatment == MarginTreatment::Simple {
                    Some(*margin)
                } else {
                    None
                })
            } else {
                factor_state.simple_factor()
            };
            if !factor.is_finite() || factor <= -1. {
                return Err("invalid complete coupon factor".into());
            }
            Ok((factor, sources, false))
        }
    }
}

pub fn build_path(request: &ContractPathRequest) -> Result<ContractPathResult, String> {
    build_path_owned(request, None, false, None)
}

/// Opt-in factor-only actual-loop descriptors, provisional until complete Ok.
/// The owning compiler admits and reserves descriptor capacity before this call.
pub(crate) fn build_path_with_segments(request: &ContractPathRequest,
    sink: &mut SegmentSink<'_>) -> Result<ContractPathResult, String> {
    build_path_owned(request, None, false, Some(sink))
}

/// Observation-boundary receipts; no interpolation, fixing substitution or cash generation.
pub fn build_path_with_capture(
    request: &ContractPathRequest,
    sink: &mut EarnedFactorSink<'_>,
) -> Result<ContractPathResult, String> {
    build_path_capture_owned(request, sink, false)
}

/// Explicit actual-day recognition fragments; no redistribution of final coupon.
/// Shifted interior dates require a one-to-one supplied calendar-day mapping.
pub fn build_path_with_daily_capture(
    request: &ContractPathRequest,
    review: &Identity,
    sink: &mut EarnedFactorSink<'_>,
) -> Result<ContractPathResult, String> {
    let mut bytes = 0;
    identity(review, &mut bytes)?;
    build_path_capture_owned(request, sink, true)
}

fn build_path_capture_owned(
    request: &ContractPathRequest,
    sink: &mut EarnedFactorSink<'_>,
    daily_fragments: bool,
) -> Result<ContractPathResult, String> {
    // Charge optional capture separately before any source projection. This is
    // a component output bound, not aggregate process RSS or policy approval.
    portfolio_compute_control::checkpoint()?;
    let mut points = 0usize;
    let mut bytes = 0usize;
    let mut metadata = 0usize;
    for p in &request.periods {
        portfolio_compute_control::checkpoint()?;
        text(&p.period_id, &mut metadata)?;
        let point_bytes = 320usize
            .checked_add(
                p.period_id
                    .len()
                    .checked_mul(6)
                    .ok_or("capture byte overflow")?,
            )
            .ok_or("capture byte overflow")?;
        match &p.coupon {
            Coupon::Fixed { .. } => {
                let count = if daily_fragments {
                    usize::try_from(
                        p.end_ordinal
                            .checked_sub(p.start_ordinal)
                            .ok_or("daily source interval overflow")?,
                    )
                    .map_err(|_| "daily source interval reversed")?
                } else {
                    1
                };
                points = points.checked_add(count).ok_or("capture point overflow")?;
                bytes = bytes
                    .checked_add(
                        point_bytes
                            .checked_mul(count)
                            .ok_or("capture byte overflow")?,
                    )
                    .ok_or("capture byte overflow")?;
            }
            Coupon::Indexed {
                observations,
                observation_shift,
                ..
            } => {
                for o in observations {
                    portfolio_compute_control::checkpoint()?;
                    if daily_fragments
                        && *observation_shift
                        && o.interest_end_ordinal
                            .checked_sub(o.interest_start_ordinal)
                            .ok_or("daily source interval overflow")?
                            != o.observation_end_ordinal
                                .checked_sub(o.observation_start_ordinal)
                                .ok_or("daily source interval overflow")?
                    {
                        return Err("daily shifted source recognition needs an explicit matched calendar-day mapping".into());
                    }
                    match &o.source {
                        RateSource::Observed { fixing, .. } => identity(fixing, &mut metadata)?,
                        RateSource::ConditionalProjection { model, path, .. } => {
                            identity(model, &mut metadata)?;
                            identity(path, &mut metadata)?;
                        }
                    }
                    let count = if daily_fragments {
                        usize::try_from(
                            o.interest_end_ordinal
                                .checked_sub(o.interest_start_ordinal)
                                .ok_or("daily source interval overflow")?,
                        )
                        .map_err(|_| "daily source interval reversed")?
                    } else {
                        1
                    };
                    points = points.checked_add(count).ok_or("capture point overflow")?;
                    let raw = serde_json::to_vec(&o.source).map_err(|e| e.to_string())?;
                    bytes = bytes
                        .checked_add(
                            point_bytes
                                .checked_mul(count)
                                .ok_or("capture byte overflow")?,
                        )
                        .and_then(|n| n.checked_add(raw.len().checked_mul(6)?.checked_mul(count)?))
                        .ok_or("capture byte overflow")?;
                    if points > 250_000 || bytes > 32 * 1024 * 1024 {
                        return Err(
                            "native earned-factor capture exceeds component work/byte admission"
                                .into(),
                        );
                    }
                }
            }
        }
        if points > 250_000 || bytes > 32 * 1024 * 1024 {
            return Err(
                "native earned-factor capture exceeds component work/byte admission".into(),
            );
        }
    }
    build_path_owned(request, Some(sink), daily_fragments, None)
}

fn build_path_owned(
    request: &ContractPathRequest,
    mut capture: Option<&mut EarnedFactorSink<'_>>,
    daily_fragments: bool,
    mut segments: Option<&mut SegmentSink<'_>>,
) -> Result<ContractPathResult, String> {
    header(
        &request.schema,
        &request.model_version,
        &request.currency,
        &request.monetary_unit,
    )?;
    domain(request.asof_ordinal, request.tail_end_ordinal)?;
    domain(request.asof_ordinal, request.reporting_end_ordinal)?;
    timestamp(request.observed_publication_cutoff_unix_seconds)?;
    if utc_day(request.observed_publication_cutoff_unix_seconds) > request.asof_ordinal {
        return Err("observed snapshot cutoff follows contractual valuation date".into());
    }
    if request.reporting_end_ordinal > request.tail_end_ordinal
        || request.periods.is_empty()
        || request.periods.len() > MAX_PERIODS
    {
        return Err("contract requires complete bounded tail and 1..4096 periods".into());
    }
    let mut bytes = 0;
    for id in [&request.contract, &request.assumption, &request.path] {
        identity(id, &mut bytes)?;
    }
    text(&request.position_id, &mut bytes)?;
    text(&request.scenario_id, &mut bytes)?;
    nonnegative(request.opening_principal)?;
    match &request.principal_plan {
        PrincipalPlan::Supplied { principal_payments } => {
            if principal_payments.len() != request.periods.len() {
                return Err("supplied principal must match all periods".into());
            }
            let mut sum = 0.;
            for &v in principal_payments {
                sum = amount(sum + nonnegative(v)?)?;
            }
            if !near(sum, request.opening_principal) {
                return Err("supplied principal does not conserve complete balance".into());
            }
        }
        PrincipalPlan::RetainedPayment {
            original_payment,
            terminal,
        } => {
            nonnegative(*original_payment)?;
            if let TerminalPrincipal::Balloon { amount: v } = terminal {
                nonnegative(*v)?;
            }
        }
    }
    let sign = if request.direction == Direction::Receipt {
        1.
    } else {
        -1.
    };
    let mut balance = request.opening_principal;
    let mut audit = Vec::with_capacity(request.periods.len());
    let mut events = Vec::with_capacity(request.periods.len());
    let mut prior_end = None;
    let mut ids = HashSet::new();
    let mut work = 0;
    let mut after_report = request.opening_principal;
    for (i, p) in request.periods.iter().enumerate() {
        text(&p.period_id, &mut bytes)?;
        domain(p.start_ordinal, p.end_ordinal)?;
        domain(request.asof_ordinal, p.payment_ordinal)?;
        if !ids.insert(&p.period_id)
            || p.start_ordinal == p.end_ordinal
            || prior_end.is_some_and(|end| end != p.start_ordinal)
            || p.end_ordinal > request.tail_end_ordinal
            || p.payment_ordinal > request.tail_end_ordinal
            || p.start_ordinal < request.asof_ordinal - 36_600
        {
            return Err(
                "invalid actual-date period, duplicate identity or incomplete tail domain".into(),
            );
        }
        prior_end = Some(p.end_ordinal);
        amount(p.fee)?;
        let (factor, sources, verified) = coupon_factor(
            request,
            p,
            &mut bytes,
            &mut work,
            &mut capture,
            daily_fragments,
            &mut segments,
        )?;
        let interest = amount(balance * factor)?;
        let last = i + 1 == request.periods.len();
        let mut balloon = 0.;
        let principal = match &request.principal_plan {
            PrincipalPlan::Supplied { principal_payments } => principal_payments[i],
            PrincipalPlan::RetainedPayment {
                original_payment,
                terminal,
            } => {
                let regular = amount(original_payment - interest)?;
                if regular < 0. {
                    return Err("retained payment causes unsupported negative amortization".into());
                }
                if last {
                    balloon = (balance - regular).max(0.);
                    if let TerminalPrincipal::Balloon { amount: v } = terminal {
                        if !near(balloon, *v) {
                            return Err(
                                "terminal balloon differs from retained-payment recurrence".into(),
                            );
                        }
                    }
                    balance
                } else {
                    regular
                }
            }
        };
        if principal > balance && !near(principal, balance) {
            return Err("principal payment exceeds remaining contractual balance".into());
        }
        let principal = principal.min(balance);
        let closing = balance - principal;
        if p.payment_ordinal <= request.reporting_end_ordinal {
            after_report -= principal;
        }
        events.push(DatedCashflowEvent {
            position_id: request.position_id.clone(),
            event_id: p.period_id.clone(),
            scenario: ScenarioSelector::Named {
                scenario_id: request.scenario_id.clone(),
            },
            payment_ordinal: p.payment_ordinal,
            day_offset: (p.payment_ordinal - request.asof_ordinal) as u32,
            accrual_period: Some(AccrualPeriod {
                start_ordinal: p.start_ordinal,
                end_ordinal: p.end_ordinal,
            }),
            currency: request.currency.clone(),
            components: CashflowComponents {
                principal: sign * principal,
                cash_interest: sign * interest,
                accrued_interest: sign * interest,
                book_amortization: 0.,
                fees: sign * p.fee,
            },
            measure: request.measure,
            provenance: EventProvenance::Modeled {
                model_id: request.assumption.id.clone(),
                model_version: MODEL_VERSION.into(),
                input_revision: request.assumption.revision.clone(),
            },
            adjustment: CashflowAdjustment::None {},
        });
        audit.push(PeriodAudit {
            period_id: p.period_id.clone(),
            opening_principal: balance,
            interest,
            principal_paid: principal,
            closing_principal: closing,
            terminal_balloon: balloon,
            calendar_mapping_verified: verified,
            selected_rate_sources: sources,
        });
        balance = closing;
    }
    if !near(balance, 0.) {
        return Err("complete contractual tail leaves unpaid principal".into());
    }
    events.sort_by_key(|e| e.payment_ordinal);
    let schedule = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: request.asof_ordinal,
        horizon_days: (request.tail_end_ordinal - request.asof_ordinal) as u32,
        scenarios: vec![request.scenario_id.clone()],
        events,
    };
    schedule.validate()?;
    Ok(ContractPathResult{schema:SCHEMA.into(),model_version:MODEL_VERSION.into(),contract:request.contract.clone(),assumption:request.assumption.clone(),path:request.path.clone(),schedule,audit,principal_after_reporting:after_report.max(0.),disclosure:"Explicit supplied conditional index path, not calibrated projection. Calendar identities/lookbacks and interval coverage retained; business-day calendar mapping is supplied and not independently verified. No default, prepayment, option exercise, OAS solve or book amortization. Complete maturity tail retained beyond report horizon.".into()})
}
