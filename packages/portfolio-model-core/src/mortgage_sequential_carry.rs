//! Reviewed acquired whole-loan source carry. No daily journal or mixed runtime adoption.
use crate::{
    cashflow::{validate_ordinal, CashflowMeasure},
    credit::CreditMeasure,
    credit_accounting::{AccruedPolicy, Identity},
    mortgage_state::{self, MortgageOwnership, MortgageRequest},
};
use crate::{
    mortgage_coupled_carry as coupled_carry,
    mortgage_default_interest::{DefaultInterestOwnership, RemainderOwnership},
    mortgage_sequential_source as boundary_receipt,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

#[path = "mortgage_sequential_carry/admission.rs"]
mod admission;
#[path = "mortgage_sequential_carry/original.rs"]
mod original;
#[path = "mortgage_sequential_carry/strict_request.rs"]
mod strict_request;
pub const SCHEMA: &str = "mortgage-credit-accounting-1";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_INTERVALS: usize = 16;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub identity: Identity,
    pub original_yield_anchor: Identity,
    pub income_review: Identity,
    pub purchase_ordinal: i32,
    pub actual_cost: f64,
    pub original_annual_effective_rate: f64,
    pub opening_gross_carrying: [f64; 3],
    pub accounting_basis: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub identity: Identity,
    pub original_income: String,
    pub remittance_float: String,
    pub live_cash: String,
    pub exit_basis: String,
    pub default_basis: String,
    pub default_principal_book: String,
    pub default_principal_book_review: Identity,
    pub economic_principal_disposal_review: Identity,
    pub economic_principal_disposal_lag_days: u32,
    #[serde(deserialize_with = "strict_request::interest")]
    pub interest: DefaultInterestOwnership,
    pub tail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    #[serde(deserialize_with = "strict_request::current")]
    pub contract: MortgageRequest,
    #[serde(deserialize_with = "strict_request::original")]
    pub original_contract: MortgageRequest,
    pub original_economic_source: Identity,
    pub pairing: Pairing,
    pub path_id: String,
    pub economic_source: Identity,
    pub acquisition: Acquisition,
    pub policy: Policy,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    pub review: Identity,
    pub original_run_id: String,
    pub current_run_id: String,
    pub policy: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Admission {
    pub interval_count: usize,
    pub original_interval_count: usize,
    pub retained_row_bound: usize,
    pub retained_byte_bound: usize,
    pub work_bound: usize,
    pub generator_code_hash_bytes: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub ordinal: i32,
    pub interval_index: usize,
    pub origin: Option<usize>,
    pub kind: String,
    pub source_id: String,
    pub review: Identity,
    /// Signed accounting income, never an economic claim disposal or extra cash.
    pub income: f64,
    pub gross_carry_disposed: f64,
    pub actual_cash: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct InterestCash {
    pub cash_id: String,
    pub claim_id: String,
    pub ordinal: i32,
    pub amount: f64,
    pub review: Identity,
}
#[derive(Debug, Clone, Serialize)]
pub struct Closeout {
    pub claim_id: String,
    pub origin: usize,
    pub default_ordinal: i32,
    pub gross_principal: f64,
    pub gross_contract_interest: f64,
    pub recognized_interest: f64,
    pub exact_default_carry: f64,
    pub signed_default_basis: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub principal_economic_disposal: f64,
    pub principal_disposal_ordinal: i32,
    pub principal_disposal_review: Identity,
    pub interest_economic_disposal: f64,
    pub interest_disposal_ordinal: i32,
    pub interest_disposal_review: Identity,
    pub native_principal_cash_ids: Vec<String>,
    pub reviewed_interest_cash_ids: Vec<String>,
    pub closing_economic_principal: f64,
    pub closing_economic_interest: f64,
    pub closing_book_principal: f64,
    pub closing_book_accrued: f64,
    pub closing_book_basis: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct EconomicCloseout {
    pub claim_id: String,
    pub origin: usize,
    pub gross_principal: f64,
    pub gross_contract_interest: f64,
    pub principal_cash: f64,
    pub interest_cash: f64,
    pub principal_economic_disposal: f64,
    pub principal_disposal_ordinal: i32,
    pub principal_disposal_review: Identity,
    pub interest_economic_disposal: f64,
    pub interest_disposal_ordinal: i32,
    pub interest_disposal_review: Identity,
    pub native_principal_cash_ids: Vec<String>,
    pub reviewed_interest_cash_ids: Vec<String>,
    pub closing_economic_principal: f64,
    pub closing_economic_interest: f64,
}
#[derive(Debug, Serialize)]
pub struct Interval {
    pub interval_index: usize,
    pub opening: coupled_carry::State,
    pub native_carry_request: coupled_carry::Request,
    pub operator: coupled_carry::Operator,
    pub transport: coupled_carry::ResultData,
    pub closing_live_gross_carrying: [f64; 3],
    pub closing_live_basis: [f64; 3],
    pub closing_live_earned_eir: [f64; 3],
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub admission: Admission,
    pub source: boundary_receipt::ResultData,
    pub original_source: boundary_receipt::ResultData,
    pub original_source_raw_hash: String,
    pub current_source_raw_hash: String,
    pub original_source_receipt_hash: String,
    pub current_source_receipt_hash: String,
    pub intervals: Vec<Interval>,
    pub actions: Vec<Action>,
    pub reviewed_interest_cash: Vec<InterestCash>,
    pub default_closeouts: Vec<Closeout>,
    pub original_reviewed_interest_cash: Vec<InterestCash>,
    pub original_economic_closeouts: Vec<EconomicCloseout>,
    pub original_full_end_ordinal: i32,
    pub original_quote_pv: f64,
    pub actual_acquisition_cost: f64,
    pub total_earned_eir: f64,
    pub total_accounting_income: f64,
    pub total_actual_cash: f64,
    pub cumulative_earned_eir_conservation_error: f64,
    pub quote_basis: &'static str,
    pub full_end_ordinal: i32,
    pub closing_live_gross_carrying: [f64; 3],
    pub closing_reconciliation_error: f64,
    pub principal_recovery_owner: &'static str,
    pub sequential_carry_adoption: bool,
    pub production_adoption: bool,
    pub installed_independent_verification: bool,
    pub mixed_ledger_adoption: bool,
}
fn source_hash(r: &impl Serialize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(&serde_json::to_value(r).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * a.abs().max(b.abs())
}
fn amount(v: f64) -> Result<(), String> {
    if !v.is_finite() || v.abs() > crate::limits::MAX_MONETARY_AMOUNT {
        return Err("finite bounded actual-currency sequential amount required".into());
    }
    Ok(())
}
fn date(day: i32, lag: u32) -> Result<i32, String> {
    let d =
        i32::try_from(i64::from(day) + i64::from(lag)).map_err(|_| "sequential date overflow")?;
    validate_ordinal(d)?;
    Ok(d)
}
pub fn admit(r: &Request) -> Result<Admission, String> {
    admission::admit_with_check(r, &mut || Ok(()))
}
pub fn admit_with_check(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    admission::admit_with_check(r, check)
}
struct ActionAt<'a> {
    ordinal: i32,
    interval_index: usize,
    origin: Option<usize>,
    kind: &'a str,
    source_id: &'a str,
    review: &'a Identity,
}
fn action(actions: &mut Vec<Action>, at: ActionAt<'_>, values: [f64; 3]) -> Result<(), String> {
    for x in values {
        amount(x)?;
    }
    actions.push(Action {
        ordinal: at.ordinal,
        interval_index: at.interval_index,
        origin: at.origin,
        kind: at.kind.into(),
        source_id: at.source_id.into(),
        review: at.review.clone(),
        income: values[0],
        gross_carry_disposed: values[1],
        actual_cash: values[2],
    });
    Ok(())
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    evaluate_with_check(r, &mut || Ok(()))
}
pub fn evaluate_with_check(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let admission = admit_with_check(r, check)?;
    let original_source = boundary_receipt::project_with_check(&r.original_contract, check)?;
    let source = boundary_receipt::project_with_check(&r.contract, check)?;
    let (original_interest_cash, original_closeouts, original_full_end) =
        original::original_economics(r, &original_source, check)?;
    for native in [
        &original_source.native_projection,
        &source.native_projection,
    ] {
        let last = native
            .points
            .last()
            .ok_or("missing paired native final point")?;
        if last.contractual_principal != 0.
            || last.investor_principal != [0.; 3]
            || last.investor_interest != [0.; 3]
            || last.borrower_principal != [0.; 3]
            || last.borrower_interest != [0.; 3]
        {
            return Err("complete paired native live contractual/principal/accrued tail required without terminal plug".into());
        }
    }
    let projection = &source.native_projection;
    let end = r.contract.as_of_ordinal + r.contract.horizon_days as i32;
    let last = projection
        .points
        .last()
        .ok_or("missing native final point")?;
    if last.contractual_principal != 0.
        || last.investor_principal != [0.; 3]
        || last.investor_interest != [0.; 3]
        || last.borrower_principal != [0.; 3]
        || last.borrower_interest != [0.; 3]
    {
        return Err("complete native live contractual/principal/accrued tail required without terminal plug".into());
    }
    let entitlements: BTreeMap<_, _> = projection
        .entitlements
        .iter()
        .map(|e| (e.event_id.as_str(), e))
        .collect();
    let cash: BTreeMap<_, _> = projection
        .cashflows
        .iter()
        .map(|e| (e.event_id.as_str(), e))
        .collect();
    let links: BTreeMap<_, _> = source
        .capture
        .cash_links
        .iter()
        .map(|e| (e.entitlement_id.as_str(), e.cash_event_id.as_str()))
        .collect();
    if links.len() != projection.entitlements.len()
        || cash.len() != projection.cashflows.len()
        || source.capture.boundaries.len() != source.intervals.len()
        || source.intervals.len() != admission.interval_count
    {
        return Err("actual source boundary/cash identity cardinality failed".into());
    }
    for (id, e) in &entitlements {
        let actual = cash
            .get(links.get(id).ok_or("missing actual native cash link")?)
            .ok_or("native cash id absent")?;
        if e.entitlement_ordinal != e.remittance_ordinal
            || actual.ordinal != e.remittance_ordinal
            || actual.principal != e.principal
            || actual.interest != e.interest
            || actual.kind != e.kind
        {
            return Err("zero-float entitled versus actual cash ownership failed".into());
        }
    }
    let mut g = r.acquisition.opening_gross_carrying;
    let mut intervals = Vec::with_capacity(admission.interval_count);
    let mut actions = Vec::new();
    let mut interest_cash = Vec::new();
    let mut closeouts = Vec::new();
    let mut earned = 0.;
    let mut live_earned = [0.; 3];
    let mut retired_earned = 0.;
    let mut full_end = end.max(original_full_end);
    let mut used_ids = BTreeSet::new();
    for (index, (receipt, boundary)) in source
        .intervals
        .iter()
        .zip(&source.capture.boundaries)
        .enumerate()
    {
        check()?;
        if receipt.interval_index != index
            || boundary.interval_index != index
            || boundary.ordinal != receipt.end_ordinal
        {
            return Err("actual native source interval order mismatch".into());
        }
        if index > 0 {
            let previous = &source.capture.boundaries[index - 1];
            if receipt.opening_investor_principal != previous.closing_investor_principal
                || receipt.opening_investor_interest != previous.closing_investor_interest
            {
                return Err("source carry discontinuity at native boundary".into());
            }
        }
        let mut opening = coupled_carry::State {
            measure: CreditMeasure::Physical,
            monetary_unit: r.contract.monetary_unit.clone(),
            probability: receipt.opening_probability,
            principal: [0.; 7],
            accrued_interest: [0.; 7],
            gross_carrying: [0.; 7],
            earned_eir: [0.; 7],
        };
        opening.principal[..3].copy_from_slice(&receipt.opening_investor_principal);
        opening.accrued_interest[..3].copy_from_slice(&receipt.opening_investor_interest);
        opening.gross_carrying[..3].copy_from_slice(&g);
        opening.earned_eir[..3].copy_from_slice(&live_earned);
        let request = receipt.carry_request(
            r.acquisition.original_annual_effective_rate,
            coupled_carry::Policy {
                identity: r.policy.identity.clone(),
                original_income: r.policy.original_income.clone(),
                remittance_float: "external_explicit_owner_required-1".into(),
            },
        )?;
        let operator = coupled_carry::compile(&request)?;
        let transport = operator.apply(&opening)?;
        if transport.closing.principal != boundary.transported_investor_principal
            || transport.closing.accrued_interest != boundary.transported_investor_interest
        {
            return Err("exact sequential P/A differs from actual native transport".into());
        }
        earned += transport.total_interval_earned_eir;
        g.copy_from_slice(&transport.closing.gross_carrying[..3]);
        live_earned.copy_from_slice(&transport.closing.earned_eir[..3]);
        retired_earned += transport.closing.earned_eir[3..].iter().sum::<f64>();
        for (offset, claim) in projection.defaults
            [boundary.default_claim_start..boundary.default_claim_end]
            .iter()
            .enumerate()
        {
            check()?;
            let claim_index = boundary.default_claim_start + offset;
            let origin = claim.origin;
            let slot = origin + 3;
            let exact_g = transport.closing.gross_carrying[slot];
            let basis = exact_g - claim.investor_principal - claim.investor_accrued_interest;
            if claim.investor_principal != transport.closing.principal[slot]
                || claim.investor_accrued_interest != transport.closing.accrued_interest[slot]
            {
                return Err("native default claim versus exact origin carry mismatch".into());
            }
            let at = |kind, review| ActionAt {
                ordinal: claim.default_ordinal,
                interval_index: index,
                origin: Some(origin),
                kind,
                source_id: &claim.claim_id,
                review,
            };
            action(
                &mut actions,
                at("default_accrued_reversal", &r.policy.interest.review),
                [
                    -claim.investor_accrued_interest,
                    claim.investor_accrued_interest,
                    0.,
                ],
            )?;
            action(
                &mut actions,
                at("default_signed_basis_disposal", &r.policy.identity),
                [-basis, basis, 0.],
            )?;
            action(
                &mut actions,
                at(
                    "default_native_principal_loss_book_chargeoff",
                    &r.policy.default_principal_book_review,
                ),
                [
                    -claim.borrower_principal_loss,
                    claim.borrower_principal_loss,
                    0.,
                ],
            )?;
            let recovered = claim
                .complete_recovery_tail
                .iter()
                .map(|x| x.principal)
                .sum::<f64>();
            let principal_disposal = claim.borrower_principal - recovered;
            let principal_date = date(
                claim.default_ordinal,
                r.policy.economic_principal_disposal_lag_days,
            )?;
            let interest_date = date(claim.default_ordinal, r.policy.interest.disposal_lag_days)?;
            let collected_interest =
                claim.borrower_contract_interest * r.policy.interest.collection_fraction[origin];
            let mut actual_collected_interest = 0.;
            let mut interest_ids = Vec::new();
            for (n, t) in r.policy.interest.complete_collection_tails[origin]
                .iter()
                .enumerate()
            {
                let ordinal = date(claim.default_ordinal, t.lag_days)?;
                let value = collected_interest * t.weight;
                actual_collected_interest += value;
                let id = format!("{}:reviewed-interest:{n}", claim.claim_id);
                action(
                    &mut actions,
                    ActionAt {
                        ordinal,
                        interval_index: index,
                        origin: Some(origin),
                        kind: "reviewed_default_interest_cash_income",
                        source_id: &id,
                        review: &r.policy.interest.review,
                    },
                    [value, 0., value],
                )?;
                interest_cash.push(InterestCash {
                    cash_id: id.clone(),
                    claim_id: claim.claim_id.clone(),
                    ordinal,
                    amount: value,
                    review: r.policy.interest.review.clone(),
                });
                interest_ids.push(id);
                full_end = full_end.max(ordinal);
            }
            let mut principal_ids = Vec::new();
            let recovery_links: Vec<_> = boundary
                .default_recoveries
                .iter()
                .filter(|e| e.claim_index == claim_index)
                .collect();
            if recovery_links.len() != claim.complete_recovery_tail.len() {
                return Err("same-loop native claim/recovery cardinality mismatch".into());
            }
            for (tail_index, (recovery, captured)) in claim
                .complete_recovery_tail
                .iter()
                .zip(recovery_links)
                .enumerate()
            {
                check()?;
                if captured.tail_index != tail_index
                    || captured.origin != origin
                    || captured.ordinal != recovery.ordinal
                    || captured.principal != recovery.principal
                {
                    return Err("same-loop native claim/recovery source mismatch".into());
                }
                if let Some(id) = &captured.entitlement_id {
                    let event = entitlements
                        .get(id.as_str())
                        .ok_or("captured recovery entitlement absent")?;
                    if event.kind != "whole_loan_default_recovery"
                        || event.entitlement_ordinal != recovery.ordinal
                        || event.principal != recovery.principal
                        || event.interest != 0.
                        || !used_ids.insert(id.clone())
                    {
                        return Err("same-loop recovery entitlement ownership differs".into());
                    }
                    let cash_id = links[id.as_str()].to_string();
                    principal_ids.push(cash_id.clone());
                    action(
                        &mut actions,
                        ActionAt {
                            ordinal: recovery.ordinal,
                            interval_index: index,
                            origin: Some(origin),
                            kind: "native_default_principal_cash",
                            source_id: &cash_id,
                            review: &r.policy.default_principal_book_review,
                        },
                        [0., recovery.principal, recovery.principal],
                    )?;
                } else if recovery.principal != 0. {
                    return Err("nonzero recovery lacks actual native entitlement/cash id".into());
                }
                full_end = full_end.max(recovery.ordinal);
            }
            full_end = full_end.max(principal_date).max(interest_date);
            let interest_disposal = claim.borrower_contract_interest - actual_collected_interest;
            if principal_disposal < 0. || interest_disposal < 0. {
                return Err("economic waiver cannot dispose overcollected gross claims".into());
            }
            closeouts.push(Closeout {
                claim_id: claim.claim_id.clone(),
                origin,
                default_ordinal: claim.default_ordinal,
                gross_principal: claim.borrower_principal,
                gross_contract_interest: claim.borrower_contract_interest,
                recognized_interest: claim.investor_accrued_interest,
                exact_default_carry: exact_g,
                signed_default_basis: basis,
                principal_cash: recovered,
                interest_cash: actual_collected_interest,
                principal_economic_disposal: principal_disposal,
                principal_disposal_ordinal: principal_date,
                principal_disposal_review: r.policy.economic_principal_disposal_review.clone(),
                interest_economic_disposal: interest_disposal,
                interest_disposal_ordinal: interest_date,
                interest_disposal_review: r
                    .policy
                    .interest
                    .economic_disposal_review
                    .clone()
                    .unwrap(),
                native_principal_cash_ids: principal_ids,
                reviewed_interest_cash_ids: interest_ids,
                closing_economic_principal: claim.borrower_principal
                    - recovered
                    - principal_disposal,
                closing_economic_interest: claim.borrower_contract_interest
                    - actual_collected_interest
                    - interest_disposal,
                closing_book_principal: claim.investor_principal
                    - claim.borrower_principal_loss
                    - recovered,
                closing_book_accrued: 0.,
                closing_book_basis: 0.,
            });
        }
        for prepay in &boundary.prepayments {
            let exact_g = transport.closing.gross_carrying[6] * prepay.fraction;
            let actual = prepay.principal + prepay.interest;
            if let Some(id) = &prepay.entitlement_id {
                if !used_ids.insert(id.clone()) {
                    return Err("native payoff cash already owned".into());
                }
                action(
                    &mut actions,
                    ActionAt {
                        ordinal: boundary.ordinal,
                        interval_index: index,
                        origin: None,
                        kind: &format!("{}_exact_exit", prepay.kind),
                        source_id: links[id.as_str()],
                        review: &r.policy.identity,
                    },
                    [actual - exact_g, exact_g, actual],
                )?;
            } else if actual != 0. || exact_g != 0. {
                return Err("prepaid exact carry lacks actual native cash ownership".into());
            }
        }
        for scheduled in &boundary.scheduled {
            let origin = scheduled.origin;
            let actual = scheduled.principal + scheduled.interest;
            g[origin] -= actual;
            if let Some(id) = &scheduled.entitlement_id {
                if !used_ids.insert(id.clone()) {
                    return Err("native scheduled cash already owned".into());
                }
                action(
                    &mut actions,
                    ActionAt {
                        ordinal: boundary.ordinal,
                        interval_index: index,
                        origin: Some(origin),
                        kind: "scheduled_native_cash",
                        source_id: links[id.as_str()],
                        review: &r.policy.identity,
                    },
                    [0., actual, actual],
                )?;
            } else if actual != 0. {
                return Err("scheduled cash lacks actual native cash identity".into());
            }
            if boundary.closing_investor_principal[origin] == 0.
                && boundary.closing_investor_interest[origin] == 0.
            {
                action(
                    &mut actions,
                    ActionAt {
                        ordinal: boundary.ordinal,
                        interval_index: index,
                        origin: Some(origin),
                        kind: "scheduled_full_exit_signed_basis",
                        source_id: &format!("scheduled:{index}:{origin}"),
                        review: &r.policy.identity,
                    },
                    [-g[origin], g[origin], 0.],
                )?;
                g[origin] = 0.;
                retired_earned += live_earned[origin];
                live_earned[origin] = 0.;
            } else if g[origin] < 0. {
                return Err(
                    "partial cash settlement would create negative live gross carrying".into(),
                );
            }
        }
        for x in g {
            amount(x)?;
        }
        let basis = std::array::from_fn(|i| {
            g[i] - boundary.closing_investor_principal[i] - boundary.closing_investor_interest[i]
        });
        intervals.push(Interval {
            interval_index: index,
            opening,
            native_carry_request: request,
            operator,
            transport,
            closing_live_gross_carrying: g,
            closing_live_basis: basis,
            closing_live_earned_eir: live_earned,
        });
    }
    if used_ids.len() != projection.entitlements.len()
        || g != [0.; 3]
        || closeouts.iter().any(|d| {
            !near(d.closing_book_principal, 0.)
                || d.closing_economic_principal != 0.
                || d.closing_economic_interest != 0.
        })
    {
        return Err("full native cash/gross economic/book ownership closure failed".into());
    }
    let y = r.acquisition.original_annual_effective_rate.ln_1p();
    let discounted = |ordinal: i32, value: f64| {
        value * (-y * f64::from(ordinal - r.contract.as_of_ordinal) / 365.).exp()
    };
    let quote = original_source
        .native_projection
        .cashflows
        .iter()
        .map(|e| discounted(e.ordinal, e.principal + e.interest))
        .sum::<f64>()
        + original_interest_cash
            .iter()
            .map(|e| discounted(e.ordinal, e.amount))
            .sum::<f64>();
    if !near(quote, r.acquisition.actual_cost) {
        return Err("actual original acquisition cost versus complete original-yield actual-cash quote failed".into());
    }
    let actual_cash = projection
        .cashflows
        .iter()
        .map(|e| e.principal + e.interest)
        .sum::<f64>()
        + interest_cash.iter().map(|e| e.amount).sum::<f64>();
    let income = earned + actions.iter().map(|e| e.income).sum::<f64>();
    let reconciliation = r.acquisition.actual_cost + income - actual_cash;
    let earned_reconciliation = retired_earned + live_earned.iter().sum::<f64>() - earned;
    if !near(retired_earned + live_earned.iter().sum::<f64>(), earned) {
        return Err("cumulative original-EIR reward transport conservation failed".into());
    }
    if !near(r.acquisition.actual_cost + income, actual_cash) {
        return Err("sequential cost plus earned/disposal income versus actual full cash reconciliation failed".into());
    }
    actions.sort_by_key(|a| (a.ordinal, a.interval_index));
    let original_source_raw_hash = source_hash(&r.original_contract)?;
    let current_source_raw_hash = source_hash(&r.contract)?;
    let original_source_receipt_hash = source_hash(&original_source)?;
    let current_source_receipt_hash = source_hash(&source)?;
    let result = ResultData {
        schema: SCHEMA,
        raw_request: r.clone(),
        admission,
        source,
        original_source,
        original_source_raw_hash,
        current_source_raw_hash,
        original_source_receipt_hash,
        current_source_receipt_hash,
        intervals,
        actions,
        reviewed_interest_cash: interest_cash,
        default_closeouts: closeouts,
        original_quote_pv: quote,
        original_reviewed_interest_cash: original_interest_cash,
        original_economic_closeouts: original_closeouts,
        original_full_end_ordinal: original_full_end,
        actual_acquisition_cost: r.acquisition.actual_cost,
        total_earned_eir: earned,
        total_accounting_income: income,
        total_actual_cash: actual_cash,
        full_end_ordinal: full_end,
        closing_live_gross_carrying: g,
        closing_reconciliation_error: reconciliation,
        principal_recovery_owner: "native_mortgage_projection_only-1",
        cumulative_earned_eir_conservation_error: earned_reconciliation,
        quote_basis: "complete_immutable_original_actual_cash_only-1",
        sequential_carry_adoption: false,
        production_adoption: false,
        installed_independent_verification: false,
        mixed_ledger_adoption: false,
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > result.admission.retained_byte_bound
    {
        return Err("completed sequential output exceeded admitted retained bytes".into());
    }
    check()?;
    Ok(result)
}
pub fn run<W: Write>(r: &Request, writer: &mut W) -> Result<(), String> {
    run_with_check(r, writer, &mut || Ok(()))
}
pub fn run_with_check<W: Write>(
    r: &Request,
    writer: &mut W,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let result = evaluate_with_check(r, check)?;
    check()?;
    serde_json::to_writer(&mut *writer, &result).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    check()
}

#[cfg(test)]
#[path = "mortgage_sequential_carry/tests.rs"]
mod tests;
