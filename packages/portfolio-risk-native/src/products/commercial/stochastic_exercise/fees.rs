//! Separate service rights in the common loan-numeraire stopping problem.
//! All trial cash comes from genuine native stopped loan/fee owners; acquisition
//! cost never enters the Pricing law or the loan original effective yield.
use super::*;
use crate::products::commercial::fee_source as source;
use portfolio_model_core::nonintegral_fee as fee;

#[cfg(test)]
thread_local! { pub(crate) static BINDINGS: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0, 0]) }; }
#[cfg(test)]
pub(crate) fn record_binding(raw: &owner::Request) {
    BINDINGS.with(|n| {
        let mut v = n.get();
        v[usize::from(raw.source.current.measure == CashflowMeasure::Physical)] += 1;
        n.set(v);
    });
}
pub const SCHEMA: &str = "commercial-generated-fee-law-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeLaw {
    pub schema: String,
    pub policy: source::Policy,
    pub discount_spread: f64,
}
#[derive(Default)]
pub(super) struct Payoffs {
    pub call_root_fee_value: Option<f64>,
    pub put_root_fee_value: Option<f64>,
}
pub(crate) fn feature_count(r: &Request) -> Result<usize, String> {
    r.nonintegral_fees.as_ref().map_or(Ok(0), |law| {
        law.policy
            .fees
            .len()
            .checked_mul(11)
            .ok_or("generated fee feature overflow".into())
    })
}
/// A raw-law proof for the *observed*, unstopped service state only. The caller
/// supplies the result of its paid census of every declared credit hazard.
/// Fixed service events and deterministic principal/maturity cannot vary with
/// the Gaussian coupon drivers. Stopped fee payoffs and all source bindings
/// still execute normally, and every observed coordinate remains in the fit.
pub(crate) fn observed_features_are_path_constant(
    r: &Request,
    all_credit_hazards_zero: bool,
) -> bool {
    all_credit_hazards_zero
        && r.nonintegral_fees
            .as_ref()
            .is_some_and(|law| law.policy.transfer.is_none())
        && r.source.performing_draw_tranches.is_empty()
        && r.source.source.transfer.is_none()
        && r.source.reviewed_credit_actions.is_none()
        && matches!(
            r.source.source.current.principal_plan,
            term::indexed::PrincipalPlan::Supplied { .. }
        )
}
pub(crate) fn admit(
    r: &Request,
    n: usize,
    loan: &owner::Admission,
) -> Result<(usize, usize), String> {
    let Some(law) = &r.nonintegral_fees else {
        return Ok((0, 0));
    };
    if law.schema != SCHEMA
        || !law.discount_spread.is_finite()
        || !(-5. ..=5.).contains(&law.discount_spread)
        || law.policy.coincident_order != "native_exit_follows_native_exercise_timing-1"
        || law.policy.transfer.is_some()
        || !r.source.performing_draw_tranches.is_empty()
    {
        return Err("generated original-tranche fee law requires separate spread and native exercise timing; drawn/secondary fee rights need separate ownership".into());
    }
    let (fee_work, fee_bytes) = source::admit(&law.policy, &r.source)?;
    let actions = r
        .policy
        .exercise_dates
        .iter()
        .try_fold(0usize, |sum, date| {
            sum.checked_add(
                usize::from(date.call_clean_price_factor.is_some())
                    + usize::from(date.put_clean_price_factor.is_some()),
            )
            .ok_or("generated fee action count overflow")
        })?;
    let groups = source::event_bound(&law.policy);
    // Each date binds the unstopped observable fee state, then each available
    // action binds a separate genuine stopped loan/fee source. One more binding
    // extracts the retained cash and one replays the selected final stop. Every
    // bind also validates the supplied fee policy. Count both fee evaluations.
    let bindings = 2usize
        .checked_add(r.policy.exercise_dates.len())
        .and_then(|v| v.checked_add(actions))
        .ok_or("generated fee binding count overflow")?;
    let per_bind = fee_work
        .checked_mul(2)
        .and_then(|v| v.checked_add(groups.checked_mul(1024)?))
        .ok_or("generated fee binding work overflow")?;
    let work = per_bind
        .checked_mul(bindings)
        .and_then(|v| v.checked_mul(n))
        .and_then(|v| {
            v.checked_add(
                loan.joined_work_upper
                    .checked_mul(actions)?
                    .checked_mul(n)?,
            )
        })
        .and_then(|v| v.checked_add(fee_work))
        .ok_or("generated fee work overflow")?;
    // The unstopped loan and fee coexist with one stopped trial pair. Retained
    // paths hold compact cash and root fee payoffs, never full fee journals.
    let path_bytes = groups
        .checked_mul(1024)
        .and_then(|v| v.checked_add(r.policy.exercise_dates.len().checked_mul(16)?))
        .ok_or("generated fee cash capacity overflow")?;
    let raw = strict_json::serialized_size(law, 1 << 20)?;
    let bytes = path_bytes
        .checked_mul(n)
        .and_then(|v| v.checked_add(loan.joined_bytes_upper))
        .and_then(|v| v.checked_add(fee_bytes.checked_mul(2)?))
        .and_then(|v| v.checked_add(raw.checked_mul(20)?))
        .ok_or("generated fee retained capacity overflow")?;
    Ok((work, bytes))
}
pub(crate) fn bind(
    r: &Request,
    raw: &owner::Request,
    carry: &portfolio_model_core::credit_path_income::ResultData,
    stops: &[owner::NativeStop],
) -> Result<Option<source::BoundPath>, String> {
    let Some(law) = &r.nonintegral_fees else {
        return Ok(None);
    };
    #[cfg(test)]
    record_binding(raw);
    let mut paths = source::bind_carry(&law.policy, raw, carry, stops)?;
    if paths.len() != 1 {
        return Err("one generated native fee path required".into());
    }
    let bound = paths.remove(0);
    source::adoption::require_complete(&bound)?;
    Ok(Some(bound))
}
pub(crate) fn cash(bound: &source::BoundPath) -> Result<Vec<EconomicCash>, String> {
    let mut result = Vec::new();
    for (year, group) in bound.native_years().iter().zip(bound.source().groups()) {
        check()?;
        let value = group
            .postings
            .iter()
            .filter(|p| p.account == fee::Account::Cash)
            .try_fold(0., |sum, p| amount(sum + p.amount))?;
        if value != 0. {
            result.push(EconomicCash {
                event_id: format!("separate-fee:{}", group.event.event_id),
                kind: "separate_nonintegral_fee".into(),
                year: *year,
                principal: 0.,
                interest: 0.,
                fees: value,
            });
        }
    }
    Ok(result)
}
pub(crate) fn discount(r: &Request, year: f64) -> Result<f64, String> {
    let law = r
        .nonintegral_fees
        .as_ref()
        .ok_or("generated fee law absent")?;
    amount(
        r.discount_curve.value_validated(year)?.discount_factor
            * (-(law.discount_spread + r.parallel_bump) * year).exp(),
    )
}
pub(super) fn point(
    r: &Request,
    raw: &owner::Request,
    carry: &portfolio_model_core::credit_path_income::ResultData,
    date: &term::exercise::ExerciseDate,
    observed: &Point,
    features: &mut Vec<f64>,
) -> Result<Option<Payoffs>, String> {
    let Some(law) = &r.nonintegral_fees else {
        return Ok(None);
    };
    let bound = bind(r, raw, carry, &[])?.ok_or("generated fee source absent")?;
    let year = f64::from(date.ordinal - raw.source.current.asof_ordinal) / 365.;
    let opening = raw.source.current.opening_principal;
    for f in &law.policy.fees {
        let state = bound
            .native_years()
            .iter()
            .zip(bound.source().groups())
            .filter(|(clock, g)| {
                g.event.fee_id == f.fee_id
                    && (**clock < year
                        || **clock == year && date.timing == ExerciseTiming::AfterScheduledCash)
            })
            .map(|(_, g)| g.next.clone())
            .next_back()
            .unwrap_or_else(|| fee::State {
                unearned_entitlement: f.entitlement,
                ..Default::default()
            });
        features.extend(
            [
                state.earned,
                state.refunded_earned,
                state.cancelled_unearned,
                state.waived_earned,
                state.recovered_waived,
                state.net_service_cash,
                state.fee_receivable,
                state.fee_payable,
                state.deferred_fee_income,
                state.prepaid_fee_expense,
                state.unearned_entitlement,
            ]
            .map(|v| v / opening),
        );
    }
    let mut result = Payoffs::default();
    if !live(observed.state) || observed.principal == 0. && !observed.surviving_rights {
        return Ok(Some(result));
    }
    for (name, action, factor, loan_fee, target) in [
        (
            "call",
            owner::NativeStopAction::Call,
            date.call_clean_price_factor,
            date.call_fee,
            &mut result.call_root_fee_value,
        ),
        (
            "put",
            owner::NativeStopAction::Put,
            date.put_clean_price_factor,
            date.put_fee,
            &mut result.put_root_fee_value,
        ),
    ] {
        let Some(factor) = factor else {
            continue;
        };
        let rights = if observed.principal == 0. {
            Some(
                r.policy
                    .rights_only_termination
                    .as_ref()
                    .ok_or("generated fee rights-only termination requires reviewed loan owner")?
                    .review
                    .clone(),
            )
        } else {
            None
        };
        let stop = owner::NativeStop::from_compiled_policy_with_timing(
            raw.credit.paths[0].path_key,
            format!(
                "generated-fee-trial:{}:{}:{name}",
                r.identity.id, date.ordinal
            ),
            date.ordinal,
            action,
            factor,
            loan_fee,
            r.policy.identity.clone(),
            match date.timing {
                ExerciseTiming::BeforeScheduledCash => owner::NativeStopTiming::BeforeScheduledCash,
                ExerciseTiming::AfterScheduledCash => owner::NativeStopTiming::AfterScheduledCash,
            },
            rights,
        )?;
        let stops = [stop];
        let stopped = if raw.source.current.measure == CashflowMeasure::Physical {
            owner::evaluate_physical_prefix_with_stops_for_acquisition(raw, &stops)?
        } else {
            owner::evaluate_economic_with_stops(raw, CreditMeasure::Pricing, &stops)?
        };
        let source = bind(r, raw, stopped.carry(), &stops)?.ok_or("generated fee trial absent")?;
        let mut value = 0.;
        for c in cash(&source)? {
            if c.year > year
                || c.year == year
                    && (date.timing == ExerciseTiming::BeforeScheduledCash
                        || c.event_id.starts_with("separate-fee:native-fee:"))
            {
                value = amount(value + c.total()? * discount(r, c.year)?)?;
            }
        }
        *target = Some(value);
    }
    Ok(Some(result))
}
pub(super) fn action_value(
    r: &Request,
    date: &term::exercise::ExerciseDate,
    p: &Point,
    call: bool,
) -> Result<Option<f64>, String> {
    let Some(fees) = &p.fee_payoffs else {
        return Err("generated fee action payoffs absent".into());
    };
    let (factor, loan_fee, root_fee) = if call {
        (
            date.call_clean_price_factor,
            date.call_fee,
            fees.call_root_fee_value,
        )
    } else {
        (
            date.put_clean_price_factor,
            date.put_fee,
            fees.put_root_fee_value,
        )
    };
    factor
        .map(|f| {
            let year = f64::from(date.ordinal - r.source.source.current.asof_ordinal) / 365.;
            amount(
                p.principal * f
                    + p.legal
                    + loan_fee
                    + root_fee.ok_or("generated complete fee action value absent")? / df(r, year)?,
            )
        })
        .transpose()
}
