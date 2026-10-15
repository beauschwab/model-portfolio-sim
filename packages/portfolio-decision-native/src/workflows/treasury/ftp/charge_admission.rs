use super::*;
/// All raw bounds and policies admitted before the first source generation.
pub fn admit(r: &Request) -> Result<(usize, usize), String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA {
        return Err("explicit charge schema required".into());
    }
    if r.acquisition_phase_policy != "actual_owned_purchase_and_initial_allowance_once-1" {
        return Err("explicit complete acquisition phase required".into());
    }
    if r.dirty_unit_state != "clean_frozen_units-1" {
        return Err("CAPITAL_REFRESH_REQUIRED".into());
    }
    label(&r.as_of)?;
    review(&r.policy_review)?;
    let costs = source::admit(&r.source)?;
    let prep = source::preparation_bound(&r.source)?;
    let source_work = costs.iter().try_fold(0usize, |a, x| add(a, x.0))?;
    let raw_bytes = serde_json::to_vec(&r.source)
        .map_err(|e| e.to_string())?
        .len();
    if raw_bytes > 1024 * 1024 {
        return Err("source body1MiB".into());
    }
    let source_capacity = add(
        costs.iter().try_fold(0usize, |a, x| add(a, x.1))?,
        add(
            source::path_capacity(&r.source)?,
            add(64 * 1024 * 1024, 8 * raw_bytes)?,
        )?,
    )?;
    if source_capacity > source::MAX_CAPACITY {
        return Err("source retained256MiB".into());
    }
    // Native source outputs and final JSON are staged. Account for separate
    // charge rows/transport staging and the optional authoritative capital replay.
    // This checked capacity reservation is not a process-RSS guarantee.
    let charge_capacity = add(
        64 * 1024 * 1024,
        source::charge_states(&r.source)?
            .checked_mul(4096)
            .ok_or("capacity overflow")?,
    )?;
    let retained = add(
        source_capacity,
        add(
            charge_capacity,
            if r.capital.is_some() {
                64 * 1024 * 1024
            } else {
                0
            },
        )?,
    )?;
    if retained > MAX_RETAINED_CAPACITY {
        return Err("joined retained384MiB bound".into());
    }
    let conversion = source::conversion_work(&r.source)?;
    let mut joined = add(add(add(source_work, conversion)?, prep)?, source_work)?; // separately reserve acquisition journal scan
    let keys = r
        .source
        .reviews
        .iter()
        .map(review_binding)
        .collect::<BTreeSet<_>>();
    let mut given = BTreeSet::new();
    if r.curves.is_empty() || r.curves.len() > 16 {
        return Err("explicit bounded curves required".into());
    }
    // Unchanged Risk validates complete curve domains before generation.
    treasury::Request {
        as_of: r.as_of.clone(),
        policy_id: r.policy_review.id.clone(),
        capital: vec![],
        curves: r.curves.clone(),
        positions: vec![],
        strategy_units: vec![],
        capital_deltas: vec![],
    }
    .run()?;
    for p in &r.charges {
        review(&p.review)?;
        review(&p.cost_review)?;
        if !keys.contains(&p.binding) || !given.insert(p.binding.clone()) {
            return Err("exact once charge binding required".into());
        }
        if p.other_earnings != "retain_seven_lanes_no_second_credit_loss-1" {
            return Err("no automatic credit-loss or basis repost".into());
        }
        let sr = r
            .source
            .reviews
            .iter()
            .find(|x| review_binding(x) == p.binding)
            .ok_or("binding")?;
        let curve = r
            .curves
            .iter()
            .find(|x| x.id == p.curve_id)
            .ok_or("missing curve")?;
        if curve.entity != sr.entity || curve.currency != sr.currency {
            return Err("source curve scope mismatch".into());
        }
        for (d, method) in [
            (&p.option, "positive_incremental_daily_funding-1"),
            (&p.contingent, "positive_incremental_native_undrawn-1"),
        ] {
            review(&d.review)?;
            if d.methodology != method || !keys.contains(&d.benchmark) || d.rate_vintage != r.as_of
            {
                return Err("explicit complete benchmark/rate/vintage policy required".into());
            }
            nonnegative(d.annual_rate)?;
            if d.annual_rate > 1.0 {
                return Err("bounded annual difference rate required".into());
            }
        }
        nonnegative(p.daily_operating_cost)?;
        nonnegative(p.allocated_capital)?;
        nonnegative(p.annual_cost_of_capital)?;
        if p.annual_cost_of_capital > 1.0 {
            return Err("bounded capital cost rate required".into());
        }
    }
    if given != keys {
        return Err("complete charge coverage including zero participation required".into());
    }
    let charge_work = add(
        source::charge_states(&r.source)?
            .checked_mul(1024)
            .ok_or("charge work overflow")?,
        r.curves.len() * 256 * 64,
    )?;
    if charge_work > MAX_CHARGE_WORK {
        return Err("charge2M bound".into());
    }
    joined = add(joined, charge_work)?;
    match (&r.capital, &r.capital_source_id) {
        (None, None) => {}
        (Some(c), Some(id)) => {
            let s = r
                .source
                .sources
                .iter()
                .find(|s| s.source_id == *id)
                .ok_or("capital source binding")?;
            let source::SourceInput::Mortgage(raw) = &s.input else {
                return Err("only reviewed mortgage native closing capital supported".into());
            };
            // Exact declared DTO equality preserves binary64 signed zero via canonical bytes.
            if serde_json::to_vec(raw).map_err(|e| e.to_string())?
                != serde_json::to_vec(&c.raw_mixed).map_err(|e| e.to_string())?
            {
                return Err("capital raw source mismatch".into());
            }
            portfolio_risk_native::treasury_bridge::validate_reviewed_mortgage_inputs(
                &c.treasury,
                &c.policies,
                c.amount_multiplier,
                &c.exposure_policy,
            )?;
            if c.schema != mortgage_closing_capital::SCHEMA {
                return Err("capital schema".into());
            }
            // Raw-source replay8M and bridge2M are separately reserved, never counted as free reuse.
            joined = add(joined, 10_000_000)?;
        }
        _ => return Err("explicit capital source and raw policy must both be present".into()),
    }
    if joined > MAX_JOINED_WORK {
        return Err("joined62M bound".into());
    }
    Ok((joined, charge_work))
}
