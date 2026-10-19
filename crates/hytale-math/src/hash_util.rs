//! Hash-/seed-based pseudo random number generation over `i64`s.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/HashUtil.java`
//!
//! Rust name collisions: Java overloads `hash`/`rehash`/`random` by arity —
//! ported as `hash`, `hash2`, `hash3`, `hash4` (and analogously for the
//! `rehash`/`random` families).
//!
//! Ported from: `com.hypixel.hytale.math.util.HashUtil` — the Java `static`
//! methods become free functions in this module.

/// Java `HashUtil.hash(long v)` — splitmix64-style finalizer.
pub fn hash(v: i64) -> i64 {
    // Java: v = ((v >>> 30) ^ v) * 0xbf58476d1ce4e5b9L;
    let mut v = ((v as u64 >> 30) ^ v as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    // Java: v = ((v >>> 27) ^ v) * 0x94d049bb133111ebL;
    v = ((v >> 27) ^ v).wrapping_mul(0x94d0_49bb_1331_11eb);
    // Java: v = (v >>> 31) ^ v;
    ((v >> 31) ^ v) as i64
}

/// Java `HashUtil.hash(long l1, long l2)`.
pub fn hash2(l1: i64, l2: i64) -> i64 {
    // Java: l1 = ((hash(l1) >>> 30) ^ l1) * 0xbf58476d_1ce4e5b9L;
    let l1 = ((((hash(l1) as u64) >> 30) ^ (l1 as u64)) as i64)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9u64 as i64);
    // Java: l1 = ((hash(l2) >>> 31) ^ l1);
    ((hash(l2) as u64) >> 31) as i64 ^ l1
}

/// Java `HashUtil.hash(long l1, long l2, long l3)`.
pub fn hash3(l1: i64, l2: i64, l3: i64) -> i64 {
    let l1 = ((((hash(l1) as u64) >> 30) ^ (l1 as u64)) as i64)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9u64 as i64);
    let l1 = ((((hash(l2) as u64) >> 27) ^ (l1 as u64)) as i64)
        .wrapping_mul(0x94d0_49bb_1331_11ebu64 as i64);
    ((hash(l3) as u64) >> 31) as i64 ^ l1
}

/// Java `HashUtil.hash(long l1, long l2, long l3, long l4)`.
pub fn hash4(l1: i64, l2: i64, l3: i64, l4: i64) -> i64 {
    let l1 = ((((hash(l1) as u64) >> 30) ^ (l1 as u64)) as i64)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9u64 as i64);
    let l1 = ((((hash(l2) as u64) >> 27) ^ (l1 as u64)) as i64)
        .wrapping_mul(0x94d0_49bb_1331_11ebu64 as i64);
    let l1 = ((((hash(l3) as u64) >> 30) ^ (l1 as u64)) as i64)
        .wrapping_mul(0xa753_2714_2784_b77du64 as i64);
    ((hash(l4) as u64) >> 31) as i64 ^ l1
}

/// Java `HashUtil.rehash(long)`.
pub fn rehash(l1: i64) -> i64 {
    hash(hash(l1))
}

/// Java `HashUtil.rehash(long, long)`.
pub fn rehash2(l1: i64, l2: i64) -> i64 {
    hash(hash2(l1, l2))
}

/// Java `HashUtil.rehash(long, long, long)`.
pub fn rehash3(l1: i64, l2: i64, l3: i64) -> i64 {
    hash(hash3(l1, l2, l3))
}

/// Java `HashUtil.rehash(long, long, long, long)`.
pub fn rehash4(l1: i64, l2: i64, l3: i64, l4: i64) -> i64 {
    hash(hash4(l1, l2, l3, l4))
}

/// Java `HashUtil.random(long)`.
pub fn random(l1: i64) -> f64 {
    hash_to_random_double(rehash(l1))
}

/// Java `HashUtil.random(long, long)`.
pub fn random2(l1: i64, l2: i64) -> f64 {
    hash_to_random_double(rehash2(l1, l2))
}

/// Java `HashUtil.random(long, long, long)`.
pub fn random3(l1: i64, l2: i64, l3: i64) -> f64 {
    hash_to_random_double(rehash3(l1, l2, l3))
}

/// Java `HashUtil.random(long, long, long, long)`.
pub fn random4(l1: i64, l2: i64, l3: i64, l4: i64) -> f64 {
    hash_to_random_double(rehash4(l1, l2, l3, l4))
}

/// Java `HashUtil.randomInt(long l1, long l2, long l3, int bound)`.
///
/// # Panics
/// Mirrors Java behavior dividing by zero when `bound == 0`; callers in
/// worldgen always pass a positive bound.
pub fn random_int(l1: i64, l2: i64, l3: i64, bound: i32) -> i32 {
    let hash = rehash3(l1, l2, l3) & 0x7FFF_FFFF_FFFF_FFFF;
    // Java: (int) (hash % bound)
    (hash % (bound as i64)) as i32
}

/// Java private `HashUtil.hashToRandomDouble(long hash)`.
fn hash_to_random_double(hash: i64) -> f64 {
    let hash = hash & 0xFFFF_FFFF;
    // Java: (double) hash / 0xFFFF_FFFFL
    (hash as f64) / (0xFFFF_FFFF_u32 as f64)
}

/// Java `HashUtil.hashUuid(UUID uuid)`.
///
/// The Rust port takes the UUID's 64-bit halves directly, since there is no
/// `std` `UUID` type: pass `uuid.getLeastSignificantBits()` as `least`
/// and `uuid.getMostSignificantBits()` as `most`.
pub fn hash_uuid(least: i64, most: i64) -> i64 {
    hash2(least, most)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_changes_input() {
        // No hand-crafted golden values per PORTING.md; assert the structural
        // properties the Java code relies on: determinism and injectivity on
        // small distinct inputs.
        assert_eq!(hash(1), hash(1));
        assert_ne!(hash(1), hash(2));
        assert_ne!(
            hash2(1, 2),
            hash2(2, 1),
            "hash combines l1/l2 asymmetrically"
        );
    }

    #[test]
    fn random_is_in_unit_interval() {
        // hashToRandomDouble masks to the low 32 bits and divides by 2^32-1,
        // so the result is always in [0, 1].
        for l1 in [-3i64, -1, 0, 1, 42] {
            let r = random(l1);
            assert!((0.0..=1.0).contains(&r), "random({l1}) = {r} out of [0,1]");
        }
    }

    #[test]
    fn random_int_respects_positive_bound() {
        for bound in [1i32, 3, 7, 1024, i32::MAX] {
            for l in [0i64, 1, -1, 0x4e5f_acef_afd0_3aceu64 as i64] {
                let v = random_int(1, 2, l, bound);
                assert!(v >= 0 && v < bound, "randomInt(…, {bound}) = {v}");
            }
        }
    }
}
