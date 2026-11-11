//! Exclusive finite Sim containers and report scratch space. No monetary pruning.
//! Source owners, snapshots, commercial Tracking and Sink rows are separate.
use super::{cut_shape, cut_spec, Key, Spec};
use crate::owners::commercial::intents::Compiled;
use std::mem::size_of;
type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("commercial Sim capacity overflow".into())
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("commercial Sim capacity overflow".into())
}
fn tick(n: usize) -> R<()> {
    portfolio_compute_control::checkpoint()?;
    cut_shape::charge(n)
}
fn slots(n: usize) -> R<usize> {
    if n == 0 {
        Ok(0)
    } else {
        add(mul(n, 3)?, 8)
    }
}
fn hash(n: usize, bucket: usize) -> R<usize> {
    // Pinned hashbrown load/growth policy, including old/new table overlap.
    mul(add(mul(n, 8)?, 32)?, bucket)
}
fn tree(n: usize, slot: usize) -> R<usize> {
    // One full internal node per entry bounds split/root/leaf sparsity.
    mul(n, add(mul(slot, 11)?, 128)?)
}
pub(super) struct Capacity {
    pub all_runs_and_scratch_bytes: usize,
    pub census_work: usize,
}
pub(super) fn inventory(spec: &Spec, source: &Compiled<'_>) -> R<Capacity> {
    if cut_shape::spent().is_none() {
        return cut_shape::with_budget(8_000_000, || inventory(spec, source));
    }
    let start = cut_shape::spent();
    tick(1)?;
    if size_of::<usize>() != 8
        || size_of::<Key>() != 24
        || size_of::<(Key, f64)>() > 63
        || size_of::<(String, usize)>() > 63
        || size_of::<((usize, String), usize)>() > 63
    {
        return Err("commercial Sim requires validated 64-bit hash layouts".into());
    }
    if !spec.netting_sets.is_empty()
        || !spec.policies.is_empty()
        || !spec.cashflows.is_empty()
        || !spec.reverse_severities.is_empty()
        || spec
            .positions
            .iter()
            .any(|p| !p.collateral_position.is_empty())
    {
        return Err("commercial Sim inventory requires exclusive owners without overlays".into());
    }
    let (a, n) = (spec.accounts.len(), spec.positions.len());
    let runs = add(spec.scenarios.len(), 1)?;
    let mut keys = add(mul(a, 8)?, mul(n, 8)?)?;
    let mut events = mul(a, 3)?;
    let mut maximum_event = "native_verified_cut_opening".len();
    let mut maximum_account = 0;
    let mut maximum_scenario = "baseline".len();
    let mut maximum_position = 0;
    let mut maximum_gl = 256;
    let mut identity_payload = 0;
    for account in &spec.accounts {
        tick(1)?;
        maximum_account = maximum_account.max(account.id.len());
        identity_payload = add(identity_payload, account.id.len())?;
    }
    for position in &spec.positions {
        tick(1)?;
        identity_payload = add(identity_payload, position.id.len())?;
        maximum_position = maximum_position.max(position.id.len());
    }
    for scenario in &spec.scenarios {
        tick(1)?;
        maximum_scenario = maximum_scenario.max(scenario.name.len());
    }
    for binding in &source.bindings {
        tick(1)?;
        if binding.accounting.compact_bytes().is_some() {
            return Err("commercial Sim inventory requires borrowed finite loan groups".into());
        }
        for index in 0..binding.accounting.group_count() {
            tick(1)?;
            let group = binding.accounting.group(index);
            // Accrual sign splits and gross legal memo pairs. Include zeros.
            keys = add(
                keys,
                add(
                    mul(add(group.postings.len(), group.next.lots.len())?, 2)?,
                    2,
                )?,
            )?;
            events = add(events, 1)?;
            maximum_event = maximum_event.max(group.phase.len());
            for posting in &group.postings {
                tick(1)?;
                maximum_gl = maximum_gl.max(
                    crate::owners::consumer::state::gl_with_owner(
                        posting.posting.account,
                        crate::owners::consumer::state::IncomeOwner::Commercial,
                    )
                    .len(),
                );
            }
        }
    }
    for fee in &source.fees {
        tick(1)?;
        if fee.source.compact_bytes().is_some() {
            return Err("commercial Sim inventory requires borrowed finite fee groups".into());
        }
        maximum_event = maximum_event.max("commercial_nonintegral_fee".len());
        keys = add(keys, mul(fee.source.fee_count(), 8)?)?;
        for index in 0..fee.source.group_count() {
            tick(1)?;
            let group = fee.source.group(index);
            keys = add(keys, group.postings.len())?;
            for posting in &group.postings {
                tick(1)?;
                maximum_gl =
                    maximum_gl.max(crate::owners::commercial::fees::gl(posting.account).len());
            }
            events = add(events, 1)?;
        }
    }
    let specification = cut_spec::specification_clone_bytes(spec)?;
    // Full spec clone dominates accounts/positions/scenario and temporary DTOs.
    let dto = mul(specification, 2)?;
    // Six position numeric arrays and two arrays of empty Vec headers.
    let position_vectors = mul(
        slots(n)?,
        6 * size_of::<f64>() + 2 * size_of::<Vec<usize>>(),
    )?;
    // Ten account numeric arrays, reclassification option and two status slots.
    let account_vectors = mul(
        slots(a)?,
        10 * size_of::<f64>() + size_of::<Option<usize>>() + size_of::<[usize; 2]>(),
    )?;
    let identity_maps = add(add(hash(n, 64)?, hash(a, 64)?)?, identity_payload)?;
    // GL, earnings and independent daily reconciliation map.
    let financial_maps = mul(hash(keys, 64)?, 3)?;
    let breach_keys = mul(a, 6)?;
    let breaches = add(hash(breach_keys, 64)?, mul(breach_keys, "leverage".len())?)?;
    let event_state = add(
        mul(slots(events)?, size_of::<(usize, String, [f64; 3])>())?,
        add(hash(events, 64)?, mul(mul(events, maximum_event)?, 2)?)?,
    )?;
    let per_run = [
        size_of::<super::Sim<'_>>(),
        dto,
        position_vectors,
        account_vectors,
        identity_maps,
        financial_maps,
        breaches,
        event_state,
    ]
    .into_iter()
    .try_fold(0, add)?;
    // Report replay and all-run attribution; repeated events are not deduplicated.
    let attribution_keys = mul(events, runs)?;
    let attribution = add(
        tree(
            attribution_keys,
            size_of::<((String, String, String), [f64; 3])>(),
        )?,
        mul(
            attribution_keys,
            add(add(maximum_scenario, maximum_account)?, maximum_event)?,
        )?,
    )?;
    let reports = add(hash(keys, 64)?, attribution)?;
    // Cut-opening/closing vectors and sort scratch, seen/funding trees, account
    // totals, processed-position set and attribution lookup keys. Returned JSON
    // report rows move to Sink; their complete row payload is reserved there.
    let scratch = [
        mul(slots(keys)?, 128)?,
        tree(keys, size_of::<Key>())?,
        tree(keys, size_of::<(&usize, &usize)>())?,
        hash(a, 128)?,
        hash(n, 64)?,
        tree(attribution_keys, size_of::<(&String, &String)>())?,
        // Changes and filtered Changes can coexist. Static GL labels are also
        // bounded by the paid dictionary census; 256 dominates their fixed set.
        mul(slots(keys)?, 3 * size_of::<(String, String, f64)>())?,
        mul(mul(keys, 2)?, add(maximum_position, maximum_gl)?)?,
        mul(slots(add(mul(a, 2)?, mul(attribution_keys, runs)?)?)?, 64)?,
        add(
            add(maximum_account, maximum_scenario)?,
            add(maximum_event, 32768)?,
        )?,
    ]
    .into_iter()
    .try_fold(0, add)?;
    let total = add(
        mul(per_run, runs)?,
        add(reports, add(scratch, specification)?)?,
    )?;
    let work = match (start, cut_shape::spent()) {
        (Some(a), Some(b)) => b
            .checked_sub(a)
            .ok_or("commercial Sim census ticket regressed")?,
        _ => 0,
    };
    Ok(Capacity {
        all_runs_and_scratch_bytes: total,
        census_work: work,
    })
}
