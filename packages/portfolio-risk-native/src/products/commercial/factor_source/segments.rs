//! Only the actual native coupon loop can construct these sealed descriptors.
use crate::products::term::contracts::{indexed::RateSource, Identity};
use portfolio_model_core::{
    cashflow::{actual_year_fraction, ActualDayCount},
    coupon_factor_state::CouponFactorState,
    floating_rate::{Calculation, MarginTreatment},
};
use serde::Serialize;

pub type SegmentSink<'a> = dyn FnMut(FactorSegment) -> Result<(), String> + 'a;

#[derive(Debug, Clone, Copy)]
enum Kind {
    Fixed {
        annual_rate: f64,
    },
    Indexed {
        before: CouponFactorState,
        calculation: Calculation,
        margin_treatment: MarginTreatment,
        all_in: f64,
        margin: f64,
    },
}

/// Has no Deserialize or public constructor; cannot be a caller checkpoint.
#[derive(Debug, Clone)]
pub struct FactorSegment {
    period_id: String,
    audit_index: usize,
    start: i32,
    end: i32,
    observation_start: i32,
    observation_end: i32,
    day_count: ActualDayCount,
    shifted: bool,
    full_factor: f64,
    source: Option<RateSource>,
    kind: Kind,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentEvidence<'a> {
    pub period_id: &'a str,
    pub native_source_event_id: &'a str,
    pub native_audit_index: usize,
    pub interest_start_ordinal: i32,
    pub interest_end_ordinal: i32,
    pub observation_start_ordinal: i32,
    pub observation_end_ordinal: i32,
    pub day_count: ActualDayCount,
    pub observation_shift: bool,
    pub full_factor: f64,
    pub source: &'a Option<RateSource>,
    pub native_factor_kind: &'static str,
    pub selected_all_in: Option<f64>,
    pub margin: Option<f64>,
    pub calculation: Option<Calculation>,
    pub margin_treatment: Option<MarginTreatment>,
    pub before_accumulator: Option<(f64, f64, f64)>,
    pub fixed_annual_rate: Option<f64>,
}

impl FactorSegment {
    pub(crate) fn fixed(
        period_id: &str,
        start: i32,
        end: i32,
        day_count: ActualDayCount,
        annual_rate: f64,
        full_factor: f64,
    ) -> Self {
        Self {
            period_id: period_id.into(),
            audit_index: 0,
            start,
            end,
            observation_start: start,
            observation_end: end,
            day_count,
            shifted: false,
            full_factor,
            source: None,
            kind: Kind::Fixed { annual_rate },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn indexed(
        period_id: &str,
        start: i32,
        end: i32,
        observation_start: i32,
        observation_end: i32,
        day_count: ActualDayCount,
        shifted: bool,
        before: CouponFactorState,
        calculation: Calculation,
        margin_treatment: MarginTreatment,
        all_in: f64,
        margin: f64,
        full_factor: f64,
        source: &RateSource,
    ) -> Self {
        Self {
            period_id: period_id.into(),
            audit_index: 0,
            start,
            end,
            observation_start,
            observation_end,
            day_count,
            shifted,
            full_factor,
            source: Some(source.clone()),
            kind: Kind::Indexed {
                before,
                calculation,
                margin_treatment,
                all_in,
                margin,
            },
        }
    }

    pub fn evidence(&self) -> SegmentEvidence<'_> {
        let (kind, all_in, margin, calculation, treatment, before, fixed) = match self.kind {
            Kind::Fixed { annual_rate } => {
                ("fixed", None, None, None, None, None, Some(annual_rate))
            }
            Kind::Indexed {
                before,
                calculation,
                margin_treatment,
                all_in,
                margin,
            } => (
                "indexed",
                Some(all_in),
                Some(margin),
                Some(calculation),
                Some(margin_treatment),
                Some(before.native_snapshot()),
                None,
            ),
        };
        SegmentEvidence {
            period_id: &self.period_id,
            native_source_event_id: &self.period_id,
            native_audit_index: self.audit_index,
            interest_start_ordinal: self.start,
            interest_end_ordinal: self.end,
            observation_start_ordinal: self.observation_start,
            observation_end_ordinal: self.observation_end,
            day_count: self.day_count,
            observation_shift: self.shifted,
            full_factor: self.full_factor,
            source: &self.source,
            native_factor_kind: kind,
            selected_all_in: all_in,
            margin,
            calculation,
            margin_treatment: treatment,
            before_accumulator: before,
            fixed_annual_rate: fixed,
        }
    }

    pub fn start_ordinal(&self) -> i32 {
        self.start
    }
    pub fn end_ordinal(&self) -> i32 {
        self.end
    }
    pub fn period_id(&self) -> &str {
        &self.period_id
    }
    pub fn audit_index(&self) -> usize {
        self.audit_index
    }
    pub(crate) fn bind_audit_index(&mut self, index: usize) {
        self.audit_index = index;
    }

    /// Explicit reviewed convention: simple fractional native calendar day.
    /// Full integer endpoint returns the actual loop's value, never a plug.
    pub(crate) fn cumulative(&self, ordinal: i32, fraction_of_day: f64) -> Result<f64, String> {
        portfolio_compute_control::checkpoint()?;
        if !fraction_of_day.is_finite()
            || !(0.0..1.0).contains(&fraction_of_day)
            || ordinal < self.start
            || ordinal > self.end
            || (ordinal == self.end && fraction_of_day != 0.)
        {
            return Err("query outside captured native segment/calendar domain".into());
        }
        if ordinal == self.end {
            return Ok(self.full_factor);
        }
        if self.shifted && self.end - self.start != self.observation_end - self.observation_start {
            return Err("continuous shifted factor requires explicit equal calendar spans".into());
        }
        let tau = partial_tau(self.start, ordinal, fraction_of_day, self.day_count)?;
        let weight = if self.shifted {
            partial_tau(
                self.observation_start,
                self.observation_start + (ordinal - self.start),
                fraction_of_day,
                self.day_count,
            )?
        } else {
            tau
        };
        let value = match self.kind {
            Kind::Fixed { annual_rate } => CouponFactorState::fixed_factor(annual_rate, tau),
            Kind::Indexed {
                before,
                calculation,
                margin_treatment,
                all_in,
                margin,
            } => {
                let mut state = before;
                if calculation == Calculation::DailyCompounded {
                    let rate = if margin_treatment == MarginTreatment::Compounded {
                        all_in
                    } else {
                        all_in - margin
                    };
                    let increment = rate * weight;
                    if !increment.is_finite() || increment <= -1. {
                        return Err("partial native observation factor is nonpositive".into());
                    }
                    state.compounded_observation(increment, tau);
                    state.compounded_factor(if margin_treatment == MarginTreatment::Simple {
                        Some(margin)
                    } else {
                        None
                    })
                } else {
                    state.simple_observation(all_in, margin, weight, tau);
                    state.simple_factor()
                }
            }
        };
        if !value.is_finite() {
            return Err("continuous factor overflow".into());
        }
        Ok(value)
    }
}

fn partial_tau(
    start: i32,
    ordinal: i32,
    fraction: f64,
    basis: ActualDayCount,
) -> Result<f64, String> {
    let complete = actual_year_fraction(start, ordinal, basis)?;
    if fraction == 0. {
        return Ok(complete);
    }
    let next = ordinal.checked_add(1).ok_or("partial calendar overflow")?;
    Ok(complete + fraction * actual_year_fraction(ordinal, next, basis)?)
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactorPolicy {
    pub review: Identity,
    pub clock: String,
    pub partial_calendar_day: String,
    pub overnight: String,
    pub observation_mapping: String,
    pub full_checkpoints: String,
}

impl FactorPolicy {
    pub fn validate(&self) -> Result<(), String> {
        let mut bytes = 0;
        crate::products::term::contracts::identity(&self.review, &mut bytes)?;
        if self.clock != "credit_act365_elapsed_from_source_asof-1"
            || self.partial_calendar_day != "simple_partial_native_daycount-1"
            || self.overnight != "pre_observation_accumulator_partial_weight-1"
            || self.observation_mapping != "explicit_equal_calendar_span-1"
            || self.full_checkpoints != "retain_actual_native_endpoints-1"
        {
            return Err("explicit reviewed native continuous factor policy required".into());
        }
        Ok(())
    }
}
