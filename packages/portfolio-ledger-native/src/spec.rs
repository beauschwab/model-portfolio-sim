//! Raw daily ledger specification normalization and admission. Research weights
//! and opening equity are explicit; no balance-sheet plug is inferred.
use crate::types::Spec;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
type R<T> = Result<T, String>;
const MODEL: &str = "balance-stress-2";
const RULES: &str = "research-weights-v2";
fn range(row: &Value, fields: &str, lo: f64, hi: f64) -> R<()> {
    for name in fields.split_whitespace() {
        let value = row[name]
            .as_f64()
            .ok_or_else(|| format!("missing numeric {name}"))?;
        if !value.is_finite() || value < lo || value > hi {
            return Err(format!("{name} must be in [{lo}, {hi}]"));
        }
    }
    Ok(())
}
fn fields(rows: &[Value], ids: &str, cap: usize, label: &str) -> R<()> {
    if rows.len() > cap {
        return Err(format!("{label} must be a list of at most {cap} items"));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        let id = row[ids].as_str().ok_or("missing ID")?;
        if id.trim().is_empty() || !seen.insert(id) {
            return Err(format!("{label} IDs must be nonempty and unique"));
        }
        for (name, value) in row.as_object().ok_or("expected record object")? {
            if value.is_null() {
                return Err(format!("{name} must be finite and non-null"));
            }
            if let Some(s) = value.as_str() {
                if s.chars().count() > 120 {
                    return Err(format!("{name} must be a short string"));
                }
            } else if value.is_f64() {
                if value
                    .as_f64()
                    .is_none_or(|v| !v.is_finite() || v.abs() > 1e15)
                {
                    return Err(format!(
                        "{name} must be a finite number of magnitude <= 1e15"
                    ));
                }
            } else if let Some(v) = value.as_u64() {
                if v > 36500 {
                    return Err(format!("{name} must be an integer in [0, 36500]"));
                }
            }
        }
    }
    Ok(())
}
pub fn normalize(raw: Value, max_positions: usize, max_work: usize) -> R<Spec> {
    let spec = parse_raw(raw)?;
    validate(&spec, max_positions, max_work)?;
    Ok(spec)
}
/// Parse typed defaults only. Callers must validate before executing.
pub fn parse_raw(mut raw: Value) -> R<Spec> {
    let map = raw
        .as_object_mut()
        .ok_or("unknown stress specification fields")?;
    if map.get("version").and_then(Value::as_str) != Some(MODEL) {
        return Err(format!("version must be {MODEL}"));
    }
    map.remove("version");
    map.remove("provenance");
    let spec: Spec = serde_json::from_value(raw).map_err(|e| e.to_string())?;
    Ok(spec)
}
pub fn validate(spec: &Spec, max_positions: usize, max_work: usize) -> R<()> {
    let days = spec.horizon_days;
    if !(30..=1080).contains(&days) {
        return Err("horizon_days must be in [30, 1080]".into());
    }
    // Validate admission/uniqueness without materializing an entire Value tree.
    for (name, ids, cap) in [
        (
            "accounts",
            spec.accounts
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            50,
        ),
        (
            "positions",
            spec.positions.iter().map(|r| r.id.as_str()).collect(),
            max_positions,
        ),
        (
            "netting_sets",
            spec.netting_sets.iter().map(|r| r.id.as_str()).collect(),
            500,
        ),
        (
            "policies",
            spec.policies.iter().map(|r| r.id.as_str()).collect(),
            100,
        ),
        (
            "scenarios",
            spec.scenarios.iter().map(|r| r.name.as_str()).collect(),
            8,
        ),
    ] {
        if ids.len() > cap {
            return Err(format!("{name} must be a list of at most {cap} items"));
        }
        if ids.iter().any(|id| id.trim().is_empty())
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        {
            return Err(format!("{name} IDs must be nonempty and unique"));
        }
    }
    let accounts: BTreeMap<_, _> = spec.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
    let positions: BTreeMap<_, _> = spec.positions.iter().map(|p| (p.id.as_str(), p)).collect();
    if accounts.is_empty() || spec.scenarios.is_empty() {
        return Err("accounts and scenarios are required".into());
    }
    if spec
        .accounts
        .iter()
        .map(|a| (&a.entity, &a.currency))
        .collect::<BTreeSet<_>>()
        .len()
        != accounts.len()
    {
        return Err("one account per legal entity and currency is required".into());
    }
    for a in &spec.accounts {
        let value = serde_json::to_value(a).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "id", 1, "accounts")?;
        if a.entity.trim().is_empty() || a.currency.trim().is_empty() {
            return Err("entity and currency are required".into());
        }
        range(
            row,
            "cash cash_floor capital_deductions annual_fees annual_costs annual_dividends",
            0.,
            1e15,
        )?;
        range(
            row,
            "cet1_floor leverage_floor tax_rate htm_asset_limit",
            0.,
            1.,
        )?;
        range(row, "lcr_floor nsfr_floor", 0., 10.)?;
    }
    for p in &spec.positions {
        let value = serde_json::to_value(p).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "id", 1, "positions")?;
        if !accounts.contains_key(p.account.as_str())
            || ![
                "loan",
                "security",
                "deposit",
                "funding",
                "repo",
                "reverse_repo",
            ]
            .contains(&p.kind.as_str())
        {
            return Err("invalid position account or kind".into());
        }
        if p.start_day > days
            || (p.start_day != 0
                && (p.allowance != 0.
                    || p.opening_accrued != 0.
                    || p.encumbered_fraction != 0.
                    || !p.collateral_position.is_empty()))
        {
            return Err(
                "forward originations cannot carry opening allowance, accrual or collateral".into(),
            );
        }
        if !["ac", "afs", "htm", "trading"].contains(&p.classification.as_str())
            || (p.kind != "security" && p.classification != "ac")
        {
            return Err("classification applies only to securities".into());
        }
        range(row, "balance commitment allowance", 0., 1e15)?;
        range(row,"floating_beta asf_weight rsf_weight lcr_outflow_weight hqla_weight annual_pd lgd watch_fraction migration_rate draw_fraction monthly_runoff uninsured_fraction concentration operational_fraction digital_fraction rollover eligible_fraction encumbered_fraction haircut lcr_inflow_weight",0.,1.)?;
        range(row, "risk_weight watch_pd_multiplier duration", 0., 100.)?;
        range(row, "rate", -1., 2.)?;
        if !["level1", "level2a"].contains(&p.hqla_level.as_str()) {
            return Err("unsupported HQLA level".into());
        }
        if p.payment_interval_days < 1 {
            return Err("payment_interval_days must be positive".into());
        }
        if p.balance + p.book_adjustment < p.allowance || p.opening_accrued < 0. {
            return Err("invalid opening carrying value or accrued interest".into());
        }
        range(row, "opening_market_price", 0.000001, 100.)?;
        if p.kind != "security" && p.opening_market_price != 1. {
            return Err("opening market price applies only to securities".into());
        }
        if p.balance == 0. && p.book_adjustment != 0. {
            return Err("book adjustment requires positive principal".into());
        }
        if p.allowance > p.balance
            || (p.kind != "loan"
                && (p.allowance != 0.
                    || p.commitment != 0.
                    || p.annual_pd != 0.
                    || p.migration_rate != 0.))
        {
            return Err(
                "credit inputs apply only to loans; allowance cannot exceed balance".into(),
            );
        }
        if p.eligible_fraction != 0. && (p.kind != "security" || p.collateral_pool.is_empty()) {
            return Err("funding collateral requires a security and explicit pool ID".into());
        }
        if !p.collateral_position.is_empty() || p.pledged_face != 0. {
            let valid = positions
                .get(p.collateral_position.as_str())
                .is_some_and(|c| c.kind == "security" && c.account == p.account);
            if !["repo", "funding"].contains(&p.kind.as_str()) || !valid || p.pledged_face <= 0. {
                return Err("opening secured funding requires owned security collateral and positive pledged face".into());
            }
        }
    }
    let mut pledged: BTreeMap<&str, f64> = BTreeMap::new();
    for p in &spec.positions {
        if !p.collateral_position.is_empty() {
            *pledged.entry(&p.collateral_position).or_default() += p.pledged_face;
        }
    }
    for p in &spec.positions {
        if pledged.get(p.id.as_str()).copied().unwrap_or(0.)
            > p.balance * p.encumbered_fraction + 1e-8
        {
            return Err("opening funding links exceed encumbered collateral".into());
        }
    }
    for n in &spec.netting_sets {
        let value = serde_json::to_value(n).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "id", 1, "netting_sets")?;
        if !accounts.contains_key(n.account.as_str()) || n.counterparty.trim().is_empty() {
            return Err("netting set needs an account and counterparty".into());
        }
        range(
            row,
            "posted_margin received_margin stress_loss margin_threshold",
            0.,
            1e15,
        )?;
        range(row, "annual_pd lgd", 0., 1.)?;
        range(row, "wrong_way_multiplier risk_weight", 0., 100.)?;
    }
    for p in &spec.policies {
        let value = serde_json::to_value(p).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "id", 1, "policies")?;
        if !accounts.contains_key(p.account.as_str())
            || !["sell", "secured_funding", "transfer", "cut_dividend"].contains(&p.kind.as_str())
        {
            return Err("invalid policy account or kind".into());
        }
        range(row, "limit htm_sale_limit", 0., 1e15)?;
        range(row, "execution_cost", 0., 0.99)?;
        range(row, "funding_rate", 0., 2.)?;
        if p.funding_tenor_days < 1 {
            return Err("funding tenor must be positive".into());
        }
        if ["sell", "secured_funding"].contains(&p.kind.as_str())
            && !positions
                .get(p.position.as_str())
                .is_some_and(|v| v.account == p.account && v.kind == "security")
        {
            return Err("sale/funding policy must reference an owned security".into());
        }
        if p.kind == "transfer"
            && !accounts.get(p.destination.as_str()).is_some_and(|a| {
                a.id != p.account && a.currency == accounts[p.account.as_str()].currency
            })
        {
            return Err(
                "transfer needs a distinct account in the same currency; no implicit FX".into(),
            );
        }
    }
    for v in &spec.scenarios {
        let value = serde_json::to_value(v).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "name", 1, "scenarios")?;
        if v.start_day < 1 {
            return Err("scenario start_day must be positive".into());
        }
        range(row, "rate_shift spread_shift", -0.5, 0.5)?;
        range(
            row,
            "deposit_flight lgd_add rollover_loss haircut_add",
            0.,
            1.,
        )?;
        range(
            row,
            "pd_multiplier migration_multiplier draw_multiplier market_shock",
            0.,
            20.,
        )?;
        if v.name == "baseline" {
            return Err("baseline is a reserved scenario name".into());
        }
    }
    if spec.reverse_severities.len() > 12
        || spec
            .reverse_severities
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=5.0).contains(v))
    {
        return Err("reverse_severities must contain at most 12 finite values in [0, 5]".into());
    }
    if spec.reverse_severities.windows(2).any(|w| w[0] >= w[1]) {
        return Err("reverse_severities must be unique and increasing".into());
    }
    if spec.cashflows.len() > 250000 {
        return Err("cashflows must be a bounded list".into());
    }
    let scenario_names: BTreeSet<_> = std::iter::once("baseline")
        .chain(spec.scenarios.iter().map(|s| s.name.as_str()))
        .collect();
    let mut unique = BTreeSet::new();
    let mut totals: BTreeMap<&str, BTreeMap<&str, f64>> = BTreeMap::new();
    for f in &spec.cashflows {
        let value = serde_json::to_value(f).map_err(|e| e.to_string())?;
        let row = &value;
        fields(std::slice::from_ref(row), "position", 1, "cashflows")?;
        // Cashflows have composite identity rather than a standalone ID.
        let p = positions
            .get(f.position.as_str())
            .ok_or("invalid cashflow position, date or scenario")?;
        if f.day < 1
            || f.day > days
            || (f.scenario != "all" && !scenario_names.contains(f.scenario.as_str()))
        {
            return Err("invalid cashflow position, date or scenario".into());
        }
        if f.principal < 0. {
            return Err("principal repayment cannot be negative".into());
        }
        if f.day < p.start_day {
            return Err("cashflow precedes origination".into());
        }
        if !unique.insert((&f.position, f.day, &f.scenario)) {
            return Err("duplicate dated cashflow".into());
        }
        *totals
            .entry(&f.position)
            .or_default()
            .entry(&f.scenario)
            .or_default() += f.principal;
    }
    for (id, labels) in totals {
        if labels.contains_key("all") && labels.len() > 1 {
            return Err("all-scenario cashflows cannot overlap scenario-specific schedules".into());
        }
        if !labels.contains_key("all")
            && labels.keys().copied().collect::<BTreeSet<_>>() != scenario_names
        {
            return Err("scheduled positions require baseline and every scenario".into());
        }
        if labels.values().any(|v| *v > positions[id].balance + 1e-7) {
            return Err("scheduled principal exceeds opening principal".into());
        }
        if positions[id].commitment != 0. {
            return Err(
                "scheduled drawn facilities require a separate position for future draws".into(),
            );
        }
    }
    if spec.ruleset != RULES {
        return Err("unsupported ruleset; no inferred regulatory certification".into());
    }
    let runs = 1 + spec.scenarios.len() * (1 + spec.reverse_severities.len());
    let work = days
        .checked_mul(
            positions.len() + spec.netting_sets.len() + accounts.len() + spec.policies.len(),
        )
        .and_then(|n| n.checked_mul(runs))
        .ok_or("stress work budget overflow")?;
    if work > max_work {
        return Err("stress work budget exceeded; reduce cohorts, horizon or scenario grid".into());
    }
    for a in &spec.accounts {
        let (mut assets, mut liabilities) = (a.cash, 0.);
        for p in &spec.positions {
            if p.account == a.id && p.start_day == 0 {
                if ["loan", "security", "reverse_repo"].contains(&p.kind.as_str()) {
                    let carrying = if p.kind == "security"
                        && ["afs", "trading"].contains(&p.classification.as_str())
                    {
                        p.balance * p.opening_market_price
                    } else {
                        p.balance + p.book_adjustment
                    };
                    assets += carrying - p.allowance + p.opening_accrued;
                } else {
                    liabilities += p.balance + p.book_adjustment + p.opening_accrued;
                }
            }
        }
        for n in &spec.netting_sets {
            if n.account == a.id {
                assets += n.fair_value.max(0.) + n.posted_margin;
                liabilities += (-n.fair_value).max(0.) + n.received_margin;
            }
        }
        if (assets - liabilities - a.equity).abs() > 1e-8 * 1_f64.max(assets).max(liabilities) {
            return Err(format!(
                "opening balance sheet does not reconcile for {}",
                a.id
            ));
        }
    }
    Ok(())
}
