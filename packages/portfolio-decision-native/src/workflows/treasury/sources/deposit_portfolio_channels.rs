//! Source2 owns all native amounts. Funding only selects reviewed channel coefficients.
use super::*;

pub(super) fn portfolio_source<'a>(
    raw: &'a portfolio::Request,
    v: &Review,
) -> Result<(&'a portfolio::Run, &'a portfolio::PathRequest), String> {
    if v.path_index != 0 {
        return fail("one complete supplied source per owner/run; path_index must be zero");
    }
    let run = raw.runs.get(v.run_index).ok_or("portfolio run review")?;
    let owner = raw
        .positions
        .get(v.owner_index)
        .ok_or("portfolio owner review")?;
    let path = owner
        .paths
        .iter()
        .find(|p| p.run_id == run.run_id)
        .ok_or("explicit complete portfolio run source")?;
    Ok((run, path))
}

pub(super) fn rebuild_portfolio(raw: &portfolio::Request) -> Result<Value, String> {
    // Match the standalone selected Request byte decoder's allocation boundary.
    // Cloning Vec<T> may compact capacity, so its observed retained counters are
    // not identical to the independently completed standalone source receipt.
    let body = serde_json::to_vec(raw).map_err(|e| e.to_string())?;
    if body.len() > 1_048_576 {
        return fail("source2 typed byte envelope bound");
    }
    portfolio_compute_control::checkpoint()?;
    let selected: portfolio::Request = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()?;
    let prepared = portfolio::prepare(selected)?;
    let output = Arc::new(Mutex::new(Vec::new()));
    portfolio::run_public(
        &serde_json::to_value(raw).map_err(|e| e.to_string())?,
        &prepared,
        Box::new(Sink(output.clone())),
    )?;
    let bytes = output.lock().map_err(|_| "portfolio sink poisoned")?;
    let frames = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(serde_json::from_slice::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let header = frames.first().ok_or("complete portfolio header")?;
    Ok(
        json!({"native":header["workflow"]["source_receipt"],"prepared_request":header["workflow"]["prepared_request"],"ledger_messages":frames}),
    )
}

pub(super) fn deposit_portfolio_path(
    raw: &portfolio::Request,
    native: &Value,
    review: &Review,
) -> Result<PathData, String> {
    let (run, p) = portfolio_source(raw, review)?;
    let owner = &raw.positions[review.owner_index];
    let position_index = array(&native["prepared_request"]["specification"], "positions")?
        .iter()
        .position(|p| p["id"] == owner.position_id)
        .ok_or("actual source-owned position")?;
    let o = array(&native["native"], "owners")?
        .iter()
        .find(|o| o["position_index"] == position_index)
        .ok_or("native portfolio owner mapping")?;
    let receipt = array(o, "paths")?
        .iter()
        .find(|p| p["run_id"] == run.run_id)
        .ok_or("native complete run receipt")?;
    let current = &receipt["native"]["native"]["current"];
    let source = &current["source"];
    let c = &p.source.continuation;
    let start = c.original.as_of_ordinal;
    let end = start
        .checked_add(c.original.horizon_days as i32)
        .ok_or("source clock overflow")?;
    let opening = &current["income_request"];
    let mut states = Vec::with_capacity(add(c.original.horizon_days as usize, 1)?);
    let key = format!(
        "deposit-portfolio:{}:{}",
        review.owner_index, review.run_index
    );
    let mut first = state(start, format!("{key}:original-opening"));
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

    // Index the unchanged allocation order once. Every origin's binary64 subtraction
    // remains native CashOrigin order; never net or discard tiny/signed residuals.
    let interest = array(source, "interest")?;
    let cashflows = array(source, "cashflows")?;
    let cash_index = cashflows
        .iter()
        .map(|cash| {
            Ok((
                cash["event_id"].as_str().ok_or("native cash ID")?,
                day(cash, "ordinal")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut deductions = vec![Vec::<(i32, f64)>::new(); interest.len()];
    for cash in array(&native["native"], "cash_origins")? {
        portfolio_compute_control::checkpoint()?;
        if cash["position_id"] != owner.position_id || cash["run_id"] != run.run_id {
            continue;
        }
        let d = *cash_index
            .get(cash["native_event_id"].as_str().ok_or("cash origin ID")?)
            .ok_or("exact portfolio cash origin")?;
        for a in array(cash, "allocations")? {
            let index = a["native_interest_index"]
                .as_u64()
                .and_then(|x| usize::try_from(x).ok())
                .ok_or("exact origin index")?;
            let list = deductions.get_mut(index).ok_or("full FIFO origin bounds")?;
            if list.last().is_some_and(|(prior, _)| *prior > d) {
                return fail("native FIFO dates must retain source order");
            }
            list.push((d, num(a, "amount")?));
        }
    }
    let mut outstanding = interest
        .iter()
        .map(|i| num(i, "amount"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut cursors = vec![0usize; interest.len()];
    let mut earned = Vec::new();
    for (i, row) in array(&current["income"], "rows")?.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let d = day(row, "payment_ordinal")?;
        let mut s = state(d, format!("{key}:source-owned-income:{i}"));
        origin(&mut s, "principal", 0, num(row, "closing_principal")?);
        right(
            &mut s,
            "coupon",
            num(row, "closing_accrued_interest")?,
            5,
            4,
        );
        origin(&mut s, "basis", 8, num(row, "closing_book_adjustment")?);
        for (ii, lot) in interest.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            if day(lot, "end_ordinal")? > d {
                continue;
            }
            let list = &deductions[ii];
            while cursors[ii] < list.len() && list[cursors[ii]].0 <= d {
                outstanding[ii] -= list[cursors[ii]].1;
                cursors[ii] += 1;
            }
            origin(
                &mut s,
                &format!("earned-origin:{ii}:{}", lot["segment_id"]),
                13,
                outstanding[ii],
            );
        }
        states.push(s);
        earned.push((
            d,
            format!("{key}:source-owned-income:{i}"),
            "original_eir".into(),
            -num(row, "effective_interest")?,
        ));
    }
    let cash = cashflows
        .iter()
        .enumerate()
        .map(|(i, cash)| {
            Ok((
                day(cash, "ordinal")?,
                format!("{key}:cash:{i}:{}", cash["event_id"]),
                num(cash, "principal")? + num(cash, "cash_interest")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    earned.push((
        c.revision_ordinal,
        format!("{key}:later-revision"),
        "deposit:revision".into(),
        num(&receipt["native"]["native"], "basis_revision")?,
    ));
    Ok(PathData {
        source: identity(&serde_json::to_value(&c.contract).map_err(|e| e.to_string())?)?,
        anchor: identity(
            &serde_json::to_value(&c.anchor.original_yield_anchor).map_err(|e| e.to_string())?,
        )?,
        path: owner.position_id.clone(),
        scenario: run.scenario_id.clone(),
        currency: c.original.identity.currency.clone(),
        side: "liability".into(),
        start,
        end,
        weight: 1.,
        face: c.anchor.original_principal,
        allocated: c.anchor.original_principal,
        states,
        cash,
        earned,
    })
}
