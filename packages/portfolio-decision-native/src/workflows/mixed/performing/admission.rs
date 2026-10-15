use super::*;
use portfolio_ledger_native::{
    dated_ledger::{CompiledDatedLedger, DatedRunState},
    types::Spec,
};
use portfolio_model_core::cashflow::CashflowMeasure;
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn bounded<T: Serialize>(x: &T, limit: usize) -> Result<usize, String> {
    struct Count {
        n: usize,
        max: usize,
    }
    impl Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
            self.n = self
                .n
                .checked_add(b.len())
                .ok_or_else(|| std::io::Error::other("mixed byte overflow"))?;
            if self.n > self.max {
                return Err(std::io::Error::other(
                    "mixed retained/output byte admission exceeded",
                ));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Count { n: 0, max: limit };
    serde_json::to_writer(&mut c, x).map_err(|e| e.to_string())?;
    Ok(c.n)
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "mixed aggregate work/capacity overflow".into())
}
pub(super) fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "mixed aggregate work/capacity overflow".into())
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("mixed original opening/full-tail source reconciliation failed".into())
    } else {
        Ok(())
    }
}
pub(super) fn validate(r: &Request) -> Result<(Spec, CompiledDatedLedger), String> {
    bounded(r, 32 * 1024 * 1024)?;
    if r.schema != SCHEMA
        || r.policy.expectation != "separately_supplied_physical_expected_paths-1"
        || r.policy.phase != "opening_acquisition_recognition_settlement-1"
        || r.policy.backbook_basis != "reviewed_supplied_dated_recognition-1"
        || r.policy.tail != "retain_complete-1"
        || [&r.policy.fees, &r.policy.defaults, &r.policy.marks]
            .iter()
            .any(|s| s.as_str() != "reject")
    {
        return Err("mixed reviewed policy/schema mismatch".into());
    }
    for identity in [&r.policy.identity, &r.opening_identity] {
        for s in [&identity.id, &identity.revision] {
            if s.trim().is_empty() || s.len() > 120 || s.chars().any(char::is_control) {
                return Err("mixed explicit bounded review/opening identity required".into());
            }
        }
    }
    let combined = portfolio_ledger_native::spec::normalize(
        r.combined_specification.clone(),
        MAX_POSITIONS,
        MAX_WORK,
    )?;
    let backbook = CompiledDatedLedger::compile(r.backbook.clone(), MAX_POSITIONS, MAX_WORK)?;
    if !combined.reverse_severities.is_empty() || !backbook.spec.reverse_severities.is_empty() {
        return Err(
            "mixed reverse severities require full run-keyed detailed journal integration".into(),
        );
    }
    let reports = &r.candidate.product.reporting_ordinals;
    if reports.first() != Some(&r.backbook.dated.as_of_ordinal)
        || reports
            .last()
            .is_none_or(|e| (*e - r.backbook.dated.as_of_ordinal) as usize != combined.horizon_days)
        || combined.horizon_days != backbook.spec.horizon_days
    {
        return Err("mixed actual reporting/as-of boundaries mismatch".into());
    }
    if backbook.owner_positions.len() != backbook.spec.positions.len()
        || !combined.cashflows.is_empty()
        || !combined.netting_sets.is_empty()
        || !combined.policies.is_empty()
    {
        return Err("mixed only explicit dated backbook and candidate owners supported".into());
    }
    let owner_ids: BTreeSet<_> = r
        .candidate
        .owners
        .iter()
        .map(|o| o.position_id.as_str())
        .collect();
    if owner_ids.len() != r.candidate.owners.len()
        || combined.positions.len() != backbook.spec.positions.len() + owner_ids.len()
    {
        return Err("mixed exact disjoint position ownership required".into());
    }
    let mut original = serde_json::to_value(&combined).map_err(|e| e.to_string())?;
    original["positions"] = serde_json::to_value(
        combined
            .positions
            .iter()
            .filter(|p| !owner_ids.contains(p.id.as_str()))
            .collect::<Vec<_>>(),
    )
    .map_err(|e| e.to_string())?;
    if original != serde_json::to_value(&backbook.spec).map_err(|e| e.to_string())? {
        return Err("mixed actual accounts/backbook opening anchors or scenarios differ".into());
    }
    let accounts: BTreeMap<_, _> = combined
        .accounts
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect();
    for a in &combined.accounts {
        if a.currency != r.candidate.product.currency
            || a.annual_fees != 0.
            || a.annual_costs != 0.
            || a.tax_rate != 0.
            || a.annual_dividends != 0.
            || a.capital_deductions != 0.
        {
            return Err(
                "mixed generic fees/tax/distributions/currency conversion unsupported".into(),
            );
        }
    }
    for owner in &r.candidate.owners {
        let p = combined
            .positions
            .iter()
            .find(|p| p.id.as_str() == owner.position_id)
            .ok_or("mixed candidate zero placeholder absent")?;
        if backbook.spec.positions.iter().any(|x| x.id == p.id)
            || p.account.as_str() != owner.account_id
            || !accounts.contains_key(owner.account_id.as_str())
            || p.classification.as_str() != owner.ledger_classification
            || p.balance != 0.
            || p.opening_accrued != 0.
            || p.book_adjustment != 0.
            || p.allowance != 0.
            || p.rate != 0.
            || p.start_day != 0
            || p.maturity_day != 0
            || p.annual_pd != 0.
            || p.watch_fraction != 0.
            || p.migration_rate != 0.
            || p.commitment != 0.
            || p.draw_fraction != 0.
            || p.monthly_runoff != 0.
            || p.floating_beta != 0.
            || p.rollover != 0.
            || p.eligible_fraction != 0.
            || p.encumbered_fraction != 0.
            || p.pledged_face != 0.
            || !p.collateral_position.is_empty()
            || !p.collateral_pool.is_empty()
            || p.opening_market_price != 1.
        {
            return Err("mixed candidate opening/source overlaps generic ownership".into());
        }
        let u = r
            .candidate
            .product
            .units
            .iter()
            .find(|u| u.unit_id == owner.unit_id)
            .ok_or("mixed candidate unit absent")?;
        let deposit = matches!(
            u.purchase_graph,
            portfolio_risk_native::product_units::GraphRequest::Deposit(_)
        );
        if (deposit && p.kind != "deposit")
            || (!deposit && !matches!(p.kind.as_str(), "loan" | "security"))
        {
            return Err("mixed candidate direction/type mismatch".into());
        }
    }
    for p in &backbook.spec.positions {
        if p.rate != 0.
            || p.rollover != 0.
            || p.maturity_day != 0
            || p.watch_fraction != 0.
            || p.pledged_face != 0.
            || !p.collateral_pool.is_empty()
            || !matches!(p.classification.as_str(), "ac" | "htm")
        {
            return Err(
                "mixed backbook performing supplied dated scope requires no generic overlays"
                    .into(),
            );
        }
    }
    for event in backbook.store.events() {
        if event.measure == CashflowMeasure::Pricing || event.components.fees != 0. {
            return Err("mixed backbook pricing/default/fee accounting unsupported".into());
        }
    }
    Ok((combined, backbook))
}
pub(super) fn retirement(c: &CompiledDatedLedger) -> Result<(), String> {
    for ri in 0..c.store.runs().len() {
        portfolio_compute_control::checkpoint()?;
        let opening = DatedRunState::new(c, ri)?;
        let run = &c.store.runs()[ri];
        let mut principal: Vec<_> = c
            .owner_positions
            .iter()
            .map(|i| c.spec.positions[*i].balance)
            .collect();
        let mut basis: Vec<_> = c
            .owner_positions
            .iter()
            .map(|i| c.spec.positions[*i].book_adjustment)
            .collect();
        for e in c
            .store
            .events()
            .iter()
            .filter(|e| e.run_index.is_none_or(|r| r == ri))
        {
            portfolio_compute_control::checkpoint()?;
            let end = c.store.rights_at(
                &run.run_id,
                e.index,
                c.store.reporting_end_ordinal(),
                portfolio_ledger_native::dated_store::Boundary::AfterEvents,
            )?;
            if !end.cash_settled || !end.accrual_complete {
                return Err("mixed complete backbook tail not retired".into());
            }
            near(end.signed_interest_right, 0.)?;
            let oi = e.owner_index;
            let sign = if c.store.owners()[oi].direction
                == portfolio_ledger_native::dated_store::Direction::Receipt
            {
                1.
            } else {
                -1.
            };
            principal[oi] -= e.components.principal.abs();
            basis[oi] += sign * (end.book_amortization_to_date - opening.events[e.index].book);
        }
        for p in principal {
            near(p, 0.)?;
        }
        for b in basis {
            near(b, 0.)?;
        }
    }
    Ok(())
}
