//! Orchestration: raw identity, independent valuations, independent replay per scenario and
//! account, and comparison with the lineage rows, the ordered journal, the closing stocks and the
//! producer's receipt. A receipt is returned only when every check passes.
use crate::{expect, near, raw, stream, swap, SCHEMA};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub verified: bool,
    pub schema: &'static str,
    pub plan_sha256: String,
    pub scenarios: Vec<String>,
    pub intents_verified: usize,
    pub transactions_verified: usize,
    pub closing_stocks_verified: usize,
    pub valuations_verified: usize,
    pub stream_lines: usize,
    pub independent_equations: bool,
    pub production_financial_crates: bool,
    pub methodology_certified: bool,
    pub disclosure: &'static str,
}

fn f(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("receipt field `{key}` is not finite"))
}
fn s<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("receipt field `{key}` missing"))
}
fn money(what: &str, actual: f64, expected: f64) -> Result<(), String> {
    if near(actual, expected) {
        Ok(())
    } else {
        Err(format!(
            "{what}: producer {actual} differs from independent {expected}"
        ))
    }
}

pub fn verify(request_bytes: &[u8], stream_bytes: &[u8]) -> Result<Receipt, String> {
    // Raw identity and typed normalization before any financial comparison.
    let request = raw::parse(request_bytes, crate::MAX_REQUEST_BYTES)?;
    if request.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Err("raw request does not select liquidity-lifecycle-1".into());
    }
    let plan_value = request.get("plan").ok_or("raw request lacks its plan")?;
    let plan_bytes = serde_json::to_vec(plan_value).map_err(|e| e.to_string())?;
    let plan_sha256 = raw::sha256_hex(&plan_bytes);
    let stream = stream::parse(stream_bytes)?;
    if stream.header.get("model_schema").and_then(Value::as_str) != Some(SCHEMA) {
        return Err("stream header does not name liquidity-lifecycle-1".into());
    }
    let receipt = stream
        .header
        .get("source_receipt")
        .ok_or("stream header lacks the source receipt")?;
    if s(receipt, "plan_sha256")? != plan_sha256 {
        return Err("producer plan hash differs from the raw plan bytes".into());
    }
    if receipt.get("formal_hedge_accounting") != Some(&Value::Bool(false)) {
        return Err("formal hedge accounting is not a capability of this journal".into());
    }
    let plan = expect::plan_from(plan_value)?;
    let executions = expect::executions_from(
        request
            .get("acquisitions")
            .ok_or("raw request lacks execution blocks")?,
    )?;
    // Independent valuation of every raw contract the producer priced, and every date it must have.
    let contracts = request
        .get("hedge_contracts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut valuations: HashMap<(String, i64), expect::Valuation> = HashMap::new();
    let entries = receipt
        .get("hedge_entries")
        .and_then(Value::as_array)
        .ok_or("receipt lacks hedge valuations")?;
    for v in entries {
        let lot_id = s(v, "lot_id")?.to_string();
        let ordinal = v
            .get("ordinal")
            .and_then(Value::as_i64)
            .ok_or("valuation ordinal")?;
        let clean = f(v, "clean_value")?;
        let due = f(v, "due_cash")?;
        let contract = contracts
            .iter()
            .find(|c| {
                c.get("lot_id").and_then(Value::as_str) == Some(&lot_id)
                    && c.get("swap")
                        .and_then(|s| s.get("asof_ordinal"))
                        .and_then(Value::as_i64)
                        == Some(ordinal)
            })
            .ok_or_else(|| format!("no raw swap contract for `{lot_id}` at {ordinal}"))?;
        let swap_value = contract.get("swap").ok_or("contract lacks its swap")?;
        let lot = plan
            .lots
            .iter()
            .find(|l| l.lot_id == lot_id)
            .ok_or("valuation names unknown lot")?;
        if swap_value.get("position_id").and_then(Value::as_str) != Some(lot.position_id.as_str()) {
            return Err(format!(
                "swap contract for `{lot_id}` prices another position"
            ));
        }
        swap::check(swap_value, clean, due)?;
        if valuations
            .insert(
                (lot_id, ordinal),
                expect::Valuation {
                    clean_value: clean,
                    due_cash: due,
                },
            )
            .is_some()
        {
            return Err("duplicate valuation in the receipt".into());
        }
    }
    for x in &executions {
        for (lot, ordinal) in x
            .hedge_entries
            .iter()
            .map(|h| (&h.lot_id, h.entry_ordinal))
            .chain(
                x.terminations
                    .iter()
                    .map(|t| (&t.lot_id, t.effective_ordinal)),
            )
        {
            if !valuations.contains_key(&(lot.clone(), ordinal)) {
                return Err(format!(
                    "receipt lacks the valuation of `{lot}` at {ordinal}"
                ));
            }
        }
    }
    // Typed identity of the opening cash: bits, not tolerance.
    let raw_accounts = plan_value
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("plan accounts")?;
    let journals = receipt
        .get("journals")
        .and_then(Value::as_array)
        .ok_or("receipt lacks journals")?;
    let (mut intents_verified, mut transactions_verified, mut stocks_verified) = (0, 0, 0);
    let mut scenarios = Vec::new();
    for x in &executions {
        let scenario = stream.scenarios.get(&x.scenario_id).ok_or_else(|| {
            format!(
                "stream carries no lifecycle rows for scenario `{}`",
                x.scenario_id
            )
        })?;
        if scenario.plan_key != plan_sha256 {
            return Err("lifecycle rows name another plan".into());
        }
        scenarios.push(x.scenario_id.clone());
        for account in &plan.accounts {
            if account.account_id != scenario.account {
                return Err("stage D verifies one lifecycle account per scenario".into());
            }
            let raw_account = raw_accounts
                .iter()
                .find(|a| {
                    a.get("account_id").and_then(Value::as_str) == Some(account.account_id.as_str())
                })
                .ok_or("raw account")?;
            let journal = journals
                .iter()
                .find(|j| {
                    j.get("account_id").and_then(Value::as_str) == Some(account.account_id.as_str())
                        && j.get("scenario_id").and_then(Value::as_str)
                            == Some(x.scenario_id.as_str())
                })
                .ok_or_else(|| {
                    format!(
                        "receipt lacks the journal of `{}` in `{}`",
                        account.account_id, x.scenario_id
                    )
                })?;
            let r = journal.get("receipt").ok_or("journal receipt")?;
            let expected = expect::replay(&plan, x, &valuations, &account.account_id)?;
            if raw::bits(raw_account.get("cash").ok_or("raw cash")?, "opening cash")?
                != f(r, "opening_cash")?.to_bits()
                || expected.opening_cash_bits != f(r, "opening_cash")?.to_bits()
            {
                return Err(
                    "opening cash typed identity differs between the raw plan and the receipt"
                        .into(),
                );
            }
            // Ordered intents against the lineage rows and the custody memo.
            let (expected_financial, expected_custody): (Vec<_>, Vec<_>) =
                expected.intents.iter().partition(|i| !i.memo);
            for (label, want, got) in [
                ("lineage", &expected_financial, &scenario.lineage),
                ("custody", &expected_custody, &scenario.custody),
            ] {
                if want.len() != got.len() {
                    return Err(format!(
                        "{label}: producer posted {} intents, independent replay expects {}",
                        got.len(),
                        want.len()
                    ));
                }
                for (w, g) in want.iter().zip(got) {
                    if w.event_id != g.event_id
                        || w.ordinal != g.ordinal
                        || w.postings.len() != g.postings.len()
                    {
                        return Err(format!(
                            "{label}: intent `{}` differs from the independent `{}`",
                            g.event_id, w.event_id
                        ));
                    }
                    for ((gl, inst, debit), p) in w.postings.iter().zip(&g.postings) {
                        if *gl != p.gl || *inst != p.instrument || !near(*debit, p.debit) {
                            return Err(format!(
                                "{label}: posting of `{}` differs: {} {} {} vs {} {} {}",
                                g.event_id, p.gl, p.instrument, p.debit, gl, inst, debit
                            ));
                        }
                    }
                    intents_verified += 1;
                }
            }
            // The same postings, in the same order, as the journal's transactions.
            if scenario.transactions.len() != expected_financial.len() {
                return Err(format!(
                    "journal carries {} lifecycle transactions, independent replay expects {}",
                    scenario.transactions.len(),
                    expected_financial.len()
                ));
            }
            for (w, t) in expected_financial.iter().zip(&scenario.transactions) {
                if t.day != w.ordinal - plan.as_of_ordinal || t.postings.len() != w.postings.len() {
                    return Err(format!(
                        "journal transaction {} differs from independent `{}`",
                        t.number, w.event_id
                    ));
                }
                for ((gl, inst, debit), p) in w.postings.iter().zip(&t.postings) {
                    if *gl != p.gl || *inst != p.instrument || !near(*debit, p.debit) {
                        return Err(format!(
                            "journal transaction {} posting differs from independent `{}`",
                            t.number, w.event_id
                        ));
                    }
                }
                transactions_verified += 1;
            }
            // Closing stocks of every lifecycle instrument, including those that closed to zero.
            for ((gl, inst), value) in &expected.closing {
                if gl.starts_with("pnl:") {
                    continue;
                }
                let got = scenario
                    .trial_balance
                    .get(&(gl.clone(), inst.clone()))
                    .copied()
                    .unwrap_or(0.);
                money(&format!("closing {gl} {inst}"), got, *value)?;
                stocks_verified += 1;
            }
            for ((gl, inst), got) in &scenario.trial_balance {
                if gl != "cash"
                    && !expected.closing.contains_key(&(gl.clone(), inst.clone()))
                    && !near(*got, 0.)
                {
                    return Err(format!(
                        "closing {gl} {inst} is not owned by any independent posting"
                    ));
                }
            }
            // The producer's receipt.
            money("closing cash", f(r, "closing_cash")?, expected.closing_cash)?;
            money(
                "settled cash movements",
                f(r, "settled_cash_movements")?,
                expected.cash_movements,
            )?;
            money("posted VM", f(r, "posted_vm")?, expected.posted_vm)?;
            money("received VM", f(r, "received_vm")?, expected.received_vm)?;
            money("posted IM", f(r, "posted_im")?, expected.posted_im)?;
            money(
                "received IM custody",
                f(r, "received_im_custody")?,
                expected.received_im,
            )?;
            money("fees", f(r, "fees_expensed")?, expected.fees)?;
            money("realized", f(r, "realized")?, expected.realized)?;
            if r.get("open_claims") != Some(&Value::Bool(expected.open_claims)) {
                return Err("open claims flag differs from the independent replay".into());
            }
            let events = r
                .get("events")
                .and_then(Value::as_array)
                .ok_or("receipt events")?;
            if events.len() != expected.events.len() {
                return Err("event count differs from the independent replay".into());
            }
            for (e, (id, status)) in events.iter().zip(&expected.events) {
                if s(e, "event_id")? != id || s(e, "status")? != *status {
                    return Err(format!(
                        "event `{id}` status differs from the independent replay"
                    ));
                }
            }
            let steps = r
                .get("steps")
                .and_then(Value::as_array)
                .ok_or("receipt liquidity steps")?;
            if steps.len() != expected.steps.len() {
                return Err("liquidity step count differs from the independent replay".into());
            }
            for (got, want) in steps.iter().zip(&expected.steps) {
                let depends: Vec<&str> = got
                    .get("depends_on")
                    .and_then(Value::as_array)
                    .ok_or("step dependencies")?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                if s(got, "event_id")? != want.event_id
                    || got.get("ordinal").and_then(Value::as_i64) != Some(want.ordinal)
                    || got.get("phase").and_then(Value::as_u64) != Some(u64::from(want.phase))
                    || s(got, "status")? != want.status
                    || depends
                        != want
                            .depends_on
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                {
                    return Err(format!(
                        "liquidity step `{}` identity, order or status differs from the independent replay",
                        want.event_id
                    ));
                }
                for (field, value) in &want.amounts {
                    money(
                        &format!("liquidity step `{}` {field}", want.event_id),
                        f(got, field)?,
                        *value,
                    )?;
                }
            }
            let facts = r
                .get("accounting_facts")
                .and_then(Value::as_array)
                .ok_or("receipt facts")?;
            if facts.len() != expected.facts.len() {
                return Err("accounting fact count differs from the independent replay".into());
            }
            for (got, want) in facts.iter().zip(&expected.facts) {
                if s(got, "kind")? != want.kind || s(got, "lot_id")? != want.lot_id {
                    return Err("accounting fact identity differs".into());
                }
                for (key, value) in [
                    ("quantity", want.quantity),
                    ("retained_quantity", want.retained_quantity),
                    ("consideration", want.consideration),
                    ("removed_carrying", want.removed_carrying),
                    ("realized", want.realized),
                ] {
                    money(&format!("fact {} {key}", want.lot_id), f(got, key)?, value)?;
                }
                let ids = |key: &str| {
                    got.get(key)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                if ids("economic_allocation_ids") != want.economic_allocation_ids
                    || ids("formal_relationship_ids") != want.formal_relationship_ids
                {
                    return Err("accounting fact lineage differs".into());
                }
            }
        }
    }
    if scenarios.len() != stream.scenarios.len() {
        return Err(
            "stream carries lifecycle rows for a scenario without an execution block".into(),
        );
    }
    Ok(Receipt {
        verified: true,
        schema: SCHEMA,
        plan_sha256,
        scenarios,
        intents_verified,
        transactions_verified,
        closing_stocks_verified: stocks_verified,
        valuations_verified: valuations.len(),
        stream_lines: stream.lines,
        independent_equations: true,
        production_financial_crates: false,
        methodology_certified: false,
        disclosure: "Independent Rust verification of the raw lifecycle request and the native stream: strict raw bytes, plan hash and typed opening identity; swap valuations recomputed from the raw contracts; every lifecycle posting re-derived from the raw instructions and matched in order against the lineage rows, the journal transactions and the closing stocks; receipt amounts, statuses and accounting facts matched. Supplied IM is checked for scope and binding, not for its methodology. This is source and journal verification, not empirical, accounting-standard or regulatory acceptance.",
    })
}
