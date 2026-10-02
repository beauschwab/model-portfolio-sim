//! NumPy-compatible seeded shared Gaussian tapes, implemented entirely in Rust.
//! SeedSequence, PCG64 XSL-RR and Ziggurat normal algorithms are adapted from
//! NumPy 2.4.6. Provenance and licenses: random-source-manifest.json and
//! THIRD_PARTY_NOTICES.md. This is simulation randomness, not cryptography.
use crate::ziggurat::*;

/// Limit retained draws to 1 GiB. FFI adapters reserve an additional output copy.
pub const MAX_DRAW_VALUES: usize = 128 * 1024 * 1024;

fn hashmix(mut value: u32, hash: &mut u32) -> u32 {
    value ^= *hash;
    *hash = hash.wrapping_mul(0x931e8875);
    value = value.wrapping_mul(*hash);
    value ^ (value >> 16)
}
fn mix(x: u32, y: u32) -> u32 {
    let r = x
        .wrapping_mul(0xca01f9dd)
        .wrapping_sub(y.wrapping_mul(0x4973f715));
    r ^ (r >> 16)
}

/// Default unspawned 128-bit SeedSequence entropy pool, generating four u64s.
fn seed_state(entropy: &[u32]) -> [u64; 4] {
    let mut pool = [0; 4];
    let mut hash = 0x43b0d7e5;
    for (i, p) in pool.iter_mut().enumerate() {
        *p = hashmix(entropy.get(i).copied().unwrap_or(0), &mut hash);
    }
    for src in 0..4 {
        for dst in 0..4 {
            if src != dst {
                pool[dst] = mix(pool[dst], hashmix(pool[src], &mut hash));
            }
        }
    }
    for &word in entropy.iter().skip(4) {
        for p in &mut pool {
            *p = mix(*p, hashmix(word, &mut hash));
        }
    }
    let mut state = [0_u64; 4];
    hash = 0x8b51f9dd;
    for i in 0..8 {
        let mut data = pool[i % 4] ^ hash;
        hash = hash.wrapping_mul(0x58f38ded);
        data = data.wrapping_mul(hash);
        data ^= data >> 16;
        state[i / 2] |= (data as u64) << (32 * (i % 2));
    }
    state
}

fn add_seed(seed: &[u32], increment: u32) -> Vec<u32> {
    let mut words = seed.to_vec();
    // Canonical integer entropy matters when offsets carry past the top word.
    while words.len() > 1 && words.last() == Some(&0) {
        words.pop();
    }
    let mut carry = increment as u64;
    for word in &mut words {
        let sum = *word as u64 + carry;
        *word = sum as u32;
        carry = sum >> 32;
    }
    if carry > 0 {
        words.push(carry as u32);
    }
    words
}

struct Pcg64 {
    state: u128,
    increment: u128,
}
impl Pcg64 {
    fn new(seed: &[u32]) -> Self {
        let words = seed_state(seed);
        let initial = ((words[0] as u128) << 64) | words[1] as u128;
        let sequence = ((words[2] as u128) << 64) | words[3] as u128;
        let mut rng = Self {
            state: 0,
            increment: sequence.wrapping_shl(1) | 1,
        };
        rng.step();
        rng.state = rng.state.wrapping_add(initial);
        rng.step();
        rng
    }
    fn step(&mut self) {
        const MULTIPLIER: u128 = (2549297995355413924_u128 << 64) | 4865540595714422341_u128;
        self.state = self
            .state
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(self.increment);
    }
    fn next(&mut self) -> u64 {
        self.step();
        ((self.state >> 64) as u64 ^ self.state as u64).rotate_right((self.state >> 122) as u32)
    }
    fn uniform(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1. / 9007199254740992.)
    }
    fn normal(&mut self) -> f64 {
        loop {
            let r = self.next();
            let idx = (r & 255) as usize;
            let r = r >> 8;
            let magnitude = (r >> 1) & 0x000f_ffff_ffff_ffff;
            let mut x = magnitude as f64 * WI_DOUBLE[idx];
            if r & 1 != 0 {
                x = -x;
            }
            if magnitude < KI_DOUBLE[idx] {
                return x;
            }
            if idx == 0 {
                loop {
                    let xx = -ZIGGURAT_NOR_INV_R * (-self.uniform()).ln_1p();
                    let yy = -(-self.uniform()).ln_1p();
                    if yy + yy > xx * xx {
                        return if (magnitude >> 8) & 1 != 0 {
                            -(ZIGGURAT_NOR_R + xx)
                        } else {
                            ZIGGURAT_NOR_R + xx
                        };
                    }
                }
            } else if (FI_DOUBLE[idx - 1] - FI_DOUBLE[idx]) * self.uniform() + FI_DOUBLE[idx]
                < (-0.5 * x * x).exp()
            {
                return x;
            }
        }
    }
    fn fill(&mut self, count: usize) -> Result<Vec<f64>, String> {
        let mut out = Vec::new();
        out.try_reserve_exact(count)
            .map_err(|_| "cannot allocate random tape")?;
        out.extend((0..count).map(|_| self.normal()));
        Ok(out)
    }
}

/// Owned tapes in row-major order. The rate stream is antithetic, including odd
/// path counts. Spread and HPI use independent seed+101 and seed+202 streams.
pub struct SharedDraws {
    pub rates: Vec<f64>,
    pub spread: Vec<f64>,
    pub hpi: Vec<f64>,
    pub shape: [usize; 3],
}
impl SharedDraws {
    /// `seed` is unsigned integer entropy encoded as little-endian u32 words.
    pub fn new(seed: &[u32], shape: [usize; 3]) -> Result<Self, String> {
        let [paths, months, factors] = shape;
        if seed.is_empty() || seed.len() > 1024 || shape.contains(&0) {
            return Err("invalid random seed or dimensions".into());
        }
        let cells = paths
            .checked_mul(months)
            .ok_or("random tape dimensions overflow")?;
        let all = factors
            .checked_add(2)
            .and_then(|n| cells.checked_mul(n))
            .ok_or("random tape dimensions overflow")?;
        if all > MAX_DRAW_VALUES {
            return Err("shared draw tapes exceed 1 GiB admission limit".into());
        }
        let seed = add_seed(seed, 0);
        let half = paths.div_ceil(2);
        let half_cells = half * months * factors;
        let mut rng = Pcg64::new(&seed);
        let mut rates = Vec::new();
        rates
            .try_reserve_exact(cells * factors)
            .map_err(|_| "cannot allocate rate tape")?;
        rates.extend((0..half_cells).map(|_| rng.normal()));
        for i in 0..(paths - half) * months * factors {
            rates.push(-rates[i]);
        }
        let (spread, hpi) = rayon::join(
            || Pcg64::new(&add_seed(&seed, 101)).fill(cells),
            || Pcg64::new(&add_seed(&seed, 202)).fill(cells),
        );
        Ok(Self {
            rates,
            spread: spread?,
            hpi: hpi?,
            shape,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numpy_pcg64_seed_zero_golden() {
        let mut rng = Pcg64::new(&[0]);
        assert_eq!(rng.next(), 11749869230777074271);
        assert_eq!(rng.next(), 4976686463289251617);
        let mut rng = Pcg64::new(&[0]);
        assert_eq!(rng.normal(), 0.1257302210933933);
        assert_eq!(rng.normal(), -0.1321048632913019);
    }
    #[test]
    fn odd_antithetic_and_admission() {
        let out = SharedDraws::new(&[19], [5, 11, 3]).unwrap();
        assert_eq!(out.rates.len(), 165);
        for i in 0..66 {
            assert_eq!(out.rates[99 + i], -out.rates[i]);
        }
        assert_ne!(out.spread, out.hpi);
        assert_eq!(
            out.rates,
            SharedDraws::new(&[19, 0, 0, 0, 0], [5, 11, 3])
                .unwrap()
                .rates
        );
        assert_eq!(add_seed(&[u32::MAX, u32::MAX], 101), vec![100, 0, 1]);
        assert!(SharedDraws::new(&[], [1, 1, 1]).is_err());
        assert!(SharedDraws::new(&[1], [0, 1, 1]).is_err());
        assert!(SharedDraws::new(&[1], [usize::MAX, 2, 1]).is_err());
        assert!(SharedDraws::new(&[1], [MAX_DRAW_VALUES, 1, 1]).is_err());
    }
}
