//! Exclusive native product-event ownership in the existing dated daily GL.
pub(crate) mod state;
use crate::dated_ledger::CompiledDatedLedger;
use portfolio_model_core::{
    consumer_credit_accounting as core,
    credit_accounting::{AccountingState, Identity},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
pub use state::{
    InitialAllowanceReview, PathBalances, Scale, StagedIntent, DAY_PHASE, OWNER_POLICY,
};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "consumer-intent-owner-1";
pub const MAX_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_WORK: usize = 8_000_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowanceReview {
    pub review: Identity,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub path_id: String,
    pub source_weight: f64,
    pub request: core::Request,
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
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Admission {
    pub work: usize,
    pub retained_bytes: usize,
    pub full_end_ordinal: i32,
    pub paths: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "consumer owner admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "consumer owner admission overflow".into())
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * b.abs() {
        Ok(())
    } else {
        Err("consumer owner source/GL conservation mismatch".into())
    }
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("consumer owner bounded identity required".into())
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
pub fn closed(s: &AccountingState) -> Result<(), String> {
    for l in &s.live {
        for v in [
            l.principal,
            l.undrawn,
            l.accrued_interest,
            l.book_adjustment,
        ] {
            near(v, 0.)?;
        }
    }
    near(s.allowance, 0.)?;
    for c in &s.claims {
        for v in [
            c.outstanding_principal,
            c.outstanding_accrued_interest,
            c.outstanding_book_adjustment,
        ] {
            near(v, 0.)?;
        }
        if !c.remaining_recoveries.is_empty() {
            return Err("consumer complete unpaid recovery tail required".into());
        }
    }
    Ok(())
}
/// Conservative source-generation, retained-table and daily replay admission.
/// Counts all raw paths including zero probability and zero allocation.
pub fn admit(r: &Request, c: &CompiledDatedLedger) -> Result<Admission, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || r.owners.is_empty()
        || r.owners.len() > 64
        || !c.spec.reverse_severities.is_empty()
    {
        return Err("consumer owner schema/count/run boundary".into());
    }
    let start = c.store.as_of_ordinal();
    let full = c.store.reporting_end_ordinal();
    if r.reporting_end_ordinal < start || r.reporting_end_ordinal > full {
        return Err("consumer actual reporting date boundary".into());
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
            return Err("duplicate consumer owner/unit".into());
        }
        let &(pi, p) = positions
            .get(owner.position_id.as_str())
            .ok_or("consumer zero placeholder missing")?;
        let a = accounts
            .get(owner.account_id.as_str())
            .ok_or("consumer actual account missing")?;
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
            || c.spec
                .cashflows
                .iter()
                .any(|f| f.position == owner.position_id)
            || c.spec
                .policies
                .iter()
                .any(|f| f.position == owner.position_id)
        {
            return Err(
                "exclusive zero consumer placeholder and no generic/double owner required".into(),
            );
        }
        if a.cet1_floor != 0.
            || a.leverage_floor != 0.
            || a.lcr_floor != 0.
            || a.nsfr_floor != 0.
            || a.htm_asset_limit != 1.
        {
            return Err("consumer interim default regulatory/liquidity constraints require explicit classification integration".into());
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
            return Err("consumer exact run/severity coverage required".into());
        }
        let mut anchor: Option<Value> = None;
        for run in &owner.runs {
            if run.paths.is_empty() || run.paths.len() > 32 {
                return Err("consumer path cardinality admission".into());
            }
            let (mut weights, mut ids) = (0., BTreeSet::new());
            for path in &run.paths {
                portfolio_compute_control::checkpoint()?;
                count = add(count, 1)?;
                text(&path.path_id)?;
                let raw = &path.request;
                if raw.schema != core::SCHEMA
                    || raw.contract.identity.measure
                        != portfolio_model_core::cashflow::CashflowMeasure::Physical
                {
                    return Err("consumer physical raw accounting source required".into());
                }
                portfolio_model_core::consumer_installment::validate_request(&raw.contract)?;
                Scale {
                    allocation_face: owner.allocated_face,
                    unit_face: owner.unit_face,
                    source_weight: path.source_weight,
                }
                .factor()?;
                if !ids.insert(&path.path_id)
                    || path.path_id != raw.path_id
                    || raw.contract.identity.run_id != run.ledger_run_id
                    || raw.contract.identity.position_id != owner.position_id
                    || raw.contract.identity.currency != a.currency
                    || raw.economic_source != owner.source_identity
                    || raw.acquisition.original_yield_anchor != owner.original_yield_anchor
                    || raw.acquisition.principal != owner.unit_face
                    || raw.acquisition.purchase_ordinal < start
                    || raw.contract.as_of_ordinal != raw.acquisition.purchase_ordinal
                {
                    return Err(
                        "consumer raw source/original acquisition/run identity mismatch".into(),
                    );
                }
                let tail = raw
                    .contract
                    .as_of_ordinal
                    .checked_add(raw.contract.horizon_days as i32)
                    .ok_or("consumer tail overflow")?;
                if raw.reporting_end_ordinal
                    != r.reporting_end_ordinal
                        .clamp(raw.acquisition.purchase_ordinal, tail)
                {
                    return Err("consumer explicit source reporting clock required".into());
                }
                if tail > full || raw.contract.horizon_days > 1080 {
                    return Err(
                        "consumer complete source tail outside admitted global window".into(),
                    );
                }
                let this = json!([
                    raw.acquisition,
                    raw.contract.identity.original_loan_id,
                    raw.contract.identity.source_revision,
                    raw.economic_source,
                    raw.policy,
                    raw.opening_allowance
                ]);
                if anchor.as_ref().is_some_and(|v| *v != this) {
                    return Err("consumer original anchors/policy differ across paths".into());
                }
                anchor = Some(this);
                if let Some(review) = &owner.initial_allowance_review {
                    identity(&review.review)?;
                    near(review.amount, raw.opening_allowance)?;
                } else if raw.opening_allowance != 0. {
                    return Err("consumer reviewed initial allowance required".into());
                }
                weights += path.source_weight;
                let d = raw.contract.horizon_days as usize;
                let reviews = add(raw.allowance_reviews.len(), raw.chargeoffs.len())?;
                let actions = add(raw.contract.payment_dates.len(), raw.contract.steps.len())?;
                let n = add(add(d, mul(actions, 2)?)?, add(mul(reviews, 6)?, 32)?)?;
                let metadata = escaped_metadata(&json!([
                    owner.unit_id,
                    owner.position_id,
                    owner.account_id,
                    path.path_id,
                    raw
                ]))?;
                bytes = add(
                    bytes,
                    mul(
                        add(mul(n, 14)?, mul(add(d, 1)?, 16)?)?,
                        add(metadata, 8192)?,
                    )?,
                )?;
                work = add(
                    work,
                    add(mul(d, add(add(actions, reviews)?, 64)?)?, mul(n, 32)?)?,
                )?;
            }
            near(weights, 1.)?;
        }
    }
    let days = usize::try_from(full - start).map_err(|_| "consumer reversed global window")?;
    bytes = add(bytes, mul(mul(add(days, 1)?, count)?, 8192)?)?;
    work = add(work, mul(add(days, 1)?, mul(count, 64)?)?)?;
    if days > 1080 || bytes > MAX_BYTES || work > MAX_WORK {
        return Err("consumer aggregate admission before generation".into());
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
    pub native: core::ResultData,
}
pub struct Compiled {
    pub request: Request,
    pub admission: Admission,
    pub sources: Vec<SourcePath>,
    pub position_owners: Vec<Option<usize>>,
    pub run_paths: Vec<Vec<usize>>,
    acquisitions: BTreeMap<(usize, usize), Vec<usize>>,
    intents: BTreeMap<(usize, usize), Vec<(usize, usize)>>,
    owner_paths: BTreeMap<(usize, usize), Vec<usize>>,
    pub as_of_ordinal: i32,
}
impl Compiled {
    pub fn compile(r: Request, c: &CompiledDatedLedger) -> Result<Self, String> {
        let admission = admit(&r, c)?;
        if add(admission.work, c.work_units)? > MAX_WORK {
            return Err("consumer combined daily work admission".into());
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
        for (oi, o) in r.owners.iter().enumerate() {
            position_owners[positions[o.position_id.as_str()]] = Some(oi);
            for (ri, run) in o.runs.iter().enumerate() {
                let run_index = runs[run.ledger_run_id.as_str()];
                for (pi, p) in run.paths.iter().enumerate() {
                    portfolio_compute_control::checkpoint()?;
                    let native = core::evaluate_with_check(
                        &p.request,
                        portfolio_compute_control::checkpoint,
                    )?;
                    closed(&native.closing)?;
                    let index = sources.len();
                    run_paths[run_index].push(index);
                    owner_paths.entry((run_index, oi)).or_default().push(index);
                    acquisitions
                        .entry((
                            run_index,
                            (p.request.acquisition.purchase_ordinal - start) as usize,
                        ))
                        .or_default()
                        .push(index);
                    for (i, intent) in native.intents.iter().enumerate() {
                        if intent.position_id != o.position_id {
                            return Err("consumer native intent position lineage mismatch".into());
                        }
                        intents
                            .entry((run_index, (intent.recognition_ordinal - start) as usize))
                            .or_default()
                            .push((index, i));
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
            owner_paths,
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
    pub(crate) fn owner_paths(&self, run: usize, owner: usize) -> &[usize] {
        self.owner_paths
            .get(&(run, owner))
            .map_or(&[], Vec::as_slice)
    }
}
pub(crate) struct Runtime {
    pub run: usize,
    pub paths: Vec<PathBalances>,
}
impl Runtime {
    pub fn new(c: crate::product_event_owner::OwnerSources<'_>, run: usize) -> Self {
        Self {
            run,
            paths: vec![PathBalances::default(); c.source_count()],
        }
    }
    pub fn owner_summary(
        &self,
        c: crate::product_event_owner::OwnerSources<'_>,
        owner: usize,
    ) -> Result<[f64; 9], String> {
        let mut sum = [0.; 9];
        for &i in c.owner_paths(self.run, owner) {
            let f = c.scale(i).factor()?;
            for (v, x) in sum.iter_mut().zip(self.paths[i].summary()) {
                *v += x * f;
            }
        }
        Ok(sum)
    }
    pub fn owner_rights_summary(
        &self,
        c: crate::product_event_owner::OwnerSources<'_>,
        owner: usize,
    ) -> Result<Option<[f64; 2]>, String> {
        let mut sum = [0.; 2];
        let mut selected = false;
        for &i in c.owner_paths(self.run, owner) {
            if c.native(i).rights_opening().is_some() {
                selected = true;
                let factor = c.scale(i).factor()?;
                for (v, x) in sum.iter_mut().zip(self.paths[i].rights_summary()) {
                    *v += x * factor;
                }
            }
        }
        Ok(selected.then_some(sum))
    }
}
