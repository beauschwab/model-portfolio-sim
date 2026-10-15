use super::*;

pub(super) fn deposit_path(
    raw: &deposit::Request,
    n: &Value,
    v: &Review,
) -> Result<PathData, String> {
    if v.owner_index != 0 || v.path_index != 0 {
        return fail("single native deposit owner/path");
    }
    let p = &array(&n["native"], "paths")?[v.run_index];
    let mut states = Vec::new();
    let start = raw.source.original.as_of_ordinal;
    let end = start + raw.source.original.horizon_days as i32;
    let opening = &p["income_request"];
    let mut s = state(start, "deposit:revision".into());
    origin(&mut s, "principal", 0, num(opening, "opening_principal")?);
    origin(
        &mut s,
        "basis",
        8,
        num(p, "revised_carrying_value")? - num(opening, "opening_principal")?,
    );
    states.push(s);
    let mut cash = Vec::new();
    let mut earned = Vec::new();
    for (i, row) in array(&p["income"], "rows")?.iter().enumerate() {
        let d = day(row, "payment_ordinal")?;
        if d == start {
            continue;
        }
        let mut s = state(d, format!("deposit:income:{i}"));
        origin(&mut s, "principal", 0, num(row, "closing_principal")?);
        right(
            &mut s,
            "coupon",
            num(row, "closing_accrued_interest")?,
            5,
            4,
        );
        origin(&mut s, "basis", 8, num(row, "closing_book_adjustment")?);
        for (ii, lot) in array(&p["source"], "interest")?.iter().enumerate() {
            if day(lot, "end_ordinal")? > d {
                continue;
            }
            let mut outstanding = num(lot, "amount")?;
            for co in array(&n["native"], "cash_origins")? {
                if co["run_id"] != p["run_id"] {
                    continue;
                }
                let cash = array(&p["source"], "cashflows")?
                    .iter()
                    .find(|c| c["event_id"] == co["native_event_id"])
                    .ok_or("exact native FIFO cash origin missing")?;
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
            format!("deposit:income:{i}"),
            "original_eir".into(),
            -num(row, "effective_interest")?,
        ));
    }
    for (i, c) in array(&p["source"], "cashflows")?.iter().enumerate() {
        cash.push((
            day(c, "ordinal")?,
            format!("deposit:cash:{i}:{}", c["event_id"]),
            num(c, "principal")? + num(c, "cash_interest")?,
        ));
    }
    // Day0 revision is retained explicitly in PathData's source receipt, not moved into recurring EIR.
    earned.push((
        start,
        "deposit:day0-revision".into(),
        "deposit:revision".into(),
        num(p, "basis_revision")?,
    ));
    Ok(PathData {
        source: identity(&serde_json::to_value(&raw.source.contract).unwrap())?,
        anchor: identity(&serde_json::to_value(&raw.source.anchor.original_yield_anchor).unwrap())?,
        path: "deposit".into(),
        scenario: p["scenario_id"].as_str().ok_or("scenario")?.into(),
        currency: raw.source.original.identity.currency.clone(),
        side: "liability".into(),
        start,
        end,
        weight: 1.,
        face: raw.source.anchor.original_principal,
        allocated: raw.source.anchor.original_principal,
        states,
        cash,
        earned,
    })
}
