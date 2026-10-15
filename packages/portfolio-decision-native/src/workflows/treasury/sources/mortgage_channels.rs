use super::*;

pub(super) fn mortgage_path(
    raw: &mortgage::Request,
    n: &Value,
    v: &Review,
) -> Result<PathData, String> {
    let o = raw.owners.get(v.owner_index).ok_or("owner review index")?;
    let run = o.runs.get(v.run_index).ok_or("run review index")?;
    let p = run.paths.get(v.path_index).ok_or("path review index")?;
    let receipt = array(n, "sources")?
        .iter()
        .find(|x| x["source_id"] == p.source_id)
        .ok_or("registry native source missing")?;
    let mut states = Vec::new();
    let native = &receipt["native"];
    for b in array(native, "daily")? {
        let d = day(b, "ordinal")?;
        let mut s = state(d, format!("{}:daily:{d}", p.source_id));
        for (i, c) in array(b, "slots")?.iter().enumerate() {
            let k = format!("slot:{i}");
            origin(
                &mut s,
                &k,
                if i < 3 {
                    0
                } else if i < 6 {
                    1
                } else {
                    2
                },
                num(c, "principal")?,
            );
            right(&mut s, &k, num(c, "recognized_interest")?, 4, 5);
            origin(&mut s, &k, 8, num(c, "basis")?);
            origin(&mut s, &k, 12, number(&b["borrower_principal"][i])?);
            right(
                &mut s,
                &format!("{k}:borrower"),
                number(&b["borrower_contract_interest"][i])?,
                13,
                13,
            );
        }
        for c in array(b, "claims")? {
            let k = c["claim_id"].as_str().ok_or("claim id")?;
            origin(&mut s, k, 3, num(&c["book"], "principal")?);
            right(&mut s, k, num(&c["book"], "recognized_interest")?, 6, 7);
            origin(&mut s, k, 10, num(&c["book"], "basis")?);
            origin(&mut s, k, 14, num(c, "economic_principal")?);
            right(
                &mut s,
                &format!("{k}:economic"),
                num(c, "economic_contract_interest")?,
                15,
                15,
            );
        }
        states.push(s);
    }
    let mut cash = Vec::new();
    let mut earned = Vec::new();
    for (gi, g) in array(native, "groups")?.iter().enumerate() {
        let d = day(g, "ordinal")?;
        let k = g["kind"].as_str().ok_or("group kind")?;
        if k == "acquisition" {
            continue;
        }
        for (li, line) in array(g, "lines")?.iter().enumerate() {
            let a = line["account"].as_str().ok_or("group account")?;
            let amount = num(line, "amount")?;
            let id = format!("group:{gi}:line:{li}:{}", g["source_id"]);
            if a == "cash" {
                cash.push((d, id, amount))
            } else if a == "original_eir" {
                earned.push((d, id, "original_eir".into(), -amount))
            } else if a == "reviewed_income" {
                earned.push((
                    d,
                    id,
                    format!(
                        "mortgage:{}",
                        match line["source_action_index"].as_u64() {
                            Some(ai) => native["source"]["actions"][ai as usize]["kind"]
                                .as_str()
                                .ok_or("source action kind")?,
                            None => k,
                        }
                    ),
                    -amount,
                ))
            }
        }
    }
    let start = raw
        .sources
        .iter()
        .find(|r| r.source_id == p.source_id)
        .ok_or("registry raw missing")?
        .request
        .source
        .contract
        .as_of_ordinal;
    let end = states.last().ok_or("empty native daily")?.ordinal;
    Ok(PathData {
        source: identity(&serde_json::to_value(&o.source_identity).unwrap())?,
        anchor: identity(&serde_json::to_value(&o.original_yield_anchor).unwrap())?,
        path: p.path_id.clone(),
        scenario: run.scenario_id.clone(),
        currency: raw.dated.specification["accounts"]
            .as_array()
            .ok_or("source accounts")?
            .iter()
            .find(|a| a["id"] == o.account_id)
            .ok_or("source owner account")?["currency"]
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
