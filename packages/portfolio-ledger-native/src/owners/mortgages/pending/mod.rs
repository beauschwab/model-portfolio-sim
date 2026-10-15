//! Uninstalled proof: explicit immutable sources, separate mutable state per run.
use crate::replay::dated::{CompiledDatedLedger, DatedLedgerRequest};
use portfolio_model_core::{credit_accounting::Identity, mortgage_pending_accounting as core};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::{Arc, Mutex},
};
pub(crate) mod guardian;
mod strict_json;

pub const SCHEMA: &str = "mortgage-mixed-accounting-1";
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryInput {
    pub source_id: String,
    pub request: core::Request,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub path_id: String,
    pub source_weight: f64,
    pub source_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub ledger_run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub paths: Vec<Path>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub unit_id: String,
    pub position_id: String,
    pub account_id: String,
    pub unit_face: f64,
    pub allocated_face: f64,
    pub source_identity: Identity,
    pub original_yield_anchor: Identity,
    pub runs: Vec<Run>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub phase: String,
    pub ownership: String,
    pub tail: String,
    pub source_usage: String,
    pub money_domain: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    #[serde(deserialize_with = "strict_json::dated")]
    pub dated: DatedLedgerRequest,
    pub sources: Vec<RegistryInput>,
    pub owners: Vec<Owner>,
    pub reporting_end_ordinal: i32,
    pub policy: Policy,
}
#[derive(Debug, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub source_work_units: usize,
    pub replay_work_units: usize,
    pub dated_work_units: usize,
    pub monetary_guard_work_units: usize,
    pub max_gl_keys: usize,
    pub supplied_gross_money: f64,
    pub gross_money_limit: f64,
    pub reserved_bytes: usize,
    pub unique_sources: usize,
    pub bindings: usize,
    pub joined_work_limit: usize,
    pub output_byte_limit: usize,
}
#[derive(Debug, Serialize)]
pub struct Source {
    pub source_id: String,
    pub native: core::ResultData,
}
pub struct Binding {
    pub owner: usize,
    pub run: usize,
    pub path: usize,
    pub source: usize,
    pub factor: f64,
}
pub struct Compiled {
    pub(crate) raw: Request,
    pub(crate) admission: Admission,
    pub(crate) sources: Vec<Source>,
    pub(crate) bindings: Vec<Binding>,
    pub(crate) position_owners: Vec<Option<usize>>,
    groups: BTreeMap<(usize, usize), Vec<(usize, usize)>>,
    pub(crate) run_paths: Vec<Vec<usize>>,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "mortgage joined admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "mortgage joined admission overflow".into())
}
pub fn near(a: f64, b: f64) -> Result<(), String> {
    if a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * a.abs().max(b.abs()) {
        Ok(())
    } else {
        Err(format!("mortgage owned conservation {a} != {b}"))
    }
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("mortgage bounded identity required".into())
    } else {
        Ok(())
    }
}
fn identity(i: &Identity) -> Result<(), String> {
    text(&i.id)?;
    text(&i.revision)
}
fn checked_size(v: &impl Serialize, cap: usize) -> Result<usize, String> {
    struct Counter {
        bytes: usize,
        cap: usize,
    }
    impl Write for Counter {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
            self.bytes = self
                .bytes
                .checked_add(b.len())
                .filter(|n| *n <= self.cap)
                .ok_or_else(|| io::Error::other("mortgage joined byte admission"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut c = Counter { bytes: 0, cap };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.bytes)
}
fn closed(book: &core::Book) -> Result<(), String> {
    for c in &book.slots {
        near(c.principal, 0.)?;
        near(c.recognized_interest, 0.)?;
        near(c.basis, 0.)?;
    }
    for v in book
        .borrower_principal
        .iter()
        .chain(&book.borrower_contract_interest)
    {
        near(*v, 0.)?;
    }
    for c in &book.claims {
        near(c.book.principal, 0.)?;
        near(c.book.recognized_interest, 0.)?;
        near(c.book.basis, 0.)?;
        near(c.economic_principal, 0.)?;
        near(c.economic_contract_interest, 0.)?;
    }
    Ok(())
}
fn placeholder(p: &crate::transport::types::Position) -> Result<(), String> {
    if p.kind != "loan"
        || p.classification != "ac"
        || p.start_day != 0
        || p.maturity_day != 0
        || !p.collateral_pool.is_empty()
        || !p.collateral_position.is_empty()
        || p.opening_market_price != 1.
        || [
            p.balance,
            p.book_adjustment,
            p.opening_accrued,
            p.allowance,
            p.rate,
            p.commitment,
            p.draw_fraction,
            p.watch_fraction,
            p.annual_pd,
            p.migration_rate,
            p.monthly_runoff,
            p.floating_beta,
            p.rollover,
            p.eligible_fraction,
            p.encumbered_fraction,
            p.pledged_face,
            p.risk_weight,
            p.rsf_weight,
            p.asf_weight,
            p.hqla_weight,
            p.lcr_outflow_weight,
            p.lcr_inflow_weight,
        ]
        .iter()
        .any(|x| *x != 0.)
    {
        return Err("exclusive zero mortgage placeholder required".into());
    }
    Ok(())
}
fn backbook_closure(c: &CompiledDatedLedger) -> Result<(), String> {
    let start = c.store.as_of_ordinal();
    let end = c.store.reporting_end_ordinal();
    for (run_index, run) in c.store.runs().iter().enumerate() {
        let mut principal = c
            .owner_positions
            .iter()
            .map(|i| c.spec.positions[*i].balance)
            .collect::<Vec<_>>();
        let mut basis = c
            .owner_positions
            .iter()
            .map(|i| c.spec.positions[*i].book_adjustment)
            .collect::<Vec<_>>();
        for e in c
            .store
            .events()
            .iter()
            .filter(|e| e.run_index.is_none_or(|r| r == run_index))
        {
            portfolio_compute_control::checkpoint()?;
            if e.payment_ordinal > end
                || e.full_contract_end_ordinal > end
                || e.components.fees != 0.
            {
                return Err("complete reviewed backbook tail required".into());
            }
            let opening = c.store.rights_at(
                &run.run_id,
                e.index,
                start,
                crate::replay::stores::dated::Boundary::BeforeEvents,
            )?;
            let final_state = c.store.rights_at(
                &run.run_id,
                e.index,
                end,
                crate::replay::stores::dated::Boundary::AfterEvents,
            )?;
            near(final_state.signed_interest_right, 0.)?;
            if !final_state.cash_settled || !final_state.accrual_complete {
                return Err("backbook remaining settlement/recognition tail".into());
            }
            if e.payment_ordinal >= start {
                principal[e.owner_index] -= e.components.principal.abs();
            }
            let direction = if c.store.owners()[e.owner_index].direction
                == crate::replay::stores::dated::Direction::Receipt
            {
                1.
            } else {
                -1.
            };
            basis[e.owner_index] += direction
                * (final_state.book_amortization_to_date - opening.book_amortization_to_date);
        }
        for v in principal.into_iter().chain(basis) {
            near(v, 0.)?;
        }
    }
    Ok(())
}
pub fn audit(r: &Request, c: &CompiledDatedLedger) -> Result<Admission, String> {
    portfolio_compute_control::checkpoint()?;
    let raw_bytes = checked_size(r, 1024 * 1024)?;
    if r.schema != SCHEMA
        || r.sources.is_empty()
        || r.sources.len() > 64
        || r.owners.is_empty()
        || r.owners.len() > 64
        || !c.spec.reverse_severities.is_empty()
        || r.policy.phase != "native_pending_groups_before_dated_settlement-1"
        || r.policy.ownership != "exclusive_native_seven_slot_owner-1"
        || r.policy.tail != "complete_unweighted_tail-1"
        || r.policy.source_usage != "explicit_immutable_source_registry-1"
        || r.policy.money_domain != "combined_gross_state_10k-1"
    {
        return Err("explicit mortgage proof registry/ownership/run policy required".into());
    }
    identity(&r.policy.identity)?;
    let start = c.store.as_of_ordinal();
    let end = c.store.reporting_end_ordinal();
    let days = usize::try_from(end - start).map_err(|_| "mortgage reversed window")?;
    if days == 0 || days > 64 || r.reporting_end_ordinal < start || r.reporting_end_ordinal > end {
        return Err("mortgage full/reporting64day window required".into());
    }
    if !c.spec.netting_sets.is_empty()
        || !c.spec.policies.is_empty()
        || !c.spec.cashflows.is_empty()
        || c.acquisitions.is_some()
    {
        return Err("mortgage proof rejects generic overlays/netting/acquisition streams".into());
    }
    let supplied_gross_money = guardian::inputs(r, c)?;
    for a in &c.spec.accounts {
        if a.cet1_floor != 0.
            || a.leverage_floor != 0.
            || a.lcr_floor != 0.
            || a.nsfr_floor != 0.
            || a.htm_asset_limit != 1.
            || a.tax_rate != 0.
            || a.annual_fees != 0.
            || a.annual_costs != 0.
            || a.annual_dividends != 0.
            || a.cash.abs() > 10_000.
            || a.equity.abs() > 10_000.
        {
            return Err("mortgage proof unreviewed account/capital/expense policy".into());
        }
    }
    backbook_closure(c)?;
    let source_map = r
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| (s.source_id.as_str(), i))
        .collect::<BTreeMap<_, _>>();
    if source_map.len() != r.sources.len() {
        return Err("duplicate immutable mortgage source id".into());
    }
    let mut source_work = 0;
    let mut guard_work = 0;
    let mut run_key_bounds = BTreeMap::<String, usize>::new();
    let mut max_transactions = mul(c.spec.accounts.len(), c.store.runs().len())?;
    let mut bytes = mul(raw_bytes, 2)?;
    let mut audits = Vec::new();
    for input in &r.sources {
        text(&input.source_id)?;
        let a = core::audit(&input.request, &mut portfolio_compute_control::checkpoint)?;
        if input.request.source.contract.monetary_unit != c.monetary_unit
            || input.request.source.acquisition.purchase_ordinal < start
            || a.reserved_end_ordinal > end
        {
            return Err("mortgage retained source exceeds actual full window/unit".into());
        }
        source_work = add(source_work, a.work)?;
        guard_work = add(
            guard_work,
            mul(
                add(add(a.max_groups, a.days)?, 2)?,
                add(1024, mul(mul(a.interval_count, 3)?, 64)?)?,
            )?,
        )?;
        bytes = add(bytes, a.reserved_bytes)?;
        audits.push(a);
    }
    let positions = c
        .spec
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), (i, p)))
        .collect::<BTreeMap<_, _>>();
    let accounts = c
        .spec
        .accounts
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect::<BTreeMap<_, _>>();
    let expected = c
        .store
        .runs()
        .iter()
        .map(|r| {
            (
                r.run_id.to_string(),
                r.scenario_id.to_string(),
                r.severity.to_bits(),
            )
        })
        .collect::<BTreeSet<_>>();
    let (mut owners, mut units, mut used, mut bindings, mut replay) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), 0, 0);
    for owner in &r.owners {
        for s in [&owner.unit_id, &owner.position_id, &owner.account_id] {
            text(s)?;
        }
        identity(&owner.source_identity)?;
        identity(&owner.original_yield_anchor)?;
        if !owners.insert(owner.position_id.as_str()) || !units.insert(owner.unit_id.as_str()) {
            return Err("duplicate mortgage owner/unit".into());
        }
        let &(pi, p) = positions
            .get(owner.position_id.as_str())
            .ok_or("mortgage placeholder missing")?;
        placeholder(p)?;
        if p.account.as_str() != owner.account_id || c.position_owners[pi].is_some() {
            return Err("mortgage account/double ownership mismatch".into());
        }
        let account = accounts
            .get(owner.account_id.as_str())
            .ok_or("mortgage account missing")?;
        if !owner.unit_face.is_finite()
            || owner.unit_face <= 0.
            || owner.unit_face > 10_000.
            || !owner.allocated_face.is_finite()
            || owner.allocated_face < 0.
            || owner.allocated_face > owner.unit_face
        {
            return Err("mortgage scaled10k face domain".into());
        }
        let got = owner
            .runs
            .iter()
            .map(|r| {
                (
                    r.ledger_run_id.clone(),
                    r.scenario_id.clone(),
                    r.severity.to_bits(),
                )
            })
            .collect::<BTreeSet<_>>();
        if got != expected || got.len() != owner.runs.len() {
            return Err("mortgage exact explicit ordinary run coverage required".into());
        }
        let mut anchor: Option<Vec<u8>> = None;
        for run in &owner.runs {
            let mut ids = BTreeSet::new();
            let mut weights = 0.;
            if run.paths.is_empty() || run.paths.len() > 32 {
                return Err("mortgage path count domain".into());
            }
            for path in &run.paths {
                portfolio_compute_control::checkpoint()?;
                text(&path.path_id)?;
                text(&path.source_id)?;
                if !ids.insert(&path.path_id)
                    || !path.source_weight.is_finite()
                    || !(0. ..=1.).contains(&path.source_weight)
                {
                    return Err("mortgage path identity/weight domain".into());
                }
                let &si = source_map
                    .get(path.source_id.as_str())
                    .ok_or("unbound mortgage source registry reference")?;
                used.insert(si);
                bindings = add(bindings, 1)?;
                weights += path.source_weight;
                let raw = &r.sources[si].request.source;
                if raw.contract.identity.position_id != owner.position_id
                    || raw.contract.identity.currency != account.currency
                    || raw.path_id != path.path_id
                    || raw.economic_source != owner.source_identity
                    || raw.acquisition.original_yield_anchor != owner.original_yield_anchor
                {
                    return Err(
                        "immutable mortgage source owner/anchor/currency identity mismatch".into(),
                    );
                }
                near(
                    raw.contract.opening_borrower_principal.iter().sum(),
                    owner.unit_face,
                )?;
                near(
                    raw.contract.opening_investor_principal.iter().sum(),
                    owner.unit_face,
                )?;
                let identity = serde_json::to_vec(&(
                    &raw.acquisition,
                    &raw.original_contract,
                    &raw.original_economic_source,
                ))
                .map_err(|e| e.to_string())?;
                if let Some(a) = &anchor {
                    if *a != identity {
                        return Err(
                            "mortgage original purchase/contract anchor changed across runs".into(),
                        );
                    }
                } else {
                    anchor = Some(identity);
                }
                let a = &audits[si];
                replay = add(
                    replay,
                    add(
                        mul(
                            add(days, 1)?,
                            add(1024, mul(mul(a.interval_count, 3)?, 128)?)?,
                        )?,
                        mul(
                            a.max_groups,
                            add(2048, mul(mul(a.interval_count, 3)?, 128)?)?,
                        )?,
                    )?,
                )?;
                max_transactions = add(max_transactions, a.max_groups)?;
                let header = checked_size(
                    &(
                        owner.unit_id.as_str(),
                        owner.position_id.as_str(),
                        owner.account_id.as_str(),
                        path,
                        run.ledger_run_id.as_str(),
                        run.scenario_id.as_str(),
                    ),
                    8192,
                )?;
                let claims = mul(a.interval_count, 3)?;
                let key_bound = run_key_bounds.entry(run.ledger_run_id.clone()).or_default();
                *key_bound = add(*key_bound, add(24, mul(claims, 3)?)?)?;

                let daily = mul(
                    add(days, 1)?,
                    add(add(mul(claims, 512)?, 2048)?, mul(header, 2)?)?,
                )?;
                let lines = mul(mul(a.max_groups, 24)?, add(512, mul(header, 2)?)?)?;
                bytes = add(bytes, add(daily, lines)?)?;
            }
            near(weights, 1.)?;
        }
    }
    if used.len() != r.sources.len() {
        return Err("unused immutable mortgage source registry entry".into());
    }
    for (i, p) in c.spec.positions.iter().enumerate() {
        if c.position_owners[i].is_none() && !owners.contains(p.id.as_str()) {
            return Err("mortgage proof requires complete native/backbook ownership".into());
        }
    }
    let dated_wire = mul(
        add(c.work_units, mul(add(days, 1)?, c.spec.accounts.len())?)?,
        1024,
    )?;
    bytes = add(bytes, add(c.store.retained_capacity_bytes(), dated_wire)?)?;
    let max_gl_keys = add(
        add(
            mul(c.spec.accounts.len(), 16)?,
            mul(c.store.owners().len(), 12)?,
        )?,
        run_key_bounds.values().copied().max().unwrap_or(0),
    )?;
    max_transactions = add(
        max_transactions,
        mul(
            mul(add(days, 1)?, c.store.events().len())?,
            mul(c.store.runs().len(), 2)?,
        )?,
    )?;
    guard_work = add(
        guard_work,
        mul(
            max_transactions,
            add(
                mul(max_gl_keys, 9)?,
                add(
                    128,
                    48usize.max(add(
                        mul(c.spec.positions.len(), 16)?,
                        mul(c.spec.accounts.len(), 16)?,
                    )?),
                )?,
            )?,
        )?,
    )?;
    guard_work = add(
        guard_work,
        mul(
            mul(add(days, 1)?, c.store.runs().len())?,
            add(
                add(
                    mul(c.spec.positions.len(), 64)?,
                    mul(c.spec.accounts.len(), 48)?,
                )?,
                256,
            )?,
        )?,
    )?;
    let segments = r
        .dated
        .dated
        .accrual_plans
        .iter()
        .try_fold(0usize, |n, p| add(n, p.segments.len()))?;
    let inputs = add(
        add(
            mul(c.spec.positions.len(), 6)?,
            mul(c.store.events().len(), 5)?,
        )?,
        add(
            add(mul(segments, 2)?, mul(c.spec.accounts.len(), 4)?)?,
            mul(r.sources.len(), 4)?,
        )?,
    )?;
    guard_work = add(guard_work, add(mul(inputs, 16)?, 128)?)?;
    // Two staged GL maps and indexed claim state are additional to source/output arenas.
    bytes = add(
        bytes,
        add(add(mul(max_gl_keys, 128)?, 288)?, mul(bindings, 48 * 64)?)?,
    )?;
    let work = add(source_work, add(add(replay, c.work_units)?, guard_work)?)?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err("mortgage joined8Mwork/32MiB admission before generation".into());
    }
    Ok(Admission {
        work_units: work,
        source_work_units: source_work,
        replay_work_units: replay,
        dated_work_units: c.work_units,
        monetary_guard_work_units: guard_work,
        max_gl_keys,
        supplied_gross_money,
        gross_money_limit: guardian::LIMIT,
        reserved_bytes: bytes,
        unique_sources: r.sources.len(),
        bindings,
        joined_work_limit: MAX_WORK,
        output_byte_limit: MAX_BYTES,
    })
}
impl Compiled {
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
    pub fn compile(raw: Request, c: &CompiledDatedLedger) -> Result<Self, String> {
        let admission = audit(&raw, c).map_err(|e| format!("mortgage preflight: {e}"))?;
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(raw.sources.len())
            .map_err(|e| e.to_string())?;
        for input in &raw.sources {
            portfolio_compute_control::checkpoint()?;
            let native =
                core::evaluate(&input.request, &mut portfolio_compute_control::checkpoint)?;
            for book in native
                .groups
                .iter()
                .map(|g| &g.committed)
                .chain(&native.daily)
                .chain(std::iter::once(&native.closing))
            {
                if book.claims.len()
                    > native
                        .admission
                        .interval_count
                        .checked_mul(3)
                        .ok_or("mortgage claim overflow")?
                {
                    return Err("mortgage source claim reservation exceeded".into());
                }
                guardian::book(book)?;
            }
            closed(&native.closing).map_err(|e| format!("mortgage source closure: {e}"))?;
            sources.push(Source {
                source_id: input.source_id.clone(),
                native,
            });
        }
        let mut bindings = Vec::new();
        bindings
            .try_reserve_exact(admission.bindings)
            .map_err(|e| e.to_string())?;
        let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let mut run_paths = vec![vec![]; c.store.runs().len()];
        let mut position_owners = vec![None; c.spec.positions.len()];
        let source_map = raw
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.source_id.as_str(), i))
            .collect::<BTreeMap<_, _>>();
        let positions = c
            .spec
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect::<BTreeMap<_, _>>();
        for (oi, owner) in raw.owners.iter().enumerate() {
            position_owners[positions[owner.position_id.as_str()]] = Some(oi);
            for (ri, run) in owner.runs.iter().enumerate() {
                let run_index = c.run_index(&run.scenario_id, run.severity)?;
                for (pi, path) in run.paths.iter().enumerate() {
                    let si = source_map[path.source_id.as_str()];
                    let bi = bindings.len();
                    bindings.push(Binding {
                        owner: oi,
                        run: ri,
                        path: pi,
                        source: si,
                        factor: (owner.allocated_face / owner.unit_face) * path.source_weight,
                    });
                    run_paths[run_index].push(bi);
                    for (gi, g) in sources[si].native.groups.iter().enumerate() {
                        portfolio_compute_control::checkpoint()?;
                        if g.lines.len() > 24 {
                            return Err("mortgage native group line reservation exceeded".into());
                        }
                        let day = usize::try_from(g.ordinal - c.store.as_of_ordinal())
                            .map_err(|_| "mortgage group precedes asof")?;
                        groups.entry((run_index, day)).or_default().push((bi, gi));
                    }
                }
            }
        }
        Ok(Self {
            raw,
            admission,
            sources,
            bindings,
            position_owners,
            groups,
            run_paths,
        })
    }
    pub fn postings(&self, run: usize, day: usize) -> &[(usize, usize)] {
        self.groups.get(&(run, day)).map_or(&[], Vec::as_slice)
    }
    pub fn binding(&self, index: usize) -> (&Owner, &Run, &Path, &Source) {
        let b = &self.bindings[index];
        let o = &self.raw.owners[b.owner];
        let r = &o.runs[b.run];
        (o, r, &r.paths[b.path], &self.sources[b.source])
    }
}
#[derive(Clone)]
pub struct Runtime {
    pub run: usize,
    pub books: Vec<Option<core::Book>>,
}
impl Runtime {
    pub fn new(c: &Compiled, run: usize) -> Self {
        Self {
            run,
            books: vec![None; c.bindings.len()],
        }
    }
    pub fn stage(&self, c: &Compiled, bi: usize, gi: usize) -> Result<core::Book, String> {
        let g = &c.sources[c.bindings[bi].source].native.groups[gi];
        let mut book = self.books[bi].clone().unwrap_or(core::Book {
            ordinal: g.ordinal,
            slots: [core::Channel::default(); 7],
            borrower_principal: [0.; 7],
            borrower_contract_interest: [0.; 7],
            claims: vec![],
            cash: 0.,
            original_eir_income: 0.,
            reviewed_income: 0.,
        });
        if self.books[bi].is_none() && g.kind != "acquisition" {
            return Err("mortgage first native group must acquire".into());
        }
        if self.books[bi].is_some() && g.kind == "acquisition" {
            return Err("mortgage duplicate acquisition".into());
        }
        let committed_claims = g
            .committed
            .claims
            .iter()
            .map(|x| (x.claim_id.as_str(), x))
            .collect::<BTreeMap<_, _>>();
        let mut claim_indices = book
            .claims
            .iter()
            .enumerate()
            .map(|(i, x)| (x.claim_id.clone(), i))
            .collect::<BTreeMap<_, _>>();
        near(g.lines.iter().map(|l| l.amount).sum(), 0.)?;
        for l in &g.lines {
            match l.account {
                core::Account::Cash => book.cash += l.amount,
                core::Account::OriginalEir => book.original_eir_income -= l.amount,
                core::Account::ReviewedIncome => book.reviewed_income -= l.amount,
                _ => {
                    let ch = if let Some(s) = l.slot {
                        book.slots
                            .get_mut(s)
                            .ok_or("mortgage slot outside seven-state book")?
                    } else {
                        let id = l
                            .claim_id
                            .as_deref()
                            .ok_or("mortgage missing claim line identity")?;
                        let index = if let Some(&index) = claim_indices.get(id) {
                            index
                        } else {
                            let n = committed_claims
                                .get(id)
                                .ok_or("mortgage unbound claim line")?;
                            let index = book.claims.len();
                            book.claims.push(core::Claim {
                                claim_id: n.claim_id.clone(),
                                origin: n.origin,
                                birth_ordinal: n.birth_ordinal,
                                book: core::Channel::default(),
                                economic_principal: 0.,
                                economic_contract_interest: 0.,
                            });
                            claim_indices.insert(id.to_owned(), index);
                            index
                        };
                        &mut book.claims[index].book
                    };
                    match l.account {
                        core::Account::Principal => ch.principal += l.amount,
                        core::Account::RecognizedInterest => ch.recognized_interest += l.amount,
                        core::Account::Basis => ch.basis += l.amount,
                        _ => unreachable!(),
                    }
                }
            }
        }
        for (a, b) in book.slots.iter().zip(&g.committed.slots) {
            near(a.principal, b.principal)?;
            near(a.recognized_interest, b.recognized_interest)?;
            near(a.basis, b.basis)?;
        }
        near(book.cash, g.committed.cash)?;
        near(book.original_eir_income, g.committed.original_eir_income)?;
        near(book.reviewed_income, g.committed.reviewed_income)?;
        if book.claims.len() != g.committed.claims.len() {
            return Err("mortgage actual claim cardinality mismatch".into());
        }
        for a in &book.claims {
            let b = committed_claims
                .get(a.claim_id.as_str())
                .ok_or("mortgage claim snapshot mismatch")?;
            near(a.book.principal, b.book.principal)?;
            near(a.book.recognized_interest, b.book.recognized_interest)?;
            near(a.book.basis, b.book.basis)?;
        }
        // Exact source borrower/economic memos are retained; no second GL model.
        book.ordinal = g.committed.ordinal;
        book.borrower_principal = g.committed.borrower_principal;
        book.borrower_contract_interest = g.committed.borrower_contract_interest;
        for a in &mut book.claims {
            let b = committed_claims.get(a.claim_id.as_str()).unwrap();
            a.economic_principal = b.economic_principal;
            a.economic_contract_interest = b.economic_contract_interest;
        }
        near(book.carrying() + book.cash, book.earnings())?;
        guardian::book(&book)?;
        Ok(book)
    }
    pub fn summary(&self, c: &Compiled, owner: usize) -> [f64; 4] {
        let mut s = [0.; 4];
        for &bi in &c.run_paths[self.run] {
            if c.bindings[bi].owner != owner {
                continue;
            }
            if let Some(book) = &self.books[bi] {
                let f = c.bindings[bi].factor;
                for x in &book.slots[..3] {
                    s[0] += x.principal * f;
                    s[1] += x.basis * f;
                    s[2] += x.recognized_interest * f;
                }
                s[3] += book.slots[3..]
                    .iter()
                    .map(core::Channel::carrying)
                    .sum::<f64>()
                    * f
                    + book.claims.iter().map(|x| x.book.carrying()).sum::<f64>() * f;
            }
        }
        s
    }
}
pub fn account(l: &core::Line) -> Result<&'static str, String> {
    match l.account {
        core::Account::Cash => Ok("cash"),
        core::Account::OriginalEir => Ok("pnl:mortgage_original_effective_interest"),
        core::Account::ReviewedIncome => Ok("pnl:mortgage_reviewed_realization"),
        a => match l.slot {
            Some(0..=2) => Ok(match a {
                core::Account::Principal => "asset_principal",
                core::Account::RecognizedInterest => "dated_interest_receivable",
                core::Account::Basis => "book_adjustment",
                _ => unreachable!(),
            }),
            Some(3..=5) => Ok(match a {
                core::Account::Principal => "mortgage_pending_default_principal",
                core::Account::RecognizedInterest => "mortgage_pending_default_interest_receivable",
                core::Account::Basis => "mortgage_pending_default_basis",
                _ => unreachable!(),
            }),
            Some(6) => Ok(match a {
                core::Account::Principal => "mortgage_pending_prepay_principal",
                core::Account::RecognizedInterest => "mortgage_pending_prepay_interest_receivable",
                core::Account::Basis => "mortgage_pending_prepay_basis",
                _ => unreachable!(),
            }),
            None if l.claim_id.is_some() => Ok(match a {
                core::Account::Principal => "credit_default_principal",
                core::Account::RecognizedInterest => "credit_default_interest_receivable",
                core::Account::Basis => "credit_default_book_adjustment",
                _ => unreachable!(),
            }),
            _ => Err("mortgage unsupported line account/slot".into()),
        },
    }
}
pub fn subject(position: &str, path: &str, l: &core::Line) -> String {
    if matches!(l.account, core::Account::Cash) {
        String::new()
    } else if let Some(id) = &l.claim_id {
        format!("{position}:{path}:claim:{id}")
    } else if let Some(s) = l.slot.filter(|s| *s >= 3) {
        format!("{position}:{path}:pending:{s}")
    } else {
        position.into()
    }
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub admission: Admission,
    pub sources: Vec<Source>,
    pub ledger_messages: Vec<Value>,
    pub production_adoption: bool,
    pub mixed_ledger_adoption: bool,
    pub accounting_standard_approval: bool,
    pub capital_ftp_eligible: bool,
    pub closing_capital_eligible: bool,
}
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let mut v = self.0.lock().unwrap();
        if v.len().checked_add(b.len()).is_none_or(|n| n > MAX_BYTES) {
            return Err(io::Error::other("mortgage stream byte bound"));
        }
        v.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn evaluate(raw: Request) -> Result<ResultData, String> {
    let dated = CompiledDatedLedger::compile(raw.dated.clone(), 512, MAX_WORK)?;
    let compiled = Compiled::compile(raw, &dated)?;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    crate::replay::daily::run_dated_with_mortgages(
        &dated,
        &compiled,
        Box::new(Capture(bytes.clone())),
    )?;
    let mut locked = bytes.lock().map_err(|e| e.to_string())?;
    let mut messages = Vec::new();
    for line in locked.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        portfolio_compute_control::checkpoint()?;
        messages.push(serde_json::from_slice(line).map_err(|e| e.to_string())?);
    }
    locked.clear();
    locked.shrink_to_fit();
    drop(locked);
    let r = ResultData {
        schema: SCHEMA,
        raw_request: compiled.raw,
        admission: compiled.admission,
        sources: compiled.sources,
        ledger_messages: messages,
        production_adoption: false,
        mixed_ledger_adoption: false,
        accounting_standard_approval: false,
        capital_ftp_eligible: false,
        closing_capital_eligible: false,
    };
    checked_size(&r, r.admission.reserved_bytes.min(MAX_BYTES))?;
    Ok(r)
}

pub struct Prepared {
    pub dated: CompiledDatedLedger,
    pub owner: Compiled,
}
pub fn prepare(raw: Request) -> Result<Prepared, String> {
    let dated = CompiledDatedLedger::compile(raw.dated.clone(), 512, MAX_WORK)?;
    let owner = Compiled::compile(raw, &dated)?;
    Ok(Prepared { dated, owner })
}
#[derive(Serialize)]
struct PreparationReceipt<'a> {
    schema: &'static str,
    raw_request: &'a Request,
    admission: &'a Admission,
    sources: &'a [Source],
    source_groups_sole_postings: bool,
    memo_checkpoint_policy: &'static str,
    production_adoption: bool,
    mixed_ledger_adoption: bool,
    accounting_standard_approval: bool,
    capital_ftp_eligible: bool,
    closing_capital_eligible: bool,
}
#[derive(Serialize)]
struct Preparation<'a> {
    schema: &'static str,
    model_schema: &'static str,
    prepared_request: &'a DatedLedgerRequest,
    source_receipt: PreparationReceipt<'a>,
}
#[derive(Serialize)]
struct Header<'a> {
    workflow: Preparation<'a>,
}
struct BoundedWriter<W> {
    inner: W,
    bytes: usize,
}
impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        let next = self
            .bytes
            .checked_add(b.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or_else(|| io::Error::other("mortgage complete stream32MiB bound"))?;
        let n = self.inner.write(b)?;
        self.bytes = next - b.len() + n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub fn run(prepared: &Prepared, output: Box<dyn Write + Send>) -> Result<(), String> {
    let c = &prepared.owner;
    let receipt = PreparationReceipt {
        schema: "mortgage-mixed-preparation-1",
        raw_request: &c.raw,
        admission: &c.admission,
        sources: &c.sources,
        source_groups_sole_postings: true,
        memo_checkpoint_policy: "native_complete_daily_boundary",
        production_adoption: false,
        mixed_ledger_adoption: false,
        accounting_standard_approval: false,
        capital_ftp_eligible: false,
        closing_capital_eligible: false,
    };
    let header = Header {
        workflow: Preparation {
            schema: "model-ledger-preparation-1",
            model_schema: SCHEMA,
            prepared_request: &c.raw.dated,
            source_receipt: receipt,
        },
    };
    let mut writer = BoundedWriter {
        inner: output,
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, &header).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    crate::replay::daily::run_dated_with_mortgages(&prepared.dated, c, Box::new(writer))
}
#[cfg(test)]
mod atomic_tests {
    use super::*;
    #[test]
    fn failed_group_leaves_old_run_book_unchanged() {
        let raw: Request = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/mortgage_mixed_pending.json"
        ))
        .unwrap();
        let dated = CompiledDatedLedger::compile(raw.dated.clone(), 512, 8_000_000).unwrap();
        let mut compiled = Compiled::compile(raw, &dated).unwrap();
        let runtime = Runtime::new(&compiled, 0);
        compiled.sources[0].native.groups[0].lines[0].amount += 1.;
        assert!(runtime.stage(&compiled, 0, 0).is_err());
        assert!(runtime.books.iter().all(Option::is_none));
    }
}
