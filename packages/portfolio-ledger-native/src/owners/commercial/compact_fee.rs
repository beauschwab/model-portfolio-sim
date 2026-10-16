//! Sealed compact atomic fee postings and absolute gross stock endpoints.
//! Only a genuine native fee owner can construct this source; no Deserialize.
use super::intents::FeeSource;
use portfolio_model_core::{
    cashflow::CashflowMeasure, credit_accounting::Identity, nonintegral_fee as fee,
};
use std::{borrow::Cow, collections::BTreeMap, mem::size_of};
struct Group {
    ids: [usize; 8],
    ordinal: i32,
    sequence: u32,
    action: fee::Action,
    reason: fee::Reason,
    amount: f64,
    postings: Vec<fee::Posting>,
    next: fee::State,
}
pub struct CompactFeeSource {
    dictionary: Vec<String>,
    groups: Vec<Group>,
    position_id: String,
    asof: i32,
    fee_count: usize,
    retained_bytes: usize,
}
impl CompactFeeSource {
    pub fn from_native_bounded(source: &fee::ResultData, remaining: usize) -> Result<Self, String> {
        portfolio_compute_control::checkpoint()?;
        let raw = source.raw_request();
        if remaining > 32 << 20
            || raw.measure != CashflowMeasure::Physical
            || raw.fees.is_empty()
            || raw
                .fees
                .iter()
                .any(|f| f.position_id != raw.fees[0].position_id)
        {
            return Err(
                "compact fee requires one genuine Physical native position and remaining32MiB"
                    .into(),
            );
        }
        let mut upper = size_of::<Self>();
        let mut previous = None;
        for g in source.groups() {
            portfolio_compute_control::checkpoint()?;
            let clock = (g.event.ordinal, g.event.sequence);
            if previous.is_some_and(|p| p >= clock)
                || g.event.ordinal < raw.asof_ordinal
                || g.event.ordinal > raw.complete_end_ordinal
            {
                return Err("compact fee native group chronology differs".into());
            }
            previous = Some(clock);
            upper = upper
                .checked_add(
                    portfolio_model_core::strict_json::serialized_size(g, 32 << 20)?
                        .checked_mul(4)
                        .and_then(|v| v.checked_add(1024))
                        .ok_or("compact fee capacity overflow")?,
                )
                .ok_or("compact fee capacity overflow")?;
            if upper > remaining {
                return Err("compact fee source exceeds remaining32MiB before copying".into());
            }
        }
        let mut dictionary = Vec::new();
        let mut index = BTreeMap::new();
        let mut intern = |s: &str| {
            if let Some(&i) = index.get(s) {
                i
            } else {
                let i = dictionary.len();
                dictionary.push(s.to_string());
                index.insert(s.to_string(), i);
                i
            }
        };
        let mut groups = Vec::with_capacity(source.groups().len());
        for g in source.groups() {
            portfolio_compute_control::checkpoint()?;
            groups.push(Group {
                ids: [
                    intern(&g.event.event_id),
                    intern(&g.event.fee_id),
                    intern(&g.event.source.id),
                    intern(&g.event.source.revision),
                    intern(&g.event.review.id),
                    intern(&g.event.review.revision),
                    intern(&g.policy.id),
                    intern(&g.policy.revision),
                ],
                ordinal: g.event.ordinal,
                sequence: g.event.sequence,
                action: g.event.action,
                reason: g.event.reason,
                amount: g.event.amount,
                postings: g.postings.clone(),
                next: g.next.clone(),
            });
        }
        drop(index);
        let bytes = groups.iter().try_fold(
            size_of::<Self>()
                + groups.capacity() * size_of::<Group>()
                + dictionary.capacity() * size_of::<String>()
                + dictionary.iter().map(|s| s.capacity() * 2).sum::<usize>()
                + raw.fees[0].position_id.len() * 2,
            |sum, g| {
                sum.checked_add(g.postings.capacity() * size_of::<fee::Posting>())
                    .ok_or("compact fee capacity overflow")
            },
        )?;
        if bytes > remaining || bytes > upper {
            return Err("compact fee exceeds reserved32MiB".into());
        }
        Ok(Self {
            dictionary,
            groups,
            position_id: raw.fees[0].position_id.clone(),
            asof: raw.asof_ordinal,
            fee_count: raw.fees.len(),
            retained_bytes: bytes,
        })
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}
impl super::intents::Sealed for CompactFeeSource {}
impl FeeSource for CompactFeeSource {
    fn measure(&self) -> CashflowMeasure {
        CashflowMeasure::Physical
    }
    fn asof(&self) -> i32 {
        self.asof
    }
    fn fee_count(&self) -> usize {
        self.fee_count
    }
    fn positions_match(&self, position: &str) -> bool {
        self.position_id == position
    }
    fn group_count(&self) -> usize {
        self.groups.len()
    }
    fn compact_bytes(&self) -> Option<usize> {
        Some(self.retained_bytes)
    }
    fn group(&self, index: usize) -> Cow<'_, fee::Group> {
        let g = &self.groups[index];
        let text = |i: usize| self.dictionary[g.ids[i]].clone();
        Cow::Owned(fee::Group {
            event: fee::Event {
                event_id: text(0),
                fee_id: text(1),
                source: Identity {
                    id: text(2),
                    revision: text(3),
                },
                review: Identity {
                    id: text(4),
                    revision: text(5),
                },
                ordinal: g.ordinal,
                sequence: g.sequence,
                action: g.action,
                reason: g.reason,
                amount: g.amount,
            },
            policy: Identity {
                id: text(6),
                revision: text(7),
            },
            postings: g.postings.clone(),
            next: g.next.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sealed_compact_fee_groups_preserve_every_native_bit_and_absolute_endpoint() {
        let raw: fee::Request = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/nonintegral-fee-accounting-1.json"
        ))
        .unwrap();
        let source = fee::evaluate(&raw).unwrap();
        let compact = CompactFeeSource::from_native_bounded(&source, 32 << 20).unwrap();
        assert_eq!(compact.group_count(), source.groups().len());
        assert!(compact.retained_bytes() < 32 << 20);
        for (index, group) in source.groups().iter().enumerate() {
            assert_eq!(
                serde_json::to_vec(group).unwrap(),
                serde_json::to_vec(&*compact.group(index)).unwrap()
            );
        }
        assert!(
            CompactFeeSource::from_native_bounded(&source, compact.retained_bytes() - 1).is_err()
        );
        assert!(CompactFeeSource::from_native_bounded(&source, (32 << 20) + 1).is_err());
    }
    #[test]
    fn fee_numeric_bound_counts_complete_calendar_gross_states_and_overflow() {
        use super::super::intents::compact_fee_work_upper as upper;
        let base = upper(8, 16, 2, 30).unwrap();
        assert!(upper(8, 16, 3, 30).unwrap() > base);
        assert!(upper(8, 16, 2, 60).unwrap() > base);
        assert!(upper(usize::MAX, 16, 2, 30).is_err());
        assert!(upper(8, 16, 2, usize::MAX).is_err());
        assert!(upper(8192, 65536, 64, 1080).is_err());
    }
}
