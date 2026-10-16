//! Sealed source obligations survive terminal live-credit ownership. No caller
//! monetary checkpoint or future fixing is valued at the terminal clock.
use super::*;
use crate::models::credit::paths::PathEvent;
use crate::models::credit::transition::CreditState;

pub struct NativeTerminalQuery {
    pub path_key: u64,
    pub year: f64,
}
/// Source-owned factor recipe evaluated only at its own later information clock.
/// Core multiplies retained native entitlement; no monetary override is supplied.
pub struct NativeTerminalCouponStep {
    pub year: f64,
    pub original_settlement_year: f64,
    pub coupon_id: String,
    pub source_event_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub policy_id: String,
    pub captured_segment_index: usize,
    pub delta_factor: f64,
    pub entitlement: NativeCouponEntitlement,
    pub tranche_id: Option<String>,
}
enum Event {
    Fix(usize),
    Cash { step: usize, amount: f64 },
}

pub(super) fn continue_obligations(
    owner: &mut Owner<'_>,
    p: &mut CreditPathResult,
    horizon: f64,
) -> Result<(), String> {
    owner.check()?;
    if owner.maximum_terminal_steps == 0 {
        return Ok(());
    }
    let terminal = if let Some(default) = &p.default {
        Some((default.year, default.origin, true))
    } else {
        p.events
            .iter()
            .find(|e| e.kind == "prepayment" || e.kind == "maturity")
            .map(|e| (e.year, e.origin, false))
    };
    let Some((year, origin, is_default)) = terminal else {
        return Ok(());
    };
    let origin_index = match origin {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        _ => return Err("pending coupon terminal origin must be live".into()),
    };
    let interval_index = owner
        .request
        .raw_credit_path
        .intervals
        .iter()
        .position(|i| year <= i.end_year)
        .ok_or("pending coupon default interval missing")?;
    let steps = owner
        .factor_source
        .as_deref()
        .ok_or("pending coupon source authority missing")?
        .terminal_coupon_steps(&NativeTerminalQuery {
            path_key: p.path_key,
            year,
        })?;
    if steps.len() > owner.maximum_terminal_steps {
        return Err("pending coupon source steps exceed preflight".into());
    }
    if steps.is_empty() {
        return Ok(());
    }
    let event_upper = owner
        .tail_meter
        .as_ref()
        .map(|m| {
            mul(
                owner.maximum_terminal_steps,
                add(1, m.maximum_recoveries())?,
            )
        })
        .transpose()?;
    let mut events = Vec::new();
    let mut amounts = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        owner.check()?;
        if !step.year.is_finite()
            || step.year <= year
            || step.year > horizon
            || !step.original_settlement_year.is_finite()
            || step.original_settlement_year < step.year
            || !step.delta_factor.is_finite()
        {
            return Err("pending coupon requires complete subsequent information and actual settlement clocks".into());
        }
        for label_ in [
            &step.coupon_id,
            &step.source_event_id,
            &step.source_id,
            &step.source_revision,
            &step.policy_id,
        ] {
            label(label_)?;
        }
        let amount = step.entitlement.opening_entitlement_principal * step.delta_factor;
        bounded(amount)?;
        amounts.push(amount);
        check_event_capacity(events.len(), event_upper)?;
        events.push((step.year, 0, Event::Fix(index)));
        if is_default {
            let term = &owner.request.raw_credit_path.intervals[interval_index];
            for tail in &term.complete_recovery_tails[origin_index] {
                let cash = amount * term.interest_recovery_fraction[origin_index] * tail.weight;
                bounded(cash)?;
                let cash_year = (year + tail.lag_years).max(step.year);
                if cash_year > horizon {
                    return Err("pending default recovery exceeds admitted complete tail".into());
                }
                check_event_capacity(events.len(), event_upper)?;
                events.push((
                    cash_year,
                    1,
                    Event::Cash {
                        step: index,
                        amount: cash,
                    },
                ));
            }
        } else {
            // Retain original legal coupon settlement. Publication is not an
            // authorization to accelerate payment or invent a terminal payoff.
            check_event_capacity(events.len(), event_upper)?;
            events.push((
                step.original_settlement_year,
                1,
                Event::Cash {
                    step: index,
                    amount,
                },
            ));
        }
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let begin = owner
        .points
        .iter()
        .position(|v| {
            v.path_key == p.path_key
                && v.year == year
                && v.kind
                    == if is_default {
                        "default_gross_transfer"
                    } else {
                        "full_live_exit"
                    }
        })
        .ok_or("pending coupon terminal carry origin missing")?;
    let old = owner.points.split_off(begin + 1);
    let mut baseline = owner.points[begin].clone();
    let mut old = old.into_iter().peekable();
    let mut extra_gross = 0.;
    let mut extra_legal = 0.;
    for (clock, _, event) in events {
        owner.check()?;
        // Existing recovery cash owns its original same-clock phase before
        // newly released fixings; pending cash always follows its fixing.
        while old.peek().is_some_and(|v| v.year <= clock) {
            baseline = old.next().ok_or("pending old carry cursor missing")?;
            let mut merged = baseline.clone();
            merged.gross_carry += extra_gross;
            merged.contractual_accrued_interest += extra_legal;
            merged.book_adjustment =
                merged.gross_carry - merged.principal - merged.contractual_accrued_interest;
            bounded(merged.gross_carry)?;
            bounded(merged.contractual_accrued_interest)?;
            bounded(merged.book_adjustment)?;
            owner.points.push(merged);
        }
        let mut point = baseline.clone();
        point.year = clock;
        point.origin_index = Some(origin_index);
        point.accounting_rate = None;
        point.pending_coupon_obligation_indices = owner.pending_indices(p.path_key, year, clock)?;
        point.effective_interest = 0.;
        point.cash = 0.;
        point.basis_realization = 0.;
        point.canonical_legal_coupons.clear();
        point.draw_tranches.clear();
        match event {
            Event::Fix(index) => {
                let step = &steps[index];
                let amount = amounts[index];
                extra_legal += amount;
                if !is_default {
                    extra_gross += amount;
                    point.effective_interest = amount;
                }
                point.kind = "pending_coupon_fixing";
                owner.retain(tail_bounds::Kind::Contribution)?;
                owner.contributions.push(EarnedOriginContribution {
                    path_key: p.path_key,
                    interval_index,
                    origin_index,
                    start_year: year,
                    end_year: clock,
                    coupon_id: step.coupon_id.clone(),
                    source_event_id: step.source_event_id.clone(),
                    source_id: step.source_id.clone(),
                    source_revision: step.source_revision.clone(),
                    captured_segment_index: step.captured_segment_index,
                    delta_factor: step.delta_factor,
                    actual_principal: step.entitlement.opening_entitlement_principal,
                    contractual_earned: amount,
                    native_entitlement: Some(step.entitlement.clone()),
                });
                p.earned_interest += amount;
                p.earned_interest_by_origin[origin_index] += amount;
                bounded(p.earned_interest)?;
                bounded(p.earned_interest_by_origin[origin_index])?;
            }
            Event::Cash {
                step: index,
                amount,
            } => {
                let step = &steps[index];
                extra_gross -= amount;
                extra_legal -= amount;
                point.cash = amount;
                point.kind = "pending_coupon_cash";
                owner.retain(tail_bounds::Kind::TerminalCash)?;
                owner.source_cash.push(SourceCashReceipt {
                    path_key: p.path_key,
                    year: clock,
                    cash: NativeContractualCash {
                        coupon_id: step.coupon_id.clone(),
                        source_event_id: step.source_event_id.clone(),
                        native_cash_index: step.entitlement.native_cash_index,
                        principal: 0.,
                        interest: amount,
                        fee: 0.,
                        tranche_id: step.tranche_id.clone(),
                    },
                });
                p.cash_received += amount;
                bounded(p.cash_received)?;
                owner.retain(tail_bounds::Kind::TerminalEvent)?;
                p.events.push(PathEvent {
                    year: clock,
                    event_id: format!(
                        "pending-coupon:{}:{}",
                        step.coupon_id,
                        owner.source_cash.len()
                    ),
                    policy_id: step.policy_id.clone(),
                    origin,
                    destination: None,
                    kind: "pending_coupon_cash".into(),
                    cash: amount,
                    principal: 0.,
                    interest: amount,
                    book_adjustment_recognition: 0.,
                });
            }
        }
        point.gross_carry = baseline.gross_carry + extra_gross;
        point.contractual_accrued_interest = baseline.contractual_accrued_interest + extra_legal;
        point.book_adjustment =
            point.gross_carry - point.principal - point.contractual_accrued_interest;
        bounded(point.gross_carry)?;
        bounded(point.contractual_accrued_interest)?;
        bounded(point.book_adjustment)?;
        if owner.points.len()
            + owner.contributions.len()
            + owner.source_cash.len()
            + owner.recoveries.len()
            + owner.coupon_recoveries.len()
            >= owner.row_upper
        {
            return Err("pending coupon rows exceed admitted retained capacity".into());
        }
        owner.retain(tail_bounds::Kind::Point)?;
        owner.points.push(point);
    }
    for baseline_ in old {
        owner.check()?;
        baseline = baseline_.clone();
        let mut merged = baseline_;
        merged.gross_carry += extra_gross;
        merged.contractual_accrued_interest += extra_legal;
        merged.book_adjustment =
            merged.gross_carry - merged.principal - merged.contractual_accrued_interest;
        bounded(merged.gross_carry)?;
        bounded(merged.contractual_accrued_interest)?;
        bounded(merged.book_adjustment)?;
        owner.points.push(merged);
    }
    owner.gross = baseline.gross_carry + extra_gross;
    p.closing_accrued_interest += extra_legal;
    p.closing_book_adjustment = owner.gross - p.closing_principal - p.closing_accrued_interest;
    bounded(owner.gross)?;
    bounded(p.closing_accrued_interest)?;
    bounded(p.closing_book_adjustment)?;
    Ok(())
}

fn check_event_capacity(len: usize, upper: Option<usize>) -> Result<(), String> {
    if upper.is_some_and(|upper| len >= upper) {
        return Err("selected terminal event buffer exceeded before growth".into());
    }
    Ok(())
}
