//! A tiny deterministic PRNG (SplitMix64).
//!
//! Not a crate dependency on purpose: the synthetic vault and the fake stream
//! must be byte-identical for the same seed across machines and across
//! `rand` releases, otherwise two reports made a month apart would not be
//! measuring the same input.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// A child generator: same seed + label always gives the same stream,
    /// independent of how many numbers the parent has drawn.
    pub fn fork(seed: u64, label: &str) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in label.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Rng::new(seed ^ h)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in [lo, hi).
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    /// Uniform in [0, n). `n == 0` gives 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let a: Vec<u64> = {
            let mut r = Rng::new(7);
            (0..5).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::new(7);
            (0..5).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b);
        assert_ne!(Rng::new(8).next_u64(), a[0]);
    }

    #[test]
    fn fork_is_label_stable() {
        assert_eq!(Rng::fork(1, "x").next_u64(), Rng::fork(1, "x").next_u64());
        assert_ne!(Rng::fork(1, "x").next_u64(), Rng::fork(1, "y").next_u64());
    }

    #[test]
    fn ranges_hold() {
        let mut r = Rng::new(3);
        for _ in 0..1000 {
            let f = r.f64();
            assert!((0.0..1.0).contains(&f));
            assert!(r.below(7) < 7);
        }
        assert_eq!(r.below(0), 0);
        assert!(r.pick::<u8>(&[]).is_none());
    }
}
