//! Joint credit intervals: absolute probability, independently weighted money and reviewed journals.
use crate::credit_accounting as accounting;
use accounting::{
    AccountCode, AccountingPolicy, AccountingState, Identity, JournalIntent, LiveState,
    OriginDefaultDelta, PostingIntent, RecoveryPoint, ReviewedAllowance, ReviewedChargeoff,
};
use portfolio_model_core::cashflow::{validate_ordinal, CashflowMeasure, MAX_HORIZON_DAYS};
use portfolio_model_core::credit::CreditMeasure;
use portfolio_model_core::credit_operator::{
    self, CreditOperator, CreditOperatorRequest, CreditOperatorState,
};
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "credit-state-1";
pub const OPENING_SCHEMA: &str = "credit-state-opening-1";
pub const MAX_INTERVAL_DAYS: u32 = 36_500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryFraction {
    pub lag_days: u32,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditStateContract {
    pub schema: String,
    pub identity: Identity,
    pub measure: CreditMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub position_id: String,
    pub claim_namespace: String,
    pub economic_source: Identity,
    pub recovery_schedule_review: Identity,
    pub accounting_policy: AccountingPolicy,
    /// Elapsed actual days; annual intensities and simple coupons use ACT/365.
    pub interval_days: u32,
    pub transition_hazards_per_year: [[f64; 5]; 3],
    pub accrual_rates: [f64; 3],
    /// Principal-only LGD and default-contingent undrawn conversion by exit origin.
    pub loss_given_default: [f64; 3],
    pub credit_conversion_factor: [f64; 3],
    pub complete_recovery_tails: [Vec<RecoveryFraction>; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditStateOpening {
    pub schema: String,
    pub contract_identity: Identity,
    pub last_ordinal: i32,
    /// Absolute probability: live3, default-from-origin3, prepaid. Never renormalized.
    pub probability: [f64; 7],
    pub accounting: AccountingState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditStateStep {
    pub recognition_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub chargeoffs: Vec<ReviewedChargeoff>,
    pub allowance_review: Option<ReviewedAllowance>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepaySettlement {
    pub principal: f64,
    pub accrued_interest: f64,
    pub book_adjustment: f64,
    pub undrawn_cancelled: f64,
    pub cash: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreditStateResult {
    pub schema: String,
    pub contract_identity: Identity,
    pub measure: CreditMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub closing: CreditStateOpening,
    pub intents: Vec<JournalIntent>,
    pub earned_interest: f64,
    pub default_by_origin: [OriginDefaultDelta; 3],
    pub prepaid: PrepaySettlement,
    pub economic_principal_loss: f64,
    pub contingent_draw_cash: f64,
    pub modeled_recovery_cash: f64,
    pub recovery_tail_after_reporting: f64,
}

/// Private compiled coefficients prevent caller mutation or unverified deserialization.
#[derive(Debug, Clone)]
pub struct CompiledCreditState {
    contract: CreditStateContract,
    operator: CreditOperator,
}

fn amount(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("credit state monetary amount exceeds finite currency-unit bounds".into())
    } else {
        Ok(())
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(128. * f64::EPSILON * a.abs().max(b.abs()))
}
fn text(s: &str, bytes: &mut usize) -> Result<(), String> {
    *bytes = bytes.saturating_add(s.len());
    if s.is_empty()
        || s.len() > 256
        || s.chars().any(char::is_control)
        || *bytes > MAX_DATED_METADATA_BYTES
    {
        return Err(
            "credit state identity text is empty, contains control characters or exceeds bounds"
                .into(),
        );
    }
    Ok(())
}
fn identity(id: &Identity, bytes: &mut usize) -> Result<(), String> {
    text(&id.id, bytes)?;
    text(&id.revision, bytes)
}
fn validate_live(live: &[LiveState; 3]) -> Result<(), String> {
    for row in live {
        for x in [
            row.principal,
            row.undrawn,
            row.accrued_interest,
            row.book_adjustment,
        ] {
            amount(x)?;
        }
        if row.principal < 0. || row.undrawn < 0. || row.principal + row.book_adjustment < 0. {
            return Err(
                "credit live principal, undrawn and net carrying value must be nonnegative".into(),
            );
        }
    }
    Ok(())
}

pub fn compile(contract: &CreditStateContract) -> Result<CompiledCreditState, String> {
    if contract.schema != SCHEMA
        || contract.measure != CreditMeasure::Physical
        || contract.monetary_unit != "currency_units"
        || contract.currency.len() != 3
        || !contract.currency.bytes().all(|b| b.is_ascii_uppercase())
        || !(1..=MAX_INTERVAL_DAYS).contains(&contract.interval_days)
        || contract.claim_namespace.len() > 128
    {
        return Err("credit state requires Physical measure, currency_units, currency code and bounded actual interval".into());
    }
    let mut bytes = 0;
    for id in [
        &contract.identity,
        &contract.economic_source,
        &contract.recovery_schedule_review,
        &contract.accounting_policy.identity,
    ] {
        identity(id, &mut bytes)?;
    }
    text(&contract.position_id, &mut bytes)?;
    text(&contract.claim_namespace, &mut bytes)?;
    for i in 0..3 {
        if [
            contract.loss_given_default[i],
            contract.credit_conversion_factor[i],
        ]
        .iter()
        .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
        {
            return Err("LGD and default-contingent CCF must be finite fractions in 0..1".into());
        }
        let tail = &contract.complete_recovery_tails[i];
        if tail.len() > accounting::MAX_RECOVERY_POINTS {
            return Err("complete recovery tail exceeds 32 points per origin".into());
        }
        let mut previous = None;
        let mut sum = 0.;
        for point in tail {
            if point.lag_days > MAX_HORIZON_DAYS
                || previous.is_some_and(|n| point.lag_days <= n)
                || !point.weight.is_finite()
                || !(0.0..=1.0).contains(&point.weight)
            {
                return Err("recovery weights require strictly increasing bounded actual lags and finite fractions".into());
            }
            sum += point.weight;
            previous = Some(point.lag_days);
        }
        if (!tail.is_empty() && (sum - 1.).abs() > 8. * f64::EPSILON)
            || (tail.is_empty() && contract.loss_given_default[i] != 1.)
        {
            return Err("recoverable principal requires a complete unit-weight tail; only total loss permits an empty tail".into());
        }
    }
    let operator = credit_operator::compile(&CreditOperatorRequest {
        schema: credit_operator::SCHEMA.into(),
        measure: contract.measure,
        duration_years: f64::from(contract.interval_days) / 365.,
        transition_hazards_per_year: contract.transition_hazards_per_year,
        accrual_rates: contract.accrual_rates,
    })?;
    Ok(CompiledCreditState {
        contract: contract.clone(),
        operator,
    })
}

fn intent(
    contract: &CreditStateContract,
    day: i32,
    event: &str,
    lines: &[(AccountCode, f64)],
) -> Result<Option<JournalIntent>, String> {
    let mut sum = 0.;
    let mut scale = 0.;
    let mut postings = Vec::with_capacity(lines.len());
    for &(account, value) in lines {
        amount(value)?;
        sum += value;
        scale += value.abs();
        if value != 0. {
            postings.push(PostingIntent {
                account,
                amount: value,
            });
        }
    }
    if sum.abs() > 1e-9_f64.max(128. * f64::EPSILON * scale) {
        return Err("joint credit intent does not balance".into());
    }
    Ok((!postings.is_empty()).then(|| JournalIntent {
        recognition_ordinal: day,
        event: event.into(),
        position_id: contract.position_id.clone(),
        claim_id: None,
        policy: contract.accounting_policy.identity.clone(),
        source: contract.economic_source.clone(),
        postings,
    }))
}

impl CompiledCreditState {
    pub fn contract(&self) -> &CreditStateContract {
        &self.contract
    }

    /// Failure-atomic immutable interval, with no second survival multiplication.
    pub fn advance(
        &self,
        opening: &CreditStateOpening,
        step: &CreditStateStep,
    ) -> Result<CreditStateResult, String> {
        let c = &self.contract;
        validate_ordinal(opening.last_ordinal)?;
        validate_ordinal(step.recognition_ordinal)?;
        if opening.schema != OPENING_SCHEMA
            || opening.contract_identity != c.identity
            || i64::from(step.recognition_ordinal) - i64::from(opening.last_ordinal)
                != i64::from(c.interval_days)
            || opening
                .probability
                .iter()
                .any(|p| !p.is_finite() || *p < 0.)
            || opening.probability.iter().sum::<f64>() > 1. + 1e-11
        {
            return Err("credit opening schema/contract, absolute probability or contiguous interval is invalid".into());
        }
        validate_live(&opening.accounting.live)?;
        if opening.accounting.claims.len() > accounting::MAX_CLAIMS
            || step.chargeoffs.len() > accounting::MAX_REVIEW_EVENTS
        {
            return Err("joint credit claims/reviews exceed bounded work admission".into());
        }
        if opening
            .accounting
            .claims
            .iter()
            .flat_map(|claim| &claim.remaining_recoveries)
            .any(|p| p.payment_ordinal < step.recognition_ordinal)
        {
            return Err(
                "interval crosses an unsettled recovery date; split before advancing".into(),
            );
        }
        let mut raw = CreditOperatorState {
            schema: credit_operator::STATE_SCHEMA.into(),
            measure: c.measure,
            monetary_unit: c.monetary_unit.clone(),
            probability: [0.; 7],
            principal: [0.; 7],
            undrawn_commitment: [0.; 7],
            accrued_interest: [0.; 7],
            book_adjustment: [0.; 7],
        };
        for (i, row) in opening.accounting.live.iter().enumerate() {
            raw.probability[i] = opening.probability[i];
            raw.principal[i] = row.principal;
            raw.undrawn_commitment[i] = row.undrawn;
            raw.accrued_interest[i] = row.accrued_interest;
            raw.book_adjustment[i] = row.book_adjustment;
        }
        let propagated = self.operator.apply(&raw)?;
        let mut probability = propagated.probability;
        for (i, value) in probability.iter_mut().enumerate().skip(3) {
            *value += opening.probability[i];
        }
        if !near(probability.iter().sum(), opening.probability.iter().sum()) {
            return Err("joint credit probability mass did not conserve".into());
        }
        for (before, after) in [
            (&raw.principal, &propagated.principal),
            (&raw.undrawn_commitment, &propagated.undrawn_commitment),
            (&raw.book_adjustment, &propagated.book_adjustment),
        ] {
            if !near(before.iter().sum(), after.iter().sum()) {
                return Err("joint credit monetary transport did not conserve".into());
            }
        }
        let earned = raw
            .principal
            .iter()
            .zip(self.operator.earned_interest())
            .map(|(principal, row)| principal * row.iter().sum::<f64>())
            .sum::<f64>();
        amount(earned)?;
        if !near(
            propagated.accrued_interest.iter().sum(),
            raw.accrued_interest.iter().sum::<f64>() + earned,
        ) {
            return Err("joint earned/accrued interest transport did not conserve".into());
        }
        let mut surviving = [LiveState::default(); 3];
        let mut default_opening = [LiveState::default(); 3];
        let mut defaults = Vec::with_capacity(3);
        for i in 0..3 {
            surviving[i] = LiveState {
                principal: propagated.principal[i],
                undrawn: propagated.undrawn_commitment[i],
                accrued_interest: propagated.accrued_interest[i],
                book_adjustment: propagated.book_adjustment[i],
            };
            let j = i + 3;
            default_opening[i] = LiveState {
                principal: propagated.principal[j],
                undrawn: propagated.undrawn_commitment[j],
                accrued_interest: propagated.accrued_interest[j],
                book_adjustment: propagated.book_adjustment[j],
            };
            let row = default_opening[i];
            if row.principal == 0.
                && row.undrawn == 0.
                && row.accrued_interest == 0.
                && row.book_adjustment == 0.
            {
                continue;
            }
            let draw = row.undrawn * c.credit_conversion_factor[i];
            let gross = row.principal + draw;
            let loss = gross * c.loss_given_default[i];
            let recoverable = gross - loss;
            let mut tail = Vec::with_capacity(c.complete_recovery_tails[i].len());
            for point in &c.complete_recovery_tails[i] {
                let day = step
                    .recognition_ordinal
                    .checked_add(point.lag_days as i32)
                    .ok_or("recovery date overflow")?;
                validate_ordinal(day)?;
                tail.push(RecoveryPoint {
                    payment_ordinal: day,
                    amount: recoverable * point.weight,
                });
            }
            let mut by_origin = [OriginDefaultDelta::default(); 3];
            by_origin[i] = OriginDefaultDelta {
                principal: row.principal,
                undrawn: row.undrawn,
                contingent_draw: draw,
                accrued_interest: row.accrued_interest,
                book_adjustment: row.book_adjustment,
            };
            defaults.push(accounting::DefaultInstruction {
                claim_id: format!("{}:{}:{}", c.claim_namespace, step.recognition_ordinal, i),
                economic_source: c.economic_source.clone(),
                recovery_schedule_review: c.recovery_schedule_review.clone(),
                by_origin,
                economic_principal_loss: loss,
                complete_recovery_tail: tail,
            });
        }
        validate_live(&surviving)?;
        // Synthetic opening contains precisely the default slice, not netted survivor/default rights.
        let accounted = accounting::evaluate(&accounting::CreditAccountingRequest {
            schema: accounting::SCHEMA.into(),
            measure: CashflowMeasure::Physical,
            currency: c.currency.clone(),
            monetary_unit: c.monetary_unit.clone(),
            position_id: c.position_id.clone(),
            recognition_ordinal: step.recognition_ordinal,
            reporting_end_ordinal: step.reporting_end_ordinal,
            policy: c.accounting_policy.clone(),
            opening: AccountingState {
                live: default_opening,
                allowance: opening.accounting.allowance,
                claims: opening.accounting.claims.clone(),
            },
            defaults,
            chargeoffs: step.chargeoffs.clone(),
            allowance_review: step.allowance_review.clone(),
        })?;
        for row in &accounted.closing.live {
            if [
                row.principal,
                row.undrawn,
                row.accrued_interest,
                row.book_adjustment,
            ]
            .iter()
            .any(|x| *x != 0.)
            {
                return Err(
                    "default-slice accounting failed to consume all transported rights".into(),
                );
            }
        }
        let prepaid = PrepaySettlement {
            principal: propagated.principal[6],
            accrued_interest: propagated.accrued_interest[6],
            book_adjustment: propagated.book_adjustment[6],
            undrawn_cancelled: propagated.undrawn_commitment[6],
            cash: propagated.principal[6] + propagated.accrued_interest[6],
        };
        amount(prepaid.cash)?;
        let mut intents = Vec::with_capacity(accounted.intents.len() + 2);
        if let Some(j) = intent(
            c,
            step.recognition_ordinal,
            "joint_credit_earned_interest",
            &[
                (AccountCode::LiveAccruedInterest, earned),
                (AccountCode::InterestIncome, -earned),
            ],
        )? {
            intents.push(j);
        }
        intents.extend(accounted.intents);
        if let Some(j) = intent(
            c,
            step.recognition_ordinal,
            "economic_prepay_right_settlement",
            &[
                (AccountCode::Cash, prepaid.cash),
                (AccountCode::LivePrincipal, -prepaid.principal),
                (AccountCode::LiveAccruedInterest, -prepaid.accrued_interest),
                (AccountCode::LiveBookAdjustment, -prepaid.book_adjustment),
                (
                    AccountCode::BasisRecognitionExpense,
                    prepaid.book_adjustment,
                ),
            ],
        )? {
            intents.push(j);
        }
        for (account, before, after) in [
            (
                AccountCode::LivePrincipal,
                opening
                    .accounting
                    .live
                    .iter()
                    .map(|r| r.principal)
                    .sum::<f64>(),
                surviving.iter().map(|r| r.principal).sum::<f64>(),
            ),
            (
                AccountCode::LiveAccruedInterest,
                opening
                    .accounting
                    .live
                    .iter()
                    .map(|r| r.accrued_interest)
                    .sum::<f64>(),
                surviving.iter().map(|r| r.accrued_interest).sum::<f64>(),
            ),
            (
                AccountCode::LiveBookAdjustment,
                opening
                    .accounting
                    .live
                    .iter()
                    .map(|r| r.book_adjustment)
                    .sum::<f64>(),
                surviving.iter().map(|r| r.book_adjustment).sum::<f64>(),
            ),
        ] {
            let movement = intents
                .iter()
                .flat_map(|j| &j.postings)
                .filter(|p| p.account == account)
                .map(|p| p.amount)
                .sum::<f64>();
            if !near(after - before, movement) {
                return Err("joint credit live ledger roll-forward did not reconcile".into());
            }
        }
        let mut closing = accounted.closing;
        closing.live = surviving;
        Ok(CreditStateResult {
            schema: SCHEMA.into(),
            contract_identity: c.identity.clone(),
            measure: c.measure,
            currency: c.currency.clone(),
            monetary_unit: c.monetary_unit.clone(),
            closing: CreditStateOpening {
                schema: OPENING_SCHEMA.into(),
                contract_identity: c.identity.clone(),
                last_ordinal: step.recognition_ordinal,
                probability,
                accounting: closing,
            },
            intents,
            earned_interest: earned,
            default_by_origin: accounted.default_by_origin,
            prepaid,
            economic_principal_loss: accounted.economic_principal_loss,
            contingent_draw_cash: accounted.contingent_draw_cash,
            modeled_recovery_cash: accounted.modeled_recovery_cash,
            recovery_tail_after_reporting: accounted.recovery_tail_after_reporting,
        })
    }
}
