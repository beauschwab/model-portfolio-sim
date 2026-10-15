//! Explicit physical credit-accounting policy step; balanced journal intents only.
//! Economic loss, allowance review and charge-off decisions are separate inputs.
use crate::cashflows::dated::{validate_ordinal, CashflowMeasure, MAX_HORIZON_DAYS};
use crate::validation::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "credit-accounting-1";
pub const MAX_CLAIMS: usize = 62_000;
pub const MAX_DEFAULT_EVENTS: usize = 256;
pub const MAX_REVIEW_EVENTS: usize = 4096;
pub const MAX_RECOVERY_POINTS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccruedPolicy {
    ReverseIncomeAtDefault,
    TransferUntilReviewedReversal,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BasisPolicy {
    TransferUntilReviewedChargeoff,
    RecognizeInEarningsAtDefault,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AllowanceShortfall {
    Reject,
    ExpenseAtReviewedChargeoff,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExcessCashRecovery {
    CreditAllowance,
    RecognizeInEarnings,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccountingPolicy {
    pub identity: Identity,
    pub accrued: AccruedPolicy,
    pub basis: BasisPolicy,
    pub allowance_shortfall: AllowanceShortfall,
    pub excess_cash_recovery: ExcessCashRecovery,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveState {
    pub principal: f64,
    pub undrawn: f64,
    /// Signed recognized right supplied by accrual ownership, not inferred coupon.
    pub accrued_interest: f64,
    /// Signed premium/cost positive, discount/deferred net fee negative.
    pub book_adjustment: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPoint {
    pub payment_ordinal: i32,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryClaim {
    pub claim_id: String,
    pub economic_source: Identity,
    pub recovery_schedule_review: Identity,
    pub policy: AccountingPolicy,
    pub default_ordinal: i32,
    pub initial_principal: f64,
    /// Principal-only economic expected loss. Never an implicit charge-off.
    pub economic_principal_loss: f64,
    pub initial_accrued_interest: f64,
    pub initial_book_adjustment: f64,
    pub outstanding_principal: f64,
    pub outstanding_accrued_interest: f64,
    pub outstanding_book_adjustment: f64,
    pub principal_charged_off: f64,
    pub accrued_reversed: f64,
    pub basis_recognized: f64,
    pub recovery_cash_collected: f64,
    pub recovery_applied_to_principal: f64,
    /// Additive signed interest cash bookkeeping; historical principal-only records read as zero.
    #[serde(default)]
    pub interest_recovery_cash_collected: f64,
    #[serde(default)]
    pub interest_recovery_applied: f64,
    #[serde(default)]
    pub settled_interest_recovery_ids: Vec<Identity>,
    /// Full uncollected nominal schedule, including points beyond reporting end.
    pub remaining_recoveries: Vec<RecoveryPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingState {
    /// Origin order performing, watch, delinquent; transitions occur upstream.
    pub live: [LiveState; 3],
    /// Positive balance of the contra-asset allowance; GL balance is negative.
    pub allowance: f64,
    pub claims: Vec<RecoveryClaim>,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginDefaultDelta {
    pub principal: f64,
    pub undrawn: f64,
    /// Explicit cash-funded utilization conditional on this default, not an ordinary draw.
    pub contingent_draw: f64,
    pub accrued_interest: f64,
    pub book_adjustment: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultInstruction {
    pub claim_id: String,
    pub economic_source: Identity,
    pub recovery_schedule_review: Identity,
    pub by_origin: [OriginDefaultDelta; 3],
    pub economic_principal_loss: f64,
    pub complete_recovery_tail: Vec<RecoveryPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedChargeoff {
    pub review: Identity,
    pub claim_id: String,
    pub recognition_ordinal: i32,
    pub principal_amount: f64,
    pub book_adjustment_amount: f64,
    pub accrued_interest_reversal: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPhase {
    BeforeCredit,
    AfterCredit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedAllowance {
    pub review: Identity,
    pub recognition_ordinal: i32,
    pub phase: ReviewPhase,
    pub target_amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditAccountingRequest {
    pub schema: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub position_id: String,
    pub recognition_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub policy: AccountingPolicy,
    pub opening: AccountingState,
    pub defaults: Vec<DefaultInstruction>,
    pub chargeoffs: Vec<ReviewedChargeoff>,
    pub allowance_review: Option<ReviewedAllowance>,
}

/// Debit-positive generic account codes. Adapters own chart-of-accounts mapping.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u16)]
pub enum AccountCode {
    Cash = 1,
    LivePrincipal = 2,
    DefaultPrincipal = 3,
    LiveAccruedInterest = 4,
    DefaultAccruedInterest = 5,
    LiveBookAdjustment = 6,
    DefaultBookAdjustment = 7,
    Allowance = 8,
    InterestIncome = 9,
    CreditLossExpense = 10,
    BasisRecognitionExpense = 11,
    RecoveryIncome = 12,
}
impl AccountCode {
    pub fn numeric_code(self) -> u16 {
        self as u16
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingIntent {
    pub account: AccountCode,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalIntent {
    pub recognition_ordinal: i32,
    pub event: String,
    pub position_id: String,
    pub claim_id: Option<String>,
    pub policy: Identity,
    pub source: Identity,
    pub postings: Vec<PostingIntent>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditAccountingResult {
    pub schema: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub position_id: String,
    pub recognition_ordinal: i32,
    pub policy: AccountingPolicy,
    pub closing: AccountingState,
    pub intents: Vec<JournalIntent>,
    pub default_by_origin: [OriginDefaultDelta; 3],
    pub economic_principal_loss: f64,
    pub modeled_recovery_cash: f64,
    pub contingent_draw_cash: f64,
    pub recovery_tail_after_reporting: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedInterestRecovery {
    pub recovery: Identity,
    pub claim_id: String,
    pub recognition_ordinal: i32,
    pub amount: f64,
    pub policy_review: Identity,
    pub source: Identity,
    pub excess_cash: ExcessCashRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterestRecoveryRequest {
    pub schema: String,
    pub measure: CashflowMeasure,
    pub currency: String,
    pub monetary_unit: String,
    pub position_id: String,
    pub recognition_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub policy: AccountingPolicy,
    pub opening: AccountingState,
    pub settlements: Vec<ReviewedInterestRecovery>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterestRecoveryResult {
    pub schema: String,
    pub closing: AccountingState,
    pub intents: Vec<JournalIntent>,
    pub interest_cash: f64,
    pub applied_to_rights: f64,
    pub excess_cash: f64,
}

fn amount(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        return Err("credit accounting amount exceeds finite actual-currency admission".into());
    }
    Ok(())
}
fn nonnegative(x: f64) -> Result<(), String> {
    amount(x)?;
    if x < 0. {
        return Err("credit accounting principal/allowance/recovery must be nonnegative".into());
    }
    Ok(())
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs()))
}
fn text(s: &str, bytes: &mut usize) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 1024 || s.chars().any(char::is_control) {
        return Err(
            "credit accounting identity must be nonempty, control-free and at most 1024 bytes"
                .into(),
        );
    }
    *bytes = bytes
        .checked_add(s.len())
        .ok_or("credit accounting metadata overflow")?;
    if *bytes > MAX_DATED_METADATA_BYTES {
        return Err("credit accounting metadata exceeds 32 MiB".into());
    }
    Ok(())
}
fn identity(i: &Identity, bytes: &mut usize) -> Result<(), String> {
    text(&i.id, bytes)?;
    text(&i.revision, bytes)
}
fn allocated(total: f64, delta: f64) -> Result<(), String> {
    amount(total)?;
    amount(delta)?;
    if (delta != 0. && (total == 0. || total.is_sign_positive() != delta.is_sign_positive()))
        || (delta.abs() > total.abs() && !near(delta.abs(), total.abs()))
    {
        return Err(
            "default/chargeoff allocation exceeds the supplied signed opening right".into(),
        );
    }
    Ok(())
}
fn subtract(total: f64, delta: f64) -> Result<f64, String> {
    allocated(total, delta)?;
    let out = total - delta;
    Ok(
        if total != 0. && out.is_sign_positive() != total.is_sign_positive() && near(total, delta) {
            0.
        } else {
            out
        },
    )
}
fn recovery_tail(points: &[RecoveryPoint], default: i32, minimum: i32) -> Result<f64, String> {
    if points.len() > MAX_RECOVERY_POINTS {
        return Err("recovery tail exceeds 32 points".into());
    }
    let mut total = 0.;
    let mut previous = None;
    for p in points {
        validate_ordinal(p.payment_ordinal)?;
        nonnegative(p.amount)?;
        if p.payment_ordinal < minimum
            || p.payment_ordinal < default
            || (p.payment_ordinal - default) as u32 > MAX_HORIZON_DAYS
            || previous.is_some_and(|d| p.payment_ordinal <= d)
        {
            return Err(
                "recovery tail must be complete, strictly dated and within the 36600-day domain"
                    .into(),
            );
        }
        previous = Some(p.payment_ordinal);
        total += p.amount;
        amount(total)?;
    }
    Ok(total)
}
fn validate_claim(
    c: &RecoveryClaim,
    request: &CreditAccountingRequest,
    bytes: &mut usize,
) -> Result<(), String> {
    text(&c.claim_id, bytes)?;
    identity(&c.economic_source, bytes)?;
    identity(&c.recovery_schedule_review, bytes)?;
    identity(&c.policy.identity, bytes)?;
    validate_ordinal(c.default_ordinal)?;
    if c.default_ordinal > request.recognition_ordinal || c.policy != request.policy {
        return Err(
            "opening recovery claim has incompatible default date or accounting policy".into(),
        );
    }
    for x in [
        c.initial_principal,
        c.economic_principal_loss,
        c.outstanding_principal,
        c.principal_charged_off,
        c.recovery_cash_collected,
        c.recovery_applied_to_principal,
    ] {
        nonnegative(x)?;
    }
    for x in [
        c.initial_accrued_interest,
        c.initial_book_adjustment,
        c.outstanding_accrued_interest,
        c.outstanding_book_adjustment,
        c.accrued_reversed,
        c.basis_recognized,
        c.interest_recovery_cash_collected,
        c.interest_recovery_applied,
    ] {
        amount(x)?;
    }
    let tail = recovery_tail(
        &c.remaining_recoveries,
        c.default_ordinal,
        request.recognition_ordinal,
    )?;
    if c.economic_principal_loss > c.initial_principal
        || c.initial_principal + c.initial_book_adjustment < 0.
        || c.outstanding_principal + c.outstanding_book_adjustment < 0.
        || c.recovery_applied_to_principal > c.recovery_cash_collected
        || !near(
            c.initial_principal,
            c.outstanding_principal + c.principal_charged_off + c.recovery_applied_to_principal,
        )
        || !near(
            c.initial_accrued_interest,
            c.outstanding_accrued_interest + c.accrued_reversed + c.interest_recovery_applied,
        )
        || !near(
            c.initial_book_adjustment,
            c.outstanding_book_adjustment + c.basis_recognized,
        )
        || !near(
            c.initial_principal - c.economic_principal_loss,
            c.recovery_cash_collected + tail,
        )
        || (request.policy.accrued == AccruedPolicy::ReverseIncomeAtDefault
            && c.outstanding_accrued_interest != 0.)
        || (request.policy.basis == BasisPolicy::RecognizeInEarningsAtDefault
            && c.outstanding_book_adjustment != 0.)
    {
        return Err("opening claim principal/accrual/basis or complete economic recovery tail does not conserve".into());
    }
    allocated(c.initial_accrued_interest, c.accrued_reversed)?;
    allocated(c.initial_accrued_interest, c.outstanding_accrued_interest)?;
    allocated(
        c.initial_accrued_interest,
        c.accrued_reversed + c.interest_recovery_applied,
    )?;
    allocated(
        c.initial_accrued_interest,
        c.interest_recovery_cash_collected,
    )?;
    allocated(
        c.interest_recovery_cash_collected,
        c.interest_recovery_applied,
    )?;
    if c.settled_interest_recovery_ids.len() > MAX_REVIEW_EVENTS {
        return Err("settled interest recovery identity admission exceeded".into());
    }
    let mut recovery_ids = HashSet::new();
    for id in &c.settled_interest_recovery_ids {
        identity(id, bytes)?;
        if !recovery_ids.insert(&id.id) {
            return Err("duplicate settled interest recovery ID".into());
        }
    }
    allocated(c.initial_book_adjustment, c.basis_recognized)?;
    Ok(())
}
fn emit(
    request: &CreditAccountingRequest,
    intents: &mut Vec<JournalIntent>,
    event: &str,
    claim: Option<&str>,
    source: &Identity,
    lines: &[(AccountCode, f64)],
) -> Result<(), String> {
    let mut sum = 0.;
    let mut scale = 0.;
    let mut postings = Vec::new();
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
    if sum.abs() > 1e-9_f64.max(64. * f64::EPSILON * scale) {
        return Err("credit accounting journal intent is unbalanced".into());
    }
    if !postings.is_empty() {
        intents.push(JournalIntent {
            recognition_ordinal: request.recognition_ordinal,
            event: event.into(),
            position_id: request.position_id.clone(),
            claim_id: claim.map(str::to_owned),
            policy: request.policy.identity.clone(),
            source: source.clone(),
            postings,
        });
    }
    Ok(())
}
fn allowance_review(
    request: &CreditAccountingRequest,
    review: &ReviewedAllowance,
    state: &mut AccountingState,
    intents: &mut Vec<JournalIntent>,
) -> Result<(), String> {
    let delta = review.target_amount - state.allowance;
    emit(
        request,
        intents,
        "reviewed_allowance_target",
        None,
        &review.review,
        &[
            (AccountCode::CreditLossExpense, delta),
            (AccountCode::Allowance, -delta),
        ],
    )?;
    state.allowance = review.target_amount;
    Ok(())
}

fn collect_principal_recovery(
    request: &CreditAccountingRequest,
    claim: &mut RecoveryClaim,
    due: f64,
    intents: &mut Vec<JournalIntent>,
) -> Result<f64, String> {
    let applied = due.min(claim.outstanding_principal);
    let excess = due - applied;
    let allowance_credit =
        if request.policy.excess_cash_recovery == ExcessCashRecovery::CreditAllowance {
            excess
        } else {
            0.
        };
    let income = excess - allowance_credit;
    emit(
        request,
        intents,
        "modeled_physical_recovery_cash",
        Some(&claim.claim_id),
        &claim.recovery_schedule_review,
        &[
            (AccountCode::Cash, due),
            (AccountCode::DefaultPrincipal, -applied),
            (AccountCode::Allowance, -allowance_credit),
            (AccountCode::RecoveryIncome, -income),
        ],
    )?;
    claim.outstanding_principal -= applied;
    claim.recovery_applied_to_principal += applied;
    claim.recovery_cash_collected += due;

    Ok(allowance_credit)
}

/// Apply today's reviewed inputs atomically to a cloned opening state.
pub fn evaluate(request: &CreditAccountingRequest) -> Result<CreditAccountingResult, String> {
    evaluate_internal(request, true)
}

/// Native event-time owners apply reviewed actions without settling later cash
/// merely because it shares today's reporting ordinal.
pub fn evaluate_reviewed_actions(
    request: &CreditAccountingRequest,
) -> Result<CreditAccountingResult, String> {
    evaluate_internal(request, false)
}

fn evaluate_internal(
    request: &CreditAccountingRequest,
    settle_due: bool,
) -> Result<CreditAccountingResult, String> {
    validate_ordinal(request.recognition_ordinal)?;
    validate_ordinal(request.reporting_end_ordinal)?;
    if request.schema != SCHEMA
        || request.measure != CashflowMeasure::Physical
        || request.monetary_unit != "currency_units"
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || request.reporting_end_ordinal < request.recognition_ordinal
        || (request.reporting_end_ordinal - request.recognition_ordinal) as u32 > MAX_HORIZON_DAYS
        || request.opening.claims.len() + request.defaults.len() > MAX_CLAIMS
        || request.defaults.len() > MAX_DEFAULT_EVENTS
        || request.chargeoffs.len() > MAX_REVIEW_EVENTS
    {
        return Err("credit accounting requires Physical measure, actual currency units and bounded day/work domains".into());
    }
    let mut bytes = 0;
    text(&request.position_id, &mut bytes)?;
    identity(&request.policy.identity, &mut bytes)?;
    nonnegative(request.opening.allowance)?;
    for s in &request.opening.live {
        nonnegative(s.principal)?;
        nonnegative(s.undrawn)?;
        amount(s.accrued_interest)?;
        amount(s.book_adjustment)?;
        if s.principal + s.book_adjustment < 0. {
            return Err("opening live principal plus basis cannot be negative".into());
        }
    }
    let mut claim_ids = HashSet::new();
    for c in &request.opening.claims {
        validate_claim(c, request, &mut bytes)?;
        if !claim_ids.insert(c.claim_id.as_str()) {
            return Err("duplicate opening credit claim".into());
        }
    }
    for d in &request.defaults {
        text(&d.claim_id, &mut bytes)?;
        identity(&d.economic_source, &mut bytes)?;
        identity(&d.recovery_schedule_review, &mut bytes)?;
        if !claim_ids.insert(d.claim_id.as_str()) {
            return Err("duplicate economic default claim".into());
        }
        nonnegative(d.economic_principal_loss)?;
        let mut gross = 0.;
        for origin in &d.by_origin {
            for v in [origin.principal, origin.undrawn, origin.contingent_draw] {
                nonnegative(v)?;
            }
            amount(origin.accrued_interest)?;
            amount(origin.book_adjustment)?;
            if origin.contingent_draw > origin.undrawn {
                return Err("default-contingent draw exceeds defaulted undrawn commitment".into());
            }
            gross += origin.principal + origin.contingent_draw;
            amount(gross)?;
        }
        let tail = recovery_tail(
            &d.complete_recovery_tail,
            request.recognition_ordinal,
            request.recognition_ordinal,
        )?;
        if d.economic_principal_loss > gross || !near(gross - d.economic_principal_loss, tail) {
            return Err("principal-only economic loss and complete recovery tail do not conserve gross default EAD".into());
        }
    }
    let mut reviews = HashSet::new();
    for c in &request.chargeoffs {
        identity(&c.review, &mut bytes)?;
        text(&c.claim_id, &mut bytes)?;
        nonnegative(c.principal_amount)?;
        amount(c.book_adjustment_amount)?;
        amount(c.accrued_interest_reversal)?;
        if c.recognition_ordinal != request.recognition_ordinal
            || !reviews.insert((&c.review.id, &c.review.revision, &c.claim_id))
        {
            return Err("chargeoff reviews require today's exact recognition date and unique revision identity".into());
        }
    }
    if let Some(r) = &request.allowance_review {
        identity(&r.review, &mut bytes)?;
        nonnegative(r.target_amount)?;
        if r.recognition_ordinal != request.recognition_ordinal {
            return Err("allowance review requires today's recognition date".into());
        }
    }
    let mut state = request.opening.clone();
    let mut intents = Vec::new();
    if let Some(r) = request
        .allowance_review
        .as_ref()
        .filter(|r| r.phase == ReviewPhase::BeforeCredit)
    {
        allowance_review(request, r, &mut state, &mut intents)?;
    }
    let mut totals = [OriginDefaultDelta::default(); 3];
    let mut economic_loss = 0.;
    let mut draw_cash = 0.;
    for d in &request.defaults {
        let mut principal = 0.;
        let mut accrued = 0.;
        let mut basis = 0.;
        for (origin_index, o) in d.by_origin.iter().enumerate() {
            let s = &mut state.live[origin_index];
            s.principal = subtract(s.principal, o.principal)?;
            s.undrawn = subtract(s.undrawn, o.undrawn)?;
            s.accrued_interest = subtract(s.accrued_interest, o.accrued_interest)?;
            s.book_adjustment = subtract(s.book_adjustment, o.book_adjustment)?;
            let total = &mut totals[origin_index];
            total.principal += o.principal;
            total.undrawn += o.undrawn;
            total.contingent_draw += o.contingent_draw;
            total.accrued_interest += o.accrued_interest;
            total.book_adjustment += o.book_adjustment;
            principal += o.principal + o.contingent_draw;
            accrued += o.accrued_interest;
            basis += o.book_adjustment;
            draw_cash += o.contingent_draw;
            emit(
                request,
                &mut intents,
                "default_contingent_draw",
                Some(&d.claim_id),
                &d.economic_source,
                &[
                    (AccountCode::LivePrincipal, o.contingent_draw),
                    (AccountCode::Cash, -o.contingent_draw),
                ],
            )?;
        }
        if principal + basis < 0. {
            return Err("default gross principal plus allocated basis cannot be negative".into());
        }
        let drawn = d.by_origin.iter().map(|o| o.contingent_draw).sum::<f64>();
        emit(
            request,
            &mut intents,
            "economic_default_transfer",
            Some(&d.claim_id),
            &d.economic_source,
            &[
                (AccountCode::LivePrincipal, -principal),
                (AccountCode::DefaultPrincipal, principal),
            ],
        )?;
        // Contingent funding already debited live principal, so net live movement
        // is -(principal-drawn), exactly the original drawn-default allocation.
        debug_assert!(near(
            principal - drawn,
            d.by_origin.iter().map(|o| o.principal).sum()
        ));
        let reverse = request.policy.accrued == AccruedPolicy::ReverseIncomeAtDefault;
        emit(
            request,
            &mut intents,
            if reverse {
                "default_accrued_income_reversal"
            } else {
                "default_accrued_transfer"
            },
            Some(&d.claim_id),
            &d.economic_source,
            &[
                (AccountCode::LiveAccruedInterest, -accrued),
                (
                    if reverse {
                        AccountCode::InterestIncome
                    } else {
                        AccountCode::DefaultAccruedInterest
                    },
                    accrued,
                ),
            ],
        )?;
        let basis_expense = request.policy.basis == BasisPolicy::RecognizeInEarningsAtDefault;
        emit(
            request,
            &mut intents,
            if basis_expense {
                "default_basis_earnings"
            } else {
                "default_basis_transfer"
            },
            Some(&d.claim_id),
            &d.economic_source,
            &[
                (AccountCode::LiveBookAdjustment, -basis),
                (
                    if basis_expense {
                        AccountCode::BasisRecognitionExpense
                    } else {
                        AccountCode::DefaultBookAdjustment
                    },
                    basis,
                ),
            ],
        )?;
        state.claims.push(RecoveryClaim {
            claim_id: d.claim_id.clone(),
            economic_source: d.economic_source.clone(),
            recovery_schedule_review: d.recovery_schedule_review.clone(),
            policy: request.policy.clone(),
            default_ordinal: request.recognition_ordinal,
            initial_principal: principal,
            economic_principal_loss: d.economic_principal_loss,
            initial_accrued_interest: accrued,
            initial_book_adjustment: basis,
            outstanding_principal: principal,
            outstanding_accrued_interest: if reverse { 0. } else { accrued },
            outstanding_book_adjustment: if basis_expense { 0. } else { basis },
            principal_charged_off: 0.,
            accrued_reversed: if reverse { accrued } else { 0. },
            basis_recognized: if basis_expense { basis } else { 0. },
            recovery_cash_collected: 0.,
            recovery_applied_to_principal: 0.,
            interest_recovery_cash_collected: 0.,
            interest_recovery_applied: 0.,
            settled_interest_recovery_ids: Vec::new(),
            remaining_recoveries: d.complete_recovery_tail.clone(),
        });
        economic_loss += d.economic_principal_loss;
        amount(economic_loss)?;
    }
    let claim_map: HashMap<_, _> = state
        .claims
        .iter()
        .enumerate()
        .map(|(i, c)| (c.claim_id.as_str(), i))
        .collect();
    let indices: Vec<_> = request
        .chargeoffs
        .iter()
        .map(|c| {
            claim_map
                .get(c.claim_id.as_str())
                .copied()
                .ok_or("chargeoff references unknown default claim")
        })
        .collect::<Result<_, _>>()?;
    for (c, index) in request.chargeoffs.iter().zip(indices) {
        let claim = &mut state.claims[index];
        let net = c.principal_amount + c.book_adjustment_amount;
        nonnegative(net)?;
        claim.outstanding_principal = subtract(claim.outstanding_principal, c.principal_amount)?;
        claim.outstanding_book_adjustment =
            subtract(claim.outstanding_book_adjustment, c.book_adjustment_amount)?;
        claim.outstanding_accrued_interest = subtract(
            claim.outstanding_accrued_interest,
            c.accrued_interest_reversal,
        )?;
        let covered = net.min(state.allowance);
        let shortfall = net - covered;
        if shortfall != 0. && request.policy.allowance_shortfall == AllowanceShortfall::Reject {
            return Err(
                "reviewed chargeoff exceeds available allowance under reject policy".into(),
            );
        }
        emit(
            request,
            &mut intents,
            "reviewed_credit_chargeoff",
            Some(&c.claim_id),
            &c.review,
            &[
                (AccountCode::DefaultPrincipal, -c.principal_amount),
                (
                    AccountCode::DefaultBookAdjustment,
                    -c.book_adjustment_amount,
                ),
                (AccountCode::Allowance, covered),
                (AccountCode::CreditLossExpense, shortfall),
            ],
        )?;
        emit(
            request,
            &mut intents,
            "reviewed_accrued_income_reversal",
            Some(&c.claim_id),
            &c.review,
            &[
                (
                    AccountCode::DefaultAccruedInterest,
                    -c.accrued_interest_reversal,
                ),
                (AccountCode::InterestIncome, c.accrued_interest_reversal),
            ],
        )?;
        state.allowance -= covered;
        claim.principal_charged_off += c.principal_amount;
        claim.basis_recognized += c.book_adjustment_amount;
        claim.accrued_reversed += c.accrued_interest_reversal;
    }
    let mut recovery_cash = 0.;
    for claim in state.claims.iter_mut().filter(|_| settle_due) {
        let due = claim
            .remaining_recoveries
            .iter()
            .find(|p| p.payment_ordinal == request.recognition_ordinal)
            .map_or(0., |p| p.amount);
        if due != 0. {
            state.allowance += collect_principal_recovery(request, claim, due, &mut intents)?;
            amount(state.allowance)?;
            recovery_cash += due;
            amount(recovery_cash)?;
        }
        claim
            .remaining_recoveries
            .retain(|p| p.payment_ordinal > request.recognition_ordinal);
    }
    if let Some(r) = request
        .allowance_review
        .as_ref()
        .filter(|r| r.phase == ReviewPhase::AfterCredit)
    {
        allowance_review(request, r, &mut state, &mut intents)?;
    }
    let mut tail_after_reporting = 0.;
    for claim in &state.claims {
        validate_claim(claim, request, &mut 0)?;
        tail_after_reporting += claim
            .remaining_recoveries
            .iter()
            .filter(|p| p.payment_ordinal > request.reporting_end_ordinal)
            .map(|p| p.amount)
            .sum::<f64>();
        amount(tail_after_reporting)?;
    }
    for s in &state.live {
        if s.principal + s.book_adjustment < 0. {
            return Err("closing live principal plus basis cannot be negative".into());
        }
    }
    amount(draw_cash)?;
    Ok(CreditAccountingResult {
        schema: SCHEMA.into(),
        measure: request.measure,
        currency: request.currency.clone(),
        monetary_unit: request.monetary_unit.clone(),
        position_id: request.position_id.clone(),
        recognition_ordinal: request.recognition_ordinal,
        policy: request.policy.clone(),
        closing: state,
        intents,
        default_by_origin: totals,
        economic_principal_loss: economic_loss,
        modeled_recovery_cash: recovery_cash,
        contingent_draw_cash: draw_cash,
        recovery_tail_after_reporting: tail_after_reporting,
    })
}

/// Settle exactly one native source recovery amount. Several actual source times
/// can share one journal date; uncollected amounts retain their complete tail.
pub fn settle_principal_recovery(
    request: &CreditAccountingRequest,
    claim_id: &str,
    due: f64,
) -> Result<CreditAccountingResult, String> {
    if !request.defaults.is_empty()
        || !request.chargeoffs.is_empty()
        || request.allowance_review.is_some()
    {
        return Err("native principal settlement cannot carry reviewed actions or defaults".into());
    }
    nonnegative(due)?;
    let mut result = evaluate_reviewed_actions(request)?;
    let claim = result
        .closing
        .claims
        .iter_mut()
        .find(|c| c.claim_id == claim_id)
        .ok_or("native recovery references unknown claim")?;
    let point = claim
        .remaining_recoveries
        .iter_mut()
        .find(|p| p.payment_ordinal == request.recognition_ordinal)
        .ok_or("native recovery lacks complete due source tail")?;
    if due > point.amount {
        return Err("native recovery exceeds remaining actual source tail".into());
    }
    point.amount -= due;
    result.closing.allowance +=
        collect_principal_recovery(request, claim, due, &mut result.intents)?;
    amount(result.closing.allowance)?;
    claim.remaining_recoveries.retain(|p| p.amount != 0.);
    validate_claim(claim, request, &mut 0)?;
    result.modeled_recovery_cash = due;
    result.recovery_tail_after_reporting = result
        .closing
        .claims
        .iter()
        .flat_map(|c| &c.remaining_recoveries)
        .filter(|p| p.payment_ordinal > request.reporting_end_ordinal)
        .map(|p| p.amount)
        .sum();
    Ok(result)
}

/// Explicit signed interest cash settlement, separate from principal recovery or reviewed reversal.
/// Existing `evaluate` ordering and policy decisions remain unchanged.
pub fn settle_interest_recoveries(
    request: &InterestRecoveryRequest,
) -> Result<InterestRecoveryResult, String> {
    if request.schema != "credit-interest-recovery-1"
        || request.measure != CashflowMeasure::Physical
        || request.monetary_unit != "currency_units"
        || request.currency.len() != 3
        || !request.currency.bytes().all(|b| b.is_ascii_uppercase())
        || (i64::from(request.reporting_end_ordinal) - i64::from(request.recognition_ordinal))
            .unsigned_abs()
            > u64::from(MAX_HORIZON_DAYS)
        || request.opening.claims.len() > MAX_CLAIMS
        || request.settlements.len() > MAX_REVIEW_EVENTS
    {
        return Err("unsupported physical interest settlement contract/units/admission".into());
    }
    validate_ordinal(request.recognition_ordinal)?;
    validate_ordinal(request.reporting_end_ordinal)?;
    if request.recognition_ordinal > request.reporting_end_ordinal {
        return Err("interest settlement lies beyond reporting end".into());
    }
    let context = CreditAccountingRequest {
        schema: SCHEMA.into(),
        measure: request.measure,
        currency: request.currency.clone(),
        monetary_unit: request.monetary_unit.clone(),
        position_id: request.position_id.clone(),
        recognition_ordinal: request.recognition_ordinal,
        reporting_end_ordinal: request.reporting_end_ordinal,
        policy: request.policy.clone(),
        opening: request.opening.clone(),
        defaults: Vec::new(),
        chargeoffs: Vec::new(),
        allowance_review: None,
    };
    let mut bytes = 0;
    text(&request.position_id, &mut bytes)?;
    identity(&request.policy.identity, &mut bytes)?;
    nonnegative(request.opening.allowance)?;
    let mut claim_ids = HashMap::new();
    for s in &request.opening.live {
        nonnegative(s.principal)?;
        nonnegative(s.undrawn)?;
        amount(s.accrued_interest)?;
        amount(s.book_adjustment)?;
        if s.principal + s.book_adjustment < 0. {
            return Err("opening live principal plus basis cannot be negative".into());
        }
    }
    let mut settled_ids = HashSet::new();
    for (i, claim) in request.opening.claims.iter().enumerate() {
        validate_claim(claim, &context, &mut bytes)?;
        if claim_ids.insert(claim.claim_id.as_str(), i).is_some() {
            return Err("duplicate interest recovery claim".into());
        }
        for id in &claim.settled_interest_recovery_ids {
            if !settled_ids.insert(id.id.clone()) || settled_ids.len() > 250_000 {
                return Err("duplicate/over-limit settled interest recovery identity".into());
            }
        }
    }
    let mut closing = request.opening.clone();
    let mut intents = Vec::new();
    let mut cash = 0.0;
    let mut applied_total = 0.0;
    let mut excess_total = 0.0;
    for settlement in &request.settlements {
        identity(&settlement.recovery, &mut bytes)?;
        identity(&settlement.policy_review, &mut bytes)?;
        identity(&settlement.source, &mut bytes)?;
        amount(settlement.amount)?;
        if settlement.recognition_ordinal != request.recognition_ordinal {
            return Err("interest recovery requires today's reviewed recognition date".into());
        }
        let index = *claim_ids
            .get(settlement.claim_id.as_str())
            .ok_or("interest recovery references unknown claim")?;
        let claim = &mut closing.claims[index];
        if !settled_ids.insert(settlement.recovery.id.clone())
            || settled_ids.len() > 250_000
            || claim.settled_interest_recovery_ids.len() >= MAX_REVIEW_EVENTS
        {
            return Err(
                "interest recovery ID was already settled or identity admission exceeded".into(),
            );
        }
        allocated(
            claim.initial_accrued_interest - claim.interest_recovery_cash_collected,
            settlement.amount,
        )?;
        let applied = if claim.outstanding_accrued_interest == 0.0 {
            0.0
        } else {
            if settlement.amount != 0.0
                && claim.outstanding_accrued_interest.is_sign_positive()
                    != settlement.amount.is_sign_positive()
            {
                return Err(
                    "interest recovery cash sign disagrees with remaining signed right".into(),
                );
            }
            settlement.amount.signum()
                * settlement
                    .amount
                    .abs()
                    .min(claim.outstanding_accrued_interest.abs())
        };
        let excess = settlement.amount - applied;
        claim.outstanding_accrued_interest = subtract(claim.outstanding_accrued_interest, applied)?;
        claim.interest_recovery_applied += applied;
        claim.interest_recovery_cash_collected += settlement.amount;
        claim
            .settled_interest_recovery_ids
            .push(settlement.recovery.clone());
        let excess_account = if settlement.excess_cash == ExcessCashRecovery::CreditAllowance {
            closing.allowance += excess;
            nonnegative(closing.allowance)?;
            AccountCode::Allowance
        } else {
            AccountCode::RecoveryIncome
        };
        let before = intents.len();
        emit(
            &context,
            &mut intents,
            "reviewed_physical_interest_recovery_cash",
            Some(&settlement.claim_id),
            &settlement.source,
            &[
                (AccountCode::Cash, settlement.amount),
                (AccountCode::DefaultAccruedInterest, -applied),
                (excess_account, -excess),
            ],
        )?;
        if let Some(intent) = intents.get_mut(before) {
            intent.policy = settlement.policy_review.clone();
        }
        cash += settlement.amount;
        applied_total += applied;
        excess_total += excess;
        for v in [cash, applied_total, excess_total] {
            amount(v)?;
        }
    }
    let mut closing_bytes = 0;
    for claim in &closing.claims {
        validate_claim(claim, &context, &mut closing_bytes)?;
    }
    Ok(InterestRecoveryResult {
        schema: request.schema.clone(),
        closing,
        intents,
        interest_cash: cash,
        applied_to_rights: applied_total,
        excess_cash: excess_total,
    })
}
