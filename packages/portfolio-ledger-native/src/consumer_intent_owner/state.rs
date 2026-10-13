//! Failure-atomic path balances and complete intent mapping for the daily owner.
use portfolio_model_core::credit_accounting::{
    AccountCode, AccountingState, Identity, JournalIntent,
};
use std::collections::BTreeMap;

pub const OWNER_POLICY: &str = "consumer_product_event_intents-1";
pub const DAY_PHASE: &str =
    "opening_acquisition_dated_recognition_consumer_events_dated_settlement_close-1";

#[derive(Clone, Debug)]
pub struct Scale {
    pub allocation_face: f64,
    pub unit_face: f64,
    pub source_weight: f64,
}
impl Scale {
    pub fn factor(&self) -> Result<f64, String> {
        if !self.allocation_face.is_finite()
            || !(0.0..=1e15).contains(&self.allocation_face)
            || !self.unit_face.is_finite()
            || !(0.0..=1e15).contains(&self.unit_face)
            || self.unit_face == 0.
            || !self.source_weight.is_finite()
            || !(0.0..=1.0).contains(&self.source_weight)
        {
            return Err("explicit bounded allocation/source scale required".into());
        }
        amount((self.allocation_face / self.unit_face) * self.source_weight)
    }
}
#[derive(Clone, Debug)]
pub struct InitialAllowanceReview {
    pub review: Identity,
    /// Unweighted amount equal to the raw primitive's opening_allowance.
    pub amount: f64,
}
#[derive(Clone, Debug, Default)]
struct ClaimBalances {
    principal: f64,
    accrued: f64,
    basis: f64,
    interest_basis: f64,
}
#[derive(Clone, Debug, Default)]
pub struct PathBalances {
    live_principal: f64,
    live_accrued: f64,
    live_basis: f64,
    allowance: f64,
    claims: BTreeMap<String, ClaimBalances>,
    acquired: bool,
    live_interest_basis: f64,
    rights_cash_received: f64,
    rights_original_eir_income: f64,
    rights_reviewed_basis_income: f64,
    rights_shared_default_income: f64,
    rights_action_count: usize,
    rights_last_ordinal: Option<i32>,
}
#[derive(Clone, Debug)]
pub struct Line {
    pub gl: &'static str,
    pub amount: f64,
}
#[derive(Clone, Debug)]
pub struct StagedIntent {
    pub event: String,
    pub claim_id: Option<String>,
    /// In native core posting order, including explicit gross-right splits.
    pub lines: Vec<Line>,
    pub cash_delta: f64,
    pub equity_delta: f64,
    pub next: PathBalances,
}
#[derive(Clone, Debug)]
pub struct AcquisitionStage {
    pub purchase: StagedIntent,
    pub reviewed_initial_allowance: Option<(Identity, StagedIntent)>,
}
fn amount(v: f64) -> Result<f64, String> {
    if v.is_finite() && v.abs() <= 1e15 {
        Ok(v)
    } else {
        Err("consumer intent finite currency domain required".into())
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("consumer source/intent/state reconciliation failed".into())
    } else {
        Ok(())
    }
}
fn is_earnings(code: AccountCode) -> bool {
    matches!(
        code,
        AccountCode::InterestIncome
            | AccountCode::CreditLossExpense
            | AccountCode::BasisRecognitionExpense
            | AccountCode::RecoveryIncome
    )
}
/// Credit channels keep the core debit-positive convention. P&L codes must have
/// the shared report's pnl: prefix; standalone credit_replay's plain names cannot
/// be copied into Sim without a different classification rule.
pub(crate) enum IncomeOwner {Consumer, Commercial}
fn gl(code: AccountCode) -> &'static str {gl_with_owner(code,IncomeOwner::Consumer)}
pub(crate) fn gl_with_owner(code: AccountCode, owner: IncomeOwner) -> &'static str {
    match code {
        AccountCode::Cash => "cash",
        AccountCode::LivePrincipal => "asset_principal",
        AccountCode::DefaultPrincipal => "credit_default_principal",
        AccountCode::LiveAccruedInterest => "dated_interest_receivable",
        AccountCode::DefaultAccruedInterest => "credit_default_interest_receivable",
        AccountCode::LiveBookAdjustment => "book_adjustment",
        AccountCode::DefaultBookAdjustment => "credit_default_book_adjustment",
        AccountCode::Allowance => "allowance",
        AccountCode::InterestIncome => match owner {IncomeOwner::Consumer=>"pnl:consumer_interest_income",IncomeOwner::Commercial=>"pnl:commercial_effective_interest_income"},
        AccountCode::CreditLossExpense => match owner {IncomeOwner::Consumer=>"pnl:consumer_credit_loss_expense",IncomeOwner::Commercial=>"pnl:commercial_credit_loss_expense"},
        AccountCode::BasisRecognitionExpense => match owner {IncomeOwner::Consumer=>"pnl:consumer_basis_recognition_expense",IncomeOwner::Commercial=>"pnl:commercial_basis_realization_expense"},
        AccountCode::RecoveryIncome => match owner {IncomeOwner::Consumer=>"pnl:consumer_recovery_income",IncomeOwner::Commercial=>"pnl:commercial_recovery_income"},
    }
}
fn push(lines: &mut Vec<Line>, name: &'static str, value: f64, factor: f64) -> Result<(), String> {
    let value = amount(value * factor)?;
    if value != 0. {
        lines.push(Line {
            gl: name,
            amount: value,
        });
    }
    Ok(())
}
/// Classify before netting across physical paths. Negative residual/prepaid rights
/// remain explicit liability buckets, never an offset against another path's asset.
pub(crate) fn accrue(
    lines: &mut Vec<Line>,
    prior: f64,
    next: f64,
    default: bool,
    factor: f64,
) -> Result<(), String> {
    let asset = if default {
        "credit_default_interest_receivable"
    } else {
        "dated_interest_receivable"
    };
    let liability = if default {
        "credit_default_deferred_income"
    } else {
        "dated_deferred_income"
    };
    push(lines, asset, next.max(0.) - prior.max(0.), factor)?;
    push(lines, liability, next.min(0.) - prior.min(0.), factor)
}
impl PathBalances {
    pub(super) fn rights_summary(&self) -> [f64; 2] {
        [
            self.live_interest_basis,
            self.claims.values().map(|c| c.interest_basis).sum(),
        ]
    }
    fn post_account(
        &mut self,
        lines: &mut Vec<Line>,
        code: AccountCode,
        value: f64,
        claim_id: Option<&str>,
        factor: f64,
    ) -> Result<(), String> {
        let prior = self.live_accrued;
        let mut default_prior = None;
        match code {
            AccountCode::LivePrincipal => {
                self.live_principal = amount(self.live_principal + value)?
            }
            AccountCode::LiveAccruedInterest => {
                self.live_accrued = amount(self.live_accrued + value)?
            }
            AccountCode::LiveBookAdjustment => self.live_basis = amount(self.live_basis + value)?,
            AccountCode::Allowance => self.allowance = amount(self.allowance - value)?,
            AccountCode::DefaultPrincipal
            | AccountCode::DefaultAccruedInterest
            | AccountCode::DefaultBookAdjustment => {
                let id = claim_id.ok_or("default channel requires immutable claim ID")?;
                let claim = self.claims.entry(id.to_owned()).or_default();
                match code {
                    AccountCode::DefaultPrincipal => {
                        claim.principal = amount(claim.principal + value)?
                    }
                    AccountCode::DefaultAccruedInterest => {
                        default_prior = Some(claim.accrued);
                        claim.accrued = amount(claim.accrued + value)?;
                    }
                    AccountCode::DefaultBookAdjustment => {
                        claim.basis = amount(claim.basis + value)?
                    }
                    _ => unreachable!(),
                }
            }
            _ => {}
        }
        if code == AccountCode::LiveAccruedInterest {
            accrue(lines, prior, self.live_accrued, false, factor)?;
        } else if let Some(prior) = default_prior {
            accrue(
                lines,
                prior,
                self.claims[claim_id.unwrap()].accrued,
                true,
                factor,
            )?;
        } else {
            push(lines, gl(code), value, factor)?;
        }
        Ok(())
    }
    pub(super) fn summary(&self) -> [f64; 9] {
        let mut v = [
            self.live_principal,
            self.live_basis,
            self.allowance,
            self.live_accrued.max(0.),
            self.live_accrued.min(0.),
            0.,
            0.,
            0.,
            0.,
        ];
        for c in self.claims.values() {
            v[5] += c.principal;
            v[6] += c.basis;
            v[7] += c.accrued.max(0.);
            v[8] += c.accrued.min(0.);
        }
        v
    }
    /// Outer coordinator generates this purchase from raw native acquisition;
    /// the primitive contains no purchase cash journal. Once only.
    pub fn acquire(
        &self,
        principal: f64,
        cost: f64,
        allowance: f64,
        allowance_review: Option<&InitialAllowanceReview>,
        scale: &Scale,
    ) -> Result<AcquisitionStage, String> {
        if self.acquired
            || self.live_principal != 0.
            || self.live_accrued != 0.
            || self.live_basis != 0.
            || self.allowance != 0.
            || !self.claims.is_empty()
            || principal <= 0.
            || principal != scale.unit_face
            || cost <= 0.
            || allowance < 0.
        {
            return Err("consumer acquisition requires exact zero owned opening".into());
        }
        for v in [principal, cost, allowance] {
            amount(v)?;
        }
        if let Some(r) = allowance_review {
            near(r.amount, allowance)?;
            if [&r.review.id, &r.review.revision]
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            {
                return Err("explicit initial allowance identity required".into());
            }
        } else if allowance != 0. {
            return Err("reviewed initial allowance recognition required".into());
        }
        let f = scale.factor()?;
        let mut lines = Vec::new();
        push(&mut lines, "asset_principal", principal, f)?;
        push(&mut lines, "book_adjustment", cost - principal, f)?;
        push(&mut lines, "cash", -cost, f)?;
        let next = Self {
            live_principal: principal,
            live_basis: cost - principal,
            acquired: true,
            ..Default::default()
        };
        let purchase = StagedIntent {
            event: "consumer_acquisition".into(),
            claim_id: None,
            lines,
            cash_delta: amount(-cost * f)?,
            equity_delta: 0.,
            next: next.clone(),
        };
        let reviewed_initial_allowance = if let Some(review) = allowance_review {
            let mut lines = Vec::new();
            push(&mut lines, "pnl:consumer_credit_loss_expense", allowance, f)?;
            push(&mut lines, "allowance", -allowance, f)?;
            let mut closing = next;
            closing.allowance = allowance;
            Some((
                review.review.clone(),
                StagedIntent {
                    event: "consumer_reviewed_initial_allowance".into(),
                    claim_id: None,
                    lines,
                    cash_delta: 0.,
                    equity_delta: amount(-allowance * f)?,
                    next: closing,
                },
            ))
        } else {
            None
        };
        Ok(AcquisitionStage {
            purchase,
            reviewed_initial_allowance,
        })
    }
    /// Stage state and mapped postings together. Commit only after the shared Sim
    /// validates the complete transaction, then update pooled cash/equity once.
    /// Core intents are generated from immutable RAW requests internally; callers
    /// cannot submit this draft's JournalIntent as an amount/exposure override.
    pub fn stage(&self, intent: &JournalIntent, scale: &Scale) -> Result<StagedIntent, String> {
        if !self.acquired {
            return Err("consumer intent precedes acquisition".into());
        }
        let factor = scale.factor()?;
        let mut next = self.clone();
        let mut lines = Vec::new();
        let (mut cash, mut equity, mut net) = (0., 0., 0.);
        for p in &intent.postings {
            amount(p.amount)?;
            net = amount(net + p.amount)?;
            if p.account == AccountCode::Cash {
                cash = amount(cash + p.amount)?;
            }
            if is_earnings(p.account) {
                equity = amount(equity - p.amount)?;
            }
            next.post_account(
                &mut lines,
                p.account,
                p.amount,
                intent.claim_id.as_deref(),
                factor,
            )?;
        }
        near(net, 0.)?;
        near(lines.iter().map(|p| p.amount).sum(), 0.)?;
        Ok(StagedIntent {
            event: intent.event.clone(),
            claim_id: intent.claim_id.clone(),
            lines,
            cash_delta: amount(cash * factor)?,
            equity_delta: amount(equity * factor)?,
            next,
        })
    }
    /// Independent per-path source state reconciliation at each actual date;
    /// terminal claim metadata/recovery tails require additional exact identity
    /// checks against the retained raw/native receipt, not monetary sums alone.
    pub fn reconcile(&self, snapshot: &AccountingState) -> Result<(), String> {
        if snapshot.live.iter().any(|s| s.undrawn != 0.) {
            return Err("installment owner does not admit undrawn commitments".into());
        }
        near(
            self.live_principal,
            snapshot.live.iter().map(|s| s.principal).sum(),
        )?;
        near(
            self.live_accrued,
            snapshot.live.iter().map(|s| s.accrued_interest).sum(),
        )?;
        near(
            self.live_basis,
            snapshot.live.iter().map(|s| s.book_adjustment).sum(),
        )?;
        near(self.allowance, snapshot.allowance)?;
        for claim in &snapshot.claims {
            let got = self
                .claims
                .get(&claim.claim_id)
                .ok_or("native claim absent from intents")?;
            near(got.principal, claim.outstanding_principal)?;
            near(got.accrued, claim.outstanding_accrued_interest)?;
            near(got.basis, claim.outstanding_book_adjustment)?;
        }
        if self.claims.len() != snapshot.claims.len() {
            return Err("claim identity cardinality mismatch".into());
        }
        Ok(())
    }
    pub fn start_rights(
        &mut self,
        snapshot: &portfolio_model_core::card_credit_accounting_v2::Snapshot,
    ) -> Result<(), String> {
        self.reconcile(&snapshot.core)?;
        if snapshot.live_interest_basis != 0.
            || !snapshot.claim_interest_basis.is_empty()
            || snapshot.cash_received != 0.
            || snapshot.original_eir_income != 0.
            || snapshot.reviewed_basis_income != 0.
            || snapshot.shared_default_income != 0.
            || snapshot.action_count != 0
            || self.rights_last_ordinal.is_some()
        {
            return Err(
                "rights owner requires exact acquired opening without replayed actions".into(),
            );
        }
        self.rights_last_ordinal = Some(snapshot.last_ordinal);
        self.reconcile_rights(snapshot)
    }
    pub fn reconcile_rights(
        &self,
        snapshot: &portfolio_model_core::card_credit_accounting_v2::Snapshot,
    ) -> Result<(), String> {
        self.reconcile(&snapshot.core)?;
        near(self.live_interest_basis, snapshot.live_interest_basis)?;
        near(self.rights_cash_received, snapshot.cash_received)?;
        near(
            self.rights_original_eir_income,
            snapshot.original_eir_income,
        )?;
        near(
            self.rights_reviewed_basis_income,
            snapshot.reviewed_basis_income,
        )?;
        near(
            self.rights_shared_default_income,
            snapshot.shared_default_income,
        )?;
        if self.rights_action_count != snapshot.action_count
            || self.rights_last_ordinal != Some(snapshot.last_ordinal)
        {
            return Err("rights owner atomic action/date identity mismatch".into());
        }
        for (id, expected) in &snapshot.claim_interest_basis {
            near(
                self.claims
                    .get(id)
                    .ok_or("rights basis orphan claim")?
                    .interest_basis,
                *expected,
            )?;
        }
        if self.claims.iter().any(|(id, c)| {
            !snapshot.claim_interest_basis.contains_key(id) && c.interest_basis != 0.
        }) {
            return Err("rights owner unresolved default interest basis".into());
        }
        Ok(())
    }
    pub fn stage_rights(
        &self,
        action: &portfolio_model_core::card_credit_accounting_v2::AtomicAction,
        events: &[portfolio_model_core::card_credit_accounting_v2::Event],
        scale: &Scale,
    ) -> Result<StagedIntent, String> {
        use portfolio_model_core::card_credit_accounting_v2::OwnedAccount as O;
        if !self.acquired
            || self.rights_last_ordinal.is_none_or(|d| action.ordinal < d)
            || action.atomic_action != self.rights_action_count + 1
            || events.len() != action.events.end - action.events.start
        {
            return Err("rights action must follow acquired native chronology exactly once".into());
        }
        let mut next = self.clone();
        let factor = scale.factor()?;
        let mut lines = Vec::new();
        let (mut cash, mut equity, mut net) = (0., 0., 0.);
        for event in events {
            portfolio_compute_control::checkpoint()?;
            if event.ordinal != action.ordinal || event.atomic_action != action.atomic_action {
                return Err("rights complete event/action identity mismatch".into());
            }
            let mut event_net = 0.;
            for p in &event.lines {
                amount(p.amount)?;
                event_net = amount(event_net + p.amount)?;
                net = amount(net + p.amount)?;
                let code = match p.account {
                    O::Cash => Some(AccountCode::Cash),
                    O::LivePrincipal => Some(AccountCode::LivePrincipal),
                    O::DefaultPrincipal => Some(AccountCode::DefaultPrincipal),
                    O::LiveAccruedInterest => Some(AccountCode::LiveAccruedInterest),
                    O::DefaultAccruedInterest => Some(AccountCode::DefaultAccruedInterest),
                    O::LiveBookAdjustment => Some(AccountCode::LiveBookAdjustment),
                    O::DefaultBookAdjustment => Some(AccountCode::DefaultBookAdjustment),
                    O::Allowance => Some(AccountCode::Allowance),
                    O::SharedInterestIncome => Some(AccountCode::InterestIncome),
                    O::CreditLossExpense => Some(AccountCode::CreditLossExpense),
                    O::BasisRecognitionExpense => Some(AccountCode::BasisRecognitionExpense),
                    O::RecoveryIncome => Some(AccountCode::RecoveryIncome),
                    _ => None,
                };
                if let Some(code) = code {
                    next.post_account(
                        &mut lines,
                        code,
                        p.amount,
                        event.claim_id.as_deref(),
                        factor,
                    )?;
                    if code == AccountCode::Cash {
                        cash = amount(cash + p.amount)?;
                    }
                    if is_earnings(code) {
                        equity = amount(equity - p.amount)?;
                        next.rights_shared_default_income =
                            amount(next.rights_shared_default_income - p.amount)?;
                    }
                } else {
                    let name = match p.account {
                        O::LiveInterestBasis => {
                            next.live_interest_basis = amount(next.live_interest_basis + p.amount)?;
                            "live_interest_basis_adjustment"
                        }
                        O::DefaultInterestBasis => {
                            let id = event
                                .claim_id
                                .as_ref()
                                .ok_or("default interest basis requires claim ID")?;
                            let claim = next.claims.entry(id.clone()).or_default();
                            claim.interest_basis = amount(claim.interest_basis + p.amount)?;
                            "default_interest_basis_adjustment"
                        }
                        O::OriginalEffectiveInterest => {
                            equity = amount(equity - p.amount)?;
                            "pnl:original_effective_interest"
                        }
                        O::ReviewedBasisRealization => {
                            equity = amount(equity - p.amount)?;
                            "pnl:reviewed_basis_realization"
                        }
                        O::ReviewedDefaultBasisRealization => {
                            equity = amount(equity - p.amount)?;
                            "pnl:reviewed_default_basis_realization"
                        }
                        O::ReviewedRecoveryBasisRealization => {
                            equity = amount(equity - p.amount)?;
                            "pnl:reviewed_recovery_basis_realization"
                        }
                        _ => unreachable!(),
                    };
                    push(&mut lines, name, p.amount, factor)?;
                }
            }
            near(event_net, 0.)?;
            next.rights_original_eir_income =
                amount(next.rights_original_eir_income + event.original_eir_income)?;
            next.rights_reviewed_basis_income =
                amount(next.rights_reviewed_basis_income + event.reviewed_basis_income)?;
        }
        next.rights_cash_received = amount(next.rights_cash_received + cash)?;
        next.rights_action_count = action.atomic_action;
        next.rights_last_ordinal = Some(action.ordinal);
        // Core retains reviewed zero-balance claim identities even when the
        // complete event emits no monetary default-account line. Preserve that
        // metadata only; a missing nonzero monetary claim still fails below.
        for claim in &action.committed.core.claims {
            if !next.claims.contains_key(&claim.claim_id)
                && claim.outstanding_principal == 0.
                && claim.outstanding_accrued_interest == 0.
                && claim.outstanding_book_adjustment == 0.
                && events
                    .iter()
                    .any(|e| e.claim_id.as_deref() == Some(claim.claim_id.as_str()))
            {
                next.claims
                    .insert(claim.claim_id.clone(), ClaimBalances::default());
            }
        }
        near(net, 0.)?;
        near(lines.iter().map(|l| l.amount).sum(), 0.)?;
        next.reconcile_rights(&action.committed)?;
        Ok(StagedIntent {
            event: format!("card_rights_atomic:{}", action.source.kind),
            claim_id: None,
            lines,
            cash_delta: amount(cash * factor)?,
            equity_delta: amount(equity * factor)?,
            next,
        })
    }
}
