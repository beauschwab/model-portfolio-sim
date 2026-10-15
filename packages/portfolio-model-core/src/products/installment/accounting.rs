//! One physical installment economic owner and reviewed original-yield/credit intents.
//! This component produces no second principal model or independent daily ledger.
use crate::{
    cashflow::{actual_year_fraction, validate_ordinal, CashflowMeasure},
    consumer_contract::ConsumerState,
    consumer_installment::{self, InstallmentAction, InstallmentRequest, InstallmentResult},
    credit_accounting::{
        self as credit, AccountCode, AccountingPolicy, AccountingState, DefaultInstruction,
        Identity, JournalIntent, LiveState, OriginDefaultDelta, PostingIntent, RecoveryPoint,
        ReviewedAllowance, ReviewedChargeoff,
    },
    dated_income::{self, DatedIncomeRequest, DatedIncomeResult, IncomeEvent},
    limits::MAX_MONETARY_AMOUNT,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
pub const SCHEMA: &str = "consumer-credit-accounting-1";
pub const MAX_DAYS: u32 = 1080;
pub const MAX_REVIEWS: usize = 64;
pub const MAX_WORK: usize = 2_000_000;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub identity: Identity,
    pub original_yield_anchor: Identity,
    pub income_review: Identity,
    /// Supplied gross purchase basis; no purchased-credit-deteriorated gross-up.
    pub accounting_basis: String,
    pub purchase_ordinal: i32,
    pub principal: f64,
    pub cost: f64,
    pub original_annual_effective_rate: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub accounting: AccountingPolicy,
    pub recovery_review: Identity,
    pub cost_review: Identity,
    pub original_income: String,
    pub terminal_basis: String,
    pub recovery_cost: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub contract: InstallmentRequest,
    pub path_id: String,
    pub economic_source: Identity,
    pub acquisition: Acquisition,
    pub policy: Policy,
    pub claim_id: String,
    pub reporting_end_ordinal: i32,
    pub opening_allowance: f64,
    pub allowance_reviews: Vec<ReviewedAllowance>,
    pub chargeoffs: Vec<ReviewedChargeoff>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Daily {
    pub ordinal: i32,
    pub economic_state: ConsumerState,
    pub owned: AccountingState,
    pub effective_interest: f64,
    pub contractual_interest: f64,
    pub cash_delta: f64,
    pub earnings_delta: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct EventLineage {
    pub ordinal: i32,
    pub phase: String,
    pub intent_index: usize,
    pub product_event_ids: Vec<String>,
    pub original_yield_anchor: Identity,
    pub economic_source: Identity,
    pub path_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryDecomposition {
    pub ordinal: i32,
    pub gross_cash: f64,
    pub cost_cash: f64,
    pub net_cash: f64,
    pub principal_only_loss: f64,
    pub total_economic_loss: f64,
    pub cost_review: Identity,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub economic: InstallmentResult,
    pub income_prefix_request: DatedIncomeRequest,
    pub income_prefix: DatedIncomeResult,
    pub acquisition_opening: AccountingState,
    pub intents: Vec<JournalIntent>,
    pub lineage: Vec<EventLineage>,
    pub daily: Vec<Daily>,
    pub reporting: Daily,
    pub closing: AccountingState,
    pub recovery: Option<RecoveryDecomposition>,
    pub admitted_work: usize,
    pub mixed_ledger_adoption: bool,
    pub scope: &'static str,
}
fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err("consumer credit finite bounded money required".into())
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if (a - b).abs() <= 1e-9 + 1e-12 * b.abs() {
        Ok(())
    } else {
        Err("consumer credit economic/accounting ownership mismatch".into())
    }
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("consumer credit bounded explicit identity required".into())
    } else {
        Ok(())
    }
}
fn identity(x: &Identity) -> Result<(), String> {
    text(&x.id)?;
    text(&x.revision)
}
fn bounded<T: Serialize>(v: &T, cap: usize) -> Result<(), String> {
    struct Count {
        n: usize,
        cap: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.n = self
                .n
                .checked_add(b.len())
                .ok_or_else(|| std::io::Error::other("consumer credit byte overflow"))?;
            if self.n > self.cap {
                return Err(std::io::Error::other(
                    "consumer credit retained byte admission exceeded",
                ));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count { n: 0, cap }, v).map_err(|e| e.to_string())
}
fn validate(r: &Request) -> Result<(i32, usize), String> {
    consumer_installment::validate_request(&r.contract)?;
    let c = &r.contract;
    let a = &r.acquisition;
    if r.schema != SCHEMA
        || c.identity.measure != CashflowMeasure::Physical
        || c.horizon_days == 0
        || c.horizon_days > MAX_DAYS
        || c.initial_state != ConsumerState::Performing
        || c.opening_principal != c.original_principal
        || c.opening_accrued_interest != 0.
        || c.opening_unrecognized_interest != 0.
        || c.opening_fees != 0.
        || c.prepayment_penalty_fraction != 0.
        || a.purchase_ordinal != c.as_of_ordinal
        || a.principal != c.opening_principal
        || a.accounting_basis != "supplied_non_pcd_gross_cost-1"
        || a.cost <= 0.
        || r.opening_allowance < 0.
        || r.policy.original_income != "frozen_original_eir_until_exit-1"
        || r.policy.terminal_basis != "recognize_in_earnings_at_payoff-1"
        || r.policy.recovery_cost != "gross_recovery_with_credit_loss_cost-1"
        || r.economic_source.revision != c.identity.source_revision
        || r.allowance_reviews.len() + r.chargeoffs.len() > MAX_REVIEWS
    {
        return Err(
            "consumer credit exclusive acquired performing physical policy/domain required".into(),
        );
    }
    for step in &c.steps {
        if !matches!(
            step.action,
            InstallmentAction::Payment { .. }
                | InstallmentAction::Payoff {}
                | InstallmentAction::Default {}
        ) {
            return Err(
                "consumer credit modifications/fees/nonaccrual require separate ownership".into(),
            );
        }
    }
    for x in [a.principal, a.cost, r.opening_allowance] {
        money(x)?;
    }
    if !a.original_annual_effective_rate.is_finite()
        || !(-0.99..=10.).contains(&a.original_annual_effective_rate)
    {
        return Err("consumer credit original effective yield domain required".into());
    }
    for id in [
        &a.identity,
        &a.original_yield_anchor,
        &a.income_review,
        &r.economic_source,
        &r.policy.accounting.identity,
        &r.policy.recovery_review,
        &r.policy.cost_review,
    ] {
        identity(id)?;
    }
    text(&r.path_id)?;
    text(&r.claim_id)?;
    let end = c
        .as_of_ordinal
        .checked_add(c.horizon_days as i32)
        .ok_or("consumer credit date overflow")?;
    validate_ordinal(end)?;
    validate_ordinal(r.reporting_end_ordinal)?;
    if r.reporting_end_ordinal < c.as_of_ordinal || r.reporting_end_ordinal > end {
        return Err("consumer credit report outside full source tail".into());
    }
    let mut ids = HashSet::new();
    for review in &r.allowance_reviews {
        identity(&review.review)?;
        money(review.target_amount)?;
        if review.recognition_ordinal <= c.as_of_ordinal
            || review.recognition_ordinal > end
            || !ids.insert((&review.review.id, &review.review.revision))
            || review.target_amount < 0.
        {
            return Err("consumer credit duplicate/outside allowance review".into());
        }
    }
    for review in &r.chargeoffs {
        identity(&review.review)?;
        for amount in [
            review.principal_amount,
            review.book_adjustment_amount,
            review.accrued_interest_reversal,
        ] {
            money(amount)?;
        }
        if review.claim_id != r.claim_id
            || review.recognition_ordinal <= c.as_of_ordinal
            || review.recognition_ordinal > end
            || !ids.insert((&review.review.id, &review.review.revision))
            || review.principal_amount < 0.
        {
            return Err("consumer credit duplicate/outside chargeoff review".into());
        }
    }
    // The shared one-day accounting operator admits one allowance review/day.
    let mut days = HashSet::new();
    for review in &r.allowance_reviews {
        if !days.insert(review.recognition_ordinal) {
            return Err("consumer credit ambiguous same-day allowance review".into());
        }
    }
    let work = (c.horizon_days as usize)
        .checked_mul(
            c.steps.len()
                + c.payment_dates.len()
                + r.allowance_reviews.len()
                + r.chargeoffs.len()
                + 64,
        )
        .ok_or("consumer credit work overflow")?;
    if work > MAX_WORK {
        return Err("consumer credit work admission exceeded".into());
    }
    bounded(r, 1024 * 1024)?;
    Ok((end, work))
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
/// The check closure is native cooperative control only; it supplies no money.
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let (end, work) = validate(r)?;
    check()?;
    let economic = consumer_installment::project(&r.contract)?;
    check()?;
    let default = economic.default.as_ref();
    let terminal_cash = economic.cashflows.iter().find(|e| {
        e.kind != "recovery"
            && e.principal != 0.
            && economic
                .points
                .iter()
                .any(|p| p.ordinal == e.ordinal && p.principal == 0.)
    });
    let exit = default
        .map(|d| d.ordinal)
        .or_else(|| terminal_cash.map(|c| c.ordinal))
        .unwrap_or(end);
    if economic.cashflows.iter().any(|c| c.fees != 0.)
        || economic
            .recognition
            .iter()
            .any(|c| c.start_ordinal == c.end_ordinal && c.interest != 0.)
        || economic.closing_unrecognized_interest != 0.
        || economic.closing_fees != 0.
    {
        return Err("consumer credit unsupported fee/unrecognized ownership".into());
    }
    if economic.closing_principal != 0. || economic.closing_accrued_interest != 0. {
        return Err("consumer credit complete contractual live tail required; retained default claims are separate".into());
    }
    let recovery = if let Some(d) = default {
        if d.unrecognized_contract_interest != 0.
            || d.fees != 0.
            || d.gross_recovery > d.principal
            || d.recovery_cost > d.gross_recovery
        {
            return Err(
                "consumer credit recovery surplus/uncovered cost/nonaccrual unsupported".into(),
            );
        }
        near(d.net_recovery, d.gross_recovery - d.recovery_cost)?;
        near(
            d.principal_loss,
            d.principal - d.gross_recovery + d.recovery_cost,
        )?;
        Some(RecoveryDecomposition {
            ordinal: d.recovery_ordinal,
            gross_cash: d.gross_recovery,
            cost_cash: d.recovery_cost,
            net_cash: d.net_recovery,
            principal_only_loss: d.principal - d.gross_recovery,
            total_economic_loss: d.principal_loss,
            cost_review: r.policy.cost_review.clone(),
        })
    } else {
        None
    };
    let mut cash_by_day: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, c) in economic.cashflows.iter().enumerate() {
        cash_by_day.entry(c.ordinal).or_default().push(i);
    }
    // Gross/cost decomposition can have zero net economic cash and therefore no
    // producer cash event. Its default observation remains the actual source.
    let mut recovery_origin_ids = default
        .map(|d| vec![format!("default:{}", d.ordinal)])
        .unwrap_or_default();
    recovery_origin_ids.extend(
        economic
            .cashflows
            .iter()
            .filter(|c| c.kind == "recovery")
            .map(|c| c.event_id.clone()),
    );
    let mut events = Vec::new();
    for day in r.contract.as_of_ordinal + 1..=exit {
        if (day - r.contract.as_of_ordinal) % 64 == 0 {
            check()?;
        }
        let mut earned = 0.;
        for segment in &economic.recognition {
            if segment.start_ordinal < day && segment.end_ordinal >= day {
                if segment.end_ordinal == segment.start_ordinal {
                    return Err("consumer credit cash-basis recognition unsupported".into());
                }
                earned = money(
                    earned
                        + segment.interest
                            * actual_year_fraction(day - 1, day, r.contract.day_count)?
                            / actual_year_fraction(
                                segment.start_ordinal,
                                segment.end_ordinal,
                                r.contract.day_count,
                            )?,
                )?;
            }
        }
        let (mut pc, mut ic) = (0., 0.);
        if day != exit || terminal_cash.is_none() {
            for index in cash_by_day.get(&day).into_iter().flatten() {
                let cash = &economic.cashflows[*index];
                if cash.kind != "recovery" {
                    pc = money(pc + cash.principal)?;
                    ic = money(ic + cash.interest)?;
                }
            }
        }
        events.push(IncomeEvent {
            event_id: format!("income:{day}"),
            source: dated_income::Identity {
                id: r.economic_source.id.clone(),
                revision: r.economic_source.revision.clone(),
            },
            payment_ordinal: day,
            contractual_interest_earned: earned,
            principal_cash: pc,
            interest_cash: ic,
            integral_fee_cash: 0.,
            nonintegral_fee_cash: 0.,
            yield_change: None,
        });
    }
    let income_request = DatedIncomeRequest {
        schema: dated_income::SCHEMA.into(),
        contract: dated_income::Identity {
            id: r.contract.identity.original_loan_id.clone(),
            revision: r.contract.identity.source_revision.clone(),
        },
        original_yield_anchor: dated_income::Identity {
            id: r.acquisition.original_yield_anchor.id.clone(),
            revision: r.acquisition.original_yield_anchor.revision.clone(),
        },
        policy: dated_income::Identity {
            id: r.acquisition.income_review.id.clone(),
            revision: r.acquisition.income_review.revision.clone(),
        },
        currency: r.contract.identity.currency.clone(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Physical,
        as_of_ordinal: r.contract.as_of_ordinal,
        full_contract_end_ordinal: end,
        opening_principal: r.acquisition.principal,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: r.acquisition.cost,
        original_annual_effective_rate: r.acquisition.original_annual_effective_rate,
        opening_annual_effective_rate: r.acquisition.original_annual_effective_rate,
        events,
    };
    let income = dated_income::evaluate(&income_request)?;
    check()?;
    let mut owned = AccountingState {
        live: [
            LiveState {
                principal: r.acquisition.principal,
                undrawn: 0.,
                accrued_interest: 0.,
                book_adjustment: r.acquisition.cost - r.acquisition.principal,
            },
            LiveState::default(),
            LiveState::default(),
        ],
        allowance: r.opening_allowance,
        claims: Vec::new(),
    };
    let acquisition_opening = owned.clone();
    let mut intents = Vec::new();
    let mut lineage = Vec::new();
    let mut daily = vec![Daily {
        ordinal: r.contract.as_of_ordinal,
        economic_state: ConsumerState::Performing,
        owned: owned.clone(),
        effective_interest: 0.,
        contractual_interest: 0.,
        cash_delta: 0.,
        earnings_delta: 0.,
    }];
    for day in r.contract.as_of_ordinal + 1..=end {
        check()?;
        let begin = intents.len();
        let mut effective = 0.;
        let mut earned = 0.;
        if day <= exit {
            let row = &income.rows[(day - r.contract.as_of_ordinal - 1) as usize];
            effective = row.effective_interest;
            earned = row.contractual_interest_earned;
            owned.live[0].accrued_interest = money(owned.live[0].accrued_interest + earned)?;
            owned.live[0].book_adjustment =
                money(owned.live[0].book_adjustment + row.basis_amortization)?;
            emit(
                r,
                day,
                "product_original_effective_interest",
                &[
                    (AccountCode::LiveAccruedInterest, earned),
                    (AccountCode::LiveBookAdjustment, row.basis_amortization),
                    (AccountCode::InterestIncome, -effective),
                ],
                economic
                    .recognition
                    .iter()
                    .filter(|s| s.start_ordinal < day && s.end_ordinal >= day)
                    .map(|s| s.origin_id.clone())
                    .collect(),
                &mut intents,
                &mut lineage,
            )?;
            if let Some(intent) = intents.last_mut().filter(|i| {
                i.event == "product_original_effective_interest" && i.recognition_ordinal == day
            }) {
                intent.policy = r.acquisition.income_review.clone();
            }
            for index in cash_by_day.get(&day).into_iter().flatten() {
                let cash = &economic.cashflows[*index];
                if cash.kind == "recovery" {
                    continue;
                }
                owned.live[0].principal = money(owned.live[0].principal - cash.principal)?;
                owned.live[0].accrued_interest =
                    money(owned.live[0].accrued_interest - cash.interest)?;
                emit(
                    r,
                    day,
                    "product_contractual_settlement",
                    &[
                        (AccountCode::Cash, cash.principal + cash.interest),
                        (AccountCode::LivePrincipal, -cash.principal),
                        (AccountCode::LiveAccruedInterest, -cash.interest),
                    ],
                    vec![cash.event_id.clone()],
                    &mut intents,
                    &mut lineage,
                )?;
                if terminal_cash.is_some_and(|t| t.event_id == cash.event_id) {
                    let basis = owned.live[0].book_adjustment;
                    owned.live[0].book_adjustment = 0.;
                    emit(
                        r,
                        day,
                        if cash.kind == "prepayment" {
                            "product_prepayment_basis_retirement"
                        } else {
                            "product_contractual_paid_off_basis_retirement"
                        },
                        &[
                            (AccountCode::LiveBookAdjustment, -basis),
                            (AccountCode::BasisRecognitionExpense, basis),
                        ],
                        vec![cash.event_id.clone()],
                        &mut intents,
                        &mut lineage,
                    )?;
                }
            }
        }
        let mut defaults = Vec::new();
        if let Some(d) = default.filter(|d| d.ordinal == day) {
            near(owned.live[0].principal, d.principal)?;
            near(
                owned.live[0].accrued_interest,
                d.recognized_accrued_interest,
            )?;
            let rec = recovery
                .as_ref()
                .ok_or("consumer credit recovery decomposition missing")?;
            defaults.push(DefaultInstruction {
                claim_id: r.claim_id.clone(),
                economic_source: r.economic_source.clone(),
                recovery_schedule_review: r.policy.recovery_review.clone(),
                by_origin: [
                    OriginDefaultDelta {
                        principal: owned.live[0].principal,
                        undrawn: 0.,
                        contingent_draw: 0.,
                        accrued_interest: owned.live[0].accrued_interest,
                        book_adjustment: owned.live[0].book_adjustment,
                    },
                    OriginDefaultDelta::default(),
                    OriginDefaultDelta::default(),
                ],
                economic_principal_loss: rec.principal_only_loss,
                complete_recovery_tail: if rec.gross_cash == 0. {
                    Vec::new()
                } else {
                    vec![RecoveryPoint {
                        payment_ordinal: rec.ordinal,
                        amount: rec.gross_cash,
                    }]
                },
            });
        }
        let shared = credit::evaluate(&credit::CreditAccountingRequest {
            schema: credit::SCHEMA.into(),
            measure: CashflowMeasure::Physical,
            currency: r.contract.identity.currency.clone(),
            monetary_unit: "currency_units".into(),
            position_id: r.contract.identity.position_id.clone(),
            recognition_ordinal: day,
            reporting_end_ordinal: end,
            policy: r.policy.accounting.clone(),
            opening: owned,
            defaults,
            chargeoffs: r
                .chargeoffs
                .iter()
                .filter(|x| x.recognition_ordinal == day)
                .cloned()
                .collect(),
            allowance_review: r
                .allowance_reviews
                .iter()
                .find(|x| x.recognition_ordinal == day)
                .cloned(),
        })?;
        owned = shared.closing;
        for intent in shared.intents {
            lineage.push(EventLineage {
                ordinal: day,
                phase: intent.event.clone(),
                intent_index: intents.len(),
                product_event_ids: if default.is_some_and(|d| d.ordinal == day) {
                    vec![format!("default:{day}")]
                } else if recovery.as_ref().is_some_and(|x| x.ordinal == day) {
                    recovery_origin_ids.clone()
                } else {
                    Vec::new()
                },
                original_yield_anchor: r.acquisition.original_yield_anchor.clone(),
                economic_source: r.economic_source.clone(),
                path_id: r.path_id.clone(),
            });
            intents.push(intent);
        }
        if let Some(rec) = recovery
            .as_ref()
            .filter(|x| x.ordinal == day && x.cost_cash != 0.)
        {
            emit(
                r,
                day,
                "product_reviewed_recovery_cost",
                &[
                    (AccountCode::Cash, -rec.cost_cash),
                    (AccountCode::CreditLossExpense, rec.cost_cash),
                ],
                recovery_origin_ids.clone(),
                &mut intents,
                &mut lineage,
            )?;
            let cost_intent = intents.last_mut().ok_or("missing recovery cost intent")?;
            cost_intent.policy = r.policy.cost_review.clone();
            cost_intent.claim_id = Some(r.claim_id.clone());
        }
        let cash_delta = money(
            intents[begin..]
                .iter()
                .flat_map(|i| &i.postings)
                .filter(|p| p.account == AccountCode::Cash)
                .map(|p| p.amount)
                .sum(),
        )?;
        let earnings_delta = money(
            -intents[begin..]
                .iter()
                .flat_map(|i| &i.postings)
                .filter(|p| {
                    matches!(
                        p.account,
                        AccountCode::InterestIncome
                            | AccountCode::CreditLossExpense
                            | AccountCode::BasisRecognitionExpense
                            | AccountCode::RecoveryIncome
                    )
                })
                .map(|p| p.amount)
                .sum::<f64>(),
        )?;
        let economic_state = economic
            .points
            .iter()
            .rev()
            .find(|p| p.ordinal <= day)
            .map(|p| p.state)
            .unwrap_or(ConsumerState::Performing);
        daily.push(Daily {
            ordinal: day,
            economic_state,
            owned: owned.clone(),
            effective_interest: effective,
            contractual_interest: earned,
            cash_delta,
            earnings_delta,
        });
    }
    let projected_cash: f64 = economic
        .cashflows
        .iter()
        .map(|c| c.principal + c.interest + c.fees)
        .sum();
    near(daily.iter().map(|d| d.cash_delta).sum(), projected_cash)?;
    near(
        owned.live.iter().map(|s| s.principal).sum(),
        economic.closing_principal,
    )?;
    near(
        owned.live.iter().map(|s| s.accrued_interest).sum(),
        economic.closing_accrued_interest,
    )?;
    let reporting = daily[(r.reporting_end_ordinal - r.contract.as_of_ordinal) as usize].clone();
    let result=ResultData{schema:SCHEMA,raw_request:r.clone(),economic,income_prefix_request:income_request,income_prefix:income,acquisition_opening,intents,lineage,daily,reporting,closing:owned,recovery,admitted_work:work,mixed_ledger_adoption:false,scope:"one acquired supplied physical installment economic owner with original-EIR prefix and reviewed shared credit intents; no second principal projection, stochastic hazard fit, pricing history, mixed ledger adoption or independently persisted verification"};
    bounded(&result, MAX_BYTES)?;
    check()?;
    Ok(result)
}
fn emit(
    r: &Request,
    day: i32,
    event: &str,
    lines: &[(AccountCode, f64)],
    ids: Vec<String>,
    intents: &mut Vec<JournalIntent>,
    lineage: &mut Vec<EventLineage>,
) -> Result<(), String> {
    let mut net = 0.;
    let mut scale = 0.;
    let mut postings = Vec::new();
    for &(account, amount) in lines {
        money(amount)?;
        net += amount;
        scale += amount.abs();
        if amount != 0. {
            postings.push(PostingIntent { account, amount });
        }
    }
    if net.abs() > 1e-9 + 1e-12 * scale {
        return Err("consumer credit intent unbalanced".into());
    }
    if postings.is_empty() {
        return Ok(());
    }
    lineage.push(EventLineage {
        ordinal: day,
        phase: event.into(),
        intent_index: intents.len(),
        product_event_ids: ids,
        original_yield_anchor: r.acquisition.original_yield_anchor.clone(),
        economic_source: r.economic_source.clone(),
        path_id: r.path_id.clone(),
    });
    intents.push(JournalIntent {
        recognition_ordinal: day,
        event: event.into(),
        position_id: r.contract.identity.position_id.clone(),
        claim_id: None,
        policy: r.policy.accounting.identity.clone(),
        source: r.economic_source.clone(),
        postings,
    });
    Ok(())
}
