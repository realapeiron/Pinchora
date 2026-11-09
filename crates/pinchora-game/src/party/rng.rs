//! A tiny seeded generator, so a race or a heist can be replayed from the seed stored with its game.

/// SplitMix64: small, fast and good enough for games; the same seed always gives the same run.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A whole number in `0..n` (`0` when `n` is `0`).
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u32
    }

    /// A number in `0.0..1.0`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A number in `lo..hi`.
    pub fn between(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

#[cfg(test)]
mod tests {
    use crate::party::rng::*;

    #[test]
    fn the_same_seed_replays_and_numbers_stay_in_range() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut r = Rng::new(7);
        for _ in 0..1_000 {
            assert!(r.below(6) < 6);
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
            let x = r.between(2.0, 3.0);
            assert!((2.0..3.0).contains(&x));
        }
        assert_eq!(r.below(0), 0);
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }
}
