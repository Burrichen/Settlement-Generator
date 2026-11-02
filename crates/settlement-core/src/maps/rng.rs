//! Deterministic randomness for map geometry.
//!
//! Map geometry must be reproducible from a saved seed for as long as the
//! engine version is unchanged. `rand`'s `StdRng` makes no promise to keep
//! its output across `rand` releases, so maps use their own small,
//! fully specified generator (SplitMix64) instead. A dependency update can
//! therefore never move a building.

use serde::Serialize;

/// Largest seed stored: 2^53 − 1, so a seed survives a round trip through
/// JavaScript numbers (the GUI) unchanged.
pub const MAX_SEED: u64 = (1 << 53) - 1;

/// A fresh random map seed in `0..=MAX_SEED`.
pub fn random_seed() -> u64 {
    rand::random::<u64>() & MAX_SEED
}

/// FNV-1a over bytes: stable across runs and platforms.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Fingerprint (16 hex digits) of a value's canonical JSON.
pub fn fingerprint<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("map data serialises");
    format!("{:016x}", fnv1a(&bytes))
}

/// The seed of one map, derived from its map set's seed and its stable ID,
/// so each district is independent and adding a district never changes the
/// others.
pub fn derive_seed(set_seed: u64, map_key: &str) -> u64 {
    let mut rng = MapRng::new(set_seed ^ fnv1a(map_key.as_bytes()));
    rng.next_u64() & MAX_SEED
}

/// SplitMix64 (Steele, Lea & Flood, 2014).
#[derive(Debug, Clone)]
pub struct MapRng(u64);

impl MapRng {
    pub fn new(seed: u64) -> Self {
        MapRng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform in `lo..=hi`.
    pub fn int(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % u64::from(hi - lo + 1)) as u32
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Uniform in `[-1, 1)`.
    pub fn signed(&mut self) -> f64 {
        self.range(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_matches_its_reference_values() {
        // First outputs of SplitMix64 seeded with 0 (reference implementation).
        let mut rng = MapRng::new(0);
        assert_eq!(rng.next_u64(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(rng.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    }

    #[test]
    fn derived_seeds_are_stable_distinct_and_js_safe() {
        let a = derive_seed(42, "map-district-1");
        assert_eq!(a, derive_seed(42, "map-district-1"));
        assert_ne!(a, derive_seed(42, "map-district-2"));
        assert_ne!(a, derive_seed(43, "map-district-1"));
        assert!(a <= MAX_SEED);
        assert!(random_seed() <= MAX_SEED);
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut rng = MapRng::new(7);
        for _ in 0..1000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u));
            let i = rng.int(3, 5);
            assert!((3..=5).contains(&i));
        }
    }
}
