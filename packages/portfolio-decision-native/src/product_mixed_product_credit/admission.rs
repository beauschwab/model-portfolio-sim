use super::*;
use serde_json::json;
use std::collections::BTreeSet;
pub(super) fn bounded<T: Serialize>(v: &T, cap: usize) -> Result<usize, String> {
    struct Count {
        n: usize,
        cap: usize,
    }
    impl Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
            self.n = self
                .n
                .checked_add(b.len())
                .ok_or_else(|| std::io::Error::other("product byte overflow"))?;
            if self.n > self.cap {
                return Err(std::io::Error::other("product complete byte bound"));
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Count { n: 0, cap };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.n)
}
pub(super) fn validate(r: &Request) -> Result<(product::Request, product::Admission, i32), String> {
    bounded(r, 32 * 1024 * 1024)?;
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || r.policy.ownership != product::OWNER_POLICY
        || r.policy.phase != product::DAY_PHASE
        || r.policy.tail != "complete_closed_claims-1"
        || r.policy.cash != "end_of_day_no_replacement_funding-1"
        || r.policy.initial_allowance != "explicit_reviewed_non_pcd_expense-1"
        || [
            &r.policy.generic_cashflows,
            &r.policy.generic_credit,
            &r.policy.generic_marks,
        ]
        .iter()
        .any(|s| s.as_str() != "reject")
    {
        return Err("mixed product reviewed exclusive policy required".into());
    }
    for s in [&r.policy.identity.id, &r.policy.identity.revision] {
        if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
            return Err("mixed product review identity domain".into());
        }
    }
    let combined =
        portfolio_ledger_native::spec::normalize(r.combined_specification.clone(), 512, 8_000_000)?;
    let original = portfolio_ledger_native::spec::normalize(
        r.performing.combined_specification.clone(),
        512,
        8_000_000,
    )?;
    let ids: BTreeSet<_> = r
        .product_owners
        .iter()
        .map(|o| o.position_id.as_str())
        .collect();
    if ids.len() != r.product_owners.len()
        || combined.positions.len() != original.positions.len() + ids.len()
    {
        return Err("mixed product complete exclusive inventory required".into());
    }
    let mut before = serde_json::to_value(&combined).map_err(|e| e.to_string())?;
    before["positions"] = serde_json::to_value(
        combined
            .positions
            .iter()
            .filter(|p| !ids.contains(p.id.as_str()))
            .collect::<Vec<_>>(),
    )
    .map_err(|e| e.to_string())?;
    if before != serde_json::to_value(&original).map_err(|e| e.to_string())? {
        return Err("mixed product actual accounts/original dated anchors differ".into());
    }
    let start = r.performing.backbook.dated.as_of_ordinal;
    let report = *r
        .performing
        .candidate
        .product
        .reporting_ordinals
        .last()
        .ok_or("mixed product report grid absent")?;
    let mut full = report;
    for owner in &r.product_owners {
        for run in &owner.runs {
            for path in &run.paths {
                let raw = path.request.view();
                full = full.max(
                    raw.contract
                        .as_of_ordinal
                        .checked_add(raw.contract.horizon_days as i32)
                        .ok_or("mixed product full tail overflow")?,
                );
            }
        }
    }
    if full
        .checked_sub(start)
        .is_none_or(|d| !(0..=1080).contains(&d))
    {
        return Err("mixed product tail beyond1080; no truncation".into());
    }
    let max_end = start
        .checked_add(1080)
        .ok_or("mixed product reservation date overflow")?;
    let mut preliminary = r.performing.backbook.clone();
    preliminary.specification = r.combined_specification.clone();
    preliminary.specification["horizon_days"] = json!(1080);
    preliminary.dated.reporting_horizon_days = 1080;
    for source in &mut preliminary.dated.sources {
        source.schedule.horizon_days = 1080;
    }
    let c = CompiledDatedLedger::compile(preliminary, 512, 8_000_000)?;
    let request = product::Request {
        schema: product::SCHEMA.into(),
        reporting_end_ordinal: report,
        owners: r.product_owners.clone(),
    };
    let reservation = product::admit(&request, &c)?;
    if reservation.full_end_ordinal != max_end {
        return Err("mixed product conservative reservation clock mismatch".into());
    }
    Ok((request, reservation, full))
}
