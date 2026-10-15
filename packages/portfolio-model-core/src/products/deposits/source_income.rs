//! Explicit reviewed source-owned principal/income contract; no accounting approval.
use crate::{
    dated_income::{DatedIncomeRequest, DatedIncomeResult, Identity, IncomeRow, YieldChange},
    deposit_continuation as c,
    deposit_principal::PrincipalAtom,
    deposit_state as captured_deposit,
    deposit_state::DepositStateResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const SCHEMA: &str = "deposit-source-income-1";
pub const OWNERSHIP: &str = "source_retained_mass_principal_with_basis_reclassification-1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub review: Identity,
    pub ownership: String,
    pub continuation: c::Request,
}
#[derive(Debug, Clone, Serialize)]
pub struct Transition {
    pub allocation_index: usize,
    pub ordinal: i32,
    pub principal_cash: f64,
    pub cash_subtraction_principal: f64,
    pub principal_partition_delta: f64,
    pub basis_partition_delta: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceOwned {
    pub source: DepositStateResult,
    pub atoms: Vec<PrincipalAtom>,
    pub transitions: Vec<Transition>,
    pub income_request: DatedIncomeRequest,
    pub income: DatedIncomeResult,
    pub rights: c::Rights,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub raw_bytes: usize,
    pub source_atom_slots: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub raw_request: Request,
    pub admission: Admission,
    pub original: SourceOwned,
    pub current: SourceOwned,
    pub original_pv: f64,
    pub basis_revision: f64,
    pub revision_rights: c::Rights,
    pub journal_deltas: Vec<c::Group>,
    pub journal_closing: BTreeMap<String, f64>,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub installed_adoption: bool,
    pub saved_book_adoption: bool,
    pub mixed_ledger_adoption: bool,
    pub capital_ftp_eligible: bool,
}
pub fn parse(bytes: &[u8]) -> Result<Request, String> {
    if bytes.len() > 1_048_576 {
        return Err("raw <=1MiB required".into());
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    for k in ["original", "current"] {
        let s = &value["continuation"][k];
        if s["identity"]["measure"].as_str() != Some("physical")
            || !matches!(s["day_count"].as_str(), Some("act360" | "act365_fixed"))
        {
            return Err("strict physical/day-count strings required".into());
        }
    }
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}
pub fn audit(r: &Request) -> Result<Admission, String> {
    if r.schema != SCHEMA || r.ownership != OWNERSHIP {
        return Err("explicit new principal ownership required".into());
    }
    c::bounded_identity(&r.review)?;
    let old = c::audit_request(&r.continuation)?;
    let raw_bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    let slots = (r.continuation.original.horizon_days as usize + 2) * 2;
    let work = old
        .work_units
        .checked_add(slots * 4096)
        .and_then(|v| v.checked_add(raw_bytes * 8))
        .ok_or("work overflow")?;
    let bytes = old
        .reserved_bytes
        .checked_add(slots * 8192)
        .and_then(|v| v.checked_add(raw_bytes * 12))
        .ok_or("bytes overflow")?;
    if raw_bytes > 1_048_576 || work > c::MAX_WORK || bytes > c::MAX_BYTES {
        return Err("joined pre-generation work/bytes exceeded".into());
    }
    Ok(Admission {
        work_units: work,
        reserved_bytes: bytes,
        raw_bytes,
        source_atom_slots: slots,
    })
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !c::near(a, b) {
        Err(format!("unchanged source/income/journal gate {a} != {b}"))
    } else {
        Ok(())
    }
}
fn compile(
    r: &Request,
    changed: bool,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<SourceOwned, String> {
    compile_mode(r, changed, false, check)
}
fn compile_mode(
    r: &Request,
    changed: bool,
    counted: bool,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<SourceOwned, String> {
    let raw = &r.continuation;
    let contract = if changed { &raw.current } else { &raw.original };
    let (source, atoms) = if counted {
        captured_deposit::capture_principal_counted_with_check(
            contract,
            crate::products::deposits::source_income_v2::MAX_DAYS,
            check,
        )?
    } else {
        captured_deposit::capture_principal_with_check(contract, check)?
    };
    let cash_index: std::collections::HashMap<_, _> = if counted {
        source
            .cashflows
            .iter()
            .enumerate()
            .map(|(i, c)| (c.event_id.as_str(), i))
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    let income_request = c::income_request(raw, &source, changed)?;
    let mut transitions = Vec::new();
    let mut p = contract.segments[0].opening_balance;
    let mut seen = std::collections::HashSet::new();
    for (index, atom) in atoms.iter().enumerate() {
        check()?;
        if atom.allocation_index != index || atom.principal_before.to_bits() != p.to_bits() {
            return Err("native atom continuity".into());
        }
        let mut principal_cash = 0.;
        for id in &atom.cash_event_ids {
            let cash = if counted {
                cash_index.get(id.as_str()).map(|i| &source.cashflows[*i])
            } else {
                source.cashflows.iter().find(|v| &v.event_id == id)
            }
            .ok_or("native atom cash ID absent")?;
            if !seen.insert(id)
                || cash.ordinal != atom.ordinal
                || cash.segment_id != atom.segment_id
            {
                return Err("atom cash once/date ownership".into());
            }
            principal_cash -= cash.principal;
        }
        let by_cash = p - principal_cash;
        let dp = atom.principal_after - by_cash;
        for v in [p, principal_cash, by_cash, dp, atom.principal_after] {
            c::money(v)?;
        }
        transitions.push(Transition {
            allocation_index: index,
            ordinal: atom.ordinal,
            principal_cash,
            cash_subtraction_principal: by_cash,
            principal_partition_delta: dp,
            basis_partition_delta: -dp,
        });
        p = atom.principal_after;
    }
    for cash in &source.cashflows {
        if cash.principal != 0. && !seen.contains(&cash.event_id) {
            return Err("unowned principal cash".into());
        }
    }
    if p != 0. || source.closing_balance != 0. {
        return Err("unweighted native principal tail incomplete".into());
    }
    let income = income(&income_request, &atoms, counted, check)?;
    let rights = rights_mode(
        &source,
        income_request.full_contract_end_ordinal,
        counted,
        check,
    )?;
    c::complete_rights(&rights)?;
    for v in [
        income.remaining_principal,
        income.remaining_accrued_interest,
        income.remaining_gross_carrying_value,
    ] {
        near(v, 0.)?;
    }
    near(rights.total_right, income.remaining_accrued_interest)?;
    Ok(SourceOwned {
        source,
        atoms,
        transitions,
        income_request,
        income,
        rights,
    })
}
// Receipts are compiler-private outputs of full native raw projection, never Request fields.
fn income(
    r: &DatedIncomeRequest,
    atoms: &[PrincipalAtom],
    counted: bool,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<DatedIncomeResult, String> {
    let atom_index: std::collections::BTreeMap<_, _> = if counted {
        atoms
            .iter()
            .map(|a| (a.ordinal, a.principal_after))
            .collect()
    } else {
        std::collections::BTreeMap::new()
    };
    let mut p = r.opening_principal;
    let mut accrued = r.opening_accrued_interest;
    let mut carry = r.opening_gross_carrying_value;
    let y = r.original_annual_effective_rate;
    let mut date = r.as_of_ordinal;
    let mut rows = Vec::new();
    for e in &r.events {
        check()?;
        let opening = carry;
        let earned = carry * (y.ln_1p() * (e.payment_ordinal - date) as f64 / 365.).exp_m1();
        let amort = earned - e.contractual_interest_earned - e.integral_fee_cash;
        // Exactly the unchanged Core income/cash reduction order.
        carry += earned - e.principal_cash - e.interest_cash - e.integral_fee_cash;
        p = if counted {
            atom_index.get(&e.payment_ordinal).copied().unwrap_or(p)
        } else {
            atoms
                .iter()
                .rfind(|a| a.ordinal == e.payment_ordinal)
                .map_or(p, |a| a.principal_after)
        };
        accrued += e.contractual_interest_earned - e.interest_cash;
        let cash =
            e.principal_cash + e.interest_cash + e.integral_fee_cash + e.nonintegral_fee_cash;
        let mut modification = 0.;
        let mut review = None;
        if let Some(YieldChange::CatchupOriginal {
            review: policy,
            complete_future_cash,
        }) = &e.yield_change
        {
            let value = complete_future_cash
                .iter()
                .map(|v| {
                    v.amount
                        * (-y.ln_1p() * (v.payment_ordinal - e.payment_ordinal) as f64 / 365.).exp()
                })
                .sum::<f64>();
            modification = value - carry;
            carry = value;
            review = Some(policy.clone());
        } else if e.yield_change.is_some() {
            return Err("no prospective yield in source-owned proof".into());
        }
        let bound = 64. * f64::EPSILON * (opening.abs() + cash.abs() + earned.abs());
        if carry < 0. && p == 0. && accrued.abs() <= bound && carry.abs() <= bound {
            carry = 0.;
        }
        if carry < 0. {
            return Err(
                "negative gross carrying value requires explicit recognition/writeoff policy"
                    .into(),
            );
        }
        let basis = carry - p - accrued;
        for v in [p, accrued, carry, basis, earned, amort, modification, cash] {
            c::money(v)?;
        }
        rows.push(IncomeRow {
            event_id: e.event_id.clone(),
            source: e.source.clone(),
            payment_ordinal: e.payment_ordinal,
            opening_gross_carrying_value: opening,
            annual_effective_rate: y,
            effective_interest: earned,
            contractual_interest_earned: e.contractual_interest_earned,
            basis_amortization: amort,
            modification_gain_loss: modification,
            recognized_nonintegral_fee_income: e.nonintegral_fee_cash,
            cash_received: cash,
            closing_principal: p,
            closing_accrued_interest: accrued,
            closing_gross_carrying_value: carry,
            closing_book_adjustment: basis,
            yield_review: review,
        });
        date = e.payment_ordinal;
    }
    Ok(DatedIncomeResult {
        schema: "deposit-source-owned-income-result-1".into(),
        contract: r.contract.clone(),
        policy: r.policy.clone(),
        original_yield_anchor: r.original_yield_anchor.clone(),
        currency: r.currency.clone(),
        monetary_unit: r.monetary_unit.clone(),
        measure: r.measure,
        rows,
        remaining_principal: p,
        remaining_accrued_interest: accrued,
        remaining_gross_carrying_value: carry,
    })
}
fn group(
    groups: &mut Vec<c::Group>,
    row: usize,
    ordinal: i32,
    kind: &str,
    review: &Identity,
    lines: Vec<(&str, f64)>,
) -> Result<(), String> {
    for (_, v) in &lines {
        c::money(*v)?;
    }
    if lines.iter().map(|(_, v)| *v).sum::<f64>().abs() > 1e-9 {
        return Err("atomic journal imbalance".into());
    }
    groups.push(c::Group {
        ordinal,
        kind: kind.into(),
        source_income_row: row,
        review: review.clone(),
        lines: lines
            .into_iter()
            .map(|(a, v)| c::Line {
                account: a.into(),
                amount: v,
            })
            .collect(),
    });
    Ok(())
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let admission = audit(r)?;
    check()?;
    let original = compile(r, false, &mut check)?;
    let current = compile(r, true, &mut check)?;
    evaluate_generated(r, admission, original, current, false, &mut check)
}
pub(crate) fn evaluate_counted(
    r: &Request,
    admission: Admission,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let original = compile_mode(r, false, true, &mut check)?;
    let current = compile_mode(r, true, true, &mut check)?;
    evaluate_generated(r, admission, original, current, true, &mut check)
}
fn rights_mode(
    s: &DepositStateResult,
    through: i32,
    counted: bool,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<c::Rights, String> {
    if counted {
        Ok(
            crate::products::deposits::fifo_capacity::allocate_without_output_count(
                s, through, check,
            )?
            .expand(),
        )
    } else {
        c::rights(s, through, check)
    }
}
fn evaluate_generated(
    r: &Request,
    admission: Admission,
    original: SourceOwned,
    current: SourceOwned,
    counted: bool,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let raw = &r.continuation;
    let oc = c::obligations(&original.source)?;
    let cc = c::obligations(&current.source)?;
    for day in raw.original.as_of_ordinal..=raw.revision_ordinal {
        if !c::exact(&oc.get(&day), &cc.get(&day))? {
            return Err("actual cash prefix changed".into());
        }
    }
    let old: Vec<_> = original
        .source
        .interest
        .iter()
        .filter(|v| v.end_ordinal <= raw.revision_ordinal)
        .collect();
    let new: Vec<_> = current
        .source
        .interest
        .iter()
        .filter(|v| v.end_ordinal <= raw.revision_ordinal)
        .collect();
    if !c::exact(&old, &new)? {
        return Err("earned prefix changed".into());
    }
    let y = raw.anchor.original_annual_effective_rate;
    let original_pv = oc
        .iter()
        .map(|(&d, &(p, i))| {
            (p + i) * (-y.ln_1p() * (d - raw.original.as_of_ordinal) as f64 / 365.).exp()
        })
        .sum::<f64>();
    near(original_pv, raw.anchor.original_gross_carrying_value)?;
    let idx = (raw.revision_ordinal - raw.original.as_of_ordinal - 1) as usize;
    let row = &current.income.rows[idx];
    near(
        row.opening_gross_carrying_value + row.effective_interest - row.cash_received,
        original.income.rows[idx].closing_gross_carrying_value,
    )?;
    let revision_rights = rights_mode(&current.source, raw.revision_ordinal, counted, check)?;
    near(revision_rights.total_right, row.closing_accrued_interest)?;
    let basis_revision = row.modification_gain_loss;
    let transitions: std::collections::BTreeMap<_, Vec<&Transition>> = if counted {
        let mut out = std::collections::BTreeMap::new();
        for t in &current.transitions {
            out.entry(t.ordinal).or_insert_with(Vec::new).push(t);
        }
        out
    } else {
        std::collections::BTreeMap::new()
    };
    let mut groups = Vec::new();
    let mut gl = BTreeMap::from([
        (
            "deposit_principal".to_string(),
            -raw.anchor.original_principal,
        ),
        (
            "deposit_book_basis".to_string(),
            -(raw.anchor.original_gross_carrying_value - raw.anchor.original_principal),
        ),
        ("interest_payable".to_string(), 0.),
    ]);
    for (n, x) in current.income.rows.iter().enumerate() {
        check()?;
        let e = &current.income_request.events[n];
        let begin = groups.len();
        group(
            &mut groups,
            n,
            x.payment_ordinal,
            "original_eir_and_earned",
            &raw.contract,
            vec![
                ("interest_payable", -x.contractual_interest_earned),
                ("deposit_book_basis", -x.basis_amortization),
                ("pnl:deposit_original_eir_expense", x.effective_interest),
            ],
        )?;
        group(
            &mut groups,
            n,
            x.payment_ordinal,
            "actual_cash_settlement",
            &raw.contract,
            vec![
                ("deposit_principal", e.principal_cash),
                ("interest_payable", e.interest_cash),
                ("cash", -x.cash_received),
            ],
        )?;
        let selected: Vec<_> = if counted {
            transitions
                .get(&x.payment_ordinal)
                .cloned()
                .unwrap_or_default()
        } else {
            current
                .transitions
                .iter()
                .filter(|v| v.ordinal == x.payment_ordinal)
                .collect()
        };
        for t in selected {
            group(
                &mut groups,
                n,
                x.payment_ordinal,
                "source_principal_basis_reclassification",
                &r.review,
                vec![
                    ("deposit_principal", -t.principal_partition_delta),
                    ("deposit_book_basis", t.principal_partition_delta),
                ],
            )?;
        }
        if x.yield_review.is_some() {
            group(
                &mut groups,
                n,
                x.payment_ordinal,
                "reviewed_expectation_revision",
                &raw.policy.review,
                vec![
                    ("deposit_book_basis", -x.modification_gain_loss),
                    ("pnl:deposit_basis_revision", x.modification_gain_loss),
                ],
            )?;
        }
        for g in &groups[begin..] {
            for l in &g.lines {
                *gl.entry(l.account.clone()).or_insert(0.) += l.amount;
            }
        }
        near(-gl["deposit_principal"], x.closing_principal)?;
        near(-gl["deposit_book_basis"], x.closing_book_adjustment)?;
        near(-gl["interest_payable"], x.closing_accrued_interest)?;
    }
    check()?;
    let result = ResultData {
        schema: if counted {
            crate::products::deposits::source_income_v2::SCHEMA.into()
        } else {
            SCHEMA.into()
        },
        raw_request: r.clone(),
        admission,
        original,
        current,
        original_pv,
        basis_revision,
        revision_rights,
        journal_deltas: groups,
        journal_closing: gl,
        accounting_standard_approval: false,
        empirical_validation: false,
        installed_adoption: false,
        saved_book_adoption: false,
        mixed_ledger_adoption: false,
        capital_ftp_eligible: false,
    };
    // Counted tier seals and sizes the COMPLETE source once in its wrapper.
    let size = if counted {
        0
    } else {
        serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
    };
    if size > result.admission.reserved_bytes {
        return Err("actual output capacity exceeded".into());
    }
    check()?;
    Ok(result)
}
