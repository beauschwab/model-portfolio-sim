//! Actual-date contractual projections for fixed, nonoptional corporate/CD rows.
//! No pricing paths, OAS solve, behavioral overlay or journal posting occurs here.

use crate::conventions::{year_fraction, Bdc, Calendar, CalendarSpec, Date, DayCount};
use crate::term_deck::{Amortization, Contract, DeckRequest, Product};
use portfolio_model_core::amortization::level_payment_principal;
use portfolio_model_core::cashflow::{
    AccrualPeriod, CashflowAdjustment, CashflowComponents, CashflowMeasure, DatedCashflowEvent,
    DatedSchedule, EventProvenance, ScenarioSelector, DATED_CASHFLOW_SCHEMA, MAX_EVENTS,
    MAX_HORIZON_DAYS,
};
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DATED_TERM_SCHEMA: &str = "dated-term-1";
pub const MAX_TERM_BATCH: usize = 256;
const MAX_EXTRA_HOLIDAYS: usize = 1024;
const MODEL_DISCLOSURE: &str = "Fixed contractual projection; no credit/default/prepayment or behavioral scenarios. Accrued interest is the whole contractual period amount, not daily ledger recognition. Fees and book amortization are zero because their policies are absent. Price is inert; no valuation/OAS solve. Calendar US uses the existing research holiday rules, including year-scoped New Year observation; NONE/WEEKEND add no named holidays. Payment and accrual dates use explicit selected conventions; no monthly-grid truncation.";

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccrualDates {
    AdjustedPaymentDates,
    UnadjustedScheduledDates,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CashflowDirection {
    Receipt,
    Payment,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatedTermPosition {
    pub position_id: String,
    pub currency: String,
    pub source_id: String,
    pub source_revision: String,
    /// Remaining coupon's original accrual anchor, at or before portfolio asof.
    /// Notional is the remaining balance here; historical settlements are rejected.
    pub accrual_anchor_ordinal: i32,
    pub direction: CashflowDirection,
    pub contract: Contract,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatedTermRequest {
    pub schema: String,
    pub product: Product,
    pub asof: i32,
    pub horizon_days: u32,
    pub calendar: CalendarSpec,
    pub bdc: Bdc,
    pub accrual_dates: AccrualDates,
    pub scenarios: Vec<String>,
    pub positions: Vec<DatedTermPosition>,
}

#[derive(Clone, Serialize)]
pub struct TermPeriodAudit {
    pub position_id: String,
    pub event_id: String,
    pub scheduled_start_ordinal: i32,
    pub scheduled_end_ordinal: i32,
    pub accrual_start_ordinal: i32,
    pub accrual_end_ordinal: i32,
    pub payment_ordinal: i32,
    pub settlement_lag_days: i32,
    pub daycount: DayCount,
    pub accrual_year_fraction: f64,
    /// Actual currency units, before principal settlement for this period.
    pub outstanding_before: f64,
    pub principal_fraction: f64,
}

#[derive(Serialize)]
pub struct DatedTermResult {
    pub schema: String,
    pub schedule: DatedSchedule,
    pub periods: Vec<TermPeriodAudit>,
    pub accrual_dates: AccrualDates,
    pub disclosure: String,
}

impl DatedTermRequest {
    pub fn build(&self) -> Result<DatedTermResult, String> {
        if self.schema != DATED_TERM_SCHEMA {
            return Err("unsupported dated term schema".into());
        }
        if self.positions.len() > MAX_TERM_BATCH {
            return Err("dated term request exceeds 256-position batch".into());
        }
        if !matches!(self.calendar.name.as_str(), "US" | "NONE" | "WEEKEND")
            || self.calendar.extra_holidays.len() > MAX_EXTRA_HOLIDAYS
        {
            return Err(
                "dated term calendar must be US/NONE/WEEKEND with at most 1024 explicit holidays"
                    .into(),
            );
        }
        let asof = Date::ordinal(self.asof)?;
        let mut calendar = Calendar::new(&self.calendar)?;
        let mut schedule = DatedSchedule {
            schema: DATED_CASHFLOW_SCHEMA.into(),
            as_of_ordinal: self.asof,
            horizon_days: self.horizon_days,
            scenarios: self.scenarios.clone(),
            events: Vec::new(),
        };
        // Admit dates and scenarios before schedule construction/allocation.
        schedule.validate()?;
        let horizon = i64::from(self.asof) + i64::from(self.horizon_days);
        let mut identities = HashSet::new();
        let mut paired = Vec::new();
        let mut metadata_bytes =
            schedule.schema.len() + self.scenarios.iter().map(String::len).sum::<usize>();
        for position in &self.positions {
            validate_position(position, self.product)?;
            if !identities.insert(position.position_id.as_str()) {
                return Err("duplicate dated term position identity".into());
            }
            let anchor = Date::ordinal(position.accrual_anchor_ordinal)?;
            if anchor > asof || self.asof - anchor.number() > MAX_HORIZON_DAYS as i32 {
                return Err("accrual anchor must be at/before asof within 100-year domain".into());
            }
            let contract = &position.contract;
            let maturity = Date::ordinal(contract.maturity)?;
            if maturity <= asof || i64::from(maturity.number()) > horizon {
                return Err(
                    "dated term maturity must be future and inside declared horizon".into(),
                );
            }
            let freq = contract.freq_months.unwrap_or(0);
            let original = if self.product == Product::Cd && freq <= 0 {
                vec![(
                    anchor,
                    maturity,
                    year_fraction(anchor, maturity, contract.daycount),
                )]
            } else {
                calendar.schedule(anchor, maturity, freq, contract.daycount, Bdc::None)?
            };
            if original.len() > MAX_EVENTS - paired.len() {
                return Err("dated term event admission exceeded".into());
            }
            let chosen = if self.accrual_dates == AccrualDates::AdjustedPaymentDates {
                if self.product == Product::Cd && freq <= 0 {
                    // Existing single-payment CD semantics keep the original
                    // anchor but accrue until adjusted maturity settlement.
                    let pay = calendar.adjust(maturity, self.bdc)?;
                    vec![(anchor, pay, year_fraction(anchor, pay, contract.daycount))]
                } else {
                    calendar.schedule(anchor, maturity, freq, contract.daycount, self.bdc)?
                }
            } else {
                original.clone()
            };
            if chosen.len() != original.len() {
                return Err("dated term original/adjusted period counts disagree".into());
            }
            let mut payments = Vec::with_capacity(original.len());
            for (_, original_end, _) in &original {
                let pay = calendar.adjust(*original_end, self.bdc)?;
                if pay < asof || i64::from(pay.number()) > horizon {
                    return Err("dated term settlement is historical or outside horizon; no clipping allowed".into());
                }
                payments.push(pay);
            }
            if chosen.iter().any(|(start, end, tau)| {
                start > end || i64::from(end.number()) > horizon || !tau.is_finite() || *tau <= 0.0
            }) {
                return Err(
                    "dated term accrual periods must be positive and inside horizon".into(),
                );
            }
            // No price/OAS/market computation: use the deck only for raw contract
            // validation and established non-level-payment principal semantics.
            // The grid ends strictly later than every selected date; true
            // ordinals are always used for output, never recovered from t_pay.
            let span = (horizon - i64::from(anchor.number())) as f64;
            let months = ((span / 365.0 * 12.0).ceil() as usize + 2).max(2);
            let deck_request = DeckRequest {
                product: self.product,
                asof: anchor.number(),
                months,
                calendar: self.calendar.clone(),
                bdc: if self.accrual_dates == AccrualDates::AdjustedPaymentDates {
                    self.bdc
                } else {
                    Bdc::None
                },
                contracts: vec![contract.clone()],
            };
            let deck = deck_request.build()?;
            if deck.tau.len() != chosen.len() {
                return Err("dated term deck and exact schedule disagree".into());
            }
            let taus: Vec<f64> = chosen.iter().map(|(_, _, tau)| *tau).collect();
            if deck.tau != taus {
                return Err("dated term accrual convention differs from normalized deck".into());
            }
            let principal = if self.product == Product::Cd {
                let mut values = vec![0.0; chosen.len()];
                *values.last_mut().ok_or("empty dated term schedule")? = 1.0;
                values
            } else if contract.amort_type == Amortization::LevelPayment {
                level_payment_principal(contract.coupon, &taus)?
            } else {
                deck.prin
            };
            if principal.len() != chosen.len()
                || (principal.iter().sum::<f64>() - 1.0).abs() > 1e-12
            {
                return Err("dated term principal coefficients do not conserve balance".into());
            }
            let sign = if position.direction == CashflowDirection::Receipt {
                1.0
            } else {
                -1.0
            };
            let mut remaining: f64 = 1.0;
            for (j, ((original_start, original_end, _), (start, end, tau))) in
                original.iter().zip(&chosen).enumerate()
            {
                let pay = payments[j];
                let p = principal[j];
                if !p.is_finite() || p < 0.0 || p > remaining + 1e-12 {
                    return Err("dated term principal reduction exceeds surviving balance".into());
                }
                let normalized_principal = if j + 1 == chosen.len() {
                    remaining
                } else {
                    p.min(remaining)
                };
                let outstanding = remaining * contract.notional;
                let interest = sign * outstanding * contract.coupon * *tau;
                let event_id = format!("coupon:{}:{j}", original_end.number());
                let event = DatedCashflowEvent {
                    position_id: position.position_id.clone(),
                    event_id: event_id.clone(),
                    scenario: ScenarioSelector::All {},
                    payment_ordinal: pay.number(),
                    day_offset: (pay.number() - self.asof) as u32,
                    accrual_period: Some(AccrualPeriod {
                        start_ordinal: start.number(),
                        end_ordinal: end.number(),
                    }),
                    currency: position.currency.clone(),
                    components: CashflowComponents {
                        principal: sign * normalized_principal * contract.notional,
                        cash_interest: interest,
                        accrued_interest: interest,
                        book_amortization: 0.0,
                        fees: 0.0,
                    },
                    measure: CashflowMeasure::Contractual,
                    provenance: EventProvenance::Contractual {
                        source_id: position.source_id.clone(),
                        source_revision: position.source_revision.clone(),
                    },
                    adjustment: CashflowAdjustment::None {},
                };
                event.components.cash_total()?;
                // Include duplicated audit strings in this producer's metadata
                // admission, beyond the core schedule's own 32 MiB guard.
                let bytes = 2 * (position.position_id.len() + event_id.len())
                    + position.currency.len()
                    + position.source_id.len()
                    + position.source_revision.len();
                metadata_bytes = metadata_bytes
                    .checked_add(bytes)
                    .ok_or("dated term metadata byte count overflow")?;
                if metadata_bytes > MAX_DATED_METADATA_BYTES {
                    return Err("dated term event/audit metadata exceeds 32 MiB admission".into());
                }
                let audit = TermPeriodAudit {
                    position_id: position.position_id.clone(),
                    event_id,
                    scheduled_start_ordinal: original_start.number(),
                    scheduled_end_ordinal: original_end.number(),
                    accrual_start_ordinal: start.number(),
                    accrual_end_ordinal: end.number(),
                    payment_ordinal: pay.number(),
                    settlement_lag_days: pay.number() - end.number(),
                    daycount: contract.daycount,
                    accrual_year_fraction: *tau,
                    outstanding_before: outstanding,
                    principal_fraction: normalized_principal,
                };
                paired.push((event, audit));
                remaining = (remaining - normalized_principal).max(0.0);
            }
            if remaining != 0.0 {
                return Err("dated term principal did not settle fully".into());
            }
        }
        // Stable sort preserves source position/period order when dates tie.
        paired.sort_by_key(|(event, _)| event.payment_ordinal);
        let mut periods = Vec::with_capacity(paired.len());
        for (event, audit) in paired {
            schedule.events.push(event);
            periods.push(audit);
        }
        schedule.validate()?;
        Ok(DatedTermResult {
            schema: DATED_TERM_SCHEMA.into(),
            schedule,
            periods,
            accrual_dates: self.accrual_dates,
            disclosure: MODEL_DISCLOSURE.into(),
        })
    }
}

fn bounded_text(value: &str, name: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(format!("dated term {name} must be bounded nonempty text"));
    }
    Ok(())
}

fn validate_position(position: &DatedTermPosition, product: Product) -> Result<(), String> {
    bounded_text(&position.position_id, "position_id", 256)?;
    bounded_text(&position.source_id, "source_id", 256)?;
    bounded_text(&position.source_revision, "source_revision", 256)?;
    if position.currency.len() != 3 || !position.currency.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err("dated term currency must be a three-letter uppercase code".into());
    }
    let contract = &position.contract;
    if !contract.notional.is_finite() || !(0.0..=MAX_MONETARY_AMOUNT).contains(&contract.notional) {
        return Err("dated term notional exceeds monetary admission".into());
    }
    if contract.is_float
        || contract
            .call_schedule
            .as_deref()
            .is_some_and(|s| !s.is_empty())
        || contract
            .put_schedule
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        return Err(
            "dated term requires fixed, nonoptional contracts; no floating/call/put projection"
                .into(),
        );
    }
    if contract.amort_type == Amortization::Sink || contract.sink_schedule.is_some() {
        return Err("dated term sinking schedules require exact dated principal support; heuristic snapping is rejected".into());
    }
    if product == Product::Cd {
        if contract.amort_type != Amortization::Bullet {
            return Err("dated term CDs require bullet principal amortization".into());
        }
        if contract.channel != "brokered" && contract.ew_mult != 0.0 {
            return Err("dated term CDs require disabled early withdrawal behavior".into());
        }
    }
    Ok(())
}
