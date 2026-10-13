//! Artifact-only supplied Physical continuation; no public/accounting approval.
use crate::cashflow::CashflowMeasure;
use crate::dated_income::{
    self, DatedIncomeRequest, DatedIncomeResult, FutureCash, Identity, IncomeEvent, YieldChange,
};
use crate::deposit_revised_income::{self, Anchor, RevisionPolicy, RunSource};
use crate::deposit_state::{
    self, DepositDriver, DepositPoint, DepositStateRequest, DepositStateResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "deposit-performing-continuation-1";
pub const MAX_DAYS: u32 = 64;
pub const MAX_MONEY: f64 = 10_000.;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

pub fn parse_request(bytes: &[u8]) -> Result<Request, String> {
    if bytes.len() > 1_048_576 {
        return Err("continuation raw input exceeds 1MiB".into());
    }
    // Inspect only enum spelling; selected typed decode still reads original bytes.
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    for name in ["original", "current"] {
        if value[name]["identity"]["measure"].as_str() != Some("physical")
            || !matches!(
                value[name]["day_count"].as_str(),
                Some("act360" | "act365_fixed")
            )
        {
            return Err("declared measure/day-count must use strict string form".into());
        }
    }
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixHistory {
    pub identity: Identity,
    /// Inclusive observation history from purchase-max_lag-1 through revision.
    pub through_ordinal: i32,
    pub drivers: Vec<DepositDriver>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub contract: Identity,
    pub anchor: Anchor,
    pub policy: RevisionPolicy,
    pub revision_ordinal: i32,
    pub prefix: PrefixHistory,
    /// Complete supplied purchase-to-maturity original expectations.
    pub original: DepositStateRequest,
    /// Complete current expectations from the same original purchase, not a new opening.
    pub current: DepositStateRequest,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub raw_bytes: usize,
    pub work_units: usize,
    pub reserved_bytes: usize,
    pub full_end_ordinal: i32,
    pub prefix_days: u32,
    pub allocation_slots_reserved: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Origin {
    pub native_interest_index: usize,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub earned: f64,
    pub allocated: f64,
    pub remaining: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Allocation {
    pub native_interest_index: usize,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Settlement {
    pub native_cash_index: usize,
    pub native_event_id: String,
    pub ordinal: i32,
    pub full_native_retirement: bool,
    pub actual_bank_cash_interest: f64,
    pub allocations: Vec<Allocation>,
    /// Visible difference between allocated earned rights and unchanged cash.
    pub retained_cash_roundoff_right: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Rights {
    pub origins: Vec<Origin>,
    pub settlements: Vec<Settlement>,
    pub remaining_origin_rights: f64,
    pub retained_cash_roundoff_right: f64,
    pub total_right: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct RevisionOpening {
    /// Native end-of-previous-interval point, before start-of-date withdrawals.
    pub native_point: DepositPoint,
    pub same_date_start_cash_indices: Vec<usize>,
    /// Paid rate after the unchanged revision-date transition, used for the next interval.
    pub next_interval_paid_rate: f64,
    pub lag_observations: Vec<DepositDriver>,
    pub principal_after_all_date_cash: f64,
    pub accrued_after_all_date_cash: f64,
    pub gross_carrying_before_revision: f64,
    pub book_basis_before_revision: f64,
    pub accumulated_original_eir: f64,
    pub actual_bank_principal_cash_to_revision: f64,
    pub actual_bank_interest_cash_to_revision: f64,
    pub unretired_earned_origins: Rights,
}
#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub account: String,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub ordinal: i32,
    pub kind: String,
    pub source_income_row: usize,
    pub review: Identity,
    pub lines: Vec<Line>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub raw_request: Request,
    pub admission: Admission,
    pub original_source: DepositStateResult,
    pub current_source: DepositStateResult,
    pub original_pv: f64,
    pub original_income_request: DatedIncomeRequest,
    pub original_income: DatedIncomeResult,
    pub current_income_request: DatedIncomeRequest,
    pub current_income: DatedIncomeResult,
    pub revision_opening: RevisionOpening,
    pub remaining_expected_cash_pv: f64,
    pub basis_revision: f64,
    pub original_full_rights: Rights,
    pub current_full_rights: Rights,
    pub journal_deltas: Vec<Group>,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub installed_adoption: bool,
    pub saved_book_adoption: bool,
    pub cohort_adoption: bool,
    pub capital_ftp_eligible: bool,
}
pub fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}
pub fn money(v: f64) -> Result<(), String> {
    if v.is_finite() && v.abs() <= MAX_MONEY {
        Ok(())
    } else {
        Err("continuation money exceeds finite 10000 domain".into())
    }
}
pub fn exact<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_vec(a).map_err(|e| e.to_string())?
        == serde_json::to_vec(b).map_err(|e| e.to_string())?)
}
pub fn bounded_identity(i: &Identity) -> Result<(), String> {
    if [&i.id, &i.revision]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err("complete prefix identity required".into());
    }
    Ok(())
}
pub fn audit_request(r: &Request) -> Result<Admission, String> {
    let raw_bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw_bytes > 1_048_576 || r.schema != SCHEMA {
        return Err("explicit continuation schema and <=1MiB raw required".into());
    }
    bounded_identity(&r.prefix.identity)?;
    let start = r.original.as_of_ordinal;
    let end = start
        .checked_add(i32::try_from(r.original.horizon_days).map_err(|_| "days overflow")?)
        .ok_or("end overflow")?;
    if r.original.horizon_days > MAX_DAYS
        || r.revision_ordinal <= start
        || r.revision_ordinal >= end
        || r.prefix.through_ordinal != r.revision_ordinal
    {
        return Err(
            "one later revision inside complete original/current <=64 day tails required".into(),
        );
    }
    if r.policy.expectation_policy != "later_date_original_yield_catchup-1"
        || r.policy.phase_policy != "all_date_recognition_settlement_then_revision-1"
    {
        return Err("explicit later-date after-all-cash revision policy required".into());
    }
    // Reuse unchanged schema1 domain admission only; no schema1 economics execute.
    // Clone capacities and raw/source metadata are included in this component reservation.
    let mut policy = r.policy.clone();
    policy.expectation_policy = "purchase_asof_original_yield_catchup-1".into();
    policy.phase_policy =
        "original_opening_then_revision_then_daily_recognition_settlement-1".into();
    deposit_revised_income::audit_request(&deposit_revised_income::Request {
        schema: deposit_revised_income::SCHEMA.into(),
        contract: r.contract.clone(),
        anchor: r.anchor.clone(),
        policy,
        original: r.original.clone(),
        runs: vec![RunSource {
            run_id: r.current.identity.run_id.clone(),
            scenario_id: "explicit-current".into(),
            severity: 1.,
            revised: r.current.clone(),
        }],
    })?;
    if r.original.segments[0]
        .maturity_ordinal
        .is_none_or(|d| d <= r.revision_ordinal)
    {
        return Err("revision requires live principal before actual maturity".into());
    }
    let prefix: Vec<_> = r
        .original
        .drivers
        .iter()
        .filter(|d| d.ordinal <= r.revision_ordinal)
        .collect();
    if r.prefix.drivers.len() != prefix.len()
        || !prefix
            .iter()
            .zip(&r.prefix.drivers)
            .all(|(a, b)| exact(*a, b).unwrap_or(false))
    {
        return Err("missing or changed complete supplied purchase/lag/prefix observations".into());
    }
    for (old, new) in r.original.drivers.iter().zip(&r.current.drivers) {
        if old.ordinal <= r.revision_ordinal && !exact(old, new)? {
            return Err("current changes supplied observations through the revision date".into());
        }
    }
    let days = r.original.horizon_days as usize;
    let cash_slots = days
        .checked_mul(3)
        .and_then(|v| v.checked_add(2))
        .ok_or("cash bound overflow")?;
    let allocation_slots_reserved = days
        .checked_mul(cash_slots)
        .ok_or("allocation bound overflow")?;
    let work_units = days
        .checked_mul(4_096)
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(allocation_slots_reserved.checked_mul(48)?))
        .and_then(|v| v.checked_add(raw_bytes.checked_mul(8)?))
        .ok_or("continuation work overflow")?;
    let reserved_bytes = days
        .checked_add(2)
        .and_then(|v| v.checked_mul(4 * 32_768))
        .and_then(|v| v.checked_add(allocation_slots_reserved.checked_mul(512)?))
        .and_then(|v| v.checked_add(raw_bytes.checked_mul(12)?))
        .ok_or("continuation byte overflow")?;
    if work_units > MAX_WORK || reserved_bytes > MAX_BYTES {
        return Err("continuation pre-generation work/bytes exceeded".into());
    }
    Ok(Admission {
        raw_bytes,
        work_units,
        reserved_bytes,
        full_end_ordinal: end,
        prefix_days: (r.revision_ordinal - start) as u32,
        allocation_slots_reserved,
    })
}
pub fn obligations(s: &DepositStateResult) -> Result<BTreeMap<i32, (f64, f64)>, String> {
    let mut out = BTreeMap::new();
    if !near(s.closing_balance, 0.) || !near(s.closing_accrued_interest, 0.) {
        return Err("complete source tail required".into());
    }
    for p in &s.points {
        for v in p.balances.iter().chain(&p.accrued_interest) {
            money(*v)?;
        }
    }
    for i in &s.interest {
        money(i.amount)?;
        if i.amount < 0. {
            return Err("negative contractual coupon unsupported".into());
        }
    }
    for c in &s.cashflows {
        for v in [c.principal, c.cash_interest] {
            money(v)?;
            if v > 0. {
                return Err("inflow/signed coupon ownership unsupported".into());
            }
        }
        let e = out.entry(c.ordinal).or_insert((0., 0.));
        e.0 -= c.principal;
        e.1 -= c.cash_interest;
        money(e.0)?;
        money(e.1)?;
        money(e.0 + e.1)?;
    }
    Ok(out)
}
pub fn income_request(
    r: &Request,
    s: &DepositStateResult,
    revision: bool,
) -> Result<DatedIncomeRequest, String> {
    let cash = obligations(s)?;
    let earned: BTreeMap<_, _> = s
        .interest
        .iter()
        .map(|i| (i.end_ordinal, i.amount))
        .collect();
    let mut events = Vec::with_capacity(r.original.horizon_days as usize);
    let end = r.original.as_of_ordinal + r.original.horizon_days as i32;
    for day in r.original.as_of_ordinal + 1..=end {
        let (p, i) = cash.get(&day).copied().unwrap_or((0., 0.));
        let change = if revision && day == r.revision_ordinal {
            Some(YieldChange::CatchupOriginal {
                review: r.policy.review.clone(),
                complete_future_cash: cash
                    .range(day + 1..)
                    .map(|(&payment_ordinal, &(p, i))| FutureCash {
                        payment_ordinal,
                        amount: p + i,
                    })
                    .collect(),
            })
        } else {
            None
        };
        events.push(IncomeEvent {
            event_id: format!("day_{day}"),
            source: r.contract.clone(),
            payment_ordinal: day,
            contractual_interest_earned: earned.get(&day).copied().unwrap_or(0.),
            principal_cash: p,
            interest_cash: i,
            integral_fee_cash: 0.,
            nonintegral_fee_cash: 0.,
            yield_change: change,
        });
    }
    // Cash at purchase would need a distinct opening/acquisition policy.
    if cash.contains_key(&r.original.as_of_ordinal) {
        return Err("purchase-date withdrawal/cash unsupported by this proof".into());
    }
    Ok(DatedIncomeRequest {
        schema: dated_income::SCHEMA.into(),
        contract: r.contract.clone(),
        original_yield_anchor: r.anchor.original_yield_anchor.clone(),
        policy: r.policy.review.clone(),
        currency: r.original.identity.currency.clone(),
        monetary_unit: "currency_units".into(),
        measure: CashflowMeasure::Physical,
        as_of_ordinal: r.original.as_of_ordinal,
        full_contract_end_ordinal: end,
        opening_principal: r.anchor.original_principal,
        opening_accrued_interest: 0.,
        opening_gross_carrying_value: r.anchor.original_gross_carrying_value,
        original_annual_effective_rate: r.anchor.original_annual_effective_rate,
        opening_annual_effective_rate: r.anchor.original_annual_effective_rate,
        events,
    })
}
pub fn rights(
    s: &DepositStateResult,
    through: i32,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<Rights, String> {
    let mut origins: Vec<_> = s
        .interest
        .iter()
        .enumerate()
        .filter(|(_, x)| x.end_ordinal <= through)
        .map(|(native_interest_index, x)| Origin {
            native_interest_index,
            start_ordinal: x.start_ordinal,
            end_ordinal: x.end_ordinal,
            earned: x.amount,
            allocated: 0.,
            remaining: x.amount,
        })
        .collect();
    let mut settlements = Vec::new();
    for (native_cash_index, c) in s
        .cashflows
        .iter()
        .enumerate()
        .filter(|(_, c)| c.ordinal <= through && c.cash_interest != 0.)
    {
        check()?;
        let full = matches!(c.kind.as_str(), "interest_payment" | "contractual_maturity");
        let actual = -c.cash_interest;
        let mut needed = actual;
        let mut allocations = Vec::new();
        for o in &mut origins {
            check()?;
            if o.end_ordinal > c.ordinal {
                break;
            }
            let amount = if full {
                o.remaining
            } else {
                needed.min(o.remaining).max(0.)
            };
            if amount > 0. {
                o.remaining -= amount;
                o.allocated += amount;
                needed -= amount;
                allocations.push(Allocation {
                    native_interest_index: o.native_interest_index,
                    amount,
                });
            }
            if !full && needed <= 0. {
                break;
            }
        }
        let assigned: f64 = allocations.iter().map(|a| a.amount).sum();
        if !near(assigned, actual) {
            return Err("cash cannot retire current eligible earned origins".into());
        }
        settlements.push(Settlement {
            native_cash_index,
            native_event_id: c.event_id.clone(),
            ordinal: c.ordinal,
            full_native_retirement: full,
            actual_bank_cash_interest: c.cash_interest,
            allocations,
            retained_cash_roundoff_right: assigned - actual,
        });
    }
    let remaining_origin_rights = origins.iter().map(|o| o.remaining).sum();
    let retained_cash_roundoff_right = settlements
        .iter()
        .map(|s| s.retained_cash_roundoff_right)
        .sum();
    Ok(Rights {
        origins,
        settlements,
        remaining_origin_rights,
        retained_cash_roundoff_right,
        total_right: remaining_origin_rights + retained_cash_roundoff_right,
    })
}
pub fn complete_rights(rights: &Rights) -> Result<(), String> {
    if rights.origins.iter().any(|o| !near(o.remaining, 0.))
        || rights
            .settlements
            .iter()
            .any(|s| !near(s.retained_cash_roundoff_right, 0.))
        || !near(rights.total_right, 0.)
    {
        return Err("individual unweighted gross origin tail incomplete".into());
    }
    Ok(())
}
fn group(
    out: &mut Vec<Group>,
    ordinal: i32,
    kind: &str,
    index: usize,
    review: &Identity,
    lines: Vec<Line>,
) -> Result<(), String> {
    for line in &lines {
        money(line.amount)?;
    }
    if lines.iter().map(|l| l.amount).sum::<f64>().abs() > 1e-9 {
        return Err("atomic journal group exceeds unchanged absolute balance gate".into());
    }
    out.push(Group {
        ordinal,
        kind: kind.into(),
        source_income_row: index,
        review: review.clone(),
        lines,
    });
    Ok(())
}
fn line(account: &str, amount: f64) -> Line {
    Line {
        account: account.into(),
        amount,
    }
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    check()?;
    let admission = audit_request(r)?;
    check()?;
    let original_source = deposit_state::simulate(&r.original)?;
    check()?;
    let current_source = deposit_state::simulate(&r.current)?;
    check()?;
    let original_cash = obligations(&original_source)?;
    let current_cash = obligations(&current_source)?;
    for day in r.original.as_of_ordinal..=r.revision_ordinal {
        check()?;
        if !exact(&original_cash.get(&day), &current_cash.get(&day))? {
            return Err("actual cash prefix changed".into());
        }
    }
    let prefix_original: Vec<_> = original_source
        .interest
        .iter()
        .filter(|i| i.end_ordinal <= r.revision_ordinal)
        .collect();
    let prefix_current: Vec<_> = current_source
        .interest
        .iter()
        .filter(|i| i.end_ordinal <= r.revision_ordinal)
        .collect();
    if !exact(&prefix_original, &prefix_current)? {
        return Err("earned prefix changed".into());
    }
    let discount = |cash: &BTreeMap<i32, (f64, f64)>, at: i32| {
        cash.range(at + 1..)
            .map(|(&d, &(p, i))| {
                (p + i)
                    * (-r.anchor.original_annual_effective_rate.ln_1p() * f64::from(d - at) / 365.)
                        .exp()
            })
            .sum::<f64>()
    };
    let original_pv = discount(&original_cash, r.original.as_of_ordinal);
    if !near(original_pv, r.anchor.original_gross_carrying_value) {
        return Err(
            "original supplied cost/yield does not reproduce complete original cash".into(),
        );
    }
    let original_income_request = income_request(r, &original_source, false)?;
    let original_income = dated_income::evaluate(&original_income_request)?;
    check()?;
    let current_income_request = income_request(r, &current_source, true)?;
    let current_income = dated_income::evaluate(&current_income_request)?;
    check()?;
    let index = (r.revision_ordinal - r.original.as_of_ordinal - 1) as usize;
    let old = &original_income.rows[index];
    let revised = &current_income.rows[index];
    let remaining_expected_cash_pv = discount(&current_cash, r.revision_ordinal);
    let before =
        revised.opening_gross_carrying_value + revised.effective_interest - revised.cash_received;
    if !near(before, old.closing_gross_carrying_value)
        || !near(
            revised.closing_gross_carrying_value,
            remaining_expected_cash_pv,
        )
    {
        return Err("purchase-prefix original-EIR continuation failed".into());
    }
    let unretired_earned_origins = rights(&current_source, r.revision_ordinal, &mut check)?;
    if !near(
        unretired_earned_origins.total_right,
        revised.closing_accrued_interest,
    ) {
        return Err("prefix gross origins do not reconcile accrued right".into());
    }
    let native_point = current_source
        .points
        .iter()
        .find(|p| p.ordinal == r.revision_ordinal)
        .ok_or("native revision point missing")?
        .clone();
    let same_date_start_cash_indices: Vec<_> = current_source
        .cashflows
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            c.ordinal == r.revision_ordinal && matches!(c.kind.as_str(), "withdrawal" | "closure")
        })
        .map(|(i, _)| i)
        .collect();
    let mut native_p = native_point.balances[0];
    let mut native_a = native_point.accrued_interest[0];
    for &i in &same_date_start_cash_indices {
        native_p += current_source.cashflows[i].principal;
        native_a += current_source.cashflows[i].cash_interest;
    }
    if !near(native_p, revised.closing_principal)
        || !near(native_a, revised.closing_accrued_interest)
    {
        return Err("native boundary cash/state reconciliation failed".into());
    }
    let lag = r.current.segments[0].repricing.observation_lag_days as i32;
    let revision_opening = RevisionOpening {
        native_point,
        same_date_start_cash_indices,
        next_interval_paid_rate: current_source.points[index + 2].paid_rates[0],
        lag_observations: r
            .current
            .drivers
            .iter()
            .filter(|d| {
                d.ordinal >= r.revision_ordinal - lag - 1 && d.ordinal <= r.revision_ordinal
            })
            .cloned()
            .collect(),
        principal_after_all_date_cash: revised.closing_principal,
        accrued_after_all_date_cash: revised.closing_accrued_interest,
        gross_carrying_before_revision: before,
        book_basis_before_revision: before
            - revised.closing_principal
            - revised.closing_accrued_interest,
        accumulated_original_eir: current_income.rows[..=index]
            .iter()
            .map(|x| x.effective_interest)
            .sum(),
        actual_bank_principal_cash_to_revision: -current_cash
            .range(..=r.revision_ordinal)
            .map(|(_, x)| x.0)
            .sum::<f64>(),
        actual_bank_interest_cash_to_revision: -current_cash
            .range(..=r.revision_ordinal)
            .map(|(_, x)| x.1)
            .sum::<f64>(),
        unretired_earned_origins,
    };
    let original_full_rights = rights(&original_source, admission.full_end_ordinal, &mut check)?;
    let current_full_rights = rights(&current_source, admission.full_end_ordinal, &mut check)?;
    complete_rights(&original_full_rights)?;
    complete_rights(&current_full_rights)?;
    for income in [&original_income, &current_income] {
        if !near(income.remaining_principal, 0.)
            || !near(income.remaining_accrued_interest, 0.)
            || !near(income.remaining_gross_carrying_value, 0.)
        {
            return Err("unweighted original/current income tail incomplete".into());
        }
        for row in &income.rows {
            check()?;
            for v in [
                row.opening_gross_carrying_value,
                row.effective_interest,
                row.contractual_interest_earned,
                row.basis_amortization,
                row.modification_gain_loss,
                row.cash_received,
                row.closing_principal,
                row.closing_accrued_interest,
                row.closing_gross_carrying_value,
                row.closing_book_adjustment,
            ] {
                money(v)?;
            }
        }
    }
    let mut journal_deltas = Vec::with_capacity(2 * current_income.rows.len() + 1);
    for (n, row) in current_income.rows.iter().enumerate() {
        check()?;
        group(
            &mut journal_deltas,
            row.payment_ordinal,
            "original_eir_and_earned",
            n,
            &r.contract,
            vec![
                line("interest_payable", -row.contractual_interest_earned),
                line("deposit_book_basis", -row.basis_amortization),
                line("pnl:deposit_original_eir_expense", row.effective_interest),
            ],
        )?;
        let event = &current_income_request.events[n];
        group(
            &mut journal_deltas,
            row.payment_ordinal,
            "actual_cash_settlement",
            n,
            &r.contract,
            vec![
                line("deposit_principal", event.principal_cash),
                line("interest_payable", event.interest_cash),
                line("cash", -row.cash_received),
            ],
        )?;
        if row.yield_review.is_some() {
            group(
                &mut journal_deltas,
                row.payment_ordinal,
                "reviewed_expectation_revision",
                n,
                &r.policy.review,
                vec![
                    line("deposit_book_basis", -row.modification_gain_loss),
                    line("pnl:deposit_basis_revision", row.modification_gain_loss),
                ],
            )?;
        }
    }
    let basis_revision = revised.modification_gain_loss;
    money(remaining_expected_cash_pv)?;
    money(before)?;
    money(basis_revision)?;
    let result = ResultData {
        schema: SCHEMA.into(),
        raw_request: r.clone(),
        admission,
        original_source,
        current_source,
        original_pv,
        original_income_request,
        original_income,
        current_income_request,
        current_income,
        revision_opening,
        remaining_expected_cash_pv,
        basis_revision,
        original_full_rights,
        current_full_rights,
        journal_deltas,
        accounting_standard_approval: false,
        empirical_validation: false,
        installed_adoption: false,
        saved_book_adoption: false,
        cohort_adoption: false,
        capital_ftp_eligible: false,
    };
    check()?;
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > result.admission.reserved_bytes
    {
        return Err("actual continuation retention exceeds reservation".into());
    }
    check()?;
    Ok(result)
}
