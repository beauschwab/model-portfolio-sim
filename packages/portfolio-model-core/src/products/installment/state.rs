//! Fixed note-rate daily-simple-interest auto and unsecured installment paths.
use crate::cashflows::dated::{actual_year_fraction, validate_ordinal, ActualDayCount};
use crate::products::consumer::contracts::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "consumer-installment-1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallmentProduct {
    Auto {
        new_vehicle: bool,
        vintage_ordinal: i32,
        original_ltv: f64,
        channel: String,
        collateral_value_as_of: f64,
        annual_depreciation: f64,
        sale_haircut: f64,
        repossession_cost: f64,
        recovery_delay_days: u32,
    },
    Personal {
        vintage_ordinal: i32,
        dti: f64,
        dti_definition: String,
        channel: String,
        collection_fraction: f64,
        collection_cost: f64,
        recovery_delay_days: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallmentAction {
    Payment { amount: f64 },
    Payoff {},
    SetState { state: ConsumerState },
    Default {},
    ChangePayment { amount: f64 },
    Fee { amount: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallmentStep {
    pub ordinal: i32,
    pub action: InstallmentAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallmentRequest {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub product: InstallmentProduct,
    pub as_of_ordinal: i32,
    pub horizon_days: u32,
    pub original_principal: f64,
    pub opening_principal: f64,
    pub opening_accrued_interest: f64,
    pub opening_unrecognized_interest: f64,
    pub opening_fees: f64,
    pub initial_state: ConsumerState,
    /// Contractual rate used for accrual. Disclosed APR includes a different fee basis.
    pub note_rate: f64,
    pub disclosed_apr: f64,
    pub day_count: ActualDayCount,
    pub original_payment: f64,
    pub payment_dates: Vec<i32>,
    pub prepayment_penalty_fraction: f64,
    /// Explicit recognition policy: if false, nonaccrual contract interest is also suspended.
    pub accrue_contract_interest_in_nonaccrual: bool,
    /// Exactly one explicit action per date. A payment/payoff replaces the scheduled payment.
    pub steps: Vec<InstallmentStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallmentPoint {
    pub ordinal: i32,
    pub state: ConsumerState,
    pub principal: f64,
    pub accrued_interest: f64,
    pub unrecognized_interest: f64,
    pub fees: f64,
    pub scheduled_payment: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallmentResult {
    pub schema: String,
    pub identity: ConsumerIdentity,
    pub original_principal: f64,
    pub original_payment: f64,
    pub disclosed_apr: f64,
    pub payment_dates: Vec<i32>,
    pub cashflows: Vec<ConsumerCashEvent>,
    pub recognition: Vec<ConsumerRecognition>,
    pub points: Vec<InstallmentPoint>,
    pub default: Option<ConsumerDefault>,
    pub principal_collected: f64,
    pub closing_principal: f64,
    pub closing_accrued_interest: f64,
    pub closing_unrecognized_interest: f64,
    pub closing_fees: f64,
}

pub fn project(r: &InstallmentRequest) -> Result<InstallmentResult, String> {
    validate(r)?;
    let end = horizon(r.as_of_ordinal, r.horizon_days)?;
    let steps: BTreeMap<_, _> = r.steps.iter().map(|s| (s.ordinal, &s.action)).collect();
    let dues: BTreeSet<_> = r.payment_dates.iter().copied().collect();
    let days: BTreeSet<_> = dues
        .iter()
        .copied()
        .chain(steps.keys().copied())
        .chain([end])
        .collect();
    let mut balance = r.opening_principal;
    let mut interest = r.opening_accrued_interest;
    let mut unrecognized = r.opening_unrecognized_interest;
    let mut fees = r.opening_fees;
    let mut state = r.initial_state;
    let mut payment = r.original_payment;
    let mut previous = r.as_of_ordinal;
    let mut collected = 0.0;
    let mut cashflows = Vec::new();
    let mut recognition = Vec::new();
    let mut points = Vec::new();
    let mut default = None;
    for day in days {
        if day > previous
            && balance > 0.0
            && !matches!(state, ConsumerState::Default | ConsumerState::PaidOff)
        {
            let earned = balance * r.note_rate * actual_year_fraction(previous, day, r.day_count)?;
            amount(earned, "interval interest")?;
            if state == ConsumerState::Nonaccrual {
                if r.accrue_contract_interest_in_nonaccrual {
                    unrecognized += earned;
                }
            } else {
                interest += earned;
                if earned != 0.0 {
                    recognition.push(ConsumerRecognition {
                        origin_id: format!("interest-{}", recognition.len()),
                        start_ordinal: previous,
                        end_ordinal: day,
                        interest: earned,
                    });
                }
            }
        }
        previous = day;
        amount(
            balance + interest + unrecognized + fees,
            "total installment obligation",
        )?;
        let action = steps.get(&day).copied();
        if matches!(state, ConsumerState::Default | ConsumerState::PaidOff) && action.is_some() {
            return Err("action after terminal consumer exit".into());
        }
        let mut cash_payment = None;
        let mut payoff = false;
        match action {
            Some(InstallmentAction::Default {}) => {
                let (gross, cost, delay) = recovery(r, day, balance)?;
                let net = (gross - cost).max(0.0).min(balance);
                let recovery_date = day
                    .checked_add(delay as i32)
                    .ok_or("recovery date overflow")?;
                if recovery_date > end {
                    return Err("full installment horizon must retain recovery tail".into());
                }
                default = Some(ConsumerDefault {
                    ordinal: day,
                    principal: balance,
                    accrued_interest: interest + unrecognized,
                    recognized_accrued_interest: interest,
                    unrecognized_contract_interest: unrecognized,
                    fees,
                    gross_recovery: gross,
                    recovery_cost: cost,
                    net_recovery: net,
                    principal_loss: balance - net,
                    recovery_ordinal: recovery_date,
                });
                let unreimbursed_cost = (cost - gross).max(0.0);
                if net != 0.0 || unreimbursed_cost != 0.0 {
                    cashflows.push(ConsumerCashEvent {
                        event_id: "recovery-0".into(),
                        ordinal: recovery_date,
                        kind: "recovery".into(),
                        principal: net,
                        interest: 0.0,
                        fees: -unreimbursed_cost,
                    });
                }
                balance = 0.0;
                interest = 0.0;
                unrecognized = 0.0;
                fees = 0.0;
                state = ConsumerState::Default;
            }
            Some(InstallmentAction::Payoff {}) => {
                cash_payment = Some(balance + interest + unrecognized + fees);
                payoff = true;
            }
            Some(InstallmentAction::Payment { amount }) => cash_payment = Some(*amount),
            Some(InstallmentAction::SetState { state: next }) => {
                state = *next;
            }
            Some(InstallmentAction::ChangePayment { amount }) => {
                payment = *amount;
            }
            Some(InstallmentAction::Fee { amount }) => {
                fees += amount;
            }
            None => {}
        }
        if cash_payment.is_none() && dues.contains(&day) && state == ConsumerState::Performing {
            cash_payment = Some(if Some(&day) == r.payment_dates.last() {
                balance + interest + unrecognized + fees
            } else {
                payment.min(balance + interest + unrecognized + fees)
            });
        }
        if let Some(cash) = cash_payment {
            let debt = balance + interest + unrecognized + fees;
            if cash > debt + 1e-9 || (debt == 0.0 && cash != 0.0) {
                return Err(
                    "payment exceeds outstanding payoff; credit balance unsupported".into(),
                );
            }
            let paid_fees = cash.min(fees);
            fees -= paid_fees;
            let rest = (cash - paid_fees).max(0.0);
            let paid_interest = rest.min(interest);
            interest -= paid_interest;
            let rest = (rest - paid_interest).max(0.0);
            let paid_unrecognized = rest.min(unrecognized);
            unrecognized -= paid_unrecognized;
            if paid_unrecognized != 0.0 {
                recognition.push(ConsumerRecognition {
                    origin_id: format!("cash-basis-{}", recognition.len()),
                    start_ordinal: day,
                    end_ordinal: day,
                    interest: paid_unrecognized,
                });
            }
            let principal = (rest - paid_unrecognized).max(0.0).min(balance);
            balance -= principal;
            collected += principal;
            let penalty = if payoff {
                principal * r.prepayment_penalty_fraction
            } else {
                0.0
            };
            amount(penalty, "payoff penalty")?;
            amount(cash + penalty, "cash payment plus penalty")?;
            if cash + penalty > 0.0 {
                cashflows.push(ConsumerCashEvent {
                    event_id: format!("payment-{}", cashflows.len()),
                    ordinal: day,
                    kind: if payoff { "prepayment" } else { "payment" }.into(),
                    principal,
                    interest: paid_interest + paid_unrecognized,
                    fees: paid_fees + penalty,
                });
            }
            if balance == 0.0 && interest + unrecognized + fees == 0.0 {
                state = ConsumerState::PaidOff;
            }
        }
        for (v, name) in [
            (balance, "balance"),
            (interest, "accrued interest"),
            (unrecognized, "unrecognized interest"),
            (fees, "fees"),
        ] {
            amount(v, name)?;
        }
        amount(
            balance + interest + unrecognized + fees,
            "closing installment obligation",
        )?;
        amount(collected, "cumulative installment principal payments")?;
        points.push(InstallmentPoint {
            ordinal: day,
            state,
            principal: balance,
            accrued_interest: interest,
            unrecognized_interest: unrecognized,
            fees,
            scheduled_payment: payment,
        });
    }
    cashflows.sort_by_key(|v| v.ordinal);
    let default_principal = default.as_ref().map_or(0.0, |v| v.principal);
    if (r.opening_principal - collected - balance - default_principal).abs()
        > 1e-8_f64.max(r.opening_principal * 1e-12)
    {
        return Err("installment principal conservation failed".into());
    }
    Ok(InstallmentResult {
        schema: SCHEMA.into(),
        identity: r.identity.clone(),
        original_principal: r.original_principal,
        original_payment: r.original_payment,
        disclosed_apr: r.disclosed_apr,
        payment_dates: r.payment_dates.clone(),
        cashflows,
        recognition,
        points,
        default,
        principal_collected: collected,
        closing_principal: balance,
        closing_accrued_interest: interest,
        closing_unrecognized_interest: unrecognized,
        closing_fees: fees,
    })
}

fn recovery(r: &InstallmentRequest, day: i32, balance: f64) -> Result<(f64, f64, u32), String> {
    let out = match &r.product {
        InstallmentProduct::Auto {
            collateral_value_as_of,
            annual_depreciation,
            sale_haircut,
            repossession_cost,
            recovery_delay_days,
            ..
        } => {
            let sale_day = day
                .checked_add(*recovery_delay_days as i32)
                .ok_or("recovery date overflow")?;
            let years = (sale_day - r.as_of_ordinal) as f64 / 365.0;
            (
                collateral_value_as_of
                    * (1.0 - annual_depreciation).powf(years)
                    * (1.0 - sale_haircut),
                *repossession_cost,
                *recovery_delay_days,
            )
        }
        InstallmentProduct::Personal {
            collection_fraction,
            collection_cost,
            recovery_delay_days,
            ..
        } => (
            balance * collection_fraction,
            *collection_cost,
            *recovery_delay_days,
        ),
    };
    amount(out.0, "gross recovery")?;
    Ok(out)
}

/// Validate a supplied history before an aggregate consumer begins projection.
pub fn validate_request(r: &InstallmentRequest) -> Result<(), String> {
    validate(r)
}

fn validate(r: &InstallmentRequest) -> Result<(), String> {
    if r.schema != SCHEMA {
        return Err("installment schema mismatch".into());
    }
    identity(&r.identity, SCHEMA)?;
    let end = horizon(r.as_of_ordinal, r.horizon_days)?;
    day_count(r.day_count)?;
    rate(r.note_rate)?;
    rate(r.disclosed_apr)?;
    for (v, name) in [
        (r.original_principal, "original principal"),
        (r.opening_principal, "opening principal"),
        (r.opening_accrued_interest, "opening interest"),
        (
            r.opening_unrecognized_interest,
            "opening unrecognized interest",
        ),
        (r.opening_fees, "opening fees"),
        (r.original_payment, "original payment"),
    ] {
        amount(v, name)?;
    }
    if r.opening_principal > r.original_principal
        || r.original_principal == 0.0
        || r.original_payment == 0.0
    {
        return Err("installment principal/payment lineage invalid".into());
    }
    amount(
        r.opening_principal
            + r.opening_accrued_interest
            + r.opening_unrecognized_interest
            + r.opening_fees,
        "opening installment obligation",
    )?;
    if matches!(
        r.initial_state,
        ConsumerState::Default | ConsumerState::PaidOff
    ) {
        return Err("initial installment state must be live".into());
    }
    fraction(r.prepayment_penalty_fraction, "prepayment penalty")?;
    if r.payment_dates.is_empty()
        || r.payment_dates.len() > MAX_CONSUMER_EVENTS
        || r.steps.len() > MAX_CONSUMER_EVENTS
        || r.payment_dates.len() + r.steps.len() > MAX_CONSUMER_EVENTS
    {
        return Err("installment event admission exceeded".into());
    }
    let mut prev = r.as_of_ordinal;
    for &d in &r.payment_dates {
        validate_ordinal(d)?;
        if d <= prev || d > end {
            return Err(
                "payment dates must be strictly ordered after valuation and within full horizon"
                    .into(),
            );
        }
        prev = d;
    }
    let mut seen = BTreeSet::new();
    for s in &r.steps {
        validate_ordinal(s.ordinal)?;
        if s.ordinal < r.as_of_ordinal || s.ordinal > end || !seen.insert(s.ordinal) {
            return Err("actions require unique dates within full horizon".into());
        }
        match &s.action {
            InstallmentAction::Payment { amount: v }
            | InstallmentAction::ChangePayment { amount: v }
            | InstallmentAction::Fee { amount: v } => amount(*v, "action amount")?,
            InstallmentAction::SetState {
                state: ConsumerState::Default | ConsumerState::PaidOff,
            } => return Err("terminal states require economic default/payoff action".into()),
            _ => {}
        }
    }
    match &r.product {
        InstallmentProduct::Auto {
            vintage_ordinal,
            original_ltv,
            channel,
            collateral_value_as_of,
            annual_depreciation,
            sale_haircut,
            repossession_cost,
            recovery_delay_days,
            ..
        } => {
            vintage(*vintage_ordinal, r.as_of_ordinal)?;
            label(channel)?;
            if !original_ltv.is_finite()
                || !(0.0..=10.0).contains(original_ltv)
                || *original_ltv == 0.0
            {
                return Err("original auto LTV must be supplied in (0,10]".into());
            }
            amount(*collateral_value_as_of, "collateral")?;
            amount(*repossession_cost, "repossession cost")?;
            fraction(*annual_depreciation, "depreciation")?;
            fraction(*sale_haircut, "sale haircut")?;
            if *recovery_delay_days > MAX_CONSUMER_DAYS {
                return Err("recovery delay exceeds admission".into());
            }
        }
        InstallmentProduct::Personal {
            vintage_ordinal,
            dti,
            dti_definition,
            channel,
            collection_fraction,
            collection_cost,
            recovery_delay_days,
        } => {
            vintage(*vintage_ordinal, r.as_of_ordinal)?;
            label(channel)?;
            label(dti_definition)?;
            if !dti.is_finite() || !(0.0..=10.0).contains(dti) {
                return Err("personal DTI must be supplied in [0,10]".into());
            }
            fraction(*collection_fraction, "collection fraction")?;
            amount(*collection_cost, "collection cost")?;
            if *recovery_delay_days > MAX_CONSUMER_DAYS {
                return Err("recovery delay exceeds admission".into());
            }
        }
    }
    Ok(())
}
fn vintage(v: i32, as_of: i32) -> Result<(), String> {
    validate_ordinal(v)?;
    if v > as_of {
        return Err("vintage follows valuation".into());
    }
    Ok(())
}
