//! Explicit mortgage/agency state paths; supplied coefficients are not empirical fits.
use crate::cashflow::{actual_year_fraction, validate_ordinal, ActualDayCount, MAX_EVENTS};
use crate::consumer_contract::{
    self as consumer, ConsumerCashEvent, ConsumerIdentity, ConsumerRecognition,
};
use crate::credit::CreditMeasure;
use crate::credit_operator::{self, CreditOperatorRequest, CreditOperatorState};
use crate::floating_rate::{
    IndexDescriptor, IndexKind, RevisionIdentity, SuppliedFixing, MAX_PUBLICATION_TIMESTAMP,
};
use crate::limits::MAX_MONETARY_AMOUNT;
use crate::mortgage_receipts::{self as receipt, MortgageReceiptSink};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "mortgage-state-1";
pub const MAX_DEFAULT_CLAIMS: usize = 4096;
pub const MAX_RECOVERY_POINTS: usize = 32;
pub const MAX_MORTGAGE_WORK: usize = 2_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MortgageOwnership {
    WholeLoan {
        remittance_lag_days: u32,
        recognized_accrual_fraction: [f64; 3],
    },
    /// Supplied guarantee economics, not automatic adoption of any agency's legal terms.
    Agency {
        issuer: String,
        guarantee: RevisionIdentity,
        remittance_lag_days: u32,
        default_buyout_lag_days: u32,
        servicing_and_guarantee_spread: f64,
        default_coupon_policy: DefaultCouponPolicy,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultCouponPolicy {
    FrozenAtDefault,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentRecast {
    RetainOriginal,
    ReamortizeRemaining,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmTerms {
    pub index: IndexDescriptor,
    pub reference_calendar: RevisionIdentity,
    pub margin: f64,
    pub original_note_rate: f64,
    pub initial_up_cap: f64,
    pub initial_down_cap: f64,
    pub subsequent_up_cap: f64,
    pub subsequent_down_cap: f64,
    pub lifetime_up_cap: f64,
    pub lifetime_down_cap: f64,
    pub note_floor: f64,
    pub prior_reset_count: u32,
    pub payment_recast: PaymentRecast,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MortgageIndexInput {
    Observed {
        fixing: SuppliedFixing,
        reset_cutoff_unix_seconds: i64,
    },
    Projected {
        fixing: SuppliedFixing,
        projection_source: RevisionIdentity,
        available_unix_seconds: i64,
        reset_cutoff_unix_seconds: i64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmReset {
    pub ordinal: i32,
    pub input: MortgageIndexInput,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MortgageRate {
    Fixed,
    Arm {
        terms: ArmTerms,
        resets: Vec<ArmReset>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefinanceCoefficients {
    /// Annual refinance intensity per decimal annual rate incentive.
    pub incentive_slope: f64,
    pub incentive_threshold: f64,
    pub turnover_seasoning_days: u32,
    pub refinance_seasoning_days: u32,
    /// Exponential suppression per retained positive-incentive model-year.
    pub burnout_sensitivity: f64,
    pub prepay_eligibility: [f64; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MortgagePathPoint {
    pub start_ordinal: i32,
    pub refinancing_market_rate: f64,
    pub turnover_hazard_per_year: f64,
    /// Live P/W/D rows, P/W/D/default/prepaid columns. Prepaid must be zero here.
    pub credit_hazards_per_year: [[f64; 5]; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MortgageRecoveryWeight {
    pub lag_days: u32,
    pub weight: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MortgageRequest {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub monetary_unit: String,
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
    pub origination_ordinal: i32,
    pub original_principal: f64,
    pub original_payment: f64,
    pub opening_contractual_principal: f64,
    pub opening_contractual_interest: f64,
    pub current_payment: f64,
    pub current_note_rate: f64,
    pub opening_probability: [f64; 3],
    pub opening_borrower_principal: [f64; 3],
    pub opening_borrower_interest: [f64; 3],
    pub opening_investor_principal: [f64; 3],
    pub opening_investor_interest: [f64; 3],
    pub opening_burnout_incentive_years: f64,
    pub day_count: ActualDayCount,
    pub rate: MortgageRate,
    pub ownership: MortgageOwnership,
    pub publication_cutoff_unix_seconds: i64,
    /// Complete remaining contractual payment calendar, including dates after reporting.
    pub payment_dates: Vec<i32>,
    pub borrower_collection_fraction: [f64; 3],
    pub refinance: RefinanceCoefficients,
    pub path: Vec<MortgagePathPoint>,
    pub loss_given_default: [f64; 3],
    pub complete_recovery_tails: [Vec<MortgageRecoveryWeight>; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageEntitlement {
    pub event_id: String,
    pub entitlement_ordinal: i32,
    pub remittance_ordinal: i32,
    pub kind: String,
    pub principal: f64,
    pub interest: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageRecoveryAmount {
    pub ordinal: i32,
    pub principal: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageDefaultClaim {
    pub claim_id: String,
    pub origin: usize,
    pub default_ordinal: i32,
    pub borrower_principal: f64,
    pub borrower_contract_interest: f64,
    pub investor_principal: f64,
    pub investor_accrued_interest: f64,
    pub borrower_principal_loss: f64,
    pub complete_recovery_tail: Vec<MortgageRecoveryAmount>,
    pub investor_buyout_ordinal: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgagePoint {
    pub ordinal: i32,
    pub seasoning_days: u32,
    pub burnout_incentive_years: f64,
    pub note_rate: f64,
    pub current_payment: f64,
    pub contractual_principal: f64,
    pub probability: [f64; 7],
    pub matured_probability: f64,
    pub borrower_principal: [f64; 3],
    pub borrower_interest: [f64; 3],
    pub investor_principal: [f64; 3],
    pub investor_interest: [f64; 3],
    pub turnover_hazard_per_year: f64,
    pub refinance_hazard_per_year: f64,
    pub borrower_default_principal: f64,
    pub investor_default_principal: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageResetResult {
    pub ordinal: i32,
    pub previous_rate: f64,
    pub selected_index_rate: f64,
    pub new_rate: f64,
    pub new_payment: f64,
    pub fixing_id: String,
    pub fixing_revision: String,
    pub projected: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MortgageResult {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub ownership: MortgageOwnership,
    pub monetary_unit: String,
    pub original_principal: f64,
    pub original_payment: f64,
    pub origination_ordinal: i32,
    pub borrower_cashflows: Vec<ConsumerCashEvent>,
    pub cashflows: Vec<ConsumerCashEvent>,
    pub entitlements: Vec<MortgageEntitlement>,
    pub recognition: Vec<ConsumerRecognition>,
    pub points: Vec<MortgagePoint>,
    pub defaults: Vec<MortgageDefaultClaim>,
    pub resets: Vec<MortgageResetResult>,
    pub borrower_scheduled_principal: f64,
    pub borrower_turnover_principal: f64,
    pub borrower_refinance_principal: f64,
    pub investor_scheduled_principal: f64,
    pub investor_turnover_principal: f64,
    pub investor_refinance_principal: f64,
    pub guarantee_principal_advance: f64,
    pub guarantee_interest_advance: f64,
    pub borrower_principal_conservation_error: f64,
    pub investor_principal_conservation_error: f64,
    /// Supplied fixing/reset dates retain calendar identity; holiday lookup is caller-owned.
    pub calendar_mapping_verified: bool,
    pub work_units: usize,
}

fn signed(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("mortgage amount exceeds finite currency-unit bounds".into())
    } else {
        Ok(())
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8_f64.max(256. * f64::EPSILON * a.abs().max(b.abs()))
}
fn revision(id: &RevisionIdentity) -> Result<(), String> {
    consumer::label(&id.id)?;
    consumer::label(&id.revision)
}
fn date_after(day: i32, lag: u32) -> Result<i32, String> {
    let day = day
        .checked_add(lag as i32)
        .ok_or("mortgage date overflow")?;
    validate_ordinal(day)?;
    Ok(day)
}
fn lag(ownership: &MortgageOwnership) -> u32 {
    match ownership {
        MortgageOwnership::WholeLoan {
            remittance_lag_days,
            ..
        }
        | MortgageOwnership::Agency {
            remittance_lag_days,
            ..
        } => *remittance_lag_days,
    }
}
fn investor_coupon(r: &MortgageRequest, note: f64) -> Result<f64, String> {
    let value = match &r.ownership {
        MortgageOwnership::WholeLoan { .. } => note,
        MortgageOwnership::Agency {
            servicing_and_guarantee_spread,
            ..
        } => note - servicing_and_guarantee_spread,
    };
    if value < 0. {
        Err("agency note rate cannot fall below supplied servicing/guarantee spread".into())
    } else {
        Ok(value)
    }
}
fn validate(r: &MortgageRequest) -> Result<i32, String> {
    if r.schema != SCHEMA || r.monetary_unit != "currency_units" {
        return Err("explicit mortgage model selection and currency_units required".into());
    }
    consumer::identity(&r.identity, SCHEMA)?;
    consumer::day_count(r.day_count)?;
    if r.identity.currency.len() != 3
        || !r.identity.currency.bytes().all(|c| c.is_ascii_uppercase())
    {
        return Err("mortgage currency requires a three-letter code".into());
    }
    let end = consumer::horizon(r.as_of_ordinal, r.horizon_days)?;
    validate_ordinal(r.origination_ordinal)?;
    if r.origination_ordinal > r.as_of_ordinal
        || (r.as_of_ordinal - r.origination_ordinal) as u32 > consumer::MAX_CONSUMER_DAYS
        || r.horizon_days == 0
    {
        return Err(
            "mortgage requires bounded positive horizon and actual origination seasoning".into(),
        );
    }
    for x in [
        r.original_principal,
        r.original_payment,
        r.opening_contractual_principal,
        r.opening_contractual_interest,
        r.current_payment,
    ] {
        consumer::amount(x, "mortgage contract amount")?;
    }
    if r.original_principal == 0.
        || r.original_payment == 0.
        || r.current_payment == 0.
        || r.opening_contractual_principal > r.original_principal
    {
        return Err(
            "mortgage original principal/payment and contractual opening are inconsistent".into(),
        );
    }
    consumer::rate(r.current_note_rate)?;
    if !r.opening_burnout_incentive_years.is_finite()
        || !(0.0..=200.0).contains(&r.opening_burnout_incentive_years)
    {
        return Err("retained burnout must be finite in 0..200 incentive-years".into());
    }
    for array in [
        &r.opening_borrower_principal,
        &r.opening_borrower_interest,
        &r.opening_investor_principal,
        &r.opening_investor_interest,
    ] {
        for &x in array {
            consumer::amount(x, "mortgage opening amount")?;
        }
        consumer::amount(array.iter().sum(), "mortgage opening aggregate")?;
    }
    if r.opening_borrower_principal.iter().sum::<f64>() > r.original_principal
        || r.opening_investor_principal
            .iter()
            .zip(r.opening_borrower_principal)
            .any(|(investor, borrower)| *investor > borrower)
    {
        return Err("mortgage outstanding borrower/agency principal exceeds original or unadvanced borrower principal".into());
    }
    for &p in &r.opening_probability {
        consumer::fraction(p, "mortgage population mass")?;
    }
    if r.opening_probability.iter().sum::<f64>() > 1. + 1e-11 {
        return Err("mortgage absolute live mass exceeds one".into());
    }
    for &f in r
        .borrower_collection_fraction
        .iter()
        .chain(&r.refinance.prepay_eligibility)
    {
        consumer::fraction(f, "mortgage state collection/prepay eligibility")?;
    }
    if !(0..=MAX_PUBLICATION_TIMESTAMP).contains(&r.publication_cutoff_unix_seconds)
        || r.payment_dates.is_empty()
        || r.payment_dates.len() > consumer::MAX_CONSUMER_EVENTS
        || r.path.is_empty()
        || r.path.len() > consumer::MAX_CONSUMER_EVENTS
    {
        return Err("mortgage dated work/fixing cutoff exceeds bounds".into());
    }
    let mut last = r.as_of_ordinal;
    for &day in &r.payment_dates {
        validate_ordinal(day)?;
        if day <= last || (day - r.as_of_ordinal) as u32 > consumer::MAX_CONSUMER_DAYS {
            return Err(
                "complete remaining payment calendar must be strictly dated within100years".into(),
            );
        }
        last = day;
    }
    if (*r.payment_dates.last().unwrap() - r.origination_ordinal) as u32
        > consumer::MAX_CONSUMER_DAYS
    {
        return Err("mortgage original-to-final contractual term exceeds100years".into());
    }
    if r.path[0].start_ordinal != r.as_of_ordinal {
        return Err("mortgage path must start exactly as-of".into());
    }
    let mut last = None;
    for p in &r.path {
        validate_ordinal(p.start_ordinal)?;
        consumer::rate(p.refinancing_market_rate)?;
        if p.start_ordinal > end
            || last.is_some_and(|d| p.start_ordinal <= d)
            || !p.turnover_hazard_per_year.is_finite()
            || !(0.0..=100.).contains(&p.turnover_hazard_per_year)
            || p.credit_hazards_per_year.iter().any(|row| row[4] != 0.)
        {
            return Err(
                "mortgage path dates/hazards invalid or prepaid has overlapping ownership".into(),
            );
        }
        credit_operator::compile(&CreditOperatorRequest {
            schema: credit_operator::SCHEMA.into(),
            measure: measure(r),
            duration_years: 0.,
            transition_hazards_per_year: p.credit_hazards_per_year,
            accrual_rates: [0.; 3],
        })?;
        last = Some(p.start_ordinal);
    }
    let c = &r.refinance;
    if !c.incentive_slope.is_finite()
        || !(0.0..=10_000.).contains(&c.incentive_slope)
        || !c.incentive_threshold.is_finite()
        || !(0.0..=2.).contains(&c.incentive_threshold)
        || !c.burnout_sensitivity.is_finite()
        || !(0.0..=100.).contains(&c.burnout_sensitivity)
        || c.turnover_seasoning_days > consumer::MAX_CONSUMER_DAYS
        || c.refinance_seasoning_days > consumer::MAX_CONSUMER_DAYS
    {
        return Err("mortgage refinance coefficients exceed supplied bounded domain".into());
    }
    for i in 0..3 {
        consumer::fraction(r.loss_given_default[i], "mortgage LGD")?;
        let tail = &r.complete_recovery_tails[i];
        if tail.len() > MAX_RECOVERY_POINTS {
            return Err("mortgage recovery tail exceeds32points".into());
        }
        let mut previous = None;
        let mut sum = 0.;
        for p in tail {
            consumer::fraction(p.weight, "mortgage recovery weight")?;
            if p.lag_days > consumer::MAX_CONSUMER_DAYS || previous.is_some_and(|d| p.lag_days <= d)
            {
                return Err("mortgage complete recovery lags must increase within100years".into());
            }
            sum += p.weight;
            previous = Some(p.lag_days);
        }
        if (!tail.is_empty() && (sum - 1.).abs() > 8. * f64::EPSILON)
            || (tail.is_empty() && r.loss_given_default[i] != 1.)
        {
            return Err("mortgage recoverable principal needs a complete unit-weight tail".into());
        }
    }
    match &r.ownership {
        MortgageOwnership::WholeLoan {
            recognized_accrual_fraction,
            remittance_lag_days,
        } => {
            for &f in recognized_accrual_fraction {
                consumer::fraction(f, "recognized accrual fraction")?;
            }
            if *remittance_lag_days > consumer::MAX_CONSUMER_DAYS
                || r.opening_investor_principal != r.opening_borrower_principal
            {
                return Err("whole-loan principal/opening/remittance mismatch".into());
            }
        }
        MortgageOwnership::Agency {
            issuer,
            guarantee,
            remittance_lag_days,
            default_buyout_lag_days,
            servicing_and_guarantee_spread,
            ..
        } => {
            consumer::label(issuer)?;
            revision(guarantee)?;
            consumer::rate(*servicing_and_guarantee_spread)?;
            if (*remittance_lag_days).max(*default_buyout_lag_days) > consumer::MAX_CONSUMER_DAYS {
                return Err("agency remittance/buyout lag exceeds100years".into());
            }
        }
    }
    investor_coupon(r, r.current_note_rate)?;
    match &r.rate {
        MortgageRate::Fixed => {
            if r.current_payment != r.original_payment {
                return Err("fixed mortgage must retain original payment".into());
            }
        }
        MortgageRate::Arm { terms, resets } => {
            consumer::label(&terms.index.index_id)?;
            revision(&terms.reference_calendar)?;
            if terms.index.kind == IndexKind::Overnight
                || resets.len() > consumer::MAX_CONSUMER_EVENTS
                || terms.prior_reset_count > 4096
            {
                return Err(
                    "ARM requires supplied term/published-average index and bounded resets".into(),
                );
            }
            match terms.index.kind {
                IndexKind::Term
                    if terms
                        .index
                        .tenor_months
                        .is_some_and(|n| (1..=120).contains(&n))
                        && terms.index.average_calendar_days.is_none() => {}
                IndexKind::PublishedAverage
                    if terms
                        .index
                        .average_calendar_days
                        .is_some_and(|n| (1..=365).contains(&n))
                        && terms.index.tenor_months.is_none() => {}
                _ => {
                    return Err(
                        "ARM index tenor/average descriptor is incomplete or ambiguous".into(),
                    )
                }
            }
            for x in [
                terms.margin,
                terms.original_note_rate,
                terms.initial_up_cap,
                terms.initial_down_cap,
                terms.subsequent_up_cap,
                terms.subsequent_down_cap,
                terms.lifetime_up_cap,
                terms.lifetime_down_cap,
                terms.note_floor,
            ] {
                consumer::rate(x)?;
            }
            let lo = (terms.original_note_rate - terms.lifetime_down_cap).max(terms.note_floor);
            let hi = terms.original_note_rate + terms.lifetime_up_cap;
            if lo > hi || hi > 2. || r.current_note_rate < lo || r.current_note_rate > hi {
                return Err("ARM current/lifetime floor-cap domain inconsistent".into());
            }
            let mut previous = r.as_of_ordinal;
            let mut ids = BTreeSet::new();
            for reset in resets {
                validate_ordinal(reset.ordinal)?;
                if reset.ordinal <= previous || reset.ordinal > *r.payment_dates.last().unwrap() {
                    return Err(
                        "ARM reset dates must be strictly afterasof and withinremainingterm".into(),
                    );
                }
                previous = reset.ordinal;
                let (fixing, cutoff, available) = match &reset.input {
                    MortgageIndexInput::Observed {
                        fixing,
                        reset_cutoff_unix_seconds,
                    } => (
                        fixing,
                        *reset_cutoff_unix_seconds,
                        fixing.publication_unix_seconds,
                    ),
                    MortgageIndexInput::Projected {
                        fixing,
                        projection_source,
                        available_unix_seconds,
                        reset_cutoff_unix_seconds,
                    } => {
                        revision(projection_source)?;
                        (fixing, *reset_cutoff_unix_seconds, *available_unix_seconds)
                    }
                };
                consumer::label(&fixing.fixing_id)?;
                consumer::label(&fixing.revision)?;
                validate_ordinal(fixing.value_ordinal)?;
                if !ids.insert(&fixing.fixing_id)
                    || fixing.index_id != terms.index.index_id
                    || fixing.value_ordinal > reset.ordinal
                    || !fixing.rate.is_finite()
                    || !(-1.0..=2.0).contains(&fixing.rate)
                    || !(0..=MAX_PUBLICATION_TIMESTAMP).contains(&cutoff)
                    || !(0..=MAX_PUBLICATION_TIMESTAMP).contains(&available)
                    || available > r.publication_cutoff_unix_seconds
                    || available > cutoff
                    || cutoff / 86_400 + 719_163 > i64::from(reset.ordinal)
                    || r.publication_cutoff_unix_seconds / 86_400 + 719_163
                        > i64::from(r.as_of_ordinal)
                    || fixing.publication_unix_seconds < 0
                    || fixing.publication_unix_seconds > r.publication_cutoff_unix_seconds
                {
                    return Err("ARM fixing identity/publication/available cutoff forbids missing or future observed data".into());
                }
            }
        }
    }
    Ok(end)
}
fn measure(r: &MortgageRequest) -> CreditMeasure {
    match r.identity.measure {
        crate::cashflow::CashflowMeasure::Pricing => CreditMeasure::Pricing,
        _ => CreditMeasure::Physical,
    }
}
fn operator_state(
    r: &MortgageRequest,
    p: [f64; 7],
    b: [f64; 3],
    a: [f64; 3],
) -> CreditOperatorState {
    let mut principal = [0.; 7];
    let mut accrued = [0.; 7];
    principal[..3].copy_from_slice(&b);
    accrued[..3].copy_from_slice(&a);
    CreditOperatorState {
        schema: credit_operator::STATE_SCHEMA.into(),
        measure: measure(r),
        monetary_unit: r.monetary_unit.clone(),
        probability: p,
        principal,
        undrawn_commitment: [0.; 7],
        accrued_interest: accrued,
        book_adjustment: [0.; 7],
    }
}
fn cash(
    events: &mut Vec<ConsumerCashEvent>,
    day: i32,
    kind: &str,
    principal: f64,
    interest: f64,
) -> Result<(), String> {
    for x in [principal, interest, principal + interest] {
        consumer::amount(x, "mortgage cash amount")?;
    }
    if events.len() >= MAX_EVENTS {
        return Err("mortgage cash events exceed250000 output bound".into());
    }
    if principal != 0. || interest != 0. {
        events.push(ConsumerCashEvent {
            event_id: format!("{}-{}", kind, events.len()),
            ordinal: day,
            kind: kind.into(),
            principal,
            interest,
            fees: 0.,
        });
    }
    Ok(())
}
fn entitlement(
    r: &MortgageRequest,
    events: &mut Vec<MortgageEntitlement>,
    day: i32,
    kind: &str,
    principal: f64,
    interest: f64,
) -> Result<(), String> {
    for x in [principal, interest, principal + interest] {
        consumer::amount(x, "mortgage entitlement")?;
    }
    if events.len() >= MAX_EVENTS {
        return Err("mortgage entitlements exceed250000 output bound".into());
    }
    if principal != 0. || interest != 0. {
        events.push(MortgageEntitlement {
            event_id: format!("{}-{}", kind, events.len()),
            entitlement_ordinal: day,
            remittance_ordinal: date_after(day, lag(&r.ownership))?,
            kind: kind.into(),
            principal,
            interest,
        });
    }
    Ok(())
}
fn seasoning(age: u32, ramp: u32) -> f64 {
    if ramp == 0 {
        1.
    } else {
        (f64::from(age) / f64::from(ramp)).min(1.)
    }
}

fn work(result: &mut MortgageResult, n: usize) -> Result<(), String> {
    result.work_units = result
        .work_units
        .checked_add(n)
        .ok_or("mortgage work count overflow")?;
    if result.work_units > MAX_MORTGAGE_WORK {
        return Err("mortgage interval/claim/coupon work exceeds2millionunits".into());
    }
    Ok(())
}
fn agency_buyout(
    r: &MortgageRequest,
    result: &mut MortgageResult,
    day: i32,
    principal: f64,
    mut interest: f64,
    coupon: f64,
    end: i32,
) -> Result<Option<i32>, String> {
    let MortgageOwnership::Agency {
        default_buyout_lag_days,
        ..
    } = &r.ownership
    else {
        return Ok(None);
    };
    let buyout = date_after(day, *default_buyout_lag_days)?;
    let mut since = day;
    if r.payment_dates.binary_search(&day).is_ok() && buyout > day {
        entitlement(
            r,
            &mut result.entitlements,
            day,
            "agency_default_guaranteed_coupon",
            0.,
            interest,
        )?;
        interest = 0.;
    }
    for &due in r.payment_dates.iter().filter(|&&d| d > day && d < buyout) {
        work(result, 1)?;
        interest += principal * coupon * actual_year_fraction(since, due, r.day_count)?;
        entitlement(
            r,
            &mut result.entitlements,
            due,
            "agency_default_guaranteed_coupon",
            0.,
            interest,
        )?;
        if since < end && principal * coupon != 0. {
            result.recognition.push(ConsumerRecognition {
                origin_id: format!("agency-default-earned-{}", result.recognition.len()),
                start_ordinal: since,
                end_ordinal: due.min(end),
                interest: principal
                    * coupon
                    * actual_year_fraction(since, due.min(end), r.day_count)?,
            });
        }
        interest = 0.;
        since = due;
    }
    interest += principal * coupon * actual_year_fraction(since, buyout, r.day_count)?;
    if since < end && buyout > since && principal * coupon != 0. {
        result.recognition.push(ConsumerRecognition {
            origin_id: format!("agency-default-earned-{}", result.recognition.len()),
            start_ordinal: since,
            end_ordinal: buyout.min(end),
            interest: principal
                * coupon
                * actual_year_fraction(since, buyout.min(end), r.day_count)?,
        });
    }
    entitlement(
        r,
        &mut result.entitlements,
        buyout,
        "agency_default_buyout",
        principal,
        interest,
    )?;
    Ok(Some(buyout))
}

/// Validate the existing model input domains without generating state paths,
/// cashflows or recognition. Projection reuses this same private validator.
pub fn validate_request(r: &MortgageRequest) -> Result<(), String> {
    validate(r).map(|_| ())
}

pub fn project(r: &MortgageRequest) -> Result<MortgageResult, String> {
    project_impl(r, None, None)
}
pub fn project_with_receipts(
    r: &MortgageRequest,
    sink: &mut dyn MortgageReceiptSink,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<MortgageResult, String> {
    project_impl(r, Some(sink), Some(check))
}
fn project_impl(
    r: &MortgageRequest,
    mut sink: Option<&mut dyn MortgageReceiptSink>,
    mut check: Option<&mut dyn FnMut() -> Result<(), String>>,
) -> Result<MortgageResult, String> {
    if let Some(check) = check.as_deref_mut() {
        check()?;
    }
    let end = validate(r)?;
    let path: BTreeMap<_, _> = r.path.iter().map(|p| (p.start_ordinal, p)).collect();
    let dues: BTreeSet<_> = r.payment_dates.iter().copied().collect();
    let resets: BTreeMap<_, _> = match &r.rate {
        MortgageRate::Fixed => BTreeMap::new(),
        MortgageRate::Arm { resets, .. } => resets.iter().map(|p| (p.ordinal, p)).collect(),
    };
    let days: BTreeSet<_> = dues
        .iter()
        .chain(path.keys())
        .chain(resets.keys())
        .copied()
        .filter(|&d| d > r.as_of_ordinal && d <= end)
        .chain([end])
        .collect();
    if days.len() > consumer::MAX_CONSUMER_EVENTS {
        return Err("mortgage combined transition calendar exceeds4096boundaries".into());
    }
    if let Some(sink) = sink.as_deref_mut() {
        sink.admit(receipt::AdmissionView {
            request: r,
            interval_count: days.len(),
        })?;
    }
    let mut result = MortgageResult {
        schema: SCHEMA.into(),
        identity: r.identity.clone(),
        ownership: r.ownership.clone(),
        monetary_unit: r.monetary_unit.clone(),
        original_principal: r.original_principal,
        original_payment: r.original_payment,
        origination_ordinal: r.origination_ordinal,
        borrower_cashflows: vec![],
        cashflows: vec![],
        entitlements: vec![],
        recognition: vec![],
        points: vec![],
        defaults: vec![],
        resets: vec![],
        borrower_scheduled_principal: 0.,
        borrower_turnover_principal: 0.,
        borrower_refinance_principal: 0.,
        investor_scheduled_principal: 0.,
        investor_turnover_principal: 0.,
        investor_refinance_principal: 0.,
        guarantee_principal_advance: 0.,
        guarantee_interest_advance: 0.,
        borrower_principal_conservation_error: 0.,
        investor_principal_conservation_error: 0.,
        calendar_mapping_verified: false,
        work_units: 0,
    };
    let mut probability = [0.; 7];
    probability[..3].copy_from_slice(&r.opening_probability);
    let mut borrower = r.opening_borrower_principal;
    let mut borrower_interest = r.opening_borrower_interest;
    let mut investor = r.opening_investor_principal;
    let mut investor_interest = r.opening_investor_interest;
    let mut note = r.current_note_rate;
    let mut payment = r.current_payment;
    let mut contractual = r.opening_contractual_principal;
    let mut burnout = r.opening_burnout_incentive_years;
    let mut previous = r.as_of_ordinal;
    let mut contract_interest = r.opening_contractual_interest;
    let mut matured = 0.;
    let mut reset_count = match &r.rate {
        MortgageRate::Fixed => 0,
        MortgageRate::Arm { terms, .. } => terms.prior_reset_count,
    };
    let mut receipt_interval_index = 0usize;
    for day in days {
        if let Some(check) = check.as_deref_mut() {
            check()?;
        }
        let interval_work = 2 + result.defaults.len();
        work(&mut result, interval_work)?;
        let p = path.range(..=previous).next_back().unwrap().1;
        let years = f64::from(day - previous) / 365.;
        let tau = actual_year_fraction(previous, day, r.day_count)?;
        contract_interest += contractual * note * tau;
        consumer::amount(contract_interest, "contractual accrued mortgage interest")?;
        let age = (previous - r.origination_ordinal) as u32;
        let incentive =
            (note - p.refinancing_market_rate - r.refinance.incentive_threshold).max(0.);
        let turnover =
            p.turnover_hazard_per_year * seasoning(age, r.refinance.turnover_seasoning_days);
        let refinance = r.refinance.incentive_slope
            * incentive
            * seasoning(age, r.refinance.refinance_seasoning_days)
            * (-r.refinance.burnout_sensitivity * burnout).exp();
        if !refinance.is_finite() || refinance > 100. {
            return Err("derived refinance intensity exceeds100/year; coefficients must be revised explicitly".into());
        }
        let mut hazards = p.credit_hazards_per_year;
        for (i, row) in hazards.iter_mut().enumerate() {
            row[4] = (turnover + refinance) * r.refinance.prepay_eligibility[i];
        }
        let mut live_probability = [0.; 7];
        live_probability[..3].copy_from_slice(&probability[..3]);
        let loan_operator_request = CreditOperatorRequest {
            schema: credit_operator::SCHEMA.into(),
            measure: measure(r),
            duration_years: years,
            transition_hazards_per_year: hazards,
            accrual_rates: [note * tau / years; 3],
        };
        let loan_operator = credit_operator::compile(&loan_operator_request)?;
        let loan = loan_operator.apply(&operator_state(
            r,
            live_probability,
            borrower,
            borrower_interest,
        ))?;
        let coupon = investor_coupon(r, note)?;
        let recognition_factors = match &r.ownership {
            MortgageOwnership::WholeLoan {
                recognized_accrual_fraction,
                ..
            } => *recognized_accrual_fraction,
            MortgageOwnership::Agency { .. } => [1.; 3],
        };
        let investor_operator_request = CreditOperatorRequest {
            schema: credit_operator::SCHEMA.into(),
            measure: measure(r),
            duration_years: years,
            transition_hazards_per_year: hazards,
            accrual_rates: recognition_factors.map(|f| coupon * f * tau / years),
        };
        let owner_operator = credit_operator::compile(&investor_operator_request)?;
        let owned = owner_operator.apply(&operator_state(
            r,
            live_probability,
            investor,
            investor_interest,
        ))?;
        if let Some(sink) = sink.as_deref_mut() {
            sink.interval(receipt::IntervalView {
                request: r,
                interval_index: receipt_interval_index,
                start_ordinal: previous,
                end_ordinal: day,
                path_start_ordinal: p.start_ordinal,
                duration_years_act365f: years,
                contractual_day_fraction: tau,
                note_rate_before_boundary: note,
                investor_coupon_before_boundary: coupon,
                payment_before_boundary: payment,
                reset_count_before_boundary: reset_count,
                burnout_before_interval: burnout,
                loan_operator_request: &loan_operator_request,
                investor_operator_request: &investor_operator_request,
                loan_transition: loan_operator.transition(),
                loan_earned_interest: loan_operator.earned_interest(),
                investor_transition: owner_operator.transition(),
                investor_earned_interest: owner_operator.earned_interest(),
                opening_probability: &live_probability,
                opening_borrower_principal: &borrower,
                opening_borrower_interest: &borrower_interest,
                opening_investor_principal: &investor,
                opening_investor_interest: &investor_interest,
                transported_loan: &loan,
                transported_investor: &owned,
                default_claim_start_index: result.defaults.len(),
                payment_is_boundary: dues.contains(&day),
                arm_reset_is_boundary: resets.contains_key(&day),
            })?;
        }
        let earned = investor
            .iter()
            .zip(owner_operator.earned_interest())
            .map(|(b, row)| b * row.iter().sum::<f64>())
            .sum::<f64>();
        if earned != 0. {
            result.recognition.push(ConsumerRecognition {
                origin_id: format!("mortgage-earned-{}", result.recognition.len()),
                start_ordinal: previous,
                end_ordinal: day,
                interest: earned,
            });
        }
        for (i, value) in probability.iter_mut().enumerate() {
            *value = loan.probability[i] + if i >= 3 { *value } else { 0. };
        }
        for i in 0..3 {
            borrower[i] = loan.principal[i];
            borrower_interest[i] = loan.accrued_interest[i];
            investor[i] = owned.principal[i];
            investor_interest[i] = owned.accrued_interest[i];
            let j = i + 3;
            if loan.principal[j] == 0.
                && loan.accrued_interest[j] == 0.
                && owned.principal[j] == 0.
                && owned.accrued_interest[j] == 0.
            {
                continue;
            }
            if result.defaults.len() >= MAX_DEFAULT_CLAIMS {
                return Err("mortgage defaults exceed4096retainedclaims".into());
            }
            let gross = loan.principal[j];
            let loss = gross * r.loss_given_default[i];
            let mut recovery = vec![];
            work(&mut result, r.complete_recovery_tails[i].len() + 1)?;
            for w in &r.complete_recovery_tails[i] {
                if let Some(check) = check.as_deref_mut() {
                    check()?;
                }
                let recovery_day = date_after(day, w.lag_days)?;
                let amount = (gross - loss) * w.weight;
                recovery.push(MortgageRecoveryAmount {
                    ordinal: recovery_day,
                    principal: amount,
                });
                cash(
                    &mut result.borrower_cashflows,
                    recovery_day,
                    "borrower_default_recovery",
                    amount,
                    0.,
                )?;
                if matches!(r.ownership, MortgageOwnership::WholeLoan { .. }) {
                    let receipt_start = result.entitlements.len();
                    entitlement(
                        r,
                        &mut result.entitlements,
                        recovery_day,
                        "whole_loan_default_recovery",
                        amount,
                        0.,
                    )?;
                    if let Some(sink) = sink.as_deref_mut() {
                        sink.recovery(receipt::RecoveryView {
                            claim_index: result.defaults.len(),
                            origin: i,
                            tail_index: recovery.len() - 1,
                            ordinal: recovery_day,
                            principal: amount,
                            entitlement_id: result
                                .entitlements
                                .get(receipt_start)
                                .map(|e| e.event_id.as_str()),
                        })?;
                    }
                }
            }
            let buyout = agency_buyout(
                r,
                &mut result,
                day,
                owned.principal[j],
                owned.accrued_interest[j],
                coupon,
                end,
            )?;
            result.defaults.push(MortgageDefaultClaim {
                claim_id: format!("mortgage-default-{}", result.defaults.len()),
                origin: i,
                default_ordinal: day,
                borrower_principal: gross,
                borrower_contract_interest: loan.accrued_interest[j],
                investor_principal: owned.principal[j],
                investor_accrued_interest: owned.accrued_interest[j],
                borrower_principal_loss: loss,
                complete_recovery_tail: recovery,
                investor_buyout_ordinal: buyout,
            });
        }
        let prepay = turnover + refinance;
        if prepay > 0. {
            for (kind, fraction) in [
                ("turnover", turnover / prepay),
                ("refinance", refinance / prepay),
            ] {
                let lb = loan.principal[6] * fraction;
                let li = loan.accrued_interest[6] * fraction;
                let ib = owned.principal[6] * fraction;
                let ii = if matches!(r.ownership, MortgageOwnership::WholeLoan { .. }) {
                    li
                } else {
                    owned.accrued_interest[6] * fraction
                };
                cash(&mut result.borrower_cashflows, day, kind, lb, li)?;
                let receipt_start = result.entitlements.len();
                entitlement(r, &mut result.entitlements, day, kind, ib, ii)?;
                if let Some(sink) = sink.as_deref_mut() {
                    sink.prepayment(receipt::PrepaymentView {
                        kind,
                        fraction,
                        borrower_principal: lb,
                        borrower_interest: li,
                        principal: ib,
                        interest: ii,
                        recognized_interest: owned.accrued_interest[6] * fraction,
                        entitlement_id: result
                            .entitlements
                            .get(receipt_start)
                            .map(|e| e.event_id.as_str()),
                    })?;
                }
                if matches!(r.ownership, MortgageOwnership::WholeLoan { .. }) {
                    let extra = li - owned.accrued_interest[6] * fraction;
                    if extra != 0. {
                        result.recognition.push(ConsumerRecognition {
                            origin_id: format!("nonaccrual-payoff-{}", result.recognition.len()),
                            start_ordinal: day,
                            end_ordinal: day,
                            interest: extra,
                        });
                    }
                }
                if kind == "turnover" {
                    result.borrower_turnover_principal += lb;
                    result.investor_turnover_principal += ib;
                } else {
                    result.borrower_refinance_principal += lb;
                    result.investor_refinance_principal += ib;
                }
            }
        }
        burnout += incentive * years;
        if burnout > 200. {
            return Err("retained mortgage burnout exceeds200incentive-years".into());
        }
        if dues.contains(&day) {
            if payment + 1e-9 < contract_interest {
                return Err(
                    "mortgage payment would cause unsupported negative amortization".into(),
                );
            }
            let final_due = day == *r.payment_dates.last().unwrap();
            let scheduled_principal = if final_due {
                contractual
            } else {
                (payment - contract_interest).min(contractual)
            };
            contractual -= scheduled_principal;
            for i in 0..3 {
                let available = if final_due {
                    (borrower[i] + borrower_interest[i]) * r.borrower_collection_fraction[i]
                } else {
                    payment * probability[i] * r.borrower_collection_fraction[i]
                };
                let actual_interest = available.min(borrower_interest[i]);
                let actual_principal = (available - actual_interest).min(borrower[i]);
                borrower[i] -= actual_principal;
                borrower_interest[i] -= actual_interest;
                cash(
                    &mut result.borrower_cashflows,
                    day,
                    "scheduled_borrower_collection",
                    actual_principal,
                    actual_interest,
                )?;
                let (owner_principal, owner_interest) =
                    if matches!(r.ownership, MortgageOwnership::Agency { .. }) {
                        let owner_p = if final_due {
                            investor[i]
                        } else {
                            (scheduled_principal * probability[i]).min(investor[i])
                        };
                        result.guarantee_principal_advance += (owner_p - actual_principal).max(0.);
                        result.guarantee_interest_advance +=
                            (investor_interest[i] - actual_interest).max(0.);
                        (owner_p, investor_interest[i])
                    } else {
                        (actual_principal, actual_interest)
                    };
                let recognized = owner_interest.min(investor_interest[i]);
                investor[i] -= owner_principal;
                investor_interest[i] -= recognized;
                let receipt_start = result.entitlements.len();
                entitlement(
                    r,
                    &mut result.entitlements,
                    day,
                    "scheduled_investor_entitlement",
                    owner_principal,
                    owner_interest,
                )?;
                if let Some(sink) = sink.as_deref_mut() {
                    sink.scheduled(receipt::ScheduledView {
                        origin: i,
                        borrower_principal: actual_principal,
                        borrower_interest: actual_interest,
                        principal: owner_principal,
                        interest: owner_interest,
                        recognized_interest: recognized,
                        entitlement_id: result
                            .entitlements
                            .get(receipt_start)
                            .map(|e| e.event_id.as_str()),
                    })?;
                }
                if matches!(r.ownership, MortgageOwnership::WholeLoan { .. })
                    && owner_interest > recognized
                {
                    result.recognition.push(ConsumerRecognition {
                        origin_id: format!("nonaccrual-collection-{}", result.recognition.len()),
                        start_ordinal: day,
                        end_ordinal: day,
                        interest: owner_interest - recognized,
                    });
                }
                result.borrower_scheduled_principal += actual_principal;
                result.investor_scheduled_principal += owner_principal;
                if final_due
                    && borrower[i] == 0.
                    && borrower_interest[i] == 0.
                    && investor[i] == 0.
                    && investor_interest[i] == 0.
                {
                    matured += probability[i];
                    probability[i] = 0.;
                }
            }
            contract_interest = 0.;
        }
        if let Some(reset) = resets.get(&day) {
            if let MortgageRate::Arm { terms, .. } = &r.rate {
                let (fixing, projected) = match &reset.input {
                    MortgageIndexInput::Observed { fixing, .. } => (fixing, false),
                    MortgageIndexInput::Projected { fixing, .. } => (fixing, true),
                };
                let previous_rate = note;
                let up = if reset_count == 0 {
                    terms.initial_up_cap
                } else {
                    terms.subsequent_up_cap
                };
                let down = if reset_count == 0 {
                    terms.initial_down_cap
                } else {
                    terms.subsequent_down_cap
                };
                let floor = (terms.original_note_rate - terms.lifetime_down_cap)
                    .max(terms.note_floor)
                    .max(note - down);
                let cap = (terms.original_note_rate + terms.lifetime_up_cap).min(note + up);
                if floor > cap {
                    return Err(
                        "ARM reset floor and periodic/lifetime caps do not intersect".into(),
                    );
                }
                note = (fixing.rate + terms.margin).clamp(floor, cap);
                investor_coupon(r, note)?;
                reset_count += 1;
                if terms.payment_recast == PaymentRecast::ReamortizeRemaining && contractual > 0. {
                    let remaining: Vec<_> = r
                        .payment_dates
                        .iter()
                        .copied()
                        .filter(|&d| d > day)
                        .collect();
                    if remaining.is_empty() {
                        return Err("ARM recast needs complete remaining payment dates".into());
                    }
                    let mut from = day;
                    let taus: Vec<_> = remaining
                        .iter()
                        .map(|&d| {
                            let value = actual_year_fraction(from, d, r.day_count);
                            from = d;
                            value
                        })
                        .collect::<Result<_, _>>()?;
                    let principals = crate::amortization::level_payment_principal(note, &taus)?;
                    // Existing accrued contract interest is due in the first remaining instalment.
                    let annuity_balance = contractual + contract_interest / (1. + note * taus[0]);
                    payment = annuity_balance * (principals[0] + note * taus[0]);
                } else if terms.payment_recast == PaymentRecast::RetainOriginal {
                    payment = r.original_payment;
                }
                result.resets.push(MortgageResetResult {
                    ordinal: day,
                    previous_rate,
                    selected_index_rate: fixing.rate,
                    new_rate: note,
                    new_payment: payment,
                    fixing_id: fixing.fixing_id.clone(),
                    fixing_revision: fixing.revision.clone(),
                    projected,
                });
            }
        }
        let borrower_default = result
            .defaults
            .iter()
            .map(|d| {
                d.borrower_principal
                    - d.complete_recovery_tail
                        .iter()
                        .filter(|p| p.ordinal <= day)
                        .map(|p| p.principal)
                        .sum::<f64>()
            })
            .sum::<f64>();
        let investor_default = if matches!(r.ownership, MortgageOwnership::WholeLoan { .. }) {
            borrower_default
        } else {
            result
                .defaults
                .iter()
                .filter(|d| d.investor_buyout_ordinal.is_some_and(|due| due > day))
                .map(|d| d.investor_principal)
                .sum()
        };
        for &x in borrower
            .iter()
            .chain(&borrower_interest)
            .chain(&investor)
            .chain(&investor_interest)
        {
            consumer::amount(x, "mortgage closing state")?;
        }
        signed(borrower_default)?;
        signed(investor_default)?;
        result.points.push(MortgagePoint {
            ordinal: day,
            seasoning_days: (day - r.origination_ordinal) as u32,
            burnout_incentive_years: burnout,
            note_rate: note,
            current_payment: payment,
            contractual_principal: contractual,
            probability,
            matured_probability: matured,
            borrower_principal: borrower,
            borrower_interest,
            investor_principal: investor,
            investor_interest,
            turnover_hazard_per_year: turnover,
            refinance_hazard_per_year: refinance,
            borrower_default_principal: borrower_default,
            investor_default_principal: investor_default,
        });
        if let Some(sink) = sink.as_deref_mut() {
            sink.boundary(receipt::BoundaryView {
                interval_index: receipt_interval_index,
                ordinal: day,
                default_claim_end_index: result.defaults.len(),
                closing_borrower_principal: &borrower,
                closing_borrower_interest: &borrower_interest,
                closing_investor_principal: &investor,
                closing_investor_interest: &investor_interest,
                closing_probability: &probability,
                matured_probability: matured,
                closing_contractual_principal: contractual,
                closing_contractual_interest: contract_interest,
                next_note_rate: note,
                next_payment: payment,
                next_reset_count: reset_count,
                next_burnout_incentive_years: burnout,
            })?;
        }
        receipt_interval_index += 1;
        previous = day;
    }
    result
        .borrower_cashflows
        .sort_by(|a, b| (a.ordinal, &a.event_id).cmp(&(b.ordinal, &b.event_id)));
    result.entitlements.sort_by(|a, b| {
        (a.entitlement_ordinal, &a.event_id).cmp(&(b.entitlement_ordinal, &b.event_id))
    });
    for e in &result.entitlements {
        if let Some(check) = check.as_deref_mut() {
            check()?;
        }
        let receipt_start = result.cashflows.len();
        cash(
            &mut result.cashflows,
            e.remittance_ordinal,
            &e.kind,
            e.principal,
            e.interest,
        )?;
        if let Some(sink) = sink.as_deref_mut() {
            let event = result
                .cashflows
                .get(receipt_start)
                .ok_or("native entitled amount did not produce cash")?;
            sink.cash_link(receipt::CashLinkView {
                entitlement_id: &e.event_id,
                cash_event_id: &event.event_id,
            })?;
        }
    }
    result
        .cashflows
        .sort_by(|a, b| (a.ordinal, &a.event_id).cmp(&(b.ordinal, &b.event_id)));
    let closing = result.points.last().unwrap();
    let borrower_collected = result
        .borrower_cashflows
        .iter()
        .filter(|e| e.ordinal <= end)
        .map(|e| e.principal)
        .sum::<f64>();
    let investor_entitled = result
        .entitlements
        .iter()
        .filter(|e| e.entitlement_ordinal <= end)
        .map(|e| e.principal)
        .sum::<f64>();
    result.borrower_principal_conservation_error = r.opening_borrower_principal.iter().sum::<f64>()
        - borrower_collected
        - closing.borrower_principal.iter().sum::<f64>()
        - closing.borrower_default_principal;
    result.investor_principal_conservation_error = r.opening_investor_principal.iter().sum::<f64>()
        - investor_entitled
        - closing.investor_principal.iter().sum::<f64>()
        - closing.investor_default_principal;
    if !near(
        r.opening_borrower_principal.iter().sum(),
        borrower_collected
            + closing.borrower_principal.iter().sum::<f64>()
            + closing.borrower_default_principal,
    ) || !near(
        r.opening_investor_principal.iter().sum(),
        investor_entitled
            + closing.investor_principal.iter().sum::<f64>()
            + closing.investor_default_principal,
    ) {
        return Err("mortgage principal collection/live/default conservation failed".into());
    }
    consumer::amount(
        result.guarantee_principal_advance,
        "guarantee principal advance",
    )?;
    consumer::amount(
        result.guarantee_interest_advance,
        "guarantee interest advance",
    )?;
    if let Some(check) = check {
        check()?;
    }
    if let Some(sink) = sink {
        sink.complete(&result)?;
    }
    Ok(result)
}
