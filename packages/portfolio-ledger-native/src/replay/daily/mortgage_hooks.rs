//! Copied Sim proof only. Borrowed source groups are the sole monetary owner.
use super::{entry, Sim};
use crate::owners::mortgages::pending::{self as owner, Runtime};
use portfolio_model_core::mortgage_pending_accounting as core;
use serde_json::json;
impl Sim<'_> {
    pub(super) fn mortgage_events(&mut self, day: usize) -> Result<(), String> {
        let Some(c) = self.mortgages else {
            return Ok(());
        };
        let mut state = self
            .mortgage_state
            .take()
            .ok_or("mortgage run state missing")?;
        let run = &self
            .dated
            .ok_or("mortgage actual dated run missing")?
            .store
            .runs()[state.run];
        for &(bi, gi) in c.postings(state.run, day) {
            portfolio_compute_control::checkpoint()?;
            let (o, _, p, source) = c.binding(bi);
            let group = &source.native.groups[gi];
            let next = state
                .stage(c, bi, gi)
                .map_err(|e| format!("mortgage group {gi} {}: {e}", group.kind))?;
            let f = c.bindings[bi].factor;
            let ai = self.aid[o.account_id.as_str()];
            let mut entries = Vec::new();
            let (mut cash, mut pnl) = (0., 0.);
            for l in &group.lines {
                let gl = owner::account(l)?;
                let amount = l.amount * f;
                entries.push(entry(
                    gl,
                    &owner::subject(&o.position_id, &p.path_id, l),
                    amount,
                ));
                if gl == "cash" {
                    cash += amount;
                } else if gl.starts_with("pnl:") {
                    pnl -= amount;
                }
            }
            // Preserve the absolute component gate after allocation scaling.
            owner::near(entries.iter().map(|x| x.2).sum(), 0.)?;
            let event = format!("mortgage:{}", group.kind);
            self.raw_post(day, ai, &event, entries)?;
            let transaction = group
                .lines
                .iter()
                .any(|l| l.amount * f != 0.)
                .then_some(self.sequence);
            for l in &group.lines {
                let gl = owner::account(l)?;
                if gl.starts_with("pnl:") {
                    *self
                        .earnings
                        .entry((ai, self.sink.intern(gl), self.sink.intern(&o.position_id)))
                        .or_default() += l.amount * f;
                }
            }
            self.accounts[ai].cash += cash;
            self.accounts[ai].equity += pnl;
            if !self.accounts[ai].cash.is_finite() || !self.accounts[ai].equity.is_finite() {
                return Err("mortgage pooled account overflow".into());
            }
            let key = (ai, event.clone());
            let aggregate = if let Some(&i) = self.aggregate_index.get(&key) {
                i
            } else {
                let i = self.aggregate.len();
                self.aggregate.push((ai, event.clone(), [0.; 3]));
                self.aggregate_index.insert(key, i);
                i
            };
            self.aggregate[aggregate].2[0] += cash;
            self.aggregate[aggregate].2[1] += pnl;
            state.books[bi] = Some(next);
            if self.detail {
                self.sink.emit("mortgage_lineage",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":group.ordinal,"position_id":o.position_id,"path_id":p.path_id,"registry_source_id":source.source_id,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"native_group_index":gi,"native_source_id":group.source_id,"source_action_indices":group.source_action_indices,"review":group.review,"original_yield_anchor":o.original_yield_anchor,"transaction_number":transaction,"source_groups_sole_postings":true}))?;
            }
        }
        let ordinal = c.raw.dated.dated.as_of_ordinal + day as i32;
        for &bi in &c.run_paths[state.run] {
            portfolio_compute_control::checkpoint()?;
            let (o, _, p, source) = c.binding(bi);
            let purchase = source
                .native
                .source
                .raw_request
                .acquisition
                .purchase_ordinal;
            let acquired = ordinal >= purchase;
            if acquired {
                let n = (ordinal - purchase) as usize;
                let expected = &source.native.daily[n.min(source.native.daily.len() - 1)];
                let book = state.books[bi]
                    .as_mut()
                    .ok_or("mortgage missing acquired book")?;
                for (a, b) in book.slots.iter().zip(&expected.slots) {
                    owner::near(a.principal, b.principal)?;
                    owner::near(a.recognized_interest, b.recognized_interest)?;
                    owner::near(a.basis, b.basis)?;
                }
                owner::near(book.cash, expected.cash)?;
                owner::near(book.original_eir_income, expected.original_eir_income)?;
                owner::near(book.reviewed_income, expected.reviewed_income)?;
                if book.claims.len() != expected.claims.len() {
                    return Err("mortgage daily full claim mismatch".into());
                }
                for (a, b) in book.claims.iter_mut().zip(&expected.claims) {
                    if a.claim_id != b.claim_id
                        || a.origin != b.origin
                        || a.birth_ordinal != b.birth_ordinal
                    {
                        return Err("mortgage daily claim identity mismatch".into());
                    }
                    owner::near(a.book.principal, b.book.principal)?;
                    owner::near(a.book.recognized_interest, b.book.recognized_interest)?;
                    owner::near(a.book.basis, b.book.basis)?;
                    a.economic_principal = b.economic_principal;
                    a.economic_contract_interest = b.economic_contract_interest;
                }
                book.borrower_principal = expected.borrower_principal;
                book.borrower_contract_interest = expected.borrower_contract_interest;
                book.ordinal = ordinal;
            } else if state.books[bi].is_some() {
                return Err("mortgage pre-acquisition rights forbidden".into());
            }
            if self.detail {
                self.sink.emit("mortgage_daily",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,"registry_source_id":source.source_id,"acquired":acquired,"memo_checkpoint_source":"native_complete_daily_boundary","native_daily_index":if acquired{Some(((ordinal-purchase)as usize).min(source.native.daily.len()-1))}else{None},"native_daily_ordinal":if acquired{Some(source.native.daily[((ordinal-purchase)as usize).min(source.native.daily.len()-1)].ordinal)}else{None},"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"unweighted_book":state.books[bi],"capital_ftp_eligible":false,"closing_capital_eligible":false}))?;
            }
        }
        for (i, o) in c.position_owners.iter().enumerate() {
            if let Some(o) = o {
                let s = state.summary(c, *o);
                self.positions[i].balance = s[0];
                self.positions[i].book_adjustment = s[1];
                self.accrued[i] = s[2];
            }
        }
        for &bi in &c.run_paths[state.run] {
            if let Some(book) = &state.books[bi] {
                owner::guardian::book(book)?;
            }
        }
        self.mortgage_state = Some(state);
        Ok(())
    }
    pub(super) fn mortgage_expected(
        &mut self,
        expected: &mut std::collections::HashMap<super::Key, f64>,
    ) -> Result<(), String> {
        let Some(c) = self.mortgages else {
            return Ok(());
        };
        let state = self
            .mortgage_state
            .as_ref()
            .ok_or("mortgage state absent")?;
        for &bi in &c.run_paths[state.run] {
            let (o, _, p, _) = c.binding(bi);
            let a = self.aid[o.account_id.as_str()];
            let f = c.bindings[bi].factor;
            let Some(book) = &state.books[bi] else {
                continue;
            };
            for (slot, ch) in book.slots.iter().enumerate() {
                for (account, amount) in [
                    (core::Account::Principal, ch.principal),
                    (core::Account::RecognizedInterest, ch.recognized_interest),
                    (core::Account::Basis, ch.basis),
                ] {
                    if slot < 3 && !matches!(account, core::Account::RecognizedInterest) {
                        continue;
                    }
                    let line = core::Line {
                        account,
                        slot: Some(slot),
                        claim_id: None,
                        amount,
                        source_action_index: None,
                    };
                    let gl = owner::account(&line)?;
                    let id = owner::subject(&o.position_id, &p.path_id, &line);
                    *expected
                        .entry((a, self.sink.intern(gl), self.sink.intern(&id)))
                        .or_default() += amount * f;
                }
            }
            for claim in &book.claims {
                for (account, amount) in [
                    (core::Account::Principal, claim.book.principal),
                    (
                        core::Account::RecognizedInterest,
                        claim.book.recognized_interest,
                    ),
                    (core::Account::Basis, claim.book.basis),
                ] {
                    let line = core::Line {
                        account,
                        slot: None,
                        claim_id: Some(claim.claim_id.clone()),
                        amount,
                        source_action_index: None,
                    };
                    let gl = owner::account(&line)?;
                    let id = owner::subject(&o.position_id, &p.path_id, &line);
                    *expected
                        .entry((a, self.sink.intern(gl), self.sink.intern(&id)))
                        .or_default() += amount * f;
                }
            }
        }
        Ok(())
    }
    pub(super) fn mortgage_closing(&mut self) -> Result<(), String> {
        let Some(c) = self.mortgages else {
            return Ok(());
        };
        let state = self
            .mortgage_state
            .as_ref()
            .ok_or("mortgage closing state absent")?;
        for &bi in &c.run_paths[state.run] {
            let (o, _, p, source) = c.binding(bi);
            let book = state.books[bi]
                .as_ref()
                .ok_or("mortgage fulltail purchase missing")?;
            for ch in &book.slots {
                owner::near(ch.principal, 0.)?;
                owner::near(ch.recognized_interest, 0.)?;
                owner::near(ch.basis, 0.)?;
            }
            for v in book
                .borrower_principal
                .iter()
                .chain(&book.borrower_contract_interest)
            {
                owner::near(*v, 0.)?;
            }
            for claim in &book.claims {
                owner::near(claim.book.principal, 0.)?;
                owner::near(claim.book.recognized_interest, 0.)?;
                owner::near(claim.book.basis, 0.)?;
                owner::near(claim.economic_principal, 0.)?;
                owner::near(claim.economic_contract_interest, 0.)?;
            }
            if self.detail {
                self.sink.emit("mortgage_closing",json!({"scenario":self.scenario.name,"position_id":o.position_id,"path_id":p.path_id,"registry_source_id":source.source_id,"unweighted_book":book,"full_tail_complete":true,"capital_ftp_eligible":false,"closing_capital_eligible":false}))?;
            }
        }
        Ok(())
    }
}
// Runtime is dedicated to seven native slots; existing consumer Runtime is untouched.
const _: Option<Runtime> = None;
