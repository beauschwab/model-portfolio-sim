//! Versioned actual daily loan-credit execution, separate from legacy overlays.
use crate::journal::credit::{
    AccountCode, Identity, JournalIntent, LiveState, ReviewedAllowance, ReviewedChargeoff,
};
use crate::owners::credit::state::{
    self as credit_state, CompiledCreditState, CreditStateContract, CreditStateOpening,
    CreditStateStep,
};
use crate::transport::types::{Account, Spec};
use portfolio_model_core::cashflow::validate_ordinal;
use portfolio_model_core::limits::{MAX_DATED_METADATA_BYTES, MAX_MONETARY_AMOUNT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufWriter, Write};

pub const SCHEMA: &str = "credit-ledger-1";
pub const PROTOCOL: u32 = 4;
pub const MAX_WORK: usize = 90_000_000;
pub const MAX_CONTRACTS: usize = 62_000;
/// Explicit `acceptance_60000` tier: 60,000 positions in each of a baseline and a stress run.
pub const ACCEPTANCE_MAX_CONTRACTS: usize = 124_000;
pub const ACCEPTANCE_TIER: &str = "acceptance_60000";
/// Retained recovery claims per run under the acceptance tier (still bounded by MAX_WORK).
pub const ACCEPTANCE_MAX_CLAIMS: usize = 1_000_000;
pub const MAX_BOUNDARIES: usize = 1_000_000;
mod strict_action;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LoanAction {
    /// Fractions of surviving actual exposure; no opening-notional scaling.
    Repayment {
        principal_fraction: [f64; 3],
        accrued_fraction: [f64; 3],
        cancel_undrawn_fraction: [f64; 3],
    },
    Draw {
        amount_by_state: [f64; 3],
    },
    /// Full principal/accrual/basis settlement and undrawn cancellation; distinct population exit.
    Maturity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoanBoundary {
    pub event_id: String,
    pub source: Identity,
    pub policy: Identity,
    pub recognition_ordinal: i32,
    #[serde(deserialize_with = "strict_action::deserialize")]
    pub action: LoanAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditReviewDay {
    pub recognition_ordinal: i32,
    pub chargeoffs: Vec<ReviewedChargeoff>,
    pub allowance_review: Option<ReviewedAllowance>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditReplayPosition {
    pub account_id: String,
    pub contract: CreditStateContract,
    pub opening: CreditStateOpening,
    pub boundaries: Vec<LoanBoundary>,
    pub reviews: Vec<CreditReviewDay>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditReplayRun {
    pub run_id: String,
    pub scenario_id: String,
    pub severity: f64,
    pub positions: Vec<CreditReplayPosition>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditReplayRequest {
    pub schema: String,
    pub monetary_unit: String,
    pub as_of_ordinal: i32,
    pub specification: Value,
    pub runs: Vec<CreditReplayRun>,
    /// Explicitly selected acceptance tier; absent keeps the historical contract bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_tier: Option<String>,
}
struct Position {
    account: usize,
    compiled: CompiledCreditState,
    opening: CreditStateOpening,
    boundaries: Vec<LoanBoundary>,
    reviews: Vec<CreditReviewDay>,
}
struct Run {
    run_id: String,
    scenario: String,
    severity: f64,
    positions: Vec<Position>,
}
pub struct CompiledCreditReplay {
    spec: Spec,
    as_of: i32,
    end: i32,
    runs: Vec<Run>,
    admitted_work: usize,
    max_work: usize,
    max_claims: usize,
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8_f64.max(128. * f64::EPSILON * a.abs().max(b.abs()))
}
fn amount(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > MAX_MONETARY_AMOUNT {
        Err("credit replay currency amount exceeds bounds".into())
    } else {
        Ok(())
    }
}
fn text(s: &str, bytes: &mut usize) -> Result<(), String> {
    *bytes = bytes.saturating_add(s.len());
    if s.trim().is_empty()
        || s.len() > 256
        || s.chars().any(char::is_control)
        || *bytes > MAX_DATED_METADATA_BYTES
    {
        Err("credit replay metadata exceeds bounded identity domain".into())
    } else {
        Ok(())
    }
}
fn identity(id: &Identity, bytes: &mut usize) -> Result<(), String> {
    text(&id.id, bytes)?;
    text(&id.revision, bytes)
}
fn bits(severity: f64) -> u64 {
    if severity == 0. {
        0.0f64.to_bits()
    } else {
        severity.to_bits()
    }
}
fn live_sum(state: &CreditStateOpening, get: impl Fn(&LiveState) -> f64) -> f64 {
    state.accounting.live.iter().map(get).sum()
}

impl CompiledCreditReplay {
    pub fn compile(request: CreditReplayRequest) -> Result<Self, String> {
        Self::compile_with_limits(request, 60_000, MAX_WORK)
    }
    pub fn normalized_spec(&self) -> &Spec {
        &self.spec
    }
    pub fn admitted_work(&self) -> usize {
        self.admitted_work
    }
    pub fn compile_with_limits(
        request: CreditReplayRequest,
        max_positions: usize,
        max_work: usize,
    ) -> Result<Self, String> {
        if max_positions > 60_000 || max_work > MAX_WORK {
            return Err(
                "credit admission cannot exceed 60000 positions/90000000 work units".into(),
            );
        }
        if request.schema != SCHEMA || request.monetary_unit != "currency_units" {
            return Err("credit replay schema/units must be explicit".into());
        }
        validate_ordinal(request.as_of_ordinal)?;
        let spec = crate::validation::specification::normalize(
            request.specification,
            max_positions,
            max_work,
        )?;
        let end = request
            .as_of_ordinal
            .checked_add(spec.horizon_days as i32)
            .ok_or("credit replay end overflow")?;
        validate_ordinal(end)?;
        if !spec.netting_sets.is_empty()
            || !spec.cashflows.is_empty()
            || !spec.policies.is_empty()
            || spec.positions.is_empty()
            || spec.accounts.iter().any(|a| {
                a.annual_fees != 0.
                    || a.annual_costs != 0.
                    || a.tax_rate != 0.
                    || a.annual_dividends != 0.
            })
        {
            return Err("credit-ledger-1 requires owned loan positions and explicit boundary/review events, without legacy overlays or account earnings policies".into());
        }
        for p in &spec.positions {
            if p.kind != "loan"
                || p.start_day != 0
                || p.annual_pd != 0.
                || p.migration_rate != 0.
                || p.draw_fraction != 0.
                || p.monthly_runoff != 0.
                || p.floating_beta != 0.
                || p.encumbered_fraction != 0.
                || !p.collateral_position.is_empty()
                || p.eligible_fraction != 0.
                || matches!(p.classification.as_str(), "afs" | "trading")
            {
                return Err("credit owned loan cannot overlap legacy default, draw, runoff, mark or collateral ownership".into());
            }
        }
        let expected: BTreeSet<_> = std::iter::once(("baseline".to_owned(), bits(1.)))
            .chain(spec.scenarios.iter().flat_map(|s| {
                std::iter::once((s.name.clone(), bits(1.))).chain(
                    spec.reverse_severities
                        .iter()
                        .map(|&v| (s.name.clone(), bits(v))),
                )
            }))
            .collect();
        if request.runs.len() != expected.len() {
            return Err("credit runs require exact baseline/scenario/reverse coverage".into());
        }
        let mut actual = BTreeSet::new();
        let mut run_ids = BTreeSet::new();
        let mut bytes = 0;
        let mut contracts = 0usize;
        let mut boundaries = 0usize;
        let mut admitted_work = spec
            .horizon_days
            .checked_mul(spec.accounts.len())
            .and_then(|n| n.checked_mul(expected.len()))
            .ok_or("credit account work overflow")?;
        let accounts: HashMap<_, _> = spec
            .accounts
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.as_str(), i))
            .collect();
        let positions: HashMap<_, _> = spec.positions.iter().map(|p| (p.id.as_str(), p)).collect();
        let (max_contracts, max_claims) = match request.scale_tier.as_deref() {
            None => (MAX_CONTRACTS, crate::journal::credit::MAX_CLAIMS),
            Some(ACCEPTANCE_TIER) => (ACCEPTANCE_MAX_CONTRACTS, ACCEPTANCE_MAX_CLAIMS),
            Some(_) => return Err("unknown credit acceptance tier".into()),
        };
        let mut runs = Vec::with_capacity(request.runs.len());
        for r in request.runs {
            portfolio_compute_control::checkpoint()?;
            text(&r.run_id, &mut bytes)?;
            text(&r.scenario_id, &mut bytes)?;
            if !r.severity.is_finite()
                || !actual.insert((r.scenario_id.clone(), bits(r.severity)))
                || !run_ids.insert(r.run_id.clone())
                || r.positions.len() != spec.positions.len()
            {
                return Err("duplicate or incomplete credit run identity/positions".into());
            }
            contracts = contracts
                .checked_add(r.positions.len())
                .ok_or("credit contract count overflow")?;
            if contracts > max_contracts {
                return Err("credit replay exceeds 62000 run-position contracts".into());
            }
            let mut seen = BTreeSet::new();
            let mut namespaces = BTreeSet::new();
            let mut opening_claim_ids = BTreeSet::new();
            let mut prepared = Vec::with_capacity(r.positions.len());
            for (i, p) in r.positions.into_iter().enumerate() {
                if i.is_multiple_of(256) {
                    portfolio_compute_control::checkpoint()?;
                }
                let c = &p.contract;
                for id in [
                    &c.identity,
                    &c.economic_source,
                    &c.recovery_schedule_review,
                    &c.accounting_policy.identity,
                ] {
                    identity(id, &mut bytes)?;
                }
                text(&c.position_id, &mut bytes)?;
                text(&c.claim_namespace, &mut bytes)?;
                if !namespaces.insert(c.claim_namespace.clone()) {
                    return Err("credit claim namespaces must be unique within each run".into());
                }
                let anchor = positions
                    .get(c.position_id.as_str())
                    .ok_or("credit contract names unknown loan")?;
                let &account = accounts
                    .get(p.account_id.as_str())
                    .ok_or("credit position account missing")?;
                if !seen.insert(c.position_id.clone())
                    || anchor.account != p.account_id.as_str()
                    || c.currency != spec.accounts[account].currency
                    || c.interval_days != 1
                    || p.opening.last_ordinal != request.as_of_ordinal
                    || p.opening.contract_identity != c.identity
                    || p.opening.schema != credit_state::OPENING_SCHEMA
                {
                    return Err(
                        "credit position identity/account/currency/opening/daily clock mismatch"
                            .into(),
                    );
                }
                let claims = &p.opening.accounting.claims;
                if claims.len() > crate::journal::credit::MAX_CLAIMS {
                    return Err("credit opening claims exceed admission".into());
                }
                for claim in claims {
                    text(&claim.claim_id, &mut bytes)?;
                    for id in [
                        &claim.economic_source,
                        &claim.recovery_schedule_review,
                        &claim.policy.identity,
                    ] {
                        identity(id, &mut bytes)?;
                    }
                    if !opening_claim_ids.insert(claim.claim_id.clone())
                        || !claim
                            .claim_id
                            .starts_with(&format!("{}:", c.claim_namespace))
                    {
                        return Err("opening claims require unique IDs belonging to their retained credit namespace".into());
                    }
                }
                if opening_claim_ids.len() > max_claims {
                    return Err("aggregate opening run claims exceed 62000 retained claims".into());
                }
                let principal = live_sum(&p.opening, |x| x.principal)
                    + claims.iter().map(|x| x.outstanding_principal).sum::<f64>();
                let basis = live_sum(&p.opening, |x| x.book_adjustment)
                    + claims
                        .iter()
                        .map(|x| x.outstanding_book_adjustment)
                        .sum::<f64>();
                let accrued = live_sum(&p.opening, |x| x.accrued_interest)
                    + claims
                        .iter()
                        .map(|x| x.outstanding_accrued_interest)
                        .sum::<f64>();
                if !near(principal, anchor.balance)
                    || !near(basis, anchor.book_adjustment)
                    || !near(accrued, anchor.opening_accrued)
                    || !near(p.opening.accounting.allowance, anchor.allowance)
                    || !near(live_sum(&p.opening, |x| x.undrawn), anchor.commitment)
                {
                    return Err(
                        "credit monetary opening does not reconcile normalized loan anchors".into(),
                    );
                }
                let compiled = credit_state::compile(c)?;
                // Validate the complete opening/claim/policy contract without projecting its economics.
                let validation = crate::journal::credit::CreditAccountingRequest {
                    schema: crate::journal::credit::SCHEMA.into(),
                    measure: portfolio_model_core::cashflow::CashflowMeasure::Physical,
                    currency: c.currency.clone(),
                    monetary_unit: c.monetary_unit.clone(),
                    position_id: c.position_id.clone(),
                    recognition_ordinal: request.as_of_ordinal,
                    reporting_end_ordinal: end,
                    policy: c.accounting_policy.clone(),
                    opening: p.opening.accounting.clone(),
                    defaults: vec![],
                    chargeoffs: vec![],
                    allowance_review: None,
                };
                if claims
                    .iter()
                    .flat_map(|x| &x.remaining_recoveries)
                    .any(|x| x.payment_ordinal <= request.as_of_ordinal)
                {
                    return Err("opening claims must already settle as-of recoveries".into());
                }
                crate::journal::credit::evaluate(&validation)?;
                if p.opening
                    .probability
                    .iter()
                    .any(|x| !x.is_finite() || *x < 0.)
                    || p.opening.probability.iter().sum::<f64>() > 1. + 1e-11
                {
                    return Err("invalid absolute opening probability".into());
                }
                let mut events = BTreeSet::new();
                let mut maturity_count = 0usize;
                let mut last = request.as_of_ordinal;
                boundaries = boundaries
                    .checked_add(p.boundaries.len() + p.reviews.len())
                    .ok_or("credit event count overflow")?;
                if boundaries > MAX_BOUNDARIES {
                    return Err("credit replay exceeds one million boundary/review dates".into());
                }
                for b in &p.boundaries {
                    text(&b.event_id, &mut bytes)?;
                    identity(&b.source, &mut bytes)?;
                    identity(&b.policy, &mut bytes)?;
                    if b.recognition_ordinal <= request.as_of_ordinal
                        || b.recognition_ordinal > end
                        || b.recognition_ordinal < last
                        || !events.insert(&b.event_id)
                    {
                        return Err("credit boundaries require unique IDs and ordered dates in the reporting interval".into());
                    }
                    last = b.recognition_ordinal;
                    match &b.action {
                        LoanAction::Repayment {
                            principal_fraction,
                            accrued_fraction,
                            cancel_undrawn_fraction,
                        } => {
                            if principal_fraction
                                .iter()
                                .chain(accrued_fraction)
                                .chain(cancel_undrawn_fraction)
                                .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
                            {
                                return Err("repayment/cancel fractions must lie in 0..1".into());
                            }
                        }
                        LoanAction::Draw { amount_by_state } => {
                            for &x in amount_by_state {
                                amount(x)?;
                                if x < 0. {
                                    return Err("ordinary draw must be nonnegative".into());
                                }
                            }
                        }
                        LoanAction::Maturity => {
                            maturity_count += 1;
                            let maturity =
                                i64::from(request.as_of_ordinal) + anchor.maturity_day as i64;
                            if anchor.maturity_day == 0
                                || maturity != i64::from(b.recognition_ordinal)
                                || maturity_count > 1
                            {
                                return Err("explicit maturity must occur once on the normalized contractual maturity anchor".into());
                            }
                        }
                    }
                }
                let mut last_review = request.as_of_ordinal;
                if anchor.maturity_day > 0
                    && anchor.maturity_day <= spec.horizon_days
                    && maturity_count != 1
                {
                    return Err(
                        "in-report contractual maturity requires its explicit Maturity boundary"
                            .into(),
                    );
                }
                for review in &p.reviews {
                    if review.recognition_ordinal <= last_review
                        || review.recognition_ordinal > end
                        || review.chargeoffs.len() > crate::journal::credit::MAX_REVIEW_EVENTS
                    {
                        return Err(
                            "credit reviews require unique increasing bounded recognition dates"
                                .into(),
                        );
                    }
                    for chargeoff in &review.chargeoffs {
                        identity(&chargeoff.review, &mut bytes)?;
                        text(&chargeoff.claim_id, &mut bytes)?;
                        for x in [
                            chargeoff.principal_amount,
                            chargeoff.book_adjustment_amount,
                            chargeoff.accrued_interest_reversal,
                        ] {
                            amount(x)?;
                        }
                        if chargeoff.recognition_ordinal != review.recognition_ordinal
                            || chargeoff.principal_amount < 0.
                        {
                            return Err("reviewed chargeoff requires its exact event date and nonnegative principal".into());
                        }
                    }
                    if let Some(allowance) = &review.allowance_review {
                        identity(&allowance.review, &mut bytes)?;
                        amount(allowance.target_amount)?;
                        if allowance.recognition_ordinal != review.recognition_ordinal
                            || allowance.target_amount < 0.
                        {
                            return Err("reviewed allowance requires its exact event date and nonnegative target".into());
                        }
                    }
                    last_review = review.recognition_ordinal;
                }
                admitted_work = admitted_work
                    .checked_add(
                        spec.horizon_days + claims.len() + p.boundaries.len() + p.reviews.len(),
                    )
                    .ok_or("credit work overflow")?;
                prepared.push(Position {
                    account,
                    compiled,
                    opening: p.opening,
                    boundaries: p.boundaries,
                    reviews: p.reviews,
                });
            }
            runs.push(Run {
                run_id: r.run_id,
                scenario: r.scenario_id,
                severity: r.severity,
                positions: prepared,
            });
        }
        if actual != expected || admitted_work > max_work {
            return Err("credit run coverage or aggregate work admission failed".into());
        }
        Ok(Self {
            spec,
            as_of: request.as_of_ordinal,
            end,
            runs,
            admitted_work,
            max_work,
            max_claims,
        })
    }
}

fn gl_name(code: AccountCode) -> &'static str {
    match code {
        AccountCode::Cash => "cash",
        AccountCode::LivePrincipal => "asset_principal",
        AccountCode::DefaultPrincipal => "credit_default_principal",
        AccountCode::LiveAccruedInterest => "accrued_interest",
        AccountCode::DefaultAccruedInterest => "credit_default_accrued_interest",
        AccountCode::LiveBookAdjustment => "book_adjustment",
        AccountCode::DefaultBookAdjustment => "credit_default_book_adjustment",
        AccountCode::Allowance => "allowance",
        AccountCode::InterestIncome => "interest_income",
        AccountCode::CreditLossExpense => "credit_loss_expense",
        AccountCode::BasisRecognitionExpense => "basis_recognition_expense",
        AccountCode::RecoveryIncome => "recovery_income",
    }
}
fn is_earnings(gl: &str) -> bool {
    matches!(
        gl,
        "interest_income" | "credit_loss_expense" | "basis_recognition_expense" | "recovery_income"
    )
}
fn is_asset(gl: &str) -> bool {
    matches!(
        gl,
        "cash"
            | "asset_principal"
            | "credit_default_principal"
            | "accrued_interest"
            | "credit_default_accrued_interest"
            | "book_adjustment"
            | "credit_default_book_adjustment"
            | "allowance"
    )
}

struct Sink<W: Write> {
    out: BufWriter<W>,
    rows: BTreeMap<&'static str, Vec<Value>>,
    count: usize,
    transaction: usize,
}
impl<W: Write> Sink<W> {
    fn emit(&mut self, table: &'static str, row: Value) -> Result<(), String> {
        self.rows.entry(table).or_default().push(row);
        self.count += 1;
        if self.count >= 1024 {
            self.flush()?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        for (table, rows) in std::mem::take(&mut self.rows) {
            serde_json::to_writer(&mut self.out, &json!({"table":table,"rows":rows}))
                .map_err(|e| e.to_string())?;
            self.out.write_all(b"\n").map_err(|e| e.to_string())?;
        }
        self.count = 0;
        Ok(())
    }
}
type Gl = BTreeMap<(usize, &'static str, String), f64>;
#[allow(clippy::too_many_arguments)]
fn post<W: Write>(
    sink: &mut Sink<W>,
    gl: &mut Gl,
    run: &Run,
    accounts: &[Account],
    account: usize,
    day: usize,
    ordinal: i32,
    event: &str,
    lines: &[(&'static str, String, f64)],
) -> Result<(), String> {
    let sum = lines.iter().map(|x| x.2).sum::<f64>();
    let scale = lines.iter().map(|x| x.2.abs()).sum::<f64>();
    if sum.abs() > 1e-8_f64.max(128. * f64::EPSILON * scale) {
        return Err("credit daily transaction is unbalanced".into());
    }
    if lines.iter().all(|x| x.2 == 0.) {
        return Ok(());
    }
    sink.transaction += 1;
    for (name, instrument, value) in lines {
        amount(*value)?;
        if *value == 0. {
            continue;
        }
        let balance = gl.entry((account, *name, instrument.clone())).or_default();
        *balance += value;
        amount(*balance)?;
        sink.emit("journal",json!({"transaction_number":sink.transaction,"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":accounts[account].id,"event":event,"gl_account":name,"instrument_id":instrument,"value":value}))?;
    }
    Ok(())
}
fn instrument(code: AccountCode, position: &str, claim: Option<&str>) -> String {
    match code {
        AccountCode::Cash => "@cash".into(),
        AccountCode::DefaultPrincipal
        | AccountCode::DefaultAccruedInterest
        | AccountCode::DefaultBookAdjustment => claim.unwrap_or(position).into(),
        _ => position.into(),
    }
}
fn boundary(
    state: &mut CreditStateOpening,
    event: &LoanBoundary,
    matured: &mut f64,
) -> Result<Vec<(&'static str, String, f64)>, String> {
    let mut principal = 0.;
    let mut accrued = 0.;
    let mut basis = 0.;
    let mut draw = 0.;
    match &event.action {
        LoanAction::Draw { amount_by_state } => {
            for (row, &x) in state.accounting.live.iter_mut().zip(amount_by_state) {
                if x > row.undrawn {
                    return Err("ordinary draw exceeds surviving undrawn commitment".into());
                }
                row.undrawn -= x;
                row.principal += x;
                amount(row.principal)?;
                draw += x;
            }
        }
        action => {
            let all = [1.; 3];
            let (pf, af, uf) = match action {
                LoanAction::Repayment {
                    principal_fraction,
                    accrued_fraction,
                    cancel_undrawn_fraction,
                } => (
                    principal_fraction,
                    accrued_fraction,
                    cancel_undrawn_fraction,
                ),
                LoanAction::Maturity => (&all, &all, &all),
                LoanAction::Draw { .. } => unreachable!(),
            };
            for (i, row) in state.accounting.live.iter_mut().enumerate() {
                let p = row.principal * pf[i];
                let a = row.accrued_interest * af[i];
                let b = row.book_adjustment * pf[i];
                principal += p;
                accrued += a;
                basis += b;
                row.principal -= p;
                row.accrued_interest -= a;
                row.book_adjustment -= b;
                row.undrawn *= 1. - uf[i];
            }
            if matches!(action, LoanAction::Maturity) {
                for p in &mut state.probability[..3] {
                    *matured += *p;
                    *p = 0.;
                }
            }
        }
    }
    let pos = state.contract_identity.id.clone(); // Caller replaces position instrument label below.
    Ok(vec![
        ("cash", "@cash".into(), principal + accrued - draw),
        ("asset_principal", pos.clone(), draw - principal),
        ("accrued_interest", pos.clone(), -accrued),
        ("book_adjustment", pos.clone(), -basis),
        ("basis_recognition_expense", pos, basis),
    ])
}

/// Stream only complete, reconciled days; final completion is withheld on any failure/cancellation.
pub fn run<W: Write>(compiled: CompiledCreditReplay, output: W) -> Result<(), String> {
    let mut sink = Sink {
        out: BufWriter::new(output),
        rows: BTreeMap::new(),
        count: 0,
        transaction: 0,
    };
    let mut work = compiled.admitted_work;
    for mut run in compiled.runs {
        portfolio_compute_control::checkpoint()?;
        let mut gl = Gl::new();
        let mut states: Vec<_> = run.positions.iter().map(|p| p.opening.clone()).collect();
        let mut matured = vec![0.; states.len()];
        let mut boundary_cursor = vec![0; states.len()];
        let mut review_cursor = vec![0; states.len()];
        let mut max_error = 0.0_f64;
        for (ai, a) in compiled.spec.accounts.iter().enumerate() {
            let mut lines = vec![
                ("cash", "@cash".into(), a.cash),
                ("equity_opening", "@equity".into(), -a.equity),
            ];
            for (p, s) in run
                .positions
                .iter()
                .zip(&states)
                .filter(|(p, _)| p.account == ai)
            {
                let pos = &p.compiled.contract().position_id;
                lines.extend([
                    ("asset_principal", pos.clone(), live_sum(s, |r| r.principal)),
                    (
                        "accrued_interest",
                        pos.clone(),
                        live_sum(s, |r| r.accrued_interest),
                    ),
                    (
                        "book_adjustment",
                        pos.clone(),
                        live_sum(s, |r| r.book_adjustment),
                    ),
                    ("allowance", pos.clone(), -s.accounting.allowance),
                ]);
                for claim in &s.accounting.claims {
                    lines.extend([
                        (
                            "credit_default_principal",
                            claim.claim_id.clone(),
                            claim.outstanding_principal,
                        ),
                        (
                            "credit_default_accrued_interest",
                            claim.claim_id.clone(),
                            claim.outstanding_accrued_interest,
                        ),
                        (
                            "credit_default_book_adjustment",
                            claim.claim_id.clone(),
                            claim.outstanding_book_adjustment,
                        ),
                    ]);
                }
            }
            post(
                &mut sink,
                &mut gl,
                &run,
                &compiled.spec.accounts,
                ai,
                0,
                compiled.as_of,
                "opening_balance",
                &lines,
            )?;
        }
        for day in 1..=compiled.spec.horizon_days {
            portfolio_compute_control::checkpoint()?;
            let ordinal = compiled.as_of + day as i32;
            let claim_count = states
                .iter()
                .map(|s| s.accounting.claims.len())
                .sum::<usize>();
            if claim_count > compiled.max_claims {
                return Err("aggregate run recovery claims exceed 62000 retained claims".into());
            }
            work = work
                .checked_add(claim_count)
                .ok_or("credit daily claim work overflow")?;
            if work > compiled.max_work {
                return Err("credit daily claim scan exceeds admitted work".into());
            }
            for (i, p) in run.positions.iter().enumerate() {
                if i.is_multiple_of(256) {
                    portfolio_compute_control::checkpoint()?;
                }
                let review = p
                    .reviews
                    .get(review_cursor[i])
                    .filter(|r| r.recognition_ordinal == ordinal);
                let step = CreditStateStep {
                    recognition_ordinal: ordinal,
                    reporting_end_ordinal: compiled.end,
                    chargeoffs: review.map_or_else(Vec::new, |r| r.chargeoffs.clone()),
                    allowance_review: review.and_then(|r| r.allowance_review.clone()),
                };
                if review.is_some() {
                    review_cursor[i] += 1;
                }
                let result = p.compiled.advance(&states[i], &step)?;
                for j in &result.intents {
                    let lines: Vec<_> = j
                        .postings
                        .iter()
                        .map(|line| {
                            (
                                gl_name(line.account),
                                instrument(
                                    line.account,
                                    &p.compiled.contract().position_id,
                                    j.claim_id.as_deref(),
                                ),
                                line.amount,
                            )
                        })
                        .collect();
                    post(
                        &mut sink,
                        &mut gl,
                        &run,
                        &compiled.spec.accounts,
                        p.account,
                        day,
                        ordinal,
                        &j.event,
                        &lines,
                    )?;
                    emit_intent_lineage(
                        &mut sink,
                        &run,
                        p,
                        &compiled.spec.accounts[p.account].id,
                        day,
                        ordinal,
                        j,
                    )?;
                }
                sink.emit("credit_lineage",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":compiled.spec.accounts[p.account].id,"position":p.compiled.contract().position_id,
                    "event_id":format!("{}:{}:interval",p.compiled.contract().claim_namespace,ordinal),"event":"joint_credit_interval","source_id":p.compiled.contract().economic_source.id,"source_revision":p.compiled.contract().economic_source.revision,
                    "policy_id":p.compiled.contract().accounting_policy.identity.id,"policy_revision":p.compiled.contract().accounting_policy.identity.revision,"claim_id":null,
                    "earned_interest":result.earned_interest,"economic_principal_loss":result.economic_principal_loss,"contingent_draw_cash":result.contingent_draw_cash,"prepay_cash":result.prepaid.cash,"recovery_cash":result.modeled_recovery_cash}))?;
                states[i] = result.closing;
                while let Some(event) = p
                    .boundaries
                    .get(boundary_cursor[i])
                    .filter(|b| b.recognition_ordinal == ordinal)
                {
                    if boundary_cursor[i].is_multiple_of(256) {
                        portfolio_compute_control::checkpoint()?;
                    }
                    let mut lines = boundary(&mut states[i], event, &mut matured[i])?;
                    for (name, instrument, _) in &mut lines {
                        if *name != "cash" {
                            *instrument = p.compiled.contract().position_id.clone();
                        }
                    }
                    post(
                        &mut sink,
                        &mut gl,
                        &run,
                        &compiled.spec.accounts,
                        p.account,
                        day,
                        ordinal,
                        &event.event_id,
                        &lines,
                    )?;
                    sink.emit("credit_lineage",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":compiled.spec.accounts[p.account].id,"position":p.compiled.contract().position_id,
                        "event_id":event.event_id,"event":match event.action {LoanAction::Repayment{..}=>"scheduled_surviving_repayment",LoanAction::Draw{..}=>"scheduled_ordinary_draw",LoanAction::Maturity=>"contractual_maturity"},"source_id":event.source.id,"source_revision":event.source.revision,"policy_id":event.policy.id,"policy_revision":event.policy.revision,"claim_id":null,
                        "earned_interest":0.,"economic_principal_loss":0.,"contingent_draw_cash":0.,"prepay_cash":0.,"recovery_cash":0.}))?;
                    boundary_cursor[i] += 1;
                }
                emit_state(
                    &mut sink,
                    &run,
                    &compiled.spec.accounts,
                    p,
                    &states[i],
                    matured[i],
                    day,
                    ordinal,
                    compiled.end,
                )?;
            }
            let total_claims = states
                .iter()
                .map(|s| s.accounting.claims.len())
                .sum::<usize>();
            if total_claims > compiled.max_claims {
                return Err("aggregate closing claims exceed native retention bound".into());
            }
            reconcile(
                &mut sink,
                &run,
                &compiled.spec.accounts,
                &states,
                &gl,
                day,
                ordinal,
                &mut max_error,
            )?;
        }
        for ((ai, name, instrument), balance) in &gl {
            sink.emit("trial_balance",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"account":compiled.spec.accounts[*ai].id,"gl_account":name,"instrument_id":instrument,"balance":balance}))?;
        }
        for (ai, a) in compiled.spec.accounts.iter().enumerate() {
            let cash = gl
                .iter()
                .filter(|((i, g, _), _)| *i == ai && *g == "cash")
                .map(|(_, v)| v)
                .sum::<f64>();
            let assets = gl
                .iter()
                .filter(|((i, g, _), _)| *i == ai && is_asset(g))
                .map(|(_, v)| v)
                .sum::<f64>();
            let equity = a.equity
                - gl.iter()
                    .filter(|((i, g, _), _)| *i == ai && is_earnings(g))
                    .map(|(_, v)| v)
                    .sum::<f64>();
            sink.emit("summary",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"account":a.id,"final_cash":cash,"final_assets":assets,"final_equity":equity,"max_reconciliation_error":max_error}))?;
        }
        // Drop a run's retained states before the next independent scenario.
        run.positions.clear();
    }
    sink.flush()?;
    portfolio_compute_control::checkpoint()?;
    serde_json::to_writer(&mut sink.out, &json!({"complete":true,"protocol":PROTOCOL}))
        .map_err(|e| e.to_string())?;
    sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
    sink.out.flush().map_err(|e| e.to_string())
}

fn emit_intent_lineage<W: Write>(
    sink: &mut Sink<W>,
    run: &Run,
    p: &Position,
    account: &str,
    day: usize,
    ordinal: i32,
    j: &JournalIntent,
) -> Result<(), String> {
    sink.emit("credit_lineage",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":account,"position":p.compiled.contract().position_id,
        "event_id":format!("{}:{}:{}:{}",p.compiled.contract().claim_namespace,ordinal,j.event,j.claim_id.as_deref().unwrap_or("live")),"event":j.event,"source_id":j.source.id,"source_revision":j.source.revision,"policy_id":j.policy.id,"policy_revision":j.policy.revision,"claim_id":j.claim_id,
        "earned_interest":0.,"economic_principal_loss":0.,"contingent_draw_cash":0.,"prepay_cash":0.,"recovery_cash":0.}))
}
#[allow(clippy::too_many_arguments)]
fn emit_state<W: Write>(
    sink: &mut Sink<W>,
    run: &Run,
    accounts: &[Account],
    p: &Position,
    state: &CreditStateOpening,
    matured: f64,
    day: usize,
    ordinal: i32,
    end: i32,
) -> Result<(), String> {
    let c = p.compiled.contract();
    let claims = &state.accounting.claims;
    sink.emit("credit_states",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":accounts[p.account].id,"position":c.position_id,"contract_id":c.identity.id,"contract_revision":c.identity.revision,
        "probability":state.probability,"matured_probability":matured,"live_principal":state.accounting.live.map(|r|r.principal),"live_undrawn":state.accounting.live.map(|r|r.undrawn),"live_accrued_interest":state.accounting.live.map(|r|r.accrued_interest),"live_book_adjustment":state.accounting.live.map(|r|r.book_adjustment),"allowance":state.accounting.allowance,
        "default_principal":claims.iter().map(|c|c.outstanding_principal).sum::<f64>(),"default_accrued_interest":claims.iter().map(|c|c.outstanding_accrued_interest).sum::<f64>(),"default_book_adjustment":claims.iter().map(|c|c.outstanding_book_adjustment).sum::<f64>()}))?;
    for claim in claims {
        sink.emit("credit_claims",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":accounts[p.account].id,"position":c.position_id,
        "claim_id":claim.claim_id,"economic_source_id":claim.economic_source.id,"economic_source_revision":claim.economic_source.revision,"recovery_review_id":claim.recovery_schedule_review.id,"recovery_review_revision":claim.recovery_schedule_review.revision,"default_ordinal":claim.default_ordinal,
        "initial_principal":claim.initial_principal,"economic_principal_loss":claim.economic_principal_loss,"outstanding_principal":claim.outstanding_principal,"outstanding_accrued_interest":claim.outstanding_accrued_interest,"outstanding_book_adjustment":claim.outstanding_book_adjustment,"principal_charged_off":claim.principal_charged_off,"recovery_cash_collected":claim.recovery_cash_collected,"recovery_applied_to_principal":claim.recovery_applied_to_principal,
        "remaining_recovery_cash":claim.remaining_recoveries.iter().map(|p|p.amount).sum::<f64>(),"recovery_tail_after_reporting":claim.remaining_recoveries.iter().filter(|p|p.payment_ordinal>end).map(|p|p.amount).sum::<f64>()}))?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn reconcile<W: Write>(
    sink: &mut Sink<W>,
    run: &Run,
    accounts: &[Account],
    states: &[CreditStateOpening],
    gl: &Gl,
    day: usize,
    ordinal: i32,
    max_error: &mut f64,
) -> Result<(), String> {
    for (ai, a) in accounts.iter().enumerate() {
        let cash = gl
            .iter()
            .filter(|((i, g, _), _)| *i == ai && *g == "cash")
            .map(|(_, v)| v)
            .sum::<f64>();
        // Each side is a sum of many balances in a different order (GL key order versus position
        // order). Count the terms and their magnitude so the comparison allows floating
        // summation error (about n * eps * sum|terms|) on top of the fixed tolerance.
        let (mut terms, mut magnitude) = (0usize, 0f64);
        let assets = gl
            .iter()
            .filter(|((i, g, _), _)| *i == ai && is_asset(g))
            .map(|(_, v)| {
                terms += 1;
                magnitude += v.abs();
                v
            })
            .sum::<f64>();
        let equity = a.equity
            - gl.iter()
                .filter(|((i, g, _), _)| *i == ai && is_earnings(g))
                .map(|(_, v)| {
                    terms += 1;
                    magnitude += v.abs();
                    v
                })
                .sum::<f64>();
        let mut source_assets = cash;
        let mut allowance = 0.;
        for (p, state) in run
            .positions
            .iter()
            .zip(states)
            .filter(|(p, _)| p.account == ai)
        {
            let _ = p;
            allowance += state.accounting.allowance;
            let live = live_sum(state, |r| {
                r.principal + r.book_adjustment + r.accrued_interest
            }) - state.accounting.allowance;
            source_assets += live;
            terms += 4;
            magnitude += live.abs() + state.accounting.allowance.abs();
            for c in &state.accounting.claims {
                let claim = c.outstanding_principal
                    + c.outstanding_book_adjustment
                    + c.outstanding_accrued_interest;
                source_assets += claim;
                terms += 3;
                magnitude += claim.abs();
            }
        }
        let error = (assets - equity).abs().max((assets - source_assets).abs());
        *max_error = max_error.max(error);
        let summed = |x: f64, y: f64| {
            near(x, y)
                || (x - y).abs() <= terms as f64 * f64::EPSILON * magnitude.max(a.equity.abs())
        };
        if !summed(assets, equity) || !summed(assets, source_assets) {
            return Err("daily credit GL/source subledger reconciliation failed".into());
        }
        for v in [cash, assets, equity, allowance] {
            amount(v)?;
        }
        sink.emit("daily_balance",json!({"run_id":run.run_id,"scenario":run.scenario,"severity":run.severity,"day":day,"ordinal":ordinal,"account":a.id,"entity":a.entity,"currency":a.currency,"cash":cash,"assets":assets,"equity":equity,"allowance":allowance,"reconciliation_error":error}))?;
    }
    Ok(())
}
