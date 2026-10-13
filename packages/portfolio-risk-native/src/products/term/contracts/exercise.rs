//! Backward conditional valuation over a supplied bounded pricing-state DAG.
use super::*;
use portfolio_model_core::cashflow::*;
use portfolio_model_core::floating_rate::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_TREE_NODES: usize = 100_000;
pub const MAX_TREE_EDGES: usize = 400_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseTiming {
    BeforeScheduledCash,
    AfterScheduledCash,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SimultaneousPriority {
    Issuer,
    Holder,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Continue,
    Call,
    Put,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExerciseDate {
    pub ordinal: i32,
    pub timing: ExerciseTiming,
    pub call_clean_price_factor: Option<f64>,
    pub put_clean_price_factor: Option<f64>,
    /// Explicit signed fee, separate from clean redemption premium.
    pub call_fee: f64,
    pub put_fee: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    pub child: usize,
    pub conditional_probability: f64,
    /// Market discount only. Native arithmetic applies fixed base OAS once.
    pub market_discount_factor: f64,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PricingNode {
    pub node_id: String,
    pub ordinal: i32,
    pub information_set: Identity,
    pub principal_before_cash: f64,
    pub scheduled_cash: CashflowComponents,
    pub scheduled_accrual_period: Option<AccrualPeriod>,
    /// Rights settled upon exercise, after/before scheduled cash as declared.
    /// Caller must remove a same-day paid coupon from after-cash rights.
    pub exercise_accrued_interest: f64,
    pub accrued_right_source: Identity,
    pub branches: Vec<Branch>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExerciseRequest {
    pub schema: String,
    pub model_version: String,
    pub contract: Identity,
    pub policy: Identity,
    pub tree: Identity,
    pub discount_source: Identity,
    pub oas_anchor: Identity,
    pub common_random_numbers: Identity,
    pub measure: CashflowMeasure,
    pub discount_basis: String,
    pub position_id: String,
    pub scenario_id: String,
    pub currency: String,
    pub monetary_unit: String,
    pub direction: Direction,
    pub asof_ordinal: i32,
    pub reporting_end_ordinal: i32,
    pub tail_end_ordinal: i32,
    pub fixed_base_oas: f64,
    pub oas_day_count: ActualDayCount,
    pub simultaneous_priority: SimultaneousPriority,
    pub exercise_dates: Vec<ExerciseDate>,
    /// Chronological, topological order; node zero is the sole initial state.
    pub nodes: Vec<PricingNode>,
}
#[derive(Debug, Clone, Serialize)]
pub struct NodeValue {
    pub node_id: String,
    pub continuation_value: f64,
    pub value: f64,
    pub decision: Decision,
    pub live_pricing_probability: f64,
    pub canceled_future_principal: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExerciseResult {
    pub schema: String,
    pub model_version: String,
    pub tree: Identity,
    pub policy: Identity,
    pub oas_anchor: Identity,
    pub common_random_numbers: Identity,
    pub fixed_base_oas: f64,
    pub value: f64,
    pub nodes: Vec<NodeValue>,
    /// Undiscounted pricing-measure expected flows under the stopping policy.
    /// These cannot be posted as Physical cash settlements.
    pub schedule: DatedSchedule,
    pub expected_principal_after_reporting: f64,
    pub disclosure: String,
}
fn redemption(
    n: &PricingNode,
    date: &ExerciseDate,
    choice: Decision,
) -> Result<CashflowComponents, String> {
    let balance = if date.timing == ExerciseTiming::AfterScheduledCash {
        n.principal_before_cash - n.scheduled_cash.principal
    } else {
        n.principal_before_cash
    };
    let (factor, fee) = match choice {
        Decision::Call => (
            date.call_clean_price_factor.ok_or("call rule missing")?,
            date.call_fee,
        ),
        Decision::Put => (
            date.put_clean_price_factor.ok_or("put rule missing")?,
            date.put_fee,
        ),
        Decision::Continue => return Err("continuation is not a redemption".into()),
    };
    let c = CashflowComponents {
        principal: balance,
        cash_interest: n.exercise_accrued_interest,
        accrued_interest: n.exercise_accrued_interest,
        book_amortization: 0.,
        fees: amount(balance * (factor - 1.) + fee)?,
    };
    c.cash_total()?;
    Ok(c)
}
fn rule_at(request: &ExerciseRequest, ordinal: i32) -> Option<&ExerciseDate> {
    request
        .exercise_dates
        .binary_search_by_key(&ordinal, |e| e.ordinal)
        .ok()
        .map(|i| &request.exercise_dates[i])
}
pub fn value_exercise(request: &ExerciseRequest) -> Result<ExerciseResult, String> {
    value_exercise_with_check(request, portfolio_compute_control::checkpoint)
}

/// Checkpoints cover validation, backward policy and forward cash propagation.
/// Check failures return no partial financial result and preserve caller reasons.
pub fn value_exercise_with_check(
    request: &ExerciseRequest,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ExerciseResult, String> {
    check()?;
    header(
        &request.schema,
        &request.model_version,
        &request.currency,
        &request.monetary_unit,
    )?;
    domain(request.asof_ordinal, request.tail_end_ordinal)?;
    domain(request.asof_ordinal, request.reporting_end_ordinal)?;
    if request.measure != CashflowMeasure::Pricing
        || request.discount_basis != "market_without_oas"
        || !request.fixed_base_oas.is_finite()
        || request.nodes.is_empty()
        || request.nodes.len() > MAX_TREE_NODES
        || request.exercise_dates.len() > MAX_PERIODS
        || request.reporting_end_ordinal > request.tail_end_ordinal
    {
        return Err(
            "exercise requires bounded explicit Pricing tree, market discount and fixed OAS".into(),
        );
    }
    let mut bytes = 0;
    for id in [
        &request.contract,
        &request.policy,
        &request.tree,
        &request.discount_source,
        &request.oas_anchor,
        &request.common_random_numbers,
    ] {
        identity(id, &mut bytes)?;
    }
    text(&request.position_id, &mut bytes)?;
    text(&request.scenario_id, &mut bytes)?;
    let mut prior = None;
    for date in &request.exercise_dates {
        check()?;
        domain(request.asof_ordinal, date.ordinal)?;
        if date.ordinal > request.tail_end_ordinal
            || prior.is_some_and(|p| p >= date.ordinal)
            || (date.call_clean_price_factor.is_none() && date.put_clean_price_factor.is_none())
        {
            return Err("exercise dates must be unique permitted actual dates in full tail".into());
        }
        for f in date
            .call_clean_price_factor
            .into_iter()
            .chain(date.put_clean_price_factor)
        {
            nonnegative(f)?;
        }
        amount(date.call_fee)?;
        amount(date.put_fee)?;
        if (date.call_clean_price_factor.is_none() && date.call_fee != 0.)
            || (date.put_clean_price_factor.is_none() && date.put_fee != 0.)
        {
            return Err("fee without permitted exercise right".into());
        }
        prior = Some(date.ordinal);
    }
    let mut ids = HashSet::new();
    let mut reachable = vec![false; request.nodes.len()];
    reachable[0] = true;
    let mut edge_count: usize = 0;
    let mut dates_used = HashSet::new();
    let mut last = request.asof_ordinal;
    if request.nodes[0].ordinal != request.asof_ordinal {
        return Err("initial pricing node must be at asof".into());
    }
    for (i, n) in request.nodes.iter().enumerate() {
        check()?;
        text(&n.node_id, &mut bytes)?;
        identity(&n.information_set, &mut bytes)?;
        identity(&n.accrued_right_source, &mut bytes)?;
        domain(request.asof_ordinal, n.ordinal)?;
        nonnegative(n.principal_before_cash)?;
        n.scheduled_cash.cash_total()?;
        nonnegative(n.scheduled_cash.principal)?;
        amount(n.exercise_accrued_interest)?;
        if !ids.insert(&n.node_id)
            || !reachable[i]
            || n.ordinal < last
            || n.ordinal > request.tail_end_ordinal
            || n.scheduled_cash.principal > n.principal_before_cash
        {
            return Err(
                "tree node order/reachability/actual-date/principal admission failed".into(),
            );
        }
        last = n.ordinal;
        if let Some(p) = n.scheduled_accrual_period {
            domain(p.start_ordinal, p.end_ordinal)?;
            if p.start_ordinal == p.end_ordinal || p.end_ordinal > request.tail_end_ordinal {
                return Err("invalid tree accrual rights period".into());
            }
        }
        if let Some(rule) = rule_at(request, n.ordinal) {
            dates_used.insert(n.ordinal);
            if rule.call_clean_price_factor.is_some() {
                redemption(n, rule, Decision::Call)?;
            }
            if rule.put_clean_price_factor.is_some() {
                redemption(n, rule, Decision::Put)?;
            }
        } else if n.exercise_accrued_interest != 0. {
            return Err("exercise accrued right on a date without an exercise rule".into());
        }
        edge_count = edge_count
            .checked_add(n.branches.len())
            .ok_or("tree work overflow")?;
        if edge_count > MAX_TREE_EDGES {
            return Err("tree edge work exceeds 400000".into());
        }
        let remaining = n.principal_before_cash - n.scheduled_cash.principal;
        if n.branches.is_empty() {
            if !near(remaining, 0.) {
                return Err("terminal tree leaves unpaid contractual principal; full maturity tail required".into());
            }
        } else {
            let mut probability = 0.;
            let mut children = HashSet::new();
            for edge in &n.branches {
                check()?;
                if edge.child <= i
                    || edge.child >= request.nodes.len()
                    || !children.insert(edge.child)
                    || !edge.conditional_probability.is_finite()
                    || !(0.0..=1.0).contains(&edge.conditional_probability)
                    || edge.conditional_probability == 0.
                    || !edge.market_discount_factor.is_finite()
                    || edge.market_discount_factor <= 0.
                {
                    return Err("invalid conditional branch/probability/market discount".into());
                }
                let child = &request.nodes[edge.child];
                if child.ordinal <= n.ordinal || !near(child.principal_before_cash, remaining) {
                    return Err(
                        "conditional branch does not conserve remaining contractual principal"
                            .into(),
                    );
                }
                probability += edge.conditional_probability;
                reachable[edge.child] = true;
                let exponent = -request.fixed_base_oas
                    * actual_year_fraction(n.ordinal, child.ordinal, request.oas_day_count)?;
                let df = edge.market_discount_factor * exponent.exp();
                if !df.is_finite() || df <= 0. {
                    return Err("fixed-OAS discount factor outside numerical domain".into());
                }
            }
            if (probability - 1.).abs() > 64. * f64::EPSILON {
                return Err("conditional branch probabilities must sum to one".into());
            }
        }
    }
    if dates_used.len() != request.exercise_dates.len() {
        return Err("permitted exercise date absent from tree; no monthly mapping allowed".into());
    }
    let mut values = vec![0.; request.nodes.len()];
    let mut continuation = vec![0.; request.nodes.len()];
    let mut choices = vec![Decision::Continue; request.nodes.len()];
    for (i, n) in request.nodes.iter().enumerate().rev() {
        check()?;
        let mut future = 0.;
        for b in &n.branches {
            check()?;
            let tau = actual_year_fraction(
                n.ordinal,
                request.nodes[b.child].ordinal,
                request.oas_day_count,
            )?;
            future = amount(
                future
                    + b.conditional_probability
                        * b.market_discount_factor
                        * (-request.fixed_base_oas * tau).exp()
                        * values[b.child],
            )?;
        }
        let cash = n.scheduled_cash.cash_total()?;
        continuation[i] = amount(cash + future)?;
        let mut selected = continuation[i];
        let mut decision = Decision::Continue;
        if let Some(rule) = rule_at(request, n.ordinal) {
            let common = if rule.timing == ExerciseTiming::AfterScheduledCash {
                cash
            } else {
                0.
            };
            let call = rule
                .call_clean_price_factor
                .map(|_| {
                    redemption(n, rule, Decision::Call)
                        .and_then(|c| amount(common + c.cash_total()?))
                })
                .transpose()?;
            let put = rule
                .put_clean_price_factor
                .map(|_| {
                    redemption(n, rule, Decision::Put)
                        .and_then(|c| amount(common + c.cash_total()?))
                })
                .transpose()?;
            // Last operation has explicit priority on conflicting same-date rights.
            let order = if request.simultaneous_priority == SimultaneousPriority::Issuer {
                [Decision::Put, Decision::Call]
            } else {
                [Decision::Call, Decision::Put]
            };
            for right in order {
                match (right, call, put) {
                    (Decision::Call, Some(v), _) if v < selected => {
                        selected = v;
                        decision = Decision::Call;
                    }
                    (Decision::Put, _, Some(v)) if v > selected => {
                        selected = v;
                        decision = Decision::Put;
                    }
                    _ => {} // exact ties retain continuation/prior decision
                }
            }
        }
        values[i] = amount(selected)?;
        choices[i] = decision;
    }
    let sign = if request.direction == Direction::Receipt {
        1.
    } else {
        -1.
    };
    let mut live = vec![0.; request.nodes.len()];
    live[0] = 1.;
    let mut events = Vec::new();
    let mut expected_principal_after_reporting = request.nodes[0].principal_before_cash;
    for (i, n) in request.nodes.iter().enumerate() {
        check()?;
        if live[i] == 0. {
            continue;
        }
        let rule = rule_at(request, n.ordinal);
        let exercising = choices[i] != Decision::Continue;
        let pay_scheduled =
            !exercising || rule.is_some_and(|r| r.timing == ExerciseTiming::AfterScheduledCash);
        let mut components = if pay_scheduled {
            n.scheduled_cash
        } else {
            CashflowComponents {
                principal: 0.,
                cash_interest: 0.,
                accrued_interest: 0.,
                book_amortization: 0.,
                fees: 0.,
            }
        };
        if exercising {
            let r = redemption(n, rule.ok_or("missing selected exercise rule")?, choices[i])?;
            components.principal += r.principal;
            components.cash_interest += r.cash_interest;
            components.accrued_interest += r.accrued_interest;
            components.fees += r.fees;
        } else {
            for b in &n.branches {
                check()?;
                live[b.child] += live[i] * b.conditional_probability;
            }
        }
        if n.ordinal <= request.reporting_end_ordinal {
            expected_principal_after_reporting -= live[i] * components.principal;
        }
        if components.cash_total()? != 0.
            || components.accrued_interest != 0.
            || components.book_amortization != 0.
        {
            let scale = sign * live[i];
            components.principal *= scale;
            components.cash_interest *= scale;
            components.accrued_interest *= scale;
            components.book_amortization *= scale;
            components.fees *= scale;
            events.push(DatedCashflowEvent {
                position_id: request.position_id.clone(),
                event_id: n.node_id.clone(),
                scenario: ScenarioSelector::Named {
                    scenario_id: request.scenario_id.clone(),
                },
                payment_ordinal: n.ordinal,
                day_offset: (n.ordinal - request.asof_ordinal) as u32,
                accrual_period: if exercising {
                    None
                } else {
                    n.scheduled_accrual_period
                },
                currency: request.currency.clone(),
                components,
                measure: CashflowMeasure::Pricing,
                provenance: EventProvenance::Modeled {
                    model_id: request.policy.id.clone(),
                    model_version: MODEL_VERSION.into(),
                    input_revision: request.policy.revision.clone(),
                },
                adjustment: CashflowAdjustment::None {},
            });
        }
    }
    let mut total = 0.;
    for event in &events {
        check()?;
        total += sign * event.components.principal;
    }
    if !near(total, request.nodes[0].principal_before_cash) {
        return Err("exercise policy failed expected principal conservation".into());
    }
    let schedule = DatedSchedule {
        schema: DATED_CASHFLOW_SCHEMA.into(),
        as_of_ordinal: request.asof_ordinal,
        horizon_days: (request.tail_end_ordinal - request.asof_ordinal) as u32,
        scenarios: vec![request.scenario_id.clone()],
        events,
    };
    schedule.validate()?;
    check()?;
    let nodes = request
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            check()?;
            Ok(NodeValue {
                node_id: n.node_id.clone(),
                continuation_value: continuation[i],
                value: values[i],
                decision: choices[i],
                live_pricing_probability: live[i],
                canceled_future_principal: if choices[i] == Decision::Continue {
                    0.
                } else {
                    n.principal_before_cash - n.scheduled_cash.principal
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    check()?;
    Ok(ExerciseResult{schema:SCHEMA.into(),model_version:MODEL_VERSION.into(),tree:request.tree.clone(),policy:request.policy.clone(),oas_anchor:request.oas_anchor.clone(),common_random_numbers:request.common_random_numbers.clone(),fixed_base_oas:request.fixed_base_oas,value:sign*values[0],nodes,schedule,expected_principal_after_reporting:expected_principal_after_reporting.max(0.),disclosure:"Native Bellman continuation-value exercise on supplied risk-neutral conditional states. Exact ties retain prior decision; simultaneous rights use explicit priority. Market discounts exclude OAS and fixed base OAS is applied once. No calibrated tree generation, notice delay, settlement lag, credit or perfect-foresight path exercise is inferred. Expected Pricing flows are not Physical ledger settlements. Full unexercised maturity tail is required and retained.".into()})
}
