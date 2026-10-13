//! Native gross contractual cash, separate from principal and earned-income metrics.
use crate::advanced_product::{amount, ContractualProjection, SourceIdentity};
use portfolio_model_core::limits::MAX_DATED_METADATA_BYTES;
use serde::Serialize;
use std::collections::BTreeSet;

pub const MAX_FUNDING_VALUES: usize = 1_000_000;
pub const MAX_CASH_EVENTS: usize = 250_000;

#[derive(Clone, Serialize)]
pub struct PurchaseCost {
    pub unit_index: usize,
    pub ordinal: i32,
    pub cash_price_per_face_unit: f64,
    pub bank_cash: f64,
    pub contract_source: SourceIdentity,
}
#[derive(Clone, Serialize)]
pub struct CashEvent {
    pub unit_index: usize,
    pub ordinal: i32,
    pub event_id: String,
    pub kind: String,
    pub purchase_cash: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub bank_cash: f64,
}
pub(crate) struct UnitCashPlan {
    pub purchase: PurchaseCost,
    pub events: Vec<CashEvent>,
}
#[derive(Clone, Serialize)]
pub struct FundingResult {
    pub policy: String,
    pub scenario_ids: Vec<String>,
    pub ordinals: Vec<i32>,
    /// Dense [scenario,unit,full-tail cash/check date]. Fixed contractual cash
    /// repeats by explicit scenario; no future incoming cash funds past dates.
    pub shape: [usize; 3],
    pub demand: Vec<f64>,
    pub purchase_costs: Vec<PurchaseCost>,
    /// Stable by ordinal, then input unit, then native source event sequence.
    pub cash_events: Vec<CashEvent>,
    pub input_revision: String,
}
pub(crate) fn plan(
    p: &ContractualProjection,
    quote: f64,
    side: f64,
    index: usize,
    source: SourceIdentity,
) -> Result<UnitCashPlan, String> {
    if ![-1., 1.].contains(&side) || amount(quote)? <= 0. {
        return Err("invalid native funding direction/price".into());
    }
    let cost = PurchaseCost {
        unit_index: index,
        ordinal: p.purchase_ordinal,
        cash_price_per_face_unit: quote,
        bank_cash: amount(-side * quote)?,
        contract_source: source,
    };
    let mut events = vec![CashEvent {
        unit_index: index,
        ordinal: p.purchase_ordinal,
        event_id: format!("purchase:{}", p.purchase_ordinal),
        kind: "purchase".into(),
        purchase_cash: cost.bank_cash,
        principal_cash: 0.,
        interest_cash: 0.,
        bank_cash: cost.bank_cash,
    }];
    for event in &p.events {
        if event.components.fees != 0. {
            return Err("fixed-unit cash policy has no fee ownership".into());
        }
        events.push(CashEvent {
            unit_index: index,
            ordinal: event.payment_ordinal,
            event_id: event.event_id.clone(),
            kind: "contractual_settlement".into(),
            purchase_cash: 0.,
            principal_cash: amount(side * event.components.principal)?,
            interest_cash: amount(side * event.components.cash_interest)?,
            bank_cash: amount(side * event.components.cash_total()?)?,
        });
    }
    Ok(UnitCashPlan {
        purchase: cost,
        events,
    })
}
pub(crate) fn compile(
    plans: Vec<UnitCashPlan>,
    scenarios: Vec<String>,
    reporting: &[i32],
    input_revision: String,
) -> Result<FundingResult, String> {
    let n = plans.len();
    let s = scenarios.len();
    let mut dates: BTreeSet<_> = reporting.iter().copied().collect();
    let mut count = 0usize;
    let mut bytes = input_revision.len() + scenarios.iter().map(String::len).sum::<usize>();
    for p in &plans {
        count = count
            .checked_add(p.events.len())
            .ok_or("native funding event admission overflow")?;
        bytes = bytes.saturating_add(
            p.purchase.contract_source.id.len() + p.purchase.contract_source.revision.len(),
        );
        for e in &p.events {
            dates.insert(e.ordinal);
            bytes = bytes.saturating_add(e.event_id.len() + e.kind.len());
        }
    }
    if count > MAX_CASH_EVENTS || bytes > MAX_DATED_METADATA_BYTES {
        return Err("native funding cash-event/metadata admission exceeded".into());
    }
    let ordinals: Vec<_> = dates.into_iter().collect();
    let f = ordinals.len();
    let size = s
        .checked_mul(n)
        .and_then(|v| v.checked_mul(f))
        .ok_or("native funding dense shape overflow")?;
    if size > MAX_FUNDING_VALUES {
        return Err("native funding dense cash/date work admission exceeded".into());
    }
    let mut demand = vec![0.; size];
    let mut costs = Vec::new();
    let mut events = Vec::with_capacity(count);
    for (i, plan) in plans.into_iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let mut cursor = 0;
        let mut cash = 0.;
        for (d, &ordinal) in ordinals.iter().enumerate() {
            while cursor < plan.events.len() && plan.events[cursor].ordinal <= ordinal {
                cash = amount(cash + plan.events[cursor].bank_cash)?;
                cursor += 1;
            }
            for scenario in 0..s {
                demand[(scenario * n + i) * f + d] = amount(-cash)?;
            }
        }
        costs.push(plan.purchase);
        events.extend(plan.events);
    }
    // Stable sort preserves same-date event phases and each source's input order.
    events.sort_by_key(|e| (e.ordinal, e.unit_index));
    Ok(FundingResult{policy:"gross contractual pretax cash retained; end-of-day netting; complete contractual tails; taxes/distributions/intraday rules not supplied".into(),scenario_ids:scenarios,ordinals,shape:[s,n,f],demand,purchase_costs:costs,cash_events:events,input_revision})
}
