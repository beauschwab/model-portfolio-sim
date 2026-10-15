//! Supplied-assumption daily physical deposit balances and offered rates.
//! One competing transition policy owns retention, migration, closure and withdrawal.
use crate::cashflows::dated::{
    actual_year_fraction, validate_ordinal, ActualDayCount, CashflowMeasure,
};
use crate::validation::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "deposit-state-1";
pub const MAX_SEGMENTS: usize = 16;
pub const MAX_WORK: usize = 250_000;
pub const MAX_DAYS: u32 = 36_600;
pub const MAX_ROW_INTENSITY: f64 = 100.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositIdentity {
    pub entity_id: String,
    pub cohort_id: String,
    pub currency: String,
    pub source_revision: String,
    pub model_version: String,
    pub assumption_id: String,
    pub assumption_rationale: String,
    pub run_id: String,
    pub measure: CashflowMeasure,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetRule {
    Linear {
        intercept: f64,
        beta: f64,
        competitor_weight: f64,
    },
    LogisticBeta {
        intercept: f64,
        beta_min: f64,
        beta_max: f64,
        midpoint: f64,
        steepness: f64,
        competitor_weight: f64,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepricingPolicy {
    pub target: TargetRule,
    pub observation_lag_days: u32,
    /// Zero means immediate adjustment; positive values are actual-calendar-day half lives.
    pub rising_half_life_days: f64,
    pub falling_half_life_days: f64,
    pub floor: f64,
    pub ceiling: f64,
    pub supported_observation_range: Option<ObservedRange>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedRange {
    pub history_revision: String,
    pub minimum_reference_rate: f64,
    pub maximum_reference_rate: f64,
    /// Caller declaration only; the model does not fit or prove a plateau from a range.
    pub logistic_plateau_identified: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentCharacteristics {
    pub uninsured_fraction: f64,
    pub concentration: f64,
    pub digital_fraction: f64,
    pub relationship_fraction: f64,
    pub operational_fraction: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    pub withdrawal_per_year: f64,
    pub closure_per_year: f64,
    pub opportunity_gap_sensitivity: f64,
    pub absolute_daily_velocity_sensitivity: f64,
    pub uninsured_stress_sensitivity: f64,
    pub concentration_stress_sensitivity: f64,
    pub digital_stress_sensitivity: f64,
    pub relationship_protection: f64,
    pub operational_protection: f64,
    pub closure_stress_sensitivity: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositSegment {
    pub segment_id: String,
    pub opening_balance: f64,
    pub opening_accrued_interest: f64,
    pub opening_paid_rate: f64,
    pub characteristics: SegmentCharacteristics,
    pub repricing: RepricingPolicy,
    pub retention: RetentionPolicy,
    /// An actual contractual maturity; None is a non-maturity deposit.
    pub maturity_ordinal: Option<i32>,
    /// Supplied end-of-day cash interest settlement dates; no monthly inference.
    pub interest_payment_ordinals: Vec<i32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationPolicy {
    pub from_segment: String,
    pub to_segment: String,
    pub base_per_year: f64,
    pub destination_rate_gap_sensitivity: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositDriver {
    pub ordinal: i32,
    pub reference_rate: f64,
    pub competitor_rates: Vec<f64>,
    pub offered_spread_stress: Vec<f64>,
    pub confidence_stress: Vec<f64>,
    pub external_inflows: Vec<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalPolicy {
    RetainResidual {},
    AssumedLiquidation {
        rationale: String,
    },
    FranchiseValueOnly {
        supplied_value: f64,
        rationale: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositStateRequest {
    pub schema: String,
    pub identity: DepositIdentity,
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
    pub day_count: ActualDayCount,
    pub segments: Vec<DepositSegment>,
    pub migrations: Vec<MigrationPolicy>,
    /// Contiguous observations from as_of-max_lag-1 through full horizon-1, inclusive.
    pub drivers: Vec<DepositDriver>,
    pub terminal: TerminalPolicy,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositCashEvent {
    pub event_id: String,
    pub ordinal: i32,
    pub segment_id: String,
    pub kind: String,
    /// Bank perspective: deposits received positive; repayments negative.
    pub principal: f64,
    pub cash_interest: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositMigration {
    pub ordinal: i32,
    pub from_segment: String,
    pub to_segment: String,
    pub principal: f64,
    pub accrued_interest: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositPoint {
    pub ordinal: i32,
    pub balances: Vec<f64>,
    pub accrued_interest: Vec<f64>,
    pub paid_rates: Vec<f64>,
    pub outside_supported_rate_regime: Vec<bool>,
    pub rate_regime_assessed: Vec<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositInterest {
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub segment_id: String,
    /// Positive deposit expense/right added; signed negative rates remain negative.
    pub amount: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepositStateResult {
    pub schema: String,
    pub identity: DepositIdentity,
    pub parameter_status: String,
    pub segment_ids: Vec<String>,
    pub points: Vec<DepositPoint>,
    pub cashflows: Vec<DepositCashEvent>,
    pub migrations: Vec<DepositMigration>,
    pub interest: Vec<DepositInterest>,
    pub total_external_inflows: f64,
    pub total_withdrawals: f64,
    pub total_closures: f64,
    pub total_contractual_maturities: f64,
    pub total_assumed_terminal_payout: f64,
    pub closing_balance: f64,
    pub closing_accrued_interest: f64,
    pub total_interest_expense: f64,
    pub total_bank_cash_interest: f64,
    pub supplied_franchise_value: Option<f64>,
}

use crate::products::deposits::principal::{emit, PrincipalAtom};

pub fn simulate(r: &DepositStateRequest) -> Result<DepositStateResult, String> {
    if r.identity.measure != CashflowMeasure::Physical {
        return Err(
            "deposit state is an explicitly physical policy, not a pricing-measure transformation"
                .into(),
        );
    }
    simulate_declared_measure(r)
}

/// Explicit supplied Physical/Pricing policies use the same state equations.
/// This entrypoint retains the declared measure and performs no P-to-Q conversion
/// or calibration. The original `simulate` boundary remains Physical-only.
pub fn simulate_declared_measure(r: &DepositStateRequest) -> Result<DepositStateResult, String> {
    simulate_inner(r, None, &mut || Ok(()))
}
pub fn capture_principal_with_check(
    r: &DepositStateRequest,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<(DepositStateResult, Vec<PrincipalAtom>), String> {
    if r.identity.measure != CashflowMeasure::Physical {
        return Err("capture requires physical".into());
    }
    // Cheap shape first. Do not scan an unsupported unbounded request.
    check()?;
    if r.horizon_days > 64
        || r.segments.len() != 1
        || !r.migrations.is_empty()
        || !matches!(r.terminal, TerminalPolicy::RetainResidual {})
        || r.segments[0].repricing.observation_lag_days > 366
        || r.drivers.len() > 64 + 366 + 1
    {
        return Err(
            "principal capture requires <=64day single-segment no-inflow/migration retained tail"
                .into(),
        );
    }
    for driver in &r.drivers {
        check()?;
        if driver.external_inflows.len() != 1
            || driver.competitor_rates.len() != 1
            || driver.offered_spread_stress.len() != 1
            || driver.confidence_stress.len() != 1
            || driver.external_inflows[0] != 0.
        {
            return Err("principal capture requires one declared channel and no-inflow".into());
        }
    }
    // Full economic validation occurs only once in simulate_inner below. The
    // source validator now receives checkpoints through its bounded driver scan.
    let slots = (r.horizon_days as usize)
        .checked_add(2)
        .ok_or("atom slots overflow")?;
    let mut receipts = Vec::new();
    receipts
        .try_reserve_exact(slots)
        .map_err(|_| "principal atom reservation failed")?;
    let result = simulate_inner(r, Some(&mut receipts), check)?;
    if receipts.len() > slots {
        return Err("principal atom capacity exceeded".into());
    }
    Ok((result, receipts))
}
/// Separately versioned counted owner uses the unchanged native loop and atom sink.
/// Historical capture admission remains64days and calls the historical entrypoint.
pub(crate) fn capture_principal_counted_with_check(
    r: &DepositStateRequest,
    maximum_days: u32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<(DepositStateResult, Vec<PrincipalAtom>), String> {
    check()?;
    if maximum_days > 365
        || r.horizon_days == 0
        || r.horizon_days > maximum_days
        || r.identity.measure != CashflowMeasure::Physical
        || r.segments.len() != 1
        || !r.migrations.is_empty()
        || !matches!(r.terminal, TerminalPolicy::RetainResidual {})
        || r.segments[0].repricing.observation_lag_days > 64
        || r.drivers.len() > maximum_days as usize + 64 + 1
    {
        return Err(
            "counted principal tier requires <=365day physical one-segment retained-tail, lag<=64"
                .into(),
        );
    }
    for d in &r.drivers {
        check()?;
        if d.external_inflows.len() != 1
            || d.external_inflows[0] != 0.
            || d.competitor_rates.len() != 1
            || d.offered_spread_stress.len() != 1
            || d.confidence_stress.len() != 1
        {
            return Err(
                "counted principal tier rejects inflow/migration/ambiguous channels".into(),
            );
        }
    }
    let slots = (r.horizon_days as usize)
        .checked_add(2)
        .ok_or("atom slots overflow")?;
    let mut atoms = Vec::new();
    atoms
        .try_reserve_exact(slots)
        .map_err(|_| "counted atom reserve failed")?;
    let result = simulate_inner(r, Some(&mut atoms), check)?;
    if atoms.len() > slots {
        return Err("counted atom capacity exceeded".into());
    }
    Ok((result, atoms))
}
fn simulate_inner(
    r: &DepositStateRequest,
    mut sink: Option<&mut Vec<PrincipalAtom>>,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<DepositStateResult, String> {
    let (end, first) = validate_with_check(r, check)?;
    let n = r.segments.len();
    let get = |day: i32| -> &DepositDriver { &r.drivers[(day - first) as usize] };
    let mut balance: Vec<_> = r.segments.iter().map(|s| s.opening_balance).collect();
    let mut accrued: Vec<_> = r
        .segments
        .iter()
        .map(|s| s.opening_accrued_interest)
        .collect();
    let mut rates: Vec<_> = r.segments.iter().map(|s| s.opening_paid_rate).collect();
    let ids: Vec<_> = r.segments.iter().map(|s| s.segment_id.clone()).collect();
    let mut migration_indices = vec![Vec::new(); n];
    for m in &r.migrations {
        let from = ids.iter().position(|v| v == &m.from_segment).unwrap();
        let to = ids.iter().position(|v| v == &m.to_segment).unwrap();
        migration_indices[from].push((to, m));
    }
    let mut cashflows = Vec::new();
    let mut movements = Vec::new();
    let mut interest = Vec::new();
    let mut points = Vec::new();
    let (mut inflows, mut withdrawals, mut closures, mut maturities, mut terminal_payout) =
        (0., 0., 0., 0., 0.);
    points.push(point(
        r.as_of_ordinal,
        &balance,
        &accrued,
        &rates,
        vec![false; n],
        vec![false; n],
    ));
    for day in r.as_of_ordinal..end {
        check()?;
        let driver = get(day);
        let active: Vec<_> = r
            .segments
            .iter()
            .map(|s| s.maturity_ordinal.is_none_or(|d| d > day))
            .collect();
        let mut outside = vec![false; n];
        let mut assessed = vec![false; n];
        for (i, s) in r.segments.iter().enumerate() {
            if s.maturity_ordinal == Some(day) {
                let atom_before = balance[i];
                let atom_cash_start = cashflows.len();
                maturities += balance[i];
                cash(
                    &mut cashflows,
                    day,
                    &s.segment_id,
                    "contractual_maturity",
                    -balance[i],
                    -accrued[i],
                )?;
                balance[i] = 0.;
                accrued[i] = 0.;
                emit(
                    &mut sink,
                    day,
                    s,
                    "contractual_maturity",
                    atom_before,
                    balance[i],
                    None,
                    None,
                    None,
                    &cashflows[atom_cash_start..],
                );
            }
            let observed = get(day - s.repricing.observation_lag_days as i32);
            let desired = (target(
                &s.repricing.target,
                observed.reference_rate,
                observed.competitor_rates[i],
            ) + observed.offered_spread_stress[i])
                .clamp(s.repricing.floor, s.repricing.ceiling);
            let half_life = if desired >= rates[i] {
                s.repricing.rising_half_life_days
            } else {
                s.repricing.falling_half_life_days
            };
            let adjustment = if half_life == 0. {
                1.
            } else {
                -(-std::f64::consts::LN_2 / half_life).exp_m1()
            };
            rates[i] = (rates[i] + adjustment * (desired - rates[i]))
                .clamp(s.repricing.floor, s.repricing.ceiling);
            if let Some(range) = &s.repricing.supported_observation_range {
                assessed[i] = true;
                outside[i] = observed.reference_rate < range.minimum_reference_rate
                    || observed.reference_rate > range.maximum_reference_rate;
            }
            if !active[i] && driver.external_inflows[i] != 0. {
                return Err("external inflow after contractual deposit maturity".into());
            }
        }
        // Simultaneous row allocation prevents newly migrated principal from migrating twice today.
        let mut next_balance = vec![0.; n];
        let mut next_accrued = vec![0.; n];
        for (i, s) in r.segments.iter().enumerate() {
            if !active[i] {
                continue;
            }
            let p = &s.retention;
            let c = &s.characteristics;
            let gap = (driver.competitor_rates[i] - rates[i]).max(0.);
            let velocity = (driver.reference_rate - get(day - 1).reference_rate).abs();
            let stress_exposure = p.uninsured_stress_sensitivity * c.uninsured_fraction
                + p.concentration_stress_sensitivity * c.concentration
                + p.digital_stress_sensitivity * c.digital_fraction
                - p.relationship_protection * c.relationship_fraction
                - p.operational_protection * c.operational_fraction;
            let w = (p.withdrawal_per_year
                + p.opportunity_gap_sensitivity * gap
                + p.absolute_daily_velocity_sensitivity * velocity
                + driver.confidence_stress[i] * stress_exposure)
                .max(0.);
            let close =
                p.closure_per_year + p.closure_stress_sensitivity * driver.confidence_stress[i];
            let mut to = vec![0.; n];
            for &(b, m) in &migration_indices[i] {
                if active[b] {
                    to[b] = m.base_per_year
                        + m.destination_rate_gap_sensitivity * (rates[b] - rates[i]).max(0.);
                }
            }
            let total = w + close + to.iter().sum::<f64>();
            if !total.is_finite() || total > MAX_ROW_INTENSITY {
                return Err("combined deposit intensity exceeds daily policy admission".into());
            }
            let exited = if total == 0. {
                0.
            } else {
                -(-total / 365.).exp_m1()
            };
            let share = if total == 0. { 0. } else { exited / total };
            let wd = balance[i] * w * share;
            let cl = balance[i] * close * share;
            let wi = accrued[i] * w * share;
            let ci = accrued[i] * close * share;
            let atom_before = balance[i];
            let atom_cash_start = cashflows.len();
            withdrawals += wd;
            closures += cl;
            cash(&mut cashflows, day, &s.segment_id, "withdrawal", -wd, -wi)?;
            cash(&mut cashflows, day, &s.segment_id, "closure", -cl, -ci)?;
            next_balance[i] += balance[i] * (1. - exited);
            next_accrued[i] += accrued[i] * (1. - exited);
            emit(
                &mut sink,
                day,
                s,
                "competing_withdrawal_closure",
                atom_before,
                next_balance[i],
                Some(exited),
                Some(w),
                Some(close),
                &cashflows[atom_cash_start..],
            );
            for (j, &hazard) in to.iter().enumerate() {
                if hazard != 0. {
                    let moved = balance[i] * hazard * share;
                    let moved_interest = accrued[i] * hazard * share;
                    next_balance[j] += moved;
                    next_accrued[j] += moved_interest;
                    if moved != 0. || moved_interest != 0. {
                        movements.push(DepositMigration {
                            ordinal: day,
                            from_segment: s.segment_id.clone(),
                            to_segment: r.segments[j].segment_id.clone(),
                            principal: moved,
                            accrued_interest: moved_interest,
                        });
                    }
                }
            }
        }
        for (i, s) in r.segments.iter().enumerate() {
            let added = driver.external_inflows[i];
            next_balance[i] += added;
            inflows += added;
            cash(
                &mut cashflows,
                day,
                &s.segment_id,
                "external_inflow",
                added,
                0.,
            )?;
            let earned =
                next_balance[i] * rates[i] * actual_year_fraction(day, day + 1, r.day_count)?;
            finite_money(earned, "daily deposit interest")?;
            next_accrued[i] += earned;
            if earned != 0. {
                interest.push(DepositInterest {
                    start_ordinal: day,
                    end_ordinal: day + 1,
                    segment_id: s.segment_id.clone(),
                    amount: earned,
                });
            }
            if s.interest_payment_ordinals
                .binary_search(&(day + 1))
                .is_ok()
            {
                cash(
                    &mut cashflows,
                    day + 1,
                    &s.segment_id,
                    "interest_payment",
                    0.,
                    -next_accrued[i],
                )?;
                next_accrued[i] = 0.;
            }
        }
        balance = next_balance;
        accrued = next_accrued;
        validate_vectors(&balance, &accrued)?;
        for value in [inflows, withdrawals, closures, maturities] {
            nonnegative(value, "cumulative deposit movement")?;
        }
        conservation(
            r,
            &balance,
            inflows,
            withdrawals,
            closures,
            maturities,
            terminal_payout,
        )?;
        points.push(point(
            day + 1,
            &balance,
            &accrued,
            &rates,
            outside,
            assessed,
        ));
    }
    // True maturity at closing is distinct from an assumed horizon liquidation.
    for (i, s) in r.segments.iter().enumerate() {
        if s.maturity_ordinal == Some(end) {
            let atom_before = balance[i];
            let atom_cash_start = cashflows.len();
            maturities += balance[i];
            cash(
                &mut cashflows,
                end,
                &s.segment_id,
                "contractual_maturity",
                -balance[i],
                -accrued[i],
            )?;
            balance[i] = 0.;
            accrued[i] = 0.;
            emit(
                &mut sink,
                end,
                s,
                "contractual_maturity",
                atom_before,
                balance[i],
                None,
                None,
                None,
                &cashflows[atom_cash_start..],
            );
        }
    }
    if matches!(r.terminal, TerminalPolicy::AssumedLiquidation { .. }) {
        for (i, s) in r.segments.iter().enumerate() {
            terminal_payout += balance[i];
            cash(
                &mut cashflows,
                end,
                &s.segment_id,
                "assumed_terminal_payout",
                -balance[i],
                -accrued[i],
            )?;
            balance[i] = 0.;
            accrued[i] = 0.;
        }
    }
    if let Some(last) = points.last_mut() {
        last.balances = balance.clone();
        last.accrued_interest = accrued.clone();
    }
    conservation(
        r,
        &balance,
        inflows,
        withdrawals,
        closures,
        maturities,
        terminal_payout,
    )?;
    let total_interest_expense = interest.iter().map(|v| v.amount).sum::<f64>();
    let total_bank_cash_interest = cashflows.iter().map(|v| v.cash_interest).sum::<f64>();
    finite_money(
        interest.iter().map(|v| v.amount.abs()).sum(),
        "gross interest expense",
    )?;
    finite_money(
        cashflows.iter().map(|v| v.cash_interest.abs()).sum(),
        "gross cash interest",
    )?;
    let opening_interest = r
        .segments
        .iter()
        .map(|s| s.opening_accrued_interest)
        .sum::<f64>();
    let closing_interest = accrued.iter().sum::<f64>();
    let scale = r
        .segments
        .iter()
        .map(|s| s.opening_accrued_interest.abs())
        .sum::<f64>()
        + interest.iter().map(|v| v.amount.abs()).sum::<f64>()
        + cashflows.iter().map(|v| v.cash_interest.abs()).sum::<f64>();
    if (opening_interest + total_interest_expense + total_bank_cash_interest - closing_interest)
        .abs()
        > 1e-8_f64.max(scale * 1e-12)
    {
        return Err("deposit interest-right conservation failed".into());
    }
    check()?;
    cashflows.sort_by_key(|v| v.ordinal);
    Ok(DepositStateResult {
        schema: SCHEMA.into(),
        identity: r.identity.clone(),
        parameter_status: "supplied_assumption".into(),
        segment_ids: ids,
        points,
        cashflows,
        migrations: movements,
        interest,
        total_external_inflows: inflows,
        total_withdrawals: withdrawals,
        total_closures: closures,
        total_contractual_maturities: maturities,
        total_assumed_terminal_payout: terminal_payout,
        closing_balance: balance.iter().sum(),
        closing_accrued_interest: accrued.iter().sum(),
        total_interest_expense,
        total_bank_cash_interest,
        supplied_franchise_value: match r.terminal {
            TerminalPolicy::FranchiseValueOnly { supplied_value, .. } => Some(supplied_value),
            _ => None,
        },
    })
}

/// Full supplied P/Q request-domain admission, without generating state rows.
pub fn validate_declared_request(r: &DepositStateRequest) -> Result<(), String> {
    validate(r).map(|_| ())
}

fn target(rule: &TargetRule, reference: f64, competitor: f64) -> f64 {
    let (intercept, beta, weight) = match *rule {
        TargetRule::Linear {
            intercept,
            beta,
            competitor_weight,
        } => (intercept, beta, competitor_weight),
        TargetRule::LogisticBeta {
            intercept,
            beta_min,
            beta_max,
            midpoint,
            steepness,
            competitor_weight,
        } => {
            let x = steepness * (reference - midpoint);
            let sigmoid = if x >= 0. {
                1. / (1. + (-x).exp())
            } else {
                x.exp() / (1. + x.exp())
            };
            (
                intercept,
                beta_min + (beta_max - beta_min) * sigmoid,
                competitor_weight,
            )
        }
    };
    intercept + beta * reference + weight * (competitor - reference)
}
fn point(
    day: i32,
    b: &[f64],
    a: &[f64],
    rates: &[f64],
    outside: Vec<bool>,
    assessed: Vec<bool>,
) -> DepositPoint {
    DepositPoint {
        ordinal: day,
        balances: b.to_vec(),
        accrued_interest: a.to_vec(),
        paid_rates: rates.to_vec(),
        outside_supported_rate_regime: outside,
        rate_regime_assessed: assessed,
    }
}
fn cash(
    events: &mut Vec<DepositCashEvent>,
    day: i32,
    segment: &str,
    kind: &str,
    principal: f64,
    interest: f64,
) -> Result<(), String> {
    finite_money(principal, "deposit cash principal")?;
    finite_money(interest, "deposit cash interest")?;
    finite_money(principal + interest, "deposit cash total")?;
    if principal != 0. || interest != 0. {
        events.push(DepositCashEvent {
            event_id: format!("{segment}-{kind}-{day}"),
            ordinal: day,
            segment_id: segment.into(),
            kind: kind.into(),
            principal,
            cash_interest: interest,
        });
    }
    Ok(())
}
fn conservation(
    r: &DepositStateRequest,
    b: &[f64],
    inflows: f64,
    w: f64,
    c: f64,
    m: f64,
    t: f64,
) -> Result<(), String> {
    let opening = r.segments.iter().map(|s| s.opening_balance).sum::<f64>();
    let residual = opening + inflows - w - c - m - t - b.iter().sum::<f64>();
    if residual.abs() > 1e-8_f64.max((opening + inflows) * 1e-12) {
        return Err("deposit principal conservation failed".into());
    }
    Ok(())
}
fn validate_vectors(b: &[f64], a: &[f64]) -> Result<(), String> {
    for &v in b {
        nonnegative(v, "deposit balance")?;
    }
    for &v in a {
        finite_money(v, "deposit accrued interest")?;
    }
    nonnegative(b.iter().sum(), "total deposit balance")?;
    finite_money(a.iter().map(|v| v.abs()).sum(), "gross accrued rights")
}
fn finite_money(v: f64, name: &str) -> Result<(), String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        return Err(format!("{name} exceeds finite monetary admission"));
    }
    Ok(())
}
fn nonnegative(v: f64, name: &str) -> Result<(), String> {
    finite_money(v, name)?;
    if v < 0. {
        return Err(format!("{name} must be nonnegative"));
    }
    Ok(())
}
fn range(v: f64, min: f64, max: f64, name: &str) -> Result<(), String> {
    if !v.is_finite() || v < min || v > max {
        return Err(format!("{name} outside [{min},{max}]"));
    }
    Ok(())
}
fn label(v: &str, max: usize) -> Result<(), String> {
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err("deposit identity/rationale must be bounded nonblank text".into());
    }
    Ok(())
}

fn validate(r: &DepositStateRequest) -> Result<(i32, i32), String> {
    validate_with_check(r, &mut || Ok(()))
}
fn validate_with_check(
    r: &DepositStateRequest,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<(i32, i32), String> {
    check()?;
    if r.schema != SCHEMA || r.identity.model_version != SCHEMA {
        return Err("deposit schema/model version mismatch".into());
    }
    if !matches!(
        r.identity.measure,
        CashflowMeasure::Physical | CashflowMeasure::Pricing
    ) {
        return Err(
            "deposit state requires an explicit supplied physical or pricing measure".into(),
        );
    }
    for v in [
        &r.identity.entity_id,
        &r.identity.cohort_id,
        &r.identity.currency,
        &r.identity.source_revision,
        &r.identity.model_version,
        &r.identity.assumption_id,
        &r.identity.run_id,
    ] {
        label(v, 256)?;
    }
    label(&r.identity.assumption_rationale, 1024)?;
    validate_ordinal(r.as_of_ordinal)?;
    if r.horizon_days > MAX_DAYS {
        return Err("deposit horizon exceeds 36600 days".into());
    }
    let end = r
        .as_of_ordinal
        .checked_add(r.horizon_days as i32)
        .ok_or("deposit horizon overflow")?;
    validate_ordinal(end)?;
    if !matches!(
        r.day_count,
        ActualDayCount::Act360 | ActualDayCount::Act365Fixed
    ) {
        return Err("deposit interest requires explicit ACT360 or ACT365F".into());
    }
    let n = r.segments.len();
    if n == 0 || n > MAX_SEGMENTS || r.migrations.len() > n * (n - 1) {
        return Err("deposit segment/migration admission exceeded".into());
    }
    if (r.horizon_days as usize + 1)
        .checked_mul(n * n + 6 * n)
        .is_none_or(|v| v > MAX_WORK)
    {
        return Err("deposit daily work admission exceeded".into());
    }
    let mut names = HashSet::new();
    let mut max_lag = 0;
    for s in &r.segments {
        label(&s.segment_id, 64)?;
        if !names.insert(&s.segment_id) {
            return Err("duplicate deposit segment identity".into());
        }
        nonnegative(s.opening_balance, "opening deposit balance")?;
        finite_money(s.opening_accrued_interest, "opening accrued interest")?;
        let p = &s.repricing;
        range(p.floor, -0.2, 2., "paid floor")?;
        range(p.ceiling, p.floor, 2., "paid ceiling")?;
        range(s.opening_paid_rate, p.floor, p.ceiling, "opening paid rate")?;
        for v in [p.rising_half_life_days, p.falling_half_life_days] {
            range(v, 0., 36600., "adjustment half life")?;
        }
        if p.observation_lag_days > 366 {
            return Err("deposit observation lag exceeds 366 days".into());
        }
        max_lag = max_lag.max(p.observation_lag_days);
        let (intercept, weight) = match p.target {
            TargetRule::Linear {
                intercept,
                beta,
                competitor_weight,
            } => {
                range(beta, 0., 2., "beta")?;
                (intercept, competitor_weight)
            }
            TargetRule::LogisticBeta {
                intercept,
                beta_min,
                beta_max,
                midpoint,
                steepness,
                competitor_weight,
            } => {
                range(beta_min, 0., 2., "minimum beta")?;
                range(beta_max, beta_min, 2., "maximum beta")?;
                range(midpoint, -0.2, 2., "beta midpoint")?;
                range(steepness, 0., 10000., "beta steepness")?;
                (intercept, competitor_weight)
            }
        };
        range(intercept, -2., 2., "rate intercept")?;
        range(weight, 0., 2., "competitor weight")?;
        if let Some(v) = &p.supported_observation_range {
            label(&v.history_revision, 256)?;
            range(v.minimum_reference_rate, -0.2, 2., "observed minimum rate")?;
            range(
                v.maximum_reference_rate,
                v.minimum_reference_rate,
                2.,
                "observed maximum rate",
            )?;
        }
        let c = &s.characteristics;
        for v in [
            c.uninsured_fraction,
            c.concentration,
            c.digital_fraction,
            c.relationship_fraction,
            c.operational_fraction,
        ] {
            range(v, 0., 1., "deposit characteristic")?;
        }
        let h = &s.retention;
        for v in [
            h.withdrawal_per_year,
            h.closure_per_year,
            h.opportunity_gap_sensitivity,
            h.absolute_daily_velocity_sensitivity,
            h.uninsured_stress_sensitivity,
            h.concentration_stress_sensitivity,
            h.digital_stress_sensitivity,
            h.relationship_protection,
            h.operational_protection,
            h.closure_stress_sensitivity,
        ] {
            range(v, 0., 10000., "retention coefficient")?;
        }
        if let Some(maturity) = s.maturity_ordinal {
            validate_ordinal(maturity)?;
            if maturity < r.as_of_ordinal || maturity > end {
                return Err("contractual maturity outside full deposit horizon".into());
            }
        }
        if s.interest_payment_ordinals.len() > r.horizon_days as usize {
            return Err("interest settlement date admission exceeded".into());
        }
        let mut previous = r.as_of_ordinal;
        for &d in &s.interest_payment_ordinals {
            validate_ordinal(d)?;
            if d <= previous || d > end {
                return Err("interest settlement dates must be strict and within horizon".into());
            }
            previous = d;
        }
    }
    validate_vectors(
        &r.segments
            .iter()
            .map(|s| s.opening_balance)
            .collect::<Vec<_>>(),
        &r.segments
            .iter()
            .map(|s| s.opening_accrued_interest)
            .collect::<Vec<_>>(),
    )?;
    let mut pairs = HashSet::new();
    for m in &r.migrations {
        if m.from_segment == m.to_segment
            || !names.contains(&m.from_segment)
            || !names.contains(&m.to_segment)
            || !pairs.insert((&m.from_segment, &m.to_segment))
        {
            return Err("migration must name distinct declared segments once".into());
        }
        range(m.base_per_year, 0., 100., "migration intensity")?;
        range(
            m.destination_rate_gap_sensitivity,
            0.,
            10000.,
            "migration sensitivity",
        )?;
    }
    let first = r
        .as_of_ordinal
        .checked_sub(max_lag as i32 + 1)
        .ok_or("deposit observation history underflow")?;
    validate_ordinal(first)?;
    let count = (end - first) as usize;
    if r.drivers.len() != count {
        return Err("missing complete deposit driver/lag history coverage".into());
    }
    for (j, d) in r.drivers.iter().enumerate() {
        check()?;
        if d.ordinal != first + j as i32 {
            return Err("deposit drivers require contiguous ordered exact dates".into());
        }
        range(d.reference_rate, -0.2, 2., "reference rate")?;
        if [
            d.competitor_rates.len(),
            d.offered_spread_stress.len(),
            d.confidence_stress.len(),
            d.external_inflows.len(),
        ]
        .iter()
        .any(|&v| v != n)
        {
            return Err("deposit driver segment dimensions mismatch".into());
        }
        for &v in &d.competitor_rates {
            range(v, -0.2, 2., "competitor rate")?;
        }
        for &v in &d.offered_spread_stress {
            range(v, -2., 2., "offered spread stress")?;
        }
        for &v in &d.confidence_stress {
            range(v, 0., 100., "confidence stress")?;
        }
        for &v in &d.external_inflows {
            nonnegative(v, "external deposit inflow")?;
            if d.ordinal < r.as_of_ordinal && v != 0. {
                return Err("history observations cannot invent past inflow replay".into());
            }
        }
    }
    match &r.terminal {
        TerminalPolicy::RetainResidual {} => {}
        TerminalPolicy::AssumedLiquidation { rationale } => label(rationale, 1024)?,
        TerminalPolicy::FranchiseValueOnly {
            supplied_value,
            rationale,
        } => {
            finite_money(*supplied_value, "supplied franchise value")?;
            label(rationale, 1024)?;
        }
    }
    Ok((end, first))
}
