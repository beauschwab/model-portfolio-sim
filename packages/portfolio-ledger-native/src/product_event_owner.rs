//! Exclusive native product-event ownership in the existing dated daily GL.
pub mod bridge;
pub mod source;
pub use crate::consumer_intent_owner::{
    InitialAllowanceReview, PathBalances, Scale, StagedIntent, DAY_PHASE, OWNER_POLICY,
};
use crate::dated_ledger::CompiledDatedLedger;
pub use bridge::{NativeView, OwnerSources, PostingIter, PostingRef, WireFormat};
use portfolio_model_core::credit_accounting::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "product-event-owner-1";
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_WORK: usize = 8_000_000;
/// Count bounded wire bytes without constructing an extra financial receipt.
fn serialized_len(value: &impl Serialize, limit: usize) -> Result<usize, String> {
    struct Count {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            portfolio_compute_control::checkpoint().map_err(std::io::Error::other)?;
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| std::io::Error::other("product source wire byte admission"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { bytes: 0, limit };
    serde_json::to_writer(&mut count, value).map_err(|e| e.to_string())?;
    Ok(count.bytes)
}
pub use crate::consumer_intent_owner::{closed, Admission, AllowanceReview};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub path_id: String,
    pub source_weight: f64,
    pub request: source::Request,
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
    pub accounting_category: String,
    pub source_identity: Identity,
    pub original_yield_anchor: Identity,
    pub initial_allowance_review: Option<AllowanceReview>,
    pub runs: Vec<Run>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub reporting_end_ordinal: i32,
    pub owners: Vec<Owner>,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "product owner admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "product owner admission overflow".into())
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * b.abs() {
        Ok(())
    } else {
        Err("product owner source/GL conservation mismatch".into())
    }
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("product owner bounded identity required".into())
    } else {
        Ok(())
    }
}
fn identity(i: &Identity) -> Result<(), String> {
    text(&i.id)?;
    text(&i.revision)
}
fn escaped_metadata(v: &Value) -> Result<usize, String> {
    portfolio_compute_control::checkpoint()?;
    match v {
        Value::String(s) => Ok(serde_json::to_vec(s).map_err(|e| e.to_string())?.len()),
        Value::Array(a) => a.iter().try_fold(0, |n, v| add(n, escaped_metadata(v)?)),
        Value::Object(a) => a.values().try_fold(0, |n, v| add(n, escaped_metadata(v)?)),
        _ => Ok(0),
    }
}
/// Conservative source-generation, retained-table and daily replay admission.
/// Counts all raw paths including zero probability and zero allocation.
pub fn admit(r: &Request, c: &CompiledDatedLedger) -> Result<Admission, String> {
    portfolio_compute_control::checkpoint()?;
    serialized_len(r, 32 * 1024 * 1024)?;
    if r.schema != SCHEMA
        || r.owners.is_empty()
        || r.owners.len() > 64
        || !c.spec.reverse_severities.is_empty()
    {
        return Err("product owner schema/count/run boundary".into());
    }
    let start = c.store.as_of_ordinal();
    let full = c.store.reporting_end_ordinal();
    if r.reporting_end_ordinal < start || r.reporting_end_ordinal > full {
        return Err("product actual reporting date boundary".into());
    }
    let expected: BTreeSet<_> = c
        .store
        .runs()
        .iter()
        .map(|s| {
            (
                s.run_id.to_string(),
                s.scenario_id.to_string(),
                s.severity.to_bits(),
            )
        })
        .collect();
    let positions: BTreeMap<_, _> = c
        .spec
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), (i, p)))
        .collect();
    let accounts: BTreeMap<_, _> = c.spec.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
    let mut overlays = BTreeSet::new();
    for f in &c.spec.cashflows {
        portfolio_compute_control::checkpoint()?;
        overlays.insert(f.position.as_str());
    }
    for f in &c.spec.policies {
        portfolio_compute_control::checkpoint()?;
        overlays.insert(f.position.as_str());
    }
    let (mut work, mut bytes, mut count) = (0, 0, 0);
    let mut owners = BTreeSet::new();
    let mut units = BTreeSet::new();
    for owner in &r.owners {
        portfolio_compute_control::checkpoint()?;
        for s in [&owner.unit_id, &owner.position_id, &owner.account_id] {
            text(s)?;
        }
        identity(&owner.source_identity)?;
        identity(&owner.original_yield_anchor)?;
        if !owners.insert(&owner.position_id) || !units.insert(&owner.unit_id) {
            return Err("duplicate product owner/unit".into());
        }
        let &(pi, p) = positions
            .get(owner.position_id.as_str())
            .ok_or("product zero placeholder missing")?;
        let a = accounts
            .get(owner.account_id.as_str())
            .ok_or("product actual account missing")?;
        if p.account.as_str() != owner.account_id
            || p.kind != "loan"
            || p.classification.as_str() != owner.accounting_category
            || owner.accounting_category != "ac"
            || p.balance != 0.
            || p.book_adjustment != 0.
            || p.opening_accrued != 0.
            || p.allowance != 0.
            || p.start_day != 0
            || p.rate != 0.
            || p.maturity_day != 0
            || p.commitment != 0.
            || p.draw_fraction != 0.
            || p.watch_fraction != 0.
            || p.annual_pd != 0.
            || p.migration_rate != 0.
            || p.monthly_runoff != 0.
            || p.floating_beta != 0.
            || p.rollover != 0.
            || p.eligible_fraction != 0.
            || p.encumbered_fraction != 0.
            || p.pledged_face != 0.
            || !p.collateral_pool.is_empty()
            || p.opening_market_price != 1.
            || p.risk_weight != 0.
            || p.rsf_weight != 0.
            || p.asf_weight != 0.
            || p.hqla_weight != 0.
            || p.lcr_outflow_weight != 0.
            || p.lcr_inflow_weight != 0.
            || !p.collateral_position.is_empty()
            || c.position_owners[pi].is_some()
            || overlays.contains(owner.position_id.as_str())
        {
            return Err(
                "exclusive zero product placeholder and no generic/double owner required".into(),
            );
        }
        if a.cet1_floor != 0.
            || a.leverage_floor != 0.
            || a.lcr_floor != 0.
            || a.nsfr_floor != 0.
            || a.htm_asset_limit != 1.
        {
            return Err("product interim default regulatory/liquidity constraints require explicit classification integration".into());
        }
        let got: BTreeSet<_> = owner
            .runs
            .iter()
            .map(|s| {
                (
                    s.ledger_run_id.clone(),
                    s.scenario_id.clone(),
                    s.severity.to_bits(),
                )
            })
            .collect();
        if got != expected || got.len() != owner.runs.len() {
            return Err("product exact run/severity coverage required".into());
        }
        let mut anchor: Option<Value> = None;
        for run in &owner.runs {
            if run.paths.is_empty() || run.paths.len() > 32 {
                return Err("product path cardinality admission".into());
            }
            let (mut weights, mut ids) = (0., BTreeSet::new());
            for path in &run.paths {
                portfolio_compute_control::checkpoint()?;
                count = add(count, 1)?;
                text(&path.path_id)?;
                let raw = path.request.view();
                serialized_len(&path.request, 1024 * 1024)?;
                path.request.validate_source()?;
                Scale {
                    allocation_face: owner.allocated_face,
                    unit_face: owner.unit_face,
                    source_weight: path.source_weight,
                }
                .factor()?;
                if !ids.insert(&path.path_id)
                    || raw.path_id.is_some_and(|id| path.path_id != id)
                    || raw.contract.identity.run_id != run.ledger_run_id
                    || raw.contract.identity.position_id != owner.position_id
                    || raw.contract.identity.currency != a.currency
                    || *raw.economic_source != owner.source_identity
                    || *raw.acquisition.original_yield_anchor != owner.original_yield_anchor
                    || raw.acquisition.principal != owner.unit_face
                    || raw.acquisition.purchase_ordinal < start
                    || raw.contract.as_of_ordinal != raw.acquisition.purchase_ordinal
                {
                    return Err(
                        "product raw source/original acquisition/run identity mismatch".into(),
                    );
                }
                let tail = raw
                    .contract
                    .as_of_ordinal
                    .checked_add(raw.contract.horizon_days as i32)
                    .ok_or("product tail overflow")?;
                if raw.reporting_end_ordinal
                    != r.reporting_end_ordinal
                        .clamp(raw.acquisition.purchase_ordinal, tail)
                {
                    return Err("product explicit source reporting clock required".into());
                }
                if tail > full || raw.contract.horizon_days > 1080 {
                    return Err(
                        "product complete source tail outside admitted global window".into(),
                    );
                }
                let this = json!([
                    raw.kind,
                    path.request.original_terms(),
                    raw.acquisition,
                    raw.contract.identity.original_loan_id,
                    raw.contract.identity.source_revision,
                    raw.economic_source,
                    raw.policy,
                    raw.opening_allowance
                ]);
                if anchor.as_ref().is_some_and(|v| *v != this) {
                    return Err("product original anchors/policy differ across paths".into());
                }
                anchor = Some(this);
                if matches!(path.request, source::Request::CardV2(_))
                    && owner.initial_allowance_review.is_some()
                {
                    return Err("card v2 mixed owner has no initial allowance policy".into());
                }
                if let Some(review) = &owner.initial_allowance_review {
                    identity(&review.review)?;
                    near(review.amount, raw.opening_allowance)?;
                } else if raw.opening_allowance != 0. {
                    return Err("product reviewed initial allowance required".into());
                }
                weights += path.source_weight;
                let d = raw.contract.horizon_days as usize;
                let reviews = raw.reviews;
                let actions = raw.actions;
                let core_v2 = match &path.request {
                    source::Request::CardV2(r) => Some(
                        portfolio_model_core::card_credit_accounting_v2::audit_request_with_check(
                            r,
                            &mut portfolio_compute_control::checkpoint,
                        )?,
                    ),
                    _ => None,
                };
                let primitive_work = if let Some(a) = &core_v2 {
                    a.work
                } else {
                    mul(add(d, 1)?, add(add(actions, reviews)?, 72)?)?
                };
                if primitive_work > 2_000_000 {
                    return Err("raw product primitive work before generation".into());
                }
                work = add(work, primitive_work)?;
                let n = add(add(d, mul(actions, 2)?)?, add(mul(reviews, 6)?, 32)?)?;
                let metadata = escaped_metadata(&json!([
                    owner.unit_id,
                    owner.position_id,
                    owner.account_id,
                    path.path_id,
                    path.request
                ]))?;
                bytes = add(
                    bytes,
                    mul(
                        add(mul(n, 14)?, mul(add(d, 1)?, 16)?)?,
                        add(metadata, 8192)?,
                    )?,
                )?;
                if let Some(a) = core_v2 {
                    // Native current-state/action/income arenas are retained once;
                    // legacy table/index reservation above and pooled replay below
                    // remain additional, including zero-participation paths.
                    bytes = add(bytes, a.reserved_bytes)?;
                    work = add(work, mul(a.max_events, 128)?)?;
                }
                work = add(
                    work,
                    add(mul(d, add(add(actions, reviews)?, 64)?)?, mul(n, 32)?)?,
                )?;
            }
            near(weights, 1.)?;
        }
    }
    let days = usize::try_from(full - start).map_err(|_| "product reversed global window")?;
    bytes = add(bytes, mul(mul(add(days, 1)?, count)?, 8192)?)?;
    work = add(work, mul(add(days, 1)?, mul(count, 64)?)?)?;
    if days > 1080 || bytes > MAX_BYTES || work > MAX_WORK {
        return Err("product aggregate admission before generation".into());
    }
    Ok(Admission {
        work,
        retained_bytes: bytes,
        full_end_ordinal: full,
        paths: count,
    })
}
#[derive(Debug, Serialize)]
pub struct SourcePath {
    pub owner_index: usize,
    pub run_index: usize,
    pub path_index: usize,
    pub native: source::Native,
}
pub struct Compiled {
    pub request: Request,
    pub admission: Admission,
    pub sources: Vec<SourcePath>,
    pub position_owners: Vec<Option<usize>>,
    pub run_paths: Vec<Vec<usize>>,
    acquisitions: BTreeMap<(usize, usize), Vec<usize>>,
    intents: BTreeMap<(usize, usize), Vec<(usize, usize)>>,
    postings: BTreeMap<(usize, usize), Vec<PostingRef>>,
    owner_paths: BTreeMap<(usize, usize), Vec<usize>>,
    statement_dates: BTreeMap<(usize, i32), Vec<usize>>,
    pub as_of_ordinal: i32,
}
impl Compiled {
    pub fn compile(r: Request, c: &CompiledDatedLedger) -> Result<Self, String> {
        let admission = admit(&r, c)?;
        if add(admission.work, c.work_units)? > MAX_WORK {
            return Err("product combined daily work admission".into());
        }
        let positions: BTreeMap<_, _> = c
            .spec
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.as_str(), i))
            .collect();
        let runs: BTreeMap<_, _> = c
            .store
            .runs()
            .iter()
            .enumerate()
            .map(|(i, p)| (p.run_id.as_ref(), i))
            .collect();
        let mut sources = Vec::with_capacity(admission.paths);
        let mut position_owners = vec![None; c.spec.positions.len()];
        let mut run_paths = vec![Vec::new(); c.store.runs().len()];
        let (mut acquisitions, mut intents, mut owner_paths) = (
            BTreeMap::<_, Vec<usize>>::new(),
            BTreeMap::<_, Vec<(usize, usize)>>::new(),
            BTreeMap::<_, Vec<usize>>::new(),
        );
        let start = c.store.as_of_ordinal();
        let mut statement_dates = BTreeMap::<_, Vec<usize>>::new();
        let mut postings = BTreeMap::<_, Vec<PostingRef>>::new();
        let mut source_bytes = 0usize;
        for (oi, o) in r.owners.iter().enumerate() {
            position_owners[positions[o.position_id.as_str()]] = Some(oi);
            for (ri, run) in o.runs.iter().enumerate() {
                let run_index = runs[run.ledger_run_id.as_str()];
                for (pi, p) in run.paths.iter().enumerate() {
                    portfolio_compute_control::checkpoint()?;
                    let native = p.request.evaluate()?;
                    if let source::Native::CardV2(card) = &native {
                        if !card.complete
                            || !card.closing.complete()
                            || !card
                                .economic
                                .points
                                .last()
                                .is_some_and(|p| p.limit == 0. && p.undrawn == 0.)
                        {
                            return Err("card v2 mixed source requires complete individual gross/basis/commitment tails".into());
                        }
                    }
                    closed(native.closing())?;
                    let wire_bytes = serialized_len(&native, MAX_BYTES)?;
                    let capacity_charge = match &native {
                        source::Native::CardV2(r) => r.retained_capacity_charge.max(wire_bytes),
                        _ => wire_bytes,
                    };
                    source_bytes = add(source_bytes, capacity_charge)?;
                    if source_bytes > admission.retained_bytes {
                        return Err(
                            "product retained source exceeded conservative reservation".into()
                        );
                    }
                    let index = sources.len();
                    if let source::Native::Card(card) = &native {
                        for (i, statement) in card.statement_lineage.iter().enumerate() {
                            portfolio_compute_control::checkpoint()?;
                            statement_dates
                                .entry((index, statement.ordinal))
                                .or_default()
                                .push(i);
                        }
                    }
                    if let source::Native::CardV2(card) = &native {
                        for (i, statement) in card.economic.statements.iter().enumerate() {
                            portfolio_compute_control::checkpoint()?;
                            statement_dates
                                .entry((index, statement.ordinal))
                                .or_default()
                                .push(i);
                        }
                        for (i, action) in card.actions.iter().enumerate() {
                            portfolio_compute_control::checkpoint()?;
                            if action.ordinal < start || action.ordinal > admission.full_end_ordinal
                            {
                                return Err(
                                    "card v2 atomic action outside full source window".into()
                                );
                            }
                            postings
                                .entry((run_index, (action.ordinal - start) as usize))
                                .or_default()
                                .push(PostingRef::CardV2Action {
                                    path: index,
                                    index: i,
                                });
                        }
                    }
                    run_paths[run_index].push(index);
                    owner_paths.entry((run_index, oi)).or_default().push(index);
                    acquisitions
                        .entry((
                            run_index,
                            (p.request.view().acquisition.purchase_ordinal - start) as usize,
                        ))
                        .or_default()
                        .push(index);
                    for (i, intent) in native.intents().iter().enumerate() {
                        portfolio_compute_control::checkpoint()?;
                        if intent.position_id != o.position_id {
                            return Err("product native intent position lineage mismatch".into());
                        }
                        if intent.recognition_ordinal < start
                            || intent.recognition_ordinal > admission.full_end_ordinal
                        {
                            return Err("product native intent outside full source window".into());
                        }
                        intents
                            .entry((run_index, (intent.recognition_ordinal - start) as usize))
                            .or_default()
                            .push((index, i));
                        postings
                            .entry((run_index, (intent.recognition_ordinal - start) as usize))
                            .or_default()
                            .push(PostingRef::Intent {
                                path: index,
                                index: i,
                            });
                    }
                    sources.push(SourcePath {
                        owner_index: oi,
                        run_index: ri,
                        path_index: pi,
                        native,
                    });
                }
            }
        }
        Ok(Self {
            request: r,
            admission,
            sources,
            position_owners,
            run_paths,
            acquisitions,
            intents,
            postings,
            owner_paths,
            statement_dates,
            as_of_ordinal: start,
        })
    }
    pub(crate) fn acquisitions(&self, run: usize, day: usize) -> &[usize] {
        self.acquisitions
            .get(&(run, day))
            .map_or(&[], Vec::as_slice)
    }
    pub(crate) fn intents(&self, run: usize, day: usize) -> &[(usize, usize)] {
        self.intents.get(&(run, day)).map_or(&[], Vec::as_slice)
    }
    pub(crate) fn postings(&self, run: usize, day: usize) -> &[PostingRef] {
        self.postings.get(&(run, day)).map_or(&[], Vec::as_slice)
    }
    pub(crate) fn owner_paths(&self, run: usize, owner: usize) -> &[usize] {
        self.owner_paths
            .get(&(run, owner))
            .map_or(&[], Vec::as_slice)
    }
    /// Native-source local day; statements are selected by exact actual date.
    pub fn daily_receipts(&self, index: usize, day: usize) -> Value {
        match &self.sources[index].native {
            source::Native::Installment(r) => {
                json!({"kind":"installment","daily":r.daily[day.min(r.daily.len()-1)]})
            }
            source::Native::CardV2(r) => {
                let actual_ordinal = i32::try_from(day)
                    .ok()
                    .and_then(|d| r.raw_request.review.acquisition_ordinal.checked_add(d));
                let statements:Vec<_>=actual_ordinal.into_iter().flat_map(|ordinal|self.statement_dates.get(&(index,ordinal)).into_iter().flatten()).map(|i|json!({"source_index":i,"ordinal":r.economic.statements[*i].ordinal,"due_ordinal":r.economic.statements[*i].due_ordinal,"interest_recognized_again":false})).collect();
                let retirement=match &r.raw_request.termination {
                    portfolio_model_core::card_credit_accounting::Termination::SourceDefault=>r.economic.default.as_ref().map(|d|d.ordinal),
                    portfolio_model_core::card_credit_accounting::Termination::ReviewedZeroRightsClosure{ordinal,..}=>Some(*ordinal),
                };
                json!({"kind":"card_v2","daily":r.daily[day.min(r.daily.len()-1)],"statement_lineage":statements,
                    "commitment_source":{"termination":r.raw_request.termination,"retirement_ordinal":retirement,"policy_review":r.raw_request.policy.review,"funded_contingent_draw":0.},
                    "termination":r.raw_request.termination})
            }
            source::Native::Card(r) => {
                let actual_ordinal = i32::try_from(day)
                    .ok()
                    .and_then(|d| r.raw_request.acquisition.purchase_ordinal.checked_add(d));
                let statements: Vec<_> = actual_ordinal
                    .into_iter()
                    .flat_map(|ordinal| {
                        self.statement_dates
                            .get(&(index, ordinal))
                            .into_iter()
                            .flatten()
                    })
                    .map(|i| &r.statement_lineage[*i])
                    .collect();
                json!({"kind":"card","daily":r.daily[day.min(r.daily.len()-1)],"statement_lineage":statements,"commitment_source":r.commitment_source,"termination":r.raw_request.termination})
            }
        }
    }
    pub fn path(&self, index: usize) -> (&Owner, &Path) {
        let s = &self.sources[index];
        let o = &self.request.owners[s.owner_index];
        (o, &o.runs[s.run_index].paths[s.path_index])
    }
    pub(crate) fn scale(&self, index: usize) -> Scale {
        let (o, p) = self.path(index);
        Scale {
            allocation_face: o.allocated_face,
            unit_face: o.unit_face,
            source_weight: p.source_weight,
        }
    }
}
