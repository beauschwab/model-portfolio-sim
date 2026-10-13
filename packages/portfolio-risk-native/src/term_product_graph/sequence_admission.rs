fn exact<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_vec(a).map_err(|e| e.to_string())?
        == serde_json::to_vec(b).map_err(|e| e.to_string())?)
}
fn sequence_immutable(
    map: &mut HashMap<(String, String), Value>,
    id: &term_enhancements::Identity,
    value: Value,
) -> Result<(), String> {
    let key = (id.id.clone(), id.revision.clone());
    if let Some(old) = map.get(&key) {
        if !exact(old, &value)? {
            return Err("immutable sequence contract/policy/action source identity reused with different content".into());
        }
    } else {
        map.insert(key, value);
    }
    Ok(())
}
fn sequence_known(
    r: &Request,
    p: &Position,
    c: &CdSequenceKnown,
    last: i32,
    map: &mut HashMap<(String, String), Value>,
) -> Result<(), String> {
    if c.model_version != term_enhancements::MODEL_VERSION
        || c.currency != r.currency
        || c.opening.position_id != p.position_id
        || c.opening.last_accrual_ordinal != r.as_of_ordinal
        || c.tail_end_ordinal < last
        || c.opening.maturity_ordinal > c.tail_end_ordinal
        || c.policies.len() > cd_sequence::MAX_LIVE
        || c.authorized_new_contracts.len() > cd_sequence::MAX_LIVE
    {
        return Err("known CD sequence identity/version/calendar/tail admission".into());
    }
    term_enhancements::domain(r.as_of_ordinal, c.tail_end_ordinal)?;
    let mut metadata = 0;
    term_enhancements::identity(&c.opening.contract, &mut metadata)?;
    sequence_immutable(map, &c.opening.contract, canonical(&c.opening)?)?;
    let mut positions = HashSet::new();
    let mut contracts = HashMap::from([(c.opening.position_id.as_str(), &c.opening.contract)]);
    let mut policy_positions = HashSet::new();
    positions.insert(c.opening.position_id.as_str());
    for t in &c.authorized_new_contracts {
        portfolio_compute_control::checkpoint()?;
        if !positions.insert(t.position_id.as_str())
            || t.maturity_ordinal <= r.as_of_ordinal
            || t.maturity_ordinal > c.tail_end_ordinal
            || !t.annual_rate.is_finite()
        {
            return Err("authorized CD renewal terms duplicate or outside complete tail".into());
        }
        term_enhancements::identity(&t.contract, &mut metadata)?;
        term_enhancements::text(&t.position_id, &mut metadata)?;
        sequence_immutable(map, &t.contract, canonical(t)?)?;
        contracts.insert(t.position_id.as_str(), &t.contract);
    }
    for b in &c.policies {
        portfolio_compute_control::checkpoint()?;
        if !positions.contains(b.position_id.as_str())
            || !policy_positions.insert(&b.position_id)
            || b.policy.penalty_waivers.len() > 256
        {
            return Err("known CD policy universe duplicate/unowned".into());
        }
        let id = *contracts
            .get(b.position_id.as_str())
            .ok_or("authorized policy owner missing")?;
        if b.contract != *id {
            return Err("known CD policy/contract revision mismatch".into());
        }
        term_enhancements::identity(&b.policy.identity, &mut metadata)?;
        sequence_immutable(map, &b.policy.identity, canonical(&b.policy)?)?;
    }
    Ok(())
}
fn sequence_history(
    p: &Position,
    c: &CdSequenceKnown,
    q: &cd_sequence::Request,
    run: &str,
    measure: CashflowMeasure,
    ctx: &mut HistoryContext<'_>,
) -> Result<usize, String> {
    if q.schema != cd_sequence::SCHEMA
        || q.measure != measure
        || q.scenario_id != run
        || q.currency != c.currency
        || q.monetary_unit != "currency_units"
        || q.direction != c.direction
        || q.as_of_ordinal != ctx.graph.as_of_ordinal
        || q.reporting_end_ordinal != ctx.last
        || q.tail_end_ordinal != c.tail_end_ordinal
        || !exact(&q.opening, &c.opening)?
    {
        return Err(
            "CD sequence history changes original known terms or explicit measure/run/report"
                .into(),
        );
    }
    let policies: HashMap<_, _> = c
        .policies
        .iter()
        .map(|b| (b.position_id.as_str(), b))
        .collect();
    let terms: HashMap<_, _> = c
        .authorized_new_contracts
        .iter()
        .map(|t| (t.position_id.as_str(), t))
        .collect();
    let mut metadata = 0;
    term_enhancements::identity(&q.terminal_projection_source, &mut metadata)?;
    for b in &q.policies {
        portfolio_compute_control::checkpoint()?;
        let authorized = policies
            .get(b.position_id.as_str())
            .copied()
            .ok_or("CD history policy not authorized")?;
        if !exact(b, authorized)? {
            return Err("CD sequence policy source content changed".into());
        }
    }
    for a in &q.actions {
        portfolio_compute_control::checkpoint()?;
        term_enhancements::identity(&a.source, &mut metadata)?;
        sequence_immutable(
            ctx.fixings,
            &a.source,
            json!([a.position_id, a.ordinal, a.election]),
        )?;
        if let Election::Maturity {
            renewal, switching, ..
        } = &a.election
        {
            for target in renewal.iter().chain(switching) {
                let t = terms
                    .get(target.position_id.as_str())
                    .copied()
                    .ok_or("CD renewal/switch contract not in authorized complete terms")?;
                if !exact(t, &**target)? {
                    return Err("CD authorized renewal/switch terms changed".into());
                }
                if ctx.owners.contains(target.position_id.as_str()) {
                    return Err(
                        "CD sequence new contract collides with another owned original position"
                            .into(),
                    );
                }
                if let Some(owner) = ctx.child_owners.get(&target.position_id) {
                    if owner != &p.position_id {
                        return Err(
                            "CD future contract assigned to multiple original owners".into()
                        );
                    }
                } else {
                    ctx.child_owners
                        .insert(target.position_id.clone(), p.position_id.clone());
                }
            }
        }
    }
    q.actions
        .len()
        .checked_mul(16)
        .and_then(|n| {
            q.policies
                .len()
                .checked_mul(4)
                .and_then(|x| n.checked_add(x))
        })
        .and_then(|n| n.checked_add(c.policies.len()))
        .and_then(|n| n.checked_add(c.authorized_new_contracts.len()))
        .and_then(|n| n.checked_add(4))
        .ok_or("sequence source row bound overflow".into())
}
fn sequence_used_universe(r: &Request) -> Result<(), String> {
    for p in &r.positions {
        if let KnownProduct::CdSequence { contract: c } = &p.known {
            let mut policies = HashSet::new();
            let mut contracts = HashSet::new();
            let original = r.original_base.as_ref().and_then(|o| {
                o.histories
                    .iter()
                    .find(|owner| owner.position_id == p.position_id)
            });
            for set in p
                .histories
                .iter()
                .chain(original.into_iter().map(|o| &o.histories))
            {
                for paths in [
                    &set.physical,
                    &set.pricing,
                    &set.pricing_down,
                    &set.pricing_up,
                ] {
                    for path in paths {
                        portfolio_compute_control::checkpoint()?;
                        if let ProductHistory::CdSequence { request: q } = &path.product {
                            policies.extend(q.policies.iter().map(|b| b.position_id.as_str()));
                            for action in &q.actions {
                                if let Election::Maturity {
                                    renewal, switching, ..
                                } = &action.election
                                {
                                    contracts.extend(
                                        renewal
                                            .iter()
                                            .chain(switching)
                                            .map(|t| t.position_id.as_str()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if policies.len() != c.policies.len()
                || contracts.len() != c.authorized_new_contracts.len()
            {
                return Err(
                    "unused known CD policy/authorized renewal terms are contradictory".into(),
                );
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn original_admission(
    r: &Request,
    last: i32,
    owners: &HashSet<&str>,
    fixings: &mut HashMap<(String, String), Value>,
    child_owners: &mut HashMap<String, String>,
    rows: &mut usize,
    work: &mut usize,
    runs: &mut usize,
) -> Result<(), String> {
    let original = r
        .original_base
        .as_ref()
        .ok_or("original complete base source required")?;
    let scenario = &original.scenario;
    label(&scenario.scenario_id)?;
    label(&scenario.run_id)?;
    number(scenario.severity, 0., 100.)?;
    if scenario.collateral_curve.currency != r.currency
        || !matches!(scenario.collateral_curve.role, CurveRole::Collateral { .. })
        || original.histories.len() != r.positions.len()
    {
        return Err("original graph scenario/owner/curve coverage mismatch".into());
    }
    scenario.collateral_curve.validate()?;
    for (p, original_owner) in r.positions.iter().zip(&original.histories) {
        portfolio_compute_control::checkpoint()?;
        if original_owner.position_id != p.position_id
            || original_owner.histories.scenario_id != scenario.scenario_id
        {
            return Err("original source ordered owner/scenario coverage mismatch".into());
        }
        let mut paired = None;
        for (role, paths) in [
            (Role::Physical, &original_owner.histories.physical),
            (Role::Pricing, &original_owner.histories.pricing),
            (Role::PricingDown, &original_owner.histories.pricing_down),
            (Role::PricingUp, &original_owner.histories.pricing_up),
        ] {
            if paths.is_empty() || paths.len() > 128 {
                return Err("original path admission exceeded".into());
            }
            let mut ids = HashSet::new();
            let mut weight = 0.;
            for path in paths {
                portfolio_compute_control::checkpoint()?;
                label(&path.path_id)?;
                number(path.weight, 0., 1.)?;
                if !ids.insert(&path.path_id) {
                    return Err("duplicate original source path".into());
                }
                weight += path.weight;
                if let ProductHistory::CdSequence { request: q } = &path.product {
                    if q.actions.len() > cd_sequence::MAX_ACTIONS
                        || q.policies.len() > cd_sequence::MAX_LIVE
                    {
                        return Err("original sequence dimensions exceed admission".into());
                    }
                    count_checked(q, cd_sequence::MAX_RAW_BYTES)?;
                }
                let n = history(
                    p,
                    path,
                    &scenario.run_id,
                    role,
                    &mut HistoryContext {
                        graph: r,
                        last,
                        fixings,
                        owners,
                        child_owners,
                    },
                )?;
                *rows = rows.checked_add(n).ok_or("original rows overflow")?;
                *work = work
                    .checked_add(
                        n.checked_mul(12)
                            .ok_or("original work multiplication overflow")?,
                    )
                    .ok_or("original work overflow")?;
                *runs = runs.checked_add(1).ok_or("original runs overflow")?;
                if *rows > MAX_ROWS || *work > MAX_WORK || *runs > MAX_RUNS {
                    return Err("term graph aggregate original/current run/row/work admission exceeded before projection".into());
                }
            }
            if (weight - 1.).abs() > 1e-12 {
                return Err("original probabilities must sum to one; no normalization".into());
            }
            if role != Role::Physical {
                let pairs: Vec<_> = paths.iter().map(|h| (&h.path_id, h.weight)).collect();
                if let Some(ref old) = paired {
                    if !exact(old, &pairs)? {
                        return Err("original Q/down/up ordered paired IDs/weights differ".into());
                    }
                } else {
                    paired = Some(pairs);
                }
            }
        }
    }
    Ok(())
}
