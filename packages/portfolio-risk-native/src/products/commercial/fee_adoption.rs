//! Separate acquired service rights in the existing commercial graph and units.
//! Review identities select supplied policy; they do not assert approval.
use super::{admit, bind, bind_economic, BoundPath, Policy};
use crate::advanced_graph::Role;
use crate::products::commercial::{cash::EconomicCash, credit_compiler as loan, graph};
use portfolio_model_core::{credit_accounting::Identity, nonintegral_fee as fee};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "commercial-fee-adoption-1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    pub fee_id: String,
    /// Signed cost paid for a separate asset, or credit received for a liability.
    pub allocated_cost: f64,
    pub review: Identity,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub fee_id: String,
    pub ordinal: i32,
    pub sequence: u32,
    /// Signed release of acquired cost, separate from contractual service income.
    pub amount: f64,
    pub review: Identity,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub path_id: String,
    pub policy: Policy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acquisition_releases: Vec<Release>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Histories {
    pub scenario_id: String,
    pub physical: Vec<History>,
    pub pricing: Vec<History>,
    pub pricing_down: Vec<History>,
    pub pricing_up: Vec<History>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub schema: String,
    pub identity: Identity,
    /// Separate from the original loan OAS, including on paired bumps.
    pub discount_spread: f64,
    pub original_signed_fee_quote: f64,
    pub total_purchase_cost: f64,
    pub allocations: Vec<Allocation>,
    pub original_pricing: Vec<History>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub original_physical: Vec<History>,
    pub histories: Vec<Histories>,
}
#[derive(Serialize)]
pub struct Projection {
    pub raw_fee_history: History,
    pub bound_path: BoundPath,
    pub cash: Vec<EconomicCash>,
    pub discount_spread: f64,
    pub allocations: Vec<Allocation>,
    pub loan_original_cost_yield_unchanged: bool,
}
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn money(x: f64) -> Result<f64, String> {
    graph::money(x)
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
fn identity(i: &Identity) -> Result<(), String> {
    crate::products::term::projection::label(&i.id)?;
    crate::products::term::projection::label(&i.revision)
}
impl Selection {
    pub fn history(
        &self,
        scene: &str,
        role: Role,
        path: &str,
        original: bool,
    ) -> Result<&History, String> {
        let hs = if original {
            if role == Role::Physical {
                &self.original_physical
            } else {
                &self.original_pricing
            }
        } else {
            let h = self
                .histories
                .iter()
                .find(|h| h.scenario_id == scene)
                .ok_or("fee scenario coverage absent")?;
            match role {
                Role::Physical => &h.physical,
                Role::Pricing => &h.pricing,
                Role::PricingDown => &h.pricing_down,
                Role::PricingUp => &h.pricing_up,
            }
        };
        hs.iter()
            .find(|h| h.path_id == path)
            .ok_or("fee path coverage absent".into())
    }
}
fn validate_releases(
    s: &Selection,
    h: &History,
    raw: &loan::Request,
    role: Role,
) -> Result<(), String> {
    if role != Role::Physical {
        if !h.acquisition_releases.is_empty() {
            return Err("Pricing fee paths cannot supply physical cost releases".into());
        }
        return Ok(());
    }
    if h.acquisition_releases.len() > 512 {
        return Err("fee acquisition release capacity exceeded".into());
    }
    let opening = super::transferred_opening(&h.policy, raw)?;
    let mut remaining: BTreeMap<_, _> = s
        .allocations
        .iter()
        .map(|a| {
            Ok((
                a.fee_id.as_str(),
                a.allocated_cost
                    - opening
                        .as_ref()
                        .map_or(Ok(0.), |t| t.net_stock(&a.fee_id))?,
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut previous = None;
    for r in &h.acquisition_releases {
        check()?;
        identity(&r.review)?;
        money(r.amount)?;
        let clock = (r.ordinal, r.sequence);
        if previous.is_some_and(|p| p >= clock)
            || r.ordinal < raw.source.current.asof_ordinal
            || r.ordinal > h.policy.complete_end_ordinal
        {
            return Err(
                "fee acquired cost releases require complete strictly ordered actual dates".into(),
            );
        }
        previous = Some(clock);
        let rest = remaining
            .get_mut(r.fee_id.as_str())
            .ok_or("unknown fee acquisition release")?;
        if r.amount == 0. || r.amount.signum() != rest.signum() || r.amount.abs() > rest.abs() {
            return Err(
                "fee acquired cost release exceeds separately allocated signed basis".into(),
            );
        }
        *rest = money(*rest - r.amount)?;
    }
    if remaining.values().any(|v| !near(*v, 0.)) {
        return Err(
            "fee acquisition requires explicit complete cost releases without terminal plugs"
                .into(),
        );
    }
    Ok(())
}
/// Reserve all explicit fee paths, fee journal owners, cash and basis receipts
/// before loan generation. Exact coverage forbids unpriced or unowned fee paths.
pub fn audit(
    s: &Selection,
    r: &graph::Request,
    p: &graph::Position,
) -> Result<(usize, usize), String> {
    check()?;
    identity(&s.identity)?;
    if s.schema != SCHEMA
        || !s.discount_spread.is_finite()
        || !(-5. ..=5.).contains(&s.discount_spread)
        || s.allocations.is_empty()
        || s.allocations.len() > 64
        || s.histories.len() != p.histories.len()
        || p.generated_policy.is_some()
    {
        return Err("explicit complete fee adoption required; generated sources need joint loan/fee continuation ownership".into());
    }
    money(s.original_signed_fee_quote)?;
    money(s.total_purchase_cost)?;
    let mut fee_ids = BTreeSet::new();
    let mut cost = 0.;
    for a in &s.allocations {
        identity(&a.review)?;
        crate::products::term::projection::label(&a.fee_id)?;
        money(a.allocated_cost)?;
        if !fee_ids.insert(a.fee_id.as_str()) {
            return Err("duplicate fee purchase allocation".into());
        }
        cost = money(cost + a.allocated_cost)?;
    }
    if !near(cost, s.original_signed_fee_quote)
        || !near(s.total_purchase_cost, p.original_signed_cash_quote + cost)
        || s.total_purchase_cost <= 0.
    {
        return Err(
            "separate fee allocations/quote and total acquisition cash do not reconcile".into(),
        );
    }
    let mut work = 0usize;
    let mut bytes = 0usize;
    let mut scenes = BTreeSet::new();
    let mut contract = None;
    for h in &s.histories {
        if !scenes.insert(&h.scenario_id)
            || !p.histories.iter().any(|x| x.scenario_id == h.scenario_id)
        {
            return Err("fee history scenario coverage differs".into());
        }
    }
    for (scene, role, hs, original) in std::iter::once((
        r.original_scenario.scenario_id.as_str(),
        Role::Pricing,
        p.original_pricing.as_slice(),
        true,
    ))
    .chain(p.histories.iter().flat_map(|h| {
        [
            (Role::Physical, h.physical.as_slice()),
            (Role::Pricing, h.pricing.as_slice()),
            (Role::PricingDown, h.pricing_down.as_slice()),
            (Role::PricingUp, h.pricing_up.as_slice()),
        ]
        .into_iter()
        .map(move |(role, hs)| (h.scenario_id.as_str(), role, hs, false))
    }))
    .chain(
        (!p.original_physical.is_empty() || !s.original_physical.is_empty()).then_some((
            r.original_scenario.scenario_id.as_str(),
            Role::Physical,
            p.original_physical.as_slice(),
            true,
        )),
    ) {
        let selected = if original {
            if role == Role::Physical {
                &s.original_physical
            } else {
                &s.original_pricing
            }
        } else {
            let h = s
                .histories
                .iter()
                .find(|h| h.scenario_id == scene)
                .ok_or("fee scenario coverage absent")?;
            match role {
                Role::Physical => &h.physical,
                Role::Pricing => &h.pricing,
                Role::PricingDown => &h.pricing_down,
                Role::PricingUp => &h.pricing_up,
            }
        };
        if selected.len() != hs.len()
            || selected
                .iter()
                .map(|h| &h.path_id)
                .collect::<BTreeSet<_>>()
                .len()
                != selected.len()
        {
            return Err("fee path coverage/order differs".into());
        }
        for loan_history in hs {
            let h = s.history(scene, role, &loan_history.path_id, original)?;
            if p.exercise.is_some()
                && h.policy.coincident_order != "native_exit_follows_native_exercise_timing-1"
            {
                return Err(
                    "joint loan/fee continuation requires explicit native before/after fee timing"
                        .into(),
                );
            }
            let ids: BTreeSet<_> = h.policy.fees.iter().map(|f| f.fee_id.as_str()).collect();
            if ids != fee_ids || ids.len() != h.policy.fees.len() {
                return Err("fee allocations and complete source rights differ".into());
            }
            let current_contract =
                portfolio_model_core::strict_json::encode(&h.policy.fees, 1 << 20)?;
            if contract
                .as_ref()
                .is_some_and(|old| *old != current_contract)
            {
                return Err(
                    "fee contractual rights must stay identical across measures and paths".into(),
                );
            }
            contract.get_or_insert(current_contract);
            for a in &s.allocations {
                let f = h
                    .policy
                    .fees
                    .iter()
                    .find(|f| f.fee_id == a.fee_id)
                    .ok_or("fee allocation absent")?;
                if a.allocated_cost != 0. && a.allocated_cost.signum() != f.entitlement.signum() {
                    return Err("fee allocation sign differs from acquired asset/liability".into());
                }
            }
            validate_releases(s, h, &loan_history.source, role)?;
            let (w, b) = admit(&h.policy, &loan_history.source)?;
            let extra = if role == Role::Physical {
                s.allocations.len() + h.acquisition_releases.len()
            } else {
                0
            };
            work = work
                .checked_add(w * 2)
                .and_then(|v| v.checked_add(extra * 128))
                .ok_or("fee graph joined work overflow")?;
            bytes = bytes
                .checked_add(b * 2)
                .and_then(|v| v.checked_add(extra * 4096))
                .ok_or("fee graph joined capacity overflow")?;
            if let Some(exercise) = &p.exercise {
                let trials = exercise
                    .exercise_dates
                    .iter()
                    .map(|d| {
                        usize::from(d.call_clean_price_factor.is_some())
                            + usize::from(d.put_clean_price_factor.is_some())
                    })
                    .sum::<usize>();
                let calibrating = original
                    && role != Role::Physical
                    && !matches!(
                        p.oas_target,
                        crate::advanced_graph::OasTarget::Locked { .. }
                    );
                let additional = if role == Role::Physical {
                    w * (exercise.exercise_dates.len() + 1)
                } else {
                    let a = loan::admit_economic_with_measure(
                        &loan_history.source,
                        portfolio_model_core::credit::CreditMeasure::Pricing,
                    )?;
                    let preparation = w
                        .checked_mul(trials + 2)
                        .and_then(|n| n.checked_add(a.joined_work_upper.checked_mul(trials)?))
                        .ok_or("joint fee continuation work overflow")?;
                    // Calibration prepares once, then the final original policy
                    // prepares once more. Repeated trials only replay cash and
                    // observed fee information; they do not rebuild owners.
                    let reduction = w
                        .checked_mul(exercise.exercise_dates.len() + 1)
                        .and_then(|n| n.checked_mul(if calibrating { 102 } else { 1 }))
                        .ok_or("joint fee reduction work overflow")?;
                    let retained = b
                        .checked_add(
                            a.carry_points_upper
                                .checked_mul(2048)
                                .ok_or("joint stopped cash capacity overflow")?,
                        )
                        .and_then(|n| n.checked_mul(trials + 1))
                        .and_then(|n| n.checked_add(a.raw_bytes))
                        .ok_or("joint stopped cash capacity overflow")?;
                    bytes = bytes
                        .checked_add(
                            retained
                                .checked_mul(if calibrating { 2 } else { 1 })
                                .ok_or("joint retained cash capacity overflow")?,
                        )
                        .ok_or("joint retained cash capacity overflow")?;
                    preparation
                        .checked_mul(if calibrating { 2 } else { 1 })
                        .and_then(|n| n.checked_add(reduction))
                        .ok_or("joint fee continuation work overflow")?
                };
                work = work
                    .checked_add(additional)
                    .ok_or("joint fee continuation work overflow")?;
            }
        }
    }
    Ok((work, bytes))
}
fn acquired(s: &Selection, h: &History, bound: &mut BoundPath) -> Result<(), String> {
    if bound.source.raw_request().measure
        != portfolio_model_core::cashflow::CashflowMeasure::Physical
    {
        return Ok(());
    }
    let mut raw = bound.source.raw_request().clone();
    let opening = bound.source.native_transfer().cloned();
    let origin = raw.asof_ordinal;
    let mut events = bound
        .native_years
        .iter()
        .skip(opening.as_ref().map_or(0, |_| raw.fees.len()))
        .copied()
        .zip(raw.events.iter().cloned())
        .map(|(year, e)| (year, 1u8, e))
        .collect::<Vec<_>>();
    for (fi, a) in s.allocations.iter().enumerate() {
        let basis = a.allocated_cost
            - opening
                .as_ref()
                .map_or(Ok(0.), |t| t.net_stock(&a.fee_id))?;
        if basis == 0. {
            continue;
        }
        let fee = raw
            .fees
            .iter()
            .find(|f| f.fee_id == a.fee_id)
            .ok_or("fee acquisition source absent")?;
        events.push((
            0.,
            0,
            fee::Event {
                event_id: format!("native-fee:{}:{fi}:acquire", bound.path_key),
                fee_id: a.fee_id.clone(),
                source: fee.source.clone(),
                review: a.review.clone(),
                ordinal: origin,
                sequence: 0,
                action: fee::Action::Acquire,
                reason: fee::Reason::OtherReviewed,
                amount: basis,
            },
        ));
    }
    for (i, r) in h.acquisition_releases.iter().enumerate() {
        let fee = raw
            .fees
            .iter()
            .find(|f| f.fee_id == r.fee_id)
            .ok_or("fee cost release source absent")?;
        events.push((
            f64::from(r.ordinal - origin) / 365.,
            2,
            fee::Event {
                event_id: format!("native-fee:{}:{i}:release-acquired-cost", bound.path_key),
                fee_id: r.fee_id.clone(),
                source: fee.source.clone(),
                review: r.review.clone(),
                ordinal: r.ordinal,
                sequence: 0,
                action: fee::Action::ReleaseAcquiredCost,
                reason: fee::Reason::OtherReviewed,
                amount: r.amount,
            },
        ));
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    raw.events.clear();
    bound.native_years.clear();
    for (i, (year, _, mut event)) in events.into_iter().enumerate() {
        event.sequence = i as u32;
        raw.events.push(event);
        bound.native_years.push(year);
    }
    bound.source = if let Some(t) = &opening {
        fee::evaluate_transferred_with_check(&raw, t, portfolio_compute_control::checkpoint)?
    } else {
        fee::evaluate_with_check(&raw, portfolio_compute_control::checkpoint)?
    };
    if opening.is_some() {
        bound
            .native_years
            .splice(0..0, std::iter::repeat_n(0., raw.fees.len()));
    }
    Ok(())
}
/// The analytical fee journal may preserve unsettled gross rights. A complete
/// acquired/priced cash source must instead supply their actual disposition;
/// no terminal zero-price assumption is inferred from an absent cash event.
pub(crate) fn require_complete(bound: &BoundPath) -> Result<(), String> {
    for closing in bound.source().closing() {
        let state = &closing.state;
        if [
            state.unearned_entitlement,
            state.fee_receivable,
            state.fee_payable,
            state.deferred_fee_income,
            state.prepaid_fee_expense,
        ]
        .iter()
        .any(|v| !near(*v, 0.))
        {
            return Err("complete acquired fee source requires explicit settlement, waiver, cancellation or refund of all remaining service/gross rights".into());
        }
    }
    Ok(())
}
fn projection(s: &Selection, h: &History, mut bound_path: BoundPath) -> Result<Projection, String> {
    require_complete(&bound_path)?;
    acquired(s, h, &mut bound_path)?;
    let mut cash = Vec::new();
    for (year, g) in bound_path
        .native_years()
        .iter()
        .zip(bound_path.source().groups())
    {
        check()?;
        // The unit purchase event already pays loan + acquired fee cost once.
        // This retained cash tape contains only subsequent service obligations.
        let amount = if matches!(
            g.event.action,
            fee::Action::Acquire | fee::Action::AcquireTransferredStock
        ) {
            0.
        } else {
            g.postings
                .iter()
                .filter(|p| p.account == fee::Account::Cash)
                .try_fold(0., |sum, p| money(sum + p.amount))?
        };
        if amount != 0. {
            cash.push(EconomicCash {
                event_id: format!("separate-fee:{}", g.event.event_id),
                kind: "separate_nonintegral_fee".into(),
                year: *year,
                principal: 0.,
                interest: 0.,
                fees: amount,
            });
        }
    }
    Ok(Projection {
        raw_fee_history: h.clone(),
        bound_path,
        cash,
        discount_spread: s.discount_spread,
        allocations: s.allocations.clone(),
        loan_original_cost_yield_unchanged: true,
    })
}
pub fn physical(
    s: &Selection,
    h: &History,
    owner: &loan::ResultData,
) -> Result<Projection, String> {
    let mut paths = bind(&h.policy, owner)?;
    if paths.len() != 1 {
        return Err("one native fee path per graph source required".into());
    }
    projection(s, h, paths.remove(0))
}
pub fn pricing(
    s: &Selection,
    h: &History,
    raw: &loan::Request,
    owner: &loan::EconomicResult,
) -> Result<Projection, String> {
    pricing_with_stops(s, h, raw, owner, &[])
}
pub fn pricing_with_stops(
    s: &Selection,
    h: &History,
    raw: &loan::Request,
    owner: &loan::EconomicResult,
    stops: &[loan::NativeStop],
) -> Result<Projection, String> {
    let mut paths = if stops.is_empty() {
        bind_economic(&h.policy, raw, owner)?
    } else {
        super::bind_carry(&h.policy, raw, owner.carry(), stops)?
    };
    if paths.len() != 1 {
        return Err("one native Pricing fee path per graph source required".into());
    }
    projection(s, h, paths.remove(0))
}
impl Projection {
    /// Fee income is deliberately outside interest/EIR; acquisition cost is
    /// released according to a complete reviewed per-path physical schedule.
    pub fn income_period(&self, left: f64, right: f64) -> Result<f64, String> {
        let mut income = 0.;
        for (year, g) in self
            .bound_path
            .native_years()
            .iter()
            .zip(self.bound_path.source().groups())
        {
            if (*year > left || (left == 0. && *year == 0.)) && *year <= right {
                for p in &g.postings {
                    if matches!(
                        p.account,
                        fee::Account::FeeIncome
                            | fee::Account::FeeWriteoffExpense
                            | fee::Account::FeeRecoveryIncome
                            | fee::Account::FeeAcquisitionExpense
                    ) {
                        income = money(income - p.amount)?;
                    }
                }
            }
        }
        Ok(income)
    }
    pub fn price(
        &self,
        curve: &crate::market::analysis::curves::SuppliedCurve,
        asof: i32,
        mark: i32,
        bump: f64,
    ) -> Result<f64, String> {
        self.price_clock(curve, asof, mark, bump, false)
    }
    /// Acquisition is cum purchase-day fee cash; subsequent marks are ex cash.
    pub fn original_price(
        &self,
        curve: &crate::market::analysis::curves::SuppliedCurve,
        asof: i32,
    ) -> Result<f64, String> {
        self.price_clock(curve, asof, asof, 0., true)
    }
    fn price_clock(
        &self,
        curve: &crate::market::analysis::curves::SuppliedCurve,
        asof: i32,
        mark: i32,
        bump: f64,
        include_at_mark: bool,
    ) -> Result<f64, String> {
        let t = f64::from(mark - asof) / 365.;
        let df0 = curve.value_validated(t)?.discount_factor;
        let mut sum = 0.;
        let mut correction = 0.;
        for c in self
            .cash
            .iter()
            .filter(|c| c.year > t || (include_at_mark && c.year == t))
        {
            check()?;
            let v = money(
                c.total()? * curve.value_validated(c.year)?.discount_factor / df0
                    * (-(self.discount_spread + bump) * (c.year - t)).exp(),
            )?;
            let y = v - correction;
            let next = money(sum + y)?;
            correction = (next - sum) - y;
            sum = next;
        }
        Ok(sum)
    }
}
