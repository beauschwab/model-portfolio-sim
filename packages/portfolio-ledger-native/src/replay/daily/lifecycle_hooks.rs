//! Exact lifecycle acquisition intent adoption: one financial owner per event, custody memo
//! separate, gross signed stocks retained. Mirrors the derivative owner's seams.
use super::*;
use crate::owners::lifecycle::Kind;
impl Sim<'_> {
    pub(super) fn lifecycle_opening(&self, account: usize) -> R<Changes> {
        let (Some(compiled), Some(runtime)) = (self.lifecycle, self.lifecycle_state.as_ref())
        else {
            return Ok(vec![]);
        };
        let mut entries = Vec::new();
        for state in &runtime.states {
            if compiled.accounts[state.source] != account {
                continue;
            }
            for line in &compiled.sources[state.source].opening {
                if line.kind.stock() {
                    entries.push(entry(line.kind.gl(), &line.instrument, line.debit));
                }
            }
        }
        Ok(entries)
    }
    pub(super) fn lifecycle_events(&mut self, day: usize) -> R<()> {
        let Some(compiled) = self.lifecycle else {
            return Ok(());
        };
        let mut runtime = self
            .lifecycle_state
            .take()
            .ok_or("lifecycle runtime absent")?;
        for state in &mut runtime.states {
            portfolio_compute_control::checkpoint()?;
            let source = &compiled.sources[state.source];
            let account = compiled.accounts[state.source];
            let ordinal = source.asof + day as i32;
            while let Some(intent) = source.intents.get(state.cursor) {
                if intent.ordinal > ordinal {
                    break;
                }
                if intent.ordinal != ordinal {
                    return Err("skipped lifecycle intent date".into());
                }
                let mut entries = Vec::new();
                let (mut cash, mut earnings) = (0., 0.);
                for line in &intent.lines {
                    *state
                        .book
                        .entry((line.kind, line.instrument.clone()))
                        .or_default() += line.debit;
                    if intent.memo {
                        continue;
                    }
                    entries.push(entry(line.kind.gl(), &line.instrument, line.debit));
                    if line.kind == Kind::Cash {
                        cash += line.debit;
                    }
                    if line.kind.pnl() {
                        earnings -= line.debit;
                        *self
                            .earnings
                            .entry((
                                account,
                                self.sink.intern(line.kind.gl()),
                                self.sink.intern(&line.instrument),
                            ))
                            .or_default() += line.debit;
                    }
                }
                if !intent.memo {
                    self.raw_post(day, account, "native_lifecycle_intent", entries)?;
                    self.accounts[account].cash += cash;
                    self.accounts[account].equity += earnings;
                    let key = (account, "native_lifecycle_intent".to_owned());
                    let index = if let Some(&index) = self.aggregate_index.get(&key) {
                        index
                    } else {
                        let index = self.aggregate.len();
                        self.aggregate
                            .push((account, "native_lifecycle_intent".into(), [0.; 3]));
                        self.aggregate_index.insert(key, index);
                        index
                    };
                    self.aggregate[index].2[0] += cash;
                    self.aggregate[index].2[1] += earnings;
                }
                if self.detail {
                    for (index, line) in intent.lines.iter().enumerate() {
                        self.sink.emit(if intent.memo {"lifecycle_custody_journal"} else {"lifecycle_journal_lineage"},json!({
                            "scenario":self.scenario.name,"day":day,"account":source.account_id,
                            "plan_key":source.plan_key,"source_event_id":intent.event_id,
                            "source_intent_index":state.cursor,"source_posting_index":index,"ordinal":intent.ordinal,
                            "gl_account":line.kind.gl(),"instrument_id":line.instrument,"debit_amount":line.debit}))?;
                    }
                }
                state.cursor += 1;
            }
            state.reconcile(source, day.min(source.daily.len() - 1))?;
        }
        self.lifecycle_state = Some(runtime);
        Ok(())
    }
    pub(super) fn lifecycle_expected(&mut self, expected: &mut HashMap<Key, f64>) -> R<()> {
        let (Some(compiled), Some(runtime)) = (self.lifecycle, self.lifecycle_state.as_ref())
        else {
            return Ok(());
        };
        for state in &runtime.states {
            let account = compiled.accounts[state.source];
            for ((kind, id), value) in &state.book {
                if kind.stock() {
                    expected.insert(
                        (account, self.sink.intern(kind.gl()), self.sink.intern(id)),
                        *value,
                    );
                }
            }
        }
        Ok(())
    }
    /// Gross lifecycle stocks for the balance-sheet identity; cash is already in the account.
    pub(super) fn lifecycle_gross(&self, account: usize) -> R<(f64, f64)> {
        let (Some(compiled), Some(runtime)) = (self.lifecycle, self.lifecycle_state.as_ref())
        else {
            return Ok((0., 0.));
        };
        let (mut assets, mut liabilities) = (0., 0.);
        for state in &runtime.states {
            if compiled.accounts[state.source] != account {
                continue;
            }
            for ((kind, _), value) in &state.book {
                if !kind.stock() {
                    continue;
                }
                if kind.liability() {
                    liabilities -= value;
                } else {
                    assets += value;
                }
            }
        }
        if assets.is_finite() && liabilities.is_finite() {
            Ok((assets, liabilities))
        } else {
            Err("lifecycle gross stock overflow".into())
        }
    }
}
