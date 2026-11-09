//! Independent source coverage for ongoing cashflows; no producer types or validators.
use crate::expect::{Execution, Plan};
use serde_json::Value;
use std::collections::BTreeSet;

pub fn check(plan: &Plan, executions: &[Execution], contracts: &[Value]) -> Result<(), String> {
    for execution in executions {
        for id in execution
            .derivative_updates
            .iter()
            .map(|u| &u.lot_id)
            .collect::<BTreeSet<_>>()
        {
            let lot = plan
                .lots
                .iter()
                .find(|l| &l.lot_id == id)
                .ok_or("ongoing source lot missing")?;
            let sources: Vec<_> = contracts
                .iter()
                .filter(|c| c["lot_id"].as_str() == Some(id))
                .map(|c| &c["swap"])
                .collect();
            let first = sources.first().ok_or("ongoing source contract missing")?;
            let mut dates = BTreeSet::new();
            for s in &sources {
                if !dates.insert(s["asof_ordinal"].as_i64().ok_or("ongoing source date")?) {
                    return Err("duplicate ongoing source date".into());
                }
                for key in [
                    "contract",
                    "position_id",
                    "currency",
                    "monetary_unit",
                    "notional",
                    "side",
                    "fixed_rate",
                    "fixed_periods",
                    "floating_periods",
                    "tail_end_ordinal",
                    "index_basis",
                    "rate_path",
                ] {
                    if s[key] != first[key] {
                        return Err(format!("ongoing immutable terms differ: {key}"));
                    }
                }
                if s["payment_timing"] != "after_scheduled_payments" {
                    return Err("ongoing payment timing differs".into());
                }
            }
            let start = plan.as_of_ordinal.max(lot.acquisition_ordinal);
            let last = execution.horizon_end_ordinal.min(
                first["tail_end_ordinal"]
                    .as_i64()
                    .ok_or("ongoing tail missing")?,
            );
            let updates: BTreeSet<_> = execution
                .derivative_updates
                .iter()
                .filter(|u| &u.lot_id == id)
                .map(|u| u.ordinal)
                .collect();
            if last > start && !updates.contains(&last) {
                return Err("ongoing final mark missing".into());
            }
            for key in ["fixed_periods", "floating_periods"] {
                for period in first[key].as_array().ok_or("ongoing periods missing")? {
                    let date = period["payment_ordinal"]
                        .as_i64()
                        .ok_or("ongoing coupon date missing")?;
                    if date > start && date <= last && !updates.contains(&date) {
                        return Err("ongoing coupon coverage missing".into());
                    }
                }
            }
        }
        for review in &execution.variation_margin_reviews {
            let agreement = plan
                .margin_agreements
                .iter()
                .find(|a| a.agreement_id == review.agreement_id)
                .ok_or("VM agreement missing")?;
            let settlement = review
                .ordinal
                .checked_add(agreement.settlement_lag_days)
                .ok_or("independent VM settlement date overflow")?;
            for t in execution
                .terminations
                .iter()
                .filter(|t| t.agreement_id == review.agreement_id)
            {
                let collateral_end = t
                    .settlement_ordinal
                    .checked_add(if agreement.vm_at_termination == "return_separately" {
                        agreement.settlement_lag_days
                    } else {
                        0
                    })
                    .ok_or("independent termination collateral date overflow")?;
                if t.effective_ordinal <= settlement && collateral_end >= review.ordinal {
                    return Err(
                        "independent VM overlaps pending termination collateral settlement".into(),
                    );
                }
            }
            if [
                review.terms.receive_threshold,
                review.terms.post_threshold,
                review.terms.minimum_transfer,
            ]
            .iter()
            .any(|x| !x.is_finite() || *x < 0.)
                || review.ordinal <= plan.as_of_ordinal
                || agreement.settlement_lag_days < 0
                || settlement > execution.horizon_end_ordinal
                || execution.variation_margin_reviews.iter().any(|r| {
                    r.agreement_id == review.agreement_id
                        && r.ordinal > review.ordinal
                        && r.ordinal <= settlement
                })
                || execution.terminations.iter().any(|t| {
                    t.agreement_id == review.agreement_id
                        && t.effective_ordinal > review.ordinal
                        && t.effective_ordinal <= settlement
                })
                || execution.hedge_entries.iter().any(|h| {
                    h.agreement_id == review.agreement_id
                        && h.entry_ordinal <= settlement
                        && h.entry_ordinal
                            .checked_add(agreement.settlement_lag_days)
                            .is_none_or(|entry_settlement| entry_settlement >= review.ordinal)
                })
            {
                return Err("independent VM terms or pending settlement overlap".into());
            }
        }
    }
    Ok(())
}
