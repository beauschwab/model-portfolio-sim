//! New schema only: conservative supplied gross domain and separate runtime lanes.
use super::{core, Request};
use crate::dated_ledger::CompiledDatedLedger;
pub const LIMIT: f64 = 10_000.;
pub fn scalar(v: f64) -> Result<f64, String> {
    if !v.is_finite() || v.abs() > LIMIT {
        Err("mortgage combined10k monetary component domain".into())
    } else {
        Ok(v.abs())
    }
}
// Exact nonnegative binary64 sum in units of 2^-1074. Eighteen limbs cover
// the bounded10k scalar domain; admission also bounds collection cardinality.
// This criterion is independent of HashMap/permutation order, with no epsilon.
#[derive(Default)]
struct AbsoluteSum([u64; 18]);
impl AbsoluteSum {
    fn word(&mut self, mut i: usize, mut n: u64) -> Result<(), String> {
        while n != 0 {
            let cell = self.0.get_mut(i).ok_or("mortgage exact sum overflow")?;
            let (value, carry) = cell.overflowing_add(n);
            *cell = value;
            n = u64::from(carry);
            i += 1;
        }
        Ok(())
    }
    fn add(&mut self, value: f64) -> Result<(), String> {
        let bits = value.abs().to_bits();
        let e = ((bits >> 52) & 0x7ff) as usize;
        let m = (bits & ((1u64 << 52) - 1)) | if e == 0 { 0 } else { 1u64 << 52 };
        let shift = e.saturating_sub(1);
        let i = shift / 64;
        let offset = shift % 64;
        self.word(i, m << offset)?;
        if offset != 0 {
            self.word(i + 1, m >> (64 - offset))?;
        }
        Ok(())
    }
    fn bit(&self, k: usize) -> bool {
        self.0[k / 64] & (1u64 << (k % 64)) != 0
    }
    fn lower_nonzero(&self, k: usize) -> bool {
        self.0[..k / 64].iter().any(|v| *v != 0)
            || (!k.is_multiple_of(64) && self.0[k / 64] & ((1u64 << (k % 64)) - 1) != 0)
    }
    fn rounded(&self) -> f64 {
        let Some(i) = self.0.iter().rposition(|v| *v != 0) else {
            return 0.;
        };
        let top = i * 64 + 63 - self.0[i].leading_zeros() as usize;
        let shift = top.saturating_sub(52);
        let mut sig = 0u64;
        for k in (shift..=top).rev() {
            sig = (sig << 1) | u64::from(self.bit(k));
        }
        if shift != 0 && self.bit(shift - 1) && (self.lower_nonzero(shift - 1) || sig & 1 != 0) {
            sig += 1;
        }
        (sig as f64)
            * if shift < 52 {
                f64::from_bits(1u64 << shift)
            } else {
                f64::from_bits(((shift - 51) as u64) << 52)
            }
    }
}
pub fn gross(values: impl IntoIterator<Item = f64>) -> Result<f64, String> {
    let mut sum = AbsoluteSum::default();
    for value in values {
        portfolio_compute_control::checkpoint()?;
        scalar(value)?;
        sum.add(value)?;
    }
    let mut limit = AbsoluteSum::default();
    limit.add(LIMIT)?;
    if sum.0.iter().rev().cmp(limit.0.iter().rev()).is_gt() {
        return Err("mortgage combined10k gross monetary domain".into());
    }
    Ok(sum.rounded())
}
pub fn inputs(r: &Request, c: &CompiledDatedLedger) -> Result<f64, String> {
    let accounts = c
        .spec
        .accounts
        .iter()
        .flat_map(|a| [a.cash, a.equity, a.cash_floor, a.capital_deductions]);
    let positions = c.spec.positions.iter().flat_map(|p| {
        [
            p.balance,
            p.book_adjustment,
            p.opening_accrued,
            p.allowance,
            p.commitment,
            p.pledged_face,
        ]
    });
    let events = c.store.events().iter().flat_map(|e| {
        let x = &e.components;
        [
            x.principal,
            x.cash_interest,
            x.accrued_interest,
            x.book_amortization,
            x.fees,
        ]
    });
    let recognition = r
        .dated
        .dated
        .accrual_plans
        .iter()
        .flat_map(|p| p.segments.iter())
        .flat_map(|s| [s.accrued_interest, s.book_amortization]);
    let mortgage = r.sources.iter().flat_map(|s| {
        let raw = &s.request.source;
        raw.contract
            .opening_investor_principal
            .into_iter()
            .chain([raw.acquisition.actual_cost])
    });
    gross(
        accounts
            .chain(positions)
            .chain(events)
            .chain(recognition)
            .chain(mortgage),
    )
}

pub fn book(b: &core::Book) -> Result<(), String> {
    if b.claims.len() > 48 {
        return Err("mortgage48claim reservation exceeded".into());
    }
    let owned = b
        .slots
        .iter()
        .flat_map(|x| [x.principal, x.recognized_interest, x.basis])
        .chain(
            b.claims
                .iter()
                .flat_map(|x| [x.book.principal, x.book.recognized_interest, x.book.basis]),
        )
        .chain([b.cash, b.original_eir_income, b.reviewed_income]);
    gross(owned)?;
    // Independent memo lanes, never substituted for recognized book balances.
    gross(
        b.borrower_principal
            .into_iter()
            .chain(b.borrower_contract_interest),
    )?;
    gross(
        b.claims
            .iter()
            .flat_map(|x| [x.economic_principal, x.economic_contract_interest]),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_threshold_is_permutation_independent_without_rounded_excess() {
        for v in [
            [10000., f64::from_bits(1), 0.],
            [0., 10000., f64::from_bits(1)],
            [f64::from_bits(1), 0., 10000.],
        ] {
            assert!(gross(v).is_err());
        }
        for v in [[9999., 0.5, 0.5], [0.5, 9999., 0.5], [0.5, 0.5, 9999.]] {
            assert_eq!(gross(v).unwrap(), 10000.);
        }
        assert_eq!(
            gross([f64::from_bits(1), f64::from_bits(1)])
                .unwrap()
                .to_bits(),
            2
        );
        assert_eq!(gross([1., f64::EPSILON]).unwrap(), 1. + f64::EPSILON);
    }
    #[test]
    fn cancelled_guard_never_returns_partial_sum() {
        let control = portfolio_compute_control::Control::new(30000).unwrap();
        control.cancel();
        assert!(portfolio_compute_control::scope(Some(control), || gross([1., 2.])).is_err());
    }
    #[test]
    fn signed_gross_has_no_offset_or_tolerance() {
        assert!(gross([5000., -5000.]).is_ok());
        assert!(gross([5000., -5000.0000000001]).is_err());
        assert!(scalar(-10000.).is_ok());
        assert!(scalar(10000.0000000001).is_err());
    }
    #[test]
    fn pending_and_claim_book_money_share_one_gross_domain() {
        let mut b = core::Book {
            ordinal: 1,
            slots: [core::Channel::default(); 7],
            borrower_principal: [0.; 7],
            borrower_contract_interest: [0.; 7],
            claims: vec![],
            cash: 0.,
            original_eir_income: 0.,
            reviewed_income: 0.,
        };
        b.slots[3].principal = 6000.;
        b.slots[6].principal = 6000.;
        assert!(book(&b).is_err());
        b.slots[6].principal = 0.;
        b.borrower_contract_interest[4] = -10001.;
        assert!(book(&b).is_err());
    }
}
