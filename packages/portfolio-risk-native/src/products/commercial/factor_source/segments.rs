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
    observable_information: Option<i64>,
    observable_information_year: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observable_information_unix_seconds: Option<i64>,
}

/// Sealed information-contingent service obligation. It deliberately contains
/// no guessed monetary amount and cannot deserialize caller state. A terminal
/// transfer needs a source-owning continuation, not a waiver of this memo.
#[derive(Debug, Serialize)]
pub struct PendingCouponObligation<'a> {
    pub period_id: &'a str,
    pub native_audit_index: usize,
    pub interest_start_ordinal: i32,
    pub interest_end_ordinal: i32,
    pub served_through_ordinal: i32,
    pub served_fraction_of_day: f64,
    pub information_unix_seconds: i64,
    pub source: &'a RateSource,
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
            observable_information: None,
            observable_information_year: None,
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
            observable_information: None,
            observable_information_year: None,
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
            observable_information_unix_seconds: self.observable_information,
        }
    }

    pub(crate) fn bind_observable_information(&mut self, information: i64, asof: i32) {
        self.observable_information = Some(information);
        self.observable_information_year =
            Some((information - (i64::from(asof) - 719163) * 86400) as f64 / 31_536_000.);
    }

    /// Re-evaluate one native observation from only information available now.
    /// Unknown index contributions are absent; a separately simple margin is
    /// already known and accrues on actual interest days. No final coupon is
    /// prorated. Partial weights concern this single, selected observation.
    pub(crate) fn observe(
        &self,
        state: &mut CouponFactorState,
        ordinal: i32,
        fraction: f64,
        information_year: f64,
    ) -> Result<bool, String> {
        portfolio_compute_control::checkpoint()?;
        let (day, part) = if ordinal >= self.end {
            (self.end, 0.)
        } else if ordinal < self.start {
            (self.start, 0.)
        } else {
            (ordinal, fraction)
        };
        let tau = partial_tau(self.start, day, part, self.day_count)?;
        let known = self
            .observable_information_year
            .is_none_or(|t| information_year >= t);
        match self.kind {
            Kind::Fixed { annual_rate } => {
                state.simple_observation(annual_rate, 0., tau, tau);
            }
            Kind::Indexed {
                calculation,
                margin_treatment,
                all_in,
                margin,
                ..
            } => {
                let weight = if self.shifted {
                    let full_interest = actual_year_fraction(self.start, self.end, self.day_count)?;
                    let full_observation = actual_year_fraction(
                        self.observation_start,
                        self.observation_end,
                        self.day_count,
                    )?;
                    if day == self.end {
                        full_observation
                    } else if self.end - self.start == self.observation_end - self.observation_start
                    {
                        partial_tau(
                            self.observation_start,
                            self.observation_start + day - self.start,
                            part,
                            self.day_count,
                        )?
                    } else {
                        full_observation * (tau / full_interest)
                    }
                } else {
                    tau
                };
                if calculation == Calculation::DailyCompounded {
                    let rate = if !known {
                        0.
                    } else if margin_treatment == MarginTreatment::Compounded {
                        all_in
                    } else {
                        all_in - margin
                    };
                    let increment = rate * weight;
                    if !increment.is_finite() || increment <= -1. {
                        return Err("nonpositive observable native observation factor".into());
                    }
                    state.compounded_observation(increment, tau);
                } else {
                    // Keep unknown simple margin separate from the index.
                    state.simple_observation(
                        if known { all_in } else { margin },
                        margin,
                        weight,
                        tau,
                    );
                }
            }
        }
        Ok(known)
    }

    pub(crate) fn observed_factor(&self, state: CouponFactorState) -> f64 {
        match self.kind {
            Kind::Fixed { .. } => state.simple_factor(),
            Kind::Indexed {
                calculation: Calculation::DailyCompounded,
                margin_treatment,
                margin,
                ..
            } => state.compounded_factor(if margin_treatment == MarginTreatment::Simple {
                Some(margin)
            } else {
                None
            }),
            Kind::Indexed { .. } => state.simple_factor(),
        }
    }

    /// Pending legal service is a sealed native obligation, never a zero-valued
    /// waiver: the caller must not terminate ownership before its fixing arrives.
    pub(crate) fn pending_at(&self, ordinal: i32, fraction: f64, information: f64) -> bool {
        matches!(self.kind, Kind::Indexed { .. })
            && (ordinal > self.start || ordinal == self.start && fraction > 0.)
            && self
                .observable_information_year
                .is_some_and(|t| information < t)
    }

    pub(crate) fn pending_obligation(
        &self,
        ordinal: i32,
        fraction: f64,
        year: f64,
    ) -> Option<PendingCouponObligation<'_>> {
        if !self.pending_at(ordinal, fraction, year) {
            return None;
        }
        Some(PendingCouponObligation {
            period_id: &self.period_id,
            native_audit_index: self.audit_index,
            interest_start_ordinal: self.start,
            interest_end_ordinal: self.end,
            served_through_ordinal: ordinal.min(self.end),
            served_fraction_of_day: if ordinal < self.end { fraction } else { 0. },
            information_unix_seconds: self.observable_information?,
            source: self.source.as_ref()?,
        })
    }

    pub(crate) fn information_unix_seconds(&self) -> Option<i64> {
        self.observable_information
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
pub struct TerminalObligationPolicy {
    pub review: Identity,
    pub live_exit_recognition: String,
    pub live_exit_settlement: String,
    pub default_recognition: String,
    pub default_recovery: String,
}
impl TerminalObligationPolicy {
    fn validate(&self) -> Result<(), String> {
        let mut bytes = 0;
        crate::products::term::contracts::identity(&self.review, &mut bytes)?;
        if self.live_exit_recognition != "recognize_known_served_right_at_information-1"
            || self.live_exit_settlement != "retain_original_coupon_settlement_after_information-1"
            || self.default_recognition != "new_known_default_right_is_legal_only-1"
            || self.default_recovery
                != "original_signed_interest_fraction_and_tails_after_information-1"
        {
            return Err("explicit reviewed contingent coupon recognition/settlement/default recovery policy required".into());
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_obligations: Option<TerminalObligationPolicy>,
}

impl FactorPolicy {
    pub fn observable(&self) -> bool {
        self.overnight == "observable_index_prefix_simple_margin-1"
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut bytes = 0;
        crate::products::term::contracts::identity(&self.review, &mut bytes)?;
        if let Some(policy) = &self.terminal_obligations {
            if !self.observable() {
                return Err(
                    "contingent terminal obligations require observable source selection".into(),
                );
            }
            policy.validate()?;
        }
        if self.clock != "credit_act365_elapsed_from_source_asof-1"
            || self.partial_calendar_day != "simple_partial_native_daycount-1"
            || !(self.overnight == "pre_observation_accumulator_partial_weight-1"
                && self.observation_mapping == "explicit_equal_calendar_span-1"
                || self.observable()
                    && self.observation_mapping
                        == "selected_observation_weight_over_interest_daycount-1")
            || self.full_checkpoints != "retain_actual_native_endpoints-1"
        {
            return Err("explicit reviewed native continuous factor policy required".into());
        }
        Ok(())
    }
}
