//! Native raw-trade exposure, explicit collateral and a separately scoped closeout.
use super::*;
use portfolio_model_core::cashflow::{actual_year_fraction, ActualDayCount, CashflowMeasure};
use std::collections::{BTreeMap, HashMap, HashSet};
mod strict_disposition;
mod types;
pub use types::*;
#[derive(Clone, Default)]
struct Balance {
    vm: f64,
    received_im: f64,
    posted_im: f64,
}
mod obligations;
mod trades;
use trades::*;
#[allow(clippy::too_many_arguments)] // Typed, audited movement fields mirror the public conservation record.
fn cash_movement(
    out: &mut Vec<Movement>,
    date: i32,
    bucket: &str,
    kind: &str,
    cash: f64,
    vm: f64,
    rim: f64,
    pim: f64,
    custodian: f64,
    measure: CashflowMeasure,
) -> Result<(), String> {
    out.push(Movement {
        ordinal: date,
        bucket: bucket.into(),
        kind: kind.into(),
        entity_cash: amount(cash)?,
        vm_delta: amount(vm)?,
        received_im_delta: amount(rim)?,
        posted_im_delta: amount(pim)?,
        custodian_cash: amount(custodian)?,
        modeled_measure: measure,
    });
    Ok(())
}
pub fn evaluate(r: &CsaRequest) -> Result<CsaResult, String> {
    let p = &r.policy;
    let mut bytes = 0;
    for id in [&p.identity, &p.opening_source, &r.conditional_model, &r.crn] {
        identity(id, &mut bytes)?;
    }
    for s in [&p.entity, &p.counterparty] {
        text(s, &mut bytes)?;
    }
    domain(r.asof_ordinal, p.tail_return_ordinal)?;
    for v in [
        p.receive_threshold,
        p.post_threshold,
        p.minimum_transfer,
        p.opening_received_im,
        p.opening_posted_im,
    ] {
        nonnegative(v)?;
    }
    amount(p.opening_vm)?;
    if r.measure == CashflowMeasure::Contractual
        || p.currency.len() != 3
        || !p.currency.bytes().all(|b| b.is_ascii_uppercase())
        || p.monetary_unit != "currency_units"
        || !p.received_im_segregated
        || p.settlement_lag_calendar_days > 366
        || r.mapping.is_empty()
        || r.mapping.len() > 256
        || r.paths.is_empty()
        || r.paths.len() > 256
    {
        return Err("CSA requires explicit Physical/Pricing currency policy, segregated IM and bounded paths/mapping".into());
    }
    let mut mapping = HashMap::new();
    let mut buckets = HashSet::new();
    let mut legal: HashMap<String, String> = HashMap::new();
    for m in &r.mapping {
        for s in [
            &m.position_id,
            &m.entity,
            &m.counterparty,
            &m.netting_set_id,
        ] {
            text(s, &mut bytes)?;
        }
        identity(&m.contract, &mut bytes)?;
        identity(&m.legal_review, &mut bytes)?;
        if m.entity != p.entity
            || m.counterparty != p.counterparty
            || m.currency != p.currency
            || mapping.insert(m.position_id.clone(), m).is_some()
        {
            return Err("CSA legal mapping must exactly cover entity/counterparty/currency and unique trades".into());
        }
        let bucket = if m.enforceable_netting {
            m.netting_set_id.clone()
        } else {
            format!("trade:{}", m.position_id)
        };
        let key = serde_json::to_string(&(&m.legal_review, m.enforceable_netting))
            .map_err(|e| e.to_string())?;
        if legal.get(&bucket).is_some_and(|k| k != &key) {
            return Err("netting bucket legal reviews disagree".into());
        }
        legal.insert(bucket.clone(), key);
        buckets.insert(bucket);
    }
    // Opening collateral cannot be guessed across independent non-netted buckets.
    if buckets.len() != 1
        && (p.opening_vm != 0. || p.opening_received_im != 0. || p.opening_posted_im != 0.)
    {
        return Err("opening collateral requires a single reviewed bucket; split separate CSA requests otherwise".into());
    }
    let mut results = Vec::new();
    let mut total_prob = 0.;
    let mut cva = 0.;
    let mut fva = 0.;
    let mut opening_clean = None;
    let mut work = 0usize;
    let mut path_ids = HashSet::new();
    for path in &r.paths {
        text(&path.path_id, &mut bytes)?;
        identity(&path.source, &mut bytes)?;
        if !path_ids.insert(&path.path_id)
            || !path.probability.is_finite()
            || path.probability < 0.
            || path.nodes.is_empty()
            || path.nodes.len() > 4096
            || path.nodes[0].ordinal != r.asof_ordinal
            || path
                .nodes
                .last()
                .is_none_or(|n| n.ordinal != p.tail_return_ordinal)
        {
            return Err("CSA paths need unique IDs, complete chronological asof-to-tail nodes and probability".into());
        }
        total_prob += path.probability;
        let mut balances: BTreeMap<String, Balance> = buckets
            .iter()
            .map(|k| (k.clone(), Balance::default()))
            .collect();
        if balances.len() == 1 {
            let b = balances.values_mut().next().ok_or("missing bucket")?;
            b.vm = p.opening_vm;
            b.received_im = p.opening_received_im;
            b.posted_im = p.opening_posted_im;
        }
        let mut exposures = Vec::new();
        let mut calls: Vec<Call> = Vec::new();
        let mut movements = Vec::new();
        let mut closeouts = Vec::new();
        let mut loss_pv = 0.;
        let mut funding_pv = 0.;
        let mut keys = HashMap::new();
        let mut expiry_keys = HashMap::new();
        let mut prior: Option<&Node> = None;
        let mut default_seen = false;
        let mut closeout_seen = false;
        if let Some(d) = &path.default {
            identity(&d.legal_review, &mut bytes)?;
            identity(&d.replacement_value_policy, &mut bytes)?;
            domain(r.asof_ordinal, d.default_ordinal)?;
            domain(d.default_ordinal, d.closeout_ordinal)?;
            for v in [
                d.counterparty_recovery,
                d.posted_vm_return_recovery,
                d.posted_im_return_recovery,
            ] {
                if !v.is_finite() || !(0. ..=1.).contains(&v) {
                    return Err("explicit recovery fractions must be within [0,1]".into());
                }
            }
            if d.closeout_ordinal > p.tail_return_ordinal
                || !path.nodes.iter().any(|n| n.ordinal == d.default_ordinal)
                || !path.nodes.iter().any(|n| n.ordinal == d.closeout_ordinal)
            {
                return Err("default and complete closeout must be explicit admitted nodes".into());
            }
        }
        let mut obligation_book = path
            .default
            .as_ref()
            .and_then(|d| d.mpor.as_ref().map(|m| (d, m)))
            .map(|(d, m)| obligations::Book::new(m, d, path, r.measure, &mut bytes, &mut work))
            .transpose()?;
        for n in &path.nodes {
            domain(r.asof_ordinal, n.ordinal)?;
            if n.ordinal > p.tail_return_ordinal
                || !(0..=253402300799_i64).contains(&n.valuation_cutoff_unix_seconds)
                || 719163 + (n.valuation_cutoff_unix_seconds / 86400) as i32 != n.ordinal
                || !n.discount_from_asof.is_finite()
                || n.discount_from_asof <= 0.
                || (n.ordinal == r.asof_ordinal && n.discount_from_asof != 1.)
                || !n.collateral_rate.is_finite()
                || !n.funding_spread.is_finite()
                || prior.is_some_and(|a| a.ordinal >= n.ordinal)
            {
                return Err("invalid CSA node date/time/rates/discount ordering".into());
            }
            for id in [
                &n.discount_source,
                &n.collateral_rate_source,
                &n.funding_source,
            ] {
                identity(id, &mut bytes)?;
            }
            if let Some(a) = prior {
                let tau =
                    actual_year_fraction(a.ordinal, n.ordinal, p.collateral_interest_day_count)?;
                for (bucket, b) in &balances {
                    let interest = amount((-b.vm + b.posted_im) * a.collateral_rate * tau)?;
                    cash_movement(
                        &mut movements,
                        n.ordinal,
                        bucket,
                        "collateral_interest",
                        interest,
                        0.,
                        0.,
                        0.,
                        amount(-b.received_im * a.collateral_rate * tau)?,
                        r.measure,
                    )?;
                    let funded = (-b.vm).max(0.) + b.posted_im
                        - if p.received_vm_reusable_for_funding {
                            b.vm.max(0.)
                        } else {
                            0.
                        };
                    funding_pv = amount(
                        funding_pv + funded * a.funding_spread * tau * n.discount_from_asof,
                    )?;
                }
            }
            let is_default = path
                .default
                .as_ref()
                .is_some_and(|d| d.default_ordinal == n.ordinal);
            let is_closeout = path
                .default
                .as_ref()
                .is_some_and(|d| d.closeout_ordinal == n.ordinal);
            if is_default {
                default_seen = true;
                for call in &mut calls {
                    if call.settled_ordinal.is_none() {
                        call.cancelled_at_default = true;
                    }
                }
            }
            if !default_seen && n.settlement_available {
                for call in &mut calls {
                    if call.settled_ordinal.is_none()
                        && !call.cancelled_at_default
                        && !call.cancelled_at_tail
                        && call.due_ordinal <= n.ordinal
                    {
                        let b = balances
                            .get_mut(&call.bucket)
                            .ok_or("call missing bucket")?;
                        b.vm = amount(b.vm + call.amount)?;
                        call.settled_ordinal = Some(n.ordinal);
                        cash_movement(
                            &mut movements,
                            n.ordinal,
                            &call.bucket,
                            "variation_margin",
                            call.amount,
                            call.amount,
                            0.,
                            0.,
                            0.,
                            r.measure,
                        )?;
                    }
                }
            }
            if n.trades.len() != r.mapping.len() {
                return Err("every exposure node must contain every raw mapped trade".into());
            }
            let mut values: BTreeMap<String, Vec<(String, f64)>> =
                buckets.iter().map(|k| (k.clone(), Vec::new())).collect();
            let mut embedded_due: BTreeMap<String, f64> =
                buckets.iter().map(|k| (k.clone(), 0.)).collect();
            let mut seen = HashSet::new();
            for t in &n.trades {
                work = work.checked_add(trade_work(t)).ok_or("CSA work overflow")?;
                if work > 250000 {
                    return Err("CSA combined raw-trade work exceeds 250000".into());
                }
                let serialized = serde_json::to_vec(t).map_err(|e| e.to_string())?;
                bytes = bytes
                    .checked_add(serialized.len())
                    .ok_or("CSA metadata overflow")?;
                if bytes > 32 * 1024 * 1024 {
                    return Err("CSA combined immutable raw metadata exceeds 32 MiB".into());
                }
                let id = position(t);
                if let Trade::ExpiredEuropean {
                    terminal, election, ..
                } = t
                {
                    let expiry_key =
                        serde_json::to_string(&(terminal, election)).map_err(|e| e.to_string())?;
                    if expiry_keys.get(id).is_some_and(|old| old != &expiry_key) {
                        return Err(
                            "immutable expiry election/state cannot change after delivery".into(),
                        );
                    }
                    if !expiry_keys.contains_key(id)
                        && r.asof_ordinal < terminal.expiry_ordinal
                        && n.ordinal != terminal.expiry_ordinal
                    {
                        return Err("explicit European expiry handoff node missing".into());
                    }
                    expiry_keys.insert(id.to_string(), expiry_key);
                    if terminal.states.iter().any(|s| {
                        n.ordinal == terminal.expiry_ordinal
                            && s.underlying.valuation_cutoff_unix_seconds
                                > n.valuation_cutoff_unix_seconds
                    }) {
                        return Err(
                            "expiry handoff uses information after exact node cutoff".into()
                        );
                    }
                }
                let m = mapping.get(id).ok_or("raw trade absent legal mapping")?;
                if !seen.insert(id)
                    || contractual(t).id != m.contract.id
                    || contractual(t).revision != m.contract.revision
                {
                    return Err("duplicate/raw contractual identity mismatch".into());
                }
                let key = trade_key(t)?;
                if keys.get(id).is_some_and(|old| old != &key) {
                    return Err("raw scenario nodes cannot change contractual economics".into());
                }
                keys.insert(id.to_string(), key);
                if let Some(d) = &path.default {
                    if let Some(book) = &obligation_book {
                        book.require_dates(t, path)?;
                    } else {
                        no_interim_payment(t, d.default_ordinal, d.closeout_ordinal)?;
                    }
                }
                complete_tail(t, p.tail_return_ordinal)?;
                let bucket = if m.enforceable_netting {
                    m.netting_set_id.clone()
                } else {
                    format!("trade:{}", id)
                };
                let raw_value = priced(t, n, &p.currency, &path.path_id, &bucket)?;
                if default_seen && !closeout_seen {
                    if let Some(book) = &mut obligation_book {
                        work = work
                            .checked_add(trade_work(t))
                            .ok_or("MPOR repricing work overflow")?;
                        if work > 250000 {
                            return Err("combined native MPOR raw-trade work exceeds 250000".into());
                        }
                        let embedded = book.capture(t, n, &bucket)?;
                        let total = embedded_due.get_mut(&bucket).ok_or("missing MPOR bucket")?;
                        *total = amount(*total + embedded)?;
                    }
                }
                values
                    .get_mut(&bucket)
                    .ok_or("missing mapped bucket")?
                    .push((id.into(), if closeout_seen { 0. } else { raw_value }));
            }
            if default_seen && !closeout_seen {
                if let Some(book) = &mut obligation_book {
                    for (bucket, cash) in book.settle(n.ordinal)? {
                        cash_movement(
                            &mut movements,
                            n.ordinal,
                            &bucket,
                            "mpor_contractual_settlement",
                            cash,
                            0.,
                            0.,
                            0.,
                            0.,
                            r.measure,
                        )?;
                    }
                }
            }
            let clean = values
                .values()
                .flatten()
                .try_fold(0., |a, (_, v)| amount(a + v))?;
            if n.ordinal == r.asof_ordinal {
                if opening_clean.is_some_and(|v| !near(v, clean)) {
                    return Err("conditional paths must share the same opening clean mark".into());
                }
                opening_clean = Some(clean);
            }
            for (bucket, trades) in values {
                let v = trades.iter().try_fold(0., |a, (_, v)| amount(a + v))?;
                let replacement = amount(v - embedded_due.get(&bucket).copied().unwrap_or(0.))?;
                let unpaid = if closeout_seen {
                    0.
                } else {
                    obligation_book
                        .as_ref()
                        .map_or(Ok(0.), |book| book.unpaid(&bucket))?
                };
                let total_claim = amount(replacement + unpaid)?;
                let b = balances.get_mut(&bucket).ok_or("missing balance")?;
                exposures.push(Exposure {
                    ordinal: n.ordinal,
                    bucket: bucket.clone(),
                    clean_value: v,
                    vm: b.vm,
                    received_im: b.received_im,
                    posted_im: b.posted_im,
                    unsecured_receivable: (total_claim - b.vm.max(0.)).max(0.),
                    posted_collateral_return_claim: (-b.vm).max(0.) + b.posted_im,
                    clean_trade_values: trades,
                    extinguished_after_closeout: closeout_seen,
                    replacement_after_due_obligations: replacement,
                    unpaid_obligation_claim: unpaid,
                    total_closeout_claim: total_claim,
                });
                if is_closeout {
                    let d = path.default.as_ref().ok_or("missing closeout policy")?;
                    let mut close = closeout(total_claim, b, d, n.ordinal, &bucket)?;
                    close.clean_replacement_value = replacement;
                    close.unpaid_obligation_claim = unpaid;
                    if let Some(book) = &mut obligation_book {
                        book.close(&bucket);
                    }
                    loss_pv = amount(
                        loss_pv
                            + (close.counterparty_loss + close.posted_return_loss)
                                * n.discount_from_asof,
                    )?;
                    cash_movement(
                        &mut movements,
                        n.ordinal,
                        &bucket,
                        "default_closeout",
                        close.entity_cash,
                        -b.vm,
                        -b.received_im,
                        -b.posted_im,
                        close.custodian_return,
                        r.measure,
                    )?;
                    *b = Balance::default();
                    closeouts.push(close);
                } else if n.ordinal == p.tail_return_ordinal && !default_seen {
                    if !n.settlement_available {
                        return Err("complete tail collateral return requires explicit settlement availability".into());
                    }
                    for call in &mut calls {
                        if call.bucket == bucket && call.settled_ordinal.is_none() {
                            call.cancelled_at_tail = true;
                        }
                    }
                    cash_movement(
                        &mut movements,
                        n.ordinal,
                        &bucket,
                        "tail_return",
                        -b.vm + b.posted_im,
                        -b.vm,
                        -b.received_im,
                        -b.posted_im,
                        -b.received_im,
                        r.measure,
                    )?;
                    *b = Balance::default();
                } else if !default_seen {
                    let target = if v > p.receive_threshold {
                        v - p.receive_threshold
                    } else if v < -p.post_threshold {
                        v + p.post_threshold
                    } else {
                        0.
                    };
                    let pending = calls
                        .iter()
                        .filter(|c| {
                            c.bucket == bucket
                                && c.settled_ordinal.is_none()
                                && !c.cancelled_at_default
                                && !c.cancelled_at_tail
                        })
                        .map(|c| c.amount)
                        .sum::<f64>();
                    let delta = amount(target - b.vm - pending)?;
                    if delta != 0. && delta.abs() >= p.minimum_transfer {
                        let due = n
                            .ordinal
                            .checked_add(p.settlement_lag_calendar_days as i32)
                            .ok_or("margin date overflow")?;
                        if due > p.tail_return_ordinal {
                            return Err(
                                "margin lag extends beyond complete tail; supply longer tail"
                                    .into(),
                            );
                        }
                        let immediate =
                            p.settlement_lag_calendar_days == 0 && n.settlement_available;
                        calls.push(Call {
                            call_ordinal: n.ordinal,
                            due_ordinal: due,
                            settled_ordinal: if immediate { Some(n.ordinal) } else { None },
                            cancelled_at_default: false,
                            cancelled_at_tail: false,
                            bucket: bucket.clone(),
                            amount: delta,
                        });
                        if immediate {
                            b.vm = amount(b.vm + delta)?;
                            cash_movement(
                                &mut movements,
                                n.ordinal,
                                &bucket,
                                "variation_margin",
                                delta,
                                delta,
                                0.,
                                0.,
                                0.,
                                r.measure,
                            )?;
                        }
                    }
                }
            }
            if is_closeout {
                closeout_seen = true;
            }
            prior = Some(n);
        }
        if path.default.is_some() && (!default_seen || !closeout_seen) {
            return Err("incomplete default closeout tail".into());
        }
        for (bucket, b) in &balances {
            let opening = if balances.len() == 1 {
                Balance {
                    vm: p.opening_vm,
                    received_im: p.opening_received_im,
                    posted_im: p.opening_posted_im,
                }
            } else {
                Balance::default()
            };
            let deltas = movements.iter().filter(|m| &m.bucket == bucket).try_fold(
                (0., 0., 0.),
                |a, m| -> Result<_, String> {
                    Ok((
                        amount(a.0 + m.vm_delta)?,
                        amount(a.1 + m.received_im_delta)?,
                        amount(a.2 + m.posted_im_delta)?,
                    ))
                },
            )?;
            if !near(opening.vm + deltas.0, b.vm)
                || !near(opening.received_im + deltas.1, b.received_im)
                || !near(opening.posted_im + deltas.2, b.posted_im)
            {
                return Err("collateral stock/flow conservation failed".into());
            }
        }
        if let Some(book) = &obligation_book {
            book.finish()?;
        }
        let (obligations, mpor_exercises) = obligation_book
            .map_or((Vec::new(), Vec::new()), |book| {
                (book.ledger, book.exercises)
            });
        let residual_vm = balances.values().map(|b| b.vm).sum();
        let residual_received_im = balances.values().map(|b| b.received_im).sum();
        let residual_posted_im = balances.values().map(|b| b.posted_im).sum();
        let pending_calls = calls
            .iter()
            .filter(|c| {
                c.settled_ordinal.is_none() && !c.cancelled_at_default && !c.cancelled_at_tail
            })
            .map(|c| c.amount)
            .sum();
        cva = amount(cva + path.probability * loss_pv)?;
        fva = amount(fva + path.probability * funding_pv)?;
        results.push(PathResult {
            mpor_policy: path
                .default
                .as_ref()
                .and_then(|d| d.mpor.as_ref().map(|m| m.identity.clone())),
            closing_legal_review: path.default.as_ref().map(|d| d.legal_review.clone()),
            replacement_value_policy: path
                .default
                .as_ref()
                .map(|d| d.replacement_value_policy.clone()),
            path_id: path.path_id.clone(),
            probability: path.probability,
            source: path.source.clone(),
            exposures,
            calls,
            movements,
            closeouts,
            discounted_credit_loss: loss_pv,
            discounted_funding_cost: funding_pv,
            residual_vm,
            residual_received_im,
            residual_posted_im,
            pending_calls,
            obligations,
            mpor_exercises,
        });
    }
    if !near(total_prob, 1.) {
        return Err("explicit complete path probabilities must sum to one".into());
    }
    let clean = opening_clean.ok_or("missing opening clean")?;
    let pricing = r.measure == CashflowMeasure::Pricing;
    Ok(CsaResult{policy:p.identity.clone(),legal_mapping:r.mapping.clone(),conditional_model:r.conditional_model.clone(),crn:r.crn.clone(),clean_value:clean,cva:pricing.then_some(cva),fva:pricing.then_some(fva),clean_less_cva_fva:if pricing{Some(amount(clean-cva-fva)?)}else{None},paths:results,disclosure:"Raw swaps/options repriced natively at every scenario/date; supplied legal mapping controls aggregation. Signed cash VM, segregated static IM and complete return/default tails remain separate from clean value. Collateral interest settles in cash on opening interval balances; funding uses supplied signed spreads and endpoint discounts. Counterparty-only closeout; own DVA omitted. Physical paths report modeled loss/funding cash, not risk-neutral CVA. Calendar-day lag is explicit; no legal/calibration or business-day inference. European expiry uses an explicit single-state handoff: collateralized cash remains until settlement, physical rights are replaced once by the mapped raw underlying swap. Segregated IM is excluded from unsecured exposure until the reviewed default waterfall applies it. Reviewed MPOR policy captures native signed due cash and European rights, separating unpaid claims from remaining replacement value and settled cash exactly once; complete event coverage and measure-specific fixing paths are mandatory.".into()})
}
fn closeout(
    v: f64,
    b: &Balance,
    d: &DefaultPolicy,
    date: i32,
    bucket: &str,
) -> Result<Closeout, String> {
    let received = b.vm.max(0.);
    let posted = (-b.vm).max(0.);
    let mut vm_applied = 0.;
    let mut im_applied = 0.;
    let unsecured;
    let recovery;
    let cp_loss;
    let posted_loss;
    let cash;
    let custodian;
    if v >= 0. {
        vm_applied = received.min(v);
        im_applied = if d.received_im_available_on_default {
            b.received_im.min(v - vm_applied)
        } else {
            0.
        };
        unsecured = (v - vm_applied - im_applied).max(0.);
        recovery = unsecured * d.counterparty_recovery;
        cp_loss = unsecured - recovery;
        posted_loss = posted * (1. - d.posted_vm_return_recovery)
            + b.posted_im * (1. - d.posted_im_return_recovery);
        cash = im_applied
            + recovery
            + posted * d.posted_vm_return_recovery
            + b.posted_im * d.posted_im_return_recovery
            - (received - vm_applied);
        custodian = -(b.received_im - im_applied);
    } else {
        let obligation = -v;
        let used_vm = if d.posted_collateral_setoff {
            posted.min(obligation)
        } else {
            0.
        };
        let used_im = if d.posted_collateral_setoff {
            b.posted_im.min(obligation - used_vm)
        } else {
            0.
        };
        let unused_vm = posted - used_vm;
        let unused_im = b.posted_im - used_im;
        unsecured = 0.;
        recovery = 0.;
        cp_loss = 0.;
        posted_loss = unused_vm * (1. - d.posted_vm_return_recovery)
            + unused_im * (1. - d.posted_im_return_recovery);
        cash = unused_vm * d.posted_vm_return_recovery + unused_im * d.posted_im_return_recovery
            - (obligation - used_vm - used_im)
            - received;
        custodian = -b.received_im;
    }
    Ok(Closeout {
        ordinal: date,
        bucket: bucket.into(),
        clean_replacement_value: v,
        unpaid_obligation_claim: 0.,
        total_closeout_claim: v,
        vm_applied,
        received_im_applied: im_applied,
        unsecured_claim: amount(unsecured)?,
        unsecured_recovery: amount(recovery)?,
        counterparty_loss: amount(cp_loss)?,
        posted_return_loss: amount(posted_loss)?,
        entity_cash: amount(cash)?,
        custodian_return: amount(custodian)?,
    })
}
