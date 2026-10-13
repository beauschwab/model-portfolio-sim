//! Copied-only accounting compilation from the actual native credit/carry receipt.
//! No second EIR, credit path, coupon or cash projection is performed here.
use crate::credit_accounting::{AccountCode, Identity, PostingIntent};
use crate::credit_path_income::{
    CarryPoint, EarnedOriginContribution, ResultData as Carry, SourceCashReceipt,
};
use crate::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};

pub mod reviews;

pub const SCHEMA: &str = "commercial-coupon-accounting-proof-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recognition {
    FullAccrual,
    LegalOnlyIncomeStopped,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionReview {
    pub interval_index: usize,
    pub review: Identity,
    pub by_origin: [Recognition; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub recognition: Vec<RecognitionReview>,
    pub cash_application: String,
    pub unrecognized_cash: String,
    pub exit_disposition: String,
    pub default_disposition: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CouponLot {
    pub coupon_id: String,
    pub source_event_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub native_audit_index: usize,
    pub native_cash_index: usize,
    pub earning_origin_index: usize,
    pub contractual_earned: f64,
    pub legal_cash_collected: f64,
    pub legal_remaining: f64,
    pub recognized_earned: f64,
    pub recognized_cash_applied: f64,
    pub recognized_remaining: f64,
    pub default_claim: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub path_key: u64,
    pub year: f64,
    pub principal: f64,
    pub recognized_interest: f64,
    pub legal_interest: f64,
    /// Different ordered f64 reductions are retained as evidence, never plugged.
    pub native_legal_reduction_difference: f64,
    pub book_adjustment: f64,
    pub gross_carry: f64,
    pub in_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_credit_state: Option<reviews::ReviewedSnapshot>,
    pub lots: Vec<CouponLot>,
    /// Coupon-level source authority. Origin lots retain their separate
    /// ordered recognition reductions and are not used to net legal coupons.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub canonical_legal_coupons: Vec<crate::credit_path_income::CanonicalCouponLegalBalance>,
}

/// Lot identity accompanies generic accounts so the existing shared ledger
/// mapper can classify every signed right separately, rather than netting it.
#[derive(Debug, Clone, Serialize)]
pub struct OwnedPosting {
    pub coupon_id: Option<String>,
    pub earning_origin_index: Option<usize>,
    pub posting: PostingIntent,
}
#[derive(Debug, Clone, Serialize)]
pub struct AtomicGroup {
    pub path_key: u64,
    pub year: f64,
    pub phase: &'static str,
    pub policy: Identity,
    pub source_event_id: Option<String>,
    pub effective_interest: f64,
    pub basis_realization: f64,
    pub cash: f64,
    pub postings: Vec<OwnedPosting>,
    pub next: Snapshot,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    schema: &'static str,
    policy: Policy,
    groups: Vec<AtomicGroup>,
    closing: Vec<Snapshot>,
    complete_native_recovery_tail_retained: bool,
    claims_fully_closed: bool,
    actual_risk_source_binding: bool,
    mixed_ledger_adoption: bool,
    accounting_approval: bool,
}
impl ResultData {
    pub fn groups(&self) -> &[AtomicGroup] {
        &self.groups
    }
    pub fn closing(&self) -> &[Snapshot] {
        &self.closing
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub points_upper: usize,
    pub lots_per_path_upper: usize,
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
}

/// Native source compiler calls this before generating either monetary source.
/// Snapshot storage depends on distinct coupon/origin lots, not query count.
pub fn admit(points_upper: usize, lots_per_path_upper: usize) -> Result<Admission, String> {
    let cells = points_upper
        .checked_mul(
            lots_per_path_upper
                .checked_add(1)
                .ok_or("commercial book upper overflow")?,
        )
        .ok_or("commercial book upper overflow")?;
    let work_upper = cells
        .checked_mul(128)
        .ok_or("commercial book upper overflow")?;
    let retained_bytes_upper = cells
        .checked_mul(4096)
        .ok_or("commercial book upper overflow")?;
    if retained_bytes_upper > 32 << 20 || work_upper > 2_000_000 {
        return Err("commercial gross snapshots exceed 2M/32MiB before compilation".into());
    }
    Ok(Admission {
        points_upper,
        lots_per_path_upper,
        work_upper,
        retained_bytes_upper,
    })
}

fn text(v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
        return Err("commercial accounting review identity requires 1..256 bytes".into());
    }
    Ok(())
}
fn amount(v: f64) -> Result<(), String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        return Err("commercial accounting money domain exceeded".into());
    }
    Ok(())
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs()))
}
fn account(default: bool, live: AccountCode, claim: AccountCode) -> AccountCode {
    if default {
        claim
    } else {
        live
    }
}
fn add(
    lines: &mut Vec<OwnedPosting>,
    code: AccountCode,
    value: f64,
    lot: Option<&CouponLot>,
) -> Result<(), String> {
    amount(value)?;
    // Exact zero alone is omitted. No small-money clamp or tolerance retirement.
    if value != 0. {
        lines.push(OwnedPosting {
            coupon_id: lot.map(|l| l.coupon_id.clone()),
            earning_origin_index: lot.map(|l| l.earning_origin_index),
            posting: PostingIntent {
                account: code,
                amount: value,
            },
        });
    }
    Ok(())
}
fn sync(lot: &mut CouponLot) -> Result<(), String> {
    lot.legal_remaining = lot.contractual_earned - lot.legal_cash_collected;
    lot.recognized_remaining = lot.recognized_earned - lot.recognized_cash_applied;
    amount(lot.legal_remaining)?;
    amount(lot.recognized_remaining)
}
fn snapshot(p: &CarryPoint, lots: Vec<CouponLot>, default: bool) -> Result<Snapshot, String> {
    let origin_legal: f64 = lots.iter().map(|l| l.legal_remaining).sum();
    let legal = if p.canonical_legal_coupons.is_empty() {
        origin_legal
    } else {
        p.canonical_legal_coupons
            .iter()
            .map(|c| c.legal_remaining)
            .sum()
    };
    let recognized: f64 = lots.iter().map(|l| l.recognized_remaining).sum();
    amount(legal)?;
    amount(recognized)?;
    if !near(legal, p.contractual_accrued_interest) {
        return Err("gross coupon legal rights do not reconcile native carry".into());
    }
    let basis = p.gross_carry - p.principal - recognized;
    amount(basis)?;
    Ok(Snapshot {
        path_key: p.path_key,
        year: p.year,
        principal: p.principal,
        recognized_interest: recognized,
        legal_interest: legal,
        native_legal_reduction_difference: p.contractual_accrued_interest - origin_legal,
        book_adjustment: basis,
        gross_carry: p.gross_carry,
        in_default: default,
        reviewed_credit_state: None,
        lots,
        canonical_legal_coupons: p.canonical_legal_coupons.clone(),
    })
}
fn post(
    previous: Option<&Snapshot>,
    next: Snapshot,
    p: &CarryPoint,
    policy: &Policy,
    source: Option<String>,
) -> Result<AtomicGroup, String> {
    let mut lines = Vec::new();
    if let Some(before) = previous {
        if before.in_default == next.in_default {
            add(
                &mut lines,
                account(
                    next.in_default,
                    AccountCode::LivePrincipal,
                    AccountCode::DefaultPrincipal,
                ),
                next.principal - before.principal,
                None,
            )?;
            add(
                &mut lines,
                account(
                    next.in_default,
                    AccountCode::LiveBookAdjustment,
                    AccountCode::DefaultBookAdjustment,
                ),
                next.book_adjustment - before.book_adjustment,
                None,
            )?;
        } else {
            add(
                &mut lines,
                account(
                    before.in_default,
                    AccountCode::LivePrincipal,
                    AccountCode::DefaultPrincipal,
                ),
                -before.principal,
                None,
            )?;
            add(
                &mut lines,
                account(
                    before.in_default,
                    AccountCode::LiveBookAdjustment,
                    AccountCode::DefaultBookAdjustment,
                ),
                -before.book_adjustment,
                None,
            )?;
            add(
                &mut lines,
                account(
                    next.in_default,
                    AccountCode::LivePrincipal,
                    AccountCode::DefaultPrincipal,
                ),
                next.principal,
                None,
            )?;
            add(
                &mut lines,
                account(
                    next.in_default,
                    AccountCode::LiveBookAdjustment,
                    AccountCode::DefaultBookAdjustment,
                ),
                next.book_adjustment,
                None,
            )?;
        }
    } else {
        add(&mut lines, AccountCode::LivePrincipal, next.principal, None)?;
        add(
            &mut lines,
            AccountCode::LiveBookAdjustment,
            next.book_adjustment,
            None,
        )?;
    }
    for lot in &next.lots {
        let before = previous.and_then(|p| {
            p.lots.iter().find(|b| {
                b.coupon_id == lot.coupon_id && b.earning_origin_index == lot.earning_origin_index
            })
        });
        if before.is_none_or(|b| b.default_claim == lot.default_claim) {
            add(
                &mut lines,
                account(
                    lot.default_claim,
                    AccountCode::LiveAccruedInterest,
                    AccountCode::DefaultAccruedInterest,
                ),
                lot.recognized_remaining - before.map_or(0., |b| b.recognized_remaining),
                Some(lot),
            )?;
        } else if let Some(before) = before {
            add(
                &mut lines,
                account(
                    before.default_claim,
                    AccountCode::LiveAccruedInterest,
                    AccountCode::DefaultAccruedInterest,
                ),
                -before.recognized_remaining,
                Some(before),
            )?;
            add(
                &mut lines,
                account(
                    lot.default_claim,
                    AccountCode::LiveAccruedInterest,
                    AccountCode::DefaultAccruedInterest,
                ),
                lot.recognized_remaining,
                Some(lot),
            )?;
        }
    }
    if previous.is_some_and(|p| {
        p.lots.iter().any(|b| {
            !next.lots.iter().any(|n| {
                n.coupon_id == b.coupon_id && n.earning_origin_index == b.earning_origin_index
            })
        })
    }) {
        return Err("gross coupon lot disappeared without a source disposition".into());
    }
    let cash = if previous.is_none() {
        -next.gross_carry
    } else {
        p.cash
    };
    add(&mut lines, AccountCode::Cash, cash, None)?;
    add(
        &mut lines,
        AccountCode::InterestIncome,
        -p.effective_interest,
        None,
    )?;
    add(
        &mut lines,
        AccountCode::BasisRecognitionExpense,
        p.basis_realization,
        None,
    )?;
    let debit: f64 = lines
        .iter()
        .filter(|l| l.posting.amount > 0.)
        .map(|l| l.posting.amount)
        .sum();
    let credit: f64 = -lines
        .iter()
        .filter(|l| l.posting.amount < 0.)
        .map(|l| l.posting.amount)
        .sum::<f64>();
    if !near(debit, credit) {
        return Err("commercial atomic group does not balance; no plug".into());
    }
    Ok(AtomicGroup {
        path_key: p.path_key,
        year: p.year,
        phase: p.kind,
        policy: policy.identity.clone(),
        source_event_id: source,
        effective_interest: p.effective_interest,
        basis_realization: p.basis_realization,
        cash,
        postings: lines,
        next,
    })
}

fn apply_cash(
    lots: &mut [CouponLot],
    cash: &SourceCashReceipt,
    origin: usize,
) -> Result<(), String> {
    // Stable earliest-born origin first, only within the selected contractual
    // coupon. A remainder is deferred on its selected current origin. This is
    // application of exact actual cash, never an added cash or terminal plug.
    let mut remaining = cash.cash.interest;
    let indices: Vec<_> = lots
        .iter()
        .enumerate()
        .filter(|(_, l)| l.coupon_id == cash.cash.coupon_id)
        .map(|(i, _)| i)
        .collect();
    if indices.is_empty() {
        return Err("cash coupon lacks native earned/entitlement ownership".into());
    }
    for &i in &indices {
        let right = lots[i].legal_remaining;
        let used = if right.signum() == remaining.signum() {
            right.abs().min(remaining.abs()).copysign(remaining)
        } else {
            0.
        };
        lots[i].legal_cash_collected += used;
        lots[i].recognized_cash_applied += used;
        remaining -= used;
        sync(&mut lots[i])?;
    }
    if remaining != 0. {
        let i = indices
            .iter()
            .copied()
            .find(|&i| lots[i].earning_origin_index == origin)
            .ok_or("unearned collection needs a matching explicitly reviewed origin")?;
        lots[i].legal_cash_collected += remaining;
        lots[i].recognized_cash_applied += remaining;
        sync(&mut lots[i])?;
    }
    Ok(())
}

fn add_earned(
    lots: &mut Vec<CouponLot>,
    c: &EarnedOriginContribution,
    recognized: bool,
) -> Result<(), String> {
    let entitlement = c
        .native_entitlement
        .as_ref()
        .ok_or("commercial book requires actual source entitlement authority")?;
    let index = if let Some(i) = lots
        .iter()
        .position(|l| l.coupon_id == c.coupon_id && l.earning_origin_index == c.origin_index)
    {
        i
    } else {
        lots.push(CouponLot {
            coupon_id: c.coupon_id.clone(),
            source_event_id: c.source_event_id.clone(),
            source_id: c.source_id.clone(),
            source_revision: c.source_revision.clone(),
            native_audit_index: entitlement.native_audit_index,
            native_cash_index: entitlement.native_cash_index,
            earning_origin_index: c.origin_index,
            contractual_earned: 0.,
            legal_cash_collected: 0.,
            legal_remaining: 0.,
            recognized_earned: 0.,
            recognized_cash_applied: 0.,
            recognized_remaining: 0.,
            default_claim: false,
        });
        lots.len() - 1
    };
    let l = &mut lots[index];
    if l.source_event_id != c.source_event_id
        || l.source_id != c.source_id
        || l.source_revision != c.source_revision
        || l.native_audit_index != entitlement.native_audit_index
        || l.native_cash_index != entitlement.native_cash_index
    {
        return Err("immutable coupon entitlement identity drift".into());
    }
    l.contractual_earned += c.contractual_earned;
    if recognized {
        l.recognized_earned += c.contractual_earned;
    }
    sync(l)
}

/// Called inside the actual credit loop before recovery generation. Reuses the
/// same source-ID/FIFO legal application as the later accounting compiler.
pub(crate) fn native_default_lots(
    path: &crate::credit_path::CreditPathResult,
    contributions: &[EarnedOriginContribution],
    cash: &[SourceCashReceipt],
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Vec<CouponLot>, String> {
    let mut lots = Vec::new();
    let mut earned = contributions
        .iter()
        .filter(|c| c.path_key == path.path_key)
        .peekable();
    for cash in cash.iter().filter(|c| c.path_key == path.path_key) {
        check()?;
        while earned.peek().is_some_and(|c| c.end_year <= cash.year) {
            check()?;
            add_earned(
                &mut lots,
                earned.next().ok_or("native earned cursor")?,
                false,
            )?;
        }
        let event = path
            .events
            .iter()
            .find(|e| e.event_id == cash.cash.source_event_id)
            .ok_or("native payment event identity missing before default")?;
        let origin = match event.origin {
            crate::credit::CreditState::Performing => 0,
            crate::credit::CreditState::Watch => 1,
            crate::credit::CreditState::Delinquent => 2,
            _ => return Err("unsupported live cash origin".into()),
        };
        apply_cash(&mut lots, cash, origin)?;
    }
    for c in earned {
        check()?;
        add_earned(&mut lots, c, false)?;
    }
    Ok(lots)
}

pub fn validate_policy(
    raw: &crate::credit_path_income::Request,
    policy: &Policy,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    check()?;
    if raw.raw_credit_path.measure != crate::credit::CreditMeasure::Physical {
        return Err(
            "commercial accounting requires independently supplied Physical credit/source cash"
                .into(),
        );
    }
    text(&policy.identity.id)?;
    text(&policy.identity.revision)?;
    if policy.cash_application != "exact_native_coupon_cash_origin_fifo-1"
        || policy.unrecognized_cash != "defer_unrecognized_cash_until_reviewed_recognition-1"
        || policy.exit_disposition
            != "collect_legal_balance_reverse_recognized_realize_remaining_carry-1"
        || policy.default_disposition
            != "transfer_all_gross_legal_and_recognized_claims_no_implicit_chargeoff-1"
    {
        return Err("unsupported explicit commercial recognition/disposition policy".into());
    }
    if policy.recognition.len() != raw.recognition_rates.len() {
        return Err("commercial recognition review coverage missing".into());
    }
    for (i, review) in policy.recognition.iter().enumerate() {
        check()?;
        text(&review.review.id)?;
        text(&review.review.revision)?;
        if review.interval_index != i {
            return Err("commercial recognition reviews require ordered exact coverage".into());
        }
        for origin in 0..3 {
            if review.by_origin[origin] == Recognition::LegalOnlyIncomeStopped
                && raw.recognition_rates[i].annual_effective_rates[origin] != 0.
            {
                return Err(
                    "legal-only review requires separately explicit stopped accounting income rate"
                        .into(),
                );
            }
        }
    }
    if raw
        .raw_credit_path
        .paths
        .iter()
        .any(|p| p.accrued_interest != 0.)
    {
        return Err("seasoned accrued prefix requires an explicit source compiler".into());
    }
    check()?;
    Ok(())
}

pub fn compile(
    carry: &Carry,
    policy: &Policy,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    validate_policy(carry.raw_request(), policy, check)?;
    let mut per_path =
        std::collections::BTreeMap::<u64, std::collections::BTreeSet<(&str, usize)>>::new();
    for atom in carry.earned_origin_contributions() {
        check()?;
        per_path
            .entry(atom.path_key)
            .or_default()
            .insert((&atom.coupon_id, atom.origin_index));
    }
    admit(
        carry.carry_points().len(),
        per_path.values().map(|s| s.len()).max().unwrap_or(0),
    )?;
    let contributions = carry.earned_origin_contributions();
    let mut ci = 0;
    let mut si = 0;
    let mut groups = Vec::new();
    let mut closing = Vec::new();
    let mut previous: Option<Snapshot> = None;
    let mut lots = Vec::<CouponLot>::new();
    let mut default = false;
    let mut recovery_index = 0;
    for p in carry.carry_points() {
        check()?;
        let mut source_id = None;
        match p.kind {
            "reviewed_opening_snapshot" => {
                previous = None;
                lots.clear();
                default = false;
                recovery_index = 0;
            }
            "live_sojourn" => {
                while let Some(c) = contributions
                    .get(ci)
                    .filter(|c| c.path_key == p.path_key && c.end_year == p.year)
                {
                    check()?;
                    let mode = policy.recognition[c.interval_index].by_origin[c.origin_index];
                    add_earned(&mut lots, c, mode == Recognition::FullAccrual)?;
                    ci += 1;
                }
            }
            "contractual_cash" => {
                let cash = carry
                    .native_source_cash()
                    .get(si)
                    .ok_or("commercial repayment lacks native source cash receipt")?;
                if cash.path_key != p.path_key
                    || cash.year != p.year
                    || (cash.cash.principal + cash.cash.interest + cash.cash.fee).to_bits()
                        != p.cash.to_bits()
                {
                    return Err("actual native cash/point identity mismatch".into());
                }
                let path = carry
                    .credit()
                    .result
                    .paths
                    .iter()
                    .find(|path| path.path_key == p.path_key)
                    .ok_or("missing native path")?;
                let event = path
                    .events
                    .iter()
                    .find(|e| e.event_id == cash.cash.source_event_id)
                    .ok_or("missing actual native payment event")?;
                let origin = match event.origin {
                    crate::credit::CreditState::Performing => 0,
                    crate::credit::CreditState::Watch => 1,
                    crate::credit::CreditState::Delinquent => 2,
                    _ => return Err("unsupported cash live origin".into()),
                };
                apply_cash(&mut lots, cash, origin)?;
                source_id = Some(cash.cash.source_event_id.clone());
                si += 1;
            }
            "default_gross_transfer" => {
                default = true;
                for lot in &mut lots {
                    lot.default_claim = true;
                }
            }
            "default_actual_cash" => {
                let path = carry
                    .credit()
                    .result
                    .paths
                    .iter()
                    .find(|path| path.path_key == p.path_key)
                    .ok_or("missing native default path")?;
                let claim = path
                    .default
                    .as_ref()
                    .ok_or("missing native default claim")?;
                let recovery = claim
                    .recoveries
                    .get(recovery_index)
                    .ok_or("missing native recovery")?;
                if recovery.year != p.year {
                    return Err("recovery order/clock mismatch".into());
                }
                let mut actual_interest = 0.;
                let mut observed = 0;
                for paid in carry
                    .native_coupon_recoveries()
                    .iter()
                    .filter(|r| r.path_key == p.path_key && r.recovery_index == recovery_index)
                {
                    check()?;
                    if !paid.applied_in_horizon
                        || paid.year != p.year
                        || paid.default_year != claim.year
                    {
                        return Err("native gross recovery clock mismatch".into());
                    }
                    let lot = lots
                        .iter_mut()
                        .find(|l| {
                            l.coupon_id == paid.coupon_id
                                && l.earning_origin_index == paid.earning_origin_index
                        })
                        .ok_or("native gross recovery origin missing")?;
                    if lot.native_audit_index != paid.native_audit_index
                        || lot.source_event_id != paid.source_event_id
                    {
                        return Err("native gross recovery immutable source mismatch".into());
                    }
                    lot.legal_cash_collected += paid.interest;
                    lot.recognized_cash_applied += paid.interest;
                    sync(lot)?;
                    actual_interest += paid.interest;
                    observed += 1;
                }
                if observed != lots.len()
                    || actual_interest.to_bits() != recovery.interest.to_bits()
                {
                    return Err("per-coupon default interest collection requires complete native gross recovery authority".into());
                }
                recovery_index += 1;
            }
            "full_live_exit" => {
                source_id = carry
                    .credit()
                    .result
                    .paths
                    .iter()
                    .find(|path| path.path_key == p.path_key)
                    .and_then(|path| {
                        path.events.iter().find(|event| {
                            event.year == p.year
                                && (event.kind == "maturity" || event.kind == "prepayment")
                        })
                    })
                    .map(|event| event.event_id.clone());
                if source_id.is_none() {
                    return Err(
                        "full commercial exit lacks actual native path event identity".into(),
                    );
                }
                for lot in &mut lots {
                    lot.legal_cash_collected += lot.legal_remaining;
                    lot.recognized_cash_applied += lot.recognized_remaining;
                    sync(lot)?;
                }
            }
            "closing_claims_retained" => {}
            "funded_draw" => {}
            _ => return Err("unsupported commercial source action or missing review".into()),
        }
        let next = snapshot(p, lots.clone(), default)?;
        let group = post(previous.as_ref(), next.clone(), p, policy, source_id)?;
        if p.kind == "closing_claims_retained" {
            closing.push(next.clone());
        }
        previous = Some(next);
        groups.push(group);
    }
    if ci != contributions.len() || si != carry.native_source_cash().len() {
        return Err("unused native coupon/cash receipt; no silent tail truncation".into());
    }
    check()?;
    let fully_closed = closing.iter().all(|p| {
        p.principal == 0.
            && p.book_adjustment == 0.
            && p.lots
                .iter()
                .all(|l| l.legal_remaining == 0. && l.recognized_remaining == 0.)
    });
    Ok(ResultData {
        schema: SCHEMA,
        policy: policy.clone(),
        groups,
        closing,
        complete_native_recovery_tail_retained: true,
        claims_fully_closed: fully_closed,
        actual_risk_source_binding: false,
        mixed_ledger_adoption: false,
        accounting_approval: false,
    })
}
