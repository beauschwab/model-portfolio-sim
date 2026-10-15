use super::*;
/// Economic rights are independent of book reversal/writeoff and original G.
pub(super) fn original_economics(
    r: &Request,
    source: &boundary_receipt::ResultData,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(Vec<InterestCash>, Vec<EconomicCloseout>, i32), String> {
    let projection = &source.native_projection;
    let links: BTreeMap<_, _> = source
        .capture
        .cash_links
        .iter()
        .map(|e| (e.entitlement_id.as_str(), e.cash_event_id.as_str()))
        .collect();
    let cash: BTreeMap<_, _> = projection
        .cashflows
        .iter()
        .map(|e| (e.event_id.as_str(), e))
        .collect();
    if links.len() != projection.entitlements.len() || cash.len() != projection.cashflows.len() {
        return Err("original same-loop cash cardinality failed".into());
    }
    let mut full = r.original_contract.as_of_ordinal + r.original_contract.horizon_days as i32;
    for e in &projection.entitlements {
        check()?;
        let actual = cash
            .get(
                links
                    .get(e.event_id.as_str())
                    .ok_or("original cash link absent")?,
            )
            .ok_or("original cash absent")?;
        if e.entitlement_ordinal != e.remittance_ordinal
            || actual.ordinal != e.remittance_ordinal
            || actual.principal != e.principal
            || actual.interest != e.interest
            || actual.kind != e.kind
        {
            return Err("original entitled and actual zero-float cash differs".into());
        }
        full = full.max(actual.ordinal);
    }
    let mut interest_cash = Vec::new();
    let mut closeouts = Vec::new();
    for boundary in &source.capture.boundaries {
        check()?;
        for (offset, claim) in projection.defaults
            [boundary.default_claim_start..boundary.default_claim_end]
            .iter()
            .enumerate()
        {
            check()?;
            let claim_index = boundary.default_claim_start + offset;
            let origin = claim.origin;
            let interest =
                claim.borrower_contract_interest * r.policy.interest.collection_fraction[origin];
            let mut actual_interest = 0.;
            let mut interest_ids = Vec::new();
            for (n, t) in r.policy.interest.complete_collection_tails[origin]
                .iter()
                .enumerate()
            {
                check()?;
                let ordinal = date(claim.default_ordinal, t.lag_days)?;
                let id = format!("{}:reviewed-interest:{n}", claim.claim_id);
                let value = interest * t.weight;
                actual_interest += value;
                interest_cash.push(InterestCash {
                    cash_id: id.clone(),
                    claim_id: claim.claim_id.clone(),
                    ordinal,
                    amount: value,
                    review: r.policy.interest.review.clone(),
                });
                interest_ids.push(id);
                full = full.max(ordinal);
            }
            let recoveries: Vec<_> = boundary
                .default_recoveries
                .iter()
                .filter(|e| e.claim_index == claim_index)
                .collect();
            if recoveries.len() != claim.complete_recovery_tail.len() {
                return Err("original same-loop recovery cardinality failed".into());
            }
            let mut principal_ids = Vec::new();
            let mut recovered = 0.;
            for (n, (native, captured)) in claim
                .complete_recovery_tail
                .iter()
                .zip(recoveries)
                .enumerate()
            {
                check()?;
                if captured.tail_index != n
                    || captured.origin != origin
                    || captured.ordinal != native.ordinal
                    || captured.principal != native.principal
                {
                    return Err("original same-loop recovery receipt differs".into());
                }
                recovered += native.principal;
                full = full.max(native.ordinal);
                if let Some(id) = &captured.entitlement_id {
                    let cash_id = links
                        .get(id.as_str())
                        .ok_or("original recovery cash link absent")?;
                    let actual = cash.get(cash_id).ok_or("original recovery cash absent")?;
                    if actual.ordinal != native.ordinal
                        || actual.principal != native.principal
                        || actual.interest != 0.
                        || actual.kind != "whole_loan_default_recovery"
                    {
                        return Err("original recovery cash ownership differs".into());
                    }
                    principal_ids.push((*cash_id).to_owned());
                } else if native.principal != 0. {
                    return Err("original nonzero recovery has no cash identity".into());
                }
            }
            let pdisposal = claim.borrower_principal - recovered;
            let idisposal = claim.borrower_contract_interest - actual_interest;
            if pdisposal < 0. || idisposal < 0. {
                return Err(
                    "original economic waiver cannot dispose overcollected gross claims".into(),
                );
            }
            let pdate = date(
                claim.default_ordinal,
                r.policy.economic_principal_disposal_lag_days,
            )?;
            let idate = date(claim.default_ordinal, r.policy.interest.disposal_lag_days)?;
            full = full.max(pdate).max(idate);
            closeouts.push(EconomicCloseout {
                claim_id: claim.claim_id.clone(),
                origin,
                gross_principal: claim.borrower_principal,
                gross_contract_interest: claim.borrower_contract_interest,
                principal_cash: recovered,
                interest_cash: actual_interest,
                principal_economic_disposal: pdisposal,
                principal_disposal_ordinal: pdate,
                principal_disposal_review: r.policy.economic_principal_disposal_review.clone(),
                interest_economic_disposal: idisposal,
                interest_disposal_ordinal: idate,
                interest_disposal_review: r
                    .policy
                    .interest
                    .economic_disposal_review
                    .clone()
                    .unwrap(),
                native_principal_cash_ids: principal_ids,
                reviewed_interest_cash_ids: interest_ids,
                closing_economic_principal: claim.borrower_principal - recovered - pdisposal,
                closing_economic_interest: claim.borrower_contract_interest
                    - actual_interest
                    - idisposal,
            });
        }
    }
    Ok((interest_cash, closeouts, full))
}
