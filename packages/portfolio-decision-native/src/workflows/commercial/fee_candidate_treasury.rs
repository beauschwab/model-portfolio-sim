//! Reviewed fee capital and funding from the acquired candidate's one daily Sim.
use super::{candidates, treasury as policy};
use portfolio_model_core::{modeled_product_funding as funding, strict_json};
use portfolio_risk_native::{advanced_graph::Role, treasury, treasury_bridge};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

pub const SCHEMA: &str = "commercial-candidate-fee-treasury-1";
const MAX_BYTES: usize = 32 << 20;
const MAX_INPUT: usize = 2 << 20;
const MAX_WORK: usize = 2_000_000;
// Additional JSON ownership is separate from the unchanged32MiB encoded result.
const JSON_PHASE_BYTES: usize = 64 << 20;
struct JsonPhase {
    bytes: usize,
}
impl JsonPhase {
    fn retain<T: serde::Serialize>(&mut self, value: &T) -> Result<(), String> {
        self.reserve_encoded(strict_json::serialized_size(value, MAX_BYTES)?)
    }
    fn reserve_encoded(&mut self, bytes: usize) -> Result<(), String> {
        let upper = bytes
            .checked_mul(20)
            .ok_or("fee treasury JSON capacity overflow")?;
        self.bytes = self
            .bytes
            .checked_add(upper)
            .ok_or("fee treasury JSON phase overflow")?;
        if self.bytes > JSON_PHASE_BYTES {
            return Err(
                "fee treasury additional JSON ownership exceeds reserved64MiB phase".into(),
            );
        }
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub raw_candidate: Value,
    pub capital: Option<policy::Capital>,
    pub ftp: Option<policy::Ftp>,
}
fn tick() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn identity(review: &funding::Identity) -> Result<(), String> {
    if [&review.id, &review.revision]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control))
    {
        return Err("explicit candidate fee treasury review required".into());
    }
    Ok(())
}
fn num(v: &Value, k: &str) -> Result<f64, String> {
    v[k].as_f64()
        .filter(|v| v.is_finite())
        .ok_or(format!("finite candidate fee treasury {k} required"))
}
fn text(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .map(str::to_owned)
        .ok_or(format!("candidate fee treasury text {k} required"))
}
fn date(day: i32) -> Result<String, String> {
    let (y, m, d) = portfolio_model_core::cashflow::ymd_from_ordinal(day)?;
    Ok(format!("{y:04}-{m:02}-{d:02}"))
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if (a - b).abs() > 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs())) {
        Err("candidate fee treasury source/journal reconciliation differs".into())
    } else {
        Ok(())
    }
}
fn day(year: f64) -> Result<i32, String> {
    let n = (year * 365.).round();
    let n = if year.to_bits() == (n / 365.).to_bits() {
        n
    } else {
        (year * 365.).ceil()
    };
    if !n.is_finite() || !(0. ..=1080.).contains(&n) {
        return Err("candidate fee native clock bound".into());
    }
    Ok(n as i32 + i32::from(year > n / 365.))
}
fn balance(s: &funding::State, p: &funding::Policy) -> f64 {
    s.channels
        .iter()
        .zip(&p.channels)
        .map(|(a, p)| a.amount * f64::from(p.coefficient))
        .sum()
}
fn key(v: &Value) -> Result<(String, String, String, String), String> {
    Ok((
        text(v, "scenario")?,
        text(v, "account")?,
        text(v, "gl_account")?,
        text(v, "instrument_id")?,
    ))
}
fn replay(candidate: &candidates::ResultData) -> Result<(), String> {
    if candidate.producer_completion != json!({"complete":true,"protocol":7}) {
        return Err("candidate fee protocol7 completion required".into());
    }
    let mut actual = BTreeMap::new();
    let mut transaction = None;
    let mut sum = 0.;
    let mut previous = BTreeMap::<String, (i64, i64)>::new();
    for row in candidate
        .tables
        .get("journal")
        .ok_or("candidate journal absent")?
    {
        tick()?;
        let scenario = text(row, "scenario")?;
        let d = row["day"].as_i64().ok_or("candidate journal integer day")?;
        let tx = row["transaction_number"]
            .as_i64()
            .ok_or("candidate journal integer transaction")?;
        if previous.get(&scenario).is_some_and(|p| *p > (d, tx)) {
            return Err("candidate fee journal order differs".into());
        }
        previous.insert(scenario.clone(), (d, tx));
        if transaction.as_ref() != Some(&(scenario.clone(), tx)) {
            near(sum, 0.)?;
            transaction = Some((scenario, tx));
            sum = 0.;
        }
        let amount = num(row, "value")?;
        sum += amount;
        *actual.entry(key(row)?).or_insert(0.) += amount;
    }
    near(sum, 0.)?;
    let trial = candidate
        .tables
        .get("trial_balance")
        .ok_or("candidate trial balance absent")?;
    if trial.len() != actual.len() {
        return Err("candidate journal/trial coverage differs".into());
    }
    for row in trial {
        near(
            num(row, "balance")?,
            *actual.get(&key(row)?).ok_or("candidate trial key absent")?,
        )?;
    }
    Ok(())
}
fn add_fee_state(state: &mut funding::State, fee_id: &str, snapshot: &Value) -> Result<(), String> {
    for (index, name) in funding::FEE_LANES.iter().enumerate().skip(18) {
        let signed = snapshot.get(*name).and_then(Value::as_f64).unwrap_or(0.);
        let amount = if matches!(
            *name,
            "fee_payable" | "deferred_fee_income" | "acquired_fee_liability"
        ) {
            -signed
        } else {
            signed
        };
        if !amount.is_finite() || amount < 0. {
            return Err("candidate fee gross stock sign differs".into());
        }
        state.channels[index].amount += amount;
        state.origins.push(funding::Origin {
            origin_id: format!("fee:{fee_id}:{name}"),
            source_event_id: format!("native-fee:{fee_id}"),
            channel: (*name).into(),
            amount,
        });
    }
    Ok(())
}
fn preflight(r: &Request, raw: &candidates::Request) -> Result<(usize, usize, i32), String> {
    portfolio_model_core::cashflow::validate_ordinal(raw.as_of_ordinal)?;
    portfolio_model_core::cashflow::validate_ordinal(raw.reporting_end_ordinal)?;
    let raw_bytes = strict_json::serialized_size(r, MAX_BYTES)?;
    let mut paths = 0usize;
    let mut maximum_origins = 0usize;
    let mut event_work = 0usize;
    let mut fees = false;
    let mut end = raw
        .as_of_ordinal
        .checked_add(
            portfolio_ledger_native::spec::normalize(raw.specification.clone(), 512, 8_000_000)?
                .horizon_days as i32,
        )
        .ok_or("candidate fee horizon overflow")?;
    for unit in &raw.commercial.units {
        portfolio_model_core::cashflow::validate_ordinal(unit.graph.as_of_ordinal)?;
        let mut loan_origins = 0;
        let mut fee_origins = 0;
        let p = unit
            .graph
            .positions
            .first()
            .ok_or("candidate position absent")?;
        if p.generated_policy.is_some() {
            return Err("fee treasury requires finite acquired candidate sources".into());
        }
        for set in &p.histories {
            for history in &set.physical {
                let a = portfolio_risk_native::commercial_credit_compiler::admit(&history.source)?;
                event_work = event_work
                    .checked_add(
                        a.accounting_groups_upper
                            .checked_mul(a.lots_per_path_upper * 2 + 32)
                            .and_then(|x| x.checked_mul(8))
                            .and_then(|x| x.checked_add(a.carry_points_upper * 32))
                            .ok_or("candidate fee source walk overflow")?,
                    )
                    .ok_or("candidate fee event work overflow")?;
                end = end.max(
                    unit.graph
                        .as_of_ordinal
                        .checked_add(day(a.full_credit_end_year)?)
                        .ok_or("candidate fee complete date overflow")?,
                );
                loan_origins = loan_origins.max(a.lots_per_path_upper * 2 + 8);
                paths = paths
                    .checked_add(1)
                    .ok_or("candidate fee bridge path overflow")?;
            }
        }
        if let Some(selection) = &p.fees {
            fees = true;
            for set in &selection.histories {
                for h in &set.physical {
                    end = end.max(h.policy.complete_end_ordinal);
                    fee_origins = fee_origins.max(h.policy.fees.len() * 6);
                    event_work = event_work
                        .checked_add(
                            (portfolio_risk_native::commercial_fee_source::event_bound(&h.policy)
                                + h.acquisition_releases.len()
                                + selection.allocations.len())
                                * 64,
                        )
                        .ok_or("candidate fee walk overflow")?;
                }
            }
        } else {
            return Err("complete fee ownership required for every treasury path".into());
        }
        maximum_origins = maximum_origins.max(loan_origins + fee_origins);
    }
    if !fees || paths == 0 || paths > 32 || end - raw.as_of_ordinal > 64 || maximum_origins > 128 {
        return Err("candidate fee bridge requires selected fee ownership and complete <=64day,32path,128origin source".into());
    }
    let work = raw_bytes
        .checked_mul(8)
        .and_then(|x| x.checked_add(event_work))
        .and_then(|x| {
            x.checked_add(
                paths * (end - raw.as_of_ordinal + 1) as usize * (maximum_origins + 24) * 32,
            )
        })
        .ok_or("candidate fee bridge work overflow")?;
    if work > MAX_WORK {
        return Err("candidate fee bridge exceeds separate2M before source generation".into());
    }
    // A retained source3 request and result can each use its one-MiB bound.
    // Output serialization coexists with all genuine candidate financial owners.
    let bytes = paths
        .checked_mul(2 << 20)
        .and_then(|x| x.checked_add(JSON_PHASE_BYTES))
        .ok_or("candidate fee bridge byte overflow")?;
    Ok((work, bytes, end))
}
pub fn evaluate(r: Request) -> Result<Value, String> {
    tick()?;
    let mut json_phase = JsonPhase { bytes: 0 };
    // Admit all raw Value copies before cloning into the typed candidate.
    for _ in 0..3 {
        json_phase.retain(&r)?;
    }
    if r.schema != SCHEMA || r.capital.is_none() && r.ftp.is_none() {
        return Err("explicit candidate fee capital/FTP selection required".into());
    }
    let raw: candidates::Request =
        serde_json::from_value(r.raw_candidate.clone()).map_err(|e| e.to_string())?;
    if raw.schema != candidates::SCHEMA {
        return Err("finite candidate raw schema required".into());
    }
    if raw.commercial.units.iter().any(|u| {
        u.graph.positions.iter().any(|p| {
            p.original_pricing
                .iter()
                .any(|h| h.source.source.transfer.is_some())
        })
    }) {
        return Err("secondary acquired rights require separately reviewed capital/FTP gross and valuation-adjustment channels".into());
    }
    let reservation = preflight(&r, &raw)?;
    // Original envelope, retained raw candidate and exact result raw identity
    // coexist with the typed native request. Charge before cloning.
    let raw_request = serde_json::to_value(&r).map_err(|e| e.to_string())?;
    if let Some(c) = &r.capital {
        identity(&c.review)?;
        treasury_bridge::validate_reviewed_commercial_fee_inputs(
            &c.treasury,
            &c.policies,
            c.amount_multiplier,
            &c.exposure_policy,
        )?;
    }
    if let Some(f) = &r.ftp {
        identity(&f.policy_review)?;
        for p in &f.reviews {
            for review in [
                &p.cash_exclusion_review,
                &p.channel_change_review,
                &p.income_review,
            ] {
                identity(review)?;
            }
            let ui = raw
                .owners
                .iter()
                .position(|o| o.position_id == p.position_id)
                .ok_or("candidate funding policy position absent")?;
            funding::validate_fee_policy_before_source(
                &p.policy,
                &p.resets,
                raw.commercial.units[ui].graph.as_of_ordinal,
                reservation.2,
            )?;
        }
    }
    let mut candidate = candidates::evaluate_reserved(&raw, (reservation.0, reservation.1))?;
    candidate.preparation.raw_request = r.raw_candidate.clone();
    replay(&candidate)?;
    let spec = &candidate.preparation.resolved_specification;
    let full_end = candidate.full_end_ordinal;
    let mut capital = Value::Null;
    if let Some(c) = r.capital {
        if c.treasury.as_of != date(full_end)? {
            return Err("candidate fee capital must use complete native closing date".into());
        }
        let mut trial = Vec::new();
        let mut memos = BTreeMap::new();
        for row in &candidate.tables["trial_balance"] {
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
                    return Err("unreviewed candidate memo channel".into());
                }
                *memos
                    .entry((
                        text(row, "scenario")?,
                        text(row, "account")?,
                        text(row, "instrument_id")?,
                    ))
                    .or_insert(0.) += num(row, "balance")?;
            } else {
                trial.push(serde_json::from_value(row.clone()).map_err(|e| e.to_string())?);
            }
        }
        for amount in memos.values() {
            near(*amount, 0.)?;
        }
        for p in &c.policies {
            let a = spec["accounts"]
                .as_array()
                .ok_or("candidate accounts absent")?
                .iter()
                .find(|a| a["id"] == p.account)
                .ok_or("candidate capital account absent")?;
            if a["entity"] != p.capital.entity
                || a["currency"] != p.capital.currency
                || p.scenario != p.capital.scenario
                || p.capital.period != date(full_end)?
            {
                return Err("candidate capital entity/currency/scenario differs".into());
            }
        }
        // Thirteen ratio rows, closing/audit/group rows and prepared policy
        // copies are bounded from the complete typed inputs before creation.
        let capital_json_upper = strict_json::serialized_size(
            &(&c.treasury, &trial, &c.policies, &c.exposure_policy),
            MAX_BYTES,
        )?
        .checked_mul(8)
        .and_then(|x| x.checked_add(8192))
        .ok_or("capital output upper overflow")?;
        json_phase.reserve_encoded(capital_json_upper)?;
        capital = treasury_bridge::prepare_reviewed_commercial_fee_exposure(
            c.treasury,
            trial,
            c.policies,
            c.amount_multiplier,
            &c.exposure_policy,
        )?;
        let bridge = capital["capital_bridge"]
            .as_array()
            .ok_or("candidate capital bridge absent")?;
        if bridge.len() != candidate.tables["closing_statements"].len() {
            return Err("candidate capital complete scope differs".into());
        }
        for close in &candidate.tables["closing_statements"] {
            let b = bridge
                .iter()
                .find(|b| b["scenario"] == close["scenario"] && b["account"] == close["account"])
                .ok_or("candidate capital closing scope absent")?;
            for (a, k) in [
                ("assets", "assets"),
                ("liabilities", "liabilities"),
                ("book_equity", "equity"),
            ] {
                near(num(b, a)?, num(close, k)? * c.amount_multiplier)?;
            }
        }
        capital["commercial_closing_review"] = json!(c.review);
        strict_json::serialized_size(&capital, capital_json_upper)?;
    }
    let mut funding_paths = Vec::new();
    let mut ftp = Value::Null;
    if let Some(f) = r.ftp {
        if f.as_of != date(raw.as_of_ordinal)? {
            return Err("candidate FTP asof must equal outer actual opening".into());
        }
        let mut used = BTreeSet::new();
        let mut positions = Vec::new();
        let mut loan_days = BTreeMap::<(&str, &str, u64), Vec<&Value>>::new();
        for row in &candidate.tables["commercial_daily"] {
            tick()?;
            loan_days
                .entry((
                    row["scenario"].as_str().ok_or("daily scenario")?,
                    row["position_id"].as_str().ok_or("daily position")?,
                    row["path_key"].as_u64().ok_or("daily path")?,
                ))
                .or_default()
                .push(row);
        }
        let mut fee_days = BTreeMap::new();
        for row in &candidate.tables["commercial_fee_daily"] {
            tick()?;
            let key = (
                row["scenario"].as_str().ok_or("fee daily scenario")?,
                row["position_id"].as_str().ok_or("fee daily position")?,
                row["path_key"].as_u64().ok_or("fee daily path")?,
                row["day"].as_i64().ok_or("fee daily day")?,
                row["fee_id"].as_str().ok_or("fee daily id")?,
            );
            if fee_days.insert(key, row).is_some() {
                return Err("duplicate native fee daily snapshot".into());
            }
        }
        for (ui, graph) in candidate
            .preparation
            .native_preparation
            .source()
            .native_graphs
            .iter()
            .enumerate()
        {
            let unit = &raw.commercial.units[ui];
            let owner = &raw.owners[ui];
            let start = unit.graph.as_of_ordinal;
            let origin = start - raw.as_of_ordinal;
            for native in graph
                .native_sources
                .iter()
                .filter(|s| s.role == Role::Physical)
            {
                tick()?;
                let scene = raw
                    .run_mapping
                    .iter()
                    .find(|m| m.source_scenario_id == native.scenario_id)
                    .ok_or("candidate FTP source mapping absent")?;
                let p = f
                    .reviews
                    .iter()
                    .find(|p| {
                        p.scenario == scene.ledger_scenario_id
                            && p.position_id == owner.position_id
                            && p.path_key == native.path_key
                    })
                    .ok_or("candidate FTP complete path review absent")?;
                if !used.insert((p.scenario.clone(), p.position_id.clone(), p.path_key)) {
                    return Err("candidate FTP duplicate physical path review".into());
                }
                let account = spec["accounts"]
                    .as_array()
                    .ok_or("candidate accounts absent")?
                    .iter()
                    .find(|a| a["id"] == owner.account_id)
                    .ok_or("candidate FTP account absent")?;
                if account["entity"] != p.entity || account["currency"] != p.currency {
                    return Err("candidate FTP entity/currency differs".into());
                }
                let loan = native
                    .physical_accounting_source()
                    .ok_or("sealed candidate Physical owner absent")?;
                let fee = native
                    .fee_projection
                    .as_ref()
                    .ok_or("complete fee ownership required for every treasury path")?;
                let mut states = Vec::new();
                let mut undrawn = Vec::new();
                let points = loan
                    .carry()
                    .carry_points()
                    .iter()
                    .filter(|point| point.path_key == native.path_key)
                    .collect::<Vec<_>>();
                let mut pi = 0;
                let mut u = None;
                for row in loan_days
                    .get(&(p.scenario.as_str(), p.position_id.as_str(), p.path_key))
                    .ok_or("candidate complete daily path absent")?
                {
                    tick()?;
                    let d = row["day"].as_i64().ok_or("candidate daily integer day")? as i32;
                    if d < origin {
                        continue;
                    }
                    let year = f64::from(d - origin) / 365.;
                    while pi < points.len() && points[pi].year <= year {
                        u = points[pi].undrawn_commitment;
                        pi += 1;
                    }
                    let u = u.ok_or("candidate complete commitment stock absent")?;
                    undrawn.push(u);
                    let mut state =
                        policy::state(&row["unweighted_book"], raw.as_of_ordinal + d, u)?;
                    state
                        .channels
                        .extend(
                            funding::FEE_LANES
                                .iter()
                                .skip(18)
                                .map(|name| funding::Amount {
                                    channel: (*name).into(),
                                    amount: 0.,
                                }),
                        );
                    for fee_id in &fee.bound_path.source().raw_request().fees {
                        let snapshot = fee_days.get(&(
                            p.scenario.as_str(),
                            p.position_id.as_str(),
                            p.path_key,
                            i64::from(d),
                            fee_id.fee_id.as_str(),
                        ));
                        let empty = Value::Null;
                        let snapshot = match snapshot {
                            Some(row) => &row["native_fee_snapshot"],
                            None => {
                                if fee
                                    .bound_path
                                    .source()
                                    .groups()
                                    .iter()
                                    .zip(fee.bound_path.native_years())
                                    .any(|(g, t)| g.event.fee_id == fee_id.fee_id && *t <= year)
                                {
                                    return Err("candidate complete fee daily state absent".into());
                                }
                                &empty
                            }
                        };
                        add_fee_state(&mut state, &fee_id.fee_id, snapshot)?;
                    }
                    states.push(state);
                }
                let mut cash = Vec::new();
                let mut earned = Vec::new();
                let mut opening_cash = 0.;
                let mut accept = |id: String,
                                  ordinal: i32,
                                  bank_cash: f64,
                                  earnings: Vec<(&str, f64)>|
                 -> Result<(), String> {
                    if ordinal == start {
                        opening_cash += bank_cash;
                        if earnings.iter().any(|(_, a)| *a != 0.) {
                            return Err(
                                "opening candidate fee P&L needs explicit opening earnings policy"
                                    .into(),
                            );
                        }
                    } else {
                        cash.push(funding::Cash {
                            event_id: id.clone(),
                            ordinal,
                            kind: "source_atomic_cash".into(),
                            bank_cash,
                            funding_change_sequence: None,
                            exclusion_review: Some(p.cash_exclusion_review.clone()),
                        });
                        for (channel, amount) in earnings {
                            if amount != 0. {
                                earned.push(funding::Earned {
                                    event_id: format!("{id}:{channel}"),
                                    ordinal,
                                    channel: channel.into(),
                                    bank_earnings: amount,
                                    review: p.income_review.clone(),
                                });
                            }
                        }
                    }
                    Ok(())
                };
                for (i, g) in loan
                    .accounting()
                    .groups()
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| g.path_key == native.path_key)
                {
                    let value = serde_json::to_value(g).map_err(|e| e.to_string())?;
                    let mut earnings = Vec::new();
                    for (channel, account) in [
                        ("original_eir", "interest_income"),
                        ("reviewed_basis", "basis_recognition_expense"),
                        ("incurred_credit", "credit_loss_expense"),
                        ("recovery_income", "recovery_income"),
                    ] {
                        let mut amount = 0.;
                        for posting in value["postings"]
                            .as_array()
                            .ok_or("candidate native postings absent")?
                        {
                            if posting["posting"]["account"] == account {
                                amount -= num(&posting["posting"], "amount")?;
                            }
                        }
                        earnings.push((channel, amount));
                    }
                    accept(format!("loan:{i}"), start + day(g.year)?, g.cash, earnings)?;
                }
                for (i, g) in fee.bound_path.source().groups().iter().enumerate() {
                    use portfolio_model_core::nonintegral_fee::Account as A;
                    let cash = g
                        .postings
                        .iter()
                        .filter(|p| p.account == A::Cash)
                        .map(|p| p.amount)
                        .sum();
                    let mut earnings = Vec::new();
                    for (channel, account) in [
                        ("service_fee", A::FeeIncome),
                        ("fee_cost_release", A::FeeAcquisitionExpense),
                        ("fee_writeoff", A::FeeWriteoffExpense),
                        ("fee_recovery", A::FeeRecoveryIncome),
                    ] {
                        earnings.push((
                            channel,
                            -g.postings
                                .iter()
                                .filter(|p| p.account == account)
                                .map(|p| p.amount)
                                .sum::<f64>(),
                        ));
                    }
                    accept(
                        format!("fee:{i}"),
                        start + day(fee.bound_path.native_years()[i])?,
                        cash,
                        earnings,
                    )?;
                }
                let changes = states
                    .windows(2)
                    .enumerate()
                    .map(|(i, s)| funding::Change {
                        sequence: (i + 1) as u32,
                        ordinal: s[1].ordinal,
                        source_event_id: s[1].source_checkpoint_id.clone(),
                        kind: "reviewed_other_reallocation".into(),
                        amount: balance(&s[1], &p.policy) - balance(&s[0], &p.policy),
                        cash_event_id: None,
                        review: p.channel_change_review.clone(),
                    })
                    .collect();
                let request = funding::Request {
                    schema: funding::FEE_SCHEMA.into(),
                    measure: "physical".into(),
                    source: funding::Identity {
                        id: loan.raw_request().source.current.contract.id.clone(),
                        revision: loan.raw_request().source.current.contract.revision.clone(),
                    },
                    original_yield_anchor: funding::Identity {
                        id: loan
                            .raw_request()
                            .source
                            .acquisition
                            .original_yield_anchor
                            .id
                            .clone(),
                        revision: loan
                            .raw_request()
                            .source
                            .acquisition
                            .original_yield_anchor
                            .revision
                            .clone(),
                    },
                    path_id: native.path_id.clone(),
                    entity: p.entity.clone(),
                    currency_unit: p.currency.clone(),
                    scenario_id: p.scenario.clone(),
                    side: "asset".into(),
                    start_ordinal: start,
                    full_end_ordinal: full_end,
                    source_weight: native.weight,
                    unit_face: unit.unit_face,
                    allocated_face: raw.allocation[ui].face,
                    opening_funding_balance: balance(
                        states.first().ok_or("candidate opening state absent")?,
                        &p.policy,
                    ),
                    policy: p.policy.clone(),
                    states,
                    cash,
                    changes,
                    earned,
                    resets: p.resets.clone(),
                };
                let prepared = funding::prepare_fee_with_check(&request, &mut tick)?;
                if !prepared.treasury_nonnegative_positions_compatible {
                    return Err("research signed funding cannot enter nonnegative Treasury positions; choose an explicit compatible allocation policy".into());
                }
                for (i, d) in prepared.days.iter().enumerate() {
                    let avg_u = if p.policy.averaging == "opening_daily" {
                        undrawn[i]
                    } else {
                        (undrawn[i] + undrawn[i + 1]) * 0.5
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
                        fees: d
                            .earnings_by_channel
                            .iter()
                            .filter(|a| a.channel != "original_eir")
                            .map(|a| a.amount)
                            .sum(),
                        operating_cost: p.daily_operating_cost * prepared.scale,
                        expected_loss: 0.,
                        allocated_capital: p.allocated_capital * prepared.scale,
                        cost_of_capital: p.annual_cost_of_capital,
                        option_spread: p.option_spread,
                        contingent_liquidity_charge: avg_u
                            * prepared.scale
                            * p.contingent_annual_rate
                            * d.accrual_fraction,
                    });
                }
                #[derive(Serialize)]
                struct Path<'a> {
                    scenario: &'a str,
                    position_id: &'a str,
                    path_key: u64,
                    request: &'a funding::Request,
                    preparation: &'a funding::ResultData,
                    opening_cash: f64,
                }
                let path = Path {
                    scenario: &p.scenario,
                    position_id: &p.position_id,
                    path_key: p.path_key,
                    request: &request,
                    preparation: &prepared,
                    opening_cash,
                };
                json_phase.retain(&path)?;
                funding_paths.push(serde_json::to_value(&path).map_err(|e| e.to_string())?);
            }
        }
        if used.len() != f.reviews.len() {
            return Err("unused candidate fee FTP review".into());
        }
        let request = treasury::Request {
            as_of: f.as_of,
            policy_id: f.policy_review.id.clone(),
            capital: vec![],
            curves: f.curves,
            positions,
            strategy_units: vec![],
            capital_deltas: vec![],
        };
        // Each input position creates one bounded report row and its prepared
        // position copy; curves/reconciliation are included in the fourfold
        // serialized bound. Value capacity is reserved before native creation.
        let ftp_json_upper = strict_json::serialized_size(&request, MAX_BYTES)?
            .checked_mul(4)
            .and_then(|x| x.checked_add(8192))
            .ok_or("FTP output upper overflow")?;
        json_phase.reserve_encoded(ftp_json_upper)?;
        ftp = request.run()?;
        ftp["prepared_specification"] = serde_json::to_value(request).map_err(|e| e.to_string())?;
        ftp["policy_review"] = json!(f.policy_review);
        strict_json::serialized_size(&ftp, ftp_json_upper)?;
    }
    json_phase.retain(&candidate)?;
    let mut result = json!({"schema":SCHEMA,"full_end_ordinal":full_end,"bridge_work_bound":reservation.0,"bridge_work_limit":MAX_WORK,"native_ordered_journal_replayed":true,"separate_fee_source_adoption":true,"original_loan_eir_unchanged":true,"economic_loss_deducted_again":false,"independent_persisted_verification":false,"opening_identity_verified":false,"mixed_ledger_adoption":false,"empirical_validation":false,"regulatory_credit_rwa_approval":false,"accounting_standard_approval":false,"production_adoption":false});
    result["source_candidate"] = serde_json::to_value(&candidate).map_err(|e| e.to_string())?;
    result["raw_request"] = raw_request;
    result["capital"] = capital;
    result["ftp"] = ftp;
    result["funding_paths"] = Value::Array(funding_paths);
    strict_json::serialized_size(&result, MAX_BYTES)?;
    tick()?;
    Ok(result)
}

pub fn run_envelope_started(bytes: &[u8], started: Instant) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope {
        op: String,
        schema: String,
        execution_timeout_ms: u64,
        request: Request,
    }
    let value = strict_json::parse_checked(bytes, MAX_INPUT, tick)?;
    let original = value["request"].clone();
    let e: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if e.op != "analyze_commercial_candidate_fee_treasury" || e.schema != SCHEMA {
        return Err("explicit acquired candidate fee treasury operation required".into());
    }
    let control = match portfolio_compute_control::active() {
        Some(c) => c.child_started(e.execution_timeout_ms, started)?,
        None => portfolio_compute_control::Control::from_started(e.execution_timeout_ms, started)?,
    };
    portfolio_compute_control::scope(Some(control), || {
        let mut result = evaluate(e.request)?;
        result["raw_request"] = original;
        strict_json::serialized_size(&result, MAX_BYTES)?;
        tick()?;
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> Value {
        serde_json::from_str(include_str!(
            "../../../../../examples/models/commercial-candidate-fee-treasury-1.json"
        ))
        .unwrap()
    }
    #[test]
    fn actual_fee_candidate_sources_fund_complete_tails_with_separate_profit() {
        for negative in [false, true] {
            let mut r = raw();
            if negative {
                for review in r["ftp"]["reviews"].as_array_mut().unwrap() {
                    review["policy"]["negative_funding"] = json!("research_signed");
                    for channel in review["policy"]["channels"].as_array_mut().unwrap() {
                        if channel["channel"] == "acquired_fee_liability" {
                            channel["coefficient"] = json!(0);
                        }
                    }
                }
                fn flip(v: &mut Value) {
                    match v {
                        Value::Object(map) => {
                            for (k, v) in map {
                                if matches!(
                                    k.as_str(),
                                    "amount"
                                        | "entitlement"
                                        | "allocated_cost"
                                        | "original_signed_fee_quote"
                                ) {
                                    *v = json!(-v.as_f64().unwrap())
                                } else {
                                    flip(v)
                                }
                            }
                        }
                        Value::Array(a) => a.iter_mut().for_each(flip),
                        _ => {}
                    }
                }
                let p = &mut r["raw_candidate"]["commercial"]["units"][0]["graph"]["positions"][0];
                flip(&mut p["fees"]);
                p["fees"]["total_purchase_cost"] =
                    json!(p["original_signed_cash_quote"].as_f64().unwrap() - 5.);
            }
            let result = evaluate(serde_json::from_value(r).unwrap()).unwrap();
            assert_eq!(result["native_ordered_journal_replayed"], true);
            assert_eq!(result["separate_fee_source_adoption"], true);
            assert_eq!(result["funding_paths"].as_array().unwrap().len(), 2);
            assert_eq!(result["capital"]["separate_fee_exposure_groups"], true);
            for path in result["funding_paths"].as_array().unwrap() {
                assert_eq!(path["request"]["schema"], funding::FEE_SCHEMA);
                assert_eq!(
                    path["request"]["states"][0]["channels"]
                        .as_array()
                        .unwrap()
                        .len(),
                    24
                );
                assert_eq!(path["preparation"]["days"].as_array().unwrap().len(), 45);
                assert!(path["preparation"]["days"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["earnings_by_channel"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a["channel"] == "service_fee"
                            && a["amount"].as_f64().unwrap() != 0.)));
                assert!(path["preparation"]["days"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["earnings_by_channel"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a["channel"] == "fee_cost_release"
                            && a["amount"].as_f64().unwrap() != 0.)));
            }
        }
    }
    #[test]
    fn zero_participation_keeps_complete_fee_funding_evidence() {
        let mut r = raw();
        r["raw_candidate"]["allocation"][0]["face"] = json!(0.);
        let result = evaluate(serde_json::from_value(r).unwrap()).unwrap();
        for path in result["funding_paths"].as_array().unwrap() {
            assert_eq!(path["preparation"]["scale"], 0.);
            assert!(!path["request"]["cash"].as_array().unwrap().is_empty());
        }
        assert!(
            result["source_candidate"]["tables"]["commercial_fee_lineage"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["transaction_number"].is_null())
        );
    }
    #[test]
    fn fee_capital_and_funding_require_complete_explicit_selections() {
        let mut r = raw();
        r["ftp"]["reviews"][0]["policy"]["channels"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(evaluate(serde_json::from_value(r).unwrap())
            .unwrap_err()
            .contains("channel"));
        let mut r = raw();
        r["capital"]["policies"][0]["gl_risk_weights"]
            .as_object_mut()
            .unwrap()
            .remove("commercial_acquired_fee_asset");
        assert!(evaluate(serde_json::from_value(r).unwrap())
            .unwrap_err()
            .contains("explicit risk weight"));
        let mut r = raw();
        r["raw_candidate"]["specification"]["horizon_days"] = json!(65);
        assert!(evaluate(serde_json::from_value(r).unwrap())
            .unwrap_err()
            .contains("64day"));
    }
}
