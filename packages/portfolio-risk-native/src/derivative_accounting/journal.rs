//! Signed-debit balanced intent writer with independent state/GL reconciliation.
use super::types::*;
use crate::derivative_contract::{amount, near, Identity};
use std::collections::BTreeMap;
#[derive(Clone)]
pub(super) struct TradeState {
    pub owner: String,
    pub bucket: String,
    pub value: f64,
}
pub(super) struct Engine<'a> {
    pub request: &'a DerivativeAccountingRequest,
    pub cash: f64,
    pub trades: BTreeMap<String, TradeState>,
    pub unpaid: BTreeMap<(String, String, i32), f64>,
    pub vm: BTreeMap<String, f64>,
    pub posted_im: BTreeMap<String, f64>,
    pub received_im: BTreeMap<String, f64>,
    pub financial: BTreeMap<AccountCode, f64>,
    pub memo: BTreeMap<AccountCode, f64>,
    pub intents: Vec<JournalIntent>,
}
fn posting(account: AccountCode, value: f64, owner: Option<&str>, bucket: Option<&str>) -> Posting {
    Posting {
        account,
        debit_amount: value,
        position_id: owner.map(str::to_string),
        bucket: bucket.map(str::to_string),
    }
}
pub(super) fn book_changes(old: f64, new: f64, owner: &str, bucket: &str) -> Vec<Posting> {
    vec![
        posting(
            AccountCode::DerivativeAsset,
            new.max(0.) - old.max(0.),
            Some(owner),
            Some(bucket),
        ),
        posting(
            AccountCode::DerivativeLiability,
            -(-new).max(0.) + (-old).max(0.),
            Some(owner),
            Some(bucket),
        ),
    ]
}
fn unpaid_changes(value: f64, owner: &str, bucket: &str) -> Vec<Posting> {
    vec![posting(
        if value >= 0. {
            AccountCode::UnpaidReceivable
        } else {
            AccountCode::UnpaidPayable
        },
        value,
        Some(owner),
        Some(bucket),
    )]
}
impl<'a> Engine<'a> {
    pub fn new(
        request: &'a DerivativeAccountingRequest,
        trades: BTreeMap<String, TradeState>,
        buckets: &[String],
    ) -> Result<Self, String> {
        let o = &request.opening;
        let mut financial = BTreeMap::new();
        let mut memo = BTreeMap::new();
        for (a, v) in [
            (AccountCode::Cash, o.cash),
            (AccountCode::PostedVmAsset, o.posted_vm_asset),
            (AccountCode::ReceivedVmLiability, -o.received_vm_liability),
            (AccountCode::PostedImAsset, o.posted_im_asset),
            (AccountCode::OpeningEquity, -o.equity_credit_balance),
        ] {
            financial.insert(a, amount(v)?);
        }
        for t in trades.values() {
            *financial.entry(AccountCode::DerivativeAsset).or_default() += t.value.max(0.);
            *financial
                .entry(AccountCode::DerivativeLiability)
                .or_default() -= (-t.value).max(0.);
        }
        for value in financial.values() {
            amount(*value)?;
        }
        memo.insert(AccountCode::MemoCustodianAsset, o.received_im_custody);
        memo.insert(AccountCode::MemoCustodianObligation, -o.received_im_custody);
        if !near(financial.values().sum(), 0.) || !near(memo.values().sum(), 0.) {
            return Err("opening derivative trial balance does not reconcile explicit equity/cash/rights/collateral anchors".into());
        }
        let mut vm = BTreeMap::new();
        let mut posted_im = BTreeMap::new();
        let mut received_im = BTreeMap::new();
        for b in buckets {
            vm.insert(
                b.clone(),
                if buckets.len() == 1 {
                    o.received_vm_liability - o.posted_vm_asset
                } else {
                    0.
                },
            );
            posted_im.insert(
                b.clone(),
                if buckets.len() == 1 {
                    o.posted_im_asset
                } else {
                    0.
                },
            );
            received_im.insert(
                b.clone(),
                if buckets.len() == 1 {
                    o.received_im_custody
                } else {
                    0.
                },
            );
        }
        Ok(Self {
            request,
            cash: o.cash,
            trades,
            unpaid: BTreeMap::new(),
            vm,
            posted_im,
            received_im,
            financial,
            memo,
            intents: Vec::new(),
        })
    }
    #[allow(clippy::too_many_arguments)] // Typed event lineage plus signed-debit postings.
    pub fn emit(
        &mut self,
        kind: &str,
        date: i32,
        ledger: JournalLedger,
        mapped: Option<&str>,
        owner: Option<&str>,
        bucket: Option<&str>,
        sources: Vec<Identity>,
        mut postings: Vec<Posting>,
    ) -> Result<(), String> {
        postings.retain(|p| p.debit_amount != 0.);
        if postings.is_empty() {
            return Ok(());
        }
        let sum = postings
            .iter()
            .try_fold(0., |a, p| amount(a + amount(p.debit_amount)?))?;
        if !near(sum, 0.) {
            return Err(format!("unbalanced native derivative intent {kind}: {sum}"));
        }
        if self.intents.len() >= 250000 {
            return Err("derivative journal output exceeds 250000 intents".into());
        }
        let balances = if ledger == JournalLedger::Financial {
            &mut self.financial
        } else {
            &mut self.memo
        };
        for p in &postings {
            let value = balances.entry(p.account).or_default();
            *value = amount(*value + p.debit_amount)?;
        }
        self.intents.push(JournalIntent {
            event_id: format!("{kind}:{}", self.intents.len()),
            ordinal: date,
            ledger,
            run: self.request.run.clone(),
            scenario_id: self.request.scenario_id.clone(),
            entity: self.request.entity.clone(),
            currency: self.request.currency.clone(),
            policy: self.request.policy.identity.clone(),
            sources,
            mapped_position_id: mapped.map(str::to_string),
            contract_position_id: owner.map(str::to_string),
            bucket: bucket.map(str::to_string),
            measure: self.request.measure,
            basis: self.request.basis,
            postings,
        });
        Ok(())
    }
    pub fn mark(
        &mut self,
        id: &str,
        target: f64,
        date: i32,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let t = self
            .trades
            .get(id)
            .ok_or("unmapped derivative carrying slot")?
            .clone();
        let target = amount(target)?;
        let mut postings = book_changes(t.value, target, &t.owner, &t.bucket);
        postings.push(posting(
            AccountCode::MarkGainLoss,
            -(target - t.value),
            Some(&t.owner),
            Some(&t.bucket),
        ));
        self.emit(
            "clean_mark",
            date,
            JournalLedger::Financial,
            Some(id),
            Some(&t.owner),
            Some(&t.bucket),
            sources,
            postings,
        )?;
        self.trades.get_mut(id).ok_or("missing trade")?.value = target;
        Ok(())
    }
    pub fn transfer_owner(
        &mut self,
        id: &str,
        new_owner: &str,
        date: i32,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let t = self.trades.get(id).ok_or("missing delivery slot")?.clone();
        if t.owner == new_owner {
            return Ok(());
        }
        let mut postings = book_changes(t.value, 0., &t.owner, &t.bucket);
        postings.extend(book_changes(0., t.value, new_owner, &t.bucket));
        self.emit(
            "physical_delivery",
            date,
            JournalLedger::Financial,
            Some(id),
            Some(new_owner),
            Some(&t.bucket),
            sources,
            postings,
        )?;
        self.trades
            .get_mut(id)
            .ok_or("missing delivery slot")?
            .owner = new_owner.into();
        Ok(())
    }
    pub fn due(
        &mut self,
        right: &CashRight,
        unpaid: bool,
        review: &TradeReview,
        date: i32,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let t = self
            .trades
            .get(&right.position_id)
            .ok_or("cash right absent carrying slot")?
            .clone();
        let mut postings = Vec::new();
        if unpaid {
            let key = (
                right.position_id.clone(),
                right.event_id.clone(),
                right.payment_ordinal,
            );
            if self.unpaid.insert(key, right.physical_amount).is_some() {
                return Err("contractual cash right recognized twice".into());
            }
            postings.extend(unpaid_changes(right.physical_amount, &t.owner, &t.bucket));
        } else {
            self.cash = amount(self.cash + right.physical_amount)?;
            postings.push(posting(
                AccountCode::Cash,
                right.physical_amount,
                Some(&t.owner),
                Some(&t.bucket),
            ));
        }
        if review.coupon_recognition == CouponRecognition::SettleDerivativeCarrying {
            let value = amount(t.value - right.physical_amount)?;
            postings.extend(book_changes(t.value, value, &t.owner, &t.bucket));
            self.trades
                .get_mut(&right.position_id)
                .ok_or("missing carrying slot")?
                .value = value;
        } else {
            postings.push(posting(
                AccountCode::ContractualIncome,
                -right.physical_amount,
                Some(&t.owner),
                Some(&t.bucket),
            ));
        }
        self.emit(
            if unpaid {
                "contractual_due_unpaid"
            } else {
                "contractual_cash_settlement"
            },
            date,
            JournalLedger::Financial,
            Some(&right.position_id),
            Some(&t.owner),
            Some(&t.bucket),
            sources,
            postings,
        )
    }
    pub fn settle_unpaid(
        &mut self,
        right: &CashRight,
        date: i32,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let key = (
            right.position_id.clone(),
            right.event_id.clone(),
            right.payment_ordinal,
        );
        let value = self
            .unpaid
            .remove(&key)
            .ok_or("settlement lacks retained unpaid right")?;
        if !near(value, right.physical_amount) {
            return Err("retained cash right changed before settlement".into());
        }
        let t = self
            .trades
            .get(&right.position_id)
            .ok_or("missing settlement owner")?
            .clone();
        let mut postings = unpaid_changes(-value, &t.owner, &t.bucket);
        // Removing a receivable/payable uses its original gross account, not the sign of the reversal.
        for p in &mut postings {
            p.account = if value >= 0. {
                AccountCode::UnpaidReceivable
            } else {
                AccountCode::UnpaidPayable
            };
        }
        self.cash = amount(self.cash + value)?;
        postings.push(posting(
            AccountCode::Cash,
            value,
            Some(&t.owner),
            Some(&t.bucket),
        ));
        self.emit(
            "mpor_cash_settlement",
            date,
            JournalLedger::Financial,
            Some(&right.position_id),
            Some(&t.owner),
            Some(&t.bucket),
            sources,
            postings,
        )
    }
    #[allow(clippy::too_many_arguments)] // Native audited stock/cash movement, with source identity.
    pub fn collateral(
        &mut self,
        kind: &str,
        date: i32,
        bucket: &str,
        cash: f64,
        vm_delta: f64,
        posted_delta: f64,
        received_delta: f64,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let old = *self.vm.get(bucket).ok_or("missing collateral bucket")?;
        let new = amount(old + vm_delta)?;
        let mut postings = vec![
            posting(AccountCode::Cash, cash, None, Some(bucket)),
            posting(
                AccountCode::PostedVmAsset,
                (-new).max(0.) - (-old).max(0.),
                None,
                Some(bucket),
            ),
            posting(
                AccountCode::ReceivedVmLiability,
                -new.max(0.) + old.max(0.),
                None,
                Some(bucket),
            ),
            posting(AccountCode::PostedImAsset, posted_delta, None, Some(bucket)),
        ];
        if kind == "collateral_interest" {
            postings.push(posting(
                AccountCode::CollateralInterestIncome,
                -cash,
                None,
                Some(bucket),
            ));
        }
        self.emit(
            kind,
            date,
            JournalLedger::Financial,
            None,
            None,
            Some(bucket),
            sources.clone(),
            postings,
        )?;
        self.cash = amount(self.cash + cash)?;
        self.vm.insert(bucket.into(), new);
        let posted = self
            .posted_im
            .get_mut(bucket)
            .ok_or("missing posted IM bucket")?;
        *posted = amount(*posted + posted_delta)?;
        self.custody(date, bucket, received_delta, sources)
    }
    pub fn custody(
        &mut self,
        date: i32,
        bucket: &str,
        delta: f64,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let balance = self
            .received_im
            .get_mut(bucket)
            .ok_or("missing custody bucket")?;
        *balance = amount(*balance + delta)?;
        if delta != 0. {
            self.emit(
                "custodian_principal",
                date,
                JournalLedger::CustodyMemo,
                None,
                None,
                Some(bucket),
                sources,
                vec![
                    posting(AccountCode::MemoCustodianAsset, delta, None, Some(bucket)),
                    posting(
                        AccountCode::MemoCustodianObligation,
                        -delta,
                        None,
                        Some(bucket),
                    ),
                ],
            )?;
        }
        Ok(())
    }
    pub fn closeout(
        &mut self,
        close: &crate::derivative_contract::csa::Closeout,
        date: i32,
        sources: Vec<Identity>,
    ) -> Result<(), String> {
        let bucket = &close.bucket;
        let mut postings = vec![
            posting(AccountCode::Cash, close.entity_cash, None, Some(bucket)),
            posting(
                AccountCode::CounterpartyLossExpense,
                close.counterparty_loss,
                None,
                Some(bucket),
            ),
            posting(
                AccountCode::PostedCollateralReturnLossExpense,
                close.posted_return_loss,
                None,
                Some(bucket),
            ),
        ];
        let mut total = 0.;
        for t in self.trades.values_mut().filter(|t| &t.bucket == bucket) {
            total = amount(total + t.value)?;
            postings.extend(book_changes(t.value, 0., &t.owner, bucket));
            t.value = 0.;
        }
        let keys = self
            .unpaid
            .keys()
            .filter(|(id, _, _)| self.trades.get(id).is_some_and(|t| &t.bucket == bucket))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let value = self
                .unpaid
                .remove(&key)
                .ok_or("missing closeout obligation")?;
            total = amount(total + value)?;
            let owner = &self
                .trades
                .get(&key.0)
                .ok_or("missing obligation owner")?
                .owner;
            postings.push(posting(
                if value >= 0. {
                    AccountCode::UnpaidReceivable
                } else {
                    AccountCode::UnpaidPayable
                },
                -value,
                Some(owner),
                Some(bucket),
            ));
        }
        if !near(total, close.total_closeout_claim) {
            return Err(
                "accounting rights do not reconcile native replacement plus unpaid closeout claim"
                    .into(),
            );
        }
        let vm = *self.vm.get(bucket).ok_or("missing VM")?;
        let pim = *self.posted_im.get(bucket).ok_or("missing posted IM")?;
        let rim = *self.received_im.get(bucket).ok_or("missing received IM")?;
        postings.push(posting(
            AccountCode::PostedVmAsset,
            -(-vm).max(0.),
            None,
            Some(bucket),
        ));
        postings.push(posting(
            AccountCode::ReceivedVmLiability,
            vm.max(0.),
            None,
            Some(bucket),
        ));
        postings.push(posting(
            AccountCode::PostedImAsset,
            -pim,
            None,
            Some(bucket),
        ));
        self.emit(
            "reviewed_default_closeout",
            date,
            JournalLedger::Financial,
            None,
            None,
            Some(bucket),
            sources.clone(),
            postings,
        )?;
        self.cash = amount(self.cash + close.entity_cash)?;
        self.vm.insert(bucket.clone(), 0.);
        self.posted_im.insert(bucket.clone(), 0.);
        self.custody(date, bucket, -rim, sources)
    }
    pub fn closing(&self, date: i32) -> Result<DailyClosing, String> {
        let asset = self.trades.values().map(|t| t.value.max(0.)).sum::<f64>();
        let liability = self
            .trades
            .values()
            .map(|t| -(-t.value).max(0.))
            .sum::<f64>();
        let unpaid_receivable = self.unpaid.values().map(|v| v.max(0.)).sum::<f64>();
        let unpaid_payable = self.unpaid.values().map(|v| (-v).max(0.)).sum::<f64>();
        let posted_vm_asset = self.vm.values().map(|v| (-v).max(0.)).sum::<f64>();
        let received_vm_liability = self.vm.values().map(|v| v.max(0.)).sum::<f64>();
        let posted_im_asset = self.posted_im.values().sum::<f64>();
        let received_im_custody = self.received_im.values().sum::<f64>();
        for (account, value) in [
            (AccountCode::Cash, self.cash),
            (AccountCode::DerivativeAsset, asset),
            (AccountCode::DerivativeLiability, liability),
            (AccountCode::UnpaidReceivable, unpaid_receivable),
            (AccountCode::UnpaidPayable, -unpaid_payable),
            (AccountCode::PostedVmAsset, posted_vm_asset),
            (AccountCode::ReceivedVmLiability, -received_vm_liability),
            (AccountCode::PostedImAsset, posted_im_asset),
        ] {
            if !near(self.financial.get(&account).copied().unwrap_or(0.), value) {
                return Err(format!(
                    "closing derivative subledger/GL mismatch {account:?}"
                ));
            }
        }
        if !near(self.financial.values().sum(), 0.)
            || !near(self.memo.values().sum(), 0.)
            || !near(
                self.memo
                    .get(&AccountCode::MemoCustodianAsset)
                    .copied()
                    .unwrap_or(0.),
                received_im_custody,
            )
            || !near(
                self.memo
                    .get(&AccountCode::MemoCustodianObligation)
                    .copied()
                    .unwrap_or(0.),
                -received_im_custody,
            )
        {
            return Err("closing financial/custody trial balance failed".into());
        }
        Ok(DailyClosing {
            ordinal: date,
            cash: self.cash,
            trades: self
                .trades
                .iter()
                .map(|(id, t)| ClosingTrade {
                    mapped_position_id: id.clone(),
                    contract_position_id: t.owner.clone(),
                    clean_carrying: t.value,
                })
                .collect(),
            unpaid_receivable,
            unpaid_payable,
            posted_vm_asset,
            received_vm_liability,
            posted_im_asset,
            received_im_custody,
            financial_trial_balance: self
                .financial
                .iter()
                .map(|(a, v)| AccountBalance {
                    account: *a,
                    debit_balance: *v,
                })
                .collect(),
            custody_trial_balance: self
                .memo
                .iter()
                .map(|(a, v)| AccountBalance {
                    account: *a,
                    debit_balance: *v,
                })
                .collect(),
        })
    }
}
