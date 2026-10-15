//! Reviewed book actions on copied native economic paths. Economic cash/claims
//! are retained independently; chargeoff never rewrites the credit generator.
use super::*;
use crate::{cashflow::CashflowMeasure, credit_accounting as ca};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathAllowanceReview {
    pub path_key: u64,
    pub review: ca::ReviewedAllowance,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathChargeoffReview {
    pub path_key: u64,
    pub review: ca::ReviewedChargeoff,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegalWaiver {
    pub path_key: u64,
    pub review: Identity,
    pub recognition_ordinal: i32,
    pub claim_id: String,
    pub principal_amount: f64,
    pub accrued_interest_amount: f64,
    /// Revoked legal claims do not cancel separately retained voluntary cash.
    pub future_cash_disposition: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedActions {
    pub policy: ca::AccountingPolicy,
    #[serde(default)]
    pub allowance_reviews: Vec<PathAllowanceReview>,
    #[serde(default)]
    pub chargeoffs: Vec<PathChargeoffReview>,
    #[serde(default)]
    pub legal_waivers: Vec<LegalWaiver>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReviewedSnapshot {
    pub legal_principal: f64,
    pub model_gross_carry: f64,
    pub allowance: f64,
    pub claims: Vec<ca::RecoveryClaim>,
    pub legal_waivers: Vec<LegalWaiver>,
}
pub fn principal_claim_id(path: u64) -> String {
    format!("commercial-default:{path}")
}
pub fn interest_claim_id(path: u64, coupon: &str, origin: usize) -> String {
    format!("commercial-interest:{path}:{coupon}:{origin}")
}
fn ordinal(asof: i32, year: f64) -> Result<i32, String> {
    let d = year * 365.;
    let n = d.round();
    let days = if n / 365. == year { n } else { d.ceil() };
    if !days.is_finite() || !(0. ..=36600.).contains(&days) {
        return Err("reviewed native clock outside domain".into());
    }
    let at = asof
        .checked_add(days as i32)
        .ok_or("review date overflow")?;
    crate::cashflows::dated::validate_ordinal(at)?;
    Ok(at)
}
pub fn admit(
    actions: &ReviewedActions,
    paths: &[u64],
    asof: i32,
    end: i32,
) -> Result<usize, String> {
    let n =
        actions.allowance_reviews.len() + actions.chargeoffs.len() + actions.legal_waivers.len();
    if n > ca::MAX_REVIEW_EVENTS {
        return Err("commercial reviewed action admission exceeds4096".into());
    }
    meta(&actions.policy.identity)?;
    let mut seen = BTreeSet::new();
    let mut check = |path: u64, date: i32, id: &Identity| -> Result<(), String> {
        meta(id)?;
        crate::cashflows::dated::validate_ordinal(date)?;
        if !paths.contains(&path)
            || date < asof
            || date > end
            || !seen.insert((path, id.id.clone(), id.revision.clone()))
        {
            return Err("reviewed commercial action requires unique identity/path and complete date coverage".into());
        }
        Ok(())
    };
    for a in &actions.allowance_reviews {
        check(a.path_key, a.review.recognition_ordinal, &a.review.review)?;
        amount(a.review.target_amount)?;
        if a.review.target_amount < 0. {
            return Err("negative allowance target".into());
        }
    }
    for a in &actions.chargeoffs {
        text(&a.review.claim_id)?;
        if a.review.principal_amount + a.review.book_adjustment_amount < 0. {
            return Err("negative reviewed net principal/basis chargeoff".into());
        }
        check(a.path_key, a.review.recognition_ordinal, &a.review.review)?;
        for x in [
            a.review.principal_amount,
            a.review.book_adjustment_amount,
            a.review.accrued_interest_reversal,
        ] {
            amount(x)?;
        }
        if a.review.principal_amount < 0. {
            return Err("negative reviewed principal chargeoff".into());
        }
    }
    for a in &actions.legal_waivers {
        text(&a.claim_id)?;
        check(a.path_key, a.recognition_ordinal, &a.review)?;
        amount(a.principal_amount)?;
        amount(a.accrued_interest_amount)?;
        if a.principal_amount < 0.
            || a.future_cash_disposition != "retain_supplied_voluntary_recovery_cash-1"
        {
            return Err(
                "legal waiver requires explicit retained voluntary recovery disposition".into(),
            );
        }
    }
    Ok(n)
}
fn context(
    actions: &ReviewedActions,
    state: &ca::AccountingState,
    asof: i32,
    year: f64,
    end: i32,
    currency: &str,
    path: u64,
) -> Result<ca::CreditAccountingRequest, String> {
    Ok(ca::CreditAccountingRequest {
        schema: ca::SCHEMA.into(),
        measure: CashflowMeasure::Physical,
        currency: currency.into(),
        monetary_unit: "currency_units".into(),
        position_id: path.to_string(),
        recognition_ordinal: ordinal(asof, year)?,
        reporting_end_ordinal: end,
        policy: actions.policy.clone(),
        opening: state.clone(),
        defaults: Vec::new(),
        chargeoffs: Vec::new(),
        allowance_review: None,
    })
}
fn owned(intents: Vec<ca::JournalIntent>, lots: &[CouponLot], path: u64) -> Vec<OwnedPosting> {
    intents
        .into_iter()
        .flat_map(|i| {
            let lot = lots.iter().find(|l| {
                Some(interest_claim_id(
                    path,
                    &l.coupon_id,
                    l.earning_origin_index,
                )) == i.claim_id
            });
            i.postings.into_iter().map(move |posting| OwnedPosting {
                coupon_id: lot.map(|l| l.coupon_id.clone()),
                earning_origin_index: lot.map(|l| l.earning_origin_index),
                posting,
            })
        })
        .collect()
}
fn signed_sub(total: f64, paid: f64) -> Result<f64, String> {
    if !paid.is_finite()
        || paid.abs() > total.abs()
        || (paid != 0. && paid.is_sign_positive() != total.is_sign_positive())
    {
        return Err("reviewed waiver exceeds exact signed legal claim".into());
    }
    Ok(total - paid)
}
fn legal_collect(total: f64, paid: f64) -> f64 {
    if total == 0. {
        0.
    } else {
        total - paid.signum() * paid.abs().min(total.abs())
    }
}
fn snapshot(
    base: &Snapshot,
    state: &ca::AccountingState,
    legal_p: f64,
    legal: &BTreeMap<String, f64>,
    waivers: &[LegalWaiver],
) -> Snapshot {
    let mut next = base.clone();
    if next.in_default {
        next.principal = state.claims.iter().map(|c| c.outstanding_principal).sum();
        next.book_adjustment = state
            .claims
            .iter()
            .map(|c| c.outstanding_book_adjustment)
            .sum();
        for lot in &mut next.lots {
            let id = interest_claim_id(next.path_key, &lot.coupon_id, lot.earning_origin_index);
            if let Some(c) = state.claims.iter().find(|c| c.claim_id == id) {
                lot.recognized_remaining = c.outstanding_accrued_interest;
                lot.recognized_cash_applied = lot.recognized_earned - lot.recognized_remaining;
            }
            if let Some(a) = legal.get(&id) {
                lot.legal_remaining = *a;
            }
        }
        next.recognized_interest = next.lots.iter().map(|l| l.recognized_remaining).sum();
        next.legal_interest = next.lots.iter().map(|l| l.legal_remaining).sum();
        next.gross_carry = next.principal + next.recognized_interest + next.book_adjustment;
    }
    next.reviewed_credit_state = Some(ReviewedSnapshot {
        legal_principal: legal_p,
        model_gross_carry: base.gross_carry,
        allowance: state.allowance,
        claims: state.claims.clone(),
        legal_waivers: waivers.to_vec(),
    });
    next
}
fn meta(id: &Identity) -> Result<(), String> {
    if id.id.is_empty()
        || id.revision.is_empty()
        || id.id.len() > 256
        || id.revision.len() > 256
        || id
            .id
            .chars()
            .chain(id.revision.chars())
            .any(char::is_control)
    {
        Err("invalid reviewed action identity".into())
    } else {
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
fn initial_claim(
    id: String,
    legal: f64,
    recognized: f64,
    p: f64,
    b: f64,
    loss: f64,
    tail: Vec<ca::RecoveryPoint>,
    at: i32,
    actions: &ReviewedActions,
    source: &Identity,
) -> ca::RecoveryClaim {
    let reverse = actions.policy.accrued == ca::AccruedPolicy::ReverseIncomeAtDefault;
    let expense = actions.policy.basis == ca::BasisPolicy::RecognizeInEarningsAtDefault;
    ca::RecoveryClaim {
        claim_id: id,
        economic_source: source.clone(),
        recovery_schedule_review: actions.policy.identity.clone(),
        policy: actions.policy.clone(),
        default_ordinal: at,
        initial_principal: p,
        economic_principal_loss: loss,
        initial_accrued_interest: legal,
        initial_book_adjustment: b,
        outstanding_principal: p,
        outstanding_accrued_interest: if reverse { 0. } else { recognized },
        outstanding_book_adjustment: if expense { 0. } else { b },
        principal_charged_off: 0.,
        accrued_reversed: legal - if reverse { 0. } else { recognized },
        basis_recognized: if expense { b } else { 0. },
        recovery_cash_collected: 0.,
        recovery_applied_to_principal: 0.,
        interest_recovery_cash_collected: 0.,
        interest_recovery_applied: 0.,
        settled_interest_recovery_ids: Vec::new(),
        remaining_recoveries: tail,
    }
}
enum Event {
    Source(Box<AtomicGroup>),
    Allowance(PathAllowanceReview),
    Chargeoff(PathChargeoffReview),
    Waiver(LegalWaiver),
}
pub fn apply(
    result: &mut ResultData,
    carry: &Carry,
    actions: &ReviewedActions,
    asof: i32,
    end: i32,
    currency: &str,
) -> Result<(), String> {
    let paths: Vec<_> = carry
        .credit()
        .result
        .paths
        .iter()
        .map(|p| p.path_key)
        .collect();
    admit(actions, &paths, asof, end)?;
    let mut groups = Vec::new();
    let mut closing = Vec::new();
    for path in &carry.credit().result.paths {
        let mut events = Vec::<(f64, u8, Event)>::new();
        for g in result.groups.iter().filter(|g| g.path_key == path.path_key) {
            let phase = if g.phase == "reviewed_opening_snapshot" {
                0
            } else if g.phase == "closing_claims_retained" {
                4
            } else {
                2
            };
            events.push((g.year, phase, Event::Source(Box::new(g.clone()))));
        }
        for a in actions
            .allowance_reviews
            .iter()
            .filter(|a| a.path_key == path.path_key)
        {
            events.push((
                f64::from(a.review.recognition_ordinal - asof) / 365.,
                if a.review.phase == ca::ReviewPhase::BeforeCredit {
                    1
                } else {
                    3
                },
                Event::Allowance(a.clone()),
            ));
        }
        for a in actions
            .chargeoffs
            .iter()
            .filter(|a| a.path_key == path.path_key)
        {
            events.push((
                f64::from(a.review.recognition_ordinal - asof) / 365.,
                3,
                Event::Chargeoff(a.clone()),
            ));
        }
        for a in actions
            .legal_waivers
            .iter()
            .filter(|a| a.path_key == path.path_key)
        {
            events.push((
                f64::from(a.recognition_ordinal - asof) / 365.,
                3,
                Event::Waiver(a.clone()),
            ));
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut state = ca::AccountingState {
            live: [ca::LiveState::default(); 3],
            allowance: 0.,
            claims: Vec::new(),
        };
        let mut base = None::<Snapshot>;
        let mut legal_p = 0.;
        let mut legal = BTreeMap::new();
        let mut waivers = Vec::new();
        let mut recovery_index = 0;
        for (year, _, event) in events {
            let mut g = match event {
                Event::Source(g) => {
                    let mut g = *g;
                    if g.phase == "default_gross_transfer" {
                        let claim = path
                            .default
                            .as_ref()
                            .ok_or("reviewed default missing native authority")?;
                        let at = ordinal(asof, year)?;
                        legal_p = g.next.principal;
                        let mut tail = BTreeMap::<i32, f64>::new();
                        for r in &claim.recoveries {
                            *tail.entry(ordinal(asof, r.year)?).or_default() += r.principal;
                        }
                        let tail = tail
                            .into_iter()
                            .filter(|(_, amount)| *amount != 0.)
                            .map(|(payment_ordinal, amount)| ca::RecoveryPoint {
                                payment_ordinal,
                                amount,
                            })
                            .collect();
                        let source = Identity {
                            id: carry.credit().result.source_id.clone(),
                            revision: carry.credit().result.source_revision.clone(),
                        };
                        state.claims.push(initial_claim(
                            principal_claim_id(path.path_key),
                            0.,
                            0.,
                            g.next.principal,
                            g.next.book_adjustment,
                            claim.economic_principal_loss,
                            tail,
                            at,
                            actions,
                            &source,
                        ));
                        if actions.policy.basis == ca::BasisPolicy::RecognizeInEarningsAtDefault {
                            add(
                                &mut g.postings,
                                AccountCode::DefaultBookAdjustment,
                                -g.next.book_adjustment,
                                None,
                            )?;
                            add(
                                &mut g.postings,
                                AccountCode::BasisRecognitionExpense,
                                g.next.book_adjustment,
                                None,
                            )?;
                        }
                        for lot in &g.next.lots {
                            let id = interest_claim_id(
                                path.path_key,
                                &lot.coupon_id,
                                lot.earning_origin_index,
                            );
                            legal.insert(id.clone(), lot.legal_remaining);
                            state.claims.push(initial_claim(
                                id.clone(),
                                lot.recognized_remaining,
                                lot.recognized_remaining,
                                0.,
                                0.,
                                0.,
                                Vec::new(),
                                at,
                                actions,
                                &source,
                            ));
                            // Source cash eligibility and booked recognition may have
                            // opposite signs after early cash/nonaccrual. Preserve a
                            // separate cash-only legal claim rather than netting them.
                            state.claims.push(initial_claim(
                                format!(
                                    "commercial-legal-cash:{}:{}:{}",
                                    path.path_key, lot.coupon_id, lot.earning_origin_index
                                ),
                                lot.legal_remaining,
                                0.,
                                0.,
                                0.,
                                0.,
                                Vec::new(),
                                at,
                                actions,
                                &source,
                            ));
                            if actions.policy.accrued == ca::AccruedPolicy::ReverseIncomeAtDefault {
                                add(
                                    &mut g.postings,
                                    AccountCode::DefaultAccruedInterest,
                                    -lot.recognized_remaining,
                                    Some(lot),
                                )?;
                                add(
                                    &mut g.postings,
                                    AccountCode::InterestIncome,
                                    lot.recognized_remaining,
                                    None,
                                )?;
                            }
                        }
                        let req =
                            context(actions, &state, asof, year, end, currency, path.path_key)?;
                        state = ca::evaluate_reviewed_actions(&req)?.closing;
                    } else if g.phase == "default_actual_cash" {
                        let r = path
                            .default
                            .as_ref()
                            .and_then(|d| d.recoveries.get(recovery_index))
                            .ok_or("reviewed recovery missing native source")?;
                        let mut req =
                            context(actions, &state, asof, year, end, currency, path.path_key)?;
                        g.postings.clear();
                        if r.principal != 0. {
                            let applied = ca::settle_principal_recovery(
                                &req,
                                &principal_claim_id(path.path_key),
                                r.principal,
                            )?;
                            g.postings
                                .extend(owned(applied.intents, &g.next.lots, path.path_key));
                            state = applied.closing;
                            req.opening = state.clone();
                        }
                        legal_p = legal_collect(legal_p, r.principal);
                        let mut settlements = Vec::new();
                        for r in carry.native_coupon_recoveries().iter().filter(|r| {
                            r.path_key == path.path_key && r.recovery_index == recovery_index
                        }) {
                            let id = interest_claim_id(
                                path.path_key,
                                &r.coupon_id,
                                r.earning_origin_index,
                            );
                            let a = legal.get_mut(&id).ok_or("legal recovery claim missing")?;
                            *a = legal_collect(*a, r.interest);
                            let book = state
                                .claims
                                .iter()
                                .find(|c| c.claim_id == id)
                                .ok_or("book recovery claim missing")?;
                            let capacity = book.initial_accrued_interest
                                - book.interest_recovery_cash_collected;
                            let book_cash = if capacity != 0.
                                && capacity.is_sign_positive() == r.interest.is_sign_positive()
                            {
                                r.interest.signum() * r.interest.abs().min(capacity.abs())
                            } else {
                                0.
                            };
                            let extra = r.interest - book_cash;
                            for (suffix, claim_id, amount) in [
                                ("book", id, book_cash),
                                (
                                    "legal",
                                    format!(
                                        "commercial-legal-cash:{}:{}:{}",
                                        path.path_key, r.coupon_id, r.earning_origin_index
                                    ),
                                    extra,
                                ),
                            ] {
                                if amount == 0. {
                                    continue;
                                }
                                settlements.push(ca::ReviewedInterestRecovery {
                                    recovery: Identity {
                                        id: format!(
                                            "native-interest:{}:{}:{}:{}:{suffix}",
                                            path.path_key,
                                            recovery_index,
                                            r.native_audit_index,
                                            r.earning_origin_index
                                        ),
                                        revision: "1".into(),
                                    },
                                    claim_id,
                                    recognition_ordinal: req.recognition_ordinal,
                                    amount,
                                    policy_review: actions.policy.identity.clone(),
                                    source: actions.policy.identity.clone(),
                                    excess_cash: actions.policy.excess_cash_recovery,
                                });
                            }
                        }
                        let paid = ca::settle_interest_recoveries(&ca::InterestRecoveryRequest {
                            schema: "credit-interest-recovery-1".into(),
                            measure: CashflowMeasure::Physical,
                            currency: currency.into(),
                            monetary_unit: "currency_units".into(),
                            position_id: path.path_key.to_string(),
                            recognition_ordinal: req.recognition_ordinal,
                            reporting_end_ordinal: end,
                            policy: actions.policy.clone(),
                            opening: state.clone(),
                            settlements,
                        })?;
                        g.postings
                            .extend(owned(paid.intents, &g.next.lots, path.path_key));
                        state = paid.closing;
                        recovery_index += 1;
                    } else if !g.next.in_default {
                        legal_p = g.next.principal;
                    }
                    base = Some(g.next.clone());
                    g
                }
                action => {
                    let b = base
                        .as_ref()
                        .ok_or("reviewed action precedes native acquisition")?;
                    let mut req =
                        context(actions, &state, asof, year, end, currency, path.path_key)?;
                    let (phase, policy, postings) = match action {
                        Event::Allowance(a) => {
                            req.allowance_review = Some(a.review);
                            let r = ca::evaluate_reviewed_actions(&req)?;
                            state = r.closing;
                            (
                                "reviewed_allowance_target",
                                actions.policy.identity.clone(),
                                owned(r.intents, &b.lots, path.path_key),
                            )
                        }
                        Event::Chargeoff(a) => {
                            req.chargeoffs.push(a.review);
                            let r = ca::evaluate_reviewed_actions(&req)?;
                            state = r.closing;
                            (
                                "reviewed_credit_chargeoff",
                                actions.policy.identity.clone(),
                                owned(r.intents, &b.lots, path.path_key),
                            )
                        }
                        Event::Waiver(a) => {
                            if !b.in_default {
                                return Err(
                                    "reviewed legal waiver requires native default claim authority"
                                        .into(),
                                );
                            }
                            if a.claim_id == principal_claim_id(path.path_key) {
                                if a.accrued_interest_amount != 0. {
                                    return Err(
                                        "principal waiver cannot substitute coupon claim".into()
                                    );
                                }
                                legal_p = signed_sub(legal_p, a.principal_amount)?;
                            } else {
                                if a.principal_amount != 0. {
                                    return Err(
                                        "coupon waiver cannot substitute principal claim".into()
                                    );
                                }
                                let right = legal
                                    .get_mut(&a.claim_id)
                                    .ok_or("waiver unknown native coupon claim")?;
                                *right = signed_sub(*right, a.accrued_interest_amount)?;
                            }
                            let review = a.review.clone();
                            waivers.push(a);
                            ("reviewed_legal_waiver", review, Vec::new())
                        }
                        Event::Source(_) => unreachable!(),
                    };
                    AtomicGroup {
                        path_key: path.path_key,
                        year,
                        phase,
                        policy,
                        source_event_id: None,
                        effective_interest: 0.,
                        basis_realization: 0.,
                        cash: 0.,
                        postings,
                        next: b.clone(),
                    }
                }
            };
            let b = base.as_ref().ok_or("reviewed snapshot missing")?;
            g.next = snapshot(b, &state, legal_p, &legal, &waivers);
            g.next.year = year;
            if g.phase == "closing_claims_retained" {
                closing.push(g.next.clone());
            }
            groups.push(g);
        }
    }
    result.groups = groups;
    result.closing = closing;
    result.claims_fully_closed = result.closing.iter().all(|s| {
        s.principal == 0.
            && s.book_adjustment == 0.
            && s.legal_interest == 0.
            && s.recognized_interest == 0.
            && s.reviewed_credit_state
                .as_ref()
                .is_some_and(|r| r.legal_principal == 0.)
    });
    Ok(())
}
