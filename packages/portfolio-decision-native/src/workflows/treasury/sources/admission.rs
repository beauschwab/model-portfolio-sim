use super::*;

pub fn preparation_bound(r: &Request) -> Result<usize, String> {
    let mut total = 0;
    for v in &r.reviews {
        let s = r
            .sources
            .iter()
            .find(|s| s.source_id == v.source_id)
            .ok_or("source review")?;
        let (days, origins, postings) = match &s.input {
            SourceInput::Mortgage(raw) => {
                let path = raw
                    .owners
                    .get(v.owner_index)
                    .ok_or("owner review")?
                    .runs
                    .get(v.run_index)
                    .ok_or("run review")?
                    .paths
                    .get(v.path_index)
                    .ok_or("path review")?;
                let source = raw
                    .sources
                    .iter()
                    .find(|s| s.source_id == path.source_id)
                    .ok_or("registry review")?;
                let a = portfolio_model_core::mortgage_pending_accounting::audit(
                    &source.request,
                    &mut || portfolio_compute_control::checkpoint(),
                )?;
                (
                    a.days,
                    add(35, mul(15, a.interval_count)?)?,
                    mul(4, a.max_groups)?,
                )
            }
            SourceInput::Product(raw) => {
                let p = raw
                    .product_owners
                    .get(v.owner_index)
                    .ok_or("owner review")?
                    .runs
                    .get(v.run_index)
                    .ok_or("run review")?
                    .paths
                    .get(v.path_index)
                    .ok_or("path review")?;
                let c = serde_json::to_value(&p.request).map_err(|e| e.to_string())?;
                let days = c["request"]["contract"]["horizon_days"]
                    .as_u64()
                    .ok_or("source horizon")? as usize;
                if c["kind"] == "card_v2" {
                    let raw: portfolio_model_core::card_credit_accounting_v2::Request =
                        serde_json::from_value(c["request"].clone()).map_err(|e| e.to_string())?;
                    let a = portfolio_model_core::card_credit_accounting_v2::audit_request(&raw)?;
                    (days, 19, mul(5, a.max_events)?)
                } else if c["kind"] == "installment" {
                    let raw: portfolio_model_core::consumer_credit_accounting::Request =
                        serde_json::from_value(c["request"].clone()).map_err(|e| e.to_string())?;
                    let reviews = add(raw.chargeoffs.len(), raw.allowance_reviews.len())?;
                    let intents = add(
                        add(
                            mul(days, 2)?,
                            mul(6, add(raw.contract.steps.len(), reviews)?)?,
                        )?,
                        96,
                    )?;
                    (days, 17, mul(4, intents)?)
                } else {
                    return fail("unsupported source channel ownership");
                }
            }
            SourceInput::DepositPortfolio(raw) => {
                let (_, p) = portfolio_source(raw, v)?;
                let days = p.source.continuation.original.horizon_days as usize;
                (days, add(days, 3)?, add(mul(days, 6)?, 8)?)
            }
            SourceInput::DepositContinuation(raw) => {
                let run = raw.runs.get(v.run_index).ok_or("continuation run review")?;
                let days = run.source.continuation.original.horizon_days as usize;
                (days, add(days, 3)?, add(mul(days, 6)?, 8)?)
            }
            SourceInput::Deposit(raw) => {
                let days = raw.source.original.horizon_days as usize;
                (days, add(days, 3)?, mul(days, 5)?)
            }
        };
        if days
            > if matches!(s.input, SourceInput::DepositPortfolio(_)) {
                ftp::PORTFOLIO_MAX_DAYS
            } else {
                64
            }
            || origins
                > if matches!(s.input, SourceInput::DepositPortfolio(_)) {
                    368
                } else {
                    128
                }
        {
            return fail("pre-generation complete source/origin bound exceeds funding scope");
        }
        let states = add(days, 1)?;
        let events = add(days, postings)?;
        let bound = add(
            add(mul(states, 18 * 16)?, mul(mul(events, states)?, 20)?)?,
            add(mul(mul(states, origins)?, 80)?, mul(v.resets.len(), 16)?)?,
        )?;
        total = add(total, bound)?;
    }
    if total > preparation_limit(r) {
        return fail(if preparation_limit(r) == 2_000_000 {
            "aggregate 2M preparation bound exceeded before source generation"
        } else {
            "aggregate source2 8M preparation bound exceeded before source generation"
        });
    }
    Ok(total)
}

pub fn admit(r: &Request) -> Result<Vec<(usize, usize)>, String> {
    if r.schema != SCHEMA
        || r.sources.is_empty()
        || r.sources.len() > 4
        || r.reviews.len() > 16
        || serde_json::to_vec(r).map_err(|e| e.to_string())?.len() > 1024 * 1024
    {
        return fail("bounded source adapter version/shape/bytes required");
    }
    // A job/source ID records lineage, not a second economic owner.  Only the
    // explicit new whole-portfolio tier has this additional overlap contract.
    let mut portfolio_owners = BTreeSet::new();
    for source in &r.sources {
        if let SourceInput::DepositPortfolio(raw) = &source.input {
            for owner in &raw.positions {
                portfolio_compute_control::checkpoint()?;
                let account = raw.specification["accounts"]
                    .as_array()
                    .ok_or("portfolio source accounts")?
                    .iter()
                    .find(|a| a["id"] == owner.account_id)
                    .ok_or("portfolio source account")?;
                let entity = account["entity"]
                    .as_str()
                    .ok_or("portfolio source entity")?;
                let currency = account["currency"]
                    .as_str()
                    .ok_or("portfolio source currency")?;
                if !portfolio_owners.insert((entity, currency, owner.position_id.as_str())) {
                    return fail("overlapping source2 economic owner across source jobs");
                }
            }
        }
    }
    let mut ids = BTreeSet::new();
    let mut review_keys = BTreeSet::new();
    let mut costs = Vec::new();
    let mut work = 0;
    let mut bytes = 0;
    for s in &r.sources {
        portfolio_compute_control::checkpoint()?;
        text(&s.source_id)?;
        id(&s.scope_review)?;
        let expected = match s.input {
            SourceInput::Mortgage(_) => {
                "exclusive_mortgage_owners_with_inherited_dated_sources_retained_not_funded-1"
            }
            SourceInput::Product(_) => {
                "exclusive_product_owners_with_inherited_dated_sources_retained_not_funded-1"
            }
            SourceInput::Deposit(_) => "whole_isolated_deposit_owner-1",
            SourceInput::DepositContinuation(_) => "whole_isolated_continuation_deposit_owner-1",
            SourceInput::DepositPortfolio(_) => "whole_performing_deposit_portfolio-2",
        };
        if s.scope != expected {
            return fail("explicit reviewed funding scope required");
        }
        if !ids.insert(&s.source_id) {
            return fail("duplicate source id");
        }
        let cost = match &s.input {
            SourceInput::Mortgage(raw) => {
                let c = portfolio_ledger_native::dated_ledger::CompiledDatedLedger::compile(
                    raw.dated.clone(),
                    512,
                    8_000_000,
                )?;
                let a = mortgage::audit(raw, &c)?;
                (a.work_units, a.reserved_bytes)
            }
            SourceInput::Product(raw) => {
                product::audit_owner_reservation(raw)?;
                for o in &raw.product_owners {
                    for run in &o.runs {
                        for p in &run.paths {
                            let v = serde_json::to_value(&p.request).map_err(|e| e.to_string())?;
                            if !matches!(v["kind"].as_str(), Some("installment" | "card_v2"))
                                || v["request"]["contract"]["horizon_days"]
                                    .as_u64()
                                    .is_none_or(|n| n > 64)
                            {
                                return fail("source adapter requires complete <=64day installment/card_v2 ownership");
                            }
                        }
                    }
                }

                let g = portfolio_risk_native::product_units::audit_source_work(
                    &raw.performing.candidate.product,
                )?;
                // Existing coordinator ceiling plus independently admitted full graph cost.
                (add(8_000_000, g)?, 128 * 1024 * 1024)
            }
            SourceInput::DepositPortfolio(raw) => {
                let a = portfolio::audit(raw)?;
                (a.work_units, a.reserved_bytes)
            }
            SourceInput::DepositContinuation(raw) => {
                let a = continuation::audit(raw)?;
                (a.work_units, a.reserved_bytes)
            }
            SourceInput::Deposit(raw) => {
                let a = portfolio_model_core::deposit_revised_income::audit_request(&raw.source)?;
                let n = raw.source.runs.len();
                let days = raw.source.original.horizon_days as usize + 1;
                (
                    add(a.work_units, mul(mul(n, days)?, 8208)?)?,
                    add(a.reserved_bytes, mul(mul(n, days)?, 65536)?)?,
                )
            }
        };
        work = add(work, cost.0)?;
        bytes = add(bytes, cost.1)?;
        costs.push(cost);
    }
    // Source-to-channel conversion/primitive arrays plus metadata copies, charged before execution.
    work = add(work, conversion_work(r)?)?;
    bytes = add(bytes, path_capacity(r)?)?;
    bytes = add(bytes, 2 * MAX_OUTPUT)?;
    bytes = add(
        bytes,
        mul(serde_json::to_vec(r).map_err(|e| e.to_string())?.len(), 8)?,
    )?;
    if work > MAX_SOURCE_WORK || bytes > MAX_CAPACITY {
        return fail("aggregate source/conversion work/capacity exceeded");
    }
    for v in &r.reviews {
        if !ids.contains(&v.source_id)
            || !review_keys.insert((&v.source_id, v.owner_index, v.run_index, v.path_index))
        {
            return fail("unknown/duplicate source path review");
        }
        text(&v.entity)?;
        text(&v.currency)?;
        let source = r
            .sources
            .iter()
            .find(|s| s.source_id == v.source_id)
            .ok_or("source review")?;
        let (spec, account) = match &source.input {
            SourceInput::Mortgage(r) => (
                &r.dated.specification,
                r.owners
                    .get(v.owner_index)
                    .ok_or("owner review index")?
                    .account_id
                    .as_str(),
            ),
            SourceInput::Product(r) => (
                &r.combined_specification,
                r.product_owners
                    .get(v.owner_index)
                    .ok_or("owner review index")?
                    .account_id
                    .as_str(),
            ),
            SourceInput::Deposit(r) => (&r.specification, r.account_id.as_str()),
            SourceInput::DepositContinuation(r) => (&r.specification, r.account_id.as_str()),
            SourceInput::DepositPortfolio(r) => (
                &r.specification,
                r.positions
                    .get(v.owner_index)
                    .ok_or("portfolio owner review")?
                    .account_id
                    .as_str(),
            ),
        };
        let a = spec["accounts"]
            .as_array()
            .ok_or("source accounts")?
            .iter()
            .find(|a| a["id"] == account)
            .ok_or("source account")?;
        if a["entity"] != v.entity || a["currency"] != v.currency {
            return fail("entity/currency review must match exact actual source account");
        }
        let (start, end) = match &source.input {
            SourceInput::Mortgage(r) => {
                let p = r.owners[v.owner_index]
                    .runs
                    .get(v.run_index)
                    .ok_or("run review")?
                    .paths
                    .get(v.path_index)
                    .ok_or("path review")?;
                let q = r
                    .sources
                    .iter()
                    .find(|s| s.source_id == p.source_id)
                    .ok_or("registry review")?;
                let a = portfolio_model_core::mortgage_pending_accounting::audit(
                    &q.request,
                    &mut || portfolio_compute_control::checkpoint(),
                )?;
                (
                    q.request.source.contract.as_of_ordinal,
                    a.reserved_end_ordinal,
                )
            }
            SourceInput::Product(r) => {
                let p = r.product_owners[v.owner_index]
                    .runs
                    .get(v.run_index)
                    .ok_or("run review")?
                    .paths
                    .get(v.path_index)
                    .ok_or("path review")?;
                let q = serde_json::to_value(&p.request).map_err(|e| e.to_string())?;
                let c = &q["request"]["contract"];
                let start = day(c, "as_of_ordinal")?;
                (
                    start,
                    start
                        .checked_add(day(c, "horizon_days")?)
                        .ok_or("reset clock overflow")?,
                )
            }
            SourceInput::DepositPortfolio(r) => {
                let (_, p) = portfolio_source(r, v)?;
                let c = &p.source.continuation.original;
                (
                    c.as_of_ordinal,
                    c.as_of_ordinal
                        .checked_add(c.horizon_days as i32)
                        .ok_or("reset clock overflow")?,
                )
            }
            SourceInput::DepositContinuation(r) => {
                if v.owner_index != 0 || v.path_index != 0 {
                    return fail("single continuation deposit owner/path");
                }
                let c = &r
                    .runs
                    .get(v.run_index)
                    .ok_or("run review")?
                    .source
                    .continuation
                    .original;
                (
                    c.as_of_ordinal,
                    c.as_of_ordinal
                        .checked_add(c.horizon_days as i32)
                        .ok_or("reset clock overflow")?,
                )
            }
            SourceInput::Deposit(r) => {
                let c = &r.source.original;
                (
                    c.as_of_ordinal,
                    c.as_of_ordinal
                        .checked_add(c.horizon_days as i32)
                        .ok_or("reset clock overflow")?,
                )
            }
        };
        if matches!(source.input, SourceInput::DepositPortfolio(_)) {
            ftp::validate_portfolio_policy_before_source(&v.policy, &v.resets, start, end)?;
        } else {
            ftp::validate_policy_before_source(&v.policy, &v.resets, start, end)?;
        }
        id(&v.cash_exclusion_review)?;
        id(&v.channel_change_review)?;
        id(&v.income_review)?;
        if v.cash_assignment != "actual_cash_retained_excluded_from_channel_rollforward-1" {
            return fail(
                "explicit reviewed cash exclusion required; no inferred liquidation assignment",
            );
        }
        for lane in v.other_income.values() {
            if !ftp::INCOME_LANES[1..].contains(&lane.as_str()) {
                return fail("only native original EIR may enter recurring external lane");
            }
        }
    }
    // Exact, complete path coverage, including zero participation; no caller path selection pruning.
    for s in &r.sources {
        let expected = path_keys(&s.input).into_iter().collect::<BTreeSet<_>>();
        let given = r
            .reviews
            .iter()
            .filter(|v| v.source_id == s.source_id)
            .map(|v| (v.owner_index, v.run_index, v.path_index))
            .collect::<BTreeSet<_>>();
        if expected != given {
            return fail("complete unweighted source path review coverage required");
        }
    }
    preparation_bound(r)?;
    Ok(costs)
}

/// Tier2 is selected solely by the explicit raw source kind, never by returned shapes.
pub fn preparation_limit(r: &Request) -> usize {
    if r.sources
        .iter()
        .any(|s| matches!(s.input, SourceInput::DepositPortfolio(_)))
    {
        ftp::PORTFOLIO_MAX_WORK
    } else {
        2_000_000
    }
}
pub fn charge_states(r: &Request) -> Result<usize, String> {
    let mut states = 0;
    for v in &r.reviews {
        portfolio_compute_control::checkpoint()?;
        let s = r
            .sources
            .iter()
            .find(|s| s.source_id == v.source_id)
            .ok_or("source review")?;
        let n = if let SourceInput::DepositPortfolio(raw) = &s.input {
            let (_, p) = portfolio_source(raw, v)?;
            add(p.source.continuation.original.horizon_days as usize, 1)?
        } else {
            65
        };
        states = add(states, n)?;
    }
    Ok(states)
}
pub fn conversion_work(r: &Request) -> Result<usize, String> {
    let mut work = mul(charge_states(r)?, 18 * 256)?;
    for s in &r.sources {
        if let SourceInput::DepositPortfolio(raw) = &s.input {
            portfolio_compute_control::checkpoint()?;
            work = add(
                work,
                mul(
                    serde_json::to_vec(raw).map_err(|e| e.to_string())?.len(),
                    16,
                )?,
            )?;
        }
    }
    for review in &r.reviews {
        portfolio_compute_control::checkpoint()?;
        let s = r
            .sources
            .iter()
            .find(|s| s.source_id == review.source_id)
            .ok_or("source review")?;
        if let SourceInput::DepositPortfolio(raw) = &s.input {
            let (_, p) = portfolio_source(raw, review)?;
            let days = p.source.continuation.original.horizon_days as usize;
            let states = add(days, 1)?;
            // One origin visit per emitted state plus indexed FIFO allocation
            // updates. Counted source FIFO has at most I+C allocations; I<=H,
            // C<=3H+1 in this one-segment no-inflow/no-migration source tier.
            // Extra complete-native cash-origin scans include all mapped paths.
            work = add(work, mul(mul(states, add(days, 3)?)?, 96)?)?;
            work = add(work, mul(mul(states, r.reviews.len())?, 64)?)?;
        }
    }
    Ok(work)
}
pub fn path_capacity(r: &Request) -> Result<usize, String> {
    let mut bytes = r.reviews.iter().try_fold(0, |total, v| {
        let s = r
            .sources
            .iter()
            .find(|s| s.source_id == v.source_id)
            .ok_or("source review")?;
        add(
            total,
            if matches!(s.input, SourceInput::DepositPortfolio(_)) {
                mul(2, ftp::PORTFOLIO_MAX_BYTES)?
            } else {
                2 * 1024 * 1024
            },
        )
    })?;
    for s in &r.sources {
        if let SourceInput::DepositPortfolio(raw) = &s.input {
            portfolio_compute_control::checkpoint()?;
            // Additional bounded typed bytes plus parsed input staging. The
            // source's actual arena use remains independently charged/measured.
            bytes = add(
                bytes,
                mul(
                    serde_json::to_vec(raw).map_err(|e| e.to_string())?.len(),
                    10,
                )?,
            )?;
        }
    }
    Ok(bytes)
}
