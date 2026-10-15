//! Optional product-event owner; all transactions enter the parent Sim::raw_post.
use super::{entry, Sim};
use crate::owners::consumer::{InitialAllowanceReview, StagedIntent};
use crate::owners::product_events::{PostingRef, WireFormat};
use serde_json::json;
impl Sim<'_> {
    fn consumer_apply(
        &mut self,
        day: usize,
        path: usize,
        stage: &StagedIntent,
    ) -> Result<(), String> {
        let c = self.consumers.ok_or("consumer source missing")?;
        let (o, _) = c.path(path);
        let a = self.aid[o.account_id];
        let entries = stage
            .lines
            .iter()
            .map(|l| {
                entry(
                    l.gl,
                    if l.gl == "cash" { "" } else { o.position_id },
                    l.amount,
                )
            })
            .collect();
        self.raw_post(day, a, &stage.event, entries)?;
        for l in &stage.lines {
            if l.gl.starts_with("pnl:") {
                *self
                    .earnings
                    .entry((a, self.sink.intern(l.gl), self.sink.intern(o.position_id)))
                    .or_default() += l.amount;
            }
        }
        self.accounts[a].cash += stage.cash_delta;
        self.accounts[a].equity += stage.equity_delta;
        if !self.accounts[a].cash.is_finite() || !self.accounts[a].equity.is_finite() {
            return Err("consumer pooled account overflow".into());
        }
        let key = (a, stage.event.clone());
        let index = if let Some(&i) = self.aggregate_index.get(&key) {
            i
        } else {
            let i = self.aggregate.len();
            self.aggregate.push((a, stage.event.clone(), [0.; 3]));
            self.aggregate_index.insert(key, i);
            i
        };
        self.aggregate[index].2[0] += stage.cash_delta;
        self.aggregate[index].2[1] += stage.equity_delta;
        Ok(())
    }
    pub(super) fn consumer_acquisitions(&mut self, day: usize) -> Result<(), String> {
        let Some(c) = self.consumers else {
            return Ok(());
        };
        let mut state = self.consumer_state.take().ok_or("consumer state missing")?;
        for &index in c.acquisitions(state.run, day) {
            portfolio_compute_control::checkpoint()?;
            let (o, p) = c.path(index);
            let native = c.native(index);
            let raw = native.raw();
            let review = o
                .initial_allowance_review
                .as_ref()
                .map(|r| InitialAllowanceReview {
                    review: r.review.clone(),
                    amount: r.amount,
                });
            let staged = state.paths[index].acquire(
                raw.acquisition.principal,
                raw.acquisition.cost,
                raw.opening_allowance,
                review.as_ref(),
                &c.scale(index),
            )?;
            self.consumer_apply(day, index, &staged.purchase)?;
            let purchase_transaction = if staged.purchase.lines.is_empty() {
                None
            } else {
                Some(self.sequence)
            };
            state.paths[index] = staged.purchase.next;
            let mut allowance_transaction = None;
            if let Some((_review, intent)) = staged.reviewed_initial_allowance {
                self.consumer_apply(day, index, &intent)?;
                if !intent.lines.is_empty() {
                    allowance_transaction = Some(self.sequence);
                }
                state.paths[index] = intent.next;
            }
            state.paths[index].reconcile(native.opening())?;
            if let Some(rights) = native.rights_opening() {
                state.paths[index].start_rights(rights)?;
                if self.detail {
                    let run = &self
                        .dated
                        .ok_or("product rights acquisition run missing")?
                        .store
                        .runs()[state.run];
                    self.sink.emit("product_rights_acquisition_lineage",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":c.as_of_ordinal()+day as i32,"position_id":o.position_id,"path_id":p.path_id,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"economic_source":o.source_identity,"acquisition":raw.acquisition,"original_yield_anchor":o.original_yield_anchor,"purchase_transaction_number":purchase_transaction,"unweighted_rights_opening":rights}))?;
                }
            }
            if self.detail {
                let run = &self
                    .dated
                    .ok_or("consumer acquisition run missing")?
                    .store
                    .runs()[state.run];
                let table = match c.wire_format() {
                    WireFormat::Consumer => "consumer_acquisition_lineage",
                    WireFormat::Product => "product_acquisition_lineage",
                };
                self.sink.emit(table,json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":c.as_of_ordinal()+day as i32,"position_id":o.position_id,"path_id":p.path_id,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"economic_source":o.source_identity,"acquisition":raw.acquisition,"original_yield_anchor":o.original_yield_anchor,"initial_allowance_review":o.initial_allowance_review,"purchase_transaction_number":purchase_transaction,"allowance_transaction_number":allowance_transaction}))?;
            }
        }
        self.consumer_state = Some(state);
        Ok(())
    }
    pub(super) fn consumer_events(&mut self, day: usize) -> Result<(), String> {
        let Some(c) = self.consumers else {
            return Ok(());
        };
        let mut state = self.consumer_state.take().ok_or("consumer state missing")?;
        let run = &self.dated.ok_or("consumer dated run missing")?.store.runs()[state.run];
        let ordinal = c.as_of_ordinal() + day as i32;
        for posting in c.postings(state.run, day) {
            portfolio_compute_control::checkpoint()?;
            if let PostingRef::CardV2Action { path, index } = posting {
                let source = c.native(path);
                let action = source
                    .rights_action(index)
                    .ok_or("native rights action missing")?;
                let events = source
                    .rights_events(action)
                    .ok_or("native complete rights event range missing")?;
                let (o, p) = c.path(path);
                let staged = state.paths[path].stage_rights(action, events, &c.scale(path))?;
                self.consumer_apply(day, path, &staged)?;
                let transaction = if staged.lines.is_empty() {
                    None
                } else {
                    Some(self.sequence)
                };
                state.paths[path] = staged.next;
                if self.detail {
                    self.sink.emit("product_rights_lineage",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"action_index":index,"transaction_number":transaction,"original_yield_anchor":o.original_yield_anchor,"native_action":action,"complete_events":events,"shared_intents_evidence_only":source.rights_shared_intents(action).ok_or("native shared intent evidence range missing")?,"native_income_steps":source.rights_income_steps(action).ok_or("native rights income range missing")?}))?;
                }
                continue;
            }
            let PostingRef::Intent { path, index } = posting else {
                unreachable!()
            };
            let source = c.native(path);
            let intent = &source.intents()[index];
            let (o, p) = c.path(path);
            let staged = state.paths[path].stage(intent, &c.scale(path))?;
            self.consumer_apply(day, path, &staged)?;
            let transaction = if staged.lines.is_empty() {
                None
            } else {
                Some(self.sequence)
            };
            state.paths[path] = staged.next;
            if self.detail {
                match c.wire_format() {
                WireFormat::Consumer=>self.sink.emit("consumer_lineage",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,
                "source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"intent_index":index,"transaction_number":transaction,"claim_id":intent.claim_id,"policy":intent.policy,"source":intent.source,
                "original_yield_anchor":o.original_yield_anchor,"product_event_ids":source.product_event_ids(index)}))?,
                WireFormat::Product=>self.sink.emit("product_lineage",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,
                "source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"intent_index":index,"transaction_number":transaction,"claim_id":intent.claim_id,"policy":intent.policy,"source":intent.source,
                "original_yield_anchor":o.original_yield_anchor,"native_lineage":source.lineage(index)}))?,
                }
            }
        }
        for &path in c.run_paths(state.run) {
            portfolio_compute_control::checkpoint()?;
            let (o, p) = c.path(path);
            let native = c.native(path);
            let raw = native.raw();
            let acquired = ordinal >= raw.acquisition.purchase_ordinal;
            let snapshot = if acquired {
                let n = (ordinal - raw.acquisition.purchase_ordinal) as usize;
                Some(native.owned(n))
            } else {
                None
            };
            let empty = portfolio_model_core::credit_accounting::AccountingState {
                live: [portfolio_model_core::credit_accounting::LiveState {
                    principal: 0.,
                    undrawn: 0.,
                    accrued_interest: 0.,
                    book_adjustment: 0.,
                }; 3],
                allowance: 0.,
                claims: vec![],
            };
            let snapshot = snapshot.unwrap_or(&empty);
            state.paths[path].reconcile(snapshot)?;
            if acquired {
                let n = (ordinal - raw.acquisition.purchase_ordinal) as usize;
                if let Some(rights) = native.rights_state(n) {
                    state.paths[path].reconcile_rights(rights)?;
                    if self.detail {
                        self.sink.emit("product_rights_daily",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"unweighted_rights":rights,"gross_rights_and_signed_basis_separate":true,"capital_ftp_eligible":false}))?;
                    }
                }
            }
            if self.detail {
                match c.wire_format() {
                WireFormat::Consumer=>self.sink.emit("consumer_daily",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,"acquired":acquired,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"unweighted_owned":snapshot}))?,
                WireFormat::Product=>self.sink.emit("product_daily",json!({"scenario":self.scenario.name,"run_id":run.run_id,"ordinal":ordinal,"position_id":o.position_id,"path_id":p.path_id,"acquired":acquired,"source_weight":p.source_weight,"allocation_factor":o.allocated_face/o.unit_face,"unweighted_owned":snapshot,"product_receipts":if acquired {c.daily_receipts(path,(ordinal-raw.acquisition.purchase_ordinal) as usize)}else{json!(null)}}))?,
                }
            }
        }
        for (i, owner) in c.position_owners().iter().enumerate() {
            if let Some(owner) = owner {
                let s = state.owner_summary(c, *owner)?;
                self.positions[i].balance = s[0];
                self.positions[i].book_adjustment = s[1];
                self.positions[i].allowance = s[2];
                self.accrued[i] = s[3] + s[4];
            }
        }
        self.consumer_state = Some(state);
        Ok(())
    }
}
