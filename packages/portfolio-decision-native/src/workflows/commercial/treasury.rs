//! Reviewed commercial closing capital and actual-day funding from native raw
//! commercial owners. Placeholder legacy exposure tables are never inputs.
use crate::workflows::commercial::credit as commercial_ledger;
use portfolio_model_core::{cashflow, modeled_product_funding as funding, strict_json};
use portfolio_risk_native::{treasury, treasury_bridge};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "commercial-treasury-1";
const MAX_BYTES: usize = 32 << 20;
const MAX_BRIDGE_WORK: usize = 2_000_000;
type Key = (String, String, String, String);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub raw_commercial: Value,
    pub capital: Option<Capital>,
    pub ftp: Option<Ftp>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capital {
    pub treasury: treasury::Request,
    pub policies: Vec<treasury_bridge::LedgerPolicy>,
    pub amount_multiplier: f64,
    pub review: funding::Identity,
    pub exposure_policy: treasury_bridge::ExposurePolicy,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ftp {
    pub as_of: String,
    pub policy_review: funding::Identity,
    pub curves: Vec<treasury::Curve>,
    pub reviews: Vec<Review>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub scenario: String,
    pub position_id: String,
    pub path_key: u64,
    pub entity: String,
    pub currency: String,
    pub policy: funding::Policy,
    pub cash_exclusion_review: funding::Identity,
    pub channel_change_review: funding::Identity,
    pub income_review: funding::Identity,
    pub resets: Vec<funding::Reset>,
    pub curve_id: String,
    pub option_spread: f64,
    pub contingent_annual_rate: f64,
    pub daily_operating_cost: f64,
    pub allocated_capital: f64,
    pub annual_cost_of_capital: f64,
}
fn tick() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        Err("explicit bounded commercial treasury identity required".into())
    } else {
        Ok(())
    }
}
fn review(r: &funding::Identity) -> Result<(), String> {
    label(&r.id)?;
    label(&r.revision)
}
fn num(v: &Value, k: &str) -> Result<f64, String> {
    v[k].as_f64()
        .filter(|x| x.is_finite())
        .ok_or(format!("finite commercial treasury field {k}"))
}
fn rows<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>, String> {
    v[k].as_array()
        .ok_or(format!("commercial treasury array {k}"))
}
fn text(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .map(String::from)
        .ok_or(format!("commercial treasury string {k}"))
}
fn key(v: &Value) -> Result<Key, String> {
    Ok((
        text(v, "scenario")?,
        text(v, "account")?,
        text(v, "gl_account")?,
        text(v, "instrument_id")?,
    ))
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if (a - b).abs() > 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs())) {
        Err("commercial treasury source/journal reconciliation differs".into())
    } else {
        Ok(())
    }
}
fn ordinal(year: f64, start: i32) -> Result<i32, String> {
    let days = year * 365.;
    let rounded = days.round();
    let mut n = if year.to_bits() == (rounded / 365.).to_bits() {
        rounded
    } else {
        days.ceil()
    };
    if year > n / 365. {
        n += 1.;
    }
    if !n.is_finite() || !(0. ..=1080.).contains(&n) {
        return Err("commercial treasury clock bound".into());
    }
    start
        .checked_add(n as i32)
        .ok_or("commercial treasury clock overflow".into())
}

struct Ledger {
    messages: Vec<Value>,
    trial: Vec<Value>,
    closing: Vec<Value>,
    daily: Vec<Value>,
    journal_rows: usize,
    work: usize,
}
fn ledger(stream: &[u8]) -> Result<Ledger, String> {
    let mut out = Ledger {
        messages: vec![],
        trial: vec![],
        closing: vec![],
        daily: vec![],
        journal_rows: 0,
        work: 0,
    };
    let mut dictionary = Vec::<String>::new();
    let mut balances = BTreeMap::<Key, f64>::new();
    let mut current = None;
    let mut txsum = 0_f64;
    let mut txabs = 0_f64;
    let mut previous = BTreeMap::<String, (i64, i64)>::new();
    let mut complete = false;
    for line in stream.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        tick()?;
        if complete {
            return Err("commercial treasury output after completion".into());
        }
        let value: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        if value.get("complete").is_some() {
            if value != json!({"complete":true,"protocol":6}) || !value["protocol"].is_u64() {
                return Err("actual commercial protocol6 completion required".into());
            }
            near(txsum, 0.)?;
            complete = true;
            out.messages.push(value);
            continue;
        }
        let table = value["table"]
            .as_str()
            .ok_or("commercial ledger table missing")?;
        if table == "journal" {
            for s in rows(&value, "dictionary")? {
                dictionary.push(
                    s.as_str()
                        .ok_or("commercial journal dictionary label")?
                        .into(),
                )
            }
            let columns = &value["columns"];
            let values = rows(columns, "value")?;
            for (i, v) in values.iter().enumerate() {
                tick()?;
                out.work = out.work.checked_add(16).ok_or("treasury work overflow")?;
                let name = |k: &str| -> Result<String, String> {
                    let n = columns[k][i]
                        .as_u64()
                        .ok_or("commercial journal integer label index")?;
                    dictionary
                        .get(n as usize)
                        .cloned()
                        .ok_or("commercial journal label index".into())
                };
                let scenario = name("scenario")?;
                let account = name("account")?;
                let gl = name("gl_account")?;
                let instrument = name("instrument_id")?;
                let event = name("event")?;
                let tx = columns["transaction_number"][i]
                    .as_i64()
                    .ok_or("commercial journal integer transaction")?;
                let day = columns["day"][i]
                    .as_i64()
                    .ok_or("commercial journal integer day")?;
                let identity = (scenario.clone(), tx, day, account.clone(), event);
                if current.as_ref() != Some(&identity) {
                    if txsum.abs() > 1e-9 * txabs.max(1.) {
                        return Err("commercial treasury unbalanced transaction".into());
                    }
                    let prior = previous.entry(scenario.clone()).or_insert((0, 0));
                    if tx != prior.0 + 1 || day < prior.1 {
                        return Err("commercial treasury transaction order".into());
                    }
                    *prior = (tx, day);
                    current = Some(identity);
                    txsum = 0.;
                    txabs = 0.;
                }
                let amount = v
                    .as_f64()
                    .filter(|x| x.is_finite())
                    .ok_or("commercial journal finite value")?;
                txsum += amount;
                txabs += amount.abs();
                *balances
                    .entry((scenario, account, gl, instrument))
                    .or_default() += amount;
                out.journal_rows += 1;
            }
        } else if ["trial_balance", "closing_statements", "commercial_daily"].contains(&table) {
            let r = rows(&value, "rows")?;
            out.work = out
                .work
                .checked_add(r.len() * 32)
                .ok_or("treasury work overflow")?;
            match table {
                "trial_balance" => out.trial.extend(r.iter().cloned()),
                "closing_statements" => out.closing.extend(r.iter().cloned()),
                _ => out.daily.extend(r.iter().cloned()),
            }
        }
        if out.work > MAX_BRIDGE_WORK || balances.len() > 10000 {
            return Err("commercial treasury separate bridge admission exceeded".into());
        }
        out.messages.push(value);
    }
    if !complete || out.trial.len() != balances.len() {
        return Err("complete commercial treasury journal/trial required".into());
    }
    let mut seen = BTreeSet::new();
    for row in &out.trial {
        tick()?;
        let k = key(row)?;
        if !seen.insert(k.clone()) {
            return Err("duplicate commercial treasury trial key".into());
        }
        near(
            num(row, "balance")?,
            *balances
                .get(&k)
                .ok_or("commercial journal/trial key missing")?,
        )?;
    }
    Ok(out)
}

fn state(snapshot: &Value, day: i32, undrawn: f64) -> Result<funding::State, String> {
    let default = snapshot["in_default"]
        .as_bool()
        .ok_or("commercial default flag")?;
    let mut s = funding::State {
        ordinal: day,
        source_checkpoint_id: format!("commercial:daily:{day}"),
        channels: funding::LANES
            .iter()
            .map(|c| funding::Amount {
                channel: (*c).into(),
                amount: 0.,
            })
            .collect(),
        origins: vec![],
    };
    let mut add = |lane: usize, id: String, event: String, amount: f64| {
        s.channels[lane].amount += amount;
        s.origins.push(funding::Origin {
            origin_id: id,
            source_event_id: event,
            channel: funding::LANES[lane].into(),
            amount,
        });
    };
    add(
        if default { 3 } else { 0 },
        "principal".into(),
        "principal-owner".into(),
        num(snapshot, "principal")?,
    );
    add(
        if default { 10 } else { 8 },
        "principal-basis".into(),
        "original-yield-owner".into(),
        num(snapshot, "book_adjustment")?,
    );
    add(16, "undrawn".into(), "commitment-owner".into(), undrawn);
    if snapshot["reviewed_credit_state"].is_object() {
        let reviewed = &snapshot["reviewed_credit_state"];
        add(
            12,
            "reviewed-legal-principal".into(),
            "legal-claim-owner".into(),
            num(reviewed, "legal_principal")?,
        );
        add(
            17,
            "reviewed-allowance".into(),
            "reviewed-contra-owner".into(),
            num(reviewed, "allowance")?,
        );
    }
    for lot in rows(snapshot, "lots")? {
        let amount = num(lot, "recognized_remaining")?;
        let is_default = lot["default_claim"]
            .as_bool()
            .ok_or("commercial lot default flag")?;
        let lane = if is_default {
            if amount >= 0. {
                6
            } else {
                7
            }
        } else if amount >= 0. {
            4
        } else {
            5
        };
        let id = format!(
            "{}:{}",
            text(lot, "coupon_id")?,
            lot["earning_origin_index"]
        );
        add(lane, id, text(lot, "source_event_id")?, amount.abs());
    }
    // Absolute individual legal rights keep gross obligations instead of netting
    // opposite coupons. Signed legal authority remains in the retained source.
    if let Some(canonical) = snapshot["canonical_legal_coupons"].as_array() {
        for c in canonical {
            let a = num(c, "legal_remaining")?;
            let id = format!(
                "canonical-legal:{}:{}",
                c["native_audit_index"],
                if a >= 0. { "receivable" } else { "payable" }
            );
            add(13, id, text(c, "coupon_id")?, a.abs());
        }
    } else {
        for l in rows(snapshot, "lots")? {
            let a = num(l, "legal_remaining")?;
            let id = format!(
                "origin-legal:{}:{}:{}",
                l["coupon_id"],
                l["earning_origin_index"],
                if a >= 0. { "receivable" } else { "payable" }
            );
            add(13, id, text(l, "source_event_id")?, a.abs());
        }
    }
    Ok(s)
}
fn funding_balance(s: &funding::State, p: &funding::Policy) -> f64 {
    s.channels
        .iter()
        .zip(&p.channels)
        .map(|(a, p)| a.amount * f64::from(p.coefficient))
        .sum()
}
fn date(day: i32) -> Result<String, String> {
    let (y, m, d) = cashflow::ymd_from_ordinal(day)?;
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}
#[derive(Serialize)]
struct FundingPath {
    scenario: String,
    position_id: String,
    path_key: u64,
    preparation: funding::ResultData,
    opening_cash: f64,
}

pub fn evaluate(r: Request) -> Result<Value, String> {
    tick()?;
    if r.schema != SCHEMA || r.capital.is_none() && r.ftp.is_none() {
        return Err("explicit commercial treasury selection required".into());
    }
    let raw: commercial_ledger::Request =
        serde_json::from_value(r.raw_commercial.clone()).map_err(|e| e.to_string())?;
    if raw.commercial.is_empty() {
        return Err("commercial treasury source owners required".into());
    }
    let preflight = portfolio_ledger_native::spec::normalize(raw.spec.clone(), 512, 8_000_000)?;
    let preflight_start = raw.commercial[0].source.current.asof_ordinal;
    let mut preflight_end = preflight_start
        .checked_add(preflight.horizon_days as i32)
        .ok_or("commercial treasury date overflow")?;
    let mut bridge_work_bound = 0usize;
    for source in &raw.commercial {
        let admission = portfolio_risk_native::commercial_credit_compiler::admit(source)?;
        preflight_end =
            preflight_end.max(ordinal(admission.full_credit_end_year, preflight_start)?);
        let postings = admission
            .lots_per_path_upper
            .checked_mul(8)
            .and_then(|n| n.checked_add(16))
            .ok_or("commercial treasury admission overflow")?;
        bridge_work_bound = bridge_work_bound
            .checked_add(
                admission
                    .carry_points_upper
                    .checked_mul(postings)
                    .and_then(|n| n.checked_mul(48))
                    .ok_or("commercial treasury admission overflow")?,
            )
            .ok_or("commercial treasury admission overflow")?;
    }
    let daily_rows = raw
        .participations
        .len()
        .checked_mul((preflight_end - preflight_start + 1) as usize)
        .ok_or("commercial treasury admission overflow")?;
    let closing_rows = preflight
        .accounts
        .len()
        .checked_mul(preflight.scenarios.len() + 1)
        .ok_or("commercial treasury admission overflow")?;
    bridge_work_bound = bridge_work_bound
        .checked_add(
            daily_rows
                .checked_add(closing_rows)
                .and_then(|n| n.checked_mul(32))
                .ok_or("commercial treasury admission overflow")?,
        )
        .ok_or("commercial treasury admission overflow")?;
    if bridge_work_bound > MAX_BRIDGE_WORK {
        return Err(
            "commercial treasury separate bridge upper exceeds 2M before source generation".into(),
        );
    }
    if r.capital
        .as_ref()
        .is_some_and(|c| c.treasury.as_of != date(preflight_end).unwrap_or_default())
    {
        return Err("commercial capital date must equal complete native tail".into());
    }
    if let Some(f) = &r.ftp {
        if preflight_end - preflight_start > 64 || f.as_of != date(preflight_start)? {
            return Err(
                "commercial daily FTP requires acquired-date asof and <=64 complete days".into(),
            );
        }
        treasury::Request {
            as_of: f.as_of.clone(),
            policy_id: f.policy_review.id.clone(),
            capital: vec![],
            curves: f.curves.clone(),
            positions: vec![],
            strategy_units: vec![],
            capital_deltas: vec![],
        }
        .run()?;
        if f.reviews.len() != raw.participations.len() {
            return Err("complete commercial FTP path review coverage required".into());
        }
    }
    if let Some(c) = &r.capital {
        review(&c.review)?;
        if c.policies
            .iter()
            .any(|p| p.capital.period != date(preflight_end).unwrap_or_default())
        {
            return Err("commercial capital period must equal complete closing date".into());
        }
        treasury_bridge::validate_reviewed_commercial_inputs(
            &c.treasury,
            &c.policies,
            c.amount_multiplier,
            &c.exposure_policy,
        )?;
    }
    if let Some(f) = &r.ftp {
        label(&f.as_of)?;
        review(&f.policy_review)?;
        if f.reviews.is_empty() || f.reviews.len() > 16 {
            return Err("commercial FTP <=16 complete path reviews required".into());
        }
        let mut seen = BTreeSet::new();
        for p in &f.reviews {
            review(&p.cash_exclusion_review)?;
            review(&p.channel_change_review)?;
            review(&p.income_review)?;
            for v in [
                &p.scenario,
                &p.position_id,
                &p.entity,
                &p.currency,
                &p.curve_id,
            ] {
                label(v)?
            }
            if !seen.insert((&p.scenario, &p.position_id, p.path_key)) {
                return Err("duplicate commercial FTP path review".into());
            }
            funding::validate_policy_before_source(
                &p.policy,
                &p.resets,
                preflight_start,
                preflight_end,
            )?;
            if p.option_spread > 1. || p.annual_cost_of_capital > 1. {
                return Err("commercial FTP option/capital rate above unity".into());
            }
            for v in [
                p.option_spread,
                p.contingent_annual_rate,
                p.daily_operating_cost,
                p.allocated_capital,
                p.annual_cost_of_capital,
            ] {
                if !v.is_finite() || !(0. ..=10000.).contains(&v) {
                    return Err("commercial FTP reviewed cost/rate domain".into());
                }
            }
        }
    }
    let (mut receipt, stream) = commercial_ledger::evaluate(&raw)?;
    receipt.original_request = r.raw_commercial.clone();
    let ledger = ledger(&stream)?;
    if ledger.work > bridge_work_bound {
        return Err("commercial treasury observed bridge work exceeds admitted bound".into());
    }
    let spec = &receipt.resolved_specification;
    let start = raw.commercial[0].source.current.asof_ordinal;
    let horizon = spec["horizon_days"]
        .as_u64()
        .ok_or("commercial resolved horizon")?;
    let end = start
        .checked_add(horizon as i32)
        .ok_or("commercial treasury full date overflow")?;
    let mut capital = Value::Null;
    if let Some(c) = r.capital {
        if c.treasury.as_of != date(end)? {
            return Err("commercial capital date must equal complete native tail".into());
        }
        for policy in &c.policies {
            let a = rows(spec, "accounts")?
                .iter()
                .find(|a| a["id"] == policy.account)
                .ok_or("commercial capital account missing")?;
            if policy.capital.entity != text(a, "entity")?
                || policy.capital.currency != text(a, "currency")?
                || policy.capital.scenario != policy.scenario
            {
                return Err("commercial capital account/scenario identity mismatch".into());
            }
        }
        let mut trial = vec![];
        for row in &ledger.trial {
            let gl = text(row, "gl_account")?;
            if gl.starts_with("memo:") {
                if ![
                    "memo:commercial_legal_coupon_right",
                    "memo:commercial_legal_coupon_contra",
                    "memo:commercial_legal_principal",
                    "memo:commercial_legal_principal_contra",
                ]
                .contains(&gl.as_str())
                {
                    return Err("unreviewed commercial memo channel".into());
                }
                continue;
            }
            trial.push(serde_json::from_value(row.clone()).map_err(|e| e.to_string())?)
        }
        capital = treasury_bridge::prepare_reviewed_commercial_exposure(
            c.treasury,
            trial,
            c.policies,
            c.amount_multiplier,
            &c.exposure_policy,
        )?;
        let bridge = rows(&capital, "capital_bridge")?;
        if bridge.len() != ledger.closing.len() {
            return Err("commercial complete closing capital scope mismatch".into());
        }
        for close in &ledger.closing {
            let b = bridge
                .iter()
                .find(|b| b["scenario"] == close["scenario"] && b["account"] == close["account"])
                .ok_or("commercial closing capital scope absent")?;
            for (a, bk) in [
                ("assets", "assets"),
                ("liabilities", "liabilities"),
                ("book_equity", "equity"),
            ] {
                near(num(b, a)?, num(close, bk)? * c.amount_multiplier)?
            }
        }
        capital["commercial_closing_review"] = json!(c.review);
    }
    let mut prepared_paths = vec![];
    let mut ftp = Value::Null;
    if let Some(f) = r.ftp {
        if horizon > 64 {
            return Err("commercial daily FTP complete source <=64 days required".into());
        }
        if f.as_of != date(start)? {
            return Err("commercial FTP asof must equal acquired source date".into());
        }
        let mut positions = vec![];
        let mut used = BTreeSet::new();
        for participation in &raw.participations {
            tick()?;
            let p = f
                .reviews
                .iter()
                .find(|p| {
                    p.scenario == participation.scenario
                        && p.position_id == participation.position_id
                        && p.path_key == participation.path_key
                })
                .ok_or("complete commercial FTP review coverage required")?;
            used.insert((&p.scenario, &p.position_id, p.path_key));
            let (si, source) = raw
                .commercial
                .iter()
                .enumerate()
                .find(|(_, s)| {
                    s.source.current.scenario_id == p.scenario
                        && s.source.current.position_id == p.position_id
                })
                .ok_or("commercial FTP raw source missing")?;
            let native = serde_json::to_value(&receipt.sources[si]).map_err(|e| e.to_string())?;
            let position = rows(spec, "positions")?
                .iter()
                .find(|a| a["id"] == p.position_id)
                .ok_or("commercial FTP position absent")?;
            let account = rows(spec, "accounts")?
                .iter()
                .find(|a| a["id"] == position["account"])
                .ok_or("commercial FTP account absent")?;
            if p.entity != text(account, "entity")? || p.currency != text(account, "currency")? {
                return Err("commercial FTP raw entity/currency identity mismatch".into());
            }
            let mut states = vec![];
            let mut undrawns = vec![];
            for row in ledger.daily.iter().filter(|x| {
                x["scenario"] == p.scenario
                    && x["position_id"] == p.position_id
                    && x["path_key"] == p.path_key
            }) {
                let day = row["day"].as_i64().ok_or("commercial daily integer day")? as i32;
                let year = f64::from(day) / 365.;
                let u = rows(&native["carry"], "carry_points")?
                    .iter()
                    .rev()
                    .find(|point| {
                        point["path_key"] == p.path_key
                            && num(point, "year").is_ok_and(|t| t <= year)
                    })
                    .and_then(|point| point["undrawn_commitment"].as_f64())
                    .ok_or("commercial daily actual commitment checkpoint missing")?;
                undrawns.push(u);
                states.push(state(&row["unweighted_book"], start + day, u)?);
            }
            let allocation = raw
                .allocations
                .iter()
                .find(|a| a.position_id == p.position_id)
                .ok_or("commercial FTP allocation missing")?
                .allocated_face;
            let face = source.source.current.opening_principal;
            let mut cash = vec![];
            let mut earned = vec![];
            let mut opening_cash = 0.;
            for (gi, g) in rows(&native["accounting"], "groups")?
                .iter()
                .enumerate()
                .filter(|(_, g)| g["path_key"] == p.path_key)
            {
                let d = ordinal(num(g, "year")?, start)?;
                let id = format!("commercial-group:{gi}");
                let amount = num(g, "cash")?;
                if d == start {
                    for p in rows(g, "postings")? {
                        if matches!(
                            p["posting"]["account"].as_str(),
                            Some(
                                "interest_income"
                                    | "basis_recognition_expense"
                                    | "credit_loss_expense"
                                    | "recovery_income"
                            )
                        ) && num(&p["posting"], "amount")? != 0.
                        {
                            return Err("opening commercial P&L requires separate FTP opening-earnings ownership".into());
                        }
                    }
                    opening_cash += amount
                } else {
                    cash.push(funding::Cash {
                        event_id: id.clone(),
                        ordinal: d,
                        kind: "source_atomic_cash".into(),
                        bank_cash: amount,
                        funding_change_sequence: None,
                        exclusion_review: Some(p.cash_exclusion_review.clone()),
                    });
                    for (channel, account) in [
                        ("original_eir", "interest_income"),
                        ("reviewed_basis", "basis_recognition_expense"),
                        ("incurred_credit", "credit_loss_expense"),
                        ("recovery_income", "recovery_income"),
                    ] {
                        let mut amount = 0.;
                        for p in rows(g, "postings")? {
                            if p["posting"]["account"].as_str() == Some(account) {
                                amount -= num(&p["posting"], "amount")?;
                            }
                        }
                        if amount != 0. {
                            earned.push(funding::Earned {
                                event_id: format!("{id}:{channel}"),
                                ordinal: d,
                                channel: channel.into(),
                                bank_earnings: amount,
                                review: p.income_review.clone(),
                            });
                        }
                    }
                }
            }
            let changes = states
                .windows(2)
                .enumerate()
                .map(|(i, s)| funding::Change {
                    sequence: (i + 1) as u32,
                    ordinal: s[1].ordinal,
                    source_event_id: s[1].source_checkpoint_id.clone(),
                    kind: "reviewed_other_reallocation".into(),
                    amount: funding_balance(&s[1], &p.policy) - funding_balance(&s[0], &p.policy),
                    cash_event_id: None,
                    review: p.channel_change_review.clone(),
                })
                .collect();
            let request = funding::Request {
                schema: funding::SCHEMA.into(),
                measure: "physical".into(),
                source: funding::Identity {
                    id: source.source.current.contract.id.clone(),
                    revision: source.source.current.contract.revision.clone(),
                },
                original_yield_anchor: funding::Identity {
                    id: source.source.acquisition.original_yield_anchor.id.clone(),
                    revision: source
                        .source
                        .acquisition
                        .original_yield_anchor
                        .revision
                        .clone(),
                },
                path_id: source
                    .credit
                    .paths
                    .iter()
                    .find(|x| x.path_key == p.path_key)
                    .ok_or("commercial FTP path identity")?
                    .path_id
                    .clone(),
                entity: p.entity.clone(),
                currency_unit: p.currency.clone(),
                scenario_id: p.scenario.clone(),
                side: "asset".into(),
                start_ordinal: start,
                full_end_ordinal: end,
                source_weight: participation.source_weight,
                unit_face: face,
                allocated_face: allocation,
                opening_funding_balance: funding_balance(
                    states.first().ok_or("commercial FTP opening missing")?,
                    &p.policy,
                ),
                policy: p.policy.clone(),
                states,
                cash,
                changes,
                earned,
                resets: p.resets.clone(),
            };
            let preparation = funding::prepare_with_check(&request, &mut tick)?;
            for (i, d) in preparation.days.iter().enumerate() {
                let other = d
                    .earnings_by_channel
                    .iter()
                    .filter(|a| a.channel != "original_eir")
                    .map(|a| a.amount)
                    .sum();
                let avg_u = if p.policy.averaging == "opening_daily" {
                    undrawns[i]
                } else {
                    (undrawns[i] + undrawns[i + 1]) * 0.5
                };
                positions.push(treasury::Position {
                    id: format!(
                        "{}:{}:{}:{}",
                        p.scenario, p.position_id, p.path_key, d.ordinal
                    ),
                    loan_id: Some(p.position_id.clone()),
                    cohort_id: None,
                    entity: p.entity.clone(),
                    currency: p.currency.clone(),
                    scenario: p.scenario.clone(),
                    period: date(d.ordinal)?,
                    curve_id: p.curve_id.clone(),
                    side: "asset".into(),
                    average_balance: d.scaled_average_funding_balance,
                    accrual_fraction: d.accrual_fraction,
                    repricing_years: d.reference_years,
                    funding_years: d.liquidity_years,
                    external_interest: d.original_eir,
                    fees: other,
                    operating_cost: p.daily_operating_cost * preparation.scale,
                    expected_loss: 0.,
                    allocated_capital: p.allocated_capital * preparation.scale,
                    cost_of_capital: p.annual_cost_of_capital,
                    option_spread: p.option_spread,
                    contingent_liquidity_charge: avg_u
                        * preparation.scale
                        * p.contingent_annual_rate
                        * d.accrual_fraction,
                });
            }
            prepared_paths.push(FundingPath {
                scenario: p.scenario.clone(),
                position_id: p.position_id.clone(),
                path_key: p.path_key,
                preparation,
                opening_cash,
            });
        }
        if used.len() != f.reviews.len() {
            return Err("unused commercial FTP reviews".into());
        }
        let request = treasury::Request {
            as_of: f.as_of,
            policy_id: f.policy_review.id,
            capital: vec![],
            curves: f.curves,
            positions,
            strategy_units: vec![],
            capital_deltas: vec![],
        };
        ftp = request.run()?;
        ftp["prepared_specification"] = serde_json::to_value(request).map_err(|e| e.to_string())?;
    }
    let value = json!({"schema":SCHEMA,"raw_request":{"schema":SCHEMA,"raw_commercial":r.raw_commercial},"source_receipt":receipt,"ledger_messages":ledger.messages,"capital":capital,"ftp":ftp,"funding_paths":prepared_paths,"full_end_ordinal":end,"native_ordered_journal_replayed":true,"journal_rows":ledger.journal_rows,"bridge_work_units":ledger.work,"bridge_work_bound":bridge_work_bound,"bridge_work_limit":MAX_BRIDGE_WORK,"stream_decode_bytes":stream.len(),"closing_capital_selected":!capital.is_null(),"daily_ftp_selected":!ftp.is_null(),"economic_loss_deducted_again":false,"independent_persisted_verification":false,"empirical_validation":false,"regulatory_credit_rwa_approval":false,"accounting_standard_approval":false,"production_adoption":false,"scope":"reviewed complete-tail capital and acquired Physical daily funding; signed recognized obligations and gross individual legal rights retained; supplied policies and day-end/trapezoid conventions; realized paths are not an expected-flow forecast"});
    strict_json::serialized_size(&value, MAX_BYTES)?;
    tick()?;
    Ok(value)
}

pub fn run_envelope(input: &[u8]) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Value,
    }
    let started = std::time::Instant::now();
    let value = strict_json::parse_checked(input, 2 << 20, tick)?;
    let envelope: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if envelope.op != "analyze_commercial_treasury" || envelope.schema != SCHEMA {
        return Err("explicit commercial treasury operation required".into());
    }
    let raw = envelope.request.clone();
    let request: Request = serde_json::from_value(envelope.request).map_err(|e| e.to_string())?;
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(envelope.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(
            envelope.execution_timeout_ms,
            started,
        )?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut result = evaluate(request)?;
        result["raw_request"] = raw;
        tick()?;
        Ok(result)
    })
}
