//! Pre-Sim output census for the explicitly selected exclusive AC cut subset.
//! Every count is structural. Zero money/participation never removes a group.
use super::cut_shape::{charge, object, shape, spent, Shape};
use crate::owners::commercial::{fees::gl as fee_gl, intents::Compiled};
use crate::owners::consumer::state::{gl_with_owner, IncomeOwner};
use crate::transport::types::Spec;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("cut output census overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or("cut output census overflow".into())
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TableInventory {
    pub rows_upper: usize,
    pub shape_upper: Shape,
    pub fields: Vec<String>,
    pub template_scan_work_upper: usize,
}
type Tables = BTreeMap<String, TableInventory>;

#[derive(Clone, Debug, Serialize)]
pub struct CommercialCutInventory {
    pub schema: &'static str,
    pub prefix: Tables,
    pub visible: Tables,
    pub all_stream: Shape,
    pub visible_stream: Shape,
    pub decoded_tables: Shape,
    pub dictionary_upper: Shape,
    pub all_flushes_upper: usize,
    pub census_work_upper: usize,
    pub stream_batch_retained_bytes_upper: usize,
    pub stream_batch_memory: super::cut_sink::CutSinkMemory,
}

fn row(
    strings: &[(&str, Shape)],
    numbers: &str,
    children: &[(&str, Shape)],
) -> Result<(Shape, Vec<String>), String> {
    let mut fields = strings.to_vec();
    fields.extend(numbers.split_whitespace().map(|k| (k, Shape::number())));
    fields.extend_from_slice(children);
    let names = fields.iter().map(|(k, _)| k.to_string()).collect();
    Ok((object(&fields)?, names))
}
fn record(
    tables: &mut Tables,
    name: &str,
    count: usize,
    template: (Shape, Vec<String>),
) -> Result<(), String> {
    charge(template.0.scan_work()?)?;
    let target = tables.entry(name.into()).or_default();
    if !target.fields.is_empty() && target.fields != template.1 {
        return Err("cut output field template differs".into());
    }
    target.fields = template.1;
    target.template_scan_work_upper =
        add(target.template_scan_work_upper, template.0.scan_work()?)?;
    target.rows_upper = add(target.rows_upper, count)?;
    target.shape_upper = target.shape_upper.checked_add(template.0.times(count)?)?;
    Ok(())
}
fn total(tables: &Tables) -> Result<Shape, String> {
    let mut result = Shape {
        encoded_bytes_upper: 2,
        cells_upper: 1,
        string_bytes_upper: 0,
    };
    for (name, t) in tables {
        let mut array = t.shape_upper;
        array.encoded_bytes_upper = add(
            array.encoded_bytes_upper,
            add(2, t.rows_upper.saturating_sub(1))?,
        )?;
        array.cells_upper = add(array.cells_upper, 1)?;
        result = result.checked_add(shape(name)?)?.checked_add(array)?;
        result.encoded_bytes_upper = add(result.encoded_bytes_upper, 2)?; // colon + possible comma
    }
    Ok(result)
}
fn stream_total(tables: &Tables) -> Result<Shape, String> {
    // Dictionary-coded journal labels are integer indices. A short decoded
    // label is not an upper bound on a32-character native integer token.
    let mut upper = total(tables)?;
    if let Some(journal) = tables.get("journal") {
        let numeric = Shape::number()
            .checked_add(Shape {
                encoded_bytes_upper: 1,
                cells_upper: 0,
                string_bytes_upper: 0,
            })?
            .times(mul(journal.rows_upper, 8)?)?;
        // Adding rather than replacing the decoded shape also retains all
        // dictionary expansion/ordinary-row work in the census.
        upper = upper.checked_add(numeric)?;
    }
    Ok(upper)
}

#[derive(Default)]
struct Labels {
    scenario: Shape,
    account: Shape,
    entity: Shape,
    currency: Shape,
    position: Shape,
    phase: Shape,
    gl: Shape,
    dictionary: Shape,
}
impl Labels {
    fn dict(&mut self, value: &str) -> Result<Shape, String> {
        let s = shape(value)?;
        // Repeated labels conservatively count even when intern() reuses them.
        self.dictionary = self.dictionary.checked_add(s)?;
        self.dictionary.encoded_bytes_upper = add(self.dictionary.encoded_bytes_upper, 1)?;
        Ok(s)
    }
    fn gl(&mut self, value: &str) -> Result<(), String> {
        self.gl = self.gl.max(self.dict(value)?);
        Ok(())
    }
}

fn supported(spec: &Spec, owner: &Compiled<'_>) -> Result<(), String> {
    if !spec.netting_sets.is_empty()
        || !spec.policies.is_empty()
        || !spec.cashflows.is_empty()
        || !spec.reverse_severities.is_empty()
        || owner.position_owners.iter().any(|b| !*b)
        || spec.accounts.iter().any(|a| {
            [
                a.annual_fees,
                a.annual_costs,
                a.annual_dividends,
                a.tax_rate,
            ]
            .iter()
            .any(|v| *v != 0.)
        })
        || spec.accounts.iter().any(|a| {
            [a.cet1_floor, a.leverage_floor, a.lcr_floor, a.nsfr_floor]
                .iter()
                .any(|v| *v != 0.)
                || a.htm_asset_limit != 1.
        })
        || spec.positions.iter().any(|p| {
            p.classification != "ac"
                || !matches!(p.kind.as_str(), "loan" | "security")
                || !p.collateral_position.is_empty()
                || p.pledged_face != 0.
                || p.eligible_fraction != 0.
                || p.encumbered_fraction != 0.
        })
        || owner
            .bindings
            .iter()
            .any(|b| b.accounting.compact_bytes().is_some())
        || owner
            .fees
            .iter()
            .any(|f| f.source.compact_bytes().is_some())
    {
        return Err("selected cut output inventory requires exclusive finite AC owners with no generic funding, tax, policy, collateral, cashflow or netting overlays".into());
    }
    Ok(())
}

fn labels(spec: &Spec, owner: &Compiled<'_>) -> Result<Labels, String> {
    let mut l = Labels {
        scenario: shape("baseline")?,
        ..Labels::default()
    };
    l.dict("baseline")?;
    for s in &spec.scenarios {
        l.scenario = l.scenario.max(l.dict(&s.name)?);
    }
    for a in &spec.accounts {
        l.account = l.account.max(l.dict(&a.id)?);
        l.entity = l.entity.max(shape(&a.entity)?);
        l.currency = l.currency.max(shape(&a.currency)?);
    }
    for p in &spec.positions {
        l.position = l.position.max(l.dict(&p.id)?);
    }
    l.position = l.position.max(l.dict("")?);
    for event in [
        "opening",
        "native_verified_cut_opening",
        "commercial_nonintegral_fee",
    ] {
        l.phase = l.phase.max(l.dict(event)?);
    }
    for name in [
        "cash",
        "opening_equity",
        "oci",
        "asset_principal",
        "book_adjustment",
        "allowance",
        "accrued_interest",
        "restricted_cash",
        "dated_interest_receivable",
        "dated_deferred_income",
        "credit_default_interest_receivable",
        "credit_default_deferred_income",
        "memo:commercial_legal_coupon_right",
        "memo:commercial_legal_coupon_contra",
        "memo:commercial_legal_principal",
        "memo:commercial_legal_principal_contra",
    ] {
        l.gl(name)?;
    }
    if !owner.fees.is_empty() {
        // Reconciliation interns all fee stock endpoint names, including zero
        // stocks which have never appeared in a source posting.
        for name in [
            "commercial_fee_receivable",
            "commercial_fee_payable",
            "commercial_deferred_fee_income",
            "commercial_prepaid_fee_expense",
            "commercial_acquired_fee_asset",
            "commercial_acquired_fee_liability",
            "commercial_acquired_fee_asset_valuation_adjustment",
            "commercial_acquired_fee_liability_valuation_adjustment",
        ] {
            l.gl(name)?;
        }
    }
    for b in &owner.bindings {
        for i in 0..b.accounting.group_count() {
            charge(1)?;
            let g = b.accounting.group(i);
            if g.path_key != b.path_key {
                continue;
            }
            l.phase = l.phase.max(l.dict(g.phase)?);
            for p in &g.postings {
                l.gl(gl_with_owner(p.posting.account, IncomeOwner::Commercial))?;
            }
        }
    }
    for f in &owner.fees {
        for i in 0..f.source.group_count() {
            charge(1)?;
            for p in &f.source.group(i).postings {
                l.gl(fee_gl(p.account))?;
            }
        }
    }
    Ok(l)
}

fn initial_keys(spec: &Spec) -> Result<usize, String> {
    mul(
        add(mul(spec.accounts.len(), 3)?, mul(spec.positions.len(), 4)?)?,
        add(spec.scenarios.len(), 1)?,
    )
}
fn loan_lines(
    g: &portfolio_model_core::commercial_coupon_accounting::AtomicGroup,
) -> Result<usize, String> {
    add(
        add(mul(g.postings.len(), 2)?, mul(g.next.lots.len(), 2)?)?,
        if g.next.reviewed_credit_state.is_some() {
            2
        } else {
            0
        },
    )
}
fn keys(spec: &Spec, c: &Compiled<'_>) -> Result<usize, String> {
    let mut n = initial_keys(spec)?;
    for b in &c.bindings {
        for i in 0..b.accounting.group_count() {
            charge(1)?;
            let g = b.accounting.group(i);
            if g.path_key == b.path_key {
                n = add(n, loan_lines(&g)?)?;
            }
        }
    }
    for f in &c.fees {
        for i in 0..f.source.group_count() {
            charge(1)?;
            n = add(n, f.source.group(i).postings.len())?;
        }
    }
    Ok(n)
}

fn inventory(
    spec: &Spec,
    c: &Compiled<'_>,
    days: usize,
    finish: bool,
    opening_keys: usize,
    l: &Labels,
) -> Result<(Tables, usize), String> {
    let mut tables = Tables::new();
    let mut scans = 0usize;
    let mut group_count = 0usize;
    let mut lines = opening_keys;
    let common = [("scenario", l.scenario), ("position_id", l.position)];
    for b in &c.bindings {
        let mut snapshot = Shape::default();
        for i in 0..b.accounting.group_count() {
            charge(1)?;
            let g = b.accounting.group(i);
            if g.path_key != b.path_key {
                continue;
            }
            let atom = shape(&*g)?;
            let next = shape(&g.next)?;
            scans = add(scans, add(atom.scan_work()?, next.scan_work()?)?)?;
            snapshot = snapshot.max(next);
            group_count = add(group_count, 1)?;
            lines = add(lines, loan_lines(&g)?)?;
            record(&mut tables,"commercial_source_lineage",1,row(&[
                ("scenario",l.scenario),("position_id",l.position),("source_class",shape(b.source_class.identity())?),
                ("date_policy",shape(crate::owners::commercial::intents::DATE_POLICY)?)],
                "day native_credit_year source_origin_day global_native_credit_year path_key source_weight allocation_factor native_group_index transaction_number",&[("native_atomic_group",atom)])?)?;
        }
        let template = row(
            &[
                ("scenario", l.scenario),
                ("position_id", l.position),
                ("source_class", shape(b.source_class.identity())?),
            ],
            "day path_key source_weight allocation_factor source_origin_day",
            &[
                ("unweighted_book", snapshot),
                ("capital_ftp_eligible", Shape::boolean()),
            ],
        )?;
        record(&mut tables, "commercial_daily", days, template.clone())?;
        if finish {
            record(&mut tables, "commercial_closing", 1, template)?;
        }
    }
    for f in &c.fees {
        let mut state = Shape::default();
        let mut fee_id = Shape::default();
        for i in 0..f.source.group_count() {
            charge(1)?;
            let g = f.source.group(i);
            let atom = shape(&*g)?;
            let next = shape(&g.next)?;
            scans = add(scans, add(atom.scan_work()?, next.scan_work()?)?)?;
            state = state.max(next);
            fee_id = fee_id.max(shape(&g.event.fee_id)?);
            group_count = add(group_count, 1)?;
            lines = add(lines, g.postings.len())?;
            record(&mut tables,"commercial_fee_lineage",1,row(&common,
                "day path_key native_year source_weight allocation_factor native_group_index transaction_number",&[("native_fee_group",atom)])?)?;
        }
        let template = row(
            &[
                ("scenario", l.scenario),
                ("position_id", l.position),
                ("fee_id", fee_id),
            ],
            "day path_key source_weight allocation_factor",
            &[("native_fee_snapshot", state)],
        )?;
        record(
            &mut tables,
            "commercial_fee_daily",
            mul(days, f.source.fee_count())?,
            template.clone(),
        )?;
        if finish {
            record(
                &mut tables,
                "commercial_fee_closing",
                f.source.fee_count(),
                template,
            )?;
        }
    }
    record(
        &mut tables,
        "journal",
        lines,
        row(
            &[
                ("scenario", l.scenario),
                ("account", l.account),
                ("event", l.phase),
                ("gl_account", l.gl),
                ("instrument_id", l.position),
            ],
            "transaction_number day value",
            &[],
        )?,
    )?;
    record(
        &mut tables,
        "ledger",
        group_count,
        row(
            &[
                ("scenario", l.scenario),
                ("account", l.account),
                ("event", l.phase),
            ],
            "day cash earnings aoci memo_amount",
            &[],
        )?,
    )?;
    let scenarios = add(spec.scenarios.len(), 1)?;
    let account_runs = mul(spec.accounts.len(), scenarios)?;
    record(&mut tables,"exposures",mul(mul(days,spec.positions.len())?,scenarios)?,row(&[("scenario",l.scenario),("account",l.account),("position",l.position),("kind",shape("security")?)],"day balance allowance watch_fraction commitment market_factor encumbered_fraction accrued_interest",&[])?)?;
    record(&mut tables,"path",mul(days,account_runs)?,row(&[("scenario",l.scenario),("account",l.account),("entity",l.entity),("currency",l.currency),("lcr_status",shape("not_applicable")?),("nsfr_status",shape("not_applicable")?)],
        "day cash restricted_cash cash_floor assets liabilities equity aoci cet1 rwa cet1_ratio leverage_ratio usable_collateral undrawn hqla_proxy lcr_proxy nsfr_proxy reconciliation_error cash_headroom cet1_headroom leverage_headroom lcr_headroom nsfr_headroom htm_headroom",&[])?)?;
    record(
        &mut tables,
        "breaches",
        mul(6, account_runs)?,
        row(
            &[
                ("scenario", l.scenario),
                ("account", l.account),
                ("metric", shape("leverage")?),
            ],
            "day",
            &[],
        )?,
    )?;
    if finish {
        record(&mut tables,"summary",account_runs,row(&[("scenario",l.scenario),("account",l.account),("entity",l.entity),("currency",l.currency),
            ("lcr_applicability",shape("not_applicable")?),("lcr_status",shape("not_applicable")?),("nsfr_applicability",shape("not_applicable")?),("nsfr_status",shape("not_applicable")?)],
            "final_cash final_equity minimum_cash peak_cash_shortfall max_reconciliation_error first_cash_breach_day first_capital_breach_day first_leverage_breach_day first_lcr_breach_day first_nsfr_breach_day first_htm_breach_day htm_reclassification_day",&[])?)?;
        record(
            &mut tables,
            "trial_balance",
            keys(spec, c)?,
            row(
                &[
                    ("scenario", l.scenario),
                    ("account", l.account),
                    ("gl_account", l.gl),
                    ("instrument_id", l.position),
                ],
                "balance",
                &[],
            )?,
        )?;
        record(&mut tables,"closing_statements",account_runs,row(&[("scenario",l.scenario),("account",l.account),("currency",l.currency)],
            "assets liabilities equity cash restricted_cash intercompany_assets intercompany_liabilities reconciliation_error",&[])?)?;
        // One per distinct currency; the account count is a conservative upper.
        record(
            &mut tables,
            "consolidated",
            account_runs,
            row(
                &[("scenario", l.scenario), ("currency", l.currency)],
                "assets liabilities equity cash restricted_cash",
                &[],
            )?,
        )?;
        record(
            &mut tables,
            "attribution",
            mul(group_count, spec.scenarios.len())?,
            row(
                &[
                    ("scenario", l.scenario),
                    ("account", l.account),
                    ("event", l.phase),
                ],
                "cash_delta earnings_delta aoci_delta",
                &[],
            )?,
        )?;
    }
    Ok((tables, scans))
}

pub fn commercial_cut_inventory(
    old: &Spec,
    new: &Spec,
    a: &Compiled<'_>,
    b: &Compiled<'_>,
    cut: usize,
) -> Result<CommercialCutInventory, String> {
    inventory_pair(old, new, a, b, cut, false)
}

/// Complete one-owner output superset for a sequential component phase.
/// The revised phase includes all retained positions/groups, so its opening
/// inventory also bounds the surviving subset's cut opening. This inventories
/// output/Sink only; raw sources, all Sim fields and joint admission are separate.
pub fn commercial_sequential_cut_inventory(
    spec: &Spec,
    source: &Compiled<'_>,
    cut: usize,
) -> Result<CommercialCutInventory, String> {
    inventory_pair(spec, spec, source, source, cut, true)
}

fn inventory_pair(
    old: &Spec,
    new: &Spec,
    a: &Compiled<'_>,
    b: &Compiled<'_>,
    cut: usize,
    sequential: bool,
) -> Result<CommercialCutInventory, String> {
    let started = spent();
    supported(old, a)?;
    supported(new, b)?;
    if cut == 0 || cut >= old.horizon_days || old.horizon_days != new.horizon_days {
        return Err(
            "selected cut inventory requires a positive cut before the same complete horizon"
                .into(),
        );
    }
    let la = labels(old, a)?;
    let lb = labels(new, b)?;
    let (prefix, ca) = inventory(old, a, add(cut, 1)?, false, initial_keys(old)?, &la)?;
    let (visible, cb) = inventory(
        new,
        b,
        add(new.horizon_days - cut, 1)?,
        true,
        keys(old, a)?,
        &lb,
    )?;
    let p = stream_total(&prefix)?;
    let v = stream_total(&visible)?;
    let decoded_tables = total(&visible)?;
    let all_rows = prefix
        .values()
        .chain(visible.values())
        .try_fold(0usize, |n, t| add(n, t.rows_upper))?;
    let scenes = add(old.scenarios.len(), 1)?;
    let flushes = add(add(all_rows / 16384, mul(scenes, 2)?)?, 1)?;
    let mut headers = Shape::default();
    // Each flush emits up to one frame PER selected table, not one in total.
    for name in visible.keys().chain(prefix.keys()) {
        headers = headers.checked_add(shape(&serde_json::json!({"table":name,"rows":[]}))?)?;
    }
    let journal_header = serde_json::json!({"table":"journal","columns":{"transaction_number":[],"scenario":[],"day":[],"account":[],"event":[],"gl_account":[],"instrument_id":[],"value":[]},"dictionary":[]});
    headers = headers.checked_add(shape(&journal_header)?)?;
    headers.encoded_bytes_upper = add(
        headers.encoded_bytes_upper,
        add(prefix.len(), add(visible.len(), 1)?)?,
    )?;
    let header_frames = headers.times(flushes)?;
    // Muted prefix dictionaries may be resent at resume. No label is omitted
    // because a balance/allocation is zero; repetitions only enlarge the bound.
    let dictionary = la
        .dictionary
        .checked_add(lb.dictionary)?
        .times(mul(scenes, 2)?)?;
    let mut completion_value = serde_json::json!({"complete":true,"protocol":if sequential{9}else{8},"cut_day":cut,"native_cut_state_moved":true,"original_calendar_retained":true,"new_sim_started_at_cut":true,"prior_publication_verified":false});
    if sequential {
        completion_value["sequential_source_owners"] = serde_json::Value::Bool(true);
    }
    let mut completion = shape(&completion_value)?;
    completion.encoded_bytes_upper = add(completion.encoded_bytes_upper, 1)?;
    let extra = header_frames
        .checked_add(dictionary)?
        .checked_add(completion)?;
    let all_stream = p.checked_add(v)?.checked_add(extra)?;
    let visible_stream = v.checked_add(extra)?;
    let templates = prefix
        .values()
        .chain(visible.values())
        .try_fold(0usize, |n, t| add(n, t.template_scan_work_upper))?;
    let declared_census_work_upper = add(
        templates,
        add(
            add(ca, cb)?,
            add(
                la.dictionary.scan_work()?,
                add(lb.dictionary.scan_work()?, headers.scan_work()?)?,
            )?,
        )?,
    )?;
    // Frames borrow their rows. Include retained high-water row buffers,
    // dictionary owners, column buffers and growth overlap separately.
    let (stream_batch_memory, stream_batch_census_work_upper) = super::cut_sink::inventory(
        &prefix,
        &visible,
        add(
            total(&prefix)?.encoded_bytes_upper,
            decoded_tables.encoded_bytes_upper,
        )?,
        dictionary,
    )?;
    let stream_batch_retained_bytes_upper = stream_batch_memory.total()?;
    let census_work_upper = match (started, spent()) {
        (Some(a), Some(b)) => b.checked_sub(a).ok_or("cut census ticket state differs")?,
        _ => add(declared_census_work_upper, stream_batch_census_work_upper)?,
    };
    Ok(CommercialCutInventory {
        schema: if sequential {
            "commercial-sequential-cut-output-inventory-1"
        } else {
            "commercial-cut-output-inventory-2"
        },
        prefix,
        visible,
        all_stream,
        visible_stream,
        decoded_tables,
        dictionary_upper: dictionary,
        all_flushes_upper: flushes,
        census_work_upper,
        stream_batch_retained_bytes_upper,
        stream_batch_memory,
    })
}

impl CommercialCutInventory {
    pub fn verify_tables(&self, tables: &BTreeMap<String, Vec<Value>>) -> Result<(), String> {
        for (name, rows) in tables {
            let bound = self
                .visible
                .get(name)
                .ok_or("selected cut emitted an uninventoried table")?;
            if rows.len() > bound.rows_upper {
                return Err(
                    "selected cut emitted row count exceeds native structural inventory".into(),
                );
            }
            let mut actual = Shape::default();
            for r in rows {
                let object = r.as_object().ok_or("selected cut emitted nonobject row")?;
                if object.len() != bound.fields.len()
                    || bound.fields.iter().any(|k| !object.contains_key(k))
                {
                    return Err(
                        "selected cut emitted field identity exceeds native structural inventory"
                            .into(),
                    );
                }
                actual = actual.checked_add(shape(r)?)?;
            }
            if actual.encoded_bytes_upper > bound.shape_upper.encoded_bytes_upper
                || actual.cells_upper > bound.shape_upper.cells_upper
                || actual.string_bytes_upper > bound.shape_upper.string_bytes_upper
            {
                return Err(format!("selected cut emitted complete row shape exceeds native structural inventory/{name}"));
            }
        }
        Ok(())
    }
}
