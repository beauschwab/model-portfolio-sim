//! Explicit dated principal cash, independent of period-opening coupon nominal.
//! No caller monetary entitlement is accepted; a seasoned basis is native replay.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalCash {
    pub event_id: String,
    pub payment_ordinal: i32,
    pub amount: f64,
}
/// Sealed by the complete-origin acquisition owner. Never deserializable.
#[derive(Debug)]
pub(crate) struct AcquiredBasis {
    contract: Identity,
    asof: i32,
    periods: Vec<PeriodNominal>,
}
#[derive(Debug)]
struct PeriodNominal {
    id: String,
    start: i32,
    end: i32,
    payment: i32,
    day_count: ActualDayCount,
    nominal: f64,
}
impl AcquiredBasis {
    pub(crate) fn retained_upper(periods: usize) -> Result<usize, String> {
        // Vec's minimum allocation, subsequent geometric growth, cloned UTF8
        // period IDs and contract identity strings, with allocator slack.
        (periods
            .checked_mul(2)
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul(std::mem::size_of::<PeriodNominal>())))
        .and_then(|n| n.checked_add(periods.checked_mul(512)?))
        .and_then(|n| n.checked_add(512 + std::mem::size_of::<Self>()))
        .ok_or_else(|| "acquired coupon basis capacity overflow".into())
    }
    pub(crate) fn from_origin(
        buyer: &ContractPathRequest,
        origin: &ContractPathResult,
    ) -> Result<Self, String> {
        let mut periods = Vec::new();
        for p in &buyer.periods {
            portfolio_compute_control::checkpoint()?;
            let a = origin
                .audit
                .iter()
                .find(|a| a.period_id == p.period_id)
                .ok_or("acquired principal basis lacks exact origin coupon")?;
            periods.push(PeriodNominal {
                id: p.period_id.clone(),
                start: p.start_ordinal,
                end: p.end_ordinal,
                payment: p.payment_ordinal,
                day_count: p.day_count,
                nominal: a.opening_principal,
            });
        }
        Ok(Self {
            contract: buyer.contract.clone(),
            asof: buyer.asof_ordinal,
            periods,
        })
    }
    pub(crate) fn entitlement(
        &self,
        raw: &ContractPathRequest,
        index: usize,
    ) -> Result<f64, String> {
        let p = raw
            .periods
            .get(index)
            .ok_or("acquired coupon index missing")?;
        let n = self
            .periods
            .get(index)
            .ok_or("acquired coupon basis missing")?;
        if raw.contract != self.contract
            || raw.asof_ordinal != self.asof
            || n.id != p.period_id
            || n.start != p.start_ordinal
            || n.end != p.end_ordinal
            || n.payment != p.payment_ordinal
            || n.day_count != p.day_count
        {
            return Err("foreign acquired coupon basis/clock/contract".into());
        }
        nonnegative(n.nominal)
    }
}
pub(crate) fn events(plan: &PrincipalPlan) -> &[PrincipalCash] {
    match plan {
        PrincipalPlan::DatedSuppliedPeriodOpening { principal_events } => principal_events,
        _ => &[],
    }
}
pub(crate) fn selected(plan: &PrincipalPlan) -> bool {
    matches!(plan, PrincipalPlan::DatedSuppliedPeriodOpening { .. })
}
pub(crate) fn validate(raw: &ContractPathRequest) -> Result<(), String> {
    if !selected(&raw.principal_plan) {
        return Ok(());
    }
    let events = events(&raw.principal_plan);
    if events.is_empty() || events.len() + raw.periods.len() > MAX_PERIODS {
        return Err("dated principal plus coupon rows exceed bounded complete source".into());
    }
    let mut prior = None;
    let mut ids = BTreeSet::new();
    let mut meta = 0;
    let mut balance = raw.opening_principal;
    for p in &raw.periods {
        ids.insert(p.period_id.as_str());
    }
    for e in events {
        portfolio_compute_control::checkpoint()?;
        text(&e.event_id, &mut meta)?;
        domain(raw.asof_ordinal, e.payment_ordinal)?;
        if e.payment_ordinal > raw.tail_end_ordinal
            || prior.is_some_and(|v| v > e.payment_ordinal)
            || !ids.insert(e.event_id.as_str())
        {
            return Err("dated principal requires unique chronological in-tail actual cash".into());
        }
        let value = nonnegative(e.amount)?;
        if value == 0. || value > balance && !near(value, balance) {
            return Err("dated principal cash exceeds remaining live balance".into());
        }
        balance = amount(balance - value.min(balance))?;
        prior = Some(e.payment_ordinal);
    }
    if !near(balance, 0.) {
        return Err("dated principal complete tail does not conserve opening balance".into());
    }
    Ok(())
}
pub(crate) fn at(raw: &ContractPathRequest, date: i32) -> Result<f64, String> {
    let mut p = raw.opening_principal;
    for e in events(&raw.principal_plan) {
        portfolio_compute_control::checkpoint()?;
        if e.payment_ordinal > date {
            break;
        }
        p = amount(p - e.amount)?;
    }
    if p < 0. && !near(p, 0.) {
        return Err("dated principal prefix is negative".into());
    }
    Ok(p.max(0.))
}
pub(crate) fn work_upper(raw: &ContractPathRequest) -> Result<usize, String> {
    let h = events(&raw.principal_plan).len();
    if h == 0 {
        return Ok(0);
    }
    let p = raw.periods.len();
    let c = p.checked_add(h).ok_or("principal work overflow")?;
    let steps = h
        .checked_mul(h)
        .and_then(|n| n.checked_add(p.checked_mul(h)?.checked_mul(2)?))
        .and_then(|n| n.checked_add(c.checked_mul(c)?.checked_mul(3)?))
        .ok_or("principal work overflow")?;
    steps
        .checked_mul(512)
        .ok_or_else(|| "principal work overflow".into())
}
pub(crate) fn append_cash(
    raw: &ContractPathRequest,
    out: &mut Vec<DatedCashflowEvent>,
    sign: f64,
) -> Result<(), String> {
    for e in events(&raw.principal_plan) {
        portfolio_compute_control::checkpoint()?;
        out.push(DatedCashflowEvent {
            position_id: raw.position_id.clone(),
            event_id: e.event_id.clone(),
            scenario: ScenarioSelector::Named {
                scenario_id: raw.scenario_id.clone(),
            },
            payment_ordinal: e.payment_ordinal,
            day_offset: (e.payment_ordinal - raw.asof_ordinal) as u32,
            accrual_period: None,
            currency: raw.currency.clone(),
            components: CashflowComponents {
                principal: sign * e.amount,
                cash_interest: 0.,
                accrued_interest: 0.,
                book_amortization: 0.,
                fees: 0.,
            },
            measure: raw.measure,
            provenance: EventProvenance::Modeled {
                model_id: raw.assumption.id.clone(),
                model_version: MODEL_VERSION.into(),
                input_revision: raw.assumption.revision.clone(),
            },
            adjustment: CashflowAdjustment::None {},
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::work::{ContractWorkSummary, GeneratedCatalogAuthority};
    use super::*;
    use crate::commercial_factor_source::{owner::NativeFactors, segments::FactorPolicy};
    fn raw() -> (ContractPathRequest, FactorPolicy) {
        let mut x: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../../examples/models/commercial-secondary-acquisition-1.json"
        ))
        .unwrap();
        let o = &mut x["source"]["transfer"]["contract_origin"];
        o["principal_plan"] = serde_json::json!({"kind":"dated_supplied_period_opening_nominal-1","principal_events":[{"event_id":"actual20","payment_ordinal":738871,"amount":20.},{"event_id":"actual80","payment_ordinal":738946,"amount":80.}]});
        (
            serde_json::from_value(o.clone()).unwrap(),
            serde_json::from_value(x["factor_policy"].clone()).unwrap(),
        )
    }
    #[test]
    fn counted_structural_census_includes_every_dated_cash_binding_and_owner_query() {
        let (raw, policy) = raw();
        let authority = GeneratedCatalogAuthority::bind(&raw).unwrap();
        let source = NativeFactors::compile_for_measure_with_work(
            raw.clone(),
            policy,
            4,
            raw.measure,
            Some(&authority),
        )
        .unwrap();
        source.earned(0., 15. / 365.).unwrap();
        source.earned(15. / 365., 60. / 365.).unwrap();
        source.cumulative_coupon_factors(0.).unwrap();
        source.cumulative_coupon_factors(30. / 365.).unwrap();
        let observed = source.counted_operations().unwrap();
        let upper = &source.admission().counted_work.as_ref().unwrap().upper;
        assert!(observed.binding <= upper.binding);
        assert!(observed.validation <= upper.validation);
        assert!(observed.ordering <= upper.ordering);
        assert!(observed.earned_steps <= upper.earned_steps);
        assert!(observed.canonical_steps <= upper.canonical_steps);
        assert_eq!(source.source().schedule.events.len(), 3);
        let legacy = ContractWorkSummary::census(&raw, 4, false, false).unwrap();
        assert_eq!(legacy.upper.binding, 14);
    }
    #[test]
    fn acquired_basis_binds_contract_clock_and_exact_period_shape() {
        let (origin, _) = raw();
        let full = build_path(&origin).unwrap();
        let mut buyer = origin.clone();
        buyer.asof_ordinal += 30;
        let basis = AcquiredBasis::from_origin(&buyer, &full).unwrap();
        assert_eq!(basis.entitlement(&buyer, 0).unwrap(), 100.);
        for mutation in 0..4 {
            let mut b = buyer.clone();
            match mutation {
                0 => b.asof_ordinal += 1,
                1 => b.contract.revision.push('x'),
                2 => b.periods[0].start_ordinal += 1,
                _ => b.periods[0].payment_ordinal += 1,
            };
            assert!(basis.entitlement(&b, 0).is_err());
        }
        // This inventory is ephemeral within one admitted origin owner. The
        // existing selected per-coupon4096 reserve covers its exact allocation.
        for count in [1, 2, 17, 4096] {
            assert!(
                AcquiredBasis::retained_upper(count).unwrap()
                    + 2 * count
                        * std::mem::size_of::<
                            crate::products::commercial::acquisition::TransferredCoupon,
                        >()
                    + 64
                    <= 4096 * count
            );
        }
    }
}
