//! Prospective reviewed pending accounting; no mixed/public accounting adoption.
use crate::{
    credit_accounting::Identity, credit_operator, mortgage_coupled_carry as coupled_carry,
    mortgage_sequential_carry as source,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub const SCHEMA: &str = "mortgage-pending-accounting-1";
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_DAYS: usize = 64;
const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
/// Tighter artifact-only scope for fixed absolute atomic-balance accuracy.
/// The underlying sequential/native primitives retain their 1e15 domain.
pub const MAX_COMPONENT_AMOUNT: f64 = 10_000.;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub checkpoint: String,
    pub pending: String,
    pub income: String,
    pub boundary: String,
    pub unsupported: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub source: source::Request,
    pub policy: Policy,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub reserved_end_ordinal: i32,
    pub days: usize,
    pub work: usize,
    pub reserved_bytes: usize,
    pub max_groups: usize,
    pub interval_count: usize,
}
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Channel {
    pub principal: f64,
    pub recognized_interest: f64,
    pub basis: f64,
}
impl Channel {
    pub fn carrying(&self) -> f64 {
        self.principal + self.recognized_interest + self.basis
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Claim {
    pub claim_id: String,
    pub origin: usize,
    pub birth_ordinal: i32,
    pub book: Channel,
    pub economic_principal: f64,
    pub economic_contract_interest: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct Book {
    pub ordinal: i32,
    /// Slots0..2 active; 3..5 pending-default origin; 6 pending-prepaid.
    pub slots: [Channel; 7],
    /// Exact separate gross loan operator checkpoints, never inferred from A.
    pub borrower_principal: [f64; 7],
    pub borrower_contract_interest: [f64; 7],
    pub claims: Vec<Claim>,
    pub cash: f64,
    pub original_eir_income: f64,
    pub reviewed_income: f64,
}
impl Book {
    pub fn carrying(&self) -> f64 {
        self.slots.iter().map(Channel::carrying).sum::<f64>()
            + self.claims.iter().map(|c| c.book.carrying()).sum::<f64>()
    }
    pub fn earnings(&self) -> f64 {
        self.original_eir_income + self.reviewed_income
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    Principal,
    RecognizedInterest,
    Basis,
    Cash,
    OriginalEir,
    ReviewedIncome,
}
#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub account: Account,
    pub slot: Option<usize>,
    pub claim_id: Option<String>,
    /// Debit sign. Income credit is negative.
    pub amount: f64,
    pub source_action_index: Option<usize>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub ordinal: i32,
    pub kind: String,
    pub source_id: String,
    pub review: Identity,
    pub source_action_indices: Vec<usize>,
    pub lines: Vec<Line>,
    pub committed: Book,
}
#[derive(Debug, Clone, Serialize)]
pub struct Checkpoint {
    pub ordinal: i32,
    pub interval_index: usize,
    pub duration_years: f64,
    pub generator: [[f64; 7]; 7],
    pub recognized_contractual_rates: [f64; 3],
    pub exact_investor: coupled_carry::State,
    pub exact_borrower: credit_operator::CreditOperatorState,
    pub new_original_eir: f64,
    pub method: String,
    pub one_day_duration_years: f64,
}
#[derive(Debug, Serialize)]
pub struct ExperimentWork {
    pub source_work_bound: usize,
    pub cached_compile_work_bound: usize,
    pub daily_apply_work_bound: usize,
    pub group_work_bound: usize,
    pub coupled_compilations: usize,
    pub loan_compilations: usize,
    pub coupled_matrix_multiplications: usize,
    pub coupled_applications: usize,
    pub loan_applications: usize,
    pub retained_full_boundary_controls: usize,
    pub actual_groups: usize,
    pub final_output_byte_cap: usize,
    pub source_and_checkpoint_under_8m: bool,
    pub joined_ledger_admission_verified: bool,
}
struct IntervalCache {
    coupled: coupled_carry::Operator,
    loan: credit_operator::CreditOperator,
    investor: coupled_carry::State,
    borrower: credit_operator::CreditOperatorState,
    cumulative_reward: f64,
}
fn group_bound(r: &Request, intervals: usize, days: usize) -> Result<usize, String> {
    let recovery = r
        .source
        .contract
        .complete_recovery_tails
        .iter()
        .try_fold(0, |n, x| add(n, x.len()))?;
    let interest = r
        .source
        .policy
        .interest
        .complete_collection_tails
        .iter()
        .try_fold(0, |n, x| add(n, x.len()))?;
    // Per actual interval: <=9 default disposition actions, <=6 scheduled
    // cash/basis actions, <=2 prepay actions, recovery/interest tails and <=6
    // economic waiver groups. This deliberately counts actions before compounding.
    add(
        mul(intervals, add(23, add(recovery, interest)?)?)?,
        add(days, 1)?,
    )
}
fn work_parts(
    source_work: usize,
    intervals: usize,
    days: usize,
    groups: usize,
) -> Result<[usize; 4], String> {
    // Same750k complete compile bound, now per source interval, not per day.
    // <=4096 operations per bounded vector/checkpoint or staged atomic group,
    // including <=48 origin claims, finite/gross/conservation scans and copies.
    Ok([
        source_work,
        mul(intervals, 750_000)?,
        mul(days, 4096)?,
        mul(groups, 4096)?,
    ])
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub policy: Policy,
    pub admission: Admission,
    pub experiment: ExperimentWork,
    pub source: source::ResultData,
    pub checkpoints: Vec<Checkpoint>,
    pub groups: Vec<Group>,
    pub daily: Vec<Book>,
    pub closing: Book,
    pub production_adoption: bool,
    pub mixed_ledger_adoption: bool,
    pub accounting_standard_approval: bool,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "pending admission overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "pending admission overflow".into())
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-9 + 1e-12 * a.abs().max(b.abs()) {
        Ok(())
    } else {
        Err(format!("pending monetary conservation {a} != {b}"))
    }
}
fn amount(a: f64) -> Result<(), String> {
    if a.is_finite() && a.abs() <= MAX_COMPONENT_AMOUNT {
        Ok(())
    } else {
        Err("finite bounded pending amount required".into())
    }
}
pub fn audit(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    check()?;
    let p = &r.policy;
    if r.schema != SCHEMA
        || p.checkpoint != "cached_native_interval_checkpoint-1"
        || p.pending != "pending_exit_assets_until_native_boundary-1"
        || p.income != "original_eir_reward_only-1"
        || p.boundary != "source_boundary_dispositions_only-1"
        || p.unsupported != "reject_added_cost_nonaccrual_allowance-1"
        || [&p.identity.id, &p.identity.revision]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err("explicit reviewed pending-source policy required".into());
    }
    let a = source::admit_with_check(&r.source, check)?;
    for c in [&r.source.contract, &r.source.original_contract] {
        for value in [
            c.original_principal,
            c.opening_contractual_principal,
            c.original_payment,
            c.current_payment,
            c.opening_borrower_principal.iter().sum(),
            c.opening_investor_principal.iter().sum(),
        ] {
            if !value.is_finite() || value.abs() > MAX_COMPONENT_AMOUNT {
                return Err("pending component money admission exceeds10000currencyunits".into());
            }
        }
    }
    let growth = (r
        .source
        .acquisition
        .original_annual_effective_rate
        .ln_1p()
        .max(0.)
        * f64::from(
            r.source
                .contract
                .horizon_days
                .max(r.source.original_contract.horizon_days),
        )
        / 365.)
        .exp();
    if r.source.acquisition.actual_cost * growth > MAX_COMPONENT_AMOUNT {
        return Err("pending component money admission exceeds10000currencyunits".into());
    }
    let start = r.source.contract.as_of_ordinal;
    let lag = r
        .source
        .policy
        .economic_principal_disposal_lag_days
        .max(r.source.policy.interest.disposal_lag_days);
    let end = r
        .source
        .contract
        .as_of_ordinal
        .checked_add(
            i32::try_from(
                r.source
                    .contract
                    .horizon_days
                    .max(r.source.original_contract.horizon_days),
            )
            .map_err(|_| "pending date overflow")?,
        )
        .and_then(|d| d.checked_add(i32::try_from(lag).ok()?))
        .ok_or("pending date overflow")?;
    let days = usize::try_from(end - start).map_err(|_| "pending day domain")?;
    if days == 0 || days > MAX_DAYS {
        return Err("pending complete original/current/action tail exceeds64days".into());
    }
    let max_groups = group_bound(r, a.interval_count, days)?;
    let work = work_parts(a.work_bound, a.interval_count, days, max_groups)?
        .into_iter()
        .try_fold(0, add)?;
    let raw = serde_json::to_vec(r).map_err(|e| e.to_string())?.len();
    if raw > 1024 * 1024 {
        return Err("pending raw byte admission exceeded".into());
    }
    // Joined source retained bound, every group snapshot/line and actual-day
    // checkpoint retained before projection. Actual escaped raw bounds identities.
    let per = add(raw, add(mul(a.interval_count, 3 * 512)?, 8192)?)?;
    let reserved_bytes = add(
        add(a.retained_byte_bound, mul(add(max_groups, days)?, per)?)?,
        add(mul(a.interval_count, 16384)?, 32768)?,
    )?;
    if work > 60_000_000 || reserved_bytes > MAX_BYTES || reserved_bytes > MAX_OUTPUT_BYTES {
        return Err("pending joined work/retained byte admission exceeded".into());
    }
    check()?;
    Ok(Admission {
        reserved_end_ordinal: end,
        days,
        work,
        reserved_bytes,
        max_groups,
        interval_count: a.interval_count,
    })
}
fn line(account: Account, slot: Option<usize>, claim: Option<&str>, value: f64) -> Line {
    Line {
        account,
        slot,
        claim_id: claim.map(str::to_string),
        amount: value,
        source_action_index: None,
    }
}
fn channel(lines: &mut Vec<Line>, slot: Option<usize>, claim: Option<&str>, values: [f64; 3]) {
    for (account, amount) in [
        Account::Principal,
        Account::RecognizedInterest,
        Account::Basis,
    ]
    .into_iter()
    .zip(values)
    {
        lines.push(line(account, slot, claim, amount));
    }
}
fn post(book: &mut Book, l: &Line) -> Result<(), String> {
    amount(l.amount)?;
    let ch = match l.account {
        Account::Principal | Account::RecognizedInterest | Account::Basis => {
            Some(if let Some(s) = l.slot {
                book.slots.get_mut(s).ok_or("pending slot index")?
            } else {
                &mut book
                    .claims
                    .iter_mut()
                    .find(|c| Some(c.claim_id.as_str()) == l.claim_id.as_deref())
                    .ok_or("pending claim identity")?
                    .book
            })
        }
        _ => None,
    };
    match l.account {
        Account::Principal => ch.unwrap().principal += l.amount,
        Account::RecognizedInterest => ch.unwrap().recognized_interest += l.amount,
        Account::Basis => ch.unwrap().basis += l.amount,
        Account::Cash => book.cash += l.amount,
        Account::OriginalEir => book.original_eir_income -= l.amount,
        Account::ReviewedIncome => book.reviewed_income -= l.amount,
    }
    Ok(())
}
fn validate_book(book: &Book) -> Result<(), String> {
    for value in [
        book.cash,
        book.original_eir_income,
        book.reviewed_income,
        book.carrying(),
        book.earnings(),
    ] {
        amount(value)?;
    }
    for value in book
        .borrower_principal
        .iter()
        .chain(&book.borrower_contract_interest)
    {
        amount(*value)?;
    }
    for v in book.slots.iter().chain(book.claims.iter().map(|c| &c.book)) {
        for a in [v.principal, v.recognized_interest, v.basis, v.carrying()] {
            amount(a)?;
        }
        if v.principal < -1e-9 || v.recognized_interest < -1e-9 || v.carrying() < -1e-9 {
            return Err("negative pending gross book right".into());
        }
    }
    for c in &book.claims {
        for a in [c.economic_principal, c.economic_contract_interest] {
            amount(a)?;
            if a < -1e-9 {
                return Err("negative pending economic claim".into());
            }
        }
    }
    near(book.carrying() + book.cash, book.earnings())
}
fn commit(
    book: &mut Book,
    mut next: Book,
    groups: &mut Vec<Group>,
    at: (&str, &str, &Identity),
    lines: Vec<Line>,
    admission: &Admission,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    check()?;
    near(lines.iter().map(|l| l.amount).sum(), 0.)?;
    for l in &lines {
        post(&mut next, l)?;
    }
    validate_book(&next)?;
    if groups.len() >= admission.max_groups {
        return Err("pending group reservation exceeded".into());
    }
    let mut source_action_indices = Vec::new();
    for index in lines.iter().filter_map(|l| l.source_action_index) {
        if source_action_indices.last() != Some(&index) {
            source_action_indices.push(index);
        }
    }
    groups.push(Group {
        ordinal: next.ordinal,
        kind: at.0.into(),
        source_id: at.1.into(),
        review: at.2.clone(),
        source_action_indices,
        lines,
        committed: next.clone(),
    });
    *book = next;
    Ok(())
}
pub fn evaluate(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let admission = audit(r, check)?;
    let mut groups = Vec::new();
    let mut daily = Vec::new();
    let mut checkpoints = Vec::new();
    groups
        .try_reserve_exact(admission.max_groups)
        .map_err(|e| e.to_string())?;
    daily
        .try_reserve_exact(add(admission.days, 1)?)
        .map_err(|e| e.to_string())?;
    checkpoints
        .try_reserve_exact(admission.days)
        .map_err(|e| e.to_string())?;
    // Reserve cache arena before either heavy native source generation.
    let source_bound = source::admit_with_check(&r.source, check)?;
    let mut caches = Vec::new();
    caches
        .try_reserve_exact(source_bound.interval_count)
        .map_err(|e| e.to_string())?;
    let source = source::evaluate_with_check(&r.source, check)?;
    if source.full_end_ordinal > admission.reserved_end_ordinal
        || source.original_full_end_ordinal > admission.reserved_end_ordinal
    {
        return Err("pending source exceeds fulltail reservation".into());
    }

    let mut matrix_multiplications = 0usize;
    for (interval, receipt) in source.intervals.iter().zip(&source.source.intervals) {
        check()?;
        let mut request = interval.native_carry_request.clone();
        request.duration_years = 1. / 365.;
        let coupled = coupled_carry::compile(&request)?;
        check()?;
        // Metadata-only count from the immutable serialized operator; no caller
        // coefficients and no financial computation through JSON.
        let value = serde_json::to_value(&coupled).map_err(|e| e.to_string())?;
        matrix_multiplications = add(
            matrix_multiplications,
            usize::try_from(
                value["work_multiplications"]
                    .as_u64()
                    .ok_or("compiled work metadata")?,
            )
            .map_err(|_| "compiled work overflow")?,
        )?;
        let mut loan_request = receipt.loan_operator_request.clone();
        loan_request.duration_years = 1. / 365.;
        let loan = credit_operator::compile(&loan_request)?;
        check()?;
        let mut borrower = credit_operator::CreditOperatorState {
            schema: credit_operator::STATE_SCHEMA.into(),
            measure: loan_request.measure,
            monetary_unit: r.source.contract.monetary_unit.clone(),
            probability: receipt.opening_probability,
            principal: [0.; 7],
            undrawn_commitment: [0.; 7],
            accrued_interest: [0.; 7],
            book_adjustment: [0.; 7],
        };
        borrower.principal[..3].copy_from_slice(&receipt.opening_borrower_principal);
        borrower.accrued_interest[..3].copy_from_slice(&receipt.opening_borrower_interest);
        caches.push(IntervalCache {
            coupled,
            loan,
            investor: interval.opening.clone(),
            borrower,
            cumulative_reward: 0.,
        });
    }
    let mut applications = 0usize;
    let mut controls = 0usize;
    let start = r.source.contract.as_of_ordinal;
    let mut book = Book {
        ordinal: start,
        slots: [Channel::default(); 7],
        borrower_principal: [0.; 7],
        borrower_contract_interest: [0.; 7],
        claims: Vec::new(),
        cash: 0.,
        original_eir_income: 0.,
        reviewed_income: 0.,
    };
    let mut next = book.clone();
    next.borrower_principal[..3].copy_from_slice(&r.source.contract.opening_borrower_principal);
    let mut lines = vec![line(
        Account::Cash,
        None,
        None,
        -source.actual_acquisition_cost,
    )];
    for i in 0..3 {
        channel(
            &mut lines,
            Some(i),
            None,
            [
                r.source.contract.opening_investor_principal[i],
                0.,
                r.source.acquisition.opening_gross_carrying[i]
                    - r.source.contract.opening_investor_principal[i],
            ],
        );
    }
    commit(
        &mut book,
        next,
        &mut groups,
        (
            "acquisition",
            &r.source.acquisition.identity.id,
            &r.source.acquisition.identity,
        ),
        lines,
        &admission,
        check,
    )?;
    daily.push(book.clone());
    let links: BTreeMap<_, _> = source
        .source
        .capture
        .cash_links
        .iter()
        .map(|x| (x.entitlement_id.as_str(), x.cash_event_id.as_str()))
        .collect();
    let mut actions: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, a) in source.actions.iter().enumerate() {
        check()?;
        actions.entry(a.ordinal).or_default().push(i);
    }
    let mut last_reward = 0.;
    let mut last_interval = None;
    let mut consumed = BTreeSet::new();
    for ordinal in start + 1..=source.full_end_ordinal {
        check()?;
        book.ordinal = ordinal;
        if let Some((i, receipt)) = source
            .source
            .intervals
            .iter()
            .enumerate()
            .find(|(_, t)| t.start_ordinal < ordinal && ordinal <= t.end_ordinal)
        {
            if last_interval != Some(i) {
                last_reward = 0.;
                last_interval = Some(i);
            }
            let interval = &source.intervals[i];
            let mut request = interval.native_carry_request.clone();
            request.duration_years = f64::from(ordinal - receipt.start_ordinal) / 365.;
            let cache = &mut caches[i];
            check()?;
            let mut transported = cache.coupled.apply(&cache.investor)?;
            check()?;
            let mut loan = cache.loan.apply(&cache.borrower)?;
            applications = add(applications, 1)?;
            cache.cumulative_reward += transported.total_interval_earned_eir;
            transported.total_interval_earned_eir = cache.cumulative_reward;
            cache.investor = transported.closing.clone();
            cache.borrower = loan.clone();
            let method = if ordinal == receipt.end_ordinal {
                check()?;
                let boundary = &source.source.capture.boundaries[i];
                // Recurrent endpoint must pass unchanged controls before selecting
                // the already-produced full native endpoint as this checkpoint.
                for slot in 0..7 {
                    near(
                        transported.closing.principal[slot],
                        boundary.transported_investor_principal[slot],
                    )?;
                    near(
                        transported.closing.accrued_interest[slot],
                        boundary.transported_investor_interest[slot],
                    )?;
                    near(
                        transported.closing.gross_carrying[slot],
                        interval.transport.closing.gross_carrying[slot],
                    )?;
                    near(
                        transported.closing.earned_eir[slot],
                        interval.transport.closing.earned_eir[slot],
                    )?;
                    near(
                        loan.principal[slot],
                        boundary.transported_loan_principal[slot],
                    )?;
                    near(
                        loan.accrued_interest[slot],
                        boundary.transported_loan_interest[slot],
                    )?;
                    near(
                        loan.probability[slot],
                        interval.transport.closing.probability[slot],
                    )?;
                }
                near(
                    transported.total_interval_earned_eir,
                    interval.transport.total_interval_earned_eir,
                )?;
                transported = interval.transport.clone();
                loan.probability = transported.closing.probability;
                loan.principal = boundary.transported_loan_principal;
                loan.accrued_interest = boundary.transported_loan_interest;
                controls = add(controls, 1)?;
                "retained_exact_source_boundary"
            } else {
                "cached_one_day_recurrence"
            };
            let new_eir = transported.total_interval_earned_eir - last_reward;
            last_reward = transported.total_interval_earned_eir;
            let mut next = book.clone();
            next.borrower_principal = loan.principal;
            next.borrower_contract_interest = loan.accrued_interest;
            let mut lines = Vec::with_capacity(22);
            for slot in 0..7 {
                let p = transported.closing.principal[slot];
                let a = transported.closing.accrued_interest[slot];
                let b = transported.closing.gross_carrying[slot] - p - a;
                let old = book.slots[slot];
                channel(
                    &mut lines,
                    Some(slot),
                    None,
                    [
                        p - old.principal,
                        a - old.recognized_interest,
                        b - old.basis,
                    ],
                );
            }
            lines.push(line(Account::OriginalEir, None, None, -new_eir));
            commit(
                &mut book,
                next,
                &mut groups,
                (
                    "exact_checkpoint",
                    &format!("interval:{i}:day:{ordinal}"),
                    &r.source.acquisition.income_review,
                ),
                lines,
                &admission,
                check,
            )?;
            checkpoints.push(Checkpoint {
                ordinal,
                interval_index: i,
                duration_years: request.duration_years,
                generator: request.generator,
                recognized_contractual_rates: request.recognized_contractual_rates,
                exact_investor: transported.closing,
                exact_borrower: loan,
                new_original_eir: new_eir,
                method: method.into(),
                one_day_duration_years: 1. / 365.,
            });
        }
        for &ai in actions.get(&ordinal).into_iter().flatten() {
            check()?;
            if consumed.contains(&ai) {
                continue;
            }
            let action = &source.actions[ai];
            let boundary = &source.source.capture.boundaries[action.interval_index];
            let mut lines = Vec::new();
            let mut next = book.clone();
            let mut compound = false;
            let mut group_kind = action.kind.as_str();
            let mut group_review = &action.review;
            match action.kind.as_str() {
                "default_accrued_reversal" => {
                    let close = source
                        .default_closeouts
                        .iter()
                        .find(|c| c.claim_id == action.source_id)
                        .ok_or("default closeout source")?;
                    if !book.claims.iter().any(|c| c.claim_id == close.claim_id) {
                        let slot = 3 + close.origin;
                        let values = [
                            close.gross_principal,
                            close.recognized_interest,
                            close.signed_default_basis,
                        ];
                        next.claims.push(Claim {
                            claim_id: close.claim_id.clone(),
                            origin: close.origin,
                            birth_ordinal: ordinal,
                            book: Channel::default(),
                            economic_principal: close.gross_principal,
                            economic_contract_interest: close.gross_contract_interest,
                        });
                        channel(&mut lines, Some(slot), None, values.map(|v| -v));
                        channel(&mut lines, None, Some(&close.claim_id), values);
                    } else {
                        return Err("duplicate native default disposition group".into());
                    }
                    for (offset, kind, account) in [
                        (0, "default_accrued_reversal", Account::RecognizedInterest),
                        (1, "default_signed_basis_disposal", Account::Basis),
                        (
                            2,
                            "default_native_principal_loss_book_chargeoff",
                            Account::Principal,
                        ),
                    ] {
                        let index = ai
                            .checked_add(offset)
                            .ok_or("default action index overflow")?;
                        let part = source
                            .actions
                            .get(index)
                            .ok_or("incomplete default atomic group")?;
                        if part.kind != kind
                            || part.source_id != action.source_id
                            || part.ordinal != ordinal
                            || part.interval_index != action.interval_index
                            || part.origin != action.origin
                        {
                            return Err("default atomic source action order mismatch".into());
                        }
                        let mut debit = line(
                            account,
                            None,
                            Some(&close.claim_id),
                            -part.gross_carry_disposed,
                        );
                        let mut income = line(Account::ReviewedIncome, None, None, -part.income);
                        debit.source_action_index = Some(index);
                        income.source_action_index = Some(index);
                        lines.push(debit);
                        lines.push(income);
                        consumed.insert(index);
                    }
                    compound = true;
                    group_kind = "native_default_claim_and_disposition";
                    group_review = &r.source.policy.identity;
                }
                "native_default_principal_cash" => {
                    let close = source
                        .default_closeouts
                        .iter()
                        .find(|c| c.native_principal_cash_ids.contains(&action.source_id))
                        .ok_or("native recovery origin source")?;
                    next.claims
                        .iter_mut()
                        .find(|c| c.claim_id == close.claim_id)
                        .ok_or("unborn recovery claim")?
                        .economic_principal -= action.actual_cash;
                    lines.push(line(Account::Cash, None, None, action.actual_cash));
                    lines.push(line(
                        Account::Principal,
                        None,
                        Some(&close.claim_id),
                        -action.actual_cash,
                    ));
                }
                "reviewed_default_interest_cash_income" => {
                    let cash = source
                        .reviewed_interest_cash
                        .iter()
                        .find(|c| c.cash_id == action.source_id)
                        .ok_or("reviewed interest source")?;
                    next.claims
                        .iter_mut()
                        .find(|c| c.claim_id == cash.claim_id)
                        .ok_or("unborn interest claim")?
                        .economic_contract_interest -= cash.amount;
                    lines.push(line(Account::Cash, None, None, action.actual_cash));
                    lines.push(line(Account::ReviewedIncome, None, None, -action.income));
                }
                "scheduled_native_cash" => {
                    let item = boundary
                        .scheduled
                        .iter()
                        .find(|x| {
                            x.entitlement_id.as_ref().is_some_and(|id| {
                                links.get(id.as_str()) == Some(&action.source_id.as_str())
                            })
                        })
                        .ok_or("scheduled native cash source")?;
                    channel(
                        &mut lines,
                        Some(item.origin),
                        None,
                        [
                            -item.principal,
                            -item.recognized_interest,
                            item.recognized_interest - item.interest,
                        ],
                    );
                    lines.push(line(Account::Cash, None, None, action.actual_cash));
                    if let Some(exit) = source.actions.get(ai + 1).filter(|a| {
                        a.kind == "scheduled_full_exit_signed_basis"
                            && a.origin == action.origin
                            && a.ordinal == ordinal
                            && a.interval_index == action.interval_index
                    }) {
                        for l in &mut lines {
                            l.source_action_index = Some(ai);
                        }
                        let mut basis = line(
                            Account::Basis,
                            exit.origin,
                            None,
                            -exit.gross_carry_disposed,
                        );
                        let mut income = line(Account::ReviewedIncome, None, None, -exit.income);
                        basis.source_action_index = Some(ai + 1);
                        income.source_action_index = Some(ai + 1);
                        lines.push(basis);
                        lines.push(income);
                        consumed.insert(ai + 1);
                        compound = true;
                        group_kind = "scheduled_full_exit_cash_and_basis";
                    }
                }
                "scheduled_full_exit_signed_basis" => {
                    lines.push(line(
                        Account::Basis,
                        action.origin,
                        None,
                        -action.gross_carry_disposed,
                    ));
                    lines.push(line(Account::ReviewedIncome, None, None, -action.income));
                }
                kind if kind.ends_with("_exact_exit") => {
                    let item = boundary
                        .prepayments
                        .iter()
                        .find(|x| {
                            x.entitlement_id.as_ref().is_some_and(|id| {
                                links.get(id.as_str()) == Some(&action.source_id.as_str())
                            })
                        })
                        .ok_or("prepaid native cash source")?;
                    channel(
                        &mut lines,
                        Some(6),
                        None,
                        [
                            -item.principal,
                            -item.recognized_interest,
                            -(action.gross_carry_disposed
                                - item.principal
                                - item.recognized_interest),
                        ],
                    );
                    lines.push(line(Account::Cash, None, None, action.actual_cash));
                    lines.push(line(Account::ReviewedIncome, None, None, -action.income));
                }
                _ => return Err("unsupported pending source action".into()),
            }
            if !compound {
                for l in &mut lines {
                    l.source_action_index = Some(ai);
                }
            }
            commit(
                &mut book,
                next,
                &mut groups,
                (group_kind, &action.source_id, group_review),
                lines,
                &admission,
                check,
            )?;
        }
        // Economic disposals follow all actual cash on their reviewed date. They
        // never create accounting expense, synthetic cash or claim normalization.
        for close in &source.default_closeouts {
            for (day, principal, value, review) in [
                (
                    close.principal_disposal_ordinal,
                    true,
                    close.principal_economic_disposal,
                    &close.principal_disposal_review,
                ),
                (
                    close.interest_disposal_ordinal,
                    false,
                    close.interest_economic_disposal,
                    &close.interest_disposal_review,
                ),
            ] {
                if day == ordinal {
                    let mut next = book.clone();
                    let claim = next
                        .claims
                        .iter_mut()
                        .find(|c| c.claim_id == close.claim_id)
                        .ok_or("unborn economic disposal")?;
                    if principal {
                        claim.economic_principal -= value;
                    } else {
                        claim.economic_contract_interest -= value;
                    }
                    commit(
                        &mut book,
                        next,
                        &mut groups,
                        (
                            if principal {
                                "economic_principal_waiver"
                            } else {
                                "economic_interest_waiver"
                            },
                            &close.claim_id,
                            review,
                        ),
                        Vec::new(),
                        &admission,
                        check,
                    )?;
                }
            }
        }
        if let Some(boundary) = source
            .source
            .capture
            .boundaries
            .iter()
            .find(|b| b.ordinal == ordinal)
        {
            book.borrower_principal = [0.; 7];
            book.borrower_contract_interest = [0.; 7];
            let point = source
                .source
                .native_projection
                .points
                .iter()
                .find(|p| p.ordinal == ordinal)
                .ok_or("native boundary point absent")?;
            book.borrower_principal[..3].copy_from_slice(&point.borrower_principal);
            book.borrower_contract_interest[..3].copy_from_slice(&point.borrower_interest);
            for i in 0..3 {
                near(
                    book.slots[i].principal,
                    boundary.closing_investor_principal[i],
                )?;
                near(
                    book.slots[i].recognized_interest,
                    boundary.closing_investor_interest[i],
                )?;
                near(
                    book.slots[i].carrying(),
                    source.intervals[boundary.interval_index].closing_live_gross_carrying[i],
                )?;
            }
            for s in &book.slots[3..] {
                near(s.principal, 0.)?;
                near(s.recognized_interest, 0.)?;
                near(s.carrying(), 0.)?;
            }
        }
        validate_book(&book)?;
        daily.push(book.clone());
    }
    for s in book.slots.iter().chain(book.claims.iter().map(|c| &c.book)) {
        near(s.principal, 0.)?;
        near(s.recognized_interest, 0.)?;
        near(s.basis, 0.)?;
    }
    for c in &book.claims {
        near(c.economic_principal, 0.)?;
        near(c.economic_contract_interest, 0.)?;
    }
    near(book.original_eir_income, source.total_earned_eir)?;
    near(book.earnings(), source.total_accounting_income)?;
    near(
        book.cash + source.actual_acquisition_cost,
        source.total_actual_cash,
    )?;
    let parts = work_parts(
        source.admission.work_bound,
        source.admission.interval_count,
        admission.days,
        admission.max_groups,
    )?;
    let experiment = ExperimentWork {
        source_work_bound: parts[0],
        cached_compile_work_bound: parts[1],
        daily_apply_work_bound: parts[2],
        group_work_bound: parts[3],
        coupled_compilations: caches.len(),
        loan_compilations: caches.len(),
        coupled_matrix_multiplications: matrix_multiplications,
        coupled_applications: applications,
        loan_applications: applications,
        retained_full_boundary_controls: controls,
        actual_groups: groups.len(),
        final_output_byte_cap: MAX_OUTPUT_BYTES,
        source_and_checkpoint_under_8m: admission.work <= 8_000_000,
        joined_ledger_admission_verified: false,
    };
    let result = ResultData {
        schema: SCHEMA,
        policy: r.policy.clone(),
        admission,
        experiment,
        source,
        checkpoints,
        groups,
        daily,
        closing: book,
        production_adoption: false,
        mixed_ledger_adoption: false,
        accounting_standard_approval: false,
    };
    check()?;
    struct Counter<'a> {
        bytes: usize,
        limit: usize,
        check: &'a mut dyn FnMut() -> Result<(), String>,
    }
    impl Write for Counter<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            (self.check)().map_err(std::io::Error::other)?;
            self.bytes = self
                .bytes
                .checked_add(b.len())
                .filter(|&n| n <= self.limit)
                .ok_or_else(|| {
                    std::io::Error::other(
                        "pending actual retained serialization exceeded reservation",
                    )
                })?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Count escaped final output without a second complete financial JSON buffer.
    serde_json::to_writer(
        Counter {
            bytes: 0,
            limit: result.admission.reserved_bytes.min(MAX_OUTPUT_BYTES),
            check,
        },
        &result,
    )
    .map_err(|e| e.to_string())?;
    check()?;
    Ok(result)
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    fn empty() -> Book {
        Book {
            ordinal: 1,
            slots: [Channel::default(); 7],
            borrower_principal: [0.; 7],
            borrower_contract_interest: [0.; 7],
            claims: Vec::new(),
            cash: 0.,
            original_eir_income: 0.,
            reviewed_income: 0.,
        }
    }
    #[test]
    fn balanced_negative_right_rejects_without_changing_book_or_groups() {
        let mut book = empty();
        let prior = serde_json::to_value(&book).unwrap();
        let mut groups = Vec::new();
        let a = Admission {
            reserved_end_ordinal: 2,
            days: 1,
            work: 1,
            reserved_bytes: 8192,
            max_groups: 2,
            interval_count: 1,
        };
        let review = Identity {
            id: "review".into(),
            revision: "1".into(),
        };
        let next = book.clone();
        let lines = vec![
            line(Account::Principal, Some(0), None, -1.),
            line(Account::Cash, None, None, 1.),
        ];
        assert!(commit(
            &mut book,
            next,
            &mut groups,
            ("bad", "bad", &review),
            lines,
            &a,
            &mut || Ok(())
        )
        .is_err());
        assert_eq!(serde_json::to_value(book).unwrap(), prior);
        assert!(groups.is_empty());
    }
    #[test]
    fn cancel_rejects_before_committing_even_balanced_memo_group() {
        let mut book = empty();
        let prior = serde_json::to_value(&book).unwrap();
        let mut groups = Vec::new();
        let a = Admission {
            reserved_end_ordinal: 2,
            days: 1,
            work: 1,
            reserved_bytes: 8192,
            max_groups: 2,
            interval_count: 1,
        };
        let review = Identity {
            id: "review".into(),
            revision: "1".into(),
        };
        let next = book.clone();
        assert!(commit(
            &mut book,
            next,
            &mut groups,
            ("cancel", "cancel", &review),
            Vec::new(),
            &a,
            &mut || Err("cancelled".into())
        )
        .is_err());
        assert_eq!(serde_json::to_value(book).unwrap(), prior);
        assert!(groups.is_empty());
    }
}
