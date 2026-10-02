//! Supplied-fixing contractual coupon arithmetic. No rate projection or fallback.
use crate::cashflow::{actual_year_fraction, validate_ordinal, ActualDayCount, MAX_HORIZON_DAYS};
use crate::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "floating-rate-1";
pub const MAX_PERIODS: usize = 4096;
pub const MAX_INTERVALS: usize = 250_000;
pub const MAX_FIXINGS: usize = 250_000;
pub const MAX_PUBLICATION_TIMESTAMP: i64 = 253_402_300_799;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Calculation {
    TermSimple,
    DailySimple,
    DailyCompounded,
    /// Reserved explicitly so unsupported capitalization cannot be mislabeled.
    BalanceCompounded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarginTreatment {
    Simple,
    Compounded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Receipt,
    Payment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexKind {
    Term,
    Overnight,
    PublishedAverage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDescriptor {
    pub index_id: String,
    pub kind: IndexKind,
    pub tenor_months: Option<u32>,
    pub average_calendar_days: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionIdentity {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedFixing {
    pub fixing_id: String,
    pub revision: String,
    pub index_id: String,
    pub value_ordinal: i32,
    pub publication_unix_seconds: i64,
    /// Decimal annual rate, not percent or basis points.
    pub rate: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateBounds {
    pub floor: Option<f64>,
    pub cap: Option<f64>,
}
impl RateBounds {
    fn validate(&self) -> Result<(), String> {
        if self
            .floor
            .into_iter()
            .chain(self.cap)
            .any(|x| !x.is_finite())
            || matches!((self.floor, self.cap), (Some(lo), Some(hi)) if lo > hi)
        {
            return Err("rate bounds must be finite and floor must not exceed cap".into());
        }
        Ok(())
    }
    fn apply(&self, rate: f64) -> f64 {
        rate.max(self.floor.unwrap_or(f64::NEG_INFINITY))
            .min(self.cap.unwrap_or(f64::INFINITY))
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalInterval {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    /// Actual currency units; nonnegative outstanding contractual principal.
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationInterval {
    pub interest_start_ordinal: i32,
    pub interest_end_ordinal: i32,
    pub observation_start_ordinal: i32,
    pub observation_end_ordinal: i32,
    pub fixing_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloatingPeriod {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    /// Mandatory for term_simple; None for overnight. Exact UTC availability
    /// cutoff for the selected in-advance fixing, independent of valuation time.
    pub reset_cutoff_unix_seconds: Option<i64>,
    pub calculation: Calculation,
    pub margin_treatment: MarginTreatment,
    pub day_count: ActualDayCount,
    pub margin: f64,
    pub index_bounds: RateBounds,
    /// Bounds on selected annual index plus margin, applied per observation.
    /// These are not bounds on the final compounded effective period rate.
    pub all_in_bounds: RateBounds,
    pub lookback_business_days: u32,
    pub observation_shift: bool,
    pub principal: Vec<PrincipalInterval>,
    pub observations: Vec<ObservationInterval>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloatingRateRequest {
    pub schema: String,
    pub position_id: String,
    pub currency: String,
    /// Must be "currency_units"; no implicit thousands/millions conversion.
    pub monetary_unit: String,
    pub direction: Direction,
    pub index: IndexDescriptor,
    pub reference_calendar: RevisionIdentity,
    pub fixing_snapshot: RevisionIdentity,
    pub publication_cutoff_unix_seconds: i64,
    /// Caller asserts supplied observation mapping is complete for its calendar.
    /// The arithmetic independently checks actual interval coverage.
    pub complete_observation_coverage: bool,
    pub fixings: Vec<SuppliedFixing>,
    pub periods: Vec<FloatingPeriod>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodCoupon {
    pub period_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub payment_ordinal: i32,
    pub interest_amount: f64,
    pub effective_annual_rate: Option<f64>,
    /// Retain precisely the immutable selected fixing revisions, in interval order.
    pub fixing_references: Vec<RevisionIdentity>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingRateResult {
    pub schema: String,
    pub position_id: String,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub index: IndexDescriptor,
    pub reference_calendar: RevisionIdentity,
    pub fixing_snapshot: RevisionIdentity,
    pub publication_cutoff_unix_seconds: i64,
    pub periods: Vec<PeriodCoupon>,
    /// This arithmetic verifies supplied date coverage, not the calendar's
    /// business-day contents or the requested business-day lookback mapping.
    pub calendar_mapping_verified: bool,
}

fn text(value: &str, bytes: &mut usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(
            "floating metadata must be nonempty, control-free and at most 1024 bytes".into(),
        );
    }
    *bytes = bytes
        .checked_add(value.len())
        .ok_or("floating metadata overflow")?;
    if *bytes > MAX_DATED_METADATA_BYTES {
        return Err("floating metadata exceeds 32 MiB admission".into());
    }
    Ok(())
}
fn interval(start: i32, end: i32) -> Result<(), String> {
    validate_ordinal(start)?;
    validate_ordinal(end)?;
    if end <= start || (end - start) as u32 > MAX_HORIZON_DAYS {
        return Err("floating intervals must be positive and at most 36600 days".into());
    }
    Ok(())
}
fn admit_amount(amount: f64) -> Result<f64, String> {
    if !amount.is_finite() || amount.abs() > MAX_MONETARY_AMOUNT {
        return Err("floating interest exceeds finite monetary admission".into());
    }
    Ok(amount)
}

pub fn evaluate(request: &FloatingRateRequest) -> Result<FloatingRateResult, String> {
    if request.schema != SCHEMA
        || !request.complete_observation_coverage
        || request.monetary_unit != "currency_units"
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || request.periods.is_empty()
        || request.periods.len() > MAX_PERIODS
        || request.fixings.is_empty()
        || request.fixings.len() > MAX_FIXINGS
        || !(0..=MAX_PUBLICATION_TIMESTAMP).contains(&request.publication_cutoff_unix_seconds)
    {
        return Err("invalid floating schema, unit, coverage, cutoff or admission".into());
    }
    let mut bytes = 0;
    for value in [
        &request.position_id,
        &request.index.index_id,
        &request.reference_calendar.id,
        &request.reference_calendar.revision,
        &request.fixing_snapshot.id,
        &request.fixing_snapshot.revision,
    ] {
        text(value, &mut bytes)?;
    }
    let valid_index = match request.index.kind {
        IndexKind::Term => {
            matches!(request.index.tenor_months, Some(1..=1200))
                && request.index.average_calendar_days.is_none()
        }
        IndexKind::Overnight => {
            request.index.tenor_months.is_none() && request.index.average_calendar_days.is_none()
        }
        IndexKind::PublishedAverage => {
            matches!(request.index.average_calendar_days, Some(1..=36600))
                && request.index.tenor_months.is_none()
        }
    };
    if !valid_index {
        return Err("index kind requires an explicit compatible tenor or averaging window".into());
    }
    let cutoff_ordinal = 719_163 + (request.publication_cutoff_unix_seconds / 86_400) as i32;
    let mut fixings = HashMap::with_capacity(request.fixings.len());
    for fixing in &request.fixings {
        for value in [&fixing.fixing_id, &fixing.revision, &fixing.index_id] {
            text(value, &mut bytes)?;
        }
        validate_ordinal(fixing.value_ordinal)?;
        if !fixing.rate.is_finite()
            || fixing.index_id != request.index.index_id
            || !(0..=request.publication_cutoff_unix_seconds)
                .contains(&fixing.publication_unix_seconds)
            || fixing.value_ordinal > cutoff_ordinal
            || 719_163 + ((fixing.publication_unix_seconds / 86_400) as i32) < fixing.value_ordinal
            || fixings.insert(fixing.fixing_id.as_str(), fixing).is_some()
        {
            return Err(
                "invalid, unavailable or duplicate supplied fixing; no inferred fallback".into(),
            );
        }
    }
    let mut interval_count = 0usize;
    let mut ids = HashSet::new();
    let mut prior_end = None;
    // Complete structural validation precedes allocation/calculation of results.
    for period in &request.periods {
        text(&period.period_id, &mut bytes)?;
        interval(period.start_ordinal, period.end_ordinal)?;
        validate_ordinal(period.payment_ordinal)?;
        if !ids.insert(period.period_id.as_str())
            || prior_end.is_some_and(|end| period.start_ordinal < end)
            || (period.payment_ordinal - period.start_ordinal).unsigned_abs() > MAX_HORIZON_DAYS
            || !period.margin.is_finite()
            || period.lookback_business_days > 366
            || period.principal.is_empty()
            || period.observations.is_empty()
            || period.calculation == Calculation::BalanceCompounded
            || (period.calculation != Calculation::DailyCompounded
                && period.margin_treatment != MarginTreatment::Simple)
        {
            return Err("invalid floating period or unsupported compounding convention".into());
        }
        prior_end = Some(period.end_ordinal);
        interval_count = interval_count
            .checked_add(period.principal.len())
            .and_then(|n| n.checked_add(period.observations.len()))
            .ok_or("floating interval work overflow")?;
        if interval_count > MAX_INTERVALS {
            return Err("floating intervals exceed 250000 work admission".into());
        }
        period.index_bounds.validate()?;
        period.all_in_bounds.validate()?;
        let constant = period
            .principal
            .iter()
            .all(|p| p.amount == period.principal[0].amount);
        if !constant
            && (period.calculation == Calculation::DailyCompounded || period.observation_shift)
        {
            return Err(
                "rate compounding or observation shift requires constant period principal".into(),
            );
        }
        if period.calculation == Calculation::TermSimple {
            if request.index.kind == IndexKind::Overnight
                || period.observations.len() != 1
                || period.observation_shift
            {
                return Err(
                    "term_simple requires one in-advance term/average fixing without shift".into(),
                );
            }
            let reset = period
                .reset_cutoff_unix_seconds
                .ok_or("term_simple requires an exact reset cutoff")?;
            if !(0..=request.publication_cutoff_unix_seconds).contains(&reset)
                || 719_163 + (reset / 86_400) as i32 > period.start_ordinal
            {
                return Err(
                    "term reset cutoff must precede period start and valuation cutoff".into(),
                );
            }
        } else if request.index.kind != IndexKind::Overnight {
            return Err("daily calculations require an explicit overnight index".into());
        } else if period.reset_cutoff_unix_seconds.is_some() {
            return Err(
                "overnight observations use publication availability, not a term reset cutoff"
                    .into(),
            );
        }
        let mut cursor = period.start_ordinal;
        for principal in &period.principal {
            interval(principal.start_ordinal, principal.end_ordinal)?;
            if principal.start_ordinal != cursor
                || principal.end_ordinal > period.end_ordinal
                || !principal.amount.is_finite()
                || !(0.0..=MAX_MONETARY_AMOUNT).contains(&principal.amount)
            {
                return Err(
                    "principal intervals must partition the period in finite currency units".into(),
                );
            }
            cursor = principal.end_ordinal;
        }
        if cursor != period.end_ordinal {
            return Err("principal intervals do not cover the interest period".into());
        }
        cursor = period.start_ordinal;
        let mut observation_end = None;
        for observation in &period.observations {
            text(&observation.fixing_id, &mut bytes)?;
            interval(
                observation.interest_start_ordinal,
                observation.interest_end_ordinal,
            )?;
            interval(
                observation.observation_start_ordinal,
                observation.observation_end_ordinal,
            )?;
            let fixing = fixings
                .get(observation.fixing_id.as_str())
                .ok_or("missing supplied fixing")?;
            if observation.interest_start_ordinal != cursor
                || observation.interest_end_ordinal > period.end_ordinal
                || observation.observation_start_ordinal > observation.interest_start_ordinal
                || fixing.value_ordinal > observation.interest_start_ordinal
                || (period.calculation == Calculation::TermSimple
                    && fixing.publication_unix_seconds > period.reset_cutoff_unix_seconds.unwrap())
                || (period.calculation != Calculation::TermSimple
                    && fixing.value_ordinal != observation.observation_start_ordinal)
                || (period.observation_shift
                    && observation_end
                        .is_some_and(|end| end != observation.observation_start_ordinal))
            {
                return Err("observation intervals must partition interest dates with compatible supplied fixing dates".into());
            }
            cursor = observation.interest_end_ordinal;
            observation_end = Some(observation.observation_end_ordinal);
        }
        if cursor != period.end_ordinal {
            return Err("observation intervals do not cover the interest period".into());
        }
    }
    let mut output = Vec::with_capacity(request.periods.len());
    for period in &request.periods {
        let mut amount = 0.;
        let mut log_factor = 0.;
        let mut simple_margin_tau = 0.;
        let mut principal_index = 0;
        let mut weighted_principal_tau = 0.;
        let mut references = Vec::with_capacity(period.observations.len());
        for observation in &period.observations {
            let fixing = fixings[observation.fixing_id.as_str()];
            references.push(RevisionIdentity {
                id: fixing.fixing_id.clone(),
                revision: fixing.revision.clone(),
            });
            let index_rate = period.index_bounds.apply(fixing.rate);
            let raw_all_in = index_rate + period.margin;
            if !raw_all_in.is_finite() {
                return Err("floating annual rate overflow".into());
            }
            let all_in_rate = period.all_in_bounds.apply(raw_all_in);
            let (weight_start, weight_end) = if period.observation_shift {
                (
                    observation.observation_start_ordinal,
                    observation.observation_end_ordinal,
                )
            } else {
                (
                    observation.interest_start_ordinal,
                    observation.interest_end_ordinal,
                )
            };
            let tau = actual_year_fraction(weight_start, weight_end, period.day_count)?;
            if period.calculation == Calculation::DailyCompounded {
                let compound_rate = if period.margin_treatment == MarginTreatment::Compounded {
                    all_in_rate
                } else {
                    all_in_rate - period.margin
                };
                let increment = compound_rate * tau;
                if !increment.is_finite() || increment <= -1. {
                    return Err(
                        "floating compound interval requires a positive finite factor".into(),
                    );
                }
                log_factor += increment.ln_1p();
                if period.margin_treatment == MarginTreatment::Simple {
                    simple_margin_tau += actual_year_fraction(
                        observation.interest_start_ordinal,
                        observation.interest_end_ordinal,
                        period.day_count,
                    )?;
                }
            } else if period.observation_shift {
                let interest_tau = actual_year_fraction(
                    observation.interest_start_ordinal,
                    observation.interest_end_ordinal,
                    period.day_count,
                )?;
                amount = admit_amount(
                    amount
                        + period.principal[0].amount
                            * ((all_in_rate - period.margin) * tau + period.margin * interest_tau),
                )?;
            } else {
                let mut cursor = observation.interest_start_ordinal;
                while cursor < observation.interest_end_ordinal {
                    let principal = &period.principal[principal_index];
                    let end = principal.end_ordinal.min(observation.interest_end_ordinal);
                    let elapsed = actual_year_fraction(cursor, end, period.day_count)?;
                    amount = admit_amount(amount + principal.amount * all_in_rate * elapsed)?;
                    cursor = end;
                    if cursor == principal.end_ordinal
                        && principal_index + 1 < period.principal.len()
                    {
                        principal_index += 1;
                    }
                }
            }
        }
        if period.calculation == Calculation::DailyCompounded {
            let interest_factor = log_factor.exp_m1() + period.margin * simple_margin_tau;
            amount = admit_amount(period.principal[0].amount * interest_factor)?;
        }
        for principal in &period.principal {
            weighted_principal_tau += principal.amount
                * actual_year_fraction(
                    principal.start_ordinal,
                    principal.end_ordinal,
                    period.day_count,
                )?;
        }
        let effective = if weighted_principal_tau > 0. {
            let value = amount / weighted_principal_tau;
            if !value.is_finite() {
                return Err("floating effective annual rate overflow".into());
            }
            Some(value)
        } else {
            None
        };
        if request.direction == Direction::Payment {
            amount = -amount;
        }
        output.push(PeriodCoupon {
            period_id: period.period_id.clone(),
            start_ordinal: period.start_ordinal,
            end_ordinal: period.end_ordinal,
            payment_ordinal: period.payment_ordinal,
            interest_amount: amount,
            effective_annual_rate: effective,
            fixing_references: references,
        });
    }
    Ok(FloatingRateResult {
        schema: SCHEMA.into(),
        position_id: request.position_id.clone(),
        currency: request.currency.clone(),
        monetary_unit: request.monetary_unit.clone(),
        direction: request.direction,
        index: request.index.clone(),
        reference_calendar: request.reference_calendar.clone(),
        fixing_snapshot: request.fixing_snapshot.clone(),
        publication_cutoff_unix_seconds: request.publication_cutoff_unix_seconds,
        periods: output,
        calendar_mapping_verified: false,
    })
}
