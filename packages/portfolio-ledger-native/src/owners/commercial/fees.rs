//! Borrowed sealed fee groups enter the same commercial daily Sim.
use crate::owners::commercial::intents::{Compiled, FeeBinding};
use portfolio_model_core::nonintegral_fee::{Account, State};
use std::collections::BTreeMap;
pub const PROTOCOL: u32 = 7;
pub(crate) fn gl(account: Account) -> &'static str {
    match account {
        Account::Cash => "cash",
        Account::FeeReceivable => "commercial_fee_receivable",
        Account::FeePayable => "commercial_fee_payable",
        Account::DeferredFeeIncome => "commercial_deferred_fee_income",
        Account::PrepaidFeeExpense => "commercial_prepaid_fee_expense",
        Account::FeeIncome => "pnl:commercial_nonintegral_fee_income",
        Account::FeeWriteoffExpense => "pnl:commercial_fee_writeoff_expense",
        Account::FeeRecoveryIncome => "pnl:commercial_fee_recovery_income",
    }
}
fn money(v: f64) -> Result<f64, String> {
    if v.is_finite() && v.abs() <= 1e15 {
        Ok(v)
    } else {
        Err("fee ledger monetary domain".into())
    }
}
#[derive(Clone, Default)]
pub(crate) struct FeeTracking {
    pub states: BTreeMap<String, State>,
    stocks: BTreeMap<(String, &'static str), f64>,
}
type StagedFee = (FeeTracking, Vec<(&'static str, f64)>, f64, f64);
impl FeeTracking {
    pub fn stage(
        &self,
        c: &Compiled<'_>,
        binding: usize,
        index: usize,
    ) -> Result<StagedFee, String> {
        let f = &c.fees[binding];
        let b = &c.bindings[f.loan_binding];
        let factor = b.factor()?;
        let group = &f.source.groups()[index];
        let mut next = self.clone();
        let mut lines = Vec::new();
        let (mut cash, mut equity) = (0., 0.);
        for p in &group.postings {
            let name = gl(p.account);
            let key = (group.event.fee_id.clone(), name);
            let old = *next.stocks.get(&key).unwrap_or(&0.);
            next.stocks.insert(key, money(old + p.amount)?);
            let value = money(p.amount * factor)?;
            if value != 0. {
                lines.push((name, value));
            }
            if name == "cash" {
                cash = money(cash + value)?;
            }
            if name.starts_with("pnl:") {
                equity = money(equity - value)?;
            }
        }
        for (name, expected) in stocks(&group.next) {
            let actual = *next
                .stocks
                .get(&(group.event.fee_id.clone(), name))
                .unwrap_or(&0.);
            if (actual - expected).abs()
                > 1e-9_f64.max(64. * f64::EPSILON * actual.abs().max(expected.abs()))
            {
                return Err("native fee absolute stock reconciliation".into());
            }
        }
        next.states
            .insert(group.event.fee_id.clone(), group.next.clone());
        Ok((next, lines, cash, equity))
    }
    pub fn values(
        &self,
        b: &FeeBinding<'_>,
        c: &Compiled<'_>,
    ) -> Result<Vec<(usize, &'static str, f64)>, String> {
        let loan = &c.bindings[b.loan_binding];
        let factor = loan.factor()?;
        let mut out = Vec::new();
        for state in self.states.values() {
            for (name, amount) in stocks(state) {
                out.push((c.positions[b.loan_binding], name, money(amount * factor)?));
            }
        }
        Ok(out)
    }
}
fn stocks(s: &State) -> [(&'static str, f64); 4] {
    [
        ("commercial_fee_receivable", s.fee_receivable),
        ("commercial_fee_payable", s.fee_payable),
        ("commercial_deferred_fee_income", s.deferred_fee_income),
        ("commercial_prepaid_fee_expense", s.prepaid_fee_expense),
    ]
}
