//! Explicit purchase-date original EIR on unchanged supplied physical cash rights.
use super::{bounded, capture, label, money, CashOrigin, MAX_RETAINED_BYTES, MAX_SEGMENTS};
use crate::{
    advanced_graph::{
        CaptureOwner, CapturePlan, CapturePolicy, CaptureSegment, Role, ScheduledCapture,
    },
    deposit_graph::DepositGraph,
};
use portfolio_model_core::{
    cashflow::{
        AccrualPeriod, CashflowAdjustment, CashflowComponents, CashflowMeasure, DatedCashflowEvent,
        EventProvenance, ScenarioSelector,
    },
    dated_income::{self, DatedIncomeRequest, DatedIncomeResult, Identity, IncomeEvent},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SCHEMA: &str = "deposit-original-income-capture-1";
pub const MAX_INCOME_WORK: usize = 250_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomeAnchor {
    pub position_id: String,
    pub purchase_ordinal: i32,
    pub purchase_quote: Identity,
    pub purchase_price_per100: f64,
    pub original_principal: f64,
    pub original_gross_carrying_value: f64,
    pub original_annual_effective_rate: f64,
    pub original_yield_anchor: Identity,
    pub basis_revision_policy: String,
    pub terminal_basis_policy: String,
}
#[derive(Serialize)]
pub struct IncomeSource {
    pub position_id: String,
    pub scenario_id: String,
    pub run_id: String,
    pub anchor: IncomeAnchor,
    pub request: DatedIncomeRequest,
    pub result: DatedIncomeResult,
}
#[derive(Serialize)]
pub struct Capture {
    pub scheduled: ScheduledCapture,
    pub cash_origins: Vec<CashOrigin>,
    pub income_sources: Vec<IncomeSource>,
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}

/// No yield solve, market value substitution, catchup or terminal cash creation.
pub fn capture_original_income(
    graph: &DepositGraph,
    owners: &[CaptureOwner],
    policy: &CapturePolicy,
    anchors: &[IncomeAnchor],
) -> Result<Capture, String> {
    portfolio_compute_control::checkpoint()?;
    let raw = graph.request();
    if policy.recognition_model != "deposit_daily_original_eir-1"
        || policy.book_basis != "original_expected_cash_deposit-1"
        || policy.fees != "reject"
        || anchors.len() != raw.positions.len()
    {
        return Err("explicit original-EIR deposit policy and complete anchors required".into());
    }
    label(&policy.policy_id)?;
    label(&policy.policy_revision)?;
    let mut ids = HashSet::new();
    for anchor in anchors {
        for value in [
            &anchor.position_id,
            &anchor.purchase_quote.id,
            &anchor.purchase_quote.revision,
            &anchor.original_yield_anchor.id,
            &anchor.original_yield_anchor.revision,
        ] {
            label(value)?;
        }
        if !ids.insert(anchor.position_id.as_str())
            || !raw
                .positions
                .iter()
                .any(|p| p.position_id == anchor.position_id)
            || anchor.purchase_ordinal != raw.as_of_ordinal
            || anchor.basis_revision_policy != "reject_changed_cash_expectations-1"
            || anchor.terminal_basis_policy != "reject_nonzero-1"
            || !anchor.purchase_price_per100.is_finite()
            || !(0. ..=1e6).contains(&anchor.purchase_price_per100)
            || anchor.purchase_price_per100 == 0.
            || !anchor.original_annual_effective_rate.is_finite()
            || !(0. ..=10.).contains(&anchor.original_annual_effective_rate)
            || money(anchor.original_principal)? <= 0.
            || money(anchor.original_gross_carrying_value)? <= 0.
            || !near(
                money(anchor.original_principal * anchor.purchase_price_per100 / 100.)?,
                anchor.original_gross_carrying_value,
            )
        {
            return Err("original deposit purchase/quote/yield anchor or reviewed policy mismatch; seasoned/negative ownership unsupported".into());
        }
    }
    // Preflight every income recurrence before the underlying source is copied.
    let work = raw
        .positions
        .iter()
        .try_fold(0usize, |total, p| {
            p.histories.iter().try_fold(total, |n, h| {
                let q = h
                    .physical
                    .first()
                    .ok_or("missing original-income physical history")?;
                let days = q.request.horizon_days as usize;
                if days >= dated_income::MAX_EVENTS {
                    return Err("original deposit income full daily tail exceeds 4095 days");
                }
                n.checked_add(days + 1)
                    .ok_or("original deposit income work overflow")
            })
        })
        .map_err(str::to_owned)?;
    if work > MAX_INCOME_WORK {
        return Err("original deposit income work admission exceeded".into());
    }
    let par_policy = CapturePolicy {
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.policy_revision.clone(),
        recognition_model: "deposit_daily_par-1".into(),
        book_basis: "par_deposit_simple_interest".into(),
        fees: "reject".into(),
    };
    let mut captured = capture(graph, owners, &par_policy)?;
    let mut income_sources = Vec::new();
    let mut count = captured
        .scheduled
        .accrual_plans
        .iter()
        .map(|p| p.segments.len())
        .sum::<usize>();
    for position in &raw.positions {
        portfolio_compute_control::checkpoint()?;
        let anchor = anchors
            .iter()
            .find(|a| a.position_id == position.position_id)
            .ok_or("missing deposit income anchor")?;
        let owner = owners
            .iter()
            .find(|o| o.position_id == position.position_id)
            .ok_or("missing deposit income owner")?;
        let opening = captured
            .scheduled
            .openings
            .iter_mut()
            .find(|o| o.position_id == position.position_id)
            .ok_or("missing deposit income opening")?;
        if anchor.original_principal != opening.principal {
            return Err("original deposit face differs from graph opening".into());
        }
        opening.gross_carrying_value = anchor.original_gross_carrying_value;
        let base = graph
            .snapshot()
            .native_sources
            .iter()
            .find(|s| {
                s.position_id == position.position_id
                    && s.scenario_id == raw.base_scenario_id
                    && s.role == Role::Physical
            })
            .ok_or("missing original deposit base physical cash")?;
        let original_cash =
            serde_json::to_vec(&base.projection.cashflows).map_err(|e| e.to_string())?;
        for scenario in &raw.scenarios {
            portfolio_compute_control::checkpoint()?;
            let source = graph
                .snapshot()
                .native_sources
                .iter()
                .find(|s| {
                    s.position_id == position.position_id
                        && s.scenario_id == scenario.scenario_id
                        && s.role == Role::Physical
                })
                .ok_or("missing original deposit physical source")?;
            if serde_json::to_vec(&source.projection.cashflows).map_err(|e| e.to_string())?
                != original_cash
            {
                return Err("changed deposit expected physical cash requires explicit catchup/modification ownership".into());
            }
            let set = position
                .histories
                .iter()
                .find(|h| h.scenario_id == scenario.scenario_id)
                .ok_or("missing original deposit history")?;
            let end = raw.as_of_ordinal + set.physical[0].request.horizon_days as i32;
            let mut cash = BTreeMap::<i32, (f64, f64)>::new();
            for c in &source.projection.cashflows {
                let v = cash.entry(c.ordinal).or_default();
                v.0 += -c.principal;
                v.1 += -c.cash_interest;
                money(v.0)?;
                money(v.1)?;
            }
            let mut earned = HashMap::<i32, f64>::new();
            for i in &source.projection.interest {
                if i.end_ordinal - i.start_ordinal != 1 {
                    return Err(
                        "original deposit income requires native daily earned origins".into(),
                    );
                }
                let v = earned.entry(i.end_ordinal).or_default();
                *v += i.amount;
                money(*v)?;
            }
            let rate_log = anchor.original_annual_effective_rate.ln_1p();
            let mut pv = 0.;
            for (date, (principal, interest)) in &cash {
                pv += (principal + interest)
                    * (-rate_log * f64::from(*date - raw.as_of_ordinal) / 365.).exp();
                money(pv)?;
            }
            if !near(pv, anchor.original_gross_carrying_value) {
                return Err(
                    "original deposit carrying/complete physical cash/yield anchor inconsistent"
                        .into(),
                );
            }
            let mut events = Vec::with_capacity(set.physical[0].request.horizon_days as usize + 1);
            if let Some(&(p, i)) = cash.get(&raw.as_of_ordinal) {
                events.push(IncomeEvent {
                    event_id: format!("deposit-eir:{}", raw.as_of_ordinal),
                    source: Identity {
                        id: owner.source_id.clone(),
                        revision: owner.source_revision.clone(),
                    },
                    payment_ordinal: raw.as_of_ordinal,
                    contractual_interest_earned: 0.,
                    principal_cash: p,
                    interest_cash: i,
                    integral_fee_cash: 0.,
                    nonintegral_fee_cash: 0.,
                    yield_change: None,
                });
            }
            for day in raw.as_of_ordinal + 1..=end {
                portfolio_compute_control::checkpoint()?;
                let (p, i) = cash.get(&day).copied().unwrap_or_default();
                events.push(IncomeEvent {
                    event_id: format!("deposit-eir:{day}"),
                    source: Identity {
                        id: owner.source_id.clone(),
                        revision: owner.source_revision.clone(),
                    },
                    payment_ordinal: day,
                    contractual_interest_earned: earned.get(&day).copied().unwrap_or_default(),
                    principal_cash: p,
                    interest_cash: i,
                    integral_fee_cash: 0.,
                    nonintegral_fee_cash: 0.,
                    yield_change: None,
                });
            }
            let request = DatedIncomeRequest {
                schema: dated_income::SCHEMA.into(),
                contract: Identity {
                    id: position.position_id.clone(),
                    revision: raw.identity.input_revision.clone(),
                },
                original_yield_anchor: anchor.original_yield_anchor.clone(),
                policy: Identity {
                    id: policy.policy_id.clone(),
                    revision: policy.policy_revision.clone(),
                },
                currency: raw.currency.clone(),
                monetary_unit: "currency_units".into(),
                measure: CashflowMeasure::Physical,
                as_of_ordinal: raw.as_of_ordinal,
                full_contract_end_ordinal: end,
                opening_principal: anchor.original_principal,
                opening_accrued_interest: 0.,
                opening_gross_carrying_value: anchor.original_gross_carrying_value,
                original_annual_effective_rate: anchor.original_annual_effective_rate,
                opening_annual_effective_rate: anchor.original_annual_effective_rate,
                events,
            };
            let result = dated_income::evaluate(&request)?;
            if !near(result.remaining_gross_carrying_value, 0.)
                || !near(result.remaining_principal, 0.)
                || !near(result.remaining_accrued_interest, 0.)
                || result
                    .rows
                    .last()
                    .is_some_and(|row| !near(row.closing_book_adjustment, 0.))
            {
                return Err("original deposit EIR terminal carrying/principal/accrual/basis does not close; no plug allowed".into());
            }
            let schedule = &mut captured
                .scheduled
                .sources
                .iter_mut()
                .find(|s| s.source_id == owner.source_id)
                .ok_or("missing original deposit dated source")?
                .schedule;
            for row in &result.rows {
                portfolio_compute_control::checkpoint()?;
                // Retain every admitted daily origin, including exact zero.
                // Event identities must not depend on floating-point zero tests.
                if row.payment_ordinal == raw.as_of_ordinal {
                    continue;
                }
                count += 1;
                if count > MAX_SEGMENTS {
                    return Err(
                        "original deposit combined recognition segments exceed admission".into(),
                    );
                }
                let event_id = row.event_id.clone();
                let selector = ScenarioSelector::Named {
                    scenario_id: scenario.run_id.clone(),
                };
                let amort = -row.basis_amortization;
                money(amort)?;
                captured.scheduled.accrual_plans.push(CapturePlan {
                    position_id: position.position_id.clone(),
                    event_id: event_id.clone(),
                    run: selector.clone(),
                    segments: vec![CaptureSegment {
                        origin_id: format!(
                            "deposit-effective-interest:{}:{}",
                            anchor.original_yield_anchor.id, row.payment_ordinal
                        ),
                        start_ordinal: row.payment_ordinal - 1,
                        end_ordinal: row.payment_ordinal,
                        accrued_interest: 0.,
                        book_amortization: amort,
                    }],
                });
                schedule.events.push(DatedCashflowEvent {
                    position_id: position.position_id.clone(),
                    event_id,
                    scenario: selector,
                    payment_ordinal: row.payment_ordinal,
                    day_offset: (row.payment_ordinal - raw.as_of_ordinal) as u32,
                    accrual_period: Some(AccrualPeriod {
                        start_ordinal: row.payment_ordinal - 1,
                        end_ordinal: row.payment_ordinal,
                    }),
                    currency: raw.currency.clone(),
                    components: CashflowComponents {
                        principal: 0.,
                        cash_interest: 0.,
                        accrued_interest: 0.,
                        book_amortization: amort,
                        fees: 0.,
                    },
                    measure: CashflowMeasure::Physical,
                    provenance: EventProvenance::Modeled {
                        model_id: SCHEMA.into(),
                        model_version: SCHEMA.into(),
                        input_revision: raw.identity.input_revision.clone(),
                    },
                    adjustment: CashflowAdjustment::None {},
                });
            }
            income_sources.push(IncomeSource {
                position_id: position.position_id.clone(),
                scenario_id: scenario.scenario_id.clone(),
                run_id: scenario.run_id.clone(),
                anchor: anchor.clone(),
                request,
                result,
            });
        }
        let schedule = &mut captured
            .scheduled
            .sources
            .iter_mut()
            .find(|s| s.source_id == owner.source_id)
            .ok_or("missing original deposit dated source")?
            .schedule;
        schedule.events.sort_by_key(|e| e.payment_ordinal);
        schedule.validate()?;
    }
    captured.scheduled.policy = policy.clone();
    captured.scheduled.schema = SCHEMA.into();
    captured.scheduled.scope="purchase-date original EIR on complete unchanged original physical expected cash; gross contractual expense/rights and EIR basis separated; no Q-yield inference, catchup, fees or terminal plug".into();
    let result = Capture {
        scheduled: captured.scheduled,
        cash_origins: captured.cash_origins,
        income_sources,
    };
    bounded(&result, MAX_RETAINED_BYTES / 4)?;
    Ok(result)
}
