//! Finite conditional stopping with separate loan and service-fee discounting.
//! Decisions use observable prefixes; realized future cash only enters conditional
//! expectations inside the complete observed-information group.
use super::*;
use crate::products::commercial::fee_source::{self as fees, BoundPath};
use portfolio_model_core::nonintegral_fee::{self as fee, Account};

fn cash(source: &BoundPath) -> Result<Vec<commercial_cash::EconomicCash>, String> {
    source
        .native_years()
        .iter()
        .zip(source.source().groups())
        .filter_map(|(year, g)| {
            let value = g
                .postings
                .iter()
                .filter(|p| p.account == Account::Cash)
                .try_fold(0., |sum, p| term::amount(sum + p.amount));
            match value {
                Ok(0.) => None,
                Ok(amount) => Some(Ok(commercial_cash::EconomicCash {
                    event_id: format!("separate-fee:{}", g.event.event_id),
                    kind: "separate_nonintegral_fee".into(),
                    year: *year,
                    principal: 0.,
                    interest: 0.,
                    fees: amount,
                })),
                Err(e) => Some(Err(e)),
            }
        })
        .collect()
}
fn source(
    policy: &fees::Policy,
    raw: &owner::Request,
    carry: &carry::ResultData,
    stops: &[owner::NativeStop],
) -> Result<BoundPath, String> {
    let mut sources = fees::bind_carry(policy, raw, carry, stops)?;
    if sources.len() != 1 {
        return Err("joint exercise requires one exact native fee path".into());
    }
    let source = sources.remove(0);
    fees::adoption::require_complete(&source)?;
    Ok(source)
}
pub(super) fn information(
    raw: &owner::Request,
    carry: &carry::ResultData,
    policy: &fees::Policy,
    ordinal: i32,
    timing: ExerciseTiming,
) -> Result<Value, String> {
    let bound = source(policy, raw, carry, &[])?;
    information_bound(raw, carry, policy, &bound, ordinal, timing)
}
fn information_bound(
    raw: &owner::Request,
    carry: &carry::ResultData,
    policy: &fees::Policy,
    bound: &BoundPath,
    ordinal: i32,
    timing: ExerciseTiming,
) -> Result<Value, String> {
    let mut info = super::information(raw, carry, ordinal, timing)?;
    let t = f64::from(ordinal - raw.source.current.asof_ordinal) / 365.;
    let mut states = policy
        .fees
        .iter()
        .map(|f| {
            (
                f.fee_id.as_str(),
                fee::State {
                    unearned_entitlement: f.entitlement,
                    ..Default::default()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut events = Vec::new();
    for (year, g) in bound.native_years().iter().zip(bound.source().groups()) {
        if *year < t || *year == t && timing == ExerciseTiming::AfterScheduledCash {
            states.insert(g.event.fee_id.as_str(), g.next.clone());
            events.push(json!({"year":year,"fee_id":g.event.fee_id,"action":g.event.action,"reason":g.event.reason,"amount":g.event.amount}));
        }
    }
    info["separate_fees"] = json!({"contracts":policy.fees,"policy":policy.policy,"default":policy.default,"prepayment":policy.prepayment,"exercise":policy.exercise,"maturity":policy.maturity,"coincident_order":policy.coincident_order,"states":states,"events":events});
    Ok(info)
}
fn fee_value(
    cash: &[commercial_cash::EconomicCash],
    curve: &SuppliedCurve,
    t: f64,
    spread: f64,
    bump: f64,
    include: impl Fn(&commercial_cash::EconomicCash) -> bool,
) -> Result<f64, String> {
    let discount = df(curve, t, spread, bump)?;
    cash.iter().filter(|c| include(c)).try_fold(0., |sum, c| {
        term::amount(sum + c.total()? * df(curve, c.year, spread, bump)? / discount)
    })
}
#[derive(Clone, Copy)]
pub struct Market<'a> {
    pub curve: &'a SuppliedCurve,
    pub oas: f64,
    pub spread: f64,
    pub bump: f64,
}
struct StoppedCash {
    event_id: String,
    loan: Vec<commercial_cash::EconomicCash>,
    fee: Vec<commercial_cash::EconomicCash>,
}
pub struct PreparedFeeExercise {
    identity: Vec<u8>,
    bounds: Vec<BoundPath>,
    loan_cash: Vec<Vec<commercial_cash::EconomicCash>>,
    fee_cash: Vec<Vec<commercial_cash::EconomicCash>>,
    stops: BTreeMap<(i32, u8, usize), StoppedCash>,
}
fn action_key(action: owner::NativeStopAction) -> u8 {
    match action {
        owner::NativeStopAction::Call => 0,
        owner::NativeStopAction::Put => 1,
    }
}
fn identity(
    policy: &Policy,
    views: &[PricingView<'_>],
    fees: &[&fees::Policy],
) -> Result<Vec<u8>, String> {
    strict_json::encode(
        &(
            policy,
            views.iter().map(|v| (v.raw, v.weight)).collect::<Vec<_>>(),
            fees,
        ),
        32 << 20,
    )
}
fn stop_values(
    cash: &StoppedCash,
    date: &ExerciseDate,
    asof: i32,
    market: &Market<'_>,
) -> Result<(f64, f64), String> {
    let Market {
        curve,
        oas,
        spread,
        bump,
    } = *market;
    let t = f64::from(date.ordinal - asof) / 365.;
    let discount = df(curve, t, oas, bump)?;
    let loan_value = cash
        .loan
        .iter()
        .filter(|c| {
            c.year > t
                || c.year == t
                    && (date.timing == ExerciseTiming::BeforeScheduledCash
                        || c.event_id == cash.event_id)
        })
        .try_fold(0., |sum, c| {
            term::amount(sum + c.total()? * df(curve, c.year, oas, bump)? / discount)
        })?;
    let fee_value = fee_value(&cash.fee, curve, t, spread, bump, |c| {
        c.year > t
            || c.year == t
                && (date.timing == ExerciseTiming::BeforeScheduledCash
                    || c.event_id.starts_with("separate-fee:native-fee:"))
    })?;
    Ok((loan_value, fee_value))
}
pub fn prepare(
    policy: &Policy,
    views: &[PricingView<'_>],
    fee_policies: &[&fees::Policy],
) -> Result<PreparedFeeExercise, String> {
    check()?;
    let mut work = admitted_reduction_work(
        policy,
        &views.iter().map(|v| (v.raw, v.weight)).collect::<Vec<_>>(),
    )?;
    if fee_policies.len() != views.len() {
        return Err("joint exercise requires complete separately priced fee policies".into());
    }
    let mut bytes = 0usize;
    for (v, p) in views.iter().zip(fee_policies) {
        if p.coincident_order != "native_exit_follows_native_exercise_timing-1" {
            return Err(
                "joint fee exercise requires explicit same native before/after cash timing".into(),
            );
        }
        let (w, b) = fees::admit(p, v.raw)?;
        let trials = policy
            .exercise_dates
            .iter()
            .map(|d| {
                usize::from(d.call_clean_price_factor.is_some())
                    + usize::from(d.put_clean_price_factor.is_some())
            })
            .sum::<usize>();
        let a = owner::admit_economic_with_measure(
            v.raw,
            portfolio_model_core::credit::CreditMeasure::Pricing,
        )?;
        work = work
            .checked_add(
                w.checked_mul(trials + 1)
                    .ok_or("joint fee exercise work overflow")?,
            )
            .and_then(|n| n.checked_add(a.joined_work_upper.checked_mul(trials)?))
            .ok_or("joint fee exercise work overflow")?;
        bytes = bytes
            .checked_add(
                b.checked_add(
                    a.carry_points_upper
                        .checked_mul(2048)
                        .ok_or("joint fee cash capacity overflow")?,
                )
                .and_then(|n| n.checked_mul(trials + 1))
                .and_then(|n| n.checked_add(a.raw_bytes))
                .ok_or("joint fee cash capacity overflow")?,
            )
            .ok_or("joint fee cash capacity overflow")?;
    }
    if work > 8_000_000 || bytes > 128 << 20 {
        return Err("joint finite fee exercise exceeds8M/128MiB before generation".into());
    }
    let identity = identity(policy, views, fee_policies)?;
    let bounds = views
        .iter()
        .zip(fee_policies)
        .map(|(v, p)| source(p, v.raw, v.carry, &[]))
        .collect::<Result<Vec<_>, _>>()?;
    let loan_cash = views
        .iter()
        .map(|v| commercial_cash::economic_cash(v.carry))
        .collect::<Result<Vec<_>, _>>()?;
    let fee_cash = bounds.iter().map(cash).collect::<Result<Vec<_>, _>>()?;
    let mut stops = BTreeMap::new();
    for date in &policy.exercise_dates {
        for (i, (view, fee)) in views.iter().zip(fee_policies).enumerate() {
            let t = f64::from(date.ordinal - view.raw.source.current.asof_ordinal) / 365.;
            let (state, p, _) = super::state(view.raw, view.carry, t, date.timing)?;
            let rights_only = live(state)
                && p == 0.
                && surviving_contractual_rights(view.raw, view.carry, date.ordinal, date.timing)?;
            if !live(state) || p == 0. && !rights_only {
                continue;
            }
            for action in [owner::NativeStopAction::Call, owner::NativeStopAction::Put] {
                if match action {
                    owner::NativeStopAction::Call => date.call_clean_price_factor.is_none(),
                    owner::NativeStopAction::Put => date.put_clean_price_factor.is_none(),
                } {
                    continue;
                }
                let (stop, loan) = stopped_source(view.raw, policy, date, action)?;
                let bound = source(fee, view.raw, loan.carry(), std::slice::from_ref(&stop))?;
                stops.insert(
                    (date.ordinal, action_key(action), i),
                    StoppedCash {
                        event_id: stop.event_id().to_owned(),
                        loan: commercial_cash::economic_cash(loan.carry())?,
                        fee: cash(&bound)?,
                    },
                );
            }
        }
    }
    Ok(PreparedFeeExercise {
        identity,
        bounds,
        loan_cash,
        fee_cash,
        stops,
    })
}
pub fn compile(
    policy: &Policy,
    views: &[PricingView<'_>],
    fee_policies: &[&fees::Policy],
    curve: &SuppliedCurve,
    oas: f64,
    spread: f64,
    bump: f64,
) -> Result<NativeExercisePolicy, String> {
    prepare(policy, views, fee_policies)?.compile(
        policy,
        views,
        fee_policies,
        Market {
            curve,
            oas,
            spread,
            bump,
        },
    )
}
impl PreparedFeeExercise {
    pub fn compile(
        &self,
        policy: &Policy,
        views: &[PricingView<'_>],
        fee_policies: &[&fees::Policy],
        market: Market<'_>,
    ) -> Result<NativeExercisePolicy, String> {
        if self.identity != identity(policy, views, fee_policies)? {
            return Err("retained joint exercise cash source identity changed".into());
        }
        let Market {
            curve,
            oas,
            spread,
            bump,
        } = market;
        if !spread.is_finite() || !(-5. ..=5.).contains(&spread) {
            return Err("explicit joint fee spread required".into());
        }
        curve.validate()?;
        term::amount(oas)?;
        term::amount(bump)?;
        if !(-5. ..=5.).contains(&oas) || !(-0.1..=0.1).contains(&bump) {
            return Err("joint exercise spread/bump domain".into());
        }
        let first = &views[0].raw.source.current;
        if curve.currency != first.currency
            || views.iter().any(|v| {
                v.carry.credit().result.measure
                    != portfolio_model_core::credit::CreditMeasure::Pricing
            })
        {
            return Err("joint continuation requires explicit Pricing source and currency".into());
        }
        let bounds = &self.bounds;
        let root = encode(&information_bound(
            views[0].raw,
            views[0].carry,
            fee_policies[0],
            &bounds[0],
            first.asof_ordinal,
            ExerciseTiming::AfterScheduledCash,
        )?)?;
        for ((v, p), b) in views.iter().zip(fee_policies).zip(bounds) {
            if encode(&information_bound(
                v.raw,
                v.carry,
                p,
                b,
                first.asof_ordinal,
                ExerciseTiming::AfterScheduledCash,
            )?)? != root
            {
                return Err(
                    "joint Pricing paths disagree on initial loan/fee observable information"
                        .into(),
                );
            }
        }
        let loan_cash = &self.loan_cash;
        let fee_cash = &self.fee_cash;
        let market = Market {
            curve,
            oas,
            spread,
            bump,
        };
        let mut next = vec![(0., 0.); views.len()];
        let mut next_time = None;
        let mut keys = BTreeMap::new();
        let mut nodes = Vec::new();
        for date in policy.exercise_dates.iter().rev() {
            let t = f64::from(date.ordinal - first.asof_ordinal) / 365.;
            let ld = df(curve, t, oas, bump)?;
            let fd = df(curve, t, spread, bump)?;
            // All choices optimize the same reported root total. Separate
            // component discounting requires conversion to one node numeraire;
            // adding their individually rebased local values is inconsistent.
            let fee_numeraire = fd / ld;
            let score = |value: (f64, f64)| term::amount(value.0 + fee_numeraire * value.1);
            let mut groups = BTreeMap::<Vec<u8>, (Value, Vec<usize>)>::new();
            for (i, ((v, p), bound)) in views.iter().zip(fee_policies).zip(bounds).enumerate() {
                let info = information_bound(v.raw, v.carry, p, bound, date.ordinal, date.timing)?;
                groups
                    .entry(encode(&info)?)
                    .or_insert_with(|| (info, Vec::new()))
                    .1
                    .push(i);
            }
            let mut now = vec![(0., 0.); views.len()];
            for (key, (info, members)) in groups {
                check()?;
                let probability = members.iter().map(|&i| views[i].weight).sum::<f64>();
                if probability == 0. {
                    continue;
                }
                let mut continuation = (0., 0.);
                for &i in &members {
                    let mut value = if let Some((end, _)) = next_time {
                        (
                            next[i].0 * df(curve, end, oas, bump)? / ld,
                            next[i].1 * df(curve, end, spread, bump)? / fd,
                        )
                    } else {
                        (0., 0.)
                    };
                    for c in loan_cash[i].iter().filter(|c| {
                        cash_after(c.year, t, date.timing)
                            && next_time
                                .is_none_or(|(end, timing)| cash_before(c.year, end, timing))
                    }) {
                        value.0 = term::amount(
                            value.0 + c.total()? * df(curve, c.year, oas, bump)? / ld,
                        )?;
                    }
                    for c in fee_cash[i].iter().filter(|c| {
                        cash_after(c.year, t, date.timing)
                            && next_time
                                .is_none_or(|(end, timing)| cash_before(c.year, end, timing))
                    }) {
                        value.1 = term::amount(
                            value.1 + c.total()? * df(curve, c.year, spread, bump)? / fd,
                        )?;
                    }
                    continuation.0 =
                        term::amount(continuation.0 + views[i].weight / probability * value.0)?;
                    continuation.1 =
                        term::amount(continuation.1 + views[i].weight / probability * value.1)?;
                }
                let (state, p, a) = super::state(
                    views[members[0]].raw,
                    views[members[0]].carry,
                    t,
                    date.timing,
                )?;
                let rights_only = live(state)
                    && p == 0.
                    && surviving_contractual_rights(
                        views[members[0]].raw,
                        views[members[0]].carry,
                        date.ordinal,
                        date.timing,
                    )?;
                if rights_only && policy.rights_only_termination.is_none() {
                    return Err(
                        "joint rights-only exercise requires explicit termination owner".into(),
                    );
                }
                let mut chosen = continuation;
                let mut decision = Decision::Continue;
                if live(state) && (p > 0. || rights_only) {
                    for action in match policy.simultaneous_priority {
                        SimultaneousPriority::Issuer => {
                            [owner::NativeStopAction::Put, owner::NativeStopAction::Call]
                        }
                        SimultaneousPriority::Holder => {
                            [owner::NativeStopAction::Call, owner::NativeStopAction::Put]
                        }
                    } {
                        let available = match action {
                            owner::NativeStopAction::Call => date.call_clean_price_factor.is_some(),
                            owner::NativeStopAction::Put => date.put_clean_price_factor.is_some(),
                        };
                        if !available {
                            continue;
                        }
                        let mut payoff = (0., 0.);
                        for &i in &members {
                            let prepared = self
                                .stops
                                .get(&(date.ordinal, action_key(action), i))
                                .ok_or("joint native stopped cash absent")?;
                            let x = stop_values(prepared, date, first.asof_ordinal, &market)?;
                            payoff.0 =
                                term::amount(payoff.0 + views[i].weight / probability * x.0)?;
                            payoff.1 =
                                term::amount(payoff.1 + views[i].weight / probability * x.1)?;
                        }
                        let improve = match action {
                            owner::NativeStopAction::Call => score(payoff)? < score(chosen)?,
                            owner::NativeStopAction::Put => score(payoff)? > score(chosen)?,
                        };
                        if improve {
                            chosen = payoff;
                            decision = match action {
                                owner::NativeStopAction::Call => Decision::Call,
                                owner::NativeStopAction::Put => Decision::Put,
                            };
                        }
                    }
                }
                for i in members {
                    now[i] = chosen;
                }
                keys.insert((date.ordinal, key), nodes.len());
                nodes.push(Node {
                    ordinal: date.ordinal,
                    timing: date.timing,
                    information: info,
                    probability,
                    continuation_value: score(continuation)?,
                    value: score(chosen)?,
                    decision,
                    live_principal: p,
                    legal_interest: a,
                });
            }
            next = now;
            next_time = Some((t, date.timing));
        }
        let mut values = (0., 0.);
        for (i, v) in views.iter().enumerate() {
            let mut pv = if let Some((end, _)) = next_time {
                (
                    next[i].0 * df(curve, end, oas, bump)?,
                    next[i].1 * df(curve, end, spread, bump)?,
                )
            } else {
                (0., 0.)
            };
            for c in loan_cash[i].iter().filter(|c| {
                c.year > 0.
                    && next_time.is_none_or(|(end, timing)| cash_before(c.year, end, timing))
            }) {
                pv.0 = term::amount(pv.0 + c.total()? * df(curve, c.year, oas, bump)?)?;
            }
            for c in fee_cash[i].iter().filter(|c| {
                c.year > 0.
                    && next_time.is_none_or(|(end, timing)| cash_before(c.year, end, timing))
            }) {
                pv.1 = term::amount(pv.1 + c.total()? * df(curve, c.year, spread, bump)?)?;
            }
            values.0 = term::amount(values.0 + v.weight * pv.0)?;
            values.1 = term::amount(values.1 + v.weight * pv.1)?;
        }
        let out = NativeExercisePolicy {
            policy: policy.clone(),
            value: term::amount(values.0 + values.1)?,
            nodes,
            keys,
            asof: first.asof_ordinal,
            position_id: first.position_id.clone(),
            fixed_base_oas: oas,
            parallel_bump: bump,
            finite_distribution: true,
            continuous_model_convergence: false,
            joint_fee_values: Some(JointFeeValues {
                loan_value: values.0,
                fee_value: values.1,
                fee_discount_spread: spread,
                decision_criterion: "root_total_loan_and_fee_in_loan_numeraire-1",
            }),
        };
        strict_json::serialized_size(&out, 32 << 20)?;
        check()?;
        Ok(out)
    }
}
