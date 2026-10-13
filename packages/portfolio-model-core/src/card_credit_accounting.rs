//! Acquired, fee-free non-grace card runoff: one native economic owner.
//! Explicit supplied physical runoff accounting; broader revolving policies
//! require separate reviewed ownership and integration.
use crate::{
    cashflow::{validate_ordinal, CashflowMeasure},
    consumer_card::{self, CardAction, CardRequest, CardResult},
    consumer_contract::ConsumerState,
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

pub const SCHEMA: &str = "card-credit-accounting-1";
pub const MAX_DAYS: u32 = 1080;
pub const MAX_REVIEWS: usize = 64;
pub const MAX_WORK: usize = 2_000_000;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) mod strict_termination;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub identity: Identity,
    pub original_yield_anchor: Identity,
    pub income_review: Identity,
    pub accounting_basis: String,
    pub purchase_ordinal: i32,
    pub principal: f64,
    pub cost: f64,
    pub original_annual_effective_rate: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub accounting: AccountingPolicy,
    pub recovery_review: Identity,
    pub cost_review: Identity,
    pub contractual_recognition: String,
    pub original_income: String,
    pub terminal_basis: String,
    pub recovery_cost: String,
    pub commitment: String,
    pub unsupported_grace_fees_draws: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Termination {
    SourceDefault,
    ReviewedZeroRightsClosure { ordinal: i32, review: Identity },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub contract: CardRequest,
    pub path_id: String,
    pub economic_source: Identity,
    pub acquisition: Acquisition,
    pub policy: Policy,
    #[serde(deserialize_with = "strict_termination::deserialize")]
    pub termination: Termination,
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
    pub accounting_status: String,
    pub owned: AccountingState,
    pub source_billed_interest: [f64; 3],
    pub source_unbilled_interest: [f64; 3],
    pub source_limit: f64,
    pub source_undrawn: f64,
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
    pub source_event_ids: Vec<String>,
    /// Card origin IDs repeat; indices plus the retained source give exact intervals.
    pub source_recognition_indices: Vec<usize>,
    pub original_yield_anchor: Identity,
    pub economic_source: Identity,
    pub path_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct StatementReceipt {
    pub source_index: usize,
    pub ordinal: i32,
    pub due_ordinal: i32,
    pub interest_recognized_again: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct CommitmentReceipt {
    pub policy: String,
    pub policy_review: Identity,
    pub retirement_ordinal: i32,
    pub retirement_kind: String,
    pub funded_contingent_draw: f64,
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
    pub economic: CardResult,
    pub income_prefix_request: DatedIncomeRequest,
    pub income_prefix: DatedIncomeResult,
    pub acquisition_opening: AccountingState,
    pub intents: Vec<JournalIntent>,
    pub lineage: Vec<EventLineage>,
    pub statement_lineage: Vec<StatementReceipt>,
    pub commitment_source: CommitmentReceipt,
    pub daily: Vec<Daily>,
    pub reporting: Daily,
    pub closing: AccountingState,
    pub recovery: Option<RecoveryDecomposition>,
    pub admitted_work: usize,
    pub mixed_ledger_adoption: bool,
    pub empirical_validation: bool,
}
fn money(v: f64) -> Result<f64, String> {
    if v.is_finite() && v.abs() <= MAX_MONETARY_AMOUNT {
        Ok(v)
    } else {
        Err("card credit finite bounded currency required".into())
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * b.abs() {
        Ok(())
    } else {
        Err("card source/accounting ownership mismatch".into())
    }
}
fn id(x: &Identity) -> Result<(), String> {
    for s in [&x.id, &x.revision] {
        label(s)?;
    }
    Ok(())
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("card credit bounded explicit identity required".into())
    } else {
        Ok(())
    }
}
fn bounded<T: Serialize>(x: &T, cap: usize) -> Result<(), String> {
    struct Count {
        n: usize,
        cap: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.n = self
                .n
                .checked_add(b.len())
                .ok_or_else(|| std::io::Error::other("card byte overflow"))?;
            if self.n > self.cap {
                return Err(std::io::Error::other(
                    "card retained byte admission exceeded",
                ));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut Count { n: 0, cap }, x).map_err(|e| e.to_string())
}
fn validate(r: &Request) -> Result<(i32, usize), String> {
    consumer_card::validate_request(&r.contract)?;
    let c = &r.contract;
    let a = &r.acquisition;
    let end = c
        .as_of_ordinal
        .checked_add(c.horizon_days as i32)
        .ok_or("card date overflow")?;
    validate_ordinal(end)?;
    validate_ordinal(r.reporting_end_ordinal)?;
    if r.schema != SCHEMA
        || c.identity.measure != CashflowMeasure::Physical
        || c.horizon_days == 0
        || c.horizon_days > MAX_DAYS
        || c.initial_state != ConsumerState::Performing
        || c.opening_principal.iter().sum::<f64>() <= 0.
        || a.principal != c.opening_principal.iter().sum::<f64>()
        || a.purchase_ordinal != c.as_of_ordinal
        || a.cost <= 0.
        || r.opening_allowance < 0.
        || a.accounting_basis != "supplied_non_pcd_gross_cost-1"
        || r.economic_source.revision != c.identity.source_revision
        || c.initial_purchase_grace
        || c.regain_grace_after_full_statement_payment
        || c.opening_contingent_purchase_interest != 0.
        || c.opening_billed_interest
            .iter()
            .chain(&c.opening_unbilled_interest)
            .chain(&c.opening_unrecognized_billed_interest)
            .chain(&c.opening_fees)
            .any(|x| *x != 0.)
        || r.reporting_end_ordinal < c.as_of_ordinal
        || r.reporting_end_ordinal > end
        || r.allowance_reviews.len() + r.chargeoffs.len() > MAX_REVIEWS
        || r.policy.contractual_recognition != "raw_card_non_grace_earned_then_billed-1"
        || r.policy.original_income != "frozen_original_account_eir_until_exit-1"
        || r.policy.terminal_basis != "recognize_in_earnings_at_reviewed_closure-1"
        || r.policy.recovery_cost != "gross_recovery_with_credit_loss_cost-1"
        || r.policy.commitment != "retained_source_undrawn_no_ccf_no_draw-1"
        || r.policy.unsupported_grace_fees_draws != "reject"
    {
        return Err(
            "card credit acquired physical no-grace/no-fee/no-draw contract required".into(),
        );
    }
    for value in [a.principal, a.cost, r.opening_allowance] {
        money(value)?;
    }
    if !a.original_annual_effective_rate.is_finite()
        || !(-0.99..=10.).contains(&a.original_annual_effective_rate)
    {
        return Err("card original EIR domain required".into());
    }
    for value in [
        &a.identity,
        &a.original_yield_anchor,
        &a.income_review,
        &r.economic_source,
        &r.policy.identity,
        &r.policy.accounting.identity,
        &r.policy.recovery_review,
        &r.policy.cost_review,
    ] {
        id(value)?;
    }
    label(&r.path_id)?;
    label(&r.claim_id)?;
    let closure = match &r.termination {
        Termination::SourceDefault => None,
        Termination::ReviewedZeroRightsClosure { ordinal, review } => {
            id(review)?;
            validate_ordinal(*ordinal)?;
            if *ordinal <= c.as_of_ordinal || *ordinal > end {
                return Err("card reviewed closure date outside source".into());
            }
            Some(*ordinal)
        }
    };
    let mut limits = 0;
    for step in &c.steps {
        if step.ordinal <= c.as_of_ordinal {
            return Err("card action must follow acquisition".into());
        }
        match step.action {
            CardAction::Payment { .. } | CardAction::Default {} => {}
            CardAction::SetLimit { amount: 0. } if closure == Some(step.ordinal) => {
                limits += 1;
            }
            _ => return Err("card credit unsupported draw/fee/state/rate/limit ownership".into()),
        }
        if closure.is_some_and(|d| step.ordinal > d) {
            return Err("card action after reviewed closure".into());
        }
    }
    if closure.is_some() && limits != 1 {
        return Err("card closure needs one actual zero-limit action".into());
    }
    let mut seen = HashSet::new();
    let mut days = HashSet::new();
    for x in &r.allowance_reviews {
        id(&x.review)?;
        money(x.target_amount)?;
        if x.target_amount < 0.
            || x.recognition_ordinal <= c.as_of_ordinal
            || x.recognition_ordinal > end
            || !seen.insert((&x.review.id, &x.review.revision))
            || !days.insert(x.recognition_ordinal)
        {
            return Err("card ambiguous/outside allowance review".into());
        }
    }
    for x in &r.chargeoffs {
        id(&x.review)?;
        for v in [
            x.principal_amount,
            x.book_adjustment_amount,
            x.accrued_interest_reversal,
        ] {
            money(v)?;
        }
        if x.claim_id != r.claim_id
            || x.principal_amount < 0.
            || x.recognition_ordinal <= c.as_of_ordinal
            || x.recognition_ordinal > end
            || !seen.insert((&x.review.id, &x.review.revision))
        {
            return Err("card duplicate/outside chargeoff review".into());
        }
    }
    // Input and generated rows are indexed; no days × all-recognition scan.
    let work = (c.horizon_days as usize + 1)
        .checked_mul(
            c.steps.len() + c.cycles.len() + r.allowance_reviews.len() + r.chargeoffs.len() + 72,
        )
        .ok_or("card work overflow")?;
    if work > MAX_WORK {
        return Err("card accounting work admission exceeded".into());
    }
    bounded(r, 1024 * 1024)?;
    Ok((end, work))
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let (end, work) = validate(r)?;
    check()?;
    let economic = consumer_card::simulate(&r.contract)?;
    check()?;
    let exit = match &r.termination {
        Termination::SourceDefault => {
            economic
                .default
                .as_ref()
                .ok_or("card source default missing")?
                .ordinal
        }
        Termination::ReviewedZeroRightsClosure { ordinal, .. } => {
            if economic.default.is_some() {
                return Err("card default cannot masquerade as reviewed closure".into());
            }
            let point = &economic.points[(*ordinal - r.contract.as_of_ordinal) as usize];
            if point
                .principal
                .iter()
                .chain(&point.billed_interest)
                .chain(&point.unbilled_interest)
                .chain(&point.fees)
                .any(|v| *v != 0.)
                || point.limit != 0.
                || point.undrawn != 0.
            {
                return Err(
                    "card closure still owns principal/interest/fee/commitment rights".into(),
                );
            }
            *ordinal
        }
    };
    if economic.drawn_principal != 0.
        || [
            economic.closing_principal,
            economic.closing_interest,
            economic.closing_unbilled_interest,
            economic.closing_unrecognized_billed_interest,
            economic.closing_fees,
            economic.closing_grace_contingent_interest,
        ]
        .iter()
        .any(|v| *v != 0.)
        || economic
            .cashflows
            .iter()
            .any(|x| x.fees != 0. || x.kind == "draw")
        || economic
            .recognition
            .iter()
            .any(|x| x.start_ordinal >= x.end_ordinal || x.end_ordinal - x.start_ordinal != 1)
        || economic
            .points
            .last()
            .is_none_or(|x| x.limit != 0. || x.undrawn != 0.)
    {
        return Err("card complete zero-rights/commitment economic tail required".into());
    }
    let recovery = if let Some(d) = &economic.default {
        if d.unrecognized_contract_interest != 0.
            || d.fees != 0.
            || d.gross_recovery > d.principal
            || d.recovery_cost > d.gross_recovery
        {
            return Err("card default grace/fee/surplus/uncovered cost unsupported".into());
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
    let mut cash = BTreeMap::<i32, Vec<usize>>::new();
    let mut earned = BTreeMap::<i32, (f64, Vec<usize>)>::new();
    for (index, x) in economic.cashflows.iter().enumerate() {
        cash.entry(x.ordinal).or_default().push(index);
    }
    for (index, x) in economic.recognition.iter().enumerate() {
        if index % 256 == 0 {
            check()?;
        }
        let v = earned.entry(x.end_ordinal).or_default();
        v.0 = money(v.0 + x.interest)?;
        v.1.push(index);
    }
    let mut income_events = Vec::new();
    for day in r.contract.as_of_ordinal + 1..=exit {
        if (day - r.contract.as_of_ordinal) % 64 == 0 {
            check()?;
        }
        let mut pc = 0.;
        let mut ic = 0.;
        if day != exit || economic.default.is_some() {
            for i in cash.get(&day).into_iter().flatten() {
                let x = &economic.cashflows[*i];
                if x.kind != "recovery" {
                    pc = money(pc + x.principal)?;
                    ic = money(ic + x.interest)?;
                }
            }
        }
        income_events.push(IncomeEvent {
            event_id: format!("card-income:{day}"),
            source: income_id(&r.economic_source),
            payment_ordinal: day,
            contractual_interest_earned: earned.get(&day).map_or(0., |v| v.0),
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
        original_yield_anchor: income_id(&r.acquisition.original_yield_anchor),
        policy: income_id(&r.acquisition.income_review),
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
        events: income_events,
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
    let opening = owned.clone();
    let mut sink = Sink {
        intents: Vec::new(),
        lineage: Vec::new(),
    };
    let mut daily = vec![day_row(&economic, 0, owned.clone(), (0., 0., 0., 0.), exit)];
    let mut recovery_origins = economic
        .default
        .as_ref()
        .map(|d| vec![format!("default:{}", d.ordinal)])
        .unwrap_or_default();
    recovery_origins.extend(
        economic
            .cashflows
            .iter()
            .filter(|x| x.kind == "recovery")
            .map(|x| x.event_id.clone()),
    );
    for day in r.contract.as_of_ordinal + 1..=end {
        check()?;
        let begin = sink.intents.len();
        let mut effective = 0.;
        let mut coupon = 0.;
        let recognition = earned.get(&day).map(|x| x.1.clone()).unwrap_or_default();
        if day <= exit {
            let row = &income.rows[(day - r.contract.as_of_ordinal - 1) as usize];
            effective = row.effective_interest;
            coupon = row.contractual_interest_earned;
            owned.live[0].accrued_interest = money(owned.live[0].accrued_interest + coupon)?;
            owned.live[0].book_adjustment =
                money(owned.live[0].book_adjustment + row.basis_amortization)?;
            sink.emit(
                r,
                day,
                "card_original_effective_interest",
                &r.acquisition.income_review,
                (vec![], recognition.clone()),
                &[
                    (AccountCode::LiveAccruedInterest, coupon),
                    (AccountCode::LiveBookAdjustment, row.basis_amortization),
                    (AccountCode::InterestIncome, -effective),
                ],
            )?;
            for i in cash.get(&day).into_iter().flatten() {
                let x = &economic.cashflows[*i];
                if x.kind == "recovery" {
                    continue;
                }
                owned.live[0].principal = money(owned.live[0].principal - x.principal)?;
                owned.live[0].accrued_interest =
                    money(owned.live[0].accrued_interest - x.interest)?;
                sink.emit(
                    r,
                    day,
                    "card_contractual_settlement",
                    &r.policy.identity,
                    (vec![x.event_id.clone()], vec![]),
                    &[
                        (AccountCode::Cash, x.principal + x.interest),
                        (AccountCode::LivePrincipal, -x.principal),
                        (AccountCode::LiveAccruedInterest, -x.interest),
                    ],
                )?;
            }
        }
        if day == exit && economic.default.is_none() {
            let basis = owned.live[0].book_adjustment;
            owned.live[0].book_adjustment = 0.;
            let review = match &r.termination {
                Termination::ReviewedZeroRightsClosure { review, .. } => review,
                _ => return Err("closure review missing".into()),
            };
            sink.emit(
                r,
                day,
                "card_reviewed_closure_basis_retirement",
                review,
                (vec![format!("closure:{day}")], vec![]),
                &[
                    (AccountCode::LiveBookAdjustment, -basis),
                    (AccountCode::BasisRecognitionExpense, basis),
                ],
            )?;
        }
        let mut defaults = Vec::new();
        if let Some(d) = economic.default.as_ref().filter(|d| d.ordinal == day) {
            near(owned.live[0].principal, d.principal)?;
            near(
                owned.live[0].accrued_interest,
                d.recognized_accrued_interest,
            )?;
            let rec = recovery.as_ref().ok_or("card recovery missing")?;
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
                    vec![]
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
            let ids = if economic.default.as_ref().is_some_and(|d| d.ordinal == day) {
                vec![format!("default:{day}")]
            } else if recovery.as_ref().is_some_and(|d| d.ordinal == day) {
                recovery_origins.clone()
            } else {
                vec![]
            };
            sink.append(r, intent, ids, vec![]);
        }
        if let Some(rec) = recovery
            .as_ref()
            .filter(|x| x.ordinal == day && x.cost_cash != 0.)
        {
            sink.emit(
                r,
                day,
                "card_reviewed_recovery_cost",
                &r.policy.cost_review,
                (recovery_origins.clone(), vec![]),
                &[
                    (AccountCode::Cash, -rec.cost_cash),
                    (AccountCode::CreditLossExpense, rec.cost_cash),
                ],
            )?;
            sink.intents
                .last_mut()
                .ok_or("missing card cost intent")?
                .claim_id = Some(r.claim_id.clone());
        }
        let source = &economic.points[(day - r.contract.as_of_ordinal) as usize];
        near(
            owned.live.iter().map(|x| x.principal).sum(),
            source.principal.iter().sum(),
        )?;
        near(
            owned.live.iter().map(|x| x.accrued_interest).sum(),
            source
                .billed_interest
                .iter()
                .chain(&source.unbilled_interest)
                .sum(),
        )?;
        let pc = money(
            sink.intents[begin..]
                .iter()
                .flat_map(|x| &x.postings)
                .filter(|x| x.account == AccountCode::Cash)
                .map(|x| x.amount)
                .sum(),
        )?;
        let profit = money(
            -sink.intents[begin..]
                .iter()
                .flat_map(|x| &x.postings)
                .filter(|x| {
                    matches!(
                        x.account,
                        AccountCode::InterestIncome
                            | AccountCode::CreditLossExpense
                            | AccountCode::BasisRecognitionExpense
                            | AccountCode::RecoveryIncome
                    )
                })
                .map(|x| x.amount)
                .sum::<f64>(),
        )?;
        daily.push(day_row(
            &economic,
            (day - r.contract.as_of_ordinal) as usize,
            owned.clone(),
            (effective, coupon, pc, profit),
            exit,
        ));
    }
    near(
        daily.iter().map(|d| d.cash_delta).sum(),
        economic
            .cashflows
            .iter()
            .map(|x| x.principal + x.interest + x.fees)
            .sum(),
    )?;
    let reporting = daily[(r.reporting_end_ordinal - r.contract.as_of_ordinal) as usize].clone();
    let statements = economic
        .statements
        .iter()
        .enumerate()
        .map(|(index, s)| StatementReceipt {
            source_index: index,
            ordinal: s.ordinal,
            due_ordinal: s.due_ordinal,
            interest_recognized_again: false,
        })
        .collect();
    let result = ResultData {
        schema: SCHEMA,
        raw_request: r.clone(),
        commitment_source: CommitmentReceipt {
            policy: r.policy.commitment.clone(),
            policy_review: r.policy.identity.clone(),
            retirement_ordinal: exit,
            retirement_kind: if economic.default.is_some() {
                "source_default"
            } else {
                "reviewed_zero_rights_closure"
            }
            .into(),
            funded_contingent_draw: 0.,
        },
        economic,
        income_prefix_request: income_request,
        income_prefix: income,
        acquisition_opening: opening,
        intents: sink.intents,
        lineage: sink.lineage,
        statement_lineage: statements,
        daily,
        reporting,
        closing: owned,
        recovery,
        admitted_work: work,
        mixed_ledger_adoption: false,
        empirical_validation: false,
    };
    bounded(&result, MAX_BYTES)?;
    check()?;
    Ok(result)
}
fn income_id(x: &Identity) -> dated_income::Identity {
    dated_income::Identity {
        id: x.id.clone(),
        revision: x.revision.clone(),
    }
}
#[derive(Default)]
struct Sink {
    intents: Vec<JournalIntent>,
    lineage: Vec<EventLineage>,
}
impl Sink {
    fn append(
        &mut self,
        r: &Request,
        intent: JournalIntent,
        ids: Vec<String>,
        recognition: Vec<usize>,
    ) {
        self.lineage.push(EventLineage {
            ordinal: intent.recognition_ordinal,
            phase: intent.event.clone(),
            intent_index: self.intents.len(),
            source_event_ids: ids,
            source_recognition_indices: recognition,
            original_yield_anchor: r.acquisition.original_yield_anchor.clone(),
            economic_source: r.economic_source.clone(),
            path_id: r.path_id.clone(),
        });
        self.intents.push(intent);
    }
    fn emit(
        &mut self,
        r: &Request,
        day: i32,
        event: &str,
        review: &Identity,
        origins: (Vec<String>, Vec<usize>),
        lines: &[(AccountCode, f64)],
    ) -> Result<(), String> {
        let mut net = 0.;
        let mut scale = 0.;
        let mut postings = Vec::new();
        for &(account, value) in lines {
            money(value)?;
            net += value;
            scale += value.abs();
            if value != 0. {
                postings.push(PostingIntent {
                    account,
                    amount: value,
                });
            }
        }
        if net.abs() > 1e-9 + 1e-12 * scale {
            return Err("card intent unbalanced".into());
        }
        if !postings.is_empty() {
            self.append(
                r,
                JournalIntent {
                    recognition_ordinal: day,
                    event: event.into(),
                    position_id: r.contract.identity.position_id.clone(),
                    claim_id: None,
                    policy: review.clone(),
                    source: r.economic_source.clone(),
                    postings,
                },
                origins.0,
                origins.1,
            );
        }
        Ok(())
    }
}
fn day_row(
    economic: &CardResult,
    index: usize,
    owned: AccountingState,
    flows: (f64, f64, f64, f64),
    exit: i32,
) -> Daily {
    let point = &economic.points[index];
    Daily {
        ordinal: point.ordinal,
        economic_state: point.state,
        accounting_status: if index == 0 {
            "acquired"
        } else if point.ordinal >= exit {
            if economic.default.is_some() {
                "default_claims"
            } else {
                "reviewed_closed"
            }
        } else {
            "live"
        }
        .into(),
        owned,
        source_billed_interest: point.billed_interest,
        source_unbilled_interest: point.unbilled_interest,
        source_limit: point.limit,
        source_undrawn: point.undrawn,
        effective_interest: flows.0,
        contractual_interest: flows.1,
        cash_delta: flows.2,
        earnings_delta: flows.3,
    }
}
