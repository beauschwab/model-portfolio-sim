//! Actual source-owned principal plus original-EIR income; no purchase cash invention.
use super::*;

pub(super) fn deposit_continuation_path(
    raw: &continuation::Request,
    n: &Value,
    v: &Review,
) -> Result<PathData, String> {
    if v.owner_index != 0 || v.path_index != 0 {
        return fail("single continuation deposit owner/path");
    }
    let run = raw.runs.get(v.run_index).ok_or("continuation run")?;
    let receipt = &array(&n["native"], "paths")?[v.run_index];
    let path = &receipt["native"]["current"];
    let r = &run.source.continuation;
    let start = r.original.as_of_ordinal;
    let end = start
        .checked_add(r.original.horizon_days as i32)
        .ok_or("source clock overflow")?;
    let opening = &path["income_request"];
    let mut states = Vec::new();
    let mut first = state(start, "deposit:original-opening".into());
    origin(
        &mut first,
        "principal",
        0,
        num(opening, "opening_principal")?,
    );
    origin(
        &mut first,
        "basis",
        8,
        num(opening, "opening_gross_carrying_value")? - num(opening, "opening_principal")?,
    );
    states.push(first);
    let mut earned = Vec::new();
    for (i, row) in array(&path["income"], "rows")?.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let d = day(row, "payment_ordinal")?;
        let mut s = state(d, format!("deposit:source-owned-income:{i}"));
        origin(&mut s, "principal", 0, num(row, "closing_principal")?);
        right(
            &mut s,
            "coupon",
            num(row, "closing_accrued_interest")?,
            5,
            4,
        );
        origin(&mut s, "basis", 8, num(row, "closing_book_adjustment")?);
        // Each complete earned origin stays visible independently of pooled accounting rights.
        for (ii, lot) in array(&path["source"], "interest")?.iter().enumerate() {
            if day(lot, "end_ordinal")? > d {
                continue;
            }
            let mut outstanding = num(lot, "amount")?;
            for co in array(&n["native"], "cash_origins")? {
                if co["run_id"] != run.run_id {
                    continue;
                }
                let cash = array(&path["source"], "cashflows")?
                    .iter()
                    .find(|c| c["event_id"] == co["native_event_id"])
                    .ok_or("exact continuation FIFO cash origin")?;
                if day(cash, "ordinal")? > d {
                    continue;
                }
                for allocation in array(co, "allocations")? {
                    if allocation["native_interest_index"] == ii {
                        outstanding -= num(allocation, "amount")?;
                    }
                }
            }
            origin(
                &mut s,
                &format!("earned-origin:{ii}:{}", lot["segment_id"]),
                13,
                outstanding,
            );
        }
        states.push(s);
        earned.push((
            d,
            format!("deposit:source-owned-income:{i}"),
            "original_eir".into(),
            -num(row, "effective_interest")?,
        ));
    }
    let cash = array(&path["source"], "cashflows")?
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Ok((
                day(c, "ordinal")?,
                format!("deposit:cash:{i}:{}", c["event_id"]),
                num(c, "principal")? + num(c, "cash_interest")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    // Revision occurs after actual cash/source reclassification; it never enters recurring EIR.
    earned.push((
        r.revision_ordinal,
        "deposit:later-revision".into(),
        "deposit:revision".into(),
        num(&receipt["native"], "basis_revision")?,
    ));
    Ok(PathData {
        source: identity(&serde_json::to_value(&r.contract).map_err(|e| e.to_string())?)?,
        anchor: identity(
            &serde_json::to_value(&r.anchor.original_yield_anchor).map_err(|e| e.to_string())?,
        )?,
        path: "deposit".into(),
        scenario: run.scenario_id.clone(),
        currency: r.original.identity.currency.clone(),
        side: "liability".into(),
        start,
        end,
        weight: 1.,
        face: r.anchor.original_principal,
        allocated: r.anchor.original_principal,
        states,
        cash,
        earned,
    })
}
