//! `FastRandom` — the seedable LCG used throughout world generation.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/FastRandom.java`
//!
//! Java's `FastRandom` extends `java.util.Random` and only overrides the
//! 48-bit LCG `next(int bits)`core; the public `nextInt`/`nextLong`/
//! `nextDouble`/`nextFloat`/`nextBoolean` methods inherited from
//! `java.util.Random` are algorithmically defined on top of `next(bits)`.
//! Since world generation relies on `nextInt()`, `nextInt(bound)`,
//! `nextDouble()`, `nextFloat()` and `setSeed` (see `CaveGenerator`,
//! `PrefabPopulator`, `UniquePrefabGenerator`, ...), those public wrappers
//! are ported here bit-for-bit from the JDK reference implementation.
//!
//! The only thing *not* ported is `nextGaussian` (the Java implementation
//! explicitly throws `UnsupportedOperationException` in `FastRandom`).
//!
//! Note: the Java no-argument constructor seeds from
//! `ThreadLocalRandom.current().nextLong()` — i.e. it is non-deterministic.
//! It is intentionally not ported; always construct with an explicit seed.

/// Ported from: `com.hypixel.hytale.math.util.FastRandom`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastRandom {
    seed: i64,
}

/// LCG multiplier of `java.util.Random`.
const MULTIPLIER: u64 = 0x5DEECE66D;
/// LCG addend of `java.util.Random`.
const ADDEND: u64 = 0xB;
/// 48-bit state mask of `java.util.Random`.
const MASK: u64 = (1u64 << 48) - 1;

/// Java `initialScramble(seed)`: `(seed ^ multiplier) & mask`.
fn initial_scramble(seed: i64) -> i64 {
    ((seed as u64 ^ MULTIPLIER) & MASK) as i64
}

impl FastRandom {
    /// Creates a new instance seeded with the provided value.
    ///
    /// Equivalent to Java `new FastRandom(seed)`.
    #[inline]
    pub fn new(seed: i64) -> Self {
        Self {
            seed: initial_scramble(seed),
        }
    }

    /// Equivalent to Java `setSeed(long)`.
    #[inline]
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = initial_scramble(seed);
    }

    /// The Java-protected `next(int bits)` core:
    /// `seed = (seed * multiplier + addend) & mask; return (int)(seed >>> (48 - bits))`.
    ///
    /// # Panics (debug builds)
    /// Panics if `bits` is not in `[0, 32]`. Java has the same restriction
    /// (`IllegalArgumentException` replaced by an assertion here; worldgen
    /// only ever calls it with the JDK-documented bit counts).
    #[inline]
    fn next_bits(&mut self, bits: u32) -> i32 {
        debug_assert!(bits <= 32, "bits must be <= 32 (Java contract)");
        let mut seed = self.seed as u64;
        seed = (seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND)) & MASK;
        self.seed = seed as i64;
        // Java: (int)(seed >>> (48 - bits)) — the seed is masked to 48 bits, so
        // `seed >> (48 - bits)` is already in the range [0, 2^bits) and the
        // arithmetic shift on the (positive) i64 matches the logical shift.
        ((seed >> (48 - bits)) as u32) as i32
    }

    /// Java `Random.nextInt()`.
    #[inline]
    pub fn next_int(&mut self) -> i32 {
        self.next_bits(32)
    }

    /// Java `Random.nextInt(bound)`, including the rejection loop for
    /// non-power-of-two bounds. Panics when `bound <= 0`, matching the Java
    /// `IllegalArgumentException`.
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            panic!("bound must be positive");
        }
        let mask_bound = bound.wrapping_sub(1);
        let mut r = self.next_bits(31);
        if (bound & mask_bound) == 0 {
            // i.e., bound is a power of 2
            let long_r = r as i64;
            // Java: r = (int) ((bound * (long) r) >> 31); product < 2^62, no overflow.
            r = (((bound as i64) * long_r) >> 31) as i32;
        } else {
            // Java: for (int u = r; u - (r = u % bound) + m < 0; u = next(31));
            let mut u = r;
            loop {
                let rem = u.rem_euclid(bound); // Java `u % bound` with u >= 0, bound > 0: identical
                r = rem;
                if u.wrapping_sub(rem).wrapping_add(mask_bound) >= 0 {
                    break;
                }
                u = self.next_bits(31);
            }
        }
        r
    }

    /// Java `Random.nextLong()`: `((long)next(32) << 32) + next(32)` (with Java
    /// silent overflow, hence wrapping add).
    #[inline]
    pub fn next_long(&mut self) -> i64 {
        let top = self.next_bits(32);
        let bottom = self.next_bits(32);
        ((top as i64) << 32).wrapping_add(bottom as i64)
    }

    /// Java `Random.nextBoolean()`: `next(1) != 0`.
    #[inline]
    pub fn next_boolean(&mut self) -> bool {
        self.next_bits(1) != 0
    }

    /// Java `Random.nextFloat()`: `next(24) / (float)(1 << 24)`.
    #[inline]
    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 / (1u32 << 24) as f32
    }

    /// Java `Random.nextDouble()`: `(((long)next(26) << 27) + next(27)) * 2^-53`.
    #[inline]
    pub fn next_double(&mut self) -> f64 {
        let high = self.next_bits(26);
        let low = self.next_bits(27);
        (((high as i64) << 27).wrapping_add(low as i64)) as f64 * f64::from_bits((1023 - 53) << 52)
        // 0x1.0p-53
    }
}

impl From<i64> for FastRandom {
    #[inline]
    fn from(seed: i64) -> Self {
        FastRandom::new(seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_next_int_matches_java_lcg_trace() {
        // Hand-derived trace (no Java run needed) for `new FastRandom(0)`:
        //
        //   seed = initialScramble(0) = (0 ^ 0x5DEECE66D) & (2^48-1) = 0x5DEECE66D
        //   next(32):
        //     seed = (0x5DEECE66D * 0x5DEECE66D + 0xB) & (2^48-1)
        //          = (6357911930251541449 + 11) & 0xFFFFFFFFFFFF
        //          = 205749139540596 = 0xBB20B4600A74
        //     result = (int)(0xBB20B4600A74 >>> 16)
        //            = (int)0x00000000BB20B460 = 0xBB20B460
        //            = -1155484576 (two's complement)
        //
        // 0xBB20B460 as i32: top bit set (0xBB2... > 0x80000000), so the value
        // is 0xBB20B460 - 0x100000000 = 3139482720 - 4294967296 = -1155484576.
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_int(), -1155484576);
    }

    #[test]
    fn set_seed_resets_state() {
        // Calling next_int() then set_seed(0) must restart the sequence.
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_int(), -1155484576);
        random.set_seed(0);
        assert_eq!(random.next_int(), -1155484576);
    }

    #[test]
    fn first_next_double_matches_java() {
        // Trace (fresh instance; the LCG updates the state BEFORE taking the
        // high bits):
        //   s0 = 0x5DEECE66D
        //   next(26): s1 = (s0 * 0x5DEECE66D + 0xB) & (2^48-1) = 0xBB20B4600A74
        //             high 26 bits of s1 = 49054417
        //   next(27): s2 = 0xD4D95138AB6F, high 27 bits = 111594121
        //   nextDouble = ((49054417 << 27) + 111594121) * 2^-53
        //              = 6492056327497645257 * 2^-53 = 0.730967787376657
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_double(), 0.730967787376657);
    }

    #[test]
    fn first_next_float_matches_java() {
        // Trace: fresh instance,
        //   next(24): s1 = 0xBB20B4600A74, high 24 bits = 12263604
        //   nextFloat = 12263604 / (float)16777216 = 0.7309677600860596
        //   (exactly representable, bits 0x3F3B20B4)
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_float(), f32::from_bits(0x3F3B_20B4));
    }

    #[test]
    fn first_next_boolean_matches_java() {
        // Trace: next(1) on s0: s1 = 0xBB20B4600A74, bit 47 of s1 = 1 -> true
        let mut random = FastRandom::new(0);
        assert!(random.next_boolean());
    }

    #[test]
    fn first_next_long_matches_java() {
        // Trace, fresh instance:
        //   first next(32) = -1155484576 (see trace in first_next_int test)
        //   second next(32): s2 = 0xD4D95138AB6F, (int)(0xD4D95138) = -723955400
        //   nextLong = (-1155484576 << 32) + (-723955400) = -4962768465676381896
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_long(), -4962768465676381896);
    }

    #[test]
    fn next_int_bound_rejection_loop() {
        // Trace, fresh FastRandom(0), bound = 10 (not a power of two):
        //   r = next(31): s1 = 0xBB20B4600A74, >>> 17 = 1569741360
        //   u % bound = 1569741360 % 10 = 0 (< bound, no rejection)
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_int_bound(10), 0);
    }

    #[test]
    fn next_int_bound_power_of_two() {
        // Trace, fresh FastRandom(0), bound = 16 (power of two):
        //   r = next(31) = 1569741360 (see trace above)
        //   (16 * 1569741360) >> 31 = 25115861760 >> 31 = 11
        let mut random = FastRandom::new(0);
        assert_eq!(random.next_int_bound(16), 11);
    }

    #[test]
    fn negative_int_bound_panics_like_java() {
        let mut random = FastRandom::new(0);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| random.next_int_bound(-1)));
        assert!(result.is_err());
    }

    #[test]
    fn ieee754_double_unit_constant_is_half_threshold() {
        // Java `Random.DOUBLE_UNIT = 0x1.0p-53`; used by next_double.
        assert_eq!(f64::from_bits((1023 - 53) << 52), 1.1102230246251565e-16);
    }
}
