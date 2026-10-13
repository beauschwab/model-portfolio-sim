use super::*;
use portfolio_ledger_native::types::Spec;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
struct Count {
    bytes: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        self.bytes = self.bytes.saturating_add(b.len());
        if self.bytes > self.limit {
            return Err(io::Error::other(
                "candidate journal byte admission exceeded",
            ));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn size<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut c = Count { bytes: 0, limit };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.bytes)
}
pub(super) fn retained<T: Serialize>(v: &T) -> Result<(), String> {
    size(v, MAX_BYTES / 4)?;
    Ok(())
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 120 || s.chars().any(char::is_control) {
        Err("candidate journal explicit bounded identity required".into())
    } else {
        Ok(())
    }
}
pub(super) fn raw(r: &Request) -> Result<(), String> {
    size(r, 32 * 1024 * 1024)?;
    if r.schema != SCHEMA
        || r.product.monetary_unit != "currency_units"
        || r.owners.len() != r.product.units.len()
        || r.allocation.len() != r.product.units.len()
        || r.owners.is_empty()
        || r.owners.len() > 256
        || r.run_mapping.is_empty()
        || r.run_mapping.len() > 13
        || r.policy.book_basis != "reviewed_original_eir-1"
        || r.policy.acquisition_phase != "after_verified_opening_before_recognition-1"
        || r.policy.tail != "retain_complete-1"
        || [&r.policy.fees, &r.policy.defaults, &r.policy.marks]
            .iter()
            .any(|v| v.as_str() != "reject")
    {
        return Err(
            "candidate journal version/measure/owner/reviewed ownership admission mismatch".into(),
        );
    }
    for id in [&r.policy.identity, &r.verified_opening_identity] {
        label(&id.id)?;
        label(&id.revision)?;
    }
    let mut units = BTreeSet::new();
    let mut positions = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for (u, o) in r.product.units.iter().zip(&r.owners) {
        for s in [
            &o.unit_id,
            &o.position_id,
            &o.account_id,
            &o.source.id,
            &o.source.revision,
            &o.accounting_category,
            &o.ledger_classification,
        ] {
            label(s)?;
        }
        if o.unit_id != u.unit_id
            || o.accounting_category != u.accounting_category
            || o.ownership != "native_product_candidate-1"
            || !units.insert(&o.unit_id)
            || !positions.insert(&o.position_id)
            || !sources.insert((&o.source.id, &o.source.revision))
            || o.pre_purchase_principal != 0.
            || o.pre_purchase_accrued != 0.
            || o.pre_purchase_basis != 0.
            || !["ac", "htm"].contains(&o.ledger_classification.as_str())
        {
            return Err(
                "candidate zero pre-purchase state/unique mapping/category ownership required"
                    .into(),
            );
        }
    }
    Ok(())
}
pub(super) fn spec(r: &Request, s: &Spec) -> Result<(), String> {
    if s.horizon_days
        != usize::try_from(r.reporting_end_ordinal - r.as_of_ordinal)
            .map_err(|_| "candidate reversed reporting dates")?
        || s.positions.len() != r.owners.len()
        || !s.netting_sets.is_empty()
        || !s.policies.is_empty()
        || !s.cashflows.is_empty()
    {
        return Err("candidate raw specification must match reporting grid and contain only explicit candidate placeholders; mixed/legacy owners unsupported".into());
    }
    let positions: BTreeMap<_, _> = s.positions.iter().map(|p| (p.id.as_str(), p)).collect();
    let accounts: BTreeMap<_, _> = s.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
    for o in &r.owners {
        let p = positions
            .get(o.position_id.as_str())
            .ok_or("candidate owner absent from explicit raw specification")?;
        let u = r
            .product
            .units
            .iter()
            .find(|u| u.unit_id == o.unit_id)
            .unwrap();
        let deposit = matches!(
            u.purchase_graph,
            portfolio_risk_native::product_units::GraphRequest::Deposit(_)
        );
        let a = accounts
            .get(o.account_id.as_str())
            .ok_or("candidate account absent from specification")?;
        if p.account.as_str() != o.account_id
            || a.currency != r.product.currency
            || p.classification.as_str() != o.ledger_classification
            || (deposit && p.kind != "deposit")
            || (!deposit && !matches!(p.kind.as_str(), "loan" | "security"))
            || p.balance != 0.
            || p.opening_accrued != 0.
            || p.book_adjustment != 0.
            || p.allowance != 0.
            || p.start_day != 0
            || p.rate != 0.
            || p.annual_pd != 0.
            || p.watch_fraction != 0.
            || p.migration_rate != 0.
            || p.commitment != 0.
            || p.draw_fraction != 0.
            || p.monthly_runoff != 0.
            || p.floating_beta != 0.
            || p.rollover != 0.
            || p.eligible_fraction != 0.
            || p.encumbered_fraction != 0.
            || p.pledged_face != 0.
            || !p.collateral_position.is_empty()
            || !p.collateral_pool.is_empty()
            || p.maturity_day != 0
            || p.opening_market_price != 1.
        {
            return Err("candidate specification opening/source overlaps generic credit/coupon/mark/collateral/maturity owners".into());
        }
    }
    for a in &s.accounts {
        if a.currency != r.product.currency
            || a.annual_fees != 0.
            || a.annual_costs != 0.
            || a.tax_rate != 0.
            || a.annual_dividends != 0.
            || a.capital_deductions != 0.
        {
            return Err("candidate accounts cannot infer taxes/fees/distributions/capital or currency conversion".into());
        }
    }
    let mut actual = BTreeSet::new();
    let mut source_ids = BTreeSet::new();
    for m in &r.run_mapping {
        label(&m.source_scenario_id)?;
        label(&m.ledger_scenario_id)?;
        if !m.severity.is_finite()
            || !source_ids.insert(&m.source_scenario_id)
            || !actual.insert((
                m.ledger_scenario_id.clone(),
                if m.severity == 0. {
                    0.0f64.to_bits()
                } else {
                    m.severity.to_bits()
                },
            ))
            || ((m.source_scenario_id == r.product.base_scenario_id)
                != (m.ledger_scenario_id == "baseline"))
        {
            return Err("candidate run mapping must be complete unique explicit baseline/scenario/severity identities".into());
        }
    }
    let mut expected = BTreeSet::from([("baseline".into(), 1.0f64.to_bits())]);
    for scene in &s.scenarios {
        expected.insert((scene.name.clone(), 1.0f64.to_bits()));
        for &severity in &s.reverse_severities {
            expected.insert((
                scene.name.clone(),
                if severity == 0. {
                    0.0f64.to_bits()
                } else {
                    severity.to_bits()
                },
            ));
        }
    }
    if actual != expected {
        return Err(
            "candidate runs must cover exact raw specification scenarios/reverse severities".into(),
        );
    }
    Ok(())
}
pub(super) fn aggregate(
    r: &Request,
    p: &Prepared,
    source_work: usize,
    source_rows: usize,
    full: i32,
) -> Result<usize, String> {
    let s = p.source();
    let ids: BTreeSet<_> = r
        .run_mapping
        .iter()
        .map(|m| m.source_scenario_id.as_str())
        .collect();
    if ids != s.scenario_ids.iter().map(String::as_str).collect() {
        return Err("candidate raw/native scenario identities differ".into());
    }
    let days = usize::try_from(full - r.as_of_ordinal + 1)
        .map_err(|_| "candidate complete tail reversed")?;
    let account_count = r.specification["accounts"]
        .as_array()
        .ok_or("explicit candidate accounts required")?
        .len();
    let account_rows = days
        .checked_mul(account_count)
        .and_then(|x| x.checked_mul(r.run_mapping.len()))
        .ok_or("candidate account-state work overflow")?;
    let mut state_rows = 0usize;
    let mut path_count = 0usize;
    let mut path_bytes = 0usize;
    for (ui, u) in s.units.iter().enumerate() {
        for income in &u.income {
            let row = income.native_income["native_income_result"]["rows"]
                .as_array()
                .and_then(|a| a.last())
                .ok_or("candidate retained income absent")?;
            let end = row["payment_ordinal"]
                .as_i64()
                .ok_or("candidate complete tail missing")?;
            let n = usize::try_from(end - r.as_of_ordinal as i64 + 1)
                .map_err(|_| "candidate complete state reversed")?;
            state_rows = state_rows
                .checked_add(n)
                .ok_or("candidate state-row overflow")?;
            if u.unit_id != r.owners[ui].unit_id {
                return Err("candidate ordered identity differs".into());
            }
            path_count = add(path_count, 1)?;
            let source = u.complete_purchase_result["native_sources"]
                .as_array()
                .and_then(|a| {
                    a.iter().find(|x| {
                        x["role"] == "physical"
                            && x["scenario_id"] == income.scenario_id
                            && x["path_id"] == income.path_id
                    })
                })
                .ok_or("candidate capacity physical source missing")?;
            let projection = &source["projection"];
            let cash = projection["cashflows"]
                .as_array()
                .ok_or("candidate capacity cash missing")?;
            let deposit = matches!(
                s.raw_source.units[ui].purchase_graph,
                portfolio_risk_native::product_units::GraphRequest::Deposit(_)
            );
            let recognition = projection[if deposit { "interest" } else { "recognition" }]
                .as_array()
                .ok_or("candidate capacity recognition missing")?;
            let origins: Vec<Value> = recognition
                .iter()
                .map(|x| {
                    if deposit {
                        serde_json::json!(format!(
                            "deposit:{}:{}:{}",
                            x["segment_id"].as_str().unwrap_or(""),
                            x["start_ordinal"],
                            x["end_ordinal"]
                        ))
                    } else {
                        x["origin_id"].clone()
                    }
                })
                .collect();
            let cash_ids: Vec<_> = cash.iter().map(|x| x["event_id"].clone()).collect();
            let mapping = r
                .run_mapping
                .iter()
                .find(|m| m.source_scenario_id == income.scenario_id)
                .ok_or("candidate capacity scenario missing")?;
            let raw_income = &income.native_income["raw_income_request"];
            let metadata = serde_json::json!([
                r.owners[ui],
                r.policy,
                mapping,
                u.unit_id,
                income.scenario_id,
                income.path_id,
                source["run_id"],
                s.currency,
                raw_income["contract"],
                raw_income["original_yield_anchor"],
                raw_income["policy"],
                origins,
                cash_ids,
                format!(
                    "{}:{}:{}:income:{end}",
                    u.unit_id, income.scenario_id, income.path_id
                )
            ]);
            // Each generated record has only a subset of these exact escaped
            // strings; four copies cover nested identities/rights lineage. 4096
            // additionally bounds fixed JSON keys, numeric fields and row storage.
            let width = add(mul(size(&metadata, MAX_BYTES)?, 4)?, 4096)?;
            let source_days = usize::try_from(end - s.raw_source.units[ui].purchase_ordinal as i64)
                .map_err(|_| "candidate capacity source reversed")?;
            let records = add(
                add(add(add(mul(source_days, 8)?, n)?, 12)?, cash.len())?,
                recognition.len(),
            )?;
            path_bytes = add(path_bytes, mul(records, width)?)?;
        }
    }
    let lines = add(mul(source_rows, 12)?, mul(path_count, 4)?)?;
    let work = add(
        add(
            add(source_work, mul(state_rows, 8)?)?,
            mul(account_rows, 4)?,
        )?,
        lines,
    )?;
    // Estimate output/capacity from actual cardinalities before generation. Raw
    // prepared receipts are separately bounded and remain included in the total.
    let raw_bytes = size(r, 32 * 1024 * 1024)?;
    let prepared_bytes = size(p, MAX_BYTES / 4)?;
    let mut account_bytes = 0usize;
    for account in r.specification["accounts"].as_array().unwrap() {
        for scenario in &s.scenario_ids {
            let metadata = serde_json::json!([account, scenario]);
            let width = add(mul(size(&metadata, MAX_BYTES)?, 4)?, 2048)?;
            account_bytes = add(account_bytes, mul(add(days, 5)?, width)?)?;
        }
    }
    let bytes = add(
        add(path_bytes, account_bytes)?,
        mul(add(raw_bytes, prepared_bytes)?, 4)?,
    )?;
    if state_rows > MAX_ROWS
        || account_rows > MAX_ROWS
        || lines > MAX_LINES
        || work > product_candidate_source::MAX_CANDIDATE_WORK
        || bytes > MAX_BYTES
    {
        return Err(
            "candidate aggregate journal/state/output work admission exceeded before generation"
                .into(),
        );
    }
    Ok(work)
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "candidate aggregate capacity/work overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "candidate aggregate capacity/work overflow".into())
}
