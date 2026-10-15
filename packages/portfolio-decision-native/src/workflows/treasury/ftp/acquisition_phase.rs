//! Successor repair: native owned acquisition cash and initial allowance are separate from FTP.
use super::*;
use serde_json::json;
fn n(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|v| v.is_finite())
        .ok_or("finite native acquisition amount".into())
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("owned acquisition/journal amount mismatch".into())
    } else {
        Ok(())
    }
}
fn journal(
    native: &Value,
    wanted: &BTreeMap<u64, BTreeSet<(String, String)>>,
    work: &mut usize,
) -> Result<Vec<Value>, String> {
    if let Some(rows) = native["journal"].as_array() {
        *work = add(*work, rows.len() * 32)?;
        let mut selected = vec![];
        for (i, r) in rows.iter().enumerate() {
            if i % 64 == 0 {
                portfolio_compute_control::checkpoint()?;
            }
            if wanted
                .get(&r["transaction_number"].as_u64().unwrap_or(u64::MAX))
                .is_some_and(|scopes| {
                    scopes.contains(&(
                        r["scenario"].as_str().unwrap_or("").into(),
                        r["account"].as_str().unwrap_or("").into(),
                    ))
                })
            {
                selected.push(r.clone());
            }
        }
        return Ok(selected);
    }
    let mut rows = vec![];
    let mut dictionary = vec![];
    for m in native["ledger_messages"]
        .as_array()
        .ok_or("actual native journal stream")?
    {
        portfolio_compute_control::checkpoint()?;
        if m["table"] != "journal" {
            continue;
        }
        for s in m["dictionary"].as_array().ok_or("journal dictionary")? {
            let s = s.as_str().ok_or("dictionary string")?;
            *work = add(*work, s.len() / 8 + 1)?;
            dictionary.push(s);
        }
        let c = &m["columns"];
        let values = c["value"].as_array().ok_or("journal values")?;
        *work = add(*work, values.len() * 32)?;
        for i in 0..values.len() {
            if i % 64 == 0 {
                portfolio_compute_control::checkpoint()?;
            }
            let tx = c["transaction_number"][i]
                .as_u64()
                .ok_or("transaction integer")?;
            let scope = |k: &str| -> Result<&str, String> {
                let index = c[k][i].as_u64().ok_or("exact index")? as usize;
                dictionary.get(index).copied().ok_or("dictionary".into())
            };
            if !wanted.get(&tx).is_some_and(|scopes| {
                scopes.contains(&(
                    scope("scenario").unwrap_or("").into(),
                    scope("account").unwrap_or("").into(),
                ))
            }) {
                continue;
            }
            let mut r = serde_json::Map::new();
            for k in ["transaction_number", "day", "value"] {
                r.insert(k.into(), c[k][i].clone());
            }
            for k in [
                "scenario",
                "account",
                "event",
                "gl_account",
                "instrument_id",
            ] {
                let index = c[k][i].as_u64().ok_or("exact journal dictionary index")? as usize;
                r.insert(
                    k.into(),
                    Value::String(
                        dictionary
                            .get(index)
                            .ok_or("dictionary coverage")?
                            .to_string(),
                    ),
                );
            }
            rows.push(Value::Object(r));
        }
    }
    Ok(rows)
}
pub fn derive(r: &Request, prepared: &source::ResultData) -> Result<(Vec<Value>, usize), String> {
    let mut result = vec![];
    let mut work = 0;
    for s in &r.source.sources {
        let native = &prepared
            .sources
            .iter()
            .find(|x| x.source_id == s.source_id)
            .ok_or("source receipt")?
            .raw_native_result;
        // Source work is additionally reserved for this ordered phase scan before
        // generation. Actual row count is checked before entering the decode loop.
        let upper = prepared
            .sources
            .iter()
            .find(|x| x.source_id == s.source_id)
            .unwrap()
            .work_bound;
        let count = if let Some(a) = native["journal"].as_array() {
            a.len()
        } else {
            native["ledger_messages"]
                .as_array()
                .ok_or("source messages")?
                .iter()
                .filter(|m| m["table"] == "journal")
                .map(|m| m["columns"]["value"].as_array().map_or(0, Vec::len))
                .sum()
        };
        if count.checked_mul(32).is_none_or(|x| x > upper) {
            return Err("owned acquisition phase scan exceeds reserved source work".into());
        }
        let start_work = work;
        let mut wanted: BTreeMap<u64, BTreeSet<(String, String)>> = BTreeMap::new();
        let metadata = if let Some(a) = native["product_acquisition_lineage"].as_array() {
            a.iter().collect::<Vec<_>>()
        } else {
            native["ledger_messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["table"] == "mortgage_lineage")
                .flat_map(|m| m["rows"].as_array().into_iter().flatten())
                .filter(|x| x["native_group_index"] == 0)
                .collect()
        };
        for a in metadata {
            let p = prepared
                .paths
                .iter()
                .find(|p| {
                    p.source_id == s.source_id
                        && a["run_id"] == p.ledger_run_id
                        && a["position_id"] == p.position_id
                        && a["path_id"] == p.preparation.raw_request.path_id
                })
                .ok_or("acquisition owner coverage")?;
            for field in [
                "transaction_number",
                "purchase_transaction_number",
                "allowance_transaction_number",
            ] {
                if let Some(tx) = a[field].as_u64() {
                    wanted.entry(tx).or_default().insert((
                        p.preparation.raw_request.scenario_id.clone(),
                        p.account_id.clone(),
                    ));
                }
            }
        }
        let rows = journal(native, &wanted, &mut work)?;
        for p in prepared.paths.iter().filter(|p| p.source_id == s.source_id) {
            portfolio_compute_control::checkpoint()?;
            work = add(work, 128)?;
            let key = binding(p);
            let q = &p.preparation.raw_request;
            let scale = p.preparation.scale;
            let (acquisition, cost, allowance, receipt, kind) = match &s.input {
                source::SourceInput::Product(raw) => {
                    let o = &raw.product_owners[p.owner_index];
                    let run = &o.runs[p.run_index];
                    let path = &run.paths[p.path_index];
                    let value = serde_json::to_value(&path.request).map_err(|e| e.to_string())?;
                    let acq = if value["kind"] == "installment" {
                        value["request"]["acquisition"].clone()
                    } else {
                        native["product_acquisition_lineage"]
                            .as_array()
                            .ok_or("product acquisition")?
                            .iter()
                            .find(|x| {
                                x["run_id"] == p.ledger_run_id
                                    && x["position_id"] == p.position_id
                                    && x["path_id"] == q.path_id
                            })
                            .ok_or("exact acquired path")?["acquisition"]
                            .clone()
                    };
                    let receipt = native["product_acquisition_lineage"]
                        .as_array()
                        .ok_or("product acquisition")?
                        .iter()
                        .find(|x| {
                            x["run_id"] == p.ledger_run_id
                                && x["position_id"] == p.position_id
                                && x["path_id"] == q.path_id
                        })
                        .ok_or("exact acquired path")?
                        .clone();
                    let allowance = serde_json::to_value(&o.initial_allowance_review)
                        .map_err(|e| e.to_string())?;
                    (
                        acq.clone(),
                        n(&acq["cost"])?,
                        if allowance.is_null() {
                            0.0
                        } else {
                            n(&allowance["amount"])?
                        },
                        receipt,
                        "actual_owned_acquisition-1",
                    )
                }
                source::SourceInput::Mortgage(raw) => {
                    let o = &raw.owners[p.owner_index];
                    let path = &o.runs[p.run_index].paths[p.path_index];
                    let a = serde_json::to_value(
                        &raw.sources
                            .iter()
                            .find(|x| x.source_id == path.source_id)
                            .ok_or("registry")?
                            .request
                            .source
                            .acquisition,
                    )
                    .map_err(|e| e.to_string())?;
                    let receipt = native["ledger_messages"]
                        .as_array()
                        .ok_or("messages")?
                        .iter()
                        .filter(|m| m["table"] == "mortgage_lineage")
                        .flat_map(|m| m["rows"].as_array().into_iter().flatten())
                        .find(|x| {
                            x["run_id"] == p.ledger_run_id
                                && x["position_id"] == p.position_id
                                && x["path_id"] == q.path_id
                                && x["native_group_index"] == 0
                        })
                        .ok_or("mortgage acquisition lineage")?
                        .clone();
                    (
                        a.clone(),
                        n(&a["actual_cost"])?,
                        0.0,
                        receipt,
                        "actual_owned_acquisition-1",
                    )
                }
                source::SourceInput::Deposit(_)
                | source::SourceInput::DepositContinuation(_)
                | source::SourceInput::DepositPortfolio(_) => (
                    Value::Null,
                    0.0,
                    0.0,
                    Value::Null,
                    "existing_opening_anchor_no_new_acquisition-1",
                ),
            };
            let scenario = &q.scenario_id;
            let purchase_tx = if matches!(s.input, source::SourceInput::Mortgage(_)) {
                receipt["transaction_number"].clone()
            } else {
                receipt["purchase_transaction_number"].clone()
            };
            let allowance_tx = receipt["allowance_transaction_number"].clone();
            let selected = |tx: &Value| -> Vec<Value> {
                if tx.is_null() {
                    vec![]
                } else {
                    rows.iter()
                        .filter(|j| {
                            j["scenario"] == *scenario
                                && j["account"] == p.account_id
                                && j["transaction_number"] == *tx
                        })
                        .cloned()
                        .collect()
                }
            };
            let purchase = selected(&purchase_tx);
            let provision = selected(&allowance_tx);
            let bank_cash = -cost * scale;
            let initial = -allowance * scale;
            near(
                purchase
                    .iter()
                    .filter(|j| j["gl_account"] == "cash")
                    .map(|j| n(&j["value"]))
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .sum(),
                bank_cash,
            )?;
            near(
                -provision
                    .iter()
                    .filter(|j| j["gl_account"] == "pnl:consumer_credit_loss_expense")
                    .map(|j| n(&j["value"]))
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .sum::<f64>(),
                initial,
            )?;
            if !receipt.is_null()
                && (receipt["ordinal"] != q.start_ordinal
                    || receipt["source_weight"] != q.source_weight
                    || receipt["allocation_factor"] != q.allocated_face / q.unit_face)
            {
                return Err("exact native acquisition mapping required".into());
            }
            let mut opening = p.opening_earnings.clone();
            opening
                .iter_mut()
                .find(|x| x.channel == "initial_allowance")
                .ok_or("seven lanes")?
                .amount += initial;
            result.push(json!({"binding":key,"kind":kind,"ordinal":q.start_ordinal,"position_id":p.position_id,"account_id":p.account_id,"ledger_run_id":p.ledger_run_id,"scenario":q.scenario_id,"native_path_id":q.path_id,"entity":q.entity,"currency":q.currency_unit,"source_weight":q.source_weight,"allocation_factor":q.allocated_face/q.unit_face,"unweighted_acquisition":acquisition,"purchase_cash":bank_cash,"initial_allowance_expense":initial,"complete_opening_source_cash":p.opening_source_cash+bank_cash,"complete_opening_earnings":opening,"native_acquisition_receipt":receipt,"purchase_postings":purchase,"initial_allowance_postings":provision,"cash_excluded_from_funding_rollforward":true,"initial_allowance_excluded_from_recurring_external_interest":true}));
        }
        if work - start_work > upper {
            return Err("acquisition scan exceeds reserved work".into());
        }
    }
    Ok((result, work))
}
