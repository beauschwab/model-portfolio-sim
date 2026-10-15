//! Complete source-authoritative groups enter the single parent Sim::raw_post.
use super::{entry, Key, Sim};
use serde_json::json;
use std::collections::HashMap;
impl Sim<'_> {
    pub(super) fn owned_event_totals(&mut self, a: usize, event: &str, cash: f64, equity: f64) {
        let key = (a, event.to_string());
        let index = if let Some(&i) = self.aggregate_index.get(&key) {
            i
        } else {
            let i = self.aggregate.len();
            self.aggregate.push((a, event.to_string(), [0.; 3]));
            self.aggregate_index.insert(key, i);
            i
        };
        self.aggregate[index].2[0] += cash;
        self.aggregate[index].2[1] += equity;
    }
    pub(super) fn commercial_events(&mut self, day: usize) -> Result<(), String> {
        let Some(c) = self.commercials else {
            return Ok(());
        };
        let mut state = self
            .commercial_state
            .take()
            .ok_or("commercial native state missing")?;
        for event in c.ordered_events(&self.scenario.name, day) {
            portfolio_compute_control::checkpoint()?;
            let (binding, index) = match event {
                crate::owners::commercial::intents::Scheduled::Loan(b, i) => (b, i),
                crate::owners::commercial::intents::Scheduled::Fee(fi, i) => {
                    let f = &c.fees[fi];
                    let b = &c.bindings[f.loan_binding];
                    let pi = c.positions[f.loan_binding];
                    let a = self.aid[self.positions[pi].account.as_str()];
                    let (next, lines, cash, equity) = state.fees[fi].stage(c, fi, i)?;
                    let entries = lines
                        .iter()
                        .map(|(gl, value)| {
                            entry(gl, if *gl == "cash" { "" } else { b.position_id }, *value)
                        })
                        .collect();
                    self.raw_post(day, a, "commercial_nonintegral_fee", entries)?;
                    for (gl, value) in &lines {
                        if gl.starts_with("pnl:") {
                            *self
                                .earnings
                                .entry((a, self.sink.intern(gl), self.sink.intern(b.position_id)))
                                .or_default() += value;
                        }
                    }
                    self.accounts[a].cash += cash;
                    self.accounts[a].equity += equity;
                    self.owned_event_totals(a, "commercial_nonintegral_fee", cash, equity);
                    if !self.accounts[a].cash.is_finite() || !self.accounts[a].equity.is_finite() {
                        return Err("fee pooled account overflow".into());
                    }
                    if self.detail {
                        self.sink.emit("commercial_fee_lineage",json!({"scenario":self.scenario.name,"day":day,"position_id":b.position_id,
                        "path_key":b.path_key,"native_year":f.native_years[i],"source_weight":b.source_weight,"allocation_factor":b.allocated_face/b.native_unit_face,
                        "native_group_index":i,"transaction_number":if lines.is_empty(){None}else{Some(self.sequence)},"native_fee_group":f.source.groups()[i]}))?;
                    }
                    state.fees[fi] = next;
                    continue;
                }
            };
            let b = &c.bindings[binding];
            let pi = c.positions[binding];
            let a = self.aid[self.positions[pi].account.as_str()];
            let group = &b.accounting.groups()[index];
            let stage = state.stage(c, binding, group)?;
            let entries = stage
                .lines
                .iter()
                .map(|l| {
                    entry(
                        l.gl,
                        if l.gl == "cash" { "" } else { b.position_id },
                        l.amount,
                    )
                })
                .collect();
            self.raw_post(day, a, group.phase, entries)?;
            for l in &stage.lines {
                if l.gl.starts_with("pnl:") {
                    *self
                        .earnings
                        .entry((a, self.sink.intern(l.gl), self.sink.intern(b.position_id)))
                        .or_default() += l.amount;
                }
            }
            self.accounts[a].cash += stage.cash;
            self.accounts[a].equity += stage.equity;
            self.owned_event_totals(a, group.phase, stage.cash, stage.equity);
            if !self.accounts[a].cash.is_finite() || !self.accounts[a].equity.is_finite() {
                return Err("commercial pooled account overflow".into());
            }
            if self.detail {
                self.sink.emit("commercial_source_lineage",json!({"scenario":self.scenario.name,"day":day,"native_credit_year":group.year,"source_origin_day":b.origin_day,"source_class":b.source_class.identity(),"global_native_credit_year":b.origin_day as f64/365.+group.year,"date_policy":crate::owners::commercial::intents::DATE_POLICY,"position_id":b.position_id,"path_key":b.path_key,"source_weight":b.source_weight,"allocation_factor":b.allocated_face/b.native_unit_face,"native_group_index":index,"transaction_number":if stage.lines.is_empty(){None}else{Some(self.sequence)},"native_atomic_group":group}))?;
            }
            state.commit(binding, stage);
        }
        self.commercial_state = Some(state);
        Ok(())
    }
    pub(super) fn commercial_expected(
        &mut self,
        expected: &mut HashMap<Key, f64>,
    ) -> Result<(), String> {
        let Some(c) = self.commercials else {
            return Ok(());
        };
        for (pi, gl, value) in self
            .commercial_state
            .as_ref()
            .ok_or("commercial state missing")?
            .values(c, &self.scenario.name)?
        {
            let p = &self.positions[pi];
            let a = self.aid[p.account.as_str()];
            let key = (a, self.sink.intern(gl), self.sink.intern(&p.id));
            *expected.entry(key).or_default() += value;
        }
        Ok(())
    }
    pub(super) fn commercial_gross(&self, a: usize) -> Result<(f64, f64), String> {
        let Some(c) = self.commercials else {
            return Ok((0., 0.));
        };
        let mut assets = 0.;
        let mut liabilities = 0.;
        for (pi, gl, value) in self
            .commercial_state
            .as_ref()
            .ok_or("commercial state missing")?
            .values(c, &self.scenario.name)?
        {
            if self.aid[self.positions[pi].account.as_str()] != a || gl.starts_with("memo:") {
                continue;
            }
            if matches!(
                gl,
                "dated_deferred_income"
                    | "credit_default_deferred_income"
                    | "commercial_fee_payable"
                    | "commercial_deferred_fee_income"
            ) {
                liabilities -= value;
            } else {
                assets += value;
            }
        }
        Ok((assets, liabilities))
    }
    pub(super) fn commercial_snapshots(&mut self, day: usize) -> Result<(), String> {
        let Some(c) = self.commercials else {
            return Ok(());
        };
        if !self.detail {
            return Ok(());
        }
        let state = self
            .commercial_state
            .as_ref()
            .ok_or("commercial state missing")?;
        for (fi, f) in c
            .fees
            .iter()
            .enumerate()
            .filter(|(_, f)| c.bindings[f.loan_binding].scenario == self.scenario.name)
        {
            let b = &c.bindings[f.loan_binding];
            for (fee_id, snapshot) in &state.fees[fi].states {
                let row = json!({"scenario":self.scenario.name,"day":day,"position_id":b.position_id,"path_key":b.path_key,"fee_id":fee_id,
                    "source_weight":b.source_weight,"allocation_factor":b.allocated_face/b.native_unit_face,"native_fee_snapshot":snapshot});
                self.sink.emit("commercial_fee_daily", row.clone())?;
                if day == self.spec.horizon_days {
                    self.sink.emit("commercial_fee_closing", row)?;
                }
            }
        }
        for (binding, snapshot) in state.snapshots(c, &self.scenario.name)? {
            portfolio_compute_control::checkpoint()?;
            let b = &c.bindings[binding];
            let elapsed = day
                .checked_sub(b.origin_day)
                .ok_or("commercial snapshot precedes source acquisition")?;
            if snapshot.year > elapsed as f64 / 365. {
                return Err("commercial daily snapshot contains a future native action".into());
            }
            let row = json!({"scenario":self.scenario.name,"day":day,
                "position_id":b.position_id,"path_key":b.path_key,
                "source_weight":b.source_weight,"allocation_factor":b.allocated_face/b.native_unit_face,
                "source_origin_day":b.origin_day,"source_class":b.source_class.identity(),"unweighted_book":snapshot,"capital_ftp_eligible":false});
            self.sink.emit("commercial_daily", row.clone())?;
            if day == self.spec.horizon_days {
                self.sink.emit("commercial_closing", row)?;
            }
        }
        Ok(())
    }
}
