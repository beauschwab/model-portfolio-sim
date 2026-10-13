//! Borrowed dispatch into the one existing daily state mapper and Sim.
use super::{source, Compiled};
use crate::consumer_intent_owner::{self as consumer, AllowanceReview, Scale};
use portfolio_model_core::{
    card_credit_accounting as card, card_credit_accounting_v2 as card_v2,
    consumer_credit_accounting as installment,
    credit_accounting::{AccountingState, Identity, JournalIntent},
};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireFormat {
    Consumer,
    Product,
}
#[derive(Clone, Copy)]
pub enum OwnerSources<'a> {
    Consumer(&'a consumer::Compiled),
    Product(&'a Compiled),
}
pub struct OwnerView<'a> {
    pub unit_id: &'a str,
    pub position_id: &'a str,
    pub account_id: &'a str,
    pub unit_face: f64,
    pub allocated_face: f64,
    pub accounting_category: &'a str,
    pub source_identity: &'a Identity,
    pub original_yield_anchor: &'a Identity,
    pub initial_allowance_review: Option<&'a AllowanceReview>,
}
pub struct PathView<'a> {
    pub path_id: &'a str,
    pub source_weight: f64,
}
#[derive(Clone, Copy)]
pub enum NativeView<'a> {
    Installment(&'a installment::ResultData),
    Card(&'a card::ResultData),
    CardV2(&'a card_v2::ResultData),
}
impl<'a> NativeView<'a> {
    pub fn raw(self) -> source::View<'a> {
        match self {
            Self::Installment(r) => source::RequestView::installment(&r.raw_request),
            Self::Card(r) => source::RequestView::card(&r.raw_request),
            Self::CardV2(r) => source::RequestView::card_v2(&r.raw_request),
        }
    }
    pub fn opening(self) -> &'a AccountingState {
        match self {
            Self::Installment(r) => &r.acquisition_opening,
            Self::Card(r) => &r.acquisition_opening,
            Self::CardV2(r) => &r.acquisition_opening.core,
        }
    }
    pub fn closing(self) -> &'a AccountingState {
        match self {
            Self::Installment(r) => &r.closing,
            Self::Card(r) => &r.closing,
            Self::CardV2(r) => &r.closing.core,
        }
    }
    pub fn intents(self) -> &'a [JournalIntent] {
        match self {
            Self::Installment(r) => &r.intents,
            Self::Card(r) => &r.intents,
            Self::CardV2(_) => &[],
        }
    }
    pub fn owned(self, day: usize) -> &'a AccountingState {
        match self {
            Self::Installment(r) => &r.daily[day.min(r.daily.len() - 1)].owned,
            Self::Card(r) => &r.daily[day.min(r.daily.len() - 1)].owned,
            Self::CardV2(r) => &r.daily[day.min(r.daily.len() - 1)].snapshot.core,
        }
    }
    pub fn lineage(self, index: usize) -> Value {
        match self {
            Self::Installment(r) => json!(r.lineage[index]),
            Self::Card(r) => json!(r.lineage[index]),
            Self::CardV2(r) => json!(r.actions[index].source),
        }
    }
    /// Exact legacy consumer field. New product rows also retain full lineage.
    pub fn product_event_ids(self, index: usize) -> Value {
        match self {
            Self::Installment(r) => json!(r.lineage[index].product_event_ids),
            Self::Card(r) => json!(r.lineage[index].source_event_ids),
            Self::CardV2(r) => json!(r.actions[index]
                .source
                .cash_event_id
                .iter()
                .collect::<Vec<_>>()),
        }
    }
    pub fn rights_opening(self) -> Option<&'a card_v2::Snapshot> {
        match self {
            Self::CardV2(r) => Some(&r.acquisition_opening),
            _ => None,
        }
    }
    pub fn rights_closing(self) -> Option<&'a card_v2::Snapshot> {
        match self {
            Self::CardV2(r) => Some(&r.closing),
            _ => None,
        }
    }
    pub fn rights_state(self, day: usize) -> Option<&'a card_v2::Snapshot> {
        match self {
            Self::CardV2(r) => Some(&r.daily[day.min(r.daily.len() - 1)].snapshot),
            _ => None,
        }
    }
    pub fn rights_action(self, index: usize) -> Option<&'a card_v2::AtomicAction> {
        match self {
            Self::CardV2(r) => r.actions.get(index),
            _ => None,
        }
    }
    pub fn rights_actions(self) -> Option<&'a [card_v2::AtomicAction]> {
        match self {
            Self::CardV2(r) => Some(&r.actions),
            _ => None,
        }
    }
    pub fn rights_events(self, action: &card_v2::AtomicAction) -> Option<&'a [card_v2::Event]> {
        match self {
            Self::CardV2(r) => {
                owned_action(r, action).and_then(|a| r.events.get(a.events.start..a.events.end))
            }
            _ => None,
        }
    }
    /// Evidence only: never replay these alongside rights_events.
    pub fn rights_shared_intents(
        self,
        action: &card_v2::AtomicAction,
    ) -> Option<&'a [JournalIntent]> {
        match self {
            Self::CardV2(r) => owned_action(r, action).and_then(|a| {
                r.shared_intents
                    .get(a.shared_intents.start..a.shared_intents.end)
            }),
            _ => None,
        }
    }
    pub fn rights_income_steps(
        self,
        action: &card_v2::AtomicAction,
    ) -> Option<&'a [card_v2::IncomeStep]> {
        match self {
            Self::CardV2(r) => owned_action(r, action)
                .and_then(|a| r.income_steps.get(a.income_steps.start..a.income_steps.end)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostingRef {
    Intent { path: usize, index: usize },
    CardV2Action { path: usize, index: usize },
}
pub enum PostingIter<'a> {
    Consumer(std::slice::Iter<'a, (usize, usize)>),
    Product(std::slice::Iter<'a, PostingRef>),
}
impl Iterator for PostingIter<'_> {
    type Item = PostingRef;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Consumer(i) => i
                .next()
                .map(|&(path, index)| PostingRef::Intent { path, index }),
            Self::Product(i) => i.next().copied(),
        }
    }
}
macro_rules! dispatch {
    ($self:ident,$c:ident,$value:expr) => {
        match $self {
            Self::Consumer($c) => $value,
            Self::Product($c) => $value,
        }
    };
}
impl<'a> OwnerSources<'a> {
    pub fn wire_format(self) -> WireFormat {
        match self {
            Self::Consumer(_) => WireFormat::Consumer,
            Self::Product(_) => WireFormat::Product,
        }
    }
    pub fn as_of_ordinal(self) -> i32 {
        dispatch!(self, c, c.as_of_ordinal)
    }
    pub fn source_count(self) -> usize {
        dispatch!(self, c, c.sources.len())
    }
    pub fn position_owners(self) -> &'a [Option<usize>] {
        dispatch!(self, c, c.position_owners.as_slice())
    }
    pub fn run_paths(self, run: usize) -> &'a [usize] {
        dispatch!(self, c, c.run_paths[run].as_slice())
    }
    pub fn owner_paths(self, run: usize, owner: usize) -> &'a [usize] {
        dispatch!(self, c, c.owner_paths(run, owner))
    }
    pub fn acquisitions(self, run: usize, day: usize) -> &'a [usize] {
        dispatch!(self, c, c.acquisitions(run, day))
    }
    pub fn intents(self, run: usize, day: usize) -> &'a [(usize, usize)] {
        dispatch!(self, c, c.intents(run, day))
    }
    pub fn postings(self, run: usize, day: usize) -> PostingIter<'a> {
        match self {
            Self::Consumer(c) => PostingIter::Consumer(c.intents(run, day).iter()),
            Self::Product(c) => PostingIter::Product(c.postings(run, day).iter()),
        }
    }
    pub fn scale(self, path: usize) -> Scale {
        dispatch!(self, c, c.scale(path))
    }
    pub fn path(self, index: usize) -> (OwnerView<'a>, PathView<'a>) {
        dispatch!(self, c, {
            let (o, p) = c.path(index);
            (
                OwnerView {
                    unit_id: &o.unit_id,
                    position_id: &o.position_id,
                    account_id: &o.account_id,
                    unit_face: o.unit_face,
                    allocated_face: o.allocated_face,
                    accounting_category: &o.accounting_category,
                    source_identity: &o.source_identity,
                    original_yield_anchor: &o.original_yield_anchor,
                    initial_allowance_review: o.initial_allowance_review.as_ref(),
                },
                PathView {
                    path_id: &p.path_id,
                    source_weight: p.source_weight,
                },
            )
        })
    }
    pub fn native(self, index: usize) -> NativeView<'a> {
        match self {
            Self::Consumer(c) => NativeView::Installment(&c.sources[index].native),
            Self::Product(c) => match &c.sources[index].native {
                source::Native::Installment(r) => NativeView::Installment(r),
                source::Native::Card(r) => NativeView::Card(r),
                source::Native::CardV2(r) => NativeView::CardV2(r),
            },
        }
    }
    /// Preserves the existing bare SourcePath.native consumer wire shape.
    pub fn emit_source(self, index: usize) -> Value {
        dispatch!(self, c, json!(c.sources[index]))
    }
    /// `day` is relative to the native acquisition; statement lookup uses the
    /// actual requested date, even if owned balances survive beyond source end.
    pub fn daily_receipts(self, index: usize, day: usize) -> Value {
        match self {
            Self::Consumer(_) => Value::Null,
            Self::Product(c) => c.daily_receipts(index, day),
        }
    }
}

fn owned_action<'a>(
    r: &'a card_v2::ResultData,
    action: &card_v2::AtomicAction,
) -> Option<&'a card_v2::AtomicAction> {
    let a = r.actions.get(action.atomic_action.checked_sub(1)?)?;
    std::ptr::eq(a, action).then_some(a)
}
