//! Native conditional stopping on an explicit finite Pricing distribution.
//! Information sets contain complete observed prefixes, never future outcomes.
//! This is exact for that finite distribution, not a fitted continuous-state law.
use crate::{
    commercial_cash, commercial_credit_compiler as owner,
    model_market::curves::SuppliedCurve,
    term_enhancements::{
        self as term,
        exercise::{Decision, ExerciseDate, ExerciseTiming, SimultaneousPriority},
    },
};
use portfolio_model_core::{
    cashflow::validate_ordinal, credit::CreditState, credit_path_income as carry, strict_json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "commercial-native-exercise-1";
const MAX_PATHS: usize = 64;
const MAX_DATES: usize = 128;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    pub identity: term::Identity,
    pub information_policy: String,
    pub exercise_dates: Vec<ExerciseDate>,
    pub simultaneous_priority: SimultaneousPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights_only_termination: Option<RightsOnlyTermination>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RightsOnlyTermination {
    pub review: term::Identity,
    pub disposition: String,
}
pub struct PricingView<'a> {
    pub raw: &'a owner::Request,
    pub carry: &'a carry::ResultData,
    pub weight: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub ordinal: i32,
    pub timing: ExerciseTiming,
    /// Canonical observed source/event/state data. No future draw is included.
    pub information: Value,
    pub probability: f64,
    pub continuation_value: f64,
    pub value: f64,
    pub decision: Decision,
    pub live_principal: f64,
    pub legal_interest: f64,
}
#[derive(Debug, Serialize)]
pub struct NativeExercisePolicy {
    pub policy: Policy,
    pub value: f64,
    pub nodes: Vec<Node>,
    pub fixed_base_oas: f64,
    pub parallel_bump: f64,
    pub finite_distribution: bool,
    pub continuous_model_convergence: bool,
    #[serde(skip)]
    keys: BTreeMap<(i32, Vec<u8>), usize>,
    #[serde(skip)]
    asof: i32,
    #[serde(skip)]
    position_id: String,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn encode(v: &Value) -> Result<Vec<u8>, String> {
    strict_json::encode(v, 1 << 20)
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * a.abs().max(b.abs())
}
/// Charge signature materialization, all cash scans and backward decisions
/// before any caller constructs the no-option native sources.
pub fn admitted_work(p: &Policy, raws: &[(&owner::Request, f64)]) -> Result<usize, String> {
    admitted_work_with_measure(
        p,
        raws,
        portfolio_model_core::credit::CreditMeasure::Pricing,
    )
}
pub(crate) fn admitted_work_with_measure(
    p: &Policy,
    raws: &[(&owner::Request, f64)],
    measure: portfolio_model_core::credit::CreditMeasure,
) -> Result<usize, String> {
    check()?;
    if p.schema != SCHEMA
        || p.information_policy != "native_full_observed_prefix-1"
        || p.exercise_dates.len() > MAX_DATES
        || raws.is_empty()
        || raws.len() > MAX_PATHS
    {
        return Err(
            "bounded native commercial finite-distribution exercise policy required".into(),
        );
    }
    let mut metadata = 0;
    term::identity(&p.identity, &mut metadata)?;
    if let Some(rights) = &p.rights_only_termination {
        term::identity(&rights.review, &mut metadata)?;
        if rights.disposition != "settle_earned_cancel_future-1" {
            return Err(
                "explicit rights-only earned settlement and future cancellation policy required"
                    .into(),
            );
        }
    }
    let s = &raws[0].0.source.current;
    let mut prior = s.asof_ordinal;
    for d in &p.exercise_dates {
        check()?;
        validate_ordinal(d.ordinal)?;
        if d.ordinal <= prior
            || d.ordinal > s.tail_end_ordinal
            || (d.call_clean_price_factor.is_none() && d.put_clean_price_factor.is_none())
        {
            return Err(
                "native exercise requires exact increasing dates in the complete contractual tail"
                    .into(),
            );
        }
        for f in d
            .call_clean_price_factor
            .into_iter()
            .chain(d.put_clean_price_factor)
        {
            if !f.is_finite() || !(0. ..=10.).contains(&f) {
                return Err("native exercise clean factor outside [0,10]".into());
            }
        }
        term::nonnegative(d.call_fee)?;
        term::nonnegative(d.put_fee)?;
        if (d.call_clean_price_factor.is_none() && d.call_fee != 0.)
            || (d.put_clean_price_factor.is_none() && d.put_fee != 0.)
        {
            return Err("native exercise fee requires the matching reviewed right".into());
        }
        prior = d.ordinal;
    }
    let mut weights = 0.;
    let mut ids = BTreeSet::new();
    let mut work = 0usize;
    for (raw, weight) in raws {
        check()?;
        let c = &raw.source.current;
        if c.asof_ordinal != s.asof_ordinal
            || c.position_id != s.position_id
            || c.currency != s.currency
            || c.tail_end_ordinal != s.tail_end_ordinal
            || raw.credit.paths.len() != 1
            || !ids.insert(raw.credit.paths[0].path_key)
            || !weight.is_finite()
            || !(0. ..=1.).contains(weight)
            || *weight == 0.
        {
            return Err("one unique weighted native credit path per conditional source and complete date/position/currency identity required".into());
        }
        weights += weight;
        let a = owner::admit_economic_with_measure(raw, measure)?;
        let per = a
            .carry_points_upper
            .checked_add(a.raw_bytes)
            .ok_or("native exercise work overflow")?;
        work = work
            .checked_add(
                per.checked_mul(p.exercise_dates.len() + 1)
                    .ok_or("native exercise work overflow")?,
            )
            .ok_or("native exercise work overflow")?;
    }
    if !near(weights, 1.) || work > 8_000_000 {
        return Err(
            "native exercise requires complete unit probability and <=8M conditional work".into(),
        );
    }
    Ok(work)
}

pub(crate) fn state(
    raw: &owner::Request,
    c: &carry::ResultData,
    t: f64,
    timing: ExerciseTiming,
) -> Result<(CreditState, f64, f64), String> {
    let key = raw
        .credit
        .paths
        .first()
        .ok_or("native exercise path absent")?
        .path_key;
    let path = c
        .credit()
        .result
        .paths
        .iter()
        .find(|p| p.path_key == key)
        .ok_or("native exercise credit receipt absent")?;
    let mut state = raw.credit.paths[0].initial_live_state;
    for e in path
        .events
        .iter()
        .filter(|e| observed(e.year, &e.kind, t, timing))
    {
        if let Some(next) = e.destination {
            state = next;
        }
        if e.kind == "maturity" {
            state = CreditState::Prepaid;
        }
    }
    if path.default.as_ref().is_some_and(|d| d.year <= t) {
        state = CreditState::Default;
    }
    let point = point(c, key, t, timing)?;
    if point.year.to_bits() != t.to_bits()
        && !matches!(state, CreditState::Default | CreditState::Prepaid)
    {
        return Err(
            "native exercise requires an exact native daily point, never interpolated money".into(),
        );
    }
    Ok((state, point.principal, point.contractual_accrued_interest))
}
pub(crate) fn point(
    c: &carry::ResultData,
    key: u64,
    t: f64,
    timing: ExerciseTiming,
) -> Result<&carry::CarryPoint, String> {
    let points = c.carry_points();
    // The earned endpoint precedes same-clock scheduled settlement. Use its
    // native snapshot, never subtract cash from a rounded post-cash balance.
    let before = if timing == ExerciseTiming::BeforeScheduledCash {
        points.iter().position(|p| {
            p.path_key == key
                && p.year.to_bits() == t.to_bits()
                && matches!(
                    p.kind,
                    "contractual_cash" | "funded_draw" | "full_live_exit"
                )
        })
    } else {
        None
    };
    points[..before.unwrap_or(points.len())]
        .iter()
        .rfind(|p| p.path_key == key && p.year <= t)
        .ok_or("native exercise complete point prefix absent".into())
}
fn observed(year: f64, kind: &str, t: f64, timing: ExerciseTiming) -> bool {
    year < t
        || year == t
            && (timing == ExerciseTiming::AfterScheduledCash
                || !matches!(kind, "repayment" | "maturity" | "draw"))
}
pub(crate) fn cash_after(year: f64, t: f64, timing: ExerciseTiming) -> bool {
    year > t || year == t && timing == ExerciseTiming::BeforeScheduledCash
}
pub(crate) fn cash_before(year: f64, t: f64, timing: ExerciseTiming) -> bool {
    year < t || year == t && timing == ExerciseTiming::AfterScheduledCash
}
/// Eligibility depends on observed gross rights and known contractual inventory,
/// never on subsequently realized rates, defaults or cash amounts.
pub(crate) fn surviving_contractual_rights(
    raw: &owner::Request,
    c: &carry::ResultData,
    ordinal: i32,
    timing: ExerciseTiming,
) -> Result<bool, String> {
    let t = f64::from(ordinal - raw.source.current.asof_ordinal) / 365.;
    let at = point(c, raw.credit.paths[0].path_key, t, timing)?;
    if at.contractual_accrued_interest != 0.
        || at
            .canonical_legal_coupons
            .iter()
            .any(|p| p.legal_remaining != 0.)
    {
        return Ok(true);
    }
    let pending =
        |day: i32| day > ordinal || day == ordinal && timing == ExerciseTiming::BeforeScheduledCash;
    let future_contract = raw.source.current.periods.iter().any(|p| {
        pending(p.payment_ordinal)
            && (p.fee != 0.
                || match p.coupon {
                    term::indexed::Coupon::Fixed { annual_rate } => annual_rate != 0.,
                    term::indexed::Coupon::Indexed { .. } => true,
                })
    });
    let future_draw = raw
        .performing_draw_tranches
        .iter()
        .any(|d| pending(d.draw_ordinal));
    Ok(future_contract || future_draw)
}
pub(crate) fn coupon_entitlement_observed(
    period: &term::indexed::ContractPeriod,
    ordinal: i32,
    timing: ExerciseTiming,
) -> bool {
    period.start_ordinal <= ordinal
        || period.payment_ordinal < ordinal
        || period.payment_ordinal == ordinal && timing == ExerciseTiming::AfterScheduledCash
}
/// Canonical contract terms plus only rate observations selected by this exact
/// date and realized event history. Numerical keys retain collision-safe bytes.
fn observed_periods(s: &term::indexed::ContractPathRequest, ordinal: i32) -> Result<Value, String> {
    let cutoff = i64::from(ordinal - 719163)
        .checked_mul(86400)
        .ok_or("exercise timestamp overflow")?;
    let mut periods = serde_json::to_value(&s.periods).map_err(|e| e.to_string())?;
    for period in periods.as_array_mut().ok_or("exercise periods shape")? {
        if let Some(observations) = period["coupon"]
            .get_mut("observations")
            .and_then(Value::as_array_mut)
        {
            for o in observations {
                check()?;
                if o["selection_cutoff_unix_seconds"]
                    .as_i64()
                    .ok_or("exercise rate cutoff absent")?
                    > cutoff
                    || o["fixing_ordinal"]
                        .as_i64()
                        .ok_or("exercise fixing date absent")?
                        > i64::from(ordinal)
                {
                    o.as_object_mut()
                        .ok_or("exercise observation shape")?
                        .remove("rate");
                }
                // Path IDs and source labels are lineage, not observed states.
                o.as_object_mut()
                    .ok_or("exercise observation shape")?
                    .remove("source");
            }
        }
    }
    Ok(periods)
}
fn information(
    raw: &owner::Request,
    c: &carry::ResultData,
    ordinal: i32,
    timing: ExerciseTiming,
) -> Result<Value, String> {
    check()?;
    let s = &raw.source.current;
    let t = f64::from(ordinal - s.asof_ordinal) / 365.;
    let periods = observed_periods(s, ordinal)?;
    let mut draws = Vec::new();
    for draw in &raw.performing_draw_tranches {
        let d = &draw.source.current;
        draws.push(json!({"draw_ordinal":draw.draw_ordinal,"eligible_origins":draw.eligible_origins,
            "contract":d.contract,"currency":d.currency,"monetary_unit":d.monetary_unit,
            "opening_principal":d.opening_principal,"principal_plan":d.principal_plan,
            "periods":observed_periods(d,ordinal)?,"cost":draw.source.acquisition.cost,
            "original_annual_effective_rate":draw.source.acquisition.original_annual_effective_rate}));
    }
    let key = raw.credit.paths[0].path_key;
    let path = c
        .credit()
        .result
        .paths
        .iter()
        .find(|p| p.path_key == key)
        .ok_or("exercise credit path absent")?;
    let events: Vec<_> = path
        .events
        .iter()
        .filter(|e| observed(e.year, &e.kind, t, timing))
        .map(|e| {
            json!({
        "year":e.year,"kind":e.kind,"origin":e.origin,"destination":e.destination,
        "cash":e.cash,"principal":e.principal,"interest":e.interest})
        })
        .collect();
    let (state, principal, legal) = state(raw, c, t, timing)?;
    let gross: Vec<_> = point(c, key, t, timing)?
        .canonical_legal_coupons
        .iter()
        .filter(|coupon| {
            raw.source
                .current
                .periods
                .iter()
                .chain(
                    raw.performing_draw_tranches
                        .iter()
                        .flat_map(|draw| draw.source.current.periods.iter()),
                )
                .any(|period| {
                    period.period_id == coupon.coupon_id
                        && coupon_entitlement_observed(period, ordinal, timing)
                })
        })
        .collect();
    Ok(
        json!({"contract":s.contract,"currency":s.currency,"monetary_unit":s.monetary_unit,
        "opening_principal":s.opening_principal,"principal_plan":s.principal_plan,"periods":periods,
        "state":state,"principal":principal,"legal_interest":legal,"gross_legal_coupons":gross,
        "performing_draw_tranches":draws,"events":events}),
    )
}
fn df(curve: &SuppliedCurve, t: f64, oas: f64, bump: f64) -> Result<f64, String> {
    term::amount(curve.value_validated(t)?.discount_factor * (-(oas + bump) * t).exp())
}
fn live(state: CreditState) -> bool {
    matches!(
        state,
        CreditState::Performing | CreditState::Watch | CreditState::Delinquent
    )
}
pub(crate) fn select(
    date: &ExerciseDate,
    priority: SimultaneousPriority,
    continuation: f64,
    p: f64,
    a: f64,
) -> Result<(Decision, f64), String> {
    let call = date
        .call_clean_price_factor
        .map(|f| term::amount(p * f + a + date.call_fee))
        .transpose()?;
    let put = date
        .put_clean_price_factor
        .map(|f| term::amount(p * f + a + date.put_fee))
        .transpose()?;
    // Exactly the supplied-DAG game convention, with stable continuation ties.
    let mut choice = Decision::Continue;
    let mut value = continuation;
    match priority {
        SimultaneousPriority::Issuer => {
            if let Some(x) = put {
                if x > value {
                    value = x;
                    choice = Decision::Put;
                }
            }
            if let Some(x) = call {
                if x < value {
                    value = x;
                    choice = Decision::Call;
                }
            }
        }
        SimultaneousPriority::Holder => {
            if let Some(x) = call {
                if x < value {
                    value = x;
                    choice = Decision::Call;
                }
            }
            if let Some(x) = put {
                if x > value {
                    value = x;
                    choice = Decision::Put;
                }
            }
        }
    }
    Ok((choice, value))
}

/// Conditional expectations and stopping cash are constructed from native
/// source amounts and actual credit prefixes. No caller monetary node exists.
pub fn compile(
    policy: &Policy,
    views: &[PricingView<'_>],
    curve: &SuppliedCurve,
    oas: f64,
    bump: f64,
) -> Result<NativeExercisePolicy, String> {
    admitted_work(
        policy,
        &views.iter().map(|v| (v.raw, v.weight)).collect::<Vec<_>>(),
    )?;
    curve.validate()?;
    term::amount(oas)?;
    term::amount(bump)?;
    if !(-5. ..=5.).contains(&oas) || !(-0.1..=0.1).contains(&bump) {
        return Err("native exercise OAS/bump domain exceeded".into());
    }
    let first = &views[0].raw.source.current;
    if curve.currency != first.currency {
        return Err("native exercise discount/source currency differs".into());
    }
    for v in views {
        if v.carry.credit().result.measure != portfolio_model_core::credit::CreditMeasure::Pricing {
            return Err("native continuation requires explicit Pricing credit sources".into());
        }
    }
    let root = information(
        views[0].raw,
        views[0].carry,
        first.asof_ordinal,
        ExerciseTiming::AfterScheduledCash,
    )?;
    let root_key = encode(&root)?;
    for v in views {
        if encode(&information(
            v.raw,
            v.carry,
            first.asof_ordinal,
            ExerciseTiming::AfterScheduledCash,
        )?)? != root_key
        {
            return Err(
                "native Pricing paths disagree on initially observable contract/state information"
                    .into(),
            );
        }
    }
    let cash = views
        .iter()
        .map(|v| commercial_cash::economic_cash(v.carry))
        .collect::<Result<Vec<_>, _>>()?;
    let mut nodes = Vec::new();
    let mut keys = BTreeMap::new();
    let mut next_value = vec![0.; views.len()];
    let mut next_time: Option<(f64, ExerciseTiming)> = None;
    for date in policy.exercise_dates.iter().rev() {
        check()?;
        let t = f64::from(date.ordinal - first.asof_ordinal) / 365.;
        let discount = df(curve, t, oas, bump)?;
        let mut groups = BTreeMap::<Vec<u8>, (Value, Vec<usize>)>::new();
        for (i, v) in views.iter().enumerate() {
            let info = information(v.raw, v.carry, date.ordinal, date.timing)?;
            groups
                .entry(encode(&info)?)
                .or_insert_with(|| (info, Vec::new()))
                .1
                .push(i);
        }
        let mut now = vec![0.; views.len()];
        for (key, (info, members)) in groups {
            check()?;
            let probability: f64 = members.iter().map(|i| views[*i].weight).sum();
            if probability == 0. {
                continue;
            }
            let mut continuation = 0.;
            for &i in &members {
                let mut value = if let Some((end, _)) = next_time {
                    next_value[i] * df(curve, end, oas, bump)? / discount
                } else {
                    0.
                };
                for c in cash[i].iter().filter(|c| {
                    cash_after(c.year, t, date.timing)
                        && next_time.is_none_or(|(end, timing)| cash_before(c.year, end, timing))
                }) {
                    check()?;
                    value = term::amount(
                        value + c.total()? * df(curve, c.year, oas, bump)? / discount,
                    )?;
                }
                continuation = term::amount(continuation + views[i].weight / probability * value)?;
            }
            let (state, p, a) = state(
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
                return Err("rights-only native exercise after principal settlement requires an explicit legal termination owner".into());
            }
            let (choice, value) = if live(state) && (p > 0. || rights_only) {
                select(date, policy.simultaneous_priority, continuation, p, a)?
            } else {
                (Decision::Continue, continuation)
            };
            for i in members {
                now[i] = value;
            }
            let index = nodes.len();
            keys.insert((date.ordinal, key), index);
            nodes.push(Node {
                ordinal: date.ordinal,
                timing: date.timing,
                information: info,
                probability,
                continuation_value: continuation,
                value,
                decision: choice,
                live_principal: p,
                legal_interest: a,
            });
        }
        next_time = Some((t, date.timing));
        next_value = now;
    }
    let mut value = 0.;
    for (i, v) in views.iter().enumerate() {
        let mut path_value = if let Some((end, _)) = next_time {
            next_value[i] * df(curve, end, oas, bump)?
        } else {
            0.
        };
        for c in cash[i].iter().filter(|c| {
            c.year > 0. && next_time.is_none_or(|(end, timing)| cash_before(c.year, end, timing))
        }) {
            check()?;
            path_value = term::amount(path_value + c.total()? * df(curve, c.year, oas, bump)?)?;
        }
        value = term::amount(value + v.weight * path_value)?;
    }
    let result = NativeExercisePolicy {
        policy: policy.clone(),
        value,
        nodes,
        keys,
        asof: first.asof_ordinal,
        position_id: first.position_id.clone(),
        fixed_base_oas: oas,
        parallel_bump: bump,
        finite_distribution: true,
        continuous_model_convergence: false,
    };
    strict_json::serialized_size(&result, 32 << 20)?;
    check()?;
    Ok(result)
}
impl NativeExercisePolicy {
    /// Replay the first reached native decision on separately supplied Physical
    /// paths. An uncovered observed state rejects; there is no guessed action.
    pub fn stops_for(
        &self,
        raw: &owner::Request,
        c: &carry::ResultData,
    ) -> Result<Vec<owner::NativeStop>, String> {
        check()?;
        if raw.source.current.asof_ordinal != self.asof
            || raw.source.current.position_id != self.position_id
            || raw.credit.paths.len() != 1
        {
            return Err("native stopping/source identity mismatch".into());
        }
        let mut stops = Vec::new();
        for date in &self.policy.exercise_dates {
            let t = f64::from(date.ordinal - self.asof) / 365.;
            let (state, p, _a) = state(raw, c, t, date.timing)?;
            let rights_only = live(state)
                && p == 0.
                && surviving_contractual_rights(raw, c, date.ordinal, date.timing)?;
            if rights_only && self.policy.rights_only_termination.is_none() {
                return Err("rights-only native exercise after principal settlement requires an explicit legal termination owner".into());
            }
            if !live(state) || p == 0. && !rights_only {
                continue;
            }
            let key = encode(&information(raw, c, date.ordinal, date.timing)?)?;
            let n = &self.nodes[*self.keys.get(&(date.ordinal, key)).ok_or(
                "Physical observed state is absent from native Pricing continuation policy",
            )?];
            let (action, factor, fee) = match n.decision {
                Decision::Continue => continue,
                Decision::Call => (
                    owner::NativeStopAction::Call,
                    date.call_clean_price_factor
                        .ok_or("native call terms absent")?,
                    date.call_fee,
                ),
                Decision::Put => (
                    owner::NativeStopAction::Put,
                    date.put_clean_price_factor
                        .ok_or("native put terms absent")?,
                    date.put_fee,
                ),
            };
            stops.push(owner::NativeStop::from_compiled_policy_with_timing(
                raw.credit.paths[0].path_key,
                format!(
                    "native-exercise:{}:{}",
                    self.policy.identity.id, date.ordinal
                ),
                date.ordinal,
                action,
                factor,
                fee,
                self.policy.identity.clone(),
                match date.timing {
                    ExerciseTiming::BeforeScheduledCash => {
                        owner::NativeStopTiming::BeforeScheduledCash
                    }
                    ExerciseTiming::AfterScheduledCash => {
                        owner::NativeStopTiming::AfterScheduledCash
                    }
                },
                if rights_only {
                    self.policy
                        .rights_only_termination
                        .as_ref()
                        .map(|r| r.review.clone())
                } else {
                    None
                },
            )?);
            break;
        }
        check()?;
        Ok(stops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(key: u64) -> owner::Request {
        let v: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-credit-ledger-1.json"
        ))
        .unwrap();
        let mut r: owner::Request = serde_json::from_value(v["commercial"][0].clone()).unwrap();
        r.source.original.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
        r.source.current.measure = portfolio_model_core::cashflow::CashflowMeasure::Pricing;
        r.credit.paths[0].path_key = key;
        r.credit.paths[0].path_id = format!("path-{key}");
        r
    }
    fn curve() -> SuppliedCurve {
        serde_json::from_value(
            json!({"curve_id":"flat","source_id":"reviewed","source_revision":"1",
            "currency":"USD","role":{"kind":"collateral","collateral_index":"cash"},
            "times_years":[0.,1.],"discount_factors":[1.,1.],"right_tail":"reject"}),
        )
        .unwrap()
    }
    fn policy(r: &owner::Request, call: Option<f64>, put: Option<f64>) -> Policy {
        Policy {
            schema: SCHEMA.into(),
            identity: term::Identity {
                id: "exercise".into(),
                revision: "1".into(),
            },
            information_policy: "native_full_observed_prefix-1".into(),
            simultaneous_priority: SimultaneousPriority::Issuer,
            rights_only_termination: None,
            exercise_dates: vec![ExerciseDate {
                ordinal: r.source.current.asof_ordinal + 15,
                timing: ExerciseTiming::AfterScheduledCash,
                call_clean_price_factor: call,
                put_clean_price_factor: put,
                call_fee: 0.,
                put_fee: 0.,
            }],
        }
    }
    fn economic(r: &owner::Request) -> owner::EconomicResult {
        owner::evaluate_economic_with_measure(
            r,
            portfolio_model_core::credit::CreditMeasure::Pricing,
        )
        .unwrap()
    }
    fn price(
        policy: &Policy,
        raws: &[owner::Request],
        weights: &[f64],
        oas: f64,
    ) -> NativeExercisePolicy {
        let sources: Vec<_> = raws.iter().map(economic).collect();
        let views: Vec<_> = raws
            .iter()
            .zip(&sources)
            .zip(weights)
            .map(|((r, s), w)| PricingView {
                raw: r,
                carry: s.carry(),
                weight: *w,
            })
            .collect();
        compile(policy, &views, &curve(), oas, 0.).unwrap()
    }
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    #[test]
    fn option_free_full_cash_matches_independent_hand_value_and_native_replay() {
        let r = raw(17);
        let mut p = policy(&r, Some(10.), None);
        p.exercise_dates.clear();
        let result = price(&p, std::slice::from_ref(&r), &[1.], 0.);
        close(result.value, 100. + 100. * 0.1 * 30. / 365.);
        let q = economic(&r);
        assert!(result.stops_for(&r, q.carry()).unwrap().is_empty());
    }
    #[test]
    fn actual_call_and_put_bounds_have_exact_native_cash_replay() {
        let r = raw(17);
        let legal = 100. * 0.1 * 15. / 365.;
        for (call, put, expected, action) in [
            (Some(0.8), None, 80. + legal, Decision::Call),
            (None, Some(1.1), 110. + legal, Decision::Put),
        ] {
            let p = policy(&r, call, put);
            let result = price(&p, std::slice::from_ref(&r), &[1.], 0.);
            close(result.value, expected);
            assert_eq!(result.nodes[0].decision, action);
            let no_option = economic(&r);
            let stops = result.stops_for(&r, no_option.carry()).unwrap();
            let stopped = owner::evaluate_economic_with_stops(
                &r,
                portfolio_model_core::credit::CreditMeasure::Pricing,
                &stops,
            )
            .unwrap();
            let cash = commercial_cash::economic_cash(stopped.carry()).unwrap();
            close(cash.iter().map(|c| c.total().unwrap()).sum(), result.value);
            assert_eq!(cash.len(), 1);
            assert_eq!(cash[0].year, 15. / 365.);
        }
    }
    #[test]
    fn continuation_conditions_on_shared_observed_prefix_not_future_default() {
        use portfolio_model_core::credit_path::{DrawStream, SuppliedPathDraws, UniformPair};
        let a = raw(17);
        let mut b = raw(18);
        b.credit.intervals[0].hazards_per_year[0][3] = 1.;
        b.credit.intervals[0].loss_given_default = [0.4; 3];
        b.credit.draw_stream = DrawStream::Supplied {
            stream_id: "default".into(),
            stream_revision: "1".into(),
            paths: vec![SuppliedPathDraws {
                path_key: 18,
                pairs: vec![UniformPair {
                    waiting_uniform: (-20_f64 / 365.).exp(),
                    destination_uniform: 0.5,
                }],
            }],
        };
        let p = policy(&a, Some(1.), None);
        let result = price(&p, &[a, b], &[0.5, 0.5], 0.);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].decision, Decision::Continue);
        close(result.nodes[0].probability, 1.);
        assert!(result.value < 90.);
    }
    #[test]
    fn repeated_equal_paths_preserve_finite_distribution_value() {
        let a = raw(17);
        let b = raw(18);
        let p = policy(&a, Some(0.8), None);
        let one = price(&p, std::slice::from_ref(&a), &[1.], 0.);
        let two = price(&p, &[a, b], &[0.25, 0.75], 0.);
        close(one.value, two.value);
        assert_eq!(two.nodes.len(), 1);
    }
    #[test]
    fn uncovered_physical_state_and_beyond_tail_reject() {
        let r = raw(17);
        let p = policy(&r, Some(0.8), None);
        let result = price(&p, std::slice::from_ref(&r), &[1.], 0.);
        let mut changed = r.clone();
        changed.credit.paths[0].initial_live_state = CreditState::Watch;
        let changed_source = economic(&changed);
        assert!(result
            .stops_for(&changed, changed_source.carry())
            .unwrap_err()
            .contains("absent"));
        let mut beyond = p;
        beyond.exercise_dates[0].ordinal = r.source.current.tail_end_ordinal + 1;
        assert!(admitted_work(&beyond, &[(&r, 1.)]).is_err());
    }
    #[test]
    fn raw_prepared_money_or_future_decision_fields_are_not_accepted() {
        let r = raw(17);
        let mut v = serde_json::to_value(policy(&r, Some(1.), None)).unwrap();
        v["continuation_value"] = json!(1.);
        assert!(serde_json::from_value::<Policy>(v).is_err());
    }
    #[test]
    fn before_cash_exercise_replaces_same_day_cash_once() {
        let r = raw(17);
        let mut p = policy(&r, Some(0.8), None);
        p.exercise_dates[0].ordinal = r.source.current.asof_ordinal + 35;
        p.exercise_dates[0].timing = ExerciseTiming::BeforeScheduledCash;
        let result = price(&p, std::slice::from_ref(&r), &[1.], 0.);
        let expected = 80. + 100. * 0.1 * 30. / 365.;
        close(result.value, expected);
        assert_eq!(result.nodes[0].decision, Decision::Call);
        close(result.nodes[0].live_principal, 100.);
        let no_option = economic(&r);
        let stops = result.stops_for(&r, no_option.carry()).unwrap();
        let stopped = owner::evaluate_economic_with_stops(
            &r,
            portfolio_model_core::credit::CreditMeasure::Pricing,
            &stops,
        )
        .unwrap();
        let cash = commercial_cash::economic_cash(stopped.carry()).unwrap();
        assert_eq!(cash.len(), 1);
        close(cash[0].total().unwrap(), expected);
        close(cash[0].year, 35. / 365.);
        p.exercise_dates[0].timing = ExerciseTiming::AfterScheduledCash;
        let after = price(&p, std::slice::from_ref(&r), &[1.], 0.);
        close(after.value, 100. + 100. * 0.1 * 30. / 365.);
        assert_eq!(after.nodes[0].decision, Decision::Continue);
    }
    #[test]
    fn rights_only_put_requires_review_and_settles_without_principal() {
        use crate::products::term::contracts::indexed::PrincipalPlan;
        let mut r = raw(17);
        for source in [&mut r.source.original, &mut r.source.current] {
            let mut early = source.periods[0].clone();
            early.period_id = "late-first-coupon".into();
            early.end_ordinal = source.asof_ordinal + 15;
            let mut last = source.periods[0].clone();
            last.period_id = "early-principal-coupon".into();
            last.start_ordinal = source.asof_ordinal + 15;
            last.payment_ordinal = source.asof_ordinal + 30;
            source.periods = vec![early, last];
            source.principal_plan = PrincipalPlan::Supplied {
                principal_payments: vec![0., 100.],
            };
        }
        let mut p = policy(&r, None, Some(1.));
        p.exercise_dates[0].ordinal = r.source.current.asof_ordinal + 31;
        p.exercise_dates[0].put_fee = 1.;
        let no_option = economic(&r);
        let views = [PricingView {
            raw: &r,
            carry: no_option.carry(),
            weight: 1.,
        }];
        assert!(compile(&p, &views, &curve(), 0., 0.)
            .err()
            .unwrap()
            .contains("rights-only"));
        p.rights_only_termination = Some(RightsOnlyTermination {
            review: term::Identity {
                id: "legal-termination-review".into(),
                revision: "1".into(),
            },
            disposition: "settle_earned_cancel_future-1".into(),
        });
        let result = compile(&p, &views, &curve(), 0., 0.).unwrap();
        assert_eq!(result.nodes[0].decision, Decision::Put);
        assert_eq!(result.nodes[0].live_principal, 0.);
        close(result.nodes[0].legal_interest, 100. * 0.1 * 15. / 365.);
        close(result.value, 101. + 100. * 0.1 * 30. / 365.);
        let stops = result.stops_for(&r, no_option.carry()).unwrap();
        let stopped = owner::evaluate_economic_with_stops(
            &r,
            portfolio_model_core::credit::CreditMeasure::Pricing,
            &stops,
        )
        .unwrap();
        let cash = commercial_cash::economic_cash(stopped.carry()).unwrap();
        let exit = cash.iter().find(|c| c.year == 31. / 365.).unwrap();
        assert_eq!(exit.principal, 0.);
        close(exit.interest, 100. * 0.1 * 15. / 365.);
        close(cash.iter().map(|c| c.total().unwrap()).sum(), result.value);
        assert!(cash.iter().all(|c| c.year <= 31. / 365.));
        p.rights_only_termination.as_mut().unwrap().disposition = "waive".into();
        assert!(admitted_work(&p, &[(&r, 1.)]).is_err());
    }
}
