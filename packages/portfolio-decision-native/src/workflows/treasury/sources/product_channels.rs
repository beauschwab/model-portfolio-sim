use super::*;

pub(super) fn product_path(
    raw: &product::Request,
    n: &Value,
    v: &Review,
) -> Result<PathData, String> {
    let o = raw.product_owners.get(v.owner_index).ok_or("owner index")?;
    let run = o.runs.get(v.run_index).ok_or("run index")?;
    let p = run.paths.get(v.path_index).ok_or("path index")?;
    let x = array(n, "product_sources")?
        .iter()
        .find(|x| {
            x["owner_index"] == v.owner_index
                && x["run_index"] == v.run_index
                && x["path_index"] == v.path_index
        })
        .ok_or("native product source")?;
    let kind = x["native"]["kind"].as_str().ok_or("native product tag")?;
    let native = &x["native"]["result"];
    if !matches!(kind, "installment" | "card_v2") {
        return fail("versioned source adapter admits installment/card_v2 only");
    }
    let raw_native = &native["raw_request"];
    let start = day(&raw_native["contract"], "as_of_ordinal")?;
    let end = start
        + raw_native["contract"]["horizon_days"]
            .as_u64()
            .ok_or("horizon")? as i32;
    let mut states = Vec::new();
    let mut cash = Vec::new();
    let mut earned = Vec::new();
    if kind == "card_v2" {
        for b in array(native, "daily")? {
            let d = day(b, "ordinal")?;
            let snap = &b["snapshot"];
            let mut s = state(d, format!("card_v2:daily:{d}"));
            shared(&mut s, &snap["core"])?;
            origin(&mut s, "live-Bi", 9, num(snap, "live_interest_basis")?);
            for (k, x) in snap["claim_interest_basis"]
                .as_object()
                .ok_or("claim basis map")?
            {
                origin(&mut s, k, 11, number(x)?);
            }
            states.push(s);
        }
        for (ei, e) in array(native, "events")?.iter().enumerate() {
            let d = day(e, "ordinal")?;
            for (li, line) in array(e, "lines")?.iter().enumerate() {
                let a = line["account"].as_str().ok_or("owned account")?;
                let amount = num(line, "amount")?;
                let id = format!("card_v2:event:{ei}:line:{li}");
                if a == "cash" {
                    cash.push((d, id, amount))
                } else if a == "pnl:original_effective_interest" {
                    earned.push((d, id, "original_eir".into(), -amount))
                } else if a.starts_with("pnl:")
                    || matches!(
                        a,
                        "interest_income"
                            | "credit_loss_expense"
                            | "basis_recognition_expense"
                            | "recovery_income"
                    )
                {
                    earned.push((d, id, format!("card_v2:{a}"), -amount))
                }
            }
        }
    } else {
        let mut opening = state(start, "installment:acquisition".into());
        shared(&mut opening, &native["acquisition_opening"])?;
        states.push(opening);
        for b in array(native, "daily")? {
            let d = day(b, "ordinal")?;
            if d == start {
                continue;
            }
            let mut s = state(d, format!("installment:daily:{d}"));
            shared(&mut s, &b["owned"])?;
            states.push(s);
        }
        for (ei, e) in array(native, "intents")?.iter().enumerate() {
            let d = day(e, "recognition_ordinal")?;
            let event = e["event"].as_str().ok_or("intent event")?;
            for (li, line) in array(e, "postings")?.iter().enumerate() {
                let a = line["account"].as_str().ok_or("intent account")?;
                let amount = num(line, "amount")?;
                let id = format!("installment:intent:{ei}:line:{li}");
                if a == "cash" {
                    cash.push((d, id, amount))
                } else if a == "interest_income" && event == "product_original_effective_interest" {
                    earned.push((d, id, "original_eir".into(), -amount))
                } else if matches!(
                    a,
                    "interest_income"
                        | "credit_loss_expense"
                        | "basis_recognition_expense"
                        | "recovery_income"
                ) {
                    earned.push((d, id, format!("installment:{event}:{a}"), -amount))
                }
            }
        }
    }
    Ok(PathData {
        source: identity(&serde_json::to_value(&o.source_identity).unwrap())?,
        anchor: identity(&serde_json::to_value(&o.original_yield_anchor).unwrap())?,
        path: p.path_id.clone(),
        scenario: run.scenario_id.clone(),
        currency: raw_native["contract"]["identity"]["currency"]
            .as_str()
            .ok_or("currency")?
            .into(),
        side: "asset".into(),
        start,
        end,
        weight: p.source_weight,
        face: o.unit_face,
        allocated: o.allocated_face,
        states,
        cash,
        earned,
    })
}
