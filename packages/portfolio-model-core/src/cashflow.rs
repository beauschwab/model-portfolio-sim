//! Versioned actual-date events. This module does not generate behavioral paths,
//! discount cashflows, or reinterpret a calendar month as thirty days.

use crate::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const DATED_CASHFLOW_SCHEMA: &str = "dated-cashflow-1";
pub const MAX_EVENTS: usize = 250_000;
pub const MAX_POSITIONS: usize = 62_000;
pub const MAX_SCENARIOS: usize = 256;
pub const MAX_HORIZON_DAYS: u32 = 36_600;
pub const MAX_ORDINAL: i32 = 3_652_059;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScenarioSelector {
    All {},
    Named { scenario_id: String },
}

/// A declared interpretation, never an implicit conversion between measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashflowMeasure {
    Contractual,
    Pricing,
    Physical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventProvenance {
    Contractual {
        source_id: String,
        source_revision: String,
    },
    Modeled {
        model_id: String,
        model_version: String,
        input_revision: String,
    },
    Assumed {
        assumption_id: String,
        rationale: String,
    },
}

/// Cash amounts are positive for receipts, negative for payments, from the
/// schedule owner's perspective. Noncash amounts are signed recognition changes
/// in that same perspective, not an additional cash settlement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashflowComponents {
    pub principal: f64,
    pub cash_interest: f64,
    pub accrued_interest: f64,
    /// Signed change in carrying value from premium/discount/fee amortization.
    /// A downstream accounting policy must map it to the proper journal side.
    pub book_amortization: f64,
    pub fees: f64,
}

impl CashflowComponents {
    pub fn cash_total(&self) -> Result<f64, String> {
        self.validate()?;
        let value = self.principal + self.cash_interest + self.fees;
        if !value.is_finite() || value.abs() > MAX_MONETARY_AMOUNT {
            return Err("cash component sum exceeds monetary admission".into());
        }
        Ok(value)
    }

    fn validate(&self) -> Result<(), String> {
        if [
            self.principal,
            self.cash_interest,
            self.accrued_interest,
            self.book_amortization,
            self.fees,
        ]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT)
        {
            return Err("cashflow components exceed finite monetary admission".into());
        }
        // Also admit only amounts whose cash settlement can be represented.
        let total = self.principal + self.cash_interest + self.fees;
        if !total.is_finite() || total.abs() > MAX_MONETARY_AMOUNT {
            return Err("cash component sum exceeds monetary admission".into());
        }
        Ok(())
    }

    fn scaled(&self, factor: f64) -> Self {
        Self {
            principal: self.principal * factor,
            cash_interest: self.cash_interest * factor,
            accrued_interest: self.accrued_interest * factor,
            book_amortization: self.book_amortization * factor,
            fees: self.fees * factor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccrualPeriod {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CashflowAdjustment {
    None {},
    /// A disclosed overlay; never substitutes for joint default/prepay repricing.
    ProportionalSurvivingPrincipal {
        surviving_fraction: f64,
        rationale: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatedCashflowEvent {
    pub position_id: String,
    /// Stable identity of an economic event across scenario versions.
    pub event_id: String,
    pub scenario: ScenarioSelector,
    pub payment_ordinal: i32,
    pub day_offset: u32,
    pub accrual_period: Option<AccrualPeriod>,
    pub currency: String,
    pub components: CashflowComponents,
    pub measure: CashflowMeasure,
    pub provenance: EventProvenance,
    pub adjustment: CashflowAdjustment,
}

impl DatedCashflowEvent {
    /// Apply one explicitly requested proportional overlay. The factor-one path
    /// returns a byte-equivalent serialized event with no approximation marker.
    pub fn proportionally_adjust(
        &self,
        surviving_fraction: f64,
        rationale: &str,
    ) -> Result<Self, String> {
        self.components.validate()?;
        validate_adjustment(&self.adjustment)?;
        if !surviving_fraction.is_finite() || !(0.0..=1.0).contains(&surviving_fraction) {
            return Err("surviving_fraction must be finite and between zero and one".into());
        }
        if surviving_fraction == 1.0 {
            return Ok(self.clone());
        }
        text_field(rationale, "adjustment rationale", 1024)?;
        if !matches!(self.adjustment, CashflowAdjustment::None {}) {
            return Err(
                "an already adjusted event requires product regeneration, not another overlay"
                    .into(),
            );
        }
        let mut adjusted = self.clone();
        adjusted.components = self.components.scaled(surviving_fraction);
        adjusted.adjustment = CashflowAdjustment::ProportionalSurvivingPrincipal {
            surviving_fraction,
            rationale: rationale.into(),
        };
        Ok(adjusted)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatedSchedule {
    pub schema: String,
    /// Proleptic Gregorian ordinal: 0001-01-01 = 1.
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
    pub scenarios: Vec<String>,
    /// Nondecreasing payment dates. Same-day ordering is the supplied ordering.
    pub events: Vec<DatedCashflowEvent>,
}

impl DatedSchedule {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != DATED_CASHFLOW_SCHEMA {
            return Err("unsupported dated cashflow schema".into());
        }
        validate_ordinal(self.as_of_ordinal)?;
        if self.horizon_days > MAX_HORIZON_DAYS {
            return Err("dated schedule horizon exceeds admission limit".into());
        }
        let last = i64::from(self.as_of_ordinal) + i64::from(self.horizon_days);
        if last > i64::from(MAX_ORDINAL) {
            return Err("dated horizon exceeds Gregorian date domain".into());
        }
        if self.scenarios.is_empty() || self.scenarios.len() > MAX_SCENARIOS {
            return Err("dated schedule must declare 1..256 scenarios".into());
        }
        let mut scenarios = HashSet::new();
        let mut metadata_bytes = self.schema.len();
        for s in &self.scenarios {
            text_field(s, "scenario_id", 128)?;
            admit_metadata(&mut metadata_bytes, s.len())?;
            if !scenarios.insert(s.as_str()) {
                return Err("duplicate scenario_id".into());
            }
        }
        if self.events.len() > MAX_EVENTS {
            return Err("dated schedule event limit exceeded".into());
        }
        let mut positions = HashSet::new();
        // Store identities once, without expanding an All event into 256 rows.
        let mut identities: HashMap<(&str, &str), (bool, HashSet<&str>)> = HashMap::new();
        let mut last_offset = 0;
        for event in &self.events {
            text_field(&event.position_id, "position_id", 256)?;
            text_field(&event.event_id, "event_id", 256)?;
            positions.insert(event.position_id.as_str());
            if positions.len() > MAX_POSITIONS {
                return Err("dated schedule position limit exceeded".into());
            }
            validate_ordinal(event.payment_ordinal)?;
            if event.day_offset > self.horizon_days || event.day_offset < last_offset {
                return Err("events must be chronological and inside the horizon".into());
            }
            if i64::from(event.payment_ordinal)
                != i64::from(self.as_of_ordinal) + i64::from(event.day_offset)
            {
                return Err("payment ordinal and day offset disagree".into());
            }
            last_offset = event.day_offset;
            if event.currency.len() != 3 || !event.currency.bytes().all(|c| c.is_ascii_uppercase())
            {
                return Err("currency must be a three-letter uppercase code".into());
            }
            event.components.validate()?;
            if let Some(period) = event.accrual_period {
                validate_ordinal(period.start_ordinal)?;
                validate_ordinal(period.end_ordinal)?;
                if period.start_ordinal > period.end_ordinal || i64::from(period.end_ordinal) > last
                {
                    return Err(
                        "accrual period must be ordered and end inside the schedule horizon".into(),
                    );
                }
            }
            validate_provenance(&event.provenance)?;
            validate_adjustment(&event.adjustment)?;
            admit_metadata(&mut metadata_bytes, event_metadata_bytes(event))?;
            if matches!(event.provenance, EventProvenance::Contractual { .. })
                && event.measure != CashflowMeasure::Contractual
            {
                return Err("exact contractual source events must use contractual measure".into());
            }
            let identity = identities
                .entry((&event.position_id, &event.event_id))
                .or_default();
            match &event.scenario {
                ScenarioSelector::All {} => {
                    if identity.0 || !identity.1.is_empty() {
                        return Err(
                            "duplicate event identity or all/specific scenario overlap".into()
                        );
                    }
                    identity.0 = true;
                }
                ScenarioSelector::Named { scenario_id } => {
                    if !scenarios.contains(scenario_id.as_str()) {
                        return Err("event references undeclared scenario".into());
                    }
                    if identity.0 || !identity.1.insert(scenario_id.as_str()) {
                        return Err(
                            "duplicate event identity or all/specific scenario overlap".into()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolve without overriding common events, resorting, or changing economics.
    pub fn for_scenario(&self, scenario_id: &str) -> Result<Vec<DatedCashflowEvent>, String> {
        self.validate()?;
        if !self.scenarios.iter().any(|s| s == scenario_id) {
            return Err("unknown dated scenario".into());
        }
        Ok(self
            .events
            .iter()
            .filter(|event| match &event.scenario {
                ScenarioSelector::All {} => true,
                ScenarioSelector::Named { scenario_id: id } => id == scenario_id,
            })
            .cloned()
            .collect())
    }
}

fn text_field(value: &str, name: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be nonempty, bounded text without control characters"
        ));
    }
    Ok(())
}

fn admit_metadata(total: &mut usize, bytes: usize) -> Result<(), String> {
    *total = total
        .checked_add(bytes)
        .ok_or("dated metadata byte count overflow")?;
    if *total > MAX_DATED_METADATA_BYTES {
        return Err("dated schedule metadata exceeds 32 MiB admission".into());
    }
    Ok(())
}

fn event_metadata_bytes(event: &DatedCashflowEvent) -> usize {
    let scenario_bytes = match &event.scenario {
        ScenarioSelector::All {} => 0,
        ScenarioSelector::Named { scenario_id } => scenario_id.len(),
    };
    let provenance_bytes = match &event.provenance {
        EventProvenance::Contractual {
            source_id,
            source_revision,
        } => source_id.len() + source_revision.len(),
        EventProvenance::Modeled {
            model_id,
            model_version,
            input_revision,
        } => model_id.len() + model_version.len() + input_revision.len(),
        EventProvenance::Assumed {
            assumption_id,
            rationale,
        } => assumption_id.len() + rationale.len(),
    };
    let adjustment_bytes = match &event.adjustment {
        CashflowAdjustment::None {} => 0,
        CashflowAdjustment::ProportionalSurvivingPrincipal { rationale, .. } => rationale.len(),
    };
    event.position_id.len()
        + event.event_id.len()
        + event.currency.len()
        + scenario_bytes
        + provenance_bytes
        + adjustment_bytes
}

fn validate_provenance(value: &EventProvenance) -> Result<(), String> {
    match value {
        EventProvenance::Contractual {
            source_id,
            source_revision,
        } => {
            text_field(source_id, "source_id", 256)?;
            text_field(source_revision, "source_revision", 256)?;
        }
        EventProvenance::Modeled {
            model_id,
            model_version,
            input_revision,
        } => {
            text_field(model_id, "model_id", 256)?;
            text_field(model_version, "model_version", 128)?;
            text_field(input_revision, "input_revision", 256)?;
        }
        EventProvenance::Assumed {
            assumption_id,
            rationale,
        } => {
            text_field(assumption_id, "assumption_id", 256)?;
            text_field(rationale, "assumption rationale", 1024)?;
        }
    }
    Ok(())
}

fn validate_adjustment(value: &CashflowAdjustment) -> Result<(), String> {
    if let CashflowAdjustment::ProportionalSurvivingPrincipal {
        surviving_fraction,
        rationale,
    } = value
    {
        if !surviving_fraction.is_finite() || !(0.0..1.0).contains(surviving_fraction) {
            return Err("proportional approximation fraction must be finite and in [0,1)".into());
        }
        text_field(rationale, "adjustment rationale", 1024)?;
    }
    Ok(())
}

pub fn validate_ordinal(ordinal: i32) -> Result<(), String> {
    if !(1..=MAX_ORDINAL).contains(&ordinal) {
        return Err("date ordinal must be in 1..3652059".into());
    }
    Ok(())
}

fn leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub fn ordinal_from_ymd(year: i32, month: u32, day: u32) -> Result<i32, String> {
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
    {
        return Err("invalid Gregorian year/month/day".into());
    }
    let prior_year = year - 1;
    let mut ordinal = prior_year * 365 + prior_year / 4 - prior_year / 100 + prior_year / 400;
    for m in 1..month {
        ordinal += days_in_month(year, m) as i32;
    }
    Ok(ordinal + day as i32)
}

pub fn ymd_from_ordinal(ordinal: i32) -> Result<(i32, u32, u32), String> {
    validate_ordinal(ordinal)?;
    // Binary-search years: bounded work even for malformed distant input dates.
    let (mut low, mut high) = (1, 9999);
    while low < high {
        let middle = (low + high + 1) / 2;
        if ordinal_from_ymd(middle, 1, 1)? <= ordinal {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let mut remaining = ordinal - ordinal_from_ymd(low, 1, 1)? + 1;
    let mut month = 1;
    while remaining > days_in_month(low, month) as i32 {
        remaining -= days_in_month(low, month) as i32;
        month += 1;
    }
    Ok((low, month, remaining as u32))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonthRoll {
    ClampDay,
    PreserveEndOfMonth,
    RejectInvalidDay,
}

/// Adds actual calendar months. Generate recurring dates from the original
/// anchor, not successive clamped dates (Jan 30 -> Feb 29 -> Mar 29 would drift).
pub fn add_calendar_months(ordinal: i32, months: i32, roll: MonthRoll) -> Result<i32, String> {
    let (year, month, day) = ymd_from_ordinal(ordinal)?;
    let target = i64::from(year - 1) * 12 + i64::from(month - 1) + i64::from(months);
    if !(0..(9999 * 12)).contains(&target) {
        return Err("calendar month offset exceeds Gregorian date domain".into());
    }
    let target_year = (target / 12 + 1) as i32;
    let target_month = (target % 12 + 1) as u32;
    let last_day = days_in_month(target_year, target_month);
    let target_day = match roll {
        MonthRoll::PreserveEndOfMonth if day == days_in_month(year, month) => last_day,
        MonthRoll::RejectInvalidDay if day > last_day => {
            return Err("month roll produces an invalid contractual day".into())
        }
        _ => day.min(last_day),
    };
    ordinal_from_ymd(target_year, target_month, target_day)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActualDayCount {
    Act360,
    Act365Fixed,
    ActActIsda,
}

/// Half-open [start,end) accrual. ACT/ACT ISDA splits at each January 1;
/// ACT/365F keeps a 365 denominator even in leap years. No 30/360 inference.
pub fn actual_year_fraction(
    start: i32,
    end: i32,
    convention: ActualDayCount,
) -> Result<f64, String> {
    validate_ordinal(start)?;
    validate_ordinal(end)?;
    if end < start {
        return Err("accrual end precedes start".into());
    }
    match convention {
        ActualDayCount::Act360 => Ok(f64::from(end - start) / 360.0),
        ActualDayCount::Act365Fixed => Ok(f64::from(end - start) / 365.0),
        ActualDayCount::ActActIsda => {
            let mut cursor = start;
            let mut total = 0.0;
            while cursor < end {
                let (year, _, _) = ymd_from_ordinal(cursor)?;
                let boundary = if year == 9999 {
                    end
                } else {
                    ordinal_from_ymd(year + 1, 1, 1)?.min(end)
                };
                total += f64::from(boundary - cursor) / if leap_year(year) { 366.0 } else { 365.0 };
                cursor = boundary;
            }
            Ok(total)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OvernightRateInterval {
    /// Interest application dates, not the publication/observation date.
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    /// Decimal annual rate (0.05 means 5%), already selected by the contract.
    pub rate: f64,
}

/// Product of (1 + r_i * actual_days_i / 360). An interval may cover a
/// weekend/holiday, applying simple interest within that interval. Input
/// intervals must exactly partition [start,end). No implicit fixings, floors,
/// margin compounding, lookback, holiday calendar, or observation shift.
pub fn overnight_compounded_factor(
    start: i32,
    end: i32,
    intervals: &[OvernightRateInterval],
) -> Result<f64, String> {
    validate_ordinal(start)?;
    validate_ordinal(end)?;
    if end < start
        || (end - start) as u32 > MAX_HORIZON_DAYS
        || intervals.len() > MAX_HORIZON_DAYS as usize
    {
        return Err("overnight accrual period exceeds bounded date domain".into());
    }
    let mut cursor = start;
    let mut factor = 1.0;
    for interval in intervals {
        if interval.start_ordinal != cursor
            || interval.end_ordinal <= cursor
            || interval.end_ordinal > end
            || !interval.rate.is_finite()
        {
            return Err(
                "overnight intervals must be finite, contiguous and partition the accrual period"
                    .into(),
            );
        }
        let term = 1.0 + interval.rate * f64::from(interval.end_ordinal - cursor) / 360.0;
        if !term.is_finite() || term <= 0.0 {
            return Err("overnight interval has nonpositive/nonfinite interest factor".into());
        }
        factor *= term;
        if !factor.is_finite() {
            return Err("overnight compounded factor overflow".into());
        }
        cursor = interval.end_ordinal;
    }
    if cursor != end {
        return Err("overnight intervals do not cover accrual period".into());
    }
    Ok(factor)
}
