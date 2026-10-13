//! Exact binary64 absolute-sum10k resource domain; no financial normalization.
pub const LIMIT: f64 = 10_000.;
pub fn scalar(v: f64) -> Result<f64, String> {
    if !v.is_finite() || v.abs() > LIMIT {
        Err("deposit portfolio combined10k monetary component domain".into())
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
            let cell = self
                .0
                .get_mut(i)
                .ok_or("deposit portfolio exact sum overflow")?;
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
        return Err("deposit portfolio combined10k gross monetary domain".into());
    }
    Ok(sum.rounded())
}
