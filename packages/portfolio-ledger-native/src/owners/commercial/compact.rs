//! Sealed journal-only copy of a genuine native accounting owner. Identities are
//! interned; postings and absolute snapshots retain their original f64 bits.
//! There is deliberately no Deserialize or constructor from caller records.
use super::intents::AccountingSource;
use portfolio_model_core::commercial_coupon_accounting::{
    AtomicGroup, CouponLot, OwnedPosting, ResultData, Snapshot,
};
use portfolio_model_core::credit_accounting::{AccountCode, Identity, PostingIntent};
use std::{borrow::Cow, collections::BTreeMap, mem::size_of};

struct Lot {
    ids: [usize; 4],
    indexes: [usize; 3],
    amounts: [f64; 6],
    default: bool,
}
struct Posting {
    account: AccountCode,
    amount: f64,
    coupon: Option<usize>,
    origin: Option<usize>,
}
struct CanonicalCoupon {
    coupon: usize,
    indexes: [usize; 2],
    amounts: [f64; 5],
}
struct Group {
    path: u64,
    year: f64,
    phase: &'static str,
    policy: [usize; 2],
    event: Option<usize>,
    amounts: [f64; 3],
    postings: Vec<Posting>,
    // Numeric endpoints and reviewed claims are copied once, never regenerated
    // by summing compact monetary deltas. Repeated coupon identities live below.
    snapshot: Snapshot,
    lots: Vec<Lot>,
    canonical: Vec<CanonicalCoupon>,
}
pub struct CompactSource {
    dictionary: Vec<String>,
    groups: Vec<Group>,
    path_key: u64,
    retained_bytes: usize,
}
impl CompactSource {
    /// Only an already constructed native owner grants this capability.
    pub fn from_native(source: &ResultData, path_key: u64) -> Result<Self, String> {
        Self::from_native_bounded(source, path_key, 32 << 20)
    }
    pub fn from_native_bounded(
        source: &ResultData,
        path_key: u64,
        remaining: usize,
    ) -> Result<Self, String> {
        if remaining > 32 << 20 || source.groups().iter().any(|g| g.path_key != path_key) {
            return Err(
                "compact source requires one complete native path and bounded remaining capacity"
                    .into(),
            );
        }
        // Reserve the entire next path before copying any owned money/identity
        // into the retained journal. Full JSON is used only by a counting
        // serializer here, never as an allocated intermediate representation.
        let mut upper = size_of::<Self>();
        for g in source.groups() {
            portfolio_compute_control::checkpoint()?;
            upper = upper
                .checked_add(
                    portfolio_model_core::strict_json::serialized_size(g, 32 << 20)?
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(1024))
                        .ok_or("compact preflight capacity overflow")?,
                )
                .ok_or("compact preflight capacity overflow")?;
            if upper > remaining {
                return Err("compact source exceeds remaining32MiB before copying".into());
            }
        }
        let mut dictionary = Vec::new();
        let mut index = BTreeMap::new();
        let mut intern = |value: &str| {
            if let Some(&i) = index.get(value) {
                i
            } else {
                let i = dictionary.len();
                dictionary.push(value.to_string());
                index.insert(value.to_string(), i);
                i
            }
        };
        let mut groups = Vec::new();
        for g in source.groups().iter().filter(|g| g.path_key == path_key) {
            portfolio_compute_control::checkpoint()?;
            let mut snapshot = g.next.clone();
            snapshot.lots.clear();
            snapshot.lots.shrink_to_fit();
            snapshot.canonical_legal_coupons.clear();
            snapshot.canonical_legal_coupons.shrink_to_fit();
            let canonical = g
                .next
                .canonical_legal_coupons
                .iter()
                .map(|c| CanonicalCoupon {
                    coupon: intern(&c.coupon_id),
                    indexes: [c.native_audit_index, c.native_cash_index],
                    amounts: [
                        c.opening_entitlement_principal,
                        c.cumulative_factor,
                        c.contractual_earned,
                        c.actual_interest_collected,
                        c.legal_remaining,
                    ],
                })
                .collect();
            let lots = g
                .next
                .lots
                .iter()
                .map(|l| Lot {
                    ids: [
                        intern(&l.coupon_id),
                        intern(&l.source_event_id),
                        intern(&l.source_id),
                        intern(&l.source_revision),
                    ],
                    indexes: [
                        l.native_audit_index,
                        l.native_cash_index,
                        l.earning_origin_index,
                    ],
                    amounts: [
                        l.contractual_earned,
                        l.legal_cash_collected,
                        l.legal_remaining,
                        l.recognized_earned,
                        l.recognized_cash_applied,
                        l.recognized_remaining,
                    ],
                    default: l.default_claim,
                })
                .collect();
            let postings = g
                .postings
                .iter()
                .map(|p| Posting {
                    account: p.posting.account,
                    amount: p.posting.amount,
                    coupon: p.coupon_id.as_deref().map(&mut intern),
                    origin: p.earning_origin_index,
                })
                .collect();
            groups.push(Group {
                path: g.path_key,
                year: g.year,
                phase: g.phase,
                policy: [intern(&g.policy.id), intern(&g.policy.revision)],
                event: g.source_event_id.as_deref().map(&mut intern),
                amounts: [g.effective_interest, g.basis_realization, g.cash],
                postings,
                snapshot,
                lots,
                canonical,
            });
        }
        if groups.is_empty()
            || groups[0].phase != "reviewed_opening_snapshot"
            || !source.closing().iter().any(|s| s.path_key == path_key)
            || groups.windows(2).any(|w| w[0].year > w[1].year)
        {
            return Err(
                "compact source requires complete ordered native acquisition/path/closing".into(),
            );
        }
        // Count actual retained vectors plus conservative allocator/string/review
        // capacity. This counts the compact source, not the live owner it replaces.
        let mut bytes = size_of::<Self>()
            + dictionary.capacity() * size_of::<String>()
            + dictionary.iter().map(|s| s.capacity() * 2).sum::<usize>()
            + groups.capacity() * size_of::<Group>();
        for g in &groups {
            bytes = bytes
                .checked_add(
                    g.postings.capacity() * size_of::<Posting>()
                        + g.lots.capacity() * size_of::<Lot>()
                        + g.canonical.capacity() * size_of::<CanonicalCoupon>()
                        + portfolio_model_core::strict_json::serialized_size(
                            &g.snapshot,
                            32 << 20,
                        )? * 4,
                )
                .ok_or("compact capacity overflow")?;
        }
        if bytes > remaining || bytes > upper {
            return Err("compact source exceeds reserved32MiB".into());
        }
        Ok(Self {
            dictionary,
            groups,
            path_key,
            retained_bytes: bytes,
        })
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    fn text(&self, index: usize) -> String {
        self.dictionary[index].clone()
    }
}
impl super::intents::Sealed for CompactSource {}
impl AccountingSource for CompactSource {
    fn group_count(&self) -> usize {
        self.groups.len()
    }
    fn year(&self, index: usize) -> f64 {
        self.groups[index].year
    }
    fn group(&self, index: usize) -> Cow<'_, AtomicGroup> {
        let g = &self.groups[index];
        let mut next = g.snapshot.clone();
        next.lots = g
            .lots
            .iter()
            .map(|l| CouponLot {
                coupon_id: self.text(l.ids[0]),
                source_event_id: self.text(l.ids[1]),
                source_id: self.text(l.ids[2]),
                source_revision: self.text(l.ids[3]),
                native_audit_index: l.indexes[0],
                native_cash_index: l.indexes[1],
                earning_origin_index: l.indexes[2],
                contractual_earned: l.amounts[0],
                legal_cash_collected: l.amounts[1],
                legal_remaining: l.amounts[2],
                recognized_earned: l.amounts[3],
                recognized_cash_applied: l.amounts[4],
                recognized_remaining: l.amounts[5],
                default_claim: l.default,
            })
            .collect();
        next.canonical_legal_coupons = g
            .canonical
            .iter()
            .map(
                |c| portfolio_model_core::credit_path_income::CanonicalCouponLegalBalance {
                    coupon_id: self.text(c.coupon),
                    native_audit_index: c.indexes[0],
                    native_cash_index: c.indexes[1],
                    opening_entitlement_principal: c.amounts[0],
                    cumulative_factor: c.amounts[1],
                    contractual_earned: c.amounts[2],
                    actual_interest_collected: c.amounts[3],
                    legal_remaining: c.amounts[4],
                },
            )
            .collect();
        Cow::Owned(AtomicGroup {
            path_key: g.path,
            year: g.year,
            phase: g.phase,
            policy: Identity {
                id: self.text(g.policy[0]),
                revision: self.text(g.policy[1]),
            },
            source_event_id: g.event.map(|i| self.text(i)),
            effective_interest: g.amounts[0],
            basis_realization: g.amounts[1],
            cash: g.amounts[2],
            postings: g
                .postings
                .iter()
                .map(|p| OwnedPosting {
                    coupon_id: p.coupon.map(|i| self.text(i)),
                    earning_origin_index: p.origin,
                    posting: PostingIntent {
                        account: p.account,
                        amount: p.amount,
                    },
                })
                .collect(),
            next,
        })
    }
    fn has_closing(&self, path_key: u64) -> bool {
        self.path_key == path_key
    }
    fn compact_bytes(&self) -> Option<usize> {
        Some(self.retained_bytes)
    }
}
